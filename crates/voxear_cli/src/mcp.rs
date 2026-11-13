//! `vox mcp`: the editor's calls as tools for an AI assistant, over the
//! Model Context Protocol on stdin and stdout, a JSON-RPC message a line.
//!
//! Each of the editor's calls is a tool of its own, named with `_` for `.`
//! (`obj.get` is `obj_get`), with its arguments as the tool's input schema.
//! `vox` itself is one more tool, for the commands that work on files, as
//! `vox render` and `vox build` do. The tools reach the editor open on the
//! project; with none open, a headless one is started for them, and closed
//! when the assistant goes. A capture comes back as a picture as well as its
//! path, so the assistant sees what it changed.

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use voxear_editor::link::{ArgKind, CALLS};
use voxear_engine::local_link::{ErrorKind, LinkClient, LinkError};

use crate::editor_link;

/// The protocol versions this server speaks, newest first.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// How long a headless editor may take to open.
const OPENING: Duration = Duration::from_secs(180);

/// What the assistant is told about the tools when it connects.
const INSTRUCTIONS: &str = "These tools drive the Voxear editor open on one game project, as a person at the editor would. \
Start with editor_state and objs_list. Objs are named by { scene, path }, as objs_list gives them, which stay the same \
through undo, Play and saving. obj_get shows an Obj's components and every field with its value; field_set changes one. \
Every change is one step on the editor's undo, so the person can step back what was done; batch makes several changes \
one step. Changes stay in the editor until save writes them to the scene files. capture shows the Scene or Game view as \
a picture. play, stop and console_read run the game and read what it logged. docs_search and docs_read read the \
engine's manual. The vox tool runs the command line for work on files, such as `render` or `build`.";

/// The server: the project, the editor's link once connected, and the
/// headless editor it started, if it did.
struct Server {
    root: PathBuf,
    start_headless: bool,
    editor: Option<LinkClient>,
    started: Option<Child>,
}

/// Serves the protocol until stdin closes.
pub fn serve(project: &Path, start_headless: bool) -> Result<()> {
    let project = voxear_engine::Project::open(project)?;
    let mut server = Server { root: project.root.clone(), start_headless, editor: None, started: None };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                let reply = json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("not JSON: {error}") } });
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
                continue;
            }
        };
        // A notification, with no id, is not answered.
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let reply = match server.answer(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, text)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": text } }),
        };
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    server.close();
    Ok(())
}

