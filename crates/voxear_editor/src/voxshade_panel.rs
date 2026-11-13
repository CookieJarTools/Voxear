//! The VoxShade panel: a shader made of nodes (`voxear_voxshade`),
//! to build and edit by hand. It opens from a double click on a
//! `.voxshade` in the Project, or from Open in its Inspector.
//!
//! The graph is on the canvas the generator graphs use (`graph_canvas`):
//! nodes with their inputs down the left and outputs down the right, wires
//! coloured by what they carry. The graph is compiled at every change, and
//! each node shows what it costs once compiled, in instructions and
//! pictures read, so a node whose work was folded away or shared shows
//! nothing; a node something is wrong with says what. The bar over it adds
//! up each stage.
//!
//! Beside the canvas: the picked node's values (what it does, and the
//! numbers each input it has no wire into takes), the shader's properties,
//! which each material using it sets, and the shader's own settings. Every
//! change is a step to undo, changes close together to one value one step,
//! and the file is saved once nothing is held, which compiles the shader
//! into the renderer's again.

use std::time::{Duration, Instant};

use voxear_ui::{Color, NodeId, Style, Ui};
use voxear_voxshade::{
    CompareOp, Compiled, Component, Graph, Input, Link, MathOp, Node, NodeId as GraphNode, NodeKind, PortTy,
    PropertyId, PropertyKind, Stage, Ty, compile,
};

use crate::{
    asset_inspector::part,
    graph_canvas::{
        CanvasEvent, CanvasGraph, CanvasKey, CanvasNode, CanvasPort, CanvasWire, GraphCanvas, PortRef, node_size,
    },
    inspector::{prop, read_numbers, write_numbers},
};

/// Changes to the same thing closer together than this are one step to undo.
const MERGE_WITHIN: Duration = Duration::from_millis(800);
/// The most steps kept to undo.
const MOST_STEPS: usize = 200;
/// How far from the nodes it copies a duplicate is put.
const BESIDE: f32 = 30.0;

/// What a change is to, for changes close together to merge into one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKey {
    Move,
    Row(Field),
}

/// The graphs an open one has been, to go back to and forward again.
#[derive(Default)]
struct History {
    done: Vec<Graph>,
    undone: Vec<Graph>,
    last: Option<(EditKey, Instant)>,
}

impl History {
    fn record(&mut self, before: &Graph, key: Option<EditKey>) {
        let merges =
            matches!((key, self.last), (Some(key), Some((last, at))) if key == last && at.elapsed() < MERGE_WITHIN);
        if !merges {
            self.done.push(before.clone());
            if self.done.len() > MOST_STEPS {
                self.done.remove(0);
            }
        }
        self.undone.clear();
        self.last = key.map(|key| (key, Instant::now()));
    }

    fn seal(&mut self) {
        self.last = None;
    }

    fn undo(&mut self, now: &Graph) -> Option<Graph> {
        let back = self.done.pop()?;
        self.undone.push(now.clone());
        self.last = None;
        Some(back)
    }

    fn redo(&mut self, now: &Graph) -> Option<Graph> {
        let again = self.undone.pop()?;
        self.done.push(now.clone());
        self.last = None;
        Some(again)
    }
}

/// Nodes copied, with the wires among them.
#[derive(Clone, Debug, Default)]
struct Copied {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

impl Copied {
    fn of(graph: &Graph, ids: &[u64]) -> Self {
        let has = |id: GraphNode| ids.contains(&u64::from(id.0));
        Self {
            nodes: graph.nodes.iter().filter(|node| has(node.id)).cloned().collect(),
            links: graph.links.iter().filter(|link| has(link.from) && has(link.to)).copied().collect(),
        }
    }

    /// Adds them to `graph`, each moved by `by`, wired as they were: the nodes made.
    fn paste(&self, graph: &mut Graph, by: [f32; 2]) -> Vec<GraphNode> {
        let made: Vec<(GraphNode, GraphNode)> = self
            .nodes
            .iter()
            .map(|node| {
                let made = graph.add(node.kind.clone(), [node.at[0] + by[0], node.at[1] + by[1]]);
                for literal in &node.literals {
                    graph.set_literal(made, literal.input, literal.value);
                }
                (node.id, made)
            })
            .collect();
        let now = |old: GraphNode| made.iter().find(|(from, _)| *from == old).map(|(_, to)| *to);
        for link in &self.links {
            if let (Some(from), Some(to)) = (now(link.from), now(link.to)) {
                graph.link(from, link.output, to, link.input);
            }
        }
        made.into_iter().map(|(_, to)| to).collect()
    }
}

/// What one of the side's rows edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Input,
    Op,
    Compare,
    ConstantTy,
    Constant,
    Color,
    CombineTy,
    PropertyPick,
    Swizzle(usize),
    Literal(usize),
    PropertyName(PropertyId),
    PropertyKind(PropertyId),
    PropertyDefault(PropertyId),
    PropertyRange(PropertyId),
    PropertyTooltip(PropertyId),
    RemoveProperty(PropertyId),
    AddProperty,
    Name,
    Unlit,
    MaxOffset,
}

/// The control a row edits its field with.
#[derive(Clone, Debug)]
enum Control {
    Choice(NodeId),
    Numbers(Vec<NodeId>),
    Text(NodeId),
    Toggle(NodeId),
    Button(NodeId),
}

/// The graph that is open.
struct Open {
    path: String,
    graph: Graph,
    history: History,
    unsaved: bool,
    /// The graph compiled as it now is, or what stops it.
    compiled: Result<Compiled, Vec<voxear_voxshade::CompileError>>,
    drawn: CanvasGraph,
    stale: bool,
    /// Where the node the search adds goes, and the wire it goes on the end of.
    adding: Option<([f32; 2], Option<PortRef>)>,
}

/// What the panel asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum VoxShadeEvent {
    /// Save the graph at this path as it now is, and compile it into the renderer's shaders.
    Save(String, Graph),
    /// Open the search for a node to add, under this element.
    Search(NodeId),
}

/// A node the search offers: its label, its folder, and the node it adds.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub label: String,
    pub folder: &'static str,
    pub kind: NodeKind,
}

pub struct VoxShadePanel {
    hint: NodeId,
    body: NodeId,
    name: NodeId,
    add: NodeId,
    frame: NodeId,
    zoom: NodeId,
    costs: NodeId,
    canvas: GraphCanvas,
    /// The side's three parts' rows, built again when what they show changes shape.
    node_rows: NodeId,
    property_rows: NodeId,
    shader_rows: NodeId,
    rows: Vec<(Field, Control)>,
    /// What the rows were built for, to build them again when that changes.
    shape: String,
    /// The properties a Property or Sample Picture node's choice lists, in order.
    choosable: Vec<PropertyId>,
    open: Option<Open>,
    copied: Copied,
    /// What the search offered last, which its pick names by place.
    offered: Vec<Offer>,
    active: bool,
    hold_active: bool,
    showing: Option<bool>,
    pending: Vec<VoxShadeEvent>,
}

