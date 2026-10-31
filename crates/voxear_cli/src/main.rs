//! `vox`: the Voxear command line.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use voxear_assets::{AssetDb, AssetKind};
use voxear_core::math::EulerRot;
use voxear_engine::{
    Content, Engine, Game, PROJECT_FILE, Project, StatsOverlay,
    content_build::{self, CodeRoot},
    core_assembly, run_windowed,
    scripts::{self, CONFIGURATION},
};
use voxear_render::{
    DEBUG_VIEW_CASCADES, DEBUG_VIEW_LIGHT_COMPLEXITY, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, PostProcess,
    Renderer, RendererSettings, ViewOptions,
};
use voxear_scene::{Name, Transform};
use voxear_scripting::find_dotnet;

#[derive(Parser)]
#[command(name = "vox", version, about = "The Voxear engine command line")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum ContentCommand {
    /// Pack the assets that levels and prefabs use into bundles, by which of them use each one.
    Build {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Check that this machine can build and run Voxear projects.
    Doctor,
    /// List a project's assets, giving new ones a GUID (`.meta` file).
    Assets {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Build or inspect a project's content bundles.
    Content {
        #[command(subcommand)]
        command: ContentCommand,
    },
    /// Run a project in a window, or headless for a number of frames.
    Run {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Simulate without a window or GPU.
        #[arg(long)]
        headless: bool,
        /// Stop after this many frames (headless defaults to 3).
        #[arg(long)]
        frames: Option<u64>,
        /// Skip building the C# scripts first.
        #[arg(long)]
        no_build: bool,
        /// Load RenderDoc so F12 captures a frame.
        #[arg(long)]
        renderdoc: bool,
        /// Read assets from built content bundles, as a player does. Builds the content first unless --no-build.
        #[arg(long)]
        bundles: bool,
        /// Built content to layer over the game's, such as a mod's (with --bundles). A project folder means its
        /// built content.
        #[arg(long = "overlay", value_name = "DIR")]
        overlays: Vec<PathBuf>,
        /// Show the stats overlay (F1 in a window).
        #[arg(long)]
        stats: bool,
    },
    /// Open a project in the editor.
    Editor {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Render the editor offscreen to this PNG instead of opening a window.
        #[arg(long, value_name = "PNG")]
        screenshot: Option<PathBuf>,
        #[arg(long, default_value_t = 1600)]
        width: u32,
        #[arg(long, default_value_t = 900)]
        height: u32,
        /// Frames to run before a screenshot.
        #[arg(long, default_value_t = 30)]
        frames: u32,
    },
    /// Render a project offscreen and save the image, with GPU timings.
    Render {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Where to write the PNG.
        #[arg(long, default_value = "frame.png")]
        out: PathBuf,
        #[arg(long, default_value_t = 1280)]
        width: u32,
        #[arg(long, default_value_t = 720)]
        height: u32,
        /// Frames to simulate first, so scripts and timings settle.
        #[arg(long, default_value_t = 10)]
        frames: u64,
        /// Skip building the C# scripts first.
        #[arg(long)]
        no_build: bool,
        /// Capture the final frame in RenderDoc.
        #[arg(long)]
        renderdoc: bool,
        /// Show `lod`, shadow `cascades`, `lights` per cluster or `normals` instead of normal shading.
        #[arg(long, value_parser = ["none", "lod", "cascades", "lights", "normals"], default_value = "none")]
        debug_view: String,
        /// Replace the scene's post-processing with a preset: `voxear`, `neutral` or `cinematic`.
        #[arg(long)]
        post: Option<String>,
        /// Read assets from built content bundles, as a player does. Builds the content first unless --no-build.
        #[arg(long)]
        bundles: bool,
        /// Built content to layer over the game's, such as a mod's (with --bundles). A project folder means its
        /// built content.
        #[arg(long = "overlay", value_name = "DIR")]
        overlays: Vec<PathBuf>,
        /// Show the stats overlay (F1 in a window).
        #[arg(long)]
        stats: bool,
    },
}

/// What to do before starting a project.
struct Prepare {
    /// Build the scripts, and the content when reading bundles.
    build: bool,
    /// Read assets from built bundles instead of the project folder.
    bundles: bool,
    /// Built content layered on top, topmost last.
    overlays: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // The editor keeps every log line for its Console.
    let log = match cli.command {
        Commands::Editor { .. } => Some(voxear_core::logging::init_capturing()),
        _ => {
            voxear_core::logging::init();
            None
        }
    };
    match cli.command {
        Commands::Editor { project, screenshot, width, height, frames } => {
            let log = log.expect("captured above");
            let project = Project::open(&project)?;
            let settings = RendererSettings { hot_reload: true, ..renderer_settings(&project, false) };
            match screenshot {
                Some(out) => {
                    voxear_editor::screenshot(project, &settings, log, &out, (width, height), frames)?;
                    println!("wrote {}", out.display());
                    Ok(())
                }
                None => voxear_editor::run_editor(project, settings, log),
            }
        }
        Commands::Doctor => doctor(),
        Commands::Assets { project } => assets(&project),
        Commands::Content { command: ContentCommand::Build { project } } => build_content(&project),
        Commands::Run { project, headless, frames, no_build, renderdoc, bundles, overlays, stats } => {
            let prepare = Prepare { build: !no_build, bundles, overlays };
            run(&project, headless, frames, prepare, renderdoc, stats)
        }
        Commands::Render {
            project,
            out,
            width,
            height,
            frames,
            no_build,
            renderdoc,
            debug_view,
            post,
            bundles,
            overlays,
            stats,
        } => {
            let debug_view = match debug_view.as_str() {
                "lod" => DEBUG_VIEW_LOD,
                "cascades" => DEBUG_VIEW_CASCADES,
                "lights" => DEBUG_VIEW_LIGHT_COMPLEXITY,
                "normals" => DEBUG_VIEW_NORMALS,
                _ => DEBUG_VIEW_NONE,
            };
            let post_override = post.as_deref().map(PostProcess::preset).transpose()?;
            let options = ViewOptions { debug_view, post_override, ..ViewOptions::default() };
            let prepare = Prepare { build: !no_build, bundles, overlays };
            let shot = Shot { out, size: (width, height), frames, options, stats };
            render(&project, shot, prepare, renderdoc)
        }
    }
}

fn run(path: &Path, headless: bool, frames: Option<u64>, prepare: Prepare, renderdoc: bool, stats: bool) -> Result<()> {
    let project = Project::open(path)?;
    let settings = RendererSettings { hot_reload: true, ..renderer_settings(&project, renderdoc) };
    let title = format!("{} — Voxear", project.name());
    let mut game = start_game(project, prepare)?;

    if !headless {
        let setup = Box::new(|game: &mut Game, renderer: &mut Renderer| game.start(Some(renderer)));
        return run_windowed(game, &title, frames, stats, settings, setup);
    }
    game.start(None)?;
    for _ in 0..frames.unwrap_or(3) {
        game.tick(1.0 / 60.0, None);
    }
    print_objs(&game.engine);
    Ok(())
}

/// What `vox render` draws and where it saves it.
struct Shot {
    out: PathBuf,
    size: (u32, u32),
    /// Frames to simulate before the one saved.
    frames: u64,
    options: ViewOptions,
    /// Draw the stats overlay over the image.
    stats: bool,
}

fn render(path: &Path, shot: Shot, prepare: Prepare, renderdoc: bool) -> Result<()> {
    let Shot { out, size: (width, height), frames, options, stats } = shot;
    let project = Project::open(path)?;
    let settings = renderer_settings(&project, renderdoc);
    let mut game = start_game(project, prepare)?;
    let mut renderer = Renderer::new_offscreen(width, height, &settings)?;
    renderer.set_view_options(options);
    game.start(Some(&mut renderer))?;
    let mut overlay = StatsOverlay::new(stats);
    let frames = frames.max(1);
    for frame in 1..=frames {
        game.tick(1.0 / 60.0, Some(&mut renderer));
        if stats {
            // Refreshed every frame, since a render is over too soon to wait.
            overlay.update(1.0 / 60.0, &renderer);
            overlay.refresh(&renderer);
            overlay.draw(&mut renderer, (width, height), 1.0)?;
        }
        let capture = frame == frames && renderer.has_renderdoc();
        if capture {
            renderer.start_capture();
        }
        renderer.render(&game.engine.world, game.engine.time().elapsed as f32)?;
        if capture && let Some(path) = renderer.end_capture() {
            println!("RenderDoc capture: {}", path.display());
        }
    }

    let (width, height, pixels) = renderer.read_pixels()?;
    write_png(&out, width, height, &pixels)?;
    println!("wrote {} ({width}×{height}) on {}", out.display(), renderer.gpu().name);
    let total: f32 = renderer.timings().iter().map(|(_, ms)| ms).sum();
    for (pass, ms) in renderer.timings() {
        println!("  {pass:<16} {ms:>7.3} ms");
    }
    println!("  {:<16} {total:>7.3} ms", "total");

    let stats = renderer.stats();
    println!(
        "culling: {} instances · {} outside the view · {} occluded · drawn {} early + {} late · {} shadow draws",
        stats.instances,
        stats.frustum_culled,
        stats.occlusion_culled,
        stats.drawn_early,
        stats.drawn_late,
        stats.shadow_draws
    );
    if stats.fade_draws > 0 {
        println!("LOD fades in progress: {}", stats.fade_draws);
    }
    println!(
        "lights: up to {} per cluster · {} light-cluster links",
        stats.cluster_max_lights, stats.cluster_light_links
    );
    let lods: Vec<String> =
        stats.lods.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| format!("LOD{i} {n}")).collect();
    println!("LODs: {}", if lods.is_empty() { "none".into() } else { lods.join(" · ") });
    if stats.average_luminance > 0.0 {
        println!("exposure: {:+.2} EV for an average luminance of {:.3}", stats.exposure_ev, stats.average_luminance);
    }
    Ok(())
}

