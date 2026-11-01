//! `vox`: the Voxear command line.

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use voxear_assets::{AssetDb, AssetKind};
use voxear_core::math::EulerRot;
use voxear_engine::{
    Content, Engine, Game, PROJECT_FILE, Project, StatsOverlay, WindowOptions, build_profile,
    content_build::{self, CodeRoot},
    core_assembly,
    player_build::{self, BUILD_STAGES, BuildEvent},
    run_windowed,
    scripts::{self, CONFIGURATION},
};
use voxear_render::{
    DEBUG_VIEW_CASCADES, DEBUG_VIEW_LIGHT_COMPLEXITY, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, PostProcess,
    Renderer, RendererSettings, ViewOptions,
};
use voxear_scene::{Name, Transform};
use voxear_scripting::find_dotnet;

#[derive(Parser)]
#[command(name = "vox", version = voxear_engine::engine_version(), about = "The Voxear engine command line")]
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
    /// Make a new project, as the launcher asks; `--describe --json` says what this version offers.
    New {
        /// The project folder itself, which must not exist or must be empty.
        #[arg(required_unless_present = "describe")]
        folder: Option<PathBuf>,
        /// Print the templates, setup items and IDEs this version offers.
        #[arg(long)]
        describe: bool,
        /// Report as JSON: one object for --describe, or one line per step.
        #[arg(long)]
        json: bool,
        /// The project's name (default: the folder's).
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "3d")]
        template: String,
        /// Setup items, comma-separated, or `none` (default: the template's own).
        #[arg(long, value_delimiter = ',')]
        setup: Option<Vec<String>>,
        /// Set up a C# IDE: rider, visual-studio or vscode.
        #[arg(long)]
        ide: Option<String>,
        /// Start a git repository, with a .gitignore and .gitattributes.
        #[arg(long)]
        git: bool,
        #[arg(long)]
        modding: bool,
        #[arg(long)]
        steam_app_id: Option<u64>,
    },
    /// Move a project onto this version of Voxear, as the launcher asks once it has backed it up.
    Upgrade {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// The version the project was pinned to, if it was.
        #[arg(long)]
        from: Option<String>,
        /// Report as JSON, one line per step and change, as the launcher reads it.
        #[arg(long)]
        json: bool,
    },
    /// Run a line in the console of a game running with its tools, from `vox run` or a development build, and print
    /// what it logged: `vox console "God true"`.
    Console {
        /// The line, as typed into the game's console; several words are joined with spaces.
        #[arg(required = true, num_args = 1..)]
        line: Vec<String>,
        /// Project folder or its voxear.toml, for a game from `vox run`.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// A development build's Data folder, for a game run from a build instead.
        #[arg(long, value_name = "DATA")]
        player: Option<PathBuf>,
    },
    /// List a project's assets, giving new ones a GUID (`.meta` file).
    Assets {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Build the game into a player with a build profile, as the editor's Build window does.
    Build {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// The build profile, by name (default: this platform's Release profile).
        #[arg(long)]
        profile: Option<String>,
        /// `json` reports one JSON object per line, as the editor reads; `cancel` on stdin stops the build.
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        progress: String,
        /// List the project's build profiles.
        #[arg(long)]
        list: bool,
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
    // The editor keeps every log line for its Console; a build for the editor
    // sends each as JSON.
    let log = match &cli.command {
        Commands::Editor { .. } => Some(voxear_core::logging::init_capturing()),
        // The launcher reads these commands' stdout, so their logs go to stderr.
        Commands::New { json: true, .. } | Commands::Upgrade { json: true, .. } => {
            voxear_core::logging::init_stderr();
            None
        }
        Commands::Build { progress, .. } if progress == "json" => {
            voxear_core::logging::init_forwarding(|line| {
                let level = line.level.as_str().to_lowercase();
                println!("{}", serde_json::json!({ "event": "log", "level": level, "message": line.message }));
            });
            None
        }
        _ => {
            voxear_core::logging::init();
            None
        }
    };
    match cli.command {
        Commands::Editor { project, screenshot, width, height, frames } => {
            let log = log.expect("captured above");
            let project = Project::open(&project)?;
            let settings = RendererSettings { hot_reload: shaders_reload(), ..renderer_settings(&project, false) };
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
        Commands::New { folder, describe, json, name, template, setup, ide, git, modding, steam_app_id } => {
            if describe {
                println!("{}", voxear_engine::new_project::describe());
                return Ok(());
            }
            let folder = folder.expect("clap asks for it without --describe");
            let name = name.unwrap_or_else(|| folder.file_name().unwrap_or_default().to_string_lossy().into_owned());
            let request = voxear_engine::new_project::NewProject {
                folder,
                name,
                template,
                setup,
                ide,
                git,
                modding,
                steam_app_id,
            };
            new_project(&request, json)
        }
        Commands::Assets { project } => assets(&project),
        Commands::Console { line, project, player } => console(&line.join(" "), &project, player.as_deref()),
        Commands::Upgrade { project, from, json } => upgrade(&project, from.as_deref(), json),
        Commands::Build { project, profile, progress, list } => build(&project, profile, progress == "json", list),
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

/// Where a game from `vox run` keeps its console's address: the project's
/// own `.voxear` folder.
fn project_console(root: &Path) -> PathBuf {
    root.join(".voxear")
}

fn console(line: &str, project: &Path, player: Option<&Path>) -> Result<()> {
    let folder = match player {
        Some(data) => data.to_path_buf(),
        None => project_console(&Project::open(project)?.root),
    };
    for said in voxear_engine::remote_console::send(&folder, line)? {
        println!("{said}");
    }
    Ok(())
}

fn run(path: &Path, headless: bool, frames: Option<u64>, prepare: Prepare, renderdoc: bool, stats: bool) -> Result<()> {
    let project = Project::open(path)?;
    let settings = RendererSettings { hot_reload: shaders_reload(), ..renderer_settings(&project, renderdoc) };
    let title = format!("{} — Voxear", project.name());
    let mut game = start_game(project, prepare)?;

    if !headless {
        // A window is for playing while working on the game, so changed scripts reload into it.
        game.watch_scripts();
        let setup = Box::new(|game: &mut Game, renderer: &mut Renderer| game.start(Some(renderer)));
        let console = Some(project_console(&game.project.root));
        let options = WindowOptions { frames, stats, console, ..WindowOptions::tools(&title) };
        return run_windowed(game, options, settings, setup);
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

/// Stops a build at its next step, as `cancel` on stdin asks.
#[derive(Debug)]
struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the build was cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// `vox build`: builds the game with a profile, writing the default
/// profiles first into a project that has none. With JSON, everything on
/// stdout is a JSON line, ending in `finished`, and `cancel` on stdin stops
/// the build at its next step.
fn build(path: &Path, profile: Option<String>, json: bool, list: bool) -> Result<()> {
    let project = Project::open(path)?;
    if build_profile::profile_names(&project.root).is_empty() {
        build_profile::write_default_profiles(&project.root)?;
        let folder = build_profile::PROFILES_FOLDER;
        tracing::info!("{} had no build profiles, so it has the default ones now, in {folder}", project.name());
    }
    if list {
        build_profile::profile_names(&project.root).iter().for_each(|name| println!("{name}"));
        return Ok(());
    }
    let name = match profile {
        Some(name) => name,
        None => build_profile::default_profile(&project.root).context("the project has no build profiles")?,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    if json {
        let cancelled = cancelled.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                if line.is_ok_and(|line| line.trim() == "cancel") {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }
        });
    }
    let line = |event: serde_json::Value| println!("{event}");
    let mut events = |event: BuildEvent| -> Result<()> {
        if cancelled.load(Ordering::Relaxed) {
            bail!(Cancelled);
        }
        match event {
            BuildEvent::Stage { stage, index } if json => {
                line(
                    serde_json::json!({ "event": "stage", "stage": stage, "index": index + 1, "of": BUILD_STAGES.len() }),
                );
            }
            BuildEvent::Stage { stage, index } => println!("[{}/{}] {stage}", index + 1, BUILD_STAGES.len()),
            BuildEvent::Progress { stage, done, total, item } if json => {
                line(
                    serde_json::json!({ "event": "progress", "stage": stage, "done": done, "total": total, "item": item }),
                );
            }
            BuildEvent::Progress { .. } => {}
        }
        Ok(())
    };
    let started = Instant::now();
    let player = player_build::sibling_program("voxear-player");
    let built = player_build::build(&project, &name, &player, &mut events);
    let seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    match built {
        Ok(report) if json => {
            let output = project.root.join(&report.output);
            line(serde_json::json!({
                "event": "finished", "result": "succeeded", "seconds": seconds, "output": output,
                "program": project.root.join(&report.program), "size": report.size,
                "report": output.join(player_build::REPORT_FILE),
            }));
        }
        Ok(report) => {
            let program = report.program.display();
            println!("Built {} with {name} in {seconds} s: {program} ({})", project.name(), size(report.size));
        }
        Err(error) if json => {
            let result = if error.downcast_ref::<Cancelled>().is_some() { "cancelled" } else { "failed" };
            let message = format!("{error:#}");
            line(serde_json::json!({ "event": "finished", "result": result, "seconds": seconds, "message": message }));
            std::process::exit(1);
        }
        Err(error) => return Err(error),
    }
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
            AssetKind::Ui => "ui",
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

/// Shaders reload on save only in a source checkout: an install has no
/// shader sources of its own, and must not pick up a checkout's.
fn shaders_reload() -> bool {
    !voxear_engine::is_install(&voxear_engine::engine_root())
}

/// `vox upgrade`: reports each step and change as it goes, as a line of
/// JSON for the launcher when asked, and ends with `done` or what went
/// wrong. With JSON, a failure is reported on stdout as well as by the exit
/// code.
fn upgrade(project: &Path, from: Option<&str>, json: bool) -> Result<()> {
    use voxear_engine::upgrade::Event;
    let line = |event: serde_json::Value| println!("{event}");
    let mut report = |event: Event| match (event, json) {
        (Event::Step(message), true) => line(serde_json::json!({ "event": "step", "message": message })),
        (Event::Change(message), true) => line(serde_json::json!({ "event": "change", "message": message })),
        (Event::Step(message), false) => println!("{message}"),
        (Event::Change(message), false) => println!("  · {message}"),
    };
    match voxear_engine::upgrade::upgrade(project, from, &mut report) {
        Ok(()) if json => line(serde_json::json!({ "event": "done" })),
        Ok(()) => println!("Upgraded to Voxear {}", voxear_engine::engine_version()),
        Err(error) if json => {
            line(serde_json::json!({ "event": "error", "message": format!("{error:#}") }));
            std::process::exit(1);
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// `vox new`: reports each step as it starts, as a line of JSON for the
/// launcher when asked, and ends with where the project is or what went
/// wrong. With JSON, a failure is reported on stdout as well as by the exit
/// code, so nothing else goes there.
fn new_project(request: &voxear_engine::new_project::NewProject, json: bool) -> Result<()> {
    let line = |event: serde_json::Value| println!("{event}");
    let mut step = |message: &str| {
        if json {
            line(serde_json::json!({ "event": "step", "message": message }));
        } else {
            println!("{message}");
        }
    };
    match voxear_engine::new_project::create(request, &mut step) {
        Ok(path) if json => line(serde_json::json!({ "event": "done", "path": path })),
        Ok(path) => println!("Made {}", path.display()),
        Err(error) if json => {
            line(serde_json::json!({ "event": "error", "message": format!("{error:#}") }));
            std::process::exit(1);
        }
        Err(error) => return Err(error),
    }
    Ok(())
}
