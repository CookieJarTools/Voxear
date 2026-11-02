//! 3D physics on Jolt Physics, which is vendored in `jolt/` (MIT, see its
//! LICENSE) and built with this crate behind a small C shim of Voxear's own
//! (`shim/`), so nothing needs installing beyond a C++ compiler.
//!
//! A [`PhysicsWorld`] holds shapes, bodies, characters and joints:
//!
//! - **Shapes** are boxes, spheres, capsules, convex hulls and triangle
//!   meshes, each placed, turned and scaled as a collider on its Obj is.
//! - **Bodies** are static, kinematic (moved by the game) or dynamic (moved
//!   by physics), and may be sensors, which notice what enters them
//!   without pushing it. Each keeps a number of the game's, such as its
//!   Obj's entity.
//! - **Characters** walk: a capsule moved by velocity, sliding along walls,
//!   climbing steps and standing on slopes, as Unity's CharacterController.
//! - **Joints** hold two bodies together: fixed, a point both turn about, a
//!   hinge, or a distance.
//!
//! [`PhysicsWorld::step`] moves everything on; contacts that began or ended
//! come after, from [`PhysicsWorld::take_contacts`]. Ray casts and point
//! overlaps ask what is where. Y is up, and units are metres, kilograms and
//! seconds.

mod ffi;

use glam::{Quat, Vec3};

/// A shape, kept by its world until released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId(u32);

/// A body, by Jolt's id for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CharacterId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JointId(u32);

/// How a body moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    /// Never moves, as the ground and walls.
    Static,
    /// Moved by the game, pushing what is in its way, as a lift.
    Kinematic,
    /// Moved by physics: falls, is pushed and tumbles.
    #[default]
    Dynamic,
}

/// What a body is made with.
#[derive(Clone, Copy, Debug)]
pub struct BodyDesc {
    pub shape: ShapeId,
    pub position: Vec3,
    pub rotation: Quat,
    pub motion: Motion,
    /// In kilograms; 0 works it out from the shape, as water weighs.
    pub mass: f32,
    pub friction: f32,
    /// How much it bounces, from 0 to 1.
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// How strongly gravity pulls it: 0 floats, 1 falls as anything does.
    pub gravity_factor: f32,
    /// Notices what enters it without pushing it, as a trigger.
    pub sensor: bool,
    /// Moves by sweeping its shape, so a fast body cannot pass through a
    /// thin one between steps.
    pub continuous: bool,
    /// A number of the game's, such as its Obj's entity.
    pub user_data: u64,
}

impl BodyDesc {
    pub fn new(shape: ShapeId, motion: Motion) -> Self {
        Self {
            shape,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            motion,
            mass: 0.0,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            gravity_factor: 1.0,
            sensor: false,
            continuous: false,
            user_data: 0,
        }
    }

    pub fn at(mut self, position: Vec3, rotation: Quat) -> Self {
        (self.position, self.rotation) = (position, rotation);
        self
    }
}

/// A push given to a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Push {
    /// Spread over the next step, in newtons, at its centre or a point.
    Force(Vec3, Option<Vec3>),
    /// At once, in newton-seconds.
    Impulse(Vec3, Option<Vec3>),
    Torque(Vec3),
    AngularImpulse(Vec3),
}

/// Where a ray met a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub body: BodyId,
    /// How far along the ray, as a share of its length.
    pub fraction: f32,
    pub point: Vec3,
    pub normal: Vec3,
}

/// Two bodies that began or stopped touching in the last step. A body
/// pair touching in several places is reported for each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub began: bool,
    pub a: BodyId,
    pub b: BodyId,
    /// Where they touch and which way `b` pushes `a`; zero when it ended.
    pub point: Vec3,
    pub normal: Vec3,
}

/// What a character is made as.
#[derive(Clone, Copy, Debug)]
pub struct CharacterDesc {
    /// Where its feet are.
    pub position: Vec3,
    pub height: f32,
    pub radius: f32,
    /// The steepest slope it stands on, in degrees.
    pub max_slope: f32,
    /// The highest step it climbs.
    pub step_height: f32,
    /// What it pushes other bodies with, in kilograms.
    pub mass: f32,
}

impl Default for CharacterDesc {
    fn default() -> Self {
        Self { position: Vec3::ZERO, height: 1.8, radius: 0.3, max_slope: 45.0, step_height: 0.3, mass: 70.0 }
    }
}

/// What a character stands on after it moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// On ground it can stand on.
    Ground,
    /// On a slope too steep to stand on, which it slides down.
    Steep,
    /// Against something, but not held up by it.
    Against,
    InAir,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub ground: Ground,
    pub ground_normal: Vec3,
}

