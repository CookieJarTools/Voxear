//! A Gun Handling file in a project: read for the guns that pick it,
//! everyone else's guns handled as usual, and read again once saved.

use voxear_ecs::World;
use voxear_engine::Content;
use voxear_engine::guns::Gun;
use voxear_engine::guns::handling::{give_handlings, handling_of};

const TUNED: &str = "kind = \"bolt_action_rifle\"\n[poses]\nhip = { at = [0.0, -0.02, 0.0] }\n";

#[test]
fn a_gun_picks_its_handling_from_the_project_and_takes_it_up_again_once_saved() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Rifle.gunhandling"), TUNED).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let reference = content.parse("Rifle.gunhandling").unwrap().to_string();
    let picked = world.spawn((Gun { handling: reference.clone(), ..Gun::default() },));
    let usual = world.spawn((Gun::default(),));
    give_handlings(&mut world, &mut content);
    let handling = |world: &World, gun| handling_of(world, world.get::<Gun>(gun).unwrap());
    assert_eq!(handling(&world, picked).poses.hip.at, [0.0, -0.02, 0.0]);
    assert_eq!(handling(&world, usual).poses.hip.at, [0.0; 3], "a gun picking none is handled as usual");

    // Saved, as its Inspector does: forgotten, and read again.
    std::fs::write(dir.path().join("Rifle.gunhandling"), TUNED.replace("-0.02", "-0.05")).unwrap();
    let guid = content.parse(&reference).unwrap().guid().unwrap();
    content.forget_gun_handling(guid);
    give_handlings(&mut world, &mut content);
    assert_eq!(handling(&world, picked).poses.hip.at, [0.0, -0.05, 0.0]);
}
