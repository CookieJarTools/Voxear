//! Physics for C#: `Collider`, `Rigidbody`, `CharacterController` and
//! `Physics` in Voxear.Core (see `voxear_physics::scene`), and in 2D
//! `Collider2D`, `Rigidbody2D` and `Physics2D` (see
//! `voxear_physics2d::scene`), through the same calls. A component's
//! values are read and written through one call, by number, each up to
//! four floats; pushes, rays, overlaps and a character's move have calls
//! of their own. The scene lives in the world as a resource, made the
//! first time a script asks it something.
//!
//! The contacts and trigger crossings of each step go the other way, to
//! `PhysicsMessages` in C#, as [`PhysicsEventData`].

mod vehicles;

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_physics::{
    Ground, Push,
    scene::{
        Acoustic, Buoyancy, CharacterController, Collider, ColliderShape, Joint, JointType, PhysicsEvent, PhysicsScene, Rigidbody,
    },
};
use voxear_physics2d::{
    Push2D,
    scene::{Collider2D, ColliderShape2D, PhysicsEvent2D, PhysicsScene2D, Rigidbody2D},
};

use crate::interop::with_world;

// What `physics_value` reads or writes, as `NativePhysics` in Voxear.Core
// numbers them. A Collider's:
const COLLIDER_HAS: i32 = 0;
const COLLIDER_ADD: i32 = 1;
const COLLIDER_REMOVE: i32 = 2;
const COLLIDER_SHAPE: i32 = 3;
const COLLIDER_CENTER: i32 = 4;
const COLLIDER_SIZE: i32 = 5;
const COLLIDER_RADIUS: i32 = 6;
const COLLIDER_HEIGHT: i32 = 7;
const COLLIDER_TRIGGER: i32 = 8;
const COLLIDER_FRICTION: i32 = 9;
const COLLIDER_BOUNCINESS: i32 = 10;
const COLLIDER_CONVEX: i32 = 11;
const COLLIDER_ACOUSTIC: i32 = 12;
// A Rigidbody's:
const BODY_HAS: i32 = 20;
const BODY_ADD: i32 = 21;
const BODY_REMOVE: i32 = 22;
const BODY_MASS: i32 = 23;
const BODY_KINEMATIC: i32 = 24;
const BODY_GRAVITY: i32 = 25;
const BODY_DRAG: i32 = 26;
const BODY_ANGULAR_DRAG: i32 = 27;
const BODY_CONTINUOUS: i32 = 28;
const BODY_VELOCITY: i32 = 29;
const BODY_ANGULAR_VELOCITY: i32 = 30;
const BODY_AWAKE: i32 = 31;
const BUOYANCY_HAS: i32 = 80;
const BUOYANCY_ADD: i32 = 81;
const BUOYANCY_REMOVE: i32 = 82;
const BUOYANCY_SUBMERGED: i32 = 83;
const BUOYANCY_DRAG: i32 = 84;
const BUOYANCY_ANGULAR_DRAG: i32 = 85;
// A Character Controller's:
const CHARACTER_HAS: i32 = 40;
const CHARACTER_ADD: i32 = 41;
const CHARACTER_REMOVE: i32 = 42;
const CHARACTER_HEIGHT: i32 = 43;
const CHARACTER_RADIUS: i32 = 44;
const CHARACTER_SLOPE_LIMIT: i32 = 45;
const CHARACTER_STEP_OFFSET: i32 = 46;
const CHARACTER_GROUNDED: i32 = 47;
const CHARACTER_VELOCITY: i32 = 48;
const CHARACTER_GROUND_NORMAL: i32 = 49;
// The world's:
const GRAVITY: i32 = 60;
// A Joint's; its limits are the least and the greatest, and whether it has them:
const JOINT_HAS: i32 = 70;
const JOINT_ADD: i32 = 71;
const JOINT_REMOVE: i32 = 72;
const JOINT_TYPE: i32 = 73;
const JOINT_ANCHOR: i32 = 74;
const JOINT_AXIS: i32 = 75;
const JOINT_LIMITS: i32 = 76;
// A Collider 2D's:
const COLLIDER_2D_HAS: i32 = 100;
const COLLIDER_2D_ADD: i32 = 101;
const COLLIDER_2D_REMOVE: i32 = 102;
const COLLIDER_2D_SHAPE: i32 = 103;
const COLLIDER_2D_OFFSET: i32 = 104;
const COLLIDER_2D_SIZE: i32 = 105;
const COLLIDER_2D_RADIUS: i32 = 106;
const COLLIDER_2D_HEIGHT: i32 = 107;
const COLLIDER_2D_TRIGGER: i32 = 108;
const COLLIDER_2D_FRICTION: i32 = 109;
const COLLIDER_2D_BOUNCINESS: i32 = 110;
// A Rigidbody 2D's; turning is in radians, as Box2D has it:
const BODY_2D_HAS: i32 = 120;
const BODY_2D_ADD: i32 = 121;
const BODY_2D_REMOVE: i32 = 122;
const BODY_2D_MASS: i32 = 123;
const BODY_2D_KINEMATIC: i32 = 124;
const BODY_2D_GRAVITY_SCALE: i32 = 125;
const BODY_2D_DRAG: i32 = 126;
const BODY_2D_ANGULAR_DRAG: i32 = 127;
const BODY_2D_FIXED_ROTATION: i32 = 128;
const BODY_2D_CONTINUOUS: i32 = 129;
const BODY_2D_VELOCITY: i32 = 130;
const BODY_2D_ANGULAR_VELOCITY: i32 = 131;
const BODY_2D_AWAKE: i32 = 132;
// The 2D world's:
const GRAVITY_2D: i32 = 160;

