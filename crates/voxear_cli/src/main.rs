//! `vox`: the Voxear command line.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use voxear_core::math::EulerRot;
use voxear_engine::{Engine, Project, core_assembly, run_windowed, spawn_objs};
use voxear_render::{DEBUG_VIEW_CASCADES, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE, Renderer, RendererSettings, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_scripting::{DotnetInstall, ScriptRuntime, find_dotnet};

const CONFIGURATION: &str = "Debug";

#[derive(Parser)]
#[command(name = "vox", version, about = "The Voxear engine command line")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check that this machine can build and run Voxear projects.
    Doctor,
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
        /// Colour by `lod` or by shadow `cascades` instead of shading normally.
        #[arg(long, value_parser = ["none", "lod", "cascades"], default_value = "none")]
        debug_view: String,
    },
}

fn main() -> Result<()> {
    voxear_core::logging::init();
    match Cli::parse().command {
        Commands::Doctor => doctor(),
        Commands::Run { project, headless, frames, no_build, renderdoc } => {
            run(&project, headless, frames, no_build, renderdoc)
        }
        Commands::Render { project, out, width, height, frames, no_build, renderdoc, debug_view } => {
            let debug_view = match debug_view.as_str() {
                "lod" => DEBUG_VIEW_LOD,
                "cascades" => DEBUG_VIEW_CASCADES,
                _ => DEBUG_VIEW_NONE,
            };
            let options = ViewOptions { debug_view, ..ViewOptions::default() };
            render(&project, &out, (width, height), frames, no_build, renderdoc, options)
        }
    }
}

fn run(path: &Path, headless: bool, frames: Option<u64>, no_build: bool, renderdoc: bool) -> Result<()> {
    let project = Project::open(path)?;
    let settings = renderer_settings(&project, renderdoc);
    let engine = start_engine(&project, no_build)?;

    if !headless {
        let title = format!("{} — Voxear", project.name());
        let setup =
            Box::new(move |engine: &mut Engine, renderer: &mut Renderer| spawn_objs(engine, Some(renderer), &project));
        return run_windowed(engine, &title, frames, settings, setup);
    }
    let mut engine = engine;
    spawn_objs(&mut engine, None, &project)?;
    for _ in 0..frames.unwrap_or(3) {
        engine.tick(1.0 / 60.0);
    }
    print_objs(&engine);
    Ok(())
}

fn render(
    path: &Path,
    out: &Path,
    (width, height): (u32, u32),
    frames: u64,
    no_build: bool,
    renderdoc: bool,
    options: ViewOptions,
) -> Result<()> {
    let project = Project::open(path)?;
    let mut engine = start_engine(&project, no_build)?;
    let mut renderer = Renderer::new_offscreen(width, height, &renderer_settings(&project, renderdoc))?;
    renderer.set_view_options(options);
    spawn_objs(&mut engine, Some(&mut renderer), &project)?;
    let frames = frames.max(1);
    for frame in 1..=frames {
        engine.tick(1.0 / 60.0);
        let capture = frame == frames && renderer.has_renderdoc();
        if capture {
            renderer.start_capture();
        }
        renderer.render(&engine.world, engine.time().elapsed as f32)?;
        if capture && let Some(path) = renderer.end_capture() {
            println!("RenderDoc capture: {}", path.display());
        }
    }

    let (width, height, pixels) = renderer.read_pixels()?;
    write_png(out, width, height, &pixels)?;
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
    let lods: Vec<String> =
        stats.lods.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| format!("LOD{i} {n}")).collect();
    println!("LODs: {}", if lods.is_empty() { "none".into() } else { lods.join(" · ") });
    Ok(())
}

/// RenderDoc captures land in the project's `.voxear/Captures`.
fn renderer_settings(project: &Project, renderdoc: bool) -> RendererSettings {
    RendererSettings { renderdoc, capture_folder: Some(project.root.join(".voxear").join("Captures")) }
}

/// Creates the engine and, when the project has scripts, builds and loads them.
fn start_engine(project: &Project, no_build: bool) -> Result<Engine> {
    let mut engine = Engine::new();
    let Some(csproj) = project.scripts_project() else { return Ok(engine) };
    let dotnet = find_dotnet().context(".NET is not installed; run `vox doctor` for how to get it")?;
    if !no_build {
        build_scripts(&dotnet, &csproj)?;
    }
    let mut scripts = ScriptRuntime::start(&dotnet, &core_assembly(CONFIGURATION))?;
    let assembly = project.scripts_assembly(CONFIGURATION).context("scripts project has no file name")?;
    scripts.load_game_assembly(&assembly)?;
    engine.set_scripts(scripts);
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

fn build_scripts(dotnet: &DotnetInstall, csproj: &Path) -> Result<()> {
    let exe = dotnet.root.join(if cfg!(windows) { "dotnet.exe" } else { "dotnet" });
    tracing::info!("building {}", csproj.display());
    let status = Command::new(&exe)
        .args(["build", "-c", CONFIGURATION, "--nologo", "-v", "quiet", "-clp:ErrorsOnly;NoSummary"])
        .arg(csproj)
        .env("DOTNET_NOLOGO", "1")
        .status()
        .with_context(|| format!("running {}", exe.display()))?;
    ensure!(status.success(), "the C# scripts failed to build");
    Ok(())
}

fn print_objs(engine: &Engine) {
    println!("after {} frames ({:.3}s):", engine.time().frame, engine.time().elapsed);
    for (name, transform) in engine.world.query_ref::<(&Name, &Transform)>() {
        let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        println!("  {:<14} position {:>5.1?}  yaw {:>6.1}°", name.0, transform.position.to_array(), yaw.to_degrees());
    }
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
