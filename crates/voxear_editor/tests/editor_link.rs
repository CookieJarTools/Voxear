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

/// Every line logged in this test program, which every editor's Console
/// here shows: one subscriber for all, as a subscriber set for one thread
/// can miss lines another test's thread met first.
fn shared_log() -> LogCapture {
    static LOG: std::sync::OnceLock<LogCapture> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        let log = LogCapture::default();
        let _ = tracing::subscriber::set_global_default(voxear_core::logging::capturing_only(log.clone()));
        log
    })
    .clone()
}

/// The editor open on `dir`, a frame in, or `None` without a GPU.
fn open(dir: &Path) -> Option<(Renderer, Editor)> {
    open_logging(dir, shared_log())
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
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
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
            let logged = call(&mut client, "console.read", json!({ "level": "info", "limit": 10000 }));
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
    let made = |line: &&Value| line["message"].as_str().is_some_and(|m| m.contains("[Tools] obj.create") && m.contains("Lamp"));
    assert!(lines.iter().any(|line| made(&line)), "the Console has no line for the Lamp made");
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
    let state = done(&mut editor, &mut renderer, "editor.state", json!({}));
    assert!(state["dotnet_started"].is_boolean(), "a debugger reads whether it can attach yet: {state}");
    let level = state["level"].clone();
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
fn a_tool_reads_the_profilers_frames_with_its_panel_hidden() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    // The first read starts frames being sampled; later reads take those from where the last stopped.
    let first = done(&mut editor, &mut renderer, "profiler.frames", json!({}));
    let mut next = first["next"].as_u64().unwrap();
    let mut taken: Vec<Value> = Vec::new();
    while taken.len() < 3 {
        frames(&mut editor, &mut renderer, 2);
        let read = done(&mut editor, &mut renderer, "profiler.frames", json!({ "after": next }));
        assert!(read["next"].as_u64().unwrap() > next);
        next = read["next"].as_u64().unwrap();
        taken.extend(read["frames"].as_array().unwrap().iter().cloned());
    }
    let numbers: Vec<u64> = taken.iter().map(|f| f["frame"].as_u64().unwrap()).collect();
    assert!(numbers.windows(2).all(|pair| pair[0] < pair[1]), "each frame once, in order: {numbers:?}");
    let frame = taken.last().unwrap();
    let names: Vec<&str> = frame["samples"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Render"), "the editor's own stages are sampled: {names:?}");
    assert!(frame["samples"][0]["depth"] == 0 && frame["cpu_ms"].as_f64().unwrap() > 0.0);
    assert!(!frame["gpu"].as_array().unwrap().is_empty(), "the GPU's passes come with each frame");
    let limited = done(&mut editor, &mut renderer, "profiler.frames", json!({ "limit": 1 }));
    assert_eq!(limited["frames"].as_array().unwrap().len(), 1);
    // The room every mesh shares, with each frame and by a call of its own.
    assert!(frame["mesh_pool"]["model_vertices"].as_u64().unwrap() > 0, "{}", frame["mesh_pool"]);
    let pool = done(&mut editor, &mut renderer, "mesh_pool.get", json!({}));
    let capacity = pool["vertex_capacity"].as_u64().unwrap();
    assert!(pool["free_vertices"].as_u64().unwrap() <= capacity && pool["lighting_share"] == 75, "{pool}");
    // The crowd, its distances and counts, and how parts were posed.
    let crowds = done(&mut editor, &mut renderer, "crowds.get", json!({}));
    assert!(crowds["enabled"] == true && crowds["near"] == 40.0 && crowds["records_beyond"] == 250.0, "{crowds}");
    assert!(crowds["counts"]["records"] == 0 && crowds["posed"]["at_rest"] == 0, "{crowds}");
}

#[test]
fn a_folder_setting_is_set_by_its_path_and_listed_with_it() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let folder = dir.path().join("Models").display().to_string();
    let set = json!({ "key": "fab_downloads", "value": folder });
    assert_eq!(done(&mut editor, &mut renderer, "settings.set", set)["value"], folder.as_str());
    let listed = done(&mut editor, &mut renderer, "settings.get", json!({}));
    let watched = listed["settings"].as_array().unwrap().iter().find(|s| s["key"] == "fab_downloads").unwrap().clone();
    assert_eq!((watched["type"].as_str(), watched["value"].as_str()), (Some("folder"), Some(folder.as_str())));
    let missing = json!({ "key": "fab_downloads", "value": dir.path().join("Nowhere").display().to_string() });
    assert_eq!(ask(&mut editor, &mut renderer, "settings.set", missing).unwrap_err().kind, ErrorKind::WrongValue);
    let cleared = done(&mut editor, &mut renderer, "settings.set", json!({ "key": "fab_downloads", "value": null }));
    assert_eq!(cleared["value"], Value::Null, "null watches nothing");
}

