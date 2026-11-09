//! Debug draw's texts (`Debug.Text`) as labels over the 3D view, each at the
//! point it names as the camera sees it, in its colour, and its screen texts
//! (`Draw.ScreenText`) one under another in the view's top-right corner. The renderer draws
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
.debug-text { position: absolute; padding: 1px 4px; border-radius: 3px; background-color: rgba(0, 0, 0, 0.45); font-family: mono; font-size: 11px; pointer-events: none; }
.debug-corner { position: absolute; flex-direction: column; align-items: flex-end; padding: 8px; gap: 2px; pointer-events: none; }
.debug-corner .debug-text { position: relative; }";

/// The labels showing debug draw's texts, in a layer of their own.
pub struct DebugTexts {
    layer: NodeId,
    labels: Vec<NodeId>,
    /// The column in the view's corner, and its lines.
    corner: NodeId,
    lines: Vec<NodeId>,
}

impl DebugTexts {
    /// Makes `layer` the texts' layer: it covers its parent and the pointer
    /// passes through it.
    pub fn new(ui: &mut Ui, layer: NodeId) -> Self {
        ui.add_class(layer, "debug-texts");
        ui.add_sheet(layer, StyleSheet::parse("DebugTexts.vss", SHEET));
        let corner = ui.panel(layer, voxear_ui::Style::default());
        ui.add_class(corner, "debug-corner");
        Self { layer, labels: Vec::new(), corner, lines: Vec::new() }
    }

    /// Shows this frame's texts where `project` puts their points, those
    /// that land inside `rect` (left, top, width and height, in the UI's
    /// pixels); with no projection, none.
    pub fn update(&mut self, ui: &mut Ui, project: Option<&dyn Fn(Vec3) -> Option<[f32; 2]>>, rect: [f32; 4]) {
        let [x, y, w, h] = rect;
        let (on_screen, in_world): (Vec<_>, Vec<_>) = debug_draw::texts().into_iter().partition(|text| text.on_screen);
        self.show_corner(ui, rect, on_screen);
        let shown: Vec<(String, [f32; 2], [f32; 4])> = match project {
            Some(project) => in_world
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
            let style = format!("left: {}px; top: {}px; {}", x + NUDGE, y - NUDGE - 14.0, css_color(*color));
            ui.set_inline_style(label, &style);
            ui.set_visible(label, true);
        }
    }

    /// The screen texts, one under another down the corner of `rect`, as
    /// many as fit.
    fn show_corner(&mut self, ui: &mut Ui, [x, y, w, h]: [f32; 4], texts: Vec<debug_draw::Text>) {
        let fit = ((h - 16.0) / 18.0).max(0.0) as usize;
        let texts = &texts[..texts.len().min(fit)];
        ui.set_inline_style(self.corner, &format!("left: {x}px; top: {y}px; width: {w}px; height: {h}px"));
        while self.lines.len() < texts.len() {
            let line = ui.add_label(self.corner, "");
            ui.add_class(line, "debug-text");
            self.lines.push(line);
        }
        for (index, &line) in self.lines.iter().enumerate() {
            let Some(text) = texts.get(index) else {
                ui.set_visible(line, false);
                continue;
            };
            if ui.text(line) != text.text {
                ui.set_text(line, &text.text);
            }
            ui.set_inline_style(line, &css_color(text.color));
            ui.set_visible(line, true);
        }
    }

    /// How many texts show, at their points and in the corner.
    pub fn shown(&self, ui: &Ui) -> usize {
        self.labels.iter().chain(&self.lines).filter(|&&label| ui.is_visible(label)).count()
    }
}

/// `color` (sRGB with alpha) as a style's text colour.
fn css_color(color: [f32; 4]) -> String {
    let [r, g, b, a] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("color: rgba({r}, {g}, {b}, {})", f32::from(a) / 255.0)
}

