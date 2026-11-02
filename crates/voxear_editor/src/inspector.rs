//! The Inspector: the selected Obj's header (its kind, name, whether it is
//! persistent, staying through level changes, and the active, tag, layer
//! and static settings to come; for a prefab instance or an Obj in one, the
//! prefab, with Open and Select, as Unity's prefab bar), then a section for its
//! Transform and for each component (see `props`), divided by lines. Values
//! apply as they are typed or picked, and values that change elsewhere show
//! here, except in the field being edited. When an Obj's components, or what
//! they show, change (a light of another kind, a component added or undone),
//! the Inspector is built again.
//!
//! Menus: a context click on a value's row offers Reset for the Transform
//! and script fields, and while playing, "Keep value after play", which
//! brings the value the played Obj has when play stops back into the edited
//! level, as one step to undo; kept rows are marked (`kept`). A context
//! click on a component's header offers Remove Component, and the
//! Transform's menu button Reset. Add Component lists the components and
//! the project's scripts (`script_types`) the Obj does not have yet.
//!
//! Script fields show as their kind says: a list as its items, each with a
//! button to take it out and the list's to add one; a `[Serializable]`
//! table as its fields, indented under it; and a reference to an Obj, a
//! behaviour or an asset as what it points at. As in Unity, a click on a
//! reference pings what it points at, a double click selects an Obj it
//! points at, and its arrow opens the menu to pick another.

use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    prefab_overrides::ValuePath,
    script_types::{BehaviourType, FieldInspector, InfoKind, ScriptTypes},
};
use voxear_render::{LodGroup, Renderer};
use voxear_scene::{Layer, Layers, Name, ScenePersistence, Transform};
use voxear_ui::{DragSource, MenuItem, NodeId, Style, Ui};

use voxear_engine::prefab::{FromPrefab, Locks, PrefabInstance};

use voxear_scripting::{DrawRequest, Drawn};

use crate::{
    asset_inspector::{AssetAction, AssetView},
    attributes,
    collection::{self, Collection, CollectionItem},
    field_drawers::{DrawerUi, FieldDrawers, ToOpen},
    fields::{self, Pick, Step},
    history::{Edit, ObjState},
    lod_inspector::{GroupLods, LodEdge, LodEye, MeshLods},
    object_picker::{Choice, Chosen},
    project_view::ProjectItem,
    props::{self, Part, Prop, Value},
};

/// The fields showing an Obj's name, persistence and Transform, by what
/// they edit, and the row each is on.
struct Fields {
    name: NodeId,
    persistent: NodeId,
    /// The Layer dropdown, and the layer each of its choices is.
    layer: (NodeId, Vec<u8>),
    position: [NodeId; 3],
    rotation: [NodeId; 3],
    scale: [NodeId; 3],
    rows: [(Field, NodeId); 4],
}

/// A value the menus can keep after play or reset: the name, a Transform
/// row, or a component's value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Position,
    Rotation,
    Scale,
    Prop(Prop),
}

impl Field {
    pub fn label(&self) -> String {
        match self {
            Field::Name => "Name".into(),
            Field::Position => "Position".into(),
            Field::Rotation => "Rotation".into(),
            Field::Scale => "Scale".into(),
            Field::Prop(prop) => props::label(prop),
        }
    }

    /// Copies the name or a Transform value from one state of an Obj into
    /// another. A component's value goes through `props` instead.
    pub fn copy(&self, from: &ObjState, to: &mut ObjState) {
        match self {
            Field::Name => to.name.clone_from(&from.name),
            Field::Position => to.transform.position = from.transform.position,
            Field::Rotation => to.transform.rotation = from.transform.rotation,
            Field::Scale => to.transform.scale = from.transform.scale,
            Field::Prop(_) => {}
        }
    }

    /// Sets a Transform value as Unity's Reset leaves it. Others are left as
    /// they are.
    pub fn reset(&self, state: &mut ObjState) {
        match self {
            Field::Position => state.transform.position = Vec3::ZERO,
            Field::Rotation => state.transform.rotation = Quat::IDENTITY,
            Field::Scale => state.transform.scale = Vec3::ONE,
            Field::Name | Field::Prop(_) => {}
        }
    }
}

/// What the Prefab row asks for: the prefab opened on its own, or shown in
/// the Project, by its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefabAction {
    Open(String),
    Select(String),
}

/// What a menu in the Inspector asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Keep the value after play, or stop keeping it.
    Keep(Field),
    Reset(Field),
    ResetTransform,
    Add(Part),
    Remove(Part),
    /// Open the object picker for a reference, under its field.
    Browse(Prop, NodeId),
    /// Light up what a reference points at, in the Hierarchy or the Project.
    Ping(Prop),
    /// Select the Obj a reference points at.
    SelectTarget(Prop),
    /// Open the colour picker for a colour, under its swatch.
    EditColor(Prop, NodeId),
    /// Point a reference at what its picker picked.
    Pick(Prop, Pick),
    /// Point a reference at what was dragged onto it, from the Hierarchy or
    /// the Project, if it fits.
    Dropped(Prop, DragSource),
    /// Move a script up (-1) or down (1) among the Obj's scripts.
    Move(Part, isize),
    /// Lock a component for designers, or unlock it: only in a prefab opened
    /// on its own.
    Lock(Part, bool),
    /// Show a locked component's value to designers, or hide it from them.
    ShowToDesigners(Prop, bool),
    /// Put a new level into a LOD Group before the one at this place, or
    /// after its last; or take the level at this place out.
    InsertLodLevel(usize),
    DeleteLodLevel(usize),
    /// Move the Scene view to where it draws this LOD of the mesh.
    ViewLod(usize),
    /// Open Add Component's search under its button.
    BrowseComponents(NodeId),
    /// Make the Mesh Renderer draw this LOD at every distance, or none.
    ForceLod(Option<u8>),
    /// Let the engine switch the Mesh Renderer's LODs again.
    ResetLodTransitions,
    /// Put what was dragged onto a LOD Group's level in it.
    DropOnLodLevel(usize, DragSource),
    /// Put the prefab's values at these paths back on the Obj, or write the
    /// Obj's into the prefab.
    RevertOverrides(Vec<ValuePath>),
    ApplyOverrides(Vec<ValuePath>),
    /// The same for every value the whole instance changes.
    RevertAll,
    ApplyAll,
    /// Copy a component's values, or paste those copied over it or onto
    /// the Obj as a new component.
    CopyComponent(Part),
    PasteComponent,
}

/// A list shown in a collection field while the Inspector is built: the
/// list, its field, whether it can change, and the item each of its items
/// shows.
struct ListBox {
    list: Prop,
    collection: Collection,
    editable: bool,
    shown: Vec<usize>,
}

impl ListBox {
    /// Whether a value is one of this list's items.
    fn holds(&self, value: &Prop) -> bool {
        let (Prop::Field { behaviour, path }, Prop::Field { behaviour: of, path: list }) = (value, &self.list) else {
            return false;
        };
        behaviour == of && path.len() == list.len() + 1 && path.starts_with(list) && props::is_item(value)
    }

    /// The next item in the field, for the list's item `item`.
    fn add(&mut self, ui: &mut Ui, item: &Prop, editable: bool) -> CollectionItem {
        if let Prop::Field { path, .. } = item
            && let Some(Step::Item(index)) = path.last()
        {
            self.shown.push(*index);
        }
        let place = self.shown.len().saturating_sub(1);
        collection::item(ui, self.collection.items, place, self.editable && editable)
    }
}

/// A component value's row and the control that edits it.
struct PropRow {
    prop: Prop,
    row: NodeId,
    control: Control,
    /// A list's button that adds an item, and an item's that takes it out.
    add: Option<NodeId>,
    remove: Option<NodeId>,
    /// What `[Tooltip]` says, shown when the pointer rests on the label.
    tooltip: Option<String>,
    /// The least and most a number may be, from `[Range]`, `[MinValue]`,
    /// `[MaxValue]` or `[ProgressBar]`.
    limits: (Option<f64>, Option<f64>),
}

enum Control {
    /// A field typed in: a number, a whole number or text.
    Text(NodeId),
    Toggle(NodeId),
    /// A field for each number, for a vector or a colour, with the
    /// colour's swatch.
    Axes(Vec<NodeId>, Option<NodeId>),
    /// A reference: a field showing what it points at, which pings it, its
    /// label, and its arrow, which opens a menu to pick another.
    Picker(NodeId, NodeId, NodeId),
    /// A dropdown, and the options it was last given.
    Choice(NodeId, Vec<String>),
    /// A `[ValueDropdown]`: a dropdown, and the value each option sets.
    Offered(NodeId, Vec<Value>),
    /// A number with `[Range]`: a slider and its field.
    Range(NodeId, NodeId),
    /// An enum with `[EnumToggleButtons]`: a button for each value.
    Buttons(NodeId, Vec<String>),
    /// A number with `[ProgressBar]`: the bar, its fill, its value, and the
    /// numbers it runs between.
    Progress(NodeId, NodeId, NodeId, [f64; 2]),
    /// A label, for what is shown but not edited here.
    Fixed(NodeId),
    /// Nothing of the Inspector's own: the scripts' drawers draw it all.
    Drawn,
}

/// What a behaviour's code said about the shown Obj's fields, by behaviour
/// and field: `[ValidateInput]` errors, and the values `[ValueDropdown]`s
/// offer, each with its name. The editor asks the scripts for them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptAnswers {
    pub errors: std::collections::HashMap<(String, String), String>,
    pub options: std::collections::HashMap<(String, String), Vec<(String, Value)>>,
}

/// A fold `[Excerpt]` puts above a field, which the editor fills with the
/// part of a VoxDocs page it names.
pub struct ExcerptFold {
    pub fold: NodeId,
    /// The page, or section of one, as `[Excerpt]` names it.
    pub reference: String,
    /// The VoxDocs version last drawn in it, to draw it again when the
    /// pages change.
    pub shown: Option<u64>,
    /// The field it is above, to keep it open while the sections are built again.
    key: String,
}

/// What the sections shown were built from: the components and their
/// values, whether the game was playing, and the Obj's locks.
type Built = (Vec<(Part, Vec<Prop>)>, bool, Option<Locks>);

pub struct Inspector {
    root: NodeId,
    /// Its lock button, and whether it is locked: showing what it shows
    /// rather than following the selection.
    lock: Option<NodeId>,
    locked: bool,
    shown: Option<Entity>,
    fields: Option<Fields>,
    /// The Transform's menu button, and each section's header with its
    /// component (`None` for the Transform).
    transform_menu: Option<NodeId>,
    headers: Vec<(Option<Part>, NodeId)>,
    rows: Vec<PropRow>,
    add: Option<NodeId>,
    /// The project's scripts, as their last build describes them.
    types: ScriptTypes,
    /// Whether the theme is dark, for the colours of code a script shows.
    dark: bool,
    /// To build again when any of it changes.
    built: Built,
    /// A prefab is open on its own, so locks can be set, and every value of
    /// a locked component shows, marked for whether designers see it.
    prefab_mode: bool,
    /// The element whose menu is open, and what each of its items does.
    menu: Option<(NodeId, Vec<Option<RowAction>>)>,
    /// What is selected in the Project panel, when that is what it shows.
    asset: Option<AssetView>,
    /// What the attributes' conditions came to when the sections were
    /// built, to build them again when it changes.
    conditions: Vec<bool>,
    /// The attribute groups' tabs and foldouts shown, by name, and the tab
    /// picked and the foldouts closed, kept while the sections are built
    /// again.
    group_tabs: Vec<(String, NodeId)>,
    group_foldouts: Vec<(String, NodeId)>,
    tab_choices: std::collections::HashMap<String, usize>,
    closed_groups: std::collections::HashSet<String>,
    /// The `[PreviewField]` images shown: the field, the image and its size.
    previews: Vec<(Prop, NodeId, u32)>,
    /// The book buttons `[Documentation]` puts on sections and fields, with
    /// the VoxDocs page each opens.
    doc_buttons: Vec<(NodeId, String)>,
    /// The `[Excerpt]` folds shown, and the fields whose folds are open.
    excerpts: Vec<ExcerptFold>,
    open_excerpts: std::collections::HashSet<String>,
    /// The `[Button]`s shown, each with its behaviour and method.
    buttons: Vec<(NodeId, String, String)>,
    /// `[OnValueChanged]` methods to call for fields changed this frame, each
    /// with its behaviour.
    callbacks: Vec<(String, String)>,
    /// What the scripts answered, and the box each `[ValidateInput]` field
    /// has for its error, shown while there is one.
    answers: ScriptAnswers,
    validation_boxes: Vec<((String, String), NodeId, NodeId)>,
    /// What the answers were last asked for: the Obj and its scene values.
    asked_for: String,
    /// The page each `[ListDrawerSettings]` list shows, by its place, and its
    /// buttons: back, on, the list's place and how many pages it has.
    pages: std::collections::HashMap<String, usize>,
    pagers: Vec<(NodeId, NodeId, String, usize)>,
    /// The `[ColorPalette]` swatches shown: each, its field and its colour.
    palette_swatches: Vec<(NodeId, Prop, [f32; 3])>,
    /// Each list shown in a collection field: the list, its items' box,
    /// whose items drag into a new order, and which item each shows, as a
    /// page shows some.
    collections: Vec<(Prop, NodeId, Vec<usize>)>,
    /// The fields the scripts' drawers draw, and the behaviours their
    /// inspectors draw whole.
    drawers: FieldDrawers,
    /// Each behaviour its inspector draws, with the panel its usual fields
    /// are built in, which goes into the inspector's `next` element, or out
    /// of sight when it has none.
    inspected: Vec<(String, NodeId)>,
    /// The Prefab row's buttons and the prefab's path, for a prefab
    /// instance or an Obj in one.
    prefab_buttons: Option<PrefabButtons>,
    /// What the Obj shown changes from its prefab, as the editor last found.
    overrides: Vec<ValuePath>,
    /// The component copied, for pasting.
    copied: Option<Part>,
    /// The other Objs selected with the one shown, which an edit made here
    /// goes to as well, each that has what was edited.
    also: Vec<Entity>,
    /// What the Mesh Renderer and the LOD Group show of levels of detail,
    /// and the camera they say it of.
    mesh_lods: Option<MeshLods>,
    group_lods: Option<GroupLods>,
    lod_eye: Option<LodEye>,
    /// The LOD bar's edge dragged this frame, for the Scene view to follow.
    lod_edge: Option<LodEdge>,
    /// The LOD Group level selected on its bar, whose settings show.
    lod_level: usize,
}

