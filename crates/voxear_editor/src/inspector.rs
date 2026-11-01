//! The Inspector: the selected Obj's header (its kind, name, and the
//! active, tag, layer and static settings to come), then a section for its
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
//! behaviour or an asset as what it points at, picked from a menu.

use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    script_types::{FieldInspector, InfoKind, ScriptTypes},
};
use voxear_render::{Camera, Light, MeshRenderer, Renderer};
use voxear_scene::{Name, Transform};
use voxear_ui::{DragSource, MenuItem, NodeId, Style, Ui};

use voxear_engine::prefab::Locks;

use crate::{
    asset_inspector::{AssetAction, AssetView},
    attributes,
    fields::{Pick, Step},
    history::{Edit, ObjState},
    project_view::ProjectItem,
    props::{self, Part, Prop, Value},
};

/// The fields showing an Obj's name and Transform, by what they edit, and
/// the row each is on.
struct Fields {
    name: NodeId,
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
    /// A reference: a button showing what it points at, which opens a menu.
    Picker(NodeId, NodeId),
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
    /// A list's items shown: each one's row, which takes drops, its label,
    /// which drags it, and the item.
    item_handles: Vec<(NodeId, NodeId, Prop)>,
}

impl Inspector {
    pub fn new(root: NodeId) -> Self {
        Self {
            root,
            shown: None,
            fields: None,
            transform_menu: None,
            headers: Vec::new(),
            rows: Vec::new(),
            add: None,
            types: ScriptTypes::default(),
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
            item_handles: Vec::new(),
        }
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
        items
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
        self.rows.iter().find_map(|row| {
            let tooltip = row.tooltip.as_ref()?;
            let label = *ui.children(row.row).first()?;
            ui.is_hovered(label).then(|| (tooltip.clone(), label))
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
            Control::Picker(picker, _) => Some((row.prop.clone(), thumb(picker)?)),
            _ => None,
        });
        pickers.collect()
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
        self.asset = Some(AssetView::build(ui, self.root, item, game, &self.types));
    }

