//! The Generator panel: a World Generator's graph, to build and edit by
//! hand. It opens from a double click on a `.worldgen` in the Project, or
//! from Open Graph in its Inspector.
//!
//! The graph is on a canvas (see `graph_canvas`): nodes with their inputs
//! down the left and outputs down the right, wires between them coloured by
//! what they carry, a picture under each node of what it makes, and, on a
//! node something is wrong with, what is wrong. Beside the canvas the ground
//! the whole graph makes shows in 3D, turned by a drag, over the square of
//! the world the rows under it name (see `generator_previews`).
//!
//! The node picked shows its values in the Inspector, and with none picked
//! the generator shows its own. Every change makes another generator, since
//! one is never changed once made, and the one before is kept to go back
//! to: the graph has its own undo and redo, which Ctrl+Z and Ctrl+Y reach
//! while the panel is the one last pressed in. Changes close together to
//! one value, or to where nodes are, are one step. The file is saved, and
//! the level's ground made again from it, once nothing is held: when a drag
//! ends, not at each step of it.

use std::time::{Duration, Instant};

use voxear_render::Renderer;
use voxear_terrain::generate::{
    Fault, Graph, Link, Node, NodeId as GraphNode, NodeKind, PortKind, PreviewArea, Value, ValueKind, ValueSpec,
    WorldGen,
};
use voxear_ui::{Color, NodeId, Style, Ui};

use crate::{
    asset_inspector::part,
    generator_previews::Previews,
    graph_canvas::{
        CanvasEvent, CanvasGraph, CanvasKey, CanvasNode, CanvasPort, CanvasWire, GraphCanvas, PortRef, node_size,
    },
    inspector::number,
    world_gen_inspector::{NodeEdit, ValueRow},
};

/// Changes to the same thing closer together than this are one step to undo, as an Obj's are.
const MERGE_WITHIN: Duration = Duration::from_millis(800);
/// The most steps kept to undo.
const MOST_STEPS: usize = 200;
/// How far the wheel turns over the ground, in logical pixels, to show a quarter more or less.
const NOTCH: f32 = 40.0;
/// How far from the nodes it copies a duplicate is put.
const BESIDE: f32 = 30.0;

const CENTER_X: ValueSpec = area_row("Center X", f32::MIN, f32::MAX);
const CENTER_Z: ValueSpec = area_row("Center Z", f32::MIN, f32::MAX);
const SIZE: ValueSpec = area_row("Size", SMALLEST, LARGEST);
/// The least and most of the world the previews show, in metres across.
const SMALLEST: f32 = 16.0;
const LARGEST: f32 = 200_000.0;

const fn area_row(label: &'static str, low: f32, high: f32) -> ValueSpec {
    ValueSpec { name: "", label, kind: ValueKind::Number { low, high, unit: "m" }, input: None, advanced: false }
}

/// What a change is to, for changes close together to merge into one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditKey {
    /// Where nodes are.
    Move,
    /// One value of one node.
    Value(GraphNode, usize),
    /// The generator's own rows, in its Inspector.
    Own,
}

/// The generators an open graph has been, to go back to and forward again.
#[derive(Default)]
struct History {
    done: Vec<WorldGen>,
    undone: Vec<WorldGen>,
    /// What the last change was to, and when, for the next to merge with.
    last: Option<(EditKey, Instant)>,
}

impl History {
    /// Notes the generator as it was `before` a change to `key`, unless the change is one more to
    /// what was changed a moment ago.
    fn record(&mut self, before: &WorldGen, key: Option<EditKey>) {
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

    /// Ends the step being merged into, as when a drag is let go.
    fn seal(&mut self) {
        self.last = None;
    }

    fn undo(&mut self, now: &WorldGen) -> Option<WorldGen> {
        let back = self.done.pop()?;
        self.undone.push(now.clone());
        self.last = None;
        Some(back)
    }

    fn redo(&mut self, now: &WorldGen) -> Option<WorldGen> {
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
            .map(|node| (node.id, graph.add(node.kind.clone(), [node.at[0] + by[0], node.at[1] + by[1]])))
            .collect();
        let now = |old: GraphNode| made.iter().find(|(from, _)| *from == old).map(|(_, to)| *to);
        for link in &self.links {
            if let (Some(from), Some(to)) = (now(link.from), now(link.to)) {
                graph.connect(from, link.output, to, link.input);
            }
        }
        made.into_iter().map(|(_, to)| to).collect()
    }
}

/// The graph that is open.
struct Open {
    /// The generator's file, by its path in the project.
    path: String,
    world: WorldGen,
    history: History,
    /// Changed since it was last saved.
    unsaved: bool,
    /// The graph as the canvas draws it, made again when the generator or its pictures change.
    drawn: CanvasGraph,
    stale: bool,
    /// Where the node the search adds goes, and the wire it goes on the end of.
    adding: Option<([f32; 2], Option<PortRef>)>,
}

/// What the panel asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum GeneratorEvent {
    /// Save the generator at this path as it now is.
    Save(String, WorldGen),
    /// Show this node in the Inspector, or with none the generator itself.
    Inspect(Option<GraphNode>),
    /// Open the search for a node to add, under this element.
    Search(NodeId),
}