impl Inspector {
    pub fn new(root: NodeId) -> Self {
        Self::with_owner(root, 0)
    }

    /// An Inspector of several, its drawers kept apart from the others' by
    /// `owner` (see `field_drawers`).
    pub fn with_owner(root: NodeId, owner: u32) -> Self {
        Self {
            root,
            lock: None,
            locked: false,
            shown: None,
            fields: None,
            transform_menu: None,
            headers: Vec::new(),
            rows: Vec::new(),
            add: None,
            types: ScriptTypes::default(),
            dark: true,
            built: (Vec::new(), false, None),
            prefab_mode: false,
            menu: None,
            asset: None,
            conditions: Vec::new(),
            group_tabs: Vec::new(),
            group_foldouts: Vec::new(),
            tab_choices: std::collections::HashMap::new(),
            closed_groups: std::collections::HashSet::new(),
            previews: Vec::new(),
            doc_buttons: Vec::new(),
            excerpts: Vec::new(),
            open_excerpts: std::collections::HashSet::new(),
            buttons: Vec::new(),
            callbacks: Vec::new(),
            answers: ScriptAnswers::default(),
            validation_boxes: Vec::new(),
            asked_for: String::new(),
            pages: std::collections::HashMap::new(),
            pagers: Vec::new(),
            palette_swatches: Vec::new(),
            collections: Vec::new(),
            drawers: FieldDrawers::new(owner),
            inspected: Vec::new(),
            prefab_buttons: None,
            overrides: Vec::new(),
            copied: None,
            also: Vec::new(),
            mesh_lods: None,
            group_lods: None,
            lod_eye: None,
            lod_edge: None,
            lod_level: 0,
        }
    }

    /// The button that locks it, as Unity's padlock does, which it lights
    /// while locked.
    pub fn set_lock_button(&mut self, ui: &mut Ui, button: NodeId) {
        self.lock = Some(button);
        ui.set_checked(button, self.locked);
    }

    /// Locks or unlocks it when its lock button was clicked. Returns whether
    /// it did.
    pub fn update_lock(&mut self, ui: &mut Ui) -> bool {
        let Some(button) = self.lock.filter(|&b| ui.clicked(b)) else { return false };
        self.locked = !self.locked;
        ui.set_checked(button, self.locked);
        true
    }

    /// Whether it keeps showing what it shows, rather than the selection.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Which of several Inspectors' drawers are its own, as the scripts know them.
    pub fn drawer_owner(&self) -> u32 {
        self.drawers.owner()
    }

    /// Whether the scripts are to be asked again, for an Obj and scene values
    /// `key` stands for: true when it changed since the last time, noting
    /// it. An empty key asks again next time.
    pub fn ask_again(&mut self, key: &str) -> bool {
        if self.asked_for == key {
            return false;
        }
        self.asked_for = key.to_string();
        true
    }

    /// Takes what the scripts answered about the shown Obj. Errors show in
    /// place, so a field being typed in keeps its focus; new values to offer
    /// build the sections again.
    pub fn set_answers(&mut self, ui: &mut Ui, answers: ScriptAnswers) {
        if answers.options != self.answers.options {
            self.built.0.clear();
        }
        for (key, frame, text) in &self.validation_boxes {
            let error = answers.errors.get(key);
            ui.set_visible(*frame, error.is_some());
            if let Some(error) = error.filter(|e| ui.text(*text) != **e) {
                ui.set_text(*text, error);
            }
        }
        self.answers = answers;
    }

    /// The `[Button]` clicked this frame: its behaviour and method.
    pub fn clicked_button(&self, ui: &Ui) -> Option<(String, String)> {
        let (_, behaviour, method) = self.buttons.iter().find(|(button, ..)| ui.clicked(*button))?;
        Some((behaviour.clone(), method.clone()))
    }

