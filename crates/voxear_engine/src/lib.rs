//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts, content, levels and the renderer, either in a window or headless.

pub mod acoustics;
pub mod animation;
pub mod audio;
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
pub mod joints;
pub mod keymaps;
pub mod lod_groups;
mod levels;
pub mod lighting;
mod lights;
mod loading;
pub mod memory;
pub mod new_project;
pub mod package_assets;
pub mod package_scripts;
pub mod packages;
mod mist;
mod particles;
pub mod smoke;
pub use voxear_localization as localization;
pub use voxear_physics::scene as physics;
pub use voxear_physics2d::scene as physics2d;
pub mod player;
pub mod player_build;
pub mod net;
pub mod prefab;
pub mod prefab_overrides;
pub mod previews;
mod project;
pub mod remote_console;
pub mod scene_file;
pub mod script_types;
pub mod scripts;
pub mod shaders;
mod stats_overlay;
pub mod steam_language;
pub mod ui_input;
pub mod upgrade;
pub mod water;
mod windowed;

pub use content::{
    Content, ContentUsage, MaterialData, MaterialMap, MaterialSource, MovedPart, PropertyValue, Reimported, Released,
    default_on_gpu, linear, srgb,
};
pub use engine::{EditBehaviours, Engine, STAGES};
pub use fly_camera::FlyCamera;
pub use game::{Display, Game};
pub use levels::{
    LEVEL_FORMAT, LevelChange, LevelFile, LevelOptions, LevelScene, LevelSummary, Levels, Tracked, summarise,
};
pub use loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads, ModelWork};
pub use prefab::{ObjPath, PrefabId, Template};
pub use previews::{AssetPreviews, ScriptPreviews, preview_parts};
pub use project::{
    ContentSettings, ENGINE_FILE, PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root, engine_version,
    is_install, manual_folder,
};
pub use stats_overlay::StatsOverlay;
pub use windowed::{Setup, WindowOptions, run_windowed};