impl VoxShadePanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let hint = ui.add_label(root, "Double-click a VoxShade in the Project to edit it.");
        ui.add_class(hint, "placeholder");
        let body = ui.panel(root, Style::column());
        ui.add_class(body, "fill");
        let bar = ui.panel(body, Style::row());
        ui.add_class(bar, "console-toolbar");
        let icon = ui.icon(bar, "nodes");
        ui.add_class(icon, "generator-icon");
        let name = ui.add_label(bar, "");
        ui.add_class(name, "generator-name");
        let button = |ui: &mut Ui, text: &str| {
            let button = ui.button(bar, text);
            ui.add_class(button, "console-button");
            button
        };
        let (add, frame) = (button(ui, "Add Node"), button(ui, "Frame"));
        let zoom = ui.add_label(bar, "");
        ui.add_class(zoom, "profiler-summary");
        let costs = ui.add_label(bar, "");
        ui.add_class(costs, "profiler-summary");
        let line = ui.panel(body, Style::default());
        ui.add_class(line, "divider");
        let split = ui.panel(body, Style::default());
        ui.add_class(split, "fill");
        ui.add_class(split, "generator-split");
        let canvas = GraphCanvas::new(ui, split);
        let side = ui.panel(split, Style::column());
        ui.add_class(side, "generator-side");
        let node_rows = part(ui, side, "Node", "nodes");
        let property_rows = part(ui, side, "Properties", "shader");
        let shader_rows = part(ui, side, "Shader", "settings");
        ui.set_visible(body, false);
        Self {
            hint,
            body,
            name,
            add,
            frame,
            zoom,
            costs,
            canvas,
            node_rows,
            property_rows,
            shader_rows,
            rows: Vec::new(),
            shape: String::new(),
            choosable: Vec::new(),
            open: None,
            copied: Copied::default(),
            offered: Vec::new(),
            active: false,
            hold_active: false,
            showing: None,
            pending: Vec::new(),
        }
    }

    /// Opens the graph at `path`: all of it in view, nothing picked, nothing to undo.
    pub fn open(&mut self, ui: &mut Ui, path: &str, graph: Graph) {
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.set_text(self.name, file.strip_suffix(".voxshade").unwrap_or(file));
        ui.set_tooltip(self.name, path, None);
        let compiled = compile(&graph);
        self.open = Some(Open {
            path: path.to_string(),
            graph,
            history: History::default(),
            unsaved: false,
            compiled,
            drawn: CanvasGraph::default(),
            stale: true,
            adding: None,
        });
        self.shape.clear();
        self.canvas.pick(Vec::new());
        self.canvas.frame_all();
        self.active = true;
    }

    /// The open graph and its file, when it has a change not yet saved: taken as saved.
    pub fn take_unsaved(&mut self) -> Option<(String, Graph)> {
        let open = self.open.as_mut().filter(|open| open.unsaved)?;
        open.unsaved = false;
        Some((open.path.clone(), open.graph.clone()))
    }

    pub fn close(&mut self) {
        self.open = None;
        self.canvas.pick(Vec::new());
    }

    pub fn path(&self) -> Option<&str> {
        self.open.as_ref().map(|open| open.path.as_str())
    }

    pub fn graph(&self) -> Option<&Graph> {
        self.open.as_ref().map(|open| &open.graph)
    }

    /// The open graph compiled, as it now is.
    pub fn compiled(&self) -> Option<&Result<Compiled, Vec<voxear_voxshade::CompileError>>> {
        self.open.as_ref().map(|open| &open.compiled)
    }

    /// The graph as the canvas draws it.
    pub fn drawn(&self) -> Option<&CanvasGraph> {
        self.open.as_ref().map(|open| &open.drawn)
    }

    pub fn moved_to(&mut self, ui: &mut Ui, path: &str) {
        if let Some(open) = self.open.as_mut() {
            open.path = path.to_string();
            let file = path.rsplit('/').next().unwrap_or(path);
            ui.set_text(self.name, file.strip_suffix(".voxshade").unwrap_or(file));
            ui.set_tooltip(self.name, path, None);
        }
    }

    pub fn picked(&self) -> Vec<GraphNode> {
        self.canvas.picked().iter().map(|&id| GraphNode(id as u32)).collect()
    }

    pub fn pick(&mut self, nodes: &[GraphNode]) {
        self.canvas.pick(nodes.iter().map(|node| u64::from(node.0)).collect());
    }

    pub fn is_active(&self) -> bool {
        self.active && self.open.is_some()
    }

    pub fn set_active(&mut self, active: bool) {
        if active || !std::mem::take(&mut self.hold_active) {
            self.active = active;
        }
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.canvas.set_dark(dark);
    }

    pub fn key(&mut self, key: CanvasKey) {
        self.canvas.key(key);
    }

    pub fn canvas(&self) -> &GraphCanvas {
        &self.canvas
    }

    /// The open graph becomes `graph`, as a step to undo.
    pub fn change(&mut self, graph: Graph) {
        self.change_by(graph, None);
    }

    fn change_by(&mut self, graph: Graph, key: Option<EditKey>) {
        let Some(open) = self.open.as_mut() else { return };
        if graph == open.graph {
            return;
        }
        open.history.record(&open.graph, key);
        (open.graph, open.unsaved, open.stale) = (graph, true, true);
    }

    /// The open graph's file was written elsewhere and now reads as `graph`.
    pub fn reread(&mut self, graph: Graph) {
        let Some(open) = self.open.as_mut() else { return };
        if graph != open.graph && !open.unsaved {
            open.history.record(&open.graph, None);
            (open.graph, open.stale) = (graph, true);
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(open) = self.open.as_mut() else { return false };
        let Some(back) = open.history.undo(&open.graph) else { return false };
        (open.graph, open.unsaved, open.stale) = (back, true, true);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(open) = self.open.as_mut() else { return false };
        let Some(again) = open.history.redo(&open.graph) else { return false };
        (open.graph, open.unsaved, open.stale) = (again, true, true);
        true
    }

    /// What the search offers to add to the open graph, kept for its pick to name by place.
    pub fn offers(&mut self) -> &[Offer] {
        self.offered = self.open.as_ref().map(|open| offers(&open.graph)).unwrap_or_default();
        &self.offered
    }

    /// Adds the node the search offered at `place`, where the search was
    /// asked for, on the end of the wire it was asked from when there was
    /// one, and picks it.
    pub fn add_offered(&mut self, place: usize) -> Option<GraphNode> {
        let kind = self.offered.get(place)?.kind.clone();
        self.add_node(kind)
    }

    pub fn add_node(&mut self, kind: NodeKind) -> Option<GraphNode> {
        let open = self.open.as_mut()?;
        let (mut at, wire) = open.adding.take().unwrap_or_default();
        let mut graph = open.graph.clone();
        let described = describe_node(&graph, &Node { id: GraphNode(0), at, kind: kind.clone(), literals: Vec::new() });
        if wire.is_some_and(|wire| !wire.output) {
            at[0] -= node_size(&described)[0];
        }
        let node = graph.add(kind, at);
        if let Some(wire) = wire {
            let other = GraphNode(wire.node as u32);
            let ports = if wire.output { described.inputs.len() } else { described.outputs.len() };
            let joined = (0..ports).find(|&port| match wire.output {
                true => allows(&graph, (other, wire.index), (node, port)),
                false => allows(&graph, (node, port), (other, wire.index)),
            });
            match (joined, wire.output) {
                (Some(port), true) => graph.link(other, wire.index, node, port),
                (Some(port), false) => graph.link(node, port, other, wire.index),
                (None, _) => {}
            }
        }
        self.change(graph);
        self.pick(&[node]);
        (self.active, self.hold_active) = (true, true);
        Some(node)
    }

    /// The panel's frame: the canvas's, the side's rows', and what they ask of the editor.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, alt: bool) -> Vec<VoxShadeEvent> {
        let mut events = std::mem::take(&mut self.pending);
        let open = self.open.is_some();
        if self.showing != Some(open) {
            self.showing = Some(open);
            ui.set_visible(self.hint, !open);
            ui.set_visible(self.body, open);
        }
        if !open {
            return events;
        }
        if shown {
            if ui.clicked(self.add) {
                self.canvas.key(CanvasKey::Search);
            }
            if ui.clicked(self.frame) {
                self.canvas.key(CanvasKey::Frame);
            }
            self.update_rows(ui);
            self.update_canvas(ui, alt, &mut events);
            self.show_toolbar(ui);
        }
        let held = ui.holds_press() || self.canvas.is_dragging();
        if let Some(open) = self.open.as_mut().filter(|open| open.unsaved && !held) {
            open.unsaved = false;
            events.push(VoxShadeEvent::Save(open.path.clone(), open.graph.clone()));
        }
        events
    }

    fn show_toolbar(&mut self, ui: &mut Ui) {
        let zoom = format!("{:.0}%", self.canvas.view().zoom() * 100.0);
        if ui.text(self.zoom) != zoom {
            ui.set_text(self.zoom, &zoom);
        }
        let Some(open) = self.open.as_ref() else { return };
        let costs = match &open.compiled {
            Ok(compiled) => summary(compiled),
            Err(errors) => format!("Does not compile: {}", errors.len()),
        };
        if ui.text(self.costs) != costs {
            ui.set_text(self.costs, &costs);
        }
    }

    /// The canvas's frame, and what was done on it carried out on the graph.
    fn update_canvas(&mut self, ui: &mut Ui, alt: bool, events: &mut Vec<VoxShadeEvent>) {
        let Some(open) = self.open.as_mut() else { return };
        if std::mem::take(&mut open.stale) {
            open.compiled = compile(&open.graph);
            open.drawn = describe(&open.graph, &open.compiled);
        }
        self.canvas.set_panning(alt);
        let graph = &open.graph;
        let allowed = |from: PortRef, to: PortRef| {
            let (out, into) = if from.output { (from, to) } else { (to, from) };
            allows(graph, (GraphNode(out.node as u32), out.index), (GraphNode(into.node as u32), into.index))
        };
        let done = self.canvas.update(ui, &open.drawn, &allowed);
        if done.is_empty() {
            return;
        }
        let mut graph = open.graph.clone();
        let mut key = None;
        let mut picked = None;
        let id = |node: u64| GraphNode(node as u32);
        for event in done {
            match event {
                CanvasEvent::Picked => self.shape.clear(),
                CanvasEvent::Moved(places) => {
                    for (node, at) in places {
                        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id(node)) {
                            node.at = at;
                        }
                    }
                    key = Some(EditKey::Move);
                }
                CanvasEvent::MoveEnded => open.history.seal(),
                CanvasEvent::Connect { from, to } => graph.link(id(from.0), from.1, id(to.0), to.1),
                CanvasEvent::Disconnect { to } => {
                    graph.links.retain(|link| !(link.to == id(to.0) && link.input == to.1))
                }
                CanvasEvent::Search { at, wire } => {
                    open.adding = Some((at, wire));
                    events.push(VoxShadeEvent::Search(self.canvas.anchor()));
                }
                CanvasEvent::Delete(nodes) => nodes.into_iter().for_each(|node| graph.remove(id(node))),
                CanvasEvent::Duplicate(nodes) => {
                    picked = Some(Copied::of(&graph, &nodes).paste(&mut graph, [BESIDE, BESIDE]));
                }
                CanvasEvent::Copy(nodes) => self.copied = Copied::of(&graph, &nodes),
                CanvasEvent::Paste(at) => {
                    let corner = self
                        .copied
                        .nodes
                        .iter()
                        .fold([f32::MAX; 2], |least, node| [least[0].min(node.at[0]), least[1].min(node.at[1])]);
                    if !self.copied.nodes.is_empty() {
                        picked = Some(self.copied.paste(&mut graph, [at[0] - corner[0], at[1] - corner[1]]));
                    }
                }
            }
        }
        if graph != open.graph {
            self.change_by(graph, key);
        }
        if let Some(made) = picked {
            self.pick(&made);
        }
    }

    /// The side's rows: built again when what they show changes shape, then
    /// what was changed in them carried out on the graph, then the graph's
    /// values shown in them.
    fn update_rows(&mut self, ui: &mut Ui) {
        let Some(graph) = self.open.as_ref().map(|open| open.graph.clone()) else { return };
        let picked = match self.canvas.picked() {
            [node] => graph.node(GraphNode(*node as u32)).cloned(),
            _ => None,
        };
        let shape = shape(&graph, picked.as_ref());
        if shape != self.shape {
            self.shape = shape;
            self.build_rows(ui, picked.as_ref());
        }
        if let Some((changed, field)) = self.read_rows(ui, &graph, picked.as_ref()) {
            let merges = !matches!(field, Field::AddProperty | Field::RemoveProperty(_));
            self.change_by(changed, merges.then_some(EditKey::Row(field)));
        }
        let Some(graph) = self.open.as_ref().map(|open| open.graph.clone()) else { return };
        let picked = picked.and_then(|node| graph.node(node.id)).cloned();
        self.show_rows(ui, &graph, picked.as_ref());
    }

    fn build_rows(&mut self, ui: &mut Ui, picked: Option<&Node>) {
        for container in [self.node_rows, self.property_rows, self.shader_rows] {
            for child in ui.children(container) {
                ui.remove(child);
            }
        }
        self.rows.clear();
        self.choosable.clear();
        let Some(graph) = self.open.as_ref().map(|open| open.graph.clone()) else { return };
        let rows = self.node_rows;
        let mut choosable = Vec::new();
        match picked {
            None => {
                let line = ui.add_label(rows, "Pick a node to set its values.");
                ui.add_class(line, "asset-line");
            }
            Some(node) => {
                let mut choice = |ui: &mut Ui, field, label: &str, options: Vec<String>| {
                    let row = prop(ui, rows, label);
                    let names: Vec<&str> = options.iter().map(String::as_str).collect();
                    let dropdown = ui.dropdown(row, &names, 0);
                    self.rows.push((field, Control::Choice(dropdown)));
                };
                match &node.kind {
                    NodeKind::Input { .. } => {
                        choice(ui, Field::Input, "Input", Input::ALL.iter().map(|i| i.name().to_string()).collect());
                    }
                    NodeKind::Math { .. } => {
                        choice(ui, Field::Op, "Does", MathOp::ALL.iter().map(|op| op.name().to_string()).collect());
                    }
                    NodeKind::Compare { .. } => {
                        let names = ["Greater", "Greater or Equal", "Less", "Less or Equal", "Equal", "Not Equal"];
                        choice(ui, Field::Compare, "Is A", names.iter().map(|n| n.to_string()).collect());
                    }
                    NodeKind::Constant { ty, .. } => {
                        choice(
                            ui,
                            Field::ConstantTy,
                            "Type",
                            NUMBER_TYS.iter().map(|t| t.name().to_string()).collect(),
                        );
                        let fields = numbers_row(ui, rows, "Value", ty.width());
                        self.rows.push((Field::Constant, Control::Numbers(fields)));
                    }
                    NodeKind::Color { .. } => {
                        let fields = letters_row(ui, rows, "Color", &["r", "g", "b", "a"]);
                        self.rows.push((Field::Color, Control::Numbers(fields)));
                    }
                    NodeKind::Combine { .. } => {
                        let tys = &NUMBER_TYS[1..];
                        choice(ui, Field::CombineTy, "Makes", tys.iter().map(|t| t.name().to_string()).collect());
                    }
                    NodeKind::Property { .. } | NodeKind::SamplePicture { .. } => {
                        let pictures = matches!(node.kind, NodeKind::SamplePicture { .. });
                        let listed: Vec<&voxear_voxshade::Property> =
                            graph.properties.iter().filter(|p| (p.kind == PropertyKind::Picture) == pictures).collect();
                        choosable = listed.iter().map(|p| p.id).collect();
                        let names = listed.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
                        let names = if names.is_empty() { vec!["None yet: add one below".to_string()] } else { names };
                        choice(ui, Field::PropertyPick, if pictures { "Picture" } else { "Property" }, names);
                    }
                    NodeKind::Swizzle { .. } => {
                        for slot in 0..4 {
                            let row = prop(ui, rows, ["First", "Second", "Third", "Fourth"][slot]);
                            let options = ["None", "X", "Y", "Z", "W"];
                            let dropdown = ui.dropdown(row, &options[usize::from(slot == 0)..], 0);
                            self.rows.push((Field::Swizzle(slot), Control::Choice(dropdown)));
                        }
                    }
                    _ => {}
                }
                // The numbers each input with no wire into it takes.
                for (index, port) in node.kind.inputs().iter().enumerate() {
                    if graph.link_into(node.id, index).is_some() {
                        continue;
                    }
                    let width = match port.ty {
                        PortTy::Fixed(ty) => ty.width(),
                        PortTy::Any => 1,
                    };
                    let fields = numbers_row(ui, rows, port.label, width);
                    // An output's input with nothing set takes the material's own, which it says.
                    if node.kind.output_stage().is_some() {
                        for &field in &fields {
                            ui.set_placeholder(field, "Material's");
                        }
                        ui.set_tooltip(fields[0], "Empty to keep the material's own", None);
                    }
                    self.rows.push((Field::Literal(index), Control::Numbers(fields)));
                }
            }
        }
        self.choosable = choosable;
        // The properties, each a few rows under its name.
        for property in &graph.properties {
            let rows = self.property_rows;
            let head = ui.panel(rows, Style::row());
            ui.add_class(head, "prop");
            let title = ui.add_label(head, &property.name);
            ui.add_class(title, "prop-label");
            let remove = ui.icon_button(head, "close");
            ui.set_tooltip(remove, "Remove this property", None);
            self.rows.push((Field::RemoveProperty(property.id), Control::Button(remove)));
            let row = prop(ui, rows, "Name");
            let name = ui.text_field(row, &property.name);
            self.rows.push((Field::PropertyName(property.id), Control::Text(name)));
            let row = prop(ui, rows, "Kind");
            let names: Vec<&str> = PropertyKind::ALL.iter().map(|kind| kind.name()).collect();
            let at = PropertyKind::ALL.iter().position(|kind| *kind == property.kind).unwrap_or(0);
            let kind = ui.dropdown(row, &names, at);
            self.rows.push((Field::PropertyKind(property.id), Control::Choice(kind)));
            if let Some(ty) = property.kind.ty() {
                let fields = numbers_row(ui, rows, "Default", ty.width());
                self.rows.push((Field::PropertyDefault(property.id), Control::Numbers(fields)));
            }
            if property.kind == PropertyKind::Number {
                let fields = numbers_row(ui, rows, "Range", 2);
                ui.set_tooltip(fields[0], "The least and most a material may set it to; the same for no limits", None);
                self.rows.push((Field::PropertyRange(property.id), Control::Numbers(fields)));
            }
            let row = prop(ui, rows, "Tooltip");
            let tooltip = ui.text_field(row, &property.tooltip);
            self.rows.push((Field::PropertyTooltip(property.id), Control::Text(tooltip)));
        }
        let add = ui.button(self.property_rows, "Add Property");
        ui.add_class(add, "add-component");
        self.rows.push((Field::AddProperty, Control::Button(add)));
        // The shader's own settings.
        let rows = self.shader_rows;
        let row = prop(ui, rows, "Name");
        let name = ui.text_field(row, &graph.name);
        ui.set_tooltip(name, "The name its Slang gives its properties: letters, digits and _", None);
        self.rows.push((Field::Name, Control::Text(name)));
        let row = prop(ui, rows, "Unlit");
        let unlit = ui.toggle(row, "", graph.unlit);
        self.rows.push((Field::Unlit, Control::Toggle(unlit)));
        let fields = numbers_row(ui, rows, "Max Offset", 1);
        ui.set_tooltip(fields[0], "How far its Vertex Output moves a vertex at most, in metres", None);
        self.rows.push((Field::MaxOffset, Control::Numbers(fields)));
    }

    /// The graph as the rows changed it this frame, and which row; none when no row changed.
    fn read_rows(&self, ui: &Ui, graph: &Graph, picked: Option<&Node>) -> Option<(Graph, Field)> {
        for (field, control) in &self.rows {
            let mut changed = graph.clone();
            // The node changed on its own, then put back into the graph.
            let mut node = picked.cloned();
            let done = match (field, control) {
                (Field::AddProperty, Control::Button(button)) if ui.clicked(*button) => {
                    let taken = |name: &str| changed.properties.iter().any(|p| p.name == name);
                    let name = (1..).map(|n| format!("property{n}")).find(|name| !taken(name)).unwrap_or_default();
                    changed.add_property(&name, PropertyKind::Number, [0.0; 4]);
                    true
                }
                (Field::RemoveProperty(id), Control::Button(button)) if ui.clicked(*button) => {
                    changed.properties.retain(|p| p.id != *id);
                    true
                }
                (_, Control::Button(_)) => false,
                (_, Control::Choice(dropdown)) if ui.changed(*dropdown) => {
                    let picked = ui.selected_option(*dropdown);
                    choose(&mut changed, node.as_mut(), *field, picked, &self.choosable)
                }
                (_, Control::Choice(_)) => false,
                (_, Control::Toggle(toggle)) if ui.changed(*toggle) => {
                    changed.unlit = ui.is_checked(*toggle);
                    true
                }
                (_, Control::Toggle(_)) => false,
                (_, Control::Text(text)) if ui.changed(*text) => {
                    let typed = ui.text(*text).trim().to_string();
                    match field {
                        Field::Name => {
                            changed.name = typed;
                            voxear_voxshade::graph::is_identifier(&changed.name)
                        }
                        Field::PropertyName(id) => {
                            let free = voxear_voxshade::graph::is_identifier(&typed)
                                && !changed.properties.iter().any(|p| p.name == typed && p.id != *id);
                            if free && let Some(p) = changed.properties.iter_mut().find(|p| p.id == *id) {
                                p.name = typed;
                            }
                            free
                        }
                        Field::PropertyTooltip(id) => {
                            if let Some(p) = changed.properties.iter_mut().find(|p| p.id == *id) {
                                p.tooltip = typed;
                            }
                            true
                        }
                        _ => false,
                    }
                }
                (_, Control::Text(_)) => false,
                // An output's value emptied goes back to the material's own.
                (Field::Literal(input), Control::Numbers(fields))
                    if node.as_ref().is_some_and(|n| n.kind.output_stage().is_some())
                        && fields.iter().any(|f| ui.changed(*f))
                        && fields.iter().all(|f| ui.text(*f).trim().is_empty()) =>
                {
                    let had = node.as_ref().is_some_and(|n| n.literals.iter().any(|l| l.input == *input));
                    if let Some(node) = node.as_mut() {
                        node.literals.retain(|l| l.input != *input);
                    }
                    had
                }
                (_, Control::Numbers(fields)) => {
                    let now = numbers_of(&changed, node.as_ref(), *field);
                    let read = read_numbers(ui, fields, &now);
                    read != now && set_numbers(&mut changed, node.as_mut(), *field, &read)
                }
            };
            if let Some(node) = node.filter(|_| done)
                && let Some(kept) = changed.nodes.iter_mut().find(|n| n.id == node.id)
            {
                *kept = node;
            }
            if done && changed != *graph {
                return Some((changed, *field));
            }
        }
        None
    }

    /// Shows the graph's values in the rows, but in a field being typed in.
    fn show_rows(&mut self, ui: &mut Ui, graph: &Graph, picked: Option<&Node>) {
        for (field, control) in &self.rows {
            match control {
                Control::Numbers(_) if takes_materials(picked, *field) => {}
                Control::Numbers(fields) => write_numbers(ui, fields, &numbers_of(graph, picked, *field)),
                Control::Choice(dropdown) => {
                    if let Some(at) = chosen(graph, picked, *field, &self.choosable)
                        && ui.selected_option(*dropdown) != at
                    {
                        ui.set_selected_option(*dropdown, at);
                    }
                }
                Control::Toggle(toggle) => {
                    if ui.is_checked(*toggle) != graph.unlit {
                        ui.set_checked(*toggle, graph.unlit);
                    }
                }
                Control::Text(text) if ui.focused() != Some(*text) => {
                    let shown = match field {
                        Field::Name => Some(graph.name.clone()),
                        Field::PropertyName(id) => graph.property(*id).map(|p| p.name.clone()),
                        Field::PropertyTooltip(id) => graph.property(*id).map(|p| p.tooltip.clone()),
                        _ => None,
                    };
                    if let Some(shown) = shown.filter(|shown| ui.text(*text) != *shown) {
                        ui.set_text(*text, &shown);
                    }
                }
                _ => {}
            }
        }
    }

    /// A point on a node's header in the window, and one on a port's dot: for a test to press on.
    pub fn node_point(&self, ui: &Ui, node: GraphNode) -> Option<(f32, f32)> {
        self.canvas.node_point(ui, &self.open.as_ref()?.drawn, u64::from(node.0))
    }

    pub fn port_point(&self, ui: &Ui, node: GraphNode, output: bool, index: usize) -> Option<(f32, f32)> {
        let port = PortRef { node: u64::from(node.0), output, index };
        self.canvas.port_point(ui, &self.open.as_ref()?.drawn, port)
    }

    /// The field or control of the side's row for the picked node's input
    /// or setting, for a test to type in or pick from.
    pub fn row_control(&self, find: impl Fn(&str) -> bool) -> Option<NodeId> {
        self.rows.iter().find(|(field, _)| find(&format!("{field:?}"))).map(|(_, control)| match control {
            Control::Choice(node) | Control::Text(node) | Control::Toggle(node) | Control::Button(node) => *node,
            Control::Numbers(fields) => fields[0],
        })
    }
}

