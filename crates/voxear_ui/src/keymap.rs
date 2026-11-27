//! Key maps: devices drawn with their keys, each bound one lit, and a label
//! beside the device for each, naming its actions with a chip of the key's
//! name, joined to its key by a line of its own, as a game's key binding
//! screen shows them. The keys and labels are elements, styled by sheets:
//!
//! - `keymap` the whole, `keymap-board` the devices under their keys, drawn
//!   from their art (see [`KeymapArt`]) in the colours of `keymap-plate`,
//!   `keymap-recess` and `keymap-part`;
//! - `keymap-key` each key, with `round`, `stick`, `pill` or `shaped` for
//!   its shape and `assigned`, `hidden`, `unused` or `conflict` for whether
//!   it is bound; `keymap-key-label` its face, its text made smaller to fit,
//!   or `keymap-key-icon`; `keymap-stick-ring` the dish of a stick's cap;
//!   `keymap-caption` a name written under a key. A shaped key is drawn in
//!   the colours of a `keymap-swatch` with the same state;
//! - `keymap-callout` each label, with `left`, `right`, `top` or `bottom`
//!   for the side it is on and `conflict` for a conflicting key's. Each
//!   action is a `keymap-callout-line` beside a `keymap-chip` naming the key,
//!   in a `keymap-callout-row`, with a `keymap-callout-icon` when the action
//!   has one. A conflict's label holds its actions in `keymap-callout-texts`
//!   beside a `keymap-callout-warn` and one chip;
//! - the lines take `keymap-line`'s background colour, a conflict's
//!   `keymap-line.conflict`'s.
//!
//! A key's own colour, as a category's, is given with its data and wins over
//! the sheets' (see [`KeymapKey::color`]). What is on the keys comes from
//! whoever owns the map, as `voxear_input`'s layouts and bindings, so the
//! same view serves the editor and a game.
//!
//! Each frame, once laid out, the devices are fitted into the map side by
//! side, or one under another when side by side would make their keys too
//! small, with room round each for its labels, and the labels placed: each on
//! the side its key asks for, or the nearest, a keyboard sending no more than
//! three of a row to one side. A key on the device's edge leads its line
//! straight out; one inside drops into the gap below its row (or on a device
//! without rows, rises above it when that is shorter) and runs out along it. Out of the device, labels
//! stand in the order of their lines, as near their keys as they can, and
//! each line bends once, in a column of its own where it would cross
//! another. Lines keep clear of keys and of each other.

use taffy::NodeId as TaffyNode;

use crate::{Color, NodeId, Shape, Style, Ui};

/// How a key is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum KeyShape {
    #[default]
    Key,
    Round,
    Stick,
    Pill,
    /// SVG path data in a `view` box stretched over the key, as a trigger's
    /// curve.
    Path {
        path: &'static str,
        view: [f32; 2],
    },
}

/// Whether a key is bound to anything shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyState {
    #[default]
    Unused,
    Assigned,
    /// Bound only to actions that keep out of key maps.
    Hidden,
    /// Bound to more than one action that can be on at once.
    Conflict,
}

/// The side of its device a key's label goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeymapSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// One action named beside a key, with its icon when it has one, and the
/// chip that names the key, as `W` or `L3`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapLine {
    pub text: String,
    /// One of the icons ([`ICONS`](crate::ICONS)), by name.
    pub icon: Option<String>,
    /// The key's name in the label's chip; empty for no chip.
    pub chip: String,
}

impl KeymapLine {
    pub fn new(text: &str) -> Self {
        Self { text: text.to_string(), icon: None, chip: String::new() }
    }

    /// An action with the chip naming its key.
    pub fn with_chip(text: &str, chip: &str) -> Self {
        Self { text: text.to_string(), icon: None, chip: chip.to_string() }
    }
}

/// The shape of a piece of a device's art.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArtShape {
    /// A box with corners rounded `radius` key widths.
    Rect { radius: f32 },
    /// A circle as wide as its box.
    Circle,
    /// SVG path data in a `view` box stretched over the piece's box.
    Path { path: &'static str, view: [f32; 2] },
}

/// How a piece of a device's art is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtLook {
    /// The device itself: a keyboard's plate, a mouse's shell, a pad's body.
    Plate,
    /// A well sunk in it, behind a stick, the d-pad or the face buttons.
    Recess,
    /// A part that is not a control, as the middle of a d-pad.
    Part,
}

/// A piece of a device's drawing behind its keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeymapArt {
    /// Where it is and how big, in key widths.
    pub rect: [f32; 4],
    pub shape: ArtShape,
    pub look: ArtLook,
    /// Which of the map's devices it belongs to.
    pub device: usize,
}

/// One device of a key map, laid out beside the others with room round it
/// for its labels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeymapDevice {
    /// Its bounds, in key widths, its keys' and art's units.
    pub rect: [f32; 4],
    /// Its keys stand in rows, as a keyboard's: no more than three a row
    /// send their labels to one side, the top row's go above it and the
    /// bottom row's below, and labels from different rows stand further
    /// apart.
    pub rows: bool,
}

/// One key of a key map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapKey {
    /// Where it is and how big, in the map's own units, as key widths.
    pub rect: [f32; 4],
    pub shape: KeyShape,
    pub label: String,
    /// An icon drawn on the key in place of its label.
    pub icon: Option<String>,
    /// A name written under the key, as a mouse's Move.
    pub caption: Option<String>,
    pub state: KeyState,
    /// Its fill, as its actions' category's; `None` leaves it to the sheets.
    pub color: Option<Color>,
    /// The actions to name beside the device, one a line; none for no label.
    pub callout: Vec<KeymapLine>,
    /// Which of the map's devices it is on.
    pub device: usize,
    /// The side its label goes on; `None` for the nearest.
    pub side: Option<KeymapSide>,
}

/// What a key map shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapData {
    /// How big the devices are together, in their keys' units.
    pub size: [f32; 2],
    pub keys: Vec<KeymapKey>,
    /// The devices' drawings behind their keys, in order.
    pub art: Vec<KeymapArt>,
    /// The devices, left to right; none for one as big as the map.
    pub devices: Vec<KeymapDevice>,
}

/// A label and what is known of it.
struct Callout {
    key: usize,
    node: TaffyNode,
    /// The first chip in it, which a line above or below meets.
    chip: Option<TaffyNode>,
    side: KeymapSide,
}

/// Elements that are never seen, whose colours the sheets set for what is
/// drawn as shapes.
struct Swatches {
    plate: TaffyNode,
    recess: TaffyNode,
    part: TaffyNode,
    line: TaffyNode,
    conflict_line: TaffyNode,
    /// A shaped key's, by its state: unused, assigned, hidden, conflict.
    states: [TaffyNode; 4],
}

/// A key map's elements, kept by the [`Ui`].
pub(crate) struct Keymap {
    board: TaffyNode,
    lines: TaffyNode,
    data: KeymapData,
    keys: Vec<TaffyNode>,
    /// Each stick's ring, by the key's place.
    rings: Vec<(usize, TaffyNode)>,
    /// Each key's icon, by the key's place.
    icons: Vec<(usize, TaffyNode)>,
    captions: Vec<(usize, TaffyNode)>,
    callouts: Vec<Callout>,
    swatches: Swatches,
    /// Each label's line as last placed, in the map's own pixels, by its key's place.
    placed_lines: Vec<(usize, Vec<[f32; 2]>)>,
    /// The map's size it was last arranged for.
    arranged: Option<[f32; 4]>,
}

