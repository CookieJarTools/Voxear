//! Builds a game into a player with a build profile, as `vox build` does,
//! in three stages:
//!
//! 1. **Scripts**: the game's C# in Release, or in Debug for a development
//!    build, with the profile's defines.
//! 2. **Content**: the bundles the profile's levels need, into `Data/Bundles`.
//!    The scripts' `[ContentRoot]`s come along, so they are read from the
//!    build just made.
//! 3. **Player**: the player program, named after the game, with
//!    `Voxear.Core` and the scripts in `Data/Managed` and what to run in
//!    `Data/player.toml`.
//!
//! Then `build-report.json` records what was built and how long each stage
//! took. Before content, every scene and prefab is checked for `[Required]`
//! fields left empty and values `[ValidateInput]` says are wrong (see
//! `field_checks`); what it finds is logged as warnings and kept in the
//! report, and the build goes on.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use voxear_assets::{AssetDb, AssetKind, bundle::Compression};

use crate::{
    ContentSettings, Project,
    build_profile::{self, BuildProfile},
    content_build::{self, CodeRoot, glob},
    core_assembly, engine_version,
    field_checks::{self, FieldProblem},
    player::{BUNDLES_FOLDER, DATA_FOLDER, MANAGED_FOLDER, PLAYER_FORMAT, PlayerSettings, file_name},
    script_types,
    scripts::{self, ScriptsFor},
};
use voxear_scripting::{InspectorAnswer, InspectorAsk};

/// A program beside this one, such as `vox` beside the editor or the player
/// beside `vox`. A test runs from `deps/`, one folder down from them.
pub fn sibling_program(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let folder = exe.parent().unwrap_or(Path::new("."));
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let beside = folder.join(&file);
    match folder.parent().filter(|_| !beside.is_file() && folder.ends_with("deps")) {
        Some(above) => above.join(file),
        None => beside,
    }
}

/// The stages of a build, in order.
pub const BUILD_STAGES: [&str; 3] = ["scripts", "content", "player"];
/// Where a build's report goes, in its folder.
pub const REPORT_FILE: &str = "build-report.json";

/// What a build tells whoever started it, as it goes.
#[derive(Debug)]
pub enum BuildEvent<'a> {
    /// A stage starts: its name, and its place among [`BUILD_STAGES`].
    Stage { stage: &'static str, index: usize },
    /// Work within a stage: how much is done, of how much, and what is next.
    Progress { stage: &'static str, done: usize, total: usize, item: &'a str },
}

/// What a build made, written beside it as `build-report.json`.
#[derive(Debug, Serialize)]
pub struct Report {
    pub format: u32,
    pub project: String,
    pub profile: String,
    pub platform: String,
    pub development: bool,
    pub engine: String,
    pub version: Option<String>,
    pub commit: Option<String>,
    /// The build's folder, and its program in it, from the project's folder.
    pub output: PathBuf,
    pub program: PathBuf,
    pub start_level: String,
    pub levels: Vec<String>,
    /// How the bundles were compressed.
    pub compression: Compression,
    pub bundles: Vec<BundleSize>,
    /// The size of everything in the build's folder, in bytes.
    pub size: u64,
    pub stages: Vec<StageTime>,
    pub seconds: f64,
    /// What the check of every scene and prefab found.
    pub problems: Vec<FieldProblem>,
}

#[derive(Debug, Serialize)]
pub struct BundleSize {
    pub file: String,
    pub size: u64,
    pub roots: Vec<String>,
    pub assets: usize,
}

#[derive(Debug, Serialize)]
pub struct StageTime {
    pub stage: &'static str,
    pub seconds: f64,
}

