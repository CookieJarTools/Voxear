//! Bake Lighting on a small level, a room of slabs with a Lightmap each, a
//! lamp and a Light Probe Group, as a tool bakes it: `lighting.bake` writes
//! a `.lightmaps` file beside the level and lights the level with it at once,
//! the level read again from its files has the same bake, texel for texel,
//! and `lighting.probe_light` reads the probes' light. Skips itself without
//! a GPU.

use std::{collections::BTreeMap, sync::mpsc::TryRecvError, time::Instant};

use serde_json::{Value, json};
use voxear_core::logging::LogCapture;
use voxear_engine::{Project, light_bakes};
use voxear_render::{BakedLighting, LightmapBake, Renderer, RendererSettings};
use voxear_scene::{Destroying, Name};

const SIZE: (u32, u32) = (1280, 800);

fn slab(id: u32, name: &str, centre: [f32; 3], size: [f32; 3]) -> String {
    format!(
        "\n[[obj]]\nid = {id}\nname = \"{name}\"\nposition = {centre:?}\nscale = {size:?}\n[obj.renderer]\nmesh = \"builtin:cube\"\n[obj.lightmap]\n"
    )
}

fn project() -> tempfile::TempDir {
    let mut scene = String::from(
        r#"format = 1

[post.global_illumination]
enabled = true
source = "baked"
samples = 32
texels_per_metre = 4.0

[[obj]]
id = 1
name = "Main Camera"
position = [0.0, 1.5, -2.5]
[obj.camera]
fov = 60.0

[[obj]]
id = 2
name = "Lamp"
position = [0.0, 2.5, 0.0]
[obj.light]
type = "point"
intensity = 40.0

[[obj]]
id = 3
name = "Probes"
position = [0.0, 1.5, 0.0]
scale = [5.0, 2.0, 5.0]
[obj.light_probe_group]
fill = 2.5
"#,
    );
    scene += &slab(10, "Floor", [0.0, -0.1, 0.0], [6.4, 0.2, 6.4]);
    scene += &slab(11, "Ceiling", [0.0, 3.1, 0.0], [6.4, 0.2, 6.4]);
    scene += &slab(12, "West", [-3.1, 1.5, 0.0], [0.2, 3.0, 6.4]);
    scene += &slab(13, "East", [3.1, 1.5, 0.0], [0.2, 3.0, 6.4]);
    scene += &slab(14, "North", [0.0, 1.5, 3.1], [6.0, 3.0, 0.2]);
    scene += &slab(15, "South", [0.0, 1.5, -3.1], [6.0, 3.0, 0.2]);
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("voxear.toml", "[project]\nname = \"Room\"\nstart_level = \"Levels/Main.level\"\n");
    write("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
    write("Scenes/Main.scene", &scene);
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
            Err(TryRecvError::Empty) => assert!(began.elapsed().as_secs() < 180, "{call} was not answered"),
            Err(TryRecvError::Disconnected) => panic!("{call} was dropped"),
        }
    }
}

/// Each slab's place in the lightmaps, by its name.
fn places(editor: &voxear_editor::Editor) -> BTreeMap<String, LightmapBake> {
    let world = &editor.game.engine.world;
    let baked = world.query_ref::<(&Name, &LightmapBake)>().without::<Destroying>();
    baked.into_iter().map(|(name, bake)| (name.0.clone(), bake.clone())).collect()
}

#[test]
fn bake_lighting_writes_the_levels_lightmaps_beside_it_and_they_load_with_it_as_baked() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let status = done(&mut editor, &mut renderer, "lighting.bake_status", json!({}));
    assert_eq!((status["baking"].as_bool(), status["lightmaps"].as_u64()), (Some(false), Some(0)), "{status}");

    let baked = done(&mut editor, &mut renderer, "lighting.bake", json!({}));
    eprintln!("baked: {baked}");
    assert_eq!(baked["file"], "Levels/Main.lightmaps");
    assert_eq!(baked["parts"], 6, "a place for each slab: {baked}");
    assert!(baked["probes"].as_u64().unwrap() >= 8, "{baked}");
    let bytes = std::fs::read(dir.path().join("Levels/Main.lightmaps")).expect("written beside the level");
    assert_eq!(bytes.len() as u64, baked["bytes"].as_u64().unwrap());
    let file = light_bakes::read(&bytes).unwrap();
    assert_eq!(file.parts.len(), 6);
    assert!(editor.game.content.db().guid_of("Levels/Main.lightmaps").is_some(), "an asset of the project");
    let world = &editor.game.engine.world;
    let lit = world.resource::<BakedLighting>().expect("the level is lit by its bake at once").clone();
    let before = places(&editor);
    assert_eq!(before.len(), 6, "every slab has its place");

    // The lamp's light, bounced, at the room's middle: brighter from above, where the ceiling is lit from close by.
    let probe = done(&mut editor, &mut renderer, "lighting.probe_light", json!({ "position": [0.0, 1.0, 0.0] }));
    assert_eq!(probe["found"], true, "{probe}");
    assert!(probe["average"][0].as_f64().unwrap() > 0.0, "{probe}");

    // Read again from its files, the level has the same bake, lightmap for lightmap and place for place.
    done(&mut editor, &mut renderer, "scene.reload", json!({}));
    frame(&mut editor, &mut renderer);
    let world = &editor.game.engine.world;
    let again = world.resource::<BakedLighting>().expect("the bake, from Levels/Main.lightmaps");
    assert_ne!(again.id, lit.id, "read afresh");
    assert!(again.atlases == lit.atlases, "the lightmaps as baked, texel for texel");
    assert_eq!(*again.probes, *lit.probes, "and the probes");
    assert_eq!(places(&editor), before, "each slab where it was");
    let status = done(&mut editor, &mut renderer, "lighting.bake_status", json!({}));
    assert_eq!(status["lightmaps"].as_u64(), Some(lit.atlases.len() as u64), "{status}");
}
