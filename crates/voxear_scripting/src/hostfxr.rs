//! Minimal CoreCLR hosting through `hostfxr`, the same entry point the
//! `dotnet` muxer uses. Written directly against the hosting API so there is
//! no build-time download of `nethost`.

use std::{
    ffi::{OsStr, c_void},
    iter,
    path::{Path, PathBuf},
    ptr,
};

use anyhow::{Context, Result, bail};
use libloading::Library;

#[cfg(windows)]
pub(crate) type CharT = u16;
#[cfg(not(windows))]
pub(crate) type CharT = u8;

#[repr(C)]
struct InitializeParameters {
    size: usize,
    host_path: *const CharT,
    dotnet_root: *const CharT,
}

type InitializeForRuntimeConfig =
    unsafe extern "C" fn(*const CharT, *const InitializeParameters, *mut *mut c_void) -> i32;
type GetRuntimeDelegate = unsafe extern "C" fn(*mut c_void, i32, *mut *mut c_void) -> i32;
type CloseHost = unsafe extern "C" fn(*mut c_void) -> i32;
type ErrorWriter = unsafe extern "C" fn(*const CharT);
type SetErrorWriter = unsafe extern "C" fn(Option<ErrorWriter>) -> Option<ErrorWriter>;

type LoadAssembly = unsafe extern "system" fn(*const CharT, *mut c_void, *mut c_void) -> i32;
type GetFunctionPointer = unsafe extern "system" fn(
    *const CharT,
    *const CharT,
    *const CharT,
    *mut c_void,
    *mut c_void,
    *mut *mut c_void,
) -> i32;

const HDT_GET_FUNCTION_POINTER: i32 = 6;
const HDT_LOAD_ASSEMBLY: i32 = 7;
/// `UNMANAGEDCALLERSONLY_METHOD`: look up an `[UnmanagedCallersOnly]` method.
const UNMANAGED_CALLERS_ONLY: *const CharT = usize::MAX as *const CharT;

#[cfg(windows)]
const HOSTFXR_NAME: &str = "hostfxr.dll";
#[cfg(target_os = "macos")]
const HOSTFXR_NAME: &str = "libhostfxr.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const HOSTFXR_NAME: &str = "libhostfxr.so";

/// A .NET install that can host the runtime.
#[derive(Clone, Debug)]
pub struct DotnetInstall {
    pub root: PathBuf,
    pub hostfxr: PathBuf,
    pub hostfxr_version: String,
}

/// Finds the newest `hostfxr` in `DOTNET_ROOT`, next to `dotnet` on `PATH`,
/// or in the platform's default install locations.
pub fn find_dotnet() -> Option<DotnetInstall> {
    candidate_roots().into_iter().find_map(|root| install_at(&root))
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("DOTNET_ROOT") {
        roots.push(PathBuf::from(root));
    }
    let exe = if cfg!(windows) { "dotnet.exe" } else { "dotnet" };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(exe);
            if let Ok(resolved) = candidate.canonicalize()
                && let Some(parent) = resolved.parent()
            {
                roots.push(parent.to_path_buf());
            }
        }
    }
    if cfg!(windows) {
        roots.push(PathBuf::from(r"C:\Program Files\dotnet"));
    } else {
        roots.extend(
            ["/usr/share/dotnet", "/usr/lib/dotnet", "/usr/local/share/dotnet", "/opt/dotnet"].map(PathBuf::from),
        );
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".dotnet"));
        }
    }
    roots
}

fn install_at(root: &Path) -> Option<DotnetInstall> {
    let fxr = root.join("host").join("fxr");
    let newest = std::fs::read_dir(&fxr)
        .ok()?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|version| fxr.join(version).join(HOSTFXR_NAME).is_file())
        .max_by_key(|version| version_key(version))?;
    Some(DotnetInstall {
        root: root.to_path_buf(),
        hostfxr: fxr.join(&newest).join(HOSTFXR_NAME),
        hostfxr_version: newest,
    })
}

/// Numeric ordering for versions like `10.0.13` and `11.0.0-rc.2`.
fn version_key(version: &str) -> Vec<u64> {
    let (release, prerelease) = version.split_once('-').unwrap_or((version, ""));
    let mut key: Vec<u64> = release.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    // A release sorts after its own prereleases.
    key.push(if prerelease.is_empty() { u64::MAX } else { 0 });
    key.extend(prerelease.split('.').filter_map(|p| p.parse::<u64>().ok()));
    key
}

pub(crate) fn to_char_t(s: &OsStr) -> Vec<CharT> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().chain(iter::once(0)).collect()
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        s.as_bytes().iter().copied().chain(iter::once(0)).collect()
    }
}

