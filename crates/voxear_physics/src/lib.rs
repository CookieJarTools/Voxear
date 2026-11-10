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
//! Bodies and characters are each on one of 32 layers, and
//! [`PhysicsWorld::set_layers`] says which layers collide; what does not
//! move never collides with itself. [`PhysicsWorld::step`] moves everything
//! on; contacts that began or ended come after, from
//! [`PhysicsWorld::take_contacts`]. Ray casts and overlaps ask what is
//! where, on the layers in a mask. Y is up, and units are metres,
//! kilograms and seconds.
//!
//! [`scene`] puts Objs in a world: their Collider, Rigidbody and Character
//! Controller components, and the [`PhysicsScene`](scene::PhysicsScene)
//! that keeps a body for each while the game plays.

mod ffi;
pub mod scene;

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

/// A vehicle, made by [`PhysicsWorld::add_vehicle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VehicleId(u32);

/// What drives a vehicle: wheels, tracks, or two wheels that lean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VehicleType {
    #[default]
    Car,
    Tank,
    Motorbike,
}

/// A vehicle's wheel, in its body's space: where its suspension is fixed,
/// how far it travels, its spring, and the wheel itself. A wheel steers
/// up to `max_steer` radians, and is driven when `powered`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelDesc {
    pub position: Vec3,
    pub radius: f32,
    pub width: f32,
    pub suspension: (f32, f32),
    pub frequency: f32,
    pub damping: f32,
    pub max_steer: f32,
    pub brake: f32,
    pub handbrake: f32,
    pub powered: bool,
    /// How hard the tyre holds, forwards and sideways, as a share of Jolt's
    /// own tyre: 1 as it is, 2 twice as hard.
    pub grip: (f32, f32),
}

/// A vehicle: what drives it, its engine's most torque in Nm and its range
/// of revolutions a minute, whether its gears change by themselves, how
/// far it may pitch or roll in radians, and its wheels; its gearbox, so
/// many forward gears from the first's ratio to the top's (none for Jolt's
/// own), and how stiff the anti-roll bars across its axles are, in N/m (0
/// for none).
#[derive(Clone, Debug, PartialEq)]
pub struct VehicleDesc {
    pub kind: VehicleType,
    pub max_torque: f32,
    pub rpm: (f32, f32),
    pub automatic: bool,
    pub max_pitch_roll: f32,
    pub wheels: Vec<WheelDesc>,
    pub gears: (u32, f32, f32),
    pub anti_roll: f32,
}

/// How a vehicle is going: its engine's revolutions a minute, its gear (-1
/// reverse, 0 neutral, 1 first), and its speed forward in metres a second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleState {
    pub rpm: f32,
    pub gear: i32,
    pub speed: f32,
}

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
    /// Its layer, from 0 to 31.
    pub layer: u8,
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
            layer: 0,
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
    /// Its layer, from 0 to 31: it walks through bodies on layers its own
    /// does not collide with.
    pub layer: u8,
}

impl Default for CharacterDesc {
    fn default() -> Self {
        Self { position: Vec3::ZERO, height: 1.8, radius: 0.3, max_slope: 45.0, step_height: 0.3, mass: 70.0, layer: 0 }
    }
}

/// A layer mask with every layer in it.
pub const EVERY_LAYER: u32 = u32::MAX;

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
    /// The first body's centre stays within a range of distances of a
    /// point in the world, held to the second body, or as far as it is
    /// now.
    Distance { point: Vec3, range: Option<(f32, f32)> },
}

/// The largest number of bodies a world holds by default.
pub const MAX_BODIES: u32 = 65_536;

/// A physics world on Jolt: see the crate's documentation.
pub struct PhysicsWorld {
    world: *mut ffi::VxWorld,
}

