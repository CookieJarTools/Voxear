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
//! it, and on a prefab instance's root to unpack it; on a scene's header or
//! the space below, to make one at the top. A double click on an Obj frames
//! it in the Scene view. A
//! scene opened alongside the open level is marked "Not in level", and its
//! header's menu offers to add it to the level; any scene's, while more than
//! one is open, to close it.
//! Dragging an Obj onto the middle of another puts it inside; near another's
//! top or bottom edge, beside it, before or after; onto a scene's header, at
//! the top of that scene; and below the rest, at the top of its own scene.
//! The editor carries these out (see [`ObjAction`]).
//!
//! An Obj drawing a whole model has a button of its own on its row, apart
//! from its arrow, that lists the model's parts under it, greyed: they are
//! the model's, not Objs, so they cannot be dragged, and clicking one
//! selects the model. Its menu, and theirs, offer to unpack the model to
//! Objs, as its Inspector does.
//!
//! While the editor lets it ([`Hierarchy::take_drops`]), as it does while a
//! prefab or model is dragged from the Project, the rows take drops from
//! elsewhere too, landing in the same places, except that below the rest is
//! the top of the active scene ([`Hierarchy::dropped_from_elsewhere`]).

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Content, EditBehaviours, Levels,
    audio::AudioSource,
    modules::Modules,
    prefab::{FromPrefab, PrefabInstance},
    script_types::ScriptTypes,
};
use voxear_render::{Camera, Light, LodGroup, MeshRenderer, ModelRenderer};
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

/// A model's part is an item of its own too, with the top bit set, which a
/// live entity's never has: its Obj's index, then its node in the model.
const PART: u64 = 1 << 63;
const NODE_BITS: u64 = 20;

fn part_item(owner: Entity, node: u32) -> u64 {
    PART | (u64::from(owner.index()) << NODE_BITS) | u64::from(node)
}

fn node_of(item: u64) -> u32 {
    (item & ((1 << NODE_BITS) - 1)) as u32
}

fn is_part(item: u64) -> bool {
    item & PART != 0
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
    /// Whether it draws a whole model, so has a button to list its parts,
    /// and whether they are listed.
    model: Option<bool>,
    /// A part of a model, listed under its Obj.
    part: bool,
}

/// What the tree shows, rebuilt from the world when it changes.
#[derive(Default, PartialEq)]
struct Snapshot {
    roots: Vec<u64>,
    children: HashMap<u64, Vec<u64>>,
    rows: HashMap<u64, Row>,
    /// Each header's scene, and those of scenes opened alongside the level.
    scenes: HashMap<u64, AssetGuid>,
    alongside: HashSet<u64>,
    /// The open level's name, when a level is open.
    level: Option<String>,
    /// Each model part's Obj, by the part's item.
    owners: HashMap<u64, Entity>,
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

/// How long a pinged row stays lit, in seconds.
const PING: f32 = 1.5;

/// What the tree shows, the row lit by a ping, if any, and each row's parts
/// button with the item it is bound to.
struct Items(Rc<RefCell<Snapshot>>, Rc<Cell<Option<u64>>>, Rc<RefCell<Vec<(NodeId, Option<u64>)>>>);

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
        let parts = ui.icon_button(row, "box");
        ui.add_class(parts, "row-parts");
        ui.set_tooltip(parts, "Show the model's parts", None);
        self.2.borrow_mut().push((parts, None));
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let snapshot = self.0.borrow();
        let Some(shown) = snapshot.rows.get(&item) else { return };
        let parts = ui.children(row);
        ui.set_class(row, "scene-header", shown.header);
        ui.set_icon(parts[0], shown.icon);
        ui.set_text(parts[1], &shown.name);
        for part in [parts[0], parts[1]] {
            ui.set_class(part, "prefab-root", shown.prefab == Some(true));
            ui.set_class(part, "prefab-member", shown.prefab == Some(false));
        }
        ui.set_text(parts[2], shown.badge);
        ui.set_visible(parts[2], !shown.badge.is_empty());
        let inside = snapshot.children.get(&item).map_or(0, Vec::len);
        let count = if inside > 0 && !shown.header { inside.to_string() } else { String::new() };
        ui.set_text(parts[3], &count);
        ui.set_class(row, "pinged", self.1.get() == Some(item));
        ui.set_class(row, "model-part", shown.part);
        ui.set_visible(parts[4], shown.model.is_some());
        ui.set_checked(parts[4], shown.model == Some(true));
        if let Some(button) = self.2.borrow_mut().iter_mut().find(|(button, _)| *button == parts[4]) {
            button.1 = Some(item);
        }
    }
}

