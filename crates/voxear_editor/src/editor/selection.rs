//! Several Objs selected at once, as in Unity: Ctrl or Shift and a click in
//! the Hierarchy or the Scene view, or a box dragged over the Scene view.
//! One of them is the active Obj, which the Inspector shows and the handles
//! sit on; the rest follow what the handles do to it, as one step to undo,
//! and Delete and Duplicate act on them all. Each selected Obj's bounds are
//! drawn in the Scene view while there are several.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Foliage, Light, MeshRenderer, ModelRenderer, Renderer};
use voxear_scene::{Destroying, EditorOnly, Name, Parent, Transform, WorldTransform};
use voxear_ui::{Color, Shape};

use super::Editor;
use crate::{
    history::{Edit, ObjState},
    objs,
    scene_tools::ViewCamera,
};

/// How far the pointer moves, pressed on the Scene view, before it drags a
/// box rather than clicks.
const BOX_START: f32 = 4.0;
/// The box and the bounds of what is selected.
const BOX_COLOR: u32 = 0x8b93ff;

/// A box being dragged over the Scene view, from where the press began to
/// the pointer, in the window's logical pixels.
#[derive(Clone, Copy)]
pub(super) struct Marquee {
    from: [f32; 2],
    to: [f32; 2],
    /// Past [`BOX_START`], so a box rather than a click.
    dragging: bool,
}

/// The other selected Objs while the handles drag the active one: each
/// with its Transform and place in the world when the drag began, and its
/// parent's place, to put its new place back in its parent's terms.
pub(super) struct Followers {
    active_start: Mat4,
    objs: Vec<(Entity, Transform, Mat4, Mat4)>,
}

impl Editor {
    /// Every selected Obj, the active one first.
    pub fn selected_objs(&self) -> Vec<Entity> {
        self.selection.into_iter().chain(self.others.iter().copied()).collect()
    }

    /// Selects these Objs, the first the active one, in every Hierarchy.
    pub fn select_many(&mut self, entities: Vec<Entity>) {
        let world = &self.game.engine.world;
        let mut chosen: Vec<Entity> = Vec::new();
        for entity in entities {
            if world.is_alive(entity) && !chosen.contains(&entity) {
                chosen.push(entity);
            }
        }
        if chosen.len() <= 1 {
            return self.select(chosen.first().copied());
        }
        self.refresh_hierarchies();
        self.others = chosen[1..].to_vec();
        self.select_many_in_hierarchies(&chosen);
        self.show_selection(Some(chosen[0]));
    }

    /// Deletes what is selected, as the Delete key does: every selected Obj,
    /// as one step to undo.
    pub fn delete_selection(&mut self, renderer: &mut Renderer) {
        self.run(super::Command::Delete, renderer);
    }

    /// Adds an Obj to the selection, as Shift and a click do, or takes it
    /// out when `toggle` and it is in, as Ctrl and a click do.
    pub(super) fn add_to_selection(&mut self, entity: Entity, toggle: bool) {
        let mut chosen = self.selected_objs();
        match chosen.iter().position(|&e| e == entity) {
            Some(at) if toggle => {
                chosen.remove(at);
            }
            Some(_) => {}
            None => chosen.push(entity),
        }
        self.select_many(chosen);
    }

    /// Hears a selection made in the editor's `hierarchy` of several Objs,
    /// keeping the active one when it is still among them. Returns whether
    /// there were several.
    pub(super) fn hear_hierarchy_many(&mut self) -> bool {
        let all = self.hierarchy.selected_objs(&self.ui);
        if all.len() < 2 {
            self.others.clear();
            return false;
        }
        let active = self.selected().filter(|active| all.contains(active)).unwrap_or(all[0]);
        let chosen: Vec<Entity> = std::iter::once(active).chain(all.into_iter().filter(|&e| e != active)).collect();
        self.others = chosen[1..].to_vec();
        self.select_many_in_other_hierarchies(&chosen);
        if self.selected() != Some(active) {
            self.show_selection(Some(active));
        }
        true
    }

    /// The selected Objs that no other selected Obj holds, as deleting or
    /// duplicating an Obj takes what is in it along.
    fn outermost_selected(&self) -> Vec<Entity> {
        let world = &self.game.engine.world;
        let chosen = self.selected_objs();
        let held = |entity: Entity| {
            std::iter::successors(world.get::<Parent>(entity).map(|p| p.0), |&up| world.get::<Parent>(up).map(|p| p.0))
                .any(|up| chosen.contains(&up))
        };
        chosen.iter().copied().filter(|&entity| world.is_alive(entity) && !held(entity)).collect()
    }

    /// Deletes every selected Obj, as one step to undo. False when there
    /// are not several, which deleting one does as before.
    pub(super) fn delete_many(&mut self) -> bool {
        if self.others.is_empty() {
            return false;
        }
        let taken: Vec<Edit> = self
            .outermost_selected()
            .into_iter()
            .map(|entity| Edit::Exists { root: entity, taken: Some(objs::take(&mut self.game, entity)) })
            .collect();
        if !self.is_playing() {
            self.history.record_step(Edit::Group(taken));
            self.unsaved = true;
        }
        self.select(None);
        true
    }

