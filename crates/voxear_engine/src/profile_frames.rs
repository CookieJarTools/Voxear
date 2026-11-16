//! Profiled frames as tools read them: what the CPU sampled in each frame and
//! what each of the GPU's passes took, as JSON. A running game's link and the
//! editor's answer `profiler.frames` with the latest ones, and `vox render
//! --profile-out` saves a render's in the same shape, so one reader shows a
//! game, the editor and a saved render alike.
//!
//! A frame is
//!
//! ```text
//! { frame, frame_ms, cpu_ms, gpu_ms,
//!   samples: [{ name, depth, ms, self_ms, calls, alloc? }],
//!   gpu: [{ pass, ms }] }
//! ```
//!
//! with its samples in the order of their tree, each straight after the one it
//! ran in and one deeper, as the Profiler's Hierarchy shows them. `cpu_ms`
//! leaves out waiting for the GPU and the display, which fill a frame with
//! vsync on. A pass that runs several times, as Bloom's do, is listed each
//! time, in the frame's order.
//!
//! A saved file is `{ voxear_profile: FORMAT, source, frames: [...] }` and
//! whatever its source adds, such as the project and the GPU's name.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use voxear_core::profiling::Samples;

/// The version of a saved profile's shape.
pub const FORMAT: u32 = 1;
/// Samples that are waiting on the GPU or the display rather than working.
pub const WAITS: [&str; 2] = ["Wait for GPU", "Wait for Display"];
/// How long frames go on being sampled after a tool last read them.
const WATCHED: Duration = Duration::from_secs(3);
/// The most frames kept for tools: ten seconds at 60 frames a second.
const KEPT: usize = 600;
/// The frames a read gives back when it does not say.
pub const READ: usize = 120;

/// Milliseconds to the microsecond, so a frame's JSON stays short.
fn rounded(ms: f32) -> f64 {
    (f64::from(ms) * 1000.0).round() / 1000.0
}

/// A frame as tools read it: numbered `frame`, `frame_ms` long as the window
/// saw it, with the CPU's samples and the GPU's passes.
pub fn frame_json(frame: u64, frame_ms: f32, cpu: &Samples, gpu: &[(String, f32)]) -> Value {
    fn walk(cpu: &Samples, node: usize, depth: usize, out: &mut Vec<Value>) {
        for &child in &cpu.nodes[node].children {
            let mut sample = json!({
                "name": cpu.name(child),
                "depth": depth,
                "ms": rounded(cpu.total_ms(child)),
                "self_ms": rounded(cpu.self_ms(child)),
                "calls": cpu.nodes[child].calls,
            });
            if cpu.nodes[child].alloc_bytes.is_some() {
                sample["alloc"] = json!(cpu.alloc_bytes(child));
            }
            out.push(sample);
            walk(cpu, child, depth + 1, out);
        }
    }
    let mut samples = Vec::new();
    walk(cpu, Samples::FRAME, 0, &mut samples);
    let waited: f32 =
        (1..cpu.nodes.len()).filter(|&node| WAITS.contains(&cpu.name(node).as_str())).map(|n| cpu.total_ms(n)).sum();
    let cpu_ms = (cpu.total_ms(Samples::FRAME) - waited).max(0.0);
    let gpu_ms: f32 = gpu.iter().map(|(_, ms)| ms).sum();
    json!({
        "frame": frame,
        "frame_ms": rounded(frame_ms),
        "cpu_ms": rounded(cpu_ms),
        "gpu_ms": rounded(gpu_ms),
        "samples": samples,
        "gpu": gpu.iter().map(|(pass, ms)| json!({ "pass": pass, "ms": rounded(*ms) })).collect::<Vec<_>>(),
    })
}

/// The latest frames, kept for tools while one reads them. Sampling costs a
/// little, so a game samples only while a tool has read in the last few
/// seconds ([`FrameLog::watched`]).
#[derive(Default)]
pub struct FrameLog {
    frames: VecDeque<Value>,
    /// The number the next frame read should start from.
    next: u64,
    watched_until: Option<Instant>,
}

impl FrameLog {
    /// Whether a tool has read frames lately, so frames should be sampled.
    pub fn watched(&self) -> bool {
        self.watched_until.is_some_and(|until| Instant::now() < until)
    }

    /// Keeps a frame, numbered `frame`, letting the oldest go.
    pub fn push(&mut self, frame: u64, frame_ms: f32, cpu: &Samples, gpu: &[(String, f32)]) {
        if self.frames.len() == KEPT {
            self.frames.pop_front();
        }
        self.frames.push_back(frame_json(frame, frame_ms, cpu, gpu));
        self.next = frame + 1;
    }

