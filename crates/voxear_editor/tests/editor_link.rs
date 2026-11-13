//! The editor driven through its link, as `vox editor call` and `vox mcp`
//! drive it: a headless editor on a copy of Hello, given calls over the
//! socket from another thread, or from the test itself, while the test runs
//! the editor's frames. Skips itself without a GPU.

use std::{
    path::Path,
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use voxear_core::logging::LogCapture;
use voxear_editor::{Editor, link};
use voxear_engine::{
    Project,
    local_link::{Answer, ErrorKind, LinkClient},
};
use voxear_render::{Renderer, RendererSettings};

const SIZE: (u32, u32) = (1280, 800);
/// The longest a test waits on the editor.
const PATIENCE: Duration = Duration::from_secs(120);

/// A copy of the Hello sample, without its scripts: the editor would build
/// them in the background, and a build outlives the test and its copy.
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
    let dir = tempfile::tempdir().unwrap();
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    let file = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, project.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
    dir
}

/// The editor open on `dir`, a frame in, or `None` without a GPU.
fn open(dir: &Path) -> Option<(Renderer, Editor)> {
    open_logging(dir, LogCapture::default())
}

/// The same, its Console showing what is put in `log`.
fn open_logging(dir: &Path, log: LogCapture) -> Option<(Renderer, Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let project = Project::open(dir).unwrap();
    let mut editor = Editor::open_without_memory(project, &mut renderer, log).unwrap();
    frames(&mut editor, &mut renderer, 2);
    Some((renderer, editor))
}

fn frames(editor: &mut Editor, renderer: &mut Renderer, count: u32) {
    for _ in 0..count {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
    }
}

/// A call made from the test, answered once frames have run it.
fn ask(editor: &mut Editor, renderer: &mut Renderer, call: &str, args: Value) -> Answer {
    let answer = editor.call(call, args);
    let began = Instant::now();
    loop {
        frames(editor, renderer, 1);
        match answer.try_recv() {
            Ok(answered) => return answered,
            Err(TryRecvError::Empty) => assert!(began.elapsed() < PATIENCE, "{call} was not answered"),
            Err(TryRecvError::Disconnected) => panic!("{call} was dropped"),
        }
    }
}

/// [`ask`], the call expected to be done.
fn done(editor: &mut Editor, renderer: &mut Renderer, call: &str, args: Value) -> Value {
    ask(editor, renderer, call, args).unwrap_or_else(|error| panic!("{call}: {error}"))
}

/// Runs the editor's frames while `tool` talks to it over the socket from
/// a thread of its own, as another program would, and returns what it
/// returned.
fn over_the_link<T: Send + 'static>(
    editor: &mut Editor,
    renderer: &mut Renderer,
    root: &Path,
    tool: impl FnOnce(LinkClient) -> T + Send + 'static,
) -> T {
    editor.serve_link(true).unwrap();
    let folder = link::link_folder(root);
    let talking = std::thread::spawn(move || tool(LinkClient::connect(&folder, link::LINK_FILE).unwrap()));
    let began = Instant::now();
    while !talking.is_finished() {
        frames(editor, renderer, 1);
        assert!(began.elapsed() < PATIENCE, "the tool did not finish");
    }
    talking.join().unwrap()
}

/// A call over the link, expected to be done.
fn call(client: &mut LinkClient, name: &str, args: Value) -> Value {
    client.call(name, args).unwrap().unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// A component's field as `obj.get` shows it.
fn field<'a>(obj: &'a Value, component: &str, name: &str) -> &'a Value {
    let components = obj["components"].as_array().unwrap();
    let found = components.iter().find(|c| c["component"] == component).unwrap_or_else(|| panic!("no {component}"));
    let fields = found["fields"].as_array().unwrap();
    fields.iter().find(|f| f["field"] == name).unwrap_or_else(|| panic!("{component} has no {name}: {fields:?}"))
}

