//! 2D physics on Objs, as Unity's: a **Collider 2D** gives an Obj a flat
//! shape in the XY plane, and a **Rigidbody 2D** beside it makes physics
//! move it, across the plane and turning about Z. Each is written in a
//! scene file as its table:
//!
//! ```toml
//! [obj.collider2d]
//! shape = "circle"       # box, circle or capsule
//! radius = 0.25
//!
//! [obj.rigidbody2d]
//! mass = 2.0
//! fixed_rotation = true
//! ```
//!
//! While the game plays, [`PhysicsScene2D`] keeps a Box2D body for each Obj
//! with a Collider 2D, as the 3D scene does for Jolt: still ones moved
//! where the game puts them, kinematic ones moved over the step, and
//! dynamic ones writing their place back to their Objs' Transforms, which
//! keep their Z.

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Parent, Transform};

use crate::{Body2D, BodyDesc2D, Motion2D, PhysicsWorld2D, Push2D, Shape2D};

/// A Collider 2D's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColliderShape2D {
    #[default]
    Box,
    Circle,
    Capsule,
}

impl ColliderShape2D {
    pub const ALL: [Self; 3] = [Self::Box, Self::Circle, Self::Capsule];

    pub fn label(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Circle => "Circle",
            Self::Capsule => "Capsule",
        }
    }
}

/// A flat shape around an Obj in the XY plane, scaled with the Obj, its
/// middle at `offset` from the Obj's origin. A box uses `size`, a circle
/// `radius`, and a capsule, standing up, `height` and `radius`. A trigger
/// pushes nothing, and only notices what enters it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collider2D {
    #[serde(default)]
    pub shape: ColliderShape2D,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: [f32; 2],
    #[serde(default = "unit", skip_serializing_if = "is_unit")]
    pub size: [f32; 2],
    #[serde(default = "half", skip_serializing_if = "is_half")]
    pub radius: f32,
    #[serde(default = "two", skip_serializing_if = "is_two")]
    pub height: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub trigger: bool,
    #[serde(default = "grip", skip_serializing_if = "is_grip")]
    pub friction: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub bounciness: f32,
}

impl Default for Collider2D {
    fn default() -> Self {
        Self {
            shape: ColliderShape2D::Box,
            offset: [0.0; 2],
            size: unit(),
            radius: half(),
            height: two(),
            trigger: false,
            friction: grip(),
            bounciness: 0.0,
        }
    }
}

impl Component for Collider2D {}

/// Makes 2D physics move its Obj, which needs a Collider 2D beside it. A
/// kinematic one is moved by the game instead, pushing what is in the way.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rigidbody2D {
    /// In kilograms.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub mass: f32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub kinematic: bool,
    /// How strongly gravity pulls it: 0 floats, 1 falls as everything else.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub gravity_scale: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub drag: f32,
    #[serde(default = "turn_drag", skip_serializing_if = "is_turn_drag")]
    pub angular_drag: f32,
    /// Never turns, as a platformer's character should not.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed_rotation: bool,
    /// Checked along its whole path each step, so a fast body cannot pass
    /// through a thin one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub continuous: bool,
}

impl Default for Rigidbody2D {
    fn default() -> Self {
        Self {
            mass: 1.0,
            kinematic: false,
            gravity_scale: 1.0,
            drag: 0.0,
            angular_drag: turn_drag(),
            fixed_rotation: false,
            continuous: false,
        }
    }
}

impl Component for Rigidbody2D {}

/// What a body was made from, to make it again when any of it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyKey {
    collider: Collider2D,
    rigidbody: Option<Rigidbody2D>,
    scale: Vec2,
}

struct Kept {
    body: Body2D,
    key: BodyKey,
    /// Where it was put last, to tell when the game moved it.
    placed: (Vec2, f32),
}