/// The types a Constant can be, in the order its choice lists them.
const NUMBER_TYS: [Ty; 4] = [Ty::Float, Ty::Float2, Ty::Float3, Ty::Float4];

/// A row of `width` number fields under `label`, as the Inspector's: one
/// field for a number, a field a letter for a vector's.
fn numbers_row(ui: &mut Ui, rows: NodeId, label: &str, width: usize) -> Vec<NodeId> {
    letters_row(ui, rows, label, &["x", "y", "z", "w"][..width.min(4)])
}

fn letters_row(ui: &mut Ui, rows: NodeId, label: &str, letters: &[&str]) -> Vec<NodeId> {
    let row = prop(ui, rows, label);
    if letters.len() == 1 {
        let field = ui.text_field(row, "");
        ui.add_class(field, "value-input");
        ui.add_class(field, "number-input");
        return vec![field];
    }
    let vector = ui.panel(row, Style::default());
    ui.add_class(vector, "vector");
    letters
        .iter()
        .map(|letter| {
            let cell = ui.panel(vector, Style::row());
            ui.add_class(cell, "axis-field");
            let shown = ui.add_label(cell, &letter.to_uppercase());
            ui.add_class(shown, "axis-letter");
            ui.add_class(shown, &format!("axis-{letter}"));
            let field = ui.text_field(cell, "");
            ui.add_class(field, "axis-input");
            field
        })
        .collect()
}

