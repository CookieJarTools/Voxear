//! Maps of a world of blocks in a game: a Map puts a minimap and a map on
//! the screen, each showing the ground round the camera as it is, with
//! what was built; Map Markers show on them; the map opens and closes.
//! Drawn too where there is a GPU: set `VOXEAR_TEST_OUTPUT` to a folder to
//! keep the pictures.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, map::Map};
use voxear_scene::Name;
use voxear_terrain::{
    blocks::{
        BlockWorld,
        file::{BlockFile, BlockKind},
        generate::{BlockEndless, BlockGenerate},
    },
    generate::Preset,
};
use voxear_ui::{MapViewKind, NodeId};

const SCENE: &str = "format = 1\n\n\
    [[obj]]\nid = 1\nname = \"Blocks\"\nposition = [0.0, 0.0, 0.0]\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
    [[obj]]\nid = 2\nname = \"Eye\"\nposition = [40.0, 40.0, 40.0]\n[obj.camera]\nfov = 60.0\n\n\
    [[obj]]\nid = 3\nname = \"Hud\"\n[obj.map]\ncorner = \"top_left\"\nsize = 240.0\n\n\
    [[obj]]\nid = 4\nname = \"Town\"\nposition = [70.0, 30.0, 60.0]\n\
    [obj.map_marker]\nicon = \"town\"\nlabel = \"Oakrest\"\ncolor = [0.2, 0.6, 1.0]\n\n\
    [[obj]]\nid = 5\nname = \"Far\"\nposition = [900.0, 30.0, 40.0]\n\
    [obj.map_marker]\nicon = \"dungeon\"\nlabel = \"The Pit\"\ncolor = [0.9, 0.2, 0.2]\nstays_at_edge = true\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Sea", [0.16, 0.45, 0.9]) });
    file.blocks.push(BlockKind::colored("Brick", [0.9, 0.1, 0.1]));
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -16,
        caves: 0.0,
        sea: 4,
        endless: Some(BlockEndless { distance: 48.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let files = [
        ("voxear.toml", "[project]\nname = \"Map\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", SCENE.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn find(game: &Game, name: &str) -> Entity {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

/// The game's map view of `kind`.
fn view(game: &mut Game, kind: MapViewKind) -> NodeId {
    let ui = game.ui.ui();
    let views = ui.map_views();
    *views.iter().find(|node| ui.map_view_settings(**node).unwrap().kind == kind).expect("a view of that kind")
}

/// Ticks until the blocks round the camera and every map tile are made,
/// laying the UI out each frame as a window does.
fn settle(game: &mut Game, mut renderer: Option<&mut voxear_render::Renderer>) {
    for frame in 0..6000 {
        game.tick(1.0 / 60.0, renderer.as_deref_mut());
        match renderer.as_deref_mut() {
            Some(renderer) => {
                game.ui.draw(renderer, 1.0 / 60.0, [0, 0, 1280, 720], 1.0).unwrap();
                renderer.render(&game.engine.world, frame as f32 / 60.0).unwrap();
            }
            None => drop(game.ui.frame(1.0 / 60.0, (1280, 720), 1.0)),
        }
        if frame > 4 && voxear_engine::blocks::streamed(&game.engine.world) && game.maps.settled() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the map round the camera was never all made");
}

#[test]
fn a_map_puts_a_minimap_and_a_map_on_the_screen_that_show_the_world_as_it_is() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game, None);
    let (blocks, eye, hud) = (find(&game, "Blocks"), find(&game, "Eye"), find(&game, "Hud"));
    let (mini, large) = (view(&mut game, MapViewKind::Minimap), view(&mut game, MapViewKind::Map));

    // The minimap sits in its corner at its size, and shows; the map is shut.
    let ui = game.ui.ui();
    assert_eq!(ui.rect(mini), [16.0, 16.0, 240.0, 240.0]);
    assert!(ui.is_visible(mini) && !ui.is_visible(large));
    let settings = ui.map_view_settings(mini).unwrap();
    assert_eq!((settings.zoom, settings.turns, settings.round), (2, true, false));

    // It shows the world of blocks round the camera, two blocks to a cell, the camera at its middle.
    let shown = *game.maps.shown(mini).expect("the minimap shows the world");
    assert_eq!((shown.world, shown.cell), (blocks, 2.0));
    let (at, inside) = shown.point(Vec3::new(40.0, 40.0, 40.0)).unwrap();
    assert!(inside && (at.x - 120.0).abs() < 1.5, "what it follows is at its middle: {at:?}");
    assert!(game.maps.shown(large).is_none(), "a map that is shut shows nothing");

    // Its ground is the world's: a column's top on the map is the highest block there.
    let world = game.engine.world.get::<BlockWorld>(blocks).unwrap().clone();
    let top =
        |world: &BlockWorld, x: i32, z: i32| (-16..200).rev().find(|&y| world.blocks.get([x, y, z]) != 0).unwrap() + 1;
    let column = game.maps.column(blocks, 1, Vec3::new(41.0, 0.0, 41.0)).expect("its tile is made");
    let high = top(&world, 41, 41);
    assert!((column.highest() * 2 - high).abs() <= 2, "the map's {} cells against {high} blocks", column.highest());

    // A tower built beside the camera shows on the map once its tile is made again, and nothing else is.
    let other = game.maps.column(blocks, 1, Vec3::new(20.0, 0.0, 70.0)).unwrap();
    let tower =
        (0..12).flat_map(|y| [[44, y], [45, y]]).flat_map(|[x, y]| [([x, high + y, 44], 5), ([x, high + y, 45], 5)]);
    game.engine.world.get_mut::<BlockWorld>(blocks).unwrap().set_blocks(tower);
    settle(&mut game, None);
    let built = game.maps.column(blocks, 1, Vec3::new(44.5, 0.0, 44.5)).unwrap();
    assert!(built.highest() * 2 >= high + 10 && built.blocks[0] == 5, "the tower's cell: {built:?}");
    assert_eq!(game.maps.column(blocks, 1, Vec3::new(20.0, 0.0, 70.0)), Some(other));

    // Opened, the map shows in place of the minimap, with the markers' labels, a title and a readout.
    Maps::set_open(&mut game.engine.world, hud, true);
    settle(&mut game, None);
    let ui = game.ui.ui();
    assert!(ui.is_visible(large) && !ui.is_visible(mini));
    let parts = ui.map_view_parts(large).unwrap();
    let labels: Vec<String> = ui.children(parts.overlay).into_iter().map(|label| ui.text(label)).collect();
    assert_eq!(labels, ["Oakrest"], "the town is on the map; the dungeon, far past its edge, has no label");
    let readout = ui.text(parts.readout.unwrap());
    assert!(readout.starts_with("X 40   Z 40") && readout.ends_with("1:4"), "{readout}");
    assert!(readout.contains("°C") && readout.contains('%'), "{readout}");
    assert!(!ui.text(parts.title.unwrap()).is_empty(), "the country it looks at has a name");
    let shown = *game.maps.shown(large).unwrap();
    assert_eq!(shown.cell, 4.0);
    // The map follows the camera as it goes.
    game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap().position.x += 64.0;
    settle(&mut game, None);
    let moved = *game.maps.shown(large).unwrap();
    let before = shown.corner[0] as f32 + shown.camera.middle.x;
    let after = moved.corner[0] as f32 + moved.camera.middle.x;
    assert!((after - before - 16.0).abs() < 0.01, "sixty-four metres is sixteen cells: {before} to {after}");

    // Shut again, the minimap is back; taken away, both go.
    Maps::set_open(&mut game.engine.world, hud, false);
    settle(&mut game, None);
    assert!(game.ui.ui().is_visible(mini) && !game.ui.ui().is_visible(large));
    game.engine.world.remove::<Map>(hud);
    game.tick(1.0 / 60.0, None);
    assert!(game.ui.ui().map_views().is_empty());
}

use voxear_engine::map::Maps;

fn keep(name: &str, (width, height, pixels): (u32, u32, Vec<u8>)) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}

#[test]
fn the_minimap_and_the_map_are_drawn_over_the_game() {
    let settings = voxear_render::RendererSettings::default();
    let Ok(mut renderer) = voxear_render::Renderer::new_offscreen(1280, 720, &settings) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    settle(&mut game, Some(&mut renderer));
    for _ in 0..4 {
        settle(&mut game, Some(&mut renderer));
    }
    let picture = renderer.read_pixels().unwrap();
    // The minimap's middle is ground, drawn: not the sky the camera sees there without it.
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * 1280 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (middle, beside) = (at(&picture.2, 136, 100), at(&picture.2, 640, 100));
    keep("game-minimap.png", picture);
    assert_ne!(middle, beside, "the minimap covers its corner");

    let hud = find(&game, "Hud");
    Maps::set_open(&mut game.engine.world, hud, true);
    for _ in 0..8 {
        settle(&mut game, Some(&mut renderer));
    }
    let picture = renderer.read_pixels().unwrap();
    let middle = at(&picture.2, 640, 400);
    keep("game-map.png", picture);
    assert_ne!(middle, beside, "the map covers the view");
}

#[test]
fn maps_show_only_the_ground_they_know_as_the_game_reveals_it_or_as_it_is_explored() {
    use voxear_engine::map::{MapKnown, MapLayerLook, MapShows};
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game, None);
    let hud = find(&game, "Hud");
    // Explored: seen from where the maps follow, the ground round the camera is known, and no further.
    game.engine.world.get_mut::<Map>(hud).unwrap().shows = MapShows::Explored;
    settle(&mut game, None);
    let known = game.engine.world.resource::<MapKnown>().unwrap().clone();
    assert_eq!(known.shows, MapShows::Explored);
    let seen = &known.all[&known.shown];
    assert!(seen.knows(40.0, 40.0) && seen.knows(80.0, 40.0) && !seen.knows(400.0, 40.0));
    // Walked on, more is known, and what was seen stays seen.
    let eye = find(&game, "Eye");
    game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap().position.x += 300.0;
    settle(&mut game, None);
    let seen = game.engine.world.resource::<MapKnown>().unwrap().all[&0].clone();
    assert!(seen.knows(340.0, 40.0) && seen.knows(40.0, 40.0));

    // The game decides: nothing is seen by walking; a script's own map for another player knows what it is told.
    game.engine.world.get_mut::<Map>(hud).unwrap().shows = MapShows::GameDecides;
    let mut known = game.engine.world.remove_resource::<MapKnown>().unwrap();
    known.all.insert(3, Default::default());
    known.shown = 3;
    known.all.get_mut(&3).unwrap().reveal(700.0, 40.0, 30.0);
    known.layers.insert(1, MapLayerLook { low: [0.0; 4], high: [0.9, 0.1, 0.1, 0.7] });
    game.engine.world.insert_resource(known);
    game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap().position.x += 300.0;
    settle(&mut game, None);
    let known = game.engine.world.resource::<MapKnown>().unwrap();
    assert!(!known.all[&3].knows(640.0, 40.0), "walking reveals nothing when the game decides");
    assert!(known.all[&3].knows(700.0, 40.0) && !known.all[&0].knows(700.0, 40.0), "each map knows its own");
}

#[test]
fn ground_a_map_does_not_know_is_drawn_as_paper_and_news_colours_what_it_does() {
    use voxear_engine::map::{MapKnown, MapLayerLook, MapShows};
    let settings = voxear_render::RendererSettings::default();
    let Ok(mut renderer) = voxear_render::Renderer::new_offscreen(1280, 720, &settings) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let hud = find(&game, "Hud");
    game.engine.world.get_mut::<Map>(hud).unwrap().shows = MapShows::GameDecides;
    Maps::set_open(&mut game.engine.world, hud, true);
    let mut known = MapKnown { shows: MapShows::GameDecides, ..MapKnown::default() };
    // Two places seen, and of one of them the news that it is dangerous.
    known.all.get_mut(&0).unwrap().reveal(40.0, 40.0, 150.0);
    known.all.get_mut(&0).unwrap().reveal(-200.0, 250.0, 90.0);
    known.layers.insert(1, MapLayerLook { low: [0.9, 0.1, 0.1, 0.0], high: [0.9, 0.1, 0.1, 0.75] });
    known.all.get_mut(&0).unwrap().paint(1, [-200.0, 250.0], 60.0, 1.0, 3);
    game.engine.world.insert_resource(known);
    for _ in 0..8 {
        settle(&mut game, Some(&mut renderer));
    }
    let large = view(&mut game, MapViewKind::Map);
    let shown = *game.maps.shown(large).unwrap();
    let picture = renderer.read_pixels().unwrap();
    let at = |place: Vec3| {
        let (pixel, _) = shown.point(place).unwrap();
        let i = ((pixel.y as u32 * 1280 + pixel.x as u32) * 4) as usize;
        [picture.2[i], picture.2[i + 1], picture.2[i + 2]].map(i32::from)
    };
    // Paper where nothing is known; green ground where it is; red where the news is of danger.
    let column =
        |x: f32, z: f32| game.maps.column(shown.world, 2, Vec3::new(x, 0.0, z)).unwrap().highest() as f32 * 4.0;
    let ground = at(Vec3::new(40.0, column(40.0, 120.0), 120.0));
    let danger = at(Vec3::new(-200.0, column(-200.0, 250.0), 250.0));
    let paper = at(Vec3::new(330.0, shown.base as f32 * 4.0, -330.0));
    keep("game-known.png", picture.clone());
    assert!(ground[1] > ground[0] + 20 && ground[1] > ground[2] + 20, "grass where it is known: {ground:?}");
    assert!(danger[0] > danger[1] + 40, "red where it is dangerous: {danger:?}");
    assert!((paper[0] - paper[2]).abs() < 90 && paper[0] > paper[2] && paper[0] > 110, "paper: {paper:?}");
}
