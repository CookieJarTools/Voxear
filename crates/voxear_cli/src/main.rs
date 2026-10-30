//! `vox`: the Voxear command line.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use voxear_core::math::{EulerRot, Vec3};
use voxear_engine::{Engine, Project, core_assembly, run_windowed};
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
    },
}

fn main() -> Result<()> {
    voxear_core::logging::init();
    match Cli::parse().command {
        Commands::Doctor => doctor(),
        Commands::Run { project, headless, frames, no_build } => run(&project, headless, frames, no_build),
    }
}

fn run(path: &Path, headless: bool, frames: Option<u64>, no_build: bool) -> Result<()> {
    let project = Project::open(path)?;
    let mut engine = Engine::new();

    if let Some(csproj) = project.scripts_project() {
        let dotnet = find_dotnet().context(".NET is not installed; run `vox doctor` for how to get it")?;
        if !no_build {
            build_scripts(&dotnet, &csproj)?;
        }
        let mut scripts = ScriptRuntime::start(&dotnet, &core_assembly(CONFIGURATION))?;
        let assembly = project.scripts_assembly(CONFIGURATION).context("scripts project has no file name")?;
        scripts.load_game_assembly(&assembly)?;
        engine.set_scripts(scripts);
    }

    for obj in &project.file.objs {
        engine.spawn_obj(&obj.name, Vec3::from(obj.position), &obj.behaviours)?;
    }

    if !headless {
        return run_windowed(engine, &format!("{} — Voxear", project.name()), frames);
    }
    let frames = frames.unwrap_or(3);
    for _ in 0..frames {
        engine.tick(1.0 / 60.0);
    }
    print_objs(&engine);
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
    match tool_version("glslc") {
        Some(version) => {
            ok(&format!("glslc ({version})"));
        }
        None => {
            ready &= fail("glslc not found");
            hint("Arch: sudo pacman -S shaderc    Windows: install the Vulkan SDK");
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
    let line = format!(
        "{} — Vulkan {major}.{minor}.{patch}, {}, {} MB{tiers}",
        gpu.name, gpu.driver, gpu.device_local_mb
    );
    match &gpu.unsupported {
        None => ok(&line),
        Some(reason) => fail(&format!("{line} ({reason})")),
    }
}

fn tool_version(tool: &str) -> Option<String> {
    let output = Command::new(tool).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
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
