//! The stats overlay: frame rate, GPU time and what the renderer drew, in a
//! panel over the game. F1 shows and hides it in a game window, and
//! clicking its header folds it away. It is the first thing built with
//! Voxear UI markup: `ui/StatsOverlay.vui` and `ui/StatsOverlay.vss`. The
//! console ([`DevConsole`]) shares its UI, so one frame draws both.

use std::collections::VecDeque;

use anyhow::Result;
use voxear_render::Renderer;
use voxear_ui::{Color, Key, NodeId, Shape, Ui, UiFrame};

use crate::{
    Engine,
    dev_console::{ConsoleAction, DevConsole},
};

/// How often the numbers change, slow enough to read.
const REFRESH: f32 = 0.25;
/// Frames the graph shows, newest at the right.
const GRAPHED: usize = 180;
/// The graph's top is never under this many milliseconds, a little over a
/// frame at 60 a second, so a steady game sits low with room above.
const GRAPH_SCALE: f32 = 25.0;
const FRAME_LINE: u32 = 0x8b93ff;
const TARGET_LINE: u32 = 0x2a2b30;

pub(crate) const MARKUP: &str = include_str!("../ui/StatsOverlay.vui");
pub(crate) const SHEET: &str = include_str!("../ui/StatsOverlay.vss");

/// The labels the numbers go in, by name, in the order [`refresh`] fills them.
///
/// [`refresh`]: StatsOverlay::refresh
const VALUES: [&str; 8] = ["fps", "frame", "gpu", "draws", "shadow-draws", "culled", "lights", "exposure"];

pub struct StatsOverlay {
    ui: Ui,
    console: DevConsole,
    panel: NodeId,
    graph: NodeId,
    /// The latest frame times, in milliseconds, oldest first.
    times: VecDeque<f32>,
    header: NodeId,
    values: Vec<NodeId>,
    frames: u32,
    seconds: f32,
}

impl StatsOverlay {
    pub fn new(visible: bool) -> Self {
        let mut ui = Ui::new();
        let root = ui.root();
        let mut load = |path: &str| match path {
            "StatsOverlay.vss" => Ok(SHEET.to_string()),
            other => Err(format!("{other} is not built in")),
        };
        let document =
            ui.load_markup(root, "StatsOverlay.vui", MARKUP, &mut load).expect("the overlay's markup is built in");
        let find = |name: &str| ui.find(document, name).unwrap_or_else(|| panic!("StatsOverlay.vui has no {name}"));
        let (panel, header, graph) = (find("stats"), find("header"), find("graph"));
        let values = VALUES.iter().map(|name| find(name)).collect();
        ui.set_visible(panel, visible);
        let console = DevConsole::new(&mut ui, document);
        Self {
            ui,
            console,
            panel,
            graph,
            times: VecDeque::with_capacity(GRAPHED),
            header,
            values,
            frames: 0,
            seconds: 0.0,
        }
    }

    /// Opens the console, ready to type, or closes it. The stats step aside
    /// while it is open, as it covers where they show, and come back after.
    pub fn toggle_console(&mut self) {
        self.console.toggle(&mut self.ui);
        self.ui.set_class(self.panel, "under-console", self.console.is_open(&self.ui));
    }

    pub fn console_open(&self) -> bool {
        self.console.is_open(&self.ui)
    }

    /// The console's frame: what was logged, and a line run on Enter.
    pub fn update_console(&mut self, engine: Option<&mut Engine>) -> Option<ConsoleAction> {
        self.console.update(&mut self.ui, engine)
    }

    /// Hands the open console a key; returns whether it took it.
    pub fn console_key(&mut self, key: Key, engine: Option<&Engine>) -> bool {
        let took = self.console.key(&mut self.ui, key, engine);
        // Escape closes it, which gives the stats their place back.
        self.ui.set_class(self.panel, "under-console", self.console.is_open(&self.ui));
        took
    }

    pub fn is_visible(&self) -> bool {
        self.ui.is_visible(self.panel)
    }

    pub fn toggle(&mut self) {
        let visible = !self.is_visible();
        self.ui.set_visible(self.panel, visible);
    }

