//! The trees of a project's generated world, close up: the maple, the
//! blossom tree and the pine nearest its camera, each from the side and
//! from over its crown; a camera behind a player standing in a wood, as a
//! third-person game has it; and the woods far off from over the land. As
//! pictures to look at. Runs only where `VOXEAR_GROWN_PLANTS` names the
//! GrownPlants level's project, which it copies and never changes, every
//! structure the copy has listed on its world; writes its pictures where
//! `VOXEAR_TEST_OUTPUT` says, each named with `VOXEAR_TREES_LABEL`. With
//! `VOXEAR_TREES_RAISE`, the copy's trees stand as the starter trees now do.

use voxear_core::math::{Quat, Vec3};
use voxear_engine::{Engine, Game, Project, blocks::BlockWorldAsset};
use voxear_render::{Renderer, RendererSettings};
use voxear_terrain::blocks::{BlockWorld, chunk::BlockId};

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

/// The numbers the world on `world` gives the block at `path`, in every way it stands.
fn numbers(game: &Game, world: voxear_ecs::Entity, path: &str) -> Vec<BlockId> {
    let Some(guid) = game.content.db().guid_of(path) else { return Vec::new() };
    let asset = game.engine.world.get::<BlockWorldAsset>(world).unwrap();
    (1..=asset.drawn.blocks.len() as BlockId).filter(|&id| asset.drawn.guid(id) == Some(guid)).collect()
}

/// The foot of the trunk of `logs` nearest `near` along the ground: a log with no log under it and logs over it.
fn nearest_foot(blocks: &BlockWorld, logs: &[BlockId], near: Vec3) -> Option<[i32; 3]> {
    let mut best: Option<([i32; 3], f32)> = None;
    for (key, chunk) in blocks.blocks.chunks() {
        if !chunk.may_hold(|id| logs.contains(&id)) {
            continue;
        }
        for y in 0..32 {
            for z in 0..32 {
                for x in 0..32 {
                    let at = [key[0] * 32 + x, key[1] * 32 + y, key[2] * 32 + z];
                    let log = |at: [i32; 3]| logs.contains(&blocks.blocks.get(at));
                    if !log(at) || log([at[0], at[1] - 1, at[2]]) || !(1..4).all(|up| log([at[0], at[1] + up, at[2]])) {
                        continue;
                    }
                    let far = Vec3::new(at[0] as f32 - near.x, 0.0, at[2] as f32 - near.z).length();
                    if best.is_none_or(|(_, kept)| far < kept) {
                        best = Some((at, far));
                    }
                }
            }
        }
    }
    best.map(|(at, _)| at)
}

fn looking(from: Vec3, to: Vec3) -> Quat {
    let look = (to - from).normalize();
    Quat::from_rotation_y(look.x.atan2(look.z)) * Quat::from_rotation_x(-look.y.asin())
}

