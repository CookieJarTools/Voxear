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
//!
//! A **Vehicle** beside a Rigidbody drives it on the **Wheel**s of its
//! child Objs (see `vehicles`): a car's, a tank's tracks, or a motorbike's.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Layer, Layers, Parent, Plant, PlantSet, Plants, Transform, Trunk, WorldTransform};

mod vehicles;

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

/// Floats its Obj on the sea, Pools, Streams and water blocks. It needs a
/// Rigidbody and a Collider beside it; how much it weighs does not change
/// how it floats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Buoyancy {
    /// How much of it sits under still water, from 0 to 1: a boat's half,
    /// a log's more. Over 1, it sinks.
    pub submerged: f32,
    /// How quickly the water slows it, moving and turning, against the way
    /// the water moves, so a Stream carries it along and it rides the
    /// waves: 1 stops it bobbing and rocking without its overshooting, less
    /// lets it bob longer.
    pub drag: f32,
    pub angular_drag: f32,
}

impl Component for Buoyancy {}

impl Default for Buoyancy {
    fn default() -> Self {
        Self { submerged: 0.5, drag: 0.4, angular_drag: 0.4 }
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

/// What drives a Vehicle: wheels, a tank's tracks, or two wheels that lean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VehicleKind {
    #[default]
    Car,
    /// Turns by running its tracks at different speeds, and on the spot by
    /// running them opposite ways.
    Tank,
    Motorbike,
}

impl VehicleKind {
    pub const ALL: [Self; 3] = [Self::Car, Self::Tank, Self::Motorbike];

    pub fn label(self) -> &'static str {
        match self {
            Self::Car => "Car",
            Self::Tank => "Tank",
            Self::Motorbike => "Motorbike",
        }
    }
}

/// Drives its Obj's Rigidbody on the Wheels of its child Objs, with an
/// engine, a gearbox and differentials, as a car, a tank or a motorbike.
/// Its Obj faces its +Z. A script drives it by `throttle` (-1 reverse to
/// 1), `steering` (-1 left to 1 right), `brake` and `handbrake` (0 to 1),
/// and reads its `speed` (metres a second, forward), `rpm` and `gear`
/// (-1 reverse, 0 neutral, 1 first), none of which scene files keep.
///
/// Its gearbox has `gears` forward gears, from `first_gear`'s ratio down to
/// `top_gear`'s, each the same step from the one before; `anti_roll` is the
/// stiffness of a bar across each pair of wheels side by side, in N/m, to
/// keep it from leaning out of turns (0 for none).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vehicle {
    #[serde(default, skip_serializing_if = "is_car")]
    pub kind: VehicleKind,
    /// The engine's most torque, in Nm, and the range it revs through.
    #[serde(default = "torque", skip_serializing_if = "is_torque")]
    pub max_torque: f32,
    #[serde(default = "min_rpm", skip_serializing_if = "is_min_rpm")]
    pub min_rpm: f32,
    #[serde(default = "max_rpm", skip_serializing_if = "is_max_rpm")]
    pub max_rpm: f32,
    /// Its gears change by themselves.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub automatic: bool,
    #[serde(default = "gears", skip_serializing_if = "is_gears")]
    pub gears: u32,
    #[serde(default = "first_gear", skip_serializing_if = "is_first_gear")]
    pub first_gear: f32,
    #[serde(default = "top_gear", skip_serializing_if = "is_top_gear")]
    pub top_gear: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub anti_roll: f32,
    #[serde(skip)]
    pub throttle: f32,
    #[serde(skip)]
    pub steering: f32,
    #[serde(skip)]
    pub brake: f32,
    #[serde(skip)]
    pub handbrake: f32,
    #[serde(skip)]
    pub speed: f32,
    #[serde(skip)]
    pub rpm: f32,
    #[serde(skip)]
    pub gear: i32,
}

impl Default for Vehicle {
    fn default() -> Self {
        Self {
            kind: VehicleKind::Car,
            max_torque: torque(),
            min_rpm: min_rpm(),
            max_rpm: max_rpm(),
            automatic: true,
            gears: gears(),
            first_gear: first_gear(),
            top_gear: top_gear(),
            anti_roll: 0.0,
            throttle: 0.0,
            steering: 0.0,
            brake: 0.0,
            handbrake: 0.0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
        }
    }
}

impl Vehicle {
    /// What it is made from, without what drives it or how it goes.
    fn settings(&self) -> Self {
        Self {
            kind: self.kind,
            max_torque: self.max_torque,
            min_rpm: self.min_rpm,
            max_rpm: self.max_rpm,
            automatic: self.automatic,
            gears: self.gears,
            first_gear: self.first_gear,
            top_gear: self.top_gear,
            anti_roll: self.anti_roll,
            ..Self::default()
        }
    }
}

impl Component for Vehicle {}

