//! The game's own UI: documents its scripts load (`UiDocument.Load` in
//! Voxear.Core) or make empty and fill with elements made from code (`new
//! Button("Pause")`), in a Voxear UI of their own, so a game's sheets never
//! meet the editor's. The window draws it under its own UI, over the game's
//! view: the whole window in a player, the Game view in the editor. Scripts
//! hold documents and elements as numbers, which [`UiNumbers`] keeps; the
//! editor's Inspector keeps its own for the drawers scripts draw fields with.
//!
//! A key map (`KeymapView`) shows the keyboard and mouse or a gamepad with
//! each control's actions, as scripts ask, filled from the game's input at
//! the end of each frame ([`GameUi::follow_input`]); so does each prompt
//! (`<Prompt action="Jump" />`, or `PromptView`) with its action's controls,
//! as glyphs for the device the player used last.
//!
//! A label, button or toggle whose text is `@menu.play`, in a document or
//! set from a script, shows that key's text in the game's language, and the
//! game shows it again whenever the language or its text changes
//! ([`GameUi::follow_language`]).
//!
//! The scripts' list and tree views (`ListView` and `TreeView`) are Voxear
//! UI's own, which keep elements only for the rows on screen. What they make
//! and bind is queued, and after the scripts' updates the game lays its UI
//! out and has the scripts make and fill those rows ([`GameUi::rows_to_fill`]),
//! so they show the right items in the frame drawn next.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use anyhow::{Context, Result};
use voxear_input::{Input, Scheme};
use voxear_localization::Localization;
use voxear_render::Renderer;
use voxear_scripting::{RowWork, UiAnswer, UiAsk};
use voxear_ui::{ElementKind, Kind, ListItems, NodeId, Style, TreeItems, Ui, UiFrame};

use crate::{
    Content,
    keymaps::{self, KeymapColors, KeymapOptions},
};

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
            // `@menu.play` shows that key's text, following the language; plain text stops following one.
            UiAsk::SetText(text) => {
                let (text, key) = voxear_ui::markup::keyed(text);
                ui.set_text(node, &text);
                ui.set_text_key(node, key);
            }
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
            UiAsk::Changed => return UiAnswer::Yes(ui.changed(node) || ui.selection_changed(node)),
            UiAsk::Value => return UiAnswer::Number(value(ui, node)),
            UiAsk::SetValue(to) => set_value(ui, node, to),
            UiAsk::SetStyle(style) => ui.set_inline_style(node, style),
            // Only the game's own UI fills lists and key maps from scripts.
            UiAsk::Items { .. } | UiAsk::Keymap { .. } | UiAsk::KeymapClicked => {}
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

/// A slider's value, or a toggle's, 1 when ticked and 0 when not; a list
/// view's selected item, by index, or a tree view's, by id, -1 for none.
fn value(ui: &Ui, node: NodeId) -> f32 {
    match ui.kind(node) {
        Some(Kind::Slider) => ui.slider_value(node),
        Some(Kind::Dropdown) => ui.selected_option(node) as f32,
        Some(Kind::ListView) => ui.selection(node).first().map_or(-1.0, |&index| index as f32),
        Some(Kind::TreeView) => ui.selected_items(node).first().map_or(-1.0, |&item| item as f32),
        _ => f32::from(u8::from(ui.is_checked(node))),
    }
}

fn set_value(ui: &mut Ui, node: NodeId, value: f32) {
    match ui.kind(node) {
        Some(Kind::Slider) => ui.set_slider_value(node, value),
        Some(Kind::Dropdown) => ui.set_selected_option(node, value.max(0.0) as usize),
        Some(Kind::ListView) if value < 0.0 => ui.set_selection(node, &[]),
        Some(Kind::ListView) => ui.set_selection(node, &[value as usize]),
        Some(Kind::TreeView) if value < 0.0 => ui.set_selection(node, &[]),
        Some(Kind::TreeView) => {
            ui.select_item(node, value as u64);
        }
        _ => ui.set_checked(node, value != 0.0),
    }
}

