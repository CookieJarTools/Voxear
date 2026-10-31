//! Controls that take input: text fields, toggles, sliders and foldouts.
//! Each is made of ordinary elements with classes, so a sheet can restyle
//! every part, and the default theme (`theme/Default.vss`) gives them the
//! editor's look:
//!
//! - **TextField**: one line, or several with `multiline`. Editing is
//!   parley's: caret and word movement, selection by click, drag, double and
//!   triple click, clipboard, and input methods.
//! - **Toggle**: a `toggle-box` holding a `toggle-mark` (the default
//!   theme's switch knob) and a `toggle-check` icon (shown instead with the
//!   `checkbox` class), then a `toggle-label`. It matches `:checked` when on.
//! - **Slider**: a `slider-track` holding a `slider-fill` and a
//!   `slider-thumb`, then a `slider-value` showing the number. Dragging or
//!   the arrow keys move it.
//! - **Foldout**: a `foldout-header` (a `foldout-arrow` and a
//!   `foldout-title`) that opens and closes its `foldout-content`. It
//!   matches `:checked` when open.
//!
//! [`Ui::changed`] says when the user changed one; setting a value from
//! code does not count.

use parley::{FontWeight, PlainEditor, PlainEditorDriver, StyleProperty};
use taffy::{Dimension, LengthPercentageAuto, Position};

use crate::{
    Color, NodeId, Style, TextStyle, Ui,
    input::Key,
    selector::States,
    tree::{Element, Kind, Text},
};

pub(crate) enum Control {
    TextField(Box<Editing>),
    Slider(Slider),
    Foldout {
        arrow: taffy::NodeId,
        content: taffy::NodeId,
        header: taffy::NodeId,
    },
    Split(crate::panes::Split),
    /// A split view's divider, naming the split view.
    Divider(taffy::NodeId),
    Tabs(crate::panes::Tabs),
    /// A tab's button: its tabs, and which tab it is.
    Tab {
        tabs: taffy::NodeId,
        index: usize,
    },
    Dropdown(crate::panes::Dropdown),
    Segmented(crate::panes::Segmented),
    /// One segment's button: its segmented control, and which segment it is.
    Segment {
        segmented: taffy::NodeId,
        index: usize,
    },
    /// One choice in an open dropdown's popup.
    Option {
        dropdown: taffy::NodeId,
        index: usize,
    },
    Dock(Box<crate::dock::DockState>),
}

/// A text field's editor and how its text sits in the field.
pub(crate) struct Editing {
    pub editor: PlainEditor<()>,
    pub placeholder: Text,
    pub multiline: bool,
    /// How far the text is scrolled left to keep the caret in view, and how
    /// far down a single line sits to be centred, in physical pixels.
    pub scroll: f32,
    pub top: f32,
    synced: Option<(TextStyle, f32, Option<f32>)>,
}

impl Editing {
    fn new(text: &str) -> Self {
        let mut editor = PlainEditor::new(TextStyle::default().size);
        editor.set_text(text);
        Self { editor, placeholder: Text::new(""), multiline: false, scroll: 0.0, top: 0.0, synced: None }
    }

    /// Brings the editor's font, scale and wrapping width up to date.
    pub fn sync(&mut self, style: TextStyle, scale: f32, width: Option<f32>) {
        let width = if self.multiline { width } else { None };
        if self.synced == Some((style, scale, width)) {
            return;
        }
        let styles = self.editor.edit_styles();
        styles.insert(StyleProperty::FontSize(style.size));
        styles.insert(StyleProperty::FontWeight(FontWeight::new(style.weight)));
        styles.insert(StyleProperty::FontFamily(style.family.source().into()));
        styles.insert(StyleProperty::LineHeight(style.parley_line_height()));
        self.editor.set_scale(scale);
        self.editor.set_width(width);
        let faded = Color { a: (style.color.a as f32 * 0.45) as u8, ..style.color };
        self.placeholder.style = TextStyle { color: faded, ..style };
        self.placeholder.shaped = None;
        self.synced = Some((style, scale, width));
    }

    /// Scrolls a single line so the caret stays inside `width` pixels.
    fn keep_caret_visible(&mut self, width: f32) {
        let Some(layout) = self.editor.try_layout() else { return };
        let text_width = layout.width();
        if self.multiline || text_width <= width {
            self.scroll = 0.0;
            return;
        }
        if let Some(caret) = self.editor.cursor_geometry(1.0) {
            let x = caret.x0 as f32;
            if x - self.scroll > width - 2.0 {
                self.scroll = x - width + 2.0;
            } else if x < self.scroll {
                self.scroll = x;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (text_width - width + 2.0).max(0.0));
    }
}

pub(crate) struct Slider {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    /// What values snap to, and what an arrow key moves by; 0 for a hundredth of the range.
    pub step: f32,
    pub track: taffy::NodeId,
    pub fill: taffy::NodeId,
    pub thumb: taffy::NodeId,
    pub label: taffy::NodeId,
}

impl Slider {
    fn step(&self) -> f32 {
        if self.step > 0.0 { self.step } else { (self.max - self.min) / 100.0 }
    }

