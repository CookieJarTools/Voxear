//! `vox editor call` and `vox editor serve`: a script or an AI assistant
//! reaching the editor open on a project, through the link it listens on
//! (see `voxear_editor::link`), or a headless editor opened for the calls.

use std::{
    io::{BufRead, IsTerminal},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use voxear_editor::link::{self, LINK_FILE, link_folder};
use voxear_engine::{
    Project,
    local_link::{ErrorKind, LinkClient, LinkError, Unreachable},
};

/// The exit codes `vox editor call` ends with, as `--describe` lists them.
pub const DONE: i32 = 0;
pub const FAILED: i32 = 1;
pub const NO_EDITOR: i32 = 3;
pub const REFUSED: i32 = 4;

/// How big a headless editor draws, as its captures come out.
pub const HEADLESS_SIZE: (u32, u32) = (1600, 900);

/// The exit code for a call refused this way.
pub fn exit_code(error: &LinkError) -> i32 {
    match error.kind {
        ErrorKind::RefusedWhilePlaying | ErrorKind::UnsavedConflict | ErrorKind::NotAllowed => REFUSED,
        _ => FAILED,
    }
}

/// Connects to the editor open on the project at `root`. `Ok(None)` when
/// none is open.
pub fn connect(root: &Path) -> Result<Option<LinkClient>> {
    match LinkClient::connect(&link_folder(root), LINK_FILE) {
        Ok(client) => Ok(Some(client)),
        Err(error) if error.downcast_ref::<Unreachable>().is_some() => Ok(None),
        Err(error) => Err(error),
    }
}

/// A call's arguments from the command line: one JSON object, or
/// `name=value` pairs, each value read as JSON when it is, and as text
/// otherwise, so `name=Lamp` and `position=[0,2,0]` both work.
pub fn parse_args(words: &[String]) -> Result<Value> {
    if let [only] = words
        && only.trim_start().starts_with('{')
    {
        return serde_json::from_str(only).context("the arguments are not a JSON object");
    }
    let mut args = serde_json::Map::new();
    for word in words {
        let Some((name, value)) = word.split_once('=') else {
            bail!("`{word}` is not `name=value`; give the arguments as name=value pairs or one JSON object")
        };
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        args.insert(name.to_string(), value);
    }
    Ok(Value::Object(args))
}

/// The calls of a batch, read from stdin: a JSON object a line, each
/// `{ "call": "…", "args": { … } }`.
fn calls_from_stdin() -> Result<Value> {
    let mut calls = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        calls.push(serde_json::from_str::<Value>(&line).with_context(|| format!("not JSON: {line}"))?);
    }
    Ok(json!({ "calls": calls }))
}

/// What `vox editor call` was asked.
pub struct CallRequest {
    pub call: Option<String>,
    pub args: Vec<String>,
    pub project: PathBuf,
    pub headless: bool,
    pub describe: bool,
    pub json: bool,
}

/// `vox editor call`: makes the call, prints its answer as JSON, and
/// returns the exit code.
pub fn call(
    request: CallRequest,
    settings: impl Fn(&Project) -> voxear_render::RendererSettings,
    log: voxear_core::logging::LogCapture,
) -> Result<i32> {
    let project = Project::open(&request.project)?;
    if request.describe {
        return describe(&project, request.headless, request.json, &settings, log);
    }
    let call = request.call.context("name the call, as `vox editor call editor.state`; `--describe` lists them")?;
    let args = match (call.as_str(), request.args.is_empty()) {
        ("batch", true) if !std::io::stdin().is_terminal() => calls_from_stdin()?,
        _ => parse_args(&request.args)?,
    };
    let answer = match connect(&project.root)? {
        Some(mut editor) => editor.call(&call, args)?,
        None if request.headless => {
            let calls = [(call.clone(), args)];
            let mut answers = voxear_editor::call_headless(Project::open(&project.root)?, &settings(&project), log, HEADLESS_SIZE, &calls)?;
            answers.pop().context("the headless editor gave no answer")?
        }
        None => {
            let message = format!(
                "no editor is open on {}; open one with `vox editor`, or give --headless to work on the files",
                project.root.display()
            );
            println!("{}", json!({ "error": { "kind": "no_editor", "message": message } }));
            eprintln!("{message}");
            return Ok(NO_EDITOR);
        }
    };
    Ok(print_answer(answer))
}

fn print_answer(answer: Result<Value, LinkError>) -> i32 {
    match answer {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
            DONE
        }
        Err(error) => {
            println!("{}", json!({ "error": { "kind": error.kind, "message": error.message } }));
            eprintln!("{}", error.message);
            exit_code(&error)
        }
    }
}

/// `--describe`: every call with its arguments, the exit codes, and, from
/// the editor open on the project when there is one, the components,
/// commands and kinds this project has.
fn describe(
    project: &Project,
    headless: bool,
    as_json: bool,
    settings: &impl Fn(&Project) -> voxear_render::RendererSettings,
    log: voxear_core::logging::LogCapture,
) -> Result<i32> {
    let mut described = link::describe_calls();
    described["exit_codes"] = json!({
        "0": "done",
        "1": "the call failed, or its arguments were wrong",
        "2": "the command line was wrong",
        "3": "no editor is open on the project, and --headless was not given",
        "4": "refused: the game is playing, edits would be lost, or the editor lets tools only read",
    });
    let asked = ["components", "commands", "kinds"];
    let answers: Option<Vec<Result<Value, LinkError>>> = match connect(&project.root)? {
        Some(mut editor) => Some(asked.iter().map(|call| editor.call(call, json!({}))).collect::<Result<_>>()?),
        None if headless => {
            let calls: Vec<(String, Value)> = asked.iter().map(|call| (call.to_string(), json!({}))).collect();
            Some(voxear_editor::call_headless(Project::open(&project.root)?, &settings(project), log, HEADLESS_SIZE, &calls)?)
        }
        None => None,
    };
    if let Some(answers) = answers {
        let mut project_has = serde_json::Map::new();
        for (name, answer) in asked.iter().zip(answers) {
            project_has.insert(name.to_string(), answer.unwrap_or_else(|error| json!({ "error": error.message })));
        }
        described["project"] = Value::Object(project_has);
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&described)?);
        return Ok(DONE);
    }
    for call in link::CALLS {
        let args: Vec<String> = call
            .args
            .iter()
            .map(|arg| if arg.required { arg.name.to_string() } else { format!("[{}]", arg.name) })
            .collect();
        println!("{} {}", call.name, args.join(" "));
        println!("    {}", call.about);
    }
    if described.get("project").is_none() {
        println!("\nNo editor is open on the project: open one, or give --headless, to list its components and commands.");
    }
    Ok(DONE)
}

/// `vox editor serve`: a headless editor answering tools until one asks it
/// to close.
pub fn serve(project: &Path, settings: impl Fn(&Project) -> voxear_render::RendererSettings, log: voxear_core::logging::LogCapture) -> Result<()> {
    let project = Project::open(project)?;
    let settings = settings(&project);
    voxear_editor::serve(project, &settings, log, HEADLESS_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_pairs_with_json_values_or_one_object() {
        let words: Vec<String> = ["name=Lamp", "position=[0,2,0]", "active=false"].map(String::from).to_vec();
        assert_eq!(parse_args(&words).unwrap(), json!({ "name": "Lamp", "position": [0, 2, 0], "active": false }));
        let object = vec![r#"{ "obj": "abc:3" }"#.to_string()];
        assert_eq!(parse_args(&object).unwrap(), json!({ "obj": "abc:3" }));
        assert!(parse_args(&["lamp".to_string()]).is_err());
    }
}
