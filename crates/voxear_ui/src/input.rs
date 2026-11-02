//! The keyboard and focus. One element has focus at a time: clicking a
//! control focuses it, Tab and Shift+Tab move through the controls in tree
//! order, and Escape lets go. The focused element matches `:focus`. Text goes
//! to a focused text field; Enter and Space press a focused button or
//! toggle; the arrow keys move a focused slider.
//!
//! The UI never touches the system clipboard itself: it reads and writes
//! through [`Clipboard`], which the window provides.

use crate::{
    NodeId, Ui,
    controls::{Control, Step},
    selector::States,
    tree::Kind,
};

/// A key the UI acts on. Typed text arrives separately, through
/// [`Ui::text_input`], so keyboard layouts and dead keys work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Space,
    /// A letter or other key, for shortcuts such as Ctrl+C.
    Char(char),
}

/// Where copied text goes and pasted text comes from.
pub trait Clipboard {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: &str);
}

/// A clipboard inside the program, for tests and tools without a window.
#[derive(Debug, Default)]
pub struct LocalClipboard {
    text: Option<String>,
}

impl Clipboard for LocalClipboard {
    fn read(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn write(&mut self, text: &str) {
        self.text = Some(text.to_string());
    }
}

/// How long the caret is shown, then hidden, in seconds.
const BLINK: f32 = 0.53;
/// Presses this close together, on the same element, count as one double
/// or triple click.
pub(crate) const MULTI_CLICK: f32 = 0.4;

pub(crate) struct Input {
    pub focused: Option<taffy::NodeId>,
    pub clipboard: Box<dyn Clipboard>,
    /// Seconds since the UI was made, as [`Ui::advance`] reports them.
    pub time: f32,
    /// Whether anything calls [`Ui::advance`]. Without a clock, transitions
    /// could never finish, so styles change at once instead.
    pub clock: bool,
    /// When the caret last moved, so it shows straight away after.
    pub blink_from: f32,
    /// Controls whose value the user changed since the last frame.
    pub changed: Vec<taffy::NodeId>,
    /// Text fields where Enter was pressed since the last frame.
    pub submitted: Vec<taffy::NodeId>,
    /// The last press: on what, when, and how many came in a row.
    pub last_press: Option<(taffy::NodeId, f32, u32)>,
    /// A text field or slider the pointer is dragging in.
    pub dragging: Option<taffy::NodeId>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            focused: None,
            clipboard: Box::new(LocalClipboard::default()),
            time: 0.0,
            clock: false,
            blink_from: 0.0,
            changed: Vec::new(),
            submitted: Vec::new(),
            last_press: None,
            dragging: None,
        }
    }
}

impl Ui {
    /// Moves the UI's clock on by a frame's time, for transitions, the
    /// caret's blink and double clicks. Call it once a frame; until it is
    /// first called, styles change at once rather than transition.
    pub fn advance(&mut self, seconds: f32) {
        self.input.time += seconds;
        self.input.clock = true;
    }