/// Whether `field` is an input of an output node that keeps nothing of its own, so takes the material's.
fn takes_materials(node: Option<&Node>, field: Field) -> bool {
    let Field::Literal(input) = field else { return false };
    node.is_some_and(|node| node.kind.output_stage().is_some() && !node.literals.iter().any(|l| l.input == input))
}

/// What the side's rows show, as text: when it changes, they are built again.
fn shape(graph: &Graph, picked: Option<&Node>) -> String {
    let node = picked.map(|node| {
        let wired: Vec<usize> = graph.links.iter().filter(|l| l.to == node.id).map(|l| l.input).collect();
        let kind = match &node.kind {
            NodeKind::Constant { ty, .. } => format!("constant {ty:?}"),
            NodeKind::Color { .. } => "color".into(),
            other => format!("{other:?}"),
        };
        format!("{} {kind} {wired:?}", node.id.0)
    });
    let properties: Vec<(u32, PropertyKind)> = graph.properties.iter().map(|p| (p.id.0, p.kind)).collect();
    format!("{node:?} {properties:?}")
}

/// What a numbers row shows now.
fn numbers_of(graph: &Graph, node: Option<&Node>, field: Field) -> Vec<f32> {
    let property = |id: PropertyId| graph.property(id);
    match field {
        Field::Constant => match node.map(|n| &n.kind) {
            Some(NodeKind::Constant { ty, value }) => value[..ty.width()].to_vec(),
            _ => Vec::new(),
        },
        Field::Color => match node.map(|n| &n.kind) {
            Some(NodeKind::Color { value }) => value.to_vec(),
            _ => Vec::new(),
        },
        Field::Literal(input) => {
            let Some(node) = node else { return Vec::new() };
            let port = node.kind.inputs().get(input).cloned();
            let width = match port.as_ref().map(|p| p.ty) {
                Some(PortTy::Fixed(ty)) => ty.width(),
                _ => 1,
            };
            let kept = node.literals.iter().find(|l| l.input == input).map(|l| l.value);
            kept.or(port.map(|p| p.default)).map_or(Vec::new(), |value| value[..width].to_vec())
        }
        Field::PropertyDefault(id) => {
            property(id).map_or(Vec::new(), |p| p.default[..p.kind.ty().map_or(0, Ty::width)].to_vec())
        }
        Field::PropertyRange(id) => property(id).map_or(Vec::new(), |p| p.range.unwrap_or([0.0, 0.0]).to_vec()),
        Field::MaxOffset => vec![graph.max_offset],
        _ => Vec::new(),
    }
}

