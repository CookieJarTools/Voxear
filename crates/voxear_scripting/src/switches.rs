//! Switching Objs and their components on and off for C#: `Obj.Active`,
//! `Obj.ActiveInHierarchy` and a component's `Enabled` (see
//! `voxear_scene::Switch`).

use voxear_ecs::Entity;
use voxear_scene::Switch;

use crate::interop::with_world;

/// `what` for the Obj's own switch, Unity's `activeSelf`.
const OBJ: i32 = -1;
/// `what` for whether the Obj and every Obj above it are on, which cannot
/// be set.
const IN_HIERARCHY: i32 = -2;

/// Whether the Obj, or one of its components by `voxear_scene::Switch`
/// number, is on, switched on or off first when `set` is 1 or 0: 1 when on,
/// 0 when off, -1 for an Obj that is gone or a number that is no component.
pub(crate) unsafe extern "C" fn obj_switch(entity: u64, what: i32, set: i32) -> i32 {
    let entity = Entity::from_bits(entity);
    let on = with_world(|world| {
        if !world.is_alive(entity) {
            return None;
        }
        let set = matches!(set, 0 | 1).then_some(set == 1);
        match what {
            OBJ => {
                if let Some(active) = set {
                    voxear_scene::set_active(world, entity, active);
                }
                Some(voxear_scene::is_active_self(world, entity))
            }
            IN_HIERARCHY => Some(voxear_scene::is_active(world, entity)),
            _ => {
                let switch = Switch::from_index(u32::try_from(what).ok()?)?;
                if let Some(on) = set {
                    voxear_scene::set_component_on(world, entity, switch, on);
                }
                Some(voxear_scene::is_component_on(world, entity, switch))
            }
        }
    });
    on.map_or(-1, i32::from)
}
