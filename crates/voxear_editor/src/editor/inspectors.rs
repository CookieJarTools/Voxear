//! Several Inspectors, as Unity has: the first, docked from the start, and
//! any more opened from a window's menu (Add Tab › Inspector), each a panel
//! of its own beside the one it was opened from. Each shows the selection,
//! unless it is locked with its padlock, when it keeps showing what it
//! showed, as to compare two Objs. Each runs the same Inspector's frame in
//! turn: while one does, it is the editor's `inspector`, so editing,
//! menus, pickers, buttons, callbacks and drawers act on the Obj it shows.
//! One opened from the menu goes for good when it closes, and opens again,
//! following the selection, with a layout that names it (see `windows`).

use voxear_ecs::Entity;
use voxear_render::{Camera, LodCamera, Renderer, ViewOptions};
use voxear_scene::Transform;
use voxear_ui::{NodeId, Style};

use super::Editor;
use crate::{
    inspector::{Field, Inspector},
    lod_inspector::{self, LodEdge, LodEye},
};

/// An Inspector opened from a window's menu: its panel's id, and itself.
pub(super) struct ExtraInspector {
    panel: String,
    inspector: Inspector,
}

impl Editor {
    /// Opens another Inspector beside `beside`, showing the selection.
    pub fn add_inspector(&mut self, beside: &str) -> String {
        let number = (2..).find(|n| !self.extra_inspectors.iter().any(|e| e.panel == format!("inspector-{n}")));
        let number = number.expect("there is always a free number");
        self.open_inspector(number, beside)
    }

    /// Opens the Inspector a layout names as `panel` (`inspector-2`), unless
    /// it is open.
    pub(super) fn reopen_inspector(&mut self, panel: &str) {
        let number = panel.strip_prefix("inspector-").and_then(|n| n.parse().ok());
        if let Some(number) = number.filter(|_| !self.extra_inspectors.iter().any(|e| e.panel == panel)) {
            self.open_inspector(number, "inspector");
        }
    }

    /// Opens an Inspector numbered `number` beside `beside`, showing the
    /// selection. Returns its panel.
    fn open_inspector(&mut self, number: u32, beside: &str) -> String {
        let panel = format!("inspector-{number}");
        let content = self.ui.add_dock_panel_beside(self.nodes.dock, &panel, "Inspector", beside);
        self.ui.add_class(content, "dock-content");
        let actions = self.ui.panel(content, Style::row());
        self.ui.add_class(actions, "panel-actions");
        let lock = self.ui.icon_button(actions, "lock");
        self.ui.add_class(lock, "inspector-lock");
        let root = self.ui.panel(content, Style::column().scroll());
        self.ui.add_class(root, "fill");
        self.ui.add_class(root, "inspector");
        let mut inspector = Inspector::with_owner(root, number);
        inspector.set_lock_button(&mut self.ui, lock);
        inspector.set_types(self.inspector.types().clone());
        inspector.set_prefab_mode(self.prefab.is_some());
        inspector.set_dark(self.theme.is_dark());
        let (selected, playing) = (self.selected(), self.is_playing());
        inspector.show(&mut self.ui, &self.game, selected, playing);
        self.extra_inspectors.push(ExtraInspector { panel: panel.clone(), inspector });
        panel
    }

    /// Takes an Inspector opened from the menu away for good, as closing it
    /// does. False for a panel that is not one.
    pub(super) fn remove_inspector(&mut self, panel: &str) -> bool {
        let Some(index) = self.extra_inspectors.iter().position(|e| e.panel == panel) else { return false };
        self.extra_inspectors.remove(index);
        self.ui.remove_dock_panel(self.nodes.dock, panel);
        true
    }

    /// How many Inspectors there are, the first included.
    pub fn inspector_count(&self) -> usize {
        1 + self.extra_inspectors.len()
    }

    /// Runs every Inspector's frame: the first's, then each other's, swapped
    /// in as the editor's `inspector` while it runs.
    pub(super) fn update_inspectors(&mut self, renderer: &mut Renderer, playing: bool) {
        self.inspector_frame(renderer, playing);
        for index in 0..self.extra_inspectors.len() {
            std::mem::swap(&mut self.inspector, &mut self.extra_inspectors[index].inspector);
            self.inspector_frame(renderer, playing);
            std::mem::swap(&mut self.inspector, &mut self.extra_inspectors[index].inspector);
        }
    }

