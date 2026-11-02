//! Layers for C#: an Obj's layer, read and set, the project's layers'
//! names, for `Obj.Layer` and `LayerMask`, and which layers collide, for
//! `Physics.IgnoreLayerCollision` (see `voxear_scene::Layers`).

use voxear_ecs::Entity;
use voxear_scene::{LAYER_COUNT, Layer, Layers};

use crate::interop::{utf8, with_world};

/// An Obj's layer, 0 to 31, set to `layer` first when `set`; -1 for an Obj
/// that is gone.
pub(crate) unsafe extern "C" fn obj_layer(entity: u64, set: i32, layer: i32) -> i32 {
    let entity = Entity::from_bits(entity);
    let found = with_world(|world| {
        if !world.is_alive(entity) {
            return None;
        }
        if set != 0 {
            match layer.clamp(0, LAYER_COUNT as i32 - 1) as u8 {
                0 => drop(world.remove::<Layer>(entity)),
                layer => drop(world.insert(entity, Layer(layer))),
            }
        }
        Some(world.get::<Layer>(entity).map_or(0, |layer| i32::from(layer.0)))
    });
    found.unwrap_or(-1)
}

/// A layer's name, written into the buffer when it fits; its length
/// either way, 0 for a layer not used.
pub(crate) unsafe extern "C" fn layer_name(layer: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let name = with_world(|world| {
        let layers = world.resource::<Layers>().cloned().unwrap_or_default();
        Some(layers.name(layer.clamp(0, LAYER_COUNT as i32 - 1) as u8).to_string())
    });
    let name = name.unwrap_or_default();
    if !buffer.is_null() && name.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(name.as_ptr(), buffer, name.len()) };
    }
    name.len() as i32
}

/// Whether Objs on two layers collide, set to `on` first when `set`, for
/// as long as the game plays: 1 when they do.
pub(crate) unsafe extern "C" fn layer_collide(a: i32, b: i32, set: i32, on: i32) -> i32 {
    let (a, b) = (a.clamp(0, LAYER_COUNT as i32 - 1) as u8, b.clamp(0, LAYER_COUNT as i32 - 1) as u8);
    let collide = with_world(|world| {
        if set != 0 {
            if world.resource::<Layers>().is_none() {
                world.insert_resource(Layers::default());
            }
            world.resource_mut::<Layers>()?.set_collide(a, b, on != 0);
        }
        Some(world.resource::<Layers>().is_none_or(|layers| layers.collide(a, b)))
    });
    i32::from(collide.unwrap_or(true))
}

/// The layer with a name, or -1 when the project has none by it.
pub(crate) unsafe extern "C" fn layer_find(name: *const u8, len: i32) -> i32 {
    let name = unsafe { utf8(name, len) };
    let found = with_world(|world| world.resource::<Layers>().cloned().unwrap_or_default().find(&name));
    found.map_or(-1, i32::from)
}
