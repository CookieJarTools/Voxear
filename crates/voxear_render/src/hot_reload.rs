//! Shader hot reload for development checkouts: edit a `.slang` file and the
//! running engine recompiles it and swaps pipelines. A compile error is
//! logged and the previous shaders keep running.

use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, bail};

include!("../shader_build.rs");

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Compiled SPIR-V by shader name (`cull`, `forward`, ...).
pub struct ShaderSet {
    spirv: HashMap<String, Cow<'static, [u8]>>,
}

impl ShaderSet {
    /// The shaders compiled into the binary at build time.
    pub fn embedded() -> Self {
        let embedded: [(&str, &'static [u8]); 19] = [
            ("cull", include_bytes!(concat!(env!("OUT_DIR"), "/cull.spv"))),
            ("pyramid", include_bytes!(concat!(env!("OUT_DIR"), "/pyramid.spv"))),
            ("shadow", include_bytes!(concat!(env!("OUT_DIR"), "/shadow.spv"))),
            ("forward", include_bytes!(concat!(env!("OUT_DIR"), "/forward.spv"))),
            ("sky", include_bytes!(concat!(env!("OUT_DIR"), "/sky.spv"))),
            ("clusters", include_bytes!(concat!(env!("OUT_DIR"), "/clusters.spv"))),
            ("post", include_bytes!(concat!(env!("OUT_DIR"), "/post.spv"))),
            ("exposure", include_bytes!(concat!(env!("OUT_DIR"), "/exposure.spv"))),
            ("ui", include_bytes!(concat!(env!("OUT_DIR"), "/ui.spv"))),
            ("preview", include_bytes!(concat!(env!("OUT_DIR"), "/preview.spv"))),
            ("debug", include_bytes!(concat!(env!("OUT_DIR"), "/debug.spv"))),
            ("meshlets", include_bytes!(concat!(env!("OUT_DIR"), "/meshlets.spv"))),
            ("meshlet_task", include_bytes!(concat!(env!("OUT_DIR"), "/meshlet_task.spv"))),
            ("meshlet_shadow", include_bytes!(concat!(env!("OUT_DIR"), "/meshlet_shadow.spv"))),
            ("ambient_occlusion", include_bytes!(concat!(env!("OUT_DIR"), "/ambient_occlusion.spv"))),
            ("volumetric", include_bytes!(concat!(env!("OUT_DIR"), "/volumetric.spv"))),
            ("ddgi", include_bytes!(concat!(env!("OUT_DIR"), "/ddgi.spv"))),
            ("ddgi_software", include_bytes!(concat!(env!("OUT_DIR"), "/ddgi_software.spv"))),
            ("glass", include_bytes!(concat!(env!("OUT_DIR"), "/glass.spv"))),
        ];
        Self { spirv: embedded.into_iter().map(|(name, bytes)| (name.to_string(), Cow::Borrowed(bytes))).collect() }
    }

    pub fn get(&self, name: &str) -> Result<&[u8]> {
        self.spirv.get(name).map(|bytes| bytes.as_ref()).with_context(|| format!("no shader named {name}"))
    }
}

pub struct ShaderWatcher {
    dir: PathBuf,
    out: PathBuf,
    stamps: HashMap<PathBuf, SystemTime>,
    last_poll: Instant,
}

impl ShaderWatcher {
    /// Watches this crate's `shaders/` folder when it exists (a source
    /// checkout) and `slangc` runs.
    pub fn new() -> Option<Self> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
        if !dir.is_dir() || Command::new(slangc()).arg("-v").output().is_err() {
            return None;
        }
        let out = std::env::temp_dir().join(format!("voxear-shaders-{}", std::process::id()));
        let mut watcher = Self { stamps: HashMap::new(), last_poll: Instant::now(), dir, out };
        watcher.changed();
        tracing::info!("watching {} for shader changes", watcher.dir.display());
        Some(watcher)
    }

    /// Recompiles everything when any shader file changed since the last poll.
    pub fn poll(&mut self) -> Option<Result<ShaderSet>> {
        if self.last_poll.elapsed() < POLL_INTERVAL {
            return None;
        }
        self.last_poll = Instant::now();
        self.changed().then(|| self.compile())
    }

    fn changed(&mut self) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return false };
        let mut changed = false;
        for path in entries.filter_map(|e| Some(e.ok()?.path())) {
            if path.extension().and_then(|e| e.to_str()) != Some("slang") {
                continue;
            }
            let Ok(modified) = path.metadata().and_then(|m| m.modified()) else { continue };
            changed |= self.stamps.insert(path, modified) != Some(modified);
        }
        changed
    }

    /// Compiles every entry-point file at once, one `slangc` process each.
    fn compile(&self) -> Result<ShaderSet> {
        std::fs::create_dir_all(&self.out)?;
        let files: Vec<(String, &PathBuf)> = self
            .stamps
            .keys()
            .map(|path| (path.file_stem().unwrap_or_default().to_string_lossy().into_owned(), path))
            .filter(|(stem, _)| !SHADER_MODULES.contains(&stem.as_str()))
            .collect();
        let compiled: Vec<Result<(String, Vec<u8>)>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = files.iter().map(|(stem, path)| scope.spawn(|| self.compile_one(stem, path))).collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap_or_else(|_| bail!("shader compiler thread panicked")))
                .collect()
        });
        let spirv =
            compiled.into_iter().map(|r| r.map(|(stem, bytes)| (stem, Cow::Owned(bytes)))).collect::<Result<_>>()?;
        Ok(ShaderSet { spirv })
    }

    fn compile_one(&self, stem: &str, path: &Path) -> Result<(String, Vec<u8>)> {
        let target = self.out.join(format!("{stem}.spv"));
        let output = Command::new(slangc())
            .arg(path)
            .args(SLANG_ARGS)
            .arg("-I")
            .arg(&self.dir)
            .arg("-o")
            .arg(&target)
            .output()?;
        if !output.status.success() {
            bail!("{stem}.slang:\n{}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok((stem.to_string(), std::fs::read(&target)?))
    }
}

impl Drop for ShaderWatcher {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.out);
    }
}

fn slangc() -> String {
    std::env::var("SLANGC").unwrap_or_else(|_| "slangc".into())
}
