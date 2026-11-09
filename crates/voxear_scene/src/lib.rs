//! The Obj model on top of the ECS.
//!
//! An Obj is an entity with a [`Name`] and a [`Transform`]; script
//! behaviours attach to it and are driven by the [`Lifecycle`], which
//! guarantees Awake → OnEnable → Start ordering.

mod hierarchy;
mod layers;
mod lifecycle;
mod obj;
mod path;
pub mod plants;
mod switches;

pub use hierarchy::{Parent, WorldTransform, propagate_transforms, world_matrix};
pub use layers::{LAYER_COUNT, Layer, Layers};
pub use lifecycle::{Adopted, BehaviourHandle, BehaviourHost, Callbacks, Lifecycle, Phase, Registration, Request};
pub use path::{Path, PathPoint, PathSample};
pub use plants::{Plant, PlantSet, Plants, Trunk};
pub use obj::{
    AttachedBehaviour, BehaviourSpec, Behaviours, Destroying, EditorOnly, Hidden, InstanceMarker, Name, Placement,
    SceneComponents, ScenePersistence, Transform, register_obj_components,
};
pub use switches::{
    Inactive, OffComponents, Switch, Switched, inactive_objs, is_active, is_active_self, is_component_on, is_on,
    set_active, set_component_on,
};
