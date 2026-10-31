//! The Hierarchy panel: every Obj in the open level, as a tree by parent,
//! with an icon for what it is and how many Objs are in it. A filter
//! narrows it to Objs whose names match.
//!
//! Its + button makes new Objs, as Unity's GameObject menu does. A context
//! click on an Obj offers to duplicate or delete it, or make an Obj inside
//! it, and on the space below them, to make one at the top. Dragging an
//! Obj onto another puts it inside, and below the rest takes it out to the
//! top. The editor carries these out (see [`ObjAction`]).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Light, MeshRenderer};
use voxear_scene::{Destroying, Name, Parent};
use voxear_ui::{MenuItem, NodeId, TreeItems, Ui};

use crate::objs::NEW_OBJS;

/// Objs the editor made for itself, such as its Scene camera, which the
/// Hierarchy leaves out and level changes leave alone.
pub use voxear_scene::EditorOnly;

/// What the tree shows, rebuilt from the world when it changes.
#[derive(Default, PartialEq)]
struct Snapshot {
    roots: Vec<u64>,
    children: HashMap<u64, Vec<u64>>,
    /// Each Obj's name and icon.
    rows: HashMap<u64, (String, &'static str)>,
}

struct Items(Rc<RefCell<Snapshot>>);

impl TreeItems for Items {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        let snapshot = self.0.borrow();
        match parent {
            None => snapshot.roots.clone(),
            Some(parent) => snapshot.children.get(&parent).cloned().unwrap_or_default(),
        }
    }

    fn has_children(&self, item: u64) -> bool {
        self.0.borrow().children.get(&item).is_some_and(|c| !c.is_empty())
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        let icon = ui.icon(row, "obj");
        ui.add_class(icon, "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
        let count = ui.add_label(row, "");
        ui.add_class(count, "row-count");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let snapshot = self.0.borrow();
        let Some((name, icon)) = snapshot.rows.get(&item) else { return };
        let parts = ui.children(row);
        ui.set_icon(parts[0], icon);
        ui.set_text(parts[1], name);
        let inside = snapshot.children.get(&item).map_or(0, Vec::len);
        ui.set_text(parts[2], &if inside > 0 { inside.to_string() } else { String::new() });
    }
}

/// What the Hierarchy asks the editor to do to Objs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjAction {
    /// Make one of `NEW_OBJS`, inside an Obj or at the top.
    Create {
        kind: usize,
        parent: Option<Entity>,
    },
    Duplicate(Entity),
    Delete(Entity),
    /// Move an Obj inside another, or out to the top.
    Reparent(Entity, Option<Entity>),
}

pub struct Hierarchy {
    tree: NodeId,
    filter: NodeId,
    add: NodeId,
    shown: Rc<RefCell<Snapshot>>,
    /// What each item of the menu open now does.
    menu: Vec<Option<ObjAction>>,
}

impl Hierarchy {
    pub fn new(ui: &mut Ui, tree: NodeId, filter: NodeId, add: NodeId) -> Self {
        let shown = Rc::new(RefCell::new(Snapshot::default()));
        ui.bind_tree(tree, Items(shown.clone()));
        ui.set_rows_draggable(tree, true);
        Self { tree, filter, add, shown, menu: Vec::new() }
    }

    /// What the + button, a context click or a dropped row asks for.
    pub fn action(&mut self, ui: &mut Ui) -> Option<ObjAction> {
        if let Some((item, onto)) = ui.dropped_item(self.tree) {
            return Some(ObjAction::Reparent(Entity::from_bits(item), onto.map(Entity::from_bits)));
        }
        if let Some(index) = ui.menu_choice(self.add).or_else(|| ui.menu_choice(self.tree)) {
            return self.menu.get(index).copied().flatten();
        }
        if ui.clicked(self.add) {
            let [x, y, _, height] = ui.rect(self.add);
            self.open_menu(ui, self.add, (x, y + height + 4.0), None);
        } else if ui.context_clicked(self.tree) {
            let on = ui.item_at_pointer(self.tree).map(Entity::from_bits);
            if on.is_none() {
                self.clear(ui);
            }
            let at = ui.pointer_position().unwrap_or_default();
            self.open_menu(ui, self.tree, at, on);
        }
        None
    }