#[test]
fn a_tool_lists_every_setting_with_its_tooltip_type_and_range_and_sets_any_by_key() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    // Project Settings: every category, and each setting where it shows, what it does and what it may be.
    let listed = done(&mut editor, &mut renderer, "project_settings.get", json!({}));
    let categories: Vec<&str> =
        listed["categories"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(categories.contains(&"Physics") && categories.contains(&"Time"), "{categories:?}");
    let settings = listed["settings"].as_array().unwrap();
    assert!(settings.len() > 60, "{} settings", settings.len());
    for setting in settings {
        assert!(setting["tooltip"].as_str().is_some_and(|tip| tip.len() > 20), "{setting}");
        assert!(setting["category"].is_string() && setting["section"].is_string() && setting["label"].is_string());
    }
    let ticks = settings.iter().find(|s| s["key"] == "network.tick_rate").unwrap();
    assert_eq!(
        (
            ticks["type"].as_str(),
            ticks["value"].as_f64(),
            ticks["min"].as_f64(),
            ticks["max"].as_f64(),
            ticks["whole"].as_bool()
        ),
        (Some("number"), Some(60.0), Some(10.0), Some(240.0), Some(true)),
        "{ticks}"
    );
    let mode = settings.iter().find(|s| s["key"] == "audio.reverb_mode").unwrap();
    assert_eq!((mode["value"].as_str(), mode["options"][1].as_str()), (Some("Parametric"), Some("Convolution")));
    // Only a category's, or those a search finds.
    let time = done(&mut editor, &mut renderer, "project_settings.get", json!({ "category": "time" }));
    let keys: Vec<&str> = time["settings"].as_array().unwrap().iter().map(|s| s["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["time.fixed_step", "time.max_step", "time.max_fixed_steps"]);
    let found = done(&mut editor, &mut renderer, "project_settings.get", json!({ "search": "gravity" }));
    assert_eq!(found["settings"].as_array().unwrap().len(), 2);

    // Any of them set by key: a choice by its label or word, a vector, an asset by its path, null for the default.
    let set = |editor: &mut Editor, renderer: &mut Renderer, key: &str, value: Value| {
        done(editor, renderer, "project.set", json!({ "key": key, "value": value }))["value"].clone()
    };
    assert_eq!(set(&mut editor, &mut renderer, "audio.reverb_mode", json!("convolution")), "Convolution");
    assert_eq!(set(&mut editor, &mut renderer, "physics.gravity", json!([0.0, -1.62, 0.0])), json!([0.0, -1.62, 0.0]));
    assert_eq!(set(&mut editor, &mut renderer, "network.authority", json!("Shared")), "Shared");
    assert_eq!(set(&mut editor, &mut renderer, "time.fixed_step", json!(0.01)), json!(0.01));
    assert_eq!(set(&mut editor, &mut renderer, "world.quality", json!("Low")), "Low");
    assert_eq!(
        set(&mut editor, &mut renderer, "localization.languages", json!(["en", "French"])),
        json!(["English", "French"])
    );
    // How plants move, put into effect at once.
    assert_eq!(set(&mut editor, &mut renderer, "foliage.wind", json!(1.5)), json!(1.5));
    assert_eq!(set(&mut editor, &mut renderer, "foliage.interaction", json!(false)), json!(false));
    let motion = editor.game.engine.world.resource::<voxear_render::FoliageMotion>().copied().unwrap();
    assert_eq!((motion.wind, motion.interaction), (1.5, false));
    let file = Project::open(dir.path()).unwrap().file;
    assert_eq!((file.physics.gravity, file.time.fixed_step), ([0.0, -1.62, 0.0], 0.01));
    assert_eq!((file.foliage.wind, file.foliage.bend), (1.5, 1.0));
    assert_eq!(file.localization.languages, ["en", "fr"], "kept by their codes");
    assert_eq!(file.network.authority, voxear_engine::net::NetAuthority::Shared);
    let text = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(text.contains("reverb_mode = \"convolution\"") && text.contains("gravity = [0.0, -1.62, 0.0]"), "{text}");
    assert_eq!(set(&mut editor, &mut renderer, "audio.reverb_mode", Value::Null), "Parametric", "null: the default");
    let text = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(!text.contains("reverb_mode"), "and the file says nothing of it: {text}");
    let wrong = |key: &str, value: Value| json!({ "key": key, "value": value });
    let refused = ask(&mut editor, &mut renderer, "project.set", wrong("network.tick_rate", json!(5000))).unwrap_err();
    assert!(refused.kind == ErrorKind::WrongValue && refused.message.contains("from 10 to 240"), "{}", refused.message);
    let refused =
        ask(&mut editor, &mut renderer, "project.set", wrong("audio.reverb_mode", json!("Echo"))).unwrap_err();
    assert!(refused.message.contains("Parametric, Convolution"), "says what it may be: {}", refused.message);
    assert_eq!(
        ask(&mut editor, &mut renderer, "project.set", wrong("audio.nonsense", json!(1))).unwrap_err().kind,
        ErrorKind::BadArguments
    );

    // Preferences, alike.
    let listed = done(&mut editor, &mut renderer, "settings.get", json!({ "category": "Play Mode" }));
    let play = listed["settings"].as_array().unwrap();
    let players = play.iter().find(|s| s["key"] == "test_players").unwrap();
    assert_eq!((players["section"].as_str(), players["max"].as_f64()), (Some("Networked Play"), Some(7.0)));
    assert!(players["tooltip"].as_str().unwrap().contains("networked Play"));
    let network = done(&mut editor, &mut renderer, "settings.set", json!({ "key": "test_network", "value": "bad" }));
    assert_eq!(network["value"], "Bad");
    assert_eq!(editor.settings().test_network, voxear_editor::TestNetwork::Bad);

    // Shown in its window, scrolled to.
    done(&mut editor, &mut renderer, "settings.show", json!({ "key": "physics.gravity" }));
    frames(&mut editor, &mut renderer, 3);
    let title = editor.ui.find(editor.ui.root(), "project-settings-title").unwrap();
    assert_eq!(editor.ui.text(title), "Physics");
    assert_eq!(
        ask(&mut editor, &mut renderer, "settings.show", json!({ "key": "nothing" })).unwrap_err().kind,
        ErrorKind::BadArguments
    );
}

#[test]
fn the_code_editor_is_whisk_unless_a_tool_picks_another() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let opens = opened.clone();
    editor.set_source_opener(Box::new(move |file, line| opens.borrow_mut().push((file.to_path_buf(), line))));
    let code_editor = |editor: &mut Editor, renderer: &mut Renderer| {
        let listed = done(editor, renderer, "settings.get", json!({}));
        listed["settings"].as_array().unwrap().iter().find(|s| s["key"] == "code_editor").unwrap().clone()
    };
    let shown = code_editor(&mut editor, &mut renderer);
    assert_eq!((shown["type"].as_str(), shown["value"].as_str()), (Some("choice"), Some("Whisk")));
    assert_eq!(shown["options"][0], "Whisk");
    let rider = done(&mut editor, &mut renderer, "settings.set", json!({ "key": "code_editor", "value": "rider" }));
    assert_eq!(rider["value"], "Rider");
    assert_eq!(code_editor(&mut editor, &mut renderer)["value"], "Rider");
    let wrong = json!({ "key": "code_editor", "value": "Notepad" });
    assert_eq!(ask(&mut editor, &mut renderer, "settings.set", wrong).unwrap_err().kind, ErrorKind::WrongValue);

    // The project opens in it too, as a command a tool can run.
    done(&mut editor, &mut renderer, "command.run", json!({ "id": "command:Open Project in Code Editor" }));
    assert_eq!(opened.borrow().as_slice(), [(editor.game.project.root.clone(), 0)]);
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
fn a_box_models_sheet_goes_out_as_a_picture_and_comes_back_painted() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let made = done(&mut editor, &mut renderer, "asset.create", json!({ "kind": "Box Model", "folder": "Mobs" }));
    let model = made["asset"].clone();
    let path = model["path"].as_str().unwrap().to_string();
    let saved = done(&mut editor, &mut renderer, "box_model.sheet_save", json!({ "asset": model }))["picture"].clone();
    let picture = saved["path"].as_str().unwrap().to_string();
    assert_eq!(picture, path.replace(".boxmodel", " Sheet.png"), "beside the model, named after it");
    let sheet = voxear_render::Sheet::from_picture(&std::fs::read(dir.path().join(&picture)).unwrap()).unwrap();
    let starter = voxear_render::BoxModel::starter();
    assert_eq!(sheet, starter.sheet, "the sheet's own pixels");

    // Painted elsewhere, a pixel red, and brought back.
    let mut painted = sheet.clone();
    painted.set(3, 3, [255, 0, 0, 255]);
    std::fs::write(dir.path().join(&picture), painted.to_png().unwrap()).unwrap();
    done(&mut editor, &mut renderer, "files.look", json!({}));
    let loaded = done(&mut editor, &mut renderer, "box_model.sheet_load", json!({ "asset": model, "picture": saved }));
    assert_eq!(loaded["sheet"], json!([painted.width, painted.height]));
    let text = std::fs::read_to_string(dir.path().join(&path)).unwrap();
    assert_eq!(voxear_render::BoxModel::parse(&text).unwrap().sheet, painted, "saved with the picture's pixels");
    let refused = ask(&mut editor, &mut renderer, "box_model.sheet_load", json!({ "asset": saved, "picture": saved }));
    assert!(refused.is_err(), "a picture is no box model");
}

#[test]
fn a_tool_finds_ways_over_a_block_world_and_sends_a_nav_agent() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let world = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Blocks", "position": [0, 0, 0] }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": world, "component": "block_world" }));
    frames(&mut editor, &mut renderer, 5);

    // The ground's top, found from high over it.
    let floor = |editor: &mut Editor, renderer: &mut Renderer, x: f32, z: f32| {
        let found = done(editor, renderer, "navigation.nearest", json!({ "at": [x, 40.0, z], "reach": 60.0 }));
        let place = found["place"].as_array().unwrap_or_else(|| panic!("no place under {x}, {z}: {found}"));
        [place[0].as_f64().unwrap(), place[1].as_f64().unwrap(), place[2].as_f64().unwrap()]
    };
    let (from, to) = (floor(&mut editor, &mut renderer, 2.5, 2.5), floor(&mut editor, &mut renderer, 20.5, 6.5));
    assert_eq!((from[0], from[2]), (2.5, 2.5), "the middle of a block, on its floor");
    let way = done(&mut editor, &mut renderer, "navigation.path", json!({ "from": from, "to": to }));
    assert_eq!(way["status"], "Complete", "{way}");
    let corners = way["corners"].as_array().unwrap();
    assert_eq!(corners.len(), 2, "straight over flat ground: {way}");
    assert!((way["length"].as_f64().unwrap() - 18.44).abs() < 0.05, "{way}");

    let status = done(&mut editor, &mut renderer, "navigation.status", json!({}));
    let kept = &status["worlds"][0];
    assert!(kept["chunks"].as_u64().unwrap() > 0 && kept["places"].as_u64().unwrap() > 0, "{status}");

    // An agent sent somewhere: listed with where it is going.
    let walker = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Walker", "position": from }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": walker, "component": "nav_agent" }));
    done(&mut editor, &mut renderer, "nav_agent.set_destination", json!({ "obj": walker, "to": to }));
    let agents = done(&mut editor, &mut renderer, "navigation.agents", json!({}));
    let listed = agents["agents"].as_array().unwrap().iter().find(|agent| agent["obj"] == walker).expect("the agent");
    assert_eq!(listed["destination"], json!(to), "{agents}");
    assert_eq!(done(&mut editor, &mut renderer, "navigation.status", json!({}))["agents"], 1);

    // The Navigation gizmo: a square on each place's floor round the camera, none with it off.
    done(&mut editor, &mut renderer, "view.set", json!({ "position": [10.0, from[1] + 12.0, -2.0], "look_at": [10.0, from[1], 8.0] }));
    let picture = |editor: &mut Editor, renderer: &mut Renderer, name: &str| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        frames(editor, renderer, 3);
        done(editor, renderer, "capture", json!({ "view": "scene", "path": path.display().to_string() }));
        frames(editor, renderer, 3);
        image::open(&path).unwrap().to_rgba8()
    };
    let off = picture(&mut editor, &mut renderer, "navigation-gizmo-off.png");
    done(&mut editor, &mut renderer, "settings.set", json!({ "key": "nav_gizmos", "value": true }));
    let on = picture(&mut editor, &mut renderer, "navigation-gizmo-on.png");
    let tinted = off.pixels().zip(on.pixels()).filter(|(a, b)| (0..3).any(|c| a.0[c].abs_diff(b.0[c]) > 24)).count();
    assert!(tinted > (on.width() * on.height()) as usize / 4, "the ground under the camera is tinted: {tinted} pixels");
}