#[test]
fn a_tool_makes_an_obj_sets_a_field_undoes_saves_and_captures() {
    let dir = hello_copy();
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let Some((mut renderer, mut editor)) = open_logging(dir.path(), log) else { return };
    let capture = dir.path().join("Lamp.png");
    let shot = capture.clone();
    let (lamp, before_undo, after_undo, saved, pictured, logged) =
        over_the_link(&mut editor, &mut renderer, dir.path(), move |mut client| {
            let state = call(&mut client, "editor.state", json!({}));
            assert_eq!(state["playing"], false);
            let made = call(
                &mut client,
                "obj.create",
                json!({ "kind": "Point Light", "name": "Lamp", "position": [0.0, 3.0, 0.0] }),
            );
            let lamp = made["obj"].clone();
            let set = call(&mut client, "field.set", json!({ "obj": lamp, "component": "light", "field": "intensity", "value": 7.5 }));
            assert_eq!(set["changed"], true);
            let before_undo = call(&mut client, "obj.get", json!({ "obj": lamp }));
            call(&mut client, "undo", json!({}));
            let after_undo = call(&mut client, "obj.get", json!({ "obj": lamp }));
            call(&mut client, "redo", json!({}));
            let saved = call(&mut client, "save", json!({}));
            let pictured = call(&mut client, "capture", json!({ "view": "scene", "path": shot.display().to_string() }));
            let logged = call(&mut client, "console.read", json!({ "level": "info" }));
            (lamp, before_undo, after_undo, saved, pictured, logged)
        });
    assert_eq!(before_undo["name"], "Lamp");
    assert_eq!(before_undo["position"], json!([0.0, 3.0, 0.0]));
    assert_eq!(field(&before_undo, "light", "intensity")["value"], 7.5);
    assert_ne!(field(&after_undo, "light", "intensity")["value"], 7.5, "undo took the change back");
    // The Obj is named the same through undo and redo.
    assert!(link::resolve_obj(&editor.game, &lamp).is_ok());

    let written = saved["saved"].as_array().unwrap();
    assert!(!written.is_empty(), "nothing was saved");
    let scene = std::fs::read_to_string(dir.path().join(written[0].as_str().unwrap())).unwrap();
    assert!(scene.contains("name = \"Lamp\""), "the saved scene has the Lamp:\n{scene}");
    assert!(scene.contains("intensity = 7.5"), "and its intensity:\n{scene}");
    assert!(!editor.has_unsaved_changes());

    assert!(pictured["width"].as_u64().unwrap() > 16 && pictured["height"].as_u64().unwrap() > 16);
    let picture = image::open(&capture).unwrap().to_rgba8();
    assert!(picture.pixels().any(|p| p.0[..3] != picture.get_pixel(0, 0).0[..3]), "the capture is one colour");

    let lines = logged["lines"].as_array().unwrap();
    assert!(lines.iter().any(|line| line["message"].as_str().unwrap_or_default().contains("[Tools] obj.create")));
}

#[test]
fn a_call_without_the_secret_is_refused_and_changes_nothing() {
    use std::io::{BufRead, BufReader, Write};
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    editor.serve_link(true).unwrap();
    let before = done(&mut editor, &mut renderer, "objs.list", json!({}))["objs"].as_array().unwrap().len();
    let file = link::link_folder(dir.path()).join(link::LINK_FILE);
    let about: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let address: std::net::SocketAddr = about["address"].as_str().unwrap().parse().unwrap();
    assert!(address.ip().is_loopback(), "the link listens beyond this computer");
    let talking = std::thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        writeln!(stream, "a guess").unwrap();
        let mut reply = String::new();
        BufReader::new(stream.try_clone().unwrap()).read_line(&mut reply).unwrap();
        let _ = writeln!(stream, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "obj.create", "params": { "kind": "Cube" } }));
        reply
    });
    while !talking.is_finished() {
        frames(&mut editor, &mut renderer, 1);
    }
    let reply: Value = serde_json::from_str(&talking.join().unwrap()).unwrap();
    assert_eq!(reply["error"]["data"]["kind"], "not_allowed");
    frames(&mut editor, &mut renderer, 5);
    let after = done(&mut editor, &mut renderer, "objs.list", json!({}))["objs"].as_array().unwrap().len();
    assert_eq!(before, after, "a call without the secret made an Obj");
}

