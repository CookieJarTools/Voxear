//! A Megascans download from Fab, picked with Import New Asset as its zip:
//! it is imported whole, in the background, into a folder named for the
//! asset inside the folder the menu was opened on, and its prefab places a
//! LOD Group of its models, drawn with its material. The download is the
//! fake one beside the tests, zipped. A test binary of its own, as it
//! changes `PATH` for the whole process, for a stand-in for the desktop's
//! file chooser. Skips itself without a GPU, and off Linux.

#![cfg(target_os = "linux")]

use std::{collections::HashSet, io::Write, os::unix::fs::PermissionsExt, path::Path};

use voxear_core::logging::LogCapture;
use voxear_engine::{Project, scene_file};
use voxear_render::{LodGroup, MeshRenderer, Renderer, RendererSettings};
use voxear_scene::Parent;
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, without its scripts, which the
/// editor would otherwise build.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
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
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> = project.lines().filter(|line| !line.starts_with("scripts")).collect();
    std::fs::write(&file, kept.join("\n")).unwrap();
    dir
}

/// The fake download beside the tests, zipped as Fab hands it over.
fn zipped_fixture(to: &Path) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/Fake_Boulder_xqzvbnd");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(to).unwrap());
    for entry in std::fs::read_dir(&fixture).unwrap().flatten() {
        let name = format!("Fake_Boulder_xqzvbnd/{}", entry.file_name().to_string_lossy());
        writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(&std::fs::read(entry.path()).unwrap()).unwrap();
    }
    writer.finish().unwrap();
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The Project panel's tile named `name`.
fn tile(ui: &Ui, name: &str) -> NodeId {
    let mut names = Vec::new();
    with_class(ui, ui.root(), "tile-name", &mut names);
    let label = names.into_iter().find(|&n| ui.is_visible(n) && ui.text(n) == name).expect("a tile for it");
    ui.parent(label).unwrap()
}

fn menu_item(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut items = Vec::new();
    with_class(ui, ui.root(), "menu-item", &mut items);
    items.into_iter().find(|&item| ui.children(item).iter().any(|&part| ui.text(part) == label))
}

fn point_at(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
}

#[test]
fn a_megascans_zip_picked_with_import_new_asset_comes_in_whole_and_places_its_lod_group() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let download = outside.path().join("Fake_Boulder_xqzvbnd.zip");
    zipped_fixture(&download);
    let chooser = outside.path().join("zenity");
    std::fs::write(&chooser, format!("#!/bin/sh\necho '{}'\n", download.display())).unwrap();
    std::fs::set_permissions(&chooser, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    // SAFETY: this binary's only test, which sets it before any thread reads it.
    unsafe { std::env::set_var("PATH", format!("{}:{path}", outside.path().display())) };

    let project = hello_copy();
    let project_dir = project.path().to_path_buf();
    let mut editor = voxear_editor::Editor::open_without_memory(
        Project::open(&project_dir).unwrap(),
        &mut renderer,
        LogCapture::default(),
    )
    .unwrap();
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }
    let folder = tile(&editor.ui, "Models");
    point_at(&mut editor.ui, folder);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    assert!(menu_item(&editor.ui, "Open Fab").is_some(), "the menu offers the store");
    let import = menu_item(&editor.ui, "Import New Asset…").expect("the menu imports");
    point_at(&mut editor.ui, import);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);

    let prefab = "Models/Fake Boulder/Fake Boulder.prefab";
    for _ in 0..500 {
        frame(&mut editor, &mut renderer);
        if editor.game.content.db().guid_of(prefab).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let guid = editor.game.content.db().guid_of(prefab).expect("the asset is imported, and the project sees it");
    assert!(!project_dir.join("Models/Fake_Boulder_xqzvbnd.zip").exists(), "the zip itself is not copied in");
    assert!(download.exists(), "and is left where it was");

    let text = format!("format = 1\n\n[[obj]]\nid = 1\nname = \"Boulder\"\nprefab = \"{guid}\"\n");
    let scene = scene_file::parse(&text, "a scene placing the boulder").unwrap();
    let game = &mut editor.game;
    let spawned =
        scene_file::spawn(&mut game.engine, &mut game.content, Some(&mut renderer), &scene, &HashSet::new()).unwrap();
    let world = &game.engine.world;
    let boulder =
        spawned.iter().find(|(at, _)| at.ids() == [1]).map(|(_, entity)| *entity).expect("the boulder is placed");
    let group = world.get::<LodGroup>(boulder).expect("with its LOD Group");
    assert_eq!(group.levels.len(), 2, "a level for each of the download's LODs");
    for level in &group.levels {
        let [obj] = level.objs.as_slice() else { panic!("one Obj a level") };
        assert_eq!(world.get::<Parent>(*obj).map(|parent| parent.0), Some(boulder));
        assert!(world.get::<MeshRenderer>(*obj).is_some(), "each drawing its LOD's model");
    }
    let [near, far] = [0, 1].map(|level| *world.get::<MeshRenderer>(group.levels[level].objs[0]).unwrap());
    assert_ne!(near.mesh, far.mesh, "a mesh of its own for each level");
    assert_eq!(near.material, far.material, "one material for both");
}