/// Sets what a numbers row edits to `numbers`; false when it edits nothing there is.
fn set_numbers(graph: &mut Graph, node: Option<&mut Node>, field: Field, numbers: &[f32]) -> bool {
    let fill = |numbers: &[f32]| {
        let mut value = [0.0; 4];
        value[..numbers.len().min(4)].copy_from_slice(&numbers[..numbers.len().min(4)]);
        value
    };
    match field {
        Field::Constant | Field::Color => match node.map(|n| &mut n.kind) {
            Some(NodeKind::Constant { value, .. } | NodeKind::Color { value }) => {
                *value = fill(numbers);
                true
            }
            _ => false,
        },
        Field::Literal(input) => match node {
            Some(node) => {
                node.literals.retain(|l| l.input != input);
                node.literals.push(voxear_voxshade::Literal { input, value: fill(numbers) });
                true
            }
            None => false,
        },
        Field::PropertyDefault(id) => match graph.properties.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.default = fill(numbers);
                true
            }
            None => false,
        },
        Field::PropertyRange(id) => match graph.properties.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.range = (numbers[0] != numbers[1]).then(|| [numbers[0].min(numbers[1]), numbers[0].max(numbers[1])]);
                true
            }
            None => false,
        },
        Field::MaxOffset => {
            graph.max_offset = numbers[0].max(0.0);
            true
        }
        _ => false,
    }
}

