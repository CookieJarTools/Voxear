//! Scripts and physics: a crate a script kicks sideways falls through a
//! trigger onto the floor, its behaviour told of each, after a ray and a
//! sphere asked what is where; a character walks across the floor; in 2D, a
//! ball kicked up lands on a ledge; and a script hangs a lamp on a bell,
//! saying so in the view's corner and drawing a solid box round it.
//! Builds a copy of Hello with these added; skips itself without .NET and
//! the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const BUMP: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Bump : Behaviour
{
	protected override void Start()
	{
		var body = Obj.Rigidbody;
		Log.Info($"mass {body.Mass}, sleeping {body.IsSleeping}");
		if (Physics.Raycast(new Vector3(0f, 10f, 0f), -Vector3.UnitY, out var hit))
		{
			Log.Info($"ray met {hit.Obj.Name} at {hit.Point.Y:0.0}");
		}
		Log.Info($"sphere found {Physics.OverlapSphere(new Vector3(0f, -0.5f, 0f), 0.2f).Length}");
		body.AddForce(Vector3.UnitX * 2f, ForceMode.VelocityChange);
	}

	protected override void OnCollisionEnter(Collision collision)
	{
		Log.Info($"hit {collision.Obj.Name}, pushed up {collision.Normal.Y:0}");
	}

	protected override void OnTriggerEnter(Collider other)
	{
		Log.Info($"entered {other.Obj.Name}");
	}

	protected override void OnTriggerExit(Collider other)
	{
		Log.Info($"left {other.Obj.Name}");
	}
}
"#;

const WALK: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Walk : Behaviour
{
	private int frames;

	protected override void Update()
	{
		var walker = Obj.CharacterController;
		walker.SimpleMove(new Vector3(0f, 0f, 2f));
		if (++frames == 60)
		{
			Log.Info($"walker grounded {walker.IsGrounded}, at {Obj.Position.Z:0.0}");
		}
	}
}
"#;

const HOP: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Hop : Behaviour
{
	protected override void Start()
	{
		if (Physics2D.Raycast(new Vector2(20f, 10f), -Vector2.UnitY, out var hit))
		{
			Log.Info($"2D ray met {hit.Obj.Name} at {hit.Point.Y:0.0}");
		}
		Obj.Rigidbody2D.AddForce(Vector2.UnitY * 2f, ForceMode2D.Impulse);
	}

	protected override void OnCollisionEnter2D(Collision2D collision)
	{
		Log.Info($"2D hit {collision.Obj.Name}, pushed up {collision.Normal.Y:0}");
	}
}
"#;

const SWING: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Swing : Behaviour
{
	protected override void Start()
	{
		var joint = Obj.AddJoint(JointType.Ball);
		joint.Anchor = new Vector3(1f, 1f, 0f);
		joint.SetLimits(0f, 1f);
		Log.Info($"joint {joint.Type}, to the world {!joint.ConnectedBody.IsValid}, limits {joint.UseLimits}");
		var lamp = Obj.AddLight(LightKind.Spot);
		lamp.Intensity = 4f;
		lamp.Color = new Color(1f, 0.5f, 0f);
		lamp.OuterSpotAngle = 50f;
		Log.Info($"lamp {lamp.Kind}, {lamp.Intensity}, green {lamp.Color.G:0.00}, outer {lamp.OuterSpotAngle}");
		using (Draw.Duration(60f))
		{
			Draw.ScreenText("bell hung");
			Draw.SolidBox(Obj.Position, Vector3.One);
		}
	}
}
"#;

const OBJS: &str = r#"
[[obj]]
id = 30
name = "Floor"
position = [0.0, -0.5, 0.0]
collider = { size = [30.0, 1.0, 30.0] }

[[obj]]
id = 31
name = "Crate"
position = [0.0, 4.0, 0.0]
collider = {}
rigidbody = { mass = 3.0 }
behaviours = ["Hello.Bump"]

[[obj]]
id = 32
name = "Gate"
position = [0.0, 2.5, 0.0]
collider = { size = [20.0, 0.5, 20.0], trigger = true }

[[obj]]
id = 33
name = "Walker"
position = [-5.0, 0.0, 0.0]
character = {}
behaviours = ["Hello.Walk"]

[[obj]]
id = 36
name = "Bell"
position = [-10.0, 4.0, 0.0]
collider = { shape = "sphere", radius = 0.25 }
rigidbody = {}
behaviours = ["Hello.Swing"]

[[obj]]
id = 34
name = "Ledge"
position = [20.0, -0.5, 0.0]
collider2d = { size = [4.0, 1.0] }

[[obj]]
id = 35
name = "Pip"
position = [20.0, 1.0, 0.0]
collider2d = { shape = "circle", radius = 0.25 }
rigidbody2d = { mass = 2.0 }
behaviours = ["Hello.Hop"]
"#;

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

#[test]
fn scripts_ask_push_and_hear_physics() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Bump.cs"), BUMP).unwrap();
    std::fs::write(dir.path().join("Scripts/Walk.cs"), WALK).unwrap();
    std::fs::write(dir.path().join("Scripts/Hop.cs"), HOP).unwrap();
    std::fs::write(dir.path().join("Scripts/Swing.cs"), SWING).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();

    // Two seconds, before Hello's own game manager changes the level.
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("mass 3, sleeping False"), "{lines:?}");
    assert!(said("ray met Crate at 4.5"), "Start finds the colliders where the scene put them: {lines:?}");
    assert!(said("sphere found 1"), "{lines:?}");
    assert!(said("entered Gate") && said("left Gate"), "{lines:?}");
    assert!(said("hit Floor, pushed up 1"), "{lines:?}");
    assert!(said("walker grounded True, at 2.0"), "{lines:?}");
    assert!(said("2D ray met Pip at 1.3"), "{lines:?}");
    assert!(said("2D hit Ledge, pushed up 1"), "the ball came down on the ledge: {lines:?}");
    assert!(said("joint Ball, to the world True, limits True"), "{lines:?}");
    assert!(said("lamp Spot, 4, green 0.50, outer 50"), "{lines:?}");
    let corner = voxear_core::debug_draw::texts().into_iter().find(|text| text.on_screen);
    assert_eq!(corner.map(|text| text.text), Some("bell hung".into()), "a line in the view's corner");
    assert_eq!(voxear_core::debug_draw::triangles().len(), 12, "a solid box round the bell");

    // The kick sideways moved the crate along X as it fell.
    let world = &game.engine.world;
    let crate_at = world
        .query_ref::<(&voxear_scene::Name, &voxear_scene::Transform)>()
        .into_iter()
        .find(|(name, _)| name.0 == "Crate")
        .map(|(_, transform)| transform.position)
        .unwrap();
    assert!(crate_at.x > 1.0 && (crate_at.y - 0.5).abs() < 0.05, "{crate_at}");

    // The bell hangs from the ball joint its script made, a step right of it and one up.
    let bell = world
        .query_ref::<(&voxear_scene::Name, &voxear_scene::Transform)>()
        .into_iter()
        .find(|(name, _)| name.0 == "Bell")
        .map(|(_, transform)| transform.position)
        .unwrap();
    let anchor = voxear_core::math::Vec3::new(-9.0, 5.0, 0.0);
    assert!((bell.distance(anchor) - 2f32.sqrt()).abs() < 0.05, "held to where its anchor was: {bell}");
}
