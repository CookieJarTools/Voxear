use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, World};

use crate::lifecycle::BehaviourHandle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(pub String);
impl Component for Name {}

/// Local transform. Scripts read and write it in place, so the layout is part
/// of the C# ABI (`Voxear.Transform`): the padding keeps it 48 bytes on every
/// target whether or not glam uses SIMD.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    _pad0: f32,
    pub rotation: Quat,
    pub scale: Vec3,
    _pad1: f32,
}
impl Component for Transform {}

const _: () = assert!(size_of::<Transform>() == 48);

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);

    pub const fn new(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self { position, _pad0: 0.0, rotation, scale, _pad1: 0.0 }
    }

    pub const fn from_position(position: Vec3) -> Self {
        Self::new(position, Quat::IDENTITY, Vec3::ONE)
    }
}

/// Script behaviours attached to an Obj, in attach order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Behaviours(pub Vec<BehaviourHandle>);
impl Component for Behaviours {}

/// The Obj carries a behaviour that implements `IInstance`. The editor shows
/// a badge for it in the hierarchy.
#[derive(Clone, Copy, Debug, Default)]
pub struct InstanceMarker;
impl Component for InstanceMarker {}

/// Survives level and scene changes (Unity's `DontDestroyOnLoad`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScenePersistence;
impl Component for ScenePersistence {}

/// Registers the Obj components under the names scripts use to find them.
pub fn register_obj_components(world: &mut World) {
    world.register_named_component::<Name>("Voxear.Name");
    world.register_named_component::<Transform>("Voxear.Transform");
    world.register_named_component::<Behaviours>("Voxear.Behaviours");
    world.register_named_component::<InstanceMarker>("Voxear.InstanceMarker");
    world.register_named_component::<ScenePersistence>("Voxear.ScenePersistence");
}
