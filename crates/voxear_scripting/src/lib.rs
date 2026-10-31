//! C# scripting on CoreCLR.
//!
//! The engine hosts .NET itself through `hostfxr`, loads `Voxear.Core` into
//! the default load context and game scripts into a collectible one, which
//! is loaded again from a new build when they change (hot reload), and talks to them through two function tables: one
//! for engine calls scripts make, one for script calls the engine makes.

mod ecs;
mod hostfxr;
mod interop;
mod runtime;
mod services;

pub use hostfxr::{DotnetInstall, find_dotnet};
pub use runtime::{CreatedBehaviour, Reloaded, ScriptHost, ScriptRuntime};
pub use services::{
    ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, PreviewOptions, PreviewSource, PreviewState,
    ScriptServices, ServicesScope, SpawnedPrefab,
};