// The world is changed from one thread at a time; Jolt's own jobs run inside
// a step. What takes it shared only asks, through Jolt's body and query
// interfaces, which lock what they read, so several threads may ask at once.
unsafe impl Send for PhysicsWorld {}
unsafe impl Sync for PhysicsWorld {}

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

    /// Which layers collide: bit `b` of `collide[a]` set when bodies on
    /// layers `a` and `b` do. Every pair collides until this is set.
    pub fn set_layers(&mut self, collide: &[u32; 32]) {
        unsafe { ffi::vx_world_set_layers(self.world, collide.as_ptr()) };
    }

    /// Moves everything on by `dt` seconds, in `steps` collision steps:
    /// one is enough at 60 a second, and more keep a slower step stable.
    pub fn step(&mut self, dt: f32, steps: u32) {
        unsafe { ffi::vx_world_step(self.world, dt, steps.max(1) as i32) };
    }

    /// Moves every body and character by `by`, as the world's origin moves
    /// everything: none wakes, and each keeps how fast it goes.
    pub fn shift(&mut self, by: Vec3) {
        let by = by.to_array();
        unsafe { ffi::vx_world_shift(self.world, by.as_ptr()) };
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

    /// Ground as heights: `count` by `count` samples, row after row along
    /// z, `spacing` metres apart, from the shape's origin along +x and +z;
    /// `f32::MAX` where there is no ground. Each quad is split from its
    /// lowest corner to its highest, as a terrain's are.
    pub fn heightfield_shape(&mut self, samples: &[f32], count: u32, spacing: f32) -> Option<ShapeId> {
        assert_eq!(samples.len(), (count * count) as usize, "a height field has count by count samples");
        let shape = unsafe { ffi::vx_shape_heightfield(self.world, samples.as_ptr(), count, spacing) };
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
            layer: u32::from(desc.layer % 32),
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

    /// Wakes a body, or puts it to sleep until something moves it.
    pub fn set_active(&mut self, body: BodyId, active: bool) {
        unsafe { ffi::vx_body_set_active(self.world, body.0, active.into()) };
    }

    /// The first body a ray meets from `origin`, going `direction`, whose
    /// length is how far it reaches, on the layers in `mask`, passing over
    /// `ignore`, and over sensors unless `sensors`. As in Unity, a ray that
    /// starts inside a shape passes out of it without meeting it.
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        mask: u32,
        ignore: Option<BodyId>,
        sensors: bool,
    ) -> Option<RayHit> {
        let mut hit = ffi::VxHit::default();
        let (origin, direction) = (origin.to_array(), direction.to_array());
        let ignore = ignore.map_or(u32::MAX, |body| body.0);
        let flags = u32::from(sensors);
        let found =
            unsafe { ffi::vx_raycast(self.world, origin.as_ptr(), direction.as_ptr(), ignore, flags, mask, &mut hit) };
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

    /// The bodies a sphere overlaps on the layers in `mask`, each once,
    /// with sensors when `sensors`.
    pub fn bodies_in_sphere(&self, center: Vec3, radius: f32, mask: u32, sensors: bool) -> Vec<BodyId> {
        let center = center.to_array();
        let flags = u32::from(sensors);
        let overlap = |found: &mut Vec<u32>| unsafe {
            let (out, capacity) = (found.as_mut_ptr(), found.len() as i32);
            ffi::vx_overlap_sphere(self.world, center.as_ptr(), radius, flags, mask, out, capacity)
        };
        let mut found = vec![0u32; 32];
        let count = overlap(&mut found);
        if count > 32 {
            found.resize(count as usize, 0);
            overlap(&mut found);
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
            layer: u32::from(desc.layer % 32),
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
    /// Holds two bodies together, or `a` to the world when `b` is none.
    pub fn add_joint(&mut self, kind: JointKind, a: BodyId, b: Option<BodyId>) -> Option<JointId> {
        let (code, point, axis, (min, max)) = match kind {
            JointKind::Fixed => (0, Vec3::ZERO, Vec3::Y, (0.0, 0.0)),
            JointKind::Point(point) => (1, point, Vec3::Y, (0.0, 0.0)),
            JointKind::Hinge { point, axis, limits } => (2, point, axis, limits.unwrap_or((0.0, 0.0))),
            JointKind::Distance { point, range } => (3, point, Vec3::Y, range.unwrap_or((-1.0, 0.0))),
        };
        let (point, axis) = (point.to_array(), axis.to_array());
        let b = b.map_or(u32::MAX, |b| b.0);
        let joint = unsafe { ffi::vx_joint_add(self.world, code, a.0, b, point.as_ptr(), axis.as_ptr(), min, max) };
        (joint != 0).then_some(JointId(joint))
    }

    pub fn remove_joint(&mut self, joint: JointId) {
        unsafe { ffi::vx_joint_remove(self.world, joint.0) };
    }

    /// Makes a vehicle of a dynamic body, Jolt's vehicle constraint with
    /// an engine, gearbox and differentials, or a tank's tracks.
    pub fn add_vehicle(&mut self, body: BodyId, desc: &VehicleDesc) -> Option<VehicleId> {
        let wheels: Vec<ffi::VxWheelDesc> = desc
            .wheels
            .iter()
            .map(|wheel| ffi::VxWheelDesc {
                position: wheel.position.to_array(),
                radius: wheel.radius,
                width: wheel.width,
                suspension_min: wheel.suspension.0,
                suspension_max: wheel.suspension.1,
                frequency: wheel.frequency,
                damping: wheel.damping,
                max_steer: wheel.max_steer,
                max_brake: wheel.brake,
                max_handbrake: wheel.handbrake,
                powered: u32::from(wheel.powered),
                grip: wheel.grip.0,
                side_grip: wheel.grip.1,
            })
            .collect();
        let kind = match desc.kind {
            VehicleType::Car => 0,
            VehicleType::Tank => 1,
            VehicleType::Motorbike => 2,
        };
        let raw = ffi::VxVehicleDesc {
            kind,
            max_torque: desc.max_torque,
            min_rpm: desc.rpm.0,
            max_rpm: desc.rpm.1,
            automatic: u32::from(desc.automatic),
            max_pitch_roll: desc.max_pitch_roll,
            wheel_count: wheels.len() as u32,
            wheels: wheels.as_ptr(),
            gears: desc.gears.0,
            first_gear: desc.gears.1,
            top_gear: desc.gears.2,
            anti_roll: desc.anti_roll,
        };
        let vehicle = unsafe { ffi::vx_vehicle_add(self.world, body.0, &raw) };
        (vehicle != 0).then_some(VehicleId(vehicle))
    }

    pub fn remove_vehicle(&mut self, vehicle: VehicleId) {
        unsafe { ffi::vx_vehicle_remove(self.world, vehicle.0) };
    }

    /// What the driver asks of a vehicle: `forward` from -1 (reverse) to 1,
    /// `right` from -1 (steering left) to 1, and the brake and hand brake
    /// from 0 to 1.
    pub fn drive_vehicle(&mut self, vehicle: VehicleId, forward: f32, right: f32, brake: f32, handbrake: f32) {
        let clamp = |value: f32, least: f32| value.clamp(least, 1.0);
        let (forward, right) = (clamp(forward, -1.0), clamp(right, -1.0));
        unsafe {
            ffi::vx_vehicle_drive(self.world, vehicle.0, forward, right, clamp(brake, 0.0), clamp(handbrake, 0.0))
        };
    }

    pub fn vehicle_state(&self, vehicle: VehicleId) -> VehicleState {
        let mut state = ffi::VxVehicleState::default();
        unsafe { ffi::vx_vehicle_state(self.world, vehicle.0, &mut state) };
        VehicleState { rpm: state.rpm, gear: state.gear, speed: state.speed }
    }

    /// Where a vehicle's wheel is now, and how it is turned: steered,
    /// rolled and down on its suspension.
    pub fn vehicle_wheel(&self, vehicle: VehicleId, wheel: usize) -> (Vec3, Quat) {
        let (mut position, mut rotation) = ([0.0f32; 3], [0.0, 0.0, 0.0, 1.0f32]);
        unsafe {
            ffi::vx_vehicle_wheel(self.world, vehicle.0, wheel as u32, position.as_mut_ptr(), rotation.as_mut_ptr())
        };
        (Vec3::from(position), Quat::from_array(rotation))
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

    /// A body on flat ground made a vehicle of `kind`, with four wheels,
    /// the front two steering, all of them powered; a tank's set wider,
    /// with a tank's torque.
    fn vehicle(world: &mut PhysicsWorld, kind: VehicleType) -> (BodyId, VehicleId) {
        tuned_vehicle(world, kind, &|_| {})
    }

    /// The same, its description changed by `tune` before it is made.
    fn tuned_vehicle(
        world: &mut PhysicsWorld,
        kind: VehicleType,
        tune: &dyn Fn(&mut VehicleDesc),
    ) -> (BodyId, VehicleId) {
        let floor = world.box_shape(Vec3::new(200.0, 0.5, 200.0));
        world.add_body(&BodyDesc::new(floor, Motion::Static).at(Vec3::new(0.0, -0.5, 0.0), Quat::IDENTITY));
        let shape = world.box_shape(Vec3::new(0.9, 0.3, 2.0));
        let mut desc = BodyDesc::new(shape, Motion::Dynamic).at(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY);
        desc.mass = 1200.0;
        let body = world.add_body(&desc).unwrap();
        let wheel = |x: f32, z: f32| WheelDesc {
            position: Vec3::new(x, -0.1, z),
            radius: 0.35,
            width: 0.25,
            suspension: (0.3, 0.5),
            frequency: 1.5,
            damping: 0.5,
            max_steer: if z > 0.0 { 0.6 } else { 0.0 },
            brake: 1500.0,
            handbrake: if z < 0.0 { 4000.0 } else { 0.0 },
            powered: true,
            grip: (1.0, 1.0),
        };
        let wheels = match kind {
            VehicleType::Tank => vec![wheel(-1.2, 0.9), wheel(1.2, 0.9), wheel(-1.2, -0.9), wheel(1.2, -0.9)],
            _ => vec![wheel(-0.9, 1.4), wheel(0.9, 1.4), wheel(-0.9, -1.4), wheel(0.9, -1.4)],
        };
        let mut desc = VehicleDesc {
            kind,
            max_torque: if kind == VehicleType::Tank { 5000.0 } else { 500.0 },
            rpm: (1000.0, 6000.0),
            automatic: true,
            max_pitch_roll: std::f32::consts::PI,
            wheels,
            gears: (5, 2.66, 0.74),
            anti_roll: 1000.0,
        };
        tune(&mut desc);
        (body, world.add_vehicle(body, &desc).expect("a vehicle"))
    }

    fn drive(world: &mut PhysicsWorld, vehicle: VehicleId, forward: f32, right: f32, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            world.drive_vehicle(vehicle, forward, right, 0.0, 0.0);
            world.step(1.0 / 60.0, 1);
        }
    }

    #[test]
    fn a_car_drives_forward_and_steers_the_way_it_is_told() {
        let mut world = PhysicsWorld::new(1024);
        let (body, car) = vehicle(&mut world, VehicleType::Car);
        drive(&mut world, car, 0.0, 0.0, 1.0);
        let (rest, _) = world.transform(body);
        assert!(rest.y > 0.3 && rest.y < 1.2, "on its wheels: {rest}");
        let (wheel, _) = world.vehicle_wheel(car, 0);
        assert!(wheel.x < -0.5 && wheel.y < rest.y, "the front left wheel under its corner: {wheel}");

        drive(&mut world, car, 1.0, 0.0, 3.0);
        let (ahead, _) = world.transform(body);
        let state = world.vehicle_state(car);
        assert!(ahead.z > 5.0 && ahead.x.abs() < 1.0, "forward is +Z: {ahead}");
        assert!(state.speed > 2.0 && state.gear >= 1 && state.rpm > 900.0, "{state:?}");

        drive(&mut world, car, 1.0, 1.0, 2.0);
        let (turned, _) = world.transform(body);
        assert!(turned.x > ahead.x + 1.0, "steering right goes to +X, Voxear's right: {turned}");
    }

    #[test]
    fn a_gearbox_and_tyres_change_how_a_car_goes() {
        // Its speed after four seconds flat out, and the highest gear it reached.
        let race = |tune: &dyn Fn(&mut VehicleDesc)| {
            let mut world = PhysicsWorld::new(1024);
            let (_, car) = tuned_vehicle(&mut world, VehicleType::Car, tune);
            drive(&mut world, car, 0.0, 0.0, 1.0);
            let mut top = 0;
            for _ in 0..240 {
                world.drive_vehicle(car, 1.0, 0.0, 0.0, 0.0);
                world.step(1.0 / 60.0, 1);
                top = top.max(world.vehicle_state(car).gear);
            }
            (world.vehicle_state(car).speed, top)
        };
        // Jolt changes up only while no driven wheel slips more than a tenth, which this car's tyres do in first
        // with all four wheels driven flat out, so the gearbox is raced on tyres that grip twice as hard.
        let grippy = |desc: &mut VehicleDesc| desc.wheels.iter_mut().for_each(|wheel| wheel.grip = (2.0, 2.0));
        let (speed, top) = race(&grippy);
        assert!(top >= 2, "an ordinary box changes up: {top} at {speed} m/s");
        let (slick, _) = race(&|desc| desc.wheels.iter_mut().for_each(|wheel| wheel.grip = (0.05, 0.05)));
        assert!(slick < speed * 0.7, "tyres with little grip spin: {slick} against {speed}");
        let (_, only) = race(&|desc| {
            grippy(desc);
            desc.gears = (1, 3.0, 3.0);
        });
        assert_eq!(only, 1, "a box of one gear stays in it");
    }

    #[test]
    fn a_tank_turns_on_the_spot() {
        let mut world = PhysicsWorld::new(1024);
        let (body, tank) = vehicle(&mut world, VehicleType::Tank);
        drive(&mut world, tank, 0.0, 0.0, 1.0);
        let (start, _) = world.transform(body);
        // Its tracks turn opposite ways; a tank turns best when they are about as far apart as it is long.
        drive(&mut world, tank, 0.0, 1.0, 0.4);
        let (at, turn) = world.transform(body);
        let facing = turn * Vec3::Z;
        assert!(at.distance(start) < 1.5, "on the spot: {start} to {at}");
        assert!(facing.x > 0.2, "turned right, to face +X: {facing}");
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
        let down = Vec3::new(0.0, -20.0, 0.0);
        let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), down, EVERY_LAYER, None, false).expect("a hit");
        assert_eq!(hit.body, crate_);
        assert!((hit.point.y - 5.5).abs() < 0.01 && hit.normal.y > 0.99, "{hit:?}");
        let past = world.raycast(Vec3::new(0.0, 10.0, 0.0), down, EVERY_LAYER, Some(crate_), false).unwrap();
        assert_eq!(past.body, ground, "past the crate when it is passed over");
        let inside = world.raycast(Vec3::new(0.0, 5.0, 0.0), down, EVERY_LAYER, None, false).unwrap();
        assert_eq!(inside.body, ground, "out of the crate it starts in, as in Unity");
        assert_eq!(world.bodies_at(Vec3::new(0.0, 5.0, 0.0)), [crate_]);

        // A sphere finds what it touches; a sensor only when asked for.
        let zone = world.box_shape(Vec3::splat(1.0));
        let mut desc = BodyDesc::new(zone, Motion::Static).at(Vec3::new(0.0, 8.0, 0.0), Quat::IDENTITY);
        desc.sensor = true;
        let zone = world.add_body(&desc).unwrap();
        assert_eq!(world.bodies_in_sphere(Vec3::new(0.0, 6.0, 0.0), 1.2, EVERY_LAYER, false), [crate_]);
        let mut both = world.bodies_in_sphere(Vec3::new(0.0, 6.0, 0.0), 1.2, EVERY_LAYER, true);
        both.sort();
        let mut expected = [crate_, zone];
        expected.sort();
        assert_eq!(both, expected);
        assert_eq!(world.raycast(Vec3::new(0.0, 10.0, 0.0), down, EVERY_LAYER, None, true).unwrap().body, zone);
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
    fn layers_that_do_not_collide_pass_through_and_masks_choose_what_rays_meet() {
        let mut world = PhysicsWorld::new(1024);
        let (ground, crate_) = ground_and_crate(&mut world);
        let shape = world.box_shape(Vec3::splat(0.5));
        let mut desc = BodyDesc::new(shape, Motion::Dynamic).at(Vec3::new(3.0, 5.0, 0.0), Quat::IDENTITY);
        desc.layer = 3;
        let ghost = world.add_body(&desc).unwrap();
        let mut collide = [EVERY_LAYER; 32];
        collide[0] &= !(1 << 3);
        collide[3] &= !1;
        world.set_layers(&collide);
        for _ in 0..120 {
            world.step(1.0 / 60.0, 1);
        }
        assert!(world.transform(crate_).0.y > -0.6, "the crate on layer 0 lands");
        assert!(world.transform(ghost).0.y < -2.0, "the one on layer 3 falls through the ground");

        // A ray meets what is on its mask's layers only.
        let down = Vec3::new(0.0, -40.0, 0.0);
        let at_ghost = Vec3::new(3.0, world.transform(ghost).0.y + 5.0, 0.0);
        assert_eq!(world.raycast(at_ghost, down, EVERY_LAYER, None, false).unwrap().body, ghost);
        assert_eq!(world.raycast(at_ghost, down, !(1 << 3), None, false).map(|hit| hit.body), None);
        assert_eq!(world.raycast(Vec3::new(0.0, 10.0, 0.0), down, 1 << 3, None, false).map(|hit| hit.body), None);
        assert_eq!(world.raycast(Vec3::new(0.0, 10.0, 0.0), down, 1, Some(crate_), false).unwrap().body, ground);

        // A character walks through a wall on a layer its own does not collide with.
        let wall = world.box_shape(Vec3::new(0.5, 2.0, 5.0));
        let mut desc = BodyDesc::new(wall, Motion::Static).at(Vec3::new(-3.0, 1.0, 0.0), Quat::IDENTITY);
        desc.layer = 4;
        world.add_body(&desc);
        collide[4] &= !(1 << 5);
        collide[5] &= !(1 << 4);
        world.set_layers(&collide);
        let walker = CharacterDesc { position: Vec3::new(0.0, -0.95, 2.0), layer: 5, ..Default::default() };
        let walker = world.add_character(&walker);
        let mut state = None;
        for _ in 0..120 {
            state = Some(world.move_character(walker, Vec3::new(-3.0, -1.0, 0.0), 1.0 / 60.0, 0.3));
        }
        assert!(state.unwrap().position.x < -4.0, "through the wall: {state:?}");
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
        assert!(world.add_joint(hinge, post, Some(door)).is_some());
        for _ in 0..120 {
            world.step(1.0 / 60.0, 1);
        }
        let (position, _) = world.transform(door);
        assert!(position.y.abs() < 0.05, "held up by its hinge rather than falling: {position}");
    }
}