/// What the Hierarchy asks the editor to do to Objs and scenes.
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
    /// Make a prefab instance's Objs Objs of the scene, by its root.
    Unpack(Entity),
    /// Make an Obj drawing a whole model an Obj for each of its parts.
    UnpackModel(Entity),
    /// Show an Obj in the Scene view, as Frame Selected does.
    Frame(Entity),
    /// Add a scene opened alongside the level to the level's file.
    AddToLevel(AssetGuid),
    /// Close an open scene, unloading its Objs.
    CloseScene(AssetGuid),
}

pub struct Hierarchy {
    tree: NodeId,
    filter: NodeId,
    add: NodeId,
    shown: Rc<RefCell<Snapshot>>,
    /// The row lit by a ping, and how many seconds it stays lit.
    pinged: Rc<Cell<Option<u64>>>,
    ping_left: f32,
    /// Headers opened once, so a scene starts open but can be closed.
    opened: HashSet<u64>,
    /// What each item of the menu open now does.
    menu: Vec<Option<ObjAction>>,
    /// Each row's parts button, with the item it shows now.
    part_buttons: Rc<RefCell<Vec<(NodeId, Option<u64>)>>>,
    /// The Objs whose models' parts are listed.
    showing_parts: HashSet<Entity>,
    /// The project's modules, whose Objs the menu offers.
    modules: Modules,
}

impl Hierarchy {
    pub fn new(ui: &mut Ui, tree: NodeId, filter: NodeId, add: NodeId) -> Self {
        let shown = Rc::new(RefCell::new(Snapshot::default()));
        let pinged = Rc::new(Cell::new(None));
        let part_buttons = Rc::new(RefCell::new(Vec::new()));
        ui.bind_tree(tree, Items(shown.clone(), pinged.clone(), part_buttons.clone()));
        ui.set_rows_draggable(tree, true);
        let (opened, menu, showing_parts) = (HashSet::new(), Vec::new(), HashSet::new());
        let modules = Modules::all();
        Self { tree, filter, add, shown, pinged, ping_left: 0.0, opened, menu, part_buttons, showing_parts, modules }
    }

