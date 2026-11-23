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
    // An Area Tag and a town's laws, as data assets.
    std::fs::create_dir_all(folder.join("Areas")).unwrap();
    std::fs::write(folder.join("Areas/Town.asset"), "type = \"Voxear.AreaTag\"\n").unwrap();
    std::fs::write(folder.join("Areas/Laws.asset"), "type = \"Voxear.AreaTag\"\n").unwrap();

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
fn land_height_and_a_biome_are_painted_into_the_generator_undone_and_saved_beside_the_world() {
    let Some((dir, mut renderer, mut editor)) = open() else { return };
    let world_entity = editor
        .game
        .engine
        .world
        .query_ref::<(voxear_ecs::Entity, &voxear_engine::blocks::BlockWorldAsset)>()
        .into_iter()
        .next()
        .map(|(entity, _)| entity)
        .unwrap();
    let height_at = |editor: &mut Editor, x: f32, z: f32| {
        let game = &mut editor.game;
        voxear_engine::ground_fit::ground_under(
            &mut game.engine.world,
            &mut game.content,
            voxear_core::math::Vec3::new(x, 0.0, z),
            None,
        )
        .unwrap()
    };
    let before = height_at(&mut editor, 120.0, 120.0);
    let painted = done(
        &mut editor,
        &mut renderer,
        "paint.map",
        json!({ "map": "height", "at": [120.0, 0.0, 120.0], "radius": 80.0, "strength": 1.0 }),
    );
    assert_eq!(painted["changed"], true);
    let raised = height_at(&mut editor, 120.0, 120.0);
    assert!(raised > before + 10.0, "a mountain painted: {before} to {raised}");
    assert_eq!(height_at(&mut editor, 2000.0, 2000.0), height_at(&mut editor, 2000.0, 2000.0));
    // A biome the world lists, chosen there before the climate rules.
    let biome = editor
        .game
        .engine
        .world
        .get::<voxear_engine::blocks::BlockWorldAsset>(world_entity)
        .unwrap()
        .file
        .generate
        .as_ref()
        .unwrap()
        .biomes[3]
        .clone();
    done(
        &mut editor,
        &mut renderer,
        "paint.map",
        json!({ "map": "biome", "biome": biome, "at": [300.0, 0.0, 300.0], "radius": 60.0, "falloff": 0.0 }),
    );
    let asset = editor.game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(world_entity).unwrap().clone();
    let making = voxear_engine::blocks::making(&mut editor.game.content, &asset).unwrap();
    let placed = making.generate.placed.clone().unwrap();
    let corner = voxear_engine::ground_fit::world_corner(&editor.game.engine.world, world_entity).unwrap();
    let [gx, gz] = [making.generate.position[0] + 300.0 - corner.x, making.generate.position[1] + 300.0 - corner.z];
    let mild = voxear_terrain::generate::biomes::Place { temperature: 15.0, humidity: 0.5, above_sea: 10.0, sunk: 0.0 };
    assert_eq!(placed.index_at(gx, gz, mild), Some(3));
    // Each dab one step to undo: the biome, then the mountain.
    done(&mut editor, &mut renderer, "undo", json!({}));
    done(&mut editor, &mut renderer, "undo", json!({}));
    assert!((height_at(&mut editor, 120.0, 120.0) - before).abs() < 0.01, "undone, the ground as it was");
    done(&mut editor, &mut renderer, "redo", json!({}));
    done(&mut editor, &mut renderer, "save", json!({}));
    let maps = dir.path().join("Fits/Blocks/Endless.maps");
    assert!(maps.is_file(), "saved beside the world");
    let read = voxear_terrain::generate::painted::PaintedMaps::from_bytes(&std::fs::read(maps).unwrap()).unwrap();
    assert!(!read.height.is_empty());
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

#[test]
fn a_world_area_and_routes_are_made_asked_about_and_undone_through_the_editors_calls() {
    let Some((_dir, mut renderer, mut editor)) = open() else { return };
    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Empty", "name": "Town", "position": [120.0, 0.0, 100.0] }),
    );
    let town = made["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": town, "component": "world_area" }));
    let outline = json!([[-20.0, -20.0], [20.0, -20.0], [20.0, 20.0], [-20.0, 20.0]]);
    done(&mut editor, &mut renderer, "world_area.set_outline", json!({ "obj": town, "outline": outline }));
    let set =
        |field: &str, value: Value| json!({ "obj": town, "component": "world_area", "field": field, "value": value });
    done(&mut editor, &mut renderer, "field.set", set("tags", json!(1)));
    done(&mut editor, &mut renderer, "field.set", set("tag_1", json!({ "path": "Areas/Town.asset" })));
    done(&mut editor, &mut renderer, "field.set", set("data_1", json!({ "path": "Areas/Laws.asset" })));
    done(&mut editor, &mut renderer, "field.set", set("no_structures", json!(true)));
    frames(&mut editor, &mut renderer, 2);
    let here = done(&mut editor, &mut renderer, "areas.at", json!({ "at": [125.0, 10.0, 95.0] }));
    assert_eq!(here.as_array().map(Vec::len), Some(1), "{here}");
    assert_eq!(here[0]["tags"][0]["data"], "Areas/Laws.asset", "{here}");
    let value = json!({ "at": [125.0, 10.0, 95.0], "tag": { "path": "Areas/Town.asset" } });
    assert_eq!(done(&mut editor, &mut renderer, "areas.value", value)["data"], "Areas/Laws.asset");
    let away = json!({ "at": [400.0, 10.0, 95.0], "tag": { "path": "Areas/Town.asset" } });
    assert!(done(&mut editor, &mut renderer, "areas.value", away).is_null());

    // Two routes that meet, and the way along them; one step each to undo.
    let route = |editor: &mut Editor, renderer: &mut Renderer, name: &str, points: Value| {
        let made =
            done(editor, renderer, "obj.create", json!({ "kind": "Empty", "name": name, "position": [0.0, 0.0, 0.0] }));
        let obj = made["obj"].clone();
        done(editor, renderer, "component.add", json!({ "obj": obj, "component": "route" }));
        done(editor, renderer, "route.set_points", json!({ "obj": obj, "points": points }));
        obj
    };
    route(&mut editor, &mut renderer, "East", json!([[100.0, 5.0, 100.0], [200.0, 5.0, 100.0]]));
    let north = route(&mut editor, &mut renderer, "North", json!([[150.0, 5.0, 101.0], [150.0, 5.0, 200.0]]));
    frames(&mut editor, &mut renderer, 2);
    let way = done(
        &mut editor,
        &mut renderer,
        "routes.find",
        json!({ "from": [100.0, 5.0, 100.0], "to": [150.0, 5.0, 200.0] }),
    );
    assert!((way["length"].as_f64().unwrap() - 150.0).abs() < 2.0, "{way}");
    let near = done(&mut editor, &mut renderer, "routes.nearest", json!({ "at": [150.0, 5.0, 160.0] }));
    assert!((near["point"][0].as_f64().unwrap() - 150.0).abs() < 1e-3, "{near}");
    assert!(
        done(
            &mut editor,
            &mut renderer,
            "routes.find",
            json!({ "from": [0.0, 5.0, 0.0], "to": [9.0, 5.0, 0.0], "kind": "sea" })
        )
        .is_null()
    );
    done(
        &mut editor,
        &mut renderer,
        "route.set_points",
        json!({ "obj": north, "points": [[0.0, 5.0, 0.0], [0.0, 5.0, 9.0]] }),
    );
    done(&mut editor, &mut renderer, "undo", json!({}));
    let world = &editor.game.engine.world;
    let named = world.query_ref::<(&voxear_scene::Name, &voxear_engine::world_routes::Route)>();
    let points = named.into_iter().find(|(name, _)| name.0 == "North").map(|(_, route)| route.points.clone()).unwrap();
    assert_eq!(points, [[150.0, 5.0, 101.0], [150.0, 5.0, 200.0]], "the set undone");
}

