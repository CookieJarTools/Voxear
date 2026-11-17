//! Pipelines made on an earlier run, kept on disk so the next start finds
//! them made. Compiling them all takes the driver a minute or more, and the
//! driver's own cache is shared with every other program and build, which
//! push out what it held.
//!
//! Each build's pipelines are kept in a file of their own, named by its
//! shaders, the GPU and its driver: an installed engine's in the user's
//! cache folder, and a build from source's beside the build, so builds made
//! while working on the engine never push out an installed one's.
//! `VOXEAR_PIPELINE_CACHE=0` turns it off, and set to a folder keeps the
//! files there.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use voxear_rhi::Gpu;

use crate::hot_reload::ShaderSet;

/// How many files a folder keeps, the ones used last.
const KEPT: usize = 4;
/// A file not used for this long is let go.
const UNUSED_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const EXTENSION: &str = "pipelines";

/// The file a renderer's pipelines are kept in, and how much of it was read.
pub(crate) struct PipelineCacheFile {
    path: PathBuf,
    held: usize,
}

impl PipelineCacheFile {
    /// Fills `gpu`'s pipeline cache from the file kept for `shaders` on it,
    /// when there is one. `None` when caching is turned off.
    pub fn open(gpu: &mut Gpu, shaders: &ShaderSet) -> Option<Self> {
        let folder = folder()?;
        let header = gpu.pipeline_cache_header();
        let key = header.iter().chain(gpu.info.driver.as_bytes()).copied().chain(shaders.fingerprint().to_le_bytes());
        let path = folder.join(format!("{:016x}.{EXTENSION}", fnv(key)));
        let held = match std::fs::read(&path) {
            Ok(data) if gpu.load_pipeline_cache(&data) => {
                // Marked used, so the folder keeps it over files not used lately.
                let _ =
                    std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(SystemTime::now()));
                tracing::debug!("pipelines read from {}", path.display());
                data.len()
            }
            _ => 0,
        };
        Some(Self { path, held })
    }

    /// Writes what the cache holds now, when it holds more than the file:
    /// pipelines made since it was read.
    pub fn save(&mut self, gpu: &Gpu) {
        let Ok(data) = gpu.pipeline_cache_data() else { return };
        if data.len() <= self.held {
            return;
        }
        match write(&self.path, &data) {
            Ok(()) => {
                self.held = data.len();
                if let Some(folder) = self.path.parent() {
                    tidy(folder);
                }
            }
            Err(error) => tracing::warn!("could not keep the pipelines in {}: {error}", self.path.display()),
        }
    }
}

/// Where the files are kept; `None` when caching is turned off.
fn folder() -> Option<PathBuf> {
    match std::env::var_os("VOXEAR_PIPELINE_CACHE") {
        Some(set) if set == "0" || set == "off" => return None,
        Some(set) if !set.is_empty() => return Some(PathBuf::from(set)),
        _ => {}
    }
    // A build from source keeps its own beside its build.
    let build = Path::new(env!("OUT_DIR"));
    if build.is_dir() {
        return Some(build.join("pipelines"));
    }
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        return Some(local.join("Voxear").join("Pipelines"));
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(cache.join("voxear").join("pipelines"))
}

/// Writes `data` to `path` whole: to a file of its own first, then put in
/// its place, so another program reading it never finds half of it.
fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let partial = path.with_extension(format!("{EXTENSION}.{}", std::process::id()));
    std::fs::write(&partial, data)?;
    std::fs::rename(&partial, path).inspect_err(|_| drop(std::fs::remove_file(&partial)))
}

/// Lets go of the files used longest ago past the [`KEPT`] most recent,
/// any not used for [`UNUSED_FOR`], and any left half written a day ago.
fn tidy(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let now = SystemTime::now();
    let mut kept: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else { continue };
        let age = now.duration_since(modified).unwrap_or_default();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.extension().is_some_and(|extension| extension == EXTENSION) {
            match age > UNUSED_FOR {
                true => drop(std::fs::remove_file(&path)),
                false => kept.push((modified, path)),
            }
        } else if name.contains(&format!(".{EXTENSION}.")) && age > Duration::from_secs(24 * 60 * 60) {
            // Part written by a program stopped while it wrote.
            let _ = std::fs::remove_file(&path);
        }
    }
    kept.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in kept.into_iter().skip(KEPT) {
        let _ = std::fs::remove_file(path);
    }
}

/// A 64-bit FNV-1a hash: quick, and the same on every run and machine.
pub(crate) fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_keeps_the_files_used_last_and_none_long_unused() {
        let folder = std::env::temp_dir().join(format!("voxear-pipeline-tidy-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let day = Duration::from_secs(24 * 60 * 60);
        let now = SystemTime::now();
        let made = |name: &str, ago: Duration| {
            let path = folder.join(name);
            std::fs::write(&path, b"kept").unwrap();
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(now - ago).unwrap();
        };
        for (at, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            made(&format!("{name}.{EXTENSION}"), day * at as u32);
        }
        made(&format!("old.{EXTENSION}"), day * 40);
        made(&format!("a.{EXTENSION}.4242"), day * 2);
        made(&format!("b.{EXTENSION}.4243"), Duration::from_secs(60));
        tidy(&folder);
        let mut left: Vec<String> =
            std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        std::fs::remove_dir_all(&folder).unwrap();
        let expected = ["a.pipelines", "b.pipelines", "b.pipelines.4243", "c.pipelines", "d.pipelines"];
        assert_eq!(left, expected, "the four used last, and a file being written now");
    }

    #[test]
    fn a_file_is_written_whole_in_place_of_the_old() {
        let folder = std::env::temp_dir().join(format!("voxear-pipeline-write-{}", std::process::id()));
        let path = folder.join(format!("x.{EXTENSION}"));
        write(&path, b"first").unwrap();
        write(&path, b"second, longer").unwrap();
        let read = std::fs::read(&path).unwrap();
        let files = std::fs::read_dir(&folder).unwrap().count();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!((read.as_slice(), files), (&b"second, longer"[..], 1), "no part-written file left over");
    }
}
