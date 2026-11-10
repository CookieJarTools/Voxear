//! A World Generator in the Inspector, and a node of its graph.
//!
//! Selected in the Project, a generator shows its own values first, which
//! are all most worlds need: its preset, its seed, scale and sea level, and
//! a row for each value its graph exposes ([`WorldFields`]). A preset is a
//! starter graph (`generate::starters`), and the row is there while the
//! generator's graph is a starter's still: picking another puts that
//! starter's graph in its place, keeping its seed and scale. A generator
//! that is one landmass and nothing more, as a file from before graphs is,
//! picks among a landmass's presets instead, which make the ground they
//! always made. Open Graph shows its graph in the Generator panel (see
//! `generator_panel`), where the rest is.
//!
//! A node picked there shows a row for each of its values ([`NodeView`]),
//! those it seldom needs under Advanced. A value that stands in for an
//! input shows only while nothing is wired into it. A context click on a
//! row's label exposes the value: it becomes a row of the generator's own.
//!
//! A Cave Network's rooms are no row of numbers: under its values it lists
//! them, each a prefab picked from the project's and how often it is
//! chosen, with a row to add one ([`RoomChoice`]).
//!
//! Both are made of [`ValueRow`]s, one for each kind of value a node can
//! have, so a new kind of value is one more arm in each of its three
//! methods. An angle is one such: its row shows degrees, while its value
//! is kept as the slope's rise over run. Each change is saved as it is made, and terrains generated
//! from the generator are made again (see `editor::generators`).

use std::sync::OnceLock;

use voxear_terrain::generate::{
    Exposed, NodeId as GraphNode, NodeKind, Preset, Room, Starter, Value, ValueKind, ValueSpec, WorldGen,
    nodes::{Land, Output, degrees_of, rise_of},
    smooth, starters,
};
use voxear_ui::{MenuItem, NodeId, Style, Ui};

use crate::{
    asset_inspector::part,
    curve_fields::CurveField,
    inspector::{number, prop},
};

/// A generator's own three values, as rows.
const SEED: ValueSpec = own("seed", "Seed", ValueKind::Whole { low: 0, high: u32::MAX });
const SCALE: ValueSpec = own("scale", "Scale", ValueKind::Number { low: 0.01, high: f32::MAX, unit: "" });
const SEA_LEVEL: ValueSpec =
    own("sea_level", "Sea level", ValueKind::Number { low: f32::MIN, high: f32::MAX, unit: "m" });

const fn own(name: &'static str, label: &'static str, kind: ValueKind) -> ValueSpec {
    ValueSpec { name, label, kind, input: None, advanced: false }
}

/// What edits a row's value.
enum RowControl {
    /// A field typed in or scrubbed by the row's label, for a number, a whole number or an angle.
    Number(NodeId),
    Choice(NodeId),
    /// A curve's graph, and what the curve is worth with no keys.
    Curve(CurveField, f32),
}

/// One value's row: its label, and the control its kind of value takes.
pub(crate) struct ValueRow {
    row: NodeId,
    label: NodeId,
    spec: ValueSpec,
    control: RowControl,
}

impl ValueRow {
    /// A row labelled `label` in `rows` for a value of `spec`'s kind, showing `value`.
    pub fn new(ui: &mut Ui, rows: NodeId, label: &str, spec: ValueSpec, value: Value) -> Self {
        let row = prop(ui, rows, label);
        let name = ui.children(row)[0];
        let control = match spec.kind {
            ValueKind::Number { unit, .. } => RowControl::Number(number_field(ui, row, unit)),
            ValueKind::Whole { .. } => RowControl::Number(number_field(ui, row, "")),
            ValueKind::Angle => RowControl::Number(number_field(ui, row, "°")),
            ValueKind::Choice(choices) => {
                let picked = if let Value::Choice(picked) = value { picked } else { 0 };
                let dropdown = ui.dropdown(row, choices, picked.min(choices.len().saturating_sub(1)));
                ui.add_class(dropdown, "value-choice");
                RowControl::Choice(dropdown)
            }
            ValueKind::Curve { empty } => RowControl::Curve(CurveField::new(ui, row), empty),
        };
        let made = Self { row, label: name, spec, control };
        made.show(ui, value);
        made
    }

    /// What the user made of the value this frame, held to what its row keeps: a number typed or
    /// scrubbed, a choice picked, or a key of its curve dragged. `now` is the value as it is.
    pub fn changed(&self, ui: &Ui, now: Value) -> Option<Value> {
        let set = match &self.control {
            RowControl::Number(field) => {
                let text = ui.changed(*field).then(|| ui.text(*field))?;
                let text = text.trim();
                match self.spec.kind {
                    // A whole number as it is typed, so a large seed keeps every digit.
                    ValueKind::Whole { .. } => match text.parse::<u32>() {
                        Ok(whole) => Value::Whole(whole),
                        Err(_) => Value::Number(text.parse().ok()?),
                    },
                    // Typed in degrees, kept as rise over run.
                    ValueKind::Angle => Value::Number(rise_of(text.parse().ok()?)),
                    _ => Value::Number(text.parse().ok()?),
                }
            }
            RowControl::Choice(dropdown) => {
                ui.changed(*dropdown).then(|| Value::Choice(ui.selected_option(*dropdown)))?
            }
            RowControl::Curve(field, _) => {
                let Value::Curve(curve) = now else { return None };
                Value::Curve(field.drag(ui, &curve)?)
            }
        };
        self.spec.kept(set).filter(|kept| *kept != now)
    }