#[test]
fn a_streamed_world_is_given_a_set_size_and_what_lies_past_its_edge_through_the_editors_calls() {
    use voxear_terrain::blocks::generate::WorldEdgeKind;
    let Some((_dir, mut renderer, mut editor)) = open() else { return };
    let world = {
        let objs = editor.game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        objs.into_iter().find(|(_, name)| name.0 == "World").map(|(obj, _)| obj).expect("the template's world")
    };
    let obj = json!({ "entity": world.to_bits() });
    let set =
        |field: &str, value: Value| json!({ "obj": obj, "component": "block_world", "field": field, "value": value });
    done(&mut editor, &mut renderer, "field.set", set("set_size", json!(true)));
    done(&mut editor, &mut renderer, "field.set", set("world_size", json!([20000.0, 12000.0])));
    done(&mut editor, &mut renderer, "field.set", set("edge", json!("Cliffs")));
    done(&mut editor, &mut renderer, "field.set", set("cliff_height", json!(220.0)));
    let endless = |editor: &Editor| {
        let asset = editor.game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(world).unwrap();
        asset.file.generate.as_ref().unwrap().endless.unwrap()
    };
    let now = endless(&editor);
    assert_eq!((now.size, now.edge, now.edge_height), (Some([20000, 12000]), WorldEdgeKind::Cliffs, 220.0));
    // Each change one step to undo.
    done(&mut editor, &mut renderer, "undo", json!({}));
    assert_eq!(endless(&editor).edge_height, 160.0);
    done(&mut editor, &mut renderer, "field.set", set("set_size", json!(false)));
    assert_eq!(endless(&editor).size, None, "without end again");
}

