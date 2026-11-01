//! Profiling samples: named spans of a frame's work, nested as they ran, as
//! Unity's Profiler takes them. The editor's Profiler shows them as Unity's
//! Hierarchy does, samples of the same name under the same parent merged
//! into one with its total time and how many times it ran.
//!
//! Sampling is per thread, as frames are: nothing is recorded on a thread
//! until [`set_enabled`] turns sampling on there (the engine's main thread,
//! where scripts run too), and then a sample costs a clock read and a short
//! search among its siblings. [`end_frame`] hands back what the thread
//! recorded and starts its next frame.
//!
//! A worker thread, such as the one a level loads on, names itself with
//! [`register_thread`]. While sampling is on anywhere, its samples are kept
//! as runs, and the main thread's [`end_frame`] gathers into
//! [`Samples::threads`] those that ended in its frame and those still
//! running, each cut to the frame, so a load lasting seconds shows in every
//! frame it ran through. Other threads record nothing.
//!
//! Built with the `tracy` feature, every sample is also a zone in the
//! [Tracy](https://github.com/wolfpld/tracy) profiler, on every thread,
//! whether or not the editor's Profiler is recording, with a frame mark at
//! each [`end_frame`]; Tracy collects only while its viewer is connected.
//!
//! Each run of a sample is kept too, as a [`Span`] with its start and end in
//! the frame, for a timeline; and scripts' samples carry the managed memory
//! allocated in them ([`end_with_alloc`]), as Unity's GC Alloc column.
//!
//! The easy form is a scope that ends with its block:
//!
//! ```
//! voxear_core::profile_scope!("Pathfinding");
//! ```
//!
//! and the advanced one a [`Marker`] made once and begun and ended by hand,
//! which is what scripts' samples come through.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
}

/// How many threads have sampling on; worker threads keep their samples
/// while any does.
static SAMPLING: AtomicUsize = AtomicUsize::new(0);

/// A worker's run of a sample: the thread, the sample, how deep it ran, and
/// when it began and, once it has, ended.
struct WorkerRun {
    thread: String,
    marker: Marker,
    depth: u16,
    start: Instant,
    end: Option<Instant>,
}

/// Workers' runs still going, and those that ended since the main thread's
/// last frame.
fn worker_runs() -> &'static Mutex<Vec<WorkerRun>> {
    static RUNS: OnceLock<Mutex<Vec<WorkerRun>>> = OnceLock::new();
    RUNS.get_or_init(Mutex::default)
}

fn lock_runs() -> std::sync::MutexGuard<'static, Vec<WorkerRun>> {
    worker_runs().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Names this thread as a worker, whose samples the Profiler shows beside the
/// main thread's, and Tracy by this name.
pub fn register_thread(name: &str) {
    RECORDER.with_borrow_mut(|recorder| recorder.worker = Some((name.to_string(), Vec::new())));
    tracy::name_thread(name);
}

/// A sample's name, made once and kept for the life of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Marker(u32);

impl Marker {
    /// The marker named `name`, the same one each time it is asked for.
    pub fn new(name: &str) -> Self {
        let mut names = names().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&id) = names.ids.get(name) {
            return Marker(id);
        }
        let id = names.list.len() as u32;
        names.list.push(name.to_string());
        names.ids.insert(name.to_string(), id);
        Marker(id)
    }

    pub fn from_id(id: u32) -> Self {
        Marker(id)
    }

    pub fn id(self) -> u32 {
        self.0
    }

    pub fn name(self) -> String {
        let names = names().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        names.list.get(self.0 as usize).cloned().unwrap_or_default()
    }

    /// Starts a sample, which [`end`] ends. Returns whether it was recorded,
    /// as it is not while sampling is off on this thread.
    pub fn begin(self) -> bool {
        let traced = tracing_zones() && tracy::begin(self);
        let recorded = RECORDER.with_borrow_mut(|recorder| {
            if recorder.enabled {
                return recorder.begin(self);
            }
            match &mut recorder.worker {
                Some((thread, open)) if SAMPLING.load(Ordering::Relaxed) > 0 => {
                    let (start, depth) = (Instant::now(), open.len() as u16);
                    open.push((self, start));
                    lock_runs().push(WorkerRun { thread: thread.clone(), marker: self, depth, start, end: None });
                    true
                }
                _ => false,
            }
        });
        recorded || traced
    }

    /// A sample that ends when the scope it returns is dropped.
    pub fn scope(self) -> Scope {
        Scope { recorded: self.begin() }
    }
}