/// A row a list or tree view made or bound since the scripts last filled
/// rows: the view, the row (a tree row's content), and the item bound.
struct RowEvent {
    list: NodeId,
    row: NodeId,
    made: bool,
    item: Option<u64>,
}

type RowQueue = Rc<RefCell<Vec<RowEvent>>>;

/// A list view the scripts fill: what it makes and binds waits for them.
struct ScriptRows {
    list: NodeId,
    queue: RowQueue,
}

impl ListItems for ScriptRows {
    fn make(&mut self, _ui: &mut Ui, row: NodeId) {
        self.queue.borrow_mut().push(RowEvent { list: self.list, row, made: true, item: None });
    }

    fn bind(&mut self, _ui: &mut Ui, row: NodeId, index: usize) {
        self.queue.borrow_mut().push(RowEvent { list: self.list, row, made: false, item: Some(index as u64) });
    }
}

/// A tree view's items as the scripts last gave them: each one's children
/// by id, in order, those at the top under `None`.
#[derive(Default)]
struct TreeShape {
    children: HashMap<Option<u64>, Vec<u64>>,
}

impl TreeShape {
    fn of(pairs: &[[u64; 2]]) -> Self {
        let mut children: HashMap<Option<u64>, Vec<u64>> = HashMap::new();
        for &[item, parent] in pairs {
            children.entry((parent != 0).then_some(parent)).or_default().push(item);
        }
        Self { children }
    }
}

/// A tree view the scripts fill, from the shape they gave it.
struct ScriptTree {
    tree: NodeId,
    shape: Rc<RefCell<TreeShape>>,
    queue: RowQueue,
}

impl TreeItems for ScriptTree {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        self.shape.borrow().children.get(&parent).cloned().unwrap_or_default()
    }

    fn has_children(&self, item: u64) -> bool {
        self.shape.borrow().children.get(&Some(item)).is_some_and(|children| !children.is_empty())
    }

    fn make(&mut self, _ui: &mut Ui, row: NodeId) {
        self.queue.borrow_mut().push(RowEvent { list: self.tree, row, made: true, item: None });
    }

    fn bind(&mut self, _ui: &mut Ui, row: NodeId, item: u64) {
        self.queue.borrow_mut().push(RowEvent { list: self.tree, row, made: false, item: Some(item) });
    }
}

#[derive(Default)]
pub struct GameUi {
    ui: Ui,
    numbers: UiNumbers,
    /// The list and tree views the scripts gave items, what they made and
    /// bound since, and each tree's items.
    lists: Vec<NodeId>,
    rows: RowQueue,
    shapes: HashMap<NodeId, Rc<RefCell<TreeShape>>>,
    /// The size and scale the last frame was laid out for, to lay the UI
    /// out again before the scripts fill rows.
    laid_out: Option<((u32, u32), f32)>,
    /// The game's text as its keys' elements show it, by
    /// [`Localization::version`].
    language: Option<u64>,
    /// The key maps scripts made, with the device each shows and how.
    keymaps: HashMap<NodeId, (Scheme, KeymapOptions)>,
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

    /// Shows each element bound to a key that key's text in the game's
    /// language: every one when the language or the text changed since the
    /// last call, or any bound since.
    pub fn follow_language(&mut self, localization: Option<&Localization>) {
        let Some(localization) = localization else { return };
        let added = self.ui.take_text_keys_added();
        if !added && self.language == Some(localization.version()) {
            return;
        }
        self.language = Some(localization.version());
        for (node, key) in self.ui.text_keys() {
            self.ui.set_text(node, &localization.text(&key));
        }
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
        match ask {
            UiAsk::Items { count, tree, item_height } => self.set_items(element, count, tree, item_height),
            UiAsk::Keymap { scheme, colors, hidden, map } => {
                let Some(node) = self.numbers.node(element).filter(|&node| self.ui.is_keymap(node)) else {
                    return UiAnswer::None;
                };
                let scheme = if scheme == 1 { Scheme::Gamepad } else { Scheme::KeyboardMouse };
                let colors = if colors == 1 { KeymapColors::Categories } else { KeymapColors::Assigned };
                let map = (!map.is_empty()).then(|| map.to_string());
                // Shown as asked from the frame's end on.
                self.keymaps.insert(node, (scheme, KeymapOptions { colors, show_hidden: hidden, map }));
            }
            UiAsk::KeymapClicked => {
                let Some(node) = self.numbers.node(element) else { return UiAnswer::None };
                let scheme = self.keymaps.get(&node).map_or(Scheme::KeyboardMouse, |(scheme, _)| *scheme);
                let Some(at) = self.ui.keymap_clicked(node) else { return UiAnswer::None };
                let layout = keymaps::layout_of(scheme);
                return layout.controls.get(at).map_or(UiAnswer::None, |c| UiAnswer::Text(c.paths[0].clone()));
            }
            ask => return self.numbers.ask(&mut self.ui, element, ask),
        }
        UiAnswer::None
    }

