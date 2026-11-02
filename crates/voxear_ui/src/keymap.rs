//! Key maps: a device drawn as its keys, each bound one lit, with a label
//! beside the device naming its actions and a line out to it, as a game's
//! key binding screen shows them. The keys and labels are elements, styled
//! by sheets:
//!
//! - `keymap` the whole, `keymap-board` the device under its keys;
//! - `keymap-key` each key, with `round`, `stick` or `pill` for its shape
//!   and `assigned`, `hidden` or `unused` for whether it is bound;
//!   `keymap-key-label` its face;
//! - `keymap-callout` each label, `keymap-callout-line` each action in it;
//!   the lines to them take the callout's border colour.
//!
//! A key's own colour, as a category's, is given with its data and wins over
//! the sheets' (see [`KeymapKey::color`]). What is on the keys comes from
//! whoever owns the map, as `voxear_input`'s layouts and bindings, so the
//! same view serves the editor and a game.
//!
//! Each frame, once laid out, the board is fitted into the middle of the
//! map and the labels placed round it: those of keys near its top above it,
//! near its bottom below, and the rest on the nearer side, in tiers where
//! they would overlap.

use std::collections::HashMap;

use taffy::NodeId as TaffyNode;

use crate::{Color, NodeId, Shape, Style, Ui};

/// How a key is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyShape {
    #[default]
    Key,
    Round,
    Stick,
    Pill,
}

/// Whether a key is bound to anything shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyState {
    #[default]
    Unused,
    Assigned,
    /// Bound only to actions that keep out of key maps.
    Hidden,
}

/// One key of a key map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapKey {
    /// Where it is and how big, in the map's own units, as key widths.
    pub rect: [f32; 4],
    pub shape: KeyShape,
    pub label: String,
    pub state: KeyState,
    /// Its fill, as its actions' category's; `None` leaves it to the sheets.
    pub color: Option<Color>,
    /// The actions to name beside the device, one a line; none for no label.
    pub callout: Vec<String>,
}

/// What a key map shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapData {
    /// How big the device is, in its keys' units.
    pub size: [f32; 2],
    pub keys: Vec<KeymapKey>,
}

/// A key map's elements, kept by the [`Ui`].
pub(crate) struct Keymap {
    board: TaffyNode,
    lines: TaffyNode,
    data: KeymapData,
    keys: Vec<TaffyNode>,
    /// Each key's label, by the key's place, when it has one.
    callouts: Vec<(usize, TaffyNode)>,
    /// The map's size it was last arranged for.
    arranged: Option<[f32; 4]>,
}

/// The gap between the board and the labels, and between labels, in
/// logical pixels.
const GAP: f32 = 14.0;
const SPACING: f32 = 4.0;
/// The smallest a key width is drawn, in logical pixels.
const MIN_KEY: f32 = 18.0;

