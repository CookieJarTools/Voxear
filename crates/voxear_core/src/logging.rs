use std::{
    collections::VecDeque,
    fmt::Write,
    sync::{Arc, Mutex},
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
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::new(filter)).with_target(false).try_init();
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

#[derive(Clone, Debug)]
pub struct LogLine {
    pub level: Level,
    pub message: String,
}

/// Installs the global log subscriber as [`init`] does, and also keeps every
/// line for the editor's Console to show.
pub fn init_capturing() -> LogCapture {
    let filter =
        std::env::var("VOXEAR_LOG").or_else(|_| std::env::var("RUST_LOG")).unwrap_or_else(|_| DEFAULT_FILTER.into());
    let capture = LogCapture::default();
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(Capture(capture.clone()))
        .try_init();
    capture
}

struct Capture(LogCapture);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut message = Message(String::new());
        event.record(&mut message);
        let Ok(mut log) = self.0.lock() else { return };
        if log.lines.len() == KEPT {
            log.lines.pop_front();
        }
        log.lines.push_back(LogLine { level: *event.metadata().level(), message: message.0 });
        log.total += 1;
    }
}

/// An event's message, then any other fields as `name=value`.
struct Message(String);

impl tracing::field::Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.insert_str(0, value);
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let text = format!("{value:?}");
            self.0.insert_str(0, &text);
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}
