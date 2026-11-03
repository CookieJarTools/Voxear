//! Bake Sound Paths, from the command palette, on a small level of a floor
//! and a wall with a doorway: the paths are baked on a thread while the
//! status bar counts up, then written beside the level, where the level's
//! Inspector says they are. Skips itself without a GPU or Steam Audio.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const ROOMS: &str = r#"
format = 1

[[obj]]
id = 1
name = "Main Camera"
[obj.camera]
fov = 60.0

[[obj]]
id = 2
name = "Floor"
position = [0.0, -1.75, 0.0]
collider = { size = [20.0, 0.5, 20.0], acoustic = "concrete" }

[[obj]]
id = 3
name = "Wall Left"
position = [-8.0, 1.5, 3.0]
collider = { size = [4.0, 6.0, 0.4], acoustic = "concrete" }

[[obj]]
id = 4
name = "Wall Right"
position = [3.0, 1.5, 3.0]
collider = { size = [14.0, 6.0, 0.4], acoustic = "concrete" }
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("voxear.toml", "[project]\nname = \"Rooms\"\nstart_level = \"Levels/Main.level\"\n");
    write("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n");
    write("Scenes/Main.scene", ROOMS);
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn texts(ui: &Ui, node: NodeId, found: &mut Vec<String>) {
    found.push(ui.text(node));
    for child in ui.children(node) {
        texts(ui, child, found);
    }
}

#[test]
fn bake_sound_paths_writes_the_open_levels_paths_beside_it() {
    if voxear_engine::acoustics::bake_paths(&[], &mut |_| {}).is_err_and(|error| error.contains("library")) {
        return eprintln!("skipping: Steam Audio's library is not fetched");
    }
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else { return };
    let dir = project();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);

    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    editor.ui.text_input("bake sound paths");
    frame(&mut editor, &mut renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    frame(&mut editor, &mut renderer);

    let baked = dir.path().join("Levels/Main.paths");
    let started = std::time::Instant::now();
    while !baked.is_file() {
        assert!(started.elapsed().as_secs() < 60, "the paths are baked");
        std::thread::sleep(std::time::Duration::from_millis(20));
        frame(&mut editor, &mut renderer);
    }
    assert!(std::fs::metadata(&baked).unwrap().len() > 0);
    let mut shown = Vec::new();
    texts(&editor.ui, editor.ui.root(), &mut shown);
    assert!(shown.iter().any(|text| text.starts_with("Baked Main's sound paths")), "the status bar says so");
    assert!(editor.game.content.db().guid_of("Levels/Main.paths").is_some(), "an asset of the project");
}
