//! The Hierarchy panel: each open scene under a header of its own, as in
//! Unity, with its Objs in the scene's order, as a tree by parent. A scene's
//! order is the order its file keeps its Objs, so it is saved as it is
//! shown. Each row has an icon for what the Obj is and how many Objs are in
//! it; an Obj with a behaviour that is an `IInstance` has a badge, and the
//! names of prefab instances are tinted. A filter narrows it to Objs whose
//! names match, as a flat list.
//!
//! Its + button makes new Objs, as Unity's GameObject menu does. A context
//! click on an Obj offers to duplicate or delete it, or make an Obj inside
//! it; on a scene's header or the space below, to make one at the top.
//! Dragging an Obj onto the middle of another puts it inside; near another's
//! top or bottom edge, beside it, before or after; onto a scene's header, at
//! the top of that scene; and below the rest, at the top of its own scene.
//! The editor carries these out (see [`ObjAction`]).
//!
//! While the editor lets it ([`Hierarchy::take_drops`]), as it does while a
//! prefab or model is dragged from the Project, the rows take drops from
//! elsewhere too, landing in the same places, except that below the rest is
//! the top of the active scene ([`Hierarchy::dropped_from_elsewhere`]).

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours, Levels,
    prefab::{FromPrefab, PrefabInstance},
    script_types::ScriptTypes,
};
use voxear_render::{Camera, Light, MeshRenderer};
use voxear_scene::{Destroying, InstanceMarker, Name, Parent};
use voxear_ui::{DragSource, DropPlace, MenuItem, NodeId, TreeItems, Ui};

use crate::objs::{Destination, NEW_OBJS};

/// Objs the editor made for itself, such as its Scene camera, which the
/// Hierarchy leaves out and level changes leave alone.
pub use voxear_scene::EditorOnly;

/// The interface whose behaviours an Obj's badge marks.
const INSTANCE: &str = "Voxear.IInstance";

/// A scene's header is an item of its own: its place among the open scenes,
/// counted from 1. An Obj's item is its entity, whose bits are never this
/// small, as a live entity's generation is never 0.
fn is_header(item: u64) -> bool {
    item < 1 << 32
}

/// What a row shows.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    name: String,
    icon: &'static str,
    header: bool,
    /// "Instance" for an Obj with an `IInstance` behaviour.
    badge: &'static str,
    /// A prefab instance's root, or an Obj inside one.
    prefab: Option<bool>,
}

/// What the tree shows, rebuilt from the world when it changes.
#[derive(Default, PartialEq)]
struct Snapshot {
    roots: Vec<u64>,
    children: HashMap<u64, Vec<u64>>,
    rows: HashMap<u64, Row>,
    /// Each header's scene.
    scenes: HashMap<u64, AssetGuid>,
}

impl Snapshot {
    /// The header an Obj shows under, through its parents.
    fn header_of(&self, item: u64) -> Option<u64> {
        let mut at = item;
        for _ in 0..1024 {
            let (parent, _) = self.children.iter().find(|(_, kids)| kids.contains(&at))?;
            if is_header(*parent) {
                return Some(*parent);
            }
            at = *parent;
        }
        None
    }
}

/// How long a pinged row stays lit.
const PING: Duration = Duration::from_millis(1500);

/// What the tree shows, and the row lit by a ping, if any.
struct Items(Rc<RefCell<Snapshot>>, Rc<Cell<Option<u64>>>);

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
        let badge = ui.add_label(row, "");
        ui.add_class(badge, "row-badge");
        let count = ui.add_label(row, "");
        ui.add_class(count, "row-count");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let snapshot = self.0.borrow();
        let Some(shown) = snapshot.rows.get(&item) else { return };
        let parts = ui.children(row);
        ui.set_class(row, "scene-header", shown.header);
        ui.set_icon(parts[0], shown.icon);
        ui.set_text(parts[1], &shown.name);
        ui.set_class(parts[1], "prefab-root", shown.prefab == Some(true));
        ui.set_class(parts[1], "prefab-member", shown.prefab == Some(false));
        ui.set_text(parts[2], shown.badge);
        ui.set_visible(parts[2], !shown.badge.is_empty());
        let inside = snapshot.children.get(&item).map_or(0, Vec::len);
        let count = if inside > 0 && !shown.header { inside.to_string() } else { String::new() };
        ui.set_text(parts[3], &count);
        ui.set_class(row, "pinged", self.1.get() == Some(item));
    }
}

