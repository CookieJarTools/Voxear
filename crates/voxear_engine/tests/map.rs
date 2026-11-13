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
    [[obj]]\nid = 3\nname = \"Hud\"\n[obj.map]\ncorner = \"top_left\"\n\n\
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
fn settle(game: &mut Game, renderer: Option<&mut voxear_render::Renderer>) {
    settle_at(game, renderer, (1280, 720));
}

/// As [`settle`], on a screen of `size` pixels.
fn settle_at(game: &mut Game, mut renderer: Option<&mut voxear_render::Renderer>, size: (u32, u32)) {
    for frame in 0..6000 {
        game.tick(1.0 / 60.0, renderer.as_deref_mut());
        match renderer.as_deref_mut() {
            Some(renderer) => {
                game.ui.draw(renderer, 1.0 / 60.0, [0, 0, size.0, size.1], 1.0).unwrap();
                renderer.render(&game.engine.world, frame as f32 / 60.0).unwrap();
            }
            None => drop(game.ui.frame(1.0 / 60.0, size, 1.0)),
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
    // On a screen two thirds as high as its design's, it is two thirds the size: 40 from the corner, 380 wide.
    let rect = ui.rect(mini);
    assert!(
        (rect[0] - 26.67).abs() < 0.05 && (rect[1] - 26.67).abs() < 0.05 && (rect[2] - 253.33).abs() < 0.05,
        "{rect:?}"
    );
    assert!(ui.is_visible(mini) && !ui.is_visible(large));
    let settings = ui.map_view_settings(mini).unwrap();
    assert_eq!((settings.zoom, settings.turns, settings.round), (2, true, false));

    // It shows the world of blocks round the camera, two blocks to a cell, the camera at its middle.
    let shown = *game.maps.shown(mini).expect("the minimap shows the world");
    assert_eq!((shown.world, shown.cell), (blocks, 2.0));
    // What it looks at is the ground the camera is over or under, so that is at its middle.
    let ground = shown.ground.expect("the ground at the camera is made");
    let (at, inside) = shown.point(Vec3::new(40.0, ground, 40.0)).unwrap();
    let off = at - shown.size * 0.5;
    assert!(inside && off.length() < 1.5, "the ground under what it follows is at its middle: {at:?}");
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
    // Its panels say where it looks, how warm and wet it is there, and its zoom; no time, as the game tells none.
    assert_eq!((ui.text(parts.coords[0]).as_str(), ui.text(parts.coords[1]).as_str()), ("40.000", "40.000"));
    assert_eq!(ui.text(parts.zoom_value), "1:4");
    assert!(ui.text(parts.warmth).parse::<i32>().is_ok() && ui.text(parts.wetness).parse::<i32>().is_ok());
    assert!(!ui.text(parts.title).is_empty(), "the country it looks at has a name");
    assert!(!ui.is_visible(parts.time_cell) && ui.is_visible(parts.environment));
    assert!(!ui.is_visible(parts.tag.unwrap()), "what it follows has no name, so no name tag");
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
    let paper = at(Vec3::new(250.0, shown.base as f32 * 4.0, -250.0));
    keep("game-known.png", picture.clone());
    assert!(ground[1] > ground[0] + 20 && ground[1] > ground[2] + 20, "grass where it is known: {ground:?}");
    assert!(danger[0] > danger[1] + 40, "red where it is dangerous: {danger:?}");
    assert!((paper[0] - paper[2]).abs() < 90 && paper[0] > paper[2] && paper[0] > 110, "paper: {paper:?}");
}

const ACTIONS: &str = "format = 1\n\n[[map]]\nname = \"Player\"\n\n[[map.action]]\nname = \"Jump\"\n\
                       bindings = [\"Keyboard/Space\"]\n\n[[map]]\nname = \"Map\"\n\n[[map.action]]\n\
                       name = \"Open Map\"\nid = \"open-map\"\nbindings = [\"Keyboard/M\"]\n";

/// One frame as a window runs it: the tick, then the screen laid out.
fn frame(game: &mut Game) {
    game.tick(1.0 / 60.0, None);
    game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
}

#[test]
fn its_action_opens_and_shuts_the_map_and_the_pointer_turns_moves_and_zooms_it() {
    let dir = project();
    std::fs::write(dir.path().join("Input.inputactions"), ACTIONS).unwrap();
    let scene = SCENE.replace("[obj.map]\n", "[obj.map]\nopen_action = \"open-map\"\n");
    std::fs::write(dir.path().join("Scenes/Field.scene"), scene).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game, None);
    let hud = find(&game, "Hud");
    let (mini, large) = (view(&mut game, MapViewKind::Minimap), view(&mut game, MapViewKind::Map));
    game.display.cursor_visible = false;
    let press = |game: &mut Game| {
        game.input.devices_mut().set_key("M", true);
        frame(game);
        game.input.devices_mut().set_key("M", false);
        frame(game);
    };

    // Its action opens it: the map in place of the minimap, the mouse free, and only the action's own input map on.
    press(&mut game);
    assert!(game.engine.world.get::<Map>(hud).unwrap().open);
    assert!(game.ui.ui().is_visible(large) && !game.ui.ui().is_visible(mini));
    assert!(game.display.cursor_visible, "the mouse shows while the map is open");
    assert!(game.input.map_enabled("Map") && !game.input.map_enabled("Player"));
    settle(&mut game, None);
    let shown = |game: &Game| *game.maps.shown(large).expect("the open map shows the world");
    let looks_at = |game: &Game| {
        let shown = shown(game);
        [shown.corner[0] as f32 + shown.camera.middle.x, shown.corner[1] as f32 + shown.camera.middle.z]
    };
    let (before, at) = (shown(&game).camera, looks_at(&game));

    // A drag turns and tilts it.
    game.ui.ui().pointer_move(640.0, 360.0);
    game.ui.ui().pointer_button(true);
    frame(&mut game);
    game.ui.ui().pointer_move(700.0, 330.0);
    frame(&mut game);
    game.ui.ui().pointer_button(false);
    frame(&mut game);
    let turned = shown(&game).camera;
    assert!(
        (turned.turn - (before.turn - 0.48)).abs() < 0.01,
        "sixty pixels across: {} to {}",
        before.turn,
        turned.turn
    );
    assert!(turned.tilt < before.tilt - 0.1, "thirty pixels up tilts it lower: {} to {}", before.tilt, turned.tilt);
    assert_eq!(looks_at(&game), at, "turning does not move it");

    // A drag with the middle button moves it over the ground, the way the pointer goes.
    game.ui.ui().pointer_middle(true);
    frame(&mut game);
    game.ui.ui().pointer_move(600.0, 330.0);
    frame(&mut game);
    game.ui.ui().pointer_middle(false);
    frame(&mut game);
    let moved = looks_at(&game);
    let by = ((moved[0] - at[0]).powi(2) + (moved[1] - at[1]).powi(2)).sqrt();
    assert!(by > 5.0, "a hundred pixels moves it some cells: {at:?} to {moved:?}");
    assert_eq!(shown(&game).camera.turn, turned.turn, "moving does not turn it");

    // The wheel turned away zooms in: each notch shows a little less ground, the slab as large on the screen as
    // it was, and past a point its cells are half the size, 1:4 becoming 1:2, with no jump in what is shown.
    assert_eq!(shown(&game).cell, 4.0);
    let ground = |game: &Game| shown(game).reach * shown(game).cell;
    let on_screen = |game: &Game| shown(game).camera.distance / shown(game).reach;
    let (mut metres, size) = (ground(&game), on_screen(&game));
    for _ in 0..4 {
        game.ui.ui().pointer_scroll(0.0, -40.0);
        frame(&mut game);
        frame(&mut game);
        let now = ground(&game);
        assert!(now < metres && now > metres * 0.85, "a notch nearer, never a jump: {metres} m to {now} m");
        assert!((on_screen(&game) - size).abs() < 1e-3, "the slab stays the size it is on the screen");
        metres = now;
    }
    settle(&mut game, None);
    assert_eq!(shown(&game).cell, 2.0, "zoomed in");
    for _ in 0..10 {
        game.ui.ui().pointer_scroll(0.0, 40.0);
        frame(&mut game);
    }
    settle(&mut game, None);
    assert_eq!(shown(&game).cell, 8.0, "and out past where it began");

    // Its action shuts it: the minimap back, the mouse and the input maps as they were.
    press(&mut game);
    assert!(!game.engine.world.get::<Map>(hud).unwrap().open);
    assert!(game.ui.ui().is_visible(mini) && !game.ui.ui().is_visible(large));
    assert!(!game.display.cursor_visible && game.input.map_enabled("Player"));
}

#[test]
fn a_minimap_has_ground_in_every_corner_of_its_window_however_high_over_the_ground_it_is_seen_from() {
    use voxear_core::math::Quat;
    let settings = voxear_render::RendererSettings::default();
    let Ok(mut renderer) = voxear_render::Renderer::new_offscreen(1280, 720, &settings) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    // The camera flies high over the hills, turned an eighth round: the minimap turns with it.
    let eye = find(&game, "Eye");
    let high = Vec3::new(-28.0, 150.0, -28.0);
    let flying = game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap();
    (flying.position, flying.rotation) = (high, Quat::from_rotation_y(std::f32::consts::FRAC_PI_4));
    for _ in 0..5 {
        settle(&mut game, Some(&mut renderer));
    }
    let hud = find(&game, "Hud");
    let mini = view(&mut game, MapViewKind::Minimap);
    let number = game.engine.world.get::<Map>(hud).unwrap().views.expect("on the screen")[0];
    let window = {
        let ui = game.ui.ui();
        ui.rect(ui.map_view_parts(mini).unwrap().picture)
    };
    // The map looks at the ground under the camera, not at the camera up in the air: so the ground there is
    // at the window's middle, and the arrow lies on it.
    let shown = *game.maps.shown(mini).unwrap();
    let ground = shown.ground.expect("the ground under the camera is made");
    assert!(ground < 110.0 && ground > 0.0, "the hills' ground, far under the camera: {ground}");
    let (under, _) = shown.point(Vec3::new(high.x, ground, high.z)).unwrap();
    assert!((under.x - window[2] * 0.5).abs() < 2.0 && (under.y - window[3] * 0.5).abs() < 2.0, "{under:?}");

    // At every zoom, each corner of the window is ground: not the panel the window is in, as it is beside it.
    for zoom in [2, 1, 8, 32] {
        wish_of(&mut game, number).settings.push(format!("zoom={zoom}"));
        for _ in 0..5 {
            settle(&mut game, Some(&mut renderer));
        }
        // The ground under the camera is as high whatever size the map's cells are, to a cell.
        let stands = game.maps.shown(mini).unwrap().ground.unwrap();
        assert!((stands - ground).abs() <= zoom as f32, "at 1:{zoom} the ground stands {stands}, at 1:2 {ground}");
        let picture = renderer.read_pixels().unwrap();
        let at = |x: f32, y: f32| {
            let i = ((y.round() as u32 * 1280 + x.round() as u32) * 4) as usize;
            [picture.2[i], picture.2[i + 1], picture.2[i + 2]].map(i32::from)
        };
        let (left, top, right, bottom) = (window[0], window[1], window[0] + window[2], window[1] + window[3]);
        let panel = at(left - 2.0, top + window[3] * 0.5);
        for (x, y) in [(left + 4.0, top + 4.0), (right - 4.0, top + 4.0), (left + 4.0, bottom - 4.0)] {
            let corner = at(x, y);
            let apart: i32 = (0..3).map(|c| (corner[c] - panel[c]).abs()).sum();
            assert!(apart > 40, "at 1:{zoom}, the window's corner at {x} {y} is {corner:?}, the panel {panel:?}");
        }
        keep(&format!("minimap-from-high-{zoom}.png"), picture);
    }
}

/// What scripts ask of the map view they hold by `number`, to change.
fn wish_of(game: &mut Game, number: u32) -> &mut voxear_engine::map::MapViewWish {
    game.engine.world.resource_mut::<voxear_engine::map::MapScript>().unwrap().wishes.entry(number).or_default()
}

#[test]
fn a_script_steers_how_a_map_is_seen_and_what_it_follows_stays_at_its_edge() {
    use voxear_core::math::Vec2;
    use voxear_engine::map::MapScript;
    use voxear_ui::Shape;
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    settle(&mut game, None);
    let hud = find(&game, "Hud");
    Maps::set_open(&mut game.engine.world, hud, true);
    settle(&mut game, None);
    let large = view(&mut game, MapViewKind::Map);
    // Its views are elements scripts hold by number, as any element is.
    let number = game.engine.world.get::<Map>(hud).unwrap().views.expect("on the screen")[1];
    let shown = |game: &Game| *game.maps.shown(large).unwrap();
    let seen = |game: &Game| game.engine.world.resource::<MapScript>().unwrap().seen[&number].clone();
    // What the map follows is an arrow of the accent colour lying on the ground: strokes from its tip.
    let accent = voxear_ui::Color::rgb(0xff, 0xd2, 0x3f);
    let strokes = |game: &mut Game| -> Vec<(Vec2, Vec2)> {
        let ui = game.ui.ui();
        let overlay = ui.map_view_parts(large).unwrap().overlay;
        let lines = ui.shapes(overlay).iter().filter_map(|shape| match shape {
            Shape::Line { from, to, color, .. } if *color == accent => Some((Vec2::from(*from), Vec2::from(*to))),
            _ => None,
        });
        lines.collect()
    };
    let arrow = |game: &mut Game| -> Vec<Vec2> { strokes(game).into_iter().map(|(tip, _)| tip).collect() };

    // Opened, its ground is drawn a little less wide than the screen is high, so all of it is on the screen
    // however it is tilted, and what it follows is at its middle.
    let first = shown(&game);
    let wide = first.reach / (first.camera.distance * (first.camera.fov * 0.5).tan());
    assert!((wide - 0.92).abs() < 1e-3, "its ground's width over the view's height: {wide}");
    let middle = first.size * 0.5;
    assert!((arrow(&mut game)[0] - middle).length() < 30.0);
    // It points the way what it follows faces: the camera looks north, so its tip is over its back, up the
    // view; turned to face east, its tip is to the right.
    let pointing = |game: &mut Game| {
        let all = strokes(game);
        let back = all.iter().fold(Vec2::ZERO, |sum, (_, back)| sum + *back) / all.len() as f32;
        all[0].0 - back
    };
    let north = pointing(&mut game);
    assert!(north.y < -5.0 && north.x.abs() < 1.0, "facing north: {north:?}");
    let eye = find(&game, "Eye");
    let east = voxear_core::math::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    game.engine.world.get_mut::<voxear_scene::Transform>(eye).unwrap().rotation = east;
    settle(&mut game, None);
    let turned = pointing(&mut game);
    assert!(turned.x > 5.0 && turned.y.abs() < 1.0, "facing east: {turned:?}");

    // A map's buttons zoom to the next power of two, and its panel says which.
    let parts = game.ui.ui().map_view_parts(large).unwrap();
    let click = |game: &mut Game, button: NodeId| {
        let [x, y, w, h] = game.ui.ui().rect(button);
        game.ui.ui().pointer_move(x + w * 0.5, y + h * 0.5);
        game.ui.ui().pointer_button(true);
        game.ui.ui().pointer_button(false);
        settle(game, None);
    };
    click(&mut game, parts.zoom_in.unwrap());
    assert_eq!((shown(&game).cell, game.ui.ui().text(parts.zoom_value).as_str()), (2.0, "1:2"));
    click(&mut game, parts.zoom_out.unwrap());
    click(&mut game, parts.zoom_out.unwrap());
    assert_eq!((shown(&game).cell, game.ui.ui().text(parts.zoom_value).as_str()), (8.0, "1:8"));
    click(&mut game, parts.zoom_in.unwrap());
    assert_eq!(shown(&game).cell, 4.0);
    // Told the time and who it follows, it shows the time, and names its mark over it.
    let script = game.engine.world.resource_mut::<MapScript>().unwrap();
    (script.time, script.player) = (Some("14:12".into()), Some("Pixie".into()));
    settle(&mut game, None);
    let ui = game.ui.ui();
    assert!(ui.is_visible(parts.time_cell) && ui.text(parts.time) == "14:12");
    let tag = ui.rect(parts.tag.unwrap());
    assert!(ui.is_visible(parts.tag.unwrap()) && ui.text(parts.tag_name.unwrap()) == "Pixie");
    assert!((tag[0] + tag[2] * 0.5 - middle.x).abs() < 30.0 && tag[1] + tag[3] < middle.y, "over its dot: {tag:?}");

    // Asked to look at a place far to the east, it does, and what it follows stays at its west edge.
    wish_of(&mut game, number).look_at = Some(Vec3::new(640.0, 0.0, 40.0));
    settle(&mut game, None);
    let moved = shown(&game);
    assert!((moved.corner[0] as f32 + moved.camera.middle.x - 160.0).abs() < 0.01, "640 m is 160 cells");
    assert_eq!(seen(&game).offset, Vec2::new(600.0, 0.0));
    let at = arrow(&mut game);
    assert!(!at.is_empty(), "what it follows is still marked");
    assert!(
        at.iter().all(|point| point.x > 0.0 && point.x < middle.x - 100.0 && point.y > 0.0 && point.y < first.size.y)
    );
    // Unless the view is set not to keep it there.
    wish_of(&mut game, number).settings.push("arrow-at-edge=false".into());
    settle(&mut game, None);
    assert!(arrow(&mut game).is_empty());
    assert!(!seen(&game).settings.arrow_at_edge);

    // Turned and zoomed by a script: any number of blocks to a cell, shown in cells of the nearest power of two.
    *wish_of(&mut game, number) =
        voxear_engine::map::MapViewWish { turn: Some(1.0), zoom: Some(6.0), ..Default::default() };
    settle(&mut game, None);
    let now = seen(&game);
    assert_eq!((now.turn, now.zoom, now.shown.cell), (1.0, 6.0, 8.0));
    // The ground under a point of the view: looked up, it shows where it was pointed at.
    let place = now.place(middle).expect("ground under the view's middle");
    let (back, inside) = now.shown.point(place).unwrap();
    assert!(inside && (back - middle).length() < 8.0, "{place:?} shows at {back:?}");
    assert_eq!(now.place(Vec2::new(4.0, 4.0)), None, "nothing under a corner past the ground");

    // Set back: north up, over what it follows, at its settings' zoom; and kept within the zooms it is given.
    wish_of(&mut game, number).reset = true;
    settle(&mut game, None);
    let now = seen(&game);
    assert_eq!((now.turn, now.zoom, now.offset), (0.0, 4.0, Vec2::ZERO));
    wish_of(&mut game, number).settings.push("min-zoom=4 max-zoom=8".into());
    wish_of(&mut game, number).zoom = Some(1.0);
    settle(&mut game, None);
    assert_eq!(seen(&game).shown.cell, 4.0, "no nearer than 1:4");
    wish_of(&mut game, number).zoom = Some(64.0);
    settle(&mut game, None);
    assert_eq!(seen(&game).shown.cell, 8.0, "no further than 1:8");
}

#[test]
fn on_a_screen_the_size_of_their_design_the_minimap_and_the_map_are_as_designed() {
    use voxear_engine::map::MapScript;
    let settings = voxear_render::RendererSettings::default();
    let Ok(mut renderer) = voxear_render::Renderer::new_offscreen(1920, 1080, &settings) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let script = MapScript { time: Some("14:12".into()), player: Some("Pixie".into()), ..MapScript::default() };
    game.engine.world.insert_resource(script);
    let size = (1920, 1080);
    for _ in 0..5 {
        settle_at(&mut game, Some(&mut renderer), size);
    }
    let mini = view(&mut game, MapViewKind::Minimap);
    let ui = game.ui.ui();
    let parts = ui.map_view_parts(mini).unwrap();
    // The widget: 380 wide, 40 from the corner, its window 254 high, with a strip of two rows under it.
    assert_eq!(ui.rect(mini)[..3], [40.0, 40.0, 380.0]);
    let (window, strip) = (ui.rect(parts.picture), ui.rect(parts.region));
    assert!(
        window[3] == 254.0 && strip[1] >= window[1] + 254.0 && strip[3] > 50.0 && strip[3] < 90.0,
        "{window:?} {strip:?}"
    );
    assert_eq!((ui.text(parts.time).as_str(), ui.text(parts.zoom_value).as_str()), ("14:12", "1:2"));
    let compass = ui.rect(parts.compass);
    assert!(
        (compass[2], compass[3]) == (64.0, 64.0) && (compass[0] + 64.0 - (window[0] + window[2] - 8.0)).abs() < 0.5
    );
    keep("design-minimap.png", renderer.read_pixels().unwrap());

    let hud = find(&game, "Hud");
    Maps::set_open(&mut game.engine.world, hud, true);
    for _ in 0..8 {
        settle_at(&mut game, Some(&mut renderer), size);
    }
    let large = view(&mut game, MapViewKind::Map);
    let ui = game.ui.ui();
    let parts = ui.map_view_parts(large).unwrap();
    // The region panel and the strip of the air 48 from the top corners; the compass over the zoom, 48 from the
    // bottom right; nothing in the middle or the bottom left.
    let (region, air, zoom, compass) =
        (ui.rect(parts.region), ui.rect(parts.environment), ui.rect(parts.zoom), ui.rect(parts.compass));
    assert_eq!(region[..2], [48.0, 48.0]);
    assert!((air[0] + air[2] - 1872.0).abs() < 0.5 && air[1] == 48.0, "{air:?}");
    assert!((zoom[1] + zoom[3] - 1032.0).abs() < 0.5 && zoom[2] == 68.0 && compass[2] == 168.0, "{zoom:?} {compass:?}");
    assert!(((compass[1] + compass[3] + 18.0) - zoom[1]).abs() < 0.5, "the compass 18 over the zoom");
    assert!(region[0] + region[2] < 700.0 && region[1] + region[3] < 300.0 && air[0] > 1200.0 && compass[0] > 1600.0);
    assert!(ui.is_visible(parts.time_cell) && ui.text(parts.tag_name.unwrap()) == "Pixie");
    keep("design-map.png", renderer.read_pixels().unwrap());
}
