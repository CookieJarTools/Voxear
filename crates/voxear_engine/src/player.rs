//! The player: a built game, run from the `Data` folder beside its program.
//! `vox build` writes that folder (see [`crate::player_build`]):
//!
//! - `player.toml`: the game's name, start level and window, as
//!   [`PlayerSettings`]
//! - `Bundles/`: the built content
//! - `Managed/`: `Voxear.Core` and the game's scripts, for .NET to run
//!
//! A player keeps its log as Unity's `Player.log` is kept, in the user's data
//! folder under the company's and the game's names ([`log_path`]), the run
//! before's beside it, so it can be read after the game has closed: the
//! editor's Console opens it.

use std::path::{Path, PathBuf};

use crate::net::NetStart;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_render::{Renderer, RendererSettings};
use voxear_scripting::{SaveSettings, ScriptRuntime};

use crate::{
    Content, ContentSettings, Engine, Game, Project, ProjectFile, WindowOptions,
    project::{AudioSettings, LayerSettings, LocalizationSettings, PhysicsSettings, ProjectInfo},
    run_windowed, scripts,
};

/// The folder beside a player's program that holds its game, and what is in
/// it.
pub const DATA_FOLDER: &str = "Data";
pub const PLAYER_FILE: &str = "player.toml";
pub const BUNDLES_FOLDER: &str = "Bundles";
pub const MANAGED_FOLDER: &str = "Managed";
/// The format of `player.toml`.
pub const PLAYER_FORMAT: u32 = 1;
/// A player's log, and the one of the run before.
pub const PLAYER_LOG: &str = "player.log";
pub const PREVIOUS_LOG: &str = "player-prev.log";
/// The folder of a game's saves, beside its log.
pub const SAVES_FOLDER: &str = "Saves";

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
    /// A dedicated server's build, which runs without a window or sound
    /// until it is stopped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub server: bool,
    /// The project's `[network]` settings.
    #[serde(default)]
    pub network: crate::net::NetworkSettings,
    /// The project's `[floating_origin]` settings.
    #[serde(default)]
    pub floating_origin: crate::origin::OriginSettings,
    /// The project's `[quality]` budget for simulated particles.
    #[serde(default = "default_particles")]
    pub particles: u32,
    /// The project's `[world]` budget for making worlds of blocks.
    #[serde(default)]
    pub world: voxear_terrain::WorldBudget,
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
    /// The project's layers by number, and the pairs that pass through each
    /// other, as its `voxear.toml` has them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<[String; 2]>,
    /// The project's languages, the one it is written in first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Whether 3D sounds start out heard as through headphones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub headphones: bool,
    /// Whether 3D sounds start out ringing as the listener's room does,
    /// and how.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverb: bool,
    #[serde(default, with = "crate::project::ReverbModeName", skip_serializing_if = "is_parametric")]
    pub reverb_mode: voxear_audio::ReverbMode,
    /// The modules the project has.
    #[serde(default = "crate::modules::Modules::older_project")]
    pub modules: crate::modules::Modules,
}

fn is_parametric(mode: &voxear_audio::ReverbMode) -> bool {
    *mode == voxear_audio::ReverbMode::Parametric
}

fn default_particles() -> u32 {
    voxear_render::DEFAULT_PARTICLE_BUDGET
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

/// Where a built game keeps its log: in the user's data folder, under its
/// company's name and its own, as `player.log`. Linux: `$XDG_DATA_HOME`, or
/// `~/.local/share`; Windows: `%LOCALAPPDATA%`. `None` without a home.
pub fn log_path(company: Option<&str>, product: &str) -> Option<PathBuf> {
    Some(game_data_folder(company, product)?.join(PLAYER_LOG))
}

/// Where a game keeps its saves: `Saves`, beside its log. The editor and
/// `vox run` use the project's name without a company, the folder a build
/// whose profile leaves both names unset uses too.
pub fn save_folder(company: Option<&str>, product: &str) -> Option<PathBuf> {
    Some(game_data_folder(company, product)?.join(SAVES_FOLDER))
}

fn game_data_folder(company: Option<&str>, product: &str) -> Option<PathBuf> {
    let data = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        let xdg = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
        xdg.or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
    };
    let mut path = data?;
    if let Some(company) = company.filter(|c| !c.trim().is_empty()) {
        path.push(file_name(company));
    }
    path.push(file_name(product));
    Some(path)
}

/// Opens a new log for the game in `data`, keeping the last run's as
/// `player-prev.log`. `None` when there is nowhere to keep it, so the game
/// still runs.
pub fn open_log(data: &Path) -> Option<std::fs::File> {
    let settings = PlayerSettings::read(data).ok()?;
    let path = log_path(settings.company_name.as_deref(), &settings.product_name)?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let _ = std::fs::rename(&path, path.with_file_name(PREVIOUS_LOG));
    std::fs::File::create(&path).ok()
}