    /// The frames from `after` on (a number an earlier read gave back as
    /// `next`), the latest `limit` of them, and goes on sampling a while.
    pub fn read(&mut self, after: Option<u64>, limit: usize) -> Value {
        self.watched_until = Some(Instant::now() + WATCHED);
        read_frames(self.frames.iter().cloned(), after, limit, self.next)
    }
}

/// The frames of `frames` from `after` on, the latest `limit`, as `profiler.frames` answers.
pub fn read_frames(frames: impl Iterator<Item = Value>, after: Option<u64>, limit: usize, next: u64) -> Value {
    let frames: Vec<Value> =
        frames.filter(|frame| after.is_none_or(|after| frame["frame"].as_u64().is_some_and(|n| n >= after))).collect();
    let skip = frames.len().saturating_sub(limit.max(1));
    json!({ "frames": frames[skip..], "next": next })
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::profiling::{self, Marker};

    /// Samples the main thread's frame the way the engine's systems do.
    fn sampled_frame() -> Samples {
        profiling::set_enabled(true);
        {
            let _game = Marker::new("Game").scope();
            for _ in 0..2 {
                let _update = Marker::new("Update").scope();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        {
            let _wait = Marker::new("Wait for GPU").scope();
            std::thread::sleep(Duration::from_millis(2));
        }
        let samples = profiling::end_frame();
        profiling::set_enabled(false);
        samples
    }

    #[test]
    fn a_frame_lists_its_samples_in_tree_order_with_their_depth_and_its_passes() {
        let gpu =
            vec![("Shadows".to_string(), 0.5), ("Bloom down".to_string(), 0.125), ("Bloom down".to_string(), 0.25)];
        let frame = frame_json(7, 16.6667, &sampled_frame(), &gpu);
        assert_eq!(frame["frame"], 7);
        assert_eq!(frame["frame_ms"], 16.667, "rounded to the microsecond");
        let samples = frame["samples"].as_array().unwrap();
        let names: Vec<(&str, u64)> =
            samples.iter().map(|s| (s["name"].as_str().unwrap(), s["depth"].as_u64().unwrap())).collect();
        assert_eq!(names, [("Game", 0), ("Update", 1), ("Wait for GPU", 0)]);
        assert_eq!(samples[1]["calls"], 2, "runs of a name under one parent are merged");
        let game = samples[0]["ms"].as_f64().unwrap();
        assert!(game >= 2.0 && samples[0]["self_ms"].as_f64().unwrap() < game);
        let cpu = frame["cpu_ms"].as_f64().unwrap();
        assert!((cpu - game).abs() < 0.01, "waiting for the GPU is left out of the CPU's time: {cpu} vs {game}");
        let passes: Vec<&str> = frame["gpu"].as_array().unwrap().iter().map(|p| p["pass"].as_str().unwrap()).collect();
        assert_eq!(passes, ["Shadows", "Bloom down", "Bloom down"], "a pass run twice is listed twice");
        assert_eq!(frame["gpu_ms"], 0.875);
    }

    #[test]
    fn frames_are_read_from_where_the_last_read_stopped_and_only_while_watched() {
        let mut log = FrameLog::default();
        assert!(!log.watched(), "nothing samples before a tool asks");
        let empty = Samples::default();
        for frame in 10..15 {
            log.push(frame, 16.0, &empty, &[]);
        }
        let first = log.read(None, 3);
        assert!(log.watched(), "a read keeps frames sampled");
        let numbers = |read: &Value| -> Vec<u64> {
            read["frames"].as_array().unwrap().iter().map(|f| f["frame"].as_u64().unwrap()).collect()
        };
        assert_eq!(numbers(&first), [12, 13, 14], "the latest of them");
        assert_eq!(first["next"], 15);
        assert!(numbers(&log.read(Some(15), READ)).is_empty(), "nothing new yet");
        log.push(15, 16.0, &empty, &[]);
        assert_eq!(numbers(&log.read(Some(15), READ)), [15]);

        let kept = (0..KEPT as u64 + 5).map(|frame| frame_json(frame, 16.0, &empty, &[]));
        let read = read_frames(kept, Some(KEPT as u64), READ, KEPT as u64 + 5);
        assert_eq!(numbers(&read), (KEPT as u64..KEPT as u64 + 5).collect::<Vec<_>>());
    }
}
