//! Debug draw in the editor. Its lines are drawn by the renderer; its texts
//! (`Debug.Text`) are labels here, at the points they name in the 3D view,
//! since the UI has the fonts. While not playing, the editor counts the
//! shapes' time down itself, as the engine does each game frame.

use voxear_core::debug_draw;
use voxear_ecs::Entity;
use voxear_ui::NodeId;

use super::Editor;
use crate::scene_tools::ViewCamera;

/// How far a text sits from its point, right and up, in pixels.
const NUDGE: f32 = 4.0;

impl Editor {
    pub(super) fn update_debug_draw(&mut self, view: Option<(NodeId, Entity)>, delta: f32) {
        if !self.is_playing() {
            debug_draw::advance(delta);
        }
        let texts = debug_draw::texts();
        let camera = view
            .and_then(|(viewport, camera)| ViewCamera::new(&self.game.engine.world, camera, self.ui.rect(viewport)));
        let shown: Vec<(String, [f32; 2], [f32; 4])> = match camera {
            Some(camera) => texts
                .into_iter()
                .filter_map(|text| {
                    let at = camera.project(text.at)?;
                    let [x, y, w, h] = camera.rect;
                    let inside = at[0] >= x && at[0] < x + w && at[1] >= y && at[1] < y + h;
                    inside.then_some((text.text, at, text.color))
                })
                .collect(),
            None => Vec::new(),
        };

        while self.debug_texts.len() < shown.len() {
            let label = self.ui.add_label(self.nodes.debug_texts, "");
            self.ui.add_class(label, "debug-text");
            self.debug_texts.push(label);
        }
        for (index, &label) in self.debug_texts.iter().enumerate() {
            let Some((text, [x, y], color)) = shown.get(index) else {
                self.ui.set_visible(label, false);
                continue;
            };
            if self.ui.text(label) != *text {
                self.ui.set_text(label, text);
            }
            let [r, g, b, a] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let style = format!(
                "left: {}px; top: {}px; color: rgba({r}, {g}, {b}, {})",
                x + NUDGE,
                y - NUDGE - 14.0,
                f32::from(a) / 255.0
            );
            self.ui.set_inline_style(label, &style);
            self.ui.set_visible(label, true);
        }
    }
}
