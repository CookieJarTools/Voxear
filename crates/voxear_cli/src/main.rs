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

use voxear_engine::net::NetStart;
use voxear_net::Conditions;
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use voxear_assets::AssetKind;
use voxear_core::math::EulerRot;
use voxear_engine::{
    Content, Engine, Game, PROJECT_FILE, Project, StatsOverlay, WindowOptions, build_profile,
    content_build::{self, CodeRoot},
    core_assembly,
    localization::{self, StringTable},
    player_build::{self, BUILD_STAGES, BuildEvent},
    run_windowed,
    scripts::{self, CONFIGURATION},
};
use voxear_render::{
    DEBUG_VIEW_AMBIENT_OCCLUSION, DEBUG_VIEW_CASCADES, DEBUG_VIEW_GLOBAL_ILLUMINATION, DEBUG_VIEW_LIGHT_COMPLEXITY,
    DEBUG_VIEW_LOD, DEBUG_VIEW_MESHLETS, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, PostProcess, Renderer, RendererSettings,
    ViewOptions,
};
use voxear_scene::{Name, Transform};
use voxear_scripting::find_dotnet;

mod package;

/// Counts Rust's heap, for the memory profiler.
#[global_allocator]
static ALLOCATOR: voxear_core::memory::CountingAllocator = voxear_core::memory::CountingAllocator;

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
enum LocCommand {
    /// Write a string table as a CSV sheet for translators, a row a key and a column a language.
    Export {
        /// The `.strings` file.
        table: PathBuf,
        /// Where to write the sheet (default: beside the table, as a `.csv`).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Read a CSV sheet into a string table, saying what it adds, changes and takes away.
    Import {
        /// The sheet, with a header naming `key`, `comment`, `limit` and each language's column: a CSV file, or a
        /// link to one or to a Google Sheet, which the table then keeps.
        sheet: String,
        /// The `.strings` file it goes into.
        table: PathBuf,
        /// Keep the sheet's text; without it, only say what would change.
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum LutCommand {
    /// Write a neutral colour lookup table, which changes nothing, to grade in an image editor.
    Neutral {
        /// The PNG to write.
        out: PathBuf,
        /// Its squares' size: 16 (256×16) or 32 (1024×32), finer.
        #[arg(long, default_value_t = 32)]
        size: u32,
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
    /// Move string tables to and from the CSV sheets translators work in.
    Loc {
        #[command(subcommand)]
        command: LocCommand,
    },
    /// Make colour lookup tables for post-processing's grading.
    Lut {
        #[command(subcommand)]
        command: LutCommand,
    },
    /// Add, remove and update the project's packages, or start and publish one.
    Package {
        #[command(subcommand)]
        command: package::PackageCommand,
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
        /// Serve the game without a window, until stopped.
        #[arg(long, conflicts_with_all = ["host", "connect"])]
        server: bool,
        /// Host the game: serve it and play it too.
        #[arg(long, conflicts_with = "connect")]
        host: bool,
        /// Join the server at this address, such as 127.0.0.1:7777.
        #[arg(long, value_name = "ADDRESS")]
        connect: Option<std::net::SocketAddr>,
        /// The port a server or host listens on; the project's `[network]` port when not given.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Play through a simulated network this bad, to test with.
        #[arg(long, value_enum, default_value_t = Network::Perfect)]
        network: Network,
        /// The window's title, such as the editor's "Player 2"; the project's name when not given.
        #[arg(long)]
        title: Option<String>,
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
        /// Show `lod`, shadow `cascades`, `lights` per cluster, `normals`, `meshlets`, ambient occlusion (`ao`) or
        /// the light the GI's probes find (`gi`) instead of normal shading.
        #[arg(
            long,
            value_parser = ["none", "lod", "cascades", "lights", "normals", "meshlets", "ao", "gi"],
            default_value = "none"
        )]
        debug_view: String,
        /// Draw whole LODs rather than culling each draw's meshlets, to compare.
        #[arg(long)]
        no_meshlet_culling: bool,
        /// Cull meshlets with a compute pass and indirect draws even where the GPU has mesh shaders, to compare.
        #[arg(long)]
        no_mesh_shaders: bool,
        /// Draw the scene at this share of the image's size, from 0.25 to 1, for TAA to upscale.
        #[arg(long)]
        render_scale: Option<f32>,
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
        /// Also save every this many frames as a sequence, numbered beside --out (frame_0060.png, ...), for
        /// watching what changes over time, such as the eye adapting.
        #[arg(long, value_name = "FRAMES")]
        every: Option<u64>,
        /// Frames a second to simulate, as on a display this fast: above the tick rate, what ticks move is shown
        /// between ticks.
        #[arg(long, value_name = "HZ", default_value_t = 60.0)]
        rate: f32,
        /// Look through the camera on the Obj with this name, rather than the first camera found.
        #[arg(long, value_name = "OBJ")]
        camera: Option<String>,
        /// Trace the GI's rays in a compute shader, as GPUs without ray tracing do, even on one that has it.
        #[arg(long)]
        software_rays: bool,
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
        Commands::Loc { command: LocCommand::Export { table, out } } => export_strings(&table, out.as_deref()),
        Commands::Loc { command: LocCommand::Import { sheet, table, apply } } => import_strings(&sheet, &table, apply),
        Commands::Package { command } => package::run(command),
        Commands::Lut { command: LutCommand::Neutral { out, size } } => write_neutral_lut(&out, size),
        Commands::Run {
            project,
            headless,
            frames,
            no_build,
            renderdoc,
            bundles,
            overlays,
            stats,
            server,
            host,
            connect,
            port,
            network,
            title,
        } => {
            let prepare = Prepare { build: !no_build, bundles, overlays };
            let net = match (server, host, connect) {
                (true, ..) => Some(NetStart::Server(port)),
                (_, true, _) => Some(NetStart::Host(port)),
                (.., Some(address)) => Some(NetStart::Connect(address)),
                _ => None,
            };
            let net = net.map(|start| (start, network.conditions()));
            run(&project, headless, frames, prepare, (renderdoc, stats), net, title)
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
            no_meshlet_culling,
            no_mesh_shaders,
            render_scale,
            every,
            rate,
            software_rays,
            camera,
        } => {
            let debug_view = match debug_view.as_str() {
                "lod" => DEBUG_VIEW_LOD,
                "cascades" => DEBUG_VIEW_CASCADES,
                "lights" => DEBUG_VIEW_LIGHT_COMPLEXITY,
                "normals" => DEBUG_VIEW_NORMALS,
                "meshlets" => DEBUG_VIEW_MESHLETS,
                "ao" => DEBUG_VIEW_AMBIENT_OCCLUSION,
                "gi" => DEBUG_VIEW_GLOBAL_ILLUMINATION,
                _ => DEBUG_VIEW_NONE,
            };
            let post_override = post.as_deref().map(PostProcess::preset).transpose()?;
            let meshlet_culling = !no_meshlet_culling;
            let options =
                ViewOptions { debug_view, post_override, meshlet_culling, render_scale, ..ViewOptions::default() };
            let prepare = Prepare { build: !no_build, bundles, overlays };
            let mesh_shaders = !no_mesh_shaders;
            let delta = 1.0 / rate.clamp(1.0, 1000.0);
            let size = (width, height);
            let shot = Shot { out, size, frames, every, delta, options, stats, mesh_shaders, software_rays, camera };
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

fn run(
    path: &Path,
    headless: bool,
    frames: Option<u64>,
    prepare: Prepare,
    (renderdoc, stats): (bool, bool),
    net: Option<(NetStart, Conditions)>,
    title: Option<String>,
) -> Result<()> {
    let project = Project::open(path)?;
    let settings = RendererSettings { hot_reload: shaders_reload(), ..renderer_settings(&project, renderdoc) };
    let title = match title {
        Some(title) => format!("{title} — {}", project.name()),
        None => format!("{} — Voxear", project.name()),
    };
    let mut game = start_game(project, prepare)?;
    let serving = matches!(net, Some((NetStart::Server(_), _)));

    if !headless && !serving {
        // A window is for playing while working on the game, so changed scripts reload into it.
        game.watch_scripts();
        let setup = Box::new(move |game: &mut Game, renderer: &mut Renderer| {
            game.start(Some(renderer))?;
            if let Some((start, conditions)) = net {
                game.start_session(start, conditions)?;
            }
            Ok(())
        });
        let console = Some(project_console(&game.project.root));
        let options = WindowOptions { frames, stats, console, ..WindowOptions::tools(&title) };
        return run_windowed(game, options, settings, setup);
    }
    // Without a window, sound plays out in silence.
    game.audio = voxear_engine::audio::GameAudio::new(voxear_engine::audio::Audio::quiet());
    game.start(None)?;
    if let Some((start, conditions)) = net {
        game.start_session(start, conditions)?;
    }
    if serving {
        voxear_engine::player::run_server(&mut game, frames);
        return Ok(());
    }
    if net.is_some() {
        voxear_engine::player::run_on_time(&mut game, frames);
        print_objs(&game.engine);
        return Ok(());
    }
    for _ in 0..frames.unwrap_or(3) {
        game.tick(1.0 / 60.0, None);
    }
    print_objs(&game.engine);
    Ok(())
}

/// How bad a network `vox run` plays through, to test with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum Network {
    Perfect,
    Good,
    Average,
    Bad,
}

impl Network {
    fn conditions(self) -> Conditions {
        match self {
            Network::Perfect => Conditions::PERFECT,
            Network::Good => Conditions::GOOD,
            Network::Average => Conditions::AVERAGE,
            Network::Bad => Conditions::BAD,
        }
    }
}

/// Where frame `frame` of a sequence saved beside `out` goes: `out`'s name
/// with the frame's number, as `frame_0060.png` beside `frame.png`.
fn sequence_path(out: &Path, frame: u64) -> PathBuf {
    let stem = out.file_stem().map_or_else(|| "frame".into(), |stem| stem.to_string_lossy().into_owned());
    out.with_file_name(format!("{stem}_{frame:04}.png"))
}

/// What `vox render` draws and where it saves it.
struct Shot {
    out: PathBuf,
    size: (u32, u32),
    /// Frames to simulate before the one saved.
    frames: u64,
    /// Save every this many frames too, as a numbered sequence.
    every: Option<u64>,
    /// Seconds each frame simulates.
    delta: f32,
    options: ViewOptions,
    /// Draw the stats overlay over the image.
    stats: bool,
    /// Cull meshlets with task and mesh shaders, where the GPU has them.
    mesh_shaders: bool,
    /// The Obj whose camera to look through, by name.
    camera: Option<String>,
    /// Trace the GI's rays in a compute shader even on a GPU with ray tracing.
    software_rays: bool,
}

/// How many of a render's last frames its timings are averaged over.
const TIMED_FRAMES: u64 = 60;

/// Each pass's GPU time added up over frames, by its place in the frame,
/// as passes such as Bloom down come several times.
#[derive(Default)]
struct TimingAverage {
    passes: Vec<(String, f32)>,
    frames: u32,
}

impl TimingAverage {
    fn add(&mut self, timings: &[(String, f32)]) {
        let same = self.passes.len() == timings.len()
            && self.passes.iter().zip(timings).all(|((name, _), (pass, _))| name == pass);
        if !same {
            // The frame's passes changed, as when an effect turned on: count from this frame.
            self.passes = timings.iter().map(|(name, _)| (name.clone(), 0.0)).collect();
            self.frames = 0;
        }
        for ((_, total), (_, ms)) in self.passes.iter_mut().zip(timings) {
            *total += ms;
        }
        self.frames += 1;
    }

    fn average(&self) -> Vec<(String, f32)> {
        let frames = self.frames.max(1) as f32;
        self.passes.iter().map(|(name, total)| (name.clone(), total / frames)).collect()
    }
}

fn render(path: &Path, shot: Shot, prepare: Prepare, renderdoc: bool) -> Result<()> {
    let Shot { out, size: (width, height), frames, every, delta, options, stats, mesh_shaders, software_rays, camera } =
        shot;
    let project = Project::open(path)?;
    let settings = RendererSettings { software_rays, ..renderer_settings(&project, renderdoc) };
    let mut game = start_game(project, prepare)?;
    let mut renderer = Renderer::new_offscreen(width, height, &settings)?;
    renderer.set_view_options(options);
    renderer.set_mesh_shaders(mesh_shaders);
    game.start(Some(&mut renderer))?;
    let mut looking_through = None;
    if let Some(name) = &camera {
        let world = &game.engine.world;
        let named = world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == *name);
        let (found, _) = named.with_context(|| format!("no Obj named {name} to look through"))?;
        renderer.set_view_options(ViewOptions { camera: Some(found), ..options });
        looking_through = Some(found);
    }
    let mut overlay = StatsOverlay::new(stats);
    let frames = frames.max(1);
    // Each pass's time over the last frames, averaged, as one frame's alone varies by a lot.
    let mut timed = TimingAverage::default();
    let mut frame_gpu = Vec::new();
    for frame in 1..=frames {
        game.tick(delta, Some(&mut renderer));
        // UI Panels in the world, with nothing pointing at them.
        let view = [0.0, 0.0, width as f32, height as f32];
        game.show_panels(&mut renderer, (view, looking_through), None, delta)?;
        if stats {
            // Refreshed every frame, since a render is over too soon to wait.
            overlay.update(delta, &renderer);
            overlay.refresh(&renderer);
            overlay.draw(&mut renderer, (width, height), 1.0)?;
        }
        let capture = frame == frames && renderer.has_renderdoc();
        if capture {
            renderer.start_capture();
        }
        renderer.render(&game.engine.world, game.engine.time().elapsed as f32)?;
        if frame + TIMED_FRAMES > frames {
            timed.add(renderer.timings());
            frame_gpu.push(renderer.gpu_frame_ms());
        }
        if capture && let Some(path) = renderer.end_capture() {
            println!("RenderDoc capture: {}", path.display());
        }
        if every.is_some_and(|every| every > 0 && frame % every == 0) {
            let (width, height, pixels) = renderer.read_pixels()?;
            write_png(&sequence_path(&out, frame), width, height, &pixels)?;
        }
    }

    let (width, height, pixels) = renderer.read_pixels()?;
    write_png(&out, width, height, &pixels)?;
    println!("wrote {} ({width}×{height}) on {}", out.display(), renderer.gpu().name);
    let passes = timed.average();
    let total: f32 = passes.iter().map(|(_, ms)| ms).sum();
    println!("GPU time, averaged over the last {} frames:", timed.frames);
    for (pass, ms) in &passes {
        println!("  {pass:<16} {ms:>7.3} ms");
    }
    println!("  {:<16} {total:>7.3} ms", "passes added");
    let frame_ms = frame_gpu.iter().sum::<f32>() / frame_gpu.len().max(1) as f32;
    println!("  {:<16} {frame_ms:>7.3} ms", "frame");

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
    if stats.meshlets > 0 {
        let how = if renderer.uses_mesh_shaders() { "mesh shaders" } else { "compute" };
        println!(
            "meshlets ({how}): {} tested · {} out of view · {} facing away · {} occluded · {} drawn · {} in shadows",
            stats.meshlets,
            stats.meshlets_frustum_culled,
            stats.meshlets_cone_culled,
            stats.meshlets_occlusion_culled,
            stats.meshlets_drawn,
            stats.shadow_meshlets
        );
    }
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
        software_rays: false,
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

/// Writes a string table as a sheet, its columns in the order its
/// project lists its languages.
fn export_strings(table: &Path, out: Option<&Path>) -> Result<()> {
    let text = std::fs::read_to_string(table).with_context(|| format!("reading {}", table.display()))?;
    let strings = StringTable::parse(&text).with_context(|| format!("reading {}", table.display()))?;
    let languages = project_of(table).map(|p| p.file.localization.languages).unwrap_or_default();
    let out = out.map_or_else(|| table.with_extension("csv"), Path::to_path_buf);
    std::fs::write(&out, localization::csv::to_csv(&strings, &languages))
        .with_context(|| format!("writing {}", out.display()))?;
    println!("{}: {} keys, written to {}", table.display(), strings.entries.len(), out.display());
    Ok(())
}

/// Reads a sheet into a string table, or with `apply` unset, says what it
/// would add, change and take away.
fn import_strings(sheet: &str, table: &Path, apply: bool) -> Result<()> {
    let linked = sheet.starts_with("https://") || sheet.starts_with("http://");
    let text = match linked {
        true => voxear_localization::csv::fetch(sheet)?,
        false => std::fs::read_to_string(sheet).with_context(|| format!("reading {sheet}"))?,
    };
    let mut new = localization::csv::from_csv(&text).with_context(|| format!("reading {sheet}"))?;
    let old = match std::fs::read_to_string(table) {
        Ok(text) => StringTable::parse(&text).with_context(|| format!("reading {}", table.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => StringTable::default(),
        Err(error) => return Err(error).with_context(|| format!("reading {}", table.display())),
    };
    // A sheet from a link is kept as the table's, to import from again.
    new.sheet = if linked { sheet.to_string() } else { old.sheet.clone() };
    let changes = localization::csv::changes(&old, &new);
    println!("{}, from {sheet}:", table.display());
    for key in &changes.added {
        println!("  + {key}");
    }
    for (key, languages) in &changes.changed {
        let which = if languages.is_empty() { String::new() } else { format!(" ({})", languages.join(", ")) };
        println!("  ~ {key}{which}");
    }
    for key in &changes.removed {
        println!("  - {key}");
    }
    let (added, changed, removed) = (changes.added.len(), changes.changed.len(), changes.removed.len());
    match (changes.is_empty(), apply) {
        (true, _) => println!("Nothing changes."),
        (false, false) => println!("{added} added, {changed} changed, {removed} removed. Run with --apply to keep it."),
        (false, true) => {
            std::fs::write(table, new.write()).with_context(|| format!("writing {}", table.display()))?;
            println!("{added} added, {changed} changed, {removed} removed, and kept.");
        }
    }
    Ok(())
}

/// The project a file is in: the nearest folder above it with a
/// `voxear.toml`.
fn project_of(file: &Path) -> Option<Project> {
    let file = std::path::absolute(file).ok()?;
    let root = file.ancestors().skip(1).find(|folder| folder.join(PROJECT_FILE).is_file())?;
    Project::open(root).ok()
}

/// A lookup table that leaves every colour as it is: `size` squares of
/// `size`×`size`, blue picking the square, red across and green down, as
/// `[post.grading] lut` reads them.
fn write_neutral_lut(out: &Path, size: u32) -> Result<()> {
    anyhow::ensure!((2..=64).contains(&size), "a lookup table's size should be between 2 and 64, as 16 or 32");
    let level = |value: u32| (value * 255 + (size - 1) / 2) / (size - 1);
    let mut rgba = Vec::with_capacity((size * size * size * 4) as usize);
    for green in 0..size {
        for blue in 0..size {
            for red in 0..size {
                rgba.extend([level(red) as u8, level(green) as u8, level(blue) as u8, 255]);
            }
        }
    }
    let file = std::fs::File::create(out).with_context(|| format!("writing {}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size * size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header()?.write_image_data(&rgba)?;
    println!("Wrote {} ({}×{size}). Grade it in an image editor, then name it in [post.grading] lut.", out.display(), size * size);
    Ok(())
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
    println!("  cooked {} models and pictures, {} more from the cache", report.cooked, report.cached);
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
    let (db, report) = voxear_engine::package_assets::scan(&project.root)?;
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
            AssetKind::Sound => "sound",
            AssetKind::InputActions => "input",
            AssetKind::Strings => "strings",
            AssetKind::Paths => "paths",
            AssetKind::AssetTable => "assets",
            AssetKind::LightProfile => "ies",
            AssetKind::Lighting => "lighting",
            AssetKind::Shader => "shader",
            AssetKind::MotionSet => "motion set",
            AssetKind::GunHandling => "gun handling",
            AssetKind::Data => "data",
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

    // An install's shaders come built: only a checkout compiles them, and reloads them on save.
    if shaders_reload() {
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