    /// Duplicates every selected Obj, as one step to undo, and selects the
    /// copies. False when there are not several.
    pub(super) fn duplicate_many(&mut self, renderer: &mut Renderer) -> bool {
        if self.others.is_empty() {
            return false;
        }
        let mut made = Vec::new();
        for entity in self.outermost_selected() {
            match objs::duplicate(&mut self.game, Some(&mut *renderer), entity) {
                Ok(copy) => made.push(copy),
                Err(error) => tracing::error!("could not duplicate the Obj: {error:#}"),
            }
        }
        if !self.is_playing() {
            let edits = made.iter().map(|&copy| Edit::Exists { root: copy, taken: None }).collect();
            self.history.record_step(Edit::Group(edits));
            self.unsaved = true;
        }
        self.select_many(made);
        true
    }

    /// Notes where the other selected Objs are as the handles begin to drag
    /// the active one, so they can follow it.
    pub(super) fn begin_following(&mut self) {
        let world = &self.game.engine.world;
        let (Some(active), true) = (self.selection, !self.others.is_empty()) else { return };
        let Some(active_start) = world_matrix(world, active) else { return };
        let objs = self
            .outermost_selected()
            .into_iter()
            .filter(|&entity| entity != active)
            .filter_map(|entity| {
                let transform = *world.get::<Transform>(entity)?;
                Some((entity, transform, world_matrix(world, entity)?, parent_matrix(world, entity)))
            })
            .collect();
        self.followers = Some(Followers { active_start, objs });
    }

    /// Moves, turns or sizes the other selected Objs as the active one has
    /// been since the drag began, about where it stands.
    pub(super) fn follow_active(&mut self) {
        let Some(followers) = &self.followers else { return };
        let world = &mut self.game.engine.world;
        let Some(active) = self.selection.and_then(|active| world_matrix(world, active)) else { return };
        let change = active * followers.active_start.inverse();
        for &(entity, _, start, parent) in &followers.objs {
            let (scale, rotation, position) = (parent.inverse() * change * start).to_scale_rotation_translation();
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                (transform.position, transform.rotation, transform.scale) = (position, rotation, scale);
            }
        }
    }

    /// What the drag did to the other selected Objs, as edits to undo with
    /// the active one's.
    pub(super) fn end_following(&mut self) -> Vec<Edit> {
        let Some(followers) = self.followers.take() else { return Vec::new() };
        let world = &self.game.engine.world;
        followers
            .objs
            .into_iter()
            .filter_map(|(entity, start, ..)| {
                let after = ObjState::of(world, entity)?;
                let before = ObjState { transform: start, ..after.clone() };
                (before != after).then_some(Edit::Changed { entity, before, after })
            })
            .collect()
    }

    /// A box dragged over the Scene view from a press on nothing: drawn
    /// while dragged, and on letting go selects what is in it, added to the
    /// selection with Ctrl or Shift. Returns whether a box was let go, so
    /// the release is not also a click.
    pub(super) fn update_marquee(&mut self, renderer: &Renderer, camera: &ViewCamera, point: Option<[f32; 2]>) -> bool {
        let pressed = self.ui.is_pressed(self.scene.viewport);
        let free = !self.tools.is_dragging() && !self.field_handles.is_dragging() && !self.script_handles.is_dragging();
        let over = self.over_viewport();
        match (&mut self.marquee, pressed && free, point) {
            (None, true, Some(point)) if over => {
                self.marquee = Some(Marquee { from: point, to: point, dragging: false });
            }
            (Some(marquee), true, Some(point)) => {
                marquee.to = point;
                let moved = (point[0] - marquee.from[0]).hypot(point[1] - marquee.from[1]);
                marquee.dragging |= moved > BOX_START;
            }
            (Some(_), false, _) => {
                let marquee = self.marquee.take().expect("matched");
                if !marquee.dragging {
                    return false;
                }
                let inside = objs_in_box(&self.game.engine.world, renderer, camera, marquee.from, marquee.to);
                let chosen = match self.control || self.shift {
                    true => self.selected_objs().into_iter().chain(inside).collect(),
                    false => inside,
                };
                self.select_many(chosen);
                return true;
            }
            _ => {}
        }
        false
    }

    /// The box being dragged, and each selected Obj's bounds while there
    /// are several, as shapes in the view's own coordinates.
    pub(super) fn selection_shapes(&self, renderer: &Renderer, camera: &ViewCamera) -> Vec<Shape> {
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let color = Color::hex(BOX_COLOR);
        let mut shapes = Vec::new();
        if !self.others.is_empty() {
            for entity in self.selected_objs() {
                shapes.extend(bounds_lines(&self.game.engine.world, renderer, camera, entity, color, local));
            }
        }
        if let Some(marquee) = self.marquee.filter(|marquee| marquee.dragging) {
            let [left, top, right, bottom] = box_of(local(marquee.from), local(marquee.to));
            let fill = color.with_alpha(28);
            shapes.push(Shape::Rect { rect: [left, top, right - left, bottom - top], radius: 0.0, fill });
            let corners = [[left, top], [right, top], [right, bottom], [left, bottom]];
            for at in 0..4 {
                shapes.push(Shape::Line { from: corners[at], to: corners[(at + 1) % 4], width: 1.0, color });
            }
        }
        shapes
    }
}

