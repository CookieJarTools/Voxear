//! C# scripting on CoreCLR.
//!
//! The engine hosts .NET itself through `hostfxr`, loads `Voxear.Core` into
//! the default load context and game scripts into a collectible one (so they
//! can be hot-reloaded), and talks to them through two function tables: one
//! for engine calls scripts make, one for script calls the engine makes.

mod hostfxr;
mod interop;
mod runtime;
mod services;

pub use hostfxr::{DotnetInstall, find_dotnet};
pub use runtime::{CreatedBehaviour, ScriptHost, ScriptRuntime};
pub use services::{
    ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, ScriptServices, ServicesScope, SpawnedPrefab,
};