/// `physics_push` kinds, as Unity's ForceMode for a force, then torques.
const PUSH_FORCE: i32 = 0;
const PUSH_ACCELERATION: i32 = 1;
const PUSH_IMPULSE: i32 = 2;
const PUSH_VELOCITY_CHANGE: i32 = 3;
const PUSH_TORQUE: i32 = 4;
const PUSH_ANGULAR_IMPULSE: i32 = 5;
// And in 2D, a torque being the amount's X:
const PUSH_2D_FORCE: i32 = 10;
const PUSH_2D_IMPULSE: i32 = 11;
const PUSH_2D_TORQUE: i32 = 12;
const PUSH_2D_ANGULAR_IMPULSE: i32 = 13;

/// Query flags: 1 counts triggers too, and 2 asks 2D physics, in the XY plane.
const QUERY_TRIGGERS: i32 = 1;
const QUERY_2D: i32 = 2;

/// Where a ray met a collider, as C# reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RaycastHitData {
    pub obj: u64,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub distance: f32,
    /// The plant whose trunk it met, its number and one; 0 for none.
    pub plant: u32,
}

const _: () = assert!(size_of::<RaycastHitData>() == 40, "RaycastHitData in Voxear.Core is 40 bytes");

/// How a character's move went, as C# reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CharacterMoveData {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub ground_normal: [f32; 3],
    /// 0 on ground it stands on, 1 on a slope too steep, 2 against
    /// something that does not hold it up, 3 in the air.
    pub ground: u32,
}

const _: () = assert!(size_of::<CharacterMoveData>() == 40, "CharacterMoveData in Voxear.Core is 40 bytes");

/// A contact or trigger crossing, for each of the Objs in it: `kind` is 0
/// when a collision began, 1 when it ended, 2 when a trigger was entered,
/// and 3 when it was left, and 4 to 7 the same in 2D. The normal points
/// from `b` towards `a`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsEventData {
    pub a: u64,
    pub b: u64,
    pub kind: u32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub _pad: u32,
}

const _: () = assert!(size_of::<PhysicsEventData>() == 48, "PhysicsEventData in Voxear.Core is 48 bytes");

impl PhysicsEventData {
    /// The event as each Obj in it is told: once with it as `a`, once as `b`.
    pub fn both(event: &PhysicsEvent) -> [Self; 2] {
        let kind = match (event.trigger, event.began) {
            (false, true) => 0,
            (false, false) => 1,
            (true, true) => 2,
            (true, false) => 3,
        };
        // Jolt's normal points from the first body to the second.
        let told = |a: Entity, b: Entity, normal: Vec3| Self {
            a: a.to_bits(),
            b: b.to_bits(),
            kind,
            point: event.point.to_array(),
            normal: normal.to_array(),
            _pad: 0,
        };
        [told(event.a, event.b, -event.normal), told(event.b, event.a, event.normal)]
    }

    /// A 2D event as each Obj in it is told, in the XY plane.
    pub fn both_2d(event: &PhysicsEvent2D) -> [Self; 2] {
        let flat = PhysicsEvent {
            began: event.began,
            trigger: event.trigger,
            a: event.a,
            b: event.b,
            point: event.point.extend(0.0),
            normal: event.normal.extend(0.0),
        };
        Self::both(&flat).map(|told| Self { kind: told.kind + 4, ..told })
    }
}

/// The world's 2D physics scene, made if need be, ready as [`with_scene`]'s.
fn with_scene_2d<R>(world: &mut World, f: impl FnOnce(&mut World, &mut PhysicsScene2D) -> R) -> R {
    let mut scene = world.remove_resource::<PhysicsScene2D>().unwrap_or_default();
    scene.ready(world);
    let result = f(world, &mut scene);
    world.insert_resource(scene);
    result
}

