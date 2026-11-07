//! What a gun throws away: a spent case out of the port, an empty magazine
//! dropped, a stripper clip flicked off, an en-bloc clip pinging out. Each
//! is a copy of the gun part it was, a look and nothing more, thrown with
//! the gun's own velocity and a push of its own, which falls, bounces and
//! is taken away after a while.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_physics::scene::{Collider, PhysicsScene, Rigidbody};
use voxear_render::{MeshRenderer, ModelRenderer};
use voxear_scene::{Destroying, Hidden, Name, Transform, world_matrix};

use super::mechanism::{GunPart, GunState, PartRole};

/// How long a leftover lies before it goes, in seconds.
const LASTS: f64 = 30.0;

/// A thing a gun threw away: when it goes, and the velocity it was thrown
/// with, given once its body is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leftover {
    until: f64,
    thrown: Option<(Vec3, Vec3)>,
}

impl Component for Leftover {}

/// Copies a gun's part of `role` where it stands, as a body of its own
/// thrown at `linear` metres a second and spinning at `angular` radians a
/// second: a box of `size` metres, whatever the part's look.
#[allow(clippy::too_many_arguments)]
pub fn throw(
    world: &mut World,
    state: &GunState,
    role: PartRole,
    name: &str,
    linear: Vec3,
    angular: Vec3,
    size: Vec3,
    now: f64,
) -> Option<Entity> {
    let &(part, ..) = state.parts.iter().find(|(e, ..)| world.get::<GunPart>(*e).is_some_and(|p| p.role == role))?;
    let (scale, rotation, position) = world_matrix(world, part).to_scale_rotation_translation();
    let mesh = world.get::<MeshRenderer>(part).copied();
    let model = world.get::<ModelRenderer>(part).cloned();
    if mesh.is_none() && model.is_none() {
        return None;
    }
    let copy = world.spawn((Name(name.into()), Transform::new(position, rotation, scale)));
    if let Some(mesh) = mesh {
        world.insert(copy, mesh);
    }
    if let Some(model) = model {
        world.insert(copy, model);
    }
    // The box is given in the copy's own space, which its scale stretches.
    let unscaled = size / scale.max(Vec3::splat(1e-4));
    world.insert(copy, Collider { size: unscaled.to_array(), ..Collider::default() });
    world.insert(copy, Rigidbody { mass: 0.05, continuous: true, ..Rigidbody::default() });
    world.insert(copy, Leftover { until: now + LASTS, thrown: Some((linear, angular)) });
    Some(copy)
}

/// A spent case thrown out of the port, to the right and up, spinning.
pub fn throw_case(
    world: &mut World,
    state: &GunState,
    held: Mat4,
    carried: Vec3,
    now: f64,
    seed: f32,
) -> Option<Entity> {
    let right = held.transform_vector3(Vec3::X).normalize_or_zero();
    let up = held.transform_vector3(Vec3::Y).normalize_or_zero();
    let back = held.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
    let linear = carried + right * (2.4 + seed * 0.6) + up * (1.4 + seed * 0.4) + back * 0.6;
    let angular = held.transform_vector3(Vec3::new(0.0, 25.0 + seed * 15.0, 8.0));
    let copy = throw(world, state, PartRole::Case, "Spent Case", linear, angular, Vec3::new(0.012, 0.012, 0.05), now)?;
    world.remove::<Hidden>(copy);
    Some(copy)
}

/// Gives each leftover thrown since last frame its velocity, now its body
/// is made, and takes away those that have lain long enough.
pub fn settle(world: &mut World, now: f64) {
    let leftovers: Vec<(Entity, Leftover)> =
        world.query_ref::<(Entity, &Leftover)>().into_iter().map(|(e, l)| (e, *l)).collect();
    for (entity, leftover) in leftovers {
        if now > leftover.until {
            world.insert(entity, Destroying);
            continue;
        }
        let Some((linear, angular)) = leftover.thrown else { continue };
        let Some(physics) = world.resource_mut::<PhysicsScene>() else { continue };
        let Some(body) = physics.body(entity) else { continue };
        physics.world_mut().set_velocity(body, linear, angular);
        if let Some(leftover) = world.get_mut::<Leftover>(entity) {
            leftover.thrown = None;
        }
    }
}

/// Hides the template parts a gun copies leftovers from, which are not part
/// of the gun's look themselves.
pub fn hide_templates(world: &mut World, state: &GunState) {
    for &(entity, ..) in &state.parts {
        if world.get::<GunPart>(entity).is_some_and(|part| part.role == PartRole::Case) && !world.has::<Hidden>(entity)
        {
            world.insert(entity, Hidden);
        }
    }
}