/// How far out of its device a line first bends, and how far apart the
/// columns it bends in are, in logical pixels.
const OUT: f32 = 18.0;
const COLUMN: f32 = 5.0;
/// Between the last column and the labels.
const LABEL_GAP: f32 = 8.0;
/// Between labels, and more between those of different rows of keys.
const SPACING: f32 = 4.0;
const ROW_GAP: f32 = 8.0;
/// Between labels side by side above or below a device.
const ACROSS_GAP: f32 = 8.0;
/// Between one device's labels and the next device's.
const DEVICE_GAP: f32 = 24.0;
/// How close a line comes to a key or a well, and how far apart lines run
/// side by side at most.
const MARGIN: f32 = 3.0;
const LANE: f32 = 5.0;
/// How far a label moves to line up with its line.
const SNAP: f32 = 4.0;
/// The smallest and the biggest a key width is drawn, in logical pixels.
const MIN_KEY: f32 = 18.0;
const MAX_KEY: f32 = 34.0;
/// As small as a key width is drawn for everything to fit.
const SMALLEST_KEY: f32 = 12.0;
/// How wide the art's edges and the lines are, in logical pixels.
const EDGE: f32 = 1.0;
const LINE: f32 = 1.5;
/// The text on a key at its biggest and smallest, in logical pixels.
const KEY_TEXT: (f32, f32) = (10.0, 6.5);
/// How many labels on one side a device keeps room for the columns of.
const ROOM_COLUMNS: usize = 6;

impl Ui {
    /// A key map under `parent`, empty until [`set_keymap`](Self::set_keymap)
    /// fills it.
    pub fn keymap(&mut self, parent: NodeId) -> NodeId {
        let map = self.panel(parent, Style::default());
        self.add_class(map, "keymap");
        let swatch = |ui: &mut Ui, classes: &[&str]| {
            let node = ui.panel(map, Style { pickable: false, ..Style::default() });
            for class in classes {
                ui.add_class(node, class);
            }
            ui.set_inline_style(node, "position: absolute; left: 0px; top: 0px; width: 0px; height: 0px; opacity: 0");
            node.0
        };
        let swatches = Swatches {
            plate: swatch(self, &["keymap-plate"]),
            recess: swatch(self, &["keymap-recess"]),
            part: swatch(self, &["keymap-part"]),
            line: swatch(self, &["keymap-line"]),
            conflict_line: swatch(self, &["keymap-line", "conflict"]),
            states: [
                swatch(self, &["keymap-swatch", "unused"]),
                swatch(self, &["keymap-swatch", "assigned"]),
                swatch(self, &["keymap-swatch", "hidden"]),
                swatch(self, &["keymap-swatch", "conflict"]),
            ],
        };
        let board = self.panel(map, Style::default());
        self.add_class(board, "keymap-board");
        self.set_inline_style(board, "position: absolute; left: 0px; top: 0px; width: 0px; height: 0px");
        let lines = self.panel(map, Style { pickable: false, ..Style::cover() });
        self.add_class(lines, "keymap-lines");
        let keymap = Keymap {
            board: board.0,
            lines: lines.0,
            data: KeymapData::default(),
            keys: Vec::new(),
            rings: Vec::new(),
            icons: Vec::new(),
            captions: Vec::new(),
            callouts: Vec::new(),
            swatches,
            placed_lines: Vec::new(),
            arranged: None,
        };
        self.keymaps.insert(map.0, keymap);
        map
    }

    /// Shows `data` in a key map, making its keys and labels again when
    /// they changed.
    pub fn set_keymap(&mut self, map: NodeId, data: KeymapData) {
        let Some(keymap) = self.keymaps.get(&map.0) else { return };
        if keymap.data == data {
            return;
        }
        let board = keymap.board;
        let old: Vec<TaffyNode> = keymap
            .keys
            .iter()
            .copied()
            .chain(keymap.captions.iter().map(|(_, node)| *node))
            .chain(keymap.callouts.iter().map(|c| c.node))
            .collect();
        for node in old {
            self.remove(NodeId(node));
        }
        let sides = sides(&data);
        let (mut keys, mut rings, mut icons, mut captions, mut callouts) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (at, key) in data.keys.iter().enumerate() {
            let node = self.panel(NodeId(board), Style::default());
            self.add_class(node, "keymap-key");
            let shape = match key.shape {
                KeyShape::Key => None,
                KeyShape::Round => Some("round"),
                KeyShape::Stick => Some("stick"),
                KeyShape::Pill => Some("pill"),
                KeyShape::Path { .. } => Some("shaped"),
            };
            if let Some(shape) = shape {
                self.add_class(node, shape);
            }
            self.add_class(node, state_class(key.state));
            let holder = if key.shape == KeyShape::Stick {
                let ring = self.panel(node, Style { pickable: false, ..Style::default() });
                self.add_class(ring, "keymap-stick-ring");
                rings.push((at, ring.0));
                ring
            } else {
                node
            };
            match &key.icon {
                Some(icon) => {
                    let mark = self.icon(holder, icon);
                    self.add_class(mark, "keymap-key-icon");
                    icons.push((at, mark.0));
                }
                None => {
                    let face = self.add_label(holder, &key.label);
                    self.add_class(face, "keymap-key-label");
                }
            }
            keys.push(node.0);
            if let Some(caption) = &key.caption {
                let text = self.add_label(NodeId(board), caption);
                self.add_class(text, "keymap-caption");
                self.set_inline_style(text, "position: absolute; left: 0px; top: 0px");
                captions.push((at, text.0));
            }
        }
        for (at, key) in data.keys.iter().enumerate() {
            if !key.callout.is_empty() {
                callouts.push(self.callout(map, at, key, sides[at]));
            }
        }
        let keymap = self.keymaps.get_mut(&map.0).expect("looked up above");
        (keymap.keys, keymap.rings, keymap.icons, keymap.captions, keymap.callouts, keymap.data, keymap.arranged) =
            (keys.clone(), rings, icons, captions, callouts, data, None);
        // A key's own colour wins over the sheets'.
        let colors: Vec<(TaffyNode, Option<Color>, bool)> = keys
            .iter()
            .zip(&keymap.data.keys)
            .map(|(&node, key)| (node, key.color, matches!(key.shape, KeyShape::Path { .. })))
            .collect();
        for (node, color, shaped) in colors {
            if let (Some(color), false) = (color, shaped) {
                self.set_inline_style(NodeId(node), &format!("background-color: {}", color.css()));
            }
        }
    }

