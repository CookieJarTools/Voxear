//! Physics on Objs, as Unity's: a **Collider** gives an Obj a shape that
//! others bump into, a **Rigidbody** beside it makes physics move it, and
//! a **Character Controller** walks it, moved by the game's scripts. Each
//! is written in a scene file as its table:
//!
//! ```toml
//! [obj.collider]
//! shape = "box"          # box, sphere, capsule or mesh
//! size = [1.0, 2.0, 1.0] # a box's
//! center = [0.0, 1.0, 0.0]
//!
//! [obj.rigidbody]
//! mass = 10.0
//!
//! [obj.character]
//! height = 1.8
//! radius = 0.3
//! ```
//!
//! While the game plays, [`PhysicsScene`] keeps a Jolt body for each Obj
//! with a Collider, made again when the collider, rigidbody or the Obj's
//! scale change: static without a Rigidbody, moved where the game puts it;
//! kinematic, moved there over a step, pushing what is in the way; and
//! dynamic, moved by physics, its Transform following. Each fixed step
//! moves them all on (see `Engine::tick`), and the contacts that began and
//! ended come after, for scripts.
//!
//! Each body is on its Obj's [`Layer`], and Objs on layers the world's
//! [`Layers`] keep apart pass through each other.
//!
//! A Mesh collider takes its shape from its Obj's mesh, as Unity's Mesh
//! Collider does, given to it as a [`ColliderMesh`] beside it: the mesh's
//! triangles for what does not move, or the convex hull round them, which
//! is all a dynamic Rigidbody can have. An Obj whose mesh has not been
//! given has no body until it is.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Layer, Layers, Parent, Transform};

use crate::{
    BodyDesc, BodyId, CharacterDesc, CharacterId, CharacterState, JointId, JointKind, Motion, PhysicsWorld, ShapeId,
};

/// A collider's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColliderShape {
    #[default]
    Box,
    Sphere,
    Capsule,
    /// The Obj's own mesh.
    Mesh,
}

impl ColliderShape {
    pub const ALL: [Self; 4] = [Self::Box, Self::Sphere, Self::Capsule, Self::Mesh];

    pub fn label(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Capsule => "Capsule",
            Self::Mesh => "Mesh",
        }
    }
}

/// What a collider is made of, to sound: Steam Audio's materials, which
/// soak up and let through more or less of a sound, a carpet soaking up
/// much and glass letting through more than brick (see
/// `voxear_audio::steam_audio::AcousticMaterial`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Acoustic {
    #[default]
    Generic,
    Brick,
    Concrete,
    Ceramic,
    Gravel,
    Carpet,
    Glass,
    Plaster,
    Wood,
    Metal,
    Rock,
}

impl Acoustic {
    pub const ALL: [Self; 11] = [
        Self::Generic,
        Self::Brick,
        Self::Concrete,
        Self::Ceramic,
        Self::Gravel,
        Self::Carpet,
        Self::Glass,
        Self::Plaster,
        Self::Wood,
        Self::Metal,
        Self::Rock,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Brick => "Brick",
            Self::Concrete => "Concrete",
            Self::Ceramic => "Ceramic",
            Self::Gravel => "Gravel",
            Self::Carpet => "Carpet",
            Self::Glass => "Glass",
            Self::Plaster => "Plaster",
            Self::Wood => "Wood",
            Self::Metal => "Metal",
            Self::Rock => "Rock",
        }
    }
}

/// A shape around an Obj that others bump into, scaled with the Obj, its
/// middle at `center` from the Obj's origin. A box uses `size`, a sphere
/// `radius`, and a capsule, standing up, `height` and `radius`. A mesh is
/// the Obj's own, wrapped in its convex hull when `convex`, and has no
/// center. A trigger pushes nothing, and only notices what enters it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collider {
    #[serde(default)]
    pub shape: ColliderShape,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub center: [f32; 3],
    #[serde(default = "unit", skip_serializing_if = "is_unit")]
    pub size: [f32; 3],
    #[serde(default = "half", skip_serializing_if = "is_half")]
    pub radius: f32,
    #[serde(default = "two", skip_serializing_if = "is_two")]
    pub height: f32,
    /// A mesh's convex hull rather than its triangles, as a dynamic
    /// Rigidbody's always is.
    #[serde(default, skip_serializing_if = "is_false")]
    pub convex: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trigger: bool,
    /// How much it grips what slides on it, from 0 for ice.
    #[serde(default = "grip", skip_serializing_if = "is_grip")]
    pub friction: f32,
    /// How much what hits it bounces back, from 0 to 1.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub bounciness: f32,
    /// What it is made of, to the sounds whose way it is in.
    #[serde(default, skip_serializing_if = "is_generic")]
    pub acoustic: Acoustic,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Box,
            center: [0.0; 3],
            size: unit(),
            radius: half(),
            height: two(),
            convex: false,
            trigger: false,
            friction: grip(),
            bounciness: 0.0,
            acoustic: Acoustic::Generic,
        }
    }
}

impl Component for Collider {}

/// The triangles a Mesh collider takes its shape from, in its Obj's own
/// space: its points, and three indices a triangle. The engine gives each
/// Obj with a Mesh collider its mesh's, and `key` names the mesh, so that
/// its body is made again when the mesh changes.
#[derive(Clone, Debug)]
pub struct ColliderMesh {
    pub key: u64,
    pub points: Arc<[Vec3]>,
    pub indices: Arc<[u32]>,
}

impl Component for ColliderMesh {}

