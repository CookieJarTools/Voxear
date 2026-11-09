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
//! ([`GameUi::follow_language`]). An element given a tooltip, in markup
//! or by a script, shows it under itself once the pointer rests on it
//! ([`GameUi::show_tooltip`]).
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

/// How long the pointer rests on an element before its tooltip shows.
const TOOLTIP_DELAY: f32 = 0.5;

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

    /// A document's container, while it is open.
    pub fn root(&self, document: u32) -> Option<NodeId> {
        *self.documents.get(document.checked_sub(self.first)? as usize)?
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
        self.forget(ui, node);
        ui.remove(node);
    }

    /// Puts `document` in `new`, a container built afresh from its file, in
    /// place of the one it was in, keeping the numbers scripts hold its
    /// elements by, and so their handlers: one found by name stands for the
    /// new element of that name, or for nothing when the file has none now,
    /// and one made in code moves into the new container, under the new
    /// element of its parent's name. Gives back the old container, to take
    /// off the screen.
    pub fn replace(&mut self, ui: &mut Ui, document: u32, new: NodeId) -> Option<NodeId> {
        let slot = self.documents.get_mut(document.checked_sub(self.first)? as usize)?;
        let old = (*slot)?;
        *slot = Some(new);
        self.renumber(old, Some(new));
        let mut stack: Vec<NodeId> = ui.children(old).into_iter().rev().collect();
        while let Some(node) = stack.pop() {
            if !self.numbers.contains_key(&node) {
                stack.extend(ui.children(node).into_iter().rev());
                continue;
            }
            if let Some(name) = ui.name(node) {
                let found = ui.find(new, name);
                self.renumber(node, found);
                stack.extend(ui.children(node).into_iter().rev());
                continue;
            }
            let parent = ui.parent(node).and_then(|parent| match parent == old {
                true => Some(new),
                false => ui.find(new, ui.name(parent)?),
            });
            match parent {
                Some(parent) => ui.move_into(node, parent),
                None => self.forget(ui, node),
            }
        }
        Some(old)
    }

    /// Lets go of the numbers of `node` and everything in it, which stand
    /// for nothing after.
    fn forget(&mut self, ui: &Ui, node: NodeId) {
        let mut gone = vec![node];
        let mut at = 0;
        while let Some(&next) = gone.get(at) {
            gone.extend(ui.children(next));
            at += 1;
        }
        for node in gone {
            self.renumber(node, None);
        }
    }

    /// Has the number scripts hold `node` by stand for `to`, or for nothing.
    fn renumber(&mut self, node: NodeId, to: Option<NodeId>) {
        let Some(number) = self.numbers.remove(&node) else { return };
        self.elements[(number - self.first) as usize] = to;
        if let Some(to) = to {
            self.numbers.insert(to, number);
        }
    }

    /// Answers what a script asks of an element: see [`UiAsk`].
    pub fn ask(&self, ui: &mut Ui, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        let Some(node) = self.node(element) else { return UiAnswer::None };
        match ask {
            UiAsk::Text => return UiAnswer::Text(ui.text(text_of(ui, node))),
            // `@menu.play` shows that key's text, following the language; plain text stops following one.
            UiAsk::SetText(text) => {
                let (text, key) = voxear_ui::markup::keyed(text);
                let node = text_of(ui, node);
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
            UiAsk::SetStyleProperty(name, value) => ui.set_inline_property(node, name, value),
            UiAsk::SetTooltip(text) => ui.set_tooltip(node, text, None),
            UiAsk::SetPlaceholder(text) => ui.set_placeholder(node, text),
            UiAsk::SetOptions(options) => ui.set_options(node, &options.lines().collect::<Vec<_>>()),
            // Only the game's own UI fills lists and key maps from scripts.
            UiAsk::Items { .. } | UiAsk::Keymap { .. } | UiAsk::KeymapClicked => {}
        }
        UiAnswer::None
    }
}

