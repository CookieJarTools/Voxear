//! The Profiler panel, the Profiling workspace's own, after Unity's: the
//! last few hundred frames as a chart of their CPU and GPU milliseconds,
//! and one frame broken down. Its **Hierarchy** is the frame's samples (see
//! `voxear_core::profiling`) as a tree: the editor's stages, the game's tick
//! in the engine's steps, each system, and each behaviour's callbacks by
//! class and method ("Hello.Spinner.Update()"), with what scripts sample
//! themselves (`Voxear.Profiler`). Each row has its share of the frame with
//! and without what is in it, how many times it ran, and its milliseconds,
//! the slowest first. Beside it are the GPU's passes, timed on the GPU, and
//! what the frame drew and culled and what is loaded.
//!
//! The **Timeline** shows the same frame as it ran: each run of each sample
//! as a bar from when it started to when it ended, under what it ran in,
//! with the one under the pointer named above. Scripts' samples have the
//! managed memory they allocated (GC Alloc), as Unity's do.
//!
//! Samples are taken only while the panel shows and Record is on. A click on
//! the chart picks a frame to break down; Live goes back to the newest.
//! Render's time includes waiting for the GPU and the display ("Wait for
//! GPU", "Acquire Image"), so with vsync on it fills the frame.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    time::Instant,
};

use voxear_core::profiling::{self, Samples};
use voxear_engine::Game;
use voxear_render::{RenderStats, Renderer};
use voxear_ui::{Color, NodeId, Shape, Style, TreeItems, Ui};

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
/// The Hierarchy's columns after the sample's name, as its header names them.
const COLUMNS: [&str; 6] = ["Total", "Self", "Calls", "GC Alloc", "Time ms", "Self ms"];
/// A timeline row's height, and the least width a bar has a name in.
const LANE: f32 = 18.0;
const NAMED: f32 = 56.0;
/// Bar colours: the engine's own work, scripts, rendering, and waiting.
const ENGINE: u32 = 0x4f63d2;
const SCRIPT: u32 = 0x8a5cf0;
const RENDER: u32 = 0x2f9a68;
const WAIT: u32 = 0x3b3e46;
/// Samples that are the renderer's, and those that are waiting on the GPU or display.
const RENDERING: [&str; 5] = ["Render", "Extract", "Record Passes", "Submit", "UI"];
const WAITING: [&str; 2] = ["Wait for GPU", "Acquire Image"];
/// Samples opened by themselves the first time they show, as Unity opens the
/// player loop, so scripts' callbacks show at once.
const OPENED: [&str; 7] = ["Game", "Editor", "Render", "FixedUpdate", "Update", "LateUpdate", "Lifecycle"];
const ROW: f32 = 22.0;
const CPU: u32 = 0x7988ff;
const GPU: u32 = 0x4cb782;
const GUIDE: u32 = 0x2e3036;
const MARKER: u32 = 0xc8ccd1;

/// A frame as the Profiler took it.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Counts up from the editor opening, so a picked frame stays picked as
    /// newer ones come.
    pub frame: u64,
    /// Milliseconds since the frame before, as the window saw it.
    pub frame_ms: f32,
    /// What ran on the main thread, as a tree of samples.
    pub cpu: Samples,
    /// Milliseconds per render pass, from the latest frame the GPU finished.
    pub gpu: Vec<(String, f32)>,
    pub stats: RenderStats,
}

impl Sample {
    pub fn cpu_ms(&self) -> f32 {
        self.cpu.total_ms(Samples::FRAME)
    }

    pub fn gpu_ms(&self) -> f32 {
        self.gpu.iter().map(|(_, ms)| ms).sum()
    }
}

