//! `vox build` on a copy of the Hello sample, end to end: the scripts, the
//! content and the player, which then runs the game.

use std::{path::Path, process::Command};

use serde_json::Value;
use voxear_engine::build_profile;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
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
    dir
}

#[test]
fn a_profile_builds_a_player_that_runs_the_game() {
    if voxear_scripting::find_dotnet().is_none() {
        return eprintln!("skipping: needs .NET");
    }
    let dir = hello_copy();
    build_profile::write_default_profiles(dir.path()).unwrap();
    // A gate whose door is required, and left empty in the scene, for the build's check to find.
    let gate = "using Voxear;\n\nnamespace Hello;\n\npublic sealed class Gate : Behaviour\n{\n\t[Required]\n\tpublic Obj Door;\n}\n";
    std::fs::write(dir.path().join("Scripts/Gate.cs"), gate).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let text = std::fs::read_to_string(&scene).unwrap();
    std::fs::write(&scene, format!("{text}\n[[obj]]\nid = 99\nname = \"Gate\"\nbehaviours = [\"Hello.Gate\"]\n"))
        .unwrap();
    // A demo: Release's settings, with one level, its own name, the development tools, and smaller bundles.
    let base = if cfg!(windows) { "Windows Release" } else { "Linux Release" };
    let demo = format!(
        "format = 1\nbase = \"{base}\"\n[levels]\ninclude = [\"Levels/Day.level\"]\n[player]\nproduct_name = \"Hello Demo\"\n\
         [development]\nenabled = true\n[content]\ncompression = \"zstd\"\n"
    );
    std::fs::write(build_profile::profile_path(dir.path(), "Demo"), demo).unwrap();

    let vox = env!("CARGO_BIN_EXE_vox");
    let built = Command::new(vox)
        .arg("build")
        .arg(dir.path())
        .args(["--profile", "Demo", "--progress", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&built.stdout);
    let lines: Vec<Value> =
        stdout.lines().map(|line| serde_json::from_str(line).expect("every line is JSON")).collect();
    let stages: Vec<&str> =
        lines.iter().filter(|l| l["event"] == "stage").map(|l| l["stage"].as_str().unwrap()).collect();
    assert_eq!(stages, ["scripts", "content", "player"], "{stdout}");
    let finished = lines.last().unwrap();
    assert_eq!(finished["result"], "succeeded", "{stdout}");
    assert!(built.status.success());

    let output = Path::new(finished["output"].as_str().unwrap());
    assert_eq!(output, dir.path().join("Builds/Demo"));
    let program = Path::new(finished["program"].as_str().unwrap());
    assert_eq!(program.file_name().unwrap(), format!("Hello Demo{}", std::env::consts::EXE_SUFFIX).as_str());
    assert!(output.join("Data/Managed/Hello.Scripts.pdb").is_file(), "a development build keeps its symbols");
    // Apart from the editor's, which is built with VOXEAR_EDITOR, and only the game's own.
    let built = dir.path().join(".voxear/Scripts/Build/Debug");
    assert!(built.join("Hello.Scripts.dll").is_file(), "a build's scripts are built in a folder of their own");
    assert!(!built.join("Voxear.Core.dll").exists(), "where Voxear.Core is not built again");
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("build-report.json")).unwrap()).unwrap();
    assert_eq!(report["levels"], serde_json::json!(["Levels/Day.level"]), "only the level the profile ships");
    assert_eq!(report["development"], true);
    assert_eq!(report["compression"], "zstd", "the profile's compression, over the project's");
    let problems = report["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["file"], "Scenes/Day.scene");
    assert_eq!(
        (problems[0]["obj"].as_str(), problems[0]["message"].as_str()),
        (Some("Gate"), Some("Door is required"))
    );

    // The game keeps its log in the user's data folder, here a throwaway one, the run before's beside it.
    let data = tempfile::tempdir().unwrap();
    let run = || Command::new(program).args(["--headless", "--frames", "5"]).env("XDG_DATA_HOME", data.path()).output();
    let ran = run().unwrap();
    let log = String::from_utf8_lossy(&ran.stdout);
    assert!(ran.status.success(), "{log}{}", String::from_utf8_lossy(&ran.stderr));
    assert!(log.contains("Start    Spinner A (Spinner)"), "the scripts run: {log}");
    assert!(log.contains("ran 5 frames"), "{log}");
    if cfg!(target_os = "linux") {
        assert!(run().unwrap().status.success());
        let kept = data.path().join("Hello Demo");
        let text = std::fs::read_to_string(kept.join("player.log")).unwrap();
        assert!(text.contains("ran 5 frames") && !text.contains("\u{1b}["), "kept without colours: {text}");
        assert!(kept.join("player-prev.log").is_file(), "the run before's is kept beside it");
    }
}
