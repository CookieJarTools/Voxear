//! Placing prefabs and models, as Unity's drag from the Project window does:
//! a prefab as an instance of it, a model as Objs drawing it (see
//! `objs::place_model`); a scene dragged onto either opens alongside the
//! scenes open instead (see `scenes`). One dragged onto the Hierarchy lands
//! where an Obj moved there would: inside the Obj it is let go of on, beside
//! it near its edge, at the top of a scene on its header, and below the
//! rows, at the top of the active scene. Let go of on a Scene view, it
//! lands where the pointer meets what is under it, or the ground, on the
//! snap grid across; failing both, in front of the camera, as a new Obj
//! does. Either is one step to undo. Only while a prefab, model or scene is
//! dragged do the two take drops, so nothing else lights them up. In a
//! prefab open on its own, what would land at the top goes inside its root
//! instead, keeping its place in the world, as a prefab has one root.

use std::collections::HashSet;

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::Parent;
use voxear_ui::DragSource;

use super::{Editor, NEW_OBJ_DISTANCE};
use crate::{
    history::Edit,
    objs::{self, Destination},
    scene_tools,
};

impl Editor {
    /// Places what was let go of on the Hierarchy or the Scene view, and lets
    /// them take drops while a prefab, model or scene is dragged.
    pub(super) fn update_placing(&mut self, renderer: &mut Renderer) {
        if let Some((source, to)) = self.hierarchy.dropped_from_elsewhere(&self.ui)
            && let Some((path, kind)) = self.dragged_placeable(&source)
        {
            match kind {
                AssetKind::Scene => self.open_scene_alongside(&path, renderer),
                _ => {
                    let at = self.new_obj_position();
                    self.place(&path, kind, to, at, renderer);
                }
            }
        }
        let dropped = self.all_scene_views().find_map(|view| self.ui.dropped_onto(view.viewport));
        if let Some(source) = dropped
            && let Some((path, kind)) = self.dragged_placeable(&source)
        {
            match kind {
                AssetKind::Scene => self.open_scene_alongside(&path, renderer),
                _ => {
                    let at = self.scene_landing(renderer);
                    self.place(&path, kind, None, at, renderer);
                }
            }
        }
        let placing = self.ui.dragging().is_some_and(|source| self.dragged_placeable(&source).is_some());
        self.hierarchy.take_drops(&mut self.ui, placing);
        let viewports: Vec<_> = self.all_scene_views().map(|view| view.viewport).collect();
        for viewport in viewports {
            self.ui.accept_drops(viewport, placing);
        }
    }

    /// The path of the prefab, model or scene being dragged, when it is one
    /// from the Project, and which it is.
    fn dragged_placeable(&self, source: &DragSource) -> Option<(String, AssetKind)> {
        let path = self.project.dragged(source)?;
        let db = self.game.content.db();
        let kind = db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind)?;
        matches!(kind, AssetKind::Prefab | AssetKind::Model | AssetKind::Scene).then_some((path, kind))
    }

    /// Places an instance of a prefab, or a model's Objs, where `to` says, or
    /// at the top of the active scene, at `at` unless it goes inside an Obj,
    /// and selects it.
    fn place(&mut self, path: &str, kind: AssetKind, to: Option<Destination>, at: Vec3, renderer: &mut Renderer) {
        if kind == AssetKind::Prefab && self.would_hold_itself(path) {
            return self.notify("A prefab cannot hold an instance of itself");
        }
        let (parent, scene) = match to {
            Some(Destination::Into(parent)) => (Some(parent), None),
            Some(Destination::SceneEnd(scene)) => (None, Some(scene)),
            Some(Destination::Before(beside) | Destination::After(beside)) => (None, self.game.levels.scene_of(beside)),
            None => (None, None),
        };
        let made = match kind {
            AssetKind::Model => objs::place_model(&mut self.game, renderer, path, parent, scene, at),
            _ => objs::place_prefab(&mut self.game, Some(renderer), path, parent, scene, at),
        };
        let made = match made {
            Ok(made) => made,
            Err(error) => return self.notify(&format!("Could not place {path}: {error:#}")),
        };
        if let Some(beside @ (Destination::Before(_) | Destination::After(_))) = to {
            objs::move_obj(&mut self.game, made, beside);
        }
        let on_top = !self.game.engine.world.has::<Parent>(made);
        if let Some(root) = self.prefab_root().filter(|&root| on_top && root != made) {
            objs::move_obj(&mut self.game, made, Destination::Into(root));
        }
        // While playing, only the played world changes, as with any new Obj.
        if !self.is_playing() {
            self.history.record_step(Edit::Exists { root: made, taken: None });
            self.unsaved = true;
        }
        self.select(Some(made));
    }

    /// The root of the prefab open on its own, if one is.
    fn prefab_root(&self) -> Option<Entity> {
        self.prefab.as_ref()?;
        let order = self.game.levels.scene_order();
        let objs = order.first().map(|(_, _, objs)| objs.as_slice()).unwrap_or_default();
        objs.iter().copied().find(|&obj| !self.game.engine.world.has::<Parent>(obj))
    }

    /// Whether `prefab` is the prefab open on its own, or holds an instance
    /// of it at any depth, a variant's base included, so placing it there
    /// would put the prefab inside itself.
    fn would_hold_itself(&mut self, prefab: &str) -> bool {
        let Some(open) = self.open_prefab_path() else { return false };
        let db = self.game.content.db();
        let (Some(open), Some(placed)) = (db.guid_of(open), db.guid_of(prefab)) else { return false };
        let (mut held, mut seen) = (vec![placed], HashSet::new());
        while let Some(guid) = held.pop() {
            if guid == open {
                return true;
            }
            if !seen.insert(guid) {
                continue;
            }
            let Ok(file) = self.game.content.prefab_file(guid) else { continue };
            let content = &self.game.content;
            held.extend(file.objs.iter().filter_map(|obj| content.parse(obj.prefab.as_deref()?).ok()?.guid()));
        }
        false
    }

    /// Where a prefab or model let go of on the Scene view goes: where the pointer
    /// meets what is under it, or the ground, on the snap grid across it;
    /// failing both, a little way along the pointer's ray, on the grid.
    fn scene_landing(&self, renderer: &Renderer) -> Vec3 {
        let snap = self.tools.snap;
        let (Some(camera), Some((x, y))) = (self.scene_camera(), self.ui.pointer_position()) else {
            return self.new_obj_position();
        };
        match scene_tools::surface_at(&self.game.engine.world, renderer, &camera, [x, y]) {
            Some(hit) => Vec3::new((hit.x / snap).round() * snap, hit.y, (hit.z / snap).round() * snap),
            None => {
                let (origin, direction) = camera.ray([x, y]);
                ((origin + direction * NEW_OBJ_DISTANCE) / snap).round() * snap
            }
        }
    }
}