    /// A key's label on `side` of its device: each action beside its chip,
    /// the chip nearest the line, or for a conflict, every action in a box
    /// with a warning and the one chip.
    fn callout(&mut self, map: NodeId, key_at: usize, key: &KeymapKey, side: KeymapSide) -> Callout {
        let left = side == KeymapSide::Left;
        let conflict = key.state == KeyState::Conflict;
        let callout = self.panel(map, if conflict { Style::row() } else { Style::column() });
        self.add_class(callout, "keymap-callout");
        self.add_class(callout, side_class(side));
        self.set_inline_style(callout, "position: absolute; left: 0px; top: 0px");
        let mut first_chip = None;
        let mut chip = |ui: &mut Ui, parent: NodeId, text: &str| {
            if text.is_empty() {
                return;
            }
            let node = ui.add_label(parent, text);
            ui.add_class(node, "keymap-chip");
            if conflict {
                ui.add_class(node, "conflict");
            }
            first_chip.get_or_insert(node.0);
        };
        if conflict {
            self.add_class(callout, "conflict");
            let named = key.callout.iter().find(|line| !line.chip.is_empty()).map(|line| line.chip.clone());
            let named = named.unwrap_or_default();
            let warn = |ui: &mut Ui| {
                let mark = ui.icon(callout, "alert");
                ui.add_class(mark, "keymap-callout-warn");
            };
            if left {
                warn(self);
            } else {
                chip(self, callout, &named);
            }
            let texts = self.panel(callout, Style::column());
            self.add_class(texts, "keymap-callout-texts");
            for line in &key.callout {
                let text = self.add_label(texts, &line.text);
                self.add_class(text, "keymap-callout-line");
            }
            if left {
                chip(self, callout, &named);
            } else {
                warn(self);
            }
        } else {
            for line in &key.callout {
                let row = self.panel(callout, Style::row());
                self.add_class(row, "keymap-callout-row");
                if !left {
                    chip(self, row, &line.chip);
                }
                if let Some(icon) = &line.icon {
                    let mark = self.icon(row, icon);
                    self.add_class(mark, "keymap-callout-icon");
                }
                let text = self.add_label(row, &line.text);
                self.add_class(text, "keymap-callout-line");
                if left {
                    chip(self, row, &line.chip);
                }
            }
        }
        Callout { key: key_at, node: callout.0, chip: first_chip, side }
    }

    /// The key of a key map clicked since the last frame, by its place in
    /// the map's data.
    pub fn keymap_clicked(&self, map: NodeId) -> Option<usize> {
        let keymap = self.keymaps.get(&map.0)?;
        let key = keymap.keys.iter().position(|&key| self.clicked(NodeId(key)));
        key.or_else(|| keymap.callouts.iter().find(|c| self.clicked(NodeId(c.node))).map(|c| c.key))
    }

    /// The key of a key map under the pointer, by its place.
    pub fn keymap_hovered(&self, map: NodeId) -> Option<usize> {
        let keymap = self.keymaps.get(&map.0)?;
        let key = keymap.keys.iter().position(|&key| self.is_hovered(NodeId(key)));
        key.or_else(|| keymap.callouts.iter().find(|c| self.is_hovered(NodeId(c.node))).map(|c| c.key))
    }

    /// Where each key of a key map is on screen, by its place: left, top,
    /// width and height, as the last frame laid it out.
    pub fn keymap_key_rect(&self, map: NodeId, key: usize) -> Option<[f32; 4]> {
        let node = *self.keymaps.get(&map.0)?.keys.get(key)?;
        Some(self.rect(NodeId(node)))
    }

    /// The lines of a key map, each from its key to its label, as the last
    /// frame placed them, in the map's own logical pixels, by the key's place.
    pub fn keymap_lines(&self, map: NodeId) -> Vec<(usize, Vec<[f32; 2]>)> {
        self.keymaps.get(&map.0).map(|keymap| keymap.placed_lines.clone()).unwrap_or_default()
    }