/// Builds the project with the profile named `name` into the profile's
/// folder, copying `player` as its program. `events` hears of each stage
/// and step; an error from it stops the build there, as cancelling does.
pub fn build(
    project: &Project,
    name: &str,
    player: &Path,
    events: &mut dyn FnMut(BuildEvent) -> Result<()>,
) -> Result<Report> {
    let started = Instant::now();
    let profile = build_profile::load_profile(&project.root, name)?;
    let platform = profile.platform().to_string();
    ensure!(
        platform == build_profile::host_platform(),
        "`{name}` builds for {platform}, and for now a build runs on the platform it is for"
    );
    ensure!(player.is_file(), "the player program is missing at {}", player.display());
    let start_level = profile
        .levels
        .start
        .clone()
        .or_else(|| project.file.project.start_level.clone())
        .with_context(|| format!("`{name}` names no start level, and neither does voxear.toml"))?;
    let configuration = if profile.development.enabled { "Debug" } else { "Release" };
    let output = profile.output_dir(&project.root, name);
    let data = output.join(DATA_FOLDER);
    let mut stages = Vec::new();
    let mut timed = |stage: &'static str, since: Instant| stages.push(StageTime { stage, seconds: seconds(since) });

    // Scripts: built, then started, for the content roots they declare.
    let stage = Instant::now();
    events(BuildEvent::Stage { stage: "scripts", index: 0 })?;
    let (code_roots, problems) = match project.scripts_project() {
        Some(_) => {
            let place = ScriptsFor::Build { development: profile.development.enabled };
            let mut defines = scripts::built_in_defines(engine_version(), &platform, place);
            defines.extend(profile.scripting.defines.iter().cloned());
            let assembly =
                scripts::build_for(project, configuration, &defines)?.context("the scripts were not built")?;
            let mut runtime = scripts::start_assembly(&assembly, configuration)?;
            let roots = runtime.content_roots()?;
            let roots = roots.into_iter().map(|(pattern, source)| CodeRoot { pattern, source }).collect();
            (roots, check_fields(project, &mut runtime)?)
        }
        None => (Vec::new(), Vec::new()),
    };
    for problem in &problems {
        tracing::warn!("{problem}");
    }
    if !problems.is_empty() {
        let found = if problems.len() == 1 { "1 problem".to_string() } else { format!("{} problems", problems.len()) };
        tracing::warn!("{found} in the scenes and prefabs; the build goes on");
    }
    timed("scripts", stage);

    // Content: what the profile's levels need.
    let stage = Instant::now();
    events(BuildEvent::Stage { stage: "content", index: 1 })?;
    let (settings, levels) = content_settings(project, &profile, &start_level)?;
    let compression = settings.compression;
    let bundles = data.join(BUNDLES_FOLDER);
    let content =
        content_build::build_with(&project.root, &bundles, &settings, &code_roots, &mut |done, total, item| {
            events(BuildEvent::Progress { stage: "content", done, total, item })
        })?;
    for root in &content.unmatched {
        tracing::warn!("[ContentRoot(\"{}\")] on {} matches no asset", root.pattern, root.source);
    }
    timed("content", stage);

    // Player: the program, the managed code, and what to run.
    let stage = Instant::now();
    events(BuildEvent::Stage { stage: "player", index: 2 })?;
    let product_name = profile.player.product_name.clone().unwrap_or_else(|| project.name().to_string());
    let program = output.join(format!("{}{}", file_name(&product_name), std::env::consts::EXE_SUFFIX));
    std::fs::copy(player, &program).with_context(|| format!("copying the player to {}", program.display()))?;
    let assembly = copy_managed(project, configuration, profile.development.enabled, &data.join(MANAGED_FOLDER))?;
    copy_steam_audio(project, &output)?;
    let commit = git_commit(&project.root);
    let settings = PlayerSettings {
        format: PLAYER_FORMAT,
        product_name,
        company_name: profile.player.company_name.clone(),
        version: profile.version.version.clone(),
        start_level: start_level.clone(),
        scripts: assembly,
        development: profile.development.enabled,
        validation_layers: profile.development.validation_layers,
        fullscreen: profile.player.window == "fullscreen",
        width: profile.player.width,
        height: profile.player.height,
        engine: engine_version().to_string(),
        profile: name.to_string(),
        commit: commit.clone(),
        layers: project.file.layers.names.clone(),
        ignore: project.file.physics.ignore.clone(),
        languages: project.file.localization.languages.clone(),
        headphones: project.file.audio.headphones,
        reverb: project.file.audio.reverb,
        reverb_mode: project.file.audio.reverb_mode,
    };
    settings.write(&data)?;
    timed("player", stage);

    let report = Report {
        format: 1,
        project: project.name().to_string(),
        profile: name.to_string(),
        platform,
        development: profile.development.enabled,
        engine: engine_version().to_string(),
        version: profile.version.version.clone(),
        commit,
        program: program.strip_prefix(&project.root).unwrap_or(&program).to_path_buf(),
        start_level,
        levels,
        compression,
        bundles: content
            .bundles
            .iter()
            .map(|b| BundleSize { file: b.file.clone(), size: b.size, roots: b.roots.clone(), assets: b.assets.len() })
            .collect(),
        size: folder_size(&output),
        stages,
        seconds: seconds(started),
        problems,
        output: output.strip_prefix(&project.root).unwrap_or(&output).to_path_buf(),
    };
    let path = output.join(REPORT_FILE);
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(report)
}

/// Checks every scene and prefab against the scripts' description, asking
/// `[ValidateInput]` methods through the scripts just started, on a copy of
/// each behaviour bound to no Obj.
fn check_fields(project: &Project, runtime: &mut voxear_scripting::ScriptRuntime) -> Result<Vec<FieldProblem>> {
    let types = match script_types::cached(project).filter(|_| !script_types::is_stale(project)) {
        Some(types) => types,
        None => script_types::describe(project)?,
    };
    let mut world = voxear_ecs::World::new();
    let nobody = world.spawn(());
    let mut validate = |spec: &voxear_scene::BehaviourSpec, field: &str, method: &str| {
        let ask = InspectorAsk::Validate { method, field };
        match runtime.inspector(&mut world, nobody, spec, ask) {
            Ok(InspectorAnswer::Valid(false, said)) => Some(said),
            _ => None,
        }
    };
    field_checks::check(&project.root, &types, &mut validate)
}

