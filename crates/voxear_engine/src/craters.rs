//! Blasting craters into the level's Crater Field (`voxear_render::craters`),
//! for `Craters` in scripts: each blast adds a crater to the field's list,
//! which the renderer stamps into the ground.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{Crater, CraterField, CraterMarks};
use voxear_scene::Destroying;

/// Sets what answers `Craters` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_crater_actions(act);
}

/// A script's blast (0: centre, radius, depth), which gives back whether
/// there was a field to dig into, or its clearing every crater (1).
fn act(world: &mut World, what: i32, v: [f32; 7]) -> f32 {
    match what {
        0 => f32::from(u8::from(blast(world, Vec3::new(v[0], v[1], v[2]), v[3], v[4]))),
        _ => {
            clear(world);
            1.0
        }
    }
}

fn field(world: &World) -> Option<Entity> {
    let query = world.query_ref::<(Entity, &CraterField)>();
    query.without::<Destroying>().into_iter().next().map(|(entity, _)| entity)
}

/// A crater `radius` metres wide and `depth` deep at `centre`, in the
/// level's Crater Field: whether it has one.
pub fn blast(world: &mut World, centre: Vec3, radius: f32, depth: f32) -> bool {
    let Some(entity) = field(world) else { return false };
    if !world.has::<CraterMarks>(entity) {
        world.insert(entity, CraterMarks::default());
    }
    let Some(marks) = world.get_mut::<CraterMarks>(entity) else { return false };
    marks.blast(Crater { centre, radius, depth });
    true
}

/// Fills every crater in the level's Crater Field back in.
pub fn clear(world: &mut World) {
    if let Some(marks) = field(world).and_then(|entity| world.get_mut::<CraterMarks>(entity)) {
        marks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_scene::Transform;

    #[test]
    fn blasts_dig_into_the_field_and_need_one() {
        let mut world = World::new();
        assert!(!blast(&mut world, Vec3::ZERO, 2.0, 1.0), "no field to dig into");
        let entity = world.spawn((CraterField::default(), Transform::default()));
        assert!(act(&mut world, 0, [1.0, 0.0, 2.0, 3.0, 0.5, 0.0, 0.0]) == 1.0);
        let marks = world.get::<CraterMarks>(entity).unwrap();
        assert_eq!(marks.craters, [Crater { centre: Vec3::new(1.0, 0.0, 2.0), radius: 3.0, depth: 0.5 }]);
        act(&mut world, 1, [0.0; 7]);
        assert!(world.get::<CraterMarks>(entity).unwrap().craters.is_empty());
    }
}