/// Two Objs that began or stopped touching in a step, or a trigger and
/// what entered or left it. The normal points from `a` towards `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsEvent2D {
    pub began: bool,
    pub trigger: bool,
    pub a: Entity,
    pub b: Entity,
    pub point: Vec2,
    pub normal: Vec2,
}

/// Where a ray met an Obj's Collider 2D.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneHit2D {
    pub obj: Entity,
    pub point: Vec2,
    pub normal: Vec2,
    /// How far from where the ray started, in metres.
    pub distance: f32,
}

/// How far a ray reaches when it is told to reach forever.
const FOREVER: f32 = 1.0e6;

/// Two Objs as a pair, the same whichever comes first.
fn pair(a: Entity, b: Entity) -> (Entity, Entity) {
    if a.to_bits() <= b.to_bits() { (a, b) } else { (b, a) }
}

/// Where an Obj is in the plane, and how far it is turned about Z.
fn flat(rotation: Quat, position: Vec3) -> (Vec2, f32) {
    let across = rotation * Vec3::X;
    (position.truncate(), across.y.atan2(across.x))
}

fn same_place(a: (Vec2, f32), b: (Vec2, f32)) -> bool {
    a.0.abs_diff_eq(b.0, 1e-4) && (a.1 - b.1).abs() < 1e-5
}

/// The Box2D world for the Objs with a Collider 2D, while the game plays.
pub struct PhysicsScene2D {
    world: PhysicsWorld2D,
    bodies: HashMap<Entity, Kept>,
    events: Vec<PhysicsEvent2D>,
    touching: HashSet<(Entity, Entity)>,
    /// Touching pairs of a body made again since the last step, which
    /// Box2D tells of afresh in the step if they still touch.
    unsure: HashSet<(Entity, Entity)>,
    gravity: Vec2,
    stepped: bool,
}

impl Default for PhysicsScene2D {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsScene2D {
    pub fn new() -> Self {
        Self {
            world: PhysicsWorld2D::default(),
            bodies: HashMap::new(),
            events: Vec::new(),
            touching: HashSet::new(),
            unsure: HashSet::new(),
            gravity: Vec2::new(0.0, -9.81),
            stepped: false,
        }
    }

    /// Whether a world has anything for 2D physics to do.
    pub fn wanted(world: &World) -> bool {
        world.query_ref::<&Collider2D>().into_iter().next().is_some()
    }

