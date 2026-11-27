//! The Tests window on a copy of Hello whose scripts have tests: one that
//! passes, one whose Assert fails and one that throws, in two categories
//! listed in their order. Run All runs every one, a category's button its
//! own, and each comes out as it should, with what a failure said; the
//! editor's `tests.run` and `tests.list` calls do the same. A file of its
//! own, so it runs in a process of its own, since .NET starts only once per
//! process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const SIZE: (u32, u32) = (1280, 800);

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
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
    dir
}

/// The editor on a copy of Hello, once its scripts are described.
fn open_described(renderer: &mut Renderer, dir: &tempfile::TempDir) -> voxear_editor::Editor {
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, renderer, LogCapture::default()).unwrap();
    frame(&mut editor, renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    frame(&mut editor, renderer);
    editor
}

/// A renderer, or why the test cannot run.
fn renderer() -> Option<Renderer> {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return None;
    }
    let renderer = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok();
    if renderer.is_none() {
        eprintln!("skipping: no usable GPU");
    }
    renderer
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

/// The first label anywhere in `node` whose text starts with `words`.
fn label(ui: &Ui, node: NodeId, words: &str) -> Option<NodeId> {
    if ui.text(node).starts_with(words) && ui.children(node).is_empty() {
        return Some(node);
    }
    ui.children(node).into_iter().find_map(|child| label(ui, child, words))
}

const TESTS: &str = r#"using System;
using Voxear;
using Voxear.Testing;

namespace Hello;

internal static class GameTests
{
	[Test("Spinners are in the level", Category = "Level", Order = 2)]
	private static void SpinnersThere()
	{
		Assert.True(Obj.Find("Spinner A").IsValid, "Spinner A");
	}

	[Test("Sums add up", Category = "Maths", Order = 1)]
	private static void Sums()
	{
		Assert.Equal(4, 2 + 3, "two and three");
	}

	[Test("Nothing throws", Category = "Maths")]
	private static void Throws()
	{
		throw new InvalidOperationException("it broke");
	}
}
"#;

/// The Tests window's test rows, top to bottom: each title and what its mark says.
fn rows(ui: &Ui) -> Vec<(String, String)> {
    let mut found = Vec::new();
    with_class(ui, ui.root(), "tests-test", &mut found);
    found
        .into_iter()
        .map(|block| {
            let mut titles = Vec::new();
            with_class(ui, block, "tests-title", &mut titles);
            let mut marks = Vec::new();
            with_class(ui, block, "tests-mark", &mut marks);
            let mark = marks[0];
            let said = ["test-passed", "test-failed", "test-waiting", "test-not-run"]
                .into_iter()
                .find(|class| ui.has_class(mark, class))
                .unwrap_or("none");
            (ui.text(titles[0]), said.to_string())
        })
        .collect()
}

#[test]
fn the_tests_window_runs_every_test_a_category_or_one_and_says_how_each_came_out() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/GameTests.cs"), TESTS).unwrap();
    let mut editor = open_described(&mut renderer, &dir);
    // Opened from the command bar, by the Console.
    let field = editor.ui.find(editor.ui.root(), "command").unwrap();
    editor.ui.focus(field);
    frame(&mut editor, &mut renderer);
    editor.ui.text_input("tests");
    frame(&mut editor, &mut renderer);
    editor.palette_key(voxear_editor::PaletteKey::Enter);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    let beside = editor.ui.dock_panel_neighbours(dock, "tests");
    assert!(beside.contains(&"console".to_string()), "by the Console: {beside:?}");
    let listed = rows(&editor.ui);
    let titles: Vec<&str> = listed.iter().map(|(title, _)| title.as_str()).collect();
    // Maths first, by its lowest order, though Level comes first by name; in it, order 0 before order 1.
    assert_eq!(titles, ["Nothing throws", "Sums add up", "Spinners are in the level"]);
    assert!(listed.iter().all(|(_, mark)| mark == "test-not-run"));

    // Run All, one a frame.
    let mut buttons = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "tests-run-all", &mut buttons);
    let [x, y, w, h] = editor.ui.rect(buttons[0]);
    editor.ui.pointer_move(x + w / 2.0, y + h / 2.0);
    editor.ui.pointer_button(true);
    editor.ui.pointer_button(false);
    let started = std::time::Instant::now();
    while rows(&editor.ui).iter().any(|(_, mark)| mark == "test-waiting" || mark == "test-not-run") {
        assert!(started.elapsed().as_secs() < 120, "the tests never finished: {:?}", rows(&editor.ui));
        frame(&mut editor, &mut renderer);
    }
    let marks: Vec<String> = rows(&editor.ui).into_iter().map(|(_, mark)| mark).collect();
    assert_eq!(marks, ["test-failed", "test-failed", "test-passed"]);
    let mut failures = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "tests-failure", &mut failures);
    let said: Vec<String> =
        failures.into_iter().filter(|&f| editor.ui.is_visible(f)).map(|f| editor.ui.text(f)).collect();
    assert!(said[0].starts_with("InvalidOperationException: it broke"), "{said:?}");
    assert_eq!(said[1], "two and three: expected 4, found 5");
    frame(&mut editor, &mut renderer);
    shot(&mut renderer, "tests-window.png");
    let mut summary = Vec::new();
    with_class(&editor.ui, editor.ui.root(), "tests-summary", &mut summary);
    assert_eq!(editor.ui.text(summary[0]), "1 passed · 2 failed");

    // The editor's calls: a category's tests, then how they came out.
    let call = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, name: &str, args: serde_json::Value| {
        let answer = editor.call(name, args);
        let started = std::time::Instant::now();
        loop {
            frame(editor, renderer);
            if let Ok(answered) = answer.try_recv() {
                return answered.unwrap_or_else(|error| panic!("{name}: {error}"));
            }
            assert!(started.elapsed().as_secs() < 30, "{name} was not answered");
        }
    };
    let queued = call(&mut editor, &mut renderer, "tests.run", serde_json::json!({ "category": "Level" }));
    assert_eq!(queued["queued"], 1);
    for _ in 0..3 {
        frame(&mut editor, &mut renderer);
    }
    let listed = call(&mut editor, &mut renderer, "tests.list", serde_json::json!({}));
    assert_eq!(listed["running"], false);
    let level = listed["tests"].as_array().unwrap().iter().find(|t| t["category"] == "Level").unwrap().clone();
    assert_eq!((level["outcome"].as_str(), level["method"].as_str()), (Some("passed"), Some("SpinnersThere")));
}

/// The editor as it looks, saved when `VOXEAR_TEST_OUTPUT` names a folder.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
}