impl Server {
    fn answer(&mut self, method: &str, params: &Value) -> std::result::Result<Value, (i64, String)> {
        match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]);
                let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "voxear", "version": voxear_engine::engine_version() },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                Ok(self.call_tool(name, args))
            }
            other => Err((-32601, format!("`{other}` is not a method this server has"))),
        }
    }

    /// A tool's result: its answer as text, and a capture's picture too.
    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        if name == "vox" {
            return run_vox(&self.root, &args);
        }
        let Some(call) = CALLS.iter().find(|call| tool_name(call.name) == name) else {
            return tool_error(&format!("no tool is named `{name}`"));
        };
        match self.call_editor(call.name, args) {
            Ok(Ok(result)) => {
                let mut content = vec![json!({ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() })];
                if call.name == "capture"
                    && let Some(bytes) = result.get("path").and_then(Value::as_str).and_then(|path| std::fs::read(path).ok())
                {
                    content.push(json!({ "type": "image", "data": base64::encode(bytes), "mimeType": "image/png" }));
                }
                json!({ "content": content, "isError": false })
            }
            Ok(Err(error)) => tool_error(&format!("{} ({})", error.message, serde_json::to_string(&error.kind).unwrap_or_default())),
            Err(error) => tool_error(&format!("{error:#}")),
        }
    }

    /// Makes a call to the editor, connecting first, and again once if the
    /// editor went, as when a windowed one took over from a headless one.
    fn call_editor(&mut self, call: &str, args: Value) -> Result<std::result::Result<Value, LinkError>> {
        for attempt in 0..2 {
            let editor = self.editor()?;
            match editor.call(call, args.clone()) {
                Ok(Err(error)) if error.kind == ErrorKind::Gone && attempt == 0 => self.editor = None,
                Ok(answer) => return Ok(answer),
                Err(_) if attempt == 0 => self.editor = None,
                Err(error) => return Err(error),
            }
        }
        unreachable!("the second attempt returns")
    }

    /// The editor's link: the editor open on the project, or a headless
    /// one started for the tools.
    fn editor(&mut self) -> Result<&mut LinkClient> {
        if self.editor.is_none() {
            let editor = match editor_link::connect(&self.root)? {
                Some(editor) => editor,
                None if self.start_headless => self.start_editor()?,
                None => anyhow::bail!(
                    "no editor is open on {}; open one with `vox editor`, or start `vox mcp` without --no-headless",
                    self.root.display()
                ),
            };
            self.editor = Some(editor);
        }
        Ok(self.editor.as_mut().expect("connected above"))
    }

    fn start_editor(&mut self) -> Result<LinkClient> {
        let vox = std::env::current_exe().context("finding vox")?;
        let child = Command::new(vox)
            .args(["editor", "serve", "--project"])
            .arg(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("starting a headless editor")?;
        self.started = Some(child);
        let began = Instant::now();
        loop {
            if let Some(editor) = editor_link::connect(&self.root)? {
                return Ok(editor);
            }
            if let Some(Ok(Some(status))) = self.started.as_mut().map(Child::try_wait) {
                anyhow::bail!("the headless editor closed as it opened ({status}); `vox editor serve` says why");
            }
            anyhow::ensure!(began.elapsed() < OPENING, "the headless editor took too long to open");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Closes the headless editor this server started, if it is still the
    /// one answering.
    fn close(&mut self) {
        let Some(mut child) = self.started.take() else { return };
        if let Some(editor) = self.editor.as_mut().filter(|editor| editor.about.headless) {
            let _ = editor.call("editor.quit", json!({ "discard": true }));
        }
        let waited = Instant::now();
        while waited.elapsed() < Duration::from_secs(10) {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close();
    }
}

fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// A call's name as a tool's, which has no dots.
pub fn tool_name(call: &str) -> String {
    call.replace('.', "_")
}

/// Every tool, with its input schema.
pub fn tools() -> Vec<Value> {
    let mut tools: Vec<Value> = CALLS
        .iter()
        .map(|call| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for arg in call.args {
                let mut schema = schema(arg.kind);
                if !arg.about.is_empty() {
                    schema["description"] = json!(arg.about);
                }
                properties.insert(arg.name.to_string(), schema);
                if arg.required {
                    required.push(arg.name);
                }
            }
            let mut description = call.about.to_string();
            if call.changes {
                description.push_str(" One step to undo.");
            }
            json!({
                "name": tool_name(call.name),
                "description": description,
                "inputSchema": { "type": "object", "properties": properties, "required": required },
            })
        })
        .collect();
    tools.push(json!({
        "name": "vox",
        "description": "Runs the Voxear command line on the project's files and gives back what it printed: \
`render --out frame.png` renders the start level offscreen, `run --headless --frames 60` runs it without a window, \
`build`, `assets`, `content build`, `package list`, `loc export`, and `console \"line\"` for a game running from `vox run`. \
The project is given for you.",
        "inputSchema": {
            "type": "object",
            "properties": { "args": { "type": "array", "items": { "type": "string" }, "description": "The words after `vox`." } },
            "required": ["args"],
        },
    }));
    tools
}

/// The JSON Schema of an argument.
fn schema(kind: ArgKind) -> Value {
    let obj = json!({
        "anyOf": [
            { "type": "object", "properties": { "scene": { "type": "string" }, "path": { "type": "string" } } },
            { "type": "object", "properties": { "entity": { "type": "integer" } } },
            { "type": "string" },
        ],
    });
    match kind {
        ArgKind::Obj => obj,
        ArgKind::Objs => json!({ "type": "array", "items": obj }),
        ArgKind::Asset => json!({
            "anyOf": [
                { "type": "object", "properties": { "guid": { "type": "string" }, "path": { "type": "string" } } },
                { "type": "string" },
            ],
        }),
        ArgKind::Component | ArgKind::Field | ArgKind::Text => json!({ "type": "string" }),
        ArgKind::Value => json!({}),
        ArgKind::Number => json!({ "type": "number" }),
        ArgKind::Whole => json!({ "type": "integer" }),
        ArgKind::Toggle => json!({ "type": "boolean" }),
        ArgKind::Vector => json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }),
        ArgKind::Choice(options) => json!({ "type": "string", "enum": options }),
        ArgKind::Calls => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": { "call": { "type": "string" }, "args": { "type": "object" } },
                "required": ["call"],
            },
        }),
    }
}

/// The `vox` tool: runs this program with the words given, on the project,
/// and gives back what it printed and how it ended. It does not start
/// editors or another server.
fn run_vox(root: &Path, args: &Value) -> Value {
    let words: Vec<String> =
        args.get("args").and_then(Value::as_array).map(|w| w.iter().filter_map(|w| w.as_str().map(String::from)).collect()).unwrap_or_default();
    let Some(first) = words.first() else { return tool_error("give the words after `vox`, as [\"render\"]") };
    if matches!(first.as_str(), "editor" | "mcp" | "new" | "upgrade") {
        return tool_error(&format!("`vox {first}` is not run from here; the editor's own tools do its work"));
    }
    let Ok(vox) = std::env::current_exe() else { return tool_error("vox could not find itself") };
    // Commands work on the project in the folder they run in.
    let mut command = Command::new(vox);
    command.args(&words).current_dir(root).stdin(Stdio::null());
    match command.output() {
        Ok(output) => {
            let text = format!(
                "exit code {}\n{}{}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            json!({ "content": [{ "type": "text", "text": text }], "isError": !output.status.success() })
        }
        Err(error) => tool_error(&format!("could not run vox: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_call_is_a_tool_with_a_schema() {
        let tools = tools();
        assert_eq!(tools.len(), CALLS.len() + 1);
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{name}");
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
        let get = tools.iter().find(|tool| tool["name"] == "obj_get").unwrap();
        assert_eq!(get["inputSchema"]["required"], json!(["obj"]));
        assert!(voxear_editor::link::call_info("obj.get").is_some());
    }
}
