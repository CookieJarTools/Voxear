//! Bake Reflection Probes on a small level of a floor and a lamp: a probe
//! made as the + menu makes one, saved, baked into a `.probes` file beside
//! the level that is smaller than its texels, and given back to the probe
//! from that file when the level's scenes are read again. Skips itself
//! without a GPU.

use std::{sync::mpsc::TryRecvError, time::Instant};

use serde_json::{Value, json};
use voxear_core::logging::LogCapture;
use voxear_engine::{Project, reflection_bakes};
use voxear_render::{PROBE_SIZE, ReflectionProbe, ReflectionProbeBake, Renderer, RendererSettings};
use voxear_scene::{Destroying, Name};

const SIZE: (u32, u32) = (1280, 800);

const ROOM: &str = r#"
format = 1

[[obj]]
id = 1
name = "Main Camera"
position = [0.0, 1.5, -6.0]
[obj.camera]
fov = 60.0

[[obj]]
id = 2
name = "Floor"
position = [0.0, -0.5, 0.0]
scale = [12.0, 1.0, 12.0]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 3
name = "Lamp"
position = [0.0, 3.0, 0.0]
[obj.light]
type = "point"
intensity = 40.0
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("voxear.toml", "[project]\nname = \"Room\"\nstart_level = \"Levels/Main.level\"\n");
    write("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
    write("Scenes/Main.scene", ROOM);
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// A call made as a tool makes it, answered once frames have run it.
fn done(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, call: &str, args: Value) -> Value {
    let answer = editor.call(call, args);
    let began = Instant::now();
    loop {
        frame(editor, renderer);
        match answer.try_recv() {
            Ok(answered) => return answered.unwrap_or_else(|error| panic!("{call}: {error}")),
            Err(TryRecvError::Empty) => assert!(began.elapsed().as_secs() < 120, "{call} was not answered"),
            Err(TryRecvError::Disconnected) => panic!("{call} was dropped"),
        }
    }
}

fn probe(editor: &voxear_editor::Editor) -> voxear_ecs::Entity {
    let world = &editor.game.engine.world;
    // One read again replaces the Obj before, which goes at the end of a frame.
    let probes = world.query_ref::<(voxear_ecs::Entity, &Name, &ReflectionProbe)>().without::<Destroying>();
    let found: Vec<_> =
        probes.into_iter().filter(|(_, name, _)| name.0 == "Reflection Probe").map(|(e, ..)| e).collect();
    assert_eq!(found.len(), 1, "one probe");
    found[0]
}

#[test]
fn bake_reflection_probes_writes_the_open_levels_probes_beside_it_and_they_load_with_it() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Reflection Probe", "position": [0.0, 1.0, 0.0] }),
    );
    assert!(made["obj"].is_object() || made["obj"].is_string() || made["obj"].is_number(), "{made}");
    let made = probe(&editor);
    let scale = editor.game.engine.world.get::<voxear_scene::Transform>(made).unwrap().scale;
    assert_eq!(scale.to_array(), [8.0, 4.0, 8.0], "the box a room's probe starts with");
    done(&mut editor, &mut renderer, "save", json!({}));
    let saved = std::fs::read_to_string(dir.path().join("Scenes/Main.scene")).unwrap();
    assert!(saved.contains("[obj.reflection_probe]"), "{saved}");

    let baked = done(&mut editor, &mut renderer, "reflection_probes.bake", json!({}));
    assert_eq!(baked["probes"], 1, "{baked}");
    assert_eq!(baked["file"], "Levels/Main.probes");
    let file = dir.path().join("Levels/Main.probes");
    let bytes = std::fs::read(&file).expect("the probes are written beside the level");
    assert_eq!(bytes.len() as u64, baked["bytes"].as_u64().unwrap());
    let texels = (PROBE_SIZE * PROBE_SIZE * 6) as usize;
    eprintln!("one probe baked in {} bytes, of {} as RGB9E5", bytes.len(), texels * 4);
    assert!(bytes.len() < texels * 4, "smaller than its texels: {} bytes", bytes.len());
    let read = reflection_bakes::read(&bytes).unwrap();
    assert_eq!((read.len(), read[0].pixels.size, read[0].pixels.texels.len()), (1, PROBE_SIZE, texels));
    assert!(editor.game.engine.world.has::<ReflectionProbeBake>(made), "the probe shows its bake at once");
    assert!(editor.game.content.db().guid_of("Levels/Main.probes").is_some(), "an asset of the project");

    // Read again from its files, the probe is a new Obj, and has its bake from the level's file.
    done(&mut editor, &mut renderer, "scene.reload", json!({}));
    let again = probe(&editor);
    assert_ne!(again, made, "made again from the scene");
    let bake = editor.game.engine.world.get::<ReflectionProbeBake>(again).expect("the bake, from Levels/Main.probes");
    assert_eq!(bake.0.texels.len(), texels);
    // What it was baked as is what it shows: lit by the lamp, not black.
    let lit = bake.0.texels.iter().filter(|t| reflection_bakes::half_to_f32(t[0]) > 0.0).count();
    assert!(lit > texels / 10, "{lit} of {texels} texels lit");
}
