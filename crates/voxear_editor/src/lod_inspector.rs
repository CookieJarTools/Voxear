//! What the Inspector shows of levels of detail. Under a Mesh Renderer, the
//! LOD of its mesh a camera draws now, of how many, with every LOD's
//! triangles when the pointer rests on it.
//! Under a LOD Group, a bar of its levels as Unity's LOD Group shows them,
//! marked where a camera sees the group now, with buttons to add and take
//! away levels.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{LodCamera, LodGroup, LodInfo, LodMeasure, LodView, MeshRenderer, Renderer, lod_at};
use voxear_scene::{Transform, WorldTransform};
use voxear_ui::{NodeId, Style, Ui};

use crate::inspector::prop;

/// How much of a bar measured by distance is left for what is past the last
/// level, so the culled part shows.
const PAST_THE_LAST: f32 = 0.2;

/// The camera the Inspector says what is drawn through: the Scene view's in
/// edit mode, and the game's while playing.
#[derive(Clone, Copy, Debug)]
pub struct LodEye {
    pub camera: LodCamera,
    /// The view's height in pixels over twice the tangent of half its field
    /// of view: the pixels a metre covers a metre away, which the GPU picks
    /// a mesh's LOD by.
    pub pixels_per_metre: f32,
    /// The LOD error the view allows, in pixels.
    pub threshold: f32,
    /// Where the camera is, as the readouts name it: "the Scene view" or
    /// "the game".
    pub name: &'static str,
}

/// A Mesh Renderer's row for its mesh's LODs: the one drawn now, of how
/// many, and every LOD's triangles as the label's tooltip.
pub struct MeshLods {
    label: NodeId,
    value: NodeId,
    chain: String,
}

impl MeshLods {
    pub fn new(ui: &mut Ui, rows: NodeId) -> Self {
        let row = prop(ui, rows, "LODs");
        let value = ui.add_label(row, "");
        ui.add_class(value, "value");
        Self { label: ui.children(row)[0], value, chain: String::new() }
    }

    pub fn update(&mut self, ui: &mut Ui, world: &World, entity: Entity, renderer: &Renderer, eye: Option<LodEye>) {
        let Some(mesh) = world.get::<MeshRenderer>(entity).map(|r| r.mesh) else { return };
        let lods = renderer.mesh_lods(mesh).unwrap_or_default();
        self.chain = chain(&lods);
        let drawn = match (eye, renderer.mesh_sphere(mesh)) {
            (Some(eye), Some((center, radius))) if !lods.is_empty() => {
                let model = model(world, entity);
                let scale = max_scale(model);
                let away = model.transform_point3(center).distance(eye.camera.position) - radius * scale;
                Some(lod_at(&lods, away, scale, eye.pixels_per_metre, eye.threshold))
            }
            _ => None,
        };
        let text = match (drawn, lods.as_slice()) {
            (_, []) => "Not loaded yet".to_string(),
            (_, [only]) => format!("1, of {} triangles", thousands(only.triangles)),
            (Some(lod), _) => format!("LOD {lod} of {}: {} triangles", lods.len(), thousands(lods[lod].triangles)),
            (None, [first, ..]) => format!("{}, the first of {} triangles", lods.len(), thousands(first.triangles)),
        };
        ui.set_text(self.value, &text);
    }

    /// Every LOD's triangles, while the pointer rests on the row's label.
    pub fn hovered_tooltip(&self, ui: &Ui) -> Option<(String, NodeId)> {
        (ui.is_hovered(self.label) && !self.chain.is_empty()).then(|| (self.chain.clone(), self.label))
    }
}

/// "12,480, 6,240 and 3,120 triangles, the most detailed first", or for a
/// mesh too small to need LODs, its triangles alone.
fn chain(lods: &[LodInfo]) -> String {
    let counts: Vec<String> = lods.iter().map(|lod| thousands(lod.triangles)).collect();
    match counts.as_slice() {
        [] => String::new(),
        [only] => format!("One LOD, of {only} triangles"),
        [rest @ .., last] => format!("{} and {last} triangles, the most detailed first", rest.join(", ")),
    }
}

/// A LOD Group's bar, its marker and what it says, and its buttons.
pub struct GroupLods {
    segments: Vec<NodeId>,
    culled: NodeId,
    marker: NodeId,
    readout: NodeId,
    add: Option<NodeId>,
    remove: Option<NodeId>,
    /// What its levels were measured by when it was built.
    pub measure: LodMeasure,
    /// The inline style each part was last given, so an unchanged one is
    /// not styled again every frame.
    styled: HashMap<NodeId, String>,
}