    /// One Inspector's frame: it follows the selection unless locked, takes
    /// what was typed or picked, runs the behaviour's code it asks for, and
    /// answers its menus.
    fn inspector_frame(&mut self, renderer: &mut Renderer, playing: bool) {
        let unlocked = self.inspector.update_lock(&mut self.ui) && !self.inspector.is_locked();
        let following = unlocked || !self.inspector.is_locked();
        // A model's part picked in the Hierarchy shows in place of its Obj.
        let part = self.selected_part.filter(|(owner, _)| Some(*owner) == self.selected());
        if following && (self.inspector.shown() != self.selected() || self.inspector.shown_part() != part) {
            let selected = self.selected();
            match part {
                Some((owner, node)) => self.inspector.show_part(&mut self.ui, &self.game, owner, node, playing),
                None => self.inspector.show(&mut self.ui, &self.game, selected, playing),
            }
        }
        if let Some(action) = self.inspector.update_part(&mut self.ui) {
            self.part_action(action, renderer);
        }
        // An Inspector not locked edits every Obj selected, shown on the active one.
        let others = if self.inspector.is_locked() { Vec::new() } else { self.others.clone() };
        self.inspector.set_also(&mut self.ui, &self.game, others, playing);
        let live = self.pull_live_fields();
        self.inspector.set_lod_eye(self.lod_eye());
        let edit = self.inspector.update(&mut self.ui, &mut self.game, Some(renderer), playing);
        if let Some(edge) = self.inspector.take_lod_edge() {
            self.follow_lod_edge(edge, renderer);
        }
        let edit = self.run_inspector_drawers(edit);
        // Before the callbacks, so a running behaviour's has the value it was called for.
        self.push_live_fields(live);
        let edit = self.run_inspector_callbacks(edit);
        // Edits while playing go with the played world, so there is nothing to undo or save.
        if let Some(edit) = edit.filter(|_| !self.is_playing()) {
            self.history.record(edit);
            self.unsaved = true;
        }
        self.run_inspector_button();
        self.update_project_layers();
        self.ask_inspector_answers();
        self.update_docs_links();
        let shown = self.inspector.shown();
        let can_keep = shown.is_some_and(|entity| self.edited_counterpart(entity).is_some());
        let kept: Vec<Field> = self.kept.iter().filter(|(e, _)| Some(*e) == shown).map(|(_, f)| f.clone()).collect();
        if let Some(action) = self.inspector.menus(&mut self.ui, &self.game, playing, can_keep, &kept) {
            self.row_action(action, renderer);
        }
    }

