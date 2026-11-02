//! The project's layers in the Inspector, as Unity's Tags and Layers: each
//! of the 32 layers' names, typed in place, layer 0 always Default, and
//! under them the Layer Collision Matrix, a tick for each pair of named
//! layers whose Objs collide. It opens from Add Layer… at the foot of the
//! Inspector's Layer menu, and from the command palette. Each change is
//! written to `voxear.toml` at once (see `Project::set_layers`), and the
//! world's `Layers` follow it.
//!
//! The matrix is a triangle, as Unity's: a row for each named layer, and a
//! column for each, last first, numbered along the top, so that each pair
//! has one tick. Over a tick, its row's name and its column's number light
//! up, and the two layers' names show under the matrix.

use voxear_scene::{LAYER_COUNT, Layers};
use voxear_ui::{NodeId, Style, Ui};

use crate::inspector::prop;

/// A change made in the view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayersEdit {
    /// A layer's name, typed and let go of.
    Name(u8, String),
    /// Whether Objs on two layers collide.
    Collide(u8, u8, bool),
}

/// One tick of the matrix: the pair of layers it is for.
struct Cell {
    a: u8,
    b: u8,
    toggle: NodeId,
    /// Its row's name and its column's number.
    row: NodeId,
    column: NodeId,
}

pub(crate) struct LayersView {
    /// The name field of each layer from 1.
    names: Vec<(u8, NodeId)>,
    /// Where the matrix goes, built again when the named layers change.
    matrix: NodeId,
    cells: Vec<Cell>,
    /// What the pair under the pointer is.
    pair: NodeId,
    lit: Option<usize>,
    /// The field typed in last frame, to tell when it is let go of.
    focused: Option<NodeId>,
    layers: Layers,
}

impl LayersView {
    pub fn build(ui: &mut Ui, root: NodeId, layers: &Layers) -> Self {
        let header = ui.panel(root, Style::column());
        ui.add_class(header, "inspector-header");
        let top = ui.panel(header, Style::row());
        ui.add_class(top, "header-row");
        let tile = ui.panel(top, Style::row());
        ui.add_class(tile, "obj-tile");
        ui.icon(tile, "layers");
        let title = ui.add_label(top, "Layers");
        ui.add_class(title, "asset-name");

        let rows = part(ui, root, "Layers", "layers");
        let mut names = Vec::new();
        for layer in 0..LAYER_COUNT as u8 {
            let row = prop(ui, rows, &format!("Layer {layer}"));
            if layer == 0 {
                let fixed = ui.add_label(row, layers.name(0));
                ui.add_class(fixed, "layer-fixed");
                continue;
            }
            let field = ui.text_field(row, layers.name(layer));
            ui.add_class(field, "value-input");
            ui.set_name(field, &format!("layer-name-{layer}"));
            names.push((layer, field));
        }

        let rows = part(ui, root, "Collision Matrix", "pivot");
        let matrix = ui.panel(rows, Style::column());
        ui.add_class(matrix, "layer-matrix");
        let pair = ui.add_label(rows, "");
        ui.add_class(pair, "layer-pair");
        let mut view =
            Self { names, matrix, cells: Vec::new(), pair, lit: None, focused: None, layers: layers.clone() };
        view.build_matrix(ui);
        view
    }

