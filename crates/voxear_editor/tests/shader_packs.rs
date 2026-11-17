//! Shader packs in the editor, on a real GPU, offscreen: worn from the
//! Lighting page's Shader pack menu and from a pack's Inspector page, whose
//! options change the pack worn; and through the calls tools make,
//! `shader_pack.list`, `shader_pack.wear` and `shader_pack.options`. Each
//! test skips itself (with a note) on machines without a usable Vulkan
//! device.

use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use voxear_core::logging::LogCapture;
use voxear_engine::{Project, shader_packs};
use voxear_render::{Renderer, RendererSettings, shader_packs::WornShaderPack};
use voxear_ui::{Key, NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

const GRADE: &str =
    "float4 run(PassInput input)\n{\n    return float4(sceneColor(input.uv).rgb * options().strength, 1.0);\n}\n";

const MANIFEST: &str = r#"
description = "A test grade."

[options]
strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength", about = "How bright." }
soft = { kind = "toggle", default = false, label = "Soft" }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color"]
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Packs\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n"),
        ("Scenes/Main.scene", "format = 1\n"),
        ("Packs/Warm.shaderpack/pack.toml", MANIFEST),
        ("Packs/Warm.shaderpack/grade.slang", GRADE),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn open(dir: &tempfile::TempDir) -> Option<(Renderer, voxear_editor::Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    Some((renderer, editor))
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Frames until `done`, for ten seconds at most.
fn frames_until(
    editor: &mut voxear_editor::Editor,
    renderer: &mut Renderer,
    done: impl Fn(&voxear_editor::Editor) -> bool,
) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        frame(editor, renderer);
        if done(editor) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// A call made as a tool makes it, answered once frames have run it.
fn call(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str, args: Value) -> Value {
    let answer = editor.call(name, args);
    let began = Instant::now();
    loop {
        frame(editor, renderer);
        match answer.try_recv() {
            Ok(answered) => return answered.unwrap_or_else(|error| panic!("{name}: {error}")),
            Err(TryRecvError::Empty) => assert!(began.elapsed().as_secs() < 60, "{name} was not answered"),
            Err(TryRecvError::Disconnected) => panic!("{name} was dropped"),
        }
    }
}

fn click(ui: &mut Ui, node: NodeId) {
    let [x, y, w, h] = ui.rect(node);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
}

fn with_class(ui: &Ui, node: NodeId, class: &str, found: &mut Vec<NodeId>) {
    if ui.has_class(node, class) {
        found.push(node);
    }
    for child in ui.children(node) {
        with_class(ui, child, class, found);
    }
}

/// The Inspector's row whose label is `label`.
fn row(ui: &Ui, label: &str) -> Option<NodeId> {
    let mut rows = Vec::new();
    with_class(ui, ui.root(), "prop", &mut rows);
    rows.into_iter().find(|&row| ui.children(row).first().is_some_and(|&first| ui.text(first) == label))
}

fn in_row(ui: &Ui, label: &str, class: &str) -> NodeId {
    let mut found = Vec::new();
    with_class(ui, row(ui, label).unwrap_or_else(|| panic!("a row {label}")), class, &mut found);
    *found.first().unwrap_or_else(|| panic!("{class} in the row {label}"))
}

fn worn(editor: &voxear_editor::Editor) -> Option<String> {
    shader_packs::worn(&editor.game.engine.world).map(str::to_string)
}

fn drawn(editor: &voxear_editor::Editor) -> Option<u64> {
    editor.game.engine.world.resource::<WornShaderPack>().map(|worn| worn.pack.generation)
}

#[test]
fn tools_list_wear_and_set_a_shader_pack() {
    let dir = project();
    let Some((mut renderer, mut editor)) = open(&dir) else { return };
    let listed = call(&mut editor, &mut renderer, "shader_pack.list", json!({}));
    let names: Vec<&str> =
        listed["packs"].as_array().unwrap().iter().filter_map(|pack| pack["name"].as_str()).collect();
    assert_eq!(names, ["Evening", "Warm"]);
    assert!(listed["worn"].is_null());

    let answer = call(&mut editor, &mut renderer, "shader_pack.wear", json!({ "pack": "builtin:evening" }));
    assert_eq!(answer["worn"]["reference"], "builtin:evening");
    assert_eq!(answer["worn"]["standing"]["state"], "drawn");
    let passes: Vec<&str> =
        answer["worn"]["passes"].as_array().unwrap().iter().filter_map(|p| p["name"].as_str()).collect();
    assert_eq!(passes, ["Rays", "Evening light", "Evening grade"]);
    frame(&mut editor, &mut renderer);
    assert_eq!(renderer.shader_pack_drawn(), drawn(&editor), "the Scene view drawn under it");

    let set = call(&mut editor, &mut renderer, "shader_pack.options", json!({ "option": "warmth", "value": 0.25 }));
    let warmth = set["worn"]["options"].as_array().unwrap().iter().find(|o| o["key"] == "warmth").unwrap().clone();
    assert_eq!((warmth["value"].as_f64(), warmth["kind"].as_str()), (Some(0.25), Some("number")));
    assert!(warmth["about"].as_str().is_some_and(|about| !about.is_empty()), "its tooltip");
    let rays = call(&mut editor, &mut renderer, "shader_pack.options", json!({ "option": "rays", "value": false }));
    let rays = rays["worn"]["options"].as_array().unwrap().iter().find(|o| o["key"] == "rays").unwrap().clone();
    assert_eq!(rays["value"], false);
    let worn_options = editor.game.engine.world.resource::<WornShaderPack>().unwrap().options;
    assert_eq!((worn_options[0], worn_options[3]), (0.0, 0.25), "the frame's");
    // A pack that does not compile, or is not there, says why and leaves the one worn.
    let refused = editor.call("shader_pack.wear", json!({ "pack": "builtin:gloomy" }));
    frame(&mut editor, &mut renderer);
    let said = refused.try_recv().unwrap().unwrap_err();
    assert!(said.to_string().contains("gloomy"), "{said}");
    assert_eq!(worn(&editor).as_deref(), Some("builtin:evening"));
    let off = call(&mut editor, &mut renderer, "shader_pack.wear", json!({ "pack": null }));
    assert!(off["worn"].is_null() && worn(&editor).is_none());
    frame(&mut editor, &mut renderer);
    assert_eq!(renderer.shader_pack_drawn(), None);
}

#[test]
fn the_lighting_page_and_a_packs_inspector_page_wear_it_and_set_its_options() {
    let dir = project();
    let Some((mut renderer, mut editor)) = open(&dir) else { return };
    let rail = editor.ui.find(editor.ui.root(), "rail-lighting").unwrap();
    click(&mut editor.ui, rail);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    // The menu's first after None is the engine's Evening, compiled on a thread and drawn once it has.
    let menu = editor.lighting_shader_menu();
    editor.ui.focus(menu);
    editor.ui.key_down(Key::Down);
    frame(&mut editor, &mut renderer);
    assert_eq!(worn(&editor).as_deref(), Some("builtin:evening"));
    assert!(frames_until(&mut editor, &mut renderer, |editor| drawn(editor).is_some()), "compiled and drawn");
    assert_eq!(editor.ui.selected_option(menu), 1, "the menu shows the pack worn");
    editor.ui.focus(menu);
    editor.ui.key_down(Key::Up);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(worn(&editor).is_none() && drawn(&editor).is_none());

    // The project's pack, selected: its page wears it, and its options change the pack worn.
    assert!(editor.inspect_path("Packs/Warm.shaderpack/pack.toml"));
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let description = in_row(&editor.ui, "Description", "value");
    assert_eq!(editor.ui.text(description), "A test grade.");
    let grade = in_row(&editor.ui, "Grade", "value");
    assert_eq!(editor.ui.text(grade), "Final · color → the colour");
    let toggle = in_row(&editor.ui, "Worn", "checkbox");
    click(&mut editor.ui, toggle);
    frame(&mut editor, &mut renderer);
    let guid = editor.game.content.db().guid_of("Packs/Warm.shaderpack/pack.toml").unwrap().to_string();
    assert_eq!(worn(&editor), Some(guid));
    assert!(frames_until(&mut editor, &mut renderer, |editor| drawn(editor).is_some()), "compiled and drawn");
    frame(&mut editor, &mut renderer);
    let compiled = in_row(&editor.ui, "Compiled", "value");
    assert_eq!(editor.ui.text(compiled), "Compiled");
    let soft = in_row(&editor.ui, "Soft", "checkbox");
    click(&mut editor.ui, soft);
    frame(&mut editor, &mut renderer);
    let strength = in_row(&editor.ui, "Strength", "number-input");
    editor.ui.focus(strength);
    editor.ui.select_all_text(strength);
    editor.ui.text_input("1.5");
    editor.ui.key_down(Key::Enter);
    frame(&mut editor, &mut renderer);
    frame(&mut editor, &mut renderer);
    let options: Vec<f32> =
        shader_packs::options(&editor.game.engine.world).into_iter().map(|(_, value)| value).collect();
    assert_eq!(options, [1.5, 1.0]);
    // A mistake saved shows on the page, with its file and line, and the pack drawn before carries on.
    let before = drawn(&editor);
    std::fs::write(
        dir.path().join("Packs/Warm.shaderpack/grade.slang"),
        "float4 run(PassInput input)\n{\n    return nothing;\n}\n",
    )
    .unwrap();
    let failed = |editor: &voxear_editor::Editor| {
        editor.ui.text(in_row(&editor.ui, "Compiled", "value")).contains("grade.slang:3")
    };
    assert!(frames_until(&mut editor, &mut renderer, failed), "the page says where");
    assert_eq!(drawn(&editor), before);
}
