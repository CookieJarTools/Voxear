//! A page's Markdown drawn as Voxear UI elements, block by block, each with
//! a class for the VoxDocs sheet to style: `md-h1` to `md-h6`, `md-p`,
//! `md-list` of `md-item`s (each a `md-bullet` or a `md-task` box, then its
//! `md-item-body`), `md-quote`, `md-callout` with `md-note`, `md-tip` and
//! the rest, `md-code`, `md-table` of `md-th` and `md-td` cells, `md-rule`
//! and `md-image`.

use voxear_ui::{NodeId, RichText, Style, Ui};

use crate::markdown::{Align, Block, Callout, Item, Task};

/// What a drawn page holds that its panel answers: checklist boxes to save
/// when ticked, headings to jump to, and Copy buttons with their code.
#[derive(Default)]
pub struct Rendered {
    pub tasks: Vec<(NodeId, Task)>,
    pub headings: Vec<(NodeId, String)>,
    pub copies: Vec<(NodeId, String)>,
}

/// Draws `blocks` into `parent`. `works` says whether a link goes
/// somewhere; one that does not is drawn as broken.
pub fn render(ui: &mut Ui, parent: NodeId, blocks: &[Block], works: &dyn Fn(&str) -> bool) -> Rendered {
    let mut rendered = Rendered::default();
    for block in blocks {
        block_into(ui, parent, block, works, &mut rendered);
    }
    rendered
}

fn block_into(ui: &mut Ui, parent: NodeId, block: &Block, works: &dyn Fn(&str) -> bool, out: &mut Rendered) {
    match block {
        Block::Heading { level, text, anchor } => {
            let node = rich(ui, parent, text, works, &format!("md-h{level}"));
            ui.add_class(node, "md-h");
            out.headings.push((node, anchor.clone()));
        }
        Block::Paragraph(text) => {
            rich(ui, parent, text, works, "md-p");
        }
        Block::List { start, items } => {
            let list = classed(ui, parent, Style::column(), "md-list");
            for (index, item) in items.iter().enumerate() {
                list_item(ui, list, item, start.map(|s| s + index as u64), works, out);
            }
        }
        Block::Quote { callout, blocks } => {
            let quote = classed(ui, parent, Style::column(), "md-quote");
            if let Some(callout) = callout {
                ui.add_class(quote, "md-callout");
                ui.add_class(quote, callout_class(*callout));
                let title = classed(ui, quote, Style::row(), "md-callout-title");
                ui.icon(title, callout_icon(*callout));
                ui.add_label(title, callout.title());
            }
            for block in blocks {
                block_into(ui, quote, block, works, out);
            }
        }
        Block::Code { language, code } => {
            let panel = classed(ui, parent, Style::column(), "md-code");
            let bar = classed(ui, panel, Style::row(), "md-code-bar");
            let name = ui.add_label(bar, if language.is_empty() { "code" } else { language });
            ui.add_class(name, "md-code-language");
            let copy = ui.icon_button(bar, "copy");
            out.copies.push((copy, code.clone()));
            let text = ui.add_label(panel, code);
            ui.add_class(text, "md-code-text");
        }
        Block::Table { align, head, rows } => {
            let table = classed(ui, parent, Style::default(), "md-table");
            ui.set_inline_style(
                table,
                &format!("grid-template-columns: repeat({}, minmax(0px, 1fr))", align.len().max(1)),
            );
            for (column, cell) in head.iter().enumerate() {
                let node = rich(ui, table, cell, works, "md-th");
                align_cell(ui, node, align[column]);
            }
            for row in rows {
                for (column, cell) in row.iter().enumerate() {
                    let node = rich(ui, table, cell, works, "md-td");
                    align_cell(ui, node, align[column]);
                }
            }
        }
        Block::Image { alt, url } => {
            // Pictures are drawn once Voxear UI shows images; until then, what and where.
            let image = classed(ui, parent, Style::row(), "md-image");
            ui.icon(image, "texture");
            let mut text = RichText::new();
            text.push_link(if alt.is_empty() { url } else { alt }, url, Default::default());
            rich(ui, image, &text, works, "md-image-name");
        }
        Block::Rule => {
            classed(ui, parent, Style::default(), "md-rule");
        }
    }
}

fn list_item(
    ui: &mut Ui,
    list: NodeId,
    item: &Item,
    number: Option<u64>,
    works: &dyn Fn(&str) -> bool,
    out: &mut Rendered,
) {
    let row = classed(ui, list, Style::row(), "md-item");
    match (item.task, number) {
        (Some(task), _) => {
            let check = ui.toggle(row, "", task.done);
            ui.add_class(check, "checkbox");
            ui.add_class(check, "md-task");
            out.tasks.push((check, task));
            if task.done {
                ui.add_class(row, "md-done");
            }
        }
        (None, Some(number)) => {
            let marker = ui.add_label(row, &format!("{number}."));
            ui.add_class(marker, "md-number");
        }
        (None, None) => {
            let marker = ui.add_label(row, "•");
            ui.add_class(marker, "md-bullet");
        }
    }
    let body = classed(ui, row, Style::column(), "md-item-body");
    for block in &item.blocks {
        block_into(ui, body, block, works, out);
    }
}

/// A rich label, its links that go nowhere drawn as broken.
fn rich(ui: &mut Ui, parent: NodeId, text: &RichText, works: &dyn Fn(&str) -> bool, class: &str) -> NodeId {
    let mut text = text.clone();
    for span in &mut text.spans {
        if span.link.as_deref().is_some_and(|url| !works(url)) {
            span.marks.broken = true;
        }
    }
    let node = ui.rich_label(parent, &text);
    ui.add_class(node, class);
    node
}

fn classed(ui: &mut Ui, parent: NodeId, style: Style, class: &str) -> NodeId {
    let node = ui.panel(parent, style);
    ui.add_class(node, class);
    node
}

fn align_cell(ui: &mut Ui, cell: NodeId, align: Align) {
    match align {
        Align::Center => ui.add_class(cell, "md-center"),
        Align::Right => ui.add_class(cell, "md-right"),
        Align::None | Align::Left => {}
    }
}

fn callout_class(callout: Callout) -> &'static str {
    match callout {
        Callout::Note => "md-note",
        Callout::Tip => "md-tip",
        Callout::Important => "md-important",
        Callout::Warning => "md-warning",
        Callout::Caution => "md-caution",
    }
}

fn callout_icon(callout: Callout) -> &'static str {
    match callout {
        Callout::Note => "info",
        Callout::Tip => "light",
        Callout::Important | Callout::Warning | Callout::Caution => "alert",
    }
}
