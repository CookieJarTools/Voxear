//! Shader hot reload for development checkouts: edit a `.slang` file and the
//! running engine recompiles it and swaps pipelines. A compile error is
//! logged and the previous shaders keep running.
//!
//! A project's custom shaders compile here too ([`CustomCompile`]): the
//! modules that draw materials, again, with `VOXEAR_CUSTOM_SHADERS` defined
//! and the project's `project_shaders.slang` on the include path. That needs the
//! Slang compiler, shipped beside the engine (`slang/bin/` next to its
//! executable), and the engine's shader sources (`shaders/` beside it, or
//! this crate's in a source checkout).

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
        let embedded: [(&str, &'static [u8]); 32] = [
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
            ("foliage_push", include_bytes!(concat!(env!("OUT_DIR"), "/foliage_push.spv"))),
            ("tracks", include_bytes!(concat!(env!("OUT_DIR"), "/tracks.spv"))),
            ("erosion", include_bytes!(concat!(env!("OUT_DIR"), "/erosion.spv"))),
            ("water", include_bytes!(concat!(env!("OUT_DIR"), "/water.spv"))),
            ("particles", include_bytes!(concat!(env!("OUT_DIR"), "/particles.spv"))),
            ("particle_sim", include_bytes!(concat!(env!("OUT_DIR"), "/particle_sim.spv"))),
            ("smoke", include_bytes!(concat!(env!("OUT_DIR"), "/smoke.spv"))),
            ("dice", include_bytes!(concat!(env!("OUT_DIR"), "/dice.spv"))),
            ("skinning", include_bytes!(concat!(env!("OUT_DIR"), "/skinning.spv"))),
            ("terrain", include_bytes!(concat!(env!("OUT_DIR"), "/terrain.spv"))),
            ("grass", include_bytes!(concat!(env!("OUT_DIR"), "/grass.spv"))),
            ("blocks", include_bytes!(concat!(env!("OUT_DIR"), "/blocks.spv"))),
            ("map", include_bytes!(concat!(env!("OUT_DIR"), "/map.spv"))),
        ];
        Self { spirv: embedded.into_iter().map(|(name, bytes)| (name.to_string(), Cow::Borrowed(bytes))).collect() }
    }

    pub fn get(&self, name: &str) -> Result<&[u8]> {
        self.spirv.get(name).map(|bytes| bytes.as_ref()).with_context(|| format!("no shader named {name}"))
    }

    /// These shaders, with some compiled again in their place.
    pub fn replaced(&self, modules: &[(String, Vec<u8>)]) -> Self {
        let mut spirv = self.spirv.clone();
        for (name, bytes) in modules {
            spirv.insert(name.clone(), Cow::Owned(bytes.clone()));
        }
        Self { spirv }
    }
}

/// The engine's shader sources: `shaders/` beside the executable, as the
/// engine is installed, or this crate's own in a source checkout.
pub fn engine_shader_dir() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join("shaders");
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    [beside, checkout].into_iter().find(|dir| dir.join("custom.slang").is_file())
}

/// A project's custom shaders as generated code: its material shaders, the
/// engine's `project_shaders.slang`, and its UI shaders,
/// `project_ui_shaders.slang`, which defines `customUi` (see
/// `shaders/ui_custom.slang`). Empty UI code compiles to none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomCode {
    pub materials: String,
    pub ui: String,
}

impl From<String> for CustomCode {
    fn from(materials: String) -> Self {
        Self { materials, ui: String::new() }
    }
}

impl From<&str> for CustomCode {
    fn from(materials: &str) -> Self {
        Self::from(materials.to_string())
    }
}

/// The UI's `customUi` when a project has no UI shaders of its own.
const NO_UI_SHADERS: &str = "float4 customUi(uint shader, uint first, UiInput input)\n{\n    return input.background;\n}\n";

/// A project's custom shaders compiling on a thread of their own, into the
/// modules that draw materials and the UI.
pub struct CustomCompile {
    job: std::thread::JoinHandle<Result<Vec<(String, Vec<u8>)>>>,
}

impl CustomCompile {
    /// Starts compiling a project's `code` in `dir`, with the engine's
    /// shaders.
    pub fn start(code: CustomCode, dir: PathBuf) -> Self {
        Self { job: std::thread::spawn(move || compile_custom_shaders(code, &dir)) }
    }

    /// The modules compiled, once done; an error says which shader failed
    /// and where.
    pub fn finished(&mut self) -> bool {
        self.job.is_finished()
    }

    pub fn take(self) -> Result<Vec<(String, Vec<u8>)>> {
        self.job.join().unwrap_or_else(|_| bail!("the shader compiler thread panicked"))
    }
}

/// Compiles the modules that draw materials and the UI with a project's
/// `code`, its `project_shaders.slang` and `project_ui_shaders.slang`
/// written into `dir`, a module a thread, as a game build does ahead of
/// time and [`CustomCompile`] while editing.
pub fn compile_custom_shaders(code: impl Into<CustomCode>, dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let code = code.into();
    let engine = engine_shader_dir().context("the engine's shader sources are not installed beside it")?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("project_shaders.slang"), &code.materials)?;
    let ui = if code.ui.is_empty() { NO_UI_SHADERS } else { &code.ui };
    std::fs::write(dir.join("project_ui_shaders.slang"), ui)?;
    let compiled: Vec<Result<(String, Vec<u8>)>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = CUSTOM_SHADER_MODULES
            .iter()
            .map(|&stem| {
                let engine = &engine;
                scope.spawn(move || {
                    let target = dir.join(format!("{stem}.spv"));
                    let output = Command::new(slangc())
                        .arg(engine.join(format!("{stem}.slang")))
                        .args(SLANG_ARGS)
                        .args(["-D", "VOXEAR_CUSTOM_SHADERS"])
                        .args(["-I".as_ref(), dir.as_os_str(), "-I".as_ref(), engine.as_os_str()])
                        .arg("-o")
                        .arg(&target)
                        .output()
                        .context("running the Slang compiler")?;
                    if !output.status.success() {
                        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Ok((stem.to_string(), std::fs::read(&target)?))
                })
            })
            .collect();
        jobs.into_iter().map(|job| job.join().unwrap_or_else(|_| bail!("shader compiler thread panicked"))).collect()
    });
    // Every module includes the same shaders, so the first error says it all.
    compiled.into_iter().collect()
}

pub struct ShaderWatcher {
    dir: PathBuf,
    out: PathBuf,
    /// Where a project's `project_shaders.slang` is, when it has custom
    /// shaders, compiled in when the modules are compiled again.
    pub custom: Option<PathBuf>,
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
        let mut watcher = Self { stamps: HashMap::new(), last_poll: Instant::now(), dir, out, custom: None };
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
        let custom = self.custom.iter().flat_map(|custom| {
            ["-D".as_ref(), "VOXEAR_CUSTOM_SHADERS".as_ref(), "-I".as_ref(), custom.as_os_str()]
        });
        let output = Command::new(slangc())
            .arg(path)
            .args(SLANG_ARGS)
            .args(custom)
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

/// The Slang compiler: `SLANGC`, else the one shipped beside the executable
/// (`slang/bin/`, its libraries in `slang/lib/` where it looks for them),
/// else `slangc` on the path.
fn slangc() -> String {
    if let Ok(path) = std::env::var("SLANGC") {
        return path;
    }
    let name = if cfg!(windows) { "slangc.exe" } else { "slangc" };
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("slang").join("bin").join(name)));
    match beside.filter(|path| path.is_file()) {
        Some(path) => path.to_string_lossy().into_owned(),
        None => "slangc".into(),
    }
}