pub struct Profiler {
    record: NodeId,
    /// Hierarchy or Timeline.
    view: NodeId,
    hierarchy: NodeId,
    timeline: NodeId,
    timeline_canvas: NodeId,
    timeline_hovered: NodeId,
    /// Names on the timeline's bars, kept and reused.
    timeline_names: Vec<NodeId>,
    /// The run under the pointer, as its index in the frame's spans.
    hovered_span: Option<usize>,
    /// How wide the timeline was drawn, to draw it again when that changes,
    /// as when it first shows.
    timeline_width: f32,
    clear: NodeId,
    live: NodeId,
    summary: NodeId,
    chart: NodeId,
    guides: Vec<NodeId>,
    picked_label: NodeId,
    tree: NodeId,
    calls: Rc<RefCell<CallTree>>,
    /// The GPU's passes and the frame's counts, beside the Hierarchy.
    side: [NodeId; 2],
    samples: VecDeque<Sample>,
    recording: bool,
    /// Whether samples were being taken this frame, so it is kept.
    sampling: bool,
    frames: u64,
    /// The frame broken down, when not the newest.
    picked: Option<u64>,
    /// The frame the breakdown shows, and when it was last built.
    shown: Option<(u64, Instant)>,
    /// Rows already opened by themselves once, so one closed stays closed.
    opened: HashSet<u64>,
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
        let view = ui.segmented(bar);
        ui.add_class(view, "profiler-view");
        ui.add_segment(view, "Hierarchy", None);
        ui.add_segment(view, "Timeline", None);
        ui.set_segment(view, 0);
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "profiler-summary");
        divider(ui, root);

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
        divider(ui, root);

        let lower = ui.panel(root, Style::row());
        ui.add_class(lower, "fill");
        ui.add_class(lower, "profiler-lower");
        let hierarchy = ui.panel(lower, Style::default());
        ui.add_class(hierarchy, "profiler-hierarchy");
        let header = ui.panel(hierarchy, Style::row());
        ui.add_class(header, "profiler-header");
        let title = ui.add_label(header, "Hierarchy");
        ui.add_class(title, "profiler-cell-name");
        for column in COLUMNS {
            let cell = ui.add_label(header, column);
            ui.add_class(cell, "profiler-cell");
        }
        let calls: Rc<RefCell<CallTree>> = Rc::default();
        let tree = ui.tree_view(hierarchy, ROW, CallItems(calls.clone()));
        ui.add_class(tree, "fill");
        ui.add_class(tree, "profiler-tree");
        let timeline = ui.panel(lower, Style::default());
        ui.add_class(timeline, "profiler-hierarchy");
        ui.add_class(timeline, "profiler-timeline");
        let timeline_hovered = ui.add_label(timeline, "Point at a bar to see what it is.");
        ui.add_class(timeline_hovered, "profiler-hovered");
        let timeline_canvas = ui.panel(timeline, Style::default());
        ui.add_class(timeline_canvas, "fill");
        ui.add_class(timeline_canvas, "profiler-canvas");
        ui.set_visible(timeline, false);
        let side = ui.panel(lower, Style::default());
        ui.add_class(side, "profiler-side");
        let side = ["GPU", "Frame"].map(|title| {
            let column = ui.panel(side, Style::default());
            ui.add_class(column, "profiler-column");
            let heading = ui.add_label(column, title);
            ui.add_class(heading, "profiler-heading");
            column
        });
        Self {
            record,
            view,
            hierarchy,
            timeline,
            timeline_canvas,
            timeline_hovered,
            timeline_names: Vec::new(),
            hovered_span: None,
            timeline_width: 0.0,
            clear,
            live,
            summary,
            chart,
            guides,
            picked_label,
            tree,
            calls,
            side,
            samples: VecDeque::with_capacity(FRAMES),
            recording: true,
            sampling: false,
            frames: 0,
            picked: None,
            shown: None,
            opened: HashSet::new(),
        }
    }

    /// Takes a frame: what was sampled on the main thread, and what the
    /// renderer says of the GPU's latest. Frames taken while samples were not
    /// are let go.
    pub fn record(&mut self, cpu: Samples, frame_ms: f32, renderer: &Renderer) {
        self.frames += 1;
        if !self.sampling {
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
    /// Samples are taken only while it shows and records.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, game: &Game) {
        if ui.clicked(self.record) {
            self.recording = !self.recording;
            ui.set_checked(self.record, self.recording);
        }
        self.sampling = shown && self.recording;
        profiling::set_enabled(self.sampling);
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
        let timeline = ui.segment(self.view) == 1;
        if timeline != ui.is_visible(self.timeline) {
            ui.set_visible(self.hierarchy, !timeline);
            ui.set_visible(self.timeline, timeline);
            self.shown = None;
        }
        self.draw_chart(ui);
        if ui.is_visible(self.timeline) {
            self.hover_timeline(ui);
        }
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

    /// Breaks the frame shown down: its samples in the Hierarchy, its GPU
    /// passes, each with a bar for its share, and what it drew and culled.
    fn show_details(&mut self, ui: &mut Ui, game: &Game) {
        let Some(sample) = self.shown_sample().cloned() else { return };
        let (cpu_ms, gpu_ms) = (sample.cpu_ms(), sample.gpu_ms());
        let fps = if sample.frame_ms > 0.0 { 1000.0 / sample.frame_ms } else { 0.0 };
        let allocated = bytes(sample.cpu.alloc_bytes(Samples::FRAME));
        let summary = format!(
            "{fps:.0} fps   {:.2} ms frame   CPU {cpu_ms:.2} ms   GPU {gpu_ms:.2} ms   GC {allocated}",
            sample.frame_ms
        );
        ui.set_text(self.summary, &summary);
        let ago = self.frames.saturating_sub(sample.frame);
        let picked =
            if self.picked.is_some() { format!("Frame {} · {ago} frames ago", sample.frame) } else { "Live".into() };
        ui.set_text(self.picked_label, &picked);

        let items = self.calls.borrow_mut().set(sample.cpu.clone());
        for (item, name) in items {
            if self.opened.insert(item) && OPENED.contains(&name.as_str()) {
                ui.set_expanded(self.tree, item, true);
            }
        }
        ui.refresh_list(self.tree);
        if ui.is_visible(self.timeline) {
            self.draw_timeline(ui, &sample.cpu);
        }

        for column in self.side {
            for row in ui.children(column).into_iter().skip(1) {
                ui.remove(row);
            }
        }
        for (pass, ms) in &sample.gpu {
            timing_row(ui, self.side[0], pass, *ms, gpu_ms);
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
            let row = ui.panel(self.side[1], Style::row());
            ui.add_class(row, "profiler-row");
            let name = ui.add_label(row, label);
            ui.add_class(name, "profiler-name");
            let value = ui.add_label(row, &value);
            ui.add_class(value, "profiler-value");
        }
    }
}

/// The Hierarchy's samples, by item. An item's id is made from the names
/// on its way down from the frame, so a sample keeps its row, open or
/// closed, from one frame to the next.
#[derive(Default)]
struct CallTree {
    samples: Samples,
    nodes: HashMap<u64, usize>,
    children: HashMap<Option<u64>, Vec<u64>>,
}

impl CallTree {
    /// Shows a frame's samples, each sample's children slowest first.
    /// Returns every item with its name, for opening some by themselves.
    fn set(&mut self, samples: Samples) -> Vec<(u64, String)> {
        self.nodes.clear();
        self.children.clear();
        let mut items = Vec::new();
        let mut pending = vec![(Samples::FRAME, None)];
        while let Some((node, item)) = pending.pop() {
            let mut inside: Vec<usize> = samples.nodes[node].children.clone();
            inside.sort_by(|a, b| samples.total_ms(*b).total_cmp(&samples.total_ms(*a)));
            let mut ids = Vec::with_capacity(inside.len());
            for child in inside {
                let mut hasher = DefaultHasher::new();
                (item, samples.nodes[child].marker.id()).hash(&mut hasher);
                let id = hasher.finish();
                self.nodes.insert(id, child);
                items.push((id, samples.name(child)));
                pending.push((child, Some(id)));
                ids.push(id);
            }
            self.children.insert(item, ids);
        }
        self.samples = samples;
        items
    }
}

struct CallItems(Rc<RefCell<CallTree>>);

impl TreeItems for CallItems {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        self.0.borrow().children.get(&parent).cloned().unwrap_or_default()
    }

    fn has_children(&self, item: u64) -> bool {
        self.0.borrow().children.get(&Some(item)).is_some_and(|c| !c.is_empty())
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let name = ui.add_label(row, "");
        ui.add_class(name, "profiler-cell-name");
        for _ in COLUMNS {
            let cell = ui.add_label(row, "");
            ui.add_class(cell, "profiler-cell");
        }
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let calls = self.0.borrow();
        let Some(&node) = calls.nodes.get(&item) else { return };
        let samples = &calls.samples;
        let frame = samples.total_ms(Samples::FRAME).max(1e-6);
        let (total, own) = (samples.total_ms(node), samples.self_ms(node));
        let texts = [
            samples.name(node),
            format!("{:.1}%", total / frame * 100.0),
            format!("{:.1}%", own / frame * 100.0),
            samples.nodes[node].calls.to_string(),
            bytes(samples.alloc_bytes(node)),
            format!("{total:.2}"),
            format!("{own:.2}"),
        ];
        for (part, text) in ui.children(row).into_iter().zip(texts) {
            ui.set_text(part, &text);
        }
    }
}