    /// Moves the Scene view in use to where the Obj the Inspector shows is
    /// at the LOD bar's edge being dragged, as Unity's LOD Group does, so
    /// the switch shows as the edge moves.
    fn follow_lod_edge(&mut self, edge: LodEdge, renderer: &Renderer) {
        let Some(entity) = self.inspector.shown() else { return };
        let eye = self.shown(self.scene.viewport).then(|| self.scene_eye()).flatten();
        let world = &self.game.engine.world;
        let Some((position, rotation)) = eye.and_then(|eye| lod_inspector::view_edge(world, entity, renderer, edge, eye))
        else {
            return;
        };
        self.glide = None;
        if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.scene.camera) {
            (transform.position, transform.rotation) = (position, rotation);
        }
    }

    /// Moves the Scene view in use to where it draws LOD `level` of an Obj's
    /// mesh, looking at it.
    pub(super) fn view_lod(&mut self, entity: Entity, level: usize, renderer: &Renderer) {
        let eye = self.shown(self.scene.viewport).then(|| self.scene_eye()).flatten();
        let world = &self.game.engine.world;
        let Some((position, rotation)) = eye.and_then(|eye| lod_inspector::view_lod(world, entity, renderer, level, eye))
        else {
            return;
        };
        if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.scene.camera) {
            (transform.position, transform.rotation) = (position, rotation);
        }
    }

    /// The camera the Inspector says which LOD is drawn through: the game's
    /// while playing with the Game view showing, and otherwise the Scene
    /// view's, when it shows.
    fn lod_eye(&self) -> Option<LodEye> {
        let game = self.game_camera().filter(|_| self.is_playing() && self.shown(self.nodes.game_viewport));
        let game = game.map(|camera| (camera, self.nodes.game_viewport));
        let scene = self.shown(self.scene.viewport).then_some((self.scene.camera, self.scene.viewport));
        let (camera, viewport) = game.or(scene)?;
        self.eye_of(camera, viewport)
    }

    /// The Scene view in use's camera, as LODs are picked for it.
    fn scene_eye(&self) -> Option<LodEye> {
        self.eye_of(self.scene.camera, self.scene.viewport)
    }

    fn eye_of(&self, camera: Entity, viewport: NodeId) -> Option<LodEye> {
        let world = &self.game.engine.world;
        let lens = world.get::<Camera>(camera)?;
        let position = voxear_scene::world_matrix(world, camera).w_axis.truncate();
        let eye = LodCamera::new(position, lens.fov_y_degrees);
        let height = self.ui.rect(viewport)[3] * self.scale;
        let threshold = ViewOptions::default().lod_threshold;
        Some(LodEye { camera: eye, pixels_per_metre: height / (2.0 * eye.tan_half_fov), threshold })
    }

    /// Shows the selection in every Inspector not locked.
    pub(super) fn show_selection(&mut self, entity: Option<Entity>) {
        self.selection = entity;
        // A part stays picked while its Obj does, as through an undo.
        if self.selected_part.is_some_and(|(owner, _)| Some(owner) != entity) {
            self.selected_part = None;
        }
        // The active Obj is not among the others, and with nothing active nothing else is selected.
        self.others.retain(|&other| entity.is_some() && Some(other) != entity);
        let (playing, part) = (self.is_playing(), self.selected_part);
        let (ui, game) = (&mut self.ui, &self.game);
        let extras = self.extra_inspectors.iter_mut().map(|extra| &mut extra.inspector);
        for inspector in std::iter::once(&mut self.inspector).chain(extras).filter(|inspector| !inspector.is_locked()) {
            match part {
                Some((owner, node)) => inspector.show_part(ui, game, owner, node, playing),
                None => inspector.show(ui, game, entity, playing),
            }
        }
    }

    /// Every Inspector, the first one first.
    pub(super) fn inspectors_mut(&mut self) -> impl Iterator<Item = &mut Inspector> {
        std::iter::once(&mut self.inspector).chain(self.extra_inspectors.iter_mut().map(|e| &mut e.inspector))
    }

    /// What each locked Inspector shows, by its place, as `counterpart`
    /// finds it in the world about to be swapped in.
    pub(super) fn locked_shown(&self, counterpart: impl Fn(Entity) -> Option<Entity>) -> Vec<(usize, Option<Entity>)> {
        let inspectors = std::iter::once(&self.inspector).chain(self.extra_inspectors.iter().map(|e| &e.inspector));
        inspectors
            .enumerate()
            .filter(|(_, inspector)| inspector.is_locked())
            .map(|(index, inspector)| (index, inspector.shown().and_then(&counterpart)))
            .collect()
    }

    /// Puts locked Inspectors back on what they showed, after a world swap.
    pub(super) fn show_locked(&mut self, locked: Vec<(usize, Option<Entity>)>) {
        let playing = self.is_playing();
        for (index, entity) in locked {
            let inspector = match index {
                0 => &mut self.inspector,
                _ => match self.extra_inspectors.get_mut(index - 1) {
                    Some(extra) => &mut extra.inspector,
                    None => continue,
                },
            };
            inspector.show(&mut self.ui, &self.game, entity, playing);
        }
    }

    /// The lock button of the first Inspector.
    pub fn inspector_lock(&self) -> NodeId {
        self.nodes.inspector_lock
    }

    /// What each Inspector shows, the first one first.
    pub fn inspectors_shown(&self) -> Vec<Option<Entity>> {
        std::iter::once(&self.inspector)
            .chain(self.extra_inspectors.iter().map(|e| &e.inspector))
            .map(Inspector::shown)
            .collect()
    }
}
