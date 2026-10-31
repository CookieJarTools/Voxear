//! Context menus, and the secondary button's clicks that open them.
//!
//! A context click is a press and release of the secondary (right) button on
//! one element. On a list or tree row that is not selected, it selects the
//! row first, so a menu acts on what was clicked, as in a file manager.
//!
//! `open_menu` opens a `menu-popup` of `menu-item`s at a point, in the layer
//! popups open in, moved back inside the screen once its size is known, with
//! `menu-separator` lines between groups. A checked item matches `:checked`
//! and shows a tick (`menu-check`); a disabled one matches `:disabled` and
//! cannot be chosen. A shortcut that does the same shows at the right
//! (`menu-shortcut`). The menu closes on a choice, Escape or a press
//! elsewhere, and the choice comes back through `menu_choice`, asked by the
//! element the menu was opened for.

use taffy::{LengthPercentageAuto, Position};

use crate::{
    NodeId, Style, Ui,
    controls::Control,
    tree::{Element, Kind, Text},
};

/// How far a menu keeps from the screen's edges.
const MARGIN: f32 = 4.0;

/// One line of a menu.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
    /// Keys that do the same, shown at the right, such as "Ctrl+D".
    pub shortcut: String,
    /// A line between groups, rather than something to choose.
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: &str) -> Self {
        Self { label: label.to_string(), checked: false, enabled: true, shortcut: String::new(), separator: false }
    }

    pub fn separator() -> Self {
        Self { separator: true, ..Self::new("") }
    }

    pub fn checked(self, checked: bool) -> Self {
        Self { checked, ..self }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    pub fn shortcut(self, keys: &str) -> Self {
        Self { shortcut: keys.to_string(), ..self }
    }
}

impl Ui {
    /// Opens a menu of `items` with its top left at `at`, in logical pixels,
    /// closing any popup already open. `owner` is what the menu is for:
    /// [`menu_choice`](Self::menu_choice) asks by it, and removing it closes
    /// the menu.
    pub fn open_menu(&mut self, owner: NodeId, at: (f32, f32), items: &[MenuItem]) {
        self.close_popup();
        let layer = self.popup_layer();
        let mut style = Style::column();
        style.layout.position = Position::Absolute;
        style.layout.inset.left = LengthPercentageAuto::length(at.0);
        style.layout.inset.top = LengthPercentageAuto::length(at.1);
        let popup = self.panel(layer, style);
        self.add_class(popup, "menu-popup");
        // It lives in the top layer, so it takes its owner's text rather than the screen's.
        if let Some(text) = self.text_style(owner) {
            self.set_text_style(popup, text);
        }
        for (index, item) in items.iter().enumerate() {
            if item.separator {
                let line = self.panel(popup, Style::default());
                self.add_class(line, "menu-separator");
                continue;
            }
            let row = Element {
                control: Some(Control::MenuItem { owner: owner.0, index }),
                ..Element::new(Kind::Panel, Style::row())
            };
            let row = self.add(popup, row);
            self.add_class(row, "menu-item");
            self.set_checked(row, item.checked);
            self.set_enabled(row, item.enabled);
            let tick = self.icon(row, if item.checked { "check" } else { "" });
            self.add_class(tick, "menu-check");
            self.add(
                row,
                Element { text: Some(Text::new(&item.label)), ..Element::new(Kind::Label, Style::default()) },
            );
            if !item.shortcut.is_empty() {
                let keys =
                    Element { text: Some(Text::new(&item.shortcut)), ..Element::new(Kind::Label, Style::default()) };
                let keys = self.add(row, keys);
                self.add_class(keys, "menu-shortcut");
            }
        }
        self.open_popup = Some((owner.0, popup.0));
        self.placing_menu = Some((popup.0, at.1));
    }

    /// Whether the menu opened for `owner` is still open.
    pub fn is_menu_open(&self, owner: NodeId) -> bool {
        self.open_popup.is_some_and(|(open, _)| open == owner.0)
    }

    /// The index of the item chosen from the menu opened for `owner`, since
    /// the last frame.
    pub fn menu_choice(&self, owner: NodeId) -> Option<usize> {
        self.menu_choices.iter().find(|(open, _)| *open == owner.0).map(|&(_, index)| index)
    }

    /// An item was chosen: noted for its owner, and the menu closes.
    pub(crate) fn choose_menu_item(&mut self, owner: taffy::NodeId, index: usize) {
        self.menu_choices.push((owner, index));
        self.close_popup();
    }

    /// Moves a popup opened since the last layout back inside the screen, now
    /// its size is known: a menu or a dropdown's list goes left at the right
    /// edge, and above the pointer or the dropdown when it has no room below,
    /// or else as low as the screen lets it. Returns whether it moved, which
    /// then needs laying out again.
    pub(crate) fn place_menu(&mut self) -> bool {
        let Some((popup, above)) = self.placing_menu.take() else { return false };
        if self.open_popup.map(|(_, open)| open) != Some(popup) {
            return false;
        }
        let popup = NodeId(popup);
        let [x, y, width, height] = self.rect(popup);
        let (screen_width, screen_height) = (self.viewport.0, self.viewport.1);
        let left = x.min(screen_width - width - MARGIN).max(MARGIN);
        let top = if y + height + MARGIN <= screen_height {
            y
        } else if above - height >= MARGIN {
            above - height
        } else {
            (screen_height - height - MARGIN).max(MARGIN)
        };
        if (left, top) == (x, y) {
            return false;
        }
        let mut style = self.element(popup).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.inset.left = LengthPercentageAuto::length(left);
        style.layout.inset.top = LengthPercentageAuto::length(top);
        self.set_style(popup, style);
        true
    }
}
