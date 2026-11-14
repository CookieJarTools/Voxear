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

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use voxear_assets::AssetGuid;
use voxear_ui::{Color, NodeId, Style, Ui};
use voxear_voxshade::{
    CompareOp, Compiled, Component, Graph, Input, Link, MathOp, Node, NodeId as GraphNode, NodeKind, Options, Pin,
    PortTy, PropertyId, PropertyKind, Stage, Ty, compile_with,
};

use crate::{
    asset_inspector::part,
    graph_canvas::{
        CanvasComment, CanvasEvent, CanvasGraph, CanvasKey, CanvasNode, CanvasPort, CanvasWire, GraphCanvas, PortRef,
        node_size,
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
    /// One comment's title and text.
    Comment(u32),
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
    /// A Slang node's name and code, and one of its inputs' or outputs'
    /// (`true`) name and type, by place.
    CodeName,
    CodeText,
    CodePinName(bool, usize),
    CodePinTy(bool, usize),
    RemoveCodePin(bool, usize),
    AddCodePin(bool),
    /// Which of the project's sub-graphs a Sub-graph node uses.
    SubGraphPick,
    /// Which of the sub-graph's inputs an Input node reads.
    InputPick,
    /// One of the open sub-graph's inputs or outputs (`true`), by number.
    PinName(bool, u32),
    PinTy(bool, u32),
    PinDefault(u32),
    RemovePin(bool, u32),
    AddPin(bool),
    /// The picture a particle graph's picture property names.
    PropertyPicture(PropertyId),
}

/// The project's sub-graphs, each its file's GUID, path and graph.
pub type SubGraphs = Vec<(AssetGuid, String, Graph)>;

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
    /// Open the picker for the picture of this picture property, under this element.
    PickPicture(NodeId, PropertyId),
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
    pin_rows: NodeId,
    shader_rows: NodeId,
    /// The project's sub-graphs, which Sub-graph nodes use, and the same by GUID, which compiling reads.
    sub_graphs: SubGraphs,
    library: HashMap<AssetGuid, Graph>,
    /// The sub-graphs a Sub-graph node's choice lists, in order.
    sub_choosable: Vec<AssetGuid>,
    /// The project's pictures' names, which a particle graph's picture properties show.
    picture_names: HashMap<AssetGuid, String>,
    rows: Vec<(Field, Control)>,
    /// What the rows were built for, to build them again when that changes.
    shape: String,
    /// The properties a Property or Sample Picture node's choice lists, in order.
    choosable: Vec<PropertyId>,
    open: Option<Open>,
    /// Each node's picture, and when the panel was made, which the time they show is counted from.
    previews: crate::voxshade_previews::Previews,
    started: Instant,
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
        let pin_rows = part(ui, side, "Inputs and Outputs", "nodes");
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
            pin_rows,
            shader_rows,
            sub_graphs: Vec::new(),
            library: HashMap::new(),
            sub_choosable: Vec::new(),
            picture_names: HashMap::new(),
            rows: Vec::new(),
            shape: String::new(),
            choosable: Vec::new(),
            open: None,
            previews: Default::default(),
            started: Instant::now(),
            copied: Copied::default(),
            offered: Vec::new(),
            active: false,
            hold_active: false,
            showing: None,
            pending: Vec::new(),
        }
    }

    /// Opens the graph at `path`: all of it in view, nothing picked, nothing to undo.
    pub fn open(&mut self, ui: &mut Ui, path: &str, mut graph: Graph) {
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.set_text(self.name, file.strip_suffix(".voxshade").unwrap_or(file));
        ui.set_tooltip(self.name, path, None);
        // Its Sub-graph nodes as their sub-graphs now are, saved so.
        let refreshed = refresh_sub_graphs(&mut graph, &self.library);
        let compiled = compile_in(&graph, &self.library);
        self.open = Some(Open {
            path: path.to_string(),
            graph,
            history: History::default(),
            unsaved: refreshed,
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

    pub fn close(&mut self, renderer: &mut voxear_render::Renderer) {
        self.previews.clear(renderer);
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
        let path = self.path().map(str::to_string);
        let others: Vec<_> = self.sub_graphs.iter().filter(|(_, at, _)| Some(at) != path.as_ref()).cloned().collect();
        self.offered = self.open.as_ref().map(|open| offers(&open.graph, &others)).unwrap_or_default();
        &self.offered
    }

    /// The project's sub-graphs as they now are. The open graph's
    /// Sub-graph nodes take their names, inputs and outputs, wires to
    /// ports gone dropped, and it is compiled with them.
    pub fn set_sub_graphs(&mut self, sub_graphs: SubGraphs) {
        self.library = sub_graphs.iter().map(|(guid, _, graph)| (*guid, graph.clone())).collect();
        self.sub_graphs = sub_graphs;
        let Some(open) = self.open.as_mut() else { return };
        open.stale = true;
        let mut graph = open.graph.clone();
        if refresh_sub_graphs(&mut graph, &self.library) {
            self.change(graph);
        }
    }

    /// The names of the project's pictures, by their GUIDs.
    pub fn set_picture_names(&mut self, names: HashMap<AssetGuid, String>) {
        if names != self.picture_names {
            self.picture_names = names;
            self.shape.clear();
        }
    }

    /// Sets the picture a particle graph's picture property names, as a step to undo.
    pub fn set_picture(&mut self, property: PropertyId, picture: Option<AssetGuid>) {
        let Some(mut graph) = self.graph().cloned() else { return };
        let Some(found) = graph.properties.iter_mut().find(|p| p.id == property) else { return };
        found.picture = picture;
        self.change(graph);
    }

    /// The project's sub-graphs, as the panel has them.
    pub fn sub_graphs(&self) -> &SubGraphs {
        &self.sub_graphs
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
    pub fn update(
        &mut self,
        ui: &mut Ui,
        renderer: &mut voxear_render::Renderer,
        shown: bool,
        alt: bool,
    ) -> Vec<VoxShadeEvent> {
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
            // Each node's picture, made again as the graph or the time changes what it shows.
            let time = self.started.elapsed().as_secs_f32();
            if let Some(open) = self.open.as_mut()
                && self.previews.update(renderer, &open.graph, &self.library, time)
            {
                open.stale = true;
            }
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
            open.compiled = compile_in(&open.graph, &self.library);
            open.drawn = describe(&open.graph, &open.compiled);
            for node in &mut open.drawn.nodes {
                node.picture = self.previews.picture(GraphNode(node.id as u32));
                node.pictured = node.picture.is_some();
            }
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
        let mut added = None;
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
                CanvasEvent::CommentsMoved(places) => {
                    for (comment, at) in places {
                        if let Some(comment) = graph.comment_mut(comment as u32) {
                            comment.at = at;
                        }
                    }
                    key = Some(EditKey::Move);
                }
                CanvasEvent::CommentResized(comment, size) => {
                    if let Some(comment) = graph.comment_mut(comment as u32) {
                        comment.size = size;
                    }
                    key = Some(EditKey::Move);
                }
                CanvasEvent::AddComment { at, size } => added = Some(graph.add_comment(at, size, "Comment", "")),
                CanvasEvent::CommentEdited { id: comment, title, text } => {
                    if let Some(comment) = graph.comment_mut(comment as u32) {
                        (comment.title, comment.text) = (title, text);
                    }
                    key = Some(EditKey::Comment(comment as u32));
                }
                CanvasEvent::DeleteComments(comments) => {
                    comments.into_iter().for_each(|comment| _ = graph.remove_comment(comment as u32))
                }
            }
        }
        if graph != open.graph {
            self.change_by(graph, key);
        }
        if let Some(comment) = added {
            self.canvas.edit_comment(u64::from(comment));
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
        let listed: Vec<&AssetGuid> = self.sub_graphs.iter().map(|(guid, _, _)| guid).collect();
        let shape = format!("{} {listed:?}", shape(&graph, picked.as_ref()));
        if shape != self.shape {
            self.shape = shape;
            self.build_rows(ui, picked.as_ref());
        }
        for (field, control) in &self.rows {
            if let (Field::PropertyPicture(id), Control::Button(button)) = (field, control)
                && ui.clicked(*button)
            {
                self.pending.push(VoxShadeEvent::PickPicture(*button, *id));
            }
        }
        if let Some((changed, field)) = self.read_rows(ui, &graph, picked.as_ref()) {
            let merges = !matches!(
                field,
                Field::AddProperty
                    | Field::RemoveProperty(_)
                    | Field::AddCodePin(_)
                    | Field::RemoveCodePin(..)
                    | Field::AddPin(_)
                    | Field::RemovePin(..)
                    | Field::SubGraphPick
            );
            self.change_by(changed, merges.then_some(EditKey::Row(field)));
        }
        let Some(graph) = self.open.as_ref().map(|open| open.graph.clone()) else { return };
        let picked = picked.and_then(|node| graph.node(node.id)).cloned();
        self.show_rows(ui, &graph, picked.as_ref());
    }

    fn build_rows(&mut self, ui: &mut Ui, picked: Option<&Node>) {
        for container in [self.node_rows, self.property_rows, self.pin_rows, self.shader_rows] {
            for child in ui.children(container) {
                ui.remove(child);
            }
        }
        self.rows.clear();
        self.choosable.clear();
        self.sub_choosable.clear();
        let Some(graph) = self.open.as_ref().map(|open| open.graph.clone()) else { return };
        // A sub-graph has inputs and outputs where a shader has properties.
        let sub = graph.sub_graph;
        show_part(ui, self.property_rows, None, !sub);
        show_part(ui, self.pin_rows, None, sub);
        let title = if sub {
            "Sub-graph"
        } else if graph.particles {
            "Particle Graph"
        } else {
            "Shader"
        };
        show_part(ui, self.shader_rows, Some(title), true);
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
                    NodeKind::Code { inputs, outputs, code, .. } => {
                        let row = prop(ui, rows, "Name");
                        let name = ui.text_field(row, "");
                        ui.set_tooltip(name, "What its function is called in the Slang: letters, digits and _", None);
                        self.rows.push((Field::CodeName, Control::Text(name)));
                        let names: Vec<&str> = NUMBER_TYS.iter().map(|t| t.name()).collect();
                        for (out, pins) in [(false, inputs), (true, outputs)] {
                            for place in 0..pins.len() {
                                let row = prop(ui, rows, if out { "Output" } else { "Input" });
                                let name = ui.text_field(row, "");
                                ui.add_class(name, "value-input");
                                self.rows.push((Field::CodePinName(out, place), Control::Text(name)));
                                let ty = ui.dropdown(row, &names, 0);
                                self.rows.push((Field::CodePinTy(out, place), Control::Choice(ty)));
                                let remove = ui.icon_button(row, "close");
                                ui.set_tooltip(
                                    remove,
                                    if out { "Remove this output" } else { "Remove this input" },
                                    None,
                                );
                                self.rows.push((Field::RemoveCodePin(out, place), Control::Button(remove)));
                            }
                        }
                        let buttons = ui.panel(rows, Style::row());
                        for (out, label) in [(false, "Add Input"), (true, "Add Output")] {
                            let add = ui.button(buttons, label);
                            ui.add_class(add, "add-component");
                            self.rows.push((Field::AddCodePin(out), Control::Button(add)));
                        }
                        let row = prop(ui, rows, "Code");
                        let field = ui.text_field(row, "");
                        ui.set_multiline(field, true);
                        ui.add_class(field, "value-input");
                        ui.add_class(field, "attr-multiline");
                        let lines = code.lines().count().clamp(6, 24);
                        ui.set_inline_style(field, &format!("height: {}px", lines * 16 + 10));
                        ui.set_tooltip(field, "Slang that sets every output from the inputs, by their names", None);
                        self.rows.push((Field::CodeText, Control::Text(field)));
                    }
                    NodeKind::SubGraph { .. } => {
                        let own = self.open.as_ref().map(|open| open.path.clone());
                        let listed: Vec<&(AssetGuid, String, Graph)> =
                            self.sub_graphs.iter().filter(|(_, path, _)| Some(path) != own.as_ref()).collect();
                        self.sub_choosable = listed.iter().map(|(guid, _, _)| *guid).collect();
                        let names: Vec<String> = listed.iter().map(|(_, _, graph)| graph.name.clone()).collect();
                        let names = if names.is_empty() {
                            vec!["None yet: make one with New Shader".to_string()]
                        } else {
                            names
                        };
                        choice(ui, Field::SubGraphPick, "Sub-graph", names);
                    }
                    NodeKind::SubGraphInput { .. } => {
                        let names: Vec<String> = graph.inputs.iter().map(|pin| pin.name.clone()).collect();
                        let names = if names.is_empty() { vec!["None yet: add one below".to_string()] } else { names };
                        choice(ui, Field::InputPick, "Input", names);
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
                for (index, port) in graph.inputs_of(&node.kind).iter().enumerate() {
                    if graph.link_into(node.id, index).is_some() {
                        continue;
                    }
                    let width = match port.ty {
                        PortTy::Fixed(ty) => ty.width(),
                        PortTy::Any => 1,
                        // A picture comes only by wire.
                        PortTy::Picture => continue,
                    };
                    let fields = numbers_row(ui, rows, &port.label, width);
                    // An output's input with nothing set takes the material's own, or the particle system's.
                    if let Some(stage) = node.kind.output_stage() {
                        let (shown, whose) =
                            if stage.is_particles() { ("System's", "system's") } else { ("Material's", "material's") };
                        for &field in &fields {
                            ui.set_placeholder(field, shown);
                        }
                        ui.set_tooltip(fields[0], &format!("Empty to keep the {whose} own"), None);
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
            // A particle graph's picture is its own to name, as no material picks one for it.
            if graph.particles && property.kind == PropertyKind::Picture {
                let row = prop(ui, rows, "Picture");
                let named = property
                    .picture
                    .map(|guid| self.picture_names.get(&guid).cloned().unwrap_or_else(|| guid.to_string()));
                let shown = ui.add_label(row, named.as_deref().unwrap_or("None: reads as white"));
                ui.add_class(shown, "value");
                let pick = ui.icon_button(row, "texture");
                ui.set_tooltip(pick, "Pick the picture", None);
                self.rows.push((Field::PropertyPicture(property.id), Control::Button(pick)));
            }
        }
        let add = ui.button(self.property_rows, "Add Property");
        ui.add_class(add, "add-component");
        self.rows.push((Field::AddProperty, Control::Button(add)));
        // A sub-graph's inputs, then its outputs, each a few rows under its name.
        let names: Vec<&str> = NUMBER_TYS.iter().map(|t| t.name()).collect();
        for (out, pins) in [(false, &graph.inputs), (true, &graph.outputs)] {
            let rows = self.pin_rows;
            for pin in pins.iter() {
                let head = ui.panel(rows, Style::row());
                ui.add_class(head, "prop");
                let title = ui.add_label(head, &format!("{} {}", if out { "Output" } else { "Input" }, pin.name));
                ui.add_class(title, "prop-label");
                let remove = ui.icon_button(head, "close");
                ui.set_tooltip(remove, if out { "Remove this output" } else { "Remove this input" }, None);
                self.rows.push((Field::RemovePin(out, pin.id), Control::Button(remove)));
                let row = prop(ui, rows, "Name");
                let name = ui.text_field(row, &pin.name);
                self.rows.push((Field::PinName(out, pin.id), Control::Text(name)));
                let row = prop(ui, rows, "Type");
                // An input may be a picture, handed on to the Samples inside; an output gives numbers.
                let at = if pin.picture {
                    NUMBER_TYS.len()
                } else {
                    NUMBER_TYS.iter().position(|ty| *ty == pin.ty).unwrap_or(0)
                };
                let mut kinds = names.clone();
                if !out {
                    kinds.push("Picture");
                }
                let ty = ui.dropdown(row, &kinds, at);
                self.rows.push((Field::PinTy(out, pin.id), Control::Choice(ty)));
                if !out && !pin.picture {
                    let fields = numbers_row(ui, rows, "Default", pin.ty.width());
                    ui.set_tooltip(fields[0], "What it takes where nothing is wired into it", None);
                    self.rows.push((Field::PinDefault(pin.id), Control::Numbers(fields)));
                }
            }
        }
        let buttons = ui.panel(self.pin_rows, Style::row());
        for (out, label) in [(false, "Add Input"), (true, "Add Output")] {
            let add = ui.button(buttons, label);
            ui.add_class(add, "add-component");
            self.rows.push((Field::AddPin(out), Control::Button(add)));
        }
        // The shader's own settings, or the sub-graph's name.
        let rows = self.shader_rows;
        let row = prop(ui, rows, "Name");
        let name = ui.text_field(row, &graph.name);
        let named = if sub {
            "The name its Slang function and the nodes using it have"
        } else if graph.particles {
            "Its name in the particle shaders"
        } else {
            "The name its Slang gives its properties"
        };
        ui.set_tooltip(name, &format!("{named}: letters, digits and _"), None);
        self.rows.push((Field::Name, Control::Text(name)));
        if sub || graph.particles {
            return;
        }
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
                (Field::AddCodePin(out), Control::Button(button)) if ui.clicked(*button) => match node.as_mut() {
                    Some(Node { kind: NodeKind::Code { inputs, outputs, .. }, .. }) => {
                        let taken: Vec<String> = inputs.iter().chain(outputs.iter()).map(|p| p.name.clone()).collect();
                        let stem = if *out { "out" } else { "in" };
                        let name = (1..).map(|n| format!("{stem}{n}")).find(|n| !taken.contains(n)).unwrap_or_default();
                        if *out { outputs } else { inputs }.push(Pin::new(&name, Ty::Float3));
                        true
                    }
                    _ => false,
                },
                (Field::RemoveCodePin(out, place), Control::Button(button)) if ui.clicked(*button) => {
                    match node.as_mut() {
                        Some(found @ Node { kind: NodeKind::Code { .. }, .. }) => {
                            let NodeKind::Code { inputs, outputs, .. } = &mut found.kind else { unreachable!() };
                            let pins = if *out { outputs } else { inputs };
                            if *place < pins.len() {
                                pins.remove(*place);
                            }
                            drop_port(&mut changed, found, *out, *place);
                            true
                        }
                        _ => false,
                    }
                }
                (Field::AddPin(out), Control::Button(button)) if ui.clicked(*button) => {
                    let pins = if *out { &changed.outputs } else { &changed.inputs };
                    let stem = if *out { "result" } else { "value" };
                    let taken = |name: &str| pins.iter().any(|p| p.name == name);
                    let name = (1..).map(|n| format!("{stem}{n}")).find(|n| !taken(n)).unwrap_or_default();
                    if *out {
                        changed.add_output(&name, Ty::Float)
                    } else {
                        changed.add_input(&name, Ty::Float)
                    };
                    true
                }
                (Field::RemovePin(out, id), Control::Button(button)) if ui.clicked(*button) => {
                    remove_pin(&mut changed, *out, *id);
                    // The picked node may have gone with it.
                    node = node.filter(|n| changed.node(n.id).is_some());
                    true
                }
                (_, Control::Button(_)) => false,
                (Field::SubGraphPick, Control::Choice(dropdown)) if ui.changed(*dropdown) => {
                    let at = ui.selected_option(*dropdown);
                    match (self.sub_choosable.get(at), node.as_mut()) {
                        (Some(guid), Some(found)) if matches!(found.kind, NodeKind::SubGraph { .. }) => {
                            let NodeKind::SubGraph { graph, .. } = &mut found.kind else { unreachable!() };
                            *graph = *guid;
                            if let Some(kept) = changed.nodes.iter_mut().find(|n| n.id == found.id) {
                                kept.kind = found.kind.clone();
                            }
                            refresh_sub_graphs(&mut changed, &self.library);
                            *found = changed.node(found.id).cloned().unwrap_or_else(|| found.clone());
                            true
                        }
                        _ => false,
                    }
                }
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
                    let identifier = voxear_voxshade::graph::is_identifier(&typed);
                    match field {
                        Field::CodeName => match node.as_mut().map(|n| &mut n.kind) {
                            Some(NodeKind::Code { name, .. }) if typed.is_empty() || identifier => {
                                *name = typed;
                                true
                            }
                            _ => false,
                        },
                        Field::CodeText => match node.as_mut().map(|n| &mut n.kind) {
                            Some(NodeKind::Code { code, .. }) => {
                                *code = ui.text(*text).to_string();
                                true
                            }
                            _ => false,
                        },
                        Field::CodePinName(out, place) => match node.as_mut().map(|n| &mut n.kind) {
                            Some(NodeKind::Code { inputs, outputs, .. }) => {
                                let taken = inputs.iter().chain(outputs.iter()).any(|p| p.name == typed);
                                let pins = if *out { outputs } else { inputs };
                                match pins.get_mut(*place) {
                                    Some(pin) if identifier && !taken => {
                                        pin.name = typed;
                                        true
                                    }
                                    _ => false,
                                }
                            }
                            _ => false,
                        },
                        Field::PinName(out, id) => {
                            let pins = if *out { &mut changed.outputs } else { &mut changed.inputs };
                            let free = identifier && !pins.iter().any(|p| p.name == typed && p.id != *id);
                            if free && let Some(pin) = pins.iter_mut().find(|p| p.id == *id) {
                                pin.name = typed;
                            }
                            free
                        }
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
                    let sub_graph = match (field, picked.map(|n| &n.kind)) {
                        (Field::SubGraphPick, Some(NodeKind::SubGraph { graph, .. })) => {
                            self.sub_choosable.iter().position(|guid| guid == graph)
                        }
                        _ => None,
                    };
                    if let Some(at) = sub_graph.or_else(|| chosen(graph, picked, *field, &self.choosable))
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
                        Field::PinName(out, id) => {
                            let pins = if *out { &graph.outputs } else { &graph.inputs };
                            pins.iter().find(|p| p.id == *id).map(|p| p.name.clone())
                        }
                        _ => match (field, picked.map(|n| &n.kind)) {
                            (Field::CodeName, Some(NodeKind::Code { name, .. })) => Some(name.clone()),
                            (Field::CodeText, Some(NodeKind::Code { code, .. })) => Some(code.clone()),
                            (Field::CodePinName(out, place), Some(NodeKind::Code { inputs, outputs, .. })) => {
                                (if *out { outputs } else { inputs }).get(*place).map(|p| p.name.clone())
                            }
                            _ => None,
                        },
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

/// Takes away the wires into input `place` of `node` (an output, with
/// `out`), and its kept numbers, and moves those of the ports after it
/// down one: the port itself was just taken away.
fn drop_port(graph: &mut Graph, node: &mut Node, out: bool, place: usize) {
    let id = node.id;
    graph.links.retain(|l| !(if out { l.from == id && l.output == place } else { l.to == id && l.input == place }));
    for link in &mut graph.links {
        if out && link.from == id && link.output > place {
            link.output -= 1;
        }
        if !out && link.to == id && link.input > place {
            link.input -= 1;
        }
    }
    if !out {
        node.literals.retain(|l| l.input != place);
        for literal in &mut node.literals {
            if literal.input > place {
                literal.input -= 1;
            }
        }
    }
}

/// Takes away one of a sub-graph's inputs, and the Input nodes reading it,
/// or one of its outputs (`out`), and the wire into the Sub-graph Output for it.
fn remove_pin(graph: &mut Graph, out: bool, id: u32) {
    if out {
        let Some(place) = graph.outputs.iter().position(|p| p.id == id) else { return };
        graph.outputs.remove(place);
        let outputs: Vec<GraphNode> =
            graph.nodes.iter().filter(|n| n.kind == NodeKind::SubGraphOutput).map(|n| n.id).collect();
        for at in outputs {
            let mut node = graph.node(at).cloned().expect("listed");
            drop_port(graph, &mut node, false, place);
            if let Some(kept) = graph.nodes.iter_mut().find(|n| n.id == at) {
                *kept = node;
            }
        }
    } else {
        graph.inputs.retain(|p| p.id != id);
        let readers: Vec<GraphNode> =
            graph.nodes.iter().filter(|n| n.kind == NodeKind::SubGraphInput { input: id }).map(|n| n.id).collect();
        for reader in readers {
            graph.remove(reader);
        }
    }
}

/// Shows or hides one of the side's parts, by its rows, retitled when `title` is given.
fn show_part(ui: &mut Ui, rows: NodeId, title: Option<&str>, shown: bool) {
    let Some(foldout) = ui.parent(rows) else { return };
    if let Some(title) = title
        && let Some(header) = ui.foldout_header(foldout)
        && let Some(&label) = ui.children(header).get(2)
    {
        ui.set_text(label, title);
    }
    ui.set_visible(foldout, shown);
    // The line drawn above it.
    let Some(side) = ui.parent(foldout) else { return };
    let siblings = ui.children(side);
    if let Some(at) = siblings.iter().position(|&n| n == foldout).filter(|&at| at > 0) {
        ui.set_visible(siblings[at - 1], shown);
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
        let tys = |pins: &[Pin]| pins.iter().map(|p| p.ty).collect::<Vec<_>>();
        let kind = match &node.kind {
            NodeKind::Constant { ty, .. } => format!("constant {ty:?}"),
            NodeKind::Color { .. } => "color".into(),
            // Not its names or code, typed into the rows themselves.
            NodeKind::Code { inputs, outputs, .. } => format!("code {:?} {:?}", tys(inputs), tys(outputs)),
            NodeKind::SubGraph { graph, inputs, outputs, .. } => {
                format!("sub-graph {graph:?} {:?} {:?}", tys(inputs), tys(outputs))
            }
            other => format!("{other:?}"),
        };
        format!("{} {kind} {wired:?}", node.id.0)
    });
    let properties: Vec<(u32, PropertyKind)> = graph.properties.iter().map(|p| (p.id.0, p.kind)).collect();
    let pins: Vec<(u32, Ty, bool)> =
        graph.inputs.iter().chain(&graph.outputs).map(|p| (p.id, p.ty, p.picture)).collect();
    let pictures: Vec<_> = graph.properties.iter().map(|p| p.picture).collect();
    let inputs: Vec<&str> = graph.inputs.iter().map(|p| p.name.as_str()).collect();
    format!("{node:?} {properties:?} {} {pins:?} {inputs:?} {pictures:?}", graph.sub_graph)
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
            let port = graph.inputs_of(&node.kind).get(input).cloned();
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
        Field::PinDefault(id) => graph.input(id).map_or(Vec::new(), |(_, pin)| pin.default[..pin.ty.width()].to_vec()),
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
        Field::PinDefault(id) => match graph.inputs.iter_mut().find(|p| p.id == id) {
            Some(pin) => {
                pin.default = fill(numbers);
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// Which option a choice row shows now.
fn chosen(graph: &Graph, node: Option<&Node>, field: Field, choosable: &[PropertyId]) -> Option<usize> {
    if let Field::PropertyKind(id) = field {
        return PropertyKind::ALL.iter().position(|kind| Some(*kind) == graph.property(id).map(|p| p.kind));
    }
    if let Field::PinTy(out, id) = field {
        let pin = (if out { &graph.outputs } else { &graph.inputs }).iter().find(|p| p.id == id)?;
        if pin.picture {
            return Some(NUMBER_TYS.len());
        }
        return NUMBER_TYS.iter().position(|ty| *ty == pin.ty);
    }
    match (field, &node?.kind) {
        (Field::InputPick, NodeKind::SubGraphInput { input }) => graph.input(*input).map(|(place, _)| place),
        (Field::CodePinTy(out, place), NodeKind::Code { inputs, outputs, .. }) => {
            let pin = (if out { outputs } else { inputs }).get(place)?;
            NUMBER_TYS.iter().position(|ty| *ty == pin.ty)
        }
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
    if let Field::PinTy(out, id) = field {
        let pins = if out { &mut graph.outputs } else { &mut graph.inputs };
        let Some(pin) = pins.iter_mut().find(|p| p.id == id) else { return false };
        // Past the numbers' types, an input's last choice is Picture.
        pin.picture = !out && at >= NUMBER_TYS.len();
        pin.ty = NUMBER_TYS[at.min(3)];
        return true;
    }
    let Some(node) = node else { return false };
    match (field, &mut node.kind) {
        (Field::InputPick, NodeKind::SubGraphInput { input }) => {
            let Some(pin) = graph.inputs.get(at) else { return false };
            *input = pin.id;
        }
        (Field::CodePinTy(out, place), NodeKind::Code { inputs, outputs, .. }) => {
            let Some(pin) = (if out { outputs } else { inputs }).get_mut(place) else { return false };
            pin.ty = NUMBER_TYS[at.min(3)];
        }
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
    let given = graph.outputs_of(&out.kind).get(from.1).map(|p| p.ty);
    let taken = graph.inputs_of(&into.kind).get(to.1).map(|p| p.ty);
    let (Some(given), Some(taken)) = (given, taken) else { return false };
    let truth = |ty: PortTy| ty == PortTy::Fixed(Ty::Bool);
    if truth(given) != truth(taken) || (given == PortTy::Picture) != (taken == PortTy::Picture) {
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

/// A graph compiled with the project's sub-graphs.
fn compile_in(
    graph: &Graph,
    library: &HashMap<AssetGuid, Graph>,
) -> Result<Compiled, Vec<voxear_voxshade::CompileError>> {
    compile_with(graph, Options { library: Some(library), ..Options::default() })
}

/// Gives each Sub-graph node in `graph` its sub-graph's name, inputs and
/// outputs as `library` has them, dropping wires to ports it no longer
/// has; whether anything changed.
fn refresh_sub_graphs(graph: &mut Graph, library: &HashMap<AssetGuid, Graph>) -> bool {
    let mut changed = false;
    let mut counts = Vec::new();
    for node in &mut graph.nodes {
        let NodeKind::SubGraph { graph: guid, name, inputs, outputs } = &mut node.kind else { continue };
        let Some(sub) = library.get(guid) else { continue };
        if *name != sub.name || *inputs != sub.inputs || *outputs != sub.outputs {
            (*name, *inputs, *outputs) = (sub.name.clone(), sub.inputs.clone(), sub.outputs.clone());
            changed = true;
        }
        counts.push((node.id, inputs.len(), outputs.len()));
    }
    let before = graph.links.len();
    graph.links.retain(|link| {
        counts
            .iter()
            .all(|&(id, ins, outs)| !(link.to == id && link.input >= ins || link.from == id && link.output >= outs))
    });
    changed || graph.links.len() != before
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
        PortTy::Picture => 0x5ad1c8,
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
        NodeKind::SubGraphInput { input } => {
            graph.input(*input).map_or("Input".to_string(), |(_, pin)| pin.name.clone())
        }
        other => other.title(),
    }
}

/// Where a comment round `nodes` of `graph` goes and how large it is, as
/// the canvas makes one round the nodes picked: none where none of them is
/// in the graph.
pub fn comment_round(graph: &Graph, nodes: &[GraphNode]) -> Option<([f32; 2], [f32; 2])> {
    let drawn = graph.nodes.iter().map(|node| describe_node(graph, node)).collect();
    let drawn = CanvasGraph { nodes: drawn, ..CanvasGraph::default() };
    let ids: Vec<u64> =
        nodes.iter().filter(|node| graph.node(**node).is_some()).map(|node| u64::from(node.0)).collect();
    crate::graph_canvas::comment_round(&drawn, &ids)
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
        inputs: ports(graph.inputs_of(&node.kind)),
        outputs: ports(graph.outputs_of(&node.kind)),
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
                    if let Some(cost) = compiled.nodes.get(&node.id) {
                        drawn.detail = node_cost_text(cost);
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
    let comments = graph.comments.iter().map(|comment| CanvasComment {
        id: u64::from(comment.id),
        at: comment.at,
        size: comment.size,
        title: comment.title.clone(),
        text: comment.text.clone(),
    });
    CanvasGraph { nodes, wires: graph.links.iter().map(wire).collect(), comments: comments.collect(), notice }
}

/// Instructions and pictures read, as a node's small text or the bar shows them.
fn cost_text(alu: u32, samples: u32) -> String {
    let mut text = format!("{alu} {}", if alu == 1 { "instruction" } else { "instructions" });
    if samples > 0 {
        text += &format!(", {samples} {}", if samples == 1 { "picture" } else { "pictures" });
    }
    text
}

/// A node's cost, as its small text: what it does on the GPU, and what is
/// worked out once a material on the CPU; empty for a node that costs nothing.
fn node_cost_text(cost: &voxear_voxshade::NodeCost) -> String {
    match (cost.alu > 0 || cost.samples > 0, cost.once > 0) {
        (true, true) => format!("{}, {} once", cost_text(cost.alu, cost.samples), cost.once),
        (true, false) => cost_text(cost.alu, cost.samples),
        (false, true) => "Once a material".into(),
        (false, false) => String::new(),
    }
}

/// Each stage's cost, for the bar.
fn summary(compiled: &Compiled) -> String {
    let stage = |(stage, cost): &(Stage, voxear_voxshade::StageCost)| {
        let mut text = format!("{}: {}", stage.name(), cost_text(cost.alu, cost.samples));
        if cost.branches > 0 {
            text += &format!(", {} {}", cost.branches, if cost.branches == 1 { "branch" } else { "branches" });
        }
        if cost.once > 0 {
            text += &format!(", {} once a material", cost.once);
        }
        text
    };
    compiled.stages.iter().map(stage).collect::<Vec<_>>().join("   ")
}

/// Every node the search offers to add to `graph`, in its folder.
/// What a new Slang node starts as: its input as its output.
pub fn new_code_node() -> NodeKind {
    NodeKind::Code {
        name: "Custom".into(),
        inputs: vec![Pin::new("value", Ty::Float3)],
        outputs: vec![Pin::new("result", Ty::Float3)],
        code: "result = value;".into(),
    }
}

/// Every node the search offers to add to `graph`, in its folder; the
/// project's other sub-graphs are `sub_graphs`.
pub fn offers(graph: &Graph, sub_graphs: &SubGraphs) -> Vec<Offer> {
    let mut all = Vec::new();
    let mut offer = |label: String, folder: &'static str, kind: NodeKind| all.push(Offer { label, folder, kind });
    // The inputs a particle graph's stages are given, or a shader's; a sub-graph may be used in either.
    let stages: &[Stage] = if graph.particles {
        &Stage::PARTICLES
    } else if graph.sub_graph {
        &[Stage::Surface, Stage::Vertex, Stage::Light, Stage::Update, Stage::Look]
    } else {
        &Stage::ALL
    };
    for input in Input::ALL.into_iter().filter(|input| stages.iter().any(|stage| input.in_stage(*stage))) {
        offer(input.name().into(), "Inputs", NodeKind::Input { input });
    }
    if !graph.particles {
        offer("Surface".into(), "Inputs", NodeKind::Surface);
    }
    for ty in NUMBER_TYS {
        offer(ty.name().into(), "Values", NodeKind::Constant { ty, value: [0.0; 4] });
    }
    offer("Color".into(), "Values", NodeKind::Color { value: [1.0; 4] });
    for pin in &graph.inputs {
        offer(pin.name.clone(), "Sub-graph", NodeKind::SubGraphInput { input: pin.id });
    }
    for property in &graph.properties {
        match property.kind {
            // A picture's node to wire on into a Sample or a sub-graph, and one that reads it there and then.
            PropertyKind::Picture => {
                offer(property.name.clone(), "Properties", NodeKind::Property { property: property.id });
                offer(
                    format!("Sample {}", property.name),
                    "Properties",
                    NodeKind::SamplePicture { property: property.id },
                );
            }
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
    offer("Sample".into(), "Pictures", NodeKind::Sample);
    offer("Slang".into(), "Custom", new_code_node());
    for (guid, _, sub) in sub_graphs {
        let kind = NodeKind::SubGraph {
            graph: *guid,
            name: sub.name.clone(),
            inputs: sub.inputs.clone(),
            outputs: sub.outputs.clone(),
        };
        offer(sub.name.clone(), "Sub-graphs", kind);
    }
    if graph.particles {
        offer("Update Output".into(), "Outputs", NodeKind::UpdateOutput);
        offer("Look Output".into(), "Outputs", NodeKind::LookOutput);
    } else if !graph.sub_graph {
        offer("Surface Output".into(), "Outputs", NodeKind::SurfaceOutput);
        offer("Vertex Output".into(), "Outputs", NodeKind::VertexOutput);
        offer("Light Output".into(), "Outputs", NodeKind::LightOutput);
    }
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
        "Custom" => "code",
        "Pictures" => "texture",
        "Sub-graphs" => "nodes",
        "Sub-graph" => "arrow-right",
        _ => "check",
    }
}

#[cfg(test)]
mod tests {
    use voxear_voxshade::{compile, starters};

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
    fn a_slang_nodes_ports_and_a_sub_graphs_inputs_come_and_go_with_their_wires() {
        let mut graph = Graph::new("Test");
        let uv = graph.add(NodeKind::Input { input: Input::Uv }, [0.0; 2]);
        let code = graph.add(new_code_node(), [0.0; 2]);
        let mut node = graph.node(code).cloned().unwrap();
        if let NodeKind::Code { inputs, .. } = &mut node.kind {
            inputs.push(Pin::new("second", Ty::Float2));
        }
        graph.nodes.retain(|n| n.id != code);
        graph.nodes.push(node.clone());
        graph.link(uv, 0, code, 1);
        node.literals.push(voxear_voxshade::Literal { input: 1, value: [0.5; 4] });
        // The first input taken away: the wire and numbers on the second move down with it.
        if let NodeKind::Code { inputs, .. } = &mut node.kind {
            inputs.remove(0);
        }
        drop_port(&mut graph, &mut node, false, 0);
        assert_eq!(graph.link_into(code, 0).map(|l| l.from), Some(uv));
        assert_eq!(node.literals[0].input, 0);
        assert!(choose(&mut graph, Some(&mut node), Field::CodePinTy(false, 0), 2, &[]));
        assert_eq!(chosen(&graph, Some(&node), Field::CodePinTy(false, 0), &[]), Some(2));
        // A sub-graph's input taken away takes the Input nodes reading it.
        let mut sub = starters::sub_graph("Edges");
        let power = sub.inputs[0].id;
        assert!(sub.nodes.iter().any(|n| n.kind == NodeKind::SubGraphInput { input: power }));
        remove_pin(&mut sub, false, power);
        assert!(sub.inputs.is_empty() && !sub.nodes.iter().any(|n| matches!(n.kind, NodeKind::SubGraphInput { .. })));
        assert!(!set_numbers(&mut sub, None, Field::PinDefault(99), &[1.0]), "no such input");
        // The search offers Slang, each sub-graph, and in a sub-graph its inputs but no shader outputs.
        let guid: AssetGuid = format!("{:032x}", 3).parse().unwrap();
        let listed = vec![(guid, "Edges.voxshade".to_string(), starters::sub_graph("Edges"))];
        let shader: Vec<String> = offers(&Graph::new("Shader"), &listed).into_iter().map(|o| o.label).collect();
        assert!(shader.contains(&"Slang".to_string()) && shader.contains(&"Edges".to_string()));
        let inside = offers(&starters::sub_graph("Other"), &Vec::new());
        assert!(inside.iter().any(|o| o.folder == "Sub-graph" && o.label == "power"));
        assert!(!inside.iter().any(|o| o.kind == NodeKind::SurfaceOutput));
    }

    #[test]
    fn a_particle_graph_is_offered_particle_inputs_and_outputs_and_its_pictures() {
        let mut graph = starters::particles("Swirl");
        graph.add_property("map", PropertyKind::Picture, [0.0; 4]);
        let offered = offers(&graph, &Vec::new());
        let has = |kind: NodeKind| offered.iter().any(|offer| offer.kind == kind);
        assert!(has(NodeKind::Input { input: Input::Age }) && has(NodeKind::Input { input: Input::DeltaTime }));
        assert!(has(NodeKind::UpdateOutput) && has(NodeKind::LookOutput));
        assert!(!has(NodeKind::Input { input: Input::Uv }) && !has(NodeKind::SurfaceOutput) && !has(NodeKind::Surface));
        assert!(offered.iter().any(|offer| offer.label == "Sample map"));
        // And a shader is offered none of a particle's.
        let shader = offers(&starters::lit("Plain"), &Vec::new());
        assert!(!shader.iter().any(|offer| offer.kind == NodeKind::Input { input: Input::Age }));
        assert!(!shader.iter().any(|offer| offer.kind == NodeKind::LookOutput));
    }

    #[test]
    fn a_picture_wire_runs_only_from_a_picture_to_where_one_is_read() {
        let mut graph = Graph::new("Test");
        let map = graph.add_property("map", PropertyKind::Picture, [0.0; 4]);
        let read = graph.add(NodeKind::Property { property: map }, [0.0; 2]);
        let uv = graph.add(NodeKind::Input { input: Input::Uv }, [0.0; 2]);
        let sample = graph.add(NodeKind::Sample, [0.0; 2]);
        assert!(allows(&graph, (read, 0), (sample, 0)), "a picture into a Sample's picture");
        assert!(!allows(&graph, (uv, 0), (sample, 0)), "numbers are no picture");
        assert!(!allows(&graph, (read, 0), (sample, 1)), "nor a picture a UV");
        assert!(!allows(&graph, (read, 0), (GraphNode(1), 0)), "nor a colour");
        let offered = offers(&graph, &Vec::new());
        assert!(offered.iter().any(|o| o.kind == NodeKind::Sample));
        assert!(offered.iter().any(|o| o.kind == NodeKind::Property { property: map }));
        // A sub-graph's input made a picture by its type's last choice, and back.
        let mut sub = starters::sub_graph("Edges");
        let power = sub.inputs[0].id;
        assert!(choose(&mut sub, None, Field::PinTy(false, power), NUMBER_TYS.len(), &[]));
        assert!(sub.inputs[0].picture);
        assert_eq!(chosen(&sub, None, Field::PinTy(false, power), &[]), Some(NUMBER_TYS.len()));
        assert!(choose(&mut sub, None, Field::PinTy(false, power), 1, &[]));
        assert!(!sub.inputs[0].picture && sub.inputs[0].ty == Ty::Float2);
        // An output gives numbers, whatever is picked.
        let result = sub.outputs[0].id;
        assert!(choose(&mut sub, None, Field::PinTy(true, result), NUMBER_TYS.len(), &[]));
        assert!(!sub.outputs[0].picture);
    }

    #[test]
    fn a_sub_graph_node_takes_its_sub_graphs_ports_as_they_change() {
        let guid: AssetGuid = format!("{:032x}", 3).parse().unwrap();
        let mut sub = starters::sub_graph("Edges");
        let mut graph = Graph::new("Test");
        let kind = NodeKind::SubGraph { graph: guid, name: "Old".into(), inputs: Vec::new(), outputs: Vec::new() };
        let used = graph.add(kind, [0.0; 2]);
        let library: HashMap<_, _> = [(guid, sub.clone())].into();
        assert!(refresh_sub_graphs(&mut graph, &library));
        assert_eq!(graph.node(used).unwrap().kind.title(), "Edges");
        assert_eq!(graph.inputs_of(&graph.node(used).unwrap().kind).len(), 1);
        graph.link(used, 0, GraphNode(1), 1);
        assert!(compile_in(&graph, &library).is_ok());
        // Its output gone, the wire from it goes too.
        sub.outputs.clear();
        let library: HashMap<_, _> = [(guid, sub)].into();
        assert!(refresh_sub_graphs(&mut graph, &library));
        assert!(graph.links.is_empty());
    }

    #[test]
    fn a_compiled_graph_shows_each_nodes_cost_and_what_stops_it_on_its_node() {
        let graph = starters::hologram();
        let compiled = compile(&graph);
        let drawn = describe(&graph, &compiled);
        let samples = drawn.nodes.iter().filter(|n| n.detail.contains("1 picture")).count();
        assert_eq!(samples, 3, "the three reads of the pattern");
        assert!(drawn.nodes.iter().all(|n| n.fault.is_empty()));
        let bar = summary(compiled.as_ref().unwrap());
        assert!(bar.starts_with("Surface: ") && bar.contains("once a material"), "{bar}");
        // The tint made brighter is worked out once a material, not at every pixel.
        assert!(drawn.nodes.iter().any(|n| n.detail == "Once a material"));
        let mut broken = graph.clone();
        let light = broken.add(NodeKind::Input { input: Input::LightColor }, [0.0; 2]);
        let out = broken.nodes.iter().find(|n| n.kind == NodeKind::SurfaceOutput).unwrap().id;
        broken.link(light, 0, out, 5);
        let drawn = describe(&broken, &compile(&broken));
        let at = drawn.node(u64::from(light.0)).unwrap();
        assert!(at.fault.contains("Surface"), "{}", at.fault);
        // The search offers each property by its name, a picture as something to sample.
        let offered: Vec<String> = offers(&graph, &Vec::new()).into_iter().map(|offer| offer.label).collect();
        assert!(offered.contains(&"tint".to_string()) && offered.contains(&"Sample pattern".to_string()));
    }
}
