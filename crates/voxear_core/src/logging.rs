use std::{
    collections::VecDeque,
    fmt::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::ThreadId,
    time::Instant,
};

use tracing::{Event, Level, Subscriber, field::Field};
use tracing_subscriber::{EnvFilter, Layer, layer::Context, prelude::*};

/// The default filter: engine info, and only errors from the windowing
/// library, whose warnings are mostly desktop-environment noise.
const DEFAULT_FILTER: &str = "info,winit=error,sctk=error";

/// Installs the global log subscriber. `VOXEAR_LOG` (or `RUST_LOG`) overrides
/// the default filter, e.g. `VOXEAR_LOG=voxear_rhi=debug`.
pub fn init() {
    let filter =
        std::env::var("VOXEAR_LOG").or_else(|_| std::env::var("RUST_LOG")).unwrap_or_else(|_| DEFAULT_FILTER.into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .fmt_fields(printed_fields())
        .try_init();
}

/// What the terminal shows of a line's fields: the message and anything
/// else, but not a script's stack or Obj, which are for the Console.
fn printed_fields() -> impl for<'writer> tracing_subscriber::fmt::FormatFields<'writer> + 'static {
    tracing_subscriber::fmt::format::debug_fn(|writer, field, value| match field.name() {
        "stack" | "context" => Ok(()),
        "message" => write!(writer, "{value:?}"),
        name => write!(writer, " {name}={value:?}"),
    })
}

/// Log lines kept for the editor's Console, newest last.
pub type LogCapture = Arc<Mutex<LogLines>>;

/// The most lines kept; older ones go first.
const KEPT: usize = 10_000;

#[derive(Debug, Default)]
pub struct LogLines {
    pub lines: VecDeque<LogLine>,
    /// Every line ever logged, kept or not, so readers can tell what is new.
    pub total: u64,
}

/// The engine frame running now, which each log line notes; the editor and
/// game loop count it up.
static FRAME: AtomicU64 = AtomicU64::new(0);

/// Notes the frame now running, for the lines logged in it.
pub fn set_frame(frame: u64) {
    FRAME.store(frame, Ordering::Relaxed);
}

/// One line logged, with where and when it came from.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub level: Level,
    pub message: String,
    /// What logged it: the Rust module (`voxear_engine::levels`), `script`
    /// for game scripts, or `compiler` for the C# compiler.
    pub target: String,
    /// The Rust file and line that logged it. Scripts give their stack.
    pub file: Option<String>,
    pub line: Option<u32>,
    /// Seconds since logging started.
    pub seconds: f64,
    /// The engine frame it was logged in.
    pub frame: u64,
    /// Logged on a thread other than the one that started logging.
    pub worker: bool,
    /// A script's stack, a frame to a line, as .NET writes it.
    pub stack: Option<String>,
    /// The Obj a script's line is about, as entity bits.
    pub context: Option<u64>,
}

impl LogLine {
    /// A line with nothing but its level and message, as tests make them.
    pub fn new(level: Level, message: &str) -> Self {
        Self {
            level,
            message: message.to_string(),
            target: String::new(),
            file: None,
            line: None,
            seconds: 0.0,
            frame: 0,
            worker: false,
            stack: None,
            context: None,
        }
    }
}

/// Installs the global log subscriber as [`init`] does, and also keeps every
/// line for the editor's Console to show.
pub fn init_capturing() -> LogCapture {
    let filter =
        std::env::var("VOXEAR_LOG").or_else(|_| std::env::var("RUST_LOG")).unwrap_or_else(|_| DEFAULT_FILTER.into());
    let capture = LogCapture::default();
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer().with_target(false).fmt_fields(printed_fields()))
        .with(Capture::new(capture.clone()))
        .try_init();
    capture
}

/// A subscriber that keeps lines in `capture` and prints nothing, for a
/// test or a tool with output of its own to set as its thread's default.
pub fn capturing_only(capture: LogCapture) -> impl Subscriber + Send + Sync {
    tracing_subscriber::registry().with(EnvFilter::new(DEFAULT_FILTER)).with(Capture::new(capture))
}

/// Keeps every line, noting when, in which frame and on which thread.
struct Capture {
    lines: LogCapture,
    started: Instant,
    main: ThreadId,
}

impl Capture {
    /// Keeping lines from now, the thread making it being the main one.
    fn new(lines: LogCapture) -> Self {
        Self { lines, started: Instant::now(), main: std::thread::current().id() }
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Message::default();
        event.record(&mut fields);
        let metadata = event.metadata();
        let line = LogLine {
            level: *metadata.level(),
            message: fields.text,
            target: metadata.target().to_string(),
            file: metadata.file().map(str::to_string),
            line: metadata.line(),
            seconds: self.started.elapsed().as_secs_f64(),
            frame: FRAME.load(Ordering::Relaxed),
            worker: std::thread::current().id() != self.main,
            stack: fields.stack,
            context: fields.context,
        };
        let Ok(mut log) = self.lines.lock() else { return };
        if log.lines.len() == KEPT {
            log.lines.pop_front();
        }
        log.lines.push_back(line);
        log.total += 1;
    }
}

/// An event's message, then any other fields as `name=value`; and the
/// `stack` and `context` fields scripts' lines carry, kept apart.
#[derive(Default)]
struct Message {
    text: String,
    stack: Option<String>,
    context: Option<u64>,
}

impl tracing::field::Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.text.insert_str(0, value),
            "stack" => self.stack = Some(value.to_string()).filter(|s| !s.trim().is_empty()),
            name => {
                let _ = write!(self.text, " {name}={value}");
            }
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "context" => self.context = Some(value).filter(|&v| v != 0),
            name => {
                let _ = write!(self.text, " {name}={value}");
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let text = format!("{value:?}");
            self.text.insert_str(0, &text);
        } else {
            let _ = write!(self.text, " {}={value:?}", field.name());
        }
    }
}
