//! A canvas for a graph of nodes: boxes with ports down their sides, joined
//! by wires, to look round and to edit by hand. It knows nothing of what
//! the nodes are. Whatever owns a graph describes it each frame as a
//! [`CanvasGraph`], and hears back what was done to it as [`CanvasEvent`]s:
//! nodes moved, a wire drawn or pulled off, a search asked for where a new
//! node should go, and what the keys asked of the picked nodes. What a wire
//! may join is the owner's to say, through the `allows` it hands
//! [`GraphCanvas::update`].
//!
//! - The wheel zooms about the pointer, in steps.
//! - A drag with the middle button, or with Alt held, moves the view.
//! - A press on a node picks it (Shift or Ctrl adds it, or takes it out);
//!   a drag moves every picked node, snapped to a grid.
//! - A drag over empty canvas picks the nodes its box touches.
//! - A drag from a port draws a wire, which lands on a port that takes it,
//!   or on a node with one; let go over empty canvas it asks for the
//!   search, to add a node on its end. A drag from a wired input pulls the
//!   wire off it.
//! - A context click asks for the search.
//!
//! The UI cannot scale a subtree and has no curve, so the canvas is drawn:
//! each node is an element placed by hand whose box, header and ports are
//! shapes, with labels and a picture from a pool laid over them at a font
//! size that follows the zoom, and each wire is a run of short lines.

use voxear_ui::{Color, ImageRef, NodeId, Overflow, Shape, Style, Ui};

/// How large the view can be, least to most; it starts at 1.
const ZOOMS: [f32; 11] = [0.25, 0.35, 0.5, 0.6, 0.7, 0.85, 1.0, 1.2, 1.4, 1.7, 2.0];
const WHOLE: usize = 6;
/// How far the wheel turns, in logical pixels, for one step of zoom.
const NOTCH: f32 = 40.0;
/// A node's parts, in canvas units, which are logical pixels at a zoom of 1.
const HEADER: f32 = 26.0;
const ROW: f32 = 20.0;
const PAD: f32 = 8.0;
const THUMB: f32 = 128.0;
const LINE: f32 = 16.0;
const MIN_WIDTH: f32 = THUMB + PAD * 2.0;
const RADIUS: f32 = 6.0;
/// A port's dot, and how far round it a press takes hold of it, in pixels on screen.
const PORT: f32 = 5.0;
const GRAB: f32 = 10.0;
/// About how wide a letter of a label is at a zoom of 1, to make a node wide enough for its labels.
const LETTER: f32 = 6.3;
const TEXT: f32 = 11.0;
/// Nodes sit on a grid this fine.
const SNAP: f32 = 10.0;
/// How far the pointer moves, pressed, before it drags rather than clicks, in pixels on screen.
const DRAG_START: f32 = 3.0;
/// The lines a wire is drawn as.
const SEGMENTS: usize = 20;
/// The room left round a graph framed in the view, in pixels on screen.
const MARGIN: f32 = 48.0;
/// Below this zoom the ports' labels are left out, and below the next every label.
const LABELS_FROM: f32 = 0.5;
const TITLES_FROM: f32 = 0.35;
/// Below this zoom only every fifth line of the grid is drawn.
const FINE_GRID_FROM: f32 = 0.7;

/// One end of a wire: an input or an output of a node, by its place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortRef {
    pub node: u64,
    pub output: bool,
    pub index: usize,
}

/// A port as the canvas draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasPort {
    pub label: String,
    pub color: Color,
}

/// A node as the canvas draws it: its place, its title, its ports, the
/// picture under them (with room kept for it while `pictured`), a line of
/// small text under that, and what is wrong with it, if anything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasNode {
    pub id: u64,
    pub at: [f32; 2],
    pub title: String,
    pub inputs: Vec<CanvasPort>,
    pub outputs: Vec<CanvasPort>,
    pub pictured: bool,
    pub picture: Option<ImageRef>,
    pub detail: String,
    pub fault: String,
}

/// A wire, from an output of one node into an input of another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasWire {
    pub from: (u64, usize),
    pub to: (u64, usize),
}

/// A graph as the canvas draws it, with a line to show over its corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasGraph {
    pub nodes: Vec<CanvasNode>,
    pub wires: Vec<CanvasWire>,
    pub notice: String,
}

impl CanvasGraph {
    pub fn node(&self, id: u64) -> Option<&CanvasNode> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// What was done on the canvas, for the graph's owner to carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasEvent {
    /// A node was pressed, or other nodes are picked now.
    Picked,
    /// Nodes are being dragged: where each is now.
    Moved(Vec<(u64, [f32; 2])>),
    /// The drag that moved them ended.
    MoveEnded,
    /// A wire drawn from an output into an input.
    Connect {
        from: (u64, usize),
        to: (u64, usize),
    },
    /// The wire into an input pulled off.
    Disconnect {
        to: (u64, usize),
    },
    /// The search asked for, to add a node at this place; on the end of a
    /// wire, when one was let go there.
    Search {
        at: [f32; 2],
        wire: Option<PortRef>,
    },
    Delete(Vec<u64>),
    Duplicate(Vec<u64>),
    Copy(Vec<u64>),
    /// What was copied wanted at this place.
    Paste([f32; 2]),
}

/// What a key asks of the canvas (see [`GraphCanvas::key`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasKey {
    Delete,
    Duplicate,
    Copy,
    Paste,
    /// Shows the picked nodes, or all of them with none picked.
    Frame,
    Search,
}

/// What of the canvas the view shows: the place at its top left corner, and how large.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub origin: [f32; 2],
    step: usize,
}

impl Default for View {
    fn default() -> Self {
        Self { origin: [0.0, 0.0], step: WHOLE }
    }
}

impl View {
    pub fn zoom(&self) -> f32 {
        ZOOMS[self.step]
    }

    /// Where a place on the canvas is in the view, in pixels from its corner.
    pub fn to_screen(&self, at: [f32; 2]) -> [f32; 2] {
        [(at[0] - self.origin[0]) * self.zoom(), (at[1] - self.origin[1]) * self.zoom()]
    }

    /// The place on the canvas under a point of the view.
    pub fn to_canvas(&self, point: [f32; 2]) -> [f32; 2] {
        [point[0] / self.zoom() + self.origin[0], point[1] / self.zoom() + self.origin[1]]
    }

    /// Zoomed `steps` further in (or out, below 0), the place under `point` staying under it.
    pub fn zoomed(&self, point: [f32; 2], steps: i32) -> Self {
        let step = (self.step as i32 + steps).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        let under = self.to_canvas(point);
        let zoom = ZOOMS[step];
        Self { origin: [under[0] - point[0] / zoom, under[1] - point[1] / zoom], step }
    }

    /// Moved by `by` pixels of the view.
    pub fn moved(&self, by: [f32; 2]) -> Self {
        Self { origin: [self.origin[0] - by[0] / self.zoom(), self.origin[1] - by[1] / self.zoom()], ..*self }
    }

    /// Showing all of `bounds` (left, top, right, bottom on the canvas) in the middle of a view
    /// `size` pixels large, as large as it fits, and no larger than life.
    pub fn framing(bounds: [f32; 4], size: [f32; 2]) -> Self {
        let (wide, high) = ((bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0));
        let room = [(size[0] - MARGIN * 2.0).max(1.0), (size[1] - MARGIN * 2.0).max(1.0)];
        let fits = |zoom: f32| wide * zoom <= room[0] && high * zoom <= room[1];
        let step = (0..=WHOLE).rev().find(|&step| fits(ZOOMS[step])).unwrap_or(0);
        let zoom = ZOOMS[step];
        let middle = [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0];
        Self { origin: [middle[0] - size[0] / zoom / 2.0, middle[1] - size[1] / zoom / 2.0], step }
    }
}