    /// The `[OnValueChanged]` methods to call for what was changed since the
    /// last time, each with its behaviour.
    pub fn take_callbacks(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.callbacks)
    }

    /// The Obj showing, if any.
    pub fn shown(&self) -> Option<Entity> {
        self.shown
    }

    /// The values of a locked component that designers see, or `None` when
    /// it is not locked.
    fn designer_fields(&self, game: &Game, entity: Entity, part: &Part) -> Option<Vec<String>> {
        game.engine.world.get::<Locks>(entity)?.0.get(&part.lock_key()).cloned()
    }

    /// A component's menu: moving a script among the Obj's scripts, locking
    /// it for designers when its prefab is open, and removing it, which a
    /// locked component outside its prefab cannot be.
    fn header_items(&self, game: &Game, part: &Part, playing: bool) -> Vec<(MenuItem, Option<RowAction>)> {
        let Some(entity) = self.shown else { return Vec::new() };
        let locked = self.designer_fields(game, entity, part).is_some();
        let fixed = locked && !self.prefab_mode;
        let mut items = Vec::new();
        if let Part::Behaviour(_) = part {
            let scripts: Vec<&Part> =
                self.built.0.iter().map(|(p, _)| p).filter(|p| matches!(p, Part::Behaviour(_))).collect();
            let at = scripts.iter().position(|p| *p == part).unwrap_or_default();
            let movable = !playing && !fixed;
            let up = MenuItem::new("Move Up").enabled(movable && at > 0);
            let down = MenuItem::new("Move Down").enabled(movable && at + 1 < scripts.len());
            items.push((up, Some(RowAction::Move(part.clone(), -1))));
            items.push((down, Some(RowAction::Move(part.clone(), 1))));
            items.push((MenuItem::separator(), None));
        }
        if self.prefab_mode {
            let label = if locked { "Unlock for Designers" } else { "Lock for Designers" };
            items.push((MenuItem::new(label), Some(RowAction::Lock(part.clone(), !locked))));
            items.push((MenuItem::separator(), None));
        }
        let removable = !(playing && matches!(part, Part::Behaviour(_))) && !fixed;
        items.push((MenuItem::new("Remove Component").enabled(removable), Some(RowAction::Remove(part.clone()))));
        items.push((MenuItem::separator(), None));
        let copies = *part != Part::LodGroup;
        items.push((MenuItem::new("Copy Component").enabled(copies), Some(RowAction::CopyComponent(part.clone()))));
        let has = |copied: &Part| self.built.0.iter().any(|(p, _)| p == copied);
        let pastes = |over: bool| {
            let copied = self.copied.as_ref().filter(|_| !playing && !fixed);
            copied.is_some_and(|copied| if over { copied == part } else { !has(copied) })
        };
        let values = MenuItem::new("Paste Component Values").enabled(pastes(true));
        let new = MenuItem::new("Paste Component As New").enabled(pastes(false));
        items.push((values, Some(RowAction::PasteComponent)));
        items.push((new, Some(RowAction::PasteComponent)));
        items
    }

    /// The component copied, which a component's menu pastes.
    pub fn set_copied(&mut self, part: Option<Part>) {
        self.copied = part;
    }

    /// Colours code a script shows for a dark theme or a light one.
    pub fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
    }

    /// The project's scripts as a new build describes them. What is showing
    /// is built again on the next update.
    pub fn set_types(&mut self, types: ScriptTypes) {
        self.types = types;
        self.built.0.clear();
    }

    /// Whether a prefab is open on its own: see `prefab_mode`.
    pub fn set_prefab_mode(&mut self, on: bool) {
        self.prefab_mode = on;
        self.built.0.clear();
    }

    /// Above a row: its `[Title]`, its `[InfoBox]`es that show, an error
    /// when a `[Required]` field is empty, and its `[Excerpt]`.
    fn decorate(
        &mut self,
        ui: &mut Ui,
        container: NodeId,
        game: &Game,
        (entity, value): (Entity, &Prop),
        (meta, label): (&FieldInspector, &str),
    ) {
        if let Some(title) = &meta.title {
            let heading = ui.add_label(container, title);
            ui.add_class(heading, "attr-title");
            let line = ui.panel(container, Style::default());
            ui.add_class(line, "attr-title-line");
        }
        for info in &meta.info {
            if attributes::info_shows(game, &self.types, entity, value, info.visible_if.as_deref()) {
                info_box(ui, container, info.kind, &info.message);
            }
        }
        if attributes::missing(game, &self.types, entity, value) {
            let message = meta.required.as_deref().filter(|m| !m.is_empty());
            let message = message.map_or_else(|| format!("{label} is required"), str::to_string);
            info_box(ui, container, InfoKind::Error, &message);
        }
        if let (Some(_), Prop::Field { behaviour, path }) = (&meta.validate, value)
            && let [Step::Key(field)] = path.as_slice()
        {
            let key = (behaviour.clone(), field.clone());
            let error = self.answers.errors.get(&key);
            let (frame, text) = info_box(ui, container, InfoKind::Error, error.map_or("", String::as_str));
            ui.set_visible(frame, error.is_some());
            self.validation_boxes.push((key, frame, text));
        }
        if let Some(reference) = &meta.excerpt {
            let key = value.lock_key();
            let fold = ui.foldout(container, reference, self.open_excerpts.contains(&key));
            ui.add_class(fold, "excerpt");
            ui.set_foldout_icon(fold, "book");
            self.excerpts.push(ExcerptFold { fold, reference: reference.clone(), shown: None, key });
        }
    }

    /// The VoxDocs page whose book button was clicked, as `[Documentation]`
    /// names it.
    /// What the Prefab row's buttons asked for this frame.
    pub fn prefab_clicked(&self, ui: &Ui) -> Option<PrefabAction> {
        let buttons = self.prefab_buttons.as_ref()?;
        if ui.clicked(buttons.open) {
            return Some(PrefabAction::Open(buttons.path.clone()));
        }
        ui.clicked(buttons.select).then(|| PrefabAction::Select(buttons.path.clone()))
    }

    pub fn clicked_documentation(&self, ui: &Ui) -> Option<&str> {
        self.doc_buttons.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, page)| page.as_str())
    }

    /// The `[Excerpt]` folds shown, for the editor to fill from VoxDocs.
    pub fn excerpts_mut(&mut self) -> &mut [ExcerptFold] {
        &mut self.excerpts
    }

    /// Keeps which tab each group of tabs shows and which foldouts are
    /// closed, for when the sections are built again.
    fn remember_groups(&mut self, ui: &Ui) {
        for excerpt in &self.excerpts {
            if ui.is_open(excerpt.fold) {
                self.open_excerpts.insert(excerpt.key.clone());
            } else {
                self.open_excerpts.remove(&excerpt.key);
            }
        }
        for (name, tabs) in &self.group_tabs {
            self.tab_choices.insert(name.clone(), ui.selected_tab(*tabs));
        }
        for (name, foldout) in &self.group_foldouts {
            if ui.is_open(*foldout) {
                self.closed_groups.remove(name);
            } else {
                self.closed_groups.insert(name.clone());
            }
        }
    }

    /// The `[Tooltip]` of the field whose label the pointer is on, and the
    /// label.
    pub fn hovered_tooltip(&self, ui: &Ui) -> Option<(String, NodeId)> {
        let lods = self.mesh_lods.as_ref().and_then(|lods| lods.hovered_tooltip(ui));
        lods.or_else(|| {
            self.rows.iter().find_map(|row| {
                let tooltip = row.tooltip.as_ref()?;
                let label = *ui.children(row.row).first()?;
                ui.is_hovered(label).then(|| (tooltip.clone(), label))
            })
        })
    }

    /// The `[PreviewField]` images shown, with the field each previews and
    /// its size in logical pixels.
    pub fn field_previews(&self) -> &[(Prop, NodeId, u32)] {
        &self.previews
    }

    /// The asset fields shown, each with the image before its name that
    /// shows a preview of what it points at.
    pub fn field_thumbs(&self, ui: &Ui) -> Vec<(Prop, NodeId)> {
        let thumb = |picker: NodeId| ui.children(picker).into_iter().find(|&n| ui.has_class(n, "field-thumb"));
        let pickers = self.rows.iter().filter_map(|row| match row.control {
            Control::Picker(picker, ..) => Some((row.prop.clone(), thumb(picker)?)),
            _ => None,
        });
        pickers.collect()
    }

    /// The fields whose drawers are to be opened, once after each build.
    pub fn drawers_to_open(&self) -> Option<Vec<ToOpen>> {
        self.drawers.to_open()
    }

    /// Builds the drawers the scripts opened, from their markup and the
    /// project's sheets it names, each field's own control inside them.
    pub fn place_drawers(&mut self, ui: &mut Ui, game: &mut Game, answers: Vec<Result<Vec<String>, String>>) {
        let content = &mut game.content;
        let mut load = |src: &str| content.read_text(src).map_err(|error| format!("{error:#}"));
        let placed = self.drawers.place(ui, answers, &mut load);
        let Some(entity) = self.shown else { return };
        for (prop, index, next, editable) in placed {
            let Some(index) = index else {
                // A behaviour's inspector: its usual fields go into its "next", or out of sight without one.
                let Prop::Field { behaviour, .. } = &prop else { continue };
                let Some(&(_, fields)) = self.inspected.iter().find(|(name, _)| name == behaviour) else { continue };
                match next {
                    Some(next) => ui.move_into(fields, next),
                    None => ui.set_visible(fields, false),
                }
                continue;
            };
            let Some(next) = next else {
                self.rows[index].control = Control::Drawn;
                continue;
            };
            let meta = attributes::of(&self.types, &prop).clone();
            let shown = props::get(game, &self.types, entity, &prop).unwrap_or(Value::Fixed(String::new()));
            let offered = offered(&self.answers, &prop, &meta);
            self.rows[index].control = control(ui, next, &shown, editable, (&meta, offered));
        }
    }

    /// The drawn fields to draw this frame, each with its value now.
    pub fn drawer_requests(&mut self, ui: &Ui, game: &Game) -> Vec<DrawRequest> {
        let Some(entity) = self.shown else { return Vec::new() };
        let (world, types) = (&game.engine.world, &self.types);
        self.drawers.requests(ui, |prop| match prop {
            Prop::Field { behaviour, path } if path.is_empty() => fields::scene_fields(world, entity, behaviour),
            Prop::Field { behaviour, path } => fields::value_at(world, types, entity, behaviour, path),
            _ => None,
        })
    }

    /// The UI as the drawers' code reaches it while they draw.
    pub fn drawer_ui<'a>(&'a mut self, ui: &'a mut Ui) -> DrawerUi<'a> {
        self.drawers.services(ui)
    }

    /// Writes what the drawers changed into the scene, and calls the
    /// fields' `[OnValueChanged]` methods as for any change. Returns what the
    /// drawers that threw said.
    pub fn take_drawn(&mut self, game: &mut Game, answers: Vec<Result<Drawn, String>>) -> Vec<String> {
        let Some(entity) = self.shown else { return Vec::new() };
        let (changed, errors) = self.drawers.take(answers, &self.types);
        for (prop, value) in changed {
            let Prop::Field { behaviour, path } = &prop else { continue };
            if fields::set_raw_at(&mut game.engine.world, &self.types, entity, behaviour, path, value)
                && let Some(method) = &attributes::top(&self.types, &prop).on_changed
            {
                self.callbacks.push((behaviour.clone(), method.clone()));
            }
        }
        errors
    }

    pub fn types(&self) -> &ScriptTypes {
        &self.types
    }

    /// Shows `entity`, or a hint when nothing is selected. While `playing`,
    /// scripts show the values they started with, which cannot be edited.
    /// Shows what is selected in the Project panel instead of an Obj.
    pub fn show_asset(&mut self, ui: &mut Ui, game: &Game, item: &ProjectItem) {
        self.shown = None;
        (self.fields, self.transform_menu, self.add, self.menu) = (None, None, None, None);
        self.headers.clear();
        self.rows.clear();
        self.asset = Some(AssetView::build(ui, self.root, item, game, &self.types, self.dark));
    }

    /// What the asset shown asks of the editor this frame.
    pub fn update_asset(&mut self, ui: &mut Ui) -> Option<AssetAction> {
        self.asset.as_mut()?.update(ui)
    }

    /// The asset view, when the Inspector shows an item of the Project panel.
    pub fn asset_view(&mut self) -> Option<&mut AssetView> {
        self.asset.as_mut()
    }

    /// The Project panel's item shown, if that is what it shows.
    pub fn shown_asset(&self) -> Option<&ProjectItem> {
        self.asset.as_ref().map(AssetView::item)
    }

    /// The asset shown's large preview: its path, the frame that takes drags
    /// to turn it, and the image.
    pub fn asset_preview(&self) -> Option<(String, NodeId, NodeId)> {
        let view = self.asset.as_ref()?;
        let (frame, image) = view.preview()?;
        Some((view.item().path.clone(), frame, image))
    }

    pub fn show(&mut self, ui: &mut Ui, game: &Game, entity: Option<Entity>, playing: bool) {
        self.asset = None;
        for child in ui.children(self.root) {
            ui.remove(child);
        }
        let world = &game.engine.world;
        let before = self.shown;
        self.shown = entity.filter(|&e| world.get::<Transform>(e).is_some());
        if self.shown != before {
            self.lod_level = 0;
        }
        (self.fields, self.transform_menu, self.add, self.menu) = (None, None, None, None);
        self.headers.clear();
        self.rows.clear();
        self.previews.clear();
        self.doc_buttons.clear();
        self.excerpts.clear();
        self.buttons.clear();
        self.validation_boxes.clear();
        self.pagers.clear();
        self.palette_swatches.clear();
        self.collections.clear();
        self.prefab_buttons = None;
        self.drawers.clear();
        self.inspected.clear();
        self.group_tabs.clear();
        self.group_foldouts.clear();
        (self.mesh_lods, self.group_lods) = (None, None);
        let Some(entity) = self.shown else {
            let hint = ui.add_label(self.root, "Select an Obj in the Hierarchy to see it here.");
            ui.add_class(hint, "placeholder");
            return;
        };
        let (name_row, name, persistent, layer, prefab_buttons) =
            header(ui, self.root, game, entity, self.also.len() + 1);
        self.prefab_buttons = prefab_buttons;

        let (transform, rows, menu) = section(ui, self.root, "Transform", "transform", false);
        let (position_row, position) = vector_row(ui, rows, "Position");
        let (rotation_row, rotation) = vector_row(ui, rows, "Rotation");
        let (scale_row, scale) = vector_row(ui, rows, "Scale");
        let rows = [
            (Field::Name, name_row),
            (Field::Position, position_row),
            (Field::Rotation, rotation_row),
            (Field::Scale, scale_row),
        ];
        self.fields = Some(Fields { name, persistent, layer, position, rotation, scale, rows });
        self.transform_menu = menu;
        self.headers.push((None, transform));

        let layout = self.layout_for(game, entity);
        let locks = world.get::<Locks>(entity).cloned();
        for (part, values) in &layout {
            // A C# component is shown as a box of data, apart from behaviours' code.
            let icon = match part {
                Part::Behaviour(name) if self.types.is_component(name) => "box",
                _ => part.icon(),
            };
            let (header, rows, _) = section(ui, self.root, part.title(), icon, true);
            self.headers.push((Some(part.clone()), header));
            // A behaviour its inspector draws gets a slot for it, its usual fields built in a panel after,
            // which goes into the inspector's "next" once it is built. While playing, it shows its fields.
            let inspector = match part {
                Part::Behaviour(name) if !playing => self.types.behaviour(name).and_then(|b| b.inspector.clone()),
                _ => None,
            };
            let rows = match (part, inspector) {
                (Part::Behaviour(name), Some(inspector)) => {
                    let slot = ui.panel(rows, Style::column());
                    ui.add_class(slot, "behaviour-inspector-slot");
                    let whole = Prop::Field { behaviour: name.clone(), path: Vec::new() };
                    self.drawers.add(&whole, None, slot, &[inspector], part.title(), true);
                    let fields = ui.panel(rows, Style::column());
                    ui.add_class(fields, "behaviour-fields");
                    self.inspected.push((name.clone(), fields));
                    fields
                }
                _ => rows,
            };
            let documentation = match part {
                Part::Behaviour(name) => self.types.behaviour(name).and_then(|b| b.documentation.clone()),
                _ => None,
            };
            if let Some(page) = documentation {
                let book = ui.icon_button(header, "book");
                ui.add_class(book, "docs-link");
                self.doc_buttons.push((book, page));
            }
            // A locked component shows designers only its chosen values; in its
            // prefab, every value shows, those designers do not see dimmed.
            let shown_to_designers = locks.as_ref().and_then(|l| l.0.get(&part.lock_key())).cloned();
            if shown_to_designers.is_some() {
                ui.add_class(header, "locked");
                let mark = ui.icon(header, "lock");
                ui.add_class(mark, "lock-mark");
            }
            let designers_see =
                |value: &Prop| shown_to_designers.as_ref().is_none_or(|s| s.contains(&value.lock_key()));
            if values.is_empty()
                && let Part::Behaviour(name) = part
            {
                let row = prop(ui, rows, "Script");
                let label = ui.add_label(row, name);
                ui.add_class(label, "value");
            }
            // A LOD Group's bar of its levels comes first, as in Unity, and only the selected level's settings show.
            if *part == Part::LodGroup
                && let Some(group) = world.get::<LodGroup>(entity)
            {
                self.lod_level = self.lod_level.min(group.levels.len().saturating_sub(1));
                self.group_lods = Some(GroupLods::new(ui, rows, group, self.lod_level));
            }
            let values = attributes::ordered(&self.types, values);
            let mut groups = attributes::Groups::new(rows);
            // Each list's collection field, and each item's body, which what is in the item goes in.
            let mut boxes: Vec<ListBox> = Vec::new();
            let mut bodies: Vec<(Prop, NodeId)> = Vec::new();
            for value in &values {
                if !self.prefab_mode && !designers_see(value) {
                    continue;
                }
                if let Prop::LodObj(level, _) | Prop::LodAdd(level) | Prop::LodUntil(level) = value
                    && *level != self.lod_level
                {
                    continue;
                }
                // While playing, a drawn value shows as it would without its drawers.
                if !attributes::shows(game, &self.types, entity, value)
                    || attributes::drawn_by_list(&self.types, value, &self.pages)
                    || (!playing && attributes::drawn_by_drawer(&self.types, value))
                {
                    continue;
                }
                let mut meta = attributes::of(&self.types, value).clone();
                let shown = props::get(game, &self.types, entity, value).unwrap_or(Value::Fixed(String::new()));
                let enabled = attributes::enabled(game, &self.types, entity, value);
                // While playing, a C# component's values are read and written in the running
                // game, and a running behaviour's on its instance, as the editor keeps them.
                let world = &game.engine.world;
                let live = matches!(value, Prop::Field { behaviour, path }
                    if crate::live::is_live(world, &self.types, entity, behaviour, path)
                        || crate::live::is_running(world, entity, behaviour));
                let editable = !(playing && matches!(value, Prop::Field { .. }) && !live) && enabled;
                // An item goes in its list's collection field, and what is in an item in the item's body,
                // indented from it; anything else in its group.
                let mut remove = None;
                let (container, from_depth) = match boxes.iter_mut().find(|list| list.holds(value)) {
                    Some(list) => {
                        let item = list.add(ui, value, editable);
                        remove = item.remove;
                        bodies.push((value.clone(), item.body));
                        (item.body, props::depth(value))
                    }
                    None => match bodies.iter().rev().find(|(item, _)| props::within(value, item)) {
                        Some((item, body)) => (*body, props::depth(item)),
                        None => (groups.container(ui, attributes::top(&self.types, value).group.as_ref()), 0),
                    },
                };
                let mut label = meta.label.clone().unwrap_or_else(|| props::label(value));
                if meta.suffix.is_none() {
                    meta.suffix = props::unit_on(world, entity, value).map(str::to_string);
                }
                // A [ListDrawerSettings] list labels its items by index, or not at all.
                if let Some((Some(settings), index)) = attributes::item_of(&self.types, value) {
                    label = index.to_string();
                    meta.hide_label |= !settings.index_labels;
                }
                self.decorate(ui, container, game, (entity, value), (&meta, &label));
                let row = prop(ui, container, &label);
                ui.set_class(row, "hidden-from-designers", !designers_see(value));
                let label_node = ui.children(row)[0];
                let indent = props::depth(value) - from_depth + meta.indent as usize;
                if indent > 0 {
                    ui.set_inline_style(label_node, &format!("padding-left: {}px", indent * 12));
                }
                if meta.hide_label {
                    ui.add_class(row, "no-label");
                    ui.set_visible(label_node, false);
                }
                ui.set_class(row, "disabled", !enabled && !meta.read_only);
                // A list is a collection field under its row, but for a [TableList], whose add button is
                // beside its value, as an item of a list outside a collection field has its remove button.
                let table = matches!(value, Prop::Field { behaviour, path }
                    if matches!(shown, Value::List(_)) && attributes::is_table(&self.types, behaviour, path));
                let item = editable && props::is_item(value) && remove.is_none();
                let list = editable && table;
                let drawers = attributes::drawers(&self.types, value);
                let drawn = !playing && !drawers.is_empty();
                let holder = if item || list || drawn || meta.suffix.is_some() || meta.documentation.is_some() {
                    let holder = ui.panel(row, Style::row());
                    ui.add_class(holder, "prop-value");
                    holder
                } else {
                    row
                };
                // A drawn field's own control waits in a slot, where its drawers are built once the scripts start.
                let slot = drawn.then(|| {
                    let slot = ui.panel(holder, Style::row());
                    ui.add_class(slot, "field-drawer-slot");
                    self.drawers.add(value, Some(self.rows.len()), slot, drawers, &label, editable);
                    slot
                });
                let offered = offered(&self.answers, value, &meta);
                let control = control(ui, slot.unwrap_or(holder), &shown, editable, (&meta, offered));
                if let Some(suffix) = &meta.suffix {
                    let unit = ui.add_label(holder, suffix);
                    ui.add_class(unit, "attr-suffix");
                }
                let button = |ui: &mut Ui, icon: &str, class: &str| {
                    let button = ui.icon_button(holder, icon);
                    ui.add_class(button, class);
                    button
                };
                let mut add = list.then(|| button(ui, "plus", "list-add"));
                let remove = remove.or_else(|| item.then(|| button(ui, "close", "list-remove")));
                if let Some(page) = &meta.documentation {
                    self.doc_buttons.push((button(ui, "book", "docs-link"), page.clone()));
                }
                if let (Some(size), Value::Asset(_)) = (meta.preview, &shown) {
                    let frame = ui.panel(container, Style::row());
                    ui.add_class(frame, "attr-preview");
                    let image = ui.image(frame);
                    ui.set_inline_style(image, &format!("width: {size}px; height: {size}px"));
                    self.previews.push((value.clone(), image, size));
                }
                let limits = limits(&meta);
                let tooltip = meta.tooltip.clone();
                let paged = match (&shown, value) {
                    (Value::List(count), Prop::Field { behaviour, path }) => {
                        let per_page = meta.list.map_or(0, |settings| settings.per_page);
                        let page = attributes::page(&self.pages, behaviour, path);
                        Some((*count, per_page, page, page_range(*count, per_page, page)))
                    }
                    _ => None,
                };
                if let Some((.., shown_items)) = paged.clone().filter(|_| !table) {
                    let collection = collection::collection(ui, container, shown_items.len(), editable, None);
                    add = collection.add;
                    boxes.push(ListBox { list: value.clone(), collection, editable, shown: Vec::new() });
                }
                self.rows.push(PropRow { prop: value.clone(), row, control, add, remove, tooltip, limits });
                if let (Some((count, per_page, page, shown_items)), Prop::Field { behaviour, path }) = (paged, value) {
                    if table {
                        self.table(ui, container, game, (entity, behaviour, path), shown_items, editable);
                    }
                    if per_page > 0 && count > per_page {
                        self.pager(
                            ui,
                            container,
                            attributes::page_key(behaviour, path),
                            page,
                            count.div_ceil(per_page),
                        );
                    }
                }
                if let (Value::Color(_), false) = (&shown, meta.palette.is_empty() || !editable) {
                    self.palette(ui, container, value, &meta.palette);
                }
            }
            self.collections.extend(boxes.into_iter().map(|list| (list.list, list.collection.items, list.shown)));
            if let Part::Behaviour(name) = part {
                let waits = playing && !crate::live::is_running(&game.engine.world, entity, name);
                self.add_buttons(ui, rows, name, waits);
            }
            if *part == Part::Renderer {
                self.mesh_lods = Some(MeshLods::new(ui, rows));
            }
            let prefix = part.lock_key();
            for (name, tabs) in groups.tabs {
                let name = format!("{prefix}/{name}");
                if let Some(&tab) = self.tab_choices.get(&name) {
                    ui.set_selected_tab(tabs, tab);
                }
                self.group_tabs.push((name, tabs));
            }
            for (name, foldout) in groups.foldouts {
                let name = format!("{prefix}/{name}");
                if self.closed_groups.contains(&name) {
                    ui.set_open(foldout, false);
                }
                self.group_foldouts.push((name, foldout));
            }
        }
        let add = ui.button(self.root, "Add Component");
        ui.add_class(add, "add-component");
        self.add = Some(add);
        self.conditions = attributes::key(game, &self.types, entity, &layout);
        self.built = (layout, playing, locks);
        self.mark_overrides(ui);
    }

    /// Marks the rows and sections whose values the Obj shown changes from
    /// its prefab, as Unity does: a bar beside each, and its label bold.
    pub fn show_overrides(&mut self, ui: &mut Ui, overrides: Vec<ValuePath>) {
        if overrides != self.overrides {
            self.overrides = overrides;
            self.mark_overrides(ui);
        }
    }

    fn mark_overrides(&self, ui: &mut Ui) {
        let changed = |under: &[ValuePath]| self.overrides.iter().any(|o| under.iter().any(|u| u.overlaps(o)));
        let rows = self.fields.iter().flat_map(|fields| fields.rows.iter().cloned());
        let rows = rows.chain(self.rows.iter().map(|row| (Field::Prop(row.prop.clone()), row.row)));
        for (field, row) in rows {
            ui.set_class(row, "overridden", changed(&[value_path(&field)]));
        }
        for (part, header) in &self.headers {
            ui.set_class(*header, "overridden", changed(&part_paths(part.as_ref())));
        }
    }

    /// Apply and Revert for the overrides under `under`, when there are any.
    fn override_items(&self, under: &[ValuePath], playing: bool) -> Vec<(MenuItem, Option<RowAction>)> {
        let paths: Vec<ValuePath> =
            self.overrides.iter().filter(|o| under.iter().any(|u| u.overlaps(o))).cloned().collect();
        let Some(buttons) = self.prefab_buttons.as_ref().filter(|_| !paths.is_empty() && !playing) else {
            return Vec::new();
        };
        let name = buttons.path.rsplit('/').next().unwrap_or(&buttons.path);
        let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        vec![
            (MenuItem::new(&format!("Apply to Prefab '{name}'")), Some(RowAction::ApplyOverrides(paths.clone()))),
            (MenuItem::new("Revert"), Some(RowAction::RevertOverrides(paths))),
        ]
    }

    /// A `[TableList]`: a column for each field of the list's values and a
    /// row for each item shown, each cell its field's control, with a button
    /// to take the item out.
    fn table(
        &mut self,
        ui: &mut Ui,
        container: NodeId,
        game: &Game,
        (entity, behaviour, list): (Entity, &str, &[Step]),
        items: std::ops::Range<usize>,
        editable: bool,
    ) {
        let Some(voxear_engine::script_types::FieldKind::List { element }) =
            crate::fields::kind_at(&self.types, behaviour, list)
        else {
            return;
        };
        let voxear_engine::script_types::FieldKind::Struct { name } = *element else { return };
        let columns: Vec<_> = self.types.fields_of(&name).iter().filter(|f| !f.inspector.hidden).cloned().collect();
        let table = ui.panel(container, Style::default());
        ui.add_class(table, "attr-table");
        ui.set_inline_style(table, &format!("grid-template-columns: repeat({}, minmax(0px, 1fr)) 22px", columns.len()));
        for column in &columns {
            let title = column.inspector.label.clone().unwrap_or_else(|| props::nice_name(&column.name));
            let head = ui.add_label(table, &title);
            ui.add_class(head, "attr-table-head");
        }
        ui.panel(table, Style::default());
        for item in items {
            let mut item_path = list.to_vec();
            item_path.push(Step::Item(item));
            for column in &columns {
                let mut path = item_path.clone();
                path.push(Step::Key(column.name.clone()));
                let prop = Prop::Field { behaviour: behaviour.to_string(), path };
                let cell = ui.panel(table, Style::row());
                ui.add_class(cell, "attr-table-cell");
                let shown = props::get(game, &self.types, entity, &prop).unwrap_or(Value::Fixed(String::new()));
                let control = control(ui, cell, &shown, editable, (&column.inspector, None));
                let (limits, tooltip) = (limits(&column.inspector), column.inspector.tooltip.clone());
                self.rows.push(PropRow { prop, row: cell, control, add: None, remove: None, tooltip, limits });
            }
            let cell = ui.panel(table, Style::row());
            let remove = editable.then(|| {
                let button = ui.icon_button(cell, "close");
                ui.add_class(button, "list-remove");
                button
            });
            let prop = Prop::Field { behaviour: behaviour.to_string(), path: item_path };
            let mark = ui.add_label(cell, "");
            let control = Control::Fixed(mark);
            self.rows.push(PropRow {
                prop,
                row: cell,
                control,
                add: None,
                remove,
                tooltip: None,
                limits: (None, None),
            });
        }
    }

    /// A `[ListDrawerSettings]` list's page buttons: back, which page of how
    /// many, and on.
    fn pager(&mut self, ui: &mut Ui, container: NodeId, key: String, page: usize, pages: usize) {
        let bar = ui.panel(container, Style::row());
        ui.add_class(bar, "attr-pager");
        let back = ui.icon_button(bar, "chevron-left");
        let shown = ui.add_label(bar, &format!("Page {} of {pages}", page + 1));
        ui.add_class(shown, "attr-pager-text");
        let on = ui.icon_button(bar, "chevron-right");
        ui.set_class(back, "disabled", page == 0);
        ui.set_class(on, "disabled", page + 1 >= pages);
        self.pagers.push((back, on, key, pages));
    }

    /// A `[ColorPalette]`'s swatches under its colour; a click picks one.
    fn palette(&mut self, ui: &mut Ui, container: NodeId, prop: &Prop, colours: &[String]) {
        let strip = ui.panel(container, Style::row());
        ui.add_class(strip, "attr-palette");
        for hex in colours {
            let Some(rgb) = parse_hex(hex) else { continue };
            let swatch = ui.panel(strip, Style::default());
            ui.add_class(swatch, "attr-palette-swatch");
            let [r, g, b] = rgb.map(|c| (c * 255.0).round() as u8);
            ui.set_inline_style(swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            self.palette_swatches.push((swatch, prop.clone(), rgb));
        }
    }

    /// Moves a list's item dragged by its grip to where it was let go. The
    /// rows stay, each showing what is now its item, so the items ease into
    /// place; the change is a step to undo like any other.
    fn update_reorders(&mut self, ui: &Ui, game: &mut Game, entity: Entity) {
        for (list, items, shown) in &self.collections {
            let (Some((from, to)), Prop::Field { behaviour, path }) = (collection::moved(ui, *items), list) else {
                continue;
            };
            let (Some(&from), Some(&to)) = (shown.get(from), shown.get(to)) else { continue };
            let item = |index: usize| {
                let mut path = path.clone();
                path.push(Step::Item(index));
                Prop::Field { behaviour: behaviour.clone(), path }
            };
            props::move_item(&mut game.engine.world, &self.types, entity, &item(from), &item(to));
        }
    }

    /// Turns a page or picks a palette's colour, as clicked this frame.
    /// True when a page turned, which builds the sections again.
    fn update_lists(&mut self, ui: &Ui, game: &mut Game, entity: Entity, renderer: Option<&mut Renderer>) -> bool {
        let mut turned = false;
        for (back, on, key, pages) in &self.pagers {
            let page = self.pages.get(key).copied().unwrap_or_default();
            let next = if ui.clicked(*back) {
                page.saturating_sub(1)
            } else if ui.clicked(*on) {
                (page + 1).min(pages - 1)
            } else {
                continue;
            };
            self.pages.insert(key.clone(), next);
            turned |= next != page;
        }
        let picked = self.palette_swatches.iter().find(|(swatch, ..)| ui.clicked(*swatch));
        if let Some((_, prop, rgb)) = picked.cloned()
            && props::set(game, renderer, &self.types, entity, &prop, Value::Color(rgb))
            && let (Prop::Field { behaviour, .. }, Some(method)) =
                (&prop, &attributes::top(&self.types, &prop).on_changed)
        {
            self.callbacks.push((behaviour.clone(), method.clone()));
        }
        turned
    }

    /// A behaviour's `[Button]`s, under its fields. They run in edit mode on
    /// the scene's values, and while playing on the running behaviour; they
    /// wait when it `waits`, as for one not running.
    fn add_buttons(&mut self, ui: &mut Ui, rows: NodeId, behaviour: &str, waits: bool) {
        let Some(described) = self.types.behaviour(behaviour) else { return };
        if described.buttons.is_empty() {
            return;
        }
        let bar = ui.panel(rows, Style::column());
        ui.add_class(bar, "attr-buttons-column");
        for button in &described.buttons {
            let label = if button.label.is_empty() { props::nice_name(&button.method) } else { button.label.clone() };
            let node = ui.button(bar, &label);
            ui.add_class(node, "attr-button");
            ui.set_class(node, "disabled", waits);
            if !waits {
                self.buttons.push((node, behaviour.to_string(), button.method.clone()));
            }
        }
    }

    /// The sections shown for `entity`: its own, and with other Objs
    /// selected, only those every one of them has too, as Unity's Inspector
    /// shows only the components all of them share.
    fn layout_for(&self, game: &Game, entity: Entity) -> Vec<(Part, Vec<Prop>)> {
        let mut parts = layout(game, entity, &self.types);
        let world = &game.engine.world;
        parts.retain(|(part, _)| self.also.iter().all(|&other| props::parts(world, other).contains(part)));
        parts
    }

    /// The camera the Mesh Renderer and the LOD Group say what is drawn
    /// through, if there is one to say it of.
    /// The LOD bar's edge dragged since the last call, for the Scene view to
    /// follow.
    pub fn take_lod_edge(&mut self) -> Option<LodEdge> {
        self.lod_edge.take()
    }

    pub fn set_lod_eye(&mut self, eye: Option<LodEye>) {
        self.lod_eye = eye;
    }

    /// Makes edits here go to `others` too, the other Objs selected with the
    /// one shown, and says how many Objs it edits.
    pub fn set_also(&mut self, ui: &mut Ui, game: &Game, others: Vec<Entity>, playing: bool) {
        if others != self.also {
            self.also = others;
            self.show(ui, game, self.shown, playing);
        }
    }

    /// Applies what was typed or picked, and shows changes made elsewhere.
    /// Returns what was edited this frame, for undo.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        game: &mut Game,
        mut renderer: Option<&mut Renderer>,
        playing: bool,
    ) -> Option<Edit> {
        let entity = self.shown?;
        let Some(&transform) = game.engine.world.get::<Transform>(entity) else {
            self.show(ui, game, None, playing);
            return None;
        };
        let locks = game.engine.world.get::<Locks>(entity).cloned();
        self.remember_groups(ui);
        let layout = self.layout_for(game, entity);
        let conditions = attributes::key(game, &self.types, entity, &layout);
        // A LOD Group measured another way labels where its levels end another way.
        let measure = game.engine.world.get::<LodGroup>(entity).map(|group| group.measure);
        let remeasured = self.group_lods.as_ref().is_some_and(|lods| Some(lods.measure) != measure);
        if layout != self.built.0
            || playing != self.built.1
            || locks != self.built.2
            || conditions != self.conditions
            || remeasured
        {
            self.show(ui, game, Some(entity), playing);
            return None;
        }
        let before = ObjState::of(&game.engine.world, entity);
        let world = &game.engine.world;
        let others_before: Vec<(Entity, ObjState)> =
            self.also.iter().filter_map(|&other| Some((other, ObjState::of(world, other)?))).collect();
        if self.update_lists(ui, game, entity, renderer.as_deref_mut()) {
            self.show(ui, game, Some(entity), playing);
            let (before, after) = (before?, ObjState::of(&game.engine.world, entity)?);
            return (before != after).then_some(Edit::Changed { entity, before, after });
        }
        self.update_transform(ui, game, entity, transform);
        self.update_reorders(ui, game, entity);
        for row in &self.rows {
            // Adding or taking out an item changes the rows, which are built again next frame.
            if row.add.is_some_and(|b| ui.clicked(b)) {
                props::add_item(&mut game.engine.world, &self.types, entity, &row.prop);
            }
            if row.remove.is_some_and(|b| ui.clicked(b)) {
                props::remove_item(&mut game.engine.world, &self.types, entity, &row.prop);
            }
        }
        for row in &mut self.rows {
            let Some(current) = props::get(game, &self.types, entity, &row.prop) else { continue };
            if let Some(picked) = read_control(ui, &row.control, &current) {
                let picked = clamp(picked, row.limits);
                let changed = props::set(game, renderer.as_deref_mut(), &self.types, entity, &row.prop, picked.clone());
                // The same value goes to each other Obj selected that has the same thing; of a vector
                // or a colour, only the parts changed, so each keeps its own other parts.
                for &other in self.also.iter().filter(|_| changed) {
                    if let Some(theirs) = props::get(game, &self.types, other, &row.prop) {
                        let value = changed_parts(&current, &picked, theirs);
                        props::set(game, renderer.as_deref_mut(), &self.types, other, &row.prop, value);
                    }
                }
                if let (true, Prop::Field { behaviour, .. }) = (changed, &row.prop)
                    && let Some(method) = &attributes::top(&self.types, &row.prop).on_changed
                {
                    self.callbacks.push((behaviour.clone(), method.clone()));
                }
            }
            if let Some(now) = props::get(game, &self.types, entity, &row.prop) {
                refresh_control(ui, &mut row.control, &now);
                let theirs: Vec<Value> =
                    self.also.iter().filter_map(|&other| props::get(game, &self.types, other, &row.prop)).collect();
                show_mixed(ui, row.row, &row.control, &now, &theirs);
            }
        }
        let selected = self.update_lods(ui, game, entity, renderer.as_deref());
        if selected {
            self.show(ui, game, Some(entity), playing);
        }
        let (before, after) = (before?, ObjState::of(&game.engine.world, entity)?);
        let world = &game.engine.world;
        let mut edits: Vec<Edit> = (before != after).then_some(Edit::Changed { entity, before, after }).into_iter().collect();
        edits.extend(others_before.into_iter().filter_map(|(other, before)| {
            let after = ObjState::of(world, other)?;
            (before != after).then_some(Edit::Changed { entity: other, before, after })
        }));
        // Several Objs edited at once are one step to undo.
        match edits.len() {
            0 | 1 => edits.pop(),
            _ => Some(Edit::Group(edits)),
        }
    }

    /// The LOD Group's bar: a level clicked is selected, and a dragged edge
    /// moves where its level ends, on every Obj edited. Then shows which
    /// LOD the camera sees, of the mesh and of the group. Returns whether
    /// another level was selected, so its settings show.
    fn update_lods(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity, renderer: Option<&Renderer>) -> bool {
        let Some(lods) = &mut self.group_lods else { return self.update_mesh_lods(ui, game, entity, renderer) };
        if let Some(level) = lods.clicked_level(ui).filter(|&level| level != self.lod_level) {
            self.lod_level = level;
            return true;
        }
        let dragged = game.engine.world.get::<LodGroup>(entity).and_then(|group| lods.dragged(ui, group));
        if let Some((level, until)) = dragged {
            for edited in std::iter::once(entity).chain(self.also.iter().copied()) {
                props::set_lod_until(&mut game.engine.world, edited, level, until);
            }
            self.lod_edge = Some(LodEdge::Group(until));
        }
        let world = &game.engine.world;
        if let (Some(renderer), Some(group)) = (renderer, world.get::<LodGroup>(entity)) {
            let view = self.lod_eye.and_then(|eye| renderer.lod_choices(world, eye.camera).groups.get(&entity).copied());
            lods.update(ui, group, view);
        }
        self.update_mesh_lods(ui, game, entity, renderer)
    }

    /// A dragged edge of the mesh's LOD bar gives every Obj edited where its
    /// LODs end; then the bar shows them, and which the camera sees.
    fn update_mesh_lods(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity, renderer: Option<&Renderer>) -> bool {
        let Some(lods) = &mut self.mesh_lods else { return false };
        if let Some(ends) = lods.dragged(ui) {
            for edited in std::iter::once(entity).chain(self.also.iter().copied()) {
                props::set_lod_transitions(&mut game.engine.world, edited, Some(ends));
            }
            if let Some(level) = lods.dragging() {
                self.lod_edge = Some(LodEdge::Mesh(ends[level]));
            }
        }
        if let Some(renderer) = renderer {
            lods.update(ui, &game.engine.world, entity, renderer, self.lod_eye);
        }
        // A renderer that says where its LODs end has no use for a bias. Hidden rather than built
        // again, which would let go of an edge being dragged.
        let world = &game.engine.world;
        let custom = world.get::<voxear_render::MeshRenderer>(entity).is_some_and(|r| r.lod_transitions.is_some());
        if let Some(bias) = self.rows.iter().find(|row| row.prop == Prop::LodBias) {
            ui.set_visible(bias.row, !custom);
        }
        false
    }

    fn update_transform(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity, transform: Transform) {
        let Some(fields) = &self.fields else { return };
        let world = &mut game.engine.world;
        if ui.changed(fields.name) {
            let name = ui.text(fields.name);
            for target in std::iter::once(entity).chain(self.also.iter().copied()) {
                if let Some(current) = world.get_mut::<Name>(target) {
                    current.0 = name.clone();
                }
            }
        }
        if ui.changed(fields.persistent) {
            let persistent = ui.is_checked(fields.persistent);
            for target in std::iter::once(entity).chain(self.also.iter().copied()) {
                if persistent {
                    world.insert(target, ScenePersistence);
                } else {
                    world.remove::<ScenePersistence>(target);
                }
            }
        }
        ui.set_checked(fields.persistent, world.has::<ScenePersistence>(entity));
        // A layer picked goes on every Obj edited; Default is no Layer at all.
        let (dropdown, numbers) = &fields.layer;
        if ui.changed(*dropdown) {
            let layer = numbers.get(ui.selected_option(*dropdown)).copied().unwrap_or_default();
            for target in std::iter::once(entity).chain(self.also.iter().copied()) {
                match layer {
                    0 => drop(world.remove::<Layer>(target)),
                    _ => drop(world.insert(target, Layer(layer))),
                }
            }
        }
        let now = world.get::<Layer>(entity).map_or(0, |layer| layer.0);
        if let Some(at) = numbers.iter().position(|&number| number == now) {
            ui.set_selected_option(*dropdown, at);
        }
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        let euler = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
        let mut edited = transform;
        edited.position = read(ui, &fields.position, transform.position);
        let rotation = read(ui, &fields.rotation, euler);
        if rotation != euler {
            let r = rotation * (std::f32::consts::PI / 180.0);
            edited.rotation = Quat::from_euler(EulerRot::YXZ, r.y, r.x, r.z);
        }
        edited.scale = read(ui, &fields.scale, transform.scale);
        if edited != transform
            && let Some(current) = world.get_mut::<Transform>(entity)
        {
            *current = edited;
        }
        // The other Objs selected take the parts that were edited, each axis on its own, and keep the rest.
        let moved = |from: Vec3, to: Vec3| [from.x != to.x, from.y != to.y, from.z != to.z];
        let (position, turned, scale) = (moved(transform.position, edited.position), moved(euler, rotation), moved(transform.scale, edited.scale));
        for &other in self.also.iter().filter(|_| edited != transform) {
            let Some(current) = world.get_mut::<Transform>(other) else { continue };
            let (y, x, z) = current.rotation.to_euler(EulerRot::YXZ);
            let mut angles = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
            for axis in 0..3 {
                if position[axis] {
                    current.position[axis] = edited.position[axis];
                }
                if turned[axis] {
                    angles[axis] = rotation[axis];
                }
                if scale[axis] {
                    current.scale[axis] = edited.scale[axis];
                }
            }
            if turned.contains(&true) {
                let r = angles * (std::f32::consts::PI / 180.0);
                current.rotation = Quat::from_euler(EulerRot::YXZ, r.y, r.x, r.z);
            }
        }
        write(ui, &fields.position, edited.position);
        write(ui, &fields.rotation, if rotation != euler { rotation } else { euler });
        write(ui, &fields.scale, edited.scale);
        // An axis the other Objs selected hold otherwise shows "—".
        let theirs: Vec<Transform> = self.also.iter().filter_map(|&other| world.get::<Transform>(other).copied()).collect();
        let angles = |t: &Transform| {
            let (y, x, z) = t.rotation.to_euler(EulerRot::YXZ);
            Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI)
        };
        let shown_angles = angles(&edited);
        for (fields, mine, of) in [
            (&fields.position, edited.position, theirs.iter().map(|t| t.position).collect::<Vec<_>>()),
            (&fields.rotation, shown_angles, theirs.iter().map(angles).collect()),
            (&fields.scale, edited.scale, theirs.iter().map(|t| t.scale).collect()),
        ] {
            for (axis, &field) in fields.iter().enumerate() {
                let differs = of.iter().any(|other| (other[axis] - mine[axis]).abs() > 1e-4);
                if differs && ui.focused() != Some(field) {
                    ui.set_text(field, MIXED);
                }
            }
        }
    }

    /// Marks the rows whose values are `kept`, and opens and answers the
    /// Inspector's menus: a row's (while `playing`, whether to keep its
    /// value, offered when `can_keep` as the Obj is in the edited level),
    /// a section's, and Add Component's. Returns what was chosen.
    pub fn menus(
        &mut self,
        ui: &mut Ui,
        game: &Game,
        playing: bool,
        can_keep: bool,
        kept: &[Field],
    ) -> Option<RowAction> {
        let fields = self.fields.as_ref()?;
        let rows: Vec<(Field, NodeId)> =
            fields.rows.iter().cloned().chain(self.rows.iter().map(|r| (Field::Prop(r.prop.clone()), r.row))).collect();
        for (field, row) in &rows {
            ui.set_class(*row, "kept", kept.contains(field));
        }
        if let Some((owner, actions)) = &self.menu
            && let Some(index) = ui.menu_choice(*owner)
        {
            let action = actions.get(index).cloned().flatten();
            // A level inserted is selected; deleting one selects the one before.
            match action {
                Some(RowAction::InsertLodLevel(at)) => self.lod_level = at,
                Some(RowAction::DeleteLodLevel(at)) => self.lod_level = at.saturating_sub(1),
                _ => {}
            }
            return action;
        }
        let dropped = self.rows.iter().find_map(|row| match row.control {
            Control::Picker(picker, ..) => ui.dropped_onto(picker).map(|source| (row.prop.clone(), source)),
            _ => None,
        });
        if let Some((prop, source)) = dropped {
            return Some(RowAction::Dropped(prop, source));
        }
        if let Some((level, source)) = self.group_lods.as_ref().and_then(|lods| lods.dropped(ui)) {
            self.lod_level = level;
            return Some(RowAction::DropOnLodLevel(level, source));
        }
        if let Some(level) = self.mesh_lods.as_ref().and_then(|lods| lods.clicked(ui)) {
            return Some(RowAction::ViewLod(level));
        }
        let at = ui.pointer_position().unwrap_or_default();
        let mut items: Vec<(MenuItem, Option<RowAction>)>;
        if let Some(add) = self.add.filter(|&add| ui.clicked(add)) {
            return Some(RowAction::BrowseComponents(add));
        }
        let overrides_button = self.prefab_buttons.as_ref().and_then(|buttons| buttons.overrides);
        let (owner, at) = if let Some(button) = self.transform_menu.filter(|&b| ui.clicked(b)) {
            items = vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))];
            let [x, y, _, height] = ui.rect(button);
            (button, (x, y + height + 4.0))
        } else if let Some(button) = overrides_button.filter(|&b| ui.clicked(b)) {
            items = vec![
                (MenuItem::new("Revert All").enabled(!playing), Some(RowAction::RevertAll)),
                (MenuItem::new("Apply All").enabled(!playing), Some(RowAction::ApplyAll)),
            ];
            let [x, y, _, height] = ui.rect(button);
            (button, (x, y + height + 4.0))
        } else if let Some(action) = self.picker_clicked(ui) {
            return Some(action);
        } else if let Some((prop, swatch)) = self.swatch_clicked(ui) {
            return Some(RowAction::EditColor(prop, swatch));
        } else if let Some((level, cell, forced, custom)) =
            self.mesh_lods.as_ref().and_then(|lods| lods.context_clicked(ui))
        {
            items = vec![match forced == Some(level) {
                true => (MenuItem::new("Stop Forcing"), Some(RowAction::ForceLod(None))),
                false => (MenuItem::new(&format!("Force LOD {level}")), Some(RowAction::ForceLod(Some(level as u8)))),
            }];
            if forced.is_some_and(|forced| forced != level) {
                items.push((MenuItem::new("Stop Forcing"), Some(RowAction::ForceLod(None))));
            }
            if custom {
                items.push((MenuItem::new("Reset Transitions"), Some(RowAction::ResetLodTransitions)));
            }
            (cell, at)
        } else if let Some((level, clicked)) = self.group_lods.as_ref().and_then(|lods| lods.context_clicked(ui)) {
            items = lod_level_items(level, self.shown.and_then(|e| game.engine.world.get::<LodGroup>(e)));
            (clicked, at)
        } else if let Some((part, header)) = self.headers.iter().find(|(_, h)| ui.context_clicked(*h)) {
            items = match part {
                None => vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))],
                Some(part) => self.header_items(game, part, playing),
            };
            let changes = self.override_items(&part_paths(part.as_ref()), playing);
            if !changes.is_empty() && !items.is_empty() {
                items.push((MenuItem::separator(), None));
            }
            items.extend(changes);
            (*header, at)
        } else if let Some((field, row)) = rows.iter().find(|(_, row)| ui.context_clicked(*row)) {
            items = row_items(field, playing, can_keep, kept.contains(field));
            if let (true, Field::Prop(prop)) = (self.prefab_mode, field)
                && let Some(shown) = self.shown.and_then(|e| self.designer_fields(game, e, &prop.part()))
            {
                if !items.is_empty() {
                    items.push((MenuItem::separator(), None));
                }
                let seen = shown.contains(&prop.lock_key());
                let toggle = MenuItem::new("Show to Designers").checked(seen);
                items.push((toggle, Some(RowAction::ShowToDesigners(prop.clone(), !seen))));
            }
            let changes = self.override_items(&[value_path(field)], playing);
            if !changes.is_empty() && !items.is_empty() {
                items.push((MenuItem::separator(), None));
            }
            items.extend(changes);
            (*row, at)
        } else {
            return None;
        };
        if items.is_empty() {
            return None;
        }
        let (items, actions): (Vec<MenuItem>, Vec<Option<RowAction>>) = items.into_iter().unzip();
        ui.open_menu(owner, at, &items);
        self.menu = Some((owner, actions));
        None
    }

    /// The reference whose button was clicked, and the button.
    /// What a click on a reference asks: its arrow, to pick another; a double
    /// click, to select what it points at; or a click, to ping it.
    fn picker_clicked(&self, ui: &Ui) -> Option<RowAction> {
        self.rows.iter().find_map(|row| match row.control {
            Control::Picker(picker, _, arrow) if ui.clicked(arrow) => Some(RowAction::Browse(row.prop.clone(), picker)),
            Control::Picker(picker, ..) if ui.double_clicked(picker) => Some(RowAction::SelectTarget(row.prop.clone())),
            Control::Picker(picker, ..) if ui.clicked(picker) => Some(RowAction::Ping(row.prop.clone())),
            _ => None,
        })
    }

    /// The colour whose swatch was clicked, and the swatch.
    fn swatch_clicked(&self, ui: &Ui) -> Option<(Prop, NodeId)> {
        self.rows.iter().find_map(|row| match row.control {
            Control::Axes(_, Some(swatch)) if ui.clicked(swatch) => Some((row.prop.clone(), swatch)),
            _ => None,
        })
    }

    /// What Add Component offers, as the picker lists it in folders, as
    /// Unity's does: the built-in components the Obj has not got, in
    /// Rendering, Audio, Physics and Physics 2D, then in edit mode the project's scripts not on
    /// it, where their `[AddComponentMenu]` puts them or else under Scripts,
    /// by namespace past the one every script shares.
    pub fn component_choices(&self, playing: bool) -> Vec<Choice> {
        let has = |part: &Part| self.built.0.iter().any(|(p, _)| p == part);
        let choice = |part: Part, label: &str, folder: &str, icon: &'static str| Choice {
            label: label.to_string(),
            detail: String::new(),
            icon,
            pick: Chosen::Component(part),
            preview: None,
            folder: folder.to_string(),
        };
        let built_in = [
            (Part::Renderer, "Rendering"),
            (Part::Light, "Rendering"),
            (Part::Camera, "Rendering"),
            (Part::LodGroup, "Rendering"),
            (Part::Audio, "Audio"),
            (Part::Listener, "Audio"),
            (Part::Collider, "Physics"),
            (Part::Rigidbody, "Physics"),
            (Part::Character, "Physics"),
            (Part::Joint, "Physics"),
            (Part::Collider2D, "Physics 2D"),
            (Part::Rigidbody2D, "Physics 2D"),
        ];
        let mut choices: Vec<Choice> = built_in
            .into_iter()
            .filter(|(part, _)| !has(part))
            .map(|(part, folder)| choice(part.clone(), part.title(), folder, part.icon()))
            .collect();
        if playing {
            return choices;
        }
        let shared = shared_namespace(self.types.behaviours.iter().chain(&self.types.components));
        for (scripts, icon) in [(&self.types.behaviours, "code"), (&self.types.components, "box")] {
            for script in scripts.iter().filter(|script| !has(&Part::Behaviour(script.name.clone()))) {
                if let Some((folder, label)) = menu_place(script, &shared) {
                    choices.push(choice(Part::Behaviour(script.name.clone()), &label, &folder, icon));
                }
            }
        }
        choices
    }
}