/// Ends the sample begun last on this thread.
pub fn end() {
    end_with_alloc(0);
}

/// Ends the sample begun last on this thread, noting that `bytes` of managed
/// memory were allocated while it ran, as scripts' samples do.
pub fn end_with_alloc(bytes: u64) {
    if tracing_zones() {
        tracy::end();
    }
    RECORDER.with_borrow_mut(|recorder| match &mut recorder.worker {
        Some((thread, open)) if !recorder.enabled => {
            let Some((marker, start)) = open.pop() else { return };
            let depth = open.len() as u16;
            let mut runs = lock_runs();
            let running = runs
                .iter_mut()
                .rev()
                .find(|run| run.end.is_none() && run.depth == depth && run.marker == marker && run.thread == *thread);
            match running {
                Some(run) => run.end = Some(Instant::now()),
                // Taken by a frame while sampling went off and on again.
                None => {
                    runs.push(WorkerRun { thread: thread.clone(), marker, depth, start, end: Some(Instant::now()) })
                }
            }
        }
        _ => recorder.end(bytes),
    });
}

/// Whether samples are Tracy zones too: built with the `tracy` feature.
fn tracing_zones() -> bool {
    cfg!(feature = "tracy")
}

/// Whether a sample begun now is kept anywhere: sampling is on on this
/// thread, or this is a build for Tracy. A caller that cannot tell what
/// `begin` answered begins and ends samples while this holds.
pub fn active() -> bool {
    tracing_zones() || enabled()
}

/// Ends its sample when dropped.
#[must_use = "the sample ends when the scope is dropped"]
pub struct Scope {
    recorded: bool,
}

impl Drop for Scope {
    fn drop(&mut self) {
        if self.recorded {
            end();
        }
    }
}

/// Samples the rest of the block it is in, under a name given once.
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        let _profile_scope = $crate::profile_marker!($name).scope();
    };
}

/// The marker named `name`, made the first time this line runs, for a
/// sample that ends before its block does: `let s = profile_marker!("Draw").scope(); ... drop(s);`.
#[macro_export]
macro_rules! profile_marker {
    ($name:expr) => {{
        static MARKER: ::std::sync::OnceLock<$crate::profiling::Marker> = ::std::sync::OnceLock::new();
        *MARKER.get_or_init(|| $crate::profiling::Marker::new($name))
    }};
}

/// Turns sampling on or off on this thread. Worker threads keep their
/// samples while any thread has it on.
pub fn set_enabled(enabled: bool) {
    let was = RECORDER.with_borrow_mut(|recorder| std::mem::replace(&mut recorder.enabled, enabled));
    match (was, enabled) {
        (false, true) => {
            SAMPLING.fetch_add(1, Ordering::Relaxed);
        }
        (true, false) if SAMPLING.fetch_sub(1, Ordering::Relaxed) == 1 => lock_runs().clear(),
        _ => {}
    }
}

/// Whether sampling is on on this thread.
pub fn enabled() -> bool {
    RECORDER.with_borrow(|recorder| recorder.enabled)
}

/// Hands back what this thread recorded since the last call, with the
/// worker threads' runs that ended meanwhile, and starts the next frame.
/// Samples still open carry on into it.
pub fn end_frame() -> Samples {
    tracy::frame_mark();
    RECORDER.with_borrow_mut(Recorder::take)
}

/// A worker thread's runs of samples in a frame, placed on the frame's own
/// timeline: a run that began before the frame starts at 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadSpans {
    pub name: String,
    pub spans: Vec<Span>,
}

/// The worker threads' runs in the frame from `frame_start` to now, by
/// thread in the order they first appear: those that ended, which go, and
/// those still running, which stay for the next frame.
fn take_worker_runs(frame_start: Instant) -> Vec<ThreadSpans> {
    let now = Instant::now();
    let mut runs = lock_runs();
    let shown: Vec<(String, Span)> = runs
        .iter()
        .map(|run| {
            let since = |at: Instant| at.saturating_duration_since(frame_start).as_nanos() as u64;
            let span = Span {
                marker: run.marker,
                depth: run.depth,
                start_ns: since(run.start),
                end_ns: since(run.end.unwrap_or(now)),
            };
            (run.thread.clone(), span)
        })
        .collect();
    runs.retain(|run| run.end.is_none());
    drop(runs);
    let mut threads: Vec<ThreadSpans> = Vec::new();
    for (name, span) in shown {
        match threads.iter_mut().find(|t| t.name == name) {
            Some(thread) => thread.spans.push(span),
            None => threads.push(ThreadSpans { name, spans: vec![span] }),
        }
    }
    for thread in &mut threads {
        thread.spans.sort_by_key(|span| (span.start_ns, span.depth));
    }
    threads
}

