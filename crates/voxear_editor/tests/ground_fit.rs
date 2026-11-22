//! Ground Fits in the editor, driven as `vox editor call` drives it, on a
//! world of blocks without end from the Infinite World template, its scripts
//! left out: a fit added and set field by field, each change one step to
//! undo; set down on the ground without its own cut; and listed among the
//! world's layers, which the Bare Ground view hides. Skips itself without a
//! GPU.

use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use voxear_core::logging::LogCapture;
use voxear_editor::Editor;
use voxear_engine::{
    Project,
    new_project::{NewProject, create},
};
use voxear_render::{Renderer, RendererSettings};

const SIZE: (u32, u32) = (1280, 800);
const PATIENCE: Duration = Duration::from_secs(120);

fn frames(editor: &mut Editor, renderer: &mut Renderer, count: u32) {
    for _ in 0..count {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    }
}

/// A call made from the test, done, once frames have run it.
fn done(editor: &mut Editor, renderer: &mut Renderer, call: &str, args: Value) -> Value {
    let answer = editor.call(call, args);
    let began = Instant::now();
    loop {
        frames(editor, renderer, 1);
        match answer.try_recv() {
            Ok(answered) => return answered.unwrap_or_else(|error| panic!("{call}: {error}")),
            Err(TryRecvError::Empty) => assert!(began.elapsed() < PATIENCE, "{call} was not answered"),
            Err(TryRecvError::Disconnected) => panic!("{call} was dropped"),
        }
    }
}

/// An editor on a new Infinite World project without its scripts, or none
/// without a GPU; and the folder it is in.
fn open() -> Option<(tempfile::TempDir, Renderer, Editor)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = dir.path().join("Fits");
    let request = NewProject {
        folder: folder.clone(),
        name: "Fits".into(),
        template: "infinite-world".into(),
        setup: None,
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    create(&request, &mut |_| {}).unwrap();
    // Without its scripts: a build would outlive the test and its folder.
    let file = folder.join("voxear.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
    let scene_path = folder.join("Scenes/Infinite World.scene");
    let mut scene = voxear_engine::scene_file::read(&scene_path).unwrap();
    for obj in &mut scene.objs {
        obj.behaviours.clear();
        obj.fields.clear();
        obj.enabled.clear();
    }
    voxear_engine::scene_file::write(&scene_path, &scene).unwrap();
    // A small house of a kit, 4 m square, facing +z.
    std::fs::create_dir_all(folder.join("Kits")).unwrap();
    let hut = "format = 1\n\n[[obj]]\nid = 1\nname = \"Hut\"\nplacement = { size = [4.0, 4.0] }\n";
    std::fs::write(folder.join("Kits/Hut.prefab"), hut).unwrap();

    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let project = Project::open(&folder).unwrap();
    let mut editor = Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frames(&mut editor, &mut renderer, 2);
    Some((dir, renderer, editor))
}

#[test]
fn houses_are_lined_up_along_a_road_facing_it_and_undone_as_one_step() {
    let Some((dir, mut renderer, mut editor)) = open() else { return };
    let _ = dir;
    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Empty", "name": "Street", "position": [100.0, 0.0, 100.0] }),
    );
    let road = made["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": road, "component": "road" }));
    let points = json!([[0.0, 0.0, 0.0, 5.0], [60.0, 0.0, 0.0, 5.0]]);
    done(&mut editor, &mut renderer, "road.set_points", json!({ "obj": road, "points": points, "land": true }));
    let count = |editor: &Editor| {
        let instances = editor.game.engine.world.query_ref::<&voxear_engine::prefab::PrefabInstance>();
        instances.without::<voxear_scene::Destroying>().into_iter().count()
    };
    let before = count(&editor);
    let prefabs = json!([{ "prefab": { "path": "Kits/Hut.prefab" } }]);
    let lined =
        done(&mut editor, &mut renderer, "place.line_up", json!({ "road": road, "prefabs": prefabs, "seed": 3 }));
    let objs = lined["objs"].as_array().unwrap().len();
    assert!(objs >= 8, "{lined}");
    let world = &editor.game.engine.world;
    let huts: Vec<(voxear_core::math::Vec3, voxear_core::math::Quat)> = world
        .query_ref::<(&voxear_engine::prefab::PrefabInstance, &voxear_scene::Transform)>()
        .into_iter()
        .map(|(_, transform)| (transform.position, transform.rotation))
        .collect();
    assert_eq!(huts.len(), objs);
    // Each faces the road: its front, +z, towards the road's middle line at z 100.
    for (position, rotation) in &huts {
        let front = *rotation * voxear_core::math::Vec3::Z;
        assert!(front.z * (100.0 - position.z) > 0.0, "a hut at {position} faces {front}");
        assert!((position.z - 100.0).abs() >= 2.5 + 2.0, "set back from the edge: {position}");
    }
    let undone = done(&mut editor, &mut renderer, "undo", json!({}));
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(count(&editor), before, "one step took every hut back: {undone}");
}

