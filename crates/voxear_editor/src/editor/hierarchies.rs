//! Several Hierarchies, as Unity has: the first, docked from the start, and
//! any more opened from a window's menu (Add Hierarchy), each a panel of its
//! own beside the one it was opened from, with its own filter and its own
//! folds. They list the same Objs and share the selection, so a click in one
//! selects in all, and a ping lights the Obj in each. Each runs the same
//! frame in turn: while one does, it is the editor's `hierarchy`, so its
//! menus, and what is dragged from it or dropped on it, act as the first's
//! do. One opened from the menu goes for good when it closes. Each is built
//! from Hierarchy.vui.

use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_ui::{DragSource, NodeId, Ui};

use super::Editor;
use crate::{hierarchy::Hierarchy, objs::Destination};

/// What every Hierarchy is built from.
const MARKUP: &str = include_str!("../../ui/Hierarchy.vui");

/// A Hierarchy opened from a window's menu: its panel's id, and itself.
pub(super) struct ExtraHierarchy {
    panel: String,
    hierarchy: Hierarchy,
}

/// Builds a Hierarchy from its markup in a dock panel's `content`.
pub(super) fn build(ui: &mut Ui, content: NodeId) -> Hierarchy {
    let mut load = |path: &str| Err(format!("{path} is not built in"));
    let root = ui.load_markup(content, "Hierarchy.vui", MARKUP, &mut load).expect("the Hierarchy's markup is built in");
    let find = |ui: &Ui, name: &str| ui.find(root, name).unwrap_or_else(|| panic!("Hierarchy.vui has no {name}"));
    let (tree, filter, add) = (find(ui, "hierarchy-tree"), find(ui, "hierarchy-filter"), find(ui, "hierarchy-add"));
    Hierarchy::new(ui, tree, filter, add)
}

impl Editor {
    /// Opens another Hierarchy beside `beside`. Returns its panel.
    pub fn add_hierarchy(&mut self, beside: &str) -> String {
        let taken = |n: u32| self.extra_hierarchies.iter().any(|extra| extra.panel == format!("hierarchy-{n}"));
        let panel = format!("hierarchy-{}", (2..).find(|&n| !taken(n)).expect("there is always a free number"));
        self.open_hierarchy(&panel, beside);
        panel
    }

    /// Opens the Hierarchy a layout names as `panel` (`hierarchy-2`),
    /// unless it is open.
    pub(super) fn reopen_hierarchy(&mut self, panel: &str) {
        if !self.extra_hierarchies.iter().any(|extra| extra.panel == panel) {
            self.open_hierarchy(panel, "hierarchy");
        }
    }

    fn open_hierarchy(&mut self, panel: &str, beside: &str) {
        let content = self.ui.add_dock_panel_beside(self.nodes.dock, panel, "Hierarchy", beside);
        self.ui.add_class(content, "dock-content");
        let mut hierarchy = build(&mut self.ui, content);
        let game = (&self.game.levels, &self.game.content);
        hierarchy.update(&mut self.ui, &self.game.engine.world, game, self.inspector.types());
        if let Some(selected) = self.selected() {
            hierarchy.select(&mut self.ui, &self.game.engine.world, selected);
        }
        self.extra_hierarchies.push(ExtraHierarchy { panel: panel.to_string(), hierarchy });
    }

    /// Takes a Hierarchy opened from the menu away for good, as closing it
    /// does. False for a panel that is not one.
    pub(super) fn remove_hierarchy(&mut self, panel: &str) -> bool {
        let Some(index) = self.extra_hierarchies.iter().position(|extra| extra.panel == panel) else { return false };
        self.extra_hierarchies.remove(index);
        self.ui.remove_dock_panel(self.nodes.dock, panel);
        true
    }

    /// How many Hierarchies there are, the first included.
    pub fn hierarchy_count(&self) -> usize {
        1 + self.extra_hierarchies.len()
    }

    /// Runs every Hierarchy's frame: the first's, then each other's,
    /// swapped in as the editor's `hierarchy` while it runs.
    pub(super) fn update_hierarchies(&mut self, renderer: &mut Renderer, delta: f32) {
        self.hierarchy_frame(renderer, delta);
        let mut index = 0;
        while index < self.extra_hierarchies.len() {
            std::mem::swap(&mut self.hierarchy, &mut self.extra_hierarchies[index].hierarchy);
            self.hierarchy_frame(renderer, delta);
            std::mem::swap(&mut self.hierarchy, &mut self.extra_hierarchies[index].hierarchy);
            index += 1;
        }
    }