#[test]
fn a_tool_bakes_a_levels_nav_mesh_from_its_colliders_and_finds_a_way_over_it() {
    let dir = hello_copy();
    // A level of its own, empty, with no behaviours to play.
    let write = |path: &str, text: &str| std::fs::write(dir.path().join(path), text).unwrap();
    write("Scenes/Yard.scene", "format = 1\n");
    write("Scenes/Yard.scene.meta", "guid = \"2d2c9f3a4b5c6d7e8f9a0b1c2d3e4f5a\"\n");
    write("Levels/Yard.level", "format = 1\n\n[[scene]]\nscene = \"Scenes/Yard.scene\"\n");
    let settings = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&settings).unwrap();
    std::fs::write(&settings, project.replace("Levels/Day.level", "Levels/Yard.level")).unwrap();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    // A floor 40 metres square and a wall across it, its gap at one end.
    for (name, position, size) in [("Floor", [0.0, -0.5, 0.0], [40.0, 1.0, 40.0]), ("Wall", [0.0, 1.5, -3.0], [1.0, 3.0, 34.0])] {
        let made = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": name, "position": position }));
        let obj = made["obj"].clone();
        done(&mut editor, &mut renderer, "component.add", json!({ "obj": obj, "component": "collider" }));
        done(&mut editor, &mut renderer, "field.set", json!({ "obj": obj, "component": "collider", "field": "size", "value": size }));
    }
    // A crate in the gap, a Nav Obstacle: left out of the bake, it shuts the gap once the game runs.
    let made = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Crate", "position": [0.0, 0.75, 17.0] }));
    let crate_obj = made["obj"].clone();
    for (component, size) in [("collider", [1.5, 1.5, 6.0]), ("nav_obstacle", [1.5, 1.5, 6.0])] {
        done(&mut editor, &mut renderer, "component.add", json!({ "obj": crate_obj, "component": component }));
        done(&mut editor, &mut renderer, "field.set", json!({ "obj": crate_obj, "component": component, "field": "size", "value": size }));
    }
    done(&mut editor, &mut renderer, "save", json!({}));
    // An agent type of the project's own, an ogre, is baked for too.
    done(&mut editor, &mut renderer, "project.set", json!({ "key": "navigation.agent_1_name", "value": "Ogre" }));
    done(&mut editor, &mut renderer, "project.set", json!({ "key": "navigation.agent_1_radius", "value": 0.8 }));
    let baked = done(&mut editor, &mut renderer, "navigation.bake", json!({}));
    assert!(baked["polygons"].as_u64().unwrap() > 0, "{baked}");
    let types = baked["agent_types"].as_array().unwrap();
    let names: Vec<&str> = types.iter().map(|kind| kind["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Default", "Ogre"], "{baked}");
    let file = dir.path().join(baked["path"].as_str().unwrap());
    assert!(file.is_file(), "written beside the level: {}", file.display());
    let way = done(&mut editor, &mut renderer, "navigation.path", json!({ "from": [-8.0, 0.0, 5.0], "to": [8.0, 0.0, 5.0] }));
    assert_eq!(way["status"], "Complete", "{way}");
    let corners = way["corners"].as_array().unwrap();
    assert!(corners.len() > 2, "round the wall's end: {way}");
    assert!(corners.iter().any(|corner| corner[2].as_f64().unwrap() > 13.0), "through the gap: {way}");

    // The gizmo shows the mesh: its polygons over the floor, none on the wall's top.
    done(&mut editor, &mut renderer, "view.set", json!({ "position": [-6.0, 14.0, -14.0], "look_at": [0.0, 0.0, 0.0] }));
    let picture = |editor: &mut Editor, renderer: &mut Renderer, name: &str| {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        frames(editor, renderer, 3);
        done(editor, renderer, "capture", json!({ "view": "scene", "path": path.display().to_string() }));
        frames(editor, renderer, 3);
        image::open(&path).unwrap().to_rgba8()
    };
    let off = picture(&mut editor, &mut renderer, "nav-mesh-gizmo-off.png");
    done(&mut editor, &mut renderer, "settings.set", json!({ "key": "nav_gizmos", "value": true }));
    let on = picture(&mut editor, &mut renderer, "nav-mesh-gizmo-on.png");
    let tinted = off.pixels().zip(on.pixels()).filter(|(a, b)| (0..3).any(|c| a.0[c].abs_diff(b.0[c]) > 24)).count();
    assert!(tinted > (on.width() * on.height()) as usize / 10, "the floor is tinted: {tinted} pixels");

    // Playing, the crate carves: the way through the gap is shut.
    done(&mut editor, &mut renderer, "play", json!({}));
    let began = Instant::now();
    let shut = loop {
        frames(&mut editor, &mut renderer, 2);
        let way = done(&mut editor, &mut renderer, "navigation.path", json!({ "from": [-8.0, 0.0, 5.0], "to": [8.0, 0.0, 5.0] }));
        if way["status"] != "Complete" || began.elapsed() > Duration::from_secs(10) {
            break way;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(shut["status"], "Partial", "{shut}");
    let status = done(&mut editor, &mut renderer, "navigation.status", json!({}));
    assert_eq!((status["obstacles"].clone(), status["carving"].clone()), (json!(1), json!(1)), "{status}");
    editor.stop(&mut renderer);
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

/// A behaviour whose fields point at an Obj and at an Obj's camera, as a
/// script's description has them.
const FOLLOWER: &str = r#"{"behaviours": [{"name": "Hello.Follower", "bases": [], "interfaces": [], "fields": [
  {"name": "Target", "type": {"kind": "obj"}, "default": null},
  {"name": "Eye", "type": {"kind": "component", "name": "Voxear.Camera"}, "default": null}
]}], "components": [], "types": {}, "commands": []}"#;