    /// Shows `value`, but in a field being typed in, and draws a curve as it is.
    pub fn show(&self, ui: &mut Ui, value: Value) {
        match (&self.control, value) {
            (RowControl::Number(field), Value::Number(rise)) if self.spec.kind == ValueKind::Angle => {
                write(ui, *field, &number(degrees_of(rise)));
            }
            (RowControl::Number(field), Value::Number(shown)) => write(ui, *field, &number(shown)),
            (RowControl::Number(field), Value::Whole(shown)) => write(ui, *field, &shown.to_string()),
            (RowControl::Choice(dropdown), Value::Choice(picked)) => {
                if ui.selected_option(*dropdown) != picked {
                    ui.set_selected_option(*dropdown, picked);
                }
            }
            (RowControl::Curve(field, empty), Value::Curve(curve)) => {
                field.draw_by(ui, &curve, &|t| smooth(&curve, t, *empty));
            }
            _ => {}
        }
    }

    /// Marks the row as one of its generator's own rows, or not.
    pub fn set_exposed(&self, ui: &mut Ui, exposed: bool) {
        ui.set_class(self.row, "exposed", exposed);
    }

    /// Whether its label was context clicked this frame.
    fn menu_asked(&self, ui: &Ui) -> bool {
        ui.context_clicked(self.label)
    }
}

/// A number's field, with its unit after it when it has one.
fn number_field(ui: &mut Ui, row: NodeId, unit: &str) -> NodeId {
    let within = if unit.is_empty() {
        row
    } else {
        let with_unit = ui.panel(row, Style::row());
        ui.add_class(with_unit, "value-with-unit");
        with_unit
    };
    let field = ui.text_field(within, "");
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    if !unit.is_empty() {
        let unit = ui.add_label(within, unit);
        ui.add_class(unit, "value-unit");
    }
    field
}

/// Shows `text` in a field the user is not typing in.
fn write(ui: &mut Ui, field: NodeId, text: &str) {
    if ui.focused() != Some(field) && ui.text(field) != text {
        ui.set_text(field, text);
    }
}

/// What was done to a generator's rows this frame.
pub enum WorldChange {
    /// A value changed: saved as it is shown.
    Typed,
    /// Another preset picked, which reshapes the generator: saved, and a step of its own to undo.
    Preset,
    /// Open Graph pressed.
    OpenGraph,
}

/// The graphs a generator can start from, made once.
pub(crate) fn starter_graphs() -> &'static [Starter] {
    static STARTERS: OnceLock<Vec<Starter>> = OnceLock::new();
    STARTERS.get_or_init(starters)
}

/// What a generator's Preset row picks among. A preset takes the place of the whole shape, so the
/// row is there only while the shape is a preset's still.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presets {
    /// Its graph is a starter's still, whatever its rows are set to: the starters.
    Starters,
    /// Its graph is one landmass and nothing more, as a file from before graphs is: a landmass's
    /// presets, exact as they were, and Custom for a shape of its own.
    Landmass,
    /// A graph of its own: no row.
    None,
}

/// What a generator's Preset row picks among, and which of them the generator is now.
fn presets_of(world: &WorldGen) -> (Presets, usize) {
    if let Some(land) = world.as_landmass() {
        let picked = land.preset().and_then(|preset| Preset::ALL.iter().position(|&other| other == preset));
        return (Presets::Landmass, picked.unwrap_or(Preset::ALL.len()));
    }
    match starter_graphs().iter().position(|starter| starter.shapes(world.graph())) {
        Some(picked) => (Presets::Starters, picked),
        None => (Presets::None, 0),
    }
}

/// What a generator's rows were built for: what its preset is picked among, and the values it
/// exposes.
#[derive(Clone, Debug, PartialEq)]
struct Built {
    presets: Presets,
    exposed: Vec<Exposed>,
}

/// A generator's own rows, and the generator as they show it: its preset (while its graph is a
/// starter's or a single landmass), seed, scale and sea level, the values its graph exposes, and
/// Open Graph.
pub struct WorldFields {
    pub world: WorldGen,
    /// Where its rows are, to build them again in once they are other rows.
    rows: NodeId,
    built: Built,
    /// The Preset row's dropdown, and which preset it shows.
    preset: Option<NodeId>,
    picked: usize,
    seed: ValueRow,
    scale: ValueRow,
    sea_level: ValueRow,
    /// Each exposed row, by its place among the graph's.
    exposed: Vec<(usize, ValueRow)>,
    open: NodeId,
}