    /// One Hierarchy's frame: it lists the Objs as they are, shares a
    /// selection made in it with the others and the Inspectors, and does
    /// what its menus or a row dropped on it ask.
    fn hierarchy_frame(&mut self, renderer: &mut Renderer, delta: f32) {
        self.hierarchy.fade_ping(&mut self.ui, delta);
        let game = (&self.game.levels, &self.game.content);
        self.hierarchy.update(&mut self.ui, &self.game.engine.world, game, self.inspector.types());
        self.hear_hierarchy_selection();
        if let Some(action) = self.hierarchy.action(&mut self.ui) {
            self.obj_action(action, renderer);
        }
        // Again, as a click on nothing clears the selection in `action`, and a
        // change not heard this frame is not heard at all.
        self.hear_hierarchy_selection();
    }

    /// A selection made in the editor's `hierarchy`, shared with the others
    /// and the Inspectors, when it is not the one there already.
    fn hear_hierarchy_selection(&mut self) {
        let Some(selected) = self.hierarchy.selection_changed(&self.ui) else { return };
        let had_others = !self.others.is_empty();
        if self.hear_hierarchy_many() {
            return;
        }
        if selected != self.selected() || had_others {
            self.select_in_other_hierarchies(selected);
            self.show_selection(selected);
        }
    }

    /// Shows several Objs selected in every Hierarchy.
    pub(super) fn select_many_in_hierarchies(&mut self, entities: &[Entity]) {
        self.hierarchy.select_many(&mut self.ui, &self.game.engine.world, entities);
        self.select_many_in_other_hierarchies(entities);
    }

    /// Shows several Objs selected in every Hierarchy but the editor's `hierarchy`.
    pub(super) fn select_many_in_other_hierarchies(&mut self, entities: &[Entity]) {
        let (ui, world) = (&mut self.ui, &self.game.engine.world);
        for extra in &mut self.extra_hierarchies {
            extra.hierarchy.select_many(ui, world, entities);
        }
    }

    /// Shows the selection in every Hierarchy.
    pub(super) fn select_in_hierarchies(&mut self, selected: Option<Entity>) {
        match selected {
            Some(entity) => self.hierarchy.select(&mut self.ui, &self.game.engine.world, entity),
            None => self.hierarchy.clear(&mut self.ui),
        }
        self.select_in_other_hierarchies(selected);
    }

    /// Shows the selection in every Hierarchy but the editor's `hierarchy`.
    pub(super) fn select_in_other_hierarchies(&mut self, selected: Option<Entity>) {
        let (ui, world) = (&mut self.ui, &self.game.engine.world);
        for extra in &mut self.extra_hierarchies {
            match selected {
                Some(entity) => extra.hierarchy.select(ui, world, entity),
                None => extra.hierarchy.clear(ui),
            }
        }
    }

    /// Lists the Objs as they are now in every Hierarchy.
    pub(super) fn refresh_hierarchies(&mut self) {
        let (ui, world, types) = (&mut self.ui, &self.game.engine.world, self.inspector.types());
        let game = (&self.game.levels, &self.game.content);
        let extras = self.extra_hierarchies.iter_mut().map(|extra| &mut extra.hierarchy);
        for hierarchy in std::iter::once(&mut self.hierarchy).chain(extras) {
            hierarchy.update(ui, world, game, types);
        }
    }

    /// Lights an Obj up in every Hierarchy, as Unity's ping does.
    pub(super) fn ping_obj(&mut self, entity: Entity) {
        let (ui, world) = (&mut self.ui, &self.game.engine.world);
        let extras = self.extra_hierarchies.iter_mut().map(|extra| &mut extra.hierarchy);
        for hierarchy in std::iter::once(&mut self.hierarchy).chain(extras) {
            hierarchy.ping(ui, world, entity);
        }
    }

    /// The Obj being dragged from any Hierarchy, when one is.
    pub(super) fn dragged_obj(&self, source: &DragSource) -> Option<Entity> {
        let extras = self.extra_hierarchies.iter().map(|extra| &extra.hierarchy);
        std::iter::once(&self.hierarchy).chain(extras).find_map(|hierarchy| hierarchy.dragged(source))
    }

    /// Something dragged from elsewhere and let go of on any Hierarchy's
    /// rows since the last frame, and where it lands there.
    pub(super) fn dropped_on_hierarchies(&self) -> Option<(DragSource, Option<Destination>)> {
        let extras = self.extra_hierarchies.iter().map(|extra| &extra.hierarchy);
        std::iter::once(&self.hierarchy).chain(extras).find_map(|hierarchy| hierarchy.dropped_from_elsewhere(&self.ui))
    }

    /// Lets every Hierarchy's rows take drops from elsewhere, or not.
    pub(super) fn hierarchies_take_drops(&mut self, take: bool) {
        let extras = self.extra_hierarchies.iter().map(|extra| &extra.hierarchy);
        for hierarchy in std::iter::once(&self.hierarchy).chain(extras) {
            hierarchy.take_drops(&mut self.ui, take);
        }
    }
}
