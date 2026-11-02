//! Placing prefabs, models and sounds, as Unity's drag from the Project
//! window does: a prefab as an instance of it, a model as Objs drawing it
//! (see `objs::place_model`), a sound as an Obj with an Audio Source
//! playing it; a scene dragged onto either opens alongside the scenes open
//! instead (see `scenes`). One dragged onto the Hierarchy lands where an
//! Obj moved there would: inside the Obj it is let go of on, beside it near
//! its edge, at the top of a scene on its header, and below the rows, at
//! the top of the active scene. Let go of on a Scene view, it lands where
//! the pointer meets what is under it, or the ground, on the snap grid
//! across; failing both, in front of the camera, as a new Obj does. Either
//! is one step to undo. As in Unity, what is dragged over a Scene view is
//! there already, following the pointer and settling on what is under it,
//! so it can be seen where it will go; let go there, it stays, and taken
//! off the view or let go elsewhere, it goes. Only while a prefab, model,
//! sound or scene is dragged do the two take drops, so nothing else lights
//! them up. In a prefab open on its own, what would land at the top goes
//! inside its root instead, keeping its place in the world, as a prefab has
//! one root.

use std::collections::HashSet;

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::Parent;
use voxear_ui::DragSource;

use super::{Editor, NEW_OBJ_DISTANCE};

/// What is being placed live, under the pointer over a Scene view: the
/// asset's path, the Obj made of it, every Obj in it, and how far its root
/// stands from where it lands, as a prefab places its root.
pub(super) struct LivePlace {
    path: String,
    root: Entity,
    family: Vec<Entity>,
    offset: Vec3,
}
use crate::{
    history::Edit,
    objs::{self, Destination},
    scene_tools,
};