    /// Counts a frame that took `delta` seconds, refreshing the numbers every
    /// [`REFRESH`] seconds.
    pub fn update(&mut self, delta: f32, renderer: &Renderer) {
        self.ui.advance(delta);
        self.handle_clicks();
        self.frames += 1;
        self.seconds += delta;
        if self.times.len() == GRAPHED {
            self.times.pop_front();
        }
        self.times.push_back(delta * 1000.0);
        if self.is_visible() {
            self.draw_graph();
        }
        if self.seconds >= REFRESH {
            self.refresh(renderer);
        }
    }

    /// The frame times as a line, a frame a step, under a line at 60 frames a
    /// second; a hitch shows as a spike.
    fn draw_graph(&mut self) {
        let [_, _, width, height] = self.ui.rect(self.graph);
        let slowest = self.times.iter().copied().fold(0.0, f32::max);
        let scale = (slowest * 1.1).max(GRAPH_SCALE);
        let y = |ms: f32| height - 2.0 - (ms / scale).min(1.0) * (height - 4.0);
        let step = width / (GRAPHED - 1) as f32;
        let first = GRAPHED - self.times.len();
        let target = y(1000.0 / 60.0);
        let mut shapes =
            vec![Shape::Line { from: [0.0, target], to: [width, target], width: 1.0, color: Color::hex(TARGET_LINE) }];
        let points: Vec<[f32; 2]> =
            self.times.iter().enumerate().map(|(i, &ms)| [(first + i) as f32 * step, y(ms)]).collect();
        for pair in points.windows(2) {
            shapes.push(Shape::Line { from: pair[0], to: pair[1], width: 1.5, color: Color::hex(FRAME_LINE) });
        }
        self.ui.set_shapes(self.graph, shapes);
    }

    /// Shows the numbers now, averaged over the frames since the last time.
    pub fn refresh(&mut self, renderer: &Renderer) {
        let (frames, seconds) = (self.frames.max(1) as f32, self.seconds.max(1e-6));
        let gpu_ms: f32 = renderer.timings().iter().map(|(_, ms)| ms).sum();
        let stats = renderer.stats();
        let exposure =
            if stats.average_luminance > 0.0 { format!("{:+.1} EV", stats.exposure_ev) } else { "fixed".into() };
        let texts = [
            format!("{:.0} fps", frames / seconds),
            format!("{:.2} ms", seconds * 1000.0 / frames),
            // Adding zero turns a rounding's -0.00 into 0.00.
            format!("{:.2} ms", gpu_ms.max(0.0) + 0.0),
            grouped(stats.drawn_early + stats.drawn_late),
            grouped(stats.shadow_draws),
            grouped(stats.frustum_culled + stats.occlusion_culled),
            grouped(stats.cluster_max_lights),
            exposure,
        ];
        for (node, text) in self.values.iter().zip(&texts) {
            self.ui.set_text(*node, text);
        }
        (self.frames, self.seconds) = (0, 0.0);
    }

    /// Hands the renderer the overlay for an output of `width` by `height`
    /// pixels at `scale` pixels per logical pixel. Hidden, it draws nothing.
    pub fn draw(&mut self, renderer: &mut Renderer, size: (u32, u32), scale: f32) -> Result<()> {
        renderer.set_ui(&self.frame(size, scale))
    }

    fn frame(&mut self, (width, height): (u32, u32), scale: f32) -> UiFrame {
        self.ui.frame(width, height, scale)
    }

    /// Hands it a window event. Returns whether the overlay used it.
    pub fn handle(&mut self, event: &winit::event::WindowEvent) -> bool {
        crate::ui_input::handle(&mut self.ui, event)
    }

    /// The pointer moved, in physical pixels from the window's top left.
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.ui.pointer_move(x, y);
    }

    pub fn pointer_leave(&mut self) {
        self.ui.pointer_leave();
    }

    /// The wheel turned by `dy` logical pixels, positive downwards. Returns
    /// whether anything in the overlay scrolled.
    pub fn pointer_scroll(&mut self, dy: f32) -> bool {
        self.ui.pointer_scroll(0.0, dy)
    }

    /// The primary button went down or up. Returns whether the overlay was
    /// under the pointer, so the game can leave the press alone.
    pub fn pointer_button(&mut self, pressed: bool) -> bool {
        self.ui.pointer_button(pressed)
    }

    fn handle_clicks(&mut self) {
        if self.ui.clicked(self.header) {
            self.ui.toggle_class(self.panel, "collapsed");
        }
    }
}

