//! A running game's link for tools, so a code editor or a script can watch a
//! game it started. A game from `vox run` writes it as [`FILE`] in the
//! project's `.voxear` folder, beside the editor's, over the same transport
//! ([`local_link`](crate::local_link)): JSON-RPC calls a line, after the
//! file's secret, answered on the game's thread between frames. `link.hello`
//! says what answers, its process among it, for a debugger to attach to.
//!
//! The calls:
//! - `profiler.frames` `{ after?, limit? }`: the latest frames' CPU samples
//!   and GPU passes, as [`profile_frames`](crate::profile_frames) has them.
//!   Frames are sampled only while a tool reads them, so a game nothing
//!   watches pays nothing.
//! - `console.read` `{ after?, level?, limit? }`: the log, as the editor's
//!   Console call gives it.
//! - `game.console` `{ line }`: runs a line in the game's console, and gives
//!   back what it logged.
//! - `gpu.capture`: captures the next frame in RenderDoc and answers with the
//!   file once it is written, in a game started with RenderDoc loaded. The
//!   tool opens it, as it likes.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Result;
use serde_json::{Value, json};
use tracing::Level;
use voxear_core::{logging::LogLines, profiling};
use voxear_render::Renderer;

use crate::{
    local_link::{Answer, ErrorKind, LinkError, LinkServer, Request},
    profile_frames::{self, FrameLog},
};

/// The file a game keeps its link's address and secret in.
pub const FILE: &str = "Game";
/// The log lines a read gives back when it does not say.
const LINES: usize = 200;
/// How long a capture may take to be written before its call gives up.
const CAPTURE_WAIT: Duration = Duration::from_secs(10);

/// A capture asked for: the call, the capture RenderDoc had before it, and
/// when it was asked.
struct Capturing {
    request: Request,
    before: Option<PathBuf>,
    asked: Instant,
}

/// The game's side of the link.
pub struct GameLink {
    server: LinkServer,
    frames: FrameLog,
    captures: Vec<Capturing>,
    /// Whether the main thread is sampling now, for a tool.
    sampling: bool,
}

impl GameLink {
    /// Listens, and writes where to `folder`. Refuses while another game
    /// from the same folder has its link.
    pub fn listen(folder: &Path) -> Result<Self> {
        let server = LinkServer::listen(folder, FILE, "game", false)?;
        Ok(Self { server, frames: FrameLog::default(), captures: Vec::new(), sampling: false })
    }

    /// The file it wrote.
    pub fn file(&self) -> &Path {
        self.server.file()
    }

    /// Before a frame: samples it while a tool reads frames, and stops once
    /// none has for a while.
    pub fn begin_frame(&mut self) {
        let wanted = self.frames.watched();
        if wanted == self.sampling {
            return;
        }
        self.sampling = wanted;
        profiling::set_enabled(wanted);
        if !wanted {
            // What was sampled since the last frame is let go.
            profiling::end_frame();
        }
    }

    /// After a frame numbered `frame`, `frame_ms` long: keeps what it
    /// sampled, and answers the calls since the last. `console` runs a line
    /// in the game's console and returns what it logged.
    pub fn end_frame(
        &mut self,
        frame: u64,
        frame_ms: f32,
        mut renderer: Option<&mut Renderer>,
        mut console: impl FnMut(&str) -> Vec<String>,
    ) {
        let gpu = renderer.as_ref().map_or(&[][..], |renderer| renderer.timings());
        if self.sampling {
            self.frames.push(frame, frame_ms, &profiling::end_frame(), gpu);
        }
        for request in self.server.poll() {
            let answer = match request.call.as_str() {
                "profiler.frames" => {
                    let (after, limit) = (whole(&request.args, "after"), whole(&request.args, "limit"));
                    Ok(self.frames.read(after, limit.map_or(profile_frames::READ, |n| n as usize)))
                }
                "console.read" => read_log(&voxear_core::logging::recent().lock().unwrap(), &request.args),
                "game.console" => match request.args.get("line").and_then(Value::as_str) {
                    Some(line) => Ok(json!({ "lines": console(line) })),
                    None => Err(LinkError::new(ErrorKind::BadArguments, "`game.console` needs a `line`")),
                },
                "gpu.capture" => match renderer.as_deref_mut().filter(|renderer| renderer.has_renderdoc()) {
                    Some(renderer) => {
                        let before = renderer.latest_capture();
                        renderer.capture_next_frame_unopened();
                        self.captures.push(Capturing { request, before, asked: Instant::now() });
                        continue;
                    }
                    None => Err(LinkError::new(
                        ErrorKind::NotAllowed,
                        "RenderDoc is not loaded in this game: run it with `vox run --renderdoc` to capture frames",
                    )),
                },
                other => Err(LinkError::new(
                    ErrorKind::NoSuchCall,
                    format!(
                        "a game answers profiler.frames, console.read, game.console and gpu.capture, not `{other}`"
                    ),
                )),
            };
            request.answer(answer);
        }
        self.answer_captures(renderer.as_deref());
    }

    /// Answers each capture whose file RenderDoc has written, and gives up on
    /// those it never wrote.
    fn answer_captures(&mut self, renderer: Option<&Renderer>) {
        let latest = renderer.and_then(Renderer::latest_capture);
        for capturing in std::mem::take(&mut self.captures) {
            let answer: Answer = match &latest {
                Some(path) if Some(path) != capturing.before.as_ref() => {
                    // RenderDoc names it from the game's folder, which the tool may not share.
                    Ok(json!({ "path": std::path::absolute(path).unwrap_or_else(|_| path.clone()) }))
                }
                _ if capturing.asked.elapsed() > CAPTURE_WAIT => {
                    Err(LinkError::new(ErrorKind::Failed, "RenderDoc wrote no capture"))
                }
                _ => {
                    self.captures.push(capturing);
                    continue;
                }
            };
            capturing.request.answer(answer);
        }
    }
}

