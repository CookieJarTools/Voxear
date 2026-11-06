//! Lights for C#: `Light` in Voxear.Core reads and writes an Obj's Light
//! through one call, by number, each value up to four floats (see
//! `voxear_scripting::set_light_values`). Colours cross as scripts keep
//! them, sRGB, and are kept linear here.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{Light, LightKind};

use crate::content::{linear, srgb};
use crate::scene_file::default_light;

// What `values` reads or writes, as `NativeLights` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const KIND: i32 = 3;
const COLOR: i32 = 4;
const INTENSITY: i32 = 5;
const RANGE: i32 = 6;
const SPOT_ANGLES: i32 = 7;
const TEMPERATURE: i32 = 8;
const SHADOWS: i32 = 9;
const SIZE: i32 = 10;
const RADIUS: i32 = 11;
const LENGTH: i32 = 12;
const SOFTNESS: i32 = 13;

/// As `LightKind` in Voxear.Core numbers them.
const KINDS: [LightKind; 6] =
    [LightKind::Directional, LightKind::Point, LightKind::Spot, LightKind::Rectangle, LightKind::Disc, LightKind::Tube];

/// Sets what answers `Light` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_light_values(values);
}

/// Reads value `what` of `entity`'s Light, or with `written`, writes it:
/// `None` when it has no Light, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<Light>(entity))),
        (ADD, Some([kind, ..])) => {
            if !world.has::<Light>(entity) {
                world.insert(entity, default_light(kind_of(kind)));
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<Light>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let light = world.get_mut::<Light>(entity)?;
    match written {
        None => read(light, what),
        Some(value) => {
            write(light, what, value);
            Some(value)
        }
    }
}

fn read(light: &Light, what: i32) -> Option<[f32; 4]> {
    Some(match what {
        KIND => number(KINDS.iter().position(|k| *k == light.kind).unwrap_or_default() as f32),
        COLOR => {
            let [r, g, b] = srgb(light.color.to_array());
            [r, g, b, 1.0]
        }
        INTENSITY => number(light.intensity),
        RANGE => number(light.range),
        SPOT_ANGLES => [light.spot_inner_degrees, light.spot_outer_degrees, 0.0, 0.0],
        TEMPERATURE => number(light.temperature.unwrap_or(0.0)),
        SHADOWS => flag(light.shadows),
        SIZE => [light.width, light.height, 0.0, 0.0],
        RADIUS => number(light.radius),
        LENGTH => number(light.length),
        SOFTNESS => number(light.softness),
        _ => return None,
    })
}

fn write(light: &mut Light, what: i32, [x, y, z, _]: [f32; 4]) {
    match what {
        // Another kind starts from that kind's settings, keeping how it looks and how bright it is.
        KIND => {
            let was = *light;
            *light = Light {
                color: was.color,
                temperature: was.temperature,
                intensity: was.intensity,
                shadows: was.shadows,
                ..default_light(kind_of(x))
            };
        }
        COLOR => light.color = Vec3::from(linear([x, y, z].map(|c| c.max(0.0)))),
        INTENSITY => light.intensity = x.max(0.0),
        RANGE => light.range = x.max(0.0),
        SPOT_ANGLES => {
            light.spot_outer_degrees = y.clamp(0.1, 179.0);
            light.spot_inner_degrees = x.clamp(0.0, light.spot_outer_degrees);
        }
        TEMPERATURE => light.temperature = (x > 0.0).then(|| x.clamp(1000.0, 20000.0)),
        SHADOWS => light.shadows = x != 0.0,
        SIZE => (light.width, light.height) = (x.max(0.01), y.max(0.01)),
        RADIUS => light.radius = x.max(0.001),
        LENGTH => light.length = x.max(0.0),
        SOFTNESS => light.softness = x.clamp(0.0, 10.0),
        _ => {}
    }
}

fn kind_of(index: f32) -> LightKind {
    KINDS[(index.max(0.0) as usize).min(KINDS.len() - 1)]
}

fn number(value: f32) -> [f32; 4] {
    [value, 0.0, 0.0, 0.0]
}

fn flag(on: bool) -> [f32; 4] {
    number(if on { 1.0 } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_and_write_a_light_in_srgb() {
        let mut world = World::new();
        let lamp = world.spawn(());
        assert_eq!(values(&mut world, lamp, HAS, None), Some(flag(false)));
        assert_eq!(values(&mut world, lamp, INTENSITY, None), None, "no Light yet");
        values(&mut world, lamp, ADD, Some(number(2.0)));
        assert_eq!(values(&mut world, lamp, KIND, None), Some(number(2.0)), "a spot light");
        values(&mut world, lamp, COLOR, Some([1.0, 0.5, 0.0, 1.0]));
        let [r, g, b, _] = values(&mut world, lamp, COLOR, None).unwrap();
        assert!((r - 1.0).abs() < 1e-4 && (g - 0.5).abs() < 1e-4 && b == 0.0, "{r} {g} {b}");
        assert!((world.get::<Light>(lamp).unwrap().color.y - 0.214).abs() < 1e-3, "kept linear");
        values(&mut world, lamp, SPOT_ANGLES, Some([60.0, 40.0, 0.0, 0.0]));
        assert_eq!(values(&mut world, lamp, SPOT_ANGLES, None), Some([40.0, 40.0, 0.0, 0.0]), "inner within outer");
        values(&mut world, lamp, INTENSITY, Some(number(7.0)));
        values(&mut world, lamp, KIND, Some(number(5.0)));
        let tube = *world.get::<Light>(lamp).unwrap();
        assert_eq!((tube.kind, tube.intensity), (LightKind::Tube, 7.0), "a tube now, as bright");
        assert!(tube.length > 0.0, "with a tube's length");
        values(&mut world, lamp, TEMPERATURE, Some(number(2700.0)));
        assert_eq!(world.get::<Light>(lamp).unwrap().temperature, Some(2700.0));
        values(&mut world, lamp, TEMPERATURE, Some(number(0.0)));
        assert_eq!(world.get::<Light>(lamp).unwrap().temperature, None, "0 for its colour alone");
        assert!(values(&mut world, lamp, REMOVE, Some([0.0; 4])).is_some() && !world.has::<Light>(lamp));
    }
}
