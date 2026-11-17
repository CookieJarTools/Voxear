//! How plants move, as scripts reach it: `Foliage` in Voxear.Core reads and
//! sets the game's `voxear_render::FoliageMotion` number by number, and reads
//! the wind plants feel now, through the one call set here.

use voxear_ecs::World;
use voxear_render::{FoliageMotion, weather::Weather};

/// Sets what answers `Foliage` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_foliage_call(call);
}

/// `Foliage`'s number `which`, as `voxear_scripting::FoliageCall` numbers
/// them, set to `value` first where one is given and the number can be set.
fn call(world: &mut World, which: i32, value: Option<f32>) -> Option<f32> {
    let now = FoliageMotion::of(world);
    if let Some(value) = value {
        let set = match which {
            0 => FoliageMotion { wind: value, ..now },
            1 => FoliageMotion { bend: value, ..now },
            2 => FoliageMotion { interaction: value >= 0.5, ..now },
            3 => FoliageMotion { push: value, ..now },
            4 => FoliageMotion { spring: value, ..now },
            _ => return None,
        };
        world.insert_resource(set.bounded());
    }
    let motion = FoliageMotion::of(world).bounded();
    let wind = || voxear_render::plants_wind(world, Weather::in_world(world), motion);
    Some(match which {
        0 => motion.wind,
        1 => motion.bend,
        2 => f32::from(u8::from(motion.interaction)),
        3 => motion.push,
        4 => motion.spring,
        5 => wind().direction[0],
        6 => wind().direction[1],
        7 => wind().strength,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_and_set_how_plants_move_number_by_number() {
        let mut world = World::new();
        world.insert_resource(FoliageMotion::default());
        assert_eq!(call(&mut world, 0, None), Some(0.5));
        assert_eq!(call(&mut world, 1, Some(9.0)), Some(FoliageMotion::MOST_BEND), "kept within bounds");
        assert_eq!(call(&mut world, 2, Some(0.0)), Some(0.0));
        assert!(!FoliageMotion::of(&world).interaction);
        assert_eq!(call(&mut world, 7, Some(3.0)), None, "the wind now is read, not set");
        assert_eq!(call(&mut world, 7, None), Some(0.5), "the fair day's");
        assert_eq!(call(&mut world, 5, None), Some(1.0), "blowing along x");
        assert_eq!(call(&mut world, 99, None), None);
    }
}
