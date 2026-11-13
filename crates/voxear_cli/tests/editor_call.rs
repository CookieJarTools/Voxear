//! `vox editor call` and `vox mcp` as a tool runs them: JSON out, exit codes
//! that say what happened, and the MCP server's handshake and tools. The
//! headless editor's test skips itself without a GPU.

use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};

use serde_json::{Value, json};

fn vox() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vox"))
}

/// A copy of the Hello sample without its scripts, so nothing builds.
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

#[test]
fn with_no_editor_open_a_call_says_so_and_exits_3() {
    let dir = hello_copy();
    let output = vox().args(["editor", "call", "editor.state", "--project"]).arg(dir.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(3), "{}", String::from_utf8_lossy(&output.stderr));
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer["error"]["kind"], "no_editor");
}

#[test]
fn describe_lists_every_call_and_the_exit_codes() {
    let dir = hello_copy();
    let output = vox().args(["editor", "call", "--describe", "--json", "--project"]).arg(dir.path()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let described: Value = serde_json::from_slice(&output.stdout).unwrap();
    let calls = described["calls"].as_array().unwrap();
    assert_eq!(calls.len(), voxear_editor::link::CALLS.len());
    let set = calls.iter().find(|call| call["name"] == "field.set").unwrap();
    let args: Vec<&str> = set["args"].as_array().unwrap().iter().map(|arg| arg["name"].as_str().unwrap()).collect();
    assert_eq!(args, ["obj", "component", "field", "value"]);
    assert!(described["exit_codes"]["3"].is_string());
}

#[test]
fn the_mcp_server_shakes_hands_and_lists_its_tools() {
    let dir = hello_copy();
    let mut server = vox()
        .args(["mcp", "--no-headless", "--project"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let mut send = |message: Value| writeln!(input, "{message}").unwrap();
    let mut read = || {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }));
    let hello = read();
    assert_eq!(hello["result"]["protocolVersion"], "2025-06-18");
    assert!(hello["result"]["capabilities"]["tools"].is_object());
    // A notification is not answered: the next line read is the list's.
    send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let listed = read();
    assert_eq!(listed["id"], 2);
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "field_set"));
    assert!(tools.iter().any(|tool| tool["name"] == "vox"));
    send(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "editor_state", "arguments": {} } }));
    let called = read();
    assert_eq!(called["result"]["isError"], true, "with no editor and --no-headless there is nothing to ask");
    drop(send);
    drop(input);
    assert!(server.wait().unwrap().success());
}

#[test]
fn a_headless_batch_makes_an_obj_and_saves_it() {
    let dir = hello_copy();
    let mut call = vox()
        .args(["editor", "call", "batch", "--headless", "--project"])
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = call.stdin.take().unwrap();
    writeln!(input, "{}", json!({ "call": "obj.create", "args": { "kind": "Cube", "name": "Made By A Tool" } })).unwrap();
    writeln!(input, "{}", json!({ "call": "save" })).unwrap();
    drop(input);
    let output = call.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("GPU") && !output.status.success() {
        eprintln!("skipping: no usable GPU");
        return;
    }
    assert!(output.status.success(), "{stderr}");
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(answer["failed"].is_null(), "{answer}");
    let saved = answer["results"][1]["saved"][0].as_str().unwrap();
    let scene = std::fs::read_to_string(dir.path().join(saved)).unwrap();
    assert!(scene.contains("Made By A Tool"), "{scene}");
}