fn flat(value: Vec2) -> [f32; 4] {
    [value.x, value.y, 0.0, 0.0]
}

/// The world's physics scene, made if need be, with its bodies where
/// their Objs are when it has not stepped yet.
fn with_scene<R>(world: &mut World, f: impl FnOnce(&mut World, &mut PhysicsScene) -> R) -> R {
    let mut scene = world.remove_resource::<PhysicsScene>().unwrap_or_default();
    scene.ready(world);
    let result = f(world, &mut scene);
    world.insert_resource(scene);
    result
}

fn flag(on: bool) -> [f32; 4] {
    [if on { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]
}

fn number(value: f32) -> [f32; 4] {
    [value, 0.0, 0.0, 0.0]
}

fn vector(value: Vec3) -> [f32; 4] {
    [value.x, value.y, value.z, 0.0]
}

fn three(value: [f32; 4]) -> [f32; 3] {
    [value[0], value[1], value[2]]
}

/// Reads or writes a physics value of an Obj (see the numbers above): 1
/// when it did, 0 when the Obj has no such component, or is gone.
pub(crate) unsafe extern "C" fn physics_value(entity: u64, what: i32, set: i32, value: *mut [f32; 4]) -> i32 {
    if value.is_null() {
        return 0;
    }
    let value = unsafe { &mut *value };
    let entity = Entity::from_bits(entity);
    let done = with_world(|world| {
        Some(match set != 0 {
            true => write(world, entity, what, *value),
            false => read(world, entity, what).map(|read| *value = read).is_some(),
        })
    });
    i32::from(done == Some(true))
}

fn read(world: &mut World, entity: Entity, what: i32) -> Option<[f32; 4]> {
    if what == GRAVITY {
        return Some(vector(world.resource::<PhysicsScene>().map_or(Vec3::new(0.0, -9.81, 0.0), |s| s.gravity())));
    }
    if what == GRAVITY_2D {
        return Some(flat(world.resource::<PhysicsScene2D>().map_or(Vec2::new(0.0, -9.81), |s| s.gravity())));
    }
    if !world.is_alive(entity) {
        return None;
    }
    if what >= vehicles::VEHICLE_HAS {
        return vehicles::read(world, entity, what);
    }
    if what >= COLLIDER_2D_HAS {
        return read_2d(world, entity, what);
    }
    if (JOINT_HAS..=JOINT_LIMITS).contains(&what) {
        let joint = world.get::<Joint>(entity);
        return Some(match what {
            JOINT_HAS => flag(joint.is_some()),
            JOINT_TYPE => {
                let kind = joint?.kind;
                number(JointType::ALL.iter().position(|k| *k == kind).unwrap_or_default() as f32)
            }
            JOINT_ANCHOR => vector(joint?.anchor.into()),
            JOINT_AXIS => vector(joint?.axis.into()),
            JOINT_LIMITS => match joint?.limits {
                Some([least, most]) => [least, most, 1.0, 0.0],
                None => [0.0; 4],
            },
            _ => return None,
        });
    }
    let collider = || world.get::<Collider>(entity);
    let body = || world.get::<Rigidbody>(entity);
    let character = || world.get::<CharacterController>(entity);
    let buoyancy = || world.get::<Buoyancy>(entity);
    Some(match what {
        COLLIDER_HAS => flag(collider().is_some()),
        COLLIDER_SHAPE => {
            let shape = collider()?.shape;
            number(ColliderShape::ALL.iter().position(|s| *s == shape).unwrap_or_default() as f32)
        }
        COLLIDER_CENTER => vector(collider()?.center.into()),
        COLLIDER_SIZE => vector(collider()?.size.into()),
        COLLIDER_RADIUS => number(collider()?.radius),
        COLLIDER_HEIGHT => number(collider()?.height),
        COLLIDER_TRIGGER => flag(collider()?.trigger),
        COLLIDER_FRICTION => number(collider()?.friction),
        COLLIDER_BOUNCINESS => number(collider()?.bounciness),
        COLLIDER_CONVEX => flag(collider()?.convex),
        COLLIDER_ACOUSTIC => {
            let acoustic = collider()?.acoustic;
            number(Acoustic::ALL.iter().position(|a| *a == acoustic).unwrap_or_default() as f32)
        }
        BUOYANCY_HAS => flag(buoyancy().is_some()),
        BUOYANCY_SUBMERGED => number(buoyancy()?.submerged),
        BUOYANCY_DRAG => number(buoyancy()?.drag),
        BUOYANCY_ANGULAR_DRAG => number(buoyancy()?.angular_drag),
        BODY_HAS => flag(body().is_some()),
        BODY_MASS => number(body()?.mass),
        BODY_KINEMATIC => flag(body()?.kinematic),
        BODY_GRAVITY => flag(body()?.gravity),
        BODY_DRAG => number(body()?.drag),
        BODY_ANGULAR_DRAG => number(body()?.angular_drag),
        BODY_CONTINUOUS => flag(body()?.continuous),
        BODY_VELOCITY | BODY_ANGULAR_VELOCITY | BODY_AWAKE => {
            body()?;
            // A body physics has not made yet is still, and awake.
            let Some(scene) = world.resource::<PhysicsScene>() else {
                return Some(if what == BODY_AWAKE { flag(true) } else { vector(Vec3::ZERO) });
            };
            let Some(id) = scene.body(entity) else {
                return Some(if what == BODY_AWAKE { flag(true) } else { vector(Vec3::ZERO) });
            };
            let (linear, angular) = scene.world().velocity(id);
            match what {
                BODY_VELOCITY => vector(linear),
                BODY_ANGULAR_VELOCITY => vector(angular),
                _ => flag(scene.world().is_active(id)),
            }
        }
        CHARACTER_HAS => flag(character().is_some()),
        CHARACTER_HEIGHT => number(character()?.height),
        CHARACTER_RADIUS => number(character()?.radius),
        CHARACTER_SLOPE_LIMIT => number(character()?.slope_limit),
        CHARACTER_STEP_OFFSET => number(character()?.step_offset),
        CHARACTER_GROUNDED | CHARACTER_VELOCITY | CHARACTER_GROUND_NORMAL => {
            character()?;
            let state = world.resource::<PhysicsScene>().and_then(|scene| scene.character_state(entity));
            match what {
                CHARACTER_GROUNDED => flag(state.is_some_and(|state| state.ground == Ground::Ground)),
                CHARACTER_VELOCITY => vector(state.map_or(Vec3::ZERO, |state| state.velocity)),
                _ => vector(state.map_or(Vec3::Y, |state| state.ground_normal)),
            }
        }
        _ => return None,
    })
}

fn read_2d(world: &World, entity: Entity, what: i32) -> Option<[f32; 4]> {
    let collider = || world.get::<Collider2D>(entity);
    let body = || world.get::<Rigidbody2D>(entity);
    Some(match what {
        COLLIDER_2D_HAS => flag(collider().is_some()),
        COLLIDER_2D_SHAPE => {
            let shape = collider()?.shape;
            number(ColliderShape2D::ALL.iter().position(|s| *s == shape).unwrap_or_default() as f32)
        }
        COLLIDER_2D_OFFSET => flat(collider()?.offset.into()),
        COLLIDER_2D_SIZE => flat(collider()?.size.into()),
        COLLIDER_2D_RADIUS => number(collider()?.radius),
        COLLIDER_2D_HEIGHT => number(collider()?.height),
        COLLIDER_2D_TRIGGER => flag(collider()?.trigger),
        COLLIDER_2D_FRICTION => number(collider()?.friction),
        COLLIDER_2D_BOUNCINESS => number(collider()?.bounciness),
        BODY_2D_HAS => flag(body().is_some()),
        BODY_2D_MASS => number(body()?.mass),
        BODY_2D_KINEMATIC => flag(body()?.kinematic),
        BODY_2D_GRAVITY_SCALE => number(body()?.gravity_scale),
        BODY_2D_DRAG => number(body()?.drag),
        BODY_2D_ANGULAR_DRAG => number(body()?.angular_drag),
        BODY_2D_FIXED_ROTATION => flag(body()?.fixed_rotation),
        BODY_2D_CONTINUOUS => flag(body()?.continuous),
        BODY_2D_VELOCITY | BODY_2D_ANGULAR_VELOCITY | BODY_2D_AWAKE => {
            body()?;
            let found = world.resource::<PhysicsScene2D>().and_then(|scene| Some((scene, scene.body(entity)?)));
            // A body physics has not made yet is still, and awake.
            let Some((scene, id)) = found else {
                return Some(if what == BODY_2D_AWAKE { flag(true) } else { flat(Vec2::ZERO) });
            };
            let (linear, angular) = scene.world().velocity(id);
            match what {
                BODY_2D_VELOCITY => flat(linear),
                BODY_2D_ANGULAR_VELOCITY => number(angular),
                _ => flag(scene.world().is_awake(id)),
            }
        }
        _ => return None,
    })
}

fn write_2d(world: &mut World, entity: Entity, what: i32, value: [f32; 4]) -> bool {
    let on = value[0] != 0.0;
    let n = value[0];
    let two = Vec2::new(value[0], value[1]);
    match what {
        COLLIDER_2D_ADD => {
            if !world.has::<Collider2D>(entity) {
                let shape = ColliderShape2D::ALL.get(n as usize).copied().unwrap_or_default();
                world.insert(entity, Collider2D { shape, ..Collider2D::default() });
            }
            true
        }
        COLLIDER_2D_REMOVE => world.remove::<Collider2D>(entity).is_some(),
        BODY_2D_ADD => {
            if !world.has::<Rigidbody2D>(entity) {
                world.insert(entity, Rigidbody2D::default());
            }
            true
        }
        BODY_2D_REMOVE => world.remove::<Rigidbody2D>(entity).is_some(),
        COLLIDER_2D_SHAPE => change(world, entity, |c: &mut Collider2D| {
            c.shape = ColliderShape2D::ALL.get(n as usize).copied().unwrap_or_default();
        }),
        COLLIDER_2D_OFFSET => change(world, entity, |c: &mut Collider2D| c.offset = two.into()),
        COLLIDER_2D_SIZE => change(world, entity, |c: &mut Collider2D| c.size = two.max(Vec2::splat(0.001)).into()),
        COLLIDER_2D_RADIUS => change(world, entity, |c: &mut Collider2D| c.radius = n.max(0.001)),
        COLLIDER_2D_HEIGHT => change(world, entity, |c: &mut Collider2D| c.height = n.max(0.001)),
        COLLIDER_2D_TRIGGER => change(world, entity, |c: &mut Collider2D| c.trigger = on),
        COLLIDER_2D_FRICTION => change(world, entity, |c: &mut Collider2D| c.friction = n.max(0.0)),
        COLLIDER_2D_BOUNCINESS => change(world, entity, |c: &mut Collider2D| c.bounciness = n.clamp(0.0, 1.0)),
        BODY_2D_MASS => change(world, entity, |b: &mut Rigidbody2D| b.mass = n.max(0.001)),
        BODY_2D_KINEMATIC => change(world, entity, |b: &mut Rigidbody2D| b.kinematic = on),
        BODY_2D_GRAVITY_SCALE => change(world, entity, |b: &mut Rigidbody2D| b.gravity_scale = n),
        BODY_2D_DRAG => change(world, entity, |b: &mut Rigidbody2D| b.drag = n.max(0.0)),
        BODY_2D_ANGULAR_DRAG => change(world, entity, |b: &mut Rigidbody2D| b.angular_drag = n.max(0.0)),
        BODY_2D_FIXED_ROTATION => change(world, entity, |b: &mut Rigidbody2D| b.fixed_rotation = on),
        BODY_2D_CONTINUOUS => change(world, entity, |b: &mut Rigidbody2D| b.continuous = on),
        BODY_2D_VELOCITY | BODY_2D_ANGULAR_VELOCITY | BODY_2D_AWAKE => {
            if !world.has::<Rigidbody2D>(entity) {
                return false;
            }
            with_scene_2d(world, |world, scene| {
                let Some(id) = scene.body_now(world, entity) else { return false };
                let (linear, angular) = scene.world().velocity(id);
                match what {
                    BODY_2D_VELOCITY => scene.world_mut().set_velocity(id, two, angular),
                    BODY_2D_ANGULAR_VELOCITY => scene.world_mut().set_velocity(id, linear, n),
                    _ => scene.world_mut().set_awake(id, on),
                }
                true
            })
        }
        _ => false,
    }
}

fn write(world: &mut World, entity: Entity, what: i32, value: [f32; 4]) -> bool {
    if what == GRAVITY {
        with_scene(world, |_, scene| scene.set_gravity(Vec3::from_array(three(value))));
        return true;
    }
    if what == GRAVITY_2D {
        with_scene_2d(world, |_, scene| scene.set_gravity(Vec2::new(value[0], value[1])));
        return true;
    }
    if !world.is_alive(entity) {
        return false;
    }
    if what >= vehicles::VEHICLE_HAS {
        return vehicles::write(world, entity, what, value);
    }
    if what >= COLLIDER_2D_HAS {
        return write_2d(world, entity, what, value);
    }
    let on = value[0] != 0.0;
    let n = value[0];
    match what {
        JOINT_ADD => {
            if !world.has::<Joint>(entity) {
                let kind = JointType::ALL.get(n as usize).copied().unwrap_or_default();
                world.insert(entity, Joint { kind, ..Joint::default() });
            }
            true
        }
        JOINT_REMOVE => world.remove::<Joint>(entity).is_some(),
        JOINT_TYPE => change(world, entity, |joint: &mut Joint| {
            joint.kind = JointType::ALL.get(n as usize).copied().unwrap_or_default();
        }),
        JOINT_ANCHOR => change(world, entity, |joint: &mut Joint| joint.anchor = three(value)),
        JOINT_AXIS => change(world, entity, |joint: &mut Joint| joint.axis = three(value)),
        JOINT_LIMITS => change(world, entity, |joint: &mut Joint| {
            joint.limits = (value[2] != 0.0).then_some([value[0], value[1]]);
        }),
        COLLIDER_ADD => {
            if !world.has::<Collider>(entity) {
                let shape = ColliderShape::ALL.get(n as usize).copied().unwrap_or_default();
                world.insert(entity, Collider { shape, ..Collider::default() });
            }
            true
        }
        COLLIDER_REMOVE => world.remove::<Collider>(entity).is_some(),
        BODY_ADD => {
            if !world.has::<Rigidbody>(entity) {
                world.insert(entity, Rigidbody::default());
            }
            true
        }
        BODY_REMOVE => world.remove::<Rigidbody>(entity).is_some(),
        BUOYANCY_ADD => {
            if !world.has::<Buoyancy>(entity) {
                world.insert(entity, Buoyancy::default());
            }
            true
        }
        BUOYANCY_REMOVE => world.remove::<Buoyancy>(entity).is_some(),
        CHARACTER_ADD => {
            if !world.has::<CharacterController>(entity) {
                world.insert(entity, CharacterController::default());
            }
            true
        }
        CHARACTER_REMOVE => world.remove::<CharacterController>(entity).is_some(),
        COLLIDER_SHAPE => change(world, entity, |c: &mut Collider| {
            c.shape = ColliderShape::ALL.get(n as usize).copied().unwrap_or_default();
        }),
        COLLIDER_CENTER => change(world, entity, |c: &mut Collider| c.center = three(value)),
        COLLIDER_SIZE => change(world, entity, |c: &mut Collider| c.size = three(value).map(|side| side.max(0.001))),
        COLLIDER_RADIUS => change(world, entity, |c: &mut Collider| c.radius = n.max(0.001)),
        COLLIDER_HEIGHT => change(world, entity, |c: &mut Collider| c.height = n.max(0.001)),
        COLLIDER_TRIGGER => change(world, entity, |c: &mut Collider| c.trigger = on),
        COLLIDER_FRICTION => change(world, entity, |c: &mut Collider| c.friction = n.max(0.0)),
        COLLIDER_BOUNCINESS => change(world, entity, |c: &mut Collider| c.bounciness = n.clamp(0.0, 1.0)),
        COLLIDER_CONVEX => change(world, entity, |c: &mut Collider| c.convex = on),
        COLLIDER_ACOUSTIC => change(world, entity, |c: &mut Collider| {
            c.acoustic = Acoustic::ALL.get(n as usize).copied().unwrap_or_default();
        }),
        BODY_MASS => change(world, entity, |b: &mut Rigidbody| b.mass = n.max(0.001)),
        BODY_KINEMATIC => change(world, entity, |b: &mut Rigidbody| b.kinematic = on),
        BODY_GRAVITY => change(world, entity, |b: &mut Rigidbody| b.gravity = on),
        BODY_DRAG => change(world, entity, |b: &mut Rigidbody| b.drag = n.max(0.0)),
        BUOYANCY_SUBMERGED => change(world, entity, |b: &mut Buoyancy| b.submerged = n.max(0.01)),
        BUOYANCY_DRAG => change(world, entity, |b: &mut Buoyancy| b.drag = n.max(0.0)),
        BUOYANCY_ANGULAR_DRAG => change(world, entity, |b: &mut Buoyancy| b.angular_drag = n.max(0.0)),
        BODY_ANGULAR_DRAG => change(world, entity, |b: &mut Rigidbody| b.angular_drag = n.max(0.0)),
        BODY_CONTINUOUS => change(world, entity, |b: &mut Rigidbody| b.continuous = on),
        BODY_VELOCITY | BODY_ANGULAR_VELOCITY | BODY_AWAKE => {
            if !world.has::<Rigidbody>(entity) {
                return false;
            }
            with_scene(world, |world, scene| {
                let Some(id) = scene.body_now(world, entity) else { return false };
                let (linear, angular) = scene.world().velocity(id);
                let given = Vec3::from_array(three(value));
                match what {
                    BODY_VELOCITY => scene.world_mut().set_velocity(id, given, angular),
                    BODY_ANGULAR_VELOCITY => scene.world_mut().set_velocity(id, linear, given),
                    _ => scene.world_mut().set_active(id, on),
                }
                true
            })
        }
        CHARACTER_HEIGHT => change(world, entity, |c: &mut CharacterController| c.height = n.max(c.radius * 2.0)),
        CHARACTER_RADIUS => {
            change(world, entity, |c: &mut CharacterController| c.radius = n.clamp(0.01, c.height * 0.5))
        }
        CHARACTER_SLOPE_LIMIT => {
            change(world, entity, |c: &mut CharacterController| c.slope_limit = n.clamp(0.0, 90.0))
        }
        CHARACTER_STEP_OFFSET => change(world, entity, |c: &mut CharacterController| c.step_offset = n.max(0.0)),
        _ => false,
    }
}

/// Changes an Obj's component in place: false when it has none.
fn change<T: voxear_ecs::Component>(world: &mut World, entity: Entity, f: impl FnOnce(&mut T)) -> bool {
    world.get_mut::<T>(entity).map(f).is_some()
}

/// The Obj an Obj's joint holds to, its entity, or 0 for the world; set to
/// `other` when `set`, none being 0. Returns 0 when the Obj has no joint,
/// and when set, 1 when it did.
pub(crate) unsafe extern "C" fn joint_connected(entity: u64, set: i32, other: u64) -> u64 {
    let entity = Entity::from_bits(entity);
    let answer = with_world(|world| {
        if set == 0 {
            let connected = world.get::<Joint>(entity)?.connected;
            return Some(connected.filter(|&obj| world.is_alive(obj)).map_or(0, Entity::to_bits));
        }
        let other = (other != 0).then(|| Entity::from_bits(other)).filter(|&obj| obj != entity && world.is_alive(obj));
        world.get_mut::<Joint>(entity)?.connected = other;
        Some(1)
    });
    answer.unwrap_or(0)
}

/// Pushes an Obj's Rigidbody (see the kinds above), at a point in the
/// world when `at` is given: 1 when it did, 0 when it has none.
pub(crate) unsafe extern "C" fn physics_push(
    entity: u64,
    kind: i32,
    amount: *const [f32; 3],
    at: *const [f32; 3],
) -> i32 {
    if amount.is_null() {
        return 0;
    }
    let amount = Vec3::from_array(unsafe { *amount });
    let at = (!at.is_null()).then(|| Vec3::from_array(unsafe { *at }));
    let entity = Entity::from_bits(entity);
    if kind >= PUSH_2D_FORCE {
        return i32::from(push_2d(entity, kind, amount, at).is_some());
    }
    let pushed = with_world(|world| {
        let mass = world.get::<Rigidbody>(entity)?.mass;
        let push = match kind {
            PUSH_FORCE => Push::Force(amount, at),
            PUSH_ACCELERATION => Push::Force(amount * mass, at),
            PUSH_IMPULSE => Push::Impulse(amount, at),
            PUSH_VELOCITY_CHANGE => Push::Impulse(amount * mass, at),
            PUSH_TORQUE => Push::Torque(amount),
            PUSH_ANGULAR_IMPULSE => Push::AngularImpulse(amount),
            _ => return None,
        };
        with_scene(world, |world, scene| {
            let id = scene.body_now(world, entity)?;
            scene.world_mut().push(id, push);
            Some(())
        })
    });
    i32::from(pushed.is_some())
}

fn push_2d(entity: Entity, kind: i32, amount: Vec3, at: Option<Vec3>) -> Option<()> {
    let (amount, at) = (amount.truncate(), at.map(Vec3::truncate));
    let push = match kind {
        PUSH_2D_FORCE => Push2D::Force(amount, at),
        PUSH_2D_IMPULSE => Push2D::Impulse(amount, at),
        PUSH_2D_TORQUE => Push2D::Torque(amount.x),
        PUSH_2D_ANGULAR_IMPULSE => Push2D::AngularImpulse(amount.x),
        _ => return None,
    };
    with_world(|world| {
        world.get::<Rigidbody2D>(entity)?;
        with_scene_2d(world, |world, scene| scene.push(world, entity, push).then_some(()))
    })
}

/// The first collider a ray meets within `distance` metres, on an Obj on
/// a layer in `mask`: 1 with the hit written, 0 when it meets none.
pub(crate) unsafe extern "C" fn physics_raycast(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    distance: f32,
    mask: u32,
    flags: i32,
    hit: *mut RaycastHitData,
) -> i32 {
    if origin.is_null() || direction.is_null() || hit.is_null() {
        return 0;
    }
    let (origin, direction) = unsafe { (Vec3::from_array(*origin), Vec3::from_array(*direction)) };
    let triggers = flags & QUERY_TRIGGERS != 0;
    let found = with_world(|world| match flags & QUERY_2D != 0 {
        false => with_scene(world, |_, scene| scene.raycast(origin, direction, distance, mask, triggers)),
        true => with_scene_2d(world, |_, scene| {
            let found = scene.raycast(origin.truncate(), direction.truncate(), distance, mask, triggers)?;
            Some(voxear_physics::scene::SceneHit {
                obj: found.obj,
                point: found.point.extend(0.0),
                normal: found.normal.extend(0.0),
                distance: found.distance,
                plant: None,
            })
        }),
    });
    let Some(found) = found else { return 0 };
    unsafe {
        *hit = RaycastHitData {
            obj: found.obj.to_bits(),
            point: found.point.to_array(),
            normal: found.normal.to_array(),
            distance: found.distance,
            plant: found.plant.map_or(0, |number| number + 1),
        };
    }
    1
}

/// The Objs on layers in `mask` whose colliders a sphere overlaps, or in
/// 2D a circle, as many as fit; returns how many there are, which may be
/// more.
pub(crate) unsafe extern "C" fn physics_overlap(
    center: *const [f32; 3],
    radius: f32,
    mask: u32,
    flags: i32,
    objs: *mut u64,
    capacity: i32,
) -> i32 {
    if center.is_null() {
        return 0;
    }
    let center = Vec3::from_array(unsafe { *center });
    let triggers = flags & QUERY_TRIGGERS != 0;
    let found = with_world(|world| {
        Some(match flags & QUERY_2D != 0 {
            false => with_scene(world, |_, scene| scene.overlap_sphere(center, radius, mask, triggers)),
            true => with_scene_2d(world, |_, scene| scene.overlap_circle(center.truncate(), radius, mask, triggers)),
        })
    });
    let found = found.unwrap_or_default();
    if !objs.is_null() {
        for (at, obj) in found.iter().take(capacity.max(0) as usize).enumerate() {
            unsafe { *objs.add(at) = obj.to_bits() };
        }
    }
    found.len() as i32
}

/// Moves an Obj's Character Controller by `motion` over `dt` seconds, as
/// Unity's CharacterController.Move does: 1 with how it went written, 0
/// when the Obj has none.
pub(crate) unsafe extern "C" fn character_move(
    entity: u64,
    motion: *const [f32; 3],
    dt: f32,
    state: *mut CharacterMoveData,
) -> i32 {
    if motion.is_null() || dt.is_nan() || dt <= 0.0 {
        return 0;
    }
    let velocity = Vec3::from_array(unsafe { *motion }) / dt;
    let entity = Entity::from_bits(entity);
    let moved = with_world(|world| with_scene(world, |world, scene| scene.move_character(world, entity, velocity, dt)));
    let Some(moved) = moved else { return 0 };
    if !state.is_null() {
        let ground = match moved.ground {
            Ground::Ground => 0,
            Ground::Steep => 1,
            Ground::Against => 2,
            Ground::InAir => 3,
        };
        unsafe {
            *state = CharacterMoveData {
                position: moved.position.to_array(),
                velocity: moved.velocity.to_array(),
                ground_normal: moved.ground_normal.to_array(),
                ground,
            };
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_floats_an_obj_and_changes_how_it_floats() {
        let mut world = World::new();
        let crate_ = world.spawn(());
        assert_eq!(read(&mut world, crate_, BUOYANCY_HAS), Some(flag(false)));
        assert!(write(&mut world, crate_, BUOYANCY_ADD, [0.0; 4]));
        assert_eq!(read(&mut world, crate_, BUOYANCY_SUBMERGED), Some(number(0.5)));
        assert!(write(&mut world, crate_, BUOYANCY_SUBMERGED, [0.8, 0.0, 0.0, 0.0]));
        assert!(write(&mut world, crate_, BUOYANCY_DRAG, [-1.0, 0.0, 0.0, 0.0]));
        let floating = *world.get::<Buoyancy>(crate_).unwrap();
        assert_eq!((floating.submerged, floating.drag), (0.8, 0.0), "no drag below none");
        assert!(write(&mut world, crate_, BUOYANCY_REMOVE, [0.0; 4]) && !world.has::<Buoyancy>(crate_));
    }

    #[test]
    fn each_obj_is_told_with_the_normal_towards_it() {
        let (a, b) = (Entity::from_bits(1), Entity::from_bits(2));
        let event = PhysicsEvent { began: true, trigger: false, a, b, point: Vec3::ZERO, normal: Vec3::NEG_Y };
        let [to_a, to_b] = PhysicsEventData::both(&event);
        assert_eq!((to_a.a, to_a.b, to_a.kind, to_a.normal), (1, 2, 0, [0.0, 1.0, 0.0]));
        assert_eq!((to_b.a, to_b.b, to_b.normal), (2, 1, [0.0, -1.0, 0.0]));
    }
}