/// Which option a choice row shows now.
fn chosen(graph: &Graph, node: Option<&Node>, field: Field, choosable: &[PropertyId]) -> Option<usize> {
    if let Field::PropertyKind(id) = field {
        return PropertyKind::ALL.iter().position(|kind| Some(*kind) == graph.property(id).map(|p| p.kind));
    }
    match (field, &node?.kind) {
        (Field::Input, NodeKind::Input { input }) => Input::ALL.iter().position(|i| i == input),
        (Field::Op, NodeKind::Math { op }) => MathOp::ALL.iter().position(|o| o == op),
        (Field::Compare, NodeKind::Compare { op }) => CompareOp::ALL.iter().position(|o| o == op),
        (Field::ConstantTy, NodeKind::Constant { ty, .. }) => NUMBER_TYS.iter().position(|t| t == ty),
        (Field::CombineTy, NodeKind::Combine { ty }) => NUMBER_TYS[1..].iter().position(|t| t == ty),
        (Field::PropertyPick, NodeKind::Property { property } | NodeKind::SamplePicture { property }) => {
            choosable.iter().position(|id| id == property)
        }
        (Field::Swizzle(slot), NodeKind::Swizzle { components }) => {
            let pick = components.get(slot).map_or(0, |c| c.index() + 1);
            Some(if slot == 0 { pick.saturating_sub(1) } else { pick })
        }
        _ => None,
    }
}