/// One merged sample: every run of a name under the same parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub marker: Marker,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub total_ns: u64,
    pub calls: u32,
    /// Managed bytes allocated in it, everything in it included, when a
    /// script said; see [`Samples::alloc_bytes`].
    pub alloc_bytes: Option<u64>,
}

/// One run of a sample: when it started and ended, in nanoseconds from the
/// frame's start, and how many samples were open around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub marker: Marker,
    pub depth: u16,
    pub start_ns: u64,
    pub end_ns: u64,
}

/// A frame's samples as a tree. Node 0 is the frame itself, holding the
/// samples nothing else was open around.
#[derive(Clone, Debug, PartialEq)]
pub struct Samples {
    pub nodes: Vec<Node>,
    /// Every run, in the order they started, for a timeline.
    pub spans: Vec<Span>,
    /// Worker threads' runs that ended in the frame, a lane each.
    pub threads: Vec<ThreadSpans>,
}

impl Default for Samples {
    fn default() -> Self {
        let frame = Node {
            marker: Marker(u32::MAX),
            parent: None,
            children: Vec::new(),
            total_ns: 0,
            calls: 0,
            alloc_bytes: None,
        };
        Self { nodes: vec![frame], spans: Vec::new(), threads: Vec::new() }
    }
}

impl Samples {
    pub const FRAME: usize = 0;

    /// Milliseconds the sample and everything in it took.
    pub fn total_ms(&self, node: usize) -> f32 {
        if node == Self::FRAME {
            return self.nodes[node].children.iter().map(|&child| self.total_ms(child)).sum();
        }
        self.nodes[node].total_ns as f32 / 1_000_000.0
    }

    /// Milliseconds the sample took outside the samples in it.
    pub fn self_ms(&self, node: usize) -> f32 {
        let inner: f32 = self.nodes[node].children.iter().map(|&child| self.total_ms(child)).sum();
        (self.total_ms(node) - inner).max(0.0)
    }

    pub fn name(&self, node: usize) -> String {
        self.nodes[node].marker.name()
    }

    /// The sample named `name` anywhere in the frame, the first found.
    pub fn find(&self, name: &str) -> Option<usize> {
        (1..self.nodes.len()).find(|&node| self.name(node) == name)
    }

    /// Managed bytes allocated in the sample and everything in it: what a
    /// script said, or else what the samples in it add up to.
    pub fn alloc_bytes(&self, node: usize) -> u64 {
        match self.nodes[node].alloc_bytes {
            Some(bytes) => bytes,
            None => self.nodes[node].children.iter().map(|&child| self.alloc_bytes(child)).sum(),
        }
    }
}

#[derive(Default)]
struct Recorder {
    enabled: bool,
    /// A worker thread's name and its samples still open, each with when it
    /// began.
    worker: Option<(String, Vec<(Marker, Instant)>)>,
    samples: Samples,
    /// Each open sample's node, when it began, and its span.
    open: Vec<(usize, Instant, usize)>,
    /// When the frame being recorded began: at the first sample, or when
    /// the last frame was taken.
    frame_start: Option<Instant>,
}

impl Recorder {
    fn begin(&mut self, marker: Marker) -> bool {
        let now = Instant::now();
        let parent = self.open.last().map_or(Samples::FRAME, |&(node, _, _)| node);
        let nodes = &mut self.samples.nodes;
        let found = nodes[parent].children.iter().copied().find(|&child| nodes[child].marker == marker);
        let node = found.unwrap_or_else(|| {
            let fresh =
                Node { marker, parent: Some(parent), children: Vec::new(), total_ns: 0, calls: 0, alloc_bytes: None };
            nodes.push(fresh);
            let node = nodes.len() - 1;
            nodes[parent].children.push(node);
            node
        });
        let start_ns = (now - *self.frame_start.get_or_insert(now)).as_nanos() as u64;
        let depth = self.open.len() as u16;
        self.samples.spans.push(Span { marker, depth, start_ns, end_ns: start_ns });
        self.open.push((node, now, self.samples.spans.len() - 1));
        true
    }