    /// What the asset shown asks of the editor this frame.
    pub fn update_asset(&mut self, ui: &Ui) -> Option<AssetAction> {
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
        self.shown = entity.filter(|&e| world.get::<Transform>(e).is_some());
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
        self.item_handles.clear();
        self.group_tabs.clear();
        self.group_foldouts.clear();
        let Some(entity) = self.shown else {
            let hint = ui.add_label(self.root, "Select an Obj in the Hierarchy to see it here.");
            ui.add_class(hint, "placeholder");
            return;
        };
        let (name_row, name) = header(ui, self.root, game, entity);

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
        self.fields = Some(Fields { name, position, rotation, scale, rows });
        self.transform_menu = menu;
        self.headers.push((None, transform));

        let layout = layout(game, entity, &self.types);
        let locks = world.get::<Locks>(entity).cloned();
        for (part, values) in &layout {
            // A C# component is shown as a box of data, apart from behaviours' code.
            let icon = match part {
                Part::Behaviour(name) if self.types.is_component(name) => "box",
                _ => part.icon(),
            };
            let (header, rows, _) = section(ui, self.root, part.title(), icon, true);
            self.headers.push((Some(part.clone()), header));
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
            let values = attributes::ordered(&self.types, values);
            let mut groups = attributes::Groups::new(rows);
            for value in &values {
                if !self.prefab_mode && !designers_see(value) {
                    continue;
                }
                if !attributes::shows(game, &self.types, entity, value)
                    || attributes::drawn_by_list(&self.types, value, &self.pages)
                {
                    continue;
                }
                let mut meta = attributes::of(&self.types, value).clone();
                let container = groups.container(ui, attributes::top(&self.types, value).group.as_ref());
                let mut label = meta.label.clone().unwrap_or_else(|| props::label(value));
                // A [ListDrawerSettings] list labels its items by index, or not at all.
                if let Some((Some(settings), index)) = attributes::item_of(&self.types, value) {
                    label = index.to_string();
                    meta.hide_label |= !settings.index_labels;
                }
                self.decorate(ui, container, game, (entity, value), (&meta, &label));
                let row = prop(ui, container, &label);
                ui.set_class(row, "hidden-from-designers", !designers_see(value));
                let label_node = ui.children(row)[0];
                let indent = props::depth(value) + meta.indent as usize;
                if indent > 0 {
                    ui.set_inline_style(label_node, &format!("padding-left: {}px", indent * 12));
                }
                if meta.hide_label {
                    ui.add_class(row, "no-label");
                    ui.set_visible(label_node, false);
                }
                let shown = props::get(game, &self.types, entity, value).unwrap_or(Value::Fixed(String::new()));
                let enabled = attributes::enabled(game, &self.types, entity, value);
                ui.set_class(row, "disabled", !enabled && !meta.read_only);
                // While playing, a script's values are those it started with, but a C#
                // component's are read and written in the running game.
                let live = matches!(value, Prop::Field { behaviour, path }
                    if crate::live::is_live(&game.engine.world, &self.types, entity, behaviour, path));
                let editable = !(playing && matches!(value, Prop::Field { .. }) && !live) && enabled;
                // An item of a list, or a list, gets its button beside its value, as a unit does.
                let item = editable && props::is_item(value);
                let list = editable && matches!(shown, Value::List(_));
                let holder = if item || list || meta.suffix.is_some() || meta.documentation.is_some() {
                    let holder = ui.panel(row, Style::row());
                    ui.add_class(holder, "prop-value");
                    holder
                } else {
                    row
                };
                let offered = match value {
                    Prop::Field { behaviour, path } if meta.dropdown.is_some() => match path.as_slice() {
                        [Step::Key(field)] => self.answers.options.get(&(behaviour.clone(), field.clone())),
                        _ => None,
                    },
                    _ => None,
                };
                let control = control(ui, holder, &shown, editable, (&meta, offered.map(Vec::as_slice)));
                if let Some(suffix) = &meta.suffix {
                    let unit = ui.add_label(holder, suffix);
                    ui.add_class(unit, "attr-suffix");
                }
                let button = |ui: &mut Ui, icon: &str, class: &str| {
                    let button = ui.icon_button(holder, icon);
                    ui.add_class(button, class);
                    button
                };
                let add = list.then(|| button(ui, "plus", "list-add"));
                let remove = item.then(|| button(ui, "close", "list-remove"));
                // An item's label drags it onto another of its list, which moves it there.
                if item && !meta.hide_label {
                    ui.set_draggable(label_node, true);
                    ui.add_class(label_node, "drag-handle");
                    ui.accept_drops(row, true);
                    self.item_handles.push((row, label_node, value.clone()));
                }
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
                self.rows.push(PropRow { prop: value.clone(), row, control, add, remove, tooltip, limits });
                if let (Value::List(count), Prop::Field { behaviour, path }) = (&shown, value) {
                    let per_page = meta.list.map_or(0, |settings| settings.per_page);
                    let page = attributes::page(&self.pages, behaviour, path);
                    let shown_items = page_range(*count, per_page, page);
                    if attributes::is_table(&self.types, behaviour, path) {
                        self.table(ui, container, game, (entity, behaviour, path), shown_items, editable);
                    }
                    if per_page > 0 && *count > per_page {
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
            if let Part::Behaviour(name) = part {
                self.add_buttons(ui, rows, name, playing);
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

    /// Turns a page, picks a palette's colour or moves a list's item, as
    /// clicked or dropped this frame. True when a page turned or an item
    /// moved, which builds the sections again.
    fn update_lists(&mut self, ui: &Ui, game: &mut Game, entity: Entity, renderer: Option<&mut Renderer>) -> bool {
        let mut turned = false;
        let dropped = self.item_handles.iter().find_map(|(row, _, onto)| {
            let Some(DragSource::Element(handle)) = ui.dropped_onto(*row) else { return None };
            let (_, _, item) = self.item_handles.iter().find(|(_, h, _)| *h == handle)?;
            Some((item.clone(), onto.clone()))
        });
        if let Some((item, onto)) = dropped {
            turned |= props::move_item(&mut game.engine.world, &self.types, entity, &item, &onto);
        }
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

    /// A behaviour's `[Button]`s, under its fields. They run in edit mode,
    /// on the scene's values; while playing they wait.
    fn add_buttons(&mut self, ui: &mut Ui, rows: NodeId, behaviour: &str, playing: bool) {
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
            ui.set_class(node, "disabled", playing);
            if !playing {
                self.buttons.push((node, behaviour.to_string(), button.method.clone()));
            }
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
        let layout = layout(game, entity, &self.types);
        let conditions = attributes::key(game, &self.types, entity, &layout);
        if layout != self.built.0 || playing != self.built.1 || locks != self.built.2 || conditions != self.conditions {
            self.show(ui, game, Some(entity), playing);
            return None;
        }
        let before = ObjState::of(&game.engine.world, entity);
        if self.update_lists(ui, game, entity, renderer.as_deref_mut()) {
            self.show(ui, game, Some(entity), playing);
            let (before, after) = (before?, ObjState::of(&game.engine.world, entity)?);
            return (before != after).then_some(Edit::Changed { entity, before, after });
        }
        self.update_transform(ui, game, entity, transform);
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
                let changed = props::set(game, renderer.as_deref_mut(), &self.types, entity, &row.prop, picked);
                if let (true, Prop::Field { behaviour, .. }) = (changed, &row.prop)
                    && let Some(method) = &attributes::top(&self.types, &row.prop).on_changed
                {
                    self.callbacks.push((behaviour.clone(), method.clone()));
                }
            }
            if let Some(now) = props::get(game, &self.types, entity, &row.prop) {
                refresh_control(ui, &mut row.control, &now);
            }
        }
        let (before, after) = (before?, ObjState::of(&game.engine.world, entity)?);
        (before != after).then_some(Edit::Changed { entity, before, after })
    }

    fn update_transform(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity, transform: Transform) {
        let Some(fields) = &self.fields else { return };
        let world = &mut game.engine.world;
        if ui.changed(fields.name) {
            let name = ui.text(fields.name);
            if let Some(current) = world.get_mut::<Name>(entity) {
                current.0 = name;
            }
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
        write(ui, &fields.position, edited.position);
        write(ui, &fields.rotation, if rotation != euler { rotation } else { euler });
        write(ui, &fields.scale, edited.scale);
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
            return actions.get(index).cloned().flatten();
        }
        let dropped = self.rows.iter().find_map(|row| match row.control {
            Control::Picker(picker, _) => ui.dropped_onto(picker).map(|source| (row.prop.clone(), source)),
            _ => None,
        });
        if let Some((prop, source)) = dropped {
            return Some(RowAction::Dropped(prop, source));
        }
        let at = ui.pointer_position().unwrap_or_default();
        let mut items: Vec<(MenuItem, Option<RowAction>)>;
        let (owner, at) = if let Some(add) = self.add.filter(|&add| ui.clicked(add)) {
            items = self.add_items(playing);
            let [x, y, _, height] = ui.rect(add);
            (add, (x, y + height + 4.0))
        } else if let Some(button) = self.transform_menu.filter(|&b| ui.clicked(b)) {
            items = vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))];
            let [x, y, _, height] = ui.rect(button);
            (button, (x, y + height + 4.0))
        } else if let Some((prop, button)) = self.picker_clicked(ui) {
            return Some(RowAction::Browse(prop, button));
        } else if let Some((prop, swatch)) = self.swatch_clicked(ui) {
            return Some(RowAction::EditColor(prop, swatch));
        } else if let Some((part, header)) = self.headers.iter().find(|(_, h)| ui.context_clicked(*h)) {
            items = match part {
                None => vec![(MenuItem::new("Reset"), Some(RowAction::ResetTransform))],
                Some(part) => self.header_items(game, part, playing),
            };
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
    fn picker_clicked(&self, ui: &Ui) -> Option<(Prop, NodeId)> {
        self.rows.iter().find_map(|row| match row.control {
            Control::Picker(button, _) if ui.clicked(button) => Some((row.prop.clone(), button)),
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

    /// What Add Component offers: the components the Obj has not got, then
    /// the project's behaviours and its C# components, which are added in
    /// edit mode.
    fn add_items(&self, playing: bool) -> Vec<(MenuItem, Option<RowAction>)> {
        let has = |part: &Part| self.built.0.iter().any(|(p, _)| p == part);
        let mut items: Vec<(MenuItem, Option<RowAction>)> = [Part::Renderer, Part::Light, Part::Camera]
            .into_iter()
            .map(|part| (MenuItem::new(part.title()).enabled(!has(&part)), Some(RowAction::Add(part))))
            .collect();
        for scripts in [&self.types.behaviours, &self.types.components] {
            if !scripts.is_empty() {
                items.push((MenuItem::separator(), None));
            }
            for script in scripts {
                let part = Part::Behaviour(script.name.clone());
                let item = MenuItem::new(script.short_name()).enabled(!playing && !has(&part));
                items.push((item, Some(RowAction::Add(part))));
            }
        }
        items
    }
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
            let arrow = ui.icon(picker, "chevron-down");
            ui.add_class(arrow, "dropdown-arrow");
            Control::Picker(picker, label)
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
        (Control::Picker(_, label), value) => {
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
/// its tag, layer and whether it is static. Returns the top row and the
/// name field. Active, tag, layer and static show their defaults until
/// Objs have them.
fn header(ui: &mut Ui, root: NodeId, game: &Game, entity: Entity) -> (NodeId, NodeId) {
    let world = &game.engine.world;
    let header = ui.panel(root, Style::column());
    ui.add_class(header, "inspector-header");
    let top = ui.panel(header, Style::row());
    ui.add_class(top, "header-row");
    let tile = ui.panel(top, Style::row());
    ui.add_class(tile, "obj-tile");
    let kind = crate::hierarchy::icon(
        world.get::<Camera>(entity).is_some(),
        world.get::<Light>(entity).is_some(),
        world.get::<MeshRenderer>(entity).is_some(),
    );
    ui.icon(tile, kind);
    let name = ui.text_field(top, &world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_default());
    ui.add_class(name, "obj-name");
    ui.toggle(top, "", true);
    for (label, choice) in [("Tag", "Untagged"), ("Layer", "Default")] {
        let row = prop(ui, header, label);
        ui.dropdown(row, &[choice], 0);
    }
    let row = prop(ui, header, "Static");
    let fixed = ui.toggle(row, "", false);
    ui.add_class(fixed, "checkbox");
    (top, name)
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
