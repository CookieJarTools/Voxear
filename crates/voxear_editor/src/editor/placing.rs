//! Placing prefabs, as Unity's drag from the Project window does. A prefab
//! dragged onto the Hierarchy lands where an Obj moved there would: inside
//! the Obj it is let go of on, beside it near its edge, at the top of a scene
//! on its header, and below the rows, at the top of the active scene. Let go
//! of on the Scene view, it lands where the pointer meets what is under it,
//! or the ground, on the snap grid across; failing both, in front of the
//! camera, as a new Obj does. Either is one step to undo. Only while a prefab
//! is dragged do the two take drops, so nothing else dragged lights them up.
//! In a prefab open on its own, what would land at the top goes inside its
//! root instead, keeping its place in the world, as a prefab has one root.

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
    /// them take drops while a prefab is dragged.
    pub(super) fn update_placing(&mut self, renderer: &mut Renderer) {
        if let Some((source, to)) = self.hierarchy.dropped_from_elsewhere(&self.ui)
            && let Some(prefab) = self.dragged_prefab(&source)
        {
            let at = self.new_obj_position();
            self.place_prefab(&prefab, to, at, renderer);
        }
        if let Some(source) = self.ui.dropped_onto(self.nodes.viewport)
            && let Some(prefab) = self.dragged_prefab(&source)
        {
            let at = self.scene_landing(renderer);
            self.place_prefab(&prefab, None, at, renderer);
        }
        let placing = self.ui.dragging().is_some_and(|source| self.dragged_prefab(&source).is_some());
        self.hierarchy.take_drops(&mut self.ui, placing);
        self.ui.accept_drops(self.nodes.viewport, placing);
    }

    /// The path of the prefab being dragged, when it is one from the Project.
    fn dragged_prefab(&self, source: &DragSource) -> Option<String> {
        let path = self.project.dragged(source)?;
        let db = self.game.content.db();
        let kind = db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind);
        (kind == Some(AssetKind::Prefab)).then_some(path)
    }

    /// Places an instance of `prefab` where `to` says, or at the top of the
    /// active scene, at `at` unless it goes inside an Obj, and selects it.
    fn place_prefab(&mut self, prefab: &str, to: Option<Destination>, at: Vec3, renderer: &mut Renderer) {
        if self.would_hold_itself(prefab) {
            return self.notify("A prefab cannot hold an instance of itself");
        }
        let (parent, scene) = match to {
            Some(Destination::Into(parent)) => (Some(parent), None),
            Some(Destination::SceneEnd(scene)) => (None, Some(scene)),
            Some(Destination::Before(beside) | Destination::After(beside)) => (None, self.game.levels.scene_of(beside)),
            None => (None, None),
        };
        let made = match objs::place_prefab(&mut self.game, Some(renderer), prefab, parent, scene, at) {
            Ok(made) => made,
            Err(error) => return self.notify(&format!("Could not place {prefab}: {error:#}")),
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

    /// Where a prefab let go of on the Scene view goes: where the pointer
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
