//! The shim's C API (shim/voxear_jolt.cpp), as it declares it.

use std::ffi::c_int;

/// A Jolt world, kept on the C++ side.
#[repr(C)]
pub struct VxWorld {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VxBodyDesc {
    pub shape: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub motion: u32,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_factor: f32,
    pub sensor: u32,
    pub continuous: u32,
    pub user_data: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VxHit {
    pub body: u32,
    pub fraction: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VxCharacterDesc {
    pub position: [f32; 3],
    pub height: f32,
    pub radius: f32,
    pub max_slope_degrees: f32,
    pub step_height: f32,
    pub mass: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VxCharacterState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub ground: u32,
    pub ground_normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VxContact {
    pub kind: u32,
    pub a: u32,
    pub b: u32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

unsafe extern "C" {
    pub fn vx_world_create(max_bodies: u32) -> *mut VxWorld;
    pub fn vx_world_destroy(world: *mut VxWorld);
    pub fn vx_world_set_gravity(world: *mut VxWorld, x: f32, y: f32, z: f32);
    pub fn vx_world_step(world: *mut VxWorld, dt: f32, steps: c_int);

    pub fn vx_shape_box(world: *mut VxWorld, hx: f32, hy: f32, hz: f32) -> u32;
    pub fn vx_shape_sphere(world: *mut VxWorld, radius: f32) -> u32;
    pub fn vx_shape_capsule(world: *mut VxWorld, half_height: f32, radius: f32) -> u32;
    pub fn vx_shape_convex_hull(world: *mut VxWorld, points: *const f32, count: u32) -> u32;
    pub fn vx_shape_mesh(
        world: *mut VxWorld,
        vertices: *const f32,
        vertex_count: u32,
        indices: *const u32,
        triangle_count: u32,
    ) -> u32;
    pub fn vx_shape_placed(
        world: *mut VxWorld,
        shape: u32,
        offset: *const f32,
        rotation: *const f32,
        scale: *const f32,
    ) -> u32;
    pub fn vx_shape_release(world: *mut VxWorld, shape: u32);

    pub fn vx_body_add(world: *mut VxWorld, desc: *const VxBodyDesc) -> u32;
    pub fn vx_body_remove(world: *mut VxWorld, body: u32);
    pub fn vx_body_get_transform(world: *mut VxWorld, body: u32, position: *mut f32, rotation: *mut f32);
    pub fn vx_body_set_transform(world: *mut VxWorld, body: u32, position: *const f32, rotation: *const f32);
    pub fn vx_body_move_kinematic(world: *mut VxWorld, body: u32, position: *const f32, rotation: *const f32, dt: f32);
    pub fn vx_body_get_velocity(world: *mut VxWorld, body: u32, linear: *mut f32, angular: *mut f32);
    pub fn vx_body_set_velocity(world: *mut VxWorld, body: u32, linear: *const f32, angular: *const f32);
    pub fn vx_body_push(world: *mut VxWorld, body: u32, kind: u32, amount: *const f32, at: *const f32);
    pub fn vx_body_is_active(world: *mut VxWorld, body: u32) -> c_int;
    pub fn vx_body_user_data(world: *mut VxWorld, body: u32) -> u64;

    pub fn vx_raycast(
        world: *mut VxWorld,
        origin: *const f32,
        direction: *const f32,
        ignore: u32,
        hit: *mut VxHit,
    ) -> c_int;
    pub fn vx_overlap_point(world: *mut VxWorld, point: *const f32, bodies: *mut u32, capacity: c_int) -> c_int;
    pub fn vx_contacts(world: *mut VxWorld, out: *mut VxContact, capacity: c_int) -> c_int;

    pub fn vx_character_add(world: *mut VxWorld, desc: *const VxCharacterDesc) -> u32;
    pub fn vx_character_remove(world: *mut VxWorld, character: u32);
    pub fn vx_character_move(
        world: *mut VxWorld,
        character: u32,
        velocity: *const f32,
        dt: f32,
        step_height: f32,
        state: *mut VxCharacterState,
    );
    pub fn vx_character_set_position(world: *mut VxWorld, character: u32, position: *const f32);

    pub fn vx_joint_add(
        world: *mut VxWorld,
        kind: u32,
        a: u32,
        b: u32,
        point: *const f32,
        axis: *const f32,
        min: f32,
        max: f32,
    ) -> u32;
    pub fn vx_joint_remove(world: *mut VxWorld, joint: u32);
}
