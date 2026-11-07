//! Networked Play from the editor, on a copy of Hello: Play as a host with
//! two test players, each a game of its own that joins the editor's
//! session, and Stop closing them. The test players run without windows
//! here. A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET, a GPU or `vox`.

use voxear_core::logging::LogCapture;
use voxear_editor::{PlayAs, TestNetwork};
use voxear_engine::{Project, net::NetSession};
use voxear_render::{Renderer, RendererSettings};

const SIZE: (u32, u32) = (1280, 800);

#[test]
fn play_hosts_and_test_players_join_then_stop_closes_them() {
    let vox = voxear_engine::player_build::sibling_program("vox");
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() || !vox.is_file() {
        return eprintln!("skipping: needs .NET, the managed build and vox");
    }
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    // SAFETY: set before anything starts a thread that reads the environment.
    unsafe { std::env::set_var("VOXEAR_HEADLESS_TEST_PLAYERS", "1") };
    let log = LogCapture::default();
    // For every thread, as the test players' log lines arrive on threads of their own.
    let _ = tracing::subscriber::set_global_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, log.clone()).unwrap();
    let frame = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer| {
        editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(16));
    };
    let until = |editor: &mut voxear_editor::Editor, renderer: &mut Renderer, what: &str, done: &dyn Fn(&voxear_editor::Editor) -> bool| {
        let started = std::time::Instant::now();
        while !done(editor) {
            assert!(started.elapsed().as_secs() < 120, "{what} never happened");
            frame(editor, renderer);
        }
    };
    until(&mut editor, &mut renderer, "the scripts described", &|e| e.scripts_described());

    editor.set_networked_play(PlayAs::Host, 2, TestNetwork::Good);
    editor.play(&mut renderer);
    let players = |e: &voxear_editor::Editor| e.game.engine.world.resource::<NetSession>().map_or(0, |s| s.players().len());
    until(&mut editor, &mut renderer, "both test players joining", &|e| e.is_playing() && players(e) == 3);
    let lines = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect::<Vec<_>>();
    assert!(lines.iter().any(|l| l.starts_with("Player 2: ")), "a test player's log in the Console: {lines:?}");

    editor.stop(&mut renderer);
    frame(&mut editor, &mut renderer);
    assert!(editor.game.engine.world.resource::<NetSession>().is_none(), "no session while editing");
}

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
