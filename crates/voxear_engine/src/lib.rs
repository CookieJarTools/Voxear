//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts and the renderer, either in a window or headless.

mod bootstrap;
mod engine;
mod fly_camera;
mod project;
mod windowed;

pub use bootstrap::spawn_objs;
pub use engine::{Engine, STAGES};
pub use fly_camera::FlyCamera;
pub use project::{ObjSpec, PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root};
pub use windowed::{Setup, run_windowed};
