//! The game's own UI: documents its scripts load (`UiDocument.Load` in
//! Voxear.Core) or make empty and fill with elements made from code (`new
//! Button("Pause")`), in a Voxear UI of their own, so a game's sheets never
//! meet the editor's. The window draws it under its own UI, over the game's
//! view: the whole window in a player, the Game view in the editor. Scripts
//! hold documents and elements as numbers, which [`UiNumbers`] keeps; the
//! editor's Inspector keeps its own for the drawers scripts draw fields with.

use std::collections::HashMap;

use anyhow::{Context, Result};
use voxear_render::Renderer;
use voxear_scripting::{UiAnswer, UiAsk};
use voxear_ui::{ElementKind, Kind, NodeId, Style, Ui, UiFrame};

use crate::Content;

/// What a script asks of an element's class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassChange {
    Add,
    Remove,
    Toggle,
    /// Only whether it has it.
    Has,
}

/// The numbers scripts hold a UI's documents and elements by, and what they
/// stand for. Numbers start at the one given, so two UIs' never meet in
/// Voxear.Core, which keeps one `UiElement` for each number.
#[derive(Default)]
pub struct UiNumbers {
    first: u32,
    /// Each document, by its number less `first`; `None` once closed.
    documents: Vec<Option<NodeId>>,
    /// Each element scripts found or made, by its number less `first`;
    /// `None` once removed.
    elements: Vec<Option<NodeId>>,
    numbers: HashMap<NodeId, u32>,
}

impl UiNumbers {
    pub fn starting_at(first: u32) -> Self {
        Self { first, ..Self::default() }
    }

    /// Gives a document's container a number scripts hold it by.
    pub fn add_document(&mut self, root: NodeId) -> u32 {
        self.documents.push(Some(root));
        self.first + self.documents.len() as u32 - 1
    }

    /// Forgets the document, and gives back its container to take off the screen.
    pub fn close(&mut self, document: u32) -> Option<NodeId> {
        self.documents.get_mut(document.checked_sub(self.first)? as usize)?.take()
    }

    /// Forgets every document and element.
    pub fn clear(&mut self) {
        self.documents.clear();
        self.elements.clear();
        self.numbers.clear();
    }

    pub fn has_documents(&self) -> bool {
        self.documents.iter().any(Option::is_some)
    }

    /// The element named `name` in a document, or the document's container
    /// for no name, as a number scripts hold.
    pub fn find(&mut self, ui: &Ui, document: u32, name: &str) -> Option<u32> {
        let root = (*self.documents.get(document.checked_sub(self.first)? as usize)?)?;
        let node = if name.is_empty() { root } else { ui.find(root, name)? };
        Some(self.number(node))
    }

    /// The number scripts hold an element by, given it the first time.
    pub fn number(&mut self, node: NodeId) -> u32 {
        if let Some(&number) = self.numbers.get(&node) {
            return number;
        }
        self.elements.push(Some(node));
        let number = self.first + self.elements.len() as u32 - 1;
        self.numbers.insert(node, number);
        number
    }

    pub fn node(&self, element: u32) -> Option<NodeId> {
        *self.elements.get(element.checked_sub(self.first)? as usize)?
    }

    /// Makes an element of `kind` at the end of the element `parent` (see
    /// [`Ui::create`] for `text`), and gives it a number scripts hold it by.
    pub fn create(&mut self, ui: &mut Ui, parent: u32, kind: ElementKind, text: &str) -> Option<u32> {
        let node = ui.create(self.node(parent)?, kind, text);
        Some(self.number(node))
    }

    /// Takes an element out, with everything in it, whose numbers stand for
    /// nothing after. A document's own container goes only when the
    /// document closes.
    pub fn remove(&mut self, ui: &mut Ui, element: u32) {
        let Some(node) = self.node(element) else { return };
        if self.documents.contains(&Some(node)) {
            return;
        }
        let mut gone = vec![node];
        let mut at = 0;
        while let Some(&next) = gone.get(at) {
            gone.extend(ui.children(next));
            at += 1;
        }
        for node in gone {
            if let Some(number) = self.numbers.remove(&node) {
                self.elements[(number - self.first) as usize] = None;
            }
        }
        ui.remove(node);
    }

    /// Answers what a script asks of an element: see [`UiAsk`].
    pub fn ask(&self, ui: &mut Ui, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        let Some(node) = self.node(element) else { return UiAnswer::None };
        match ask {
            UiAsk::Text => return UiAnswer::Text(ui.text(node)),
            UiAsk::SetText(text) => ui.set_text(node, text),
            UiAsk::Class(class, change) => {
                let change = match change {
                    0 => ClassChange::Remove,
                    1 => ClassChange::Add,
                    2 => ClassChange::Toggle,
                    _ => ClassChange::Has,
                };
                return UiAnswer::Yes(change_class(ui, node, class, change));
            }
            UiAsk::Visible => return UiAnswer::Yes(ui.is_visible(node)),
            UiAsk::SetVisible(visible) => ui.set_visible(node, visible),
            UiAsk::Clicked => return UiAnswer::Yes(ui.clicked(node)),
            UiAsk::Changed => return UiAnswer::Yes(ui.changed(node)),
            UiAsk::Value => return UiAnswer::Number(value(ui, node)),
            UiAsk::SetValue(to) => set_value(ui, node, to),
            UiAsk::SetStyle(style) => ui.set_inline_style(node, style),
        }
        UiAnswer::None
    }
}

/// Adds, removes or toggles a class, and says whether the element has it after.
fn change_class(ui: &mut Ui, node: NodeId, class: &str, change: ClassChange) -> bool {
    match change {
        ClassChange::Add => ui.add_class(node, class),
        ClassChange::Remove => ui.remove_class(node, class),
        ClassChange::Toggle => ui.toggle_class(node, class),
        ClassChange::Has => {}
    }
    ui.has_class(node, class)
}

