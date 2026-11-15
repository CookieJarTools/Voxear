//! Reflection probes for scripts: `ReflectionProbe` in Voxear.Core reads
//! and writes an Obj's probe through here. The probe itself, and how it is
//! captured, are in `voxear_render::reflection_probes`; its bake, in
//! [`crate::reflection_bakes`].

use voxear_ecs::{Entity, World};
use voxear_render::{ProbeUpdate, ReflectionProbe, ReflectionProbeBake};
use voxear_scripting::ComponentKind;

/// Sets what answers `ReflectionProbe` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::ReflectionProbe, values);
}

// What `values` reads or writes, as `ReflectionProbe` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const UPDATE: i32 = 3;
const INTERVAL: i32 = 4;
const BLEND_DISTANCE: i32 = 5;
const BOX_PROJECTION: i32 = 6;
const INTENSITY: i32 = 7;
const IMPORTANCE: i32 = 8;
const CAPTURE: i32 = 9;

/// Reads value `what` of `entity`'s Reflection Probe, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<ReflectionProbe>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<ReflectionProbe>(entity) {
                world.insert(entity, ReflectionProbe::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<ReflectionProbeBake>(entity);
            return world.remove::<ReflectionProbe>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    let probe = world.get_mut::<ReflectionProbe>(entity)?;
    match written {
        None => Some(match what {
            UPDATE => number(if probe.update == ProbeUpdate::Live { 1.0 } else { 0.0 }),
            INTERVAL => number(probe.interval),
            BLEND_DISTANCE => number(probe.blend_distance),
            BOX_PROJECTION => number(f32::from(u8::from(probe.box_projection))),
            INTENSITY => number(probe.intensity),
            IMPORTANCE => number(probe.importance as f32),
            _ => return None,
        }),
        Some([x, y, z, w]) => {
            match what {
                UPDATE => probe.update = if x >= 0.5 { ProbeUpdate::Live } else { ProbeUpdate::Once },
                INTERVAL => probe.interval = x.max(0.0),
                BLEND_DISTANCE => probe.blend_distance = x.max(0.0),
                BOX_PROJECTION => probe.box_projection = x != 0.0,
                INTENSITY => probe.intensity = x.max(0.0),
                IMPORTANCE => probe.importance = x.round() as i32,
                CAPTURE => probe.capture(),
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_adds_a_probe_changes_it_asks_for_a_capture_and_takes_it_off() {
        let mut world = World::new();
        let hall = world.spawn(());
        assert_eq!(values(&mut world, hall, HAS, None), Some([0.0; 4]));
        assert_eq!(values(&mut world, hall, INTENSITY, None), None, "none to read");
        values(&mut world, hall, ADD, Some([0.0; 4]));
        values(&mut world, hall, UPDATE, Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, hall, INTERVAL, Some([-2.0, 0.0, 0.0, 0.0]));
        values(&mut world, hall, IMPORTANCE, Some([3.0, 0.0, 0.0, 0.0]));
        values(&mut world, hall, CAPTURE, Some([0.0; 4]));
        let probe = world.get::<ReflectionProbe>(hall).unwrap().clone();
        assert_eq!((probe.update, probe.interval, probe.importance, probe.captures), (ProbeUpdate::Live, 0.0, 3, 1));
        assert_eq!(values(&mut world, hall, BOX_PROJECTION, None), Some([1.0, 0.0, 0.0, 0.0]), "on as it starts");
        assert!(values(&mut world, hall, REMOVE, Some([0.0; 4])).is_some());
        assert!(!world.has::<ReflectionProbe>(hall));
    }
}