    /// Shows each key map scripts made the game's input as they asked, as
    /// it is now, with what was rebound, and each prompt its action's
    /// controls. Once a frame.
    pub fn follow_input(&mut self, input: &Input) {
        for (node, action) in self.ui.prompts() {
            self.ui.set_prompt(node, keymaps::prompt_glyphs(input, &action));
        }
        self.keymaps.retain(|node, _| self.ui.is_keymap(*node));
        let views: Vec<(NodeId, Scheme, KeymapOptions)> =
            self.keymaps.iter().map(|(node, (scheme, options))| (*node, *scheme, options.clone())).collect();
        for (node, scheme, options) in views {
            self.ui.set_keymap(node, keymaps::keymap_data(input, scheme, &options));
        }
    }

    /// Gives a list or tree view its items, each row `item_height` tall:
    /// how many a list has, or a tree's ids with their parents'. Every row
    /// on screen is filled again.
    fn set_items(&mut self, element: u32, count: usize, tree: Option<&[[u64; 2]]>, item_height: f32) {
        let Some(node) = self.numbers.node(element) else { return };
        let given = self.lists.contains(&node);
        match (self.ui.kind(node), tree) {
            (Some(Kind::ListView), _) if given => self.ui.set_list_count(node, count),
            (Some(Kind::ListView), _) => {
                self.ui.bind_list(node, count, ScriptRows { list: node, queue: self.rows.clone() });
            }
            (Some(Kind::TreeView), Some(pairs)) => {
                let shape = self.shapes.entry(node).or_default().clone();
                *shape.borrow_mut() = TreeShape::of(pairs);
                match given {
                    true => self.ui.refresh_list(node),
                    false => self.ui.bind_tree(node, ScriptTree { tree: node, shape, queue: self.rows.clone() }),
                }
            }
            _ => return,
        }
        if item_height > 0.0 {
            self.ui.set_item_height(node, item_height);
        }
        if !given {
            self.lists.push(node);
        }
    }

    /// Lays the UI out as the last frame was, and gives the rows its list
    /// and tree views made or bound for the scripts to make and fill, so
    /// the frame drawn next shows them right. Nothing before a first frame.
    pub fn rows_to_fill(&mut self) -> Vec<RowWork> {
        self.lists.retain(|&list| self.ui.kind(list).is_some());
        self.shapes.retain(|list, _| self.lists.contains(list));
        let Some(((width, height), scale)) = self.laid_out.filter(|_| !self.lists.is_empty()) else {
            return Vec::new();
        };
        self.ui.prepare(width, height, scale);
        let events = std::mem::take(&mut *self.rows.borrow_mut());
        let mut work: Vec<RowWork> = Vec::new();
        for event in events {
            if self.ui.kind(event.list).is_none() || self.ui.kind(event.row).is_none() {
                continue;
            }
            let (list, row) = (self.numbers.number(event.list), self.numbers.number(event.row));
            match work.iter_mut().find(|work| work.row == row) {
                Some(work) => {
                    work.made |= event.made;
                    work.item = event.item.or(work.item);
                }
                None => work.push(RowWork { list, row, made: event.made, item: event.item }),
            }
        }
        work
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
        self.laid_out = Some(((width, height), scale));
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