#[test]
fn a_roads_points_are_set_on_the_ground_found_by_place_and_undone() {
    let Some((_dir, mut renderer, mut editor)) = open() else { return };
    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Empty", "name": "Road", "position": [120.0, 0.0, 100.0] }),
    );
    let road = made["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": road, "component": "road" }));
    let points = json!([[0.0, 80.0, 0.0, 5.0], [30.0, 80.0, 0.0, 7.0], [30.0, 80.0, 30.0, 7.0]]);
    let set =
        done(&mut editor, &mut renderer, "road.set_points", json!({ "obj": road, "points": points, "land": true }));
    let landed = set["points"].as_array().unwrap();
    assert_eq!(landed.len(), 3);
    assert!(landed.iter().all(|point| point[1].as_f64().unwrap() < 79.0), "set down on the ground: {set}");
    assert_eq!(landed[1][3], 7.0);
    let mix = json!([{ "block": { "path": "Blocks/Gravel.block" }, "weight": 1 }]);
    done(&mut editor, &mut renderer, "road.set_mix", json!({ "obj": road, "mix": mix, "pattern": ["1"] }));
    let near = done(&mut editor, &mut renderer, "roads.nearest", json!({ "at": [135.0, 0.0, 103.0], "reach": 10.0 }));
    assert!(near["road"].is_object() || near["road"].is_string(), "{near}");
    assert!((near["point"][2].as_f64().unwrap() - 100.0).abs() < 0.5, "{near}");
    let far = done(&mut editor, &mut renderer, "roads.nearest", json!({ "at": [135.0, 0.0, 300.0], "reach": 10.0 }));
    assert!(far.is_null());
    // Each call one step to undo: the mix, then the points.
    done(&mut editor, &mut renderer, "undo", json!({}));
    done(&mut editor, &mut renderer, "undo", json!({}));
    let world = &editor.game.engine.world;
    let roads = world.query_ref::<(voxear_ecs::Entity, &voxear_engine::roads::Road)>();
    let kept = roads.into_iter().next().map(|(_, road)| road.clone()).unwrap();
    assert_eq!(kept, voxear_engine::roads::Road::default(), "back as it was added");
}

#[test]
fn blocks_built_and_dug_by_hand_are_restored_to_the_ground_as_generated() {
    let Some((_dir, mut renderer, mut editor)) = open() else { return };
    // Wait for the world to make ground under its viewer, where the Scene view looks.
    let began = Instant::now();
    let (at, generated) = loop {
        frames(&mut editor, &mut renderer, 5);
        let world = &editor.game.engine.world;
        let made = world
            .query_ref::<(voxear_ecs::Entity, &voxear_engine::blocks::BlockWorldAsset)>()
            .into_iter()
            .find_map(|(_, asset)| asset.base.chunks().find(|(_, chunk)| !chunk.is_air()).map(|(key, _)| *key));
        if let Some([cx, cy, cz]) = made {
            let at = [cx * 32 + 8, cy * 32 + 31, cz * 32 + 8];
            let blocks = world.query_ref::<&voxear_terrain::blocks::BlockWorld>();
            let block = blocks.into_iter().next().map(|blocks| blocks.blocks.get(at)).unwrap();
            break (at, block);
        }
        assert!(began.elapsed() < PATIENCE, "no ground made");
    };
    let corner = editor
        .game
        .engine
        .world
        .query_ref::<(&voxear_terrain::blocks::BlockWorld, &voxear_scene::Transform)>()
        .into_iter()
        .next()
        .map(|(blocks, transform)| (transform.position, blocks.block_size))
        .unwrap();
    let point = |cell: [i32; 3]| {
        let (origin, size) = corner;
        [0, 1, 2].map(|axis| origin[axis] + (cell[axis] as f32 + 0.5) * size)
    };
    // A box of stone built round it, over the ground and in it, then put back.
    let from = point([at[0] - 2, at[1] - 3, at[2] - 2]);
    let to = point([at[0] + 2, at[1] + 2, at[2] + 2]);
    let stone = json!({ "path": "Blocks/Buildings/Stone Bricks.block" });
    let filled = done(&mut editor, &mut renderer, "blocks.fill", json!({ "from": from, "to": to, "block": stone }));
    assert!(filled["changed"].as_u64().unwrap() > 0);
    let restored = done(&mut editor, &mut renderer, "blocks.restore", json!({ "from": from, "to": to }));
    assert_eq!(restored["changed"], filled["changed"], "every block changed by hand put back");
    let world = &editor.game.engine.world;
    let blocks = world.query_ref::<&voxear_terrain::blocks::BlockWorld>().into_iter().next().unwrap().blocks.clone();
    assert_eq!(blocks.get(at), generated);
    // One step to undo: the stone back.
    done(&mut editor, &mut renderer, "undo", json!({}));
    let world = &editor.game.engine.world;
    let blocks = world.query_ref::<&voxear_terrain::blocks::BlockWorld>().into_iter().next().unwrap().blocks.clone();
    assert_ne!(blocks.get(at), generated, "undone, the stone is back");
}