    /// The menu of what can be done to `on`, or made at the top.
    fn open_menu(&mut self, ui: &mut Ui, owner: NodeId, at: (f32, f32), on: Option<Entity>) {
        let mut items = Vec::new();
        self.menu.clear();
        if let Some(entity) = on {
            let name =
                self.shown.borrow().rows.get(&entity.to_bits()).map(|(name, _)| name.clone()).unwrap_or_default();
            let entries = [
                (MenuItem::new("Duplicate").shortcut("Ctrl+D"), Some(ObjAction::Duplicate(entity))),
                (MenuItem::new("Delete").shortcut("Delete"), Some(ObjAction::Delete(entity))),
                (MenuItem::separator(), None),
                (MenuItem::new(&format!("New inside {name}")).enabled(false), None),
            ];
            for (item, action) in entries {
                items.push(item);
                self.menu.push(action);
            }
        }
        for (kind, (label, ..)) in NEW_OBJS.iter().enumerate() {
            items.push(MenuItem::new(label));
            self.menu.push(Some(ObjAction::Create { kind, parent: on }));
        }
        ui.open_menu(owner, at, &items);
    }

    /// Shows the world as it is now, if anything changed.
    pub fn update(&mut self, ui: &mut Ui, world: &World) {
        let filter = ui.text(self.filter).to_lowercase();
        let snapshot = snapshot(world, &filter);
        if *self.shown.borrow() != snapshot {
            *self.shown.borrow_mut() = snapshot;
            ui.refresh_list(self.tree);
        }
    }

    /// Selects nothing, as clicking empty space in the Scene view does.
    pub fn clear(&mut self, ui: &mut Ui) {
        ui.set_selection(self.tree, &[]);
    }

    /// The selected Obj, when the selection changed since the last frame.
    pub fn selection_changed(&self, ui: &Ui) -> Option<Option<Entity>> {
        if !ui.selection_changed(self.tree) {
            return None;
        }
        Some(ui.selected_items(self.tree).first().map(|&bits| Entity::from_bits(bits)))
    }

    /// Selects the Obj, opening the Objs it is in.
    pub fn select(&mut self, ui: &mut Ui, world: &World, entity: Entity) {
        let mut parent = world.get::<Parent>(entity).map(|p| p.0);
        while let Some(at) = parent {
            ui.set_expanded(self.tree, at.to_bits(), true);
            parent = world.get::<Parent>(at).map(|p| p.0);
        }
        ui.select_item(self.tree, entity.to_bits());
    }
}

/// The icon for an Obj with a camera, a light or a mesh, in that order.
pub fn icon(camera: bool, light: bool, mesh: bool) -> &'static str {
    match (camera, light, mesh) {
        (true, _, _) => "camera",
        (_, true, _) => "light",
        (_, _, true) => "cube",
        _ => "obj",
    }
}

/// The Objs as a tree, or with a filter, those whose names contain it as a
/// flat list.
fn snapshot(world: &World, filter: &str) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let objs = world
        .query_ref::<(Entity, &Name, Option<&Parent>, Option<&Camera>, Option<&Light>, Option<&MeshRenderer>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    let mut all: Vec<(Entity, Option<Entity>)> = Vec::new();
    for (entity, name, parent, camera, light, mesh) in objs {
        let icon = icon(camera.is_some(), light.is_some(), mesh.is_some());
        if !filter.is_empty() && !name.0.to_lowercase().contains(filter) {
            continue;
        }
        snapshot.rows.insert(entity.to_bits(), (name.0.clone(), icon));
        all.push((entity, parent.map(|p| p.0)));
    }
    all.sort_by_key(|(entity, _)| entity.index());
    for (entity, parent) in all {
        match parent.filter(|p| filter.is_empty() && snapshot.rows.contains_key(&p.to_bits())) {
            Some(parent) => snapshot.children.entry(parent.to_bits()).or_default().push(entity.to_bits()),
            None => snapshot.roots.push(entity.to_bits()),
        }
    }
    snapshot
}
