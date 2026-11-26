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
mod heat;
mod storms;
mod hostfxr;
mod input;
mod inspector;
mod interop;
mod layers;
pub mod blocks;
mod physics;
mod columns;
mod crowds;
mod plants;
mod post;
mod playing;
mod runtime;
mod saves;
mod services;
mod signals;
mod terrain;
mod switches;
mod text;
mod trees;
mod ui;
mod ui_rows;
mod voxels;
mod windows;

pub use audio::{AudioAsk, BusValue, PlaySound, SoundFacts, SourceValue, VoiceValue};
pub use display::{CursorLock, HdrInfo};
pub use interop::{
    ComponentKind, ComponentTexts, ComponentValues, LightValues, MotionMatcherTexts, NetCalls, NetFields, NetRaycast, NetRpcs, ParticleBlaster, RpcSend, RpcTaken, SmokeActions,
    set_particle_blaster,
    WaterSampler, set_block_mob_values, set_bone_attachment_values, set_light_values, set_mist_values,
    set_mob_motion_values, set_motion_matcher_texts,
    set_motion_matcher_values, set_net_calls, set_net_fields, set_net_object_values, set_net_raycast, set_net_rpcs,
    set_kind_texts, set_kind_values, set_particle_values, set_smoke_actions, set_smoke_values, set_water_sampler, set_water_splasher,
    PoseOps, set_pose_ops,
    ShaderPackRequest, WeatherRequest, WeatherState,
};
pub use heat::{HeatEventData, HeatHooks, set_heat_hooks};
pub use post::{
    LEVEL_COMPONENT, POST_IN_EFFECT, POST_OVERRIDES, POST_RESET, POST_SET, PROJECT_CHANGE, PROJECT_SECTION, PostCall,
    SHADOWS_IN_EFFECT, SHADOWS_RESET, SHADOWS_SET, SKY_IN_EFFECT, SKY_RESET, SKY_SET, SUN_LOOK, SUN_LOOK_RESET,
    SUN_LOOK_SET, WORLD_ASK, set_post_call,
};
pub use plants::{FoliageCall, set_foliage_call};
pub use columns::{ColumnEventData, GroundMade, set_ground_made};
pub use voxels::{
    VoxelEdits, VoxelPrintRequest, VoxelRaycast, VoxelSea, VoxelSets, VoxelShapeRequest, VoxelShapes, VoxelWaterAt,
    set_voxel_edits, set_voxel_raycast, set_voxel_sets, set_voxel_shapes, set_voxel_water, take_voxel_prints,
};
pub use crowds::{CrowdRecordsState, FarFigureData, FarFiguresSet, set_crowd_records, set_far_figures};
pub use storms::{STORM_NUMBERS, StormCall, StormState, set_storm_call, set_storm_state};
pub use physics::{ParticleHitData, PhysicsEventData, vehicle_curve_texts};
pub use drawers::{DrawRequest, DrawerField, Drawn, FieldStep};
pub use gizmos::{GizmoAnswer, GizmoTarget, HandleId, HandleInput, ScriptHandle, ScriptHandleKind};
pub use hostfxr::{DotnetInstall, find_dotnet};
pub use inspector::{DebugField, InspectorAnswer, InspectorAsk};
pub use runtime::{
    ConsoleName, CreatedBehaviour, ManagedMemory, PACKAGE_ASSEMBLIES, Reloaded, ScriptHost, ScriptRuntime,
    dotnet_started, package_assemblies,
};
pub use saves::{
    SAVE_EXTENSION, SaveError, SaveFolder, SavePart, SaveSettings, SlotEntry, saved_language, saves_folder,
};
pub use services::{
    AssetName, ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, ModelChange, ObjAsset, PreviewOptions,
    PreviewSource, PreviewState, SceneLoadOptions, SceneRequest, ScriptServices, ServicesScope, SpawnedPrefab,
    UiAnswer, UiAsk, change_model, model_part_name, obj_asset, paint_map, ask_world, prefab_placement, set_model, set_parent,
    ui_panel_document, ui_screen_document,
};
pub use trees::FelledInfo;
pub use ui_rows::RowWork;
pub use windows::WindowUpdate;