    /// What is wrong with how a key map's lines run, as the last frame
    /// placed them, one sentence each: a line that is not level or upright,
    /// that does not run from its key's edge to its label's, or that comes
    /// within a pixel of another key or label, or another line. None when
    /// every line is clean.
    pub fn keymap_crossings(&self, map: NodeId) -> Vec<String> {
        let Some(keymap) = self.keymaps.get(&map.0) else { return Vec::new() };
        let origin = self.rect(map);
        let local = |r: [f32; 4]| [r[0] - origin[0], r[1] - origin[1], r[2], r[3]];
        let keys: Vec<[f32; 4]> = keymap.keys.iter().map(|&k| local(self.rect(NodeId(k)))).collect();
        let labels: Vec<[f32; 4]> = keymap.callouts.iter().map(|c| local(self.rect(NodeId(c.node)))).collect();
        let near = |r: [f32; 4], a: [f32; 2], b: [f32; 2], margin: f32| {
            a[0].max(b[0]) > r[0] - margin
                && a[0].min(b[0]) < r[0] + r[2] + margin
                && a[1].max(b[1]) > r[1] - margin
                && a[1].min(b[1]) < r[1] + r[3] + margin
        };
        let on_edge = |r: [f32; 4], p: [f32; 2]| {
            let e = 0.6;
            let inside = p[0] >= r[0] - e && p[0] <= r[0] + r[2] + e && p[1] >= r[1] - e && p[1] <= r[1] + r[3] + e;
            let edge = (p[0] - r[0]).abs() < e
                || (p[0] - r[0] - r[2]).abs() < e
                || (p[1] - r[1]).abs() < e
                || (p[1] - r[1] - r[3]).abs() < e;
            inside && edge
        };
        let piece = |p: &[[f32; 2]]| {
            [p[0][0].min(p[1][0]), p[0][1].min(p[1][1]), (p[0][0] - p[1][0]).abs(), (p[0][1] - p[1][1]).abs()]
        };
        let name = |at: usize| {
            let key = &keymap.data.keys[keymap.placed_lines[at].0];
            key.callout.first().map_or_else(|| key.label.clone(), |line| line.text.clone())
        };
        let mut problems = Vec::new();
        for (at, (key, points)) in keymap.placed_lines.iter().enumerate() {
            let (Some(first), Some(last)) = (points.first(), points.last()) else { continue };
            if !on_edge(keys[*key], *first) {
                problems.push(format!("{}'s line does not start on its key", name(at)));
            }
            if !labels.get(at).is_some_and(|label| on_edge(*label, *last)) {
                problems.push(format!("{}'s line does not end on its label", name(at)));
            }
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if a[0] != b[0] && a[1] != b[1] {
                    problems.push(format!("{}'s line runs aslant", name(at)));
                }
                for (other, rect) in keys.iter().enumerate().filter(|(other, _)| other != key) {
                    if near(*rect, a, b, 1.0) {
                        problems.push(format!("{}'s line crosses the key {}", name(at), keymap.data.keys[other].label));
                    }
                }
                for (other, rect) in labels.iter().enumerate().filter(|(other, _)| *other != at) {
                    if near(*rect, a, b, 1.0) {
                        problems.push(format!("{}'s line crosses {}'s label", name(at), name(other)));
                    }
                }
                for (other, (_, theirs)) in keymap.placed_lines.iter().enumerate().skip(at + 1) {
                    if theirs.windows(2).any(|their| near(piece(their), a, b, 1.5)) {
                        problems.push(format!("{}'s line meets {}'s", name(at), name(other)));
                    }
                }
            }
        }
        problems.dedup();
        problems
    }

    /// Whether an element is a key map.
    pub fn is_keymap(&self, node: NodeId) -> bool {
        self.keymaps.contains_key(&node.0)
    }

    pub(crate) fn forget_keymap(&mut self, node: TaffyNode) {
        self.keymaps.remove(&node);
    }

    /// The fill and edge a swatch has from the sheets.
    fn swatch(&self, node: TaffyNode) -> (Color, Color) {
        let style = self.style(NodeId(node));
        let fill = style.and_then(|s| s.background).unwrap_or(Color::rgba(0, 0, 0, 0));
        let edge = style.and_then(|s| s.border.map(|(_, color)| color)).unwrap_or(fill);
        (fill, edge)
    }

    /// Fits each key map's devices into it and places its labels round
    /// them, as laid out this frame. Returns whether anything moved, for the
    /// frame to be laid out again.
    pub(crate) fn arrange_keymaps(&mut self) -> bool {
        let maps: Vec<TaffyNode> = self.keymaps.keys().copied().collect();
        let mut moved = false;
        for map in maps {
            let rect = self.rect(NodeId(map));
            let keymap = &self.keymaps[&map];
            let labels: Vec<Label> = keymap
                .callouts
                .iter()
                .map(|c| {
                    let r = self.rect(NodeId(c.node));
                    let chip_x = c.chip.map_or(r[2] / 2.0, |chip| {
                        let chip = self.rect(NodeId(chip));
                        chip[0] - r[0] + chip[2] / 2.0
                    });
                    Label { key: c.key, side: c.side, size: [r[2], r[3]], chip_x }
                })
                .collect();
            // A map not laid out, as one hidden, has nothing to fit its keys to.
            let hidden = rect[2] <= 0.0 || rect[3] <= 0.0;
            let captions_laid = keymap.captions.iter().all(|(_, c)| self.rect(NodeId(*c))[2] > 0.0);
            if hidden || (keymap.arranged == Some(rect) && captions_laid && labels.iter().all(|l| l.size[0] > 0.0)) {
                continue;
            }
            let placed = arrange(&keymap.data, &labels, [rect[2], rect[3]]);
            let px = |v: f32| format!("{v:.1}px");
            let (board, lines) = (keymap.board, keymap.lines);
            let data = keymap.data.clone();
            let (keys, rings, icons, captions) =
                (keymap.keys.clone(), keymap.rings.clone(), keymap.icons.clone(), keymap.captions.clone());
            let callouts: Vec<(TaffyNode, KeyState)> =
                keymap.callouts.iter().map(|c| (c.node, data.keys[c.key].state)).collect();
            let placed_lines: Vec<(usize, Vec<[f32; 2]>)> =
                keymap.callouts.iter().map(|c| c.key).zip(placed.lines.iter().cloned()).collect();
            let sw = &keymap.swatches;
            let looks = [sw.plate, sw.recess, sw.part].map(|node| self.swatch(node));
            let states = sw.states.map(|node| self.swatch(node));
            let line_colors = [self.swatch(sw.line).0, self.swatch(sw.conflict_line).0];
            self.set_inline_style(
                NodeId(board),
                &format!("position: absolute; left: 0px; top: 0px; width: {}; height: {}", px(rect[2]), px(rect[3])),
            );

            // The devices' art, on the board under the keys.
            let mut art = Vec::new();
            for (piece, at) in data.art.iter().zip(&placed.art) {
                let (fill, edge) = looks[piece.look as usize];
                let [x, y, w, h] = *at;
                match piece.shape {
                    ArtShape::Rect { radius } => {
                        let radius = radius * placed.scale;
                        let grown = [x - EDGE, y - EDGE, w + EDGE * 2.0, h + EDGE * 2.0];
                        art.push(Shape::Rect { rect: grown, radius: radius + EDGE, fill: edge });
                        art.push(Shape::Rect { rect: *at, radius, fill });
                    }
                    ArtShape::Circle => art.push(Shape::Circle {
                        center: [x + w / 2.0, y + h / 2.0],
                        radius: w.min(h) / 2.0,
                        fill,
                        border: Some((EDGE, edge)),
                    }),
                    ArtShape::Path { path, view } => {
                        art.push(Shape::Path { path, view, rect: *at, color: fill, stroke: 0.0 });
                        art.push(Shape::Path { path, view, rect: *at, color: edge, stroke: EDGE * 1.5 });
                    }
                }
            }
            self.set_shapes(NodeId(board), art);

            for (at, (node, key)) in keys.iter().zip(&data.keys).enumerate() {
                let [x, y, w, h] = placed.keys[at];
                let shaped = matches!(key.shape, KeyShape::Path { .. });
                let fill = match (key.color, shaped) {
                    (Some(color), false) => format!("; background-color: {}", color.css()),
                    _ => String::new(),
                };
                let room = [w, h];
                let font = fitted_text(&key.label, room, placed.scale);
                self.set_inline_style(
                    NodeId(*node),
                    &format!(
                        "position: absolute; left: {}; top: {}; width: {}; height: {}; font-size: {}{fill}",
                        px(x),
                        px(y),
                        px(w),
                        px(h),
                        px(font),
                    ),
                );
                if let KeyShape::Path { path, view } = key.shape {
                    let (fill, edge) = states[key.state as usize];
                    let fill = key.color.filter(|_| key.state == KeyState::Assigned).unwrap_or(fill);
                    let rect = [0.0, 0.0, room[0], room[1]];
                    self.set_shapes(
                        NodeId(*node),
                        vec![
                            Shape::Path { path, view, rect, color: fill, stroke: 0.0 },
                            Shape::Path { path, view, rect, color: edge, stroke: EDGE },
                        ],
                    );
                }
            }
            for (at, ring) in rings {
                let [_, _, w, h] = placed.keys[at];
                let d = w.min(h) * 0.62;
                self.set_inline_style(NodeId(ring), &format!("width: {}; height: {}", px(d), px(d)));
            }
            for (at, icon) in icons {
                let [_, _, w, h] = placed.keys[at];
                let d = (w.min(h) * 0.6).round();
                self.set_inline_style(NodeId(icon), &format!("width: {}; height: {}", px(d), px(d)));
            }
            for (at, caption) in captions {
                let [x, y, w, h] = placed.keys[at];
                let size = self.rect(NodeId(caption));
                self.set_inline_style(
                    NodeId(caption),
                    &format!("position: absolute; left: {}; top: {}", px(x + (w - size[2]) / 2.0), px(y + h + 3.0)),
                );
            }
            let mut shapes = Vec::new();
            for ((node, state), (at, points)) in callouts.iter().zip(placed.callouts.iter().zip(&placed.lines)) {
                self.set_inline_style(
                    NodeId(*node),
                    &format!("position: absolute; left: {}; top: {}", px(at[0]), px(at[1])),
                );
                let color = line_colors[usize::from(*state == KeyState::Conflict)];
                for pair in points.windows(2) {
                    shapes.push(Shape::Line { from: pair[0], to: pair[1], width: LINE, color });
                }
            }
            self.set_shapes(NodeId(lines), shapes);
            let keymap = self.keymaps.get_mut(&map).expect("still there");
            keymap.arranged = Some(rect);
            keymap.placed_lines = placed_lines;
            moved = true;
        }
        moved
    }
}

