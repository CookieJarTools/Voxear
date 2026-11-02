//! What the Inspector shows of levels of detail, laid out as Unity does.
//! Under a Mesh Renderer, a strip of its mesh's LODs, each with its
//! triangles, the one a camera draws lit; a click on one moves the Scene
//! view to where it is drawn, and a right-click forces it. Under a LOD Group, the bar of its
//! levels: a click selects a level, a drag on an edge moves where it ends, a
//! right-click inserts or deletes one, and a camera mark shows where the
//! Scene view sees the group.

use std::collections::HashMap;

use voxear_core::math::{EulerRot, Mat4, Quat, Vec3, Vec4};
use voxear_ecs::{Entity, World};
use voxear_render::{LodCamera, LodGroup, LodInfo, LodMeasure, LodView, MeshRenderer, Renderer, lod_at};
use voxear_scene::{Transform, WorldTransform};
use voxear_ui::{NodeId, Style, Ui};

use crate::inspector::prop;

/// How much of a bar measured by distance is left for what is past the last
/// level, so the culled part shows.
const PAST_THE_LAST: f32 = 0.2;

/// The colours the bar and the strip have, one a level, repeating.
const COLOURS: usize = 6;

/// The camera the Inspector shows LODs for: the Scene view's in edit mode,
/// and the game's while playing.
#[derive(Clone, Copy, Debug)]
pub struct LodEye {
    pub camera: LodCamera,
    /// The view's height in pixels over twice the tangent of half its field
    /// of view: the pixels a metre covers a metre away, which the GPU picks
    /// a mesh's LOD by.
    pub pixels_per_metre: f32,
    /// The LOD error the view allows, in pixels.
    pub threshold: f32,
}

impl LodEye {
    /// The same camera for a renderer with a LOD bias, which scales the
    /// error it allows as the GPU does.
    pub fn biased(self, bias: f32) -> Self {
        Self { threshold: self.threshold * bias.exp2(), ..self }
    }
}

/// A Mesh Renderer's LODs row: a cell for each LOD of its mesh with its
/// triangles, the one drawn now lit.
pub struct MeshLods {
    strip: NodeId,
    cells: Vec<NodeId>,
    lods: Vec<LodInfo>,
    /// The LOD its renderer forces, if any.
    forced: Option<usize>,
}

impl MeshLods {
    pub fn new(ui: &mut Ui, rows: NodeId) -> Self {
        let row = prop(ui, rows, "LODs");
        let strip = ui.panel(row, Style::row());
        ui.add_class(strip, "lod-strip");
        Self { strip, cells: Vec::new(), lods: Vec::new(), forced: None }
    }

    pub fn update(&mut self, ui: &mut Ui, world: &World, entity: Entity, renderer: &Renderer, eye: Option<LodEye>) {
        let Some(mesh) = world.get::<MeshRenderer>(entity).map(|r| r.mesh) else { return };
        let lods = renderer.mesh_lods(mesh).unwrap_or_default();
        if lods != self.lods {
            for cell in self.cells.drain(..) {
                ui.remove(cell);
            }
            for (level, lod) in lods.iter().enumerate() {
                let cell = ui.add_label(self.strip, &short_count(lod.triangles));
                ui.add_class(cell, "lod-cell");
                ui.add_class(cell, &format!("lod-level-{}", level % COLOURS));
                self.cells.push(cell);
            }
            self.lods = lods;
        }
        let settings = world.get::<MeshRenderer>(entity).copied();
        let forced = settings.and_then(|r| r.forced_lod).map(|lod| usize::from(lod).min(self.lods.len().saturating_sub(1)));
        let drawn = match (eye.zip(settings), renderer.mesh_sphere(mesh)) {
            _ if forced.is_some() => forced,
            (Some((eye, settings)), Some((center, radius))) if !self.lods.is_empty() => {
                let model = model(world, entity);
                let scale = max_scale(model);
                let away = model.transform_point3(center).distance(eye.camera.position) - radius * scale;
                let eye = eye.biased(settings.lod_bias);
                Some(lod_at(&self.lods, away, scale, eye.pixels_per_metre, eye.threshold))
            }
            _ => None,
        };
        for (level, &cell) in self.cells.iter().enumerate() {
            ui.set_class(cell, "lod-drawn", drawn == Some(level));
            ui.set_class(cell, "lod-forced", forced == Some(level));
        }
        self.forced = forced;
    }