impl Ui {
    /// A key map under `parent`, empty until [`set_keymap`](Self::set_keymap)
    /// fills it.
    pub fn keymap(&mut self, parent: NodeId) -> NodeId {
        let map = self.panel(parent, Style::default());
        self.add_class(map, "keymap");
        let lines = self.panel(map, Style { pickable: false, ..Style::cover() });
        self.add_class(lines, "keymap-lines");
        let board = self.panel(map, Style::default());
        self.add_class(board, "keymap-board");
        self.set_inline_style(board, "position: absolute; left: 0px; top: 0px; width: 0px; height: 0px");
        let keymap = Keymap {
            board: board.0,
            lines: lines.0,
            data: KeymapData::default(),
            keys: Vec::new(),
            callouts: Vec::new(),
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
        let (board, old_keys, old_callouts) = (keymap.board, keymap.keys.clone(), keymap.callouts.clone());
        for node in old_keys.into_iter().chain(old_callouts.into_iter().map(|(_, node)| node)) {
            self.remove(NodeId(node));
        }
        let mut keys = Vec::new();
        let mut callouts = Vec::new();
        for (at, key) in data.keys.iter().enumerate() {
            let node = self.panel(NodeId(board), Style::default());
            self.add_class(node, "keymap-key");
            let shape = match key.shape {
                KeyShape::Key => None,
                KeyShape::Round => Some("round"),
                KeyShape::Stick => Some("stick"),
                KeyShape::Pill => Some("pill"),
            };
            if let Some(shape) = shape {
                self.add_class(node, shape);
            }
            let state = match key.state {
                KeyState::Unused => "unused",
                KeyState::Assigned => "assigned",
                KeyState::Hidden => "hidden",
            };
            self.add_class(node, state);
            let face = self.add_label(node, &key.label);
            self.add_class(face, "keymap-key-label");
            keys.push(node.0);
            if key.callout.is_empty() {
                continue;
            }
            let callout = self.panel(map, Style::column());
            self.add_class(callout, "keymap-callout");
            self.set_inline_style(callout, "position: absolute; left: 0px; top: 0px");
            for line in &key.callout {
                let text = self.add_label(callout, line);
                self.add_class(text, "keymap-callout-line");
            }
            callouts.push((at, callout.0));
        }
        let keymap = self.keymaps.get_mut(&map.0).expect("looked up above");
        (keymap.keys, keymap.callouts, keymap.data, keymap.arranged) = (keys.clone(), callouts, data, None);
        // A key's own colour wins over the sheets'.
        let colors: Vec<(TaffyNode, Option<Color>)> =
            keys.iter().zip(&keymap.data.keys).map(|(&node, key)| (node, key.color)).collect();
        for (node, color) in colors {
            if let Some(color) = color {
                self.set_inline_style(NodeId(node), &format!("background-color: {}", color.css()));
            }
        }
    }

    /// The key of a key map clicked since the last frame, by its place in
    /// the map's data.
    pub fn keymap_clicked(&self, map: NodeId) -> Option<usize> {
        let keymap = self.keymaps.get(&map.0)?;
        let key = keymap.keys.iter().position(|&key| self.clicked(NodeId(key)));
        key.or_else(|| keymap.callouts.iter().find(|(_, callout)| self.clicked(NodeId(*callout))).map(|(at, _)| *at))
    }

    /// The key of a key map under the pointer, by its place.
    pub fn keymap_hovered(&self, map: NodeId) -> Option<usize> {
        let keymap = self.keymaps.get(&map.0)?;
        let key = keymap.keys.iter().position(|&key| self.is_hovered(NodeId(key)));
        key.or_else(|| keymap.callouts.iter().find(|(_, callout)| self.is_hovered(NodeId(*callout))).map(|(at, _)| *at))
    }

    /// Where each key of a key map is on screen, by its place: left, top,
    /// width and height, as the last frame laid it out.
    pub fn keymap_key_rect(&self, map: NodeId, key: usize) -> Option<[f32; 4]> {
        let node = *self.keymaps.get(&map.0)?.keys.get(key)?;
        Some(self.rect(NodeId(node)))
    }

    /// Whether an element is a key map.
    pub fn is_keymap(&self, node: NodeId) -> bool {
        self.keymaps.contains_key(&node.0)
    }

    pub(crate) fn forget_keymap(&mut self, node: TaffyNode) {
        self.keymaps.remove(&node);
    }

    /// Fits each key map's board into its middle and places its labels round
    /// it, as laid out this frame. Returns whether anything moved, for the
    /// frame to be laid out again.
    pub(crate) fn arrange_keymaps(&mut self) -> bool {
        let maps: Vec<TaffyNode> = self.keymaps.keys().copied().collect();
        let mut moved = false;
        for map in maps {
            let rect = self.rect(NodeId(map));
            let sizes: Vec<[f32; 2]> = {
                let keymap = &self.keymaps[&map];
                keymap.callouts.iter().map(|(_, c)| self.rect(NodeId(*c))).map(|r| [r[2], r[3]]).collect()
            };
            let keymap = &self.keymaps[&map];
            // A map not laid out, as one hidden, has nothing to fit its keys to.
            let hidden = rect[2] <= 0.0 || rect[3] <= 0.0;
            if hidden || (keymap.arranged == Some(rect) && sizes.iter().all(|s| s[0] > 0.0)) {
                continue;
            }
            let placed = arrange(&keymap.data, &keymap.callouts, &sizes, [rect[2], rect[3]]);
            let (board, lines, keys, callouts) =
                (keymap.board, keymap.lines, keymap.keys.clone(), keymap.callouts.clone());
            let line_color = callouts
                .first()
                .and_then(|(_, c)| self.style(NodeId(*c)))
                .and_then(|style| style.border.map(|(_, color)| color))
                .unwrap_or(Color::rgba(160, 166, 176, 200));
            let [bx, by, scale] = placed.board;
            let size = keymap.data.size;
            let px = |v: f32| format!("{v:.1}px");
            self.set_inline_style(
                NodeId(board),
                &format!(
                    "position: absolute; left: {}; top: {}; width: {}; height: {}",
                    px(bx),
                    px(by),
                    px(size[0] * scale),
                    px(size[1] * scale)
                ),
            );
            let key_rects: Vec<[f32; 4]> = self.keymaps[&map].data.keys.iter().map(|k| k.rect).collect();
            for (node, [x, y, w, h]) in keys.iter().zip(key_rects) {
                let inset = (scale * 0.06).max(1.0);
                let color =
                    self.keymaps[&map].data.keys.iter().zip(&keys).find(|(_, n)| *n == node).and_then(|(k, _)| k.color);
                let fill = color.map(|c| format!("; background-color: {}", c.css())).unwrap_or_default();
                let font = (scale * 0.34).clamp(8.0, 14.0);
                self.set_inline_style(
                    NodeId(*node),
                    &format!(
                        "position: absolute; left: {}; top: {}; width: {}; height: {}; font-size: {}{fill}",
                        px(x * scale + inset),
                        px(y * scale + inset),
                        px(w * scale - inset * 2.0),
                        px(h * scale - inset * 2.0),
                        px(font),
                    ),
                );
            }
            for ((_, node), at) in callouts.iter().zip(&placed.callouts) {
                self.set_inline_style(
                    NodeId(*node),
                    &format!("position: absolute; left: {}; top: {}", px(at[0]), px(at[1])),
                );
            }
            let shapes = placed
                .lines
                .iter()
                .flat_map(|points| points.windows(2).map(|pair| (pair[0], pair[1])).collect::<Vec<_>>())
                .map(|(from, to)| Shape::Line { from, to, width: 1.0, color: line_color })
                .collect();
            self.set_shapes(NodeId(lines), shapes);
            let keymap = self.keymaps.get_mut(&map).expect("still there");
            keymap.arranged = Some(rect);
            moved = true;
        }
        moved
    }
}

/// Where a key map's board and labels go, and the lines between.
struct Placed {
    /// The board's left and top, and how many pixels a key width is.
    board: [f32; 3],
    /// Each label's left and top.
    callouts: Vec<[f32; 2]>,
    lines: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Fits the board into the middle of a map `size` pixels big, leaving room
/// round it for the labels, and places each label, `sizes` big, beside it.
fn arrange(data: &KeymapData, callouts: &[(usize, TaffyNode)], sizes: &[[f32; 2]], size: [f32; 2]) -> Placed {
    let widest = sizes.iter().map(|s| s[0]).fold(0.0f32, f32::max).max(60.0);
    let tallest = sizes.iter().map(|s| s[1]).fold(0.0f32, f32::max).max(14.0);
    let room = [
        (size[0] - 2.0 * (widest + GAP)).max(size[0] * 0.3),
        (size[1] - 2.0 * (tallest * 2.0 + GAP * 2.0)).max(size[1] * 0.3),
    ];
    let [w, h] = [data.size[0].max(0.01), data.size[1].max(0.01)];
    // A key stays big enough to read, though the labels then reach past the map's edges.
    let scale = (room[0] / w).min(room[1] / h).max(MIN_KEY);
    let board = [(size[0] - w * scale) / 2.0, (size[1] - h * scale) / 2.0];
    let key_at = |at: usize| {
        let [x, y, kw, kh] = data.keys[at].rect;
        [board[0] + x * scale, board[1] + y * scale, kw * scale, kh * scale]
    };
    let side_of = |at: usize| {
        let [x, y, kw, kh] = data.keys[at].rect;
        let (cx, cy) = (x + kw / 2.0, y + kh / 2.0);
        if cy < h * 0.2 {
            Side::Top
        } else if cy > h * 0.8 {
            Side::Bottom
        } else if cx < w / 2.0 {
            Side::Left
        } else {
            Side::Right
        }
    };
    let mut placed = vec![[0.0f32; 2]; callouts.len()];
    let mut lines = vec![Vec::new(); callouts.len()];
    let mut by_side: HashMap<u8, Vec<usize>> = HashMap::new();
    for (i, (at, _)) in callouts.iter().enumerate() {
        by_side.entry(side_of(*at) as u8).or_default().push(i);
    }
    let board_rect = [board[0], board[1], w * scale, h * scale];
    for (side, mut list) in by_side {
        let side = [Side::Top, Side::Bottom, Side::Left, Side::Right][side as usize];
        let centre = |i: usize| {
            let [x, y, kw, kh] = key_at(callouts[i].0);
            [x + kw / 2.0, y + kh / 2.0]
        };
        match side {
            Side::Top | Side::Bottom => {
                list.sort_by(|a, b| centre(*a)[0].total_cmp(&centre(*b)[0]));
                // Each tier keeps where its last label ended, so the next goes further out when it would overlap.
                let mut tiers: Vec<f32> = Vec::new();
                for i in list {
                    let [lw, lh] = sizes[i];
                    let [cx, _] = centre(i);
                    let left = (cx - lw / 2.0).clamp(0.0, (size[0] - lw).max(0.0));
                    let tier = tiers.iter().position(|&end| left >= end + SPACING).unwrap_or(tiers.len());
                    if tier == tiers.len() {
                        tiers.push(f32::MIN);
                    }
                    tiers[tier] = left + lw;
                    let out = GAP + tier as f32 * (tallest + SPACING);
                    let top = match side {
                        Side::Top => board_rect[1] - out - lh,
                        _ => board_rect[1] + board_rect[3] + out,
                    };
                    placed[i] = [left, top];
                    let [kx, ky, kw, kh] = key_at(callouts[i].0);
                    let from = [kx + kw / 2.0, if side == Side::Top { ky } else { ky + kh }];
                    let to_y = if side == Side::Top { top + lh } else { top };
                    let elbow = if side == Side::Top { to_y + SPACING } else { to_y - SPACING };
                    let lx = from[0].clamp(left + 4.0, (left + lw - 4.0).max(left + 4.0));
                    lines[i] = vec![from, [from[0], elbow], [lx, elbow], [lx, to_y]];
                }
            }
            Side::Left | Side::Right => {
                list.sort_by(|a, b| centre(*a)[1].total_cmp(&centre(*b)[1]));
                let mut next_top = f32::MIN;
                let mut tops = Vec::new();
                for &i in &list {
                    let [_, lh] = sizes[i];
                    let top = (centre(i)[1] - lh / 2.0).max(next_top);
                    tops.push(top);
                    next_top = top + lh + SPACING;
                }
                // Pushed past the bottom, the column moves up as far as it overflows.
                let overflow = (next_top - SPACING - size[1]).max(0.0);
                for (&i, top) in list.iter().zip(tops) {
                    let [lw, lh] = sizes[i];
                    let top = (top - overflow).max(0.0);
                    let left = match side {
                        Side::Left => (board_rect[0] - GAP - lw).max(0.0),
                        _ => (board_rect[0] + board_rect[2] + GAP).min((size[0] - lw).max(0.0)),
                    };
                    placed[i] = [left, top];
                    let [kx, ky, kw, kh] = key_at(callouts[i].0);
                    let cy = top + lh / 2.0;
                    let (from, edge, elbow) = match side {
                        Side::Left => ([kx, ky + kh / 2.0], left + lw, board_rect[0] - GAP / 2.0),
                        _ => ([kx + kw, ky + kh / 2.0], left, board_rect[0] + board_rect[2] + GAP / 2.0),
                    };
                    lines[i] = vec![from, [elbow, from[1]], [elbow, cy], [edge, cy]];
                }
            }
        }
    }
    Placed { board: [board[0], board[1], scale], callouts: placed, lines }
}
