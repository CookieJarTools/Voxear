//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts, content, levels and the renderer, either in a window or headless.

pub mod build_profile;
mod content;
pub mod content_build;
pub mod debug_texts;
pub mod field_checks;
mod dev_console;
mod engine;
mod fly_camera;
mod game;
pub mod game_ui;
mod levels;
mod loading;
pub mod new_project;
pub mod player;
pub mod player_build;
pub mod prefab;
pub mod previews;
mod project;
pub mod remote_console;
pub mod scene_file;
pub mod script_types;
pub mod scripts;
mod stats_overlay;
pub mod ui_input;
pub mod upgrade;
mod windowed;

pub use content::{Content, ContentUsage, MaterialData, MaterialSource, Released, linear, srgb};
pub use engine::{EditBehaviours, Engine, STAGES};
pub use fly_camera::FlyCamera;
pub use game::Game;
pub use levels::{
    LEVEL_FORMAT, LevelChange, LevelFile, LevelOptions, LevelScene, LevelSummary, Levels, Tracked, summarise,
};
pub use loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads, ModelWork};
pub use prefab::{ObjPath, PrefabId, Template};
pub use previews::{AssetPreviews, ScriptPreviews, preview_parts};
pub use project::{
    ContentSettings, ENGINE_FILE, PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root, engine_version,
    is_install,
};
pub use stats_overlay::StatsOverlay;
pub use windowed::{Setup, WindowOptions, run_windowed};
