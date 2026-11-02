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
use voxear_render::{
    LodCamera, LodGroup, LodInfo, LodMeasure, LodView, MAX_TRANSITIONS, MeshRenderer, Renderer, lod_at, lod_at_share,
    screen_share,
};
use voxear_scene::{Transform, WorldTransform};
use voxear_ui::{NodeId, Style, Ui};


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

/// A Mesh Renderer's LODs as a bar, as a LOD Group's levels show: a part
/// for each LOD of its mesh with its triangles, from the whole screen at
/// the left to nothing at the right, the one drawn now outlined and the
/// camera's mark over it. Its edges show where the engine switches LODs
/// for the camera, until one is dragged, which gives the renderer ends of
/// its own.
pub struct MeshLods {
    bar: NodeId,
    eye: NodeId,
    eye_text: NodeId,
    marker: NodeId,
    segments: Vec<NodeId>,
    edges: Vec<NodeId>,
    lods: Vec<LodInfo>,
    /// Where each LOD but the last ends, as shown now, as a share of the
    /// screen's height.
    ends: Vec<f32>,
    /// Its renderer says where its LODs end, rather than the engine.
    custom: bool,
    /// The LOD its renderer forces, if any.
    forced: Option<usize>,
    /// The edge being dragged.
    drag: Option<usize>,
    styled: HashMap<NodeId, String>,
}

impl MeshLods {
    pub fn new(ui: &mut Ui, rows: NodeId) -> Self {
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
        let marker = ui.panel(bar, Style::default());
        ui.add_class(marker, "lod-marker");
        Self {
            bar,
            eye,
            eye_text,
            marker,
            segments: Vec::new(),
            edges: Vec::new(),
            lods: Vec::new(),
            ends: Vec::new(),
            custom: false,
            forced: None,
            drag: None,
            styled: HashMap::new(),
        }
    }

    pub fn update(&mut self, ui: &mut Ui, world: &World, entity: Entity, renderer: &Renderer, eye: Option<LodEye>) {
        let Some(settings) = world.get::<MeshRenderer>(entity).copied() else { return };
        let lods = renderer.mesh_lods(settings.mesh).unwrap_or_default();
        if lods.len() != self.segments.len() {
            self.build(ui, lods.len());
        }
        self.lods = lods;
        let count = self.lods.len();
        let sphere = renderer.mesh_sphere(settings.mesh);
        let model = model(world, entity);
        let scale = max_scale(model);
        // The mesh's sphere in the world, and how much of the screen the camera sees it cover.
        let (center, radius) = sphere.map_or((Vec3::ZERO, 0.0), |(c, r)| (model.transform_point3(c), r * scale));
        let share = eye.map(|eye| screen_share(radius, center.distance(eye.camera.position), eye.camera.tan_half_fov));
        self.custom = settings.lod_transitions.is_some();
        self.ends = match (settings.lod_transitions, eye) {
            (Some(ends), _) => ends[..count.saturating_sub(1).min(MAX_TRANSITIONS)].to_vec(),
            // Where the engine switches, for this camera: where each LOD's error shrinks to the threshold.
            (None, Some(eye)) => {
                let eye = eye.biased(settings.lod_bias);
                let start = |lod: &LodInfo| lod.from_distance(scale, eye.pixels_per_metre, eye.threshold) + radius;
                self.lods.iter().skip(1).map(|lod| screen_share(radius, start(lod), eye.camera.tan_half_fov)).collect()
            }
            (None, None) => (1..count).map(|lod| 1.0 - lod as f32 / count as f32).collect(),
        };
        self.forced = settings.forced_lod.map(|lod| usize::from(lod).min(count.saturating_sub(1)));
        let drawn = match (self.forced, share) {
            (Some(forced), _) => Some(forced),
            (None, Some(share)) if self.custom => Some(lod_at_share(&self.ends, count, share)),
            (None, Some(_)) => eye.map(|eye| {
                let eye = eye.biased(settings.lod_bias);
                let away = center.distance(eye.camera.position) - radius;
                lod_at(&self.lods, away, scale, eye.pixels_per_metre, eye.threshold)
            }),
            (None, None) => None,
        };
        self.show(ui, drawn, share);
    }

    /// A part and an edge for each LOD, the last with no edge.
    fn build(&mut self, ui: &mut Ui, count: usize) {
        for node in self.segments.drain(..).chain(self.edges.drain(..)) {
            ui.remove(node);
        }
        self.styled.clear();
        for level in 0..count {
            let segment = ui.panel(self.bar, Style::column());
            ui.add_class(segment, "lod-segment");
            ui.add_class(segment, &format!("lod-level-{}", level % COLOURS));
            let name = ui.add_label(segment, &format!("LOD {level}"));
            ui.add_class(name, "lod-segment-name");
            let end = ui.add_label(segment, "");
            ui.add_class(end, "lod-segment-end");
            self.segments.push(segment);
        }
        for _ in 1..count {
            let edge = ui.panel(self.bar, Style::column());
            ui.add_class(edge, "lod-edge");
            let line = ui.panel(edge, Style::default());
            ui.add_class(line, "lod-edge-line");
            self.edges.push(edge);
        }
        // The marker over the parts, as it was before them.
        let marker = ui.panel(self.bar, Style::default());
        ui.add_class(marker, "lod-marker");
        ui.remove(self.marker);
        self.marker = marker;
    }

