//! A project's C# scripts: building them with the .NET SDK and starting the
//! runtime that runs them, for `vox` and for the editor's play mode.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Mutex, MutexGuard,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, bail, ensure};
use voxear_scripting::{DotnetInstall, ScriptRuntime, find_dotnet};

use crate::project::{Project, core_assembly, engine_root};

/// The build configuration scripts are built and loaded in.
pub const CONFIGURATION: &str = "Debug";

/// How long a build of the scripts may take before it is taken as stuck and
/// stopped: long enough for a first build that fetches packages.
const BUILD_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How often a running game's scripts are looked at for changes.
const WATCH_EVERY: Duration = Duration::from_secs(2);

/// How long a stopped program's output is waited for: a process it started
/// can keep its pipes open, and what it printed is not worth hanging on.
const OUTPUT_WAIT: Duration = Duration::from_secs(5);

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

/// Builds the project's scripts in `configuration`, `Debug` or `Release`,
/// with extra symbols for C#'s `#if`, as a build profile asks.
pub fn build_for(project: &Project, configuration: &str, defines: &[String]) -> Result<()> {
    for define in defines {
        let mut chars = define.chars();
        let starts = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        ensure!(
            starts && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "`{define}` is not a symbol C# can define: use letters, digits and underscores"
        );
    }
    let _building = lock();
    let Some(csproj) = project.scripts_project() else { return Ok(()) };
    write_engine_props(project)?;
    build_csproj(&dotnet()?, &csproj, configuration, defines)
}

/// Holds the scripts' build to this thread, for work that builds and then
/// reads the build.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    BUILDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// [`build`], for a thread already holding [`lock`].
pub(crate) fn build_unlocked(project: &Project) -> Result<()> {
    let Some(csproj) = project.scripts_project() else { return Ok(()) };
    write_engine_props(project)?;
    build_csproj(&dotnet()?, &csproj, CONFIGURATION, &[])
}

fn write_engine_props(project: &Project) -> Result<()> {
    let props = project.root.join(".voxear").join("Engine.props");
    std::fs::create_dir_all(project.root.join(".voxear"))?;
    std::fs::write(&props, engine_props()).with_context(|| format!("writing {}", props.display()))
}

/// The `dotnet` command, from the install that runs the scripts.
pub fn dotnet_exe() -> Result<PathBuf> {
    Ok(dotnet()?.root.join(if cfg!(windows) { "dotnet.exe" } else { "dotnet" }))
}

/// Starts .NET with the project's built scripts loaded, or `None` for a
/// project without scripts. .NET starts only once per process, so the
/// runtime is kept and handed from run to run.
pub fn start(project: &Project) -> Result<Option<ScriptRuntime>> {
    start_for(project, CONFIGURATION)
}

/// [`start`], with the scripts and `Voxear.Core` built in `configuration`.
pub fn start_for(project: &Project, configuration: &str) -> Result<Option<ScriptRuntime>> {
    if project.scripts_project().is_none() {
        return Ok(None);
    }
    let mut scripts = ScriptRuntime::start(&dotnet()?, &core_assembly(configuration))?;
    let assembly = project.scripts_assembly(configuration).context("scripts project has no file name")?;
    scripts.load_game_assembly(&assembly)?;
    Ok(Some(scripts))
}

/// Watches a project's scripts while its game runs outside the editor, as
/// `vox run` does: once a script changes, the scripts build off the main
/// thread, and [`poll`](Self::poll) hands back how the build went, for the
/// game to reload them.
pub struct ScriptWatcher {
    root: PathBuf,
    /// Changes before this are in the scripts running.
    since: SystemTime,
    checked: Instant,
    building: Option<Receiver<Result<()>>>,
}

impl ScriptWatcher {
    pub fn new(project: &Project) -> Self {
        Self { root: project.root.clone(), since: SystemTime::now(), checked: Instant::now(), building: None }
    }

    /// A build that has finished since the last call, if one has; a build
    /// starts once a script changed, looked at every couple of seconds.
    pub fn poll(&mut self, project: &Project) -> Option<Result<()>> {
        if let Some(building) = &self.building {
            let finished = building.try_recv().ok()?;
            self.building = None;
            return Some(finished);
        }
        if self.checked.elapsed() < WATCH_EVERY {
            return None;
        }
        self.checked = Instant::now();
        if !crate::script_types::changed_since(project, self.since) {
            return None;
        }
        self.since = SystemTime::now();
        let (done, building) = mpsc::channel();
        let root = self.root.clone();
        std::thread::spawn(move || {
            let _ = done.send(Project::open(&root).and_then(|project| build(&project)));
        });
        self.building = Some(building);
        None
    }
}