impl Editor {
    /// Places what was let go of on the Hierarchy or the Scene view, and lets
    /// them take drops while a prefab, model, sound or scene is dragged.
    pub(super) fn update_placing(&mut self, renderer: &mut Renderer) {
        if let Some((source, to)) = self.dropped_on_hierarchies()
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
            match (kind, self.live_place.take()) {
                (AssetKind::Scene, _) => self.open_scene_alongside(&path, renderer),
                (_, Some(live)) if live.path == path => self.keep_live(live, renderer),
                (_, live) => {
                    if let Some(live) = live {
                        self.drop_live(live);
                    }
                    let at = self.scene_landing(renderer, &[]);
                    self.place(&path, kind, None, at, renderer);
                }
            }
        }
        self.update_live_place(renderer);
        let placing = self.ui.dragging().is_some_and(|source| self.dragged_placeable(&source).is_some());
        // A material dropped on an Obj there goes to it (see `material_drops`).
        let material = self.ui.dragging().is_some_and(|source| self.dragged_material(&source).is_some());
        self.hierarchies_take_drops(placing || material);
        let viewports: Vec<_> = self.all_scene_views().map(|view| view.viewport).collect();
        for viewport in viewports {
            self.ui.accept_drops(viewport, placing || material);
        }
    }

    /// The path of the prefab, model, sound or scene being dragged, when it
    /// is one from the Project, and which it is.
    fn dragged_placeable(&self, source: &DragSource) -> Option<(String, AssetKind)> {
        let path = self.dragged_asset(source)?;
        let db = self.game.content.db();
        let kind = db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind)?;
        matches!(kind, AssetKind::Prefab | AssetKind::Model | AssetKind::Sound | AssetKind::Scene)
            .then_some((path, kind))
    }

    /// While a prefab, model or sound is dragged over the Scene view in use,
    /// it is there, settled on what is under the pointer; off the view, or
    /// once the drag is over without landing there, it goes.
    fn update_live_place(&mut self, renderer: &mut Renderer) {
        let dragged = self.ui.dragging().and_then(|source| self.dragged_placeable(&source));
        let over = self.shown(self.scene.viewport)
            && self.ui.pointer_position().is_some_and(|(x, y)| {
                let [left, top, width, height] = self.ui.rect(self.scene.viewport);
                x >= left && x < left + width && y >= top && y < top + height
            });
        let wanted = dragged.filter(|(_, kind)| *kind != AssetKind::Scene && over);
        let Some((path, kind)) = wanted else {
            if let Some(live) = self.live_place.take() {
                self.drop_live(live);
            }
            return;
        };
        if self.live_place.as_ref().is_some_and(|live| live.path != path)
            && let Some(live) = self.live_place.take()
        {
            self.drop_live(live);
        }
        if self.live_place.is_none() {
            if kind == AssetKind::Prefab && self.would_hold_itself(&path) {
                return;
            }
            let at = self.scene_landing(renderer, &[]);
            let Ok(root) = self.make(&path, kind, (None, None), at, renderer) else { return };
            let world = &self.game.engine.world;
            let offset = world.get::<voxear_scene::Transform>(root).map_or(Vec3::ZERO, |t| t.position - at);
            let family = objs::family(world, root);
            self.live_place = Some(LivePlace { path, root, family, offset });
        }
        let Some(live) = &self.live_place else { return };
        let at = self.scene_landing(renderer, &live.family) + live.offset;
        if let Some(transform) = self.game.engine.world.get_mut::<voxear_scene::Transform>(live.root) {
            transform.position = at;
        }
    }

    /// Keeps what was placed live where it was let go of, as one step to
    /// undo, and selects it.
    fn keep_live(&mut self, live: LivePlace, renderer: &Renderer) {
        let at = self.scene_landing(renderer, &live.family) + live.offset;
        if let Some(transform) = self.game.engine.world.get_mut::<voxear_scene::Transform>(live.root) {
            transform.position = at;
        }
        self.settle(live.root, None);
    }

    /// Takes away what was placed live, leaving nothing to undo.
    fn drop_live(&mut self, live: LivePlace) {
        if self.game.engine.world.is_alive(live.root) {
            drop(objs::take(&mut self.game, live.root));
        }
    }

    /// Makes an instance of a prefab, a model's Objs, or an Obj playing a
    /// sound, inside `parent` or at the top of `scene` (or the active one),
    /// at `at` unless it goes inside an Obj.
    fn make(
        &mut self,
        path: &str,
        kind: AssetKind,
        (parent, scene): (Option<Entity>, Option<voxear_assets::AssetGuid>),
        at: Vec3,
        renderer: &mut Renderer,
    ) -> anyhow::Result<Entity> {
        match kind {
            AssetKind::Model => objs::place_model(&mut self.game, renderer, path, parent, scene, at),
            AssetKind::Sound => objs::place_sound(&mut self.game, path, parent, scene, at),
            _ => objs::place_prefab(&mut self.game, Some(renderer), path, parent, scene, at),
        }
    }

    /// Places an instance of a prefab, a model's Objs, or an Obj playing a
    /// sound, where `to` says, or at the top of the active scene, at `at`
    /// unless it goes inside an Obj, and selects it.
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
        let made = match self.make(path, kind, (parent, scene), at, renderer) {
            Ok(made) => made,
            Err(error) => return self.notify(&format!("Could not place {path}: {error:#}")),
        };
        self.settle(made, to);
    }

    /// Finishes placing what was made: beside an Obj if `to` says, inside
    /// the root of a prefab open on its own if at the top, as one step to
    /// undo, and selected.
    fn settle(&mut self, made: Entity, to: Option<Destination>) {
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

    /// Where what is let go of on the Scene view goes: where the pointer
    /// meets what is under it, `skip` aside, or the ground, on the snap grid
    /// across it; failing both, a little way along the pointer's ray, on the
    /// grid.
    fn scene_landing(&self, renderer: &Renderer, skip: &[Entity]) -> Vec3 {
        let snap = self.tools.snap;
        let (Some(camera), Some((x, y))) = (self.scene_camera(), self.ui.pointer_position()) else {
            return self.new_obj_position();
        };
        match scene_tools::surface_past(&self.game.engine.world, renderer, &camera, [x, y], skip) {
            Some(hit) => Vec3::new((hit.x / snap).round() * snap, hit.y, (hit.z / snap).round() * snap),
            None => {
                let (origin, direction) = camera.ray([x, y]);
                ((origin + direction * NEW_OBJ_DISTANCE) / snap).round() * snap
            }
        }
    }
}