fn state_class(state: KeyState) -> &'static str {
    match state {
        KeyState::Unused => "unused",
        KeyState::Assigned => "assigned",
        KeyState::Hidden => "hidden",
        KeyState::Conflict => "conflict",
    }
}

fn side_class(side: KeymapSide) -> &'static str {
    match side {
        KeymapSide::Left => "left",
        KeymapSide::Right => "right",
        KeymapSide::Top => "top",
        KeymapSide::Bottom => "bottom",
    }
}

/// How big a key's text is drawn for it to fit inside the key, `room`
/// logical pixels wide and high, with a key width `scale` pixels: as big as
/// the key's size gives it, smaller for a long name, as Backspace. Text is
/// not measured until it is laid out, so it is reckoned from how wide its
/// letters are in Inter, by their kind.
fn fitted_text(label: &str, [width, height]: [f32; 2], scale: f32) -> f32 {
    let ems: f32 = label
        .chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | 't' | 'f' | 'r' | 'I' | '1' | '.' | ',' | ';' | ':' | '\'' | '`' | '|' | '!' => 0.32,
            'm' | 'w' | 'M' | 'W' => 0.86,
            c if c.is_uppercase() => 0.66,
            c if c.is_ascii() => 0.56,
            // Arrows and the like are as wide as a capital.
            _ => 0.7,
        })
        .sum();
    let by_size = (scale * 0.36).clamp(KEY_TEXT.1, KEY_TEXT.0);
    let by_width = (width - 4.0).max(0.0) / ems.max(0.01);
    let by_height = height * 0.7;
    by_size.min(by_width).min(by_height).max(KEY_TEXT.1.min(by_size))
}

/// The devices of a map, or one as big as it when it names none.
fn devices(data: &KeymapData) -> Vec<KeymapDevice> {
    if data.devices.is_empty() {
        vec![KeymapDevice { rect: [0.0, 0.0, data.size[0], data.size[1]], rows: false }]
    } else {
        data.devices.clone()
    }
}

fn device_of(data: &KeymapData, devices: usize, key: usize) -> usize {
    data.keys[key].device.min(devices.saturating_sub(1))
}

/// The side each key's label goes on: the one it asks for, or the nearest.
/// On a device whose keys stand in rows, the top row's go above it and the
/// bottom row's below, and the rest to the nearer side, but no more than
/// three of a row to one side while the other has fewer: the innermost go
/// across.
pub fn sides(data: &KeymapData) -> Vec<KeymapSide> {
    let devices = devices(data);
    let mut sides = Vec::with_capacity(data.keys.len());
    for (at, key) in data.keys.iter().enumerate() {
        let d = device_of(data, devices.len(), at);
        let [dx, dy, dw, dh] = devices[d].rect;
        let [x, y, w, h] = key.rect;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let across = if cx < dx + dw / 2.0 { KeymapSide::Left } else { KeymapSide::Right };
        let side = key.side.unwrap_or_else(|| {
            if devices[d].rows {
                let on = data.keys.iter().enumerate().filter(|(i, _)| device_of(data, devices.len(), *i) == d);
                let (top, bottom) =
                    on.fold((f32::MAX, f32::MIN), |(t, b), (_, k)| (t.min(k.rect[1]), b.max(k.rect[1] + k.rect[3])));
                if y <= top + 0.01 {
                    KeymapSide::Top
                } else if y + h >= bottom - 0.01 {
                    KeymapSide::Bottom
                } else {
                    across
                }
            } else if cy < dy + dh * 0.2 {
                KeymapSide::Top
            } else if cy > dy + dh * 0.8 {
                KeymapSide::Bottom
            } else {
                across
            }
        });
        sides.push(side);
    }
    // Rows shared out between the sides.
    for (d, device) in devices.iter().enumerate() {
        if !device.rows {
            continue;
        }
        let labelled = |at: &usize| {
            let key = &data.keys[*at];
            !key.callout.is_empty() && key.side.is_none() && device_of(data, devices.len(), *at) == d
        };
        let mut rows: Vec<f32> = (0..data.keys.len()).filter(labelled).map(|at| data.keys[at].rect[1]).collect();
        rows.sort_by(f32::total_cmp);
        rows.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for row in rows {
            let mut in_row: Vec<usize> = (0..data.keys.len())
                .filter(labelled)
                .filter(|&at| (data.keys[at].rect[1] - row).abs() < 0.01)
                .filter(|&at| matches!(sides[at], KeymapSide::Left | KeymapSide::Right))
                .collect();
            in_row.sort_by(|&a, &b| data.keys[a].rect[0].total_cmp(&data.keys[b].rect[0]));
            loop {
                let left: Vec<usize> = in_row.iter().copied().filter(|&at| sides[at] == KeymapSide::Left).collect();
                let right: Vec<usize> = in_row.iter().copied().filter(|&at| sides[at] == KeymapSide::Right).collect();
                if left.len() > 3 && left.len() > right.len() + 1 {
                    sides[*left.last().expect("more than three")] = KeymapSide::Right;
                } else if right.len() > 3 && right.len() > left.len() + 1 {
                    sides[right[0]] = KeymapSide::Left;
                } else {
                    break;
                }
            }
        }
    }
    sides
}

/// A label as the arranging sees it.
pub(crate) struct Label {
    /// Its key's place.
    pub key: usize,
    pub side: KeymapSide,
    /// How big it is, in logical pixels.
    pub size: [f32; 2],
    /// Where its chip's middle is across it, from its left.
    pub chip_x: f32,
}

/// Where a key map's pieces go, in the map's logical pixels.
pub(crate) struct Placed {
    /// How many pixels a key width is.
    pub scale: f32,
    /// Each key's left, top, width and height.
    pub keys: Vec<[f32; 4]>,
    /// Each piece of art's.
    pub art: Vec<[f32; 4]>,
    /// Each label's left and top, in the labels' order.
    pub callouts: Vec<[f32; 2]>,
    /// Each label's line, from its key to it.
    pub lines: Vec<Vec<[f32; 2]>>,
}

/// Something a line keeps clear of: a key or a well, as a box or a circle.
#[derive(Clone, Copy)]
enum Solid {
    Box([f32; 4]),
    Round([f32; 2], f32),
}