/// RenderDoc captures land in the project's `.voxear/Captures`.
fn renderer_settings(project: &Project, renderdoc: bool) -> RendererSettings {
    RendererSettings {
        renderdoc,
        capture_folder: Some(project.root.join(".voxear").join("Captures")),
        hot_reload: false,
    }
}

/// Creates the game and, when the project has scripts, builds and loads them.
fn start_game(project: Project, prepare: Prepare) -> Result<Game> {
    let engine = start_engine(&project, !prepare.build)?;
    if !prepare.bundles {
        anyhow::ensure!(prepare.overlays.is_empty(), "--overlay layers built content, so it needs --bundles");
        return Game::new(project, engine);
    }
    let output = content_build::default_output(&project.root);
    if prepare.build {
        let report = content_build::build(&project.root, &output, &project.file.content, &code_roots(&engine)?)?;
        let written = report.bundles.iter().filter(|b| b.written).count();
        tracing::info!("built content: {} bundles, {written} written", report.bundles.len());
        warn_unmatched(&report);
    }
    let mut content = Content::open_bundles(&project.root, &output)?;
    for overlay in &prepare.overlays {
        // A project folder means the content built from it.
        let dir =
            if overlay.join(PROJECT_FILE).is_file() { content_build::default_output(overlay) } else { overlay.clone() };
        content.mount(&dir)?;
    }
    Ok(Game::with_content(project, engine, content))
}