/// What the Hierarchy asks the editor to do to Objs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjAction {
    /// Make one of `NEW_OBJS`, inside an Obj, or at the top of a scene (the
    /// parent's, or the given one, or the active one).
    Create {
        kind: usize,
        parent: Option<Entity>,
        scene: Option<AssetGuid>,
    },
    Duplicate(Entity),
    Delete(Entity),
    /// Move an Obj, keeping where it is in the world.
    Move(Entity, Destination),
    /// Open the prefab an Obj is an instance of, or part of one, on its own.
    OpenPrefab(Entity),
}

pub struct Hierarchy {
    tree: NodeId,
    filter: NodeId,
    add: NodeId,
    shown: Rc<RefCell<Snapshot>>,
    /// The row lit by a ping, and until when.
    pinged: Rc<Cell<Option<u64>>>,
    ping_until: Option<Instant>,
    /// Headers opened once, so a scene starts open but can be closed.
    opened: HashSet<u64>,
    /// What each item of the menu open now does.
    menu: Vec<Option<ObjAction>>,
}

impl Hierarchy {
    pub fn new(ui: &mut Ui, tree: NodeId, filter: NodeId, add: NodeId) -> Self {
        let shown = Rc::new(RefCell::new(Snapshot::default()));
        let pinged = Rc::new(Cell::new(None));
        ui.bind_tree(tree, Items(shown.clone(), pinged.clone()));
        ui.set_rows_draggable(tree, true);
        Self { tree, filter, add, shown, pinged, ping_until: None, opened: HashSet::new(), menu: Vec::new() }
    }

    /// What the + button, a context click or a dropped row asks for.
    pub fn action(&mut self, ui: &mut Ui) -> Option<ObjAction> {
        if let Some((item, onto)) = ui.dropped_item(self.tree) {
            return self.dropped(item, onto);
        }
        if let Some(index) = ui.menu_choice(self.add).or_else(|| ui.menu_choice(self.tree)) {
            return self.menu.get(index).copied().flatten();
        }
        if ui.clicked(self.add) {
            let [x, y, _, height] = ui.rect(self.add);
            self.open_menu(ui, self.add, (x, y + height + 4.0), None);
        } else if ui.context_clicked(self.tree) {
            let on = ui.item_at_pointer(self.tree);
            if on.is_none_or(is_header) {
                self.clear(ui);
            }
            let at = ui.pointer_position().unwrap_or_default();
            self.open_menu(ui, self.tree, at, on);
        }
        None
    }

    /// Where a row let go of takes its Obj. A scene's header is not moved.
    fn dropped(&self, item: u64, onto: Option<(u64, DropPlace)>) -> Option<ObjAction> {
        if is_header(item) {
            return None;
        }
        let entity = Entity::from_bits(item);
        let shown = self.shown.borrow();
        let to = match onto {
            Some((target, place)) => destination(&shown, target, place)?,
            // Below the rows: at the top of its own scene.
            None => Destination::SceneEnd(*shown.scenes.get(&shown.header_of(item)?)?),
        };
        Some(ObjAction::Move(entity, to))
    }

    /// Lets the rows take what is dragged from elsewhere, or stops them.
    pub fn take_drops(&self, ui: &mut Ui, take: bool) {
        ui.accept_row_drops(self.tree, take);
    }

    /// Something dragged from elsewhere and let go of on the rows since the
    /// last frame, and where it lands: inside an Obj, beside one, at the top
    /// of a scene, or `None` for the top of the active scene.
    pub fn dropped_from_elsewhere(&self, ui: &Ui) -> Option<(DragSource, Option<Destination>)> {
        let (source, onto) = ui.dropped_on_item(self.tree)?;
        let to = onto.and_then(|(target, place)| destination(&self.shown.borrow(), target, place));
        Some((source, to))
    }