/// An Obj's place in the world now, from its Transform and its parent's
/// place, as the frame's own may not have caught up yet.
fn world_matrix(world: &World, entity: Entity) -> Option<Mat4> {
    Some(parent_matrix(world, entity) * world.get::<Transform>(entity)?.matrix())
}

fn parent_matrix(world: &World, entity: Entity) -> Mat4 {
    world.get::<Parent>(entity).and_then(|p| world.get::<WorldTransform>(p.0)).map_or(Mat4::IDENTITY, |w| w.0)
}

/// Left, top, right and bottom of the box between two corners.
fn box_of(from: [f32; 2], to: [f32; 2]) -> [f32; 4] {
    [from[0].min(to[0]), from[1].min(to[1]), from[0].max(to[0]), from[1].max(to[1])]
}

/// The box round what an Obj draws, in its own space: its mesh's, every
/// part of the model it draws, or every plant of its Foliage.
pub(crate) fn drawn_bounds(world: &World, renderer: &Renderer, entity: Entity) -> Option<(Vec3, Vec3)> {
    if let Some(mesh) = world.get::<MeshRenderer>(entity) {
        return renderer.mesh_bounds(mesh.mesh);
    }
    if let Some(model) = world.get::<ModelRenderer>(entity) {
        return model.bounds(renderer);
    }
    let foliage = world.get::<Foliage>(entity)?;
    let (min, max) = renderer.mesh_bounds(foliage.mesh)?;
    // Each plant's mesh as far as it can reach, whichever way it is turned.
    let reach = min.abs().max(max.abs());
    let reach = Vec3::new(reach.x.max(reach.z), reach.y, reach.x.max(reach.z));
    let mut found: Option<(Vec3, Vec3)> = None;
    for plant in foliage.plants.iter() {
        let at = Vec3::from(plant.position);
        let (low, high) = (at - reach * plant.scale, at + reach * plant.scale);
        found = Some(found.map_or((low, high), |(a, b)| (a.min(low), b.max(high))));
    }
    found
}

/// The corners of an Obj's mesh's bounds in the world, or none for an Obj
/// without a mesh.
fn bounds_corners(world: &World, renderer: &Renderer, entity: Entity) -> Option<[Vec3; 8]> {
    let (min, max) = drawn_bounds(world, renderer, entity)?;
    let matrix = world.get::<WorldTransform>(entity).map(|w| w.0)?;
    Some(std::array::from_fn(|at| {
        let pick = |bit: usize, low: f32, high: f32| if at & bit == 0 { low } else { high };
        matrix.transform_point3(Vec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z)))
    }))
}

/// The edges of an Obj's bounds on screen.
fn bounds_lines(
    world: &World,
    renderer: &Renderer,
    camera: &ViewCamera,
    entity: Entity,
    color: Color,
    local: impl Fn([f32; 2]) -> [f32; 2],
) -> Vec<Shape> {
    let Some(corners) = bounds_corners(world, renderer, entity) else { return Vec::new() };
    let edges = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    edges
        .into_iter()
        .filter_map(|(a, b)| {
            let (from, to) = (camera.project(corners[a])?, camera.project(corners[b])?);
            Some(Shape::Line { from: local(from), to: local(to), width: 1.0, color: color.with_alpha(180) })
        })
        .collect()
}

/// The Objs whose bounds on screen, or whose place for one without a mesh,
/// fall in the box: those the Scene view shows, a mesh, a light or a
/// camera, and not the editor's own.
fn objs_in_box(world: &World, renderer: &Renderer, camera: &ViewCamera, from: [f32; 2], to: [f32; 2]) -> Vec<Entity> {
    let [left, top, right, bottom] = box_of(from, to);
    let overlaps = |[x0, y0, x1, y1]: [f32; 4]| x0 <= right && x1 >= left && y0 <= bottom && y1 >= top;
    let shown =
        |entity| drawn_bounds(world, renderer, entity).is_some() || world.has::<Light>(entity) || world.has::<Camera>(entity);
    world
        .query_ref::<(Entity, &Name, &Transform)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .filter(|&(entity, ..)| shown(entity))
        .filter(|&(entity, ..)| {
            let points: Vec<[f32; 2]> = match bounds_corners(world, renderer, entity) {
                Some(corners) => corners.iter().filter_map(|&corner| camera.project(corner)).collect(),
                None => world_matrix(world, entity)
                    .and_then(|matrix| camera.project(matrix.to_scale_rotation_translation().2))
                    .into_iter()
                    .collect(),
            };
            if points.is_empty() {
                return false;
            }
            let start = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            overlaps(points.iter().fold(start, |[x0, y0, x1, y1], &[x, y]| [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]))
        })
        .map(|(entity, ..)| entity)
        .collect()
}