    /// The LOD whose cell was right-clicked, the cell, and the LOD forced now.
    pub fn context_clicked(&self, ui: &Ui) -> Option<(usize, NodeId, Option<usize>)> {
        let level = self.cells.iter().position(|&cell| ui.context_clicked(cell))?;
        Some((level, self.cells[level], self.forced))
    }

    /// The LOD whose cell was clicked.
    pub fn clicked(&self, ui: &Ui) -> Option<usize> {
        self.cells.iter().position(|&cell| ui.clicked(cell))
    }

    /// A cell's LOD and triangles, while the pointer rests on it.
    pub fn hovered_tooltip(&self, ui: &Ui) -> Option<(String, NodeId)> {
        let (level, &cell) = self.cells.iter().enumerate().find(|(_, cell)| ui.is_hovered(**cell))?;
        Some((format!("LOD {level}: {} triangles", thousands(self.lods[level].triangles)), cell))
    }
}

/// Where a camera goes to see LOD `level` of an Obj's mesh drawn, and how
/// it turns to look at it: on the line from the mesh to where the camera is
/// now, inside the distances that LOD is drawn at.
pub fn view_lod(world: &World, entity: Entity, renderer: &Renderer, level: usize, eye: LodEye) -> Option<(Vec3, Quat)> {
    let settings = world.get::<MeshRenderer>(entity)?;
    let (mesh, eye) = (settings.mesh, eye.biased(settings.lod_bias));
    let (lods, (center, radius)) = (renderer.mesh_lods(mesh)?, renderer.mesh_sphere(mesh)?);
    let model = model(world, entity);
    let scale = max_scale(model);
    let center = model.transform_point3(center);
    let away = distance_for(&lods, level, scale, eye) + radius * scale;
    let back = (eye.camera.position - center).try_normalize().unwrap_or(Vec3::new(0.0, 0.3, -1.0).normalize());
    let forward = -back;
    let rotation = Quat::from_euler(EulerRot::YXZ, forward.x.atan2(forward.z), -forward.y.asin(), 0.0);
    Some((center + back * away, rotation))
}

/// How far from a mesh's bounding sphere LOD `level` is drawn, well inside
/// where it starts and where the next takes over: halfway to the second
/// for the first, between the two for the rest, and past the start of the last.
fn distance_for(lods: &[LodInfo], level: usize, scale: f32, eye: LodEye) -> f32 {
    let from = |lod: &LodInfo| lod.from_distance(scale, eye.pixels_per_metre, eye.threshold);
    let level = level.min(lods.len().saturating_sub(1));
    let (start, next) = (lods.get(level).map_or(0.0, from), lods.get(level + 1).map(from));
    match next {
        _ if lods.len() < 2 => 1.0,
        Some(next) if level == 0 => next * 0.5,
        Some(next) if next > start * 1.02 => (start * next).sqrt(),
        _ => start * 1.5,
    }
}

/// A LOD Group's bar, its edges and the camera's mark.
pub struct GroupLods {
    bar: NodeId,
    segments: Vec<NodeId>,
    culled: NodeId,
    /// The edge at the end of each level, which a drag moves.
    edges: Vec<NodeId>,
    marker: NodeId,
    eye: NodeId,
    eye_text: NodeId,
    /// What its levels were measured by when it was built.
    pub measure: LodMeasure,
    /// The level whose edge is being dragged, and the metres the bar ran
    /// over when the drag began, so the bar holds still under the pointer.
    drag: Option<(usize, f32)>,
    /// The inline style each part was last given, so an unchanged one is
    /// not styled again every frame.
    styled: HashMap<NodeId, String>,
}

