//! C# scripting on CoreCLR.
//!
//! The engine hosts .NET itself through `hostfxr`, loads `Voxear.Core` into
//! the default load context and game scripts into a collectible one, which
//! is loaded again from a new build when they change (hot reload), and talks to them through two function tables: one
//! for engine calls scripts make, one for script calls the engine makes.

mod audio;
mod debug;
mod display;
mod drawers;
mod ecs;
mod gizmos;
mod hostfxr;
mod inspector;
mod interop;
mod playing;
mod runtime;
mod saves;
mod services;
mod ui;
mod ui_rows;

pub use audio::{AudioAsk, BusValue, PlaySound, SoundFacts, SourceValue, VoiceValue};
pub use display::CursorLock;
pub use drawers::{DrawRequest, DrawerField, Drawn, FieldStep};
pub use gizmos::{GizmoAnswer, GizmoTarget, HandleId, HandleInput, ScriptHandle, ScriptHandleKind};
pub use hostfxr::{DotnetInstall, find_dotnet};
pub use inspector::{InspectorAnswer, InspectorAsk};
pub use runtime::{ConsoleName, CreatedBehaviour, ManagedMemory, Reloaded, ScriptHost, ScriptRuntime};
pub use saves::{SAVE_EXTENSION, SaveError, SaveFolder, SavePart, SaveSettings, SlotEntry};
pub use services::{
    AssetName, ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, PreviewOptions, PreviewSource, PreviewState,
    SceneLoadOptions, SceneRequest, ScriptServices, ServicesScope, SpawnedPrefab, UiAnswer, UiAsk,
};
pub use ui_rows::RowWork;