    pub fn world(&self) -> &PhysicsWorld2D {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut PhysicsWorld2D {
        &mut self.world
    }

    pub fn gravity(&self) -> Vec2 {
        self.gravity
    }

    pub fn set_gravity(&mut self, gravity: Vec2) {
        self.gravity = gravity;
        self.world.set_gravity(gravity);
    }

    /// The body of an Obj's Collider 2D, while it has one.
    pub fn body(&self, entity: Entity) -> Option<Body2D> {
        self.bodies.get(&entity).map(|kept| kept.body)
    }

    /// Whether two Objs' colliders are touching, or one is inside the other's trigger.
    pub fn touching(&self, a: Entity, b: Entity) -> bool {
        self.touching.contains(&pair(a, b))
    }

    /// Brings the bodies to where their Objs are, before the first step,
    /// so that a script's question in `Start` finds them.
    pub fn ready(&mut self, world: &World) {
        if !self.stepped {
            self.sync(world, 0.0);
        }
    }

    /// The body of an Obj's Collider 2D, made or made again now if it
    /// changed since the last step, so that a script can push a body it
    /// has only just given a Rigidbody 2D.
    pub fn body_now(&mut self, world: &World, entity: Entity) -> Option<Body2D> {
        let collider = *world.get::<Collider2D>(entity)?;
        self.sync_body(world, entity, collider, 0.0);
        self.body(entity)
    }

    /// The first Collider 2D a ray from `origin` meets along `direction`
    /// within `distance` metres, passing over triggers unless `triggers`.
    pub fn raycast(&self, origin: Vec2, direction: Vec2, distance: f32, triggers: bool) -> Option<SceneHit2D> {
        let direction = direction.normalize_or_zero();
        if direction == Vec2::ZERO || distance.is_nan() || distance <= 0.0 {
            return None;
        }
        let reach = distance.min(FOREVER);
        let hit = self.world.raycast(origin, direction * reach, triggers)?;
        let obj = Entity::from_bits(hit.user_data);
        Some(SceneHit2D { obj, point: hit.point, normal: hit.normal, distance: hit.fraction * reach })
    }

    /// The Objs whose Collider 2D a circle overlaps, each once, with
    /// triggers when `triggers`.
    pub fn overlap_circle(&self, center: Vec2, radius: f32, triggers: bool) -> Vec<Entity> {
        let mut objs: Vec<Entity> = Vec::new();
        for data in self.world.overlap_circle(center, radius, triggers) {
            let obj = Entity::from_bits(data);
            if !objs.contains(&obj) {
                objs.push(obj);
            }
        }
        objs
    }

    /// The contacts and trigger crossings since the last call.
    pub fn take_events(&mut self) -> Vec<PhysicsEvent2D> {
        std::mem::take(&mut self.events)
    }

    /// Moves 2D physics on by one fixed step: bodies made, moved or taken
    /// out as their Objs are now, the step, the dynamic bodies' Objs moved
    /// to where physics put them, and the step's contacts noted.
    pub fn step(&mut self, world: &mut World, dt: f32) {
        self.sync(world, dt);
        self.stepped = true;
        self.world.step(dt);
        self.write_back(world);
        self.note_contacts();
    }

    fn sync(&mut self, world: &World, dt: f32) {
        let colliders: Vec<(Entity, Collider2D)> = world
            .query_ref::<(Entity, &Collider2D)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, collider)| (entity, *collider))
            .collect();
        let gone: Vec<Entity> =
            self.bodies.keys().copied().filter(|entity| !colliders.iter().any(|(e, _)| e == entity)).collect();
        for entity in gone {
            if let Some(kept) = self.bodies.remove(&entity) {
                self.world.remove_body(kept.body);
            }
            // Unity tells nothing of an Obj's contacts ending as it goes.
            self.touching.retain(|(a, b)| *a != entity && *b != entity);
        }
        for (entity, collider) in colliders {
            self.sync_body(world, entity, collider, dt);
        }
    }

