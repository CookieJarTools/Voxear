//! Cameras for scripts: a Camera's field of view, near plane and exposure,
//! as `Camera` in Voxear.Core reads and writes them.

use voxear_ecs::{Entity, World};
use voxear_render::Camera;
use voxear_scripting::ComponentKind;

// What `values` reads or writes, as `Camera` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const FIELD_OF_VIEW: i32 = 3;
const NEAR: i32 = 4;
const EXPOSURE: i32 = 5;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Camera, values);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<Camera>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<Camera>(entity) {
                world.insert(entity, Camera::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<Camera>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let camera = world.get_mut::<Camera>(entity)?;
    match (what, written) {
        (FIELD_OF_VIEW, None) => Some(number(camera.fov_y_degrees)),
        (NEAR, None) => Some(number(camera.near)),
        (EXPOSURE, None) => Some(number(camera.exposure)),
        (FIELD_OF_VIEW, Some([x, ..])) => {
            camera.fov_y_degrees = x.clamp(1.0, 170.0);
            Some([0.0; 4])
        }
        (NEAR, Some([x, ..])) => {
            camera.near = x.max(1e-4);
            Some([0.0; 4])
        }
        (EXPOSURE, Some([x, ..])) => {
            camera.exposure = x.max(0.0);
            Some([0.0; 4])
        }
        _ => None,
    }
}

/// An Obj as a script value: its id's two halves, bit for bit.
pub(crate) fn obj_value(entity: Option<Entity>) -> [f32; 4] {
    let bits = entity.map_or(0, Entity::to_bits);
    [f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32), 0.0, 0.0]
}

/// The Obj a script value names, if any.
pub(crate) fn value_obj([low, high, ..]: [f32; 4]) -> Option<Entity> {
    let bits = u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32);
    (bits != 0).then(|| Entity::from_bits(bits))
}
