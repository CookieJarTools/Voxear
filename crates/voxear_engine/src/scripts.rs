//! A project's C# scripts: building them with the .NET SDK and starting the
//! runtime that runs them, for `vox` and for the editor's play mode.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard},
};

use anyhow::{Context, Result, ensure};
use voxear_scripting::{DotnetInstall, ScriptRuntime, find_dotnet};

use crate::project::{Project, core_assembly, engine_root};

/// The build configuration scripts are built and loaded in.
pub const CONFIGURATION: &str = "Debug";

/// Held while the scripts build: the editor builds them both to describe
/// them and to play, and two builds of one project at once fight over its
/// files.
static BUILDING: Mutex<()> = Mutex::new(());

/// Builds the project's scripts, if it has any. Takes a few seconds, so the
/// editor runs it off the main thread.
pub fn build(project: &Project) -> Result<()> {
    let _building = lock();
    build_unlocked(project)
}

/// Holds the scripts' build to this thread, for work that builds and then
/// reads the build.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    BUILDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// [`build`], for a thread already holding [`lock`].
pub(crate) fn build_unlocked(project: &Project) -> Result<()> {
    let Some(csproj) = project.scripts_project() else { return Ok(()) };
    build_csproj(&dotnet()?, &csproj)
}

/// The `dotnet` command, from the install that runs the scripts.
pub fn dotnet_exe() -> Result<PathBuf> {
    Ok(dotnet()?.root.join(if cfg!(windows) { "dotnet.exe" } else { "dotnet" }))
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

/// `-p:VoxearHome=<engine>/`: which engine the scripts build against, through
/// `managed/Voxear.Scripts.props` there, so a project needs no checkout.
/// MSBuild reads `;` and `,` in a property as separators, so they are escaped.
fn engine_home_property() -> String {
    let root = engine_root();
    let root = root.canonicalize().unwrap_or(root);
    let escaped = root.display().to_string().replace('%', "%25").replace(';', "%3B").replace(',', "%2C");
    format!("-p:VoxearHome={escaped}{}", std::path::MAIN_SEPARATOR)
}

fn dotnet() -> Result<DotnetInstall> {
    find_dotnet().context(".NET is not installed; run `vox doctor` for how to get it")
}

fn build_csproj(dotnet: &DotnetInstall, csproj: &Path) -> Result<()> {
    let exe = dotnet.root.join(if cfg!(windows) { "dotnet.exe" } else { "dotnet" });
    tracing::info!("building {}", csproj.display());
    let output = Command::new(&exe)
        .args(["build", "-c", CONFIGURATION, "--nologo", "-v", "quiet", "-clp:ErrorsOnly;NoSummary"])
        .arg(engine_home_property())
        .arg(csproj)
        .env("DOTNET_NOLOGO", "1")
        .output()
        .with_context(|| format!("running {}", exe.display()))?;
    // The compiler's errors go to the log, which the editor shows in its Console.
    if !output.status.success() {
        let printed = [output.stdout, output.stderr].concat();
        for line in String::from_utf8_lossy(&printed).lines().map(str::trim).filter(|l| !l.is_empty()) {
            tracing::error!(target: "compiler", "{line}");
        }
    }
    ensure!(output.status.success(), "the C# scripts failed to build");
    Ok(())
}
