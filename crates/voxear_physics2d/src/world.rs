//! A Box2D world as Voxear uses it: bodies with one shape each, moved by
//! physics or by the game, the contacts and sensor crossings each step
//! found, and rays and circles that ask what is where. Box2D's ids are
//! plain values, so the world is too, and it goes away when dropped.

use std::{os::raw::c_void, sync::Mutex};

use boxdd_sys::ffi as b2;
use glam::Vec2;

/// A body, by Box2D's id for it.
#[derive(Clone, Copy, Debug)]
pub struct Body2D(b2::b2BodyId);

impl Body2D {
    /// The id as one number, the same for the same body.
    pub fn key(self) -> u64 {
        let id = self.0;
        (id.index1 as u32 as u64) | (u64::from(id.world0) << 32) | (u64::from(id.generation) << 48)
    }
}

impl PartialEq for Body2D {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Body2D {}

impl std::hash::Hash for Body2D {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// How a body moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion2D {
    Static,
    Kinematic,
    Dynamic,
}

/// A body's one shape, in the body's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape2D {
    Box {
        half: Vec2,
        center: Vec2,
    },
    Circle {
        radius: f32,
        center: Vec2,
    },
    /// Standing up: its two round ends `half` above and below its centre.
    Capsule {
        half: f32,
        radius: f32,
        center: Vec2,
    },
}

/// What a body is made as.
#[derive(Clone, Copy, Debug)]
pub struct BodyDesc2D {
    pub shape: Shape2D,
    pub motion: Motion2D,
    pub position: Vec2,
    /// Turned anticlockwise, in radians.
    pub angle: f32,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub sensor: bool,
    /// Never turns, as a platformer's character should not.
    pub fixed_rotation: bool,
    /// Checked along its whole path each step, so that it cannot pass through thin things.
    pub bullet: bool,
    pub user_data: u64,
}

impl BodyDesc2D {
    pub fn new(shape: Shape2D, motion: Motion2D) -> Self {
        Self {
            shape,
            motion,
            position: Vec2::ZERO,
            angle: 0.0,
            mass: 1.0,
            friction: 0.6,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.05,
            gravity_scale: 1.0,
            sensor: false,
            fixed_rotation: false,
            bullet: false,
            user_data: 0,
        }
    }
}

/// A push given to a body: a force spread over the next step, or an
/// impulse at once, through its middle or at a point in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Push2D {
    Force(Vec2, Option<Vec2>),
    Impulse(Vec2, Option<Vec2>),
    Torque(f32),
    AngularImpulse(f32),
}

/// Two bodies that began or stopped touching in the last step, or a
/// sensor and a body that entered or left it, by their user data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact2D {
    pub began: bool,
    pub sensor: bool,
    pub a: u64,
    pub b: u64,
    /// Where they touch, and which way from `a` to `b`; zero when it ended, or for a sensor.
    pub point: Vec2,
    pub normal: Vec2,
}

/// Where a ray met a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit2D {
    pub user_data: u64,
    pub point: Vec2,
    pub normal: Vec2,
    /// How far along the ray, as a share of its length.
    pub fraction: f32,
}

pub struct PhysicsWorld2D {
    id: b2::b2WorldId,
}

/// Box2D keeps its worlds in one table, which making or ending two worlds
/// at once on two threads would race over, as tests and the editor's
/// previews do. Each world is used by one thread at a time after that.
static WORLDS: Mutex<()> = Mutex::new(());

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self::new(Vec2::new(0.0, -9.81))
    }
}

impl Drop for PhysicsWorld2D {
    fn drop(&mut self) {
        let _table = WORLDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe { b2::b2DestroyWorld(self.id) };
    }
}

fn vec(v: Vec2) -> b2::b2Vec2 {
    b2::b2Vec2 { x: v.x, y: v.y }
}