#[test]
fn a_new_flat_world_made_twenty_kilometres_across_streams_and_keeps_blocks_built_far_out() {
    use voxear_terrain::blocks::generate::WorldEdgeKind;
    let Some((dir, mut renderer, mut editor)) = open() else { return };
    let made = done(
        &mut editor,
        &mut renderer,
        "obj.create",
        json!({ "kind": "Empty", "name": "Big", "position": [0.0, 400.0, 0.0] }),
    );
    let obj = made["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": obj, "component": "block_world" }));
    let set = json!({ "obj": obj, "component": "block_world", "field": "ground_size", "value": [20000.0, 20000.0] });
    done(&mut editor, &mut renderer, "field.set", set);
    let big = {
        let objs = editor.game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        objs.into_iter().find(|(_, name)| name.0 == "Big").map(|(obj, _)| obj).unwrap()
    };
    let asset =
        |editor: &Editor| editor.game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(big).unwrap().clone();
    // Too large to be made whole: streamed, ground made flat by a generator of its own beside it.
    let generate = asset(&editor).file.generate.clone().expect("generated now");
    let endless = generate.endless.expect("streamed");
    assert_eq!((endless.size, endless.edge), (Some([20000, 20000]), WorldEdgeKind::Nothing));
    assert_eq!((endless.distance, endless.far), (256.0, 16384.0), "seeing much of it at once");
    let path = editor.game.content.db().get(asset(&editor).guid).unwrap().path.clone();
    let folder = dir.path().join("Fits");
    let generator = path.trim_end_matches(".blockworld").to_string() + " Flat.worldgen";
    assert!(folder.join(&generator).exists(), "{generator}");
    // Far out, 15 km from its corner, its ground is made where the Scene view looks, flat at its height.
    let far = [15000.5f32, 0.0, 15000.5];
    done(
        &mut editor,
        &mut renderer,
        "view.set",
        json!({ "position": [far[0], 430.0, far[2] - 20.0], "look_at": [far[0], 408.0, far[2]] }),
    );
    let began = Instant::now();
    let top = loop {
        frames(&mut editor, &mut renderer, 5);
        let ground =
            done(&mut editor, &mut renderer, "blocks.get", json!({ "at": [far[0], 407.5, far[2]], "obj": obj }));
        if ground["block"].is_string() || ground["block"].is_object() {
            break ground;
        }
        assert!(began.elapsed() < PATIENCE, "no ground made far out: {ground}");
    };
    let air = done(&mut editor, &mut renderer, "blocks.get", json!({ "at": [far[0], 408.5, far[2]], "obj": obj }));
    assert!(air["block"].is_null(), "flat at 8 blocks: {top} under {air}");
    // A block built there by hand, saved, is kept as the world's own.
    let stone = json!({ "path": "Blocks/Buildings/Stone Bricks.block" });
    done(
        &mut editor,
        &mut renderer,
        "blocks.set",
        json!({ "at": [far[0], 408.5, far[2]], "block": stone, "obj": obj }),
    );
    done(&mut editor, &mut renderer, "save", json!({}));
    let built = done(&mut editor, &mut renderer, "blocks.get", json!({ "at": [far[0], 408.5, far[2]], "obj": obj }));
    assert!(!built["block"].is_null(), "{built}");
    // Gone from as the view goes 10 km off, and back as it comes again.
    let at_far = |editor: &mut Editor, renderer: &mut Renderer| {
        done(editor, renderer, "blocks.get", json!({ "at": [far[0], 408.5, far[2]], "obj": obj }))["block"].clone()
    };
    done(
        &mut editor,
        &mut renderer,
        "view.set",
        json!({ "position": [5000.0, 430.0, 5000.0], "look_at": [5000.0, 408.0, 5020.0] }),
    );
    let began = Instant::now();
    while !at_far(&mut editor, &mut renderer).is_null() {
        frames(&mut editor, &mut renderer, 5);
        assert!(began.elapsed() < PATIENCE, "the far column never let go");
    }
    done(
        &mut editor,
        &mut renderer,
        "view.set",
        json!({ "position": [far[0], 430.0, far[2] - 20.0], "look_at": [far[0], 408.0, far[2]] }),
    );
    let began = Instant::now();
    while at_far(&mut editor, &mut renderer) != built["block"] {
        frames(&mut editor, &mut renderer, 5);
        assert!(began.elapsed() < PATIENCE, "the block built by hand did not come back");
    }
    let saved: Vec<String> = std::fs::read_dir(folder.join(path.rsplit_once('/').map_or("", |(f, _)| f)))
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with(path.rsplit('/').next().unwrap().trim_end_matches(".blockworld")))
        .collect();
    assert!(saved.iter().any(|name| name.ends_with(".blocks")), "its changes written beside it: {saved:?}");
}