impl WorldFields {
    pub fn new(ui: &mut Ui, root: NodeId, world: WorldGen) -> Self {
        let rows = part(ui, root, "World Generator", "sprout");
        Self::build(ui, rows, world)
    }

    /// The rows of `world` in `rows`, in place of what it held.
    fn build(ui: &mut Ui, rows: NodeId, world: WorldGen) -> Self {
        for child in ui.children(rows) {
            ui.remove(child);
        }
        let (presets, picked) = presets_of(&world);
        let names: Vec<&str> = match presets {
            Presets::Starters => starter_graphs().iter().map(|starter| starter.name).collect(),
            Presets::Landmass => Preset::ALL.iter().map(|preset| preset.name()).chain(["Custom"]).collect(),
            Presets::None => Vec::new(),
        };
        let preset = (presets != Presets::None).then(|| {
            let row = prop(ui, rows, "Preset");
            let dropdown = ui.dropdown(row, &names, picked);
            ui.add_class(dropdown, "value-choice");
            dropdown
        });
        let seed = ValueRow::new(ui, rows, SEED.label, SEED, Value::Whole(world.seed()));
        let scale = ValueRow::new(ui, rows, SCALE.label, SCALE, Value::Number(world.scale()));
        let sea_level = ValueRow::new(ui, rows, SEA_LEVEL.label, SEA_LEVEL, Value::Number(world.sea_level()));
        let graph = world.graph();
        let mut exposed = Vec::new();
        for (at, shown) in graph.exposed.iter().enumerate() {
            let spec = graph.node(shown.node).and_then(|node| node.kind.values().get(shown.value));
            let (Some(spec), Some(value)) = (spec, graph.exposed_value(at)) else { continue };
            exposed.push((at, ValueRow::new(ui, rows, &shown.label, *spec, value)));
        }
        let open = ui.button(rows, "Open Graph");
        ui.add_class(open, "add-component");
        ui.add_class(open, "open-graph");
        let built = Built { presets, exposed: graph.exposed.clone() };
        Self { world, rows, built, preset, picked, seed, scale, sea_level, exposed, open }
    }

    /// What was done this frame, the generator changed to match; its rows then show it as it is.
    pub fn update(&mut self, ui: &mut Ui) -> Option<WorldChange> {
        if ui.clicked(self.open) {
            return Some(WorldChange::OpenGraph);
        }
        let mut changed = false;
        let mut reshaped = false;
        if let Some(dropdown) = self.preset.filter(|&dropdown| ui.changed(dropdown))
            && let Some(world) = self.with_preset(ui.selected_option(dropdown))
        {
            (self.world, changed, reshaped) = (world, true, true);
        }
        if let Some(Value::Whole(seed)) = self.seed.changed(ui, Value::Whole(self.world.seed())) {
            (self.world, changed) = (self.world.with_seed(seed), true);
        }
        if let Some(Value::Number(scale)) = self.scale.changed(ui, Value::Number(self.world.scale())) {
            (self.world, changed) = (self.world.with_scale(scale), true);
        }
        if let Some(Value::Number(level)) = self.sea_level.changed(ui, Value::Number(self.world.sea_level())) {
            (self.world, changed) = (self.world.with_sea_level(level), true);
        }
        for (at, row) in &self.exposed {
            let Some(now) = self.world.graph().exposed_value(*at) else { continue };
            if let Some(world) = row.changed(ui, now).and_then(|value| self.world.with_exposed(*at, value)) {
                (self.world, changed) = (world, true);
            }
        }
        match changed {
            true => self.refresh(ui),
            false => self.show(ui),
        }
        changed.then_some(if reshaped { WorldChange::Preset } else { WorldChange::Typed })
    }

    /// Shows `world`, as changed elsewhere.
    pub fn follow(&mut self, ui: &mut Ui, world: &WorldGen) {
        self.world = world.clone();
        self.refresh(ui);
    }

    /// The generator shaped as the preset at `picked` in its Preset row, keeping its seed and
    /// scale: a starter's graph in place of its own, or for a landmass, the landmass's preset.
    fn with_preset(&self, picked: usize) -> Option<WorldGen> {
        match self.built.presets {
            Presets::Starters => {
                let starter = starter_graphs().get(picked)?;
                let made = starter.world(self.world.seed(), self.world.scale());
                Some(made.with_preview_area(self.world.preview_area()))
            }
            Presets::Landmass => Some(reshaped(&self.world, *Preset::ALL.get(picked)?)),
            Presets::None => None,
        }
    }

    /// Shows the generator after it changed: in the rows it has, or in new ones where its preset
    /// is picked among others now or it exposes other values.
    fn refresh(&mut self, ui: &mut Ui) {
        let (presets, picked) = presets_of(&self.world);
        if presets != self.built.presets || self.world.graph().exposed != self.built.exposed {
            *self = Self::build(ui, self.rows, self.world.clone());
            return;
        }
        self.picked = picked;
        self.show(ui);
    }