/// Makes physics move its Obj, which needs a Collider beside it. A
/// kinematic one is moved by the game instead, pushing what is in the way.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rigidbody {
    /// In kilograms.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub mass: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub kinematic: bool,
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub gravity: bool,
    /// How quickly it slows, moving and turning, through the air.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub drag: f32,
    #[serde(default = "turn_drag", skip_serializing_if = "is_turn_drag")]
    pub angular_drag: f32,
    /// Sweeps its shape as it moves, so a fast body cannot pass through a
    /// thin one between steps.
    #[serde(default, skip_serializing_if = "is_false")]
    pub continuous: bool,
}

impl Default for Rigidbody {
    fn default() -> Self {
        Self { mass: 1.0, kinematic: false, gravity: true, drag: 0.0, angular_drag: turn_drag(), continuous: false }
    }
}

impl Component for Rigidbody {}

/// Walks its Obj: a capsule standing on the Obj's origin that the game
/// moves by velocity, sliding along walls, climbing steps up to
/// `step_offset` and standing on slopes up to `slope_limit` degrees.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterController {
    #[serde(default = "body_height", skip_serializing_if = "is_body_height")]
    pub height: f32,
    #[serde(default = "body_radius", skip_serializing_if = "is_body_radius")]
    pub radius: f32,
    #[serde(default = "slope", skip_serializing_if = "is_slope")]
    pub slope_limit: f32,
    #[serde(default = "step", skip_serializing_if = "is_step")]
    pub step_offset: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self { height: body_height(), radius: body_radius(), slope_limit: slope(), step_offset: step() }
    }
}

impl Component for CharacterController {}

/// How a Joint holds its Obj.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JointType {
    /// Glued where they are when the game starts.
    #[default]
    Fixed,
    /// Turning about an axis through its anchor, as a door on its hinges.
    Hinge,
    /// Turning every way about its anchor, as a ball in its socket.
    Ball,
    /// Its centre kept within a range of its anchor, as a weight on a rope.
    Distance,
}

impl JointType {
    pub const ALL: [Self; 4] = [Self::Fixed, Self::Hinge, Self::Ball, Self::Distance];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed",
            Self::Hinge => "Hinge",
            Self::Ball => "Ball",
            Self::Distance => "Distance",
        }
    }
}

/// Holds its Obj's Rigidbody to another Obj's, or to where it is in the
/// world when it names none, as Unity's joints do. Its anchor, from the
/// Obj's origin, is the hinge's or the ball's point, and the point a
/// Distance joint's Obj hangs from; a hinge turns about `axis`, in the
/// Obj's space. Limits, when given, are a hinge's least and greatest angle
/// in degrees, or a Distance joint's least and greatest length in metres.
/// Changing it remakes the joint where its Objs are now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub kind: JointType,
    pub connected: Option<Entity>,
    pub anchor: [f32; 3],
    pub axis: [f32; 3],
    pub limits: Option<[f32; 2]>,
}

impl Default for Joint {
    fn default() -> Self {
        Self { kind: JointType::Fixed, connected: None, anchor: [0.0; 3], axis: [0.0, 1.0, 0.0], limits: None }
    }
}

impl Component for Joint {}

/// What a joint was made from, to make it again when any of it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct JointKey {
    joint: Joint,
    body: BodyId,
    other: Option<BodyId>,
}

/// What a body was made from, to make it again when any of it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyKey {
    collider: Collider,
    rigidbody: Option<Rigidbody>,
    scale: Vec3,
    layer: u8,
    /// A Mesh collider's [`ColliderMesh`] key.
    mesh: Option<u64>,
}

struct Kept {
    body: BodyId,
    shape: ShapeId,
    key: BodyKey,
    /// Where it was put last, to tell when the game moved it.
    placed: (Vec3, Quat),
}

/// A Character Controller's character, what it was made from, where its
/// Obj was last, and how its last move went.
struct Walker {
    id: CharacterId,
    controller: CharacterController,
    layer: u8,
    placed: Vec3,
    state: Option<CharacterState>,
}

/// Where an Obj is and where its body was put last are the same place:
/// near enough, and a turn and its negative alike.
fn same_place(a: (Vec3, Quat), b: (Vec3, Quat)) -> bool {
    a.0.abs_diff_eq(b.0, 1e-4) && a.1.dot(b.1).abs() > 1.0 - 1e-6
}

/// Where a ray met an Obj's collider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneHit {
    pub obj: Entity,
    pub point: Vec3,
    pub normal: Vec3,
    /// How far from where the ray started, in metres.
    pub distance: f32,
}

/// How far a ray reaches when it is told to reach forever.
const FOREVER: f32 = 1.0e6;

/// Two Objs that began or stopped touching in a step, or a trigger and
/// what entered or left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsEvent {
    pub began: bool,
    pub trigger: bool,
    pub a: Entity,
    pub b: Entity,
    pub point: Vec3,
    pub normal: Vec3,
}

/// How many steps a pair Jolt let go of without parting, as when a body
/// is made again, is kept touching once both are awake, to be told of again.
const UNSURE_STEPS: u8 = 2;

/// The Jolt world for the Objs with colliders, while the game plays.
pub struct PhysicsScene {
    world: PhysicsWorld,
    bodies: HashMap<Entity, Kept>,
    owners: HashMap<BodyId, Entity>,
    characters: HashMap<Entity, Walker>,
    joints: HashMap<Entity, (JointId, JointKey)>,
    events: Vec<PhysicsEvent>,
    gravity: Vec3,
    /// Which layers collide, as the world was told last.
    collide: [u32; 32],
    /// Whether it has stepped: until then, a script's question brings its
    /// bodies to where their Objs are first.
    stepped: bool,
    /// The pairs of Objs touching, by [`pair`].
    touching: HashSet<(Entity, Entity)>,
    /// Touching pairs Jolt stopped telling of without their parting: one
    /// went to sleep, or was made again. Each counts the steps since both
    /// were awake.
    unsure: HashMap<(Entity, Entity), u8>,
}