    /// The matrix for the named layers: a row each, and a column each, the
    /// last first, so row `i` holds the ticks from the last layer down to
    /// its own.
    fn build_matrix(&mut self, ui: &mut Ui) {
        for child in ui.children(self.matrix) {
            ui.remove(child);
        }
        (self.cells, self.lit) = (Vec::new(), None);
        let used: Vec<(u8, String)> = self.layers.used().map(|(n, name)| (n, name.to_string())).collect();
        let heading = ui.panel(self.matrix, Style::row());
        ui.add_class(heading, "layer-matrix-row");
        let corner = ui.add_label(heading, "");
        ui.add_class(corner, "layer-matrix-name");
        let mut columns = Vec::new();
        for (number, _) in used.iter().rev() {
            let column = ui.add_label(heading, &number.to_string());
            ui.add_class(column, "layer-matrix-number");
            columns.push((*number, column));
        }
        for (place, (a, name)) in used.iter().enumerate() {
            let line = ui.panel(self.matrix, Style::row());
            ui.add_class(line, "layer-matrix-row");
            let row = ui.add_label(line, name);
            ui.add_class(row, "layer-matrix-name");
            for &(b, column) in &columns[..used.len() - place] {
                let toggle = ui.toggle(line, "", self.layers.collide(*a, b));
                for class in ["checkbox", "layer-matrix-check"] {
                    ui.add_class(toggle, class);
                }
                ui.set_name(toggle, &format!("layer-collide-{a}-{b}"));
                self.cells.push(Cell { a: *a, b, toggle, row, column });
            }
        }
        ui.set_text(self.pair, "");
    }

    /// The change made this frame, if any, and the pair under the pointer
    /// shown.
    pub fn update(&mut self, ui: &mut Ui) -> Option<LayersEdit> {
        self.light(ui);
        let focused = ui.focused();
        let left = self.focused.filter(|&field| focused != Some(field));
        self.focused = focused.filter(|focused| self.names.iter().any(|(_, field)| field == focused));
        for &(layer, field) in &self.names {
            if !(ui.submitted(field) || left == Some(field)) {
                continue;
            }
            let name = ui.text(field).trim().to_string();
            if name != self.layers.name(layer) {
                return Some(LayersEdit::Name(layer, name));
            }
        }
        let cell = self.cells.iter().find(|cell| ui.changed(cell.toggle))?;
        Some(LayersEdit::Collide(cell.a, cell.b, ui.is_checked(cell.toggle)))
    }

    /// Lights the row and column of the tick under the pointer.
    fn light(&mut self, ui: &mut Ui) {
        let hovered = self.cells.iter().position(|cell| ui.is_hovered(cell.toggle));
        if hovered == self.lit {
            return;
        }
        if let Some(cell) = self.lit.and_then(|at| self.cells.get(at)) {
            ui.set_class(cell.row, "lit", false);
            ui.set_class(cell.column, "lit", false);
        }
        self.lit = hovered;
        let text = match hovered.and_then(|at| self.cells.get(at)) {
            Some(cell) => {
                ui.set_class(cell.row, "lit", true);
                ui.set_class(cell.column, "lit", true);
                format!("{} · {}", self.layers.name(cell.a), self.layers.name(cell.b))
            }
            None => String::new(),
        };
        ui.set_text(self.pair, &text);
    }

    /// Puts a layer's name back as the project has it, as when the one
    /// typed is another layer's.
    pub fn reset_name(&mut self, ui: &mut Ui, layer: u8) {
        if let Some(&(_, field)) = self.names.iter().find(|(number, _)| *number == layer) {
            ui.set_text(field, self.layers.name(layer));
        }
    }

    /// Shows the project's layers as they are now: the names not being
    /// typed in, and the matrix, built again when the named layers changed.
    pub fn show(&mut self, ui: &mut Ui, layers: &Layers) {
        let renamed = !self.layers.used().eq(layers.used());
        self.layers = layers.clone();
        for &(layer, field) in &self.names {
            if Some(field) != ui.focused() && ui.text(field) != layers.name(layer) {
                ui.set_text(field, layers.name(layer));
            }
        }
        if renamed {
            return self.build_matrix(ui);
        }
        for cell in &self.cells {
            ui.set_checked(cell.toggle, layers.collide(cell.a, cell.b));
        }
    }
}

/// A section, headed by its icon and title, returning where its rows go.
fn part(ui: &mut Ui, root: NodeId, title: &str, icon: &str) -> NodeId {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    ui.foldout_content(foldout).expect("a foldout has content")
}
