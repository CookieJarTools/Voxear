//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts and the renderer, either in a window or headless.

mod bootstrap;
mod engine;
mod project;
mod windowed;

pub use bootstrap::spawn_objs;
pub use engine::{Engine, STAGES};
pub use project::{ObjSpec, PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root};
pub use windowed::{Setup, run_windowed};