/// How large a node is drawn, in canvas units.
pub fn node_size(node: &CanvasNode) -> [f32; 2] {
    let rows = node.inputs.len().max(node.outputs.len());
    let wide = |port: Option<&CanvasPort>| port.map_or(0.0, |port| port.label.chars().count() as f32 * LETTER);
    let labels = (0..rows).map(|row| wide(node.inputs.get(row)) + wide(node.outputs.get(row)) + PAD * 5.0);
    let title = node.title.chars().count() as f32 * (LETTER + 0.6) + PAD * 2.0;
    let width = labels.fold(MIN_WIDTH.max(title), f32::max);
    let mut height = HEADER + rows as f32 * ROW + PAD / 2.0;
    if node.pictured {
        height += THUMB + PAD;
    }
    if !node.detail.is_empty() {
        height += LINE;
    }
    if !node.fault.is_empty() {
        height += LINE + 2.0;
    }
    [(width / SNAP).ceil() * SNAP, height]
}

/// Where a port's dot is on the canvas: inputs down a node's left side, outputs down its right.
pub fn port_at(node: &CanvasNode, output: bool, index: usize) -> [f32; 2] {
    let x = if output { node.at[0] + node_size(node)[0] } else { node.at[0] };
    [x, node.at[1] + HEADER + (index as f32 + 0.5) * ROW]
}

/// What is under a place on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Port(PortRef),
    Node(u64),
    Nothing,
}

/// What of `graph` is at `at`, its nodes lying over one another in `order` (the last on top): the
/// nearest port within `grab` of it, or else the node it is on.
pub fn hit(graph: &CanvasGraph, order: &[usize], at: [f32; 2], grab: f32) -> Hit {
    let mut nearest: Option<(f32, PortRef)> = None;
    for node in &graph.nodes {
        let ports = [(false, node.inputs.len()), (true, node.outputs.len())];
        for (output, index) in ports.into_iter().flat_map(|(output, count)| (0..count).map(move |i| (output, i))) {
            let port = port_at(node, output, index);
            let away = (port[0] - at[0]).hypot(port[1] - at[1]);
            if away <= grab && nearest.is_none_or(|(least, _)| away < least) {
                nearest = Some((away, PortRef { node: node.id, output, index }));
            }
        }
    }
    if let Some((_, port)) = nearest {
        return Hit::Port(port);
    }
    let inside = |node: &CanvasNode| {
        let [width, height] = node_size(node);
        (node.at[0]..node.at[0] + width).contains(&at[0]) && (node.at[1]..node.at[1] + height).contains(&at[1])
    };
    let on = order.iter().rev().filter_map(|&index| graph.nodes.get(index)).find(|node| inside(node));
    on.map_or(Hit::Nothing, |node| Hit::Node(node.id))
}

/// The points a wire is drawn through from an output at `from` to an input at `to`: a curve that
/// leaves and arrives level.
pub fn wire_points(from: [f32; 2], to: [f32; 2]) -> Vec<[f32; 2]> {
    let reach = ((to[0] - from[0]).abs() * 0.5).clamp(30.0, 160.0);
    let (a, b) = ([from[0] + reach, from[1]], [to[0] - reach, to[1]]);
    (0..=SEGMENTS)
        .map(|step| {
            let t = step as f32 / SEGMENTS as f32;
            let s = 1.0 - t;
            let along = |axis: usize| {
                s * s * s * from[axis] + 3.0 * s * s * t * a[axis] + 3.0 * s * t * t * b[axis] + t * t * t * to[axis]
            };
            [along(0), along(1)]
        })
        .collect()
}

/// The nodes a box from `a` to `b` on the canvas touches.
pub fn boxed(graph: &CanvasGraph, a: [f32; 2], b: [f32; 2]) -> Vec<u64> {
    let (low, high) = ([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]);
    let touches = |node: &CanvasNode| {
        let [width, height] = node_size(node);
        node.at[0] < high[0] && node.at[0] + width > low[0] && node.at[1] < high[1] && node.at[1] + height > low[1]
    };
    graph.nodes.iter().filter(|node| touches(node)).map(|node| node.id).collect()
}

/// The box round these nodes, or round every node when `ids` is empty: left, top, right, bottom.
pub fn bounds(graph: &CanvasGraph, ids: &[u64]) -> Option<[f32; 4]> {
    let among = graph.nodes.iter().filter(|node| ids.is_empty() || ids.contains(&node.id));
    among.fold(None, |found: Option<[f32; 4]>, node| {
        let [width, height] = node_size(node);
        let own = [node.at[0], node.at[1], node.at[0] + width, node.at[1] + height];
        Some(found.map_or(own, |b| [b[0].min(own[0]), b[1].min(own[1]), b[2].max(own[2]), b[3].max(own[3])]))
    })
}

/// A place on the canvas's grid.
pub fn snapped(at: [f32; 2]) -> [f32; 2] {
    [(at[0] / SNAP).round() * SNAP, (at[1] / SNAP).round() * SNAP]
}

/// What a press that is held is doing.
#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// On a node: the picked nodes follow the pointer once it has moved, each from where it was;
    /// let go without moving, `alone` is the one node left picked.
    Nodes { from: [f32; 2], starts: Vec<(u64, [f32; 2])>, moved: bool, alone: Option<u64> },
    /// A wire from a port, and the input it was pulled off, if it was.
    Wire { from: PortRef, taken: Option<(u64, usize)> },
    /// A box from this place on the canvas, added to what was picked.
    Marquee { from: [f32; 2], kept: Vec<u64> },
    /// The view, by the pointer, from where it last was on screen.
    Pan { last: [f32; 2] },
}

/// The canvas's colours, for a dark theme or a light one. They are whole colours, not faint ones
/// laid over the ground: a faint white blends far brighter than it reads.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
    grid: Color,
    grid_major: Color,
    body: Color,
    header: Color,
    /// A picked node's header.
    header_picked: Color,
    edge: Color,
    picked: Color,
    fault: Color,
    marquee: Color,
}

impl Colors {
    fn of(dark: bool) -> Self {
        let pick = |dark_one: u32, light_one: u32| Color::hex(if dark { dark_one } else { light_one });
        Self {
            grid: pick(0x121418, 0xeceef1),
            grid_major: pick(0x181b20, 0xe2e5e9),
            body: pick(0x1a1d22, 0xffffff),
            header: pick(0x282c34, 0xe6e8ee),
            header_picked: pick(0x39407a, 0xc9cdf3),
            edge: pick(0x30353d, 0xc3c7cf),
            picked: pick(0x7988ff, 0x5e6ad2),
            fault: Color::hex(0xe5484d),
            marquee: pick(0x8b93ff, 0x5e6ad2),
        }
    }
}

/// One node's elements, from the pool: its panel, title, the labels of its ports, its picture and
/// its two lines of small text.
struct Drawn {
    panel: NodeId,
    title: NodeId,
    labels: Vec<NodeId>,
    picture: NodeId,
    detail: NodeId,
    fault: NodeId,
}

pub struct GraphCanvas {
    /// The view: it clips what is drawn in it, and takes the pointer and the wheel.
    viewport: NodeId,
    /// Under the nodes, the grid and the wires; the nodes; and over them the wire or box being
    /// dragged, the notice, and the place a search opens from.
    wires: NodeId,
    nodes: NodeId,
    over: NodeId,
    notice: NodeId,
    anchor: NodeId,
    pool: Vec<Drawn>,
    view: View,
    picked: Vec<u64>,
    drag: Option<Drag>,
    /// Whether the buttons were down last frame, and where the pointer was, in the view.
    was_pressed: bool,
    pointer: Option<[f32; 2]>,
    /// The wheel's turns not yet a whole step.
    turned: f32,
    hovered: Hit,
    /// Alt is held, so a drag moves the view.
    panning: bool,
    keys: Vec<CanvasKey>,
    /// The view is to show the picked nodes, or all, once it knows how large it is.
    frame: Option<bool>,
    dark: bool,
    /// What was last drawn, to draw again only what changed.
    drawn: Option<(CanvasGraph, Look)>,
}

/// Everything but the graph that decides how the canvas looks.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    view: View,
    size: [f32; 2],
    picked: Vec<u64>,
    hovered: Hit,
    drag: Option<Drag>,
    pointer: Option<[f32; 2]>,
    dark: bool,
}