    /// The menu of what can be done to the Obj or scene `on`, or made at
    /// the top.
    fn open_menu(&mut self, ui: &mut Ui, owner: NodeId, at: (f32, f32), on: Option<u64>) {
        let mut items = Vec::new();
        self.menu.clear();
        let shown = self.shown.borrow();
        let row = on.and_then(|item| shown.rows.get(&item));
        let name = row.map(|row| row.name.clone()).unwrap_or_default();
        let in_prefab = row.is_some_and(|row| row.prefab.is_some());
        let (parent, scene) = match on {
            Some(item) if is_header(item) => (None, shown.scenes.get(&item).copied()),
            Some(item) => (Some(Entity::from_bits(item)), None),
            None => (None, None),
        };
        drop(shown);
        if let Some(entity) = parent.filter(|_| in_prefab) {
            items.extend([MenuItem::new("Open Prefab"), MenuItem::separator()]);
            self.menu.extend([Some(ObjAction::OpenPrefab(entity)), None]);
        }
        if let Some(entity) = parent {
            let entries = [
                (MenuItem::new("Duplicate").shortcut("Ctrl+D"), Some(ObjAction::Duplicate(entity))),
                (MenuItem::new("Delete").shortcut("Delete"), Some(ObjAction::Delete(entity))),
                (MenuItem::separator(), None),
            ];
            for (item, action) in entries {
                items.push(item);
                self.menu.push(action);
            }
        }
        if on.is_some() {
            items.push(MenuItem::new(&format!("New in {name}")).enabled(false));
            self.menu.push(None);
        }
        for (kind, (label, ..)) in NEW_OBJS.iter().enumerate() {
            items.push(MenuItem::new(label));
            self.menu.push(Some(ObjAction::Create { kind, parent, scene }));
        }
        ui.open_menu(owner, at, &items);
    }

    /// Shows the world as it is now, if anything changed. `types` say which
    /// scripts are `IInstance`s while their behaviours are not running.
    pub fn update(&mut self, ui: &mut Ui, world: &World, levels: &Levels, types: &ScriptTypes) {
        if self.ping_until.is_some_and(|until| Instant::now() >= until) {
            self.ping_until = None;
            self.pinged.set(None);
            ui.refresh_list(self.tree);
        }
        let filter = ui.text(self.filter).to_lowercase();
        let snapshot = snapshot(world, levels, types, &filter);
        if *self.shown.borrow() == snapshot {
            return;
        }
        let headers: Vec<u64> = snapshot.scenes.keys().copied().collect();
        *self.shown.borrow_mut() = snapshot;
        for header in headers {
            if self.opened.insert(header) {
                ui.set_expanded(self.tree, header, true);
            }
        }
        ui.refresh_list(self.tree);
    }

    /// The Obj being dragged, when it is a row of this tree, as the
    /// Inspector's reference fields take it.
    pub fn dragged(&self, source: &DragSource) -> Option<Entity> {
        match *source {
            DragSource::Row { list, item: Some(item), .. } if list == self.tree && !is_header(item) => {
                Some(Entity::from_bits(item))
            }
            _ => None,
        }
    }

    /// Selects nothing, as clicking empty space in the Scene view does.
    pub fn clear(&mut self, ui: &mut Ui) {
        ui.set_selection(self.tree, &[]);
    }

    /// The selected Obj, when the selection changed since the last frame.
    /// A scene's header selects no Obj.
    pub fn selection_changed(&self, ui: &Ui) -> Option<Option<Entity>> {
        if !ui.selection_changed(self.tree) {
            return None;
        }
        let item = ui.selected_items(self.tree).first().copied().filter(|&item| !is_header(item));
        Some(item.map(Entity::from_bits))
    }

    /// Selects the Obj, opening its scene and the Objs it is in.
    pub fn select(&mut self, ui: &mut Ui, world: &World, entity: Entity) {
        self.open_to(ui, world, entity);
        ui.select_item(self.tree, entity.to_bits());
    }

    /// Shows the Obj without selecting it, as a log line about it asks, like
    /// Unity's ping: its scene and the Objs it is in open, the tree scrolls
    /// to it, and its row is lit for a moment.
    pub fn ping(&mut self, ui: &mut Ui, world: &World, entity: Entity) {
        self.open_to(ui, world, entity);
        ui.scroll_to_item(self.tree, entity.to_bits());
        self.pinged.set(Some(entity.to_bits()));
        self.ping_until = Some(Instant::now() + PING);
        ui.refresh_list(self.tree);
    }

    /// Opens the Obj's scene and the Objs it is in.
    fn open_to(&mut self, ui: &mut Ui, world: &World, entity: Entity) {
        let mut parent = world.get::<Parent>(entity).map(|p| p.0);
        while let Some(at) = parent {
            ui.set_expanded(self.tree, at.to_bits(), true);
            parent = world.get::<Parent>(at).map(|p| p.0);
        }
        if let Some(header) = self.shown.borrow().header_of(entity.to_bits()) {
            ui.set_expanded(self.tree, header, true);
        }
    }
}

