//! A script gives an Obj a Block Mob and a Mob Motion and tells them what
//! to do: where to walk, whom to follow, where to look, to attack. Builds a
//! copy of Hello with one more script; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    animation::MobMotion,
    block_mob::{BlockMob, MobGoal},
    physics::CharacterController,
    scripts,
};
use voxear_scene::{Name, Transform};

const HERDER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Herder : SystemBase
{
	private int frame;

	protected override void Update()
	{
		var sheep = Obj.Find("Sheep");
		frame++;
		if (frame == 1)
		{
			Log.Info($"before: {sheep.BlockMob.IsValid} {sheep.MobMotion.IsValid} {sheep.BlockMob.Arrived}");
			var mob = sheep.AddBlockMob();
			mob.Speed = 3.5f;
			mob.Swims = false;
			mob.MoveTo(new Vector3(4f, 0f, 2f));
			var motion = sheep.AddMobMotion();
			motion.LegSwing = 25f;
			motion.LookAt(new Vector3(1f, 2f, 3f));
			motion.Attack();
			Log.Info($"added: {mob.IsValid} {mob.Goal} {mob.Speed} {mob.Swims} {sheep.CharacterController.IsValid} {motion.IsValid} {motion.IsLooking} {motion.LegSwing}");
		}
		if (frame == 2)
		{
			sheep.BlockMob.Follow(Obj.Find("Dog"), 1.5f);
			sheep.MobMotion.LookAhead();
			Log.Info($"following: {sheep.BlockMob.Goal} {sheep.MobMotion.IsLooking}");
		}
		if (frame == 3)
		{
			sheep.BlockMob.Wander(6f);
			Log.Info($"wandering: {sheep.BlockMob.Goal}");
		}
		if (frame == 4)
		{
			sheep.BlockMob.Stop();
			sheep.MobMotion.Remove();
			Log.Info($"stopped: {sheep.BlockMob.Goal} {sheep.BlockMob.Arrived} {sheep.MobMotion.IsValid}");
		}
	}
}
"#;

#[test]
fn a_script_gives_an_obj_a_block_mob_and_a_mob_motion_and_tells_them_what_to_do() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Herder.cs"), HERDER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let at = Transform::new(Vec3::new(0.0, 1.0, 0.0), Default::default(), Vec3::ONE);
    let sheep = game.engine.world.spawn((Name("Sheep".into()), at));
    let dog = game.engine.world.spawn((Name("Dog".into()), Transform::from_position(Vec3::new(8.0, 0.0, 0.0))));
    let lines =
        |log: &LogCapture| -> Vec<String> { log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect() };
    let mob = |game: &Game, entity: Entity| game.engine.world.get::<BlockMob>(entity).cloned();

    game.tick(1.0 / 60.0, None);
    let said = lines(&log);
    assert!(said.iter().any(|l| l == "before: False False True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "added: True MoveTo 3.5 False True True True 25"), "{said:#?}");
    let made = mob(&game, sheep).expect("the script gave it a Block Mob");
    assert_eq!((made.speed, made.swims), (3.5, false));
    assert_eq!(made.goal, MobGoal::To(Vec3::new(4.0, 0.0, 2.0)));
    assert!(game.engine.world.has::<CharacterController>(sheep), "and the controller it walks with");
    let motion = game.engine.world.get::<MobMotion>(sheep).expect("and a Mob Motion");
    assert_eq!((motion.leg_swing, motion.look_at), (25.0, Some(Vec3::new(1.0, 2.0, 3.0))));

    game.tick(1.0 / 60.0, None);
    assert!(lines(&log).iter().any(|l| l == "following: Follow False"), "{:#?}", lines(&log));
    assert_eq!(mob(&game, sheep).unwrap().goal, MobGoal::Follow { obj: dog, near: 1.5 }, "the very Obj, by its number");
    assert_eq!(game.engine.world.get::<MobMotion>(sheep).unwrap().look_at, None);

    game.tick(1.0 / 60.0, None);
    assert!(lines(&log).iter().any(|l| l == "wandering: Wander"));
    assert_eq!(mob(&game, sheep).unwrap().goal, MobGoal::Wander { radius: 6.0 });

    game.tick(1.0 / 60.0, None);
    assert!(lines(&log).iter().any(|l| l == "stopped: Stand True False"), "{:#?}", lines(&log));
    assert!(!game.engine.world.has::<MobMotion>(sheep));
}

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}