    fn fraction(&self) -> f32 {
        if self.max > self.min { ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// The value as shown: as many decimals as the step has, or two.
    fn shown(&self) -> String {
        let decimals = if self.step > 0.0 {
            (0..4).find(|&d| (self.step * 10f32.powi(d)).fract().abs() < 1e-3).unwrap_or(3) as usize
        } else {
            2
        };
        format!("{:.*}", decimals, self.value)
    }

    fn clamp(&self, value: f32) -> f32 {
        let value =
            if self.step > 0.0 { self.min + ((value - self.min) / self.step).round() * self.step } else { value };
        value.clamp(self.min.min(self.max), self.max.max(self.min))
    }
}

impl Ui {
    /// A field to type text in.
    pub fn text_field(&mut self, parent: NodeId, text: &str) -> NodeId {
        let element = Element {
            control: Some(Control::TextField(Box::new(Editing::new(text)))),
            ..Element::new(Kind::TextField, Style::default())
        };
        let field = self.add(parent, element);
        self.fields.push(field.0);
        field
    }

    /// Faded text a field shows while it is empty.
    pub fn set_placeholder(&mut self, field: NodeId, text: &str) {
        if let Some(edit) = self.editing(field) {
            edit.placeholder.content = text.to_string();
            edit.placeholder.shaped = None;
        }
    }

    /// Lets a field wrap and take several lines, with Enter starting a new one.
    pub fn set_multiline(&mut self, field: NodeId, multiline: bool) {
        if let Some(edit) = self.editing(field) {
            edit.multiline = multiline;
            edit.synced = None;
        }
        let _ = self.tree.mark_dirty(field.0);
    }

    pub(crate) fn is_multiline(&self, field: NodeId) -> bool {
        matches!(self.element(field).and_then(|e| e.control.as_ref()), Some(Control::TextField(edit)) if edit.multiline)
    }

    /// A box that is ticked or not, with a label beside it.
    pub fn toggle(&mut self, parent: NodeId, text: &str, checked: bool) -> NodeId {
        let toggle = self.add(parent, Element::new(Kind::Toggle, Style::row()));
        let mut box_style = Style::row();
        box_style.layout.justify_content = taffy::JustifyContent::CENTER;
        box_style.layout.flex_shrink = 0.0;
        let check_box = self.panel(toggle, box_style);
        self.add_class(check_box, "toggle-box");
        let mark = self.panel(check_box, Style { pickable: false, ..Style::default() });
        self.add_class(mark, "toggle-mark");
        let check = self.icon(check_box, "check");
        self.add_class(check, "toggle-check");
        let label =
            self.add(toggle, Element { text: Some(Text::new(text)), ..Element::new(Kind::Label, Style::default()) });
        self.add_class(label, "toggle-label");
        self.set_visible(label, !text.is_empty());
        self.set_checked(toggle, checked);
        toggle
    }

    /// A value from `min` to `max`, dragged along a track.
    pub fn slider(&mut self, parent: NodeId, min: f32, max: f32, value: f32) -> NodeId {
        let slider = self.add(parent, Element::new(Kind::Slider, Style::row()));
        let track = self.panel(slider, Style::default().grow());
        self.add_class(track, "slider-track");
        let mut fill = Style { pickable: false, ..Style::default() };
        fill.layout.position = Position::Absolute;
        fill.layout.inset.left = LengthPercentageAuto::length(0.0);
        fill.layout.inset.top = LengthPercentageAuto::length(0.0);
        fill.layout.inset.bottom = LengthPercentageAuto::length(0.0);
        let fill = self.panel(track, fill);
        self.add_class(fill, "slider-fill");
        let mut thumb = Style { pickable: false, ..Style::default() };
        thumb.layout.position = Position::Absolute;
        let thumb = self.panel(track, thumb);
        self.add_class(thumb, "slider-thumb");
        let label = self.add_label(slider, "");
        self.add_class(label, "slider-value");
        let state =
            Slider { min, max, value: min, step: 0.0, track: track.0, fill: fill.0, thumb: thumb.0, label: label.0 };
        if let Some(element) = self.element_mut(slider) {
            element.control = Some(Control::Slider(state));
        }
        self.set_slider_value(slider, value);
        slider
    }

    pub fn slider_value(&self, slider: NodeId) -> f32 {
        match self.element(slider).and_then(|e| e.control.as_ref()) {
            Some(Control::Slider(state)) => state.value,
            _ => 0.0,
        }
    }

    /// Moves a slider, within its range and to its step.
    pub fn set_slider_value(&mut self, slider: NodeId, value: f32) {
        let Some(Control::Slider(state)) = self.element_mut(slider).and_then(|e| e.control.as_mut()) else { return };
        state.value = state.clamp(value);
        let (fraction, fill, thumb, label, shown) =
            (state.fraction(), state.fill, state.thumb, state.label, state.shown());
        self.set_text(NodeId(label), &shown);
        let mut style = self.element(NodeId(fill)).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.size.width = Dimension::percent(fraction);
        self.set_style(NodeId(fill), style);
        let mut style = self.element(NodeId(thumb)).map(|e| e.base.clone()).unwrap_or_default();
        style.layout.inset.left = LengthPercentageAuto::percent(fraction);
        self.set_style(NodeId(thumb), style);
    }

    /// Values snap to multiples of `step` from the minimum; 0 lets them be anything.
    pub fn set_slider_step(&mut self, slider: NodeId, step: f32) {
        if let Some(Control::Slider(state)) = self.element_mut(slider).and_then(|e| e.control.as_mut()) {
            state.step = step.max(0.0);
        }
        let value = self.slider_value(slider);
        self.set_slider_value(slider, value);
    }

    /// A titled section that opens and closes. Put what it holds in
    /// [`foldout_content`](Self::foldout_content).
    pub fn foldout(&mut self, parent: NodeId, title: &str, open: bool) -> NodeId {
        let foldout = self.add(parent, Element::new(Kind::Foldout, Style::column()));
        let header = self.panel(foldout, Style::row());
        self.add_class(header, "foldout-header");
        let arrow = self.icon(header, "chevron-right");
        self.add_class(arrow, "foldout-arrow");
        let label =
            self.add(header, Element { text: Some(Text::new(title)), ..Element::new(Kind::Label, Style::default()) });
        self.add_class(label, "foldout-title");
        let content = self.panel(foldout, Style::column());
        self.add_class(content, "foldout-content");
        if let Some(element) = self.element_mut(foldout) {
            element.control = Some(Control::Foldout { arrow: arrow.0, content: content.0, header: header.0 });
        }
        self.set_open(foldout, open);
        foldout
    }

    /// Where a foldout's contents go.
    pub fn foldout_content(&self, foldout: NodeId) -> Option<NodeId> {
        match self.element(foldout)?.control.as_ref()? {
            Control::Foldout { content, .. } => Some(NodeId(*content)),
            _ => None,
        }
    }

    pub fn is_open(&self, foldout: NodeId) -> bool {
        self.kind(foldout) == Some(Kind::Foldout) && self.is_checked(foldout)
    }

    pub fn set_open(&mut self, foldout: NodeId, open: bool) {
        let Some(Control::Foldout { arrow, content, .. }) = self.element(foldout).and_then(|e| e.control.as_ref())
        else {
            return;
        };
        let (arrow, content) = (NodeId(*arrow), NodeId(*content));
        self.set_checked(foldout, open);
        self.set_visible(content, open);
        self.set_icon(arrow, if open { "chevron-down" } else { "chevron-right" });
    }

    /// Whether a toggle is on or a foldout open, or anything else matches `:checked`.
    pub fn is_checked(&self, node: NodeId) -> bool {
        self.element(node).is_some_and(|e| e.states.contains(States::CHECKED))
    }

    /// A text field's text, or a label's or button's.
    pub fn text(&self, node: NodeId) -> String {
        let Some(element) = self.element(node) else { return String::new() };
        match (&element.control, &element.text) {
            (Some(Control::TextField(edit)), _) => edit.editor.raw_text().to_string(),
            (_, Some(text)) => text.content.clone(),
            _ => String::new(),
        }
    }

    pub(crate) fn editing(&mut self, field: NodeId) -> Option<&mut Editing> {
        match self.element_mut(field)?.control.as_mut()? {
            Control::TextField(edit) => Some(edit),
            _ => None,
        }
    }

    /// Runs editor commands on a text field, then notes a change, keeps the
    /// caret in view and restarts its blink.
    pub(crate) fn edit(&mut self, field: NodeId, command: impl FnOnce(&mut PlainEditorDriver<'_, ()>)) {
        let scale = self.viewport.2.max(0.25);
        let [_, _, width, _] = self.content_box(field);
        let Ui { tree, fonts, layouts, .. } = self;
        let Some(element) = tree.get_node_context_mut(field.0) else { return };
        let style = element.text_style;
        let Some(Control::TextField(edit)) = &mut element.control else { return };
        edit.sync(style, scale, Some(width * scale));
        let before = edit.editor.raw_text().to_string();
        let mut driver = edit.editor.driver(fonts, layouts);
        command(&mut driver);
        driver.refresh_layout();
        let changed = edit.editor.raw_text() != before;
        edit.keep_caret_visible(width * scale);
        self.input.blink_from = self.input.time;
        if changed {
            self.mark_changed(field);
            let _ = self.tree.mark_dirty(field.0);
        }
    }

    pub(crate) fn field_key(&mut self, field: NodeId, key: Key) {
        let (shift, word) = (self.modifiers.shift, self.modifiers.control);
        match key {
            Key::Left => self.edit(field, |d| match (shift, word) {
                (false, false) => d.move_left(),
                (true, false) => d.select_left(),
                (false, true) => d.move_word_left(),
                (true, true) => d.select_word_left(),
            }),
            Key::Right => self.edit(field, |d| match (shift, word) {
                (false, false) => d.move_right(),
                (true, false) => d.select_right(),
                (false, true) => d.move_word_right(),
                (true, true) => d.select_word_right(),
            }),
            Key::Up => self.edit(field, |d| if shift { d.select_up() } else { d.move_up() }),
            Key::Down => self.edit(field, |d| if shift { d.select_down() } else { d.move_down() }),
            Key::Home => self.edit(field, |d| if shift { d.select_to_line_start() } else { d.move_to_line_start() }),
            Key::End => self.edit(field, |d| if shift { d.select_to_line_end() } else { d.move_to_line_end() }),
            Key::Backspace => self.edit(field, |d| if word { d.backdelete_word() } else { d.backdelete() }),
            Key::Delete => self.edit(field, |d| if word { d.delete_word() } else { d.delete() }),
            Key::Enter if self.is_multiline(field) && !word => {
                self.edit(field, |d| d.insert_or_replace_selection("\n"))
            }
            Key::Enter => {
                if !self.input.submitted.contains(&field.0) {
                    self.input.submitted.push(field.0);
                }
            }
            Key::Escape => self.blur(),
            Key::Char('a') if word => self.edit(field, |d| d.select_all()),
            Key::Char(c @ ('c' | 'x')) if word => {
                let selected = self.editing(field).and_then(|e| e.editor.selected_text().map(str::to_string));
                if let Some(selected) = selected {
                    self.input.clipboard.write(&selected);
                    if c == 'x' {
                        self.edit(field, |d| d.delete_selection());
                    }
                }
            }
            Key::Char('v') if word => {
                if let Some(text) = self.input.clipboard.read() {
                    self.text_input(&text);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn slider_key(&mut self, slider: NodeId, key: Key) -> bool {
        let Some(Control::Slider(state)) = self.element(slider).and_then(|e| e.control.as_ref()) else { return false };
        let (value, step, min, max) = (state.value, state.step(), state.min, state.max);
        let target = match key {
            Key::Left | Key::Down => value - step,
            Key::Right | Key::Up => value + step,
            Key::PageDown => value - step * 10.0,
            Key::PageUp => value + step * 10.0,
            Key::Home => min,
            Key::End => max,
            Key::Escape => {
                self.blur();
                return true;
            }
            _ => return false,
        };
        self.set_slider_value(slider, target);
        if self.slider_value(slider) != value {
            self.mark_changed(slider);
        }
        true
    }

    /// What a click or Enter does to a toggle or foldout.
    pub(crate) fn press_control(&mut self, node: NodeId) {
        match self.kind(node) {
            Some(Kind::Toggle) => {
                let on = !self.is_checked(node);
                self.set_checked(node, on);
                self.mark_changed(node);
            }
            Some(Kind::Foldout) => {
                let open = !self.is_open(node);
                self.set_open(node, open);
                self.mark_changed(node);
            }
            _ => {}
        }
    }

    /// The press at `point` focuses the control it is on, if any, places a
    /// text field's caret, or starts moving a slider.
    pub(crate) fn press_controls(&mut self, path: &[taffy::NodeId], point: (f32, f32)) {
        let focus = path.iter().rev().copied().find(|&n| self.focusable(n));
        let Some(node) = focus.map(NodeId) else {
            self.blur();
            return;
        };
        self.focus(node);
        match self.kind(node) {
            Some(Kind::TextField) => {
                let count = self.count_press(node.0);
                let (x, y) = self.field_point(node, point);
                let shift = self.modifiers.shift;
                self.edit(node, |d| match count {
                    1 if shift => d.shift_click_extension(x, y),
                    1 => d.move_to_point(x, y),
                    2 => d.select_word_at_point(x, y),
                    _ => d.select_line_at_point(x, y),
                });
                self.input.dragging = Some(node.0);
            }
            Some(Kind::Slider) => {
                self.input.dragging = Some(node.0);
                self.slide_to(node, point.0);
            }
            _ => {}
        }
    }

    /// The pointer moved while pressed on a text field or slider.
    pub(crate) fn drag_controls(&mut self, point: (f32, f32)) {
        let Some(node) = self.input.dragging.map(NodeId) else { return };
        match self.kind(node) {
            Some(Kind::TextField) => {
                let (x, y) = self.field_point(node, point);
                self.edit(node, |d| d.extend_selection_to_point(x, y));
            }
            Some(Kind::Slider) => self.slide_to(node, point.0),
            Some(Kind::SplitView) => self.drag_divider(node, point),
            _ => {}
        }
    }

    /// A click toggles the toggle it is in, or the foldout whose header it is on.
    pub(crate) fn click_controls(&mut self, target: taffy::NodeId) {
        let mut at = Some(target);
        while let Some(node) = at {
            match self.tree.get_node_context(node).map(|e| (e.kind, &e.control)) {
                Some((Kind::Toggle, _)) => return self.press_control(NodeId(node)),
                Some((Kind::Foldout, Some(Control::Foldout { header, .. }))) => {
                    if self.is_within(NodeId(target), NodeId(*header)) {
                        self.press_control(NodeId(node));
                    }
                    return;
                }
                _ => {}
            }
            at = self.tree.parent(node);
        }
    }

    fn slide_to(&mut self, slider: NodeId, x: f32) {
        let Some(Control::Slider(state)) = self.element(slider).and_then(|e| e.control.as_ref()) else { return };
        let (min, max, value, track) = (state.min, state.max, state.value, NodeId(state.track));
        let [left, _, width, _] = self.rect(track);
        let fraction = if width > 0.0 { ((x - left) / width).clamp(0.0, 1.0) } else { 0.0 };
        self.set_slider_value(slider, min + fraction * (max - min));
        if self.slider_value(slider) != value {
            self.mark_changed(slider);
        }
    }

    /// A point on screen as a place in a text field's layout, in physical pixels.
    fn field_point(&self, field: NodeId, point: (f32, f32)) -> (f32, f32) {
        let scale = self.viewport.2.max(0.25);
        let [x, y, ..] = self.content_box(field);
        let (scroll, top) = match self.element(field).and_then(|e| e.control.as_ref()) {
            Some(Control::TextField(edit)) => (edit.scroll, edit.top),
            _ => (0.0, 0.0),
        };
        ((point.0 - x) * scale + scroll, (point.1 - y) * scale - top)
    }

    /// An element's content box on screen, inside its border and padding:
    /// left, top, width and height in logical pixels.
    pub(crate) fn content_box(&self, node: NodeId) -> [f32; 4] {
        let [x, y, w, h] = self.rect(node);
        let Ok(layout) = self.tree.layout(node.0) else { return [x, y, w, h] };
        let (left, top) = (layout.border.left + layout.padding.left, layout.border.top + layout.padding.top);
        let (right, bottom) =
            (layout.border.right + layout.padding.right, layout.border.bottom + layout.padding.bottom);
        [x + left, y + top, (w - left - right).max(0.0), (h - top - bottom).max(0.0)]
    }

    /// Before drawing: every text field's layout up to date, its single line
    /// centred, and its placeholder shaped.
    pub(crate) fn refresh_fields(&mut self) {
        let scale = self.viewport.2.max(0.25);
        for field in self.fields.clone() {
            let [_, _, width, height] = self.content_box(NodeId(field));
            let Ui { tree, fonts, layouts, .. } = self;
            let Some(element) = tree.get_node_context_mut(field) else { continue };
            let style = element.text_style;
            let Some(Control::TextField(edit)) = &mut element.control else { continue };
            edit.sync(style, scale, Some(width * scale));
            let text_height = edit.editor.layout(fonts, layouts).height();
            edit.top = if edit.multiline { 0.0 } else { ((height * scale - text_height) / 2.0).max(0.0).floor() };
            edit.keep_caret_visible(width * scale);
            if !edit.placeholder.content.is_empty() {
                let layout = edit.placeholder.shaped(fonts, layouts, scale);
                layout.break_all_lines(None);
            }
        }
    }
}