impl Solid {
    /// Whether a level or upright piece of line comes within `margin` of it.
    fn hit(&self, a: [f32; 2], b: [f32; 2], margin: f32) -> bool {
        match *self {
            Solid::Box([x, y, w, h]) => {
                a[0].max(b[0]) > x - margin
                    && a[0].min(b[0]) < x + w + margin
                    && a[1].max(b[1]) > y - margin
                    && a[1].min(b[1]) < y + h + margin
            }
            Solid::Round(c, r) => {
                // Round buttons are passed wider, so a line between two reads as going round them.
                let margin = margin * 2.0;
                let along = [b[0] - a[0], b[1] - a[1]];
                let length = along[0] * along[0] + along[1] * along[1];
                let t = if length > 0.0 {
                    (((c[0] - a[0]) * along[0] + (c[1] - a[1]) * along[1]) / length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let near = [a[0] + along[0] * t - c[0], a[1] + along[1] * t - c[1]];
                (near[0] * near[0] + near[1] * near[1]).sqrt() < r + margin
            }
        }
    }

    fn contains(&self, p: [f32; 2]) -> bool {
        match *self {
            Solid::Box([x, y, w, h]) => p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h,
            Solid::Round(c, r) => (p[0] - c[0]).hypot(p[1] - c[1]) <= r,
        }
    }
}

/// How far apart two level or upright pieces of line are, and whether they
/// run the same way.
fn apart(a: [[f32; 2]; 2], b: [[f32; 2]; 2]) -> (f32, bool) {
    let range = |s: [[f32; 2]; 2], i: usize| (s[0][i].min(s[1][i]), s[0][i].max(s[1][i]));
    let gap = |i: usize| {
        let ((a0, a1), (b0, b1)) = (range(a, i), range(b, i));
        (b0 - a1).max(a0 - b1).max(0.0)
    };
    let level = |s: [[f32; 2]; 2]| (s[0][1] - s[1][1]).abs() < 0.01;
    (gap(0).max(gap(1)), level(a) == level(b))
}

/// Fits the devices into a map `size` pixels big, side by side with room
/// round each for its labels, and places each label, with its line. What is
/// placed is measured: when it reaches past the map, the keys are made
/// smaller, below the size they would keep otherwise, until it fits, and
/// whatever fits is centred.
pub(crate) fn arrange(data: &KeymapData, labels: &[Label], size: [f32; 2]) -> Placed {
    let mut placed = arrange_at(data, labels, size, None);
    let reach = |placed: &Placed| {
        let mut points: Vec<[f32; 2]> = placed.lines.iter().flatten().copied().collect();
        for rect in placed.keys.iter().chain(&placed.art) {
            points.extend([[rect[0], rect[1]], [rect[0] + rect[2], rect[1] + rect[3]]]);
        }
        for (at, label) in placed.callouts.iter().zip(labels) {
            points.extend([*at, [at[0] + label.size[0], at[1] + label.size[1]]]);
        }
        points.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |[l, t, r, b], p| {
            [l.min(p[0]), t.min(p[1]), r.max(p[0]), b.max(p[1])]
        })
    };
    // Only the devices shrink, so each try takes what they would give up from their share of the width or
    // height; the lines' columns change with them, so it may take more than one.
    let devices = devices(data);
    let widths: f32 = devices.iter().map(|d| d.rect[2]).sum::<f32>().max(0.01);
    let tallest: f32 = devices.iter().map(|d| d.rect[3]).fold(0.0f32, f32::max).max(0.01);
    for _ in 0..4 {
        let [l, t, r, b] = reach(&placed);
        let over = [(r - l) - size[0], (b - t) - size[1]];
        if (over[0] <= 0.5 && over[1] <= 0.5) || placed.scale <= SMALLEST_KEY {
            break;
        }
        let by_width = placed.scale - (over[0].max(0.0) + 1.0) / widths;
        let by_height = placed.scale - (over[1].max(0.0) + 1.0) / tallest;
        let scale = by_width.min(by_height).clamp(SMALLEST_KEY, placed.scale);
        placed = arrange_at(data, labels, size, Some(scale));
    }
    // Centred on what it reaches, or from the left and top when it is bigger.
    let [l, t, r, b] = reach(&placed);
    let shift = [(size[0] - (r - l)) / 2.0 - l, (size[1] - (b - t)) / 2.0 - t];
    let shift = [shift[0].max(-l), shift[1].max(-t)];
    let moved = |rect: &mut [f32; 4]| {
        rect[0] += shift[0];
        rect[1] += shift[1];
    };
    placed.keys.iter_mut().chain(placed.art.iter_mut()).for_each(moved);
    for at in &mut placed.callouts {
        *at = [at[0] + shift[0], at[1] + shift[1]];
    }
    for point in placed.lines.iter_mut().flatten() {
        *point = [point[0] + shift[0], point[1] + shift[1]];
    }
    placed
}

/// [`arrange`] at a key width of `scale` pixels, or by the map's size.
fn arrange_at(data: &KeymapData, labels: &[Label], size: [f32; 2], scale: Option<f32>) -> Placed {
    use KeymapSide::{Bottom, Left, Right, Top};
    let devices = devices(data);
    let n = devices.len();
    let device = |label: &Label| device_of(data, n, label.key);
    // How far out of a device its labels on one side reach.
    let room = |d: usize, side: KeymapSide| {
        let on: Vec<&Label> = labels.iter().filter(|l| device(l) == d && l.side == side).collect();
        if on.is_empty() {
            return 0.0;
        }
        let across = on.iter().map(|l| if matches!(side, Left | Right) { l.size[0] } else { l.size[1] });
        OUT + COLUMN * on.len().min(ROOM_COLUMNS) as f32 + LABEL_GAP + across.fold(0.0f32, f32::max)
    };
    let lefts: Vec<f32> = (0..n).map(|d| room(d, Left)).collect();
    let rights: Vec<f32> = (0..n).map(|d| room(d, Right)).collect();
    let tops: Vec<f32> = (0..n).map(|d| room(d, Top)).collect();
    let bottoms: Vec<f32> = (0..n).map(|d| room(d, Bottom)).collect();
    let gaps = DEVICE_GAP * (n - 1) as f32;
    // Side by side, the devices share the rows of labels above and below them.
    let (top, bottom) = (tops.iter().copied().fold(0.0f32, f32::max), bottoms.iter().copied().fold(0.0f32, f32::max));
    let widths: f32 = devices.iter().map(|d| d.rect[2]).sum::<f32>().max(0.01);
    let tallest = devices.iter().map(|d| d.rect[3]).fold(0.0f32, f32::max).max(0.01);
    let fixed = lefts.iter().sum::<f32>() + rights.iter().sum::<f32>() + gaps;
    let across = ((size[0] - fixed) / widths).min((size[1] - top - bottom) / tallest);
    // One under another, each has its own.
    let heights: f32 = devices.iter().map(|d| d.rect[3]).sum::<f32>().max(0.01);
    let fixed_down = tops.iter().sum::<f32>() + bottoms.iter().sum::<f32>() + gaps;
    let down = devices
        .iter()
        .enumerate()
        .map(|(d, device)| (size[0] - lefts[d] - rights[d]) / device.rect[2].max(0.01))
        .fold((size[1] - fixed_down) / heights, f32::min);
    // They stand side by side when that leaves the keys big enough, or bigger than one under another would.
    let side_by_side = n == 1 || across >= MIN_KEY || across >= down;
    // A key stays big enough to read, though the labels then reach past the map's edges.
    let scale = scale.unwrap_or_else(|| if side_by_side { across } else { down }.clamp(MIN_KEY, MAX_KEY));
    let (mut origins, mut bounds) = (Vec::new(), Vec::new());
    if side_by_side {
        let mut x = ((size[0] - fixed - widths * scale) / 2.0).max(0.0);
        let middle = top + ((size[1] - top - bottom - tallest * scale) / 2.0).max(0.0);
        for (d, device) in devices.iter().enumerate() {
            let [dx, dy, dw, dh] = device.rect;
            x += lefts[d];
            let y = middle + (tallest - dh) * scale / 2.0;
            origins.push([x - dx * scale, y - dy * scale]);
            bounds.push([x, y, dw * scale, dh * scale]);
            x += dw * scale + rights[d] + DEVICE_GAP;
        }
    } else {
        let mut y = ((size[1] - fixed_down - heights * scale) / 2.0).max(0.0);
        for (d, device) in devices.iter().enumerate() {
            let [dx, dy, dw, dh] = device.rect;
            y += tops[d];
            let x = ((size[0] - lefts[d] - dw * scale - rights[d]) / 2.0).max(0.0) + lefts[d];
            origins.push([x - dx * scale, y - dy * scale]);
            bounds.push([x, y, dw * scale, dh * scale]);
            y += dh * scale + bottoms[d] + DEVICE_GAP;
        }
    }
    let place = |rect: [f32; 4], d: usize| {
        let [x, y, w, h] = rect;
        [origins[d][0] + x * scale, origins[d][1] + y * scale, w * scale, h * scale]
    };
    // Keys stand a little apart, but for shaped ones, which draw their own gaps.
    let inset = (scale * 0.06).max(1.0);
    let keys: Vec<[f32; 4]> = data
        .keys
        .iter()
        .enumerate()
        .map(|(at, k)| {
            let [x, y, w, h] = place(k.rect, device_of(data, n, at));
            let i = if matches!(k.shape, KeyShape::Path { .. }) { 0.0 } else { inset };
            [x + i, y + i, w - i * 2.0, h - i * 2.0]
        })
        .collect();
    let art: Vec<[f32; 4]> = data.art.iter().map(|a| place(a.rect, a.device.min(n - 1))).collect();

    // What lines keep clear of: every key, and the wells but the one a line starts in.
    let solid = |rect: [f32; 4], round: bool| match round {
        true => Solid::Round([rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0], rect[2].min(rect[3]) / 2.0),
        false => Solid::Box(rect),
    };
    let key_solids: Vec<Solid> = data
        .keys
        .iter()
        .zip(&keys)
        .map(|(k, r)| solid(*r, matches!(k.shape, KeyShape::Round | KeyShape::Stick)))
        .collect();
    let wells: Vec<Solid> = data
        .art
        .iter()
        .zip(&art)
        .filter(|(a, _)| a.look == ArtLook::Recess)
        .map(|(a, r)| solid(*r, a.shape == ArtShape::Circle))
        .collect();
    // Lanes in the gaps between rows are as far apart as the gaps allow.
    let step = LANE.min(scale * 0.15);
    let margin = MARGIN.min(step);

    // Each line out of its device, the outermost keys' first so those further in pass under them.
    let mut order: Vec<usize> = (0..labels.len()).collect();
    let outermost = |i: usize| {
        let [x, y, w, h] = keys[labels[i].key];
        match labels[i].side {
            Left => x,
            Right => -(x + w),
            Top => y,
            Bottom => -(y + h),
        }
    };
    order.sort_by(|&a, &b| {
        (labels[a].side as u8, outermost(a)).partial_cmp(&(labels[b].side as u8, outermost(b))).unwrap()
    });
    let mut drawn: Vec<(usize, [[f32; 2]; 2])> = Vec::new();
    let mut exits: Vec<Vec<[f32; 2]>> = vec![Vec::new(); labels.len()];
    for i in order {
        let label = &labels[i];
        let (k, d) = (label.key, device(label));
        let [x, y, w, h] = keys[k];
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let own_well: Vec<bool> = wells.iter().map(|well| well.contains([cx, cy])).collect();
        let clear = |points: &[[f32; 2]]| {
            points.windows(2).all(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let keys_clear = key_solids.iter().enumerate().all(|(at, s)| at == k || !s.hit(a, b, margin));
                let wells_clear = wells.iter().zip(&own_well).all(|(s, own)| *own || !s.hit(a, b, margin));
                let lines_clear = drawn.iter().all(|(_, other)| {
                    let (gap, same_way) = apart([a, b], *other);
                    gap >= if same_way { step - 0.5 } else { 2.0 }
                });
                keys_clear && wells_clear && lines_clear
            })
        };
        let [bx, by, bw, bh] = bounds[d];
        let route = match label.side {
            Left | Right => {
                let (from, edge) = if label.side == Left { (x, bx) } else { (x + w, bx + bw) };
                let straight = vec![[from, cy], [edge, cy]];
                if clear(&straight) {
                    straight
                } else {
                    let lane = |sign: f32| {
                        let start = if sign > 0.0 { y + h } else { y };
                        (0..80)
                            .map(|n| start + sign * (margin + n as f32 * step))
                            .take_while(|lane| *lane > by - scale * 2.0 && *lane < by + bh + scale * 2.0)
                            .map(|lane| vec![[cx, start], [cx, lane], [edge, lane]])
                            .find(|route| clear(route))
                    };
                    let travel = |route: &Vec<[f32; 2]>| (route[1][1] - route[0][1]).abs();
                    // A keyboard's keys always drop when they can, so their labels keep the keys' order.
                    let rows = devices[d].rows;
                    match (lane(1.0), lane(-1.0)) {
                        (Some(down), Some(up)) if !rows && travel(&up) + 2.0 < travel(&down) => up,
                        (Some(down), _) => down,
                        (None, Some(up)) => up,
                        (None, None) => straight,
                    }
                }
            }
            Top => vec![[cx, y], [cx, by]],
            Bottom => vec![[cx, y + h], [cx, by + bh]],
        };
        for pair in route.windows(2) {
            drawn.push((i, [pair[0], pair[1]]));
        }
        exits[i] = route;
    }