    /// What the + button, a context click or a dropped row asks for.
    pub fn action(&mut self, ui: &mut Ui) -> Option<ObjAction> {
        let toggled = self.part_buttons.borrow().iter().find_map(|&(button, item)| item.filter(|_| ui.clicked(button)));
        if let Some(item) = toggled {
            let owner = Entity::from_bits(item);
            if !self.showing_parts.remove(&owner) {
                self.showing_parts.insert(owner);
                ui.set_expanded(self.tree, item, true);
            }
            return None;
        }
        if let Some((item, onto)) = ui.dropped_item(self.tree) {
            return self.dropped(item, onto);
        }
        if let Some(index) = ui.menu_choice(self.add).or_else(|| ui.menu_choice(self.tree)) {
            return self.menu.get(index).copied().flatten();
        }
        if ui.clicked(self.add) {
            let [x, y, _, height] = ui.rect(self.add);
            self.open_menu(ui, self.add, (x, y + height + 4.0), None);
        } else if let Some(item) = ui.item_at_pointer(self.tree).filter(|&item| !is_header(item))
            && ui.double_clicked(self.tree)
        {
            return self.obj_of(item).map(ObjAction::Frame);
        } else if ui.clicked(self.tree) && ui.item_at_pointer(self.tree).is_none() {
            // A click on the space below the rows selects nothing, as in Unity.
            self.clear(ui);
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
        if is_header(item) || is_part(item) || onto.is_some_and(|(target, _)| is_part(target)) {
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
        let instance_root = row.is_some_and(|row| row.prefab == Some(true));
        let (parent, scene) = match on {
            Some(item) if is_header(item) => (None, shown.scenes.get(&item).copied()),
            Some(item) => (shown.owners.get(&item).copied().or(Some(Entity::from_bits(item))), None),
            None => (None, None),
        };
        let model = parent.filter(|owner| shown.rows.get(&owner.to_bits()).is_some_and(|row| row.model.is_some()));
        let header = on.filter(|&item| is_header(item));
        let add_to_level = header.filter(|item| shown.alongside.contains(item)).and(shown.level.clone()).zip(scene);
        let closes = header.and(scene).filter(|_| shown.scenes.len() > 1);
        drop(shown);
        if let Some((level, scene)) = &add_to_level {
            items.push(MenuItem::new(&format!("Add to the {level} Level")));
            self.menu.push(Some(ObjAction::AddToLevel(*scene)));
        }
        if let Some(scene) = closes {
            items.push(MenuItem::new("Close Scene"));
            self.menu.push(Some(ObjAction::CloseScene(scene)));
        }
        if add_to_level.is_some() || closes.is_some() {
            items.push(MenuItem::separator());
            self.menu.push(None);
        }
        if let Some(entity) = parent.filter(|_| in_prefab) {
            items.push(MenuItem::new("Open Prefab"));
            self.menu.push(Some(ObjAction::OpenPrefab(entity)));
            if instance_root {
                items.push(MenuItem::new("Unpack Prefab"));
                self.menu.push(Some(ObjAction::Unpack(entity)));
            }
            items.push(MenuItem::separator());
            self.menu.push(None);
        }
        if let Some(entity) = model {
            items.push(MenuItem::new("Unpack to Objs"));
            self.menu.push(Some(ObjAction::UnpackModel(entity)));
            items.push(MenuItem::separator());
            self.menu.push(None);
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
        for (kind, (label, .., module)) in NEW_OBJS.iter().enumerate() {
            if module.is_none_or(|module| self.modules.has(module)) {
                items.push(MenuItem::new(label));
                self.menu.push(Some(ObjAction::Create { kind, parent, scene }));
            }
        }
        ui.open_menu(owner, at, &items);
    }

    /// Shows the world as it is now, if anything changed. `types` say which
    /// scripts are `IInstance`s while their behaviours are not running.
    pub fn update(&mut self, ui: &mut Ui, world: &World, (levels, content): (&Levels, &Content), types: &ScriptTypes) {
        self.modules = world.resource::<Modules>().copied().unwrap_or_else(Modules::all);
        let filter = ui.text(self.filter).to_lowercase();
        self.showing_parts.retain(|&owner| world.has::<ModelRenderer>(owner));
        let snapshot = snapshot(world, (levels, content), types, (&filter, &self.showing_parts));
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
            DragSource::Row { list, item: Some(item), .. } if list == self.tree => {
                (!is_header(item) && !is_part(item)).then(|| Entity::from_bits(item))
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
        Some(item.and_then(|item| self.obj_of(item)))
    }

    /// Every Obj selected, in the tree's order; a scene's header is none,
    /// and a model's part its model's Obj.
    pub fn selected_objs(&self, ui: &Ui) -> Vec<Entity> {
        let mut objs: Vec<Entity> = Vec::new();
        for obj in ui.selected_items(self.tree).into_iter().filter_map(|item| self.obj_of(item)) {
            if !objs.contains(&obj) {
                objs.push(obj);
            }
        }
        objs
    }

    /// The model's part selected, by its Obj and node, when a part is what
    /// is selected.
    pub fn selected_part(&self, ui: &Ui) -> Option<(Entity, u32)> {
        let item = ui.selected_items(self.tree).first().copied().filter(|&item| is_part(item))?;
        Some((self.shown.borrow().owners.get(&item).copied()?, node_of(item)))
    }

    /// Selects a model's part, by its Obj and node, when its row is listed,
    /// and otherwise its Obj.
    pub fn select_part(&mut self, ui: &mut Ui, world: &World, (owner, node): (Entity, u32)) {
        self.open_to(ui, world, owner);
        ui.set_expanded(self.tree, owner.to_bits(), true);
        if !ui.select_item(self.tree, part_item(owner, node)) {
            ui.select_item(self.tree, owner.to_bits());
        }
    }

    /// The Obj an item is: none for a scene's header, and its model's Obj
    /// for a model's part.
    fn obj_of(&self, item: u64) -> Option<Entity> {
        match (is_header(item), is_part(item)) {
            (true, _) => None,
            (_, true) => self.shown.borrow().owners.get(&item).copied(),
            _ => Some(Entity::from_bits(item)),
        }
    }

    /// Selects these Objs, opening their scenes and the Objs they are in.
    pub fn select_many(&mut self, ui: &mut Ui, world: &World, entities: &[Entity]) {
        for &entity in entities {
            self.open_to(ui, world, entity);
        }
        let items: Vec<u64> = entities.iter().map(|entity| entity.to_bits()).collect();
        ui.select_items(self.tree, &items);
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
        self.ping_left = PING;
        ui.refresh_list(self.tree);
    }

    /// Counts a ping down by a frame's `delta` seconds, and puts its light
    /// out once it is over.
    pub fn fade_ping(&mut self, ui: &mut Ui, delta: f32) {
        if self.ping_left <= 0.0 {
            return;
        }
        self.ping_left -= delta;
        if self.ping_left <= 0.0 {
            self.pinged.set(None);
            ui.refresh_list(self.tree);
        }
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
    if is_part(target) {
        return None;
    }
    let entity = Entity::from_bits(target);
    Some(match place {
        DropPlace::Onto => Destination::Into(entity),
        DropPlace::Before => Destination::Before(entity),
        DropPlace::After => Destination::After(entity),
    })
}

/// The icon for an Obj with a camera, a light, a mesh, a whole model, an
/// Audio Source or a LOD Group, the first it has in that order, or else the
/// plain Obj's.
pub fn icon(world: &World, entity: Entity) -> &'static str {
    if world.has::<Camera>(entity) {
        "camera"
    } else if world.has::<Light>(entity) {
        "light"
    } else if world.has::<MeshRenderer>(entity) {
        "cube"
    } else if world.has::<ModelRenderer>(entity) {
        "box"
    } else if world.has::<AudioSource>(entity) {
        "audio"
    } else if world.has::<LodGroup>(entity) {
        "lod"
    } else {
        "obj"
    }
}

/// The scenes and their Objs as a tree, or with a filter, the Objs whose
/// names contain it as a flat list, in the scenes' order either way.
fn snapshot(
    world: &World,
    (levels, content): (&Levels, &Content),
    types: &ScriptTypes,
    (filter, showing_parts): (&str, &HashSet<Entity>),
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let scenes = levels.scene_order();
    // Each Obj's place: its scene, then its place there. Objs no scene keeps
    // show at the end of the active scene, where a save would put them.
    let active = levels.active_scene_guid().and_then(|guid| scenes.iter().position(|(g, ..)| *g == guid)).unwrap_or(0);
    let mut places: HashMap<Entity, (usize, usize)> = HashMap::new();
    for (scene, (_, _, entities)) in scenes.iter().enumerate() {
        places.extend(entities.iter().enumerate().map(|(at, entity)| (*entity, (scene, at))));
    }
    let objs = world.query_ref::<(Entity, &Name, Option<&Parent>)>().without::<EditorOnly>().without::<Destroying>();
    let mut all: Vec<(Entity, Option<Entity>, Place)> = Vec::new();
    for (entity, name, parent) in objs {
        if !filter.is_empty() && !name.0.to_lowercase().contains(filter) {
            continue;
        }
        let prefab = prefab(world, entity);
        // A prefab instance shows Unity's prefab cube, tinted.
        let icon = if prefab == Some(true) { "cube" } else { icon(world, entity) };
        let badge = if is_instance(world, types, entity) { "Instance" } else { "" };
        let model = world.has::<ModelRenderer>(entity).then(|| showing_parts.contains(&entity));
        let row = Row { name: name.0.clone(), icon, header: false, badge, prefab, model, part: false };
        snapshot.rows.insert(entity.to_bits(), row);
        let (scene, at) = places.get(&entity).copied().unwrap_or((active, usize::MAX));
        all.push((entity, parent.map(|p| p.0), (scene, at, entity.index())));
    }
    all.sort_by_key(|(.., place)| *place);
    if !filter.is_empty() {
        snapshot.roots = all.iter().map(|(entity, ..)| entity.to_bits()).collect();
        return snapshot;
    }
    // A level's path ends in .level; a scene or prefab may be open on its own instead.
    snapshot.level = levels.current().filter(|path| path.ends_with(".level")).map(|path| stem(path).to_string());
    for (index, (guid, path, _)) in scenes.iter().enumerate() {
        let header = index as u64 + 1;
        let alongside = snapshot.level.is_some() && levels.opened_alongside(*guid);
        let badge = if alongside { "Not in level" } else { "" };
        snapshot
            .rows
            .insert(header, Row {
                name: stem(path).to_string(),
                icon: "scene",
                header: true,
                badge,
                prefab: None,
                model: None,
                part: false,
            });
        snapshot.scenes.insert(header, *guid);
        if alongside {
            snapshot.alongside.insert(header);
        }
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
    // A listed model's parts, after its own Objs.
    let listed: Vec<Entity> =
        showing_parts.iter().copied().filter(|owner| snapshot.rows.contains_key(&owner.to_bits())).collect();
    for owner in listed {
        let Some(model) = world.get::<ModelRenderer>(owner) else { continue };
        let names = part_names(content, model);
        for part in model.parts.iter() {
            let item = part_item(owner, part.node);
            let name = names.get(part.node as usize).filter(|name| !name.is_empty()).cloned();
            let name = name.unwrap_or_else(|| format!("Part {}", part.node + 1));
            let row = Row { name, icon: "box", header: false, badge: "", prefab: None, model: None, part: true };
            snapshot.rows.insert(item, row);
            snapshot.owners.insert(item, owner);
            snapshot.children.entry(owner.to_bits()).or_default().push(item);
        }
    }
    snapshot
}

/// The names a model gives its nodes, in order; none when it is not
/// loaded. A name many nodes share, as a model whose every node is `mesh`,
/// is told apart by each node's material's name.
pub(crate) fn part_names(content: &Content, model: &ModelRenderer) -> Vec<String> {
    let guid = model.parts.first().and_then(|part| content.mesh_ref(part.mesh)).and_then(|mesh| mesh.guid());
    let Some(guid) = guid else { return Vec::new() };
    let nodes = content.loaded_model(guid).unwrap_or_default();
    let materials = content.loaded_model_materials(guid).unwrap_or_default();
    let shared = |name: &str| nodes.iter().filter(|node| node.name == name).count() > 1;
    nodes
        .iter()
        .map(|node| {
            let material = node.material_part.as_ref().and_then(|part| materials.iter().find(|(p, _)| p == part));
            match material.map(|(_, name)| name.as_str()).filter(|name| !name.is_empty()) {
                Some(material) if node.name.is_empty() => material.to_string(),
                Some(material) if shared(&node.name) => format!("{} · {material}", node.name),
                _ => node.name.clone(),
            }
        })
        .collect()
}

/// A file's name without its folder or extension.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
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

#[cfg(test)]
mod tests {
    use voxear_core::math::Vec3;

    use super::*;

    #[test]
    fn an_obj_shows_the_icon_of_what_it_has() {
        let mut world = World::new();
        let plain = world.spawn((Name("Obj".into()),));
        let sound = world.spawn((Name("Waterfall".into()), AudioSource::default()));
        let lamp = world.spawn((Name("Lamp".into()), AudioSource::default(), Light::point(Vec3::ONE, 1.0, 5.0)));
        assert_eq!(icon(&world, plain), "obj");
        assert_eq!(icon(&world, sound), "audio", "an Obj with only an Audio Source");
        assert_eq!(icon(&world, lamp), "light", "a light comes before a sound");
    }
}
