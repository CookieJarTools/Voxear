//! The stats overlay: frame rate, GPU time and what the renderer drew, in a
//! panel over the game. F1 shows and hides it in a game window, and
//! clicking its header folds it away. It is the first thing built with
//! Voxear UI markup: `ui/StatsOverlay.vui` and `ui/StatsOverlay.vss`.

use anyhow::Result;
use voxear_render::Renderer;
use voxear_ui::{NodeId, Ui, UiFrame};

/// How often the numbers change, slow enough to read.
const REFRESH: f32 = 0.25;

const MARKUP: &str = include_str!("../ui/StatsOverlay.vui");
const SHEET: &str = include_str!("../ui/StatsOverlay.vss");

/// The labels the numbers go in, by name, in the order [`refresh`] fills them.
///
/// [`refresh`]: StatsOverlay::refresh
const VALUES: [&str; 8] = ["fps", "frame", "gpu", "draws", "shadow-draws", "culled", "lights", "exposure"];

pub struct StatsOverlay {
    ui: Ui,
    panel: NodeId,
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
        let (panel, header) = (find("stats"), find("header"));
        let values = VALUES.iter().map(|name| find(name)).collect();
        ui.set_visible(panel, visible);
        Self { ui, panel, header, values, frames: 0, seconds: 0.0 }
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
        self.handle_clicks();
        self.frames += 1;
        self.seconds += delta;
        if self.seconds >= REFRESH {
            self.refresh(renderer);
        }
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
            format!("{gpu_ms:.2} ms"),
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
        assert!(overlay.ui.take_warnings().is_empty(), "the built-in markup and sheet are clean");

        overlay.pointer_move(x + w / 2.0, y + h / 2.0);
        overlay.pointer_button(true);
        overlay.pointer_button(false);
        overlay.handle_clicks();
        assert_eq!(overlay.frame(size, 1.0).quads.len(), open, "and unfolds it again");
    }

    #[test]
    fn numbers_are_grouped_in_thousands() {
        assert_eq!(super::grouped(0), "0");
        assert_eq!(super::grouped(999), "999");
        assert_eq!(super::grouped(1000), "1,000");
        assert_eq!(super::grouped(1234567), "1,234,567");
    }
}