#[test]
fn a_scripts_obj_and_camera_fields_take_their_targets_and_read_them_back() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    editor.set_script_types(voxear_engine::script_types::parse(FOLLOWER).unwrap());
    let follower = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Follower" }))["obj"].clone();
    let camera = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Camera", "name": "Eye" }))["obj"].clone();
    let ball = done(&mut editor, &mut renderer, "objs.list", json!({ "name": "Chrome Ball" }))["objs"][0]["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": follower, "component": "Hello.Follower" }));

    // The camera field offers the Objs with a camera.
    let eye = json!({ "obj": follower, "component": "Hello.Follower", "field": "Eye" });
    let choices = done(&mut editor, &mut renderer, "field.choices", eye.clone());
    let offered: Vec<&Value> = choices["choices"].as_array().unwrap().iter().map(|c| &c["value"]).collect();
    assert!(offered.contains(&&camera), "the camera field offers no camera: {choices}");

    for (field, target) in [("Eye", &camera), ("Target", &ball)] {
        let set = json!({ "obj": follower, "component": "Hello.Follower", "field": field, "value": target });
        done(&mut editor, &mut renderer, "field.set", set);
    }
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": follower }));
    assert_eq!(field(&shown, "Hello.Follower", "Eye")["value"], camera, "{shown}");
    assert_eq!(field(&shown, "Hello.Follower", "Target")["value"], ball, "{shown}");
}

#[test]
fn a_maps_open_action_is_set_from_its_menu() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let map = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Map" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": map, "component": "map" }));
    let action = json!({ "obj": map, "component": "map", "field": "open_action" });
    let choices = done(&mut editor, &mut renderer, "field.choices", action.clone());
    let picked = choices["choices"].as_array().unwrap().iter().find(|c| !c["value"].is_null()).cloned();
    let Some(picked) = picked else { return eprintln!("skipping: Hello has no input actions to open a map with") };
    let mut set = action.clone();
    set["value"] = picked["value"].clone();
    done(&mut editor, &mut renderer, "field.set", set);
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": map }));
    assert_eq!(field(&shown, "map", "open_action")["value"], picked["value"], "{shown}");
    // By its label, as the menu shows it, too.
    let mut by_label = action;
    by_label["value"] = json!(null);
    done(&mut editor, &mut renderer, "field.set", by_label.clone());
    by_label["value"] = picked["label"].clone();
    assert_eq!(done(&mut editor, &mut renderer, "field.set", by_label)["changed"], true);
}

#[test]
fn a_block_world_takes_blocks_from_a_tool_and_its_file_from_disk() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let block = done(&mut editor, &mut renderer, "asset.create", json!({ "kind": "Block", "folder": "Blocks" }))["asset"].clone();
    let world = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Blocks", "position": [0, 0, 0] }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": world, "component": "block_world" }));
    frames(&mut editor, &mut renderer, 5);

    // Its Settings tab's fields and its Build tab's alike, whichever the Inspector shows.
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": world }));
    field(&shown, "block_world", "block_size");
    field(&shown, "block_world", "brush");

    let high = [0.5, 40.5, 0.5];
    let placed = done(&mut editor, &mut renderer, "blocks.set", json!({ "obj": world, "at": high, "block": block }));
    assert_eq!(placed["changed"], 1, "{placed}");
    let found = done(&mut editor, &mut renderer, "blocks.get", json!({ "obj": world, "at": high }));
    assert_eq!(found["block"]["path"], block["path"], "{found}");
    done(&mut editor, &mut renderer, "undo", json!({}));
    let gone = done(&mut editor, &mut renderer, "blocks.get", json!({ "obj": world, "at": high }));
    assert!(gone["block"].is_null(), "undo left the block: {gone}");
    let filled = done(
        &mut editor,
        &mut renderer,
        "blocks.fill",
        json!({ "obj": world, "from": [0.5, 40.5, 0.5], "to": [2.5, 41.5, 2.5], "block": block }),
    );
    assert_eq!(filled["changed"], 18, "{filled}");

    // Its file written by a tool is read again.
    done(&mut editor, &mut renderer, "save", json!({}));
    let data = field(&done(&mut editor, &mut renderer, "obj.get", json!({ "obj": world })), "block_world", "data")["value"]
        ["path"]
        .as_str()
        .unwrap()
        .to_string();
    let file = dir.path().join(&data);
    let mut table: toml::Table = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    table.insert("block_size".into(), toml::Value::Float(0.5));
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&file, toml::to_string(&table).unwrap()).unwrap();
    done(&mut editor, &mut renderer, "files.look", json!({}));
    frames(&mut editor, &mut renderer, 3);
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": world }));
    assert_eq!(field(&shown, "block_world", "block_size")["value"], 0.5, "{shown}");
}