/// Where points in the world show in `rect` (left, top, width and height),
/// seen through the camera the renderer draws with: `camera`, or the first
/// in the world, as the renderer picks it. `None` without a camera.
pub fn camera_projection(
    world: &World,
    camera: Option<Entity>,
    rect: [f32; 4],
) -> Option<impl Fn(Vec3) -> Option<[f32; 2]>> {
    let view_projection = view_projection(world, camera, rect)?;
    Some(move |point: Vec3| {
        let clip = view_projection * point.extend(1.0);
        if clip.w <= 1e-4 {
            return None;
        }
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        Some([rect[0] + (x + 1.0) * 0.5 * rect[2], rect[1] + (y + 1.0) * 0.5 * rect[3]])
    })
}

/// The ray from the camera the renderer draws with (`camera`, or the first
/// in the world) through `point` of `rect` (left, top, width and height):
/// where it starts, on the camera's near plane, and which way it goes, a
/// metre long. `None` without a camera.
pub fn camera_ray(world: &World, camera: Option<Entity>, rect: [f32; 4], point: [f32; 2]) -> Option<(Vec3, Vec3)> {
    let inverse = view_projection(world, camera, rect)?.inverse();
    let x = (point[0] - rect[0]) / rect[2].max(1.0) * 2.0 - 1.0;
    let y = (point[1] - rect[1]) / rect[3].max(1.0) * 2.0 - 1.0;
    // Depth runs the other way: 1 at the near plane, smaller farther off.
    let near = inverse.project_point3(Vec3::new(x, y, 1.0));
    let far = inverse.project_point3(Vec3::new(x, y, 0.5));
    Some((near, (far - near).normalize_or_zero()))
}

/// What the camera the renderer draws with sees, for a view `rect` in size.
fn view_projection(world: &World, camera: Option<Entity>, rect: [f32; 4]) -> Option<Mat4> {
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
    Some(vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), aspect, lens.near) * view)
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
        let (from, along) = camera_ray(&world, None, rect, [400.0, 300.0]).expect("the world has a camera");
        assert!((along - Vec3::Z).length() < 1e-4 && from.x.abs() < 1e-4 && from.z > -10.0, "{from} {along}");
        let (_, right) = camera_ray(&world, None, rect, [800.0, 300.0]).unwrap();
        assert!(right.x > 0.0 && right.y.abs() < 1e-4, "the view's right edge is to the camera's right: {right}");
        let (_, low) = camera_ray(&world, None, rect, [400.0, 600.0]).unwrap();
        assert!(low.y < 0.0, "the bottom edge is below: {low}");

        debug_draw::set_enabled(true);
        debug_draw::text(Vec3::ZERO, "texts_show_where_the_camera_sees_their_points", [1.0, 0.5, 0.0, 1.0], 0.0);
        texts.update(&mut ui, Some(&project), rect);
        ui.frame(800, 600, 1.0);
        let label = texts.labels.iter().copied().find(|&l| ui.text(l).starts_with("texts_show_where")).unwrap();
        let [x, y, ..] = ui.rect(label);
        assert_eq!([x, y], [400.0 + NUDGE, 300.0 - NUDGE - 14.0], "beside its point");
        assert_eq!(ui.text_style(label).unwrap().color, voxear_ui::Color::rgba(255, 128, 0, 255), "in its colour");

        // A screen text, in the corner whether or not there is a camera, right against its right edge.
        debug_draw::screen_text("speed 12", [1.0; 4], 0.0);
        texts.update(&mut ui, None, rect);
        ui.frame(800, 600, 1.0);
        assert_eq!(texts.shown(&ui), 1, "with no camera, only the screen text");
        let [x, y, w, _] = ui.rect(texts.lines[0]);
        assert!(x + w > 780.0 && x + w <= 800.0 && y < 20.0, "in the top-right corner: {x} {y} {w}");
        debug_draw::clear();
        assert!(camera_projection(&World::new(), None, rect).is_none());
    }
}