#[test]
fn a_batch_is_one_step_to_undo() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let objs = |editor: &mut Editor, renderer: &mut Renderer| {
        done(editor, renderer, "objs.list", json!({}))["objs"].as_array().unwrap().len()
    };
    let before = objs(&mut editor, &mut renderer);
    let batch = done(
        &mut editor,
        &mut renderer,
        "batch",
        json!({ "calls": [
            { "call": "obj.create", "args": { "kind": "Cube", "name": "A" } },
            { "call": "obj.create", "args": { "kind": "Sphere", "name": "B" } },
        ] }),
    );
    assert_eq!(batch["results"].as_array().unwrap().len(), 2, "{batch}");
    assert_eq!(objs(&mut editor, &mut renderer), before + 2);
    let undone = done(&mut editor, &mut renderer, "undo", json!({}));
    assert_eq!(undone["undone"], true);
    assert_eq!(objs(&mut editor, &mut renderer), before, "one undo took the whole batch back");
}

#[test]
fn calls_are_refused_with_a_reason_a_tool_can_read() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let kind = |answer: Answer| answer.unwrap_err().kind;
    assert_eq!(kind(ask(&mut editor, &mut renderer, "obj.fly", json!({}))), ErrorKind::NoSuchCall);
    assert_eq!(kind(ask(&mut editor, &mut renderer, "obj.get", json!({}))), ErrorKind::BadArguments);
    let missing = json!({ "obj": { "scene": "00000000000000000000000000000001", "path": "99" } });
    assert_eq!(kind(ask(&mut editor, &mut renderer, "obj.get", missing)), ErrorKind::NoSuchObj);
    let made = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Point Light" }));
    let light = json!({ "obj": made["obj"], "component": "light", "field": "intensity", "value": "bright" });
    assert_eq!(kind(ask(&mut editor, &mut renderer, "field.set", light)), ErrorKind::WrongValue);
    let unknown = json!({ "obj": made["obj"], "component": "light", "field": "glow", "value": 1 });
    assert_eq!(kind(ask(&mut editor, &mut renderer, "field.set", unknown)), ErrorKind::NoSuchField);
    let level = done(&mut editor, &mut renderer, "editor.state", json!({}))["level"].clone();
    let open = json!({ "level": { "path": level } });
    assert_eq!(kind(ask(&mut editor, &mut renderer, "level.open", open)), ErrorKind::UnsavedConflict);

    // The person may let tools only read; a tool cannot give itself leave back.
    done(&mut editor, &mut renderer, "settings.set", json!({ "key": "tools_may_change", "value": false }));
    assert_eq!(kind(ask(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Cube" }))), ErrorKind::NotAllowed);
    let back = json!({ "key": "tools_may_change", "value": true });
    assert_eq!(kind(ask(&mut editor, &mut renderer, "settings.set", back)), ErrorKind::NotAllowed);
    assert!(done(&mut editor, &mut renderer, "obj.get", json!({ "obj": made["obj"] }))["name"].is_string(), "reads still work");
}

#[test]
fn a_scene_a_tool_writes_is_read_again_unless_edits_wait() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let state = done(&mut editor, &mut renderer, "editor.state", json!({}));
    let scene = state["scenes"][0]["path"].as_str().unwrap().to_string();
    let file = dir.path().join(&scene);
    let named = |editor: &mut Editor, renderer: &mut Renderer, name: &str| {
        !done(editor, renderer, "objs.list", json!({ "name": name }))["objs"].as_array().unwrap().is_empty()
    };
    let rename = |from: &str, to: &str| {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(from), "{scene} has no {from}");
        // The file's time must move on for the change to be seen, on file systems that keep whole seconds.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(&file, text.replacen(from, to, 1)).unwrap();
    };
    let first = done(&mut editor, &mut renderer, "objs.list", json!({}))["objs"][0]["name"].as_str().unwrap().to_string();
    rename(&format!("name = \"{first}\""), "name = \"Written Outside\"");
    done(&mut editor, &mut renderer, "files.look", json!({}));
    assert!(named(&mut editor, &mut renderer, "Written Outside"), "the scene was not read again");

    // With an edit waiting, the file is left until the person or the tool says.
    done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Cube", "name": "Waiting" }));
    rename("name = \"Written Outside\"", "name = \"Written Again\"");
    done(&mut editor, &mut renderer, "files.look", json!({}));
    assert!(!named(&mut editor, &mut renderer, "Written Again"), "edits not saved were lost");
    assert!(named(&mut editor, &mut renderer, "Waiting"));
    let refused = ask(&mut editor, &mut renderer, "scene.reload", json!({}));
    assert_eq!(refused.unwrap_err().kind, ErrorKind::UnsavedConflict);
    done(&mut editor, &mut renderer, "scene.reload", json!({ "discard": true }));
    assert!(named(&mut editor, &mut renderer, "Written Again"));
    assert!(!named(&mut editor, &mut renderer, "Waiting"));

    // The editor's own save is not taken for a change from outside: undo is kept.
    done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Cube", "name": "Mine" }));
    done(&mut editor, &mut renderer, "save", json!({}));
    std::thread::sleep(Duration::from_millis(600));
    done(&mut editor, &mut renderer, "files.look", json!({}));
    assert_eq!(done(&mut editor, &mut renderer, "editor.state", json!({}))["undo"], 1, "the save was read back as a change");
}