#[test]
fn the_scene_view_turns_to_look_at_a_point() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let set = json!({ "position": [0.0, 10.0, -10.0], "look_at": [0.0, 0.0, 0.0] });
    done(&mut editor, &mut renderer, "view.set", set);
    frames(&mut editor, &mut renderer, 2);
    let view = done(&mut editor, &mut renderer, "view.get", json!({}));
    let forward: Vec<f64> = view["forward"].as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect();
    let wanted = [0.0, -std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2];
    assert!(forward.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-3), "looks along {forward:?}");
    assert_eq!(view["position"], json!([0.0, 10.0, -10.0]));
}

#[test]
fn an_editor_takes_the_link_once_the_editor_holding_it_closes() {
    let dir = hello_copy();
    let Some((mut renderer, mut first)) = open(dir.path()) else { return };
    first.serve_link(false).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut second = Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    assert!(second.serve_link(false).is_err(), "two editors had the link");
    frames(&mut second, &mut renderer, 3);
    assert!(!second.has_link());
    drop(first);
    let began = Instant::now();
    while !second.has_link() {
        assert!(began.elapsed() < Duration::from_secs(10), "the second editor never took the link");
        frames(&mut second, &mut renderer, 1);
        std::thread::sleep(Duration::from_millis(50));
    }
    let folder = link::link_folder(dir.path());
    let talking = std::thread::spawn(move || LinkClient::connect(&folder, link::LINK_FILE).map(|client| client.about.pid));
    while !talking.is_finished() {
        frames(&mut second, &mut renderer, 1);
    }
    assert_eq!(talking.join().unwrap().unwrap(), std::process::id());
}

/// The only test here that starts .NET, which starts once per process: a
/// `[Command]` run on the edited level has the game's services, so it puts
/// a block by its file into an open Block World, one step to undo; and
/// Add Component offers the engine's own behaviours, as Block User.
#[test]
fn a_command_on_the_edited_level_puts_blocks_and_the_engines_behaviours_are_offered() {
    if voxear_scripting::find_dotnet().is_none() {
        return eprintln!("skipping: needs .NET");
    }
    let dir = hello_copy();
    // With its scripts, this once.
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello/voxear.toml");
    std::fs::copy(hello, dir.path().join("voxear.toml")).unwrap();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let block = done(&mut editor, &mut renderer, "asset.create", json!({ "kind": "Block", "folder": "Blocks" }))["asset"].clone();
    let world = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Blocks" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": world, "component": "block_world" }));
    let tool = format!(
        "using System.Numerics;\nusing Voxear;\n\nnamespace Hello;\n\ninternal static class BlockTools\n{{\n\
         \t[Command(\"Place Test Block\")]\n\tprivate static void PlaceTestBlock()\n\t{{\n\
         \t\tBlocks.Set(new Vector3(0.5f, 40.5f, 0.5f), Block.Load(\"{}\"));\n\t}}\n}}\n",
        block["path"].as_str().unwrap()
    );
    std::fs::write(dir.path().join("Scripts/BlockTools.cs"), tool).unwrap();
    let built = done(&mut editor, &mut renderer, "scripts.build", json!({}));
    assert_eq!(built["ok"], true, "{built}");

    let components = done(&mut editor, &mut renderer, "components", json!({}));
    let offered: Vec<&str> = components["components"].as_array().unwrap().iter().filter_map(|c| c["component"].as_str()).collect();
    assert!(offered.contains(&"Voxear.BlockUser"), "{offered:?}");
    let user = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Player" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": user, "component": "Voxear.BlockUser" }));

    let commands = done(&mut editor, &mut renderer, "commands", json!({}));
    let id = commands["commands"].as_array().unwrap().iter().find(|c| c["title"] == "Place Test Block").unwrap()["id"].clone();
    let undo = done(&mut editor, &mut renderer, "editor.state", json!({}))["undo"].as_u64().unwrap();
    done(&mut editor, &mut renderer, "command.run", json!({ "id": id }));
    let at = json!({ "obj": world, "at": [0.5, 40.5, 0.5] });
    let began = Instant::now();
    while done(&mut editor, &mut renderer, "blocks.get", at.clone())["block"].is_null() {
        assert!(began.elapsed() < Duration::from_secs(300), "the command put no block: {:?}", editor.play_error());
        frames(&mut editor, &mut renderer, 1);
    }
    assert_eq!(done(&mut editor, &mut renderer, "blocks.get", at.clone())["block"]["path"], block["path"]);
    assert_eq!(done(&mut editor, &mut renderer, "editor.state", json!({}))["undo"].as_u64().unwrap(), undo + 1);
    done(&mut editor, &mut renderer, "undo", json!({}));
    assert!(done(&mut editor, &mut renderer, "blocks.get", at)["block"].is_null(), "one undo takes the block back");
}

#[test]
fn a_tool_adds_changes_and_takes_out_a_graphs_comments_and_a_world_without_land_is_told_so() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let made = done(&mut editor, &mut renderer, "asset.create", json!({ "kind": "World Generator" }));
    let asset = made["asset"].clone();
    let path = asset["path"].as_str().unwrap().to_string();
    let file = dir.path().join(&path);
    let comments = |editor: &mut Editor, renderer: &mut Renderer| {
        done(editor, renderer, "graph.comments", json!({ "asset": asset }))["comments"].as_array().unwrap().clone()
    };
    assert!(comments(&mut editor, &mut renderer).is_empty());

    // Not open in its panel: written into its file. Round two of its nodes, it frames them.
    let added = done(
        &mut editor,
        &mut renderer,
        "graph.comment.add",
        json!({ "asset": asset, "title": "Hills", "text": "Two noises", "around": [1, 2] }),
    );
    assert_eq!(added["comment"], 1);
    let listed = comments(&mut editor, &mut renderer);
    assert_eq!((listed[0]["title"].as_str(), listed[0]["text"].as_str()), (Some("Hills"), Some("Two noises")));
    let size = listed[0]["size"].as_array().unwrap();
    assert!(size[0].as_f64().unwrap() >= 160.0, "{listed:?}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[[comments]]") && text.contains("Hills"), "{text}");

    let set = json!({ "asset": asset, "comment": 1, "title": "Rolling hills", "at": [-500, -400], "size": [400, 300] });
    assert_eq!(done(&mut editor, &mut renderer, "graph.comment.set", set)["changed"], true);
    let listed = comments(&mut editor, &mut renderer);
    assert_eq!(listed[0]["at"], json!([-500.0, -400.0]));
    assert_eq!(listed[0]["title"], "Rolling hills");
    let missing = ask(&mut editor, &mut renderer, "graph.comment.remove", json!({ "asset": asset, "comment": 7 }));
    assert!(missing.is_err(), "{missing:?}");
    let not_a_graph = ask(&mut editor, &mut renderer, "graph.comments", json!({ "asset": "voxear.toml" }));
    assert!(not_a_graph.is_err());

    // Open in its panel: changed there, a step to undo, and saved as the panel saves.
    assert!(editor.open_generator(&path));
    frames(&mut editor, &mut renderer, 2);
    done(&mut editor, &mut renderer, "graph.comment.add", json!({ "asset": asset, "title": "Seas" }));
    frames(&mut editor, &mut renderer, 2);
    let drawn = editor.generator().drawn().unwrap().comments.clone();
    assert_eq!(drawn.iter().map(|comment| comment.title.as_str()).collect::<Vec<_>>(), ["Rolling hills", "Seas"]);
    assert!(std::fs::read_to_string(&file).unwrap().contains("Seas"), "saved");
    done(&mut editor, &mut renderer, "undo", json!({}));
    frames(&mut editor, &mut renderer, 2);
    assert_eq!(comments(&mut editor, &mut renderer).len(), 1);
    done(&mut editor, &mut renderer, "graph.comment.remove", json!({ "asset": asset, "comment": 1 }));
    frames(&mut editor, &mut renderer, 2);
    assert!(comments(&mut editor, &mut renderer).is_empty());
    assert!(!std::fs::read_to_string(&file).unwrap().contains("[[comments]]"));

    // A Block World with no land laid in: nothing to write, and why.
    let world =
        done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Blocks" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": world, "component": "block_world" }));
    let refused = ask(&mut editor, &mut renderer, "block_world.write_land", json!({ "obj": world }));
    let error = refused.expect_err("nothing to write");
    assert!(error.to_string().contains("not written"), "{error}");
}