impl GraphCanvas {
    /// A canvas filling `parent`.
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let mut style = Style::default().grow();
        style.overflow = Overflow::Hidden;
        let viewport = ui.panel(parent, style);
        ui.add_class(viewport, "graph-canvas");
        ui.take_wheel(viewport, true);
        let layer = |ui: &mut Ui, class: &str| {
            let layer = ui.panel(viewport, Style::cover());
            ui.add_class(layer, class);
            layer
        };
        let (wires, nodes, over) = (layer(ui, "graph-wires"), layer(ui, "graph-nodes"), layer(ui, "graph-over"));
        let notice = ui.add_label(over, "");
        ui.add_class(notice, "graph-notice");
        ui.set_visible(notice, false);
        let mut hidden = Style::default().at(0.0, 0.0).width(1.0).height(1.0);
        hidden.pickable = false;
        let anchor = ui.panel(over, hidden);
        Self {
            viewport,
            wires,
            nodes,
            over,
            notice,
            anchor,
            pool: Vec::new(),
            view: View::default(),
            picked: Vec::new(),
            drag: None,
            was_pressed: false,
            pointer: None,
            turned: 0.0,
            hovered: Hit::Nothing,
            panning: false,
            keys: Vec::new(),
            frame: Some(false),
            dark: true,
            drawn: None,
        }
    }

    /// The element the canvas is drawn in, which takes its presses.
    pub fn viewport(&self) -> NodeId {
        self.viewport
    }

    /// A point-sized element where the last search was asked for, for it to open under.
    pub fn anchor(&self) -> NodeId {
        self.anchor
    }

    pub fn picked(&self) -> &[u64] {
        &self.picked
    }

    /// Picks these nodes, the last the one on top.
    pub fn pick(&mut self, ids: Vec<u64>) {
        self.picked = ids;
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Whether Alt is held, which makes a drag move the view.
    pub fn set_panning(&mut self, held: bool) {
        self.panning = held;
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
    }

    /// Whether a press on the canvas is held, dragging something.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// A key for the canvas, done at its next frame.
    pub fn key(&mut self, key: CanvasKey) {
        self.keys.push(key);
    }

    /// Shows the whole graph at the next frame, as F does with nothing picked.
    pub fn frame_all(&mut self) {
        self.frame = Some(false);
    }

    /// The order nodes are drawn in, the last on top: as the graph has them, then the picked ones.
    fn order(&self, graph: &CanvasGraph) -> Vec<usize> {
        let picked = |index: &usize| self.picked.contains(&graph.nodes[*index].id);
        let (top, under): (Vec<usize>, Vec<usize>) = (0..graph.nodes.len()).partition(picked);
        under.into_iter().chain(top).collect()
    }

    /// Where a port's dot is in the window, for a test to press on it.
    pub fn port_point(&self, ui: &Ui, graph: &CanvasGraph, port: PortRef) -> Option<(f32, f32)> {
        let [x, y, ..] = ui.rect(self.viewport);
        let at = self.view.to_screen(port_at(graph.node(port.node)?, port.output, port.index));
        Some((x + at[0], y + at[1]))
    }

    /// A point on a node's header in the window, for a test to press on it.
    pub fn node_point(&self, ui: &Ui, graph: &CanvasGraph, id: u64) -> Option<(f32, f32)> {
        let [x, y, ..] = ui.rect(self.viewport);
        let node = graph.node(id)?;
        let at = self.view.to_screen([node.at[0] + node_size(node)[0] / 2.0, node.at[1] + HEADER / 2.0]);
        Some((x + at[0], y + at[1]))
    }

    /// The canvas's frame: follows the pointer, the wheel and the keys, and draws `graph`.
    /// `allows` says whether a wire may join two ports, the first the one it is dragged from.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        graph: &CanvasGraph,
        allows: &dyn Fn(PortRef, PortRef) -> bool,
    ) -> Vec<CanvasEvent> {
        let mut events = Vec::new();
        let [left, top, width, height] = ui.rect(self.viewport);
        if width <= 0.0 || height <= 0.0 {
            return events;
        }
        let size = [width, height];
        if let Some(picked_only) = self.frame.take() {
            let ids: &[u64] = if picked_only { &self.picked } else { &[] };
            if let Some(bounds) = bounds(graph, ids).or_else(|| bounds(graph, &[])) {
                self.view = View::framing(bounds, size);
            }
        }
        let inside = |point: &[f32; 2]| (0.0..width).contains(&point[0]) && (0.0..height).contains(&point[1]);
        let pointer = ui.pointer_position().map(|(x, y)| [x - left, y - top]);
        let over = pointer.filter(|point| inside(point) && ui.is_hovered(self.viewport));

        // The wheel: a step for each notch, about the pointer.
        self.turned += ui.wheeled(self.viewport)[1];
        let steps = (self.turned / NOTCH).trunc();
        if steps != 0.0 {
            self.turned -= steps * NOTCH;
            self.view = self.view.zoomed(over.unwrap_or([width / 2.0, height / 2.0]), -(steps as i32));
        }

        let grab = GRAB / self.view.zoom();
        let order = self.order(graph);
        let (primary, middle) = (ui.is_pressed(self.viewport), ui.is_middle_pressed(self.viewport));
        let pressed = primary || middle;
        let modifiers = ui.modifiers();
        if pressed && !self.was_pressed {
            if let Some(point) = pointer {
                let at = self.view.to_canvas(point);
                self.drag = Some(if middle || self.panning {
                    Drag::Pan { last: point }
                } else {
                    match hit(graph, &order, at, grab) {
                        Hit::Port(port) => self.take_wire(graph, port),
                        Hit::Node(id) => {
                            let adding = modifiers.shift || modifiers.control;
                            let was_picked = self.picked.contains(&id);
                            match (adding, was_picked) {
                                (true, true) => self.picked.retain(|&other| other != id),
                                (true, false) => self.picked.push(id),
                                (false, false) => self.picked = vec![id],
                                (false, true) => {}
                            }
                            events.push(CanvasEvent::Picked);
                            let place = |id: &u64| graph.node(*id).map(|node| (*id, node.at));
                            let starts = self.picked.iter().filter_map(place).collect();
                            let alone = (!adding && was_picked && self.picked.len() > 1).then_some(id);
                            Drag::Nodes { from: at, starts, moved: false, alone }
                        }
                        Hit::Nothing => {
                            let kept =
                                if modifiers.shift || modifiers.control { self.picked.clone() } else { Vec::new() };
                            Drag::Marquee { from: at, kept }
                        }
                    }
                });
            }
        }
        // The pointer where it was last seen, for a release after it left the window.
        if pointer.is_some() {
            self.pointer = pointer;
        }
        if self.drag.is_some()
            && let Some(point) = self.pointer
        {
            self.dragged(graph, point, &mut events);
        }
        if !pressed && self.was_pressed {
            if let (Some(drag), Some(point)) = (self.drag.take(), self.pointer) {
                self.let_go(ui, graph, &order, drag, self.view.to_canvas(point), allows, &mut events);
            }
            self.drag = None;
        }
        self.was_pressed = pressed;
        self.hovered = match (over, &self.drag) {
            (Some(point), None | Some(Drag::Wire { .. })) => hit(graph, &order, self.view.to_canvas(point), grab),
            _ => Hit::Nothing,
        };

        let middle_of = self.view.to_canvas([width / 2.0, height / 2.0]);
        let here = over.map_or(middle_of, |point| self.view.to_canvas(point));
        if ui.context_clicked(self.viewport) && self.drag.is_none() {
            self.search(ui, here, None, &mut events);
        }
        for key in std::mem::take(&mut self.keys) {
            let picked = self.picked.clone();
            match key {
                CanvasKey::Delete if !picked.is_empty() => events.push(CanvasEvent::Delete(picked)),
                CanvasKey::Duplicate if !picked.is_empty() => events.push(CanvasEvent::Duplicate(picked)),
                CanvasKey::Copy if !picked.is_empty() => events.push(CanvasEvent::Copy(picked)),
                CanvasKey::Paste => events.push(CanvasEvent::Paste(snapped(here))),
                CanvasKey::Search => self.search(ui, here, None, &mut events),
                CanvasKey::Frame => {
                    if let Some(bounds) = bounds(graph, &picked).or_else(|| bounds(graph, &[])) {
                        self.view = View::framing(bounds, size);
                    }
                }
                CanvasKey::Delete | CanvasKey::Duplicate | CanvasKey::Copy => {}
            }
        }
        // Nodes that went are picked no longer.
        let before = self.picked.len();
        self.picked.retain(|id| graph.node(*id).is_some());
        if self.picked.len() != before {
            events.push(CanvasEvent::Picked);
        }

        let look = Look {
            view: self.view,
            size,
            picked: self.picked.clone(),
            hovered: self.hovered,
            drag: self.drag.clone(),
            pointer: self.pointer.filter(|_| self.drag.is_some()),
            dark: self.dark,
        };
        if self.drawn.as_ref().is_none_or(|(drawn, looked)| drawn != graph || *looked != look) {
            self.draw(ui, graph, &look, allows);
            self.drawn = Some((graph.clone(), look));
        }
        events
    }

    /// The drag a press on a port begins: a wire from it, or from the far end of the wire into it,
    /// pulled off.
    fn take_wire(&self, graph: &CanvasGraph, port: PortRef) -> Drag {
        let wired = graph.wires.iter().find(|wire| !port.output && wire.to == (port.node, port.index));
        match wired {
            Some(wire) => Drag::Wire {
                from: PortRef { node: wire.from.0, output: true, index: wire.from.1 },
                taken: Some(wire.to),
            },
            None => Drag::Wire { from: port, taken: None },
        }
    }

    /// What a held press does as the pointer moves.
    fn dragged(&mut self, graph: &CanvasGraph, point: [f32; 2], events: &mut Vec<CanvasEvent>) {
        let at = self.view.to_canvas(point);
        match &mut self.drag {
            Some(Drag::Pan { last }) => {
                let by = [point[0] - last[0], point[1] - last[1]];
                *last = point;
                self.view = self.view.moved(by);
            }
            Some(Drag::Nodes { from, starts, moved, .. }) => {
                let by = [at[0] - from[0], at[1] - from[1]];
                if !*moved && by[0].hypot(by[1]) * self.view.zoom() < DRAG_START {
                    return;
                }
                *moved = true;
                let places: Vec<(u64, [f32; 2])> =
                    starts.iter().map(|&(id, start)| (id, snapped([start[0] + by[0], start[1] + by[1]]))).collect();
                let stays = |(id, place): &(u64, [f32; 2])| graph.node(*id).is_some_and(|node| node.at == *place);
                if !places.iter().all(stays) {
                    events.push(CanvasEvent::Moved(places));
                }
            }
            Some(Drag::Marquee { from, kept }) => {
                let mut picked = kept.clone();
                picked.extend(boxed(graph, *from, at).into_iter().filter(|id| !kept.contains(id)));
                if picked != self.picked {
                    self.picked = picked;
                    events.push(CanvasEvent::Picked);
                }
            }
            Some(Drag::Wire { .. }) | None => {}
        }
    }

    /// What letting go of a drag does, at `at` on the canvas.
    #[allow(clippy::too_many_arguments)]
    fn let_go(
        &mut self,
        ui: &mut Ui,
        graph: &CanvasGraph,
        order: &[usize],
        drag: Drag,
        at: [f32; 2],
        allows: &dyn Fn(PortRef, PortRef) -> bool,
        events: &mut Vec<CanvasEvent>,
    ) {
        match drag {
            Drag::Nodes { moved: true, .. } => events.push(CanvasEvent::MoveEnded),
            Drag::Nodes { alone: Some(id), .. } => {
                self.picked = vec![id];
                events.push(CanvasEvent::Picked);
            }
            Drag::Wire { from, taken } => {
                let landed = self.landing(graph, order, from, at, allows);
                let joined = |to: PortRef| {
                    let (out, into) = if from.output { (from, to) } else { (to, from) };
                    ((out.node, out.index), (into.node, into.index))
                };
                match (landed.map(joined), taken) {
                    // Put back where it was pulled from.
                    (Some((_, to)), Some(taken)) if to == taken => {}
                    (Some((from, to)), taken) => {
                        events.extend(taken.map(|to| CanvasEvent::Disconnect { to }));
                        events.push(CanvasEvent::Connect { from, to });
                    }
                    (None, Some(to)) => events.push(CanvasEvent::Disconnect { to }),
                    (None, None) if hit(graph, order, at, GRAB / self.view.zoom()) == Hit::Nothing => {
                        // Not a wire at all when it was let go where it began.
                        let start = graph.node(from.node).map(|node| port_at(node, from.output, from.index));
                        let far = start.is_none_or(|start| (start[0] - at[0]).hypot(start[1] - at[1]) > GRAB * 2.0);
                        if far {
                            self.search(ui, at, Some(from), events);
                        }
                    }
                    (None, None) => {}
                }
            }
            Drag::Nodes { .. } | Drag::Marquee { .. } | Drag::Pan { .. } => {}
        }
    }

    /// The port a wire from `from` lands on when let go at `at`: the one there, or the first of the
    /// node there, that takes it.
    fn landing(
        &self,
        graph: &CanvasGraph,
        order: &[usize],
        from: PortRef,
        at: [f32; 2],
        allows: &dyn Fn(PortRef, PortRef) -> bool,
    ) -> Option<PortRef> {
        match hit(graph, order, at, GRAB / self.view.zoom()) {
            Hit::Port(port) => (port.output != from.output && allows(from, port)).then_some(port),
            Hit::Node(id) => {
                let node = graph.node(id)?;
                let count = if from.output { node.inputs.len() } else { node.outputs.len() };
                let ports = (0..count).map(|index| PortRef { node: id, output: !from.output, index });
                // An input with no wire yet before one that has.
                let wired = |port: &PortRef| graph.wires.iter().any(|wire| wire.to == (port.node, port.index));
                let fits: Vec<PortRef> = ports.filter(|&port| allows(from, port)).collect();
                fits.iter().copied().find(|port| port.output || !wired(port)).or(fits.first().copied())
            }
            Hit::Nothing => None,
        }
    }

    /// Asks for the search at a place on the canvas, with the anchor put there for it to open under.
    fn search(&mut self, ui: &mut Ui, at: [f32; 2], wire: Option<PortRef>, events: &mut Vec<CanvasEvent>) {
        let [x, y] = self.view.to_screen(at);
        ui.set_inline_style(self.anchor, &format!("left: {x}px; top: {y}px"));
        events.push(CanvasEvent::Search { at: snapped(at), wire });
    }

    /// Draws the grid and wires, each node, and what is being dragged.
    fn draw(&mut self, ui: &mut Ui, graph: &CanvasGraph, look: &Look, allows: &dyn Fn(PortRef, PortRef) -> bool) {
        let colors = Colors::of(look.dark);
        let (view, zoom, [width, height]) = (look.view, look.view.zoom(), look.size);
        let mut shapes = Vec::new();
        // The grid: a line every two snaps, and a stronger one every ten.
        let spacing = SNAP * 2.0;
        if zoom >= FINE_GRID_FROM {
            grid(&mut shapes, view, look.size, spacing, colors.grid);
        }
        grid(&mut shapes, view, look.size, spacing * 5.0, colors.grid_major);
        // The wire being dragged leaves the input it was pulled off bare.
        let taken = match &look.drag {
            Some(Drag::Wire { taken, .. }) => *taken,
            _ => None,
        };
        let thick = (2.0 * zoom).max(1.25);
        for wire in graph.wires.iter().filter(|wire| Some(wire.to) != taken) {
            let (Some(from), Some(to)) = (graph.node(wire.from.0), graph.node(wire.to.0)) else { continue };
            let color = from.outputs.get(wire.from.1).map_or(colors.edge, |port| port.color);
            let (a, b) =
                (view.to_screen(port_at(from, true, wire.from.1)), view.to_screen(port_at(to, false, wire.to.1)));
            let outside = |low: f32, high: f32, most: f32| low.max(high) < -160.0 || low.min(high) > most + 160.0;
            if outside(a[0], b[0], width) || outside(a[1], b[1], height) {
                continue;
            }
            line(&mut shapes, &wire_points(a, b), thick, color);
        }
        ui.set_shapes(self.wires, shapes);

        // What a wire being dragged may land on, to dim the rest.
        let dragging = match &look.drag {
            Some(Drag::Wire { from, .. }) => Some(*from),
            _ => None,
        };
        ui.set_inline_style(self.nodes, &format!("font-size: {:.2}px", (TEXT * zoom).clamp(6.0, 22.0)));
        let order = self.order(graph);
        while self.pool.len() < order.len() {
            let drawn = self.make(ui);
            self.pool.push(drawn);
        }
        for (place, drawn) in self.pool.iter().enumerate() {
            ui.set_visible(drawn.panel, place < order.len());
        }
        for (place, &index) in order.iter().enumerate() {
            let node = &graph.nodes[index];
            let fits = |port: PortRef| match dragging {
                Some(from) => from == port || (from.output != port.output && allows(from, port)),
                None => true,
            };
            let wired = |port: PortRef| {
                graph.wires.iter().any(|wire| Some(wire.to) != taken && port_of(wire, port.output) == port)
            };
            let state = NodeLook { picked: look.picked.contains(&node.id), hovered: look.hovered };
            draw_node(ui, &mut self.pool[place], node, view, &colors, state, &fits, &wired);
        }

        let mut shapes = Vec::new();
        match (&look.drag, look.pointer) {
            (Some(Drag::Wire { from, .. }), Some(point)) => {
                if let Some(node) = graph.node(from.node) {
                    let start = view.to_screen(port_at(node, from.output, from.index));
                    // Snapped to the port it would land on.
                    let ports = if from.output { &node.outputs } else { &node.inputs };
                    let color = ports.get(from.index).map_or(colors.edge, |port| port.color);
                    let landing = self.landing(graph, &order, *from, view.to_canvas(point), allows);
                    let end = landing
                        .and_then(|port| Some(view.to_screen(port_at(graph.node(port.node)?, port.output, port.index))))
                        .unwrap_or(point);
                    let (a, b) = if from.output { (start, end) } else { (end, start) };
                    line(&mut shapes, &wire_points(a, b), thick, color);
                }
            }
            (Some(Drag::Marquee { from, .. }), Some(point)) => {
                let a = view.to_screen(*from);
                let (low, high) = ([a[0].min(point[0]), a[1].min(point[1])], [a[0].max(point[0]), a[1].max(point[1])]);
                let rect = [low[0], low[1], high[0] - low[0], high[1] - low[1]];
                if rect[2] > 1.0 || rect[3] > 1.0 {
                    shapes.push(Shape::Rect { rect, radius: 2.0, fill: colors.marquee.with_alpha(22) });
                    let corners = [low, [high[0], low[1]], high, [low[0], high[1]], low];
                    line(&mut shapes, &corners, 1.0, colors.marquee);
                }
            }
            _ => {}
        }
        ui.set_shapes(self.over, shapes);
        ui.set_visible(self.notice, !graph.notice.is_empty());
        if ui.text(self.notice) != graph.notice {
            ui.set_text(self.notice, &graph.notice);
        }
    }

    /// One more node's elements for the pool.
    fn make(&mut self, ui: &mut Ui) -> Drawn {
        let mut style = Style::default().at(0.0, 0.0);
        style.pickable = false;
        let panel = ui.panel(self.nodes, style);
        ui.add_class(panel, "graph-node");
        let label = |ui: &mut Ui, class: &str| {
            let label = ui.add_label(panel, "");
            ui.add_class(label, "graph-label");
            ui.add_class(label, class);
            label
        };
        let title = label(ui, "graph-title");
        let picture = ui.image(panel);
        ui.add_class(picture, "graph-picture");
        let (detail, fault) = (label(ui, "graph-detail"), label(ui, "graph-fault"));
        Drawn { panel, title, labels: Vec::new(), picture, detail, fault }
    }
}

