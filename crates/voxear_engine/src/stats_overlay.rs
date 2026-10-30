//! The stats overlay: frame rate, GPU time and what the renderer drew, in a
//! panel over the game. F1 shows and hides it in a game window. It is the
//! first thing built with Voxear UI, styled as the editor will be.

use anyhow::Result;
use voxear_render::Renderer;
use voxear_ui::{Color, NodeId, Style, TextStyle, Ui};

/// How often the numbers change, slow enough to read.
const REFRESH: f32 = 0.25;

const PANEL: Color = Color::hex(0x111216).with_alpha(0xeb);
const EDGE: Color = Color::hex(0x2a2b30);
const TEXT: Color = Color::hex(0xe8e8ec);
const MUTED: Color = Color::hex(0x8b8c96);
const FAINT: Color = Color::hex(0x5c5d66);
const ACCENT: Color = Color::hex(0x8b93ff);

/// Rows in groups, each group under a divider.
const SECTIONS: [&[&str]; 3] =
    [&["Frame rate", "Frame", "GPU"], &["Draws", "Shadow draws", "Culled"], &["Lights per cluster", "Exposure"]];

pub struct StatsOverlay {
    ui: Ui,
    panel: NodeId,
    values: Vec<NodeId>,
    frames: u32,
    seconds: f32,
}

impl StatsOverlay {
    pub fn new(visible: bool) -> Self {
        let mut ui = Ui::new();
        let root = ui.root();
        let style = Style::column().at(12.0, 12.0).width(228.0).padding(12.0).gap(6.0).radius(10.0);
        let panel = ui.panel(root, style.background(PANEL).border(1.0, EDGE));
        let header = ui.panel(panel, Style::row().spread());
        ui.label(header, "Stats", TextStyle::default().size(12.0).weight(600.0).color(TEXT));
        ui.label(header, "F1", TextStyle::default().size(11.0).color(FAINT).mono());

        let name = TextStyle::default().size(12.0).color(MUTED);
        let value = TextStyle::default().size(12.0).color(TEXT).mono();
        let mut values = Vec::new();
        for section in SECTIONS {
            ui.panel(panel, Style::default().height(1.0).background(EDGE));
            for row_name in section {
                let row = ui.panel(panel, Style::row().spread());
                ui.label(row, row_name, name);
                values.push(ui.label(row, "–", value));
            }
        }
        ui.set_text_style(values[0], value.color(ACCENT));
        ui.set_visible(panel, visible);
        Self { ui, panel, values, frames: 0, seconds: 0.0 }
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
    pub fn draw(&mut self, renderer: &mut Renderer, (width, height): (u32, u32), scale: f32) -> Result<()> {
        renderer.set_ui(&self.ui.frame(width, height, scale))
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
    #[test]
    fn numbers_are_grouped_in_thousands() {
        assert_eq!(super::grouped(0), "0");
        assert_eq!(super::grouped(999), "999");
        assert_eq!(super::grouped(1000), "1,000");
        assert_eq!(super::grouped(1234567), "1,234,567");
    }
}