    fn show(&self, ui: &mut Ui) {
        if let Some(dropdown) = self.preset
            && ui.selected_option(dropdown) != self.picked
        {
            ui.set_selected_option(dropdown, self.picked);
        }
        self.seed.show(ui, Value::Whole(self.world.seed()));
        self.scale.show(ui, Value::Number(self.world.scale()));
        self.sea_level.show(ui, Value::Number(self.world.sea_level()));
        for (at, row) in &self.exposed {
            if let Some(value) = self.world.graph().exposed_value(*at) {
                row.show(ui, value);
            }
        }
    }
}

/// `world` shaped as `preset`: the landmass its ground is and the climate beside it become the
/// preset's, and its sea level, while its seed, its scale and the rest of its graph stay.
fn reshaped(world: &WorldGen, preset: Preset) -> WorldGen {
    let land = preset.landmass();
    let mut graph = world.graph().clone();
    let read_from = |input: usize| {
        let output = world.graph().output()?.id;
        world.graph().link_into(output, input).map(|link| link.from)
    };
    if let Some(node) = read_from(Output::HEIGHT).and_then(|id| graph.node_mut(id)) {
        node.kind = NodeKind::Landmass(Land::from(&land));
    }
    if let Some(node) = read_from(Output::TEMPERATURE).and_then(|id| graph.node_mut(id)) {
        node.kind = NodeKind::Climate(land.climate);
    }
    world.with_sea_level(land.sea_level).with_graph(graph)
}

/// What a node's rows change.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeEdit {
    /// One of its values, by its place among them.
    Set(usize, Value),
    /// One of its values shown as a row of the generator's own, or no longer.
    Expose(usize),
    Hide(usize),
    /// The rooms a Cave Network lists. One whose prefab was just picked has no shapes yet: whoever
    /// takes the edit reads them from the prefab.
    Rooms(Vec<Room>),
}

/// A prefab a Cave Network's room can be: the path its row shows, and the reference its file keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomChoice {
    pub path: String,
    pub reference: String,
}

/// One listed room's row: its prefab, how often it is chosen, and what takes it off the list.
struct RoomRow {
    prefab: NodeId,
    weight: NodeId,
    remove: NodeId,
}

/// A Cave Network's rooms as rows, the row that adds one, and the prefabs they pick among.
struct RoomRows {
    rows: Vec<RoomRow>,
    add: NodeId,
    choices: Vec<RoomChoice>,
}

impl RoomRows {
    fn build(ui: &mut Ui, root: NodeId, rooms: &[Room], choices: &[RoomChoice]) -> Self {
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");
        let list = part(ui, root, "Rooms", "box");
        let mut names: Vec<&str> = vec!["None"];
        names.extend(choices.iter().map(|choice| choice.path.as_str()));
        let mut rows = Vec::new();
        for room in rooms {
            let row = ui.panel(list, Style::row());
            ui.add_class(row, "room-row");
            // A prefab the project no longer has shows as missing, and keeps its reference until another is picked.
            let picked = Self::picked(choices, room);
            let prefab = match picked {
                Some(picked) => ui.dropdown(row, &names, picked),
                None => {
                    let mut with_missing = names.clone();
                    with_missing.push("Missing");
                    ui.dropdown(row, &with_missing, names.len())
                }
            };
            ui.add_class(prefab, "value-choice");
            ui.add_class(prefab, "room-prefab");
            let weight = ui.text_field(row, &number(room.weight));
            ui.add_class(weight, "value-input");
            ui.add_class(weight, "number-input");
            ui.add_class(weight, "room-weight");
            ui.set_tooltip(weight, "Weight", Some("How often the room is chosen, beside the others."));
            let remove = ui.icon_button(row, "close");
            ui.add_class(remove, "room-remove");
            rows.push(RoomRow { prefab, weight, remove });
        }
        if rooms.is_empty() {
            let hint = ui.add_label(list, "A plain chamber, until a room is added.");
            ui.add_class(hint, "entities-hint");
        }
        let mut adding: Vec<&str> = vec!["Add Room"];
        adding.extend(choices.iter().map(|choice| choice.path.as_str()));
        let add = ui.dropdown(list, &adding, 0);
        ui.add_class(add, "value-choice");
        ui.add_class(add, "room-add");
        Self { rows, add, choices: choices.to_vec() }
    }

    /// Which of a dropdown's options `room`'s prefab is: 0 for none, and none for one not among `choices`.
    fn picked(choices: &[RoomChoice], room: &Room) -> Option<usize> {
        let Some(reference) = room.prefab.as_deref() else { return Some(0) };
        choices.iter().position(|choice| choice.reference == reference || choice.path == reference).map(|at| at + 1)
    }