/// The port of a wire at its output end, or its input end.
fn port_of(wire: &CanvasWire, output: bool) -> PortRef {
    let (node, index) = if output { wire.from } else { wire.to };
    PortRef { node, output, index }
}

/// A run of lines through `points`.
fn line(shapes: &mut Vec<Shape>, points: &[[f32; 2]], width: f32, color: Color) {
    shapes.extend(points.windows(2).map(|pair| Shape::Line { from: pair[0], to: pair[1], width, color }));
}

/// Grid lines every `spacing` canvas units across a view.
fn grid(shapes: &mut Vec<Shape>, view: View, [width, height]: [f32; 2], spacing: f32, color: Color) {
    let (first, last) = (view.to_canvas([0.0, 0.0]), view.to_canvas([width, height]));
    let lines = |low: f32, high: f32| {
        let from = (low / spacing).ceil() as i32;
        (from..=(high / spacing).floor() as i32).map(move |line| line as f32 * spacing)
    };
    for x in lines(first[0], last[0]) {
        let x = view.to_screen([x, 0.0])[0].round();
        shapes.push(Shape::Line { from: [x, 0.0], to: [x, height], width: 1.0, color });
    }
    for y in lines(first[1], last[1]) {
        let y = view.to_screen([0.0, y])[1].round();
        shapes.push(Shape::Line { from: [0.0, y], to: [width, y], width: 1.0, color });
    }
}