    // Out of each device, the labels on each side stand in their lines' order.
    let mut callouts = vec![[0.0f32; 2]; labels.len()];
    let mut lines = vec![Vec::new(); labels.len()];
    for d in 0..n {
        let [bx, by, bw, bh] = bounds[d];
        for side in [Left, Right, Top, Bottom] {
            let upright = matches!(side, Left | Right);
            let mut on: Vec<usize> =
                (0..labels.len()).filter(|&i| device(&labels[i]) == d && labels[i].side == side).collect();
            if on.is_empty() {
                continue;
            }
            // Along the side: down it for one beside the device, across for one above or below.
            let along = |i: usize| {
                let end = *exits[i].last().expect("a line has two ends");
                if upright { end[1] } else { end[0] }
            };
            on.sort_by(|&a, &b| along(a).total_cmp(&along(b)));
            let extent: Vec<f32> =
                on.iter().map(|&i| if upright { labels[i].size[1] } else { labels[i].size[0] }).collect();
            let anchor: Vec<f32> =
                on.iter().map(|&i| if upright { labels[i].size[1] / 2.0 } else { labels[i].chip_x }).collect();
            let gaps: Vec<f32> = on
                .windows(2)
                .map(|pair| {
                    let rows = devices[d].rows && upright;
                    let (a, b) = (data.keys[labels[pair[0]].key].rect[1], data.keys[labels[pair[1]].key].rect[1]);
                    match (upright, rows && (a - b).abs() > 0.01) {
                        (false, _) => ACROSS_GAP,
                        (true, true) => SPACING + ROW_GAP,
                        (true, false) => SPACING,
                    }
                })
                .collect();
            let exit: Vec<f32> = on.iter().map(|&i| along(i)).collect();
            let ideal: Vec<f32> = exit.iter().zip(&anchor).map(|(e, a)| e - a).collect();
            let limit = if upright { size[1] } else { size[0] };
            let mut starts = stack(&ideal, &extent, &gaps, [0.0, limit]);
            // A label a few pixels off its line's exit lines up with it where there is room, so the line runs
            // straight rather than stepping.
            for at in 0..starts.len() {
                let want = exit[at] - anchor[at];
                if (want - starts[at]).abs() > SNAP {
                    continue;
                }
                let after = at == 0 || starts[at - 1] + extent[at - 1] + gaps[at - 1] <= want + 0.01;
                let before = at + 1 == starts.len() || want + extent[at] + gaps[at] <= starts[at + 1] + 0.01;
                if after && before {
                    starts[at] = want;
                }
            }
            let ends: Vec<f32> = starts.iter().zip(&anchor).map(|(s, a)| s + a).collect();
            let columns = bend_columns(&exit, &ends);
            let deepest = columns.iter().flatten().copied().max().unwrap_or(0);
            let label_out = OUT + COLUMN * deepest as f32 + LABEL_GAP;
            let real = |along: f32, out: f32| match side {
                Left => [bx - out, along],
                Right => [bx + bw + out, along],
                Top => [along, by - out],
                Bottom => [along, by + bh + out],
            };
            for (at, &i) in on.iter().enumerate() {
                let [lw, lh] = labels[i].size;
                callouts[i] = match side {
                    Left => [bx - label_out - lw, starts[at]],
                    Right => [bx + bw + label_out, starts[at]],
                    Top => [starts[at], by - label_out - lh],
                    Bottom => [starts[at], by + bh + label_out],
                };
                let mut points = exits[i].clone();
                if let Some(column) = columns[at] {
                    let out = OUT + COLUMN * column as f32;
                    points.push(real(exit[at], out));
                    points.push(real(ends[at], out));
                }
                points.push(real(ends[at], label_out));
                points.dedup();
                lines[i] = points;
            }
        }
    }
    Placed { scale, keys, art, callouts, lines }
}

