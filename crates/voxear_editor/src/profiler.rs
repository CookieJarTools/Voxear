//! The Profiler panel, the Profiling workspace's own: the last few hundred
//! frames as a chart of their CPU and GPU milliseconds, and one frame broken
//! down, as Unity's Profiler window does. The CPU side is the editor's own
//! stages, with the game's tick split into the engine's stages; the GPU side
//! is the renderer's passes, timed on the GPU. Beside them, what the frame
//! drew and culled and what is loaded. Render's CPU time includes waiting
//! for the GPU and the display, so with vsync on it fills the frame.
//!
//! A click on the chart picks a frame to break down; Live goes back to the
//! newest. Record stops and starts taking frames, so a spike can be looked
//! at while the game keeps running.

use std::{collections::VecDeque, time::Instant};

use voxear_engine::Game;
use voxear_render::{RenderStats, Renderer};
use voxear_ui::{Color, NodeId, Shape, Style, Ui};

/// How many frames the chart keeps.
const FRAMES: usize = 300;
/// The chart's top is never under this many milliseconds, so a quiet frame
/// rate does not fill it with noise.
const LEAST_SCALE: f32 = 20.0;
/// How often the live breakdown changes, in seconds, so its numbers can be
/// read.
const DETAIL_INTERVAL: f32 = 0.25;
/// The milliseconds of 60 and 30 frames a second, drawn across the chart.
const GUIDES: [(f32, &str); 2] = [(1000.0 / 60.0, "60 fps"), (1000.0 / 30.0, "30 fps")];
const CPU: u32 = 0x7988ff;
const GPU: u32 = 0x4cb782;
const GUIDE: u32 = 0x2e3036;
const MARKER: u32 = 0xc8ccd1;

/// One stage of a frame on the CPU: its name, milliseconds, and how deep it
/// is in the stage it is part of.
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub name: String,
    pub ms: f32,
    pub depth: u8,
}

/// Times a frame's stages one after another.
pub(crate) struct Laps {
    mark: Instant,
    stages: Vec<Stage>,
}

impl Laps {
    pub fn start() -> Self {
        Self { mark: Instant::now(), stages: Vec::new() }
    }

    /// Ends the stage that ran since the last mark, naming it.
    pub fn mark(&mut self, name: &str) {
        let now = Instant::now();
        let ms = (now - self.mark).as_secs_f32() * 1000.0;
        self.mark = now;
        self.stages.push(Stage { name: name.to_string(), ms, depth: 0 });
    }

    /// Stages that ran inside the one marked last, timed by what ran them.
    pub fn inside(&mut self, stages: &[(&'static str, f32)]) {
        let inner = stages.iter().map(|&(name, ms)| Stage { name: name.to_string(), ms, depth: 1 });
        self.stages.extend(inner);
    }

    pub fn finish(self) -> Vec<Stage> {
        self.stages
    }
}

/// A frame as the Profiler took it.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Counts up from the editor opening, so a picked frame stays picked as
    /// newer ones come.
    pub frame: u64,
    /// Milliseconds since the frame before, as the window saw it.
    pub frame_ms: f32,
    pub cpu: Vec<Stage>,
    /// Milliseconds per render pass, from the latest frame the GPU finished.
    pub gpu: Vec<(String, f32)>,
    pub stats: RenderStats,
}

impl Sample {
    /// Milliseconds the CPU spent, counting each stage once.
    pub fn cpu_ms(&self) -> f32 {
        self.cpu.iter().filter(|s| s.depth == 0).map(|s| s.ms).sum()
    }

