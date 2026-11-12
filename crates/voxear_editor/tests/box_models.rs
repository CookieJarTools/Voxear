//! Box models in the editor: New Box Model makes a block on one bone, the
//! Inspector shows what it is made of, and a model changed in its file while
//! the editor was away is drawn as the file now has it, bones and all, by
//! the Objs already drawing it. Skips itself without a GPU.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_engine::Project;
use voxear_render::{BoxModel, ModelRenderer, Renderer, RendererSettings};
use voxear_scene::Name;
use voxear_ui::{NodeId, Ui};
use winit::event::WindowEvent;

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, without its scripts, which the
/// editor would otherwise build, and with an Obj drawing a box model the
/// project does not have yet.
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
fn tile(ui: &Ui, name: &str) -> Option<NodeId> {
    let mut names = Vec::new();
    with_class(ui, ui.root(), "tile-name", &mut names);
    let label = names.into_iter().find(|&n| ui.is_visible(n) && ui.text(n) == name)?;
    ui.parent(label)
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

fn texts_in(ui: &Ui, node: NodeId) -> Vec<String> {
    let mut out = vec![ui.text(node)];
    for child in ui.children(node) {
        out.extend(texts_in(ui, child));
    }
    out.retain(|text| !text.is_empty());
    out
}

/// The value the Inspector shows beside `label`.
fn row(shown: &[String], label: &str) -> String {
    let at = shown.iter().position(|text| text == label).unwrap_or_else(|| panic!("no {label} in {shown:#?}"));
    shown[at + 1].clone()
}

#[test]
fn a_new_box_model_is_made_shown_in_the_inspector_and_followed_as_its_file_changes() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let project = hello_copy();
    let dir = project.path().to_path_buf();
    let mut editor =
        voxear_editor::Editor::open_without_memory(Project::open(&dir).unwrap(), &mut renderer, LogCapture::default())
            .unwrap();
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }

    // New Box Model, from the menu of the Models folder.
    let folder = tile(&editor.ui, "Models").expect("Hello's Models folder");
    point_at(&mut editor.ui, folder);
    editor.ui.pointer_secondary(true);
    editor.ui.pointer_secondary(false);
    frame(&mut editor, &mut renderer);
    // The menu is longer than this window is high: the wheel brings its last items up.
    let import = menu_item(&editor.ui, "Import New Asset…").expect("the folder's menu");
    point_at(&mut editor.ui, import);
    editor.ui.pointer_scroll(0.0, 1000.0);
    frame(&mut editor, &mut renderer);
    let new = menu_item(&editor.ui, "New Box Model").expect("the menu makes box models");
    let [_, top, _, height] = editor.ui.rect(new);
    assert!(top + height <= SIZE.1 as f32, "in the window: {top} + {height}");
    point_at(&mut editor.ui, new);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let path = "Models/New Box Model.boxmodel";
    let file = dir.join(path);
    let made = BoxModel::parse(&std::fs::read_to_string(&file).expect("made in the folder")).unwrap();
    assert_eq!(made, BoxModel::starter());
    assert!(dir.join(format!("{path}.meta")).is_file(), "given a GUID");

    // The Inspector: its kind, and what it is made of.
    assert!(editor.inspect_path(path));
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let inspector = editor.ui.find(editor.ui.root(), "inspector").unwrap();
    let shown = texts_in(&editor.ui, inspector);
    assert_eq!(row(&shown, "Kind"), "Box Model");
    assert_eq!(row(&shown, "Scale"), "16 px per block");
    assert_eq!(row(&shown, "Sheet"), "64 × 32 px");
    assert_eq!((row(&shown, "Bones"), row(&shown, "Boxes")), ("1".to_string(), "1".to_string()));
    assert!(!shown.iter().any(|text| text == "Import Settings"), "a mesh file's settings do not apply: {shown:#?}");

    // An Obj drawing it gets its one part, skinned, and its bone.
    let guid = editor.game.content.db().guid_of(path).unwrap();
    let data = voxear_engine::scene_file::ModelData { model: path.to_string(), lod_bias: 0.0, parts: Vec::new() };
    let drawn = voxear_engine::prefab::model_renderer(&mut editor.game.content, &mut renderer, &data).unwrap();
    assert_eq!((drawn.parts.len(), drawn.parts[0].skin), (1, Some(0)));
    let mesh = drawn.parts[0].mesh;
    let source = voxear_engine::scene_file::ModelSource(data);
    let walker =
        editor.game.engine.world.spawn((Name("Walker".into()), voxear_scene::Transform::default(), source, drawn));
    frame(&mut editor, &mut renderer);
    let (least, most) = renderer.mesh_bounds(mesh).unwrap();
    assert!((least - Vec3::new(-0.5, 0.0, -0.5)).length() < 1e-4 && (most - Vec3::new(0.5, 1.0, 0.5)).length() < 1e-4);

    // Given a head in its file while the editor was away: the Obj draws it, behind the same mesh, with the new bone.
    let mut taller = made.clone();
    let head = taller.add_bone("Head", Some(0), Vec3::new(0.0, 16.0, 0.0));
    taller.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
    std::fs::write(&file, taller.to_text()).unwrap();
    editor.handle(&WindowEvent::Focused(false));
    editor.handle(&WindowEvent::Focused(true));
    for _ in 0..2 {
        frame(&mut editor, &mut renderer);
    }
    let (_, most) = renderer.mesh_bounds(mesh).unwrap();
    assert!((most.y - 1.5).abs() < 1e-4, "a head half a block high on the block: {most}");
    let drawn = editor.game.engine.world.get::<ModelRenderer>(walker).unwrap();
    let bones: Vec<&str> =
        drawn.animation.as_ref().unwrap().skeleton.bones.iter().map(|bone| bone.name.as_str()).collect();
    assert_eq!(bones, ["Body", "Head"]);
    assert_eq!(editor.game.content.model_animation(guid).unwrap().skeleton.bones.len(), 2);
}