/// The element whose text is an element's own: a toggle's label, a
/// foldout's title, or the element itself.
fn text_of(ui: &Ui, node: NodeId) -> NodeId {
    let title = |header: NodeId| ui.children(header).into_iter().find(|&n| ui.has_class(n, "foldout-title"));
    match ui.kind(node) {
        Some(Kind::Toggle) => ui.children(node).last().copied().unwrap_or(node),
        Some(Kind::Foldout) => ui.foldout_header(node).and_then(title).unwrap_or(node),
        _ => node,
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

/// One Voxear UI the game shows, the screen's or a panel's in the world:
/// its documents, the numbers scripts hold them by, and what it keeps for
/// its list and tree views, key maps, language and tooltip.
#[derive(Default)]
pub(crate) struct Surface {
    pub(crate) ui: Ui,
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
    /// The label tooltips show in, once one has, and the tooltip the pointer
    /// rests on, with for how long, in seconds.
    tooltip: Option<NodeId>,
    rested: Option<(String, f32)>,
    /// Each document loaded from a file, by its number, with the file's
    /// path, to load again when it changes.
    files: HashMap<u32, String>,
}

impl Surface {
    /// A surface whose numbers start at `first`, finding fonts as `fonts`
    /// does.
    fn sharing_fonts(fonts: &Ui, first: u32) -> Self {
        Self { ui: fonts.sharing_fonts(), numbers: UiNumbers::starting_at(first), ..Self::default() }
    }

    /// Loads a document by project path, with the sheets its `<Style src>`
    /// names beside it, and returns the number scripts hold it by.
    pub fn load(&mut self, content: &mut Content, path: &str) -> Result<u32> {
        let document = self.build(content, path)?;
        let number = self.numbers.add_document(document);
        self.files.insert(number, path.to_string());
        Ok(number)
    }

    /// Loads `document` again from `path`, as after its file changed. Its
    /// elements keep their numbers (see [`UiNumbers::replace`]), so the
    /// handlers scripts added to them keep running. It stays as it was when
    /// the file cannot be read.
    pub fn reload(&mut self, content: &mut Content, document: u32, path: &str) -> Result<()> {
        let container = self.build(content, path)?;
        let Some(old) = self.numbers.replace(&mut self.ui, document, container) else {
            self.ui.remove(container);
            return Ok(());
        };
        // In the old one's place among the documents, so what showed over it still does.
        if let Some(parent) = self.ui.parent(old) {
            let children = self.ui.children(parent);
            let after = children.into_iter().skip_while(|&node| node != old).skip(1).filter(|&node| node != container);
            for node in after.collect::<Vec<_>>() {
                self.ui.move_into(node, parent);
            }
        }
        self.ui.remove(old);
        Ok(())
    }

    /// Loads each document from a file again, as after files changed (see
    /// [`reload`](Self::reload)). One that cannot be read stays as it was.
    pub fn reload_files(&mut self, content: &mut Content) {
        let files: Vec<(u32, String)> = self.files.iter().map(|(&document, path)| (document, path.clone())).collect();
        for (document, path) in files {
            if let Err(error) = self.reload(content, document, &path) {
                tracing::warn!("{error:#}");
            }
        }
    }

    /// Shows or hides a whole document.
    pub(crate) fn set_document_visible(&mut self, document: u32, visible: bool) {
        if let Some(root) = self.numbers.root(document) {
            self.ui.set_visible(root, visible);
        }
    }

    /// Puts these documents over every other, the last over all, under the
    /// tooltip.
    pub(crate) fn raise(&mut self, documents: &[u32]) {
        let root = self.ui.root();
        for node in documents.iter().filter_map(|&document| self.numbers.root(document)) {
            self.ui.move_into(node, root);
        }
        if let Some(tooltip) = self.tooltip {
            self.ui.move_into(tooltip, root);
        }
    }

    /// Builds the document at `path` in a container of its own on the
    /// screen, and gives back the container.
    fn build(&mut self, content: &mut Content, path: &str) -> Result<NodeId> {
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
        Ok(document)
    }

    /// Shows each element bound to a key that key's text in the game's
    /// language: every one when the language or the text changed since the
    /// last call, or any bound since. The UI is laid out right to left when
    /// the language reads that way.
    pub fn follow_language(&mut self, localization: Option<&Localization>) {
        let Some(localization) = localization else { return };
        let added = self.ui.take_text_keys_added();
        if !added && self.language == Some(localization.version()) {
            return;
        }
        self.language = Some(localization.version());
        self.ui.set_right_to_left(voxear_localization::is_right_to_left(localization.language()));
        for (node, key) in self.ui.text_keys() {
            self.ui.set_text(node, &localization.text(&key));
        }
        for (node, key) in self.ui.tooltip_keys() {
            self.ui.set_tooltip(node, &localization.text(&key), Some(&key));
        }
        // Showing the tooltips' keys counts as binding them; they are shown already.
        self.ui.take_text_keys_added();
    }

    /// Shows the tooltip of the element the pointer has rested on for half
    /// a second, under it, and hides it as soon as the pointer moves off, as
    /// the editor does. Once a frame, `delta` seconds after the last.
    pub fn show_tooltip(&mut self, delta: f32) {
        let Some((text, over)) = self.ui.hovered_tooltip() else {
            self.rested = None;
            return self.hide_tooltip();
        };
        let rested = match self.rested.take() {
            Some((shown, time)) if shown == text => time + delta,
            _ => 0.0,
        };
        self.rested = Some((text.clone(), rested));
        if rested < TOOLTIP_DELAY {
            return self.hide_tooltip();
        }
        let root = self.ui.root();
        let tooltip = *self.tooltip.get_or_insert_with(|| {
            let tooltip = self.ui.create(root, ElementKind::Label, "");
            self.ui.add_class(tooltip, "tooltip");
            tooltip
        });
        self.ui.set_text(tooltip, &text);
        let [x, y, _, height] = self.ui.rect(over);
        self.ui.set_inline_style(tooltip, &format!("left: {x}px; top: {}px", y + height + 4.0));
        self.ui.set_visible(tooltip, true);
    }

    fn hide_tooltip(&mut self) {
        if let Some(tooltip) = self.tooltip {
            self.ui.set_visible(tooltip, false);
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
        self.files.remove(&document);
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

    /// Adds, removes or toggles a class, and says whether the element has it after.
    pub fn class(&mut self, element: u32, class: &str, change: ClassChange) -> bool {
        let Some(node) = self.numbers.node(element) else { return false };
        change_class(&mut self.ui, node, class, change)
    }

    pub fn visible(&self, element: u32) -> bool {
        self.numbers.node(element).is_some_and(|node| self.ui.is_visible(node))
    }

    /// Whether the element was clicked since the last frame.
    pub fn clicked(&self, element: u32) -> bool {
        self.numbers.node(element).is_some_and(|node| self.ui.clicked(node))
    }

    /// A slider's value, or a toggle's, 1 when ticked and 0 when not.
    pub fn value(&self, element: u32) -> f32 {
        self.numbers.node(element).map_or(0.0, |node| value(&self.ui, node))
    }

    /// Moves the UI's clock on by a frame and lays it out for an area of
    /// `width` by `height` pixels, at `scale` pixels to a logical one.
    pub fn frame(&mut self, delta: f32, (width, height): (u32, u32), scale: f32) -> UiFrame {
        self.ui.advance(delta);
        self.laid_out = Some(((width, height), scale));
        self.ui.frame(width, height, scale)
    }
}

/// The first number a UI panel's documents and elements are known by, past
/// the editor's drawers' and windows', and how many numbers each panel has.
const PANEL_NUMBERS: u32 = 1 << 27;
const PANEL_SPAN: u32 = 1 << 16;

/// The game's UI: its screen's, over the game's view, and each UI panel's
/// in the world (see [`crate::ui_panels`]). Scripts reach either through the
/// same numbers, each UI's in a range of its own.
#[derive(Default)]
pub struct GameUi {
    pub(crate) screen: Surface,
    pub(crate) panels: Vec<crate::ui_panels::PanelUi>,
    /// The Screen UIs in the world, each a document of the screen's.
    pub(crate) screens: Vec<crate::screen_ui::ScreenDoc>,
    /// The next panel's first number, less [`PANEL_NUMBERS`], in spans.
    next_panel: u32,
    /// The panel the pointer is over, and the one a press went down on,
    /// which hears its release wherever the pointer is then.
    pub(crate) hovered: Option<voxear_ecs::Entity>,
    pub(crate) pressed: Option<voxear_ecs::Entity>,
    /// The shaders backgrounds can be drawn with, which every UI it shows is
    /// given (see [`GameUi::set_shaders`]).
    shaders: std::rc::Rc<voxear_ui::UiShaders>,
}

impl GameUi {
    /// The UI that holds the document or element `number`: the screen's, or
    /// a panel's.
    fn surface(&mut self, number: u32) -> Option<&mut Surface> {
        if number < PANEL_NUMBERS {
            return Some(&mut self.screen);
        }
        let held = |first: u32| (first..first.saturating_add(PANEL_SPAN)).contains(&number);
        self.panels.iter_mut().find(|panel| held(panel.first)).map(|panel| &mut panel.surface)
    }

    fn surface_ref(&self, number: u32) -> Option<&Surface> {
        if number < PANEL_NUMBERS {
            return Some(&self.screen);
        }
        let held = |first: u32| (first..first.saturating_add(PANEL_SPAN)).contains(&number);
        self.panels.iter().find(|panel| held(panel.first)).map(|panel| &panel.surface)
    }

    /// A UI for a new panel, its numbers in a range no panel had before,
    /// finding fonts as the screen's does, and its first number.
    pub(crate) fn new_surface(&mut self) -> (Surface, u32) {
        let spans = (u32::MAX - PANEL_NUMBERS) / PANEL_SPAN;
        let first = PANEL_NUMBERS + (self.next_panel % spans) * PANEL_SPAN;
        self.next_panel += 1;
        let mut surface = Surface::sharing_fonts(&self.screen.ui, first);
        surface.ui.set_shaders(self.shaders.clone());
        // Drawn small far off and large up close, and laid out alike at either.
        surface.ui.set_scalable(true);
        (surface, first)
    }

    /// The shaders backgrounds can be drawn with: the built-in effects and
    /// the project's UI shaders, for the screen's UI and every panel's.
    pub fn set_shaders(&mut self, shaders: std::rc::Rc<voxear_ui::UiShaders>) {
        self.shaders = shaders.clone();
        for surface in self.surfaces() {
            surface.ui.set_shaders(shaders.clone());
        }
    }

    /// The shaders backgrounds can be drawn with, for a UI of the window's
    /// own, as the editor's, to draw with the same.
    pub fn shaders(&self) -> std::rc::Rc<voxear_ui::UiShaders> {
        self.shaders.clone()
    }

    /// Every UI: the screen's, then each panel's.
    fn surfaces(&mut self) -> impl Iterator<Item = &mut Surface> {
        std::iter::once(&mut self.screen).chain(self.panels.iter_mut().map(|panel| &mut panel.surface))
    }

    /// Loads a document onto the screen by project path, with the sheets
    /// its `<Style src>` names beside it, and returns the number scripts
    /// hold it by.
    pub fn load(&mut self, content: &mut Content, path: &str) -> Result<u32> {
        self.screen.load(content, path)
    }

    /// Shows each element bound to a key that key's text in the game's
    /// language, on the screen and every panel, every one when the language
    /// or its text changed.
    pub fn follow_language(&mut self, localization: Option<&Localization>) {
        for surface in self.surfaces() {
            surface.follow_language(localization);
        }
    }

    /// Shows the tooltip the pointer rests on, on the screen and every panel.
    pub fn show_tooltip(&mut self, delta: f32) {
        for surface in self.surfaces() {
            surface.show_tooltip(delta);
        }
    }

    /// An empty document over the game, to fill with elements made from
    /// code, and the number scripts hold it by.
    pub fn new_document(&mut self) -> u32 {
        self.screen.new_document()
    }

    /// The element named `name` in a document, as a number scripts hold.
    pub fn find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.surface(document)?.find(document, name)
    }

    /// Makes an element at the end of `parent`: see [`UiNumbers::create`].
    pub fn create(&mut self, parent: u32, kind: ElementKind, text: &str) -> Option<u32> {
        self.surface(parent)?.create(parent, kind, text)
    }

    /// Takes an element out: see [`UiNumbers::remove`].
    pub fn remove(&mut self, element: u32) {
        if let Some(surface) = self.surface(element) {
            surface.remove(element);
        }
    }

    /// Takes a document away, its elements with it.
    pub fn close(&mut self, document: u32) {
        if let Some(surface) = self.surface(document) {
            surface.close(document);
        }
    }

    /// Answers what a script asks of an element: see [`UiAsk`].
    pub fn ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        self.surface(element).map_or(UiAnswer::None, |surface| surface.ask(element, ask))
    }

    /// Shows each key map and prompt the game's input as it is now, on the
    /// screen and every panel.
    pub fn follow_input(&mut self, input: &Input) {
        for surface in self.surfaces() {
            surface.follow_input(input);
        }
    }

    /// The rows the list and tree views on the screen and every panel made
    /// or bound, for the scripts to make and fill, laid out as the last
    /// frame was so the frame drawn next shows them right.
    pub fn rows_to_fill(&mut self) -> Vec<RowWork> {
        self.surfaces().flat_map(Surface::rows_to_fill).collect()
    }

    pub fn text(&self, element: u32) -> Option<String> {
        self.surface_ref(element)?.text(element)
    }

    pub fn set_text(&mut self, element: u32, text: &str) {
        self.ask(element, UiAsk::SetText(text));
    }

    /// Adds, removes or toggles a class, and says whether the element has it after.
    pub fn class(&mut self, element: u32, class: &str, change: ClassChange) -> bool {
        self.surface(element).is_some_and(|surface| surface.class(element, class, change))
    }

    pub fn visible(&self, element: u32) -> bool {
        self.surface_ref(element).is_some_and(|surface| surface.visible(element))
    }

    pub fn set_visible(&mut self, element: u32, visible: bool) {
        self.ask(element, UiAsk::SetVisible(visible));
    }

    /// Whether the element was clicked since the last frame.
    pub fn clicked(&self, element: u32) -> bool {
        self.surface_ref(element).is_some_and(|surface| surface.clicked(element))
    }

    /// A slider's value, or a toggle's, 1 when ticked and 0 when not.
    pub fn value(&self, element: u32) -> f32 {
        self.surface_ref(element).map_or(0.0, |surface| surface.value(element))
    }

    pub fn set_value(&mut self, element: u32, value: f32) {
        self.ask(element, UiAsk::SetValue(value));
    }

    /// Declarations that win over its sheets, as a `style` attribute's.
    pub fn set_style(&mut self, element: u32, style: &str) {
        self.ask(element, UiAsk::SetStyle(style));
    }

    /// Whether anything shows on the screen, so a window can skip it when not.
    pub fn is_empty(&self) -> bool {
        !self.screen.numbers.has_documents()
    }

    /// The screen's UI, for a window to hand input to and lay out.
    pub fn ui(&mut self) -> &mut Ui {
        &mut self.screen.ui
    }

    /// Moves the screen's UI's clock on by a frame and lays it out for an
    /// area of `width` by `height` pixels, at `scale` pixels to a logical one.
    pub fn frame(&mut self, delta: f32, size: (u32, u32), scale: f32) -> UiFrame {
        self.screen.frame(delta, size, scale)
    }

    /// Hands the renderer the screen's UI for the area at `origin` in the
    /// output, or nothing when no document shows.
    pub fn draw(&mut self, renderer: &mut Renderer, delta: f32, area: [u32; 4], scale: f32) -> Result<()> {
        if self.is_empty() {
            return renderer.set_game_ui(&UiFrame::default(), [0, 0]);
        }
        let frame = self.frame(delta, (area[2], area[3]), scale);
        renderer.set_game_ui(&frame, [area[0], area[1]])
    }
}