impl GroupLods {
    /// The bar and what it says, at the top of the group's section. Its
    /// buttons go at the bottom, with [`finish`](Self::finish).
    pub fn new(ui: &mut Ui, rows: NodeId, group: &LodGroup) -> Self {
        let bar = ui.panel(rows, Style::row());
        ui.add_class(bar, "lod-bar");
        let segment = |ui: &mut Ui, text: &str, class: &str| {
            let segment = ui.panel(bar, Style::column());
            ui.add_class(segment, "lod-segment");
            ui.add_class(segment, class);
            let label = ui.add_label(segment, text);
            ui.add_class(label, "lod-segment-name");
            let end = ui.add_label(segment, "");
            ui.add_class(end, "lod-segment-end");
            segment
        };
        let segments = (0..group.levels.len())
            .map(|level| segment(ui, &format!("LOD {level}"), &format!("lod-level-{}", level.min(5))))
            .collect();
        let culled = segment(ui, "Culled", "lod-culled");
        let marker = ui.panel(bar, Style::default());
        ui.add_class(marker, "lod-marker");
        let readout = ui.add_label(rows, "");
        ui.add_class(readout, "lod-now");
        let (add, remove, measure, styled) = (None, None, group.measure, HashMap::new());
        Self { segments, culled, marker, readout, add, remove, measure, styled }
    }

    /// The buttons that add and take away levels, under the group's values,
    /// and while it has none, what to do first.
    pub fn finish(&mut self, ui: &mut Ui, rows: NodeId, group: &LodGroup) {
        if group.levels.is_empty() {
            let hint = "Put an Obj for each level inside this Obj, the most detailed first, then add a level for each.";
            let hint = ui.add_label(rows, hint);
            ui.add_class(hint, "lod-hint");
        }
        let buttons = ui.panel(rows, Style::row());
        ui.add_class(buttons, "lod-buttons");
        let add = ui.button(buttons, "Add Level");
        let remove = ui.button(buttons, "Remove Last Level");
        for button in [add, remove] {
            ui.add_class(button, "attr-button");
        }
        ui.set_class(remove, "disabled", group.levels.is_empty());
        (self.add, self.remove) = (Some(add), Some(remove));
    }

    /// Whether Add Level or Remove Last Level was clicked.
    pub fn clicked(&self, ui: &Ui) -> (bool, bool) {
        let clicked = |button: Option<NodeId>| button.is_some_and(|button| ui.clicked(button));
        (clicked(self.add), clicked(self.remove))
    }

    /// Sizes the bar's levels to where each ends, and marks where `eye`
    /// sees the group, if it is given.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        group: &LodGroup,
        view: Option<(LodView, Option<usize>)>,
        eye: Option<LodEye>,
    ) {
        let ends = bar_ends(group);
        let mut from = 0.0;
        for (level, (&segment, &end)) in self.segments.clone().iter().zip(&ends).enumerate() {
            self.style(ui, segment, format!("flex-grow: {:.4}", (end - from).max(0.001)));
            // Without culling, the last level has no end.
            let ends_here = group.cull || level + 1 < group.levels.len();
            let text = group.levels.get(level).filter(|_| ends_here).map_or_else(String::new, |l| until(group, l.until));
            let label = ui.children(segment)[1];
            ui.set_text(label, &text);
            from = end;
        }
        let past = (1.0 - from).max(0.0);
        ui.set_visible(self.culled, group.cull && past > 0.0);
        self.style(ui, self.culled, format!("flex-grow: {:.4}", past.max(0.001)));

        let Some(((view, level), eye)) = view.zip(eye) else {
            ui.set_visible(self.marker, false);
            ui.set_text(self.readout, "Open a Scene view to see which level it shows.");
            return;
        };
        let at = place(group, view).clamp(0.0, 1.0);
        ui.set_visible(self.marker, true);
        self.style(ui, self.marker, format!("left: {:.2}%", at * 100.0));
        let shown = level.map_or_else(|| "nothing, past its last level".to_string(), |level| format!("LOD {level}"));
        let measured = format!("{:.0}% of the screen, {:.1} m away", view.size * 100.0, view.distance);
        ui.set_text(self.readout, &format!("{} sees it at {measured}: it shows {shown}.", capital(eye.name)));
        for (index, &segment) in self.segments.iter().enumerate() {
            ui.set_class(segment, "lod-shown", level == Some(index));
        }
        ui.set_class(self.culled, "lod-shown", level.is_none());
    }

    fn style(&mut self, ui: &mut Ui, node: NodeId, style: String) {
        if self.styled.get(&node) != Some(&style) {
            ui.set_inline_style(node, &style);
            self.styled.insert(node, style);
        }
    }
}