/// Labels stood along a side: each as near as it can be to where it would
/// be on its own, `ideal`, apart from the next by `gaps`. Labels that would
/// overlap stand together, centred on where they would each be, and all of
/// them inside `limit`.
fn stack(ideal: &[f32], extent: &[f32], gaps: &[f32], limit: [f32; 2]) -> Vec<f32> {
    let n = ideal.len();
    // Where each label is in a run of them that starts at `first`.
    let offset = |first: usize, at: usize| (first..at).map(|j| extent[j] + gaps[j]).sum::<f32>();
    let mut runs: Vec<(usize, usize, f32)> = Vec::new();
    for at in 0..n {
        runs.push((at, at, ideal[at]));
        while runs.len() >= 2 {
            let (b, a) = (runs[runs.len() - 1], runs[runs.len() - 2]);
            let a_end = a.2 + offset(a.0, a.1) + extent[a.1];
            if a_end + gaps[a.1] <= b.2 {
                break;
            }
            let (first, last) = (a.0, b.1);
            let start = (first..=last).map(|j| ideal[j] - offset(first, j)).sum::<f32>() / (last - first + 1) as f32;
            runs.truncate(runs.len() - 2);
            runs.push((first, last, start));
        }
    }
    let mut starts = vec![0.0; n];
    for (first, last, start) in runs {
        for j in first..=last {
            starts[j] = start + offset(first, j);
        }
    }
    // Kept inside: pushed down past the start, up from the end, and down again should they not fit.
    for at in 0..n {
        let floor = if at == 0 { limit[0] } else { starts[at - 1] + extent[at - 1] + gaps[at - 1] };
        starts[at] = starts[at].max(floor);
    }
    for at in (0..n).rev() {
        let ceiling = if at + 1 == n { limit[1] - extent[at] } else { starts[at + 1] - gaps[at] - extent[at] };
        starts[at] = starts[at].min(ceiling);
    }
    for at in 0..n {
        let floor = if at == 0 { limit[0] } else { starts[at - 1] + extent[at - 1] + gaps[at - 1] };
        starts[at] = starts[at].max(floor);
    }
    starts
}

/// The column each line bends in, out from its device, counted from the
/// nearest; `None` for one that runs straight to its label. `exit` is where
/// each leaves the device and `end` where it meets its label, along the
/// side, in order. A line that rises to its label bends further out than any
/// line above it that it rises past, and one that falls further out than any
/// below it that it falls past, so none crosses another.
fn bend_columns(exit: &[f32], end: &[f32]) -> Vec<Option<usize>> {
    let n = exit.len();
    let mut columns: Vec<Option<usize>> = vec![None; n];
    let rises = |i: usize| end[i] < exit[i] - 0.25;
    let falls = |i: usize| end[i] > exit[i] + 0.25;
    for i in 0..n {
        if rises(i) {
            let past = (0..i).filter(|&j| rises(j) && exit[j] > end[i] - LANE);
            columns[i] = Some(past.filter_map(|j| columns[j]).map(|c| c + 1).max().unwrap_or(0));
        }
    }
    for i in (0..n).rev() {
        if falls(i) {
            let past = (i + 1..n).filter(|&j| falls(j) && exit[j] < end[i] + LANE);
            columns[i] = Some(past.filter_map(|j| columns[j]).map(|c| c + 1).max().unwrap_or(0));
        }
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_that_would_overlap_stand_together_round_their_keys() {
        let starts = stack(&[0.0, 10.0, 100.0], &[22.0; 3], &[4.0; 2], [-100.0, 1000.0]);
        // The first two overlap, so they centre on where they would be: -8 and 18 averaged about 5.
        assert_eq!(starts, [-8.0, 18.0, 100.0]);
        let kept = stack(&[-50.0], &[22.0], &[], [0.0, 100.0]);
        assert_eq!(kept, [0.0], "kept inside");
    }

    #[test]
    fn lines_that_rise_past_each_other_bend_in_columns_further_out() {
        // Four lines leave 10 apart and meet labels 26 apart, centred on them: the top two rise to
        // their labels and the bottom two fall. The inner of each pair ends within a lane of where
        // the outer leaves, so it bends further out.
        let columns = bend_columns(&[0.0, 10.0, 20.0, 30.0], &[-24.0, 2.0, 28.0, 54.0]);
        assert_eq!(columns, [Some(0), Some(1), Some(1), Some(0)]);
        // One rising past where another leaves bends outside it.
        assert_eq!(bend_columns(&[0.0, 10.0], &[-30.0, -4.0]), [Some(0), Some(1)]);
        assert_eq!(bend_columns(&[5.0], &[5.0]), [None], "straight");
    }
}