    fn end(&mut self, alloc_bytes: u64) {
        let Some((node, started, span)) = self.open.pop() else { return };
        let now = Instant::now();
        if let Some(span) = self.samples.spans.get_mut(span) {
            span.end_ns = span.start_ns + (now - started).as_nanos() as u64;
        }
        let Some(node) = self.samples.nodes.get_mut(node) else { return };
        node.total_ns += (now - started).as_nanos() as u64;
        node.calls += 1;
        if alloc_bytes > 0 || node.alloc_bytes.is_some() {
            *node.alloc_bytes.get_or_insert(0) += alloc_bytes;
        }
    }

    /// The frame's samples, leaving those still open to carry on in the next.
    fn take(&mut self) -> Samples {
        let now = Instant::now();
        let mut finished = std::mem::take(&mut self.samples);
        // Only a thread that is sampling takes the workers' runs, so one that is not leaves them be.
        if self.enabled {
            finished.threads = take_worker_runs(self.frame_start.unwrap_or(now));
        }
        // A sample still open is counted up to now in the frame taken, and starts again in the next.
        let mut reopened = Vec::new();
        for &(node, started, span) in &self.open {
            finished.nodes[node].total_ns += (now - started).as_nanos() as u64;
            finished.nodes[node].calls += 1;
            if let Some(span) = finished.spans.get_mut(span) {
                span.end_ns = span.start_ns + (now - started).as_nanos() as u64;
            }
            reopened.push(finished.nodes[node].marker);
        }
        self.open.clear();
        self.frame_start = Some(now);
        for marker in reopened {
            self.begin(marker);
        }
        finished
    }
}

#[derive(Default)]
struct Names {
    list: Vec<String>,
    ids: HashMap<String, u32>,
}