    /// The rooms as the user left them this frame, where they changed `rooms`.
    fn changed(&self, ui: &mut Ui, rooms: &[Room]) -> Option<Vec<Room>> {
        let mut now = rooms.to_vec();
        if ui.changed(self.add) {
            let picked = ui.selected_option(self.add);
            ui.set_selected_option(self.add, 0);
            let choice = self.choices.get(picked.checked_sub(1)?)?;
            now.push(Room { prefab: Some(choice.reference.clone()), ..Room::default() });
            return Some(now);
        }
        for (at, row) in self.rows.iter().enumerate().take(now.len()) {
            if ui.clicked(row.remove) {
                now.remove(at);
                return Some(now);
            }
            if ui.changed(row.prefab) {
                let picked = ui.selected_option(row.prefab);
                let chosen = |at: usize| self.choices.get(at).map(|choice| choice.reference.clone());
                let prefab = picked.checked_sub(1).map(chosen);
                // The last option of a row whose prefab is missing is that prefab still.
                let Some(prefab) = prefab.map_or(Some(None), |found| found.map(Some)) else { continue };
                now[at] = Room { prefab, weight: now[at].weight, ..Room::default() };
                return Some(now);
            }
            if ui.changed(row.weight) {
                let Ok(weight) = ui.text(row.weight).trim().parse::<f32>() else { continue };
                if weight.is_finite() && weight.max(0.0) != now[at].weight {
                    now[at].weight = weight.max(0.0);
                    return Some(now);
                }
            } else {
                write(ui, row.weight, &number(now[at].weight));
            }
        }
        None
    }
}

/// What a node's rows were built for: what it is, which of its inputs have
/// wires, and which of its values are exposed.
#[derive(Clone, Debug, PartialEq)]
struct NodeBuilt {
    title: &'static str,
    values: usize,
    wired: Vec<bool>,
    exposed: Vec<usize>,
    /// The prefabs of the rooms a Cave Network lists.
    rooms: Option<Vec<Option<String>>>,
}

impl NodeBuilt {
    fn of(world: &WorldGen, node: GraphNode) -> Option<Self> {
        let graph = world.graph();
        let kind = &graph.node(node)?.kind;
        let wired = (0..kind.inputs().len()).map(|input| graph.link_into(node, input).is_some()).collect();
        let exposed = graph.exposed.iter().filter(|shown| shown.node == node).map(|shown| shown.value).collect();
        let rooms = kind.rooms().map(|rooms| rooms.iter().map(|room| room.prefab.clone()).collect());
        Some(Self { title: kind.title(), values: kind.values().len(), wired, exposed, rooms })
    }
}

/// A node of a generator's graph in the Inspector: a row for each of its
/// values, and the menu that exposes one.
pub struct NodeView {
    /// The generator's file, and the node in its graph.
    path: String,
    node: GraphNode,
    /// Each row, by its value's place among the node's.
    rows: Vec<(usize, ValueRow)>,
    built: NodeBuilt,
    /// The row whose menu is open: its label, and its value.
    menu: Option<(NodeId, usize)>,
    /// A Cave Network's rooms.
    rooms: Option<RoomRows>,
}

impl NodeView {
    /// The rows of node `node` of `world`, the generator at `path`, built in `root` in place of
    /// what it held; none where the graph has no such node.
    pub fn build(
        ui: &mut Ui,
        root: NodeId,
        path: &str,
        world: &WorldGen,
        node: GraphNode,
        prefabs: &[RoomChoice],
    ) -> Option<Self> {
        let built = NodeBuilt::of(world, node)?;
        let kind = &world.graph().node(node)?.kind;
        for child in ui.children(root) {
            ui.remove(child);
        }
        let header = ui.panel(root, Style::column());
        ui.add_class(header, "inspector-header");
        let top = ui.panel(header, Style::row());
        ui.add_class(top, "header-row");
        let tile = ui.panel(top, Style::row());
        ui.add_class(tile, "obj-tile");
        ui.icon(tile, "sprout");
        let title = ui.add_label(top, kind.title());
        ui.add_class(title, "asset-name");
        let file = path.rsplit('/').next().unwrap_or(path);
        let from = ui.add_label(top, file.strip_suffix(".worldgen").unwrap_or(file));
        ui.add_class(from, "data-type");
        ui.set_tooltip(from, path, None);

        let rows = part(ui, root, kind.title(), "settings");
        let shows = |spec: &ValueSpec| spec.input.is_none_or(|input| !built.wired.get(input).copied().unwrap_or(false));
        let specs: Vec<(usize, ValueSpec)> =
            kind.values().iter().copied().enumerate().filter(|(_, spec)| shows(spec)).collect();
        let mut made = Vec::new();
        let mut add = |ui: &mut Ui, rows: NodeId, at: usize, spec: ValueSpec| {
            let Some(value) = kind.value(at) else { return };
            let row = ValueRow::new(ui, rows, spec.label, spec, value);
            row.set_exposed(ui, built.exposed.contains(&at));
            made.push((at, row));
        };
        for &(at, spec) in specs.iter().filter(|(_, spec)| !spec.advanced) {
            add(ui, rows, at, spec);
        }
        if specs.is_empty() {
            let hint = ui.add_label(rows, "This node has no values of its own.");
            ui.add_class(hint, "entities-hint");
        }
        if specs.iter().any(|(_, spec)| spec.advanced) {
            let line = ui.panel(root, Style::default());
            ui.add_class(line, "divider");
            let foldout = ui.foldout(root, "Advanced", false);
            ui.add_class(foldout, "section");
            ui.set_foldout_icon(foldout, "settings");
            let advanced = ui.foldout_content(foldout).expect("a foldout has content");
            for &(at, spec) in specs.iter().filter(|(_, spec)| spec.advanced) {
                add(ui, advanced, at, spec);
            }
        }
        let rooms = kind.rooms().map(|rooms| RoomRows::build(ui, root, rooms, prefabs));
        Some(Self { path: path.to_string(), node, rows: made, built, menu: None, rooms })
    }

