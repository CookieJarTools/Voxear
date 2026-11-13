//! A level with a Map, played from the editor, stopped and played again:
//! the minimap is on the screen both times. The game's UI starts afresh
//! when play stops, and its maps with it. A file of its own, so it runs in
//! a process of its own, since .NET starts only once per process. Skips
//! itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::{Project, map::Map};
use voxear_render::{Renderer, RendererSettings};
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
    [[obj]]\nid = 2\nname = \"Eye\"\nposition = [40.0, 40.0, 40.0]\n[obj.camera]\nfov = 60.0\n\n\
    [[obj]]\nid = 3\nname = \"Hud\"\n[obj.map]\ncorner = \"top_left\"\n";

/// A project with a world of hills, a camera over it and a Map, inside the
/// repository, where its scripts find the engine to build against.
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

#[test]
fn a_map_is_on_the_screen_each_time_its_level_is_played() {
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
        editor.stop(&mut renderer);
        frame(&mut editor, &mut renderer);
        assert!(!editor.is_playing() && editor.game.ui.ui().map_views().is_empty(), "stopped {time}");
    }
}
