//! The compact stats panel over a Scene view and the Game view: frame
//! rate and frame time, GPU time and memory, draws and shadow draws, Objs
//! drawn and culled, in two columns of names and numbers. The Scene view's
//! shows with Preferences' Show stats, and the Game view's with its title
//! bar's Stats button. A built game has the engine's own stats panel,
//! which its build profile can include (see `build_window`).

use voxear_ui::{NodeId, Ui};

/// What the panel names, two to a row, in the order [`show`] takes them.
pub const NAMES: [&str; 8] = ["FPS", "Frame", "GPU", "Memory", "Draws", "Shadows", "Objs", "Culled"];

/// The numbers a panel shows, in [`NAMES`]' order.
pub type Numbers = [String; 8];

/// Fills a panel with the numbers, making its rows the first time.
pub fn show(ui: &mut Ui, panel: NodeId, numbers: &Numbers) {
    if ui.children(panel).is_empty() {
        for name in NAMES {
            let label = ui.add_label(panel, name);
            ui.add_class(label, "view-stats-name");
            let value = ui.add_label(panel, "–");
            ui.add_class(value, "view-stats-value");
        }
    }
    let values: Vec<NodeId> = ui.children(panel).into_iter().skip(1).step_by(2).collect();
    for (value, number) in values.into_iter().zip(numbers) {
        if ui.text(value) != *number {
            ui.set_text(value, number);
        }
    }
}

/// The numbers from what the editor measured and the renderer drew.
pub fn numbers(fps: f32, gpu_ms: f32, megabytes: Option<f32>, stats: &voxear_render::RenderStats) -> Numbers {
    let frame = if fps > 0.0 { format!("{:.1} ms", 1000.0 / fps) } else { "–".into() };
    [
        format!("{fps:.0}"),
        frame,
        format!("{gpu_ms:.2} ms"),
        megabytes.map_or_else(|| "–".into(), |mb| format!("{mb:.0} MB")),
        (stats.drawn_early + stats.drawn_late).to_string(),
        stats.shadow_draws.to_string(),
        stats.instances.to_string(),
        (stats.frustum_culled + stats.occlusion_culled).to_string(),
    ]
}
