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
    RECORDER.with_borrow_mut(Recorder::end);
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
}

/// A frame's samples as a tree. Node 0 is the frame itself, holding the
/// samples nothing else was open around.
#[derive(Clone, Debug, PartialEq)]
pub struct Samples {
    pub nodes: Vec<Node>,
}

impl Default for Samples {
    fn default() -> Self {
        let frame = Node { marker: Marker(u32::MAX), parent: None, children: Vec::new(), total_ns: 0, calls: 0 };
        Self { nodes: vec![frame] }
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
}

#[derive(Default)]
struct Recorder {
    enabled: bool,
    samples: Samples,
    open: Vec<(usize, Instant)>,
}

impl Recorder {
    fn begin(&mut self, marker: Marker) -> bool {
        let parent = self.open.last().map_or(Samples::FRAME, |&(node, _)| node);
        let nodes = &mut self.samples.nodes;
        let found = nodes[parent].children.iter().copied().find(|&child| nodes[child].marker == marker);
        let node = found.unwrap_or_else(|| {
            nodes.push(Node { marker, parent: Some(parent), children: Vec::new(), total_ns: 0, calls: 0 });
            let node = nodes.len() - 1;
            nodes[parent].children.push(node);
            node
        });
        self.open.push((node, Instant::now()));
        true
    }

    fn end(&mut self) {
        let Some((node, started)) = self.open.pop() else { return };
        let Some(node) = self.samples.nodes.get_mut(node) else { return };
        node.total_ns += started.elapsed().as_nanos() as u64;
        node.calls += 1;
    }

    /// The frame's samples, leaving those still open to carry on in the next.
    fn take(&mut self) -> Samples {
        let now = Instant::now();
        let taken = std::mem::take(&mut self.samples);
        // A sample still open is counted up to now in the frame taken, and starts again in the next.
        let mut reopened = Vec::new();
        let mut finished = taken;
        for &(node, started) in &self.open {
            finished.nodes[node].total_ns += (now - started).as_nanos() as u64;
            finished.nodes[node].calls += 1;
            reopened.push(finished.nodes[node].marker);
        }
        self.open.clear();
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
    }
}