/// How one node is lit.
#[derive(Clone, Copy)]
struct NodeLook {
    picked: bool,
    hovered: Hit,
}

/// Places one node's elements and draws its box, header and ports.
#[allow(clippy::too_many_arguments)]
fn draw_node(
    ui: &mut Ui,
    drawn: &mut Drawn,
    node: &CanvasNode,
    view: View,
    colors: &Colors,
    look: NodeLook,
    fits: &dyn Fn(PortRef) -> bool,
    wired: &dyn Fn(PortRef) -> bool,
) {
    let zoom = view.zoom();
    let [width, height] = node_size(node).map(|side| side * zoom);
    let [x, y] = view.to_screen(node.at);
    // Room round the box for the ports' dots and the ring of a picked node.
    let room = PORT * zoom.max(0.7) + 3.0;
    let (panel_width, panel_height) = (width + room * 2.0, height + room * 2.0);
    ui.set_inline_style(
        drawn.panel,
        &format!(
            "left: {:.1}px; top: {:.1}px; width: {panel_width:.1}px; height: {panel_height:.1}px",
            x - room,
            y - room
        ),
    );
    let radius = RADIUS * zoom;
    let header = HEADER * zoom;
    let mut shapes = Vec::new();
    let ring = if !node.fault.is_empty() {
        Some(colors.fault)
    } else if look.picked {
        Some(colors.picked)
    } else {
        None
    };
    let edge = if ring.is_some() { 2.0 } else { 1.0 };
    let outer = [room - edge, room - edge, width + edge * 2.0, height + edge * 2.0];
    shapes.push(Shape::Rect { rect: outer, radius: radius + edge, fill: ring.unwrap_or(colors.edge) });
    shapes.push(Shape::Rect { rect: [room, room, width, height], radius, fill: colors.body });
    let top = if look.picked { colors.header_picked } else { colors.header };
    shapes.push(Shape::Rect { rect: [room, room, width, header], radius, fill: top });
    shapes.push(Shape::Rect { rect: [room, room + header / 2.0, width, header / 2.0], radius: 0.0, fill: top });
    if node.pictured && node.picture.is_none() {
        let rows = node.inputs.len().max(node.outputs.len()) as f32;
        let side = THUMB * zoom;
        let rect = [room + (width - side) / 2.0, room + header + rows * ROW * zoom + PAD * zoom / 2.0, side, side];
        shapes.push(Shape::Rect { rect, radius: 3.0 * zoom, fill: colors.header });
    }
    let ports = [(false, &node.inputs), (true, &node.outputs)];
    for (output, port, index) in
        ports.into_iter().flat_map(|(o, list)| list.iter().enumerate().map(move |(i, p)| (o, p, i)))
    {
        let port_ref = PortRef { node: node.id, output, index };
        let at = view.to_screen(port_at(node, output, index));
        let center = [at[0] - x + room, at[1] - y + room];
        let lit = look.hovered == Hit::Port(port_ref);
        let color = if fits(port_ref) { port.color } else { port.color.with_alpha(60) };
        let dot = PORT * zoom.max(0.7) + if lit { 1.5 } else { 0.0 };
        let fill = if wired(port_ref) || lit { color } else { colors.body };
        shapes.push(Shape::Circle { center, radius: dot, fill, border: Some((1.5, color)) });
    }
    ui.set_shapes(drawn.panel, shapes);

    let titled = zoom >= TITLES_FROM;
    let labelled = zoom >= LABELS_FROM;
    show(
        ui,
        drawn.title,
        titled,
        &node.title,
        &format!("left: {:.1}px; top: {:.1}px", room + PAD * zoom, room + 6.0 * zoom),
    );
    let rows = node.inputs.len().max(node.outputs.len());
    while drawn.labels.len() < node.inputs.len() + node.outputs.len() {
        let label = ui.add_label(drawn.panel, "");
        ui.add_class(label, "graph-label");
        ui.add_class(label, "graph-port");
        drawn.labels.push(label);
    }
    let mut labels = drawn.labels.iter().copied();
    for (index, port) in node.inputs.iter().enumerate() {
        let top = room + (HEADER + index as f32 * ROW + 3.0) * zoom;
        let place = format!("left: {:.1}px; top: {top:.1}px", room + (PAD + 4.0) * zoom);
        show(ui, labels.next().expect("a label for each port"), labelled, &port.label, &place);
    }
    for (index, port) in node.outputs.iter().enumerate() {
        let top = room + (HEADER + index as f32 * ROW + 3.0) * zoom;
        let place = format!("right: {:.1}px; top: {top:.1}px", room + (PAD + 4.0) * zoom);
        show(ui, labels.next().expect("a label for each port"), labelled, &port.label, &place);
    }
    for spare in labels {
        ui.set_visible(spare, false);
    }
    let mut below = HEADER + rows as f32 * ROW + PAD / 2.0;
    let side = THUMB * zoom;
    ui.set_visible(drawn.picture, node.picture.is_some());
    if node.picture.is_some() {
        ui.set_image(drawn.picture, node.picture);
        let (left, top) = (room + (width - side) / 2.0, room + below * zoom);
        ui.set_inline_style(
            drawn.picture,
            &format!("left: {left:.1}px; top: {top:.1}px; width: {side:.1}px; height: {side:.1}px"),
        );
    }
    if node.pictured {
        below += THUMB + PAD / 2.0;
    }
    let left = room + PAD * zoom;
    show(
        ui,
        drawn.detail,
        labelled && !node.detail.is_empty(),
        &node.detail,
        &format!("left: {left:.1}px; top: {:.1}px", room + below * zoom),
    );
    if !node.detail.is_empty() {
        below += LINE;
    }
    show(
        ui,
        drawn.fault,
        labelled && !node.fault.is_empty(),
        &node.fault,
        &format!("left: {left:.1}px; top: {:.1}px", room + below * zoom),
    );
}