/// A slider's value, or a toggle's, 1 when ticked and 0 when not.
fn value(ui: &Ui, node: NodeId) -> f32 {
    match ui.kind(node) {
        Some(Kind::Slider) => ui.slider_value(node),
        Some(Kind::Dropdown) => ui.selected_option(node) as f32,
        _ => f32::from(u8::from(ui.is_checked(node))),
    }
}

fn set_value(ui: &mut Ui, node: NodeId, value: f32) {
    match ui.kind(node) {
        Some(Kind::Slider) => ui.set_slider_value(node, value),
        Some(Kind::Dropdown) => ui.set_selected_option(node, value.max(0.0) as usize),
        _ => ui.set_checked(node, value != 0.0),
    }
}

#[derive(Default)]
pub struct GameUi {
    ui: Ui,
    numbers: UiNumbers,
}

impl GameUi {
    /// Loads a document by project path, with the sheets its `<Style src>`
    /// names beside it, and returns the number scripts hold it by.
    pub fn load(&mut self, content: &mut Content, path: &str) -> Result<u32> {
        let markup = content.read_text(path).with_context(|| format!("reading {path}"))?;
        let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
        let mut load = |src: &str| {
            let sheet = if folder.is_empty() { src.to_string() } else { format!("{folder}/{src}") };
            content.read_text(&sheet).map_err(|error| format!("{sheet}: {error:#}"))
        };
        let root = self.ui.root();
        let document = self.ui.load_markup(root, path, &markup, &mut load).map_err(|e| anyhow::anyhow!("{e}"))?;
        for warning in self.ui.take_warnings() {
            tracing::warn!(target: "script", "{warning}");
        }
        Ok(self.numbers.add_document(document))
    }

    /// An empty document over the game, to fill with elements made from
    /// code, and the number scripts hold it by.
    pub fn new_document(&mut self) -> u32 {
        let root = self.ui.root();
        let document = self.ui.panel(root, Style::cover());
        self.numbers.add_document(document)
    }

    /// The element named `name` in a document, as a number scripts hold.
    pub fn find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.numbers.find(&self.ui, document, name)
    }

    /// Makes an element at the end of `parent`: see [`UiNumbers::create`].
    pub fn create(&mut self, parent: u32, kind: ElementKind, text: &str) -> Option<u32> {
        self.numbers.create(&mut self.ui, parent, kind, text)
    }

    /// Takes an element out: see [`UiNumbers::remove`].
    pub fn remove(&mut self, element: u32) {
        self.numbers.remove(&mut self.ui, element);
    }

    /// Takes a document off the screen, its elements with it.
    pub fn close(&mut self, document: u32) {
        if let Some(node) = self.numbers.close(document) {
            self.ui.remove(node);
        }
    }

    /// Answers what a script asks of an element: see [`UiAsk`].
    pub fn ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        self.numbers.ask(&mut self.ui, element, ask)
    }

    pub fn text(&self, element: u32) -> Option<String> {
        Some(self.ui.text(self.numbers.node(element)?))
    }

    pub fn set_text(&mut self, element: u32, text: &str) {
        self.ask(element, UiAsk::SetText(text));
    }

    /// Adds, removes or toggles a class, and says whether the element has it after.
    pub fn class(&mut self, element: u32, class: &str, change: ClassChange) -> bool {
        let Some(node) = self.numbers.node(element) else { return false };
        change_class(&mut self.ui, node, class, change)
    }

    pub fn visible(&self, element: u32) -> bool {
        self.numbers.node(element).is_some_and(|node| self.ui.is_visible(node))
    }

    pub fn set_visible(&mut self, element: u32, visible: bool) {
        self.ask(element, UiAsk::SetVisible(visible));
    }

    /// Whether the element was clicked since the last frame.
    pub fn clicked(&self, element: u32) -> bool {
        self.numbers.node(element).is_some_and(|node| self.ui.clicked(node))
    }

    /// A slider's value, or a toggle's, 1 when ticked and 0 when not.
    pub fn value(&self, element: u32) -> f32 {
        self.numbers.node(element).map_or(0.0, |node| value(&self.ui, node))
    }

    pub fn set_value(&mut self, element: u32, value: f32) {
        self.ask(element, UiAsk::SetValue(value));
    }

    /// Declarations that win over its sheets, as a `style` attribute's.
    pub fn set_style(&mut self, element: u32, style: &str) {
        self.ask(element, UiAsk::SetStyle(style));
    }

    /// Whether anything is showing, so a window can skip it when not.
    pub fn is_empty(&self) -> bool {
        !self.numbers.has_documents()
    }

    /// The UI, for a window to hand input to and lay out.
    pub fn ui(&mut self) -> &mut Ui {
        &mut self.ui
    }

    /// Moves the UI's clock on by a frame and lays it out for an area of
    /// `width` by `height` pixels, at `scale` pixels to a logical one.
    pub fn frame(&mut self, delta: f32, (width, height): (u32, u32), scale: f32) -> UiFrame {
        self.ui.advance(delta);
        self.ui.frame(width, height, scale)
    }

    /// Hands the renderer the game's UI for the area at `origin` in the
    /// output, or nothing when no document shows.
    pub fn draw(&mut self, renderer: &mut Renderer, delta: f32, area: [u32; 4], scale: f32) -> Result<()> {
        if self.is_empty() {
            return renderer.set_game_ui(&UiFrame::default(), [0, 0]);
        }
        let frame = self.frame(delta, (area[2], area[3]), scale);
        renderer.set_game_ui(&frame, [area[0], area[1]])
    }
}