/// Sets what a choice row edits to option `at`; false when it edits nothing there is.
fn choose(graph: &mut Graph, node: Option<&mut Node>, field: Field, at: usize, choosable: &[PropertyId]) -> bool {
    if let Field::PropertyKind(id) = field {
        let Some(property) = graph.properties.iter_mut().find(|p| p.id == id) else { return false };
        property.kind = PropertyKind::ALL[at.min(PropertyKind::ALL.len() - 1)];
        return true;
    }
    let Some(node) = node else { return false };
    match (field, &mut node.kind) {
        (Field::Input, NodeKind::Input { input }) => *input = Input::ALL[at.min(Input::ALL.len() - 1)],
        (Field::Op, NodeKind::Math { op }) => *op = MathOp::ALL[at.min(MathOp::ALL.len() - 1)],
        (Field::Compare, NodeKind::Compare { op }) => *op = CompareOp::ALL[at.min(CompareOp::ALL.len() - 1)],
        (Field::ConstantTy, NodeKind::Constant { ty, .. }) => *ty = NUMBER_TYS[at.min(3)],
        (Field::CombineTy, NodeKind::Combine { ty }) => *ty = NUMBER_TYS[1 + at.min(2)],
        (Field::PropertyPick, NodeKind::Property { property } | NodeKind::SamplePicture { property }) => {
            let Some(id) = choosable.get(at) else { return false };
            *property = *id;
        }
        (Field::Swizzle(slot), NodeKind::Swizzle { components }) => {
            let pick = if slot == 0 { Some(at) } else { at.checked_sub(1) };
            match pick.and_then(|pick| Component::ALL.get(pick)) {
                Some(&component) if slot < components.len() => components[slot] = component,
                Some(&component) if slot == components.len() => components.push(component),
                Some(_) => return false,
                None => components.truncate(slot.max(1)),
            }
        }
        _ => return false,
    }
    true
}

/// Whether a wire may run from output `from` into input `to`: true or
/// false only into a condition and nowhere else, and never round in a loop.
fn allows(graph: &Graph, from: (GraphNode, usize), to: (GraphNode, usize)) -> bool {
    let (Some(out), Some(into)) = (graph.node(from.0), graph.node(to.0)) else { return false };
    if from.0 == to.0 {
        return false;
    }
    let given = out.kind.outputs(graph_property_ty(graph, &out.kind)).get(from.1).map(|p| p.ty);
    let taken = into.kind.inputs().get(to.1).map(|p| p.ty);
    let (Some(given), Some(taken)) = (given, taken) else { return false };
    let truth = |ty: PortTy| ty == PortTy::Fixed(Ty::Bool);
    if truth(given) != truth(taken) {
        return false;
    }
    // A loop: the node fed already feeds, down its wires, the node it would be fed from.
    let mut reached = vec![to.0];
    let mut at = 0;
    while at < reached.len() {
        let node = reached[at];
        if node == from.0 {
            return false;
        }
        for link in graph.links.iter().filter(|link| link.from == node) {
            if !reached.contains(&link.to) {
                reached.push(link.to);
            }
        }
        at += 1;
    }
    true
}

fn graph_property_ty(graph: &Graph, kind: &NodeKind) -> Option<Ty> {
    match kind {
        NodeKind::Property { property } => graph.property_ty(*property),
        _ => None,
    }
}

/// What a wire carrying each type is coloured.
fn port_color(ty: PortTy) -> Color {
    Color::hex(match ty {
        PortTy::Fixed(Ty::Float) => 0x9fb4c7,
        PortTy::Fixed(Ty::Float2) => 0x7fd37a,
        PortTy::Fixed(Ty::Float3) => 0xffcf5a,
        PortTy::Fixed(Ty::Float4) => 0xc58cff,
        PortTy::Fixed(Ty::Bool) => 0xff7a7a,
        PortTy::Any => 0xe2e2e2,
    })
}

/// A node's title as its box shows it: a Property or Sample Picture node by its property's name.
fn title(graph: &Graph, kind: &NodeKind) -> String {
    let name = |id: PropertyId| graph.property(id).map_or("?".to_string(), |p| p.name.clone());
    match kind {
        NodeKind::Property { property } => name(*property),
        NodeKind::SamplePicture { property } => format!("Sample {}", name(*property)),
        NodeKind::Combine { ty } => format!("Combine {}", ty.name()),
        NodeKind::Compare { op } => format!("Compare {}", op.slang()),
        other => other.title(),
    }
}

/// A node as the canvas draws it, without its cost.
fn describe_node(graph: &Graph, node: &Node) -> CanvasNode {
    let ports = |ports: Vec<voxear_voxshade::Port>| -> Vec<CanvasPort> {
        ports.into_iter().map(|port| CanvasPort { label: port.label.to_string(), color: port_color(port.ty) }).collect()
    };
    CanvasNode {
        id: u64::from(node.id.0),
        at: node.at,
        title: title(graph, &node.kind),
        inputs: ports(node.kind.inputs()),
        outputs: ports(node.kind.outputs(graph_property_ty(graph, &node.kind))),
        pictured: false,
        ..CanvasNode::default()
    }
}

/// A graph as the canvas draws it: each node with what it costs once
/// compiled, and what stops the graph compiling on the nodes at fault.
fn describe(graph: &Graph, compiled: &Result<Compiled, Vec<voxear_voxshade::CompileError>>) -> CanvasGraph {
    let mut notice = String::new();
    let nodes = graph
        .nodes
        .iter()
        .map(|node| {
            let mut drawn = describe_node(graph, node);
            match compiled {
                Ok(compiled) => {
                    if let Some(cost) = compiled.nodes.get(&node.id).filter(|cost| cost.alu > 0 || cost.samples > 0) {
                        drawn.detail = cost_text(cost.alu, cost.samples);
                    }
                }
                Err(errors) => {
                    let mine: Vec<&str> =
                        errors.iter().filter(|e| e.node == Some(node.id)).map(|e| e.message.as_str()).collect();
                    drawn.fault = mine.join("; ");
                }
            }
            drawn
        })
        .collect();
    if let Err(errors) = compiled {
        notice = errors.iter().filter(|e| e.node.is_none()).map(|e| e.message.clone()).collect::<Vec<_>>().join("  ");
    }
    let wire = |link: &Link| CanvasWire {
        from: (u64::from(link.from.0), link.output),
        to: (u64::from(link.to.0), link.input),
    };
    CanvasGraph { nodes, wires: graph.links.iter().map(wire).collect(), notice }
}

/// Instructions and pictures read, as a node's small text or the bar shows them.
fn cost_text(alu: u32, samples: u32) -> String {
    let mut text = format!("{alu} {}", if alu == 1 { "instruction" } else { "instructions" });
    if samples > 0 {
        text += &format!(", {samples} {}", if samples == 1 { "picture" } else { "pictures" });
    }
    text
}

/// Each stage's cost, for the bar.
fn summary(compiled: &Compiled) -> String {
    let stage = |(stage, cost): &(Stage, voxear_voxshade::StageCost)| {
        let mut text = format!("{}: {}", stage.name(), cost_text(cost.alu, cost.samples));
        if cost.branches > 0 {
            text += &format!(", {} {}", cost.branches, if cost.branches == 1 { "branch" } else { "branches" });
        }
        text
    };
    compiled.stages.iter().map(stage).collect::<Vec<_>>().join("   ")
}