impl GroupLods {
    /// The bar, with `selected` lit, and the ruler over it for the camera.
    pub fn new(ui: &mut Ui, rows: NodeId, group: &LodGroup, selected: usize) -> Self {
        let ruler = ui.panel(rows, Style::row());
        ui.add_class(ruler, "lod-ruler");
        let eye = ui.panel(ruler, Style::row());
        ui.add_class(eye, "lod-eye");
        let icon = ui.icon(eye, "camera");
        ui.add_class(icon, "lod-eye-icon");
        let eye_text = ui.add_label(eye, "");
        ui.add_class(eye_text, "lod-eye-text");

        let bar = ui.panel(rows, Style::row());
        ui.add_class(bar, "lod-bar");
        let segment = |ui: &mut Ui, text: &str, class: &str| {
            let segment = ui.panel(bar, Style::column());
            ui.add_class(segment, "lod-segment");
            ui.add_class(segment, class);
            let name = ui.add_label(segment, text);
            ui.add_class(name, "lod-segment-name");
            let end = ui.add_label(segment, "");
            ui.add_class(end, "lod-segment-end");
            segment
        };
        let segments: Vec<NodeId> = (0..group.levels.len())
            .map(|level| segment(ui, &format!("LOD {level}"), &format!("lod-level-{}", level % COLOURS)))
            .collect();
        if let Some(&chosen) = segments.get(selected) {
            ui.add_class(chosen, "lod-selected");
        }
        let culled = segment(ui, "Culled", "lod-culled");
        let edges = (0..group.levels.len())
            .map(|_| {
                let edge = ui.panel(bar, Style::column());
                ui.add_class(edge, "lod-edge");
                let line = ui.panel(edge, Style::default());
                ui.add_class(line, "lod-edge-line");
                edge
            })
            .collect();
        let marker = ui.panel(bar, Style::default());
        ui.add_class(marker, "lod-marker");
        let styled = HashMap::new();
        Self { bar, segments, culled, edges, marker, eye, eye_text, measure: group.measure, drag: None, styled }
    }

    /// The level whose part of the bar was clicked.
    pub fn clicked_level(&self, ui: &Ui) -> Option<usize> {
        self.segments.iter().position(|&segment| ui.clicked(segment))
    }

    /// The level right-clicked, on its part of the bar or its edge, or one
    /// past the last for the culled part or an empty bar, and what was
    /// clicked.
    pub fn context_clicked(&self, ui: &Ui) -> Option<(usize, NodeId)> {
        let on = |parts: &[NodeId]| parts.iter().position(|&part| ui.context_clicked(part)).map(|at| (at, parts[at]));
        on(&self.edges)
            .or_else(|| on(&self.segments))
            .or_else(|| ui.context_clicked(self.bar).then_some((self.segments.len(), self.bar)))
    }

    /// While an edge is dragged, its level and where the level now ends,
    /// kept between the levels on either side.
    pub fn dragged(&mut self, ui: &Ui, group: &LodGroup) -> Option<(usize, f32)> {
        let Some(level) = self.edges.iter().position(|&edge| ui.is_pressed(edge)) else {
            self.drag = None;
            return None;
        };
        let span = self.drag.filter(|(at, _)| *at == level).map_or_else(|| distance_span(group), |(_, span)| span);
        self.drag = Some((level, span));
        let (x, _) = ui.pointer_position()?;
        let [left, _, width, _] = ui.rect(self.bar);
        let along = ((x - left) / width.max(1.0)).clamp(0.0, 1.0);
        let until = |at: usize| group.levels.get(at).map(|l| l.until);
        let until = match group.measure {
            LodMeasure::ScreenSize => {
                let (above, below) = (level.checked_sub(1).and_then(until).unwrap_or(1.0), until(level + 1).unwrap_or(0.0));
                (1.0 - along).clamp(below + 0.001, (above - 0.001).max(below + 0.001))
            }
            LodMeasure::Distance => {
                let (nearer, further) = (level.checked_sub(1).and_then(until).unwrap_or(0.0), until(level + 1));
                let furthest = further.map_or(f32::MAX, |f| f - 0.01);
                (along * span).clamp(nearer + 0.01, furthest.max(nearer + 0.01))
            }
        };
        Some((level, until))
    }

