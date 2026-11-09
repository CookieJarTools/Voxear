//! A `ClipFrame` read while the editor edits: a data asset's inspector,
//! written in C#, lists the bones of its clip's own skeleton by
//! `ClipFrame.BoneName`, once the editor has found the clip, with nothing
//! playing. A file of its own, so it runs in a process of its own, since
//! .NET starts only once per process. Skips itself without .NET or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_ui::{NodeId, Ui};

const GAIT: &str = r#"using Voxear;

namespace Hello;

public sealed class Gait : Definition
{
	public AnimationClip Walk;
}
"#;

const GAIT_INSPECTOR: &str = r#"using System.Linq;
using Voxear;

namespace Hello;

public sealed class GaitInspector : DefinitionInspector<Gait>
{
	public override string Markup => """
		<vui>
			<Label name="bones" />
		</vui>
		""";

	protected override void Draw(Gait gait, UiDocument ui)
	{
		var frame = ClipFrame.AtRest(gait.Walk);
		var names = frame == null ? "finding" : string.Join(", ", Enumerable.Range(0, frame.BoneCount).Select(frame.BoneName));
		ui.Find("bones")!.Text = $"bones: {names}";
	}
}
"#;

/// A model file with a clip, Walk, that moves the hips under the root, its
/// data beside it.
const WALKS: &str = r#"{
  "asset": { "version": "2.0" },
  "buffers": [{ "uri": "Walks.bin", "byteLength": 32 }],
  "bufferViews": [
    { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
    { "buffer": 0, "byteOffset": 8, "byteLength": 24 }
  ],
  "accessors": [
    { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] },
    { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" }
  ],
  "nodes": [{ "name": "root", "children": [1] }, { "name": "hips", "translation": [0, 0.9, 0] }],
  "animations": [
    {
      "name": "Walk",
      "samplers": [{ "input": 0, "output": 1 }],
      "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }]
    }
  ],
  "scenes": [{ "nodes": [0] }]
}"#;

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

/// The first element in `node`, or `node` itself, whose own text starts
/// with `text`, and what it says.
fn starting(ui: &Ui, node: NodeId, text: &str) -> Option<String> {
    if ui.text(node).starts_with(text) && ui.children(node).is_empty() {
        return Some(ui.text(node));
    }
    ui.children(node).into_iter().find_map(|child| starting(ui, child, text))
}

#[test]
fn a_clip_frame_names_its_clips_bones_in_an_inspector_while_editing() {
    let Some(mut renderer) = renderer() else { return };
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Gait.cs"), GAIT).unwrap();
    std::fs::write(dir.path().join("Scripts/GaitInspector.cs"), GAIT_INSPECTOR).unwrap();
    std::fs::create_dir_all(dir.path().join("Gaits")).unwrap();
    std::fs::write(dir.path().join("Models/Walks.gltf"), WALKS).unwrap();
    let values = [0.0f32, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5];
    std::fs::write(
        dir.path().join("Models/Walks.bin"),
        values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>(),
    )
    .unwrap();
    let asset = "type = \"Hello.Gait\"\n\n[fields]\nwalk = \"Models/Walks.gltf#clip:Walk\"\n";
    std::fs::write(dir.path().join("Gaits/Walker.asset"), asset).unwrap();

    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    frame(&mut editor, &mut renderer);
    let started = std::time::Instant::now();
    while !editor.scripts_described() {
        assert!(started.elapsed().as_secs() < 300, "the scripts take too long to build");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(editor.inspect_path("Gaits/Walker.asset"));

    // Nothing plays, so the editor finds the clip the inspector asked for, and the inspector draws again.
    let started = std::time::Instant::now();
    loop {
        let said = starting(&editor.ui, editor.ui.root(), "bones: ");
        if said.as_deref() == Some("bones: root, hips") {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "the clip's bones never showed: {said:?}");
        frame(&mut editor, &mut renderer);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!editor.is_playing());
}