    /// Where copied text goes and pasted text comes from.
    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.input.clipboard = clipboard;
    }

    /// Puts text on the clipboard, as a Copy button does.
    pub fn copy_text(&mut self, text: &str) {
        self.input.clipboard.write(text);
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.input.focused.map(NodeId)
    }

    /// Whether a text field has focus, so keys belong to it, not the game.
    pub fn wants_keyboard(&self) -> bool {
        self.focused().is_some_and(|node| self.kind(node) == Some(Kind::TextField))
    }

    /// Selects all of a text field's text, so typing replaces it.
    pub fn select_all_text(&mut self, field: NodeId) {
        if self.kind(field) == Some(Kind::TextField) {
            self.edit(field, |driver| driver.select_all());
        }
    }

    /// Puts a text field's caret after its last character.
    pub fn caret_to_end(&mut self, field: NodeId) {
        if self.kind(field) == Some(Kind::TextField) {
            self.edit(field, |driver| driver.move_to_text_end());
        }
    }

    /// Gives `node` focus, if it is a control that takes it. A control shown
    /// this frame takes it only once restyled, from the next frame.
    pub fn focus(&mut self, node: NodeId) {
        if !self.focusable(node.0) || self.input.focused == Some(node.0) {
            return;
        }
        self.blur();
        self.input.focused = Some(node.0);
        self.input.blink_from = self.input.time;
        self.set_state(node, States::FOCUS, true);
        // A text field shows it is focused however it got there, as it takes typing.
        if self.kind(node) == Some(Kind::TextField) {
            self.set_state(node, States::FOCUS_VISIBLE, true);
        }
    }

    pub fn blur(&mut self) {
        if let Some(node) = self.input.focused.take() {
            self.set_state(NodeId(node), States::FOCUS, false);
            self.set_state(NodeId(node), States::FOCUS_VISIBLE, false);
            self.edit(NodeId(node), |driver| driver.collapse_selection());
        }
    }

    /// Whether the user changed this control's value since the last frame:
    /// typed in it, toggled it, moved it or opened it.
    pub fn changed(&self, node: NodeId) -> bool {
        self.input.changed.contains(&node.0)
    }

    /// Whether Enter was pressed in this text field since the last frame.
    pub fn submitted(&self, node: NodeId) -> bool {
        self.input.submitted.contains(&node.0)
    }

    /// A key went down, with the modifiers last set. Returns whether the UI
    /// used it, so the game can leave it alone.
    pub fn key_down(&mut self, key: Key) -> bool {
        if key == Key::Escape && self.open_popup.is_some() {
            self.close_popup();
            return true;
        }
        if key == Key::Tab {
            return self.move_focus(!self.modifiers.shift);
        }
        let Some(focused) = self.focused() else { return false };
        let kind = self.kind(focused);
        match kind {
            Some(Kind::TextField) => {
                self.field_key(focused, key);
                true
            }
            Some(Kind::Button) if matches!(key, Key::Enter | Key::Space) => {
                self.pointer.clicks.push(focused.0);
                true
            }
            Some(Kind::Toggle | Kind::Foldout) if matches!(key, Key::Enter | Key::Space) => {
                self.press_control(focused);
                true
            }
            Some(Kind::Slider) => self.slider_key(focused, key),
            Some(Kind::Dropdown) => self.dropdown_key(focused, key),
            _ if key == Key::Escape => {
                self.blur();
                true
            }
            _ => false,
        }
    }

    /// Typed text, for the focused text field. Returns whether it was used.
    pub fn text_input(&mut self, text: &str) -> bool {
        let Some(field) = self.focused().filter(|&f| self.kind(f) == Some(Kind::TextField)) else { return false };
        let multiline = self.is_multiline(field);
        let text: String = text.chars().filter(|c| !c.is_control() || (multiline && *c == '\n')).collect();
        if !text.is_empty() {
            let ends_word = text.chars().any(|c| c.is_whitespace() || c.is_ascii_punctuation());
            self.edit_step(field, Step::Typing(ends_word), |driver| driver.insert_or_replace_selection(&text));
        }
        true
    }

    /// Text an input method is composing, shown underlined at the caret
    /// until it is committed with [`text_input`](Self::text_input). Empty
    /// text ends the composition.
    pub fn ime_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        let Some(field) = self.focused() else { return };
        if text.is_empty() {
            self.edit(field, |driver| driver.clear_compose());
        } else {
            self.edit(field, |driver| driver.set_compose(text, cursor));
        }
    }

    /// Where the caret is, for an input method to place its candidate list:
    /// left, top, width and height in logical pixels.
    pub fn ime_area(&self) -> Option<[f32; 4]> {
        let field = self.focused().filter(|&f| self.kind(f) == Some(Kind::TextField))?;
        let Some(Control::TextField(edit)) = &self.element(field)?.control else { return None };
        let area = edit.editor.ime_cursor_area();
        let scale = self.viewport.2.max(0.25);
        let [x, y, ..] = self.content_box(field);
        let left = x + (area.x0 as f32 - edit.scroll) / scale;
        let top = y + (area.y0 as f32 + edit.top) / scale;
        Some([left, top, (area.x1 - area.x0) as f32 / scale, (area.y1 - area.y0) as f32 / scale])
    }

    pub(crate) fn caret_on(&self) -> bool {
        (self.input.time - self.input.blink_from).rem_euclid(2.0 * BLINK) < BLINK
    }

    pub(crate) fn mark_changed(&mut self, node: NodeId) {
        if !self.input.changed.contains(&node.0) {
            self.input.changed.push(node.0);
        }
    }

    /// Counts presses in a row on the same element: 1, 2 for a double click,
    /// 3 for a triple.
    pub(crate) fn count_press(&mut self, node: taffy::NodeId) -> u32 {
        let time = self.input.time;
        let count = match self.input.last_press {
            Some((last, at, count)) if last == node && time - at <= MULTI_CLICK => count + 1,
            _ => 1,
        };
        self.input.last_press = Some((node, time, count));
        count
    }

    /// Controls take focus when shown and enabled, and nothing they are in
    /// is disabled.
    pub(crate) fn focusable(&self, node: taffy::NodeId) -> bool {
        let Some(element) = self.tree.get_node_context(node) else { return false };
        let control = matches!(
            element.kind,
            Kind::TextField | Kind::Button | Kind::Toggle | Kind::Slider | Kind::Foldout | Kind::Dropdown
        );
        let mut at = Some(node);
        while let Some(current) = at {
            let Some(e) = self.tree.get_node_context(current) else { return false };
            if e.computed.layout.display == taffy::Display::None || e.states.contains(States::DISABLED) {
                return false;
            }
            at = self.tree.parent(current);
        }
        control
    }

    /// Tab: the next control in tree order, wrapping round. A text field
    /// reached this way has its text selected, as in a browser.
    fn move_focus(&mut self, forward: bool) -> bool {
        let mut order = Vec::new();
        self.collect_focusable(self.root, &mut order);
        if order.is_empty() {
            return false;
        }
        let current = self.input.focused.and_then(|f| order.iter().position(|&n| n == f));
        let next = match (current, forward) {
            (Some(i), true) => (i + 1) % order.len(),
            (Some(i), false) => (i + order.len() - 1) % order.len(),
            (None, true) => 0,
            (None, false) => order.len() - 1,
        };
        let node = NodeId(order[next]);
        self.focus(node);
        self.set_state(node, States::FOCUS_VISIBLE, true);
        if self.kind(node) == Some(Kind::TextField) {
            self.edit(node, |driver| driver.select_all());
        }
        true
    }

    fn collect_focusable(&self, node: taffy::NodeId, order: &mut Vec<taffy::NodeId>) {
        if self.focusable(node) {
            order.push(node);
        }
        for child in self.tree.children(node).unwrap_or_default() {
            self.collect_focusable(child, order);
        }
    }

    pub(crate) fn clear_input_events(&mut self) {
        self.input.changed.clear();
        self.input.submitted.clear();
    }
}