#[test]
fn a_tool_reads_and_sets_the_world_quality_by_preset_number_and_for_this_machine() {
    use voxear_terrain::{MachineBudget, WorldBudget, WorldQuality};
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let now = done(&mut editor, &mut renderer, "world_quality.get", json!({}));
    assert_eq!((now["quality"].as_str(), now["machine"].as_bool()), (Some("high"), Some(false)), "{now}");
    // A preset, then a detail after it: Custom, written into voxear.toml and taken up at once.
    let set = done(&mut editor, &mut renderer, "world_quality.set", json!({ "quality": "ultra", "plant_detail": 2.0 }));
    assert_eq!(
        (set["quality"].as_str(), set["near_detail"].as_f64(), set["plant_detail"].as_f64()),
        (Some("custom"), Some(1.5), Some(2.0))
    );
    let ultra = WorldQuality::Ultra.budget().unwrap();
    let custom = WorldBudget { quality: WorldQuality::Custom, plant_detail: 2.0, ..ultra };
    assert_eq!(WorldBudget::of(&editor.game.engine.world), custom);
    assert_eq!(Project::open(dir.path()).unwrap().file.world, custom, "voxear.toml has it");
    // For this machine: its own beside the saves, the project's left as it is.
    let saves = tempfile::tempdir().unwrap();
    let file = saves.path().join(voxear_terrain::budget::MACHINE_FILE);
    MachineBudget::open(&mut editor.game.engine.world, custom, Some(file.clone()));
    let low = done(&mut editor, &mut renderer, "world_quality.set", json!({ "machine": true, "quality": "low" }));
    assert_eq!(
        (low["quality"].as_str(), low["machine"].as_bool(), low["project"]["quality"].as_str()),
        (Some("low"), Some(true), Some("custom"))
    );
    assert!(std::fs::read_to_string(&file).unwrap().contains("quality = \"low\""));
    assert_eq!(Project::open(dir.path()).unwrap().file.world, custom, "the project's as it was");
    // Forgotten: the project's back.
    let back = done(&mut editor, &mut renderer, "world_quality.set", json!({ "machine": false }));
    assert_eq!((back["quality"].as_str(), back["machine"].as_bool()), (Some("custom"), Some(false)));
    assert!(!file.exists());
    let kind = |answer: Answer| answer.unwrap_err().kind;
    assert_eq!(
        kind(ask(&mut editor, &mut renderer, "world_quality.set", json!({ "quality": "epic" }))),
        ErrorKind::BadArguments
    );
}

#[test]
fn a_portals_view_distance_is_the_projects_or_its_own_and_a_tool_sets_both() {
    use voxear_render::portals::PortalQuality;
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let gate =
        done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Empty", "name": "Gate" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": gate, "component": "portal" }));
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": gate }));
    assert_eq!(field(&shown, "portal", "view_distance")["value"], "Project", "{shown}");

    // Its own starts at the project's, and is then its own.
    let own = json!({ "obj": gate, "component": "portal", "field": "view_distance", "value": "Own" });
    done(&mut editor, &mut renderer, "field.set", own);
    let shown = done(&mut editor, &mut renderer, "obj.get", json!({ "obj": gate }));
    assert_eq!(field(&shown, "portal", "view_distance#2")["value"], 96.0, "{shown}");
    let set = json!({ "obj": gate, "component": "portal", "field": "view_distance#2", "value": 200.0 });
    done(&mut editor, &mut renderer, "field.set", set);
    let entity = link::resolve_obj(&editor.game, &gate).unwrap();
    let portal = *editor.game.engine.world.get::<voxear_render::Portal>(entity).unwrap();
    assert_eq!(portal.view_distance, Some(200.0));

    // The project's, written to voxear.toml and in effect at once.
    let changed =
        done(&mut editor, &mut renderer, "project.set", json!({ "key": "portals.view_distance", "value": 64.0 }));
    assert_eq!(changed["value"], 64.0);
    let file = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(file.contains("[portals]") && file.contains("view_distance = 64"), "{file}");
    let quality = editor.game.engine.world.resource::<PortalQuality>().copied().unwrap();
    assert_eq!(quality.view_distance, 64.0);
    assert_eq!(Project::open(dir.path()).unwrap().file.portals.view_distance, 64.0, "read back from the file");
    let wrong = json!({ "key": "portals.view_distance", "value": -3.0 });
    assert_eq!(ask(&mut editor, &mut renderer, "project.set", wrong).unwrap_err().kind, ErrorKind::WrongValue);
}