/// The namespace every script is in, as `Hello`, which Add Component's
/// folders leave out; empty when they differ.
fn shared_namespace<'a>(scripts: impl Iterator<Item = &'a BehaviourType>) -> Vec<String> {
    let mut shared: Option<Vec<String>> = None;
    for script in scripts {
        let mut namespace: Vec<String> = script.name.split('.').map(str::to_string).collect();
        namespace.pop();
        shared = Some(match shared {
            None => namespace,
            Some(so_far) => so_far.into_iter().zip(namespace).take_while(|(a, b)| a == b).map(|(a, _)| a).collect(),
        });
    }
    shared.unwrap_or_default()
}

/// Where Add Component lists a script, as its folder and name: where its
/// `[AddComponentMenu]` puts it, or else under Scripts in a folder for each
/// part of its namespace past the `shared` one. `None` leaves it out, as an
/// empty menu path asks.
fn menu_place(script: &BehaviourType, shared: &[String]) -> Option<(String, String)> {
    if let Some(path) = &script.menu {
        let path = path.trim().trim_matches('/');
        return match path.rsplit_once('/') {
            _ if path.is_empty() => None,
            Some((folder, name)) => Some((folder.to_string(), name.to_string())),
            None => Some((String::new(), path.to_string())),
        };
    }
    let parts: Vec<&str> = script.name.split('.').collect();
    let namespace = &parts[..parts.len().saturating_sub(1)];
    let inner = namespace.iter().skip(shared.len()).copied().collect::<Vec<_>>();
    let folder = std::iter::once("Scripts").chain(inner).collect::<Vec<_>>().join("/");
    Some((folder, script.short_name().to_string()))
}