/// Where each level ends along the bar, from 0 at its left to 1 at its
/// right: measured by screen size, the bar runs from the whole screen down
/// to nothing; by distance, from the group out to past its last level.
fn bar_ends(group: &LodGroup) -> Vec<f32> {
    let last = group.levels.len().saturating_sub(1);
    let ends = group.levels.iter().enumerate().map(|(index, level)| {
        if index == last && !group.cull {
            return 1.0;
        }
        match group.measure {
            LodMeasure::ScreenSize => 1.0 - level.until.clamp(0.0, 1.0),
            LodMeasure::Distance => level.until / distance_span(group),
        }
    });
    // A level ending before the one before it shows nothing.
    let mut furthest = 0.0f32;
    ends.map(|end| {
        furthest = furthest.max(end.clamp(0.0, 1.0));
        furthest
    })
    .collect()
}

/// The metres a bar measured by distance runs over.
fn distance_span(group: &LodGroup) -> f32 {
    let furthest = group.levels.iter().map(|level| level.until).fold(0.0, f32::max).max(1.0);
    if group.cull { furthest / (1.0 - PAST_THE_LAST) } else { furthest }
}

/// Where along the bar a view falls.
fn place(group: &LodGroup, view: LodView) -> f32 {
    match group.measure {
        LodMeasure::ScreenSize => 1.0 - view.size.clamp(0.0, 1.0),
        LodMeasure::Distance => view.distance / distance_span(group),
    }
}

/// Where a level ends, as its segment of the bar says it.
fn until(group: &LodGroup, until: f32) -> String {
    match group.measure {
        LodMeasure::ScreenSize => format!("{}%", trim(until * 100.0)),
        LodMeasure::Distance => format!("{} m", trim(until)),
    }
}

fn trim(value: f32) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_string)
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// 12480 as "12,480".
fn thousands(count: u32) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn model(world: &World, entity: Entity) -> voxear_core::math::Mat4 {
    match (world.get::<WorldTransform>(entity), world.get::<Transform>(entity)) {
        (Some(world), _) => world.0,
        (None, Some(local)) => local.matrix(),
        (None, None) => voxear_core::math::Mat4::IDENTITY,
    }
}

fn max_scale(model: voxear_core::math::Mat4) -> f32 {
    let axis = |axis: voxear_core::math::Vec4| Vec3::new(axis.x, axis.y, axis.z).length();
    axis(model.x_axis).max(axis(model.y_axis)).max(axis(model.z_axis))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_with_commas_and_chains_as_a_list() {
        assert_eq!(thousands(12480), "12,480");
        assert_eq!(thousands(1_560_000), "1,560,000");
        assert_eq!(thousands(12), "12");
        let lod = |triangles| LodInfo { triangles, error: 0.0 };
        assert_eq!(chain(&[lod(12480), lod(6240), lod(3120)]), "12,480, 6,240 and 3,120 triangles, the most detailed first");
        assert_eq!(chain(&[lod(12)]), "One LOD, of 12 triangles");
    }

    #[test]
    fn the_bar_puts_each_level_where_it_ends() {
        let mut group = LodGroup::of(&[Entity::from_bits(1), Entity::from_bits(2), Entity::from_bits(3)]);
        let ends = bar_ends(&group);
        assert!((ends[0] - 0.4).abs() < 1e-5 && (ends[1] - 0.7).abs() < 1e-5 && (ends[2] - 0.85).abs() < 1e-5);
        group.cull = false;
        assert_eq!(bar_ends(&group)[2], 1.0, "the last level runs to the end when nothing is culled");
        group.set_measure(LodMeasure::Distance);
        group.cull = true;
        // 20, 40 and 80 m, with a fifth of the bar past 80 m.
        let ends = bar_ends(&group);
        assert!((ends[0] - 0.2).abs() < 1e-5 && (ends[2] - 0.8).abs() < 1e-5, "{ends:?}");
        assert_eq!(until(&group, 20.0), "20 m");
    }
}