#[test]
fn every_component_and_its_fields_are_reached() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    // Hello predates modules, so it has every one, and every component is offered.
    let components = done(&mut editor, &mut renderer, "components", json!({}));
    let built_in: Vec<String> = components["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["script"] == false && c["offered"] == true)
        .map(|c| c["component"].as_str().unwrap().to_string())
        .collect();
    assert!(built_in.len() >= 40, "only {} components are offered", built_in.len());
    let mut problems = Vec::new();
    for component in &built_in {
        let made = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": component }));
        let obj = made["obj"].clone();
        if let Err(error) = ask(&mut editor, &mut renderer, "component.add", json!({ "obj": obj, "component": component })) {
            problems.push(format!("{component}: could not be added: {error}"));
            continue;
        }
        let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": obj }));
        let on_obj = shown["components"].as_array().unwrap().iter().find(|c| {
            c["component"] == component.as_str() || (component == "voxel_terrain" && c["component"] == "block_world")
        });
        let Some(on_obj) = on_obj else {
            problems.push(format!("{component}: added, but obj.get does not show it"));
            continue;
        };
        let id = on_obj["component"].as_str().unwrap().to_string();
        let fields = on_obj["fields"].as_array().unwrap();
        let mut names: Vec<&str> = fields.iter().map(|f| f["field"].as_str().unwrap()).collect();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            problems.push(format!("{component}: two fields share a name: {names:?}"));
        }
        // Each value set back as it is: the field is found, its value read, and taken as written.
        for field in fields {
            let kind = field["type"].as_str().unwrap();
            if !matches!(kind, "number" | "whole" | "toggle" | "text" | "vector" | "color" | "choice" | "curve" | "gradient") {
                continue;
            }
            let set = json!({ "obj": obj, "component": id, "field": field["field"], "value": field["value"] });
            if let Err(error) = ask(&mut editor, &mut renderer, "field.set", set) {
                problems.push(format!("{component}.{}: {error}", field["field"]));
            }
        }
        if let Err(error) = ask(&mut editor, &mut renderer, "component.remove", json!({ "obj": obj, "component": id })) {
            problems.push(format!("{component}: could not be removed: {error}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
