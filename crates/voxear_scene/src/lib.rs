//! The Obj model on top of the ECS.
//!
//! An Obj is an entity with a [`Name`] and a [`Transform`]; script
//! behaviours attach to it and are driven by the [`Lifecycle`], which
//! guarantees Awake → OnEnable → Start ordering.

mod hierarchy;
mod lifecycle;
mod obj;

pub use hierarchy::{Parent, WorldTransform, propagate_transforms, world_matrix};
pub use lifecycle::{Adopted, BehaviourHandle, BehaviourHost, Callbacks, Lifecycle, Phase, Registration, Request};
pub use obj::{
    AttachedBehaviour, BehaviourSpec, Behaviours, Destroying, InstanceMarker, Name, Placement, ScenePersistence,
    Transform, register_obj_components,
};
