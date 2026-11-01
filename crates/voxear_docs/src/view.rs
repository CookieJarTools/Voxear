//! A page's Markdown drawn as Voxear UI elements, block by block, each with
//! a class for the VoxDocs sheet to style: `md-h1` to `md-h6`, `md-p`,
//! `md-list` of `md-item`s (each a `md-bullet` or a `md-task` box, then its
//! `md-item-body`), `md-quote`, `md-callout` with `md-note`, `md-tip` and
//! the rest, `md-code`, `md-table` of `md-th` and `md-td` cells, `md-rule`,
//! and `md-picture`, or `md-image` with its name until it is read.

use voxear_ui::{ImageRef, NodeId, RichText, Style, Ui};

use crate::markdown::{Align, Block, Callout, Item, Task};

/// How a page's picture shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Picture {
    /// Drawn from a texture, with the picture's size in pixels.
    Shown(ImageRef, [u32; 2]),
    /// Being read: its name shows until it is.
    Loading,
    /// Not there, or not a picture that can be read: its name shows.
    Missing,
}

/// What a page needs to know as it is drawn: whether a link goes
/// somewhere, as one that does not is drawn as broken, and how a picture
/// shows, from its address.
pub struct Lookups<'a> {
    pub works: &'a dyn Fn(&str) -> bool,
    pub pictures: &'a dyn Fn(&str) -> Picture,
}

/// What a drawn page holds that its panel answers: checklist boxes to save
/// when ticked, headings to jump to, and Copy buttons with their code.
#[derive(Default)]
pub struct Rendered {
    pub tasks: Vec<(NodeId, Task)>,
    pub headings: Vec<Heading>,
    pub copies: Vec<(NodeId, String)>,
    pub steps: Vec<Step>,
}

/// What a guide's step link starts with: an action runs a command, and a
/// check looks at the project and ticks itself when it passes.
pub const ACTION_SCHEME: &str = "action:";
pub const CHECK_SCHEME: &str = "check:";

/// A step of a guide, as drawn: a link to an action or a check alone in its
/// paragraph. The panel ticks it and says what a check still wants.
#[derive(Clone, Debug)]
pub struct Step {
    /// What it runs or looks at, as its link names it: `action:Play`.
    pub url: String,
    pub row: NodeId,
    /// Its tick.
    pub mark: NodeId,
    /// An action's button.
    pub button: Option<NodeId>,
    /// Under it, what a check found still to do.
    pub note: NodeId,
}

/// The words and link of a paragraph that is a step: one link to an action
/// or a check, and nothing else.
pub fn step_of(text: &RichText) -> Option<(&str, &str)> {
    let [span] = text.spans.as_slice() else { return None };
    let url = span.link.as_deref()?;
    let whole = span.range.start == 0 && span.range.end == text.text.len();
    let step = url.starts_with(ACTION_SCHEME) || url.starts_with(CHECK_SCHEME);
    (whole && step).then_some((text.text.as_str(), url))
}

/// The steps in some blocks, in order, as their links name them, for how
/// far a reader has got before the page is drawn.
pub fn steps_in(blocks: &[Block]) -> Vec<String> {
    let mut steps = Vec::new();
    for block in blocks {
        match block {
            Block::Paragraph(text) => steps.extend(step_of(text).map(|(_, url)| url.to_string())),
            Block::List { items, .. } => items.iter().for_each(|item| steps.extend(steps_in(&item.blocks))),
            Block::Quote { blocks, .. } => steps.extend(steps_in(blocks)),
            _ => {}
        }
    }
    steps
}

/// A heading as drawn: its element, the anchor links reach it by, its
/// words and its level, 1 to 6.
#[derive(Clone, Debug)]
pub struct Heading {
    pub node: NodeId,
    pub anchor: String,
    pub text: String,
    pub level: u8,
}

/// Draws `blocks` into `parent`.
pub fn render(ui: &mut Ui, parent: NodeId, blocks: &[Block], lookups: &Lookups) -> Rendered {
    let mut rendered = Rendered::default();
    for block in blocks {
        if let Block::Image { alt, url } = block {
            picture(ui, parent, alt, url, lookups);
            continue;
        }
        block_into(ui, parent, block, lookups.works, &mut rendered);
    }
    rendered
}

/// A picture at its own size, or narrower to fit the page, keeping its
/// shape; or, until it is read or when it cannot be, its name.
fn picture(ui: &mut Ui, parent: NodeId, alt: &str, url: &str, lookups: &Lookups) {
    let picture = (lookups.pictures)(url);
    if let Picture::Shown(image, [width, height]) = picture {
        let node = ui.image(parent);
        ui.set_image(node, Some(image));
        ui.add_class(node, "md-picture");
        ui.set_inline_style(node, &format!("width: {width}px; max-width: 100%; aspect-ratio: {width} / {height}"));
        return;
    }
    let holder = classed(ui, parent, Style::row(), "md-image");
    ui.set_class(holder, "md-loading", picture == Picture::Loading);
    ui.icon(holder, "texture");
    let mut text = RichText::new();
    text.push_link(if alt.is_empty() { url } else { alt }, url, Default::default());
    rich(ui, holder, &text, lookups.works, "md-image-name");
}

fn block_into(ui: &mut Ui, parent: NodeId, block: &Block, works: &dyn Fn(&str) -> bool, out: &mut Rendered) {
    match block {
        Block::Heading { level, text, anchor } => {
            let node = rich(ui, parent, text, works, &format!("md-h{level}"));
            ui.add_class(node, "md-h");
            out.headings.push(Heading { node, anchor: anchor.clone(), text: text.text.clone(), level: *level });
        }
        Block::Paragraph(text) => match step_of(text) {
            Some((words, url)) => out.steps.push(step(ui, parent, words, url)),
            None => drop(rich(ui, parent, text, works, "md-p")),
        },
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
        // Drawn by `render`, which knows how pictures show; one in a list or quote shows as its name.
        Block::Image { alt, url } => {
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

/// A step's row: its tick, then an action's button or a check's words, and
/// a line under it for what a check still wants.
fn step(ui: &mut Ui, parent: NodeId, words: &str, url: &str) -> Step {
    let row = classed(ui, parent, Style::column(), "md-step");
    let line = classed(ui, row, Style::row(), "md-step-line");
    let mark = ui.icon(line, "check");
    ui.add_class(mark, "md-step-mark");
    let button = if url.starts_with(ACTION_SCHEME) {
        let button = ui.button(line, words);
        ui.add_class(button, "md-step-action");
        Some(button)
    } else {
        let label = ui.add_label(line, words);
        ui.add_class(label, "md-step-check");
        None
    };
    let note = ui.add_label(row, "");
    ui.add_class(note, "md-step-note");
    ui.set_visible(note, false);
    Step { url: url.to_string(), row, mark, button, note }
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
