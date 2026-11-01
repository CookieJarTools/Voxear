//! The game's own UI: documents its scripts load (`UiDocument.Load` in
//! Voxear.Core), in a Voxear UI of their own, so a game's sheets never meet
//! the editor's. The window draws it under its own UI, over the game's
//! view: the whole window in a player, the Game view in the editor. Scripts
//! hold documents and elements as numbers, which this keeps.

use std::collections::HashMap;

use anyhow::{Context, Result};
use voxear_render::Renderer;
use voxear_ui::{Kind, NodeId, Ui, UiFrame};

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

#[derive(Default)]
pub struct GameUi {
    ui: Ui,
    /// Each document loaded, by the number scripts hold; `None` once closed.
    documents: Vec<Option<NodeId>>,
    /// Each element scripts found, by the number they hold.
    elements: Vec<NodeId>,
    numbers: HashMap<NodeId, u32>,
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
        self.documents.push(Some(document));
        Ok(self.documents.len() as u32 - 1)
    }

    /// The element named `name` in a document, as a number scripts hold.
    pub fn find(&mut self, document: u32, name: &str) -> Option<u32> {
        let root = (*self.documents.get(document as usize)?)?;
        let node = if name.is_empty() { root } else { self.ui.find(root, name)? };
        Some(self.number(node))
    }

    /// Takes a document off the screen, its elements with it.
    pub fn close(&mut self, document: u32) {
        if let Some(slot) = self.documents.get_mut(document as usize)
            && let Some(node) = slot.take()
        {
            self.ui.remove(node);
        }
    }

    pub fn text(&self, element: u32) -> Option<String> {
        Some(self.ui.text(self.node(element)?))
    }

    pub fn set_text(&mut self, element: u32, text: &str) {
        if let Some(node) = self.node(element) {
            self.ui.set_text(node, text);
        }
    }

    /// Adds, removes or toggles a class, and says whether the element has it after.
    pub fn class(&mut self, element: u32, class: &str, change: ClassChange) -> bool {
        let Some(node) = self.node(element) else { return false };
        match change {
            ClassChange::Add => self.ui.add_class(node, class),
            ClassChange::Remove => self.ui.remove_class(node, class),
            ClassChange::Toggle => self.ui.toggle_class(node, class),
            ClassChange::Has => {}
        }
        self.ui.has_class(node, class)
    }

    pub fn visible(&self, element: u32) -> bool {
        self.node(element).is_some_and(|node| self.ui.is_visible(node))
    }

    pub fn set_visible(&mut self, element: u32, visible: bool) {
        if let Some(node) = self.node(element) {
            self.ui.set_visible(node, visible);
        }
    }

    /// Whether the element was clicked since the last frame.
    pub fn clicked(&self, element: u32) -> bool {
        self.node(element).is_some_and(|node| self.ui.clicked(node))
    }

    /// A slider's value, or a toggle's, 1 when ticked and 0 when not.
    pub fn value(&self, element: u32) -> f32 {
        let Some(node) = self.node(element) else { return 0.0 };
        match self.ui.kind(node) {
            Some(Kind::Slider) => self.ui.slider_value(node),
            _ => f32::from(u8::from(self.ui.is_checked(node))),
        }
    }

    pub fn set_value(&mut self, element: u32, value: f32) {
        let Some(node) = self.node(element) else { return };
        match self.ui.kind(node) {
            Some(Kind::Slider) => self.ui.set_slider_value(node, value),
            _ => self.ui.set_checked(node, value != 0.0),
        }
    }

    /// Declarations that win over its sheets, as a `style` attribute's.
    pub fn set_style(&mut self, element: u32, style: &str) {
        if let Some(node) = self.node(element) {
            self.ui.set_inline_style(node, style);
        }
    }

    /// Whether anything is showing, so a window can skip it when not.
    pub fn is_empty(&self) -> bool {
        self.documents.iter().all(Option::is_none)
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

    fn node(&self, element: u32) -> Option<NodeId> {
        self.elements.get(element as usize).copied()
    }

    fn number(&mut self, node: NodeId) -> u32 {
        if let Some(&number) = self.numbers.get(&node) {
            return number;
        }
        self.elements.push(node);
        let number = self.elements.len() as u32 - 1;
        self.numbers.insert(node, number);
        number
    }
}