/// How a joint holds two bodies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointKind {
    /// As if glued where they are now.
    Fixed,
    /// Both turn freely about a point in the world.
    Point(Vec3),
    /// Both turn about an axis through a point, within limits in radians
    /// when given, as a door.
    Hinge { point: Vec3, axis: Vec3, limits: Option<(f32, f32)> },
    /// Their centres stay within a range of distances, or as far apart as
    /// they are now.
    Distance(Option<(f32, f32)>),
}

/// The largest number of bodies a world holds by default.
pub const MAX_BODIES: u32 = 65_536;

/// A physics world on Jolt: see the crate's documentation.
pub struct PhysicsWorld {
    world: *mut ffi::VxWorld,
}

// The world is used from one thread at a time; Jolt's own jobs run inside a step.
unsafe impl Send for PhysicsWorld {}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new(MAX_BODIES)
    }
}

impl Drop for PhysicsWorld {
    fn drop(&mut self) {
        unsafe { ffi::vx_world_destroy(self.world) };
    }
}

impl PhysicsWorld {
    /// A world for up to `max_bodies` bodies, with Earth's gravity.
    pub fn new(max_bodies: u32) -> Self {
        let world = unsafe { ffi::vx_world_create(max_bodies) };
        assert!(!world.is_null(), "Jolt made no world");
        let mut made = Self { world };
        made.set_gravity(Vec3::new(0.0, -9.81, 0.0));
        made
    }

    pub fn set_gravity(&mut self, gravity: Vec3) {
        unsafe { ffi::vx_world_set_gravity(self.world, gravity.x, gravity.y, gravity.z) };
    }

    /// Moves everything on by `dt` seconds, in `steps` collision steps:
    /// one is enough at 60 a second, and more keep a slower step stable.
    pub fn step(&mut self, dt: f32, steps: u32) {
        unsafe { ffi::vx_world_step(self.world, dt, steps.max(1) as i32) };
    }

    pub fn box_shape(&mut self, half_extents: Vec3) -> ShapeId {
        let h = half_extents.max(Vec3::splat(1e-3));
        ShapeId(unsafe { ffi::vx_shape_box(self.world, h.x, h.y, h.z) })
    }

    pub fn sphere_shape(&mut self, radius: f32) -> ShapeId {
        ShapeId(unsafe { ffi::vx_shape_sphere(self.world, radius.max(1e-3)) })
    }

    /// A capsule standing up, its middle at the origin: `half_height` of
    /// straight side above and below, and a half sphere on each end.
    pub fn capsule_shape(&mut self, half_height: f32, radius: f32) -> ShapeId {
        ShapeId(unsafe { ffi::vx_shape_capsule(self.world, half_height.max(1e-3), radius.max(1e-3)) })
    }

    /// The smallest convex shape around the points, or `None` for too few.
    pub fn convex_hull_shape(&mut self, points: &[Vec3]) -> Option<ShapeId> {
        let flat: Vec<f32> = points.iter().flat_map(|p| p.to_array()).collect();
        let shape = unsafe { ffi::vx_shape_convex_hull(self.world, flat.as_ptr(), points.len() as u32) };
        (shape != 0).then_some(ShapeId(shape))
    }

    /// A triangle mesh, for what does not move: its vertices, and three
    /// indices a triangle.
    pub fn mesh_shape(&mut self, vertices: &[Vec3], indices: &[u32]) -> Option<ShapeId> {
        let flat: Vec<f32> = vertices.iter().flat_map(|p| p.to_array()).collect();
        let triangles = (indices.len() / 3) as u32;
        let shape = unsafe {
            ffi::vx_shape_mesh(self.world, flat.as_ptr(), vertices.len() as u32, indices.as_ptr(), triangles)
        };
        (shape != 0).then_some(ShapeId(shape))
    }

    /// A shape moved, turned and scaled from where it was made.
    pub fn placed_shape(&mut self, shape: ShapeId, offset: Vec3, rotation: Quat, scale: Vec3) -> ShapeId {
        let (offset, rotation, scale) = (offset.to_array(), rotation.to_array(), scale.to_array());
        let placed =
            unsafe { ffi::vx_shape_placed(self.world, shape.0, offset.as_ptr(), rotation.as_ptr(), scale.as_ptr()) };
        if placed == 0 { shape } else { ShapeId(placed) }
    }

    /// Lets go of a shape; bodies made with it keep it.
    pub fn release_shape(&mut self, shape: ShapeId) {
        unsafe { ffi::vx_shape_release(self.world, shape.0) };
    }