#[test]
fn trees_close_up_from_the_side_and_over_their_crowns_behind_a_player_and_far_off() {
    let Some(project) = std::env::var_os("VOXEAR_GROWN_PLANTS") else {
        return eprintln!("skipping: VOXEAR_GROWN_PLANTS names no project");
    };
    let Ok(mut renderer) = Renderer::new_offscreen(800, 600, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let label = std::env::var("VOXEAR_TREES_LABEL").unwrap_or_else(|_| "now".into());
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(std::path::Path::new(&project), dir.path());
    // Every tree the project has, listed on the copy's world.
    let world_file = dir.path().join("Blocks/Endless.blockworld");
    let text = std::fs::read_to_string(&world_file).unwrap();
    let mut file = voxear_terrain::blocks::file::BlockFile::parse(&text).unwrap();
    let mut listed: Vec<String> = std::fs::read_dir(dir.path().join("Structures"))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".structure"))
        .map(|name| format!("Structures/{name}"))
        .collect();
    listed.sort();
    file.generate.as_mut().unwrap().structures = listed;
    std::fs::write(&world_file, file.to_text()).unwrap();
    // With `VOXEAR_TREES_RAISE`, the copy's trees as the starters stand now, on longer trunks.
    if std::env::var_os("VOXEAR_TREES_RAISE").is_some() {
        let raised = [
            ("Oak", [9.0, 13.0], None),
            ("Birch", [9.5, 13.5], None),
            ("Pine", [14.0, 19.0], Some([8.0, 11.0])),
            ("Maple", [10.0, 14.0], None),
            ("Blossom", [8.0, 11.0], None),
        ];
        for (name, height, crown) in raised {
            let path = dir.path().join(format!("Plants/{name}.voxplant"));
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let mut plant = voxear_terrain::blocks::plant::PlantDef::parse(&text).unwrap();
            plant.height = height;
            plant.crown_height = crown.unwrap_or(plant.crown_height);
            std::fs::write(&path, plant.to_text()).unwrap();
        }
    }
    // Its scenes without the Objs its scripts drive, a player and the camera following them among them: the
    // copy's scripts are not built, and the level's own camera looks.
    for entry in std::fs::read_dir(dir.path().join("Scenes")).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "scene") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut objs = text.split("\n[[obj]]");
        let head = objs.next().unwrap_or_default().to_string();
        let mut kept: Vec<&str> = objs.collect();
        // Those with behaviours, and what is under them, one level at a time.
        let line =
            |obj: &str, key: &str| obj.lines().find_map(|l| l.strip_prefix(key).map(str::trim).map(str::to_string));
        let mut gone: Vec<String> = Vec::new();
        loop {
            let (out, stay): (Vec<&str>, Vec<&str>) = kept.into_iter().partition(|obj| {
                obj.contains("\nbehaviours = ") || line(obj, "parent = ").is_some_and(|parent| gone.contains(&parent))
            });
            kept = stay;
            if out.is_empty() {
                break;
            }
            gone.extend(out.iter().filter_map(|obj| line(obj, "id = ")));
        }
        std::fs::write(&path, kept.iter().fold(head, |text, obj| text + "\n[[obj]]" + obj)).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let named = |game: &Game, name: &str| {
        let objs = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap()
    };
    let (islands, eye) = (named(&game, "Islands"), named(&game, "Eye"));
    let started = *game.engine.world.get::<voxear_scene::Transform>(eye).unwrap();
    let start = started.position;
    let mut picture = |game: &mut Game, renderer: &mut Renderer, name: String, at: Vec3, turn: Quat, frames: u32| {
        {
            let transform = game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap();
            (transform.position, transform.rotation) = (at, turn);
        }
        for _ in 0..frames {
            game.tick(1.0 / 30.0, Some(&mut *renderer));
            renderer.render(&game.engine.world, 1.0).unwrap();
        }
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&out).join(format!("{name} {label}.png"))).unwrap();
            let mut encoder = png::Encoder::new(file, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
    };
    // Its ground made round the camera, and the woods far off, from over the land, looking as its camera looks.
    picture(&mut game, &mut renderer, "far woods".into(), start + Vec3::Y * 25.0, started.rotation, 900);
    let mut feet = Vec::new();
    let kinds = [
        ("maple", "Blocks/Maple Log.block"),
        ("blossom", "Blocks/Blossom Log.block"),
        ("pine", "Blocks/Pine Log.block"),
    ];
    for (kind, log) in kinds {
        let logs = numbers(&game, islands, log);
        let blocks = game.engine.world.get::<BlockWorld>(islands).unwrap().clone();
        let Some(foot) = nearest_foot(&blocks, &logs, start) else {
            eprintln!("no {kind} near the camera");
            continue;
        };
        feet.push(foot);
        let middle = Vec3::new(foot[0] as f32 + 0.5, foot[1] as f32 + 5.0, foot[2] as f32 + 0.5);
        let views = [("side", middle + Vec3::new(-11.0, 3.0, -11.0)), ("top", middle + Vec3::new(-3.0, 16.0, -3.0))];
        for (view, at) in views {
            picture(&mut game, &mut renderer, format!("{kind} {view}"), at, looking(at, middle + Vec3::Y * 3.0), 40);
            eprintln!("{kind} at {foot:?}: {view} from {at}");
        }
    }
    // A camera six metres behind a player standing beside the first tree, a metre and a half over their head,
    // looking past them along the ground.
    if let Some(foot) = feet.get(1).or(feet.first()) {
        let way = Vec3::new(1.0, 0.0, 1.0).normalize();
        // The ground's top under a place: the first block of ground down from high over it, not leaves or wood.
        let blocks = game.engine.world.get::<BlockWorld>(islands).unwrap().clone();
        let ground = |x: f32, z: f32| {
            let (x, z) = (x.floor() as i32, z.floor() as i32);
            let solid = |y: i32| {
                let id = blocks.blocks.get([x, y, z]);
                let kind = blocks.faces.get(usize::from(id)).copied().unwrap_or_default();
                voxear_terrain::blocks::is_ground(&blocks.faces, id) && !kind.sways && kind.shape.is_cube()
            };
            (foot[1] - 20..foot[1] + 40).rev().find(|&y| solid(y)).map_or(foot[1] as f32, |y| (y + 1) as f32)
        };
        let (px, pz) = (foot[0] as f32 + 2.5, foot[2] as f32 - 1.5);
        let player = Vec3::new(px, ground(px, pz), pz);
        let behind = player - way * 6.0;
        let at = Vec3::new(behind.x, ground(behind.x, behind.z).max(player.y) + 1.8 + 1.5, behind.z);
        let ahead = player + Vec3::Y * 1.5 + way * 6.0;
        picture(&mut game, &mut renderer, "third person".into(), at, looking(at, ahead), 40);
    }
}