    /// Brings an Obj's body to its Collider 2D and its Obj: made again
    /// when either changed, and moved where the game put the Obj.
    fn sync_body(&mut self, world: &World, entity: Entity, collider: Collider2D, dt: f32) {
        let (scale, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
        let place = flat(rotation, position);
        // Rounded, as a scale read back from a turned Obj's matrix is a hair off, and the body would be
        // made again each step it turns.
        let scale = (scale.truncate() * 1e4).round() / 1e4;
        let key = BodyKey { collider, rigidbody: world.get::<Rigidbody2D>(entity).copied(), scale };
        let Some(kept) = self.bodies.get_mut(&entity).filter(|kept| kept.key == key) else {
            if let Some(kept) = self.bodies.remove(&entity) {
                self.world.remove_body(kept.body);
                let pairs = self.touching.iter().filter(|(a, b)| *a == entity || *b == entity);
                self.unsure.extend(pairs.copied());
            }
            let body = self.world.add_body(&desc(entity, &key, place));
            self.bodies.insert(entity, Kept { body, key, placed: place });
            return;
        };
        if same_place(kept.placed, place) {
            return;
        }
        kept.placed = place;
        let body = kept.body;
        match key.rigidbody.is_some_and(|body| body.kinematic) && dt > 0.0 {
            true => self.world.move_kinematic(body, place.0, place.1, dt),
            false => self.world.set_transform(body, place.0, place.1),
        }
    }

    /// Dynamic bodies' Objs move to where physics put them, keeping their Z.
    fn write_back(&mut self, world: &mut World) {
        for (entity, kept) in &mut self.bodies {
            if !kept.key.rigidbody.is_some_and(|body| !body.kinematic) || !self.world.is_awake(kept.body) {
                continue;
            }
            let (at, angle) = self.world.transform(kept.body);
            kept.placed = (at, angle);
            let placed = voxear_scene::world_matrix(world, *entity);
            let (_, _, before) = placed.to_scale_rotation_translation();
            let (position, rotation) = (at.extend(before.z), Quat::from_rotation_z(angle));
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

    /// The step's contacts, as the pairs that began and stopped touching,
    /// once for a pair touching in several places. A body made again
    /// begins its contacts afresh, which say only that it still touches;
    /// what it touched and no longer does has ended.
    fn note_contacts(&mut self) {
        let contacts = self.world.contacts();
        for contact in contacts.iter().filter(|c| !c.began) {
            let (a, b) = (Entity::from_bits(contact.a), Entity::from_bits(contact.b));
            if self.touching.remove(&pair(a, b)) {
                self.tell(false, contact.sensor, a, b, Vec2::ZERO, Vec2::ZERO);
            }
        }
        for contact in contacts.iter().filter(|c| c.began) {
            let (a, b) = (Entity::from_bits(contact.a), Entity::from_bits(contact.b));
            let known = self.bodies.contains_key(&a) && self.bodies.contains_key(&b);
            if self.unsure.remove(&pair(a, b)) {
                continue;
            }
            if known && self.touching.insert(pair(a, b)) {
                self.tell(true, contact.sensor, a, b, contact.point, contact.normal);
            }
        }
        for (a, b) in std::mem::take(&mut self.unsure) {
            if self.touching.remove(&(a, b)) {
                let sensor = [a, b].iter().any(|e| self.bodies.get(e).is_some_and(|kept| kept.key.collider.trigger));
                self.tell(false, sensor, a, b, Vec2::ZERO, Vec2::ZERO);
            }
        }
    }

    fn tell(&mut self, began: bool, sensor: bool, a: Entity, b: Entity, point: Vec2, normal: Vec2) {
        let trigger = sensor || [a, b].iter().any(|e| self.bodies.get(e).is_some_and(|kept| kept.key.collider.trigger));
        self.events.push(PhysicsEvent2D { began, trigger, a, b, point, normal });
    }

    /// Pushes an Obj's body, made now if need be.
    pub fn push(&mut self, world: &World, entity: Entity, push: Push2D) -> bool {
        let Some(body) = self.body_now(world, entity) else { return false };
        self.world.push(body, push);
        true
    }
}

/// The Box2D body for an Obj's Collider 2D and Rigidbody 2D at its scale.
/// A circle and a capsule take the larger scale across them, as round
/// shapes cannot stretch one way.
fn desc(entity: Entity, key: &BodyKey, place: (Vec2, f32)) -> BodyDesc2D {
    let collider = &key.collider;
    let scale = key.scale.abs().max(Vec2::splat(1e-4));
    let center = Vec2::from(collider.offset) * scale;
    let shape = match collider.shape {
        ColliderShape2D::Box => Shape2D::Box { half: Vec2::from(collider.size) * scale * 0.5, center },
        ColliderShape2D::Circle => Shape2D::Circle { radius: collider.radius * scale.max_element(), center },
        ColliderShape2D::Capsule => {
            let radius = collider.radius * scale.x;
            Shape2D::Capsule { half: (collider.height * scale.y * 0.5 - radius).max(0.01), radius, center }
        }
    };
    let motion = match key.rigidbody {
        None => Motion2D::Static,
        Some(body) if body.kinematic => Motion2D::Kinematic,
        Some(_) => Motion2D::Dynamic,
    };
    let mut desc = BodyDesc2D::new(shape, motion);
    (desc.position, desc.angle) = place;
    desc.friction = collider.friction;
    desc.restitution = collider.bounciness;
    desc.sensor = collider.trigger;
    desc.user_data = entity.to_bits();
    if let Some(body) = key.rigidbody {
        desc.mass = body.mass;
        desc.gravity_scale = body.gravity_scale;
        desc.linear_damping = body.drag;
        desc.angular_damping = body.angular_drag;
        desc.fixed_rotation = body.fixed_rotation;
        desc.bullet = body.continuous;
    }
    desc
}

fn unit() -> [f32; 2] {
    [1.0; 2]
}

fn is_unit(value: &[f32; 2]) -> bool {
    *value == [1.0; 2]
}

fn is_zero(value: &[f32; 2]) -> bool {
    *value == [0.0; 2]
}

fn half() -> f32 {
    0.5
}

fn is_half(value: &f32) -> bool {
    *value == half()
}

fn two() -> f32 {
    2.0
}

fn is_two(value: &f32) -> bool {
    *value == two()
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == one()
}

fn grip() -> f32 {
    0.6
}

fn is_grip(value: &f32) -> bool {
    *value == grip()
}

fn turn_drag() -> f32 {
    0.05
}

fn is_turn_drag(value: &f32) -> bool {
    *value == turn_drag()
}

fn is_zero_f32(value: &f32) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(world: &mut World, at: Vec3, collider: Collider2D) -> Entity {
        let entity = world.spawn((Transform::from_position(at),));
        world.insert(entity, collider);
        entity
    }

    #[test]
    fn a_rigidbody_falls_onto_a_collider_through_a_trigger_keeping_its_z() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = obj(&mut world, Vec3::new(0.0, -0.5, 0.0), Collider2D { size: [20.0, 1.0], ..Default::default() });
        let gate = Collider2D { size: [10.0, 0.5], trigger: true, ..Default::default() };
        let gate = obj(&mut world, Vec3::new(0.0, 2.0, 0.0), gate);
        let ball = Collider2D { shape: ColliderShape2D::Circle, ..Default::default() };
        let ball = obj(&mut world, Vec3::new(0.0, 4.0, 3.0), ball);
        world.insert(ball, Rigidbody2D::default());
        let mut physics = PhysicsScene2D::new();
        let mut events = Vec::new();
        for _ in 0..240 {
            physics.step(&mut world, 1.0 / 60.0);
            events.extend(physics.take_events());
        }
        let at = world.get::<Transform>(ball).unwrap().position;
        assert!((at.y - 0.5).abs() < 0.02 && at.z == 3.0, "it rests on the ground, where it was in Z: {at}");
        let landed = events.iter().find(|e| e.began && !e.trigger).expect("it landed");
        assert_eq!((landed.a, landed.b), (ground, ball), "{events:?}");
        let crossed = |began| events.iter().filter(|e| e.trigger && e.began == began && e.a == gate).count();
        assert_eq!((crossed(true), crossed(false)), (1, 1), "in and out of the gate: {events:?}");

        // A ray and a circle find what is where.
        let hit = physics.raycast(Vec2::new(0.0, 10.0), Vec2::NEG_Y, f32::INFINITY, false).unwrap();
        assert_eq!(hit.obj, ball);
        let trigger = physics.raycast(Vec2::new(0.0, 10.0), Vec2::NEG_Y, f32::INFINITY, true).unwrap();
        assert_eq!(trigger.obj, gate, "when triggers are asked for");
        assert!((hit.distance - 9.0).abs() < 0.02, "{hit:?}");
        assert_eq!(physics.overlap_circle(Vec2::new(0.0, 2.0), 0.1, true), [gate]);

        // Made again, a body keeps touching what it touched, untold.
        world.get_mut::<Collider2D>(ball).unwrap().friction = 0.2;
        physics.step(&mut world, 1.0 / 60.0);
        physics.step(&mut world, 1.0 / 60.0);
        assert!(physics.take_events().is_empty() && physics.touching(ground, ball));
    }
}
