//! Columns that are resized by dragging the lines between them: panes side
//! by side, as the Entities and Memory panels' lists are, and a table's
//! columns under a header whose edges drag, as the Memory panel's amounts
//! are. Both are split views, one inside the next, so the lines drag as the
//! dock's do.

use std::{cell::RefCell, rc::Rc};

use voxear_ui::{NodeId, Style, Ui};

/// The least a column can be dragged to, in logical pixels.
const MIN: f32 = 40.0;

/// `count` panes side by side in `parent`, each starting with an equal
/// share of its width and keeping its share as the panel resizes. Returns
/// them left to right.
pub fn panes(ui: &mut Ui, parent: NodeId, count: usize) -> Vec<NodeId> {
    let mut panes = Vec::with_capacity(count);
    let mut rest = parent;
    for left in (2..=count).rev() {
        let split = ui.split_view_fraction(rest, true, 1.0 / left as f32);
        ui.add_class(split, "fill");
        ui.add_class(split, "resizable-columns");
        ui.set_split_min(split, MIN);
        let (first, second) = ui.split_panes(split).expect("a split view has panes");
        panes.push(first);
        rest = second;
    }
    panes.push(rest);
    panes
}

/// The widths a table's cells take, shared with the rows that lay their
/// cells out by them as they are made.
pub type Widths = Rc<RefCell<Vec<f32>>>;

/// A table's header: a heading for each column, whose edges drag to resize
/// the columns. Each column keeps its share of the width as the panel
/// resizes, and the last takes what the others leave. Rows line their
/// cells up with it through [`fit`].
pub struct TableHeader {
    /// Each column's pane in the header, left to right.
    panes: Vec<NodeId>,
    widths: Widths,
}

impl TableHeader {
    /// A header in `parent` with a heading for each column, given with the
    /// share of the width it starts with, from 0 to 1; the last column's
    /// share is what is left.
    pub fn new(ui: &mut Ui, parent: NodeId, columns: &[(&str, f32)]) -> Self {
        let header = ui.panel(parent, Style::row());
        ui.add_class(header, "table-header");
        let mut panes = Vec::with_capacity(columns.len());
        let (mut rest, mut left) = (header, 1.0);
        for (index, (title, share)) in columns.iter().enumerate() {
            let pane = if index + 1 < columns.len() {
                // Each split shares out what the ones before it left.
                let split = ui.split_view_fraction(rest, true, share / left);
                left -= share;
                ui.add_class(split, "fill");
                ui.add_class(split, "resizable-columns");
                ui.set_split_min(split, MIN);
                let (first, second) = ui.split_panes(split).expect("a split view has panes");
                rest = second;
                first
            } else {
                rest
            };
            let heading = ui.add_label(pane, title);
            ui.add_class(heading, "table-heading");
            panes.push(pane);
        }
        // Measured once the header is laid out; until then, rows take what is left.
        let widths = vec![0.0; columns.len().saturating_sub(1)];
        Self { panes, widths: Rc::new(RefCell::new(widths)) }
    }

    /// The widths rows lay their cells out by.
    pub fn widths(&self) -> Widths {
        self.widths.clone()
    }

    /// Reads the columns' widths from the header as last laid out: each
    /// from its heading's left edge to the next one's, so a cell takes the
    /// line beside it too. Returns whether they changed.
    pub fn update(&mut self, ui: &Ui) -> bool {
        let lefts: Vec<f32> = self.panes.iter().map(|&pane| ui.rect(pane)[0]).collect();
        if lefts.windows(2).any(|pair| pair[1] <= pair[0]) {
            return false;
        }
        let measured: Vec<f32> = lefts.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let mut widths = self.widths.borrow_mut();
        let changed = measured.iter().zip(widths.iter()).any(|(new, old)| (new - old).abs() > 0.5);
        widths[..measured.len()].copy_from_slice(&measured);
        changed
    }
}

/// Sizes a row's cells to the columns: each but the last as wide as its
/// column, cutting off what does not fit, and the last taking the rest.
pub fn fit(ui: &mut Ui, row: NodeId, widths: &[f32]) {
    let cells = ui.children(row);
    for (index, cell) in cells.iter().enumerate() {
        let style = match widths.get(index).filter(|&&width| width > 0.0 && index + 1 < cells.len()) {
            Some(width) => format!("width: {width}px; flex-shrink: 0; flex-grow: 0"),
            None => "flex-grow: 1; flex-basis: 0px".to_string(),
        };
        ui.set_inline_style(*cell, &style);
    }
}