#[test]
fn a_tool_puts_an_obj_inside_another_while_editing_and_while_playing() {
    let dir = hello_copy();
    // A level of its own, of a deck with a boat and a rider and a shore beside it, with no behaviours to play.
    let (deck, shore) = ("0b0a7d1e2f3a4b5c6d7e8f9a0b1c2d3e", "1c1b8e2f3a4b5c6d7e8f9a0b1c2d3e4f");
    let write = |path: &str, text: &str| std::fs::write(dir.path().join(path), text).unwrap();
    write(
        "Scenes/Deck.scene",
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Boat\"\nposition = [2.0, 0.5, 1.0]\nrotation = [0.0, 40.0, 0.0]\n\n[[obj]]\nid = 2\nname = \"Rider\"\nposition = [0.0, 1.0, 0.0]\n",
    );
    write("Scenes/Deck.scene.meta", &format!("guid = \"{deck}\"\n"));
    write("Scenes/Shore.scene", "format = 1\n\n[[obj]]\nid = 1\nname = \"Dock\"\n");
    write("Scenes/Shore.scene.meta", &format!("guid = \"{shore}\"\n"));
    write(
        "Levels/Harbour.level",
        "format = 1\n\n[[scene]]\nscene = \"Scenes/Shore.scene\"\n\n[[scene]]\nscene = \"Scenes/Deck.scene\"\n",
    );
    let settings = dir.path().join("voxear.toml");
    let project = std::fs::read_to_string(&settings).unwrap();
    std::fs::write(&settings, project.replace("Levels/Day.level", "Levels/Harbour.level")).unwrap();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let (rider, boat) = (json!({ "scene": deck, "path": "2" }), json!({ "scene": deck, "path": "1" }));
    let dock = json!({ "scene": shore, "path": "1" });
    let entity = |editor: &Editor, obj: &Value| link::resolve_obj(&editor.game, obj).unwrap();
    let placed =
        |editor: &Editor, obj: &Value| voxear_scene::world_matrix(&editor.game.engine.world, entity(editor, obj));
    let parent = |editor: &Editor, obj: &Value| {
        editor.game.engine.world.get::<voxear_scene::Parent>(entity(editor, obj)).map(|parent| parent.0)
    };

    // While editing, as the Hierarchy moves it, one step to undo.
    let was = placed(&editor, &rider);
    let moved = done(&mut editor, &mut renderer, "obj.parent", json!({ "obj": rider, "parent": boat }));
    assert_eq!(moved["changed"], true);
    assert_eq!(parent(&editor, &rider), Some(entity(&editor, &boat)));
    assert!(placed(&editor, &rider).abs_diff_eq(was, 1e-4), "kept where it is in the world");
    let refused = ask(&mut editor, &mut renderer, "obj.parent", json!({ "obj": boat, "parent": rider }));
    assert!(refused.is_err_and(|error| error.message.contains("inside itself")), "a loop is refused");
    done(&mut editor, &mut renderer, "undo", json!({}));
    assert_eq!(parent(&editor, &rider), None, "undone");

    // While playing, as a script's Obj.SetParent: into another scene is refused, aboard is not.
    done(&mut editor, &mut renderer, "play", json!({}));
    let refused = ask(&mut editor, &mut renderer, "obj.parent", json!({ "obj": rider, "parent": dock }));
    assert!(refused.is_err_and(|error| error.message.contains("stays in its scene")), "into another scene");
    let moved =
        done(&mut editor, &mut renderer, "obj.parent", json!({ "obj": rider, "parent": boat, "keep_world": true }));
    assert_eq!(moved["changed"], true);
    assert_eq!(parent(&editor, &rider), Some(entity(&editor, &boat)));
    assert!(editor.game.engine.world.has::<voxear_engine::Reparented>(entity(&editor, &rider)), "for a save");
    let off = done(&mut editor, &mut renderer, "obj.parent", json!({ "obj": rider }));
    assert_eq!((off["changed"].clone(), parent(&editor, &rider)), (json!(true), None), "back at the top");
    editor.stop(&mut renderer);
}

#[test]
fn a_dimension_opens_beside_the_level_in_a_view_of_its_own_and_saves_with_its_settings() {
    let dir = hello_copy();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let state = done(&mut editor, &mut renderer, "editor.state", json!({}));
    let scenes: Vec<String> =
        state["scenes"].as_array().unwrap().iter().map(|scene| scene["path"].as_str().unwrap().to_string()).collect();
    let level =
        || scenes.iter().map(|scene| std::fs::read_to_string(dir.path().join(scene)).unwrap()).collect::<String>();
    let made = done(&mut editor, &mut renderer, "dimension.create", json!({ "kind": "caves" }));
    let file = made["asset"]["path"].as_str().unwrap().to_string();
    assert_eq!(file, "Dimensions/New Dimension.dimension");
    let id =
        done(&mut editor, &mut renderer, "dimension.make", json!({ "dimension": file }))["dimension"].as_u64().unwrap();
    assert!(id > 0, "a dimension of its own");
    frames(&mut editor, &mut renderer, 2);
    let listed = done(&mut editor, &mut renderer, "dimension.list", json!({}));
    assert_eq!(listed["shown"], id, "the Scene view shows the dimension made");
    let dimensions = listed["dimensions"].as_array().unwrap();
    assert_eq!(dimensions.len(), 2, "{listed}");
    let caves = &dimensions[1];
    assert_eq!(
        (caves["kind"].as_str(), caves["sky"].as_bool(), caves["saved"].as_bool()),
        (Some("caves"), Some(false), Some(true))
    );
    assert_eq!(caves["objs"][0]["name"], "Way Back", "its file's Portal");
    assert!(
        dimensions[0]["objs"].as_array().unwrap().iter().all(|obj| obj["name"] != "Way Back"),
        "not in the level's"
    );

    // A new Obj made while the Scene view shows it goes into its file, which keeps its settings as it saves.
    done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Cube", "name": "Stalagmite" }));
    frames(&mut editor, &mut renderer, 1);
    done(&mut editor, &mut renderer, "save", json!({}));
    let text = std::fs::read_to_string(dir.path().join(&file)).unwrap();
    assert!(text.contains("Stalagmite") && text.contains("[dimension]") && text.contains("kind = \"caves\""), "{text}");
    let saved = level();
    assert!(
        !saved.contains("Stalagmite") && !saved.contains("Way Back"),
        "the level's scenes took the dimension's Objs"
    );

    // Back in the level's, a Portal there leads into the dimension's Way Back: picked, never a name in a string.
    assert_eq!(done(&mut editor, &mut renderer, "dimension.show", json!({ "dimension": 0 }))["shown"], 0);
    let gate = done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Cube", "name": "Gate" }))["obj"].clone();
    done(&mut editor, &mut renderer, "component.add", json!({ "obj": gate, "component": "portal" }));
    let choices =
        done(&mut editor, &mut renderer, "field.choices", json!({ "obj": gate, "component": "portal", "field": "to" }));
    assert!(choices.to_string().contains("Way Back · New Dimension"), "{choices}");
    let into = json!({ "dimension": file, "obj": 1 });
    let set = done(
        &mut editor,
        &mut renderer,
        "field.set",
        json!({ "obj": gate, "component": "portal", "field": "to", "value": into }),
    );
    assert_eq!(set["value"], json!({ "dimension": file, "obj": "1" }), "a tool reads it back as it gave it");
    done(&mut editor, &mut renderer, "save", json!({}));
    let saved = level();
    assert!(saved.contains("into") && saved.contains("Dimensions/New Dimension.dimension"), "{saved}");

    // Going through shows the far Portal in its dimension; the Scene view kept its place in each.
    let gate = link::resolve_obj(&editor.game, &gate).unwrap();
    let far = editor.go_through(gate, &mut renderer).unwrap();
    assert_eq!(editor.scene_dimension().0 as u64, id);
    assert_eq!(editor.selected(), Some(far));
    // Selecting an Obj of the level's shows the level's again.
    let gate = link::obj_ref(&editor.game, gate);
    done(&mut editor, &mut renderer, "select", json!({ "objs": [gate] }));
    assert_eq!(editor.scene_dimension().0, 0);

    // An Obj moved into it goes into its file; letting it go with edits waiting needs `discard`.
    let lamp =
        done(&mut editor, &mut renderer, "obj.create", json!({ "kind": "Point Light", "name": "Lantern" }))["obj"]
            .clone();
    done(
        &mut editor,
        &mut renderer,
        "obj.move_to_dimension",
        json!({ "obj": lamp, "dimension": id, "at": [0.0, 2.0, 3.0] }),
    );
    let listed = done(&mut editor, &mut renderer, "dimension.list", json!({}));
    assert!(listed["dimensions"][1]["objs"].to_string().contains("Lantern"), "{listed}");
    let refused = ask(&mut editor, &mut renderer, "dimension.let_go", json!({ "dimension": id }));
    assert_eq!(refused.unwrap_err().kind, ErrorKind::UnsavedConflict);
    done(&mut editor, &mut renderer, "dimension.let_go", json!({ "dimension": id, "discard": true }));
    frames(&mut editor, &mut renderer, 1);
    let listed = done(&mut editor, &mut renderer, "dimension.list", json!({}));
    assert_eq!((listed["dimensions"].as_array().unwrap().len(), listed["shown"].as_u64()), (1, Some(0)), "{listed}");
}