impl Profiler {
    /// The frame as it ran: each run a bar, along the frame's time and down
    /// by how deep it ran, named when it is wide enough, the one under the
    /// pointer lighter.
    fn draw_timeline(&mut self, ui: &mut Ui, samples: &Samples) {
        let [_, _, width, _] = ui.rect(self.timeline_canvas);
        self.timeline_width = width;
        let length = samples.spans.iter().map(|span| span.end_ns).max().unwrap_or(0).max(1) as f32;
        let x = |ns: u64| ns as f32 / length * width;
        let mut shapes = Vec::with_capacity(samples.spans.len());
        let mut named = Vec::new();
        for (index, span) in samples.spans.iter().enumerate() {
            let (left, right) = (x(span.start_ns), x(span.end_ns));
            let top = f32::from(span.depth) * LANE;
            let name = span.marker.name();
            let lit = self.hovered_span == Some(index);
            let fill = Color::hex(bar_color(&name)).with_alpha(if lit { 255 } else { 210 });
            let rect = [left, top + 1.0, (right - left).max(1.0), LANE - 2.0];
            shapes.push(Shape::Rect { rect, radius: 3.0, fill });
            if right - left >= NAMED {
                named.push((name, span.end_ns - span.start_ns, [left, top, right - left]));
            }
        }
        ui.set_shapes(self.timeline_canvas, shapes);
        while self.timeline_names.len() < named.len() {
            let label = ui.add_label(self.timeline_canvas, "");
            ui.add_class(label, "profiler-bar-name");
            self.timeline_names.push(label);
        }
        for (index, &label) in self.timeline_names.iter().enumerate() {
            let Some((name, ns, [left, top, width])) = named.get(index) else {
                ui.set_visible(label, false);
                continue;
            };
            // As much of the name as fits, with its time when that fits too.
            let room = ((width - 8.0) / 6.2) as usize;
            let full = format!("{name} {:.2} ms", *ns as f32 / 1e6);
            let text: String = if full.chars().count() <= room { full } else { name.chars().take(room).collect() };
            ui.set_text(label, &text);
            ui.set_inline_style(label, &format!("left: {}px; top: {}px", left + 4.0, top + 2.0));
            ui.set_visible(label, true);
        }
    }