fn build_content(path: &Path) -> Result<()> {
    let project = Project::open(path)?;
    // The scripts declare roots of their own with [ContentRoot].
    let code_roots = code_roots(&start_engine(&project, false)?)?;
    let started = std::time::Instant::now();
    let output = content_build::default_output(&project.root);
    let report = content_build::build(&project.root, &output, &project.file.content, &code_roots)?;
    let written = report.bundles.iter().filter(|b| b.written).count();
    let shown = output.strip_prefix(&project.root).unwrap_or(&output);
    println!(
        "{}: {} bundles in {} ({:.2} s), {written} written, {} unchanged",
        project.name(),
        report.bundles.len(),
        shown.display(),
        started.elapsed().as_secs_f32(),
        report.bundles.len() - written
    );
    println!("  cooked {} models, {} more from the cache", report.cooked, report.cached);
    for bundle in &report.bundles {
        println!("\n  {}  {:>8}  used by {}", &bundle.file[..8], size(bundle.size), bundle.roots.join(", "));
        for asset in &bundle.assets {
            println!("      {asset}");
        }
    }
    if !report.excluded.is_empty() {
        println!("\nleft out, since no level or prefab uses them:");
        for asset in &report.excluded {
            println!("      {asset}");
        }
    }
    if !code_roots.is_empty() {
        println!("\nroots declared in scripts:");
        for root in &code_roots {
            println!("      {}  ({})", root.pattern, root.source);
        }
    }
    warn_unmatched(&report);
    Ok(())
}

/// The `[ContentRoot]` patterns in the game's scripts, if it has any.
fn code_roots(engine: &Engine) -> Result<Vec<CodeRoot>> {
    let Some(scripts) = engine.scripts() else { return Ok(Vec::new()) };
    let roots = scripts.content_roots()?;
    Ok(roots.into_iter().map(|(pattern, source)| CodeRoot { pattern, source }).collect())
}

