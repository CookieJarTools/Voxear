//! Debug draw's texts (`Debug.Text`) as labels over the 3D view, each at the
//! point it names as the camera sees it, in its colour. The renderer draws
//! debug draw's lines; texts are labels because the UI has the fonts. The
//! editor shows them over its Scene and Game views, and a player over its
//! window, both through [`DebugTexts`].

use voxear_core::{
    debug_draw,
    math::{Mat4, Vec3, camera::lh::proj::vulkan},
};
use voxear_ecs::{Entity, World};
use voxear_render::Camera;
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_ui::{NodeId, StyleSheet, Ui};

/// How far a text sits from its point, right and up, in pixels.
const NUDGE: f32 = 4.0;
/// How a text and the layer holding them look, wherever they are shown.
const SHEET: &str = ".debug-texts { position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; pointer-events: none; }
.debug-text { position: absolute; padding: 1px 4px; border-radius: 3px; background-color: rgba(0, 0, 0, 0.45); font-family: mono; font-size: 11px; pointer-events: none; }";

/// The labels showing debug draw's texts, in a layer of their own.
pub struct DebugTexts {
    layer: NodeId,
    labels: Vec<NodeId>,
}

impl DebugTexts {
    /// Makes `layer` the texts' layer: it covers its parent and the pointer
    /// passes through it.
    pub fn new(ui: &mut Ui, layer: NodeId) -> Self {
        ui.add_class(layer, "debug-texts");
        ui.add_sheet(layer, StyleSheet::parse("DebugTexts.vss", SHEET));
        Self { layer, labels: Vec::new() }
    }

    /// Shows this frame's texts where `project` puts their points, those
    /// that land inside `rect` (left, top, width and height, in the UI's
    /// pixels); with no projection, none.
    pub fn update(&mut self, ui: &mut Ui, project: Option<&dyn Fn(Vec3) -> Option<[f32; 2]>>, rect: [f32; 4]) {
        let [x, y, w, h] = rect;
        let shown: Vec<(String, [f32; 2], [f32; 4])> = match project {
            Some(project) => debug_draw::texts()
                .into_iter()
                .filter_map(|text| {
                    let at = project(text.at)?;
                    let inside = at[0] >= x && at[0] < x + w && at[1] >= y && at[1] < y + h;
                    inside.then_some((text.text, at, text.color))
                })
                .collect(),
            None => Vec::new(),
        };
        while self.labels.len() < shown.len() {
            let label = ui.add_label(self.layer, "");
            ui.add_class(label, "debug-text");
            self.labels.push(label);
        }
        for (index, &label) in self.labels.iter().enumerate() {
            let Some((text, [x, y], color)) = shown.get(index) else {
                ui.set_visible(label, false);
                continue;
            };
            if ui.text(label) != *text {
                ui.set_text(label, text);
            }
            let [r, g, b, a] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let style = format!(
                "left: {}px; top: {}px; color: rgba({r}, {g}, {b}, {})",
                x + NUDGE,
                y - NUDGE - 14.0,
                f32::from(a) / 255.0
            );
            ui.set_inline_style(label, &style);
            ui.set_visible(label, true);
        }
    }

    /// How many texts show.
    pub fn shown(&self, ui: &Ui) -> usize {
        self.labels.iter().filter(|&&label| ui.is_visible(label)).count()
    }
}

/// Where points in the world show in `rect` (left, top, width and height),
/// seen through the camera the renderer draws with: `camera`, or the first
/// in the world, as the renderer picks it. `None` without a camera.
pub fn camera_projection(
    world: &World,
    camera: Option<Entity>,
    rect: [f32; 4],
) -> Option<impl Fn(Vec3) -> Option<[f32; 2]>> {
    let (lens, position, rotation) = world
        .query_ref::<(Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .find(|(entity, ..)| camera.is_none_or(|chosen| chosen == *entity))
        .map(|(_, lens, local, global)| {
            let (_, rotation, position) =
                global.map_or((local.scale, local.rotation, local.position), |g| g.0.to_scale_rotation_translation());
            (*lens, position, rotation)
        })?;
    let aspect = rect[2] / rect[3].max(1.0);
    let view = Mat4::from_rotation_translation(rotation, position).inverse();
    let view_projection =
        vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), aspect, lens.near) * view;
    Some(move |point: Vec3| {
        let clip = view_projection * point.extend(1.0);
        if clip.w <= 1e-4 {
            return None;
        }
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        Some([rect[0] + (x + 1.0) * 0.5 * rect[2], rect[1] + (y + 1.0) * 0.5 * rect[3]])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_show_where_the_camera_sees_their_points() {
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -10.0))));
        let mut ui = Ui::new();
        let layer = ui.panel(ui.root(), voxear_ui::Style::default());
        let mut texts = DebugTexts::new(&mut ui, layer);
        let rect = [0.0, 0.0, 800.0, 600.0];
        let project = camera_projection(&world, None, rect).expect("the world has a camera");
        assert_eq!(project(Vec3::ZERO), Some([400.0, 300.0]), "straight ahead is the middle");
        assert_eq!(project(Vec3::new(0.0, 0.0, -20.0)), None, "behind the camera");

        debug_draw::set_enabled(true);
        debug_draw::text(Vec3::ZERO, "texts_show_where_the_camera_sees_their_points", [1.0, 0.5, 0.0, 1.0], 0.0);
        texts.update(&mut ui, Some(&project), rect);
        ui.frame(800, 600, 1.0);
        let label = texts.labels.iter().copied().find(|&l| ui.text(l).starts_with("texts_show_where")).unwrap();
        let [x, y, ..] = ui.rect(label);
        assert_eq!([x, y], [400.0 + NUDGE, 300.0 - NUDGE - 14.0], "beside its point");
        assert_eq!(ui.text_style(label).unwrap().color, voxear_ui::Color::rgba(255, 128, 0, 255), "in its colour");

        texts.update(&mut ui, None, rect);
        assert_eq!(texts.shown(&ui), 0, "with no camera, none");
        assert!(camera_projection(&World::new(), None, rect).is_none());
    }
}