/// Runs a program to its end and collects what it printed, stopping it once
/// it has run for `limit`. A build or tool that hangs would otherwise hold the
/// scripts' build lock, and Play with it, for good.
pub(crate) fn output_within(command: &mut Command, limit: Duration) -> Result<Output> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (stdout, stderr) = (read_all(child.stdout.take()), read_all(child.stderr.take()));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            bail!("it ran for over {} seconds, so it was stopped", limit.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let collected = |read: Receiver<Vec<u8>>| read.recv_timeout(OUTPUT_WAIT).unwrap_or_default();
    Ok(Output { status, stdout: collected(stdout), stderr: collected(stderr) })
}

/// Reads a pipe to its end on a thread of its own, so neither pipe can fill
/// and stall the program writing to it.
fn read_all(pipe: Option<impl Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (done, read) = mpsc::channel();
    if let Some(mut pipe) = pipe {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            let _ = done.send(bytes);
        });
    }
    read
}

/// `-p:VoxearHome=<engine>/`: which engine the scripts build against, through
/// `managed/Voxear.Scripts.props` there, so a project needs no checkout.
/// MSBuild reads `;` and `,` in a property as separators, so they are escaped.
fn engine_home_property() -> String {
    let escaped = engine_home().replace('%', "%25").replace(';', "%3B").replace(',', "%2C");
    format!("-p:VoxearHome={escaped}")
}

/// A project's `.voxear/Engine.props`: the engine its scripts last built
/// with, for an IDE, which builds them without the editor or `vox` naming
/// one. Every build of the scripts writes it.
pub fn engine_props() -> String {
    let home = engine_home().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<Project>\n\t<!-- Written by Voxear on each build of the scripts: the engine they build with, for an IDE. -->\n\
         \t<PropertyGroup>\n\t\t<VoxearHome>{home}</VoxearHome>\n\t</PropertyGroup>\n</Project>\n"
    )
}

/// The engine's folder as a scripts build names it, ending in a separator.
fn engine_home() -> String {
    let root = engine_root();
    let root = root.canonicalize().unwrap_or(root);
    format!("{}{}", root.display(), std::path::MAIN_SEPARATOR)
}

pub(crate) fn dotnet() -> Result<DotnetInstall> {
    find_dotnet().context(".NET is not installed; run `vox doctor` for how to get it")
}

fn build_csproj(dotnet: &DotnetInstall, csproj: &Path, configuration: &str, defines: &[String]) -> Result<()> {
    let exe = dotnet.root.join(if cfg!(windows) { "dotnet.exe" } else { "dotnet" });
    tracing::info!("building {}", csproj.display());
    let mut command = Command::new(&exe);
    command
        .args(["build", "-c", configuration, "--nologo", "-v", "quiet", "-clp:ErrorsOnly;NoSummary"])
        .arg(engine_home_property())
        .arg(csproj)
        .env("DOTNET_NOLOGO", "1");
    // Voxear.Scripts.props adds these to DefineConstants; `;` would end the property.
    if !defines.is_empty() {
        command.arg(format!("-p:VoxearDefines={}", defines.join("%3B")));
    }
    let output = output_within(&mut command, BUILD_LIMIT).with_context(|| format!("building {}", csproj.display()))?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_hangs_is_stopped_and_one_that_ends_gives_its_output() {
        let started = Instant::now();
        let stuck = output_within(Command::new("sleep").arg("30"), Duration::from_millis(200));
        assert!(stuck.is_err_and(|e| e.to_string().contains("stopped")));
        assert!(started.elapsed() < Duration::from_secs(5), "stopped at the limit, not when it ended");

        let said = output_within(Command::new("sh").args(["-c", "echo out; echo err >&2"]), BUILD_LIMIT).unwrap();
        assert!(said.status.success());
        assert_eq!((said.stdout.as_slice(), said.stderr.as_slice()), (&b"out\n"[..], &b"err\n"[..]));
    }
}
