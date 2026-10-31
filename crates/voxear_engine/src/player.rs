//! The player: a built game, run from the `Data` folder beside its program.
//! `vox build` writes that folder (see [`crate::player_build`]):
//!
//! - `player.toml`: the game's name, start level and window, as
//!   [`PlayerSettings`]
//! - `Bundles/`: the built content
//! - `Managed/`: `Voxear.Core` and the game's scripts, for .NET to run

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_render::{Renderer, RendererSettings};
use voxear_scripting::ScriptRuntime;

use crate::{
    Content, ContentSettings, Engine, Game, Project, ProjectFile, WindowOptions, project::ProjectInfo, run_windowed,
    scripts,
};

/// The folder beside a player's program that holds its game, and what is in
/// it.
pub const DATA_FOLDER: &str = "Data";
pub const PLAYER_FILE: &str = "player.toml";
pub const BUNDLES_FOLDER: &str = "Bundles";
pub const MANAGED_FOLDER: &str = "Managed";
/// The format of `player.toml`.
pub const PLAYER_FORMAT: u32 = 1;

/// `Data/player.toml`: what the player runs, and how, from the build profile
/// it was built with.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSettings {
    pub format: u32,
    /// The game's name, which titles its window.
    pub product_name: String,
    pub company_name: Option<String>,
    /// The game's version, as players see it.
    pub version: Option<String>,
    /// The level the game opens on.
    pub start_level: String,
    /// The game's scripts in `Managed/`, when it has any.
    pub scripts: Option<String>,
    /// A development build, with the tools `vox run` has.
    pub development: bool,
    /// Vulkan's validation layers, when installed.
    pub validation_layers: bool,
    pub fullscreen: bool,
    pub width: u32,
    pub height: u32,
    /// What built it: the engine's version, the profile, and the project's
    /// git commit, when it has one.
    pub engine: String,
    pub profile: String,
    pub commit: Option<String>,
}

impl PlayerSettings {
    pub fn read(data: &Path) -> Result<Self> {
        let path = data.join(PLAYER_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("no game at {}", path.display()))?;
        let settings: Self = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        anyhow::ensure!(
            settings.format == PLAYER_FORMAT,
            "{} is format {}, and this player reads format {PLAYER_FORMAT}; build the game again",
            path.display(),
            settings.format
        );
        Ok(settings)
    }

    pub fn write(&self, data: &Path) -> Result<()> {
        let path = data.join(PLAYER_FILE);
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, format!("# Written by vox build: what this player runs.\n{text}"))
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// How the player was asked to run, from its command line.
#[derive(Default)]
pub struct PlayerArgs {
    /// Simulate without a window or GPU.
    pub headless: bool,
    /// Stop after this many frames.
    pub frames: Option<u64>,
}

/// The `Data` folder beside the running program.
pub fn data_folder() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding the player's program")?;
    Ok(exe.parent().context("the player's program has no folder")?.join(DATA_FOLDER))
}

/// Runs the game in `data` until its window closes.
pub fn run_player(data: &Path, args: &PlayerArgs) -> Result<()> {
    let settings = PlayerSettings::read(data)?;
    let version = settings.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
    let kind = if settings.development { "a development build" } else { "built" };
    tracing::info!("{}{version}, {kind} with Voxear {} ({})", settings.product_name, settings.engine, settings.profile);

    let mut engine = Engine::new();
    if let Some(assembly) = &settings.scripts {
        let managed = data.join(MANAGED_FOLDER);
        let mut runtime = ScriptRuntime::start(&scripts::dotnet()?, &managed.join("Voxear.Core.dll"))?;
        runtime.load_game_assembly(&managed.join(assembly))?;
        engine.set_scripts(runtime);
    }
    let content = Content::open_bundles(data, &data.join(BUNDLES_FOLDER))?;
    let project = Project {
        root: data.to_path_buf(),
        file: ProjectFile {
            project: ProjectInfo {
                name: settings.product_name.clone(),
                scripts: None,
                start_level: Some(settings.start_level.clone()),
                engine: None,
            },
            content: ContentSettings::default(),
        },
    };
    let mut game = Game::with_content(project, engine, content);

    if args.headless {
        game.start(None)?;
        for _ in 0..args.frames.unwrap_or(3) {
            game.tick(1.0 / 60.0, None);
        }
        tracing::info!("ran {} frames", game.engine.time().frame);
        return Ok(());
    }
    let options = WindowOptions {
        title: settings.product_name.clone(),
        size: (settings.width, settings.height),
        fullscreen: settings.fullscreen,
        frames: args.frames,
        stats: false,
        tools: settings.development,
    };
    let renderer = RendererSettings { renderdoc: false, capture_folder: None, hot_reload: false };
    let setup = Box::new(|game: &mut Game, renderer: &mut Renderer| game.start(Some(renderer)));
    run_windowed(game, options, renderer, setup)
}

/// Whether Vulkan's validation layers run, unless `VOXEAR_VALIDATION` says.
/// Call before anything else starts a thread, since it sets the environment.
pub fn choose_validation(data: &Path) {
    if std::env::var_os("VOXEAR_VALIDATION").is_some() {
        return;
    }
    let wanted = PlayerSettings::read(data).is_ok_and(|settings| settings.validation_layers);
    // SAFETY: the player calls this first thing in main, before any thread starts.
    unsafe { std::env::set_var("VOXEAR_VALIDATION", if wanted { "1" } else { "0" }) };
}