unsafe extern "C" fn log_host_error(message: *const CharT) {
    let mut len = 0;
    unsafe {
        while *message.add(len) != 0 {
            len += 1;
        }
        let units = std::slice::from_raw_parts(message, len);
        #[cfg(windows)]
        let text = String::from_utf16_lossy(units);
        #[cfg(not(windows))]
        let text = String::from_utf8_lossy(units);
        tracing::error!(target: "dotnet", "{text}");
    }
}

/// The two runtime delegates Voxear needs. The runtime itself can never be
/// unloaded, so `hostfxr` stays loaded for the life of the process.
pub(crate) struct Runtime {
    load_assembly: LoadAssembly,
    get_function_pointer: GetFunctionPointer,
}

impl Runtime {
    /// Starts CoreCLR for the framework named in `runtime_config`.
    pub fn start(install: &DotnetInstall, runtime_config: &Path) -> Result<Self> {
        let library = unsafe { Library::new(&install.hostfxr) }
            .with_context(|| format!("loading {}", install.hostfxr.display()))?;
        let library: &'static Library = Box::leak(Box::new(library));

        unsafe {
            let set_error_writer = *library.get::<SetErrorWriter>(b"hostfxr_set_error_writer\0")?;
            let initialize = *library.get::<InitializeForRuntimeConfig>(b"hostfxr_initialize_for_runtime_config\0")?;
            let get_delegate = *library.get::<GetRuntimeDelegate>(b"hostfxr_get_runtime_delegate\0")?;
            let close = *library.get::<CloseHost>(b"hostfxr_close\0")?;
            set_error_writer(Some(log_host_error));

            let config = to_char_t(runtime_config.as_os_str());
            let root = to_char_t(install.root.as_os_str());
            let parameters = InitializeParameters {
                size: size_of::<InitializeParameters>(),
                host_path: ptr::null(),
                dotnet_root: root.as_ptr(),
            };
            let mut host = ptr::null_mut();
            let status = initialize(config.as_ptr(), &parameters, &mut host);
            if status < 0 || host.is_null() {
                bail!("could not start .NET for {} (hostfxr status {:#x})", runtime_config.display(), status as u32);
            }

            let load_assembly = runtime_delegate(get_delegate, host, HDT_LOAD_ASSEMBLY);
            let get_function_pointer = runtime_delegate(get_delegate, host, HDT_GET_FUNCTION_POINTER);
            close(host);
            Ok(Self {
                load_assembly: std::mem::transmute::<*mut c_void, LoadAssembly>(load_assembly?),
                get_function_pointer: std::mem::transmute::<*mut c_void, GetFunctionPointer>(get_function_pointer?),
            })
        }
    }

    /// Loads an assembly into the default load context.
    pub fn load_assembly(&self, path: &Path) -> Result<()> {
        let path_chars = to_char_t(path.as_os_str());
        let status = unsafe { (self.load_assembly)(path_chars.as_ptr(), ptr::null_mut(), ptr::null_mut()) };
        if status < 0 {
            bail!("could not load {} (status {:#x})", path.display(), status as u32);
        }
        Ok(())
    }

    /// Returns a static `[UnmanagedCallersOnly]` method, e.g.
    /// `("Voxear.Interop.Bootstrap, Voxear.Core", "Initialize")`.
    pub fn function(&self, type_name: &str, method: &str) -> Result<*mut c_void> {
        let type_chars = to_char_t(OsStr::new(type_name));
        let method_chars = to_char_t(OsStr::new(method));
        let mut function = ptr::null_mut();
        let status = unsafe {
            (self.get_function_pointer)(
                type_chars.as_ptr(),
                method_chars.as_ptr(),
                UNMANAGED_CALLERS_ONLY,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut function,
            )
        };
        if status < 0 || function.is_null() {
            bail!("could not find {type_name}::{method} (status {:#x})", status as u32);
        }
        Ok(function)
    }
}

unsafe fn runtime_delegate(get: GetRuntimeDelegate, host: *mut c_void, kind: i32) -> Result<*mut c_void> {
    let mut delegate = ptr::null_mut();
    let status = unsafe { get(host, kind, &mut delegate) };
    if status < 0 || delegate.is_null() {
        bail!("hostfxr_get_runtime_delegate({kind}) failed (status {:#x}); .NET 8 or newer is required", status as u32);
    }
    Ok(delegate)
}

#[cfg(test)]
mod tests {
    use super::version_key;

    #[test]
    fn versions_sort_numerically_with_prereleases_first() {
        let mut versions = vec!["9.0.4", "10.0.13", "10.0.2", "11.0.0-rc.2", "11.0.0-rc.10", "11.0.0"];
        versions.sort_by_key(|v| version_key(v));
        assert_eq!(versions, ["9.0.4", "10.0.2", "10.0.13", "11.0.0-rc.2", "11.0.0-rc.10", "11.0.0"]);
    }
}
