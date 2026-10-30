//! RenderDoc's in-app API: one-button frame capture from inside the engine.
//!
//! RenderDoc has to be loaded before the Vulkan instance exists so its layer
//! can hook the device, so [`RenderDoc::attach`] runs before [`Gpu`] is
//! created. When the game was launched from RenderDoc it is already in the
//! process and is simply picked up.
//!
//! [`Gpu`]: crate::Gpu

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::{Path, PathBuf},
};

use libloading::Library;

/// `eRENDERDOC_API_Version_1_6_0`.
const API_VERSION: c_int = 10600;

type GetApi = unsafe extern "C" fn(version: c_int, out: *mut *mut c_void) -> c_int;
type Unused = Option<unsafe extern "C" fn()>;

/// `RENDERDOC_API_1_6_0` from `renderdoc_app.h`; the field order is the ABI.
#[repr(C)]
struct Api {
    get_api_version: Unused,
    set_capture_option_u32: Unused,
    set_capture_option_f32: Unused,
    get_capture_option_u32: Unused,
    get_capture_option_f32: Unused,
    set_focus_toggle_keys: Unused,
    set_capture_keys: Unused,
    get_overlay_bits: Unused,
    mask_overlay_bits: Unused,
    remove_hooks: Unused,
    unload_crash_handler: Unused,
    set_capture_file_path_template: unsafe extern "C" fn(template: *const c_char),
    get_capture_file_path_template: Unused,
    get_num_captures: unsafe extern "C" fn() -> u32,
    get_capture: unsafe extern "C" fn(index: u32, path: *mut c_char, path_len: *mut u32, timestamp: *mut u64) -> u32,
    trigger_capture: unsafe extern "C" fn(),
    is_target_control_connected: unsafe extern "C" fn() -> u32,
    launch_replay_ui: unsafe extern "C" fn(connect: u32, command_line: *const c_char) -> u32,
    set_active_window: Unused,
    start_frame_capture: unsafe extern "C" fn(device: *mut c_void, window: *mut c_void),
    is_frame_capturing: unsafe extern "C" fn() -> u32,
    end_frame_capture: unsafe extern "C" fn(device: *mut c_void, window: *mut c_void) -> u32,
    trigger_multi_frame_capture: Unused,
    set_capture_file_comments: Unused,
    discard_frame_capture: Unused,
    show_replay_ui: unsafe extern "C" fn() -> u32,
    set_capture_title: Unused,
}

pub struct RenderDoc {
    api: &'static Api,
    _library: Library,
}

#[cfg(windows)]
const LIBRARY: &str = "renderdoc.dll";
#[cfg(not(windows))]
const LIBRARY: &str = "librenderdoc.so";

impl RenderDoc {
    /// Connects to RenderDoc if it is already in the process (the game was
    /// launched from RenderDoc), or loads it when `load` is set and RenderDoc
    /// is installed. Must run before the Vulkan instance is created.
    pub fn attach(load: bool) -> Option<Self> {
        let library = already_loaded().or_else(|| if load { unsafe { Library::new(LIBRARY).ok() } } else { None })?;
        let api = unsafe {
            let get_api = library.get::<GetApi>(b"RENDERDOC_GetAPI\0").ok()?;
            let mut api: *mut c_void = std::ptr::null_mut();
            if get_api(API_VERSION, &mut api) != 1 || api.is_null() {
                return None;
            }
            &*(api as *const Api)
        };
        tracing::info!("RenderDoc attached: F12 captures a frame");
        Some(Self { api, _library: library })
    }

    /// Writes captures as `<folder>/<name>_<timestamp>.rdc`.
    pub fn set_capture_folder(&self, folder: &Path, name: &str) {
        let _ = std::fs::create_dir_all(folder);
        let Ok(template) = CString::new(folder.join(name).to_string_lossy().into_owned()) else { return };
        unsafe { (self.api.set_capture_file_path_template)(template.as_ptr()) };
    }

    /// Captures the next presented frame.
    pub fn trigger_capture(&self) {
        unsafe { (self.api.trigger_capture)() };
    }

    /// Starts capturing commands for `instance` (for frames that are never
    /// presented, such as offscreen renders).
    pub fn start_capture(&self, instance: &ash::Instance) {
        unsafe { (self.api.start_frame_capture)(device_pointer(instance), std::ptr::null_mut()) };
    }

    pub fn end_capture(&self, instance: &ash::Instance) -> bool {
        unsafe { (self.api.end_frame_capture)(device_pointer(instance), std::ptr::null_mut()) == 1 }
    }

    pub fn is_capturing(&self) -> bool {
        unsafe { (self.api.is_frame_capturing)() == 1 }
    }

    /// The most recent capture file, if any.
    pub fn latest_capture(&self) -> Option<PathBuf> {
        unsafe {
            let count = (self.api.get_num_captures)();
            let index = count.checked_sub(1)?;
            let mut len = 0u32;
            if (self.api.get_capture)(index, std::ptr::null_mut(), &mut len, std::ptr::null_mut()) != 1 {
                return None;
            }
            let mut buffer = vec![0u8; len as usize];
            (self.api.get_capture)(index, buffer.as_mut_ptr().cast(), &mut len, std::ptr::null_mut());
            let path = CStr::from_bytes_until_nul(&buffer).ok()?;
            Some(PathBuf::from(path.to_string_lossy().into_owned()))
        }
    }

    /// Opens the RenderDoc UI connected to this process, or brings it forward.
    pub fn show_ui(&self) {
        unsafe {
            if (self.api.is_target_control_connected)() == 1 {
                (self.api.show_replay_ui)();
            } else {
                (self.api.launch_replay_ui)(1, std::ptr::null());
            }
        }
    }
}

/// RenderDoc identifies a Vulkan instance by its dispatch table pointer.
fn device_pointer(instance: &ash::Instance) -> *mut c_void {
    use ash::vk::Handle;
    let handle = instance.handle().as_raw() as *const *mut c_void;
    unsafe { *handle }
}

#[cfg(unix)]
fn already_loaded() -> Option<Library> {
    use libloading::os::unix::{Library as UnixLibrary, RTLD_NOW};
    // glibc's RTLD_NOLOAD: only succeed if the library is already loaded.
    const RTLD_NOLOAD: c_int = 0x4;
    unsafe { UnixLibrary::open(Some(LIBRARY), RTLD_NOW | RTLD_NOLOAD).ok().map(Library::from) }
}

#[cfg(windows)]
fn already_loaded() -> Option<Library> {
    libloading::os::windows::Library::open_already_loaded(LIBRARY).ok().map(Library::from)
}