/// The project's content settings with the levels the profile leaves out
/// excluded, and the levels that ship, which always include the start level.
fn content_settings(
    project: &Project,
    profile: &BuildProfile,
    start_level: &str,
) -> Result<(ContentSettings, Vec<String>)> {
    let own = &project.file.content;
    let mut settings = ContentSettings {
        include: own.include.clone(),
        exclude: own.exclude.clone(),
        merge_below_kb: own.merge_below_kb,
        split_above_mb: own.split_above_mb,
        compression: profile.content.compression.unwrap_or(own.compression),
    };
    let (db, _) = AssetDb::scan(&project.root)?;
    let start = db.guid_of(start_level).or_else(|| start_level.parse().ok()).and_then(|guid| db.get(guid));
    let start = start.with_context(|| format!("the start level {start_level} is not in the project"))?;
    ensure!(start.kind == AssetKind::Level, "the start level {start_level} is not a level");

    // A level is named by its GUID, as the Build window's level fields write it, or by a path or pattern.
    let wanted = |entry: &voxear_assets::AssetEntry| {
        let named = |p: &String| p.parse().ok() == Some(entry.guid) || glob(p, &entry.path);
        profile.levels.include.is_empty() || profile.levels.include.iter().any(named)
    };
    let mut levels = Vec::new();
    for entry in db.entries().filter(|e| e.kind == AssetKind::Level) {
        if wanted(entry) || entry.guid == start.guid {
            levels.push(entry.path.clone());
        } else {
            settings.exclude.push(entry.path.clone());
        }
    }
    levels.sort();
    Ok((settings, levels))
}

/// Copies `Voxear.Core` and the game's scripts into `managed`, with their
/// symbols for a development build. Returns the scripts' file name.
fn copy_managed(project: &Project, configuration: &str, symbols: bool, managed: &Path) -> Result<Option<String>> {
    if managed.exists() {
        std::fs::remove_dir_all(managed).with_context(|| format!("clearing {}", managed.display()))?;
    }
    std::fs::create_dir_all(managed).with_context(|| format!("making {}", managed.display()))?;
    let Some(assembly) = project.built_scripts_assembly(configuration) else { return Ok(None) };
    let core = core_assembly(configuration);
    for name in ["Voxear.Core.dll", "Voxear.Core.runtimeconfig.json", "Voxear.Core.deps.json"] {
        copy(&core.with_file_name(name), &managed.join(name))?;
    }
    if symbols {
        copy(&core.with_extension("pdb"), &managed.join("Voxear.Core.pdb"))?;
    }
    // The scripts' folder holds their own build and any library they use.
    let built = assembly.parent().context("the scripts were built nowhere")?;
    for entry in std::fs::read_dir(built).with_context(|| format!("reading {}", built.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let ships = name.ends_with(".dll") || name.ends_with(".deps.json") || (symbols && name.ends_with(".pdb"));
        if path.is_file() && ships && !name.starts_with("Voxear.Core.") {
            copy(&path, &managed.join(name))?;
        }
    }
    let name = assembly.file_name().context("the scripts have no file name")?.to_string_lossy().into_owned();
    Ok(Some(name))
}

/// Steam Audio's library beside the player, where it looks first, with its
/// licence, unless the project leaves it out. Without it, the player pans
/// 3D sound.
fn copy_steam_audio(project: &Project, output: &Path) -> Result<()> {
    use voxear_audio::steam_audio;
    if !project.file.audio.steam_audio {
        return Ok(());
    }
    let Some(library) = steam_audio::library_paths().into_iter().find(|path| path.is_file()) else {
        tracing::info!("Steam Audio is not here, so the player pans 3D sound");
        return Ok(());
    };
    copy(&library, &output.join(steam_audio::file_name()))?;
    let notices = output.join("Third Party/Steam Audio");
    std::fs::create_dir_all(&notices).with_context(|| format!("making {}", notices.display()))?;
    for (name, text) in steam_audio::NOTICES {
        std::fs::write(notices.join(name), text).with_context(|| format!("writing {name}"))?;
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    Ok(())
}

/// The commit the project is at, when it is a git repository.
fn git_commit(root: &Path) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(root).args(["rev-parse", "--short=12", "HEAD"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn folder_size(folder: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(folder) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => folder_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

fn seconds(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_name_becomes_a_file_name() {
        assert_eq!(file_name("Orbit Run"), "Orbit Run");
        assert_eq!(file_name("What? Now: 2"), "What_ Now_ 2");
        assert_eq!(file_name(" ... "), "Game");
    }
}