/// The layer an Obj is on, 0 without one.
fn layer(world: &World, entity: Entity) -> u8 {
    world.get::<Layer>(entity).map_or(0, |layer| layer.0)
}

/// Two Objs as a pair, the same whichever comes first.
fn pair(a: Entity, b: Entity) -> (Entity, Entity) {
    if a.to_bits() <= b.to_bits() { (a, b) } else { (b, a) }
}

impl Default for PhysicsScene {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsScene {
    pub fn new() -> Self {
        Self {
            world: PhysicsWorld::default(),
            bodies: HashMap::new(),
            owners: HashMap::new(),
            characters: HashMap::new(),
            joints: HashMap::new(),
            events: Vec::new(),
            gravity: Vec3::new(0.0, -9.81, 0.0),
            collide: [u32::MAX; 32],
            stepped: false,
            touching: HashSet::new(),
            unsure: HashMap::new(),
        }
    }

    /// Whether a world has anything for physics to do: an Obj with a
    /// Collider or a Character Controller.
    pub fn wanted(world: &World) -> bool {
        world.query_ref::<&Collider>().into_iter().next().is_some()
            || world.query_ref::<&CharacterController>().into_iter().next().is_some()
    }

    /// Which way and how hard everything falls, in metres per second per second.
    pub fn gravity(&self) -> Vec3 {
        self.gravity
    }

    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.gravity = gravity;
        self.world.set_gravity(gravity);
    }

    /// Brings the bodies to where their Objs are, before the first step,
    /// so that a script's question in `Start` finds them.
    pub fn ready(&mut self, world: &World) {
        if !self.stepped {
            self.sync(world, 0.0);
        }
    }

    /// The body of an Obj's collider, made or made again now if its
    /// collider, Rigidbody or scale changed since the last step, so that
    /// a script can push a body it has only just given a Rigidbody.
    pub fn body_now(&mut self, world: &World, entity: Entity) -> Option<BodyId> {
        let collider = *world.get::<Collider>(entity)?;
        self.sync_body(world, entity, collider, 0.0);
        self.body(entity)
    }

