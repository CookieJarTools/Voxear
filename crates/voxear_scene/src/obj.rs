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

/// Where to put a new Obj's root, as Unity's `Instantiate(prefab, position,
/// rotation)` does: its scale stays the prefab's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Placement {
    pub const fn at(position: Vec3) -> Self {
        Self { position, rotation: Quat::IDENTITY }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedBehaviour {
    pub handle: BehaviourHandle,
    /// The full C# type name, which scenes save.
    pub type_name: String,
}

/// Script behaviours attached to an Obj, in attach order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Behaviours(pub Vec<AttachedBehaviour>);
impl Component for Behaviours {}

impl Behaviours {
    pub fn handles(&self) -> impl Iterator<Item = BehaviourHandle> + '_ {
        self.0.iter().map(|b| b.handle)
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|b| b.type_name.as_str())
    }
}

/// The Obj is being destroyed: its behaviours get OnDisable and OnDestroy,
/// it stops rendering, and it is removed at the end of the frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Destroying;
impl Component for Destroying {}

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
    world.register_named_component::<crate::Parent>("Voxear.Parent");
    world.register_named_component::<crate::WorldTransform>("Voxear.WorldTransform");
    world.register_named_component::<Destroying>("Voxear.Destroying");
}