    /// Adds a body, or `None` when the world is full or the shape is gone.
    pub fn add_body(&mut self, desc: &BodyDesc) -> Option<BodyId> {
        let raw = ffi::VxBodyDesc {
            shape: desc.shape.0,
            position: desc.position.to_array(),
            rotation: desc.rotation.to_array(),
            motion: match desc.motion {
                Motion::Static => 0,
                Motion::Kinematic => 1,
                Motion::Dynamic => 2,
            },
            mass: desc.mass,
            friction: desc.friction,
            restitution: desc.restitution,
            linear_damping: desc.linear_damping,
            angular_damping: desc.angular_damping,
            gravity_factor: desc.gravity_factor,
            sensor: u32::from(desc.sensor),
            continuous: u32::from(desc.continuous),
            user_data: desc.user_data,
        };
        let body = unsafe { ffi::vx_body_add(self.world, &raw) };
        (body != u32::MAX).then_some(BodyId(body))
    }

    pub fn remove_body(&mut self, body: BodyId) {
        unsafe { ffi::vx_body_remove(self.world, body.0) };
    }

    pub fn transform(&self, body: BodyId) -> (Vec3, Quat) {
        let (mut position, mut rotation) = ([0.0f32; 3], [0.0f32; 4]);
        unsafe { ffi::vx_body_get_transform(self.world, body.0, position.as_mut_ptr(), rotation.as_mut_ptr()) };
        (Vec3::from_array(position), Quat::from_array(rotation))
    }

    /// Puts a body somewhere at once, as a teleport.
    pub fn set_transform(&mut self, body: BodyId, position: Vec3, rotation: Quat) {
        let (position, rotation) = (position.to_array(), rotation.to_array());
        unsafe { ffi::vx_body_set_transform(self.world, body.0, position.as_ptr(), rotation.as_ptr()) };
    }

    /// Moves a kinematic body to arrive at a place over the next `dt`
    /// seconds, pushing what is in its way.
    pub fn move_kinematic(&mut self, body: BodyId, position: Vec3, rotation: Quat, dt: f32) {
        let (position, rotation) = (position.to_array(), rotation.to_array());
        unsafe { ffi::vx_body_move_kinematic(self.world, body.0, position.as_ptr(), rotation.as_ptr(), dt) };
    }

    /// A body's linear and angular velocity.
    pub fn velocity(&self, body: BodyId) -> (Vec3, Vec3) {
        let (mut linear, mut angular) = ([0.0f32; 3], [0.0f32; 3]);
        unsafe { ffi::vx_body_get_velocity(self.world, body.0, linear.as_mut_ptr(), angular.as_mut_ptr()) };
        (Vec3::from_array(linear), Vec3::from_array(angular))
    }

    pub fn set_velocity(&mut self, body: BodyId, linear: Vec3, angular: Vec3) {
        let (linear, angular) = (linear.to_array(), angular.to_array());
        unsafe { ffi::vx_body_set_velocity(self.world, body.0, linear.as_ptr(), angular.as_ptr()) };
    }

    pub fn push(&mut self, body: BodyId, push: Push) {
        let (kind, amount, at) = match push {
            Push::Force(amount, at) => (0, amount, at),
            Push::Impulse(amount, at) => (1, amount, at),
            Push::Torque(amount) => (2, amount, None),
            Push::AngularImpulse(amount) => (3, amount, None),
        };
        let amount = amount.to_array();
        let at = at.map(|at| at.to_array());
        let at = at.as_ref().map_or(std::ptr::null(), |at| at.as_ptr());
        unsafe { ffi::vx_body_push(self.world, body.0, kind, amount.as_ptr(), at) };
    }

    /// Whether a body is awake: one at rest sleeps until something moves it.
    pub fn is_active(&self, body: BodyId) -> bool {
        unsafe { ffi::vx_body_is_active(self.world, body.0) != 0 }
    }

    pub fn user_data(&self, body: BodyId) -> u64 {
        unsafe { ffi::vx_body_user_data(self.world, body.0) }
    }