    /// The generator's file and the node shown.
    pub fn shown(&self) -> (&str, GraphNode) {
        (&self.path, self.node)
    }

    /// Whether its rows are no longer the ones `world` gives the node: it is gone, another kind,
    /// wired or exposed otherwise.
    pub fn stale(&self, world: &WorldGen) -> bool {
        NodeBuilt::of(world, self.node).as_ref() != Some(&self.built)
    }

    /// What the user did to the node's rows this frame, and the rows showing the node as `world`
    /// has it.
    pub fn update(&mut self, ui: &mut Ui, world: &WorldGen) -> Option<NodeEdit> {
        let kind = &world.graph().node(self.node)?.kind;
        if let Some((owner, value)) = self.menu
            && let Some(choice) = ui.menu_choice(owner)
        {
            self.menu = None;
            let exposed = self.built.exposed.contains(&value);
            return (choice == 0).then_some(if exposed { NodeEdit::Hide(value) } else { NodeEdit::Expose(value) });
        }
        if let Some((at, row)) = self.rows.iter().find(|(_, row)| row.menu_asked(ui)) {
            let label = if self.built.exposed.contains(at) { "Stop Exposing" } else { "Expose" };
            let place = ui.pointer_position().unwrap_or_default();
            ui.open_menu(row.label, place, &[MenuItem::new(label)]);
            self.menu = Some((row.label, *at));
        }
        let mut edit = None;
        for (at, row) in &self.rows {
            let Some(now) = kind.value(*at) else { continue };
            match row.changed(ui, now) {
                Some(value) if edit.is_none() => edit = Some(NodeEdit::Set(*at, value)),
                _ => row.show(ui, now),
            }
        }
        if let (Some(rows), Some(rooms)) = (&self.rooms, kind.rooms())
            && let Some(rooms) = rows.changed(ui, rooms)
        {
            edit = edit.or(Some(NodeEdit::Rooms(rooms)));
        }
        edit
    }
}

#[cfg(test)]
mod tests {
    use voxear_ui::{Key, Modifiers};

    use super::*;

    /// A UI with somewhere to build rows.
    fn rows() -> (Ui, NodeId) {
        let mut ui = Ui::new();
        let root = ui.root();
        let rows = ui.panel(root, Style::column().at(10.0, 10.0).width(300.0));
        (ui, rows)
    }

    fn field(row: &ValueRow) -> NodeId {
        match row.control {
            RowControl::Number(field) => field,
            _ => panic!("a number's row"),
        }
    }

    /// Types `text` over what a row's field holds, as a user does, in a frame of its own.
    fn type_in(ui: &mut Ui, row: &ValueRow, text: &str) {
        ui.frame(800, 600, 1.0);
        ui.focus(field(row));
        ui.set_modifiers(Modifiers { shift: false, control: true });
        ui.key_down(Key::Char('a'));
        ui.set_modifiers(Modifiers::default());
        ui.text_input(text);
    }