/// Every node the search offers to add to `graph`, in its folder.
pub fn offers(graph: &Graph) -> Vec<Offer> {
    let mut all = Vec::new();
    let mut offer = |label: String, folder: &'static str, kind: NodeKind| all.push(Offer { label, folder, kind });
    for input in Input::ALL {
        offer(input.name().into(), "Inputs", NodeKind::Input { input });
    }
    offer("Surface".into(), "Inputs", NodeKind::Surface);
    for ty in NUMBER_TYS {
        offer(ty.name().into(), "Values", NodeKind::Constant { ty, value: [0.0; 4] });
    }
    offer("Color".into(), "Values", NodeKind::Color { value: [1.0; 4] });
    for property in &graph.properties {
        match property.kind {
            PropertyKind::Picture => offer(
                format!("Sample {}", property.name),
                "Properties",
                NodeKind::SamplePicture { property: property.id },
            ),
            _ => offer(property.name.clone(), "Properties", NodeKind::Property { property: property.id }),
        }
    }
    for op in MathOp::ALL {
        offer(op.name().into(), "Math", NodeKind::Math { op });
    }
    offer("Compare".into(), "Logic", NodeKind::Compare { op: CompareOp::Greater });
    offer("Branch".into(), "Logic", NodeKind::Branch);
    offer("Split".into(), "Vectors", NodeKind::Split);
    for ty in &NUMBER_TYS[1..] {
        offer(format!("Combine {}", ty.name()), "Vectors", NodeKind::Combine { ty: *ty });
    }
    offer("Swizzle".into(), "Vectors", NodeKind::Swizzle { components: vec![Component::X, Component::Y] });
    offer("Random".into(), "Noise", NodeKind::Random);
    offer("Surface Output".into(), "Outputs", NodeKind::SurfaceOutput);
    offer("Vertex Output".into(), "Outputs", NodeKind::VertexOutput);
    offer("Light Output".into(), "Outputs", NodeKind::LightOutput);
    all
}

/// The icon the search shows beside a folder's nodes.
pub fn folder_icon(folder: &str) -> &'static str {
    match folder {
        "Inputs" => "arrow-right",
        "Values" => "edit",
        "Properties" => "shader",
        "Math" => "plus",
        "Logic" => "branch",
        "Vectors" => "move",
        "Noise" => "grid",
        _ => "check",
    }
}

#[cfg(test)]
mod tests {
    use voxear_voxshade::starters;

    use super::*;

    #[test]
    fn a_wire_takes_only_what_its_input_takes_and_never_runs_round_in_a_loop() {
        let mut graph = Graph::new("Test");
        let uv = graph.add(NodeKind::Input { input: Input::Uv }, [0.0, 0.0]);
        let compare = graph.add(NodeKind::Compare { op: CompareOp::Less }, [0.0, 0.0]);
        let branch = graph.add(NodeKind::Branch, [0.0, 0.0]);
        let (a, b) = (
            graph.add(NodeKind::Math { op: MathOp::Add }, [0.0; 2]),
            graph.add(NodeKind::Math { op: MathOp::Sine }, [0.0; 2]),
        );
        assert!(allows(&graph, (uv, 0), (a, 0)));
        assert!(!allows(&graph, (uv, 0), (branch, 0)), "numbers into a condition");
        assert!(allows(&graph, (compare, 0), (branch, 0)));
        assert!(!allows(&graph, (compare, 0), (a, 0)), "a truth into numbers");
        graph.link(a, 0, b, 0);
        assert!(!allows(&graph, (b, 0), (a, 1)), "round in a loop");
        assert!(!allows(&graph, (a, 0), (a, 1)), "into itself");
    }

    #[test]
    fn rows_set_a_nodes_values_and_its_unwired_inputs() {
        let mut graph = Graph::new("Test");
        let math = graph.add(NodeKind::Math { op: MathOp::Add }, [0.0; 2]);
        let mut node = graph.node(math).cloned().unwrap();
        assert!(choose(&mut graph, Some(&mut node), Field::Op, 2, &[]));
        assert_eq!(node.kind, NodeKind::Math { op: MathOp::Multiply });
        assert_eq!(numbers_of(&graph, Some(&node), Field::Literal(1)), [1.0], "times one with nothing wired in");
        assert!(set_numbers(&mut graph, Some(&mut node), Field::Literal(1), &[2.5]));
        assert_eq!(numbers_of(&graph, Some(&node), Field::Literal(1)), [2.5]);
        // A swizzle picks numbers one by one, None ending it, never with none at all.
        let mut swizzle = Node {
            id: GraphNode(9),
            at: [0.0; 2],
            kind: NodeKind::Swizzle { components: vec![Component::X] },
            literals: Vec::new(),
        };
        assert!(choose(&mut graph, Some(&mut swizzle), Field::Swizzle(1), 3, &[]));
        assert_eq!(swizzle.kind, NodeKind::Swizzle { components: vec![Component::X, Component::Z] });
        assert!(choose(&mut graph, Some(&mut swizzle), Field::Swizzle(1), 0, &[]));
        assert_eq!(swizzle.kind, NodeKind::Swizzle { components: vec![Component::X] });
        assert!(!choose(&mut graph, Some(&mut swizzle), Field::Swizzle(3), 1, &[]), "not past the end");
        // A property's range, the same at both ends for none.
        let id = graph.add_property("speed", PropertyKind::Number, [1.0; 4]);
        assert!(set_numbers(&mut graph, None, Field::PropertyRange(id), &[4.0, 0.5]));
        assert_eq!(graph.property(id).unwrap().range, Some([0.5, 4.0]));
        assert!(set_numbers(&mut graph, None, Field::PropertyRange(id), &[1.0, 1.0]));
        assert_eq!(graph.property(id).unwrap().range, None);
    }

    #[test]
    fn a_compiled_graph_shows_each_nodes_cost_and_what_stops_it_on_its_node() {
        let graph = starters::hologram();
        let compiled = compile(&graph);
        let drawn = describe(&graph, &compiled);
        let samples = drawn.nodes.iter().filter(|n| n.detail.contains("1 picture")).count();
        assert_eq!(samples, 3, "the three reads of the pattern");
        assert!(drawn.nodes.iter().all(|n| n.fault.is_empty()));
        assert!(summary(compiled.as_ref().unwrap()).starts_with("Surface: "));
        let mut broken = graph.clone();
        let light = broken.add(NodeKind::Input { input: Input::LightColor }, [0.0; 2]);
        let out = broken.nodes.iter().find(|n| n.kind == NodeKind::SurfaceOutput).unwrap().id;
        broken.link(light, 0, out, 5);
        let drawn = describe(&broken, &compile(&broken));
        let at = drawn.node(u64::from(light.0)).unwrap();
        assert!(at.fault.contains("Surface"), "{}", at.fault);
        // The search offers each property by its name, a picture as something to sample.
        let offered: Vec<String> = offers(&graph).into_iter().map(|offer| offer.label).collect();
        assert!(offered.contains(&"tint".to_string()) && offered.contains(&"Sample pattern".to_string()));
    }
}