/// A LOD Group bar's menu, as Unity's: on a level, Insert Before and
/// Delete; past the last, Insert, which adds one at the end.
fn lod_level_items(level: usize, group: Option<&LodGroup>) -> Vec<(MenuItem, Option<RowAction>)> {
    let count = group.map_or(0, |group| group.levels.len());
    if level >= count {
        return vec![(MenuItem::new("Insert"), Some(RowAction::InsertLodLevel(count)))];
    }
    vec![
        (MenuItem::new("Insert Before"), Some(RowAction::InsertLodLevel(level))),
        (MenuItem::new("Delete"), Some(RowAction::DeleteLodLevel(level))),
    ]
}

/// A row's menu: while playing, keeping its value, for any but a script's
/// (those are the values it started with); in edit mode, Reset, for the
/// Transform and script fields.
fn row_items(field: &Field, playing: bool, can_keep: bool, kept: bool) -> Vec<(MenuItem, Option<RowAction>)> {
    let script = matches!(field, Field::Prop(Prop::Field { .. }));
    let mut items = Vec::new();
    if playing && !script {
        let keep = MenuItem::new("Keep value after play").checked(kept).enabled(can_keep);
        items.push((keep, Some(RowAction::Keep(field.clone()))));
    }
    let whole_field = matches!(field, Field::Prop(prop) if props::depth(prop) == 0);
    let resets =
        matches!(field, Field::Position | Field::Rotation | Field::Scale) || (script && whole_field && !playing);
    if resets {
        if !items.is_empty() {
            items.push((MenuItem::separator(), None));
        }
        let label = if script { "Reset to the script's value" } else { "Reset" };
        items.push((MenuItem::new(label), Some(RowAction::Reset(field.clone()))));
    }
    items
}