    #[test]
    fn a_row_shows_its_value_and_keeps_what_is_typed_as_its_kind_keeps_it() {
        let (mut ui, rows) = rows();
        // A number, held within its range.
        let sharpness = own("sharpness", "Sharpness", ValueKind::Number { low: 0.0, high: 1.0, unit: "" });
        let row = ValueRow::new(&mut ui, rows, "Sharpness", sharpness, Value::Number(0.25));
        assert_eq!(ui.text(ui.children(row.row)[0]), "Sharpness");
        assert_eq!(ui.text(field(&row)), "0.25");
        assert_eq!(row.changed(&ui, Value::Number(0.25)), None, "nothing typed yet");
        type_in(&mut ui, &row, "0.6");
        assert_eq!(row.changed(&ui, Value::Number(0.25)), Some(Value::Number(0.6)));
        type_in(&mut ui, &row, "7");
        assert_eq!(row.changed(&ui, Value::Number(0.6)), Some(Value::Number(1.0)), "held to its most");
        type_in(&mut ui, &row, "1");
        assert_eq!(row.changed(&ui, Value::Number(1.0)), None, "what it already is, is no change");
        type_in(&mut ui, &row, "steep");
        assert_eq!(row.changed(&ui, Value::Number(1.0)), None, "nor is what is no number");
        // What is being typed is left alone; once the field is left, it shows the value again.
        row.show(&mut ui, Value::Number(1.0));
        assert_eq!(ui.text(field(&row)), "steep");
        ui.blur();
        row.show(&mut ui, Value::Number(1.0));
        assert_eq!(ui.text(field(&row)), "1");

        // A number with a unit has it after its field.
        let row = ValueRow::new(&mut ui, rows, SEA_LEVEL.label, SEA_LEVEL, Value::Number(-12.5));
        assert_eq!(ui.text(field(&row)), "-12.5");
        let beside = ui.children(ui.parent(field(&row)).unwrap());
        assert_eq!(beside.last().map(|&unit| ui.text(unit)).as_deref(), Some("m"));

        // A whole number keeps every digit of a large one, and the whole part of one with a fraction.
        let row = ValueRow::new(&mut ui, rows, SEED.label, SEED, Value::Whole(7));
        assert_eq!(ui.text(field(&row)), "7");
        type_in(&mut ui, &row, "4000000001");
        assert_eq!(row.changed(&ui, Value::Whole(7)), Some(Value::Whole(4_000_000_001)));
        type_in(&mut ui, &row, "12.7");
        assert_eq!(row.changed(&ui, Value::Whole(7)), Some(Value::Whole(12)));
        ui.blur();
        row.show(&mut ui, Value::Whole(4_000_000_001));
        assert_eq!(ui.text(field(&row)), "4000000001");

        // A choice shows the one picked, and follows it as it changes elsewhere.
        let style = own("style", "Style", ValueKind::Choice(&["Fractal", "Ridged", "Billow"]));
        let row = ValueRow::new(&mut ui, rows, "Style", style, Value::Choice(1));
        let RowControl::Choice(dropdown) = row.control else { panic!("a dropdown") };
        assert_eq!(ui.selected_option(dropdown), 1);
        row.show(&mut ui, Value::Choice(2));
        assert_eq!(ui.selected_option(dropdown), 2);
        assert_eq!(row.changed(&ui, Value::Choice(2)), None);
        // And a row marked as one its generator shows says so to the sheet.
        row.set_exposed(&mut ui, true);
        assert!(ui.has_class(row.row, "exposed"));
    }

    #[test]
    fn an_angles_row_shows_degrees_and_keeps_the_slopes_rise_over_run() {
        use voxear_terrain::generate::nodes::STEEPEST;
        let (mut ui, rows) = rows();
        let steepness = own("min", "Min", ValueKind::Angle);
        let row = ValueRow::new(&mut ui, rows, "Min", steepness, Value::Number(1.0));
        assert_eq!(ui.text(field(&row)), "45", "a rise of 1 in 1");
        let beside = ui.children(ui.parent(field(&row)).unwrap());
        assert_eq!(beside.last().map(|&unit| ui.text(unit)).as_deref(), Some("°"));
        // 30° typed is kept as its tangent, and shows as 30 again.
        type_in(&mut ui, &row, "30");
        let Some(Value::Number(rise)) = row.changed(&ui, Value::Number(1.0)) else { panic!("a change") };
        assert!((rise - 0.57735).abs() < 1e-4, "{rise}");
        ui.blur();
        row.show(&mut ui, Value::Number(rise));
        assert_eq!(ui.text(field(&row)), "30");
        // Straight up and past it are the steepest there is; below level is level.
        for (typed, kept) in [("90", STEEPEST), ("120", STEEPEST), ("-5", 0.0)] {
            type_in(&mut ui, &row, typed);
            assert_eq!(row.changed(&ui, Value::Number(rise)), Some(Value::Number(kept)), "{typed}");
        }
        ui.blur();
        row.show(&mut ui, Value::Number(STEEPEST));
        assert_eq!(ui.text(field(&row)), "90");
        // The same angle typed again is no change.
        type_in(&mut ui, &row, "90");
        assert_eq!(row.changed(&ui, Value::Number(STEEPEST)), None);
    }

    #[test]
    fn a_preset_reshapes_the_landmass_and_climate_where_they_stand() {
        let mut graph = Preset::RollingHills.world().graph().clone();
        // Moved about, with a node of its own beside them that nothing reads.
        for node in &mut graph.nodes {
            node.at = [node.at[0] + 500.0, node.at[1] - 70.0];
        }
        let spare = graph.add(NodeKind::Output(Output::default()), [0.0, 900.0]);
        let world = WorldGen::new(9, 2.0, 3.0, graph.clone());
        assert_eq!((presets_of(&world), world.preset()), ((Presets::Landmass, 1), Some(Preset::RollingHills)));
        let alpine = reshaped(&world, Preset::Alpine);
        assert_eq!((alpine.preset(), alpine.seed(), alpine.scale()), (Some(Preset::Alpine), 9, 2.0));
        assert_eq!(alpine.sea_level(), Preset::Alpine.landmass().sea_level);
        let places = |graph: &voxear_terrain::generate::Graph| -> Vec<[f32; 2]> {
            graph.nodes.iter().map(|node| node.at).collect()
        };
        assert_eq!(places(alpine.graph()), places(&graph), "its nodes where they were");
        assert!(alpine.graph().node(spare).is_some(), "and the node of its own still there");
        assert_eq!(alpine.hash(), Preset::Alpine.world().with_seed(9).with_scale(2.0).hash(), "the preset's ground");
        // A shape of its own is past the presets, as Custom.
        let mut own_shape = alpine.as_landmass().unwrap();
        own_shape.sharpness = 0.33;
        assert_eq!(presets_of(&WorldGen::from(own_shape)), (Presets::Landmass, Preset::ALL.len()));
    }