    /// The first collider a ray from `origin` meets along `direction`
    /// within `distance` metres, on an Obj on a layer in `mask`, passing
    /// over triggers unless `triggers`. As in Unity, a ray that starts
    /// inside a collider passes out of it.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, distance: f32, mask: u32, triggers: bool) -> Option<SceneHit> {
        let direction = direction.normalize_or_zero();
        if direction == Vec3::ZERO || distance.is_nan() || distance <= 0.0 {
            return None;
        }
        let reach = distance.min(FOREVER);
        let hit = self.world.raycast(origin, direction * reach, mask, None, triggers)?;
        let obj = self.owner(hit.body)?;
        Some(SceneHit { obj, point: hit.point, normal: hit.normal, distance: hit.fraction * reach })
    }

    /// The Objs on layers in `mask` whose colliders a sphere overlaps, each
    /// once, with triggers when `triggers`.
    pub fn overlap_sphere(&self, center: Vec3, radius: f32, mask: u32, triggers: bool) -> Vec<Entity> {
        let mut objs: Vec<Entity> = Vec::new();
        for body in self.world.bodies_in_sphere(center, radius, mask, triggers) {
            if let Some(obj) = self.owner(body).filter(|obj| !objs.contains(obj)) {
                objs.push(obj);
            }
        }
        objs
    }

    /// How an Obj's Character Controller's last move went.
    pub fn character_state(&self, entity: Entity) -> Option<CharacterState> {
        self.characters.get(&entity)?.state
    }

    /// Whether two Objs' colliders are touching, or one is inside the other's trigger.
    pub fn touching(&self, a: Entity, b: Entity) -> bool {
        self.touching.contains(&pair(a, b))
    }

    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut PhysicsWorld {
        &mut self.world
    }

    /// The Obj a body is the collider of.
    pub fn owner(&self, body: BodyId) -> Option<Entity> {
        self.owners.get(&body).copied()
    }

    /// The body of an Obj's collider, while it has one.
    pub fn body(&self, entity: Entity) -> Option<BodyId> {
        self.bodies.get(&entity).map(|kept| kept.body)
    }

    /// The contacts and trigger crossings since the last call.
    pub fn take_events(&mut self) -> Vec<PhysicsEvent> {
        std::mem::take(&mut self.events)
    }

    /// Moves physics on by one fixed step: bodies made, moved or taken out
    /// as their Objs are now, the step, the dynamic bodies' Objs moved to
    /// where physics put them, and the step's contacts noted.
    pub fn step(&mut self, world: &mut World, dt: f32) {
        self.sync(world, dt);
        self.stepped = true;
        self.world.step(dt, 1);
        self.write_back(world);
        self.note_contacts();
    }

    fn sync(&mut self, world: &World, dt: f32) {
        let collide = world.resource::<Layers>().map_or([u32::MAX; 32], Layers::matrix);
        if collide != self.collide {
            self.collide = collide;
            self.world.set_layers(&collide);
        }
        let colliders: Vec<(Entity, Collider)> = world
            .query_ref::<(Entity, &Collider)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, collider)| (entity, *collider))
            .collect();
        let gone: Vec<Entity> =
            self.bodies.keys().copied().filter(|entity| !colliders.iter().any(|(e, _)| e == entity)).collect();
        for entity in gone {
            self.remove(entity);
            self.forget(entity);
        }
        for (entity, collider) in colliders {
            self.sync_body(world, entity, collider, dt);
        }
        self.sync_characters(world);
        self.sync_joints(world);
    }

    /// Makes each Obj's joint between its body and the connected Obj's, or
    /// the world, where they are now, and again when either body or the
    /// joint changed. A joint whose Objs have no bodies waits for them.
    fn sync_joints(&mut self, world: &World) {
        let wanted: Vec<(Entity, Joint)> = world
            .query_ref::<(Entity, &Joint)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, joint)| (entity, *joint))
            .collect();
        let gone: Vec<Entity> =
            self.joints.keys().copied().filter(|entity| !wanted.iter().any(|(e, _)| e == entity)).collect();
        for entity in gone {
            if let Some((joint, _)) = self.joints.remove(&entity) {
                self.world.remove_joint(joint);
            }
        }
        for (entity, joint) in wanted {
            let Some(body) = self.body(entity) else { continue };
            let other = match joint.connected {
                Some(connected) => match self.body(connected) {
                    Some(other) => Some(other),
                    None => continue,
                },
                None => None,
            };
            let key = JointKey { joint, body, other };
            if self.joints.get(&entity).is_some_and(|(_, had)| *had == key) {
                continue;
            }
            if let Some((old, _)) = self.joints.remove(&entity) {
                self.world.remove_joint(old);
            }
            let placed = voxear_scene::world_matrix(world, entity);
            let (_, rotation, _) = placed.to_scale_rotation_translation();
            let point = placed.transform_point3(Vec3::from(joint.anchor));
            let axis = (rotation * Vec3::from(joint.axis)).normalize_or(Vec3::Y);
            let limits = joint.limits.map(|[least, most]| (least.min(most), least.max(most)));
            let kind = match joint.kind {
                JointType::Fixed => JointKind::Fixed,
                JointType::Ball => JointKind::Point(point),
                JointType::Hinge => {
                    // Jolt measures how far the second body turns from the first, and the first is
                    // this Obj, so its own angles are turned round.
                    let limits = limits.map(|(least, most)| (-most.to_radians(), -least.to_radians()));
                    JointKind::Hinge { point, axis, limits }
                }
                JointType::Distance => JointKind::Distance { point, range: limits },
            };
            if let Some(made) = self.world.add_joint(kind, body, other) {
                self.joints.insert(entity, (made, key));
            }
        }
    }

    /// Brings an Obj's body to its collider and its Obj: made again when
    /// either changed, and moved where the game put the Obj, a kinematic
    /// body over the step and any other at once, as Unity teleports a
    /// Rigidbody whose Transform is set.
    fn sync_body(&mut self, world: &World, entity: Entity, collider: Collider, dt: f32) {
        let (scale, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
        // The scale is rounded, as one read back from a turned body's matrix is a hair off what was set,
        // and the body would be made again, losing its speed, each step it turns.
        let scale = (scale * 1e4).round() / 1e4;
        let (rigidbody, layer) = (world.get::<Rigidbody>(entity).copied(), layer(world, entity));
        let mesh = world.get::<ColliderMesh>(entity).filter(|_| collider.shape == ColliderShape::Mesh);
        let key = BodyKey { collider, rigidbody, scale, layer, mesh: mesh.map(|mesh| mesh.key) };
        let Some(kept) = self.bodies.get_mut(&entity).filter(|kept| kept.key == key) else {
            if self.bodies.contains_key(&entity) {
                self.doubt(entity);
            }
            self.remove(entity);
            if collider.shape == ColliderShape::Mesh && mesh.is_none() {
                return;
            }
            return self.add(entity, key, mesh.cloned(), position, rotation);
        };
        if same_place(kept.placed, (position, rotation)) {
            return;
        }
        kept.placed = (position, rotation);
        let body = kept.body;
        match key.rigidbody.is_some_and(|body| body.kinematic) && dt > 0.0 {
            true => self.world.move_kinematic(body, position, rotation, dt),
            false => self.world.set_transform(body, position, rotation),
        }
    }

    fn add(&mut self, entity: Entity, key: BodyKey, mesh: Option<ColliderMesh>, position: Vec3, rotation: Quat) {
        let motion = match key.rigidbody {
            None => Motion::Static,
            Some(body) if body.kinematic => Motion::Kinematic,
            Some(_) => Motion::Dynamic,
        };
        let Some(shape) = self.shape(&key.collider, key.scale, mesh.as_ref(), motion == Motion::Dynamic) else {
            return tracing::warn!("an Obj's mesh has no triangles to collide with; its collider is left out");
        };
        let mut desc = BodyDesc::new(shape, motion).at(position, rotation);
        desc.friction = key.collider.friction;
        desc.restitution = key.collider.bounciness;
        desc.sensor = key.collider.trigger;
        desc.user_data = entity.to_bits();
        desc.layer = key.layer;
        if let Some(body) = key.rigidbody {
            desc.mass = body.mass.max(1e-3);
            desc.linear_damping = body.drag;
            desc.angular_damping = body.angular_drag;
            desc.gravity_factor = if body.gravity { 1.0 } else { 0.0 };
            desc.continuous = body.continuous;
        }
        let Some(body) = self.world.add_body(&desc) else {
            return tracing::warn!("physics is full; an Obj's collider is left out");
        };
        self.owners.insert(body, entity);
        self.bodies.insert(entity, Kept { body, shape, key, placed: (position, rotation) });
    }

    fn remove(&mut self, entity: Entity) {
        if let Some(kept) = self.bodies.get(&entity) {
            // A joint holds its bodies, so it goes first; it is made again once its bodies are back.
            let body = kept.body;
            let holding: Vec<Entity> = (self.joints.iter())
                .filter(|(_, (_, key))| key.body == body || key.other == Some(body))
                .map(|(entity, _)| *entity)
                .collect();
            for holder in holding {
                if let Some((joint, _)) = self.joints.remove(&holder) {
                    self.world.remove_joint(joint);
                }
            }
        }
        if let Some(kept) = self.bodies.remove(&entity) {
            self.world.remove_body(kept.body);
            self.world.release_shape(kept.shape);
            self.owners.remove(&kept.body);
        }
    }

    /// A collider's shape at an Obj's scale. A sphere and a capsule take
    /// the scale across them, as round shapes cannot stretch one way.
    /// A collider's shape at its Obj's scale. A mesh's is its triangles,
    /// or their convex hull when it is convex or moved by physics; `None`
    /// for one with too few to make either.
    fn shape(&mut self, collider: &Collider, scale: Vec3, mesh: Option<&ColliderMesh>, moving: bool) -> Option<ShapeId> {
        if let (ColliderShape::Mesh, Some(mesh)) = (collider.shape, mesh) {
            // Mirrored, the points keep their side; a scale of 0 would flatten them to nothing.
            let scale = scale.signum() * scale.abs().max(Vec3::splat(1e-4));
            let points: Vec<Vec3> = mesh.points.iter().map(|point| *point * scale).collect();
            return match collider.convex || moving {
                true => self.world.convex_hull_shape(&points),
                false => self.world.mesh_shape(&points, &mesh.indices),
            };
        }
        let scale = scale.abs().max(Vec3::splat(1e-4));
        let across = scale.x.max(scale.z);
        let base = match collider.shape {
            ColliderShape::Box | ColliderShape::Mesh => self.world.box_shape(Vec3::from(collider.size) * scale * 0.5),
            ColliderShape::Sphere => self.world.sphere_shape(collider.radius * scale.max_element()),
            ColliderShape::Capsule => {
                let radius = collider.radius * across;
                let half = (collider.height * scale.y * 0.5 - radius).max(0.01);
                self.world.capsule_shape(half, radius)
            }
        };
        let center = Vec3::from(collider.center) * scale;
        if center == Vec3::ZERO {
            return Some(base);
        }
        let placed = self.world.placed_shape(base, center, Quat::IDENTITY, Vec3::ONE);
        self.world.release_shape(base);
        Some(placed)
    }

    /// Dynamic bodies' Objs move to where physics put them.
    fn write_back(&mut self, world: &mut World) {
        for (entity, kept) in &mut self.bodies {
            if !kept.key.rigidbody.is_some_and(|body| !body.kinematic) || !self.world.is_active(kept.body) {
                continue;
            }
            let (position, rotation) = self.world.transform(kept.body);
            kept.placed = (position, rotation);
            let parent = world.get::<Parent>(*entity).map(|parent| voxear_scene::world_matrix(world, parent.0));
            let Some(transform) = world.get_mut::<Transform>(*entity) else { continue };
            match parent {
                None => (transform.position, transform.rotation) = (position, rotation),
                Some(parent) => {
                    let local = parent.inverse() * Mat4::from_rotation_translation(rotation, position);
                    let (_, rotation, position) = local.to_scale_rotation_translation();
                    (transform.position, transform.rotation) = (position, rotation);
                }
            }
        }
    }

    /// The step's contacts, as the pairs that began and stopped touching:
    /// once for a pair touching in several places, and not when Jolt lets
    /// go of a sleeping body's contacts, as Unity's stay touching till they
    /// part.
    fn note_contacts(&mut self) {
        let mut told = HashSet::new();
        for contact in self.world.take_contacts() {
            let (Some(&a), Some(&b)) = (self.owners.get(&contact.a), self.owners.get(&contact.b)) else { continue };
            let both = pair(a, b);
            told.insert(both);
            if contact.began {
                self.unsure.remove(&both);
                if self.touching.insert(both) {
                    self.tell(true, a, b, contact.point, contact.normal);
                }
            } else if self.asleep(a) || self.asleep(b) {
                if self.touching.contains(&both) {
                    self.unsure.insert(both, 0);
                }
            } else if self.touching.remove(&both) {
                self.unsure.remove(&both);
                self.tell(false, a, b, Vec3::ZERO, Vec3::ZERO);
            }
        }
        // Once both are awake, a pair not told of again has parted.
        let waiting: Vec<(Entity, Entity)> = self.unsure.keys().copied().collect();
        for (a, b) in waiting {
            if told.contains(&(a, b)) || self.asleep(a) || self.asleep(b) {
                continue;
            }
            let steps = self.unsure.entry((a, b)).or_default();
            *steps += 1;
            if *steps >= UNSURE_STEPS {
                self.unsure.remove(&(a, b));
                if self.touching.remove(&(a, b)) {
                    self.tell(false, a, b, Vec3::ZERO, Vec3::ZERO);
                }
            }
        }
    }

    /// Lets go of what a gone Obj touched, unless it still has physics:
    /// Unity tells nothing of an Obj's contacts ending as it goes.
    fn forget(&mut self, entity: Entity) {
        if self.bodies.contains_key(&entity) || self.characters.contains_key(&entity) {
            return;
        }
        self.touching.retain(|(a, b)| *a != entity && *b != entity);
        self.unsure.retain(|(a, b), _| *a != entity && *b != entity);
    }

    /// Keeps what an Obj made again touched until Jolt tells of it again.
    fn doubt(&mut self, entity: Entity) {
        let pairs = self.touching.iter().filter(|(a, b)| *a == entity || *b == entity);
        self.unsure.extend(pairs.map(|pair| (*pair, 0)));
    }

    fn remove_character(&mut self, entity: Entity) {
        let Some(walker) = self.characters.remove(&entity) else { return };
        if let Some(body) = self.world.character_body(walker.id) {
            self.owners.remove(&body);
        }
        self.world.remove_character(walker.id);
    }

    /// Whether an Obj's body is one physics moves that has gone to sleep.
    fn asleep(&self, entity: Entity) -> bool {
        self.bodies.get(&entity).is_some_and(|kept| kept.key.rigidbody.is_some() && !self.world.is_active(kept.body))
    }

    fn tell(&mut self, began: bool, a: Entity, b: Entity, point: Vec3, normal: Vec3) {
        let trigger = [a, b].iter().any(|e| self.bodies.get(e).is_some_and(|kept| kept.key.collider.trigger));
        self.events.push(PhysicsEvent { began, trigger, a, b, point, normal });
    }

    fn sync_characters(&mut self, world: &World) {
        let wanted: Vec<(Entity, CharacterController)> = world
            .query_ref::<(Entity, &CharacterController)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, controller)| (entity, *controller))
            .collect();
        let gone: Vec<Entity> =
            self.characters.keys().copied().filter(|entity| !wanted.iter().any(|(e, _)| e == entity)).collect();
        for entity in gone {
            self.remove_character(entity);
            self.forget(entity);
        }
        for (entity, controller) in wanted {
            self.sync_character(world, entity, controller);
        }
    }

    /// Brings an Obj's character to its Character Controller, made again
    /// when that or its layer changed, and to where the game put the Obj.
    fn sync_character(&mut self, world: &World, entity: Entity, controller: CharacterController) {
        let position = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        let layer = layer(world, entity);
        match self.characters.get_mut(&entity) {
            Some(walker) if walker.controller == controller && walker.layer == layer => {
                if !walker.placed.abs_diff_eq(position, 1e-4) {
                    walker.placed = position;
                    let id = walker.id;
                    self.world.set_character_position(id, position);
                }
                return;
            }
            Some(_) => self.doubt(entity),
            None => {}
        }
        self.remove_character(entity);
        let desc = CharacterDesc {
            position,
            height: controller.height,
            radius: controller.radius,
            max_slope: controller.slope_limit,
            step_height: controller.step_offset,
            layer,
            ..CharacterDesc::default()
        };
        let character = self.world.add_character(&desc);
        if let Some(body) = self.world.character_body(character) {
            self.owners.insert(body, entity);
        }
        self.characters.insert(entity, Walker { id: character, controller, layer, placed: position, state: None });
    }

    /// Moves an Obj's Character Controller at `velocity` for `dt` seconds,
    /// at once, as Unity's CharacterController.Move does, and its Obj with
    /// it. Gravity is the caller's to add. `None` for an Obj without one.
    pub fn move_character(
        &mut self,
        world: &mut World,
        entity: Entity,
        velocity: Vec3,
        dt: f32,
    ) -> Option<CharacterState> {
        let controller = *world.get::<CharacterController>(entity)?;
        self.sync_character(world, entity, controller);
        let walker = self.characters.get_mut(&entity)?;
        let state = self.world.move_character(walker.id, velocity, dt, walker.controller.step_offset);
        walker.placed = state.position;
        walker.state = Some(state);
        let parent = world.get::<Parent>(entity).map(|parent| voxear_scene::world_matrix(world, parent.0));
        let local = parent.map_or(state.position, |parent| parent.inverse().transform_point3(state.position));
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.position = local;
        }
        Some(state)
    }
}

