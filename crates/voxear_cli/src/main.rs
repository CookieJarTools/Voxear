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
    DEBUG_VIEW_LIGHTMAP_DENSITY,
    DEBUG_VIEW_LOD, DEBUG_VIEW_MESHLETS, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, PostProcess, Renderer, RendererSettings,
    ViewOptions,
};
use voxear_scene::{Name, Transform};
use voxear_scripting::find_dotnet;

mod editor_link;
mod mcp;
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
enum ScriptsCommand {
    /// Build the project's C# scripts as `vox run` does before it starts, printing the compiler's errors; then
    /// `vox run --no-build` starts at once, as a debugger starting the game wants.
    Build {
        /// Project folder or its voxear.toml.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Subcommand)]
enum EditorCommand {
    /// Make a call to the editor open on the project and print its answer as JSON: `vox editor call obj.get
    /// obj=<scene>:3`. `--describe` lists every call.
    Call {
        /// The call, as `editor.state`.
        #[arg(required_unless_present = "describe")]
        call: Option<String>,
        /// Its arguments: `name=value` pairs, each value JSON or else text, or one JSON object. `batch` with none
        /// reads its calls from stdin, a JSON object a line.
        args: Vec<String>,
        /// Project folder or its voxear.toml.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// With no editor open, open one without a window for the call, on the project's files.
        #[arg(long)]
        headless: bool,
        /// List every call with its arguments, and the exit codes.
        #[arg(long)]
        describe: bool,
        /// With --describe, as JSON, adding the components, commands and kinds of the editor open.
        #[arg(long)]
        json: bool,
    },
    /// Run the editor without a window, answering `vox editor call` until a call closes it.
    Serve {
        /// Project folder or its voxear.toml.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// How big the editor draws, as its captures come out.
        #[arg(long, default_value_t = editor_link::HEADLESS_SIZE.0)]
        width: u32,
        #[arg(long, default_value_t = editor_link::HEADLESS_SIZE.1)]
        height: u32,
        /// Frames a second at most; 0 draws as fast as it can, to time the editor's frame.
        #[arg(long, default_value_t = 30.0)]
        rate: f32,
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
        /// Set up a C# IDE: whisk, rider, visual-studio or vscode.
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
    /// Build a project's C# scripts.
    Scripts {
        #[command(subcommand)]
        command: ScriptsCommand,
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
    /// The editor's calls as tools for an AI assistant, over the Model Context Protocol on stdin and stdout.
    Mcp {
        /// Project folder or its voxear.toml.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// With no editor open, say so, rather than starting a headless one for the tools.
        #[arg(long)]
        no_headless: bool,
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
    /// Open a project in the editor; `vox editor call` and `vox editor serve` reach one from scripts and tools.
    #[command(args_conflicts_with_subcommands = true)]
    Editor {
        #[command(subcommand)]
        action: Option<EditorCommand>,
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
        /// An asset to open before a screenshot, as a double click on it in the Project does: a World Generator
        /// shows its graph, and `Hills.worldgen#Noise` picks its Noise node too; a box model shows in the Model
        /// window, and `Pig.boxmodel#Head` picks its Head bone's first box too.
        #[arg(long, value_name = "ASSET")]
        open: Option<String>,
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
        /// the light the GI's probes find (`gi`), or lightmapped surfaces' texels (`lightmaps`) instead of normal
        /// shading.
        #[arg(
            long,
            value_parser = ["none", "lod", "cascades", "lights", "normals", "meshlets", "ao", "gi", "lightmaps"],
            default_value = "none"
        )]
        debug_view: String,
        /// Draw whole LODs rather than culling each draw's meshlets, to compare.
        #[arg(long)]
        no_meshlet_culling: bool,
        /// Draw local lights' shadows whole every frame rather than keeping what stands still, to compare.
        #[arg(long)]
        no_shadow_cache: bool,
        /// Draw the sun's shadows as `virtual` shadow maps or `cascades`, whatever the lighting says, to compare.
        #[arg(long, value_parser = ["virtual", "cascades"])]
        sun_shadows: Option<String>,
        /// Draw lamps' shadows as `virtual` shadow maps or into the `atlas`, whatever the lighting says, to compare.
        #[arg(long, value_parser = ["virtual", "atlas"])]
        lamp_shadows: Option<String>,
        /// Cull meshlets with a compute pass and indirect draws even where the GPU has mesh shaders, to compare.
        #[arg(long)]
        no_mesh_shaders: bool,
        /// Draw the scene at this share of the image's size, from 0.25 to 1, for TAA to upscale.
        #[arg(long)]
        render_scale: Option<f32>,
        /// Upscale with `voxear`'s own or `fsr` (AMD FSR 3), in place of the scene's choice, to compare.
        #[arg(long, value_parser = ["voxear", "fsr", "dlss", "xess"])]
        upscaler: Option<String>,
        /// Draw the scene as an upscaler's mode does: `native`, `quality`, `balanced`, `performance` or
        /// `ultra_performance`, in place of the scene's choice.
        #[arg(long, value_parser = ["native", "quality", "balanced", "performance", "ultra_performance"])]
        quality: Option<String>,
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
        /// Draw the game's own UI over the picture, as a player sees it: Screen UIs, and the menus and screens
        /// scripts open, such as a title menu or a character's creation.
        #[arg(long)]
        ui: bool,
        /// How large the game's UI is drawn with --ui, as a display's scale is: 2 for a screen of twice the
        /// pixels.
        #[arg(long, value_name = "SCALE", default_value_t = 1.0)]
        ui_scale: f32,
        /// Also save every this many frames as a sequence, numbered beside --out (frame_0060.png, ...), for
        /// watching what changes over time, such as the eye adapting.
        #[arg(long, value_name = "FRAMES")]
        every: Option<u64>,
        /// Frames a second to simulate, as on a display this fast: above the tick rate, what ticks move is shown
        /// between ticks.
        #[arg(long, value_name = "HZ", default_value_t = 60.0)]
        rate: f32,
        /// Take each frame no quicker than `--rate` says, as on a display that fast, so what is made in the
        /// background, as worlds round a flying camera, keeps the pace it would in play.
        #[arg(long)]
        real_time: bool,
        /// Look through the camera on the Obj with this name, rather than the first camera found.
        #[arg(long, value_name = "OBJ")]
        camera: Option<String>,
        /// Trace the GI's rays in a compute shader, as GPUs without ray tracing do, even on one that has it.
        #[arg(long)]
        software_rays: bool,
        /// Load this level rather than the project's start level, by its path in the project.
        #[arg(long, value_name = "LEVEL")]
        level: Option<String>,
        /// Also time the CPU: loading, each frame's tick and render, and the profiler's samples in them.
        #[arg(long)]
        profile: bool,
        /// Save the profile as JSON too: every frame's samples and GPU passes, for a tool to show (with or without
        /// --profile).
        #[arg(long, value_name = "JSON")]
        profile_out: Option<PathBuf>,
        /// Move the camera this many metres a second along x, y and z as the frames go, as `--fly 0,0,20`.
        #[arg(long, value_name = "X,Y,Z", value_parser = parse_vector)]
        fly: Option<voxear_core::math::Vec3>,
        /// Make worlds of blocks spend as this preset does, in place of the project's `[world]` quality, to
        /// compare what each costs.
        #[arg(long, value_parser = ["low", "medium", "high", "ultra"])]
        world_quality: Option<String>,
        /// Seconds to wait at most before the frames for the ground round the camera to be made, as a game's
        /// loading waits for it: terrains and worlds of blocks without end, and the plants grown on them. 0 starts
        /// the frames at once, the ground coming in as they go.
        #[arg(long, value_name = "SECONDS", default_value_t = 120.0)]
        wait: f32,
        /// Draw the frames under a shader pack: `builtin:evening`, one of the project's by its `pack.toml`'s path or
        /// GUID, or `file:` and the path of a pack's folder.
        #[arg(long, value_name = "PACK")]
        shader_pack: Option<String>,
        /// Set one of the shader pack's options, as `rays=false`, `strength=1.5` or `quality=High`; again for each.
        #[arg(long = "shader-pack-option", value_name = "NAME=VALUE", requires = "shader_pack")]
        shader_pack_options: Vec<String>,
    },
}

/// Three numbers apart by commas, as `0,0,20`.
fn parse_vector(text: &str) -> Result<voxear_core::math::Vec3, String> {
    let numbers: Vec<f32> =
        text.split(',').map(|v| v.trim().parse::<f32>().map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    match numbers[..] {
        [x, y, z] => Ok(voxear_core::math::Vec3::new(x, y, z)),
        _ => Err(format!("three numbers apart by commas, not {text}")),
    }
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
        // Tools read these commands' stdout, so their logs go to stderr.
        Commands::Editor { action: Some(_), .. } | Commands::Mcp { .. } => {
            Some(voxear_core::logging::init_capturing_stderr())
        }
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
        Commands::Editor { action: Some(EditorCommand::Call { call, args, project, headless, describe, json }), .. } => {
            let request = editor_link::CallRequest { call, args, project, headless, describe, json };
            let settings = |project: &Project| renderer_settings(project, false);
            let code = editor_link::call(request, settings, log.expect("captured above"))?;
            std::process::exit(code);
        }
        Commands::Editor { action: Some(EditorCommand::Serve { project, width, height, rate }), .. } => {
            let settings = |project: &Project| RendererSettings { hot_reload: shaders_reload(), ..renderer_settings(project, false) };
            editor_link::serve(&project, settings, log.expect("captured above"), (width, height), rate)
        }
        Commands::Mcp { project, no_headless } => mcp::serve(&project, !no_headless),
        Commands::Editor { action: None, project, screenshot, width, height, frames, open } => {
            let log = log.expect("captured above");
            let project = Project::open(&project)?;
            let settings = RendererSettings { hot_reload: shaders_reload(), ..renderer_settings(&project, false) };
            match screenshot {
                Some(out) => {
                    voxear_editor::screenshot(project, &settings, log, &out, (width, height), frames, open.as_deref())?;
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
        Commands::Scripts { command: ScriptsCommand::Build { project } } => build_scripts(&project),
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
            ui,
            ui_scale,
            no_meshlet_culling,
            no_shadow_cache,
            sun_shadows,
            lamp_shadows,
            no_mesh_shaders,
            render_scale,
            upscaler,
            quality,
            every,
            rate,
            real_time,
            software_rays,
            camera,
            level,
            profile,
            profile_out,
            fly,
            world_quality,
            wait,
            shader_pack,
            shader_pack_options,
        } => {
            let debug_view = match debug_view.as_str() {
                "lod" => DEBUG_VIEW_LOD,
                "cascades" => DEBUG_VIEW_CASCADES,
                "lights" => DEBUG_VIEW_LIGHT_COMPLEXITY,
                "normals" => DEBUG_VIEW_NORMALS,
                "meshlets" => DEBUG_VIEW_MESHLETS,
                "ao" => DEBUG_VIEW_AMBIENT_OCCLUSION,
                "gi" => DEBUG_VIEW_GLOBAL_ILLUMINATION,
                "lightmaps" => DEBUG_VIEW_LIGHTMAP_DENSITY,
                _ => DEBUG_VIEW_NONE,
            };
            let post_override = post.as_deref().map(PostProcess::preset).transpose()?;
            let meshlet_culling = !no_meshlet_culling;
            let local_shadow_cache = !no_shadow_cache;
            use voxear_render::{UpscaleQuality, Upscaler};
            let upscaler = upscaler.as_deref().map(|name| match name {
                "fsr" => Upscaler::Fsr,
                "dlss" => Upscaler::Dlss,
                "xess" => Upscaler::Xess,
                _ => Upscaler::Voxear,
            });
            let quality = quality.as_deref().map(|name| match name {
                "native" => UpscaleQuality::Native,
                "quality" => UpscaleQuality::Quality,
                "balanced" => UpscaleQuality::Balanced,
                "ultra_performance" => UpscaleQuality::UltraPerformance,
                _ => UpscaleQuality::Performance,
            });
            let options = ViewOptions {
                debug_view,
                post_override,
                meshlet_culling,
                render_scale: render_scale.or(quality.map(UpscaleQuality::scale)),
                upscaler,
                local_shadow_cache,
                sun_shadows: sun_shadows.as_deref().and_then(voxear_render::SunShadows::named),
                lamp_shadows: lamp_shadows.as_deref().and_then(voxear_render::LampShadows::named),
                ..ViewOptions::default()
            };
            let prepare = Prepare { build: !no_build, bundles, overlays };
            let mesh_shaders = !no_mesh_shaders;
            let delta = 1.0 / rate.clamp(1.0, 1000.0);
            let size = (width, height);
            let shot = Shot {
                out,
                size,
                frames,
                every,
                delta,
                options,
                stats,
                ui: ui.then_some(ui_scale.clamp(0.25, 8.0)),
                mesh_shaders,
                software_rays,
                camera,
                level,
                profile: profile || profile_out.is_some(),
                profile_out,
                fly,
                real_time,
                quality: world_quality.and_then(|quality| {
                    use voxear_terrain::WorldQuality;
                    WorldQuality::ALL.into_iter().find(|preset| preset.label().eq_ignore_ascii_case(&quality))
                }),
                wait: std::time::Duration::from_secs_f32(wait.max(0.0)),
                shader_pack: shader_pack.map(|pack| (pack, shader_pack_options)),
            };
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
    // What this machine keeps for the game, as a built game has it.
    game.use_machine_budget(None);
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
    /// Draw the game's own UI over the image, at this scale.
    ui: Option<f32>,
    /// Cull meshlets with task and mesh shaders, where the GPU has them.
    mesh_shaders: bool,
    /// The Obj whose camera to look through, by name.
    camera: Option<String>,
    /// Trace the GI's rays in a compute shader even on a GPU with ray tracing.
    software_rays: bool,
    /// The level to load in place of the start level.
    level: Option<String>,
    /// Time the CPU too.
    profile: bool,
    /// Where to save the profile as JSON.
    profile_out: Option<PathBuf>,
    /// How fast the camera moves, in metres a second.
    fly: Option<voxear_core::math::Vec3>,
    /// Each frame taken no quicker than `delta`.
    real_time: bool,
    /// The preset worlds of blocks spend as, in place of the project's.
    quality: Option<voxear_terrain::WorldQuality>,
    /// How long to wait at most for the ground round the camera before the frames.
    wait: std::time::Duration,
    /// The shader pack to draw the frames under, and its options set, each `name=value`.
    shader_pack: Option<(String, Vec<String>)>,
}

/// How many of a render's last frames its timings are averaged over.
const TIMED_FRAMES: u64 = 60;

/// The CPU's time over a render with `--profile`: loading, each frame's
/// tick and render, and the profiler's samples by where they ran, over every
/// frame and over the timed ones.
#[derive(Default)]
struct CpuTimes {
    load_ms: f32,
    frames: u32,
    /// Ticks and renders over every frame, the slowest of each, and over the timed frames.
    all: [f32; 2],
    slowest: [f32; 2],
    timed: [f32; 2],
    timed_frames: u32,
    /// The first frame by which every world of blocks without end had made its chunks round the camera, once
    /// any was making them: 0 while one still is.
    streamed_by: Option<u32>,
    /// Each sample by its path: milliseconds over every frame, the slowest frame's, over the timed ones, and over
    /// the frames slower than [`SLOW_FRAME_MS`].
    samples: Vec<(String, f32, f32, f32, f32)>,
    /// Each frame's tick and render together, and whether the last was slower than [`SLOW_FRAME_MS`].
    frame_ms: Vec<f32>,
    slow: bool,
    /// Each frame's time on the GPU as last known, and the slowest's passes.
    gpu_ms: Vec<f32>,
    slowest_gpu: Vec<(String, f32)>,
    /// Each frame's time this thread spent on a core, where the system says: what the frame's own work took,
    /// without the time other programs had the core or the frame waited for the GPU.
    on_core_ms: Vec<f32>,
}

/// How long this thread has spent on a core so far, in nanoseconds, where the system says.
fn on_core_ns() -> Option<u64> {
    let read = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    read.split_whitespace().next()?.parse().ok()
}

/// A frame slower than this, in milliseconds, shows on a 60 Hz display as one missed: what is in such frames is
/// listed apart.
const SLOW_FRAME_MS: f32 = 1000.0 / 60.0;

impl CpuTimes {
    fn add_frame(&mut self, tick_ms: f32, render_ms: f32, timed: bool, streamed: bool) {
        self.frames += 1;
        for (i, ms) in [tick_ms, render_ms].into_iter().enumerate() {
            self.all[i] += ms;
            self.slowest[i] = self.slowest[i].max(ms);
            if timed {
                self.timed[i] += ms;
            }
        }
        self.timed_frames += u32::from(timed);
        self.frame_ms.push(tick_ms + render_ms);
        self.slow = tick_ms + render_ms > SLOW_FRAME_MS;
        match (streamed, self.streamed_by) {
            (false, _) => self.streamed_by = Some(0),
            (true, Some(0)) => self.streamed_by = Some(self.frames),
            _ => {}
        }
    }

    fn add_gpu(&mut self, frame_ms: f32, passes: &[(String, f32)]) {
        if frame_ms > self.gpu_ms.iter().copied().fold(0.0, f32::max) {
            self.slowest_gpu = passes.to_vec();
        }
        self.gpu_ms.push(frame_ms);
    }

    fn add_samples(&mut self, samples: &voxear_core::profiling::Samples, timed: bool) {
        fn walk(samples: &voxear_core::profiling::Samples, node: usize, path: &str, out: &mut Vec<(String, f32)>) {
            for &child in &samples.nodes[node].children {
                let name =
                    if path.is_empty() { samples.name(child) } else { format!("{path} / {}", samples.name(child)) };
                out.push((name.clone(), samples.total_ms(child)));
                walk(samples, child, &name, out);
            }
        }
        let mut found = Vec::new();
        walk(samples, voxear_core::profiling::Samples::FRAME, "", &mut found);
        for (path, ms) in found {
            let at = match self.samples.iter().position(|(kept, ..)| *kept == path) {
                Some(at) => at,
                None => {
                    self.samples.push((path, 0.0, 0.0, 0.0, 0.0));
                    self.samples.len() - 1
                }
            };
            let kept = &mut self.samples[at];
            kept.1 += ms;
            kept.2 = kept.2.max(ms);
            if timed {
                kept.3 += ms;
            }
            if self.slow {
                kept.4 += ms;
            }
        }
    }

    fn print(&self) {
        let frames = self.frames.max(1) as f32;
        let timed = self.timed_frames.max(1) as f32;
        println!(
            "CPU time: loading {:.1} ms; over {} frames, then the last {}:",
            self.load_ms, self.frames, self.timed_frames
        );
        for (i, name) in ["tick", "render"].iter().enumerate() {
            println!(
                "  {name:<12} {:>8.2} ms a frame (slowest {:.2}) · {:>8.2} ms",
                self.all[i] / frames,
                self.slowest[i],
                self.timed[i] / timed
            );
        }
        // How the frames spread: a frame's tick and render, the half, the 95 and 99 in a hundred quicker than it, and
        // how many are slower than a 60 Hz and a 144 Hz display's frame.
        let mut sorted = self.frame_ms.clone();
        sorted.sort_by(f32::total_cmp);
        let at = |share: f32| sorted.get(((sorted.len() as f32 * share) as usize).min(sorted.len().saturating_sub(1)));
        if let (Some(half), Some(most), Some(nearly_all)) = (at(0.5), at(0.95), at(0.99)) {
            let over = |ms: f32| sorted.iter().filter(|&&frame| frame > ms).count();
            println!(
                "  frames         {half:>8.2} ms for half, {most:.2} for 95 in 100, {nearly_all:.2} for 99; {} slower \
                 than 60 fps, {} than 144",
                over(SLOW_FRAME_MS),
                over(1000.0 / 144.0)
            );
        }
        // The frames' work on the core alone, as other programs and the GPU kept it waiting.
        let mut core = self.on_core_ms.clone();
        core.sort_by(f32::total_cmp);
        let at =
            |share: f32| core.get(((core.len() as f32 * share) as usize).min(core.len().saturating_sub(1))).copied();
        if let (Some(half), Some(most), Some(nearly_all)) = (at(0.5), at(0.95), at(0.99)) {
            let over = |ms: f32| core.iter().filter(|&&frame| frame > ms).count();
            println!(
                "  on the core    {half:>8.2} ms for half, {most:.2} for 95 in 100, {nearly_all:.2} for 99; {} over \
                 60 fps' frame, {} over 144's",
                over(SLOW_FRAME_MS),
                over(1000.0 / 144.0)
            );
        }
        // And the GPU's frames the same way, with what the slowest of them spent its time on.
        let mut gpu = self.gpu_ms.clone();
        gpu.sort_by(f32::total_cmp);
        let at = |share: f32| gpu.get(((gpu.len() as f32 * share) as usize).min(gpu.len().saturating_sub(1))).copied();
        if let (Some(half), Some(most), Some(nearly_all)) = (at(0.5), at(0.95), at(0.99)) {
            let over = |ms: f32| gpu.iter().filter(|&&frame| frame > ms).count();
            println!(
                "  GPU frames     {half:>8.2} ms for half, {most:.2} for 95 in 100, {nearly_all:.2} for 99; {} slower \
                 than 60 fps, {} than 144",
                over(SLOW_FRAME_MS),
                over(1000.0 / 144.0)
            );
            let mut passes = self.slowest_gpu.clone();
            passes.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<String> = passes.iter().take(8).map(|(pass, ms)| format!("{pass} {ms:.2}")).collect();
            println!("  the slowest GPU frame's passes: {}", top.join(" · "));
        }
        match self.streamed_by {
            Some(0) => println!("  worlds of blocks without end still making chunks round the camera"),
            Some(by) => println!("  worlds of blocks without end made round the camera by frame {by}"),
            None => {}
        }
        let mut samples = self.samples.clone();
        samples.retain(|(_, all, ..)| *all >= 0.5);
        samples.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("  samples, in all · slowest frame · a timed frame:");
        for (path, all, slowest, last, _) in samples.iter().take(40) {
            println!("    {path:<60} {all:>9.1} ms · {slowest:>8.2} · {:>7.3}", last / timed);
        }
        // What the frames slower than 60 fps spent their time on, a frame of them on average.
        let slow = self.frame_ms.iter().filter(|&&frame| frame > SLOW_FRAME_MS).count();
        if slow > 0 {
            samples.sort_by(|a, b| b.4.total_cmp(&a.4));
            println!("  in the {slow} frames slower than 60 fps, a frame of them:");
            for (path, .., in_slow) in samples.iter().take(20).filter(|sample| sample.4 > 0.0) {
                println!("    {path:<60} {:>8.2} ms", in_slow / slow as f32);
            }
        }
    }
}

/// Each pass's GPU time added up over frames, by its place in the frame,
/// as passes such as Bloom down come several times.
#[derive(Default)]
struct TimingAverage {
    passes: Vec<(String, f32)>,
    frames: u32,
    /// Each pass's least time over those frames: other programs using the GPU are counted in a pass's time, so on a
    /// busy GPU the least is the pass's own.
    least: Vec<f32>,
}

impl TimingAverage {
    fn add(&mut self, timings: &[(String, f32)]) {
        let same = self.passes.len() == timings.len()
            && self.passes.iter().zip(timings).all(|((name, _), (pass, _))| name == pass);
        if !same {
            // The frame's passes changed, as when an effect turned on: count from this frame.
            self.passes = timings.iter().map(|(name, _)| (name.clone(), 0.0)).collect();
            self.least = vec![f32::MAX; timings.len()];
            self.frames = 0;
        }
        for (((_, total), least), (_, ms)) in self.passes.iter_mut().zip(&mut self.least).zip(timings) {
            *total += ms;
            *least = least.min(*ms);
        }
        self.frames += 1;
    }

    fn average(&self) -> Vec<(String, f32)> {
        let frames = self.frames.max(1) as f32;
        self.passes.iter().map(|(name, total)| (name.clone(), total / frames)).collect()
    }
}

fn render(path: &Path, shot: Shot, prepare: Prepare, renderdoc: bool) -> Result<()> {
    let Shot {
        out,
        size: (width, height),
        frames,
        every,
        delta,
        options,
        stats,
        ui,
        mesh_shaders,
        software_rays,
        camera,
        level,
        profile,
        profile_out,
        fly,
        real_time,
        quality,
        wait,
        shader_pack,
    } = shot;
    let mut project = Project::open(path)?;
    let project_name = project.name().to_string();
    if level.is_some() {
        project.file.project.start_level = level;
    }
    if let Some(budget) = quality.and_then(voxear_terrain::WorldQuality::budget) {
        project.file.world = budget;
    }
    let settings = RendererSettings { software_rays, ..renderer_settings(&project, renderdoc) };
    let mut game = start_game(project, prepare)?;
    let mut renderer = Renderer::new_offscreen(width, height, &settings)?;
    renderer.set_view_options(options);
    renderer.set_mesh_shaders(mesh_shaders);
    let mut cpu = profile.then(CpuTimes::default);
    if profile {
        voxear_core::profiling::set_enabled(true);
    }
    // Every frame as tools read them, for --profile-out; loading is frame 0.
    let mut saved: Vec<serde_json::Value> = Vec::new();
    let loading = std::time::Instant::now();
    game.start(Some(&mut renderer))?;
    // The ground round the camera made first, as a game's loading waits for it: the frames alone can be over before
    // the terrain's threads have made any of it.
    if !wait.is_zero() {
        let making = std::time::Instant::now();
        let seconds = || making.elapsed().as_secs_f32();
        if !game.make_world(wait) {
            tracing::warn!("the ground round the camera was not all made in {:.0} s; the frames go on", seconds());
        } else if seconds() >= 0.1 {
            tracing::info!("made the ground round the camera in {:.1} s", seconds());
        }
        // Voxel Volumes too: theirs are made on threads of their own.
        let making = std::time::Instant::now();
        if !renderer.make_voxel_volumes(&game.engine.world, wait)? {
            tracing::warn!("the Voxel Volumes were not all made in {:.0} s; the frames go on", wait.as_secs_f32());
        } else if making.elapsed().as_secs_f32() >= 0.1 {
            tracing::info!("made the Voxel Volumes in {:.1} s", making.elapsed().as_secs_f32());
        }
    }
    if let Some(cpu) = &mut cpu {
        cpu.load_ms = loading.elapsed().as_secs_f32() * 1000.0;
        let samples = voxear_core::profiling::end_frame();
        cpu.add_samples(&samples, false);
        if profile_out.is_some() {
            let mut frame = voxear_engine::profile_frames::frame_json(0, cpu.load_ms, &samples, &[]);
            frame["loading"] = true.into();
            saved.push(frame);
        }
    }
    if let Some((pack, settings)) = &shader_pack {
        let world = &mut game.engine.world;
        voxear_engine::shader_packs::wear(world, &mut game.content, pack)?;
        for setting in settings {
            set_pack_option(world, setting)?;
        }
        println!("drawn under the shader pack {pack}");
    }
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
    // The camera that flies: the one looked through, or the first.
    let flying = fly.and_then(|velocity| {
        let world = &game.engine.world;
        let first =
            || world.query_ref::<(voxear_ecs::Entity, &voxear_render::Camera)>().into_iter().next().map(|(e, _)| e);
        looking_through.or_else(first).map(|camera| (camera, velocity))
    });
    for frame in 1..=frames {
        let started = std::time::Instant::now();
        let on_core = on_core_ns();
        if let Some((camera, velocity)) = flying
            && let Some(transform) = game.engine.world.get_mut::<Transform>(camera)
        {
            transform.position += velocity * delta;
        }
        let ticking = std::time::Instant::now();
        game.tick(delta, Some(&mut renderer));
        let tick_ms = ticking.elapsed().as_secs_f32() * 1000.0;
        // UI Panels in the world, with nothing pointing at them.
        let view = [0.0, 0.0, width as f32, height as f32];
        game.show_panels(&mut renderer, (view, looking_through), None, delta)?;
        // The game's own UI over the whole picture, under the stats, as a window draws it.
        if let Some(scale) = ui {
            game.ui.draw(&mut renderer, delta, [0, 0, width, height], scale)?;
        }
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
        let rendering = std::time::Instant::now();
        renderer.render(&game.engine.world, game.engine.time().elapsed as f32)?;
        let render_ms = rendering.elapsed().as_secs_f32() * 1000.0;
        let timing = frame + TIMED_FRAMES > frames;
        if timing {
            timed.add(renderer.timings());
            frame_gpu.push(renderer.gpu_frame_ms());
        }
        if let Some(cpu) = &mut cpu {
            if let (Some(from), Some(to)) = (on_core, on_core_ns()) {
                cpu.on_core_ms.push(to.saturating_sub(from) as f32 / 1.0e6);
            }
            cpu.add_gpu(renderer.gpu_frame_ms(), renderer.timings());
            cpu.add_frame(tick_ms, render_ms, timing, voxear_engine::blocks::streamed(&game.engine.world));
            let samples = voxear_core::profiling::end_frame();
            cpu.add_samples(&samples, timing);
            if profile_out.is_some() {
                let frame_ms = tick_ms + render_ms;
                let timings = renderer.timings();
                let mut frame = voxear_engine::profile_frames::frame_json(frame, frame_ms, &samples, timings);
                frame["timed"] = timing.into();
                saved.push(frame);
            }
        }
        if capture && let Some(path) = renderer.end_capture() {
            println!("RenderDoc capture: {}", path.display());
        }
        if every.is_some_and(|every| every > 0 && frame % every == 0) {
            let (width, height, pixels) = renderer.read_pixels()?;
            write_png(&sequence_path(&out, frame), width, height, &pixels)?;
        }
        if real_time {
            std::thread::sleep(std::time::Duration::from_secs_f32(delta).saturating_sub(started.elapsed()));
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
    // Other programs using the GPU are counted in a frame's time: the least of the frames is the frame's own.
    let least = frame_gpu.iter().copied().fold(f32::MAX, f32::min);
    println!("  {:<16} {least:>7.3} ms", "frame, the least");
    println!("GPU time, the least of those frames:");
    for ((pass, _), ms) in timed.passes.iter().zip(&timed.least) {
        println!("  {pass:<16} {ms:>7.3} ms");
    }

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
    let particles = stats.particles;
    if particles.systems + particles.left_out > 0 {
        println!(
            "particles: {} alive in {} systems, {} stepped · room for {} of {} · {} systems left out",
            particles.alive,
            particles.systems,
            particles.awake,
            particles.reserved,
            particles.budget,
            particles.left_out
        );
    }
    let blocks = stats.blocks;
    if blocks.draws + blocks.shadow_draws > 0 {
        println!(
            "blocks: {} triangles in {} draws · {} of water · {} triangles in {} shadow draws",
            blocks.triangles, blocks.draws, blocks.water_triangles, blocks.shadow_triangles, blocks.shadow_draws
        );
    }
    // What the GPU holds, that of worlds of blocks among it, and the heap, to compare what a quality costs.
    let memory = renderer.gpu_memory();
    let held =
        |of: &dyn Fn(&str) -> bool| -> u64 { memory.allocations.iter().filter(|(n, _)| of(n)).map(|(_, b)| b).sum() };
    let megabytes = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    println!(
        "memory: {:.0} MB on the GPU, {:.0} MB of it blocks' · {:.0} MB of heap",
        megabytes(held(&|_| true)),
        megabytes(held(&|name| name.starts_with("block"))),
        megabytes(voxear_core::memory::heap().0)
    );
    println!("mesh space: {}", renderer.mesh_pool().describe());
    if let Some(cpu) = &cpu {
        cpu.print();
    }
    if let Some(file) = &profile_out {
        let profile = serde_json::json!({
            "voxear_profile": voxear_engine::profile_frames::FORMAT,
            "source": "render",
            "project": project_name,
            "gpu_name": renderer.gpu().name,
            "size": [width, height],
            "timed_frames": timed.frames,
            "frames": saved,
        });
        make_folder_for(file)?;
        std::fs::write(file, serde_json::to_vec(&profile)?).with_context(|| format!("writing {}", file.display()))?;
        println!("profile: {}", file.display());
    }
    let lods: Vec<String> =
        stats.lods.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| format!("LOD{i} {n}")).collect();
    println!("LODs: {}", if lods.is_empty() { "none".into() } else { lods.join(" · ") });
    if stats.average_luminance > 0.0 {
        println!("exposure: {:+.2} EV for an average luminance of {:.3}", stats.exposure_ev, stats.average_luminance);
    }
    Ok(())
}

/// Sets one of the worn shader pack's options from `name=value`: a number,
/// `true` or `false`, or a choice's name.
fn set_pack_option(world: &mut voxear_ecs::World, setting: &str) -> Result<()> {
    use voxear_engine::shader_packs;
    let (key, value) = setting.split_once('=').with_context(|| format!("`{setting}` is not name=value"))?;
    let (key, value) = (key.trim(), value.trim());
    let options = shader_packs::options(world);
    let option = options.iter().find(|(option, _)| option.key == key).map(|(option, _)| option);
    let number = match value {
        "true" | "on" => Some(1.0),
        "false" | "off" => Some(0.0),
        _ => value.parse::<f32>().ok(),
    };
    let chosen = option.and_then(|option| shader_packs::choice_value(option, value));
    let color = option.and_then(|option| shader_packs::color_text_value(option, value));
    let Some(number) = chosen.or(color).or(number) else {
        anyhow::bail!("`{value}` is not a number, true or false, a colour (#rrggbb), or one of `{key}`'s choices");
    };
    let set = shader_packs::set_option(world, key, number)?;
    println!("  {key} = {set}");
    Ok(())
}

/// RenderDoc captures land in the project's `.voxear/Captures`.
fn renderer_settings(project: &Project, renderdoc: bool) -> RendererSettings {
    RendererSettings {
        renderdoc,
        capture_folder: Some(project.root.join(".voxear").join("Captures")),
        ..RendererSettings::default()
    }
}

/// `vox scripts build`: the scripts alone, as `vox run` builds them.
fn build_scripts(path: &Path) -> Result<()> {
    let project = Project::open(path)?;
    if project.scripts_project().is_none() {
        println!("{} has no scripts to build", project.name());
        return Ok(());
    }
    scripts::build(&project)?;
    println!("built {}'s scripts", project.name());
    Ok(())
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

/// Writes an image, making the folder it goes in when there is none yet.
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    make_folder_for(path)?;
    let file = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// Makes the folder `file` goes in, when it names one that is not there yet.
fn make_folder_for(file: &Path) -> Result<()> {
    match file.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        Some(folder) => std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display())),
        None => Ok(()),
    }
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
            AssetKind::Font => "font",
            AssetKind::Paths => "paths",
            AssetKind::ReflectionProbes => "probes",
            AssetKind::Lightmaps => "lightmaps",
            AssetKind::AssetTable => "assets",
            AssetKind::LightProfile => "ies",
            AssetKind::Lighting => "lighting",
            AssetKind::Dimension => "dimension",
            AssetKind::LookPack => "look pack",
            AssetKind::TexturePack => "texture pack",
            AssetKind::ShaderPack => "shader pack",
            AssetKind::Shader => "shader",
            AssetKind::VoxShade => "voxshade",
            AssetKind::MotionSet => "motion set",
            AssetKind::GunHandling => "gun handling",
            AssetKind::Particles => "particles",
            AssetKind::Data => "data",
            AssetKind::Terrain => "terrain",
            AssetKind::PromptIcons => "prompts",
            AssetKind::WorldGen => "worldgen",
            AssetKind::BlockWorld => "blockworld",
            AssetKind::Block => "block",
            AssetKind::Biome => "biome",
            AssetKind::Structure => "structure",
            AssetKind::Plant => "voxplant",
            AssetKind::VoxModel => "vox",
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
