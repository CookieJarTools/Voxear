//! Scripts and Motion Sets: a character's script plays an action of its
//! body's set, crouches, and hears the action's mark through the body's
//! parent; marks and actions picked as fields compare with what it is told.
//! Builds a copy of Hello with these added; skips itself without .NET and
//! the managed build.

use voxear_core::logging::LogCapture;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::ModelRenderer;
use voxear_scene::Name;

const HERO: &str = r#"
[[state]]
name = "Walking"
role = "grounded"
[[state.clip]]
clip = "Models/Walks.gltf#clip:Walk"

[[state]]
name = "Crouching"
role = "crouched"
[[state.clip]]
clip = "Models/Walks.gltf#clip:Sneak"

[[action]]
name = "Lunge"
clip = "Models/Walks.gltf#clip:Walk"
blend_in = 0.0
[[action.mark]]
name = "Strike"
time = 0.5
"#;

/// A root with hips under it and two clips a second long, moving the root.
const WALKS: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Walks.bin", "byteLength": 56 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
    { "buffer": 0, "byteOffset": 8, "byteLength": 24 },
    { "buffer": 0, "byteOffset": 32, "byteLength": 24 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] },
    { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" },
    { "bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3" }
  ],
  "nodes": [{ "name": "root", "children": [1] }, { "name": "hips", "translation": [0, 0.9, 0] }],
  "animations": [
    {
      "name": "Walk",
      "samplers": [{ "input": 0, "output": 1 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    },
    {
      "name": "Sneak",
      "samplers": [{ "input": 0, "output": 2 }],
      "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
    }
  ],
  "scenes": [{ "nodes": [0] }]
}"#;

const FIGHTER: &str = r#"using Voxear;

namespace Hello;

public sealed class Fighter : Behaviour
{
	[SerializeField] private Obj body;
	[SerializeField] private MotionAction lunge;
	[SerializeField] private MotionMark strike;
	private int frames;

	protected override void Update()
	{
		var matcher = body.MotionMatcher;
		frames++;
		if (frames == 5)
		{
			matcher.RootMotion = true;
			matcher.Play(lunge);
			Log.Info($"playing {matcher.ActionPlaying}, same {matcher.ActionPlaying == lunge}, root {matcher.RootMotion}");
		}
		if (frames == 20)
		{
			Log.Info($"in {matcher.CurrentState.Name}, {matcher.State}, {matcher.ActionTime:0.00}s in");
			matcher.State = MotionState.Crouched;
			Log.Info($"now {matcher.CurrentState.Name}");
		}
	}

	protected override void OnMotionMark(MotionMark mark)
	{
		Log.Info($"mark {mark.Name}, the strike {mark == strike}");
	}
}
"#;

const OBJS: &str = r#"

[[obj]]
id = 40
name = "Fighter"
position = [5.0, 0.0, 5.0]
behaviours = ["Hello.Fighter"]

[obj.fields."Hello.Fighter"]
body = { obj = 41 }
lunge = "Sets/Hero.motionset#action:Lunge"
strike = "Sets/Hero.motionset#mark:Strike"

[[obj]]
id = 41
name = "Body"
parent = 40

[obj.model]
model = "Models/Walks.gltf"

[obj.motion_matcher]
motion_set = "Sets/Hero.motionset"
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
fn scripts_play_actions_change_state_and_hear_marks() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::create_dir_all(dir.path().join("Models")).unwrap();
    std::fs::create_dir_all(dir.path().join("Sets")).unwrap();
    std::fs::write(dir.path().join("Models/Walks.gltf"), WALKS).unwrap();
    let values = [0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5];
    let data: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(dir.path().join("Models/Walks.bin"), data).unwrap();
    std::fs::write(dir.path().join("Sets/Hero.motionset"), HERO).unwrap();
    std::fs::write(dir.path().join("Scripts/Fighter.cs"), FIGHTER).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // Headless, nothing draws the model: the body plays its clips from the file as read without the GPU.
    let walks = game.content.parse("Models/Walks.gltf").unwrap().guid().unwrap();
    let model = ModelRenderer { animation: game.content.clip_file(walks), ..ModelRenderer::new(Vec::new()) };
    let world = &mut game.engine.world;
    let body = world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, name)| name.0 == "Body").unwrap().0;
    world.insert(body, model);

    for _ in 0..90 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().filter(|line| *line == text).count();
    assert_eq!(said("playing Lunge, same True, root True"), 1, "{lines:?}");
    assert_eq!(said("in Walking, Grounded, 0.25s in"), 1, "{lines:?}");
    assert_eq!(said("now Crouching"), 1, "{lines:?}");
    assert_eq!(said("mark Strike, the strike True"), 1, "told once, on the body's parent: {lines:?}");
}
