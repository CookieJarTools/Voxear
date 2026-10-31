//! A line of the log as the Console holds it: what it says, its parsed
//! stack, and the categories it is in, which nobody had to declare, as in
//! Sifter. A line starting `[Combat]` is in Combat; a script's line is in
//! its code's root namespace, from the first frame of the project's own; an
//! engine line is in its crate (Engine, Render, Editor); and the C#
//! compiler's lines are in Compiler.

use std::path::Path;

use tracing::Level;
use voxear_core::logging::LogLine;
use voxear_ecs::Entity;

use super::frame::Frame;

/// Namespaces that are not the project's own.
const NOT_MINE: [&str; 3] = ["System", "Microsoft", "Voxear"];

#[derive(Clone, Debug)]
pub struct Entry {
    pub level: Level,
    pub message: String,
    /// What a search looks through, in lowercase: the message and the stack.
    pub lower: String,
    pub seconds: f64,
    pub frame: u64,
    pub worker: bool,
    pub frames: Vec<Frame>,
    /// Which frames are the project's own code.
    pub mine: Vec<bool>,
    pub categories: Vec<String>,
    /// The Obj a script's line is about.
    pub context: Option<Entity>,
}

impl Entry {
    /// A logged line, its stack read and its categories found. `project` is
    /// the project's folder, whose files are its own code.
    pub fn new(line: &LogLine, project: &Path) -> Self {
        let frames: Vec<Frame> = match (&line.stack, &line.file, line.target.as_str()) {
            (Some(stack), ..) => stack.lines().filter_map(Frame::parse).collect(),
            (None, _, "compiler") => Frame::compiler(&line.message).into_iter().collect(),
            (None, Some(file), _) => vec![Frame::at(file, line.line)],
            (None, None, _) => Vec::new(),
        };
        let mine: Vec<bool> = frames.iter().map(|f| is_mine(f, project, &line.target)).collect();
        let categories = categories(line, &frames, &mine);
        let raw: Vec<&str> = frames.iter().map(|f| f.raw.as_str()).collect();
        let lower = format!("{}\n{}", line.message, raw.join("\n")).to_lowercase();
        Self {
            level: line.level,
            message: line.message.clone(),
            lower,
            seconds: line.seconds,
            frame: line.frame,
            worker: line.worker,
            frames,
            mine,
            categories,
            context: line.context.map(Entity::from_bits),
        }
    }

    /// Where the line came from, for its row: the first of the project's
    /// own frames with a file, or else the first frame with one.
    pub fn source(&self) -> Option<&Frame> {
        let with_file = |&(frame, _): &(&Frame, &bool)| frame.file.is_some();
        let mut frames = self.frames.iter().zip(&self.mine);
        frames.clone().filter(with_file).find(|(_, mine)| **mine).or_else(|| frames.find(with_file)).map(|(f, _)| f)
    }

    /// The line as text, for the clipboard: its level and message, then its
    /// stack.
    pub fn as_text(&self) -> String {
        let mut text = format!("{} {}", self.level, self.message);
        for frame in &self.frames {
            text.push_str("\n  ");
            text.push_str(&frame.raw);
        }
        text
    }
}

/// Whether a frame is the project's own code: its file is in the project,
/// or, for a script frame with no file, its namespace is not the engine's or
/// .NET's. Rust frames are the engine's, but an engine line's only frame is
/// where it was logged, which is worth opening, so they count.
fn is_mine(frame: &Frame, project: &Path, target: &str) -> bool {
    if target != "script" {
        return true;
    }
    match &frame.file {
        Some(file) => Path::new(file).starts_with(project),
        None => {
            let root = frame.namespace.split('.').next().unwrap_or_default();
            !root.is_empty() && !NOT_MINE.contains(&root)
        }
    }
}

/// The categories a line is in, which nobody declared.
fn categories(line: &LogLine, frames: &[Frame], mine: &[bool]) -> Vec<String> {
    let mut found = Vec::new();
    let tagged = line.message.strip_prefix('[').and_then(|m| m.split_once(']')).map(|(tag, _)| tag.trim());
    if let Some(tag) = tagged.filter(|t| !t.is_empty() && t.len() <= 32) {
        found.push(tag.to_string());
    }
    let from = match line.target.as_str() {
        "compiler" => "Compiler".to_string(),
        "script" => {
            let own = frames.iter().zip(mine).find(|(f, mine)| **mine && f.has_call()).map(|(f, _)| f);
            let root = own.map(|f| f.namespace.split('.').next().unwrap_or_default().to_string());
            root.filter(|r| !r.is_empty()).unwrap_or_else(|| "Scripts".to_string())
        }
        target => crate_name(target),
    };
    if !from.is_empty() && !found.contains(&from) {
        found.push(from);
    }
    found
}

/// An engine crate as a category: `voxear_render::shadows` is Render,
/// `voxear_ui` is UI, and another crate is its own name.
fn crate_name(target: &str) -> String {
    let krate = target.split("::").next().unwrap_or(target);
    match krate.strip_prefix("voxear_") {
        Some(name @ ("rhi" | "ui" | "ecs")) => name.to_uppercase(),
        Some(name) => {
            let mut letters = name.chars();
            letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
        }
        None => krate.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(target: &str, message: &str, stack: Option<&str>) -> LogLine {
        LogLine { target: target.into(), stack: stack.map(str::to_string), ..LogLine::new(Level::INFO, message) }
    }

    #[test]
    fn categories_come_from_the_line_without_being_declared() {
        let project = Path::new("/home/jeo/Hello");
        let engine = Entry::new(&line("voxear_engine::levels", "level loaded", None), project);
        assert_eq!(engine.categories, ["Engine"]);
        let tagged = Entry::new(&line("voxear_render", "[Shadows] cascades rebuilt", None), project);
        assert_eq!(tagged.categories, ["Shadows", "Render"]);
        let stack = "   at Voxear.Log.Info(String message)\n   at Hello.Spinner.Start() in /home/jeo/Hello/Scripts/Spinner.cs:line 33";
        let script = Entry::new(&line("script", "Start Spinner A", Some(stack)), project);
        assert_eq!(script.categories, ["Hello"], "from the first frame of the project's own");
        assert_eq!(script.mine, [false, true]);
        assert_eq!(script.source().and_then(Frame::file_name), Some("Spinner.cs"));
        let compiler =
            Entry::new(&line("compiler", "/home/jeo/Hello/Scripts/Spinner.cs(12,5): error CS1002", None), project);
        assert_eq!(compiler.categories, ["Compiler"]);
        assert_eq!(compiler.source().and_then(|f| f.line), Some(12));
        assert_eq!(crate_name("wgpu_core::device"), "wgpu_core");
        assert_eq!(crate_name("voxear_rhi::vulkan"), "RHI");
        assert_eq!(crate_name("voxear_ui::list"), "UI");
        assert!(Entry::new(&LogLine::new(Level::INFO, "from nowhere"), project).categories.is_empty());
    }
}