/// Shows a pooled label with this text at this place, or hides it.
fn show(ui: &mut Ui, label: NodeId, visible: bool, text: &str, place: &str) {
    ui.set_visible(label, visible);
    if !visible {
        return;
    }
    if ui.text(label) != text {
        ui.set_text(label, text);
    }
    ui.set_inline_style(label, &format!("position: absolute; {place}"));
}

#[cfg(test)]
mod tests {
    use voxear_ui::Modifiers;

    use super::*;

    const SIZE: (u32, u32) = (900, 600);

    fn port(label: &str) -> CanvasPort {
        CanvasPort { label: label.to_string(), color: Color::hex(0x6fb1ff) }
    }

    /// A source, a shaper and a sink in a row, the source wired into the shaper.
    fn graph() -> CanvasGraph {
        let node = |id: u64, at: [f32; 2], title: &str, inputs: &[&str], outputs: &[&str]| CanvasNode {
            id,
            at,
            title: title.to_string(),
            inputs: inputs.iter().map(|label| port(label)).collect(),
            outputs: outputs.iter().map(|label| port(label)).collect(),
            pictured: !outputs.is_empty(),
            ..CanvasNode::default()
        };
        CanvasGraph {
            nodes: vec![
                node(1, [0.0, 0.0], "Noise", &[], &["Noise"]),
                node(2, [250.0, 0.0], "Math", &["A", "B"], &["Result"]),
                node(3, [500.0, 40.0], "Output", &["Height", "Temperature"], &[]),
            ],
            wires: vec![CanvasWire { from: (1, 0), to: (2, 0) }],
            notice: String::new(),
        }
    }

    struct Bench {
        ui: Ui,
        canvas: GraphCanvas,
        graph: CanvasGraph,
    }

    impl Bench {
        fn new() -> Self {
            let mut ui = Ui::new();
            let root = ui.root();
            let frame = ui.panel(root, Style::column().at(20.0, 30.0).width(800.0).height(500.0));
            let canvas = GraphCanvas::new(&mut ui, frame);
            let mut bench = Self { ui, canvas, graph: graph() };
            bench.frame();
            bench.frame();
            bench
        }

        /// A frame: the canvas's, carrying out what it asks as an owner would, then the UI's.
        fn frame(&mut self) -> Vec<CanvasEvent> {
            let events = self.canvas.update(&mut self.ui, &self.graph, &|from, to| from.node != to.node);
            for event in &events {
                match event {
                    CanvasEvent::Moved(places) => {
                        for (id, at) in places {
                            self.graph.nodes.iter_mut().find(|node| node.id == *id).unwrap().at = *at;
                        }
                    }
                    CanvasEvent::Connect { from, to } => {
                        self.graph.wires.retain(|wire| wire.to != *to);
                        self.graph.wires.push(CanvasWire { from: *from, to: *to });
                    }
                    CanvasEvent::Disconnect { to } => self.graph.wires.retain(|wire| wire.to != *to),
                    CanvasEvent::Delete(ids) => {
                        self.graph.nodes.retain(|node| !ids.contains(&node.id));
                        self.graph.wires.retain(|wire| !ids.contains(&wire.from.0) && !ids.contains(&wire.to.0));
                    }
                    _ => {}
                }
            }
            self.ui.frame(SIZE.0, SIZE.1, 1.0);
            events
        }

        fn to(&mut self, point: (f32, f32)) {
            self.ui.pointer_move(point.0, point.1);
        }

        fn node(&self, id: u64) -> (f32, f32) {
            self.canvas.node_point(&self.ui, &self.graph, id).unwrap()
        }

        fn port(&self, node: u64, output: bool, index: usize) -> (f32, f32) {
            self.canvas.port_point(&self.ui, &self.graph, PortRef { node, output, index }).unwrap()
        }

