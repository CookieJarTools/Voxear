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
mod input;
mod inspector;
mod interop;
mod layers;
mod physics;
mod playing;
mod runtime;
mod saves;
mod services;
mod text;
mod ui;
mod ui_rows;
mod windows;

pub use audio::{AudioAsk, BusValue, PlaySound, SoundFacts, SourceValue, VoiceValue};
pub use display::CursorLock;
pub use interop::{
    ComponentValues, LightValues, SmokeActions, WaterSampler, set_light_values, set_mist_values, set_particle_values,
    set_smoke_actions, set_smoke_values, set_water_sampler,
};
pub use physics::PhysicsEventData;
pub use drawers::{DrawRequest, DrawerField, Drawn, FieldStep};
pub use gizmos::{GizmoAnswer, GizmoTarget, HandleId, HandleInput, ScriptHandle, ScriptHandleKind};
pub use hostfxr::{DotnetInstall, find_dotnet};
pub use inspector::{InspectorAnswer, InspectorAsk};
pub use runtime::{
    ConsoleName, CreatedBehaviour, ManagedMemory, PACKAGE_ASSEMBLIES, Reloaded, ScriptHost, ScriptRuntime,
    package_assemblies,
};
pub use saves::{SAVE_EXTENSION, SaveError, SaveFolder, SavePart, SaveSettings, SlotEntry, saved_language};
pub use services::{
    AssetName, ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, PreviewOptions, PreviewSource, PreviewState,
    SceneLoadOptions, SceneRequest, ScriptServices, ServicesScope, SpawnedPrefab, UiAnswer, UiAsk,
};
pub use ui_rows::RowWork;
pub use windows::WindowUpdate;