/// The components an Obj has and the values each shows.
fn layout(game: &Game, entity: Entity, types: &ScriptTypes) -> Vec<(Part, Vec<Prop>)> {
    let world = &game.engine.world;
    props::parts(world, entity)
        .into_iter()
        .map(|part| (part.clone(), props::props(world, entity, &part, types)))
        .collect()
}

/// The values a `[ValueDropdown]` field offers, as its behaviour's code
/// last answered.
fn offered<'a>(answers: &'a ScriptAnswers, prop: &Prop, meta: &FieldInspector) -> Option<&'a [(String, Value)]> {
    match prop {
        Prop::Field { behaviour, path } if meta.dropdown.is_some() => match path.as_slice() {
            [Step::Key(field)] => answers.options.get(&(behaviour.clone(), field.clone())).map(Vec::as_slice),
            _ => None,
        },
        _ => None,
    }
}

/// The control for a value: a field, a toggle, three fields, a dropdown, or
/// a label when it is not edited here.
fn control(
    ui: &mut Ui,
    row: NodeId,
    value: &Value,
    editable: bool,
    (meta, offered): (&FieldInspector, Option<&[(String, Value)]>),
) -> Control {
    if !editable {
        let label = ui.add_label(row, &display(value));
        ui.add_class(label, "value");
        return Control::Fixed(label);
    }
    if let Some(offered) = offered.filter(|o| !o.is_empty()) {
        // A value the scene set that is not offered stays first, as itself, until another is picked.
        let mut options: Vec<(String, Value)> = offered.to_vec();
        if !options.iter().any(|(_, v)| v == value) {
            options.insert(0, (display(value), value.clone()));
        }
        let labels: Vec<&str> = options.iter().map(|(label, _)| label.as_str()).collect();
        let index = options.iter().position(|(_, v)| v == value).unwrap_or_default();
        let dropdown = ui.dropdown(row, &labels, index);
        ui.add_class(dropdown, "value-choice");
        ui.add_class(dropdown, "attr-dropdown");
        return Control::Offered(dropdown, options.into_iter().map(|(_, v)| v).collect());
    }
    if let Some(control) = attribute_control(ui, row, value, meta) {
        return control;
    }
    match value {
        Value::Number(_) | Value::Whole(_) | Value::Text(_) => {
            let field = ui.text_field(row, &display(value));
            ui.add_class(field, "value-input");
            ui.set_class(field, "number-input", !matches!(value, Value::Text(_)));
            if let (Some(lines), Value::Text(_)) = (meta.multiline, value) {
                ui.set_multiline(field, true);
                ui.add_class(field, "attr-multiline");
                ui.set_inline_style(field, &format!("height: {}px", lines.max(1) * 16 + 10));
            }
            Control::Text(field)
        }
        Value::Toggle(on) => {
            let toggle = ui.toggle(row, "", *on);
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        Value::Vector(_) | Value::Color(_) => {
            let colour = matches!(value, Value::Color(_));
            let count = numbers(value).len();
            let letters = if colour { &["r", "g", "b"][..] } else { &["x", "y", "z", "w"][..count.min(4)] };
            let vector = ui.panel(row, Style::default());
            ui.add_class(vector, "vector");
            ui.set_class(vector, "color", colour);
            ui.set_class(vector, &format!("vector-{count}"), !colour);
            let swatch = colour.then(|| {
                let swatch = ui.panel(vector, Style::default());
                ui.add_class(swatch, "swatch");
                swatch
            });
            let fields = axis_fields(ui, vector, letters);
            let mut control = Control::Axes(fields, swatch);
            refresh_control(ui, &mut control, value);
            control
        }
        Value::Obj(_) | Value::Asset(_) => {
            let picker = ui.panel(row, Style::row());
            ui.add_class(picker, "value-picker");
            ui.accept_drops(picker, true);
            if matches!(value, Value::Asset(_)) {
                // The asset's preview, before its name, once the editor has one drawn.
                let thumb = ui.image(picker);
                ui.add_class(thumb, "field-thumb");
                ui.set_visible(thumb, false);
            }
            let label = ui.add_label(picker, &display(value));
            ui.add_class(label, "picker-label");
            let arrow = ui.icon_button(picker, "chevron-down");
            ui.add_class(arrow, "picker-arrow");
            Control::Picker(picker, label, arrow)
        }
        Value::Choice(index, options) => {
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            let dropdown = ui.dropdown(row, &labels, *index);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown, options.clone())
        }
        Value::Fixed(_) | Value::List(_) | Value::Group => {
            let label = ui.add_label(row, &display(value));
            ui.add_class(label, "value");
            Control::Fixed(label)
        }
    }
}