pub struct GeneratorPanel {
    /// Shown while no graph is open, and the rest while one is.
    hint: NodeId,
    body: NodeId,
    name: NodeId,
    add: NodeId,
    frame: NodeId,
    zoom: NodeId,
    canvas: GraphCanvas,
    /// The ground in 3D: the frame that takes drags and the wheel, its image, and the note of how
    /// fine it is.
    ground_frame: NodeId,
    ground: NodeId,
    ground_note: NodeId,
    /// The preview area's rows: its middle's x and z, and its size.
    area: [ValueRow; 3],
    open: Option<Open>,
    previews: Previews,
    copied: Copied,
    /// The wheel's turns over the ground not yet a whole step.
    turned: f32,
    /// The panel was the last one pressed in, so the keys are its graph's; and they stay its own
    /// through the press that picked a node to add in the search, which is outside it.
    active: bool,
    hold_active: bool,
    /// Whether a graph was open at the last frame, to show the hint or the graph when that changes.
    showing: Option<bool>,
    /// Asked of the editor outside the panel's own frame.
    pending: Vec<GeneratorEvent>,
}

impl GeneratorPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let hint = ui.add_label(root, "Double-click a World Generator in the Project to edit its graph.");
        ui.add_class(hint, "placeholder");
        let body = ui.panel(root, Style::column());
        ui.add_class(body, "fill");
        let bar = ui.panel(body, Style::row());
        ui.add_class(bar, "console-toolbar");
        let icon = ui.icon(bar, "sprout");
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
        let line = ui.panel(body, Style::default());
        ui.add_class(line, "divider");

        // A row whose two sides are as tall as it is.
        let split = ui.panel(body, Style::default());
        ui.add_class(split, "fill");
        ui.add_class(split, "generator-split");
        let canvas = GraphCanvas::new(ui, split);
        let side = ui.panel(split, Style::column());
        ui.add_class(side, "generator-side");
        let ground_frame = ui.panel(side, Style::default());
        ui.add_class(ground_frame, "generator-ground");
        ui.take_wheel(ground_frame, true);
        let ground = ui.image(ground_frame);
        ui.add_class(ground, "generator-ground-image");
        ui.set_visible(ground, false);
        let ground_note = ui.add_label(ground_frame, "");
        ui.add_class(ground_note, "asset-preview-hint");
        let rows = part(ui, side, "Preview", "eye");
        let area = PreviewArea::default();
        let row =
            |ui: &mut Ui, spec: ValueSpec, value: f32| ValueRow::new(ui, rows, spec.label, spec, Value::Number(value));
        let area = [row(ui, CENTER_X, area.center[0]), row(ui, CENTER_Z, area.center[1]), row(ui, SIZE, area.size)];
        ui.set_visible(body, false);
        Self {
            hint,
            body,
            name,
            add,
            frame,
            zoom,
            canvas,
            ground_frame,
            ground,
            ground_note,
            area,
            open: None,
            previews: Previews::default(),
            copied: Copied::default(),
            turned: 0.0,
            active: false,
            hold_active: false,
            showing: None,
            pending: Vec::new(),
        }
    }

    /// Opens the generator at `path`, as `world` reads it: its whole graph in view, nothing picked,
    /// and nothing to undo.
    pub fn open(&mut self, ui: &mut Ui, path: &str, world: WorldGen) {
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.set_text(self.name, file.strip_suffix(".worldgen").unwrap_or(file));
        ui.set_tooltip(self.name, path, None);
        self.open = Some(Open {
            path: path.to_string(),
            world,
            history: History::default(),
            unsaved: false,
            drawn: CanvasGraph::default(),
            stale: true,
            adding: None,
        });
        self.canvas.pick(Vec::new());
        self.canvas.frame_all();
        self.previews.forget();
        self.active = true;
    }

    /// The open generator and its file, when it has a change not yet saved: taken as saved.
    pub fn take_unsaved(&mut self) -> Option<(String, WorldGen)> {
        let open = self.open.as_mut().filter(|open| open.unsaved)?;
        open.unsaved = false;
        Some((open.path.clone(), open.world.clone()))
    }

    /// Closes the open graph, as when its file goes.
    pub fn close(&mut self, renderer: &mut Renderer) {
        self.open = None;
        self.previews.clear(renderer);
        self.canvas.pick(Vec::new());
    }

    /// The open generator's file, by its path in the project.
    pub fn path(&self) -> Option<&str> {
        self.open.as_ref().map(|open| open.path.as_str())
    }

    /// The open generator, as the graph now has it.
    pub fn world(&self) -> Option<&WorldGen> {
        self.open.as_ref().map(|open| &open.world)
    }

    /// The open generator's file renamed or moved to `path`.
    pub fn moved_to(&mut self, ui: &mut Ui, path: &str) {
        if let Some(open) = self.open.as_mut() {
            open.path = path.to_string();
            let file = path.rsplit('/').next().unwrap_or(path);
            ui.set_text(self.name, file.strip_suffix(".worldgen").unwrap_or(file));
            ui.set_tooltip(self.name, path, None);
        }
    }

    /// The nodes picked on the canvas, the last the one pressed last.
    pub fn picked(&self) -> Vec<GraphNode> {
        self.canvas.picked().iter().map(|&id| GraphNode(id as u32)).collect()
    }

    /// Picks these nodes on the canvas.
    pub fn pick(&mut self, nodes: &[GraphNode]) {
        self.canvas.pick(nodes.iter().map(|node| u64::from(node.0)).collect());
    }

    /// Whether the keys are the graph's: the panel was the last one pressed in, with a graph open.
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

    /// A key for the graph, done at the panel's next frame.
    pub fn key(&mut self, key: CanvasKey) {
        self.canvas.key(key);
    }

    /// The canvas, for where its nodes and ports are.
    pub fn canvas(&self) -> &GraphCanvas {
        &self.canvas
    }

    /// The graph as the canvas draws it.
    pub fn drawn(&self) -> Option<&CanvasGraph> {
        self.open.as_ref().map(|open| &open.drawn)
    }

    /// How many places a side the ground shown in 3D is worked out at, and whether every preview
    /// is as fine as it gets.
    pub fn ground_across(&self) -> usize {
        self.previews.ground_across()
    }

    pub fn previews_settled(&self) -> bool {
        self.previews.is_settled()
    }

    /// How far drags have turned the ground, round and towards the eye, in radians.
    pub fn ground_turn(&self) -> [f32; 2] {
        self.previews.turned().to_array()
    }

    /// The open generator becomes `world`, as a step to undo: for a change made outside the panel,
    /// as in the Inspector.
    pub fn change(&mut self, world: WorldGen, key: Option<EditKey>) {
        let Some(open) = self.open.as_mut() else { return };
        if world == open.world {
            return;
        }
        open.history.record(&open.world, key);
        (open.world, open.unsaved, open.stale) = (world, true, true);
    }

    /// The open generator becomes `world`, a generator of another shape, as when a preset is
    /// picked for it: a step of its own to undo, with the whole of its graph in view.
    pub fn replace(&mut self, world: WorldGen) {
        self.change(world, None);
        self.canvas.pick(Vec::new());
        self.canvas.frame_all();
    }

    /// The open generator's file was written elsewhere and now reads as `world`: followed, as a
    /// step to undo, with nothing to save.
    pub fn reread(&mut self, world: WorldGen) {
        let Some(open) = self.open.as_mut() else { return };
        if world != open.world && !open.unsaved {
            open.history.record(&open.world, None);
            (open.world, open.stale) = (world, true);
        }
    }

    /// What the Inspector did to a node's rows: false where the graph took no such change.
    pub fn edit_node(&mut self, node: GraphNode, edit: NodeEdit) -> bool {
        let Some(open) = self.open.as_ref() else { return false };
        let mut graph = open.world.graph().clone();
        let (done, key) = match edit {
            NodeEdit::Set(at, value) => {
                let set = graph.node_mut(node).is_some_and(|found| found.kind.set_value(at, value));
                (set, Some(EditKey::Value(node, at)))
            }
            NodeEdit::Expose(at) => {
                let label = row_label(&graph, node, at);
                (label.is_some_and(|label| graph.expose(node, at, &label)), None)
            }
            NodeEdit::Hide(at) => {
                let before = graph.exposed.len();
                graph.exposed.retain(|shown| !(shown.node == node && shown.value == at));
                (graph.exposed.len() != before, None)
            }
        };
        if done {
            let world = open.world.with_graph(graph);
            self.change(world, key);
        }
        done
    }

    /// Adds a node of this kind where the search was asked for, on the end of the wire it was asked
    /// from when there was one, and picks it.
    pub fn add_node(&mut self, kind: NodeKind) -> Option<GraphNode> {
        let open = self.open.as_mut()?;
        let (mut at, wire) = open.adding.take().unwrap_or_default();
        let mut graph = open.world.graph().clone();
        // On the end of a wire from an input, its outputs are at the place asked for.
        let described = describe_node(&Node { id: GraphNode(0), at, kind: kind.clone() });
        if wire.is_some_and(|wire| !wire.output) {
            at[0] -= node_size(&described)[0];
        }
        let node = graph.add(kind, at);
        if let Some(wire) = wire {
            let other = GraphNode(wire.node as u32);
            let ports = if wire.output { described.inputs.len() } else { described.outputs.len() };
            // The first of its ports the wire can go to.
            let _ = (0..ports).any(|port| match wire.output {
                true => graph.connect(other, wire.index, node, port),
                false => graph.connect(node, port, other, wire.index),
            });
        }
        let world = open.world.with_graph(graph);
        self.change(world, None);
        self.pick(&[node]);
        (self.active, self.hold_active) = (true, true);
        self.pending.push(GeneratorEvent::Inspect(Some(node)));
        Some(node)
    }

    /// Takes back the last change to the graph. False with nothing to take back.
    pub fn undo(&mut self) -> bool {
        let Some(open) = self.open.as_mut() else { return false };
        let Some(back) = open.history.undo(&open.world) else { return false };
        // The area its previews show is no part of a step: it stays as it is.
        (open.world, open.unsaved, open.stale) = (back.with_preview_area(open.world.preview_area()), true, true);
        true
    }

    /// Makes the last change taken back again. False with none taken back.
    pub fn redo(&mut self) -> bool {
        let Some(open) = self.open.as_mut() else { return false };
        let Some(again) = open.history.redo(&open.world) else { return false };
        (open.world, open.unsaved, open.stale) = (again.with_preview_area(open.world.preview_area()), true, true);
        true
    }

    /// The panel's frame: the canvas's, the previews', and what they ask of the editor. `alt` is
    /// whether Alt is held, and `scale` the window's, for how fine the ground is drawn.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        shown: bool,
        renderer: &mut Renderer,
        alt: bool,
        scale: f32,
    ) -> Vec<GeneratorEvent> {
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
            self.update_toolbar(ui);
            self.update_area(ui);
            // Before the canvas, so no node is drawn with a picture from a texture just let go of.
            self.update_previews(ui, renderer, scale);
            self.update_canvas(ui, alt, &mut events);
        }
        // Saved once nothing is held: when a drag ends, not at each step of it.
        let held = ui.holds_press() || self.canvas.is_dragging();
        if let Some(open) = self.open.as_mut().filter(|open| open.unsaved && !held) {
            open.unsaved = false;
            events.push(GeneratorEvent::Save(open.path.clone(), open.world.clone()));
        }
        events
    }

    fn update_toolbar(&mut self, ui: &mut Ui) {
        if ui.clicked(self.add) {
            self.canvas.key(CanvasKey::Search);
        }
        if ui.clicked(self.frame) {
            self.canvas.key(CanvasKey::Frame);
        }
        let zoom = format!("{:.0}%", self.canvas.view().zoom() * 100.0);
        if ui.text(self.zoom) != zoom {
            ui.set_text(self.zoom, &zoom);
        }
    }

    /// The preview area's rows and the wheel over the ground, which shows more of it or less. The
    /// area is kept in the file, but is no step to undo.
    fn update_area(&mut self, ui: &mut Ui) {
        let Some(open) = self.open.as_mut() else { return };
        let before = open.world.preview_area();
        let mut values = [before.center[0], before.center[1], before.size];
        for (row, value) in self.area.iter().zip(&mut values) {
            if let Some(Value::Number(typed)) = row.changed(ui, Value::Number(*value)) {
                *value = typed;
            }
        }
        self.turned += ui.wheeled(self.ground_frame)[1];
        let steps = (self.turned / NOTCH).trunc();
        if steps != 0.0 {
            self.turned -= steps * NOTCH;
            values[2] = (values[2] * 1.25f32.powf(steps)).clamp(SMALLEST, LARGEST).round();
        }
        let area = PreviewArea { center: [values[0], values[1]], size: values[2] };
        if area != before {
            open.world = open.world.with_preview_area(area);
            open.unsaved = true;
        }
        for (row, value) in self.area.iter().zip(values) {
            row.show(ui, Value::Number(value));
        }
    }

    /// The canvas's frame, and what was done on it carried out on the graph.
    fn update_canvas(&mut self, ui: &mut Ui, alt: bool, events: &mut Vec<GeneratorEvent>) {
        let Some(open) = self.open.as_mut() else { return };
        if std::mem::take(&mut open.stale) {
            open.drawn = describe(&open.world, &self.previews);
        }
        self.canvas.set_panning(alt);
        let graph = open.world.graph();
        let allows = |from: PortRef, to: PortRef| {
            let (out, into) = if from.output { (from, to) } else { (to, from) };
            // As the graph itself decides it.
            graph.clone().connect(GraphNode(out.node as u32), out.index, GraphNode(into.node as u32), into.index)
        };
        let done = self.canvas.update(ui, &open.drawn, &allows);
        if done.is_empty() {
            return;
        }
        let mut graph = open.world.graph().clone();
        let mut key = None;
        let mut picked = None;
        let id = |node: u64| GraphNode(node as u32);
        for event in done {
            match event {
                CanvasEvent::Picked => {
                    events.push(GeneratorEvent::Inspect(self.canvas.picked().last().map(|&last| id(last))))
                }
                CanvasEvent::Moved(places) => {
                    for (node, at) in places {
                        if let Some(node) = graph.node_mut(id(node)) {
                            node.at = at;
                        }
                    }
                    key = Some(EditKey::Move);
                }
                CanvasEvent::MoveEnded => open.history.seal(),
                CanvasEvent::Connect { from, to } => {
                    graph.connect(id(from.0), from.1, id(to.0), to.1);
                }
                CanvasEvent::Disconnect { to } => {
                    graph.disconnect(id(to.0), to.1);
                }
                CanvasEvent::Search { at, wire } => {
                    open.adding = Some((at, wire));
                    events.push(GeneratorEvent::Search(self.canvas.anchor()));
                }
                CanvasEvent::Delete(nodes) => nodes.into_iter().for_each(|node| graph.remove(id(node))),
                CanvasEvent::Duplicate(nodes) => {
                    picked = Some(Copied::of(&graph, &nodes).paste(&mut graph, [BESIDE, BESIDE]));
                }
                CanvasEvent::Copy(nodes) => self.copied = Copied::of(&graph, &nodes),
                CanvasEvent::Paste(at) => {
                    // Its top left corner where it was asked for.
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
        if graph != *open.world.graph() {
            let world = open.world.with_graph(graph);
            self.change(world, key);
        }
        if let Some(made) = picked {
            self.pick(&made);
            events.push(GeneratorEvent::Inspect(made.last().copied()));
        }
    }

    /// The previews' frame: the nodes' pictures into the graph drawn, and the ground into its
    /// frame, turned by a drag on it.
    fn update_previews(&mut self, ui: &mut Ui, renderer: &mut Renderer, scale: f32) {
        let Some(open) = self.open.as_mut() else { return };
        let [_, _, width, height] = ui.rect(self.ground_frame);
        let pixels = (width.min(height) * scale).round().max(1.0) as u32;
        if self.previews.update(renderer, &open.world, pixels) {
            open.stale = true;
        }
        self.previews.drag(renderer, ui.is_pressed(self.ground_frame), ui.pointer_position());
        let image = self.previews.ground_image(renderer);
        ui.set_image(self.ground, image);
        ui.set_visible(self.ground, image.is_some());
        let across = self.previews.ground_across();
        let note = if across == 0 { String::new() } else { format!("{across} × {across}") };
        if ui.text(self.ground_note) != note {
            ui.set_text(self.ground_note, &note);
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

    /// The ground's frame, which takes drags that turn it.
    pub fn ground_frame(&self) -> NodeId {
        self.ground_frame
    }
}

/// The label a value takes as a row of its generator: its own, with its node's name before it when
/// another row has that already, and a number after when that is taken too.
fn row_label(graph: &Graph, node: GraphNode, value: usize) -> Option<String> {
    let kind = &graph.node(node)?.kind;
    let own = kind.values().get(value)?.label;
    let taken = |label: &str| {
        graph.exposed.iter().any(|shown| shown.label == label && !(shown.node == node && shown.value == value))
    };
    let named = format!("{} {own}", kind.title());
    let numbered = (2..).map(|number| format!("{named} {number}"));
    [own.to_string(), named.clone()].into_iter().chain(numbered).find(|label| !taken(label))
}

/// What a wire of each kind is coloured.
fn port_color(kind: PortKind) -> Color {
    Color::hex(match kind {
        PortKind::Field => 0x6fb1ff,
        PortKind::Volume => 0xffa24a,
        PortKind::Biome => 0x7fd37a,
        PortKind::Biomes => 0xc58cff,
    })
}

/// A node as the canvas draws it, without its picture.
fn describe_node(node: &Node) -> CanvasNode {
    let ports = |ports: &[voxear_terrain::generate::Port]| -> Vec<CanvasPort> {
        ports.iter().map(|port| CanvasPort { label: port.label.to_string(), color: port_color(port.kind) }).collect()
    };
    CanvasNode {
        id: u64::from(node.id.0),
        at: node.at,
        title: node.kind.title().to_string(),
        inputs: ports(node.kind.inputs()),
        outputs: ports(node.kind.outputs()),
        pictured: !node.kind.outputs().is_empty(),
        ..CanvasNode::default()
    }
}

/// A generator's graph as the canvas draws it: each node with its picture and the numbers it
/// runs between, and what is wrong with the graph on the nodes at fault.
fn describe(world: &WorldGen, previews: &Previews) -> CanvasGraph {
    let graph = world.graph();
    let mut notices = Vec::new();
    let mut faults: Vec<(GraphNode, &str)> = Vec::new();
    for fault in world.faults() {
        match *fault {
            Fault::NoOutput => notices.push("No Output node: the ground lies at the sea level."),
            Fault::Lost(_) => notices.push("A wire comes from a node that is gone."),
            Fault::Loop(node) => faults.push((node, "A wire leads back round to it")),
            Fault::NoGround(node) => faults.push((node, "No height wired in to cool by")),
        }
    }
    let nodes = graph
        .nodes
        .iter()
        .map(|node| {
            let thumb = previews.thumb(node.id, 0);
            let fault = faults.iter().find(|(at, _)| *at == node.id).map_or("", |(_, fault)| fault);
            CanvasNode {
                picture: thumb.map(|thumb| thumb.image),
                detail: thumb.map_or(String::new(), |thumb| format!("{} to {}", short(thumb.low), short(thumb.high))),
                fault: fault.to_string(),
                ..describe_node(node)
            }
        })
        .collect();
    let wire = |link: &Link| CanvasWire {
        from: (u64::from(link.from.0), link.output),
        to: (u64::from(link.to.0), link.input),
    };
    CanvasGraph { nodes, wires: graph.links.iter().map(wire).collect(), notice: notices.join("  ") }
}

/// A number short enough for a node's small text: fewer decimals the larger it is.
fn short(value: f32) -> String {
    match value.abs() {
        size if size >= 100.0 => format!("{value:.0}"),
        size if size >= 10.0 => number((value * 10.0).round() / 10.0),
        _ => number((value * 100.0).round() / 100.0),
    }
}

#[cfg(test)]
mod tests {
    use voxear_terrain::generate::{Preset, nodes};

    use super::*;

    #[test]
    fn changes_close_together_to_one_thing_are_one_step_to_undo() {
        let first = Preset::RollingHills.world();
        let (second, third, fourth) = (first.with_seed(2), first.with_seed(3), first.with_seed(4));
        let mut history = History::default();
        // Three changes to one value in a row are one step; a change to another is its own.
        let key = Some(EditKey::Value(GraphNode(1), 1));
        history.record(&first, key);
        history.record(&second, key);
        history.record(&third, Some(EditKey::Value(GraphNode(1), 2)));
        assert_eq!(history.done, [first.clone(), third.clone()]);
        assert_eq!(history.undo(&fourth), Some(third.clone()));
        assert_eq!(history.undo(&third), Some(first.clone()));
        assert_eq!(history.undo(&first), None);
        assert_eq!(history.redo(&first), Some(third.clone()));
        assert_eq!(history.redo(&third), Some(fourth.clone()));
        assert_eq!(history.redo(&fourth), None);
        // A change after an undo forgets what was undone; and a step sealed, as a drag let go is, merges no more.
        history.undo(&fourth);
        history.record(&third, Some(EditKey::Move));
        assert_eq!(history.redo(&second), None);
        history.seal();
        history.record(&second, Some(EditKey::Move));
        assert_eq!(history.done.len(), 3);
        // A change with nothing to merge by is always a step, and so is one made later.
        history.record(&first, None);
        history.record(&first, None);
        assert_eq!(history.done.len(), 5);
        history.last = Some((EditKey::Move, Instant::now() - MERGE_WITHIN * 2));
        history.record(&first, Some(EditKey::Move));
        assert_eq!(history.done.len(), 6);
    }

    #[test]
    fn copied_nodes_paste_wired_as_they_were_and_a_row_takes_a_label_no_other_has() {
        let mut graph = Graph::default();
        let noise = graph.add(NodeKind::Noise(nodes::Noise::default()), [0.0, 0.0]);
        let math = graph.add(NodeKind::Math(nodes::Math::default()), [200.0, 40.0]);
        let output = graph.add(NodeKind::Output(nodes::Output::default()), [400.0, 0.0]);
        assert!(graph.connect(noise, 0, math, 0) && graph.connect(math, 0, output, 0));
        let copied = Copied::of(&graph, &[u64::from(noise.0), u64::from(math.0)]);
        assert_eq!((copied.nodes.len(), copied.links.len()), (2, 1), "the wire to the Output is not among them");
        let made = copied.paste(&mut graph, [30.0, 30.0]);
        assert_eq!(made, [GraphNode(4), GraphNode(5)]);
        assert_eq!(graph.node(made[1]).map(|node| node.at), Some([230.0, 70.0]));
        assert_eq!(graph.link_into(made[1], 0).map(|link| link.from), Some(made[0]));
        assert_eq!(graph.link_into(output, 0).map(|link| link.from), Some(math), "the first still feeds it");

        // Two Noise nodes' Size: the first is Size, the second says whose it is.
        assert_eq!(row_label(&graph, noise, 1).as_deref(), Some("Size"));
        assert!(graph.expose(noise, 1, "Size"));
        assert_eq!(row_label(&graph, noise, 1).as_deref(), Some("Size"), "its own label is not taken from it");
        assert_eq!(row_label(&graph, made[0], 1).as_deref(), Some("Noise Size"));
        assert!(graph.expose(made[0], 1, "Noise Size"));
        let third = graph.add(NodeKind::Noise(nodes::Noise::default()), [0.0, 300.0]);
        assert_eq!(row_label(&graph, third, 1).as_deref(), Some("Noise Size 2"));
        assert_eq!(row_label(&graph, third, 99), None);

        // As the canvas draws it: a title, its ports coloured by what they carry, and room for its picture.
        let drawn = describe_node(graph.node(math).unwrap());
        assert_eq!(
            (drawn.title.as_str(), drawn.inputs.len(), drawn.outputs.len(), drawn.pictured),
            ("Math", 2, 1, true)
        );
        assert_eq!(drawn.inputs[0].color, port_color(PortKind::Field));
        assert!(!describe_node(graph.node(output).unwrap()).pictured, "nothing comes out of an Output");
        assert_eq!([short(1234.56), short(12.345), short(0.5), short(-3.0)], ["1235", "12.3", "0.5", "-3"]);
    }
}
