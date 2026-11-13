//! A level with a Map, played from the editor, stopped and played again:
//! the minimap is on the screen each time, and the Game view looks through
//! the game's camera. The game's UI starts afresh when play stops, and its
//! maps with it; and the Game view's camera is the first switched on, as
//! the game's is, though a script switched the level's first off for its
//! own to take over. A file of its own, so it runs in a process of its
//! own, since .NET starts only once per process. Skips itself without .NET
//! or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::{Project, map::Map};
use voxear_render::{Camera, Renderer, RendererSettings};
use voxear_scene::{Name, Switch};
use voxear_terrain::{
    blocks::{
        file::{BlockFile, BlockKind},
        generate::{BlockEndless, BlockGenerate},
    },
    generate::Preset,
};
use voxear_ui::MapViewKind;

const SIZE: (u32, u32) = (1280, 800);

const SCENE: &str = "format = 1\n\n\
    [[obj]]\nid = 1\nname = \"Blocks\"\nposition = [0.0, 0.0, 0.0]\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
    [[obj]]\nid = 2\nname = \"Eye\"\nposition = [40.0, 40.0, 40.0]\nrotation = [-90.0, 0.0, 0.0]\n\
    [obj.camera]\nfov = 60.0\n\n\
    [[obj]]\nid = 3\nname = \"Hud\"\n[obj.map]\ncorner = \"top_left\"\n\n\
    [[obj]]\nid = 4\nname = \"Over\"\nposition = [40.5, 110.0, 40.5]\nrotation = [90.0, 0.0, 0.0]\n\
    [obj.camera]\nfov = 60.0\n\n\
    [[obj]]\nid = 5\nname = \"Sun\"\nrotation = [42.0, -35.0, 0.0]\n\
    [obj.light]\ntype = \"directional\"\ncolor = [1.0, 0.95, 0.88]\nintensity = 3.2\n";

/// A project with a world of hills, a Map, and two cameras: the level's
/// first, Eye, which looks up, and Over, which looks straight down at the
/// hills from above them. Inside the repository, where its scripts find the
/// engine to build against.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let mut file = BlockFile::default();
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Sea", [0.16, 0.45, 0.9]) });
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

/// Whether the playing game's Map has its minimap on the screen, showing
/// the world.
fn minimap_shows(editor: &mut voxear_editor::Editor) -> bool {
    let game = &mut editor.game;
    let ui = game.ui.ui();
    let views = ui.map_views();
    let mini =
        views.iter().find(|node| ui.map_view_settings(**node).is_some_and(|set| set.kind == MapViewKind::Minimap));
    let on_screen = mini.is_some_and(|mini| ui.is_visible(*mini) && game.maps.shown(*mini).is_some());
    let numbered = game.engine.world.query_ref::<&Map>().into_iter().any(|map| map.views.is_some());
    on_screen && numbered
}

/// How much of the middle of a picture is grass: greener than it is red or
/// blue.
fn grass_in_the_middle(path: &std::path::Path) -> f32 {
    let picture = image::open(path).unwrap().to_rgb8();
    let (width, height) = picture.dimensions();
    let middle = (width / 4..width * 3 / 4).flat_map(|x| (height / 4..height * 3 / 4).map(move |y| (x, y)));
    let green = |(x, y): (u32, u32)| {
        let [r, g, b] = picture.get_pixel(x, y).0.map(f32::from);
        g > r * 1.1 && g > b * 1.1
    };
    let (mut all, mut grass) = (0.0, 0.0);
    for at in middle {
        all += 1.0;
        grass += f32::from(u8::from(green(at)));
    }
    grass / all
}

#[test]
fn a_map_is_on_the_screen_and_the_game_view_looks_through_the_games_camera_each_time_its_level_is_played() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let dir = project();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let frame = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer| {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(8));
    };
    let until = |editor: &mut voxear_editor::Editor,
                 renderer: &mut Renderer,
                 what: &str,
                 done: &dyn Fn(&mut voxear_editor::Editor) -> bool| {
        let started = std::time::Instant::now();
        while !done(editor) {
            assert!(started.elapsed().as_secs() < 180, "{what} never happened: {:?}", editor.play_error());
            frame(editor, renderer);
        }
    };
    for time in ["the first time", "the second time", "the third time"] {
        assert!(!minimap_shows(&mut editor), "nothing of the game's is on the screen while editing");
        editor.play(&mut renderer);
        until(&mut editor, &mut renderer, &format!("the minimap showing, played {time}"), &|editor| {
            editor.is_playing() && minimap_shows(editor)
        });
        // The level's first camera is switched off as the game begins, as a script does whose own camera
        // takes over: the game looks through the next, and so does the Game view, with the minimap on.
        let world = &mut editor.game.engine.world;
        let eye =
            world.query_ref::<(voxear_ecs::Entity, &Camera, &Name)>().into_iter().find(|(.., name)| name.0 == "Eye");
        let eye = eye.expect("the level's first camera").0;
        voxear_scene::set_component_on(world, eye, Switch::Camera, false);
        // Once the ground round the camera is made, and a few frames on for the view to settle.
        until(&mut editor, &mut renderer, &format!("the world made, played {time}"), &|editor| {
            voxear_engine::blocks::streamed(&editor.game.engine.world)
        });
        (0..30).for_each(|_| frame(&mut editor, &mut renderer));
        // Kept where `VOXEAR_TEST_OUTPUT` names a folder, to look at.
        let kept = std::env::var("VOXEAR_TEST_OUTPUT").map(std::path::PathBuf::from);
        let name = format!("game-view-{}.png", time.replace(' ', "-"));
        let picture = kept.map_or(dir.path().join("game.png"), |folder| folder.join(name));
        let _ = std::fs::remove_file(&picture);
        let asked = editor.call("capture", serde_json::json!({ "view": "game", "path": picture.to_string_lossy() }));
        until(&mut editor, &mut renderer, &format!("the Game view captured, played {time}"), &|_| {
            asked.try_recv().is_ok_and(|answer| answer.is_ok())
        });
        let grass = grass_in_the_middle(&picture);
        assert!(grass > 0.6, "played {time}, the Game view looks down at the hills through Over: {grass} of it grass");
        assert!(minimap_shows(&mut editor), "and the minimap is still on");
        editor.stop(&mut renderer);
        frame(&mut editor, &mut renderer);
        assert!(!editor.is_playing() && editor.game.ui.ui().map_views().is_empty(), "stopped {time}");
    }
}