/// A name as a file name on Linux and Windows.
pub(crate) fn file_name(name: &str) -> String {
    let name: String =
        name.chars().map(|c| if r#"/\:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() { "Game".into() } else { name.to_string() }
}

/// How the player was asked to run, from its command line.
#[derive(Default)]
pub struct PlayerArgs {
    /// Simulate without a window or GPU.
    pub headless: bool,
    /// Stop after this many frames.
    pub frames: Option<u64>,
    /// Networked play to start with: a server, a host or joining one.
    pub net: Option<NetStart>,
    /// The port a server or host listens on; 0 for the project's.
    pub port: u16,
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
    // Debug draw is for development builds; a release draws none of it.
    voxear_core::debug_draw::set_enabled(settings.development);
    tracing::info!("{}{version}, {kind} with Voxear {} ({})", settings.product_name, settings.engine, settings.profile);

    let mut engine = Engine::new();
    if let Some(assembly) = &settings.scripts {
        let managed = data.join(MANAGED_FOLDER);
        let mut runtime = ScriptRuntime::start(&scripts::dotnet()?, &managed.join("Voxear.Core.dll"))?;
        runtime.load_game_assembly(&managed.join(assembly))?;
        let folder = save_folder(settings.company_name.as_deref(), &settings.product_name);
        runtime.set_saves(SaveSettings { folder, readable: false });
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
            layers: LayerSettings { names: settings.layers.clone() },
            physics: PhysicsSettings { ignore: settings.ignore.clone() },
            localization: LocalizationSettings { languages: settings.languages.clone() },
            audio: AudioSettings {
                headphones: settings.headphones,
                reverb: settings.reverb,
                reverb_mode: settings.reverb_mode,
                steam_audio: true,
            },
            network: settings.network.clone(),
            floating_origin: settings.floating_origin.clone(),
            quality: crate::project::QualitySettings { particles: settings.particles },
            world: settings.world,
            modules: Some(settings.modules),
        },
    };
    let mut game = Game::with_content(project, engine, content);
    // A development build marks a key no language has, as the editor does; a shipped game shows the key.
    if let Some(localization) = game.engine.world.resource_mut::<voxear_localization::Localization>() {
        localization.set_mark_missing(settings.development);
    }

    // A server build serves unless told otherwise; a port given applies to whichever.
    let net = args.net.or(settings.server.then_some(NetStart::Server(0))).map(|start| match start {
        NetStart::Server(_) => NetStart::Server(args.port),
        NetStart::Host(_) => NetStart::Host(args.port),
        joining => joining,
    });
    let serving = matches!(net, Some(NetStart::Server(_)));
    if args.headless || serving {
        // Without a window, sound plays out in silence.
        game.audio = crate::audio::GameAudio::new(crate::audio::Audio::quiet());
        game.start(None)?;
        if let Some(start) = net {
            game.start_session(start, voxear_net::Conditions::PERFECT)?;
        }
        match (serving, net.is_some()) {
            (true, _) => run_server(&mut game, args.frames),
            (false, true) => run_on_time(&mut game, args.frames),
            (false, false) => {
                for _ in 0..args.frames.unwrap_or(3) {
                    game.tick(1.0 / 60.0, None);
                }
            }
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
        console: settings.development.then(|| data.to_path_buf()),
    };
    let renderer = RendererSettings { renderdoc: false, capture_folder: None, hot_reload: false, software_rays: false };
    let setup = Box::new(move |game: &mut Game, renderer: &mut Renderer| {
        game.start(Some(renderer))?;
        if let Some(start) = net {
            game.start_session(start, voxear_net::Conditions::PERFECT)?;
        }
        Ok(())
    });
    run_windowed(game, options, renderer, setup)
}

/// Runs a server's game until it is stopped, or for `frames` ticks, saying
/// first that it is ready (see [`run_on_time`]).
pub fn run_server(game: &mut Game, frames: Option<u64>) {
    let session = game.engine.world.resource::<crate::net::NetSession>();
    let (rate, address) = session.map_or((60, None), |s| (s.tick_rate(), Some(s.address())));
    tracing::info!(
        "ready: {} on {}, {rate} ticks a second",
        game.levels.current().unwrap_or("no level"),
        address.map_or("no port".into(), |a| a.to_string())
    );
    run_on_time(game, frames);
}

/// Runs a game without a window a tick at a time, in real time, until it is
/// stopped or for `frames` ticks, as a networked game must. Each tick waits
/// for its own moment rather than for a length of time, so ticks never
/// drift; on Linux the system is asked to wake it within a microsecond of
/// that moment rather than its usual fifty. On Windows the standard
/// library's sleep already uses the system's high-resolution timer, so it
/// wakes within a millisecond rather than the usual 15.6. One that falls far
/// behind starts afresh rather than rushing.
pub fn run_on_time(game: &mut Game, frames: Option<u64>) {
    use std::time::{Duration, Instant};
    #[cfg(target_os = "linux")]
    // SAFETY: changing this thread's timer slack touches nothing else.
    unsafe {
        libc::prctl(libc::PR_SET_TIMERSLACK, 1000 as libc::c_ulong);
    }
    let rate = game.engine.world.resource::<crate::net::NetSession>().map_or(60, |s| s.tick_rate());
    let tick = Duration::from_secs_f64(1.0 / f64::from(rate));
    let mut next = Instant::now();
    while frames.is_none_or(|frames| game.engine.time().frame < frames) {
        game.tick(tick.as_secs_f32(), None);
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(250) {
            next = now;
        }
    }
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