/// What the user changed in a control this frame, as a value of the same
/// kind as `current`.
fn read_control(ui: &Ui, control: &Control, current: &Value) -> Option<Value> {
    match (control, current) {
        (Control::Text(field), _) if !ui.changed(*field) => None,
        (Control::Text(field), Value::Number(_)) => ui.text(*field).trim().parse().ok().map(Value::Number),
        (Control::Text(field), Value::Whole(_)) => ui.text(*field).trim().parse().ok().map(Value::Whole),
        (Control::Text(field), Value::Text(_)) => Some(Value::Text(ui.text(*field))),
        (Control::Toggle(toggle), _) if ui.changed(*toggle) => Some(Value::Toggle(ui.is_checked(*toggle))),
        (Control::Axes(fields, _), Value::Vector(values)) if fields.iter().any(|&f| ui.changed(f)) => {
            Some(Value::Vector(read_numbers(ui, fields, values)))
        }
        (Control::Axes(fields, _), Value::Color(rgb)) if fields.iter().any(|&f| ui.changed(f)) => {
            let rgb = read_numbers(ui, fields, rgb);
            Some(Value::Color([rgb[0], rgb[1], rgb[2]]))
        }
        (Control::Choice(dropdown, options), _) if ui.changed(*dropdown) => {
            Some(Value::Choice(ui.selected_option(*dropdown), options.clone()))
        }
        (Control::Offered(dropdown, values), _) if ui.changed(*dropdown) => {
            values.get(ui.selected_option(*dropdown)).cloned()
        }
        (Control::Buttons(buttons, options), _) if ui.changed(*buttons) => {
            Some(Value::Choice(ui.segment(*buttons), options.clone()))
        }
        (Control::Range(slider, _), value) if ui.changed(*slider) => {
            Some(numeric(value, ui.slider_value(*slider) as f64))
        }
        (Control::Range(_, field), value) if ui.changed(*field) => {
            ui.text(*field).trim().parse::<f64>().ok().map(|n| numeric(value, n))
        }
        (Control::Progress(bar, _, _, [min, max]), value) if ui.is_pressed(*bar) => {
            let (x, _) = ui.pointer_position()?;
            let [left, _, width, _] = ui.rect(*bar);
            let share = f64::from(((x - left) / width.max(1.0)).clamp(0.0, 1.0));
            Some(numeric(value, min + share * (max - min)))
        }
        _ => None,
    }
}

/// A number as the value it replaces is: a whole number rounded.
fn numeric(current: &Value, n: f64) -> Value {
    match current {
        Value::Whole(_) => Value::Whole(n.round() as i64),
        _ => Value::Number(n as f32),
    }
}

/// The controls attributes ask for in place of the usual: a bar for
/// `[ProgressBar]`, a slider for `[Range]`, and buttons for
/// `[EnumToggleButtons]`.
fn attribute_control(ui: &mut Ui, row: NodeId, value: &Value, meta: &FieldInspector) -> Option<Control> {
    let number = match value {
        Value::Number(n) => Some(f64::from(*n)),
        Value::Whole(n) => Some(*n as f64),
        _ => None,
    };
    if let (Some(n), Some([min, max])) = (number, meta.progress) {
        let bar = ui.panel(row, Style::default());
        ui.add_class(bar, "attr-progress");
        let fill = ui.panel(bar, Style::default());
        ui.add_class(fill, "attr-progress-fill");
        let label = ui.add_label(bar, "");
        ui.add_class(label, "attr-progress-value");
        let mut control = Control::Progress(bar, fill, label, [min, max]);
        refresh_control(ui, &mut control, &numeric(value, n));
        return Some(control);
    }
    if let (Some(n), Some([min, max])) = (number, meta.range) {
        let holder = ui.panel(row, Style::row());
        ui.add_class(holder, "attr-range");
        let slider = ui.slider(holder, min as f32, max as f32, n as f32);
        ui.add_class(slider, "attr-slider");
        if matches!(value, Value::Whole(_)) {
            ui.set_slider_step(slider, 1.0);
        }
        let field = ui.text_field(holder, &display(value));
        ui.add_class(field, "value-input");
        ui.add_class(field, "number-input");
        ui.add_class(field, "attr-range-input");
        return Some(Control::Range(slider, field));
    }
    if let (Value::Choice(index, options), true) = (value, meta.enum_buttons) {
        let buttons = ui.segmented(row);
        ui.add_class(buttons, "attr-buttons");
        for option in options {
            ui.add_segment(buttons, option, None);
        }
        ui.set_segment(buttons, *index);
        return Some(Control::Buttons(buttons, options.clone()));
    }
    None
}

/// The least and most a number may be, as its attributes say.
fn limits(meta: &FieldInspector) -> (Option<f64>, Option<f64>) {
    let [low, high] = meta.range.or(meta.progress).map_or([None, None], |[a, b]| [Some(a), Some(b)]);
    let low = match (low, meta.min) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let high = match (high, meta.max) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    (low, high)
}

/// A number held within its limits; anything else as it is.
fn clamp(value: Value, (low, high): (Option<f64>, Option<f64>)) -> Value {
    let hold = |n: f64| n.max(low.unwrap_or(f64::MIN)).min(high.unwrap_or(f64::MAX));
    match value {
        Value::Number(n) => Value::Number(hold(f64::from(n)) as f32),
        Value::Whole(n) => Value::Whole(hold(n as f64).round() as i64),
        other => other,
    }
}

/// A message in a box: a note, a warning or an error.
/// The items of a list of `count` shown on a page, or all of them.
fn page_range(count: usize, per_page: usize, page: usize) -> std::ops::Range<usize> {
    if per_page == 0 {
        return 0..count;
    }
    let start = (page * per_page).min(count);
    start..(start + per_page).min(count)
}

/// A colour from hex, `#rrggbb`, as sRGB from 0 to 1.
fn parse_hex(hex: &str) -> Option<[f32; 3]> {
    let digits = hex.trim().trim_start_matches('#');
    let bits = u32::from_str_radix(digits, 16).ok().filter(|_| digits.len() == 6)?;
    Some([bits >> 16, bits >> 8, bits].map(|c| (c & 0xff) as f32 / 255.0))
}

/// A note, warning or error above a field; returns it and its text.
fn info_box(ui: &mut Ui, container: NodeId, kind: InfoKind, message: &str) -> (NodeId, NodeId) {
    let frame = ui.panel(container, Style::row());
    ui.add_class(frame, "info-box");
    let (class, icon) = match kind {
        InfoKind::Info => ("info", "info"),
        InfoKind::Warning => ("warning", "alert"),
        InfoKind::Error => ("error", "alert"),
    };
    ui.add_class(frame, class);
    let mark = ui.icon(frame, icon);
    ui.add_class(mark, "info-icon");
    let text = ui.add_label(frame, message);
    ui.add_class(text, "info-text");
    (frame, text)
}

/// Shows a value in its control, except in a field being typed in.
/// What a field shows where the Objs selected hold different values.
const MIXED: &str = "—";

/// Shows "—" where the other Objs selected hold another value than the one
/// shown, as Unity's Inspector does: each part of a vector or a colour on
/// its own, any other value whole, in a field not being typed in. The row
/// matches `.mixed` while any part differs.
fn show_mixed(ui: &mut Ui, row: NodeId, control: &Control, value: &Value, theirs: &[Value]) {
    let mixed = match (control, value) {
        (Control::Axes(fields, _), Value::Vector(_) | Value::Color(_)) => {
            let mine = numbers(value);
            let mut any = false;
            for (axis, &field) in fields.iter().enumerate() {
                let differs = theirs.iter().any(|other| numbers(other).get(axis) != mine.get(axis));
                if differs && ui.focused() != Some(field) {
                    ui.set_text(field, MIXED);
                }
                any |= differs;
            }
            any
        }
        _ => theirs.iter().any(|other| other != value),
    };
    match control {
        Control::Text(field) | Control::Fixed(field) if mixed && ui.focused() != Some(*field) => {
            ui.set_text(*field, MIXED);
        }
        Control::Picker(_, label, _) if mixed => ui.set_text(*label, MIXED),
        _ => {}
    }
    ui.set_class(row, "mixed", mixed);
}

/// The value an edit gives another Obj selected: `picked`, though of a
/// vector or a colour only the parts that changed from `was`, the rest kept
/// as `theirs` has them.
fn changed_parts(was: &Value, picked: &Value, theirs: Value) -> Value {
    let merge = |was: &[f32], now: &[f32], theirs: &[f32]| -> Vec<f32> {
        theirs.iter().enumerate().map(|(i, &t)| if was.get(i) != now.get(i) { now[i] } else { t }).collect()
    };
    match (was, picked, theirs) {
        (Value::Vector(was), Value::Vector(now), Value::Vector(theirs)) if theirs.len() == now.len() => {
            Value::Vector(merge(was, now, &theirs))
        }
        (Value::Color(was), Value::Color(now), Value::Color(theirs)) => {
            let merged = merge(was, now, &theirs);
            Value::Color([merged[0], merged[1], merged[2]])
        }
        _ => picked.clone(),
    }
}