/// One of a Vehicle's wheels, on a child Obj of the Vehicle's, placed where
/// the wheel's middle is at rest, as Unity's Wheel Collider is: its radius
/// and width in metres, how far its suspension travels, its spring's
/// frequency (Hz) and damping, how far it steers (degrees; 0 for a wheel
/// that does not), whether the engine drives it, and its brake's and hand
/// brake's torque in Nm; and how hard its tyre grips, forwards and
/// sideways, as a share of an ordinary tyre's (2 holds twice as hard, as a
/// racing tyre; 0.3 slides, as on ice). While the game plays, physics turns
/// and drops its Obj as the wheel rolls, steers and rides its suspension.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wheel {
    #[serde(default = "wheel_radius", skip_serializing_if = "is_wheel_radius")]
    pub radius: f32,
    #[serde(default = "wheel_width", skip_serializing_if = "is_wheel_width")]
    pub width: f32,
    #[serde(default = "travel", skip_serializing_if = "is_travel")]
    pub suspension: f32,
    #[serde(default = "frequency", skip_serializing_if = "is_frequency")]
    pub spring: f32,
    #[serde(default = "half", skip_serializing_if = "is_half")]
    pub damping: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub steer_angle: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub powered: bool,
    #[serde(default = "brake", skip_serializing_if = "is_brake")]
    pub brake: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub handbrake: f32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub grip: f32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub side_grip: f32,
}

impl Default for Wheel {
    fn default() -> Self {
        Self {
            radius: wheel_radius(),
            width: wheel_width(),
            suspension: travel(),
            spring: frequency(),
            damping: half(),
            steer_angle: 0.0,
            powered: false,
            brake: brake(),
            handbrake: 0.0,
            grip: 1.0,
            side_grip: 1.0,
        }
    }
}