/// Keeps the window's picture as `name` where `VOXEAR_TEST_OUTPUT` says.
fn shot(renderer: &mut Renderer, name: &str) {
    let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    image::RgbaImage::from_raw(width, height, pixels).unwrap().save(Path::new(&dir).join(name)).unwrap();
}

/// A level of rolling hills without end with a small dungeon every 240 m or
/// so, each in a dimension of its own.
fn dungeons_project() -> tempfile::TempDir {
    use voxear_terrain::{
        blocks::{
            file::BlockFile,
            generate::{BlockEndless, BlockGenerate},
        },
        generate::Preset,
    };
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -60,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        structures: vec!["Structures/Dungeon.structure".into()],
        ..BlockGenerate::default()
    });
    let structure = "ground = \"leave\"\n\n[placement]\nspacing = 240.0\nseparation = 60.0\nchance = 1.0\non = \"land\"\n\
                     height = [0.0, 500.0]\nslope = 60.0\n\n[dungeon]\nsize = \"small\"\nfloors = [2, 2]\n\n\
                     [dungeon.blocks]\nstone = \"Blocks/Temple Stone.block\"\nlight = \"Blocks/Fire.block\"\n";
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Hills\"\n[obj.block_world]\ndata = \"Blocks/Hills.blockworld\"\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Dungeons\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Hills.scene\"\n".into()),
        ("Scenes/Hills.scene", scene.into()),
        ("Blocks/Hills.blockworld", file.to_text()),
        ("Structures/Dungeon.structure", structure.into()),
        ("Blocks/Temple Stone.block", "color = [0.62, 0.58, 0.5]\n".into()),
        ("Blocks/Fire.block", "color = [1.0, 0.62, 0.25]\nlight = 15\n".into()),
        ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_dungeons_gate_opens_its_dimension_in_the_editor_and_its_way_out_goes_back() {
    let dir = dungeons_project();
    let Some((mut renderer, mut editor)) = open(dir.path()) else { return };
    let found = done(&mut editor, &mut renderer, "dungeon.layouts", json!({ "near": [0.0, 0.0, 0.0], "reach": 800.0 }));
    let first = &found["dungeons"][0];
    assert_eq!(
        (first["place"].as_str(), first["dimension"].is_null(), first["stage"].as_u64()),
        (Some("own_dimension"), true, Some(0))
    );
    let entrance: Vec<f32> =
        first["entrance"]["at"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();

    // Looked at from over its entrance, its gate stands in the editor too, with Open Dungeon.
    let camera = editor.scene_view_cameras()[0].1;
    let over = voxear_core::math::Vec3::new(entrance[0], entrance[1] + 12.0, entrance[2] - 16.0);
    editor.game.engine.world.get_mut::<voxear_scene::Transform>(camera).unwrap().position = over;
    let by = voxear_core::math::Vec3::new(entrance[0], entrance[1], entrance[2]);
    let gate = (0..240).find_map(|_| {
        frames(&mut editor, &mut renderer, 1);
        let world = &editor.game.engine.world;
        let gates =
            world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Transform, &voxear_engine::dungeons::DungeonGate)>();
        let near = gates.into_iter().filter(|(_, placed, _)| placed.position.distance(by) < 40.0);
        near.map(|(entity, ..)| entity).next()
    });
    let gate = gate.expect("a Portal stands in the dungeon's gate");
    let gate_ref = link::obj_ref(&editor.game, gate);
    let opened = done(&mut editor, &mut renderer, "dungeon.open", json!({ "obj": gate_ref }));
    let id = opened["dimension"].as_u64().unwrap();
    assert_eq!(editor.scene_dimension().0 as u64, id, "the Scene view shows the dungeon");
    frames(&mut editor, &mut renderer, 2);
    let listed = done(&mut editor, &mut renderer, "dimension.list", json!({}));
    let dungeon = &listed["dimensions"][1];
    assert_eq!((dungeon["kind"].as_str(), dungeon["saved"].as_bool()), (Some("dungeon"), Some(false)), "{listed}");
    frames(&mut editor, &mut renderer, 90);
    shot(&mut renderer, "dimension-dungeon.png");
    let names = dungeon["objs"].to_string();
    assert!(names.contains("Way Out") && names.contains("Dungeon"), "{names}");
    let found = done(&mut editor, &mut renderer, "dungeon.layouts", json!({ "near": entrance, "reach": 64.0 }));
    assert!(found["dungeons"].as_array().unwrap().iter().any(|d| d["dimension"].as_u64() == Some(id)), "{found}");

    // Its way out leads back to the gate, in the level's.
    let way_out = link::resolve_obj(&editor.game, &opened["way_out"]).unwrap();
    assert_eq!(editor.go_through(way_out, &mut renderer).unwrap(), gate);
    assert_eq!(editor.scene_dimension().0, 0);
    frames(&mut editor, &mut renderer, 90);
    shot(&mut renderer, "dimension-gate.png");
    // Saving leaves the dungeon's Objs out of the level's file.
    done(&mut editor, &mut renderer, "save", json!({}));
    let level = std::fs::read_to_string(dir.path().join("Scenes/Hills.scene")).unwrap();
    assert!(!level.contains("Way Out") && !level.contains("Dungeon"), "{level}");
    done(&mut editor, &mut renderer, "dimension.let_go", json!({ "dimension": id }));
    let listed = done(&mut editor, &mut renderer, "dimension.list", json!({}));
    assert_eq!(listed["dimensions"].as_array().unwrap().len(), 1, "{listed}");
}
