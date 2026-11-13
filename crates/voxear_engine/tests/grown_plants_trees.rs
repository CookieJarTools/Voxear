//! The trees of a project's generated world, close up: the maple and the
//! blossom tree nearest its camera, each from the side and from over its
//! crown, as pictures to look at. Runs only where `VOXEAR_GROWN_PLANTS`
//! names a copy of the GrownPlants level's project, which it copies again
//! and never changes; writes its pictures where `VOXEAR_TEST_OUTPUT` says.

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
    let guid = game.content.db().guid_of(path).expect("the block");
    let asset = game.engine.world.get::<BlockWorldAsset>(world).unwrap();
    (1..=asset.drawn.blocks.len() as BlockId).filter(|&id| asset.drawn.guid(id) == Some(guid)).collect()
}

/// The foot of the trunk of `logs` nearest `near` along the ground: a log with no log under it.
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
                    if !logs.contains(&blocks.blocks.get(at))
                        || logs.contains(&blocks.blocks.get([at[0], at[1] - 1, at[2]]))
                    {
                        continue;
                    }
                    // A trunk: logs up from it for three blocks at least.
                    if !(1..4).all(|up| logs.contains(&blocks.blocks.get([at[0], at[1] + up, at[2]]))) {
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

#[test]
fn a_maple_and_a_blossom_tree_close_up_from_the_side_and_from_over_their_crowns() {
    let Some(project) = std::env::var_os("VOXEAR_GROWN_PLANTS") else {
        return eprintln!("skipping: VOXEAR_GROWN_PLANTS names no project");
    };
    let Ok(mut renderer) = Renderer::new_offscreen(800, 600, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let label = std::env::var("VOXEAR_TREES_LABEL").unwrap_or_else(|_| "now".into());
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(std::path::Path::new(&project), dir.path());
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let named = |game: &Game, name: &str| {
        let objs = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>();
        objs.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap()
    };
    let (islands, eye) = (named(&game, "Islands"), named(&game, "Eye"));
    let start = game.engine.world.get::<voxear_scene::Transform>(eye).unwrap().position;
    // Its ground made round the camera.
    for _ in 0..600 {
        game.tick(1.0 / 30.0, Some(&mut renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
    }
    for (kind, log) in [("maple", "Blocks/Maple Log.block"), ("blossom", "Blocks/Blossom Log.block")] {
        let logs = numbers(&game, islands, log);
        let blocks = game.engine.world.get::<BlockWorld>(islands).unwrap().clone();
        let Some(foot) = nearest_foot(&blocks, &logs, start) else {
            eprintln!("no {kind} near the camera");
            continue;
        };
        let middle = Vec3::new(foot[0] as f32 + 0.5, foot[1] as f32 + 5.0, foot[2] as f32 + 0.5);
        let views = [("side", middle + Vec3::new(-7.0, 0.0, -7.0)), ("top", middle + Vec3::new(-3.0, 16.0, -3.0))];
        for (view, at) in views {
            let look = (middle + Vec3::Y * 3.0 - at).normalize();
            let turn = Quat::from_rotation_y(look.x.atan2(look.z)) * Quat::from_rotation_x(-look.y.asin());
            {
                let transform = game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap();
                (transform.position, transform.rotation) = (at, turn);
            }
            for _ in 0..40 {
                game.tick(1.0 / 30.0, Some(&mut renderer));
                renderer.render(&game.engine.world, 1.0).unwrap();
            }
            let (width, height, pixels) = renderer.read_pixels().unwrap();
            if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
                let file = std::fs::File::create(std::path::Path::new(&out).join(format!("{kind} {view} {label}.png")))
                    .unwrap();
                let mut encoder = png::Encoder::new(file, width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
            }
            eprintln!("{kind} at {foot:?}: {view} from {at}");
        }
    }
}
