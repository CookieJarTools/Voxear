//! Physics on Objs, as Unity's: a **Collider** gives an Obj a shape that
//! others bump into, a **Rigidbody** beside it makes physics move it, and
//! a **Character Controller** walks it, moved by the game's scripts. Each
//! is written in a scene file as its table:
//!
//! ```toml
//! [obj.collider]
//! shape = "box"          # box, sphere or capsule
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

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_physics::{BodyDesc, BodyId, CharacterDesc, CharacterId, CharacterState, Motion, PhysicsWorld, ShapeId};
use voxear_scene::{Destroying, Parent, Transform};

/// A collider's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColliderShape {
    #[default]
    Box,
    Sphere,
    Capsule,
}

impl ColliderShape {
    pub const ALL: [Self; 3] = [Self::Box, Self::Sphere, Self::Capsule];

    pub fn label(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Capsule => "Capsule",
        }
    }
}

/// A shape around an Obj that others bump into, scaled with the Obj, its
/// middle at `center` from the Obj's origin. A box uses `size`, a sphere
/// `radius`, and a capsule, standing up, `height` and `radius`. A trigger
/// pushes nothing, and only notices what enters it.
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub trigger: bool,
    /// How much it grips what slides on it, from 0 for ice.
    #[serde(default = "grip", skip_serializing_if = "is_grip")]
    pub friction: f32,
    /// How much what hits it bounces back, from 0 to 1.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub bounciness: f32,
}

impl Default for Collider {
    fn default() -> Self {
        Self {
            shape: ColliderShape::Box,
            center: [0.0; 3],
            size: unit(),
            radius: half(),
            height: two(),
            trigger: false,
            friction: grip(),
            bounciness: 0.0,
        }
    }
}

impl Component for Collider {}

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

/// What a body was made from, to make it again when any of it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyKey {
    collider: Collider,
    rigidbody: Option<Rigidbody>,
    scale: Vec3,
}

struct Kept {
    body: BodyId,
    shape: ShapeId,
    key: BodyKey,
    /// Where it was put last, to tell when the game moved it.
    placed: (Vec3, Quat),
}

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
    characters: HashMap<Entity, (CharacterId, CharacterController)>,
    events: Vec<PhysicsEvent>,
    /// The pairs of Objs touching, by [`pair`].
    touching: HashSet<(Entity, Entity)>,
    /// Touching pairs Jolt stopped telling of without their parting: one
    /// went to sleep, or was made again. Each counts the steps since both
    /// were awake.
    unsure: HashMap<(Entity, Entity), u8>,
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
            events: Vec::new(),
            touching: HashSet::new(),
            unsure: HashMap::new(),
        }
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
        self.world.step(dt, 1);
        self.write_back(world);
        self.note_contacts();
    }

    fn sync(&mut self, world: &mut World, dt: f32) {
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
            let (scale, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
            let key = BodyKey { collider, rigidbody: world.get::<Rigidbody>(entity).copied(), scale };
            match self.bodies.get(&entity) {
                Some(kept) if kept.key == key => {
                    let moved =
                        !kept.placed.0.abs_diff_eq(position, 1e-5) || !kept.placed.1.abs_diff_eq(rotation, 1e-5);
                    let dynamic = key.rigidbody.is_some_and(|body| !body.kinematic);
                    if moved && !dynamic {
                        let body = kept.body;
                        match key.rigidbody.is_some() {
                            true => self.world.move_kinematic(body, position, rotation, dt),
                            false => self.world.set_transform(body, position, rotation),
                        }
                        if let Some(kept) = self.bodies.get_mut(&entity) {
                            kept.placed = (position, rotation);
                        }
                    }
                }
                had => {
                    if had.is_some() {
                        self.doubt(entity);
                    }
                    self.remove(entity);
                    self.add(entity, key, position, rotation);
                }
            }
        }
        self.sync_characters(world);
    }

    fn add(&mut self, entity: Entity, key: BodyKey, position: Vec3, rotation: Quat) {
        let shape = self.shape(&key.collider, key.scale);
        let motion = match key.rigidbody {
            None => Motion::Static,
            Some(body) if body.kinematic => Motion::Kinematic,
            Some(_) => Motion::Dynamic,
        };
        let mut desc = BodyDesc::new(shape, motion).at(position, rotation);
        desc.friction = key.collider.friction;
        desc.restitution = key.collider.bounciness;
        desc.sensor = key.collider.trigger;
        desc.user_data = entity.to_bits();
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
        if let Some(kept) = self.bodies.remove(&entity) {
            self.world.remove_body(kept.body);
            self.world.release_shape(kept.shape);
            self.owners.remove(&kept.body);
        }
    }

    /// A collider's shape at an Obj's scale. A sphere and a capsule take
    /// the scale across them, as round shapes cannot stretch one way.
    fn shape(&mut self, collider: &Collider, scale: Vec3) -> ShapeId {
        let scale = scale.abs().max(Vec3::splat(1e-4));
        let across = scale.x.max(scale.z);
        let base = match collider.shape {
            ColliderShape::Box => self.world.box_shape(Vec3::from(collider.size) * scale * 0.5),
            ColliderShape::Sphere => self.world.sphere_shape(collider.radius * scale.max_element()),
            ColliderShape::Capsule => {
                let radius = collider.radius * across;
                let half = (collider.height * scale.y * 0.5 - radius).max(0.01);
                self.world.capsule_shape(half, radius)
            }
        };
        let center = Vec3::from(collider.center) * scale;
        if center == Vec3::ZERO {
            return base;
        }
        let placed = self.world.placed_shape(base, center, Quat::IDENTITY, Vec3::ONE);
        self.world.release_shape(base);
        placed
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
        let Some((character, _)) = self.characters.remove(&entity) else { return };
        if let Some(body) = self.world.character_body(character) {
            self.owners.remove(&body);
        }
        self.world.remove_character(character);
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
            match self.characters.get(&entity) {
                Some((_, had)) if *had == controller => continue,
                Some(_) => self.doubt(entity),
                None => {}
            }
            self.remove_character(entity);
            let position = voxear_scene::world_matrix(world, entity).w_axis.truncate();
            let desc = CharacterDesc {
                position,
                height: controller.height,
                radius: controller.radius,
                max_slope: controller.slope_limit,
                step_height: controller.step_offset,
                ..CharacterDesc::default()
            };
            let character = self.world.add_character(&desc);
            if let Some(body) = self.world.character_body(character) {
                self.owners.insert(body, entity);
            }
            self.characters.insert(entity, (character, controller));
        }
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
        if !self.characters.contains_key(&entity) {
            self.sync_characters(world);
        }
        let (character, controller) = *self.characters.get(&entity)?;
        // Where the game put the Obj since, the character goes too.
        let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        self.world.set_character_position(character, at);
        let state = self.world.move_character(character, velocity, dt, controller.step_offset);
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
        assert_eq!(state.map(|s| s.ground), Some(voxear_physics::Ground::Ground));
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
}
