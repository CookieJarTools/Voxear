//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts, content, levels and the renderer, either in a window or headless.

mod content;
pub mod content_build;
mod engine;
mod fly_camera;
mod game;
mod levels;
mod loading;
pub mod prefab;
mod project;
pub mod scene_file;
mod windowed;

pub use content::{Content, MaterialData, MaterialSource};
pub use engine::{Engine, STAGES};
pub use fly_camera::FlyCamera;
pub use game::Game;
pub use levels::{LEVEL_FORMAT, LevelChange, LevelFile, LevelScene, Levels};
pub use loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads};
pub use prefab::{ObjPath, PrefabId, Template};
pub use project::{PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root};
pub use windowed::{Setup, run_windowed};