    pub fn gpu_ms(&self) -> f32 {
        self.gpu.iter().map(|(_, ms)| ms).sum()
    }
}

pub struct Profiler {
    record: NodeId,
    clear: NodeId,
    live: NodeId,
    summary: NodeId,
    chart: NodeId,
    guides: Vec<NodeId>,
    picked_label: NodeId,
    columns: [NodeId; 3],
    samples: VecDeque<Sample>,
    recording: bool,
    frames: u64,
    /// The frame broken down, when not the newest.
    picked: Option<u64>,
    /// The frame the breakdown shows, and when it was last built.
    shown: Option<(u64, Instant)>,
}

impl Profiler {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let button = |ui: &mut Ui, text: &str| {
            let button = ui.button(bar, text);
            ui.add_class(button, "console-button");
            button
        };
        let (record, clear, live) = (button(ui, "Record"), button(ui, "Clear"), button(ui, "Live"));
        ui.set_checked(record, true);
        ui.set_visible(live, false);
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "profiler-summary");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let chart = ui.panel(root, Style::default());
        ui.add_class(chart, "profiler-chart");
        let guides = GUIDES
            .iter()
            .map(|(_, label)| {
                let guide = ui.add_label(chart, label);
                ui.add_class(guide, "profiler-guide");
                guide
            })
            .collect();
        let legend = ui.panel(root, Style::row());
        ui.add_class(legend, "profiler-legend");
        for (name, class) in [("CPU", "cpu"), ("GPU", "gpu")] {
            let swatch = ui.panel(legend, Style::default());
            ui.add_class(swatch, "profiler-swatch");
            ui.add_class(swatch, class);
            ui.add_label(legend, name);
        }
        let picked_label = ui.add_label(legend, "");
        ui.add_class(picked_label, "profiler-picked");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let details = ui.panel(root, Style::row());
        ui.add_class(details, "fill");
        ui.add_class(details, "profiler-details");
        let columns = ["CPU", "GPU", "Frame"].map(|title| {
            let column = ui.panel(details, Style::default());
            ui.add_class(column, "profiler-column");
            let heading = ui.add_label(column, title);
            ui.add_class(heading, "profiler-heading");
            column
        });
        Self {
            record,
            clear,
            live,
            summary,
            chart,
            guides,
            picked_label,
            columns,
            samples: VecDeque::with_capacity(FRAMES),
            recording: true,
            frames: 0,
            picked: None,
            shown: None,
        }
    }

    /// Takes a frame: its stages, as `Laps` timed them, and what the
    /// renderer says of the GPU's latest.
    pub fn record(&mut self, cpu: Vec<Stage>, frame_ms: f32, renderer: &Renderer) {
        self.frames += 1;
        if !self.recording {
            return;
        }
        if self.samples.len() == FRAMES {
            self.samples.pop_front();
        }
        let gpu = renderer.timings().to_vec();
        self.samples.push_back(Sample { frame: self.frames, frame_ms, cpu, gpu, stats: renderer.stats() });
    }

    pub fn samples(&self) -> &VecDeque<Sample> {
        &self.samples
    }

    /// The frame broken down: the one picked, or the newest.
    pub fn shown_sample(&self) -> Option<&Sample> {
        match self.picked {
            Some(frame) => self.samples.iter().find(|s| s.frame == frame),
            None => self.samples.back(),
        }
    }

    /// The panel's buttons and chart, and, while `shown`, its drawing.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, game: &Game) {
        if ui.clicked(self.record) {
            self.recording = !self.recording;
            ui.set_checked(self.record, self.recording);
        }
        let picked = self.picked;
        if ui.clicked(self.clear) {
            self.samples.clear();
            self.picked = None;
        }
        if ui.clicked(self.live) {
            self.picked = None;
        }
        if ui.clicked(self.chart) {
            self.picked = self.frame_at(ui);
        }
        if self.picked.is_some() && self.shown_sample().is_none() {
            self.picked = None;
        }
        if self.picked != picked {
            self.shown = None;
        }
        ui.set_visible(self.live, self.picked.is_some());
        if !shown {
            return;
        }
        self.draw_chart(ui);
        let Some(sample) = self.shown_sample() else { return };
        let frame = sample.frame;
        let due = self.shown.is_none_or(|(at, built)| {
            at != frame && (self.picked.is_some() || built.elapsed().as_secs_f32() >= DETAIL_INTERVAL)
        });
        if due {
            self.shown = Some((frame, Instant::now()));
            self.show_details(ui, game);
        }
    }

    /// The frame under the pointer on the chart, or the oldest kept when
    /// the pointer is left of it.
    fn frame_at(&self, ui: &Ui) -> Option<u64> {
        let (px, _) = ui.pointer_position()?;
        let [x, _, width, _] = ui.rect(self.chart);
        let step = width / (FRAMES - 1) as f32;
        let from_right = ((x + width - px) / step).round().max(0.0) as usize;
        let index = self.samples.len().saturating_sub(1 + from_right);
        self.samples.get(index).map(|s| s.frame)
    }

    /// The newest frame at the right, the oldest kept at the left, each a
    /// point on a CPU line and a GPU line, under lines at 60 and 30 frames
    /// a second when they fit.
    fn draw_chart(&mut self, ui: &mut Ui) {
        let [_, _, width, height] = ui.rect(self.chart);
        let slowest = self.samples.iter().map(|s| s.cpu_ms().max(s.gpu_ms())).fold(0.0, f32::max);
        let scale = (slowest * 1.15).max(LEAST_SCALE);
        let y = |ms: f32| height - (ms / scale).min(1.0) * (height - 4.0);
        let step = width / (FRAMES - 1) as f32;
        let x = |i: usize| width - (self.samples.len() - 1 - i) as f32 * step;
        let mut shapes = Vec::new();
        for (&(ms, _), &label) in GUIDES.iter().zip(&self.guides) {
            let fits = ms < scale;
            ui.set_visible(label, fits);
            if fits {
                let at = y(ms);
                shapes.push(Shape::Line { from: [0.0, at], to: [width, at], width: 1.0, color: Color::hex(GUIDE) });
                ui.set_inline_style(label, &format!("top: {}px", (at - 14.0).max(0.0)));
            }
        }
        let mut line = |value: fn(&Sample) -> f32, color: u32| {
            let points: Vec<[f32; 2]> = self.samples.iter().enumerate().map(|(i, s)| [x(i), y(value(s))]).collect();
            for pair in points.windows(2) {
                shapes.push(Shape::Line { from: pair[0], to: pair[1], width: 1.5, color: Color::hex(color) });
            }
        };
        line(Sample::cpu_ms, CPU);
        line(Sample::gpu_ms, GPU);
        if let Some(index) = self.picked.and_then(|frame| self.samples.iter().position(|s| s.frame == frame)) {
            let at = x(index);
            let color = Color::hex(MARKER).with_alpha(160);
            shapes.push(Shape::Line { from: [at, 0.0], to: [at, height], width: 1.0, color });
        }
        ui.set_shapes(self.chart, shapes);
    }

    /// Breaks the frame shown down: its CPU stages and GPU passes, each with
    /// a bar for its share, and what it drew and culled.
    fn show_details(&mut self, ui: &mut Ui, game: &Game) {
        let Some(sample) = self.shown_sample().cloned() else { return };
        let (cpu_ms, gpu_ms) = (sample.cpu_ms(), sample.gpu_ms());
        let fps = if sample.frame_ms > 0.0 { 1000.0 / sample.frame_ms } else { 0.0 };
        let summary =
            format!("{fps:.0} fps   {:.2} ms frame   CPU {cpu_ms:.2} ms   GPU {gpu_ms:.2} ms", sample.frame_ms);
        ui.set_text(self.summary, &summary);
        let ago = self.frames.saturating_sub(sample.frame);
        let picked =
            if self.picked.is_some() { format!("Frame {} · {ago} frames ago", sample.frame) } else { "Live".into() };
        ui.set_text(self.picked_label, &picked);

        for column in self.columns {
            for row in ui.children(column).into_iter().skip(1) {
                ui.remove(row);
            }
        }
        for stage in &sample.cpu {
            timing_row(ui, self.columns[0], &stage.name, stage.ms, cpu_ms, stage.depth);
        }
        for (pass, ms) in &sample.gpu {
            timing_row(ui, self.columns[1], pass, *ms, gpu_ms, 0);
        }
        let stats = &sample.stats;
        let usage = game.content.usage();
        let memory = resident_megabytes().map_or_else(|| "-".to_string(), |mb| format!("{mb:.0} MB"));
        let lods: Vec<String> = stats.lods.iter().take_while(|&&n| n > 0).map(u32::to_string).collect();
        let counts = [
            ("Objs drawn", format!("{} of {}", stats.drawn_early + stats.drawn_late, stats.instances)),
            ("Frustum culled", stats.frustum_culled.to_string()),
            ("Occlusion culled", stats.occlusion_culled.to_string()),
            ("Shadow draws", stats.shadow_draws.to_string()),
            ("By LOD", if lods.is_empty() { "-".into() } else { lods.join(" / ") }),
            ("LOD fades", stats.fade_draws.to_string()),
            ("Lights per cluster", stats.cluster_max_lights.to_string()),
            ("Memory", memory),
            ("Models", usage.models.to_string()),
            ("Prefabs", usage.prefabs.to_string()),
            ("Materials", usage.materials.to_string()),
        ];
        for (label, value) in counts {
            let row = ui.panel(self.columns[2], Style::row());
            ui.add_class(row, "profiler-row");
            let name = ui.add_label(row, label);
            ui.add_class(name, "profiler-name");
            let value = ui.add_label(row, &value);
            ui.add_class(value, "profiler-value");
        }
    }
}

/// A stage or pass: its name, indented by its depth, its milliseconds, and
/// a bar for its share of `total`.
fn timing_row(ui: &mut Ui, column: NodeId, name: &str, ms: f32, total: f32, depth: u8) {
    let row = ui.panel(column, Style::row());
    ui.add_class(row, "profiler-row");
    let label = ui.add_label(row, name);
    ui.add_class(label, "profiler-name");
    if depth > 0 {
        ui.set_inline_style(label, &format!("padding-left: {}px; color: var(--vx-muted)", u32::from(depth) * 12));
    }
    let track = ui.panel(row, Style::default());
    ui.add_class(track, "profiler-track");
    let fill = ui.panel(track, Style::default());
    ui.add_class(fill, "profiler-fill");
    let share = if total > 0.0 { (ms / total).clamp(0.0, 1.0) } else { 0.0 };
    ui.set_inline_style(fill, &format!("width: {:.1}%", share * 100.0));
    let value = ui.add_label(row, &format!("{ms:.2} ms"));
    ui.add_class(value, "profiler-value");
}

/// How much memory the editor holds, from Linux's /proc.
pub(crate) fn resident_megabytes() -> Option<f32> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: f32 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096.0 / 1_048_576.0)
}