fn names() -> &'static Mutex<Names> {
    static NAMES: OnceLock<Mutex<Names>> = OnceLock::new();
    NAMES.get_or_init(Mutex::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Worker threads' sampling is shared, so the tests take turns.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn spin(micros: u64) {
        let started = Instant::now();
        while started.elapsed().as_micros() < micros as u128 {}
    }

    #[test]
    fn samples_nest_and_merge_by_name_under_their_parent() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        set_enabled(true);
        end_frame();
        {
            crate::profile_scope!("Update");
            for _ in 0..3 {
                let _spin = Marker::new("Spinner.Update()").scope();
                spin(200);
            }
            let _turn = Marker::new("Turner.Update()").scope();
            spin(100);
        }
        let samples = end_frame();
        set_enabled(false);

        let update = samples.find("Update").unwrap();
        assert_eq!(samples.nodes[Samples::FRAME].children, [update]);
        let children: Vec<String> = samples.nodes[update].children.iter().map(|&c| samples.name(c)).collect();
        assert_eq!(children, ["Spinner.Update()", "Turner.Update()"], "runs of one name merge");
        let spinner = samples.find("Spinner.Update()").unwrap();
        assert_eq!(samples.nodes[spinner].calls, 3);
        assert!(samples.total_ms(spinner) >= 0.6);
        let turner = samples.find("Turner.Update()").unwrap();
        assert!(samples.total_ms(update) >= samples.total_ms(spinner) + samples.total_ms(turner));
        assert!(samples.self_ms(update) < samples.total_ms(update));
        // Tracy takes every sample, but the Profiler none while sampling is off.
        assert_eq!(Marker::new("Off").scope().recorded, cfg!(feature = "tracy"), "begun only for Tracy");

        // Each run is a span, in the order they started, at its depth.
        let runs: Vec<(String, u16)> = samples.spans.iter().map(|s| (s.marker.name(), s.depth)).collect();
        assert_eq!(runs[0], ("Update".to_string(), 0));
        assert_eq!(runs.iter().filter(|(name, depth)| name == "Spinner.Update()" && *depth == 1).count(), 3);
        let spans = &samples.spans;
        assert!(spans.windows(2).all(|pair| pair[0].start_ns <= pair[1].start_ns));
        assert!(spans[1..].iter().all(|span| span.start_ns >= spans[0].start_ns && span.end_ns <= spans[0].end_ns));
        assert!(spans[1].end_ns - spans[1].start_ns >= 200_000, "a span lasts as long as its run");
    }

    #[test]
    fn scripts_samples_carry_what_they_allocated() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        set_enabled(true);
        end_frame();
        {
            crate::profile_scope!("Update");
            Marker::new("Spawner.Update()").begin();
            end_with_alloc(1024);
            Marker::new("Spawner.Update()").begin();
            end_with_alloc(512);
            let _quiet = Marker::new("Mover.Update()").scope();
        }
        let samples = end_frame();
        set_enabled(false);
        let spawner = samples.find("Spawner.Update()").unwrap();
        assert_eq!(samples.alloc_bytes(spawner), 1536);
        assert_eq!(samples.alloc_bytes(samples.find("Update").unwrap()), 1536, "a parent adds up what is in it");
        assert_eq!(samples.alloc_bytes(samples.find("Mover.Update()").unwrap()), 0);
    }

    #[test]
    fn a_worker_threads_samples_join_the_main_threads_frame() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        set_enabled(true);
        end_frame();
        std::thread::spawn(|| {
            register_thread("Loading");
            let _read = Marker::new("Read files").scope();
            spin(200);
            let _unpack = Marker::new("Decompress").scope();
            spin(100);
        })
        .join()
        .unwrap();
        std::thread::spawn(|| {
            let _work = Marker::new("Unnamed work").scope();
            spin(50);
        })
        .join()
        .unwrap();
        let samples = end_frame();
        set_enabled(false);

        assert_eq!(samples.threads.len(), 1, "only a registered thread is sampled");
        let loading = &samples.threads[0];
        assert_eq!(loading.name, "Loading");
        let runs: Vec<(String, u16)> = loading.spans.iter().map(|s| (s.marker.name(), s.depth)).collect();
        assert_eq!(runs, [("Read files".to_string(), 0), ("Decompress".to_string(), 1)]);
        let (outer, inner) = (loading.spans[0], loading.spans[1]);
        assert!(outer.start_ns <= inner.start_ns && inner.end_ns <= outer.end_ns, "nested in time too");
        assert!(outer.end_ns - outer.start_ns >= 300_000, "on the frame's clock");
        assert!(end_frame().threads.is_empty(), "taken once");

        // A run still going shows in each frame it runs through, cut to the frame.
        set_enabled(true);
        let (started, finish) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
        let (tell, wait) = (started.0, finish.1);
        let worker = std::thread::spawn(move || {
            register_thread("Streaming");
            let _stream = Marker::new("Stream").scope();
            tell.send(()).unwrap();
            wait.recv().unwrap();
        });
        started.1.recv().unwrap();
        end_frame();
        spin(100);
        let during = end_frame();
        finish.0.send(()).unwrap();
        worker.join().unwrap();
        let after = end_frame();
        set_enabled(false);
        let stream = |samples: &Samples| samples.threads.iter().find(|t| t.name == "Streaming").map(|t| t.spans[0]);
        let during = stream(&during).expect("shown while it runs");
        assert_eq!(during.start_ns, 0, "from the frame's start");
        assert!(during.end_ns >= 100_000, "to the frame's end");
        assert!(stream(&after).is_some(), "and in the frame it ends in");
    }
}

/// Tracy's zones, one for each sample begun, kept per thread so a sample's
/// zone ends with it. The client starts with the first zone; it collects
/// only while Tracy's viewer is connected, on this machine.
#[cfg(feature = "tracy")]
mod tracy {
    use std::cell::RefCell;

    use super::Marker;

    thread_local! {
        static ZONES: RefCell<Vec<tracy_client::Span>> = const { RefCell::new(Vec::new()) };
    }

    pub fn begin(marker: Marker) -> bool {
        let zone = tracy_client::Client::start().span_alloc(Some(&marker.name()), "", "", 0, 0);
        ZONES.with_borrow_mut(|zones| zones.push(zone));
        true
    }

    pub fn end() {
        ZONES.with_borrow_mut(|zones| drop(zones.pop()));
    }

    pub fn name_thread(name: &str) {
        tracy_client::Client::start().set_thread_name(name);
    }

    pub fn frame_mark() {
        tracy_client::Client::start().frame_mark();
    }
}

/// Without the `tracy` feature, nothing.
#[cfg(not(feature = "tracy"))]
mod tracy {
    use super::Marker;

    pub fn begin(_: Marker) -> bool {
        false
    }

    pub fn end() {}

    pub fn name_thread(_: &str) {}

    pub fn frame_mark() {}
}
