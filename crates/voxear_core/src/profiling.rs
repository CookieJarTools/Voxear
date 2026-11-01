//! Profiling samples: named spans of a frame's work, nested as they ran, as
//! Unity's Profiler takes them. The editor's Profiler shows them as Unity's
//! Hierarchy does, samples of the same name under the same parent merged
//! into one with its total time and how many times it ran.
//!
//! Sampling is per thread, as frames are: nothing is recorded on a thread
//! until [`set_enabled`] turns sampling on there (the engine's main thread,
//! where scripts run too), and then a sample costs a clock read and a short
//! search among its siblings. [`end_frame`] hands back what the thread
//! recorded and starts its next frame. Other threads record nothing.
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
    sync::{Mutex, OnceLock},
    time::Instant,
};

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
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
        RECORDER.with_borrow_mut(|recorder| recorder.enabled && recorder.begin(self))
    }

    /// A sample that ends when the scope it returns is dropped.
    pub fn scope(self) -> Scope {
        Scope { recorded: self.begin() }
    }
}

/// Ends the sample begun last on this thread.
pub fn end() {
    RECORDER.with_borrow_mut(|recorder| recorder.end(0));
}

/// Ends the sample begun last on this thread, noting that `bytes` of managed
/// memory were allocated while it ran, as scripts' samples do.
pub fn end_with_alloc(bytes: u64) {
    RECORDER.with_borrow_mut(|recorder| recorder.end(bytes));
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

/// Turns sampling on or off on this thread.
pub fn set_enabled(enabled: bool) {
    RECORDER.with_borrow_mut(|recorder| recorder.enabled = enabled);
}

/// Whether sampling is on on this thread.
pub fn enabled() -> bool {
    RECORDER.with_borrow(|recorder| recorder.enabled)
}

/// Hands back what this thread recorded since the last call, and starts the
/// next frame. Samples still open carry on into it.
pub fn end_frame() -> Samples {
    RECORDER.with_borrow_mut(Recorder::take)
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
        Self { nodes: vec![frame], spans: Vec::new() }
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

    fn spin(micros: u64) {
        let started = Instant::now();
        while started.elapsed().as_micros() < micros as u128 {}
    }

    #[test]
    fn samples_nest_and_merge_by_name_under_their_parent() {
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
        assert!(!Marker::new("Off").scope().recorded, "nothing is recorded while sampling is off");

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
}
