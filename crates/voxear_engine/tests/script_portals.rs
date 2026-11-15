//! Portals from C#: a behaviour makes two that lead to each other, sets
//! what they show and who passes, follows a ball with a camera, throws the
//! ball through, and hears it go. On a copy of Hello. A file of its own, so
//! it runs in a process of its own, since .NET starts only once per process.
//! Skips itself without .NET.

use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec2, Vec3},
};
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{CameraFollows, Portal};
use voxear_scene::{Name, Transform};

const DT: f32 = 1.0 / 60.0;

const SCRIPT: &str = r##"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class PortalScript : Behaviour
{
	private Portal town;
	private Portal camp;
	private int frames;

	protected override void OnEnable()
	{
		Portal.Passed += Heard;
	}

	protected override void OnDisable()
	{
		Portal.Passed -= Heard;
	}

	protected override void Start()
	{
		var size = new Vector2(3f, 4f);
		(town, camp) = Portal.CreatePair(new Vector3(0f, 0f, 30f), Quaternion.Identity, new Vector3(500f, 10f, -200f), Quaternion.CreateFromYawPitchRoll(1.2f, 0f, 0f), size);
		camp.Mist = new Color(1f, 0.5f, 0.25f);
		camp.ViewDistance = 48f;
		// The town's is the project's, until the game's options lower it for every Portal without its own.
		Log.Info($"view {town.ViewDistance is null} {camp.ViewDistance} {Portal.DefaultViewDistance}");
		Portal.DefaultViewDistance = 64f;
		camp.Passes = LayerMask.GetMask("Default");
		var shut = Portal.Create(new Vector3(-40f, 0f, 0f), Quaternion.Identity, new Vector2(2f, 2f));
		shut.Close();
		Log.Info($"made {town.IsValid} {camp.IsValid} {town.To == camp} {camp.To == town} {town.Size} {shut.IsOpen} {shut.To.IsValid} {town.Obj.Name}");
		// A camera that films this Obj.
		var eye = Obj.Find("Eye").Camera;
		eye.Follows = Obj;
		Log.Info($"follows {eye.Follows == Obj}");
	}

	protected override void Update()
	{
		frames++;
		if (frames == 2)
		{
			// The way from here to beyond the town Portal goes through it; one beside it does not.
			var through = Portal.Between(new Vector3(0f, 1f, 33f), new Vector3(0f, 1f, 27f));
			var past = Portal.Between(new Vector3(9f, 1f, 33f), new Vector3(9f, 1f, 27f));
			Log.Info($"between {through == town} {past.IsValid}");
		}
	}

	private void Heard(PortalPassage passage)
	{
		Log.Info($"passed {passage.Obj.Name} {passage.From == town} {passage.To == camp} {passage.WentInAtBack} {passage.Yaw:F1}");
	}
}
"##;

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

#[test]
fn a_script_makes_two_portals_that_lead_to_each_other_and_hears_what_goes_through() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/PortalScript.cs"), SCRIPT).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let added = "\n[[obj]]\nid = 900\nname = \"Gatekeeper\"\nposition = [3.0, 1.0, 2.0]\nbehaviours = [\"Hello.PortalScript\"]\n\
                 \n[[obj]]\nid = 901\nname = \"Eye\"\nposition = [0.0, 2.0, 0.0]\n[obj.camera]\nfov = 60.0\n";
    std::fs::write(&scene, format!("{before}{added}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
        panic!("{error:#}: {}", lines.iter().filter(|line| line.contains("error")).cloned().collect::<Vec<_>>().join("\n"));
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let find = |game: &Game, name: &str| -> Entity {
        game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    for _ in 0..3 {
        game.tick(DT, None);
    }
    let lines = || -> Vec<String> { log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect() };
    let made = "made True True True True <3, 4> False False Portal";
    assert!(lines().iter().any(|line| line == made), "{:?}", lines());
    assert!(lines().iter().any(|line| line == "follows True"), "{:?}", lines());
    assert!(lines().iter().any(|line| line == "view True 48 96"), "{:?}", lines());
    let quality = game.engine.world.resource::<voxear_render::portals::PortalQuality>().copied().unwrap();
    assert_eq!(quality.view_distance, 64.0, "the script set the view distance of Portals without their own");
    assert!(lines().iter().any(|line| line == "between True False"), "{:?}", lines());

    // In the world: two open Portals that lead to each other where the script put them, and a shut one leading nowhere.
    let portals: Vec<(Entity, Portal, Transform)> = game
        .engine
        .world
        .query_ref::<(Entity, &Portal, &Transform)>()
        .into_iter()
        .map(|(entity, portal, transform)| (entity, *portal, *transform))
        .collect();
    assert_eq!(portals.len(), 3);
    let at = |x: f32| portals.iter().find(|(_, _, transform)| transform.position.x == x).unwrap();
    let (town, camp, shut) = (at(0.0), at(500.0), at(-40.0));
    assert_eq!((town.1.to, camp.1.to), (Some(camp.0), Some(town.0)));
    assert_eq!((town.1.size, town.1.open), (Vec2::new(3.0, 4.0), true));
    assert_eq!(camp.2.position, Vec3::new(500.0, 10.0, -200.0));
    assert!(camp.2.rotation.abs_diff_eq(Quat::from_rotation_y(1.2), 1e-4), "{:?}", camp.2.rotation);
    assert_eq!((camp.1.mist, camp.1.view_distance, camp.1.passes), (Vec3::new(1.0, 0.5, 0.25), Some(48.0), 1));
    assert_eq!(town.1.view_distance, None, "the project's");
    assert_eq!((shut.1.open, shut.1.to), (false, None));
    let (keeper, eye) = (find(&game, "Gatekeeper"), find(&game, "Eye"));
    assert_eq!(game.engine.world.get::<CameraFollows>(eye), Some(&CameraFollows(keeper)));

    // A ball going into the front of the town Portal, which faces +z: walked through it, the script hears it come out
    // of the camp's, turned as the camp's Portal is.
    let ball = game.engine.world.spawn((
        Name("Ball".into()),
        voxear_render::PortalTraveller,
        Transform::from_position(Vec3::new(0.5, 1.0, 31.0)),
    ));
    // Moved in Update, as a script moves what it throws.
    game.engine.schedule.add_system("Update", "throw", move |world| {
        if let Some(transform) = world.get_mut::<Transform>(ball) {
            let ahead = transform.rotation * Vec3::NEG_Z;
            transform.position += ahead * 0.1;
        }
    });
    for _ in 0..30 {
        game.tick(DT, None);
    }
    let heard: Vec<String> = lines().into_iter().filter(|line| line.starts_with("passed")).collect();
    assert_eq!(heard.len(), 1, "{heard:?}");
    let yaw = (1.2f32.to_degrees() + 180.0 + 180.0).rem_euclid(360.0) - 180.0;
    assert_eq!(heard[0], format!("passed Ball True True False {yaw:.1}"));
    let landed = game.engine.world.get::<Transform>(ball).unwrap().position;
    assert!(landed.distance(Vec3::new(500.0, 10.0, -200.0)) < 3.0, "by the camp's Portal: {landed:?}");
}