        /// Presses at one point, moves to another over a few frames, and lets go: every event heard.
        fn drag(&mut self, from: (f32, f32), to: (f32, f32)) -> Vec<CanvasEvent> {
            self.to(from);
            self.ui.pointer_button(true);
            let mut events = self.frame();
            for step in 1..=4 {
                let t = step as f32 / 4.0;
                self.to((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
                events.extend(self.frame());
            }
            self.ui.pointer_button(false);
            events.extend(self.frame());
            events
        }

        fn at(&self, id: u64) -> [f32; 2] {
            self.graph.node(id).unwrap().at
        }
    }

    #[test]
    fn a_node_is_as_large_as_its_ports_picture_and_lines_need() {
        let graph = graph();
        let [width, height] = node_size(&graph.nodes[1]);
        assert_eq!(width, 150.0, "as wide as its picture, on the grid");
        assert_eq!(height, HEADER + 2.0 * ROW + PAD / 2.0 + THUMB + PAD);
        let mut faulty = graph.nodes[1].clone();
        (faulty.detail, faulty.fault) = ("0 to 1".into(), "Nothing reads it".into());
        assert_eq!(node_size(&faulty)[1], height + LINE + LINE + 2.0);
        let long = CanvasNode {
            inputs: vec![port("A very long input's label")],
            outputs: vec![port("And its output")],
            ..CanvasNode::default()
        };
        assert!(node_size(&long)[0] > MIN_WIDTH, "wide enough for both labels side by side");
        // Inputs down the left, outputs down the right, a row each under the header.
        assert_eq!(port_at(&graph.nodes[1], false, 1), [250.0, HEADER + 1.5 * ROW]);
        assert_eq!(port_at(&graph.nodes[1], true, 0), [250.0 + width, HEADER + 0.5 * ROW]);
    }

    #[test]
    fn a_view_zooms_about_a_point_in_steps_and_frames_what_it_is_given() {
        let view = View::default();
        assert_eq!((view.zoom(), view.to_screen([40.0, 60.0])), (1.0, [40.0, 60.0]));
        let point = [200.0, 100.0];
        let under = view.to_canvas(point);
        let closer = view.zoomed(point, 1);
        assert_eq!(closer.zoom(), 1.2);
        let still = closer.to_screen(under);
        assert!((still[0] - point[0]).abs() < 1e-3 && (still[1] - point[1]).abs() < 1e-3, "{still:?}");
        assert_eq!(view.zoomed(point, 99).zoom(), 2.0);
        assert_eq!(view.zoomed(point, -99).zoom(), 0.25);
        assert_eq!(view.moved([30.0, -10.0]).origin, [-30.0, 10.0]);
        // A graph wider than the view is shown smaller, in its middle; a small one no larger than life.
        let framed = View::framing([0.0, 0.0, 2000.0, 400.0], [800.0, 500.0]);
        assert_eq!(framed.zoom(), 0.35);
        let middle = framed.to_screen([1000.0, 200.0]);
        assert!((middle[0] - 400.0).abs() < 1e-3 && (middle[1] - 250.0).abs() < 1e-3, "{middle:?}");
        assert_eq!(View::framing([0.0, 0.0, 100.0, 100.0], [800.0, 500.0]).zoom(), 1.0);
    }

    #[test]
    fn a_place_on_the_canvas_is_on_a_port_a_node_or_nothing() {
        let graph = graph();
        let order = [0, 1, 2];
        let out = port_at(&graph.nodes[0], true, 0);
        assert_eq!(
            hit(&graph, &order, [out[0] + 4.0, out[1] - 3.0], 10.0),
            Hit::Port(PortRef { node: 1, output: true, index: 0 })
        );
        assert_eq!(hit(&graph, &order, [20.0, 10.0], 10.0), Hit::Node(1));
        assert_eq!(hit(&graph, &order, [220.0, 300.0], 10.0), Hit::Nothing);
        // Of two nodes over one another, the one drawn last is the one pressed.
        let mut stacked = graph.clone();
        stacked.nodes[1].at = [20.0, 20.0];
        assert_eq!(hit(&stacked, &[0, 1, 2], [60.0, 60.0], 0.0), Hit::Node(2));
        assert_eq!(hit(&stacked, &[1, 0, 2], [60.0, 60.0], 0.0), Hit::Node(1));
        // A box picks what it touches; a wire runs level out of one port and into the other.
        assert_eq!(boxed(&graph, [250.0, -20.0], [700.0, 60.0]), vec![2, 3]);
        assert_eq!(bounds(&graph, &[1]).map(|b| [b[0], b[1]]), Some([0.0, 0.0]));
        let points = wire_points([0.0, 0.0], [200.0, 80.0]);
        assert_eq!((points.len(), points[0], points[SEGMENTS]), (SEGMENTS + 1, [0.0, 0.0], [200.0, 80.0]));
        assert!(points[1][1] < 2.0 && points[SEGMENTS - 1][1] > 78.0, "level at both ends");
        assert_eq!(snapped([14.9, -26.0]), [10.0, -30.0]);
    }

    #[test]
    fn a_new_canvas_frames_its_graph_and_the_wheel_zooms_about_the_pointer() {
        let mut bench = Bench::new();
        let view = bench.canvas.view();
        assert_eq!(view.zoom(), 1.0, "three nodes fit as they are");
        let [x, y, width, height] = bench.ui.rect(bench.canvas.viewport());
        assert_eq!([x, y, width, height], [20.0, 30.0, 800.0, 500.0]);
        let all = bounds(&bench.graph, &[]).unwrap();
        let middle = view.to_screen([(all[0] + all[2]) / 2.0, (all[1] + all[3]) / 2.0]);
        assert!((middle[0] - 400.0).abs() < 1.0 && (middle[1] - 250.0).abs() < 1.0, "in the middle: {middle:?}");

        // A notch of the wheel away is a step in, the node under the pointer staying under it.
        let over = bench.node(2);
        bench.to(over);
        bench.ui.pointer_scroll(0.0, -NOTCH);
        bench.frame();
        assert_eq!(bench.canvas.view().zoom(), 1.2);
        let now = bench.node(2);
        assert!((now.0 - over.0).abs() < 1.0 && (now.1 - over.1).abs() < 1.0, "{now:?} not {over:?}");
        bench.ui.pointer_scroll(0.0, NOTCH * 3.0);
        bench.frame();
        assert_eq!(bench.canvas.view().zoom(), 0.7);
        // Half a notch is no step yet; the other half makes one.
        bench.ui.pointer_scroll(0.0, NOTCH / 2.0);
        bench.frame();
        assert_eq!(bench.canvas.view().zoom(), 0.7);
        bench.ui.pointer_scroll(0.0, NOTCH / 2.0);
        bench.frame();
        assert_eq!(bench.canvas.view().zoom(), 0.6);
        // F shows it all again.
        bench.canvas.key(CanvasKey::Frame);
        bench.frame();
        assert_eq!(bench.canvas.view(), view);
    }

    #[test]
    fn the_middle_button_and_alt_move_the_view() {
        let mut bench = Bench::new();
        let before = bench.canvas.view().origin;
        let start = bench.node(2);
        bench.to(start);
        bench.ui.pointer_middle(true);
        bench.frame();
        bench.to((start.0 + 60.0, start.1 - 25.0));
        let events = bench.frame();
        bench.ui.pointer_middle(false);
        bench.frame();
        assert!(events.is_empty(), "moving the view changes nothing of the graph: {events:?}");
        assert_eq!(bench.canvas.view().origin, [before[0] - 60.0, before[1] + 25.0]);
        assert!(bench.canvas.picked().is_empty() && bench.at(2) == [250.0, 0.0], "though it began on a node");

        // With Alt held the first button does the same.
        bench.canvas.set_panning(true);
        let from = bench.node(2);
        let events = bench.drag(from, (from.0 - 60.0, from.1 + 25.0));
        assert!(events.is_empty(), "{events:?}");
        let back = bench.canvas.view().origin;
        assert!((back[0] - before[0]).abs() < 1e-3 && (back[1] - before[1]).abs() < 1e-3, "{back:?} not {before:?}");
    }

    #[test]
    fn a_press_picks_a_node_and_a_drag_moves_every_picked_node_on_the_grid() {
        let mut bench = Bench::new();
        let on = bench.node(2);
        bench.to(on);
        bench.ui.pointer_button(true);
        assert_eq!(bench.frame(), [CanvasEvent::Picked]);
        bench.ui.pointer_button(false);
        assert!(bench.frame().is_empty(), "a click moves nothing");
        assert_eq!(bench.canvas.picked(), [2]);

        // Shift and a press add another; a drag of either moves both, a step to tell the owner of at its end.
        bench.ui.set_modifiers(Modifiers { shift: true, control: false });
        let other = bench.node(3);
        bench.to(other);
        bench.ui.pointer_button(true);
        bench.frame();
        bench.ui.pointer_button(false);
        bench.frame();
        bench.ui.set_modifiers(Modifiers::default());
        assert_eq!(bench.canvas.picked(), [2, 3]);
        let from = bench.node(2);
        let events = bench.drag(from, (from.0 + 53.0, from.1 + 78.0));
        assert_eq!(bench.at(2), [300.0, 80.0], "snapped to the grid");
        assert_eq!(bench.at(3), [550.0, 120.0]);
        assert_eq!(bench.at(1), [0.0, 0.0]);
        assert_eq!(events.last(), Some(&CanvasEvent::MoveEnded));
        assert!(events.iter().any(|event| matches!(event, CanvasEvent::Moved(places) if places.len() == 2)));

        // A click on one of several picked nodes leaves it alone picked; on empty canvas, none.
        let on = bench.node(3);
        bench.to(on);
        bench.ui.pointer_button(true);
        bench.frame();
        bench.ui.pointer_button(false);
        bench.frame();
        assert_eq!(bench.canvas.picked(), [3]);
        let [x, y, ..] = bench.ui.rect(bench.canvas.viewport());
        bench.to((x + 15.0, y + 480.0));
        bench.ui.pointer_button(true);
        assert_eq!(bench.frame(), [CanvasEvent::Picked]);
        bench.ui.pointer_button(false);
        assert!(bench.frame().is_empty());
        assert!(bench.canvas.picked().is_empty());
    }

    #[test]
    fn a_drag_over_empty_canvas_picks_what_its_box_touches() {
        let mut bench = Bench::new();
        let (first, last) = (bench.node(1), bench.node(2));
        let events = bench.drag((first.0 - 100.0, first.1 - 20.0), (last.0, last.1 + 30.0));
        assert_eq!(bench.canvas.picked(), [1, 2]);
        assert!(events.iter().all(|event| *event == CanvasEvent::Picked), "{events:?}");
        // Delete, Ctrl+D and Ctrl+C are about the picked nodes; with none picked they ask nothing.
        for (key, asked) in [
            (CanvasKey::Duplicate, CanvasEvent::Duplicate(vec![1, 2])),
            (CanvasKey::Copy, CanvasEvent::Copy(vec![1, 2])),
            (CanvasKey::Delete, CanvasEvent::Delete(vec![1, 2])),
        ] {
            bench.canvas.key(key);
            assert_eq!(bench.frame()[0], asked);
        }
        assert_eq!(bench.graph.nodes.len(), 1, "the owner took them out");
        assert_eq!(bench.frame(), [CanvasEvent::Picked]);
        assert!(bench.canvas.picked().is_empty(), "and nodes that went are picked no longer");
        bench.canvas.key(CanvasKey::Delete);
        assert!(bench.frame().is_empty());
        // Paste asks for what was copied under the pointer.
        let on = bench.node(3);
        bench.to(on);
        bench.canvas.key(CanvasKey::Paste);
        let [CanvasEvent::Paste(at)] = bench.frame()[..] else { panic!("a paste") };
        let node = &bench.graph.nodes[0];
        assert!((node.at[0]..node.at[0] + 200.0).contains(&at[0]) && at == snapped(at), "{at:?}");
    }

    #[test]
    fn a_drag_from_port_to_port_wires_them_where_the_owner_allows() {
        let mut bench = Bench::new();
        // From the Math's output onto the Output's second input.
        let (from, to) = (bench.port(2, true, 0), bench.port(3, false, 1));
        let events = bench.drag(from, to);
        assert_eq!(events, [CanvasEvent::Connect { from: (2, 0), to: (3, 1) }]);
        // Backwards, from an input to an output, is the same wire.
        let (from, to) = (bench.port(3, false, 0), bench.port(2, true, 0));
        assert_eq!(bench.drag(from, to), [CanvasEvent::Connect { from: (2, 0), to: (3, 0) }]);
        // Onto a node's body it takes the first input with no wire yet.
        let (from, to) = (bench.port(1, true, 0), bench.node(2));
        assert_eq!(bench.drag(from, to), [CanvasEvent::Connect { from: (1, 0), to: (2, 1) }]);
        // A wire the owner refuses (here, a node into itself) does not land, nor one output on another.
        let (from, to) = (bench.port(2, true, 0), bench.port(2, false, 0));
        assert!(bench.drag(from, to).is_empty());
        let (from, to) = (bench.port(2, true, 0), bench.port(1, true, 0));
        assert!(bench.drag(from, to).is_empty());
        assert_eq!(bench.graph.wires.len(), 4);

        // A drag from a wired input pulls its wire off: onto another input it moves there, onto nothing it goes.
        let (from, to) = (bench.port(3, false, 1), bench.port(3, false, 0));
        let events = bench.drag(from, to);
        assert_eq!(events, [CanvasEvent::Disconnect { to: (3, 1) }, CanvasEvent::Connect { from: (2, 0), to: (3, 0) }]);
        let from = bench.port(3, false, 0);
        assert_eq!(bench.drag(from, (from.0 - 40.0, from.1 + 200.0)), [CanvasEvent::Disconnect { to: (3, 0) }]);
        // Let go where it was pulled from, it stays.
        let from = bench.port(2, false, 0);
        assert!(bench.drag(from, (from.0 + 2.0, from.1 + 1.0)).is_empty());
        assert_eq!(
            bench.graph.wires,
            [CanvasWire { from: (1, 0), to: (2, 0) }, CanvasWire { from: (1, 0), to: (2, 1) }]
        );
    }

    #[test]
    fn a_context_click_a_key_or_a_wire_let_go_on_nothing_asks_for_the_search() {
        let mut bench = Bench::new();
        let [x, y, ..] = bench.ui.rect(bench.canvas.viewport());
        bench.to((x + 420.0, y + 400.0));
        bench.ui.pointer_secondary(true);
        bench.ui.pointer_secondary(false);
        let [CanvasEvent::Search { at, wire: None }] = bench.frame()[..] else { panic!("a search") };
        assert_eq!(at, snapped(bench.canvas.view().to_canvas([420.0, 400.0])));
        bench.frame();
        let [ax, ay, ..] = bench.ui.rect(bench.canvas.anchor());
        assert!((ax - x - 420.0).abs() < 1.0 && (ay - y - 400.0).abs() < 1.0, "the anchor is where it was asked");

        bench.canvas.key(CanvasKey::Search);
        assert!(matches!(bench.frame()[..], [CanvasEvent::Search { wire: None, .. }]));

        // A wire let go over empty canvas asks for a node on its end.
        let from = bench.port(2, true, 0);
        let events = bench.drag(from, (from.0 + 90.0, from.1 + 260.0));
        let wire = Some(PortRef { node: 2, output: true, index: 0 });
        assert!(matches!(events[..], [CanvasEvent::Search { wire: found, .. }] if found == wire), "{events:?}");
    }

    #[test]
    fn what_is_drawn_follows_the_graph_the_zoom_and_what_is_picked() {
        let mut bench = Bench::new();
        let shapes = |bench: &Bench, layer: NodeId| bench.ui.shapes(layer).len();
        let lines = shapes(&bench, bench.canvas.wires);
        assert!(lines > SEGMENTS, "the grid and a wire: {lines}");
        // Each node has its box from the pool, with its title and its ports' labels.
        let panels: Vec<NodeId> = bench.ui.children(bench.canvas.nodes);
        assert_eq!(panels.iter().filter(|&&panel| bench.ui.is_visible(panel)).count(), 3);
        let texts = |bench: &Bench, panel: NodeId| -> Vec<String> {
            let shown = bench.ui.children(panel).into_iter().filter(|&label| bench.ui.is_visible(label));
            shown.map(|label| bench.ui.text(label)).filter(|text| !text.is_empty()).collect()
        };
        assert_eq!(texts(&bench, panels[1]), ["Math", "A", "B", "Result"]);
        let [x, y, width, _] = bench.ui.rect(panels[1]);
        let on = bench.node(2);
        assert!((x..x + width).contains(&on.0) && on.1 > y, "the box is where its node is");
        // A wire more is twenty lines more; a node fewer, a box hidden.
        bench.graph.wires.push(CanvasWire { from: (2, 0), to: (3, 0) });
        bench.frame();
        assert_eq!(shapes(&bench, bench.canvas.wires), lines + SEGMENTS);
        bench.graph.nodes.remove(0);
        bench.graph.wires.remove(0);
        bench.graph.notice = "No Output node".into();
        bench.frame();
        let panels: Vec<NodeId> = bench.ui.children(bench.canvas.nodes);
        assert_eq!(panels.iter().filter(|&&panel| bench.ui.is_visible(panel)).count(), 2);
        assert!(bench.ui.is_visible(bench.canvas.notice) && bench.ui.text(bench.canvas.notice) == "No Output node");
        // Far out, the ports' labels are left out, and the titles are smaller.
        bench.to(on);
        bench.ui.pointer_scroll(0.0, NOTCH * 6.0);
        bench.frame();
        bench.frame();
        assert_eq!(bench.canvas.view().zoom(), 0.25);
        assert!(texts(&bench, panels[0]).is_empty(), "{:?}", texts(&bench, panels[0]));
        // A wire being dragged is drawn over the nodes until it is let go.
        bench.canvas.key(CanvasKey::Frame);
        bench.frame();
        let from = bench.port(2, true, 0);
        bench.to(from);
        bench.ui.pointer_button(true);
        bench.frame();
        bench.to((from.0 + 50.0, from.1 + 120.0));
        bench.frame();
        assert_eq!(shapes(&bench, bench.canvas.over), SEGMENTS);
        bench.ui.pointer_button(false);
        bench.frame();
        bench.frame();
        assert_eq!(shapes(&bench, bench.canvas.over), 0);
    }
}
