//! Runs Voxear: the frame loop over the ECS, the behaviour lifecycle, C#
//! scripts and the renderer, either in a window or headless.

mod engine;
mod project;
mod windowed;

pub use engine::{Engine, STAGES};
pub use project::{ObjSpec, PROJECT_FILE, Project, ProjectFile, core_assembly, engine_root};
pub use windowed::run_windowed;