    /// Sizes the bar's levels to where each ends, and marks where `eye`
    /// sees the group, if it is given.
    pub fn update(&mut self, ui: &mut Ui, group: &LodGroup, view: Option<(LodView, Option<usize>)>) {
        let ends = bar_ends(group);
        let mut from = 0.0;
        for (level, (&segment, &end)) in self.segments.clone().iter().zip(&ends).enumerate() {
            self.style(ui, segment, span_style(from, end));
            // Without culling, the last level has no end.
            let ends_here = group.cull || level + 1 < group.levels.len();
            let text = group.levels.get(level).filter(|_| ends_here).map_or_else(String::new, |l| until(group, l.until));
            let label = ui.children(segment)[1];
            ui.set_text(label, &text);
            if let Some(&edge) = self.edges.get(level) {
                ui.set_visible(edge, ends_here);
                self.style(ui, edge, format!("left: {:.2}%", end * 100.0));
            }
            from = end;
        }
        ui.set_visible(self.culled, group.cull && from < 1.0);
        self.style(ui, self.culled, span_style(from, 1.0));

        let Some((view, level)) = view else {
            ui.set_visible(self.marker, false);
            ui.set_visible(self.eye, false);
            return;
        };
        let at = place(group, view).clamp(0.0, 1.0);
        ui.set_visible(self.marker, true);
        ui.set_visible(self.eye, true);
        self.style(ui, self.marker, format!("left: {:.2}%", at * 100.0));
        // The mark stays inside the ruler near its right end.
        self.style(ui, self.eye, format!("left: {:.2}%", at.min(0.86) * 100.0));
        let reading = match group.measure {
            LodMeasure::ScreenSize => format!("{}%", trim(view.size * 100.0)),
            LodMeasure::Distance => format!("{} m", trim(view.distance)),
        };
        ui.set_text(self.eye_text, &reading);
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

/// Places a part of the bar from one point along it to another, exactly,
/// so the edges dragged between levels sit on their boundaries.
fn span_style(from: f32, to: f32) -> String {
    format!("left: {:.3}%; width: {:.3}%", from * 100.0, (to - from).max(0.0) * 100.0)
}

/// Where along the bar a view falls.
fn place(group: &LodGroup, view: LodView) -> f32 {
    match group.measure {
        LodMeasure::ScreenSize => 1.0 - view.size.clamp(0.0, 1.0),
        LodMeasure::Distance => view.distance / distance_span(group),
    }
}

/// Where a level ends, as its part of the bar says it.
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

/// A triangle count short enough for a cell: 512, 4.1k, 41k, 1.2M.
fn short_count(count: u32) -> String {
    let count = count as f32;
    match count {
        c if c < 1000.0 => format!("{c}"),
        c if c < 10_000.0 => format!("{}k", trim(c / 1000.0)),
        c if c < 1_000_000.0 => format!("{:.0}k", c / 1000.0),
        c => format!("{}M", trim(c / 1_000_000.0)),
    }
}

fn model(world: &World, entity: Entity) -> Mat4 {
    match (world.get::<WorldTransform>(entity), world.get::<Transform>(entity)) {
        (Some(world), _) => world.0,
        (None, Some(local)) => local.matrix(),
        (None, None) => Mat4::IDENTITY,
    }
}

fn max_scale(model: Mat4) -> f32 {
    let axis = |axis: Vec4| Vec3::new(axis.x, axis.y, axis.z).length();
    axis(model.x_axis).max(axis(model.y_axis)).max(axis(model.z_axis))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_short_in_cells_and_whole_in_tooltips() {
        assert_eq!(thousands(12480), "12,480");
        assert_eq!(thousands(1_560_000), "1,560,000");
        assert_eq!(thousands(12), "12");
        assert_eq!(short_count(321), "321");
        assert_eq!(short_count(4096), "4.1k");
        assert_eq!(short_count(2000), "2k");
        assert_eq!(short_count(41_200), "41k");
        assert_eq!(short_count(1_250_000), "1.2M");
    }

    #[test]
    fn each_lod_is_viewed_from_a_distance_the_gpu_draws_it_at() {
        let lods: Vec<LodInfo> = [(4096, 0.0), (2048, 0.004), (1024, 0.02), (512, 0.09)]
            .map(|(triangles, error)| LodInfo { triangles, error })
            .to_vec();
        let eye = LodEye { camera: LodCamera::new(Vec3::ZERO, 60.0), pixels_per_metre: 700.0, threshold: 1.0 };
        for level in 0..lods.len() {
            let away = distance_for(&lods, level, 1.0, eye);
            assert_eq!(lod_at(&lods, away, 1.0, eye.pixels_per_metre, eye.threshold), level, "{away} m");
        }
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