/// `12345` as `12,345`.
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_the_header_folds_the_panel() {
        let mut overlay = StatsOverlay::new(true);
        let size = (800, 600);
        let open = overlay.frame(size, 1.0).quads.len();
        let [x, y, w, h] = overlay.ui.rect(overlay.header);
        overlay.pointer_move(x + w / 2.0, y + h / 2.0);
        assert!(overlay.pointer_button(true), "the overlay takes the press");
        overlay.pointer_button(false);
        overlay.handle_clicks();
        let folded = overlay.frame(size, 1.0).quads.len();
        assert!(folded < open / 2, "{open} quads open, {folded} folded");
        let warnings = overlay.ui.take_warnings();
        assert!(warnings.is_empty(), "the built-in markup and sheet are clean: {warnings:?}");

        overlay.pointer_move(x + w / 2.0, y + h / 2.0);
        overlay.pointer_button(true);
        overlay.pointer_button(false);
        overlay.handle_clicks();
        assert_eq!(overlay.frame(size, 1.0).quads.len(), open, "and unfolds it again");
    }

    /// The console open over a frame, with lines in it. Set `VOXEAR_TEST_OUTPUT`
    /// to a folder to keep the image.
    #[test]
    fn the_console_draws_over_the_game() {
        let Ok(mut renderer) = Renderer::new_offscreen(960, 540, &voxear_render::RendererSettings::default()) else {
            return eprintln!("skipping: no usable GPU");
        };
        let _log =
            tracing::subscriber::set_default(voxear_core::logging::capturing_only(voxear_core::logging::recent()));
        let mut overlay = StatsOverlay::new(true);
        overlay.toggle_console();
        tracing::info!("Loaded game scripts");
        tracing::warn!("Spinner B has no leader");
        overlay.update_console(None);
        let closed_quads = StatsOverlay::new(false).frame((960, 540), 1.0).quads.len();
        let quads = overlay.frame((960, 540), 1.0).quads.len();
        assert!(quads > closed_quads, "the console draws: {quads} quads, {closed_quads} without it");
        assert!(overlay.ui.take_warnings().is_empty(), "{:?}", overlay.ui.take_warnings());
        assert!(!overlay.ui.is_visible(overlay.panel) || overlay.ui.has_class(overlay.panel, "under-console"));
        overlay.draw(&mut renderer, (960, 540), 1.0).unwrap();
        renderer.render(&voxear_ecs::World::new(), 0.0).unwrap();
        if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let (width, height, pixels) = renderer.read_pixels().unwrap();
            image::save_buffer(
                std::path::Path::new(&dir).join("dev-console.png"),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }

    /// Frame times as a line, with a hitch as a spike. Set `VOXEAR_TEST_OUTPUT`
    /// to a folder to keep the image.
    #[test]
    fn the_stats_graph_their_frame_times() {
        let Ok(mut renderer) = Renderer::new_offscreen(640, 360, &voxear_render::RendererSettings::default()) else {
            return eprintln!("skipping: no usable GPU");
        };
        let mut overlay = StatsOverlay::new(true);
        let before = overlay.frame((640, 360), 1.0).quads.len();
        for frame in 0..120 {
            let delta = if frame == 90 { 0.05 } else { 1.0 / 60.0 };
            overlay.update(delta, &renderer);
        }
        let after = overlay.frame((640, 360), 1.0).quads.len();
        assert!(after >= before + 100, "a line between each two frames: {before} quads, then {after}");
        overlay.draw(&mut renderer, (640, 360), 1.0).unwrap();
        renderer.render(&voxear_ecs::World::new(), 0.0).unwrap();
        if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let (width, height, pixels) = renderer.read_pixels().unwrap();
            let path = std::path::Path::new(&dir).join("stats-graph.png");
            image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8).unwrap();
        }
    }

    #[test]
    fn numbers_are_grouped_in_thousands() {
        assert_eq!(super::grouped(0), "0");
        assert_eq!(super::grouped(999), "999");
        assert_eq!(super::grouped(1000), "1,000");
        assert_eq!(super::grouped(1234567), "1,234,567");
    }
}
