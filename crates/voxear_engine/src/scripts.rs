//! A project's C# scripts: building them with the .NET SDK and starting the
//! runtime that runs them, for `vox` and for the editor's play mode.

use std::{path::Path, process::Command};

use anyhow::{Context, Result, ensure};
use voxear_scripting::{DotnetInstall, ScriptRuntime, find_dotnet};

use crate::project::{Project, core_assembly};

/// The build configuration scripts are built and loaded in.
pub const CONFIGURATION: &str = "Debug";

/// Builds the project's scripts, if it has any. Takes a few seconds, so the
/// editor runs it off the main thread.
pub fn build(project: &Project) -> Result<()> {
    let Some(csproj) = project.scripts_project() else { return Ok(()) };
    build_csproj(&dotnet()?, &csproj)
}

/// Starts .NET with the project's built scripts loaded, or `None` for a
/// project without scripts. .NET starts only once per process, so the
/// runtime is kept and handed from run to run.
pub fn start(project: &Project) -> Result<Option<ScriptRuntime>> {
    if project.scripts_project().is_none() {
        return Ok(None);
    }
    let mut scripts = ScriptRuntime::start(&dotnet()?, &core_assembly(CONFIGURATION))?;
    let assembly = project.scripts_assembly(CONFIGURATION).context("scripts project has no file name")?;
    scripts.load_game_assembly(&assembly)?;
    Ok(Some(scripts))
}

fn dotnet() -> Result<DotnetInstall> {
    find_dotnet().context(".NET is not installed; run `vox doctor` for how to get it")
}

fn build_csproj(dotnet: &DotnetInstall, csproj: &Path) -> Result<()> {
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