fn unit() -> [f32; 3] {
    [1.0; 3]
}

fn is_unit(value: &[f32; 3]) -> bool {
    *value == [1.0; 3]
}

fn is_zero(value: &[f32; 3]) -> bool {
    *value == [0.0; 3]
}

fn half() -> f32 {
    0.5
}

fn is_half(value: &f32) -> bool {
    *value == 0.5
}

fn two() -> f32 {
    2.0
}

fn is_two(value: &f32) -> bool {
    *value == 2.0
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

fn grip() -> f32 {
    0.6
}

fn is_grip(value: &f32) -> bool {
    *value == 0.6
}

fn turn_drag() -> f32 {
    0.05
}

fn is_turn_drag(value: &f32) -> bool {
    *value == 0.05
}

fn is_zero_f32(value: &f32) -> bool {
    *value == 0.0
}

fn is_generic(acoustic: &Acoustic) -> bool {
    *acoustic == Acoustic::Generic
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn yes() -> bool {
    true
}

fn is_yes(value: &bool) -> bool {
    *value
}

fn body_height() -> f32 {
    1.8
}

fn is_body_height(value: &f32) -> bool {
    *value == 1.8
}

fn body_radius() -> f32 {
    0.3
}

fn is_body_radius(value: &f32) -> bool {
    *value == 0.3
}

fn slope() -> f32 {
    45.0
}

fn is_slope(value: &f32) -> bool {
    *value == 45.0
}

fn step() -> f32 {
    0.3
}

fn is_step(value: &f32) -> bool {
    *value == 0.3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rigidbody_falls_onto_a_collider_and_its_transform_follows() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = world.spawn((Transform::from_position(Vec3::new(0.0, -0.5, 0.0)),));
        world.insert(ground, Collider { size: [20.0, 1.0, 20.0], ..Collider::default() });
        let falling = world.spawn((Transform::from_position(Vec3::new(0.0, 4.0, 0.0)),));
        world.insert(falling, Collider::default());
        world.insert(falling, Rigidbody::default());
        let mut physics = PhysicsScene::new();
        let mut events = Vec::new();
        for _ in 0..180 {
            physics.step(&mut world, 1.0 / 60.0);
            events.extend(physics.take_events());
        }
        let y = world.get::<Transform>(falling).unwrap().position.y;
        assert!((y - 0.5).abs() < 0.05, "it rests on the ground: {y}");
        assert!(events.iter().any(|e| e.began && !e.trigger && [e.a, e.b].contains(&ground)), "{events:?}");

        // Put somewhere by the game, a dynamic body goes there, as Unity's does.
        world.get_mut::<Transform>(falling).unwrap().position = Vec3::new(3.0, 6.0, 0.0);
        physics.step(&mut world, 1.0 / 60.0);
        let moved = world.get::<Transform>(falling).unwrap().position;
        assert!(moved.x == 3.0 && moved.y > 5.9 && moved.y < 6.0, "falling again from there: {moved}");

        // Taken out, its body goes; the ground's stays, unmoved by the game.
        world.remove::<Collider>(falling);
        physics.step(&mut world, 1.0 / 60.0);
        assert!(physics.body(falling).is_none() && physics.body(ground).is_some());
    }

    #[test]
    fn a_character_controller_walks_its_obj() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = world.spawn((Transform::from_position(Vec3::new(0.0, -0.5, 0.0)),));
        world.insert(ground, Collider { size: [20.0, 1.0, 20.0], ..Collider::default() });
        let walker = world.spawn((Transform::from_position(Vec3::new(0.0, 0.05, 0.0)),));
        world.insert(walker, CharacterController::default());
        let mut physics = PhysicsScene::new();
        physics.step(&mut world, 1.0 / 60.0);
        let mut state = None;
        for _ in 0..60 {
            state = physics.move_character(&mut world, walker, Vec3::new(0.0, -2.0, 2.0), 1.0 / 60.0);
        }
        let z = world.get::<Transform>(walker).unwrap().position.z;
        assert!((z - 2.0).abs() < 0.1, "two metres on in a second: {z}");
        assert_eq!(state.map(|s| s.ground), Some(crate::Ground::Ground));
    }

    #[test]
    fn joints_hold_a_door_to_the_world_and_a_weight_on_a_rope() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        // A door a metre wide, its hinge up its left edge, pushed round.
        let door = world.spawn((Transform::from_position(Vec3::new(0.5, 1.0, 0.0)),));
        world.insert(door, Collider { size: [1.0, 2.0, 0.1], ..Collider::default() });
        world.insert(door, Rigidbody { gravity: false, ..Rigidbody::default() });
        let hinge =
            Joint { kind: JointType::Hinge, anchor: [-0.5, 0.0, 0.0], limits: Some([0.0, 90.0]), ..Joint::default() };
        world.insert(door, hinge);
        // A weight hanging two metres under a point it swings from.
        let weight = world.spawn((Transform::from_position(Vec3::new(1.0, 3.0, 5.0)),));
        world.insert(weight, Collider { shape: ColliderShape::Sphere, radius: 0.2, ..Collider::default() });
        world.insert(weight, Rigidbody::default());
        world.insert(weight, Joint { kind: JointType::Distance, anchor: [-2.0, 0.0, 0.0], ..Joint::default() });
        let mut physics = PhysicsScene::new();
        physics.step(&mut world, 1.0 / 60.0);
        let body = physics.body(door).unwrap();
        physics.world_mut().set_velocity(body, Vec3::ZERO, Vec3::new(0.0, 6.0, 0.0));
        let mut lowest = f32::MAX;
        for step in 0..120 {
            physics.step(&mut world, 1.0 / 60.0);
            assert_eq!(physics.body(door), Some(body), "the same body as it turns, step {step}");
            lowest = lowest.min(world.get::<Transform>(weight).unwrap().position.y);
        }
        let at = world.get::<Transform>(door).unwrap().position;
        let edge = Vec3::new(0.0, 1.0, 0.0);
        assert!((at.distance(edge) - 0.5).abs() < 0.02, "still on its hinge: {at}");
        // Turned a quarter about +Y, its middle is half a metre along -Z of its hinge.
        assert!(at.z < -0.45 && at.x.abs() < 0.05, "swung open to its limit, and no further: {at}");
        let hung = world.get::<Transform>(weight).unwrap().position;
        assert!((hung.distance(Vec3::new(-1.0, 3.0, 5.0)) - 2.0).abs() < 0.05, "on its rope: {hung}");
        assert!((lowest - 1.0).abs() < 0.05, "swung through the bottom, two metres under: {lowest}");

        // Taken off, the door is free; a body made again takes its joint again.
        world.remove::<Joint>(door);
        physics.step(&mut world, 1.0 / 60.0);
        assert!(!physics.joints.contains_key(&door));
        world.get_mut::<Collider>(weight).unwrap().radius = 0.3;
        physics.step(&mut world, 1.0 / 60.0);
        assert_eq!(physics.joints.get(&weight).map(|(_, key)| key.body), physics.body(weight));
    }

    #[test]
    fn a_character_walking_through_a_trigger_is_noticed() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = world.spawn((Transform::from_position(Vec3::new(0.0, -0.5, 0.0)),));
        world.insert(ground, Collider { size: [20.0, 1.0, 20.0], ..Collider::default() });
        let gate = world.spawn((Transform::from_position(Vec3::new(0.0, 1.0, 2.0)),));
        world.insert(gate, Collider { size: [4.0, 2.0, 0.5], trigger: true, ..Collider::default() });
        let walker = world.spawn((Transform::from_position(Vec3::new(0.0, 0.05, 0.0)),));
        world.insert(walker, CharacterController::default());
        let mut physics = PhysicsScene::new();
        let mut events = Vec::new();
        let mut inside = false;
        for _ in 0..120 {
            physics.move_character(&mut world, walker, Vec3::new(0.0, -2.0, 2.0), 1.0 / 60.0);
            physics.step(&mut world, 1.0 / 60.0);
            events.extend(physics.take_events());
            inside |= physics.touching(gate, walker);
        }
        let crossed =
            |began| events.iter().filter(|e| e.trigger && e.began == began && [e.a, e.b].contains(&walker)).count();
        assert_eq!((crossed(true), crossed(false)), (1, 1), "in and out of the gate: {events:?}");
        assert!(inside && !physics.touching(gate, walker));
        assert!(!events.iter().any(|e| [e.a, e.b] == [ground, walker] || [e.a, e.b] == [walker, ground]));

        // Gone, the walker's body goes with it, and nothing is told.
        world.remove::<CharacterController>(walker);
        physics.step(&mut world, 1.0 / 60.0);
        assert!(physics.take_events().is_empty() && physics.owners.values().all(|e| *e != walker));
    }

    #[test]
    fn a_mesh_collider_is_its_meshs_triangles_or_their_hull() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let mesh = |key: u64, points: &[[f32; 3]], indices: &[u32]| ColliderMesh {
            key,
            points: points.iter().map(|p| Vec3::from(*p)).collect(),
            indices: indices.into(),
        };
        // A floor of two triangles, ten metres across before its Obj's scale of two.
        let floor = mesh(1, &[[-5.0, 0.0, -5.0], [5.0, 0.0, -5.0], [5.0, 0.0, 5.0], [-5.0, 0.0, 5.0]], &[0, 2, 1, 0, 3, 2]);
        let mut scaled = Transform::default();
        scaled.scale = Vec3::splat(2.0);
        let ground = world.spawn((scaled,));
        world.insert(ground, Collider { shape: ColliderShape::Mesh, ..Collider::default() });
        let corners: Vec<[f32; 3]> =
            (0..8).map(|i| [[-0.5, 0.5][i & 1], [-0.5, 0.5][i >> 1 & 1], [-0.5, 0.5][i >> 2 & 1]]).collect();
        let falling = world.spawn((Transform::from_position(Vec3::new(8.0, 3.0, 0.0)),));
        world.insert(falling, Collider { shape: ColliderShape::Mesh, ..Collider::default() });
        world.insert(falling, Rigidbody::default());
        world.insert(falling, mesh(2, &corners, &[]));
        let mut physics = PhysicsScene::new();
        physics.step(&mut world, 1.0 / 60.0);
        assert!(physics.body(ground).is_none(), "no body until its mesh is given");
        world.insert(ground, floor);
        for _ in 0..120 {
            physics.step(&mut world, 1.0 / 60.0);
        }
        let y = world.get::<Transform>(falling).unwrap().position.y;
        assert!((y - 0.5).abs() < 0.05, "a moving one is its hull, resting on the floor's triangles: {y}");
        let hit = physics.raycast(Vec3::new(9.5, 5.0, 9.5), Vec3::NEG_Y, 10.0, u32::MAX, false).unwrap();
        assert_eq!(hit.obj, ground, "the floor reaches as far as its Obj's scale takes it");
        assert!(physics.raycast(Vec3::new(10.5, 5.0, 0.0), Vec3::NEG_Y, 10.0, u32::MAX, false).is_none());
    }

    #[test]
    fn objs_on_layers_kept_apart_pass_through_and_rays_ask_after_some_layers() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let mut layers = Layers::named(&["Default".into(), "Ground".into(), "Ghost".into()]);
        layers.set_collide(1, 2, false);
        world.insert_resource(layers);
        let ground = world.spawn((Transform::from_position(Vec3::new(0.0, -0.5, 0.0)), Layer(1)));
        world.insert(ground, Collider { size: [20.0, 1.0, 20.0], ..Collider::default() });
        let falling = |world: &mut World, x: f32, layer: u8| {
            let obj = world.spawn((Transform::from_position(Vec3::new(x, 4.0, 0.0)), Layer(layer)));
            world.insert(obj, Collider::default());
            world.insert(obj, Rigidbody::default());
            obj
        };
        let (solid, ghost) = (falling(&mut world, 0.0, 0), falling(&mut world, 3.0, 2));
        let mut physics = PhysicsScene::new();
        for _ in 0..90 {
            physics.step(&mut world, 1.0 / 60.0);
        }
        let y = |world: &World, obj: Entity| world.get::<Transform>(obj).unwrap().position.y;
        assert!((y(&world, solid) - 0.5).abs() < 0.05 && y(&world, ghost) < -2.0);

        let down = |x: f32, mask: u32| physics.raycast(Vec3::new(x, 10.0, 0.0), Vec3::NEG_Y, 30.0, mask, false);
        assert_eq!(down(0.0, u32::MAX).map(|hit| hit.obj), Some(solid));
        assert_eq!(down(0.0, 1 << 1).map(|hit| hit.obj), Some(ground), "past the Obj on Default");
        assert_eq!(physics.overlap_sphere(Vec3::new(0.0, 0.0, 0.0), 0.8, 1 << 1, false), [ground]);

        // Moved onto Default, the ghost's body is made again and lands on what is under it.
        world.get_mut::<Transform>(ghost).unwrap().position = Vec3::new(3.0, 4.0, 0.0);
        world.insert(ghost, Layer(0));
        for _ in 0..90 {
            physics.step(&mut world, 1.0 / 60.0);
        }
        assert!((y(&world, ghost) - 0.5).abs() < 0.05, "{}", y(&world, ghost));
    }
}