impl Component for Wheel {}

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
/// Obj was last, how its last move went, and which way it stands: its up,
/// and its capsule's turn from standing on +Y.
struct Walker {
    id: CharacterId,
    controller: CharacterController,
    layer: u8,
    placed: Vec3,
    state: Option<CharacterState>,
    up: Vec3,
    turn: Quat,
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
    /// The plant whose trunk it met, by its number in the Obj's plants.
    pub plant: Option<u32>,
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

/// How many of a terrain without end's tiles get their bodies a step, at
/// most, as its window moves: each takes a few milliseconds to make.
const ENDLESS_TILES_A_STEP: usize = 4;

/// The Jolt world for the Objs with colliders, while the game plays.
pub struct PhysicsScene {
    world: PhysicsWorld,
    bodies: HashMap<Entity, Kept>,
    owners: HashMap<BodyId, Entity>,
    characters: HashMap<Entity, Walker>,
    joints: HashMap<Entity, (JointId, JointKey)>,
    vehicles: HashMap<Entity, vehicles::Ride>,
    /// Where each Wheel's Obj was when first seen, before physics moved it.
    wheels_at_rest: HashMap<Entity, Vec3>,
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
    /// Each terrain's bodies, a height field for each of its tiles.
    terrains: HashMap<Entity, TerrainBodies>,
    /// Each world of blocks' bodies, a mesh for each of its chunks with faces.
    block_worlds: HashMap<Entity, BlockBodies>,
    plants: HashMap<Entity, PlantBodies>,
    /// Each plant trunk's body's plant, by its number in its Obj's plants.
    plant_bodies: HashMap<BodyId, u32>,
}

/// The most an Obj far from the middle can be from its body after the
/// world's origin moves, by rounding alone, in metres.
const ROUNDING: f32 = 0.01;

/// Which way and how hard things fall at a point, in metres per second per
/// second, in a world whose gravity is `gravity`: the same everywhere.
pub fn gravity_at(_world: &World, gravity: Vec3, _at: Vec3) -> Vec3 {
    gravity
}

/// A terrain's bodies: which heights they were made from, where its Obj
/// was, its layer, and for each tile's slot (see
/// `voxear_terrain::endless::slot_of`) the tile it was made from, its body
/// and its shape; and for each dug chunk the chunk it was made from and,
/// where it has ground in it, its mesh's body and shape.
struct TerrainBodies {
    heights: u64,
    corner: Vec3,
    layer: u8,
    tiles: Vec<Option<(Arc<voxear_terrain::HeightTile>, BodyId, ShapeId)>>,
    dug: HashMap<voxear_terrain::dig::ChunkKey, DugBody>,
}

/// A world of blocks' bodies: where its Obj was, its layer, how large a
/// block was, its chunks' faces as they were made from, and for each chunk
/// the faces it was made from, its body and its shape.
struct BlockBodies {
    corner: Vec3,
    layer: u8,
    block_size: f32,
    meshes: Option<Arc<voxear_terrain::blocks::ChunkMeshes>>,
    chunks: HashMap<[i32; 3], (Arc<voxear_terrain::blocks::mesh::ChunkMesh>, BodyId, ShapeId)>,
}

/// A dug chunk's body: the chunk it was made from, and its body and shape
/// where it has ground in it.
type DugBody = (Arc<voxear_terrain::dig::DugChunk>, Option<(BodyId, ShapeId)>);

/// An Obj's plants' trunks: what they were made from (the plants, where the
/// Obj was, the trunk's shape, its layer, the plants taken away), and each
/// plant's body and shape while it stands.
struct PlantBodies {
    plants: Arc<Vec<Plant>>,
    model: Mat4,
    trunk: Trunk,
    layer: u8,
    removed: Arc<PlantSet>,
    bodies: Vec<Option<(BodyId, ShapeId)>>,
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
            vehicles: HashMap::new(),
            wheels_at_rest: HashMap::new(),
            events: Vec::new(),
            gravity: crate::EARTH_GRAVITY,
            collide: [u32::MAX; 32],
            stepped: false,
            touching: HashSet::new(),
            unsure: HashMap::new(),
            terrains: HashMap::new(),
            block_worlds: HashMap::new(),
            plants: HashMap::new(),
            plant_bodies: HashMap::new(),
        }
    }

    /// Whether a world has anything for physics to do: an Obj with a
    /// Collider, a Character Controller or a Terrain.
    pub fn wanted(world: &World) -> bool {
        world.query_ref::<&Collider>().into_iter().next().is_some()
            || world.query_ref::<&CharacterController>().into_iter().next().is_some()
            || world.query_ref::<&voxear_terrain::Terrain>().into_iter().next().is_some()
            || world.query_ref::<&voxear_terrain::blocks::BlockWorld>().into_iter().next().is_some()
            || world.query_ref::<&Plants>().into_iter().any(|plants| plants.trunk.is_some())
    }

    /// Which way and how hard everything falls, in metres per second per second.
    pub fn gravity(&self) -> Vec3 {
        self.gravity
    }

    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.gravity = gravity;
        self.world.set_gravity(gravity);
    }

    /// Which way and how hard things fall at a point: the world's gravity
    /// (see [`gravity_at`]).
    pub fn gravity_at(&self, world: &World, at: Vec3) -> Vec3 {
        gravity_at(world, self.gravity, at)
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
        let plant = self.plant_bodies.get(&hit.body).copied();
        Some(SceneHit { obj, point: hit.point, normal: hit.normal, distance: hit.fraction * reach, plant })
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

    /// Which way an Obj's Character Controller stands: against what pulls
    /// it where it is (see [`gravity_at`]).
    pub fn character_up(&self, entity: Entity) -> Option<Vec3> {
        Some(self.characters.get(&entity)?.up)
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
        self.write_back_vehicles(world);
        self.note_contacts();
    }

    /// Moves every body, character and joint by `by`, after the world's
    /// origin has moved every Obj by as much (see
    /// `voxear_scene::move_objs`): none wakes, each keeps how fast it goes,
    /// and nothing is made again. An Obj far from the middle may have been
    /// put back where it was exactly, a rounding step from where its body
    /// went; that is taken as its body's place, and a still body is put there
    /// too.
    pub fn shift(&mut self, world: &World, by: Vec3) {
        self.world.shift(by);
        for (entity, kept) in &mut self.bodies {
            kept.placed.0 += by;
            let (_, rotation, position) = voxear_scene::world_matrix(world, *entity).to_scale_rotation_translation();
            if same_place(kept.placed, (position, rotation)) || position.distance(kept.placed.0) > ROUNDING {
                continue;
            }
            if kept.key.rigidbody.is_none() {
                self.world.set_transform(kept.body, position, rotation);
            }
            kept.placed = (position, rotation);
        }
        for walker in self.characters.values_mut() {
            walker.placed += by;
            if let Some(state) = walker.state.as_mut() {
                state.position += by;
            }
        }
        for kept in self.terrains.values_mut() {
            kept.corner += by;
        }
        for kept in self.block_worlds.values_mut() {
            kept.corner += by;
        }
        for (entity, kept) in &mut self.plants {
            kept.model.w_axis += by.extend(0.0);
            // As the Obj's transforms will be worked out at the end of the frame.
            let now = voxear_scene::world_matrix(world, *entity);
            let model = kept.model;
            let turned = [now.x_axis, now.y_axis, now.z_axis] == [model.x_axis, model.y_axis, model.z_axis];
            if turned && now.w_axis.distance(kept.model.w_axis) < ROUNDING {
                kept.model = now;
            }
        }
    }

    fn sync(&mut self, world: &World, dt: f32) {
        let collide = world.resource::<Layers>().map_or([u32::MAX; 32], Layers::matrix);
        if collide != self.collide {
            self.collide = collide;
            self.world.set_layers(&collide);
        }
        // A Collider switched off, or on an Obj that is, leaves physics, its body with it, until it is back.
        let switched = voxear_scene::Switched::of(world);
        let colliders: Vec<(Entity, Collider)> = world
            .query_ref::<(Entity, &Collider)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(entity, _)| switched.is_on(world, *entity, voxear_scene::Switch::Collider))
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
        self.sync_terrains(world);
        self.sync_block_worlds(world);
        self.sync_plants(world);
        self.sync_characters(world);
        self.sync_joints(world);
        self.sync_vehicles(world);
    }

    /// Gives each terrain a static height field for each of its tiles, made
    /// again for a tile whose heights changed, and moved with its Obj. A
    /// terrain without end's tiles get theirs a few a step, those nearest the
    /// middle of its window, where the camera is, first; its tiles not made
    /// yet get none.
    fn sync_terrains(&mut self, world: &World) {
        let found = voxear_terrain::terrains(world);
        let gone: Vec<Entity> =
            self.terrains.keys().copied().filter(|entity| !found.iter().any(|(e, ..)| e == entity)).collect();
        for entity in gone {
            self.remove_terrain(entity);
        }
        for (entity, terrain, corner) in found {
            let map = &terrain.heights;
            let layer = layer(world, entity);
            let fits = self.terrains.get(&entity).is_some_and(|kept| {
                kept.heights == map.id() && kept.tiles.len() == map.tiles().len() && kept.layer == layer
            });
            if !fits {
                self.remove_terrain(entity);
                // Seen for the first time, all its dug ground is made at once, so nothing falls through it.
                if let Some(dug) = &terrain.dug {
                    dug.mesh_all(map);
                }
                let tiles = vec![None; map.tiles().len()];
                let dug = HashMap::new();
                let corner = corner - terrain.offset();
                self.terrains.insert(entity, TerrainBodies { heights: map.id(), corner, layer, tiles, dug });
            }
            let obj = corner - terrain.offset();
            let moved = self.terrains[&entity].corner != obj;
            let width = voxear_terrain::TILE_QUADS as f32 * map.spacing();
            let mut order: Vec<usize> = (0..map.tiles().len()).collect();
            let mut left = usize::MAX;
            if terrain.origin.is_some() {
                let middle = |count: u32| (count / 2) as i64;
                let (mx, mz) = (middle(map.tiles_x()), middle(map.tiles_z()));
                order.sort_by_key(|&index| {
                    let (tx, tz) = ((index as u32 % map.tiles_x()) as i64, (index as u32 / map.tiles_x()) as i64);
                    (tx - mx).pow(2) + (tz - mz).pow(2)
                });
                left = ENDLESS_TILES_A_STEP;
            }
            for index in order {
                let tile = &map.tiles()[index];
                let slot = voxear_terrain::endless::slot_of(map, terrain.origin, index);
                let (tx, tz) = (index as u32 % map.tiles_x(), index as u32 / map.tiles_x());
                let at = corner + Vec3::new(tx as f32 * width, 0.0, tz as f32 * width);
                let kept = self.terrains[&entity].tiles[slot].clone();
                // A tile holed for ground dug in it stays as it was until that ground's triangles are made.
                let waits = terrain.dug.as_ref().is_some_and(|dug| !dug.tile_meshed(tx, tz));
                if let Some((made, body, _)) = &kept
                    && (Arc::ptr_eq(made, tile) || waits)
                {
                    if moved {
                        self.world.set_transform(*body, at, Quat::IDENTITY);
                    }
                    continue;
                }
                let unmade = voxear_terrain::endless::is_unmade(tile);
                // Out of turns this step, the slot keeps the tile it had: one just gone from the window, still where
                // it was.
                if left == 0 && !unmade {
                    continue;
                }
                left = left.saturating_sub(usize::from(!unmade));
                if let Some((_, body, shape)) = kept {
                    self.world.remove_body(body);
                    self.world.release_shape(shape);
                    self.owners.remove(&body);
                    self.terrains.get_mut(&entity).expect("kept").tiles[slot] = None;
                }
                if unmade {
                    continue;
                }
                // A hole's samples have no ground, which cuts away every triangle touching them, as it is drawn.
                let holed: Option<Vec<f32>> = tile.holes().map(|_| {
                    let side = voxear_terrain::TILE_SAMPLES;
                    (0..side * side).map(|i| if tile.is_hole(i % side, i / side) { f32::MAX } else { tile.samples()[i as usize] }).collect()
                });
                let samples = holed.as_deref().unwrap_or(tile.samples());
                let made = self.world.heightfield_shape(samples, voxear_terrain::TILE_SAMPLES, map.spacing());
                let Some(shape) = made else {
                    self.terrains.get_mut(&entity).expect("kept").tiles[slot] = None;
                    continue;
                };
                let mut desc = BodyDesc::new(shape, Motion::Static).at(at, Quat::IDENTITY);
                desc.friction = Collider::default().friction;
                desc.user_data = entity.to_bits();
                desc.layer = layer;
                let Some(body) = self.world.add_body(&desc) else {
                    self.world.release_shape(shape);
                    tracing::warn!("physics is full; a terrain's tile is left out");
                    continue;
                };
                self.owners.insert(body, entity);
                self.terrains.get_mut(&entity).expect("kept").tiles[slot] = Some((tile.clone(), body, shape));
            }
            self.sync_dug(entity, terrain, corner, moved, layer);
            self.terrains.get_mut(&entity).expect("kept").corner = obj;
        }
    }

    /// A static body for each dug chunk of a terrain, its ground's triangles
    /// as a mesh, made again when the chunk changes, by pointer. A chunk
    /// whose triangles are still being made on the terrain's background
    /// threads keeps the body it had until they are done.
    fn sync_dug(&mut self, entity: Entity, terrain: &voxear_terrain::Terrain, corner: Vec3, moved: bool, layer: u8) {
        use voxear_terrain::dig::Excavation;
        let mut kept = std::mem::take(&mut self.terrains.get_mut(&entity).expect("kept").dug);
        let mut now = HashMap::new();
        for (key, chunk) in terrain.dug.as_ref().map(|dug| dug.chunks().iter()).into_iter().flatten() {
            let held = kept.remove(key);
            let mesh = match &held {
                Some((made, _)) if Arc::ptr_eq(made, chunk) => None,
                _ => terrain.dug.as_ref().and_then(|dug| Excavation::meshed(dug, &terrain.heights, *key)),
            };
            let Some(mesh) = mesh else {
                // The same, or not made yet: as it was.
                if let Some((made, body)) = held {
                    if moved && let Some((body, _)) = body {
                        self.world.set_transform(body, corner, Quat::IDENTITY);
                    }
                    now.insert(*key, (made, body));
                }
                continue;
            };
            if let Some((_, Some((body, shape)))) = held {
                self.world.remove_body(body);
                self.world.release_shape(shape);
                self.owners.remove(&body);
            }
            let body = (!mesh.indices.is_empty()).then(|| {
                let points: Vec<Vec3> = mesh.positions.iter().map(|p| Vec3::from(*p)).collect();
                let shape = self.world.mesh_shape(&points, &mesh.indices)?;
                let mut desc = BodyDesc::new(shape, Motion::Static).at(corner, Quat::IDENTITY);
                desc.friction = Collider::default().friction;
                desc.user_data = entity.to_bits();
                desc.layer = layer;
                let Some(body) = self.world.add_body(&desc) else {
                    self.world.release_shape(shape);
                    tracing::warn!("physics is full; a terrain's dug ground is left out");
                    return None;
                };
                self.owners.insert(body, entity);
                Some((body, shape))
            });
            now.insert(*key, (chunk.clone(), body.flatten()));
        }
        // Chunks no longer dug take their bodies with them.
        for (_, (_, body)) in kept {
            if let Some((body, shape)) = body {
                self.world.remove_body(body);
                self.world.release_shape(shape);
                self.owners.remove(&body);
            }
        }
        self.terrains.get_mut(&entity).expect("kept").dug = now;
    }

    /// Gives each world of blocks a static mesh for each of its chunks with
    /// faces, made again for a chunk whose faces changed, by pointer, and
    /// moved with its Obj.
    fn sync_block_worlds(&mut self, world: &World) {
        use voxear_terrain::blocks::{BlockWorld, chunk::CHUNK};
        voxear_core::profile_scope!("Block Bodies");
        let found: Vec<(Entity, &BlockWorld, Vec3)> = world
            .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, blocks, local, placed)| (entity, blocks, voxear_terrain::corner(local, placed)))
            .collect();
        let gone: Vec<Entity> =
            self.block_worlds.keys().copied().filter(|entity| !found.iter().any(|(e, ..)| e == entity)).collect();
        for entity in gone {
            self.remove_block_world(entity);
        }
        for (entity, blocks, corner) in found {
            let layer = layer(world, entity);
            let fits = self
                .block_worlds
                .get(&entity)
                .is_some_and(|kept| kept.layer == layer && kept.block_size == blocks.block_size);
            if !fits {
                self.remove_block_world(entity);
                let kept =
                    BlockBodies { corner, layer, block_size: blocks.block_size, meshes: None, chunks: HashMap::new() };
                self.block_worlds.insert(entity, kept);
            }
            // Nothing to do where its faces are those its bodies were made from, and it has not moved.
            let moved = self.block_worlds[&entity].corner != corner;
            let same = self.block_worlds[&entity].meshes.as_ref().is_some_and(|made| Arc::ptr_eq(made, &blocks.meshes));
            if !moved && same {
                continue;
            }
            let mut kept = self.block_worlds.remove(&entity).expect("kept above");
            let side = CHUNK as f32 * blocks.block_size;
            let at = |key: &[i32; 3]| corner + Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
            kept.chunks.retain(|key, (_, body, shape)| {
                let stays = blocks.meshes.contains_key(key);
                if !stays {
                    self.world.remove_body(*body);
                    self.world.release_shape(*shape);
                    self.owners.remove(body);
                }
                stays
            });
            let mut wanted: Vec<([i32; 3], &Arc<voxear_terrain::blocks::mesh::ChunkMesh>)> = Vec::new();
            for (key, mesh) in blocks.meshes.iter() {
                // Faces made again with the same triangles, as when only a neighbour's shading changed, keep their body.
                if let Some((made, body, _)) = kept.chunks.get_mut(key)
                    && (Arc::ptr_eq(made, mesh) || made.positions == mesh.positions && made.indices == mesh.indices)
                {
                    *made = mesh.clone();
                    if moved {
                        self.world.set_transform(*body, at(key), Quat::IDENTITY);
                    }
                    continue;
                }
                if let Some((_, body, shape)) = kept.chunks.remove(key) {
                    self.world.remove_body(body);
                    self.world.release_shape(shape);
                    self.owners.remove(&body);
                }
                // A chunk of water alone has no ground to stand on.
                if !mesh.indices.is_empty() {
                    wanted.push((*key, mesh));
                }
            }
            // The new chunks' shapes made on every core, then given bodies here.
            let block_size = blocks.block_size;
            let made = voxear_terrain::workers::each(&wanted, |(_, mesh)| {
                let points: Vec<Vec3> = mesh.positions.iter().map(|p| Vec3::from(*p) * block_size).collect();
                crate::MadeShape::mesh(&points, &mesh.indices)
            });
            for ((key, mesh), shape) in wanted.into_iter().zip(made) {
                let Some(shape) = shape else { continue };
                let shape = self.world.keep_shape(shape);
                let key = &key;
                let mut desc = BodyDesc::new(shape, Motion::Static).at(at(key), Quat::IDENTITY);
                desc.friction = Collider::default().friction;
                desc.user_data = entity.to_bits();
                desc.layer = layer;
                let Some(body) = self.world.add_body(&desc) else {
                    self.world.release_shape(shape);
                    tracing::warn!("physics is full; a world of blocks' chunk is left out");
                    continue;
                };
                self.owners.insert(body, entity);
                kept.chunks.insert(*key, (mesh.clone(), body, shape));
            }
            kept.corner = corner;
            kept.meshes = Some(blocks.meshes.clone());
            self.block_worlds.insert(entity, kept);
        }
    }

    fn remove_block_world(&mut self, entity: Entity) {
        let Some(kept) = self.block_worlds.remove(&entity) else { return };
        for (_, body, shape) in kept.chunks.into_values() {
            self.world.remove_body(body);
            self.world.release_shape(shape);
            self.owners.remove(&body);
        }
        self.forget(entity);
    }

    /// Stands a static capsule up for the trunk of each plant still there,
    /// on Objs whose plants have trunks; a plant taken away takes only its
    /// own body with it.
    fn sync_plants(&mut self, world: &World) {
        let found: Vec<(Entity, Plants, Mat4)> = world
            .query_ref::<(Entity, &Plants, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(_, plants, ..)| plants.trunk.is_some())
            .map(|(entity, plants, transform, placed)| {
                let model = placed.map_or_else(
                    || Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
                    |w| w.0,
                );
                (entity, plants.clone(), model)
            })
            .collect();
        let gone: Vec<Entity> =
            self.plants.keys().copied().filter(|entity| !found.iter().any(|(e, ..)| e == entity)).collect();
        for entity in gone {
            self.remove_plants(entity);
        }
        for (entity, plants, model) in found {
            let trunk = plants.trunk.expect("filtered for trunks");
            let layer = layer(world, entity);
            let kept = self.plants.get(&entity);
            let same = kept.is_some_and(|kept| {
                Arc::ptr_eq(&kept.plants, &plants.plants) && kept.model == model && kept.trunk == trunk && kept.layer == layer
            });
            // Plants are only ever taken away while a game runs; any back again (an undo) stands every trunk again.
            let only_taken = same
                && kept.is_some_and(|kept| kept.removed.iter().all(|number| plants.removed.contains(number)));
            if only_taken {
                if Arc::ptr_eq(&self.plants[&entity].removed, &plants.removed) {
                    continue;
                }
                let taken: Vec<u32> = plants.removed.iter().filter(|&n| !self.plants[&entity].removed.contains(n)).collect();
                for number in taken {
                    let place = self.plants.get_mut(&entity).expect("kept").bodies.get_mut(number as usize);
                    if let Some((body, shape)) = place.and_then(Option::take) {
                        self.world.remove_body(body);
                        self.world.release_shape(shape);
                        self.owners.remove(&body);
                        self.plant_bodies.remove(&body);
                    }
                }
                self.plants.get_mut(&entity).expect("kept").removed = plants.removed.clone();
                continue;
            }
            self.remove_plants(entity);
            let scale = model.to_scale_rotation_translation().0.max_element();
            let mut bodies = vec![None; plants.plants.len()];
            for (number, plant) in plants.standing() {
                let placed = plant.matrix(model);
                let (_, rotation, _) = placed.to_scale_rotation_translation();
                let size = plant.scale * scale;
                let radius = trunk.radius * size;
                let half_height = (trunk.height * size * 0.5 - radius).max(0.01);
                let centre = placed.transform_point3(Vec3::new(0.0, trunk.height * 0.5, 0.0));
                let shape = self.world.capsule_shape(half_height, radius);
                let mut desc = BodyDesc::new(shape, Motion::Static).at(centre, rotation);
                desc.friction = Collider::default().friction;
                desc.user_data = entity.to_bits();
                desc.layer = layer;
                let Some(body) = self.world.add_body(&desc) else {
                    self.world.release_shape(shape);
                    tracing::warn!("physics is full; plants' trunks are left out");
                    break;
                };
                self.owners.insert(body, entity);
                self.plant_bodies.insert(body, number);
                bodies[number as usize] = Some((body, shape));
            }
            let removed = plants.removed.clone();
            self.plants.insert(entity, PlantBodies { plants: plants.plants, model, trunk, layer, removed, bodies });
        }
    }

    fn remove_plants(&mut self, entity: Entity) {
        let Some(kept) = self.plants.remove(&entity) else { return };
        for (body, shape) in kept.bodies.into_iter().flatten() {
            self.world.remove_body(body);
            self.world.release_shape(shape);
            self.owners.remove(&body);
            self.plant_bodies.remove(&body);
        }
        self.forget(entity);
    }

    fn remove_terrain(&mut self, entity: Entity) {
        let Some(kept) = self.terrains.remove(&entity) else { return };
        let dug = kept.dug.into_values().filter_map(|(_, body)| body);
        for (body, shape) in kept.tiles.into_iter().flatten().map(|(_, body, shape)| (body, shape)).chain(dug) {
            self.world.remove_body(body);
            self.world.release_shape(shape);
            self.owners.remove(&body);
        }
        self.forget(entity);
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
            self.remove_vehicles_of(body);
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
        if self.bodies.contains_key(&entity)
            || self.characters.contains_key(&entity)
            || self.terrains.contains_key(&entity)
            || self.block_worlds.contains_key(&entity)
            || self.plants.contains_key(&entity)
        {
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
            .filter(|(entity, _)| voxear_scene::is_on(world, *entity, voxear_scene::Switch::Character))
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
    /// when that or its layer changed, to where the game put the Obj, and
    /// stands it against what pulls it there.
    fn sync_character(&mut self, world: &World, entity: Entity, controller: CharacterController) {
        let position = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        let layer = layer(world, entity);
        let gravity = gravity_at(world, self.gravity, position);
        match self.characters.get_mut(&entity) {
            Some(walker) if walker.controller == controller && walker.layer == layer => {
                if !walker.placed.abs_diff_eq(position, 1e-4) {
                    walker.placed = position;
                    let id = walker.id;
                    self.world.set_character_position(id, position);
                }
                self.stand(entity, gravity);
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
        let walker = Walker {
            id: character,
            controller,
            layer,
            placed: position,
            state: None,
            up: Vec3::Y,
            turn: Quat::IDENTITY,
        };
        self.characters.insert(entity, walker);
        self.stand(entity, gravity);
    }

    /// Stands a character against what pulls it, `gravity`: its up the
    /// other way, its capsule turned from its last up by the least turn, so
    /// it never spins about it. Where nothing pulls, it keeps the up it had.
    fn stand(&mut self, entity: Entity, gravity: Vec3) {
        let Some(walker) = self.characters.get_mut(&entity) else { return };
        let up = -gravity.normalize_or_zero();
        if up == Vec3::ZERO || up.dot(walker.up) > 1.0 - 1e-6 {
            return;
        }
        walker.turn = (Quat::from_rotation_arc(walker.up, up) * walker.turn).normalize();
        walker.up = up;
        self.world.set_character_up(walker.id, up, walker.turn);
    }

    /// Moves an Obj's Character Controller at `velocity` for `dt` seconds,
    /// at once, as Unity's CharacterController.Move does, and its Obj with
    /// it, standing against what pulls it where it is. Gravity is the
    /// caller's to add (see
    /// [`PhysicsScene::gravity_at`]). `None` for an Obj without one.
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
        let gravity = gravity_at(world, self.gravity, walker.placed);
        let state = self.world.move_character(walker.id, velocity, dt, walker.controller.step_offset, gravity);
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

fn is_car(kind: &VehicleKind) -> bool {
    *kind == VehicleKind::Car
}

/// A car's gearbox, as Jolt's own: five gears from 2.66 to 0.74.
fn gears() -> u32 {
    5
}

fn is_gears(value: &u32) -> bool {
    *value == gears()
}

fn first_gear() -> f32 {
    2.66
}

fn is_first_gear(value: &f32) -> bool {
    *value == first_gear()
}

fn top_gear() -> f32 {
    0.74
}

fn is_top_gear(value: &f32) -> bool {
    *value == top_gear()
}

fn torque() -> f32 {
    500.0
}

fn is_torque(value: &f32) -> bool {
    *value == torque()
}

fn min_rpm() -> f32 {
    1000.0
}

fn is_min_rpm(value: &f32) -> bool {
    *value == min_rpm()
}

fn max_rpm() -> f32 {
    6000.0
}

fn is_max_rpm(value: &f32) -> bool {
    *value == max_rpm()
}

fn wheel_radius() -> f32 {
    0.35
}

fn is_wheel_radius(value: &f32) -> bool {
    *value == wheel_radius()
}

fn wheel_width() -> f32 {
    0.25
}

fn is_wheel_width(value: &f32) -> bool {
    *value == wheel_width()
}

fn travel() -> f32 {
    0.3
}

fn is_travel(value: &f32) -> bool {
    *value == travel()
}

fn frequency() -> f32 {
    1.5
}

fn is_frequency(value: &f32) -> bool {
    *value == frequency()
}

fn brake() -> f32 {
    1500.0
}

fn is_brake(value: &f32) -> bool {
    *value == brake()
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
    fn a_vehicle_drives_on_its_childrens_wheels_and_turns_them() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = world.spawn((Transform::from_position(Vec3::new(0.0, -0.5, 0.0)),));
        world.insert(ground, Collider { size: [400.0, 1.0, 400.0], ..Collider::default() });
        let car = world.spawn((Transform::from_position(Vec3::new(0.0, 1.0, 0.0)),));
        world.insert(car, Collider { size: [1.8, 0.6, 4.0], ..Collider::default() });
        world.insert(car, Rigidbody { mass: 1200.0, ..Rigidbody::default() });
        world.insert(car, Vehicle::default());
        let mut wheels = Vec::new();
        for (x, z) in [(-0.9, 1.4), (0.9, 1.4), (-0.9, -1.4), (0.9, -1.4)] {
            let wheel = world.spawn((Transform::from_position(Vec3::new(x, -0.4, z)), Parent(car)));
            let front = z > 0.0;
            world.insert(
                wheel,
                Wheel { steer_angle: if front { 35.0 } else { 0.0 }, powered: !front, ..Wheel::default() },
            );
            wheels.push(wheel);
        }
        let mut physics = PhysicsScene::new();
        let drive = |world: &mut World, physics: &mut PhysicsScene, throttle: f32, steering: f32, steps: usize| {
            for _ in 0..steps {
                let vehicle = world.get_mut::<Vehicle>(car).unwrap();
                (vehicle.throttle, vehicle.steering) = (throttle, steering);
                physics.step(world, 1.0 / 60.0);
            }
        };
        drive(&mut world, &mut physics, 0.0, 0.0, 60);
        let wheel = world.get::<Transform>(wheels[0]).unwrap().position;
        assert!(wheel.x < -0.8 && wheel.y < 0.0, "its wheel's Obj hangs where the wheel is: {wheel}");

        drive(&mut world, &mut physics, 1.0, 0.0, 180);
        let at = world.get::<Transform>(car).unwrap().position;
        let vehicle = *world.get::<Vehicle>(car).unwrap();
        assert!(at.z > 5.0, "driven forward: {at}");
        assert!(vehicle.speed > 2.0 && vehicle.gear >= 1, "its speed and gear for scripts: {vehicle:?}");
        let spun = world.get::<Transform>(wheels[2]).unwrap().rotation;
        assert!(spun.angle_between(Quat::IDENTITY) > 0.01, "a wheel's Obj rolls with it");

        drive(&mut world, &mut physics, 1.0, 1.0, 120);
        assert!(world.get::<Transform>(car).unwrap().position.x > at.x + 1.0, "steered right");

        // Taken away, the Vehicle leaves an ordinary Rigidbody.
        world.remove::<Vehicle>(car);
        physics.step(&mut world, 1.0 / 60.0);
        assert!(physics.vehicles.is_empty());
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

    #[test]
    fn a_chunk_of_blocks_made_again_with_the_same_triangles_keeps_its_body_and_a_changed_one_gets_one_as_drawn() {
        use voxear_terrain::blocks::{BlockFaces, BlockMap, BlockWorld};
        // Ground 4 blocks high over two chunks along x, a metre a block.
        let mut blocks = BlockMap::default();
        for z in 0..32 {
            for x in 0..64 {
                for y in 0..4 {
                    blocks.set([x, y, z], 1);
                }
            }
        }
        let faces = [BlockFaces::default(), BlockFaces::new(1, 2, 3)];
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let entity = world.spawn((BlockWorld::new(blocks, &faces, 1.0), Transform::default()));
        let mut physics = PhysicsScene::new();
        physics.step(&mut world, 1.0 / 60.0);
        let body = |physics: &PhysicsScene, key: [i32; 3]| physics.block_worlds[&entity].chunks[&key].1;
        let (west, east) = (body(&physics, [0, 0, 0]), body(&physics, [1, 0, 0]));
        // A block on the second chunk's edge: the first's faces made again, only their shading changed.
        world.get_mut::<BlockWorld>(entity).unwrap().set_blocks([([32, 4, 5], 1)]);
        let drawn = world.get::<BlockWorld>(entity).unwrap().meshes.clone();
        physics.step(&mut world, 1.0 / 60.0);
        assert_eq!(body(&physics, [0, 0, 0]), west, "the same triangles, the same body");
        assert!(Arc::ptr_eq(&physics.block_worlds[&entity].chunks[&[0, 0, 0]].0, &drawn[&[0, 0, 0]]));
        assert_ne!(body(&physics, [1, 0, 0]), east, "new triangles, a new body");
        // Rays meet the ground as it is drawn: the new block's top, and the ground's beside it.
        let down = |x: f32| physics.raycast(Vec3::new(x, 20.0, 5.5), Vec3::NEG_Y, 40.0, u32::MAX, false).unwrap();
        assert!((down(32.5).point.y - 5.0).abs() < 1e-4 && (down(31.5).point.y - 4.0).abs() < 1e-4);
        assert_eq!(down(32.5).obj, entity);
    }
}
