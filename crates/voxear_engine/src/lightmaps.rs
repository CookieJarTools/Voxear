//! Baked lighting for scripts: `Lightmap`, `LightProbeGroup` and
//! `BakedLighting` in Voxear.Core read and write through here. What they
//! are is in `voxear_render::lightmaps`; the bake, in
//! `voxear_render::lightmapper`; the level's file of it, in
//! [`crate::light_bakes`].

use std::sync::Mutex;

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{BakedLighting, LightBakeState, LightProbeGroup, Lightmap, LightmapBake, ProbeFill};
use voxear_scripting::ComponentKind;

/// Sets what answers `Lightmap`, `LightProbeGroup` and `BakedLighting` in
/// scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Lightmap, lightmap_values);
    voxear_scripting::set_kind_values(ComponentKind::LightProbeGroup, group_values);
    voxear_scripting::set_kind_values(ComponentKind::BakedLighting, baked_values);
}

// What the values read or write, as Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
// A Lightmap's.
const SCALE: i32 = 3;
const CAST: i32 = 4;
const RECEIVE: i32 = 5;
const BAKED: i32 = 6;
// A Light Probe Group's.
const FILL: i32 = 3;
const POINT_COUNT: i32 = 4;
const ADD_POINT: i32 = 5;
const CLEAR_POINTS: i32 = 6;
const PROBE_COUNT: i32 = 7;
/// Points from here on read and write the point of that number.
const POINT: i32 = 100;
// The level's baked lighting's.
const LIGHTMAP_COUNT: i32 = 3;
const LIGHT_PROBE_COUNT: i32 = 4;
const BAKING: i32 = 5;
/// Where `HARMONIC` and later read the light probes' light at, written first.
const ASK_AT: i32 = 6;
/// The nine harmonics' red, green and blue, from here on.
const HARMONIC: i32 = 10;

fn number(n: f32) -> [f32; 4] {
    [n, 0.0, 0.0, 0.0]
}

fn flag(on: bool) -> [f32; 4] {
    number(f32::from(u8::from(on)))
}

fn lightmap_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<Lightmap>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<Lightmap>(entity) {
                world.insert(entity, Lightmap::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<LightmapBake>(entity);
            return world.remove::<Lightmap>(entity).map(|_| [0.0; 4]);
        }
        (BAKED, None) => return Some(flag(world.has::<LightmapBake>(entity))),
        _ => {}
    }
    let lightmap = world.get_mut::<Lightmap>(entity)?;
    match written {
        None => Some(match what {
            SCALE => number(lightmap.scale),
            CAST => flag(lightmap.cast),
            RECEIVE => flag(lightmap.receive),
            _ => return None,
        }),
        Some(value @ [x, ..]) => {
            match what {
                SCALE => lightmap.scale = x.clamp(0.0, 64.0),
                CAST => lightmap.cast = x != 0.0,
                RECEIVE => lightmap.receive = x != 0.0,
                _ => return None,
            }
            Some(value)
        }
    }
}

fn group_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<LightProbeGroup>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<LightProbeGroup>(entity) {
                world.insert(entity, LightProbeGroup::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<LightProbeGroup>(entity).map(|_| [0.0; 4]),
        (PROBE_COUNT, None) => {
            let group = world.get::<LightProbeGroup>(entity)?;
            let matrix = voxear_scene::world_matrix(world, entity);
            return Some(number(group.probes(&matrix).len() as f32));
        }
        _ => {}
    }
    let group = world.get_mut::<LightProbeGroup>(entity)?;
    match written {
        None => match what {
            FILL => Some(number(group.fill.map_or(0.0, |fill| fill.spacing))),
            POINT_COUNT => Some(number(group.points.len() as f32)),
            index if index >= POINT => group.points.get((index - POINT) as usize).map(|p| p.extend(0.0).to_array()),
            _ => None,
        },
        Some(value @ [x, y, z, _]) => {
            match what {
                FILL => group.fill = (x > 0.0).then(|| ProbeFill { spacing: x.clamp(0.05, 1000.0) }),
                ADD_POINT => group.points.push(Vec3::new(x, y, z)),
                CLEAR_POINTS => group.points.clear(),
                index if index >= POINT => *group.points.get_mut((index - POINT) as usize)? = Vec3::new(x, y, z),
                _ => return None,
            }
            Some(value)
        }
    }
}

/// Where scripts last asked for the light probes' light.
static ASKED: Mutex<Vec3> = Mutex::new(Vec3::ZERO);

fn baked_values(world: &mut World, _: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let baked = world.resource::<BakedLighting>();
    match (what, written) {
        (LIGHTMAP_COUNT, None) => Some(number(baked.map_or(0, |b| b.atlases.len()) as f32)),
        (LIGHT_PROBE_COUNT, None) => Some(number(baked.map_or(0, |b| b.probes.positions.len()) as f32)),
        (BAKING, None) => {
            let state = world.resource::<LightBakeState>().copied().unwrap_or_default();
            Some([f32::from(u8::from(state.baking)), state.progress, 0.0, 0.0])
        }
        (ASK_AT, Some(value @ [x, y, z, _])) => {
            *ASKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Vec3::new(x, y, z);
            Some(value)
        }
        (index, None) if (HARMONIC..HARMONIC + 9).contains(&index) => {
            let at = *ASKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let light = baked?.probes.light_at(at)?;
            Some(Vec3::from(light.0[(index - HARMONIC) as usize]).extend(0.0).to_array())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use voxear_render::{BakedProbes, ProbeLight};

    use super::*;

    #[test]
    fn a_script_gives_an_obj_a_lightmap_and_a_probe_group_and_asks_the_probes_light() {
        let mut world = World::new();
        let wall = world.spawn(());
        lightmap_values(&mut world, wall, ADD, Some([0.0; 4]));
        lightmap_values(&mut world, wall, SCALE, Some(number(2.0)));
        lightmap_values(&mut world, wall, CAST, Some(number(0.0)));
        assert_eq!(*world.get::<Lightmap>(wall).unwrap(), Lightmap { scale: 2.0, cast: false, receive: true });
        assert_eq!(lightmap_values(&mut world, wall, BAKED, None), Some(flag(false)), "not baked yet");

        let room = world.spawn(());
        group_values(&mut world, room, ADD, Some([0.0; 4]));
        group_values(&mut world, room, FILL, Some(number(0.0)));
        group_values(&mut world, room, ADD_POINT, Some([1.0, 2.0, 3.0, 0.0]));
        assert_eq!(group_values(&mut world, room, POINT, None), Some([1.0, 2.0, 3.0, 0.0]));
        assert_eq!(group_values(&mut world, room, PROBE_COUNT, None), Some(number(1.0)), "the point alone, no fill");

        let none = Entity::from_bits(0);
        assert_eq!(baked_values(&mut world, none, LIGHT_PROBE_COUNT, None), Some(number(0.0)));
        let light = ProbeLight(std::array::from_fn(|i| [i as f32, 0.5, 0.25]));
        let probes = BakedProbes { positions: vec![Vec3::ZERO], lights: vec![light] };
        world.insert_resource(BakedLighting::new(Vec::new(), probes));
        baked_values(&mut world, none, ASK_AT, Some([0.5, 0.0, 0.0, 0.0]));
        assert_eq!(
            baked_values(&mut world, none, HARMONIC + 4, None),
            Some([4.0, 0.5, 0.25, 0.0]),
            "one probe's light anywhere"
        );
    }
}