    /// Names the run under the pointer, and lights its bar.
    fn hover_timeline(&mut self, ui: &mut Ui) {
        let Some(sample) = self.shown_sample().cloned() else { return };
        let [x, y, width, height] = ui.rect(self.timeline_canvas);
        let hovered = ui.pointer_position().filter(|&(px, py)| px >= x && px < x + width && py >= y && py < y + height);
        let found = hovered.and_then(|(px, py)| {
            let length = sample.cpu.spans.iter().map(|span| span.end_ns).max().unwrap_or(0).max(1) as f32;
            let at = ((px - x) / width * length) as u64;
            let depth = ((py - y) / LANE) as u16;
            sample.cpu.spans.iter().rposition(|s| s.depth == depth && s.start_ns <= at && at <= s.end_ns)
        });
        if found == self.hovered_span && width == self.timeline_width {
            return;
        }
        self.hovered_span = found;
        let text = found.map_or_else(
            || "Point at a bar to see what it is.".to_string(),
            |index| {
                let span = sample.cpu.spans[index];
                let (ms, at) = ((span.end_ns - span.start_ns) as f32 / 1e6, span.start_ns as f32 / 1e6);
                format!("{}   {ms:.3} ms, from {at:.2} ms into the frame", span.marker.name())
            },
        );
        ui.set_text(self.timeline_hovered, &text);
        self.draw_timeline(ui, &sample.cpu);
    }
}

/// A bar's colour by what ran: scripts' callbacks and systems, the
/// renderer's work, waiting on the GPU or display, or the engine's own.
fn bar_color(name: &str) -> u32 {
    if name.ends_with("()") {
        SCRIPT
    } else if WAITING.contains(&name) {
        WAIT
    } else if RENDERING.contains(&name) {
        RENDER
    } else {
        ENGINE
    }
}

/// Bytes as people read them: 0 B, 512 B, 1.5 KB, 2.3 MB.
fn bytes(count: u64) -> String {
    match count {
        0..1024 => format!("{count} B"),
        1024..1_048_576 => format!("{:.1} KB", count as f64 / 1024.0),
        _ => format!("{:.1} MB", count as f64 / 1_048_576.0),
    }
}

fn divider(ui: &mut Ui, root: NodeId) {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
}

/// A pass: its name, its milliseconds, and a bar for its share of `total`.
fn timing_row(ui: &mut Ui, column: NodeId, name: &str, ms: f32, total: f32) {
    let row = ui.panel(column, Style::row());
    ui.add_class(row, "profiler-row");
    let label = ui.add_label(row, name);
    ui.add_class(label, "profiler-name");
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
