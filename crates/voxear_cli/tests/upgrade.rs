//! `vox upgrade --json` as the launcher runs it: every line on stdout is
//! JSON, ending in `done`, and the project is pinned to this version after.

use std::process::Command;

use serde_json::Value;

#[test]
fn upgrade_speaks_the_launchers_json_and_pins_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = "[project]\nname = \"Orbit\"\nengine = \"0.0.9\"\nstart_level = \"Main.level\"\n";
    std::fs::write(dir.path().join("voxear.toml"), manifest).unwrap();
    std::fs::write(dir.path().join("Main.level"), "format = 1\nscenes = []\n").unwrap();
    let ran = Command::new(env!("CARGO_BIN_EXE_vox"))
        .args(["upgrade"])
        .arg(dir.path())
        .args(["--from", "0.0.9", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(ran.status.success(), "{stdout}{}", String::from_utf8_lossy(&ran.stderr));
    let lines: Vec<Value> =
        stdout.lines().map(|line| serde_json::from_str(line).expect("every line is JSON")).collect();
    assert_eq!(lines.last().unwrap()["event"], "done");
    let changes: Vec<&str> =
        lines.iter().filter(|l| l["event"] == "change").map(|l| l["message"].as_str().unwrap()).collect();
    assert!(changes[0].starts_with("Moved from Voxear 0.0.9 to Voxear "), "{changes:?}");
    let pinned = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    assert!(pinned.contains(&format!("engine = \"{}\"", voxear_engine::engine_version())), "{pinned}");

    std::fs::write(dir.path().join("Main.level"), "format = 99\n").unwrap();
    let refused =
        Command::new(env!("CARGO_BIN_EXE_vox")).args(["upgrade"]).arg(dir.path()).arg("--json").output().unwrap();
    assert!(!refused.status.success());
    let last: Value = serde_json::from_str(String::from_utf8_lossy(&refused.stdout).lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], "error");
}
