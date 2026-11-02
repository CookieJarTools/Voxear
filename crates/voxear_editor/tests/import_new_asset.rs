//! Import New Asset, from the Project panel's menu, with a stand-in for the
//! desktop's file chooser that picks a picture: it is copied into the
//! folder the menu was opened on. A test binary of its own, as it changes
//! `PATH` for the whole process. Skips itself without a GPU, and off Linux.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, without its scripts, which the
/// editor would otherwise build.
fn hello_copy() -> tempfile::TempDir {
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
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let kept: Vec<&str> = project.lines().filter(|line| !line.starts_with("scripts")).collect();
    std::fs::write(&file, kept.join("\n")).unwrap();
    dir
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
fn import_new_asset_copies_what_the_file_chooser_picks_into_the_folder() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let picture = outside.path().join("Moss.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([60, 120, 40, 255])).save(&picture).unwrap();
    let chooser = outside.path().join("zenity");
    std::fs::write(&chooser, format!("#!/bin/sh\necho '{}'\n", picture.display())).unwrap();
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
    let import = menu_item(&editor.ui, "Import New Asset…").expect("the menu imports");
    point_at(&mut editor.ui, import);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    let copied = project_dir.join("Models/Moss.png");
    for _ in 0..200 {
        frame(&mut editor, &mut renderer);
        if copied.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(copied.exists(), "the picture is copied into the folder its menu was opened on");
    assert!(picture.exists(), "and left where it was");
}