fn warn_unmatched(report: &content_build::BuildReport) {
    for root in &report.unmatched {
        tracing::warn!("[ContentRoot(\"{}\")] on {} matches no asset", root.pattern, root.source);
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

fn start_engine(project: &Project, no_build: bool) -> Result<Engine> {
    let mut engine = Engine::new();
    if !no_build {
        scripts::build(project)?;
    }
    if let Some(runtime) = scripts::start(project)? {
        engine.set_scripts(runtime);
    }
    Ok(engine)
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let file = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

fn print_objs(engine: &Engine) {
    println!("after {} frames ({:.3}s):", engine.time().frame, engine.time().elapsed);
    for (name, transform) in engine.world.query_ref::<(&Name, &Transform)>() {
        let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        println!("  {:<14} position {:>5.1?}  yaw {:>6.1}°", name.0, transform.position.to_array(), yaw.to_degrees());
    }
}

fn assets(path: &Path) -> Result<()> {
    let project = Project::open(path)?;
    let (db, report) = AssetDb::scan(&project.root)?;
    let mut entries: Vec<_> = db.entries().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    for entry in entries {
        let kind = match entry.kind {
            AssetKind::Model => "model",
            AssetKind::Texture => "texture",
            AssetKind::Material => "material",
            AssetKind::Scene => "scene",
            AssetKind::Level => "level",
            AssetKind::Prefab => "prefab",
        };
        println!("  {:<9} {}  {}", kind, entry.guid, entry.path);
    }
    println!("{} assets in {}", db.len(), project.name());
    for path in &report.created {
        println!("  new: {path} (wrote {path}.meta)");
    }
    for path in &report.reassigned {
        println!("  {path} shared a GUID with another asset and was given a new one");
    }
    Ok(())
}

fn doctor() -> Result<()> {
    println!("Voxear {} ({} {})\n", voxear_core::VERSION, std::env::consts::OS, std::env::consts::ARCH);
    let mut ready = true;

    println!("GPU (Vulkan 1.3 baseline)");
    match voxear_rhi::enumerate_gpus() {
        Ok(gpus) if !gpus.is_empty() => {
            for gpu in &gpus {
                ready &= report_gpu(gpu) || gpus.iter().any(|g| g.unsupported.is_none());
            }
        }
        Ok(_) => ready &= fail("no Vulkan devices found"),
        Err(error) => ready &= fail(&format!("{error:#}; install your GPU's Vulkan driver")),
    }

    println!("\n.NET (C# scripting)");
    match find_dotnet() {
        Some(dotnet) => {
            ok(&format!("hostfxr {} in {}", dotnet.hostfxr_version, dotnet.root.display()));
        }
        None => {
            ready &= fail(".NET not found");
            hint("Arch: sudo pacman -S dotnet-sdk    Windows: winget install Microsoft.DotNet.SDK.10");
        }
    }
    let core = core_assembly(CONFIGURATION);
    if core.is_file() {
        ok(&format!("Voxear.Core built at {}", core.display()));
    } else {
        println!("  · Voxear.Core not built yet (`vox run` builds it with the game scripts)");
    }

    println!("\nShader compiler");
    match tool_version("slangc", "-v") {
        Some(version) => {
            ok(&format!("slangc ({version})"));
        }
        None => {
            ready &= fail("slangc not found");
            hint("Arch: sudo pacman -S shader-slang    Windows: install the Vulkan SDK");
        }
    }

    println!("\n{}", if ready { "Ready to run Voxear." } else { "Fix the items marked ✗ above." });
    Ok(())
}

fn report_gpu(gpu: &voxear_rhi::GpuInfo) -> bool {
    let (major, minor, patch) = gpu.api_version;
    let mut tiers = Vec::new();
    if gpu.mesh_shaders {
        tiers.push("mesh shaders");
    }
    if gpu.ray_tracing {
        tiers.push("ray tracing");
    }
    if gpu.memory_decompression {
        tiers.push("GPU decompression");
    }
    let tiers = if tiers.is_empty() { String::new() } else { format!(", {}", tiers.join(", ")) };
    let line =
        format!("{} — Vulkan {major}.{minor}.{patch}, {}, {} MB{tiers}", gpu.name, gpu.driver, gpu.device_local_mb);
    match &gpu.unsupported {
        None => ok(&line),
        Some(reason) => fail(&format!("{line} ({reason})")),
    }
}

fn tool_version(tool: &str, flag: &str) -> Option<String> {
    let output = Command::new(tool).arg(flag).output().ok()?;
    let text = [output.stdout, output.stderr].concat();
    let text = String::from_utf8_lossy(&text);
    Some(text.lines().next().unwrap_or_default().trim().to_string())
}

fn ok(message: &str) -> bool {
    println!("  ✓ {message}");
    true
}

fn fail(message: &str) -> bool {
    println!("  ✗ {message}");
    false
}

fn hint(message: &str) {
    println!("    {message}");
}