fn glam(v: b2::b2Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

fn rot(angle: f32) -> b2::b2Rot {
    b2::b2Rot { c: angle.cos(), s: angle.sin() }
}

impl PhysicsWorld2D {
    pub fn new(gravity: Vec2) -> Self {
        let mut def = unsafe { b2::b2DefaultWorldDef() };
        def.gravity = vec(gravity);
        let _table = WORLDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Self { id: unsafe { b2::b2CreateWorld(&def) } }
    }

    pub fn set_gravity(&mut self, gravity: Vec2) {
        unsafe { b2::b2World_SetGravity(self.id, vec(gravity)) };
    }

    /// Moves everything on by `dt` seconds, in four sub-steps, as Box2D suggests.
    pub fn step(&mut self, dt: f32) {
        unsafe { b2::b2World_Step(self.id, dt, 4) };
    }

    pub fn add_body(&mut self, desc: &BodyDesc2D) -> Body2D {
        let mut def = unsafe { b2::b2DefaultBodyDef() };
        def.type_ = match desc.motion {
            Motion2D::Static => b2::b2BodyType_b2_staticBody,
            Motion2D::Kinematic => b2::b2BodyType_b2_kinematicBody,
            Motion2D::Dynamic => b2::b2BodyType_b2_dynamicBody,
        };
        def.position = vec(desc.position);
        def.rotation = rot(desc.angle);
        def.linearDamping = desc.linear_damping;
        def.angularDamping = desc.angular_damping;
        def.gravityScale = desc.gravity_scale;
        def.userData = desc.user_data as usize as *mut c_void;
        def.motionLocks.angularZ = desc.fixed_rotation;
        let body = unsafe { b2::b2CreateBody(self.id, &def) };

        let mut shape = unsafe { b2::b2DefaultShapeDef() };
        shape.userData = desc.user_data as usize as *mut c_void;
        shape.material.friction = desc.friction;
        shape.material.restitution = desc.restitution;
        shape.isSensor = desc.sensor;
        shape.enableSensorEvents = true;
        shape.enableContactEvents = true;
        unsafe {
            match desc.shape {
                Shape2D::Box { half, center } => {
                    let polygon = b2::b2MakeOffsetBox(half.x, half.y, vec(center), rot(0.0));
                    b2::b2CreatePolygonShape(body, &shape, &polygon);
                }
                Shape2D::Circle { radius, center } => {
                    let circle = b2::b2Circle { center: vec(center), radius };
                    b2::b2CreateCircleShape(body, &shape, &circle);
                }
                Shape2D::Capsule { half, radius, center } => {
                    let capsule = b2::b2Capsule {
                        center1: vec(center - Vec2::Y * half),
                        center2: vec(center + Vec2::Y * half),
                        radius,
                    };
                    b2::b2CreateCapsuleShape(body, &shape, &capsule);
                }
            }
            if desc.motion == Motion2D::Dynamic {
                // Box2D weighs a body by its shape's area; Voxear's are weighed in kilograms.
                let mut mass = b2::b2Body_GetMassData(body);
                let scale = desc.mass.max(1e-3) / mass.mass.max(1e-6);
                mass.mass *= scale;
                mass.rotationalInertia *= scale;
                b2::b2Body_SetMassData(body, mass);
            }
            b2::b2Body_SetBullet(body, desc.bullet);
        }
        Body2D(body)
    }

    pub fn remove_body(&mut self, body: Body2D) {
        if unsafe { b2::b2Body_IsValid(body.0) } {
            unsafe { b2::b2DestroyBody(body.0) };
        }
    }

    /// Where a body is, and how far it has turned anticlockwise, in radians.
    pub fn transform(&self, body: Body2D) -> (Vec2, f32) {
        let (position, rotation) = unsafe { (b2::b2Body_GetPosition(body.0), b2::b2Body_GetRotation(body.0)) };
        (glam(position), rotation.s.atan2(rotation.c))
    }

    /// Puts a body somewhere at once.
    pub fn set_transform(&mut self, body: Body2D, position: Vec2, angle: f32) {
        unsafe { b2::b2Body_SetTransform(body.0, vec(position), rot(angle)) };
    }

    /// Moves a kinematic body there over the next step, pushing what is in the way.
    pub fn move_kinematic(&mut self, body: Body2D, position: Vec2, angle: f32, dt: f32) {
        let target = b2::b2Transform { p: vec(position), q: rot(angle) };
        unsafe { b2::b2Body_SetTargetTransform(body.0, target, dt, true) };
    }

    pub fn velocity(&self, body: Body2D) -> (Vec2, f32) {
        unsafe { (glam(b2::b2Body_GetLinearVelocity(body.0)), b2::b2Body_GetAngularVelocity(body.0)) }
    }

    pub fn set_velocity(&mut self, body: Body2D, linear: Vec2, angular: f32) {
        unsafe {
            b2::b2Body_SetLinearVelocity(body.0, vec(linear));
            b2::b2Body_SetAngularVelocity(body.0, angular);
        }
    }

    pub fn push(&mut self, body: Body2D, push: Push2D) {
        unsafe {
            match push {
                Push2D::Force(force, None) => b2::b2Body_ApplyForceToCenter(body.0, vec(force), true),
                Push2D::Force(force, Some(at)) => b2::b2Body_ApplyForce(body.0, vec(force), vec(at), true),
                Push2D::Impulse(impulse, None) => b2::b2Body_ApplyLinearImpulseToCenter(body.0, vec(impulse), true),
                Push2D::Impulse(impulse, Some(at)) => {
                    b2::b2Body_ApplyLinearImpulse(body.0, vec(impulse), vec(at), true)
                }
                Push2D::Torque(torque) => b2::b2Body_ApplyTorque(body.0, torque, true),
                Push2D::AngularImpulse(impulse) => b2::b2Body_ApplyAngularImpulse(body.0, impulse, true),
            }
        }
    }

    pub fn is_awake(&self, body: Body2D) -> bool {
        unsafe { b2::b2Body_IsAwake(body.0) }
    }

    pub fn set_awake(&mut self, body: Body2D, awake: bool) {
        unsafe { b2::b2Body_SetAwake(body.0, awake) };
    }

    /// The first shape a ray from `origin` meets along `translation`, whose
    /// length is how far it reaches, passing over sensors unless `sensors`.
    pub fn raycast(&self, origin: Vec2, translation: Vec2, sensors: bool) -> Option<RayHit2D> {
        unsafe extern "C" fn met(
            shape: b2::b2ShapeId,
            point: b2::b2Pos,
            normal: b2::b2Vec2,
            fraction: f32,
            context: *mut c_void,
        ) -> f32 {
            let (nearest, sensors) = unsafe { &mut *context.cast::<(Option<RayHit2D>, bool)>() };
            if !*sensors && unsafe { b2::b2Shape_IsSensor(shape) } {
                return -1.0;
            }
            let user_data = unsafe { b2::b2Shape_GetUserData(shape) } as usize as u64;
            *nearest = Some(RayHit2D { user_data, point: glam(point), normal: glam(normal), fraction });
            // Clipped here, the ray goes on only for something nearer.
            fraction
        }
        let mut nearest: (Option<RayHit2D>, bool) = (None, sensors);
        let filter = unsafe { b2::b2DefaultQueryFilter() };
        let context = (&mut nearest as *mut (Option<RayHit2D>, bool)).cast();
        unsafe { b2::b2World_CastRay(self.id, vec(origin), vec(translation), filter, Some(met), context) };
        nearest.0
    }

    /// The user data of each shape a circle overlaps, sensors with them when `sensors`.
    pub fn overlap_circle(&self, center: Vec2, radius: f32, sensors: bool) -> Vec<u64> {
        unsafe extern "C" fn found(shape: b2::b2ShapeId, context: *mut c_void) -> bool {
            let (seen, sensors) = unsafe { &mut *context.cast::<(Vec<u64>, bool)>() };
            if *sensors || !unsafe { b2::b2Shape_IsSensor(shape) } {
                seen.push(unsafe { b2::b2Shape_GetUserData(shape) } as usize as u64);
            }
            true
        }
        let mut seen: (Vec<u64>, bool) = (Vec::new(), sensors);
        let point = vec(Vec2::ZERO);
        let filter = unsafe { b2::b2DefaultQueryFilter() };
        unsafe {
            let proxy = b2::b2MakeProxy(&point, 1, radius.max(1e-4));
            let context = (&mut seen as *mut (Vec<u64>, bool)).cast();
            b2::b2World_OverlapShape(self.id, vec(center), &proxy, filter, Some(found), context);
        }
        seen.0
    }

    /// The contacts and sensor crossings of the last step. An end whose
    /// shape was destroyed since, as a body made again, is left out: Box2D
    /// no longer knows whose it was.
    pub fn contacts(&self) -> Vec<Contact2D> {
        let mut found = Vec::new();
        let data = |shape: b2::b2ShapeId| unsafe { b2::b2Shape_GetUserData(shape) } as usize as u64;
        let alive = |shapes: [b2::b2ShapeId; 2]| shapes.into_iter().all(|shape| unsafe { b2::b2Shape_IsValid(shape) });
        unsafe {
            let touches = b2::b2World_GetContactEvents(self.id);
            for begin in slice(touches.beginEvents, touches.beginCount) {
                let (mut point, mut normal) = (Vec2::ZERO, Vec2::ZERO);
                if b2::b2Contact_IsValid(begin.contactId) {
                    let manifold = b2::b2Contact_GetData(begin.contactId).manifold;
                    normal = glam(manifold.normal);
                    if manifold.pointCount > 0 {
                        let body = b2::b2Shape_GetBody(begin.shapeIdA);
                        point = glam(b2::b2Body_GetPosition(body)) + glam(manifold.points[0].anchorA);
                    }
                }
                let (a, b) = (data(begin.shapeIdA), data(begin.shapeIdB));
                found.push(Contact2D { began: true, sensor: false, a, b, point, normal });
            }
            for end in slice(touches.endEvents, touches.endCount).iter().filter(|e| alive([e.shapeIdA, e.shapeIdB])) {
                let (a, b) = (data(end.shapeIdA), data(end.shapeIdB));
                found.push(Contact2D { began: false, sensor: false, a, b, point: Vec2::ZERO, normal: Vec2::ZERO });
            }
            let sensors = b2::b2World_GetSensorEvents(self.id);
            for begin in slice(sensors.beginEvents, sensors.beginCount) {
                let (a, b) = (data(begin.sensorShapeId), data(begin.visitorShapeId));
                found.push(Contact2D { began: true, sensor: true, a, b, point: Vec2::ZERO, normal: Vec2::ZERO });
            }
            let sensor_ends = slice(sensors.endEvents, sensors.endCount);
            for end in sensor_ends.iter().filter(|e| alive([e.sensorShapeId, e.visitorShapeId])) {
                let (a, b) = (data(end.sensorShapeId), data(end.visitorShapeId));
                found.push(Contact2D { began: false, sensor: true, a, b, point: Vec2::ZERO, normal: Vec2::ZERO });
            }
        }
        found
    }
}

/// Box2D's events, which last until the next step.
unsafe fn slice<'a, T>(start: *const T, count: i32) -> &'a [T] {
    if start.is_null() || count <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(start, count as usize) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_and_crate(world: &mut PhysicsWorld2D) -> (Body2D, Body2D) {
        let floor = Shape2D::Box { half: Vec2::new(20.0, 0.5), center: Vec2::ZERO };
        let mut desc = BodyDesc2D::new(floor, Motion2D::Static);
        desc.position = Vec2::new(0.0, -0.5);
        desc.user_data = 1;
        let ground = world.add_body(&desc);
        let mut desc = BodyDesc2D::new(Shape2D::Box { half: Vec2::splat(0.5), center: Vec2::ZERO }, Motion2D::Dynamic);
        desc.position = Vec2::new(0.0, 4.0);
        desc.mass = 5.0;
        desc.user_data = 2;
        (ground, world.add_body(&desc))
    }

    #[test]
    fn a_crate_falls_lands_and_is_told_of() {
        let mut world = PhysicsWorld2D::default();
        let (_, crate_) = ground_and_crate(&mut world);
        let (mut began, mut ended) = (None, false);
        for _ in 0..300 {
            world.step(1.0 / 60.0);
            let contacts = world.contacts();
            began = began.or(contacts.iter().copied().find(|c| c.began && !c.sensor));
            ended |= contacts.iter().any(|c| !c.began);
        }
        assert!(!world.is_awake(crate_), "at rest, it sleeps");
        assert!(!ended, "and still touches the ground, asleep");
        let (position, _) = world.transform(crate_);
        assert!((position.y - 0.5).abs() < 0.02, "it rests on the ground: {position}");
        let began = began.expect("the landing was told of");
        assert_eq!((began.a, began.b), (1, 2));
        assert!(began.normal.y > 0.99, "from the ground up to the crate: {began:?}");
        assert!((unsafe { b2::b2Body_GetMass(crate_.0) } - 5.0).abs() < 1e-4, "weighed in kilograms");
    }

    #[test]
    fn rays_and_circles_find_what_is_where() {
        let mut world = PhysicsWorld2D::default();
        ground_and_crate(&mut world);
        let hit = world.raycast(Vec2::new(0.0, 10.0), Vec2::new(0.0, -20.0), false).expect("a hit");
        assert_eq!(hit.user_data, 2);
        assert!((hit.point.y - 4.5).abs() < 0.01 && hit.normal.y > 0.99, "{hit:?}");
        let mut found = world.overlap_circle(Vec2::new(0.0, 0.0), 0.2, false);
        found.sort();
        assert_eq!(found, [1]);
        assert!(world.overlap_circle(Vec2::new(5.0, 10.0), 0.5, false).is_empty());
    }

    #[test]
    fn a_sensor_notices_what_falls_through_it() {
        let mut world = PhysicsWorld2D::default();
        ground_and_crate(&mut world);
        let zone = Shape2D::Box { half: Vec2::new(5.0, 0.25), center: Vec2::ZERO };
        let mut desc = BodyDesc2D::new(zone, Motion2D::Static);
        desc.position = Vec2::new(0.0, 2.0);
        desc.sensor = true;
        desc.user_data = 3;
        world.add_body(&desc);
        let mut crossings = Vec::new();
        for _ in 0..120 {
            world.step(1.0 / 60.0);
            crossings.extend(world.contacts().into_iter().filter(|c| c.sensor).map(|c| (c.began, c.a, c.b)));
        }
        assert_eq!(crossings, [(true, 3, 2), (false, 3, 2)]);
    }
}