fn refresh_control(ui: &mut Ui, control: &mut Control, value: &Value) {
    match (control, value) {
        (Control::Text(field), value) => {
            let text = display(value);
            if ui.focused() != Some(*field) && ui.text(*field) != text {
                ui.set_text(*field, &text);
            }
        }
        (Control::Toggle(toggle), Value::Toggle(on)) => {
            if ui.is_checked(*toggle) != *on {
                ui.set_checked(*toggle, *on);
            }
        }
        (Control::Axes(fields, swatch), Value::Vector(_) | Value::Color(_)) => {
            write_numbers(ui, fields, &numbers(value));
            if let (Some(swatch), Value::Color(rgb)) = (swatch, value) {
                let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                ui.set_inline_style(*swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            }
        }
        (Control::Picker(_, label, _), value) => {
            let text = display(value);
            if ui.text(*label) != text {
                ui.set_text(*label, &text);
            }
        }
        (Control::Choice(dropdown, shown), Value::Choice(index, options)) => {
            if shown != options {
                let labels: Vec<&str> = options.iter().map(String::as_str).collect();
                ui.set_options(*dropdown, &labels);
                shown.clone_from(options);
            }
            if ui.selected_option(*dropdown) != *index {
                ui.set_selected_option(*dropdown, *index);
            }
        }
        (Control::Fixed(label), value) => {
            let text = display(value);
            if ui.text(*label) != text {
                ui.set_text(*label, &text);
            }
        }
        (Control::Offered(dropdown, values), value) => {
            if let Some(index) = values.iter().position(|v| v == value).filter(|&i| ui.selected_option(*dropdown) != i)
            {
                ui.set_selected_option(*dropdown, index);
            }
        }
        (Control::Buttons(buttons, _), Value::Choice(index, _)) => {
            if ui.segment(*buttons) != *index {
                ui.set_segment(*buttons, *index);
            }
        }
        (Control::Range(slider, field), value) => {
            let n = number_of(value);
            if (ui.slider_value(*slider) - n).abs() > f32::EPSILON {
                ui.set_slider_value(*slider, n);
            }
            let text = display(value);
            if ui.focused() != Some(*field) && ui.text(*field) != text {
                ui.set_text(*field, &text);
            }
        }
        (Control::Progress(_, fill, label, [min, max]), value) => {
            let n = f64::from(number_of(value));
            let share = if max > min { ((n - *min) / (*max - *min)).clamp(0.0, 1.0) } else { 0.0 };
            ui.set_inline_style(*fill, &format!("width: {:.2}%", share * 100.0));
            ui.set_text(*label, &display(value));
        }
        _ => {}
    }
}

/// A number's value, or a whole number's, as a float.
fn number_of(value: &Value) -> f32 {
    match value {
        Value::Number(n) => *n,
        Value::Whole(n) => *n as f32,
        _ => 0.0,
    }
}

/// A value as text, for a field or a label.
fn display(value: &Value) -> String {
    match value {
        Value::Number(n) => number(*n),
        Value::Whole(n) => n.to_string(),
        Value::Toggle(on) => if *on { "On" } else { "Off" }.into(),
        Value::Text(text) | Value::Fixed(text) | Value::Obj(text) => text.clone(),
        Value::Asset(path) => path.rsplit('/').next().unwrap_or(path).to_string(),
        Value::Vector(_) | Value::Color(_) => numbers(value).into_iter().map(number).collect::<Vec<_>>().join(", "),
        Value::Choice(index, options) => options.get(*index).cloned().unwrap_or_default(),
        Value::List(1) => "1 item".into(),
        Value::List(count) => format!("{count} items"),
        Value::Group => String::new(),
    }
}

/// A vector's or colour's numbers.
fn numbers(value: &Value) -> Vec<f32> {
    match value {
        Value::Vector(values) => values.clone(),
        Value::Color(rgb) => rgb.to_vec(),
        _ => Vec::new(),
    }
}

/// The Obj's icon, its name field and whether it is active, then rows for
/// its tag, layer, whether it is static and whether it is persistent, and
/// for a prefab instance or an Obj in one, its prefab. Returns the top row,
/// the name field, the persistent box, and the prefab row's Open and Select
/// with the prefab's path. Active, tag, layer and static show their
/// defaults until Objs have them.
/// The Obj's name and its switch on one row, Tag and Layer side by side on
/// the next, and Static and Persistent on a short one under them, kept
/// short as Unity's header is. With
/// `editing` more than one, a chip by the name says how many Objs an edit
/// goes to.
fn header(
    ui: &mut Ui,
    root: NodeId,
    game: &Game,
    entity: Entity,
    editing: usize,
) -> (NodeId, NodeId, NodeId, (NodeId, Vec<u8>), Option<PrefabButtons>) {
    let world = &game.engine.world;
    let header = ui.panel(root, Style::column());
    ui.add_class(header, "inspector-header");
    let top = ui.panel(header, Style::row());
    ui.add_class(top, "header-row");
    let tile = ui.panel(top, Style::row());
    ui.add_class(tile, "obj-tile");
    let kind = crate::hierarchy::icon(world, entity);
    let prefab = prefab_of(game, entity);
    // A prefab instance shows Unity's prefab cube, tinted.
    let instance = prefab.as_ref().is_some_and(|(_, root)| *root);
    ui.icon(tile, if instance { "cube" } else { kind });
    ui.set_class(tile, "prefab-root", instance);
    let name = ui.text_field(top, &world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_default());
    ui.add_class(name, "obj-name");
    if editing > 1 {
        let many = ui.add_label(top, &format!("{editing} Objs"));
        ui.add_class(many, "inspector-many");
    }
    ui.toggle(top, "", true);
    let tags = ui.panel(header, Style::row());
    ui.add_class(tags, "header-tags");
    let label = ui.add_label(tags, "Tag");
    ui.add_class(label, "header-label");
    ui.dropdown(tags, &["Untagged"], 0);
    // The layers the project names, in number order, an Obj without one on Default.
    let label = ui.add_label(tags, "Layer");
    ui.add_class(label, "header-label");
    let layers = world.resource::<Layers>().cloned().unwrap_or_default();
    let (numbers, names): (Vec<u8>, Vec<&str>) = layers.used().unzip();
    let now = world.get::<Layer>(entity).map_or(0, |layer| layer.0);
    let shown = numbers.iter().position(|&number| number == now).unwrap_or_default();
    let layer = ui.dropdown(tags, &names, shown);
    ui.set_name(layer, "obj-layer");
    let checks = ui.panel(header, Style::row());
    ui.add_class(checks, "header-checks");
    let fixed = ui.toggle(checks, "Static", false);
    let persistent = ui.toggle(checks, "Persistent", world.has::<ScenePersistence>(entity));
    for check in [fixed, persistent] {
        ui.add_class(check, "checkbox");
        ui.add_class(check, "header-check");
    }
    ui.add_class(persistent, "persistent-check");
    let buttons = prefab.map(|(path, root)| {
        let row = prop(ui, header, if root { "Prefab" } else { "In prefab" });
        let holder = ui.panel(row, Style::row());
        ui.add_class(holder, "prop-value");
        let icon = ui.icon(holder, "cube");
        ui.add_class(icon, "prefab-icon");
        let file = ui.add_label(holder, path.rsplit('/').next().unwrap_or(&path));
        ui.add_class(file, "prefab-file");
        let mut button = |label: &str| {
            let button = ui.button(holder, label);
            ui.add_class(button, "prefab-button");
            button
        };
        let (open, select) = (button("Open"), button("Select"));
        // As in Unity, on the instance's root only.
        let overrides = root.then(|| button("Overrides"));
        PrefabButtons { open, select, overrides, path }
    });
    (top, name, persistent, (layer, numbers), buttons)
}

/// The Prefab row's Open, Select and, on an instance's root, Overrides, and
/// the prefab's path.
struct PrefabButtons {
    open: NodeId,
    select: NodeId,
    overrides: Option<NodeId>,
    path: String,
}

/// Where a row's value is in a scene file's Obj, as overrides name it.
fn value_path(field: &Field) -> ValuePath {
    let prop = match field {
        Field::Name => return ValuePath::new(&["name"]),
        Field::Position => return ValuePath::new(&["position"]),
        Field::Rotation => return ValuePath::new(&["rotation"]),
        Field::Scale => return ValuePath::new(&["scale"]),
        Field::Prop(prop) => prop,
    };
    let mut path = part_paths(Some(&prop.part())).swap_remove(0);
    let key = match prop {
        Prop::LightKind => "type".to_string(),
        Prop::SpotInner => "inner_angle".to_string(),
        Prop::SpotOuter => "outer_angle".to_string(),
        Prop::FieldOfView => "fov".to_string(),
        Prop::Exposure => "exposure".to_string(),
        Prop::Near => "near".to_string(),
        Prop::LodMeasure | Prop::LodCull | Prop::LodObj(..) | Prop::LodAdd(_) | Prop::LodUntil(_) => return path,
        _ => prop.lock_key(),
    };
    if !key.is_empty() {
        path.0.push(key);
    }
    path
}

/// Where a component's values are in a scene file's Obj: the Transform's
/// for none.
fn part_paths(part: Option<&Part>) -> Vec<ValuePath> {
    let key = match part {
        None => return ["position", "rotation", "scale"].map(|key| ValuePath::new(&[key])).to_vec(),
        Some(Part::Renderer) => "renderer",
        Some(Part::Light) => "light",
        Some(Part::Camera) => "camera",
        Some(Part::Audio) => "audio",
        Some(Part::Listener) => "audio_listener",
        Some(Part::LodGroup) => "lod_group",
        Some(Part::Collider) => "collider",
        Some(Part::Rigidbody) => "rigidbody",
        Some(Part::Character) => "character",
        Some(Part::Joint) => "joint",
        Some(Part::Collider2D) => "collider2d",
        Some(Part::Rigidbody2D) => "rigidbody2d",
        Some(Part::Behaviour(name)) => return vec![ValuePath::new(&["fields", name])],
    };
    vec![ValuePath::new(&[key])]
}

/// The prefab an Obj is an instance of, or the one whose instance it is
/// in, by path, and whether the Obj is that instance's root.
fn prefab_of(game: &Game, entity: Entity) -> Option<(String, bool)> {
    let world = &game.engine.world;
    let root = match world.has::<PrefabInstance>(entity) {
        true => entity,
        false => world.get::<FromPrefab>(entity)?.instance,
    };
    let PrefabInstance(guid) = *world.get::<PrefabInstance>(root)?;
    Some((game.content.db().get(guid)?.path.clone(), root == entity))
}

/// A row of a label and three axis fields, X, Y and Z. Returns the row and
/// the fields.
/// A colour's row as the Inspector shows one: a swatch that opens the colour
/// picker, then its red, green and blue. Returns the fields and the swatch.
pub(crate) fn color_row(ui: &mut Ui, rows: NodeId, label: &str) -> ([NodeId; 3], NodeId) {
    let row = prop(ui, rows, label);
    let vector = ui.panel(row, Style::default());
    ui.add_class(vector, "vector");
    ui.add_class(vector, "color");
    let swatch = ui.panel(vector, Style::default());
    ui.add_class(swatch, "swatch");
    let fields = axis_fields(ui, vector, &["r", "g", "b"]);
    ([fields[0], fields[1], fields[2]], swatch)
}

pub(crate) fn vector_row(ui: &mut Ui, rows: NodeId, label: &str) -> (NodeId, [NodeId; 3]) {
    let row = prop(ui, rows, label);
    let vector = ui.panel(row, Style::default());
    ui.add_class(vector, "vector");
    let fields = axis_fields(ui, vector, &["x", "y", "z"]);
    (row, [fields[0], fields[1], fields[2]])
}

/// A field for each letter, each with its coloured letter inside.
fn axis_fields(ui: &mut Ui, vector: NodeId, letters: &[&str]) -> Vec<NodeId> {
    letters
        .iter()
        .map(|axis| {
            let cell = ui.panel(vector, Style::row());
            ui.add_class(cell, "axis-field");
            let letter = ui.add_label(cell, &axis.to_uppercase());
            ui.add_class(letter, "axis-letter");
            ui.add_class(letter, &format!("axis-{axis}"));
            let field = ui.text_field(cell, "");
            ui.add_class(field, "axis-input");
            field
        })
        .collect()
}

/// A component's section: a line above it, then a foldout headed by its
/// icon and title, and at the right a switch for whether it is enabled
/// (shown on, until components can be turned off) or, for the Transform,
/// which cannot be, a menu button. Returns its header, where its rows go,
/// and the menu button.
fn section(ui: &mut Ui, root: NodeId, title: &str, icon: &str, switch: bool) -> (NodeId, NodeId, Option<NodeId>) {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    let header = ui.foldout_header(foldout).expect("a foldout has a header");
    let menu = if switch {
        ui.toggle(header, "", true);
        None
    } else {
        Some(ui.icon_button(header, "more"))
    };
    (header, ui.foldout_content(foldout).expect("a foldout has content"), menu)
}

/// A property's row: its label in the first 88 pixels, the value after.
pub(crate) fn prop(ui: &mut Ui, rows: NodeId, label: &str) -> NodeId {
    let row = ui.panel(rows, Style::default());
    ui.add_class(row, "prop");
    let name = ui.add_label(row, label);
    ui.add_class(name, "prop-label");
    row
}

/// A number as the Inspector writes it: up to three decimals, no trailing zeros.
pub(crate) fn number(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.to_string() }
}

/// What three axis fields say, keeping `current` for any that is not a number
/// or that the user has not changed.
fn read(ui: &Ui, fields: &[NodeId; 3], current: Vec3) -> Vec3 {
    let read = read_numbers(ui, fields, &current.to_array());
    Vec3::new(read[0], read[1], read[2])
}

/// Shows `value` in fields the user is not editing.
fn write(ui: &mut Ui, fields: &[NodeId; 3], value: Vec3) {
    write_numbers(ui, fields, &value.to_array());
}

/// What a row of number fields says, keeping `current` for any that is not
/// a number or that the user has not changed.
fn read_numbers(ui: &Ui, fields: &[NodeId], current: &[f32]) -> Vec<f32> {
    let mut values = current.to_vec();
    for (value, field) in values.iter_mut().zip(fields) {
        if ui.changed(*field)
            && let Ok(number) = ui.text(*field).trim().parse::<f32>()
        {
            *value = number;
        }
    }
    values
}

/// Shows numbers in fields the user is not editing.
fn write_numbers(ui: &mut Ui, fields: &[NodeId], values: &[f32]) {
    for (field, value) in fields.iter().zip(values) {
        let text = number(*value);
        if ui.focused() != Some(*field) && ui.text(*field) != text {
            ui.set_text(*field, &text);
        }
    }
}