/// Where something let go of on a row lands: on a scene's header, at the
/// top of that scene; otherwise inside the row's Obj, or beside it.
fn destination(shown: &Snapshot, target: u64, place: DropPlace) -> Option<Destination> {
    if is_header(target) {
        return shown.scenes.get(&target).map(|scene| Destination::SceneEnd(*scene));
    }
    let entity = Entity::from_bits(target);
    Some(match place {
        DropPlace::Onto => Destination::Into(entity),
        DropPlace::Before => Destination::Before(entity),
        DropPlace::After => Destination::After(entity),
    })
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

/// The scenes and their Objs as a tree, or with a filter, the Objs whose
/// names contain it as a flat list, in the scenes' order either way.
fn snapshot(world: &World, levels: &Levels, types: &ScriptTypes, filter: &str) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let scenes = levels.scene_order();
    // Each Obj's place: its scene, then its place there. Objs no scene keeps
    // show at the end of the active scene, where a save would put them.
    let active = levels.active_scene_guid().and_then(|guid| scenes.iter().position(|(g, ..)| *g == guid)).unwrap_or(0);
    let mut places: HashMap<Entity, (usize, usize)> = HashMap::new();
    for (scene, (_, _, entities)) in scenes.iter().enumerate() {
        places.extend(entities.iter().enumerate().map(|(at, entity)| (*entity, (scene, at))));
    }
    let objs = world
        .query_ref::<(Entity, &Name, Option<&Parent>, Option<&Camera>, Option<&Light>, Option<&MeshRenderer>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    let mut all: Vec<(Entity, Option<Entity>, Place)> = Vec::new();
    for (entity, name, parent, camera, light, mesh) in objs {
        if !filter.is_empty() && !name.0.to_lowercase().contains(filter) {
            continue;
        }
        let row = Row {
            name: name.0.clone(),
            icon: icon(camera.is_some(), light.is_some(), mesh.is_some()),
            header: false,
            badge: if is_instance(world, types, entity) { "Instance" } else { "" },
            prefab: prefab(world, entity),
        };
        snapshot.rows.insert(entity.to_bits(), row);
        let (scene, at) = places.get(&entity).copied().unwrap_or((active, usize::MAX));
        all.push((entity, parent.map(|p| p.0), (scene, at, entity.index())));
    }
    all.sort_by_key(|(.., place)| *place);
    if !filter.is_empty() {
        snapshot.roots = all.iter().map(|(entity, ..)| entity.to_bits()).collect();
        return snapshot;
    }
    for (index, (guid, path, _)) in scenes.iter().enumerate() {
        let header = index as u64 + 1;
        let name = path.rsplit('/').next().unwrap_or(path);
        let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string();
        snapshot.rows.insert(header, Row { name, icon: "scene", header: true, badge: "", prefab: None });
        snapshot.scenes.insert(header, *guid);
        snapshot.roots.push(header);
        snapshot.children.insert(header, Vec::new());
    }
    for (entity, parent, (scene, ..)) in all {
        let under = match parent.filter(|p| snapshot.rows.contains_key(&p.to_bits())) {
            Some(parent) => parent.to_bits(),
            None if scenes.is_empty() => {
                snapshot.roots.push(entity.to_bits());
                continue;
            }
            None => scene as u64 + 1,
        };
        snapshot.children.entry(under).or_default().push(entity.to_bits());
    }
    snapshot
}

/// Where an Obj shows: its scene's place among the open scenes, its place in
/// that scene, and its entity's index for Objs no scene keeps.
type Place = (usize, usize, u32);

/// Whether an Obj has a behaviour that is an `IInstance`: marked so while
/// it runs, or in edit mode, of a type the scripts' description says is one.
fn is_instance(world: &World, types: &ScriptTypes, entity: Entity) -> bool {
    if world.has::<InstanceMarker>(entity) {
        return true;
    }
    let Some(kept) = world.get::<EditBehaviours>(entity) else { return false };
    kept.0.iter().any(|spec| types.behaviour(&spec.type_name).is_some_and(|b| b.is_a(INSTANCE)))
}

/// `Some(true)` for a prefab instance's root, `Some(false)` for an Obj
/// inside one.
fn prefab(world: &World, entity: Entity) -> Option<bool> {
    if world.has::<PrefabInstance>(entity) {
        return Some(true);
    }
    world.has::<FromPrefab>(entity).then_some(false)
}