    fn show(&mut self, ui: &mut Ui, drawn: Option<usize>, share: Option<f32>) {
        let mut from = 0.0f32;
        for level in 0..self.segments.len() {
            let to = self.ends.get(level).map_or(1.0, |end| (1.0 - end.clamp(0.0, 1.0)).max(from));
            let segment = self.segments[level];
            self.style(ui, segment, span_style(from, to));
            let triangles = short_count(self.lods.get(level).map_or(0, |lod| lod.triangles));
            let text = match self.ends.get(level) {
                Some(&end) => format!("{triangles} · {}%", trim(end * 100.0)),
                None => triangles,
            };
            let label = ui.children(segment)[1];
            ui.set_text(label, &text);
            ui.set_class(segment, "lod-drawn", drawn == Some(level));
            ui.set_class(segment, "lod-forced", self.forced == Some(level));
            if let Some(&edge) = self.edges.get(level) {
                self.style(ui, edge, format!("left: {:.2}%", to * 100.0));
            }
            from = to;
        }
        let Some(share) = share else {
            ui.set_visible(self.marker, false);
            ui.set_visible(self.eye, false);
            return;
        };
        let at = (1.0 - share.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        ui.set_visible(self.marker, true);
        ui.set_visible(self.eye, true);
        self.style(ui, self.marker, format!("left: {:.2}%", at * 100.0));
        self.style(ui, self.eye, format!("left: {:.2}%", at.min(0.86) * 100.0));
        ui.set_text(self.eye_text, &format!("{}%", trim(share * 100.0)));
    }

    /// While an edge is dragged, where every LOD ends with it moved, kept
    /// between the LODs on either side: the renderer's own ends from then.
    pub fn dragged(&mut self, ui: &Ui) -> Option<[f32; MAX_TRANSITIONS]> {
        let Some(level) = self.edges.iter().position(|&edge| ui.is_pressed(edge)) else {
            self.drag = None;
            return None;
        };
        self.drag = Some(level);
        let (x, _) = ui.pointer_position()?;
        let [left, _, width, _] = ui.rect(self.bar);
        let along = ((x - left) / width.max(1.0)).clamp(0.0, 1.0);
        let above = level.checked_sub(1).and_then(|before| self.ends.get(before).copied()).unwrap_or(1.0);
        let below = self.ends.get(level + 1).copied().unwrap_or(0.0);
        let mut ends = [0.0; MAX_TRANSITIONS];
        for (end, &shown) in ends.iter_mut().zip(&self.ends) {
            *end = shown.clamp(0.0, 1.0);
        }
        ends[level] = (1.0 - along).clamp(below + 0.001, (above - 0.001).max(below + 0.001));
        Some(ends)
    }

    /// The LOD whose part of the bar was clicked.
    pub fn clicked(&self, ui: &Ui) -> Option<usize> {
        self.segments.iter().position(|&segment| ui.clicked(segment))
    }

    /// The LOD right-clicked, on its part or its edge, what was clicked, the
    /// LOD forced now, and whether the renderer says where its LODs end.
    pub fn context_clicked(&self, ui: &Ui) -> Option<(usize, NodeId, Option<usize>, bool)> {
        let on = |parts: &[NodeId]| parts.iter().position(|&part| ui.context_clicked(part)).map(|at| (at, parts[at]));
        let (level, node) = on(&self.edges).or_else(|| on(&self.segments))?;
        Some((level, node, self.forced, self.custom))
    }

    /// A LOD's triangles, while the pointer rests on its part of the bar.
    pub fn hovered_tooltip(&self, ui: &Ui) -> Option<(String, NodeId)> {
        let (level, &part) = self.segments.iter().enumerate().find(|(_, part)| ui.is_hovered(**part))?;
        let triangles = self.lods.get(level).map_or(0, |lod| lod.triangles);
        Some((format!("LOD {level}: {} triangles", thousands(triangles)), part))
    }

    fn style(&mut self, ui: &mut Ui, node: NodeId, style: String) {
        if self.styled.get(&node) != Some(&style) {
            ui.set_inline_style(node, &style);
            self.styled.insert(node, style);
        }
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
    let away = match settings.lod_transitions {
        // Halfway down the share of the screen the renderer gives the LOD.
        Some(ends) => {
            let last = lods.len().saturating_sub(1);
            let level = level.min(last);
            let top = level.checked_sub(1).map_or(1.0, |before| ends[before]).min(1.0);
            let bottom = if level < last { ends[level] } else { 0.0 };
            let share = ((top + bottom) * 0.5).max(1e-3);
            radius * scale / (share * eye.camera.tan_half_fov)
        }
        None => distance_for(&lods, level, scale, eye) + radius * scale,
    };
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