    /// The labels of the rows under `node`, in order.
    fn labels(ui: &Ui, node: NodeId, found: &mut Vec<String>) {
        if ui.has_class(node, "prop") {
            found.extend(ui.children(node).first().map(|&label| ui.text(label)));
        }
        for child in ui.children(node) {
            labels(ui, child, found);
        }
    }

    #[test]
    fn a_generators_preset_is_picked_among_starters_or_for_a_landmass_among_its_own() {
        // The starters carry the presets' names, in the presets' order, and a new generator is one of them.
        let starters = starter_graphs();
        assert!(
            starters.len() == Preset::ALL.len() && starters.iter().zip(Preset::ALL).all(|(s, p)| s.name == p.name())
        );
        let new = crate::project_files::new_world();
        assert_eq!(
            (presets_of(&new), new.starter(), new.seed(), new.scale()),
            ((Presets::Starters, 1), Some("Rolling Hills"), 1, 1.0)
        );
        assert!(new.faults().is_empty() && new.as_landmass().is_none());
        // A starter is that starter still whatever its rows are set to and wherever its nodes lie.
        let canyons = starters[4].world(3, 2.0);
        let mut moved = canyons.with_exposed(0, Value::Number(90.0)).expect("its height").graph().clone();
        moved.nodes.iter_mut().for_each(|node| node.at[0] += 400.0);
        assert_eq!(presets_of(&canyons.with_graph(moved).with_sea_level(-5.0)), (Presets::Starters, 4));
        // A node more, or a value more shown as a row, and its graph is its own: no preset to pick.
        let mut grown = canyons.graph().clone();
        let spare = grown.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
        assert_eq!(presets_of(&canyons.with_graph(grown.clone())).0, Presets::None);
        assert!(grown.nodes.len() > 4 && spare.0 as usize == grown.nodes.len());
        // A file from before graphs picks among a landmass's presets, as it always did.
        assert_eq!(presets_of(&Preset::Alpine.world()), (Presets::Landmass, 2));

        // Its rows: Preset, its own three, and what its graph exposes.
        let (mut ui, root) = rows();
        let mut fields = WorldFields::new(&mut ui, root, new.clone());
        let shown = |ui: &Ui| {
            let mut found = Vec::new();
            labels(ui, root, &mut found);
            found
        };
        let rolling = ["Preset", "Seed", "Scale", "Sea level", "Height", "Gullies", "Temperature", "Humidity"];
        assert_eq!(shown(&ui), rolling);
        let dropdown = fields.preset.expect("a Preset row");
        assert_eq!(ui.selected_option(dropdown), 1);
        // Another starter picked takes the place of its graph, keeping its seed and scale; its rows are that
        // starter's.
        let kept = new.with_seed(8).with_scale(1.5);
        fields.follow(&mut ui, &kept);
        let picked = fields.with_preset(4).expect("Canyons");
        assert_eq!((picked.starter(), picked.seed(), picked.scale()), (Some("Canyons"), 8, 1.5));
        assert_eq!(picked.sea_level(), starters[4].sea_level);
        fields.follow(&mut ui, &picked);
        assert_eq!(
            shown(&ui),
            ["Preset", "Seed", "Scale", "Sea level", "Height", "Ravines", "Temperature", "Humidity"]
        );
        assert_eq!(fields.preset.map(|dropdown| ui.selected_option(dropdown)), Some(4));
        // A graph of its own has no Preset row; a landmass has a landmass's presets, and Custom after them.
        fields.follow(&mut ui, &canyons.with_graph(grown));
        assert_eq!(shown(&ui)[..2], ["Seed", "Scale"]);
        assert!(fields.preset.is_none() && fields.with_preset(0).is_none());
        fields.follow(&mut ui, &Preset::Alpine.world());
        assert_eq!(shown(&ui), ["Preset", "Seed", "Scale", "Sea level", "Height", "Temperature", "Humidity"]);
        let alpine = fields.with_preset(4).expect("a landmass's Canyons");
        assert_eq!((alpine.preset(), alpine.starter()), (Some(Preset::Canyons), None), "a landmass still");
        assert!(fields.with_preset(Preset::ALL.len()).is_none(), "Custom is nothing to pick");
    }
}