#[test]
fn a_ground_fit_is_added_set_landed_and_undone_through_the_editors_calls() {
    let Some((_dir, mut renderer, mut editor)) = open() else { return };
    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Empty", "name": "House", "position": [120.0, 60.0, 100.0] }),
    );
    let house = made["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": house, "component": "ground_fit" }));
    let set =
        |field: &str, value: Value| json!({ "obj": house, "component": "ground_fit", "field": field, "value": value });
    let changed = done(&mut editor, &mut renderer, "field.set", set("slope_out", json!(2)));
    assert_eq!(changed["changed"], true);
    done(&mut editor, &mut renderer, "field.set", set("mode", json!("Cut")));
    {
        let world = &editor.game.engine.world;
        let fits = world.query_ref::<(voxear_ecs::Entity, &voxear_engine::ground_fit::GroundFit)>();
        let fit = fits.into_iter().next().map(|(_, fit)| fit.clone()).expect("the House has a Ground Fit");
        assert_eq!((fit.slope_out, fit.mode), (2, voxear_engine::ground_fit::FitMode::Cut));
    }

    // Set down on the ground the generator makes there: well under where it was made.
    let landed = done(&mut editor, &mut renderer, "ground_fit.land", json!({ "obj": house }));
    let y = landed["position"][1].as_f64().unwrap();
    assert!(y < 59.0, "landed at {landed}");

    // Among the world's layers once it has streamed; hidden by Bare Ground.
    let began = Instant::now();
    let status = loop {
        let status = done(&mut editor, &mut renderer, "world_layers.status", json!({}));
        if status["worlds"].as_array().is_some_and(|worlds| worlds.iter().any(|world| world["fits"] == 1)) {
            break status;
        }
        assert!(began.elapsed() < PATIENCE, "the fit never joined the world's layers: {status}");
    };
    assert_eq!(status["hidden"]["fits"], false);
    done(&mut editor, &mut renderer, "world_layers.hide", json!({ "fits": true }));
    frames(&mut editor, &mut renderer, 3);
    let hidden = done(&mut editor, &mut renderer, "world_layers.status", json!({}));
    assert_eq!(hidden["hidden"]["fits"], true);
    assert!(hidden["worlds"].as_array().unwrap().iter().all(|world| world["fits"] != 1), "{hidden}");

    // Each change one step to undo: the landing, then the mode, then the slope out.
    for _ in 0..2 {
        done(&mut editor, &mut renderer, "undo", json!({}));
    }
    let world = &editor.game.engine.world;
    let fits = world.query_ref::<(voxear_ecs::Entity, &voxear_engine::ground_fit::GroundFit)>();
    let fit = fits.into_iter().next().map(|(_, fit)| fit.clone()).unwrap();
    assert_eq!((fit.slope_out, fit.mode), (2, voxear_engine::ground_fit::FitMode::Flatten));
}