/// A whole number argument.
fn whole(args: &Value, name: &str) -> Option<u64> {
    args.get(name).and_then(Value::as_u64)
}

/// The log's lines after `after` (a number a read gave back as `next`), at
/// `level` or worse, the latest `limit` of them, as the editor's
/// `console.read` gives its Console's.
pub fn read_log(log: &LogLines, args: &Value) -> Answer {
    let level = match args.get("level").and_then(Value::as_str) {
        Some("error") => Level::ERROR,
        Some("warning") => Level::WARN,
        Some("info") | None => Level::TRACE,
        Some(other) => {
            return Err(LinkError::new(ErrorKind::BadArguments, format!("`{other}` is not info, warning or error")));
        }
    };
    let limit = whole(args, "limit").map_or(LINES, |n| n as usize).max(1);
    let after = whole(args, "after");
    let first = log.total - log.lines.len() as u64;
    let mut lines: Vec<Value> = Vec::new();
    for (index, line) in log.lines.iter().enumerate() {
        let number = first + index as u64;
        if after.is_some_and(|after| number < after) || line.level > level {
            continue;
        }
        let mut shown = json!({
            "number": number,
            "level": line.level.as_str().to_lowercase(),
            "message": line.message,
            "from": line.target,
            "seconds": line.seconds,
            "frame": line.frame,
        });
        if let (Some(file), Some(at)) = (&line.file, line.line) {
            (shown["file"], shown["line"]) = (json!(file), json!(at));
        }
        if let Some(stack) = &line.stack {
            shown["stack"] = json!(stack);
        }
        if let Some(asset) = &line.asset {
            shown["asset"] = json!(asset);
        }
        lines.push(shown);
    }
    let skip = lines.len().saturating_sub(limit);
    Ok(json!({ "lines": lines.split_off(skip), "next": log.total }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_link::LinkClient;
    use voxear_core::logging::LogLine;

    /// Answers `link`'s calls as a game's frames would, until `asked` ends.
    fn frames_until<T>(link: &mut GameLink, asked: std::thread::JoinHandle<T>) -> T {
        let started = Instant::now();
        let mut frame = 0;
        while !asked.is_finished() {
            link.begin_frame();
            {
                let _tick = profiling::Marker::new("Game").scope();
            }
            link.end_frame(frame, 16.0, None, |line| vec![format!("ran {line}")]);
            frame += 1;
            assert!(started.elapsed() < Duration::from_secs(10), "the call was never answered");
            std::thread::sleep(Duration::from_millis(2));
        }
        asked.join().unwrap()
    }

    #[test]
    fn a_tool_reads_a_running_games_frames_and_runs_its_console_through_the_link() {
        let folder = tempfile::tempdir().unwrap();
        let mut link = GameLink::listen(folder.path()).unwrap();
        let path = folder.path().to_path_buf();
        let asked = std::thread::spawn(move || {
            let mut client = LinkClient::connect(&path, FILE).unwrap();
            assert_eq!(client.about.kind, "game");
            assert_eq!(client.about.pid, std::process::id(), "a debugger can attach to the process it names");
            let first = client.call("profiler.frames", json!({})).unwrap().unwrap();
            // Frames are sampled from the first read on; wait until some have been.
            let mut frames = Vec::new();
            let mut next = first["next"].as_u64().unwrap();
            while frames.is_empty() {
                let read = client.call("profiler.frames", json!({ "after": next })).unwrap().unwrap();
                next = read["next"].as_u64().unwrap();
                frames.extend(read["frames"].as_array().unwrap().iter().cloned());
            }
            let said = client.call("game.console", json!({ "line": "God true" })).unwrap().unwrap();
            let capture = client.call("gpu.capture", json!({})).unwrap();
            let wrong = client.call("obj.get", json!({})).unwrap();
            (frames, said, capture, wrong)
        });
        let (frames, said, capture, wrong) = frames_until(&mut link, asked);
        assert_eq!(frames[0]["samples"][0]["name"], "Game", "{frames:?}");
        assert_eq!(said["lines"], json!(["ran God true"]));
        assert_eq!(capture.unwrap_err().kind, ErrorKind::NotAllowed, "no RenderDoc, no capture");
        assert_eq!(wrong.unwrap_err().kind, ErrorKind::NoSuchCall);
        let file = link.file().to_path_buf();
        drop(link);
        assert!(!file.exists(), "closing tidies up");
    }

    #[test]
    fn the_log_is_read_from_where_the_last_read_stopped_at_a_level_or_worse() {
        let mut log = LogLines::default();
        for (level, message) in [(Level::INFO, "loaded"), (Level::WARN, "slow"), (Level::ERROR, "broke")] {
            log.lines.push_back(LogLine::new(level, message));
            log.total += 1;
        }
        let read = read_log(&log, &json!({})).unwrap();
        assert_eq!(read["next"], 3);
        assert_eq!(read["lines"].as_array().unwrap().len(), 3);
        let warnings = read_log(&log, &json!({ "level": "warning" })).unwrap();
        let messages: Vec<&str> =
            warnings["lines"].as_array().unwrap().iter().map(|l| l["message"].as_str().unwrap()).collect();
        assert_eq!(messages, ["slow", "broke"]);
        let newer = read_log(&log, &json!({ "after": 2 })).unwrap();
        assert_eq!(newer["lines"][0]["number"], 2);
        assert_eq!(newer["lines"][0]["level"], "error");
        assert!(read_log(&log, &json!({ "level": "loud" })).is_err());
    }
}
