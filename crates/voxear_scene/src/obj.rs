use std::sync::Arc;

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

    /// Scale, then rotation, then position, as one matrix.
    pub fn matrix(&self) -> voxear_core::math::Mat4 {
        voxear_core::math::Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
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

/// A script to attach: a behaviour, with field values to set before its
/// Awake, or, when `component` is set, a C# component (an `IComponent`
/// struct) made with those values. Both are saved, loaded and edited the
/// same way; only attaching them differs.
#[derive(Clone, Debug, PartialEq)]
pub struct BehaviourSpec {
    /// The full C# type name, which scenes save.
    pub type_name: String,
    /// Field values as the scene or prefab wrote them, by field name.
    pub fields: Arc<toml::Table>,
    pub component: bool,
    /// Whether a behaviour starts enabled, as its tick in the Inspector
    /// says. One that does not gets Awake but no OnEnable, Start or
    /// updates until something enables it. Always on for a C# component.
    pub enabled: bool,
}

impl Default for BehaviourSpec {
    fn default() -> Self {
        Self { type_name: String::new(), fields: Arc::default(), component: false, enabled: true }
    }
}

impl BehaviourSpec {
    pub fn new(type_name: &str) -> Self {
        Self { type_name: type_name.to_string(), ..Self::default() }
    }

    /// A C# component of this type, with every field as the struct starts.
    pub fn component(type_name: &str) -> Self {
        Self { component: true, ..Self::new(type_name) }
    }
}

/// While playing, the C# components an Obj was given by its scene or
/// prefab, as they were given, for the Inspector and saving; their values
/// are in components of their own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneComponents(pub Vec<BehaviourSpec>);
impl Component for SceneComponents {}

impl SceneComponents {
    /// Notes that `entity` was given these components.
    pub fn record(world: &mut World, entity: voxear_ecs::Entity, made: impl IntoIterator<Item = BehaviourSpec>) {
        let mut made = made.into_iter().peekable();
        if made.peek().is_none() {
            return;
        }
        match world.get_mut::<SceneComponents>(entity) {
            Some(kept) => kept.0.extend(made),
            None => {
                world.insert(entity, SceneComponents(made.collect()));
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttachedBehaviour {
    pub handle: BehaviourHandle,
    pub spec: BehaviourSpec,
}

/// Marks Objs the editor or a tool made for itself, such as the Scene
/// view's camera. No scene holds them, so level changes leave them alone,
/// and they are never saved or shown in the Hierarchy.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorOnly;
impl voxear_ecs::Component for EditorOnly {}

/// Script behaviours attached to an Obj, in attach order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Behaviours(pub Vec<AttachedBehaviour>);
impl Component for Behaviours {}

impl Behaviours {
    pub fn handles(&self) -> impl Iterator<Item = BehaviourHandle> + '_ {
        self.0.iter().map(|b| b.handle)
    }

    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|b| b.spec.type_name.as_str())
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

/// Not drawn: the Obj's meshes and models, and those of every Obj under it,
/// are hidden, as a wizard turned to mist is. Its lights, effects,
/// colliders and scripts carry on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hidden;
impl Component for Hidden {}

/// Survives level and scene changes (Unity's `DontDestroyOnLoad`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