    /// The first body a ray meets from `origin`, going `direction`, whose
    /// length is how far it reaches, passing over `ignore`.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, ignore: Option<BodyId>) -> Option<RayHit> {
        let mut hit = ffi::VxHit::default();
        let (origin, direction) = (origin.to_array(), direction.to_array());
        let ignore = ignore.map_or(u32::MAX, |body| body.0);
        let found = unsafe { ffi::vx_raycast(self.world, origin.as_ptr(), direction.as_ptr(), ignore, &mut hit) };
        (found != 0).then(|| RayHit {
            body: BodyId(hit.body),
            fraction: hit.fraction,
            point: Vec3::from_array(hit.point),
            normal: Vec3::from_array(hit.normal),
        })
    }

    /// The bodies whose shapes hold a point.
    pub fn bodies_at(&self, point: Vec3) -> Vec<BodyId> {
        let point = point.to_array();
        let mut found = vec![0u32; 16];
        let mut count = unsafe { ffi::vx_overlap_point(self.world, point.as_ptr(), found.as_mut_ptr(), 16) };
        if count > 16 {
            found.resize(count as usize, 0);
            count = unsafe { ffi::vx_overlap_point(self.world, point.as_ptr(), found.as_mut_ptr(), count) };
        }
        found.truncate(count.max(0) as usize);
        found.into_iter().map(BodyId).collect()
    }

    /// The contacts that began or ended since the last call.
    pub fn take_contacts(&mut self) -> Vec<Contact> {
        let mut out = Vec::new();
        loop {
            let mut batch = vec![ffi::VxContact::default(); 256];
            let count = unsafe { ffi::vx_contacts(self.world, batch.as_mut_ptr(), 256) };
            let taken = count.clamp(0, 256) as usize;
            out.extend(batch[..taken].iter().map(|c| Contact {
                began: c.kind == 0,
                a: BodyId(c.a),
                b: BodyId(c.b),
                point: Vec3::from_array(c.point),
                normal: Vec3::from_array(c.normal),
            }));
            if count <= 256 {
                return out;
            }
        }
    }

    pub fn add_character(&mut self, desc: &CharacterDesc) -> CharacterId {
        let raw = ffi::VxCharacterDesc {
            position: desc.position.to_array(),
            height: desc.height,
            radius: desc.radius,
            max_slope_degrees: desc.max_slope,
            step_height: desc.step_height,
            mass: desc.mass,
        };
        CharacterId(unsafe { ffi::vx_character_add(self.world, &raw) })
    }

    /// The kinematic body inside a character, which follows it, for
    /// triggers to notice and bodies to hit.
    pub fn character_body(&self, character: CharacterId) -> Option<BodyId> {
        let body = unsafe { ffi::vx_character_body(self.world, character.0) };
        (body != u32::MAX).then_some(BodyId(body))
    }

    pub fn remove_character(&mut self, character: CharacterId) {
        unsafe { ffi::vx_character_remove(self.world, character.0) };
    }

    /// Moves a character on for `dt` seconds at `velocity`, climbing steps
    /// up to `step_height`. Gravity is the caller's to add to the velocity,
    /// so that a jump is a velocity up.
    pub fn move_character(
        &mut self,
        character: CharacterId,
        velocity: Vec3,
        dt: f32,
        step_height: f32,
    ) -> CharacterState {
        let mut state = ffi::VxCharacterState::default();
        let velocity = velocity.to_array();
        unsafe { ffi::vx_character_move(self.world, character.0, velocity.as_ptr(), dt, step_height, &mut state) };
        CharacterState {
            position: Vec3::from_array(state.position),
            velocity: Vec3::from_array(state.velocity),
            ground: match state.ground {
                0 => Ground::Ground,
                1 => Ground::Steep,
                2 => Ground::Against,
                _ => Ground::InAir,
            },
            ground_normal: Vec3::from_array(state.ground_normal),
        }
    }

    pub fn set_character_position(&mut self, character: CharacterId, position: Vec3) {
        let position = position.to_array();
        unsafe { ffi::vx_character_set_position(self.world, character.0, position.as_ptr()) };
    }

    /// Joins two bodies, or `None` when either is gone.
    pub fn add_joint(&mut self, kind: JointKind, a: BodyId, b: BodyId) -> Option<JointId> {
        let (code, point, axis, (min, max)) = match kind {
            JointKind::Fixed => (0, Vec3::ZERO, Vec3::Y, (0.0, 0.0)),
            JointKind::Point(point) => (1, point, Vec3::Y, (0.0, 0.0)),
            JointKind::Hinge { point, axis, limits } => (2, point, axis, limits.unwrap_or((0.0, 0.0))),
            JointKind::Distance(range) => (3, Vec3::ZERO, Vec3::Y, range.unwrap_or((-1.0, 0.0))),
        };
        let (point, axis) = (point.to_array(), axis.to_array());
        let joint = unsafe { ffi::vx_joint_add(self.world, code, a.0, b.0, point.as_ptr(), axis.as_ptr(), min, max) };
        (joint != 0).then_some(JointId(joint))
    }

    pub fn remove_joint(&mut self, joint: JointId) {
        unsafe { ffi::vx_joint_remove(self.world, joint.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground a metre below the origin and a crate dropped from 5 metres.
    fn ground_and_crate(world: &mut PhysicsWorld) -> (BodyId, BodyId) {
        let floor = world.box_shape(Vec3::new(50.0, 0.5, 50.0));
        let ground =
            world.add_body(&BodyDesc::new(floor, Motion::Static).at(Vec3::new(0.0, -1.5, 0.0), Quat::IDENTITY));
        let shape = world.box_shape(Vec3::splat(0.5));
        let mut desc = BodyDesc::new(shape, Motion::Dynamic).at(Vec3::new(0.0, 5.0, 0.0), Quat::IDENTITY);
        desc.user_data = 42;
        let falling = world.add_body(&desc);
        (ground.unwrap(), falling.unwrap())
    }

    #[test]
    fn a_crate_falls_lands_and_is_told_of() {
        let mut world = PhysicsWorld::new(1024);
        let (ground, crate_) = ground_and_crate(&mut world);
        assert_eq!(world.user_data(crate_), 42);
        let mut touched = false;
        for _ in 0..180 {
            world.step(1.0 / 60.0, 1);
            touched |= world.take_contacts().iter().any(|c| c.began && [c.a, c.b].contains(&ground));
        }
        let (position, _) = world.transform(crate_);
        assert!((position.y - (-0.5)).abs() < 0.05, "it rests on the ground: {position}");
        assert!(touched, "the contact was told of");
        assert!(!world.is_active(crate_), "at rest, it sleeps");
    }

    #[test]
    fn rays_meet_the_nearest_body_and_points_find_what_holds_them() {
        let mut world = PhysicsWorld::new(1024);
        let (ground, crate_) = ground_and_crate(&mut world);
        let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -20.0, 0.0), None).expect("a hit");
        assert_eq!(hit.body, crate_);
        assert!((hit.point.y - 5.5).abs() < 0.01 && hit.normal.y > 0.99, "{hit:?}");
        let past = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -20.0, 0.0), Some(crate_)).unwrap();
        assert_eq!(past.body, ground, "past the crate when it is passed over");
        assert_eq!(world.bodies_at(Vec3::new(0.0, 5.0, 0.0)), [crate_]);
    }

    #[test]
    fn a_character_walks_on_the_ground_and_pushes_into_a_wall() {
        let mut world = PhysicsWorld::new(1024);
        let floor = world.box_shape(Vec3::new(50.0, 0.5, 50.0));
        world.add_body(&BodyDesc::new(floor, Motion::Static).at(Vec3::new(0.0, -0.5, 0.0), Quat::IDENTITY));
        let wall = world.box_shape(Vec3::new(0.5, 2.0, 5.0));
        world.add_body(&BodyDesc::new(wall, Motion::Static).at(Vec3::new(3.0, 2.0, 0.0), Quat::IDENTITY));
        let character =
            world.add_character(&CharacterDesc { position: Vec3::new(0.0, 0.05, 0.0), ..Default::default() });
        let mut state = None;
        for _ in 0..120 {
            state = Some(world.move_character(character, Vec3::new(3.0, -1.0, 0.0), 1.0 / 60.0, 0.3));
        }
        let state = state.unwrap();
        assert_eq!(state.ground, Ground::Ground, "{state:?}");
        assert!(state.position.x < 2.5 && state.position.x > 2.0, "stopped by the wall: {state:?}");
        assert!(state.position.y.abs() < 0.05, "on the floor: {state:?}");
    }

    #[test]
    fn a_hinge_holds_a_swinging_door() {
        let mut world = PhysicsWorld::new(1024);
        let post = world.box_shape(Vec3::splat(0.1));
        let post = world.add_body(&BodyDesc::new(post, Motion::Static)).unwrap();
        let slab = world.box_shape(Vec3::new(0.5, 1.0, 0.05));
        let door = world.add_body(&BodyDesc::new(slab, Motion::Dynamic).at(Vec3::new(0.6, 0.0, 0.0), Quat::IDENTITY));
        let door = door.unwrap();
        let hinge = JointKind::Hinge { point: Vec3::new(0.1, 0.0, 0.0), axis: Vec3::Y, limits: None };
        assert!(world.add_joint(hinge, post, door).is_some());
        for _ in 0..120 {
            world.step(1.0 / 60.0, 1);
        }
        let (position, _) = world.transform(door);
        assert!(position.y.abs() < 0.05, "held up by its hinge rather than falling: {position}");
    }
}
