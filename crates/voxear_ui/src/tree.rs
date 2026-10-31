//! The element tree: taffy nodes, each carrying its element's identity
//! (type, name, classes and states), the style it was given, the style the
//! cascade worked out, and its text.

use std::rc::Rc;

use parley::{FontContext, FontWeight, Layout, LayoutContext, StyleProperty};
use taffy::{AvailableSpace, Dimension, Display, Size, TaffyTree};

use crate::{
    Style, TextStyle, UiFrame,
    atlas::GlyphAtlas,
    draw::{Draw, measure},
    selector::States,
    sheet::{Declaration, StyleSheet, parse_inline},
};

/// An element in a [`Ui`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) taffy::NodeId);

/// What an element is, which style sheets select by type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A box that holds other elements.
    Panel,
    /// Text.
    Label,
    /// Text that can be clicked.
    Button,
    /// A panel whose children scroll.
    ScrollView,
    /// A scrolling list that only has elements for the rows on screen.
    ListView,
    /// A [`Kind::ListView`] of rows that open and close, as in a hierarchy.
    TreeView,
    /// A field to type text in.
    TextField,
    /// A box that is ticked or not, with a label.
    Toggle,
    /// A value dragged along a track.
    Slider,
    /// A titled section that opens and closes.
    Foldout,
    /// Two panes with a divider between them.
    SplitView,
    /// Pages chosen by a row of tabs.
    Tabs,
    /// A choice from a list that opens on demand.
    Dropdown,
    /// Tabbed areas of panels in split panes, rearranged by dragging tabs.
    DockSpace,
    /// One of Voxear's line icons, in its text colour; see [`crate::ICONS`].
    Icon,
    /// Part of a texture the renderer holds, such as a preview; see
    /// [`ImageRef`].
    Image,
    /// A row of choices, exactly one on.
    Segmented,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Panel => "Panel",
            Kind::Label => "Label",
            Kind::Button => "Button",
            Kind::ScrollView => "ScrollView",
            Kind::ListView => "ListView",
            Kind::TreeView => "TreeView",
            Kind::TextField => "TextField",
            Kind::Toggle => "Toggle",
            Kind::Slider => "Slider",
            Kind::Foldout => "Foldout",
            Kind::SplitView => "SplitView",
            Kind::Tabs => "Tabs",
            Kind::Dropdown => "Dropdown",
            Kind::DockSpace => "DockSpace",
            Kind::Icon => "Icon",
            Kind::Image => "Image",
            Kind::Segmented => "Segmented",
        }
    }
}

/// Part of a texture for an image to show. The UI does not hold textures:
/// `texture` is the renderer's index for one (a preview's, say), and `uv`
/// the part of it, as fractions: left, top, right, bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRef {
    pub texture: u32,
    pub uv: [f32; 4],
}

/// What each taffy node carries besides its layout.
pub(crate) struct Element {
    pub kind: Kind,
    pub name: Option<String>,
    pub classes: Vec<String>,
    pub states: States,
    /// Sheets that style this element and everything in it.
    pub sheets: Vec<Rc<StyleSheet>>,
    /// The style given in code, which sheets refine.
    pub base: Style,
    /// The text style given in code; without one, text is inherited.
    pub base_text: Option<TextStyle>,
    /// Declarations from a `style` attribute, which win over sheets.
    pub inline: Vec<Declaration>,
    /// The result of the cascade, which is what is drawn.
    pub computed: Style,
    pub text_style: TextStyle,
    pub text: Option<Text>,
    /// For an icon: which.
    pub icon: Option<&'static str>,
    /// For an image: what it shows.
    pub image: Option<ImageRef>,
    /// Lines and circles drawn over its background.
    pub shapes: Vec<crate::Shape>,
    /// How far its children are scrolled, and how far they can be.
    pub scroll: [f32; 2],
    pub scroll_max: [f32; 2],
    /// The rows and items of a list or tree view.
    pub list: Option<Box<crate::list::ListState>>,
    /// For a list's row: the list, so a click on it selects it.
    pub row_of: Option<taffy::NodeId>,
    /// What a text field, slider or foldout keeps.
    pub control: Option<crate::controls::Control>,
    /// Properties easing to new values.
    pub animations: Vec<crate::animation::Animation>,
    /// The cascade has styled it at least once, so changes from now on may animate.
    pub styled: bool,
}

impl Element {
    pub fn new(kind: Kind, base: Style) -> Self {
        Self {
            kind,
            name: None,
            classes: Vec::new(),
            states: States::default(),
            sheets: Vec::new(),
            computed: base.clone(),
            base,
            base_text: None,
            inline: Vec::new(),
            text_style: TextStyle::default(),
            text: None,
            icon: None,
            image: None,
            shapes: Vec::new(),
            scroll: [0.0; 2],
            scroll_max: [0.0; 2],
            list: None,
            row_of: None,
            control: None,
            animations: Vec::new(),
            styled: false,
        }
    }
}

pub(crate) struct Text {
    pub content: String,
    /// The style it was last shaped with.
    pub style: TextStyle,
    /// Shaped at the display scale it was last laid out at.
    pub shaped: Option<(f32, Layout<u16>)>,
    /// For rich text: how parts of it differ, and its links.
    pub spans: Vec<crate::Span>,
}

impl Text {
    pub fn new(content: &str) -> Self {
        Self { content: content.to_owned(), style: TextStyle::default(), shaped: None, spans: Vec::new() }
    }

    /// Shaped at physical size, so hinting and line breaks match the pixels
    /// it is drawn at.
    pub fn shaped(
        &mut self,
        fonts: &mut FontContext,
        layouts: &mut LayoutContext<u16>,
        scale: f32,
    ) -> &mut Layout<u16> {
        if self.shaped.as_ref().is_none_or(|(at, _)| *at != scale) {
            let mut builder = layouts.ranged_builder(fonts, &self.content, scale, true);
            builder.push_default(StyleProperty::FontSize(self.style.size));
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
            builder.push_default(StyleProperty::FontFamily(self.style.family.source().into()));
            builder.push_default(StyleProperty::LineHeight(self.style.parley_line_height()));
            crate::rich::push_spans(&mut builder, &self.spans, &self.style);
            self.shaped = Some((scale, builder.build(&self.content)));
        }
        &mut self.shaped.as_mut().expect("shaped above").1
    }
}

/// A retained tree of UI elements. Build it in code or from markup
/// ([`load_markup`](Self::load_markup)), style it with sheets, change what
/// changes, and call [`frame`](Self::frame) each frame for what to draw.
pub struct Ui {
    pub(crate) tree: TaffyTree<Element>,
    pub(crate) root: taffy::NodeId,
    pub(crate) fonts: FontContext,
    pub(crate) layouts: LayoutContext<u16>,
    pub(crate) atlas: GlyphAtlas,
    /// Logical size and display scale of the last frame.
    pub(crate) viewport: (f32, f32, f32),
    /// Something the cascade reads changed since it last ran.
    pub(crate) restyle: bool,
    pub(crate) pointer: crate::pointer::Pointer,
    pub(crate) warnings: Vec<String>,
    /// Elements that scroll, as of the last cascade.
    pub(crate) scrollers: Vec<taffy::NodeId>,
    /// List and tree views, which make rows as they scroll.
    pub(crate) lists: Vec<taffy::NodeId>,
    pub(crate) modifiers: crate::pointer::Modifiers,
    pub(crate) input: crate::input::Input,
    /// Text fields, whose layouts are refreshed before each draw.
    pub(crate) fields: Vec<taffy::NodeId>,
    /// The layer popups open in, and the open dropdown with its popup.
    pub(crate) popups: Option<taffy::NodeId>,
    pub(crate) open_popup: Option<(taffy::NodeId, taffy::NodeId)>,
    /// A dock tab pressed, and maybe being dragged.
    pub(crate) dock_drag: Option<crate::dock::DockDrag>,
    /// Menu items chosen since the last frame, by the element each menu was
    /// for, and a popup just opened, to keep on screen once laid out, with
    /// where its bottom goes when it has no room below.
    pub(crate) menu_choices: Vec<(taffy::NodeId, usize)>,
    pub(crate) placing_menu: Option<(taffy::NodeId, f32)>,
    /// Something pressed, and maybe being dragged; rows let go of in their
    /// lists and drops onto elements since the last frame; and the elements
    /// that drag and that take drops.
    pub(crate) drag: Option<crate::drag::Drag>,
    pub(crate) row_drops: Vec<crate::list::RowDrop>,
    pub(crate) element_drops: Vec<(taffy::NodeId, crate::drag::DragSource)>,
    pub(crate) draggables: std::collections::HashSet<taffy::NodeId>,
    pub(crate) drop_targets: std::collections::HashSet<taffy::NodeId>,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    pub fn new() -> Self {
        let mut tree = TaffyTree::new();
        // Snapped to physical pixels when drawn instead, which is what matters
        // at display scales other than 1.
        tree.disable_rounding();
        let root = Element {
            base: Style { pickable: false, ..Style::default() },
            ..Element::new(Kind::Panel, Style::default())
        };
        let root = tree.new_leaf_with_context(taffy::Style::default(), root).expect("a new tree has room");
        Self {
            tree,
            root,
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
            atlas: GlyphAtlas::new(),
            viewport: (0.0, 0.0, 0.0),
            restyle: true,
            pointer: Default::default(),
            warnings: Vec::new(),
            scrollers: Vec::new(),
            lists: Vec::new(),
            modifiers: Default::default(),
            input: Default::default(),
            fields: Vec::new(),
            popups: None,
            open_popup: None,
            dock_drag: None,
            menu_choices: Vec::new(),
            placing_menu: None,
            drag: None,
            row_drops: Vec::new(),
            element_drops: Vec::new(),
            draggables: Default::default(),
            drop_targets: Default::default(),
        }
        .with_theme()
    }

    /// A UI with no default theme, for styling from nothing.
    pub fn bare() -> Self {
        let mut ui = Self::new();
        ui.clear_sheets(ui.root());
        ui
    }

    fn with_theme(mut self) -> Self {
        let root = self.root();
        self.add_sheet(root, StyleSheet::parse("Default.vss", crate::DEFAULT_THEME));
        self
    }

    /// The element covering the whole screen, which everything else is in.
    pub fn root(&self) -> NodeId {
        NodeId(self.root)
    }

    /// A box that holds other elements, styled by `style` and any sheets.
    pub fn panel(&mut self, parent: NodeId, style: Style) -> NodeId {
        self.add(parent, Element::new(Kind::Panel, style))
    }

    /// A line or paragraph of text, wrapping when its parent is narrower.
    pub fn label(&mut self, parent: NodeId, text: &str, style: TextStyle) -> NodeId {
        let element = Element {
            base_text: Some(style),
            text: Some(Text::new(text)),
            ..Element::new(Kind::Label, Style::default())
        };
        self.add(parent, element)
    }

    /// A label whose text style comes from its parent and sheets, as one
    /// written in markup does.
    pub fn add_label(&mut self, parent: NodeId, text: &str) -> NodeId {
        self.add(parent, Element { text: Some(Text::new(text)), ..Element::new(Kind::Label, Style::default()) })
    }

    /// Text that can be clicked; see [`clicked`](Self::clicked). Its text
    /// style comes from sheets and its parent.
    pub fn button(&mut self, parent: NodeId, text: &str) -> NodeId {
        self.add(parent, Element { text: Some(Text::new(text)), ..Element::new(Kind::Button, Style::default()) })
    }

    /// An icon from [`crate::ICONS`], 14 pixels square unless sheets size it,
    /// drawn in its text colour. The pointer passes through it to what it is in.
    pub fn icon(&mut self, parent: NodeId, name: &str) -> NodeId {
        let element = Element {
            base: Style { pickable: false, ..Style::default() },
            ..Element::new(Kind::Icon, Style::default())
        };
        let node = self.add(parent, element);
        self.set_icon(node, name);
        node
    }

    /// Which icon an icon element shows: an empty name shows none, and an
    /// unknown one shows none with a warning.
    pub fn set_icon(&mut self, node: NodeId, name: &str) {
        let found = crate::icons::parts(name).map(|(known, _)| known);
        if found.is_none() && !name.is_empty() {
            self.warn(vec![format!("there is no icon '{name}'")]);
        }
        if let Some(element) = self.element_mut(node) {
            element.icon = found;
        }
    }

    /// An image, empty until [`set_image`](Self::set_image) gives it one,
    /// filling its box unless sheets size it. Its corners round with
    /// `border-radius`, and the pointer passes through it to what it is in.
    pub fn image(&mut self, parent: NodeId) -> NodeId {
        let element = Element {
            base: Style { pickable: false, ..Style::default() },
            ..Element::new(Kind::Image, Style::default())
        };
        self.add(parent, element)
    }

    /// What an image shows, or nothing with `None`.
    pub fn set_image(&mut self, node: NodeId, image: Option<ImageRef>) {
        if let Some(element) = self.element_mut(node) {
            element.image = image;
        }
    }

    /// An icon the pointer can press, such as a tree's or foldout's arrow.
    pub(crate) fn pressable_icon(&mut self, parent: NodeId, name: &str, style: Style) -> NodeId {
        let node = self.add(parent, Element::new(Kind::Icon, style));
        self.set_icon(node, name);
        node
    }

    /// Lines and circles to draw on an element, over its background and
    /// under its children, in logical pixels from its content box's top
    /// left: for gizmos, graphs and the like. Replaces what was there.
    pub fn set_shapes(&mut self, node: NodeId, shapes: Vec<crate::Shape>) {
        if let Some(element) = self.element_mut(node) {
            element.shapes = shapes;
        }
    }

    /// A button showing only an icon, with the class `icon-button`.
    pub fn icon_button(&mut self, parent: NodeId, icon: &str) -> NodeId {
        let node = self.add(parent, Element::new(Kind::Button, Style::row()));
        self.add_class(node, "icon-button");
        self.icon(node, icon);
        node
    }

    pub(crate) fn add(&mut self, parent: NodeId, element: Element) -> NodeId {
        let node = self.tree.new_leaf_with_context(element.base.layout.clone(), element).expect("room for a node");
        self.tree.add_child(parent.0, node).expect("the parent is in this tree");
        self.restyle = true;
        NodeId(node)
    }

    pub(crate) fn element(&self, node: NodeId) -> Option<&Element> {
        self.tree.get_node_context(node.0)
    }

    pub(crate) fn element_mut(&mut self, node: NodeId) -> Option<&mut Element> {
        self.tree.get_node_context_mut(node.0)
    }

    /// Changes an element's text. Setting the same text again costs nothing,
    /// so this can be called every frame.
    pub fn set_text(&mut self, node: NodeId, text: &str) {
        if let Some(edit) = self.editing(node) {
            if edit.editor.raw_text() != text {
                edit.editor.set_text(text);
                let _ = self.tree.mark_dirty(node.0);
            }
            return;
        }
        let Some(label) = self.element_mut(node).and_then(|e| e.text.as_mut()) else { return };
        if label.content == text && label.spans.is_empty() {
            return;
        }
        label.content.clear();
        label.content.push_str(text);
        label.spans.clear();
        label.shaped = None;
        let _ = self.tree.mark_dirty(node.0);
    }

    /// The text style a label starts from, before sheets.
    pub fn set_text_style(&mut self, node: NodeId, style: TextStyle) {
        if let Some(element) = self.element_mut(node)
            && element.base_text != Some(style)
        {
            element.base_text = Some(style);
            self.restyle = true;
        }
    }

    /// The style an element starts from, before sheets.
    pub fn set_style(&mut self, node: NodeId, style: Style) {
        if let Some(element) = self.element_mut(node)
            && element.base != style
        {
            element.base = style;
            self.restyle = true;
        }
    }

    /// Declarations that win over every sheet, written as in a `style`
    /// attribute: `"width: 120px; color: #fff"`.
    pub fn set_inline_style(&mut self, node: NodeId, declarations: &str) {
        let mut warnings = Vec::new();
        let inline = parse_inline("inline style", declarations, &mut warnings);
        self.warn(warnings);
        if let Some(element) = self.element_mut(node) {
            element.inline = inline;
            self.restyle = true;
        }
    }

    /// Styles `node` and everything in it with `sheet`. Sheets nearer an
    /// element win over those further up when rules are equally specific.
    pub fn add_sheet(&mut self, node: NodeId, sheet: StyleSheet) {
        self.warn(sheet.warnings().to_vec());
        if let Some(element) = self.element_mut(node) {
            element.sheets.push(Rc::new(sheet));
            self.restyle = true;
        }
    }

    /// Takes away every sheet `node` was given, for loading them again.
    pub fn clear_sheets(&mut self, node: NodeId) {
        if let Some(element) = self.element_mut(node) {
            element.sheets.clear();
            self.restyle = true;
        }
    }

    pub fn set_name(&mut self, node: NodeId, name: &str) {
        if let Some(element) = self.element_mut(node) {
            element.name = Some(name.to_string());
            self.restyle = true;
        }
    }

    pub fn name(&self, node: NodeId) -> Option<&str> {
        self.element(node)?.name.as_deref()
    }

    pub fn kind(&self, node: NodeId) -> Option<Kind> {
        self.element(node).map(|e| e.kind)
    }

    pub fn add_class(&mut self, node: NodeId, class: &str) {
        self.set_class(node, class, true);
    }

    pub fn remove_class(&mut self, node: NodeId, class: &str) {
        self.set_class(node, class, false);
    }

    pub fn toggle_class(&mut self, node: NodeId, class: &str) {
        let on = !self.has_class(node, class);
        self.set_class(node, class, on);
    }

    pub fn set_class(&mut self, node: NodeId, class: &str, on: bool) {
        let Some(element) = self.element_mut(node) else { return };
        let has = element.classes.iter().any(|c| c == class);
        if has == on {
            return;
        }
        if on {
            element.classes.push(class.to_string());
        } else {
            element.classes.retain(|c| c != class);
        }
        self.restyle = true;
    }

    pub fn has_class(&self, node: NodeId, class: &str) -> bool {
        self.element(node).is_some_and(|e| e.classes.iter().any(|c| c == class))
    }

    pub(crate) fn set_state(&mut self, node: NodeId, state: States, on: bool) {
        let Some(element) = self.element_mut(node) else { return };
        if element.states.contains(state) != on {
            element.states.set(state, on);
            self.restyle = true;
        }
    }

    /// A disabled element matches `:disabled`, and neither it nor anything in
    /// it can be clicked.
    pub fn set_enabled(&mut self, node: NodeId, enabled: bool) {
        self.set_state(node, States::DISABLED, !enabled);
    }

    /// For toggles and selected rows: matches `:checked`.
    pub fn set_checked(&mut self, node: NodeId, checked: bool) {
        self.set_state(node, States::CHECKED, checked);
    }

    /// Hidden elements take no space and draw nothing, nor do their children.
    /// This wins over sheets.
    pub fn set_visible(&mut self, node: NodeId, visible: bool) {
        let Some(element) = self.element_mut(node) else { return };
        let display = if visible { Display::Flex } else { Display::None };
        if element.base.layout.display != display {
            element.base.layout.display = display;
            element.inline.retain(|d| d.name != "display");
            self.restyle = true;
        }
    }

    pub fn is_visible(&self, node: NodeId) -> bool {
        self.element(node).is_some_and(|e| e.base.layout.display != Display::None)
    }

    /// The first element with this name in `node`, or `node` itself,
    /// searching depth first, like Unity's `Q`.
    pub fn find(&self, node: NodeId, name: &str) -> Option<NodeId> {
        if self.name(node) == Some(name) {
            return Some(node);
        }
        let children = self.tree.children(node.0).unwrap_or_default();
        children.into_iter().find_map(|child| self.find(NodeId(child), name))
    }

    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.tree.children(node.0).unwrap_or_default().into_iter().map(NodeId).collect()
    }

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.tree.parent(node.0).map(NodeId)
    }

    /// Whether `node` is `ancestor` or inside it.
    pub fn is_within(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut at = Some(node.0);
        while let Some(current) = at {
            if current == ancestor.0 {
                return true;
            }
            at = self.tree.parent(current);
        }
        false
    }

    /// Removes an element and everything in it.
    pub fn remove(&mut self, node: NodeId) {
        if self.open_popup.is_some_and(|(dropdown, _)| dropdown == node.0) {
            self.close_popup();
        }
        if self.open_popup.is_some_and(|(_, popup)| popup == node.0) {
            self.open_popup = None;
        }
        for child in self.tree.children(node.0).unwrap_or_default() {
            self.remove(NodeId(child));
        }
        let _ = self.tree.remove(node.0);
        self.pointer.forget(node.0);
        self.draggables.remove(&node.0);
        self.drop_targets.remove(&node.0);
        self.lists.retain(|&n| n != node.0);
        self.scrollers.retain(|&n| n != node.0);
        self.fields.retain(|&n| n != node.0);
        if self.input.focused == Some(node.0) {
            self.input.focused = None;
        }
        if self.input.dragging == Some(node.0) {
            self.input.dragging = None;
        }
        self.restyle = true;
    }

    /// An element's style as the last frame's cascade worked it out.
    pub fn style(&self, node: NodeId) -> Option<&Style> {
        self.element(node).map(|e| &e.computed)
    }

    /// Whether anything is mid-transition, so a caller that only redraws on
    /// change knows to keep drawing.
    pub fn is_animating(&self) -> bool {
        self.animating(self.root)
    }

    fn animating(&self, node: taffy::NodeId) -> bool {
        let busy = self.tree.get_node_context(node).is_some_and(|e| !e.animations.is_empty());
        busy || self.tree.children(node).unwrap_or_default().into_iter().any(|child| self.animating(child))
    }

    /// The text style an element has, or passes to the text in it, as of the
    /// last frame.
    pub fn text_style(&self, node: NodeId) -> Option<TextStyle> {
        self.element(node).map(|e| e.text_style)
    }

    /// Where an element was laid out by the last frame, as it is seen on
    /// screen (scrolled): left, top, width and height in logical pixels from
    /// the screen's top left.
    pub fn rect(&self, node: NodeId) -> [f32; 4] {
        let Ok(layout) = self.tree.layout(node.0) else { return [0.0; 4] };
        let (mut x, mut y) = (layout.location.x, layout.location.y);
        let mut at = node.0;
        while let Some(parent) = self.tree.parent(at) {
            let location = self.tree.layout(parent).map(|l| l.location).unwrap_or_default();
            let scroll = self.tree.get_node_context(parent).map_or([0.0; 2], |e| e.scroll);
            (x, y) = (x + location.x - scroll[0], y + location.y - scroll[1]);
            at = parent;
        }
        [x, y, layout.size.width, layout.size.height]
    }

    /// Problems found in sheets and markup since the last call: unknown
    /// properties, bad values and the like. Each is also logged once.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    pub(crate) fn warn(&mut self, warnings: Vec<String>) {
        for warning in warnings {
            if !self.warnings.contains(&warning) {
                tracing::warn!("{warning}");
                self.warnings.push(warning);
            }
        }
    }

    /// Restyles what changed, lays the tree out for a screen `width` by
    /// `height` physical pixels at `scale` physical pixels per logical one,
    /// and returns what to draw. Clicks seen since the last frame are
    /// forgotten after this.
    pub fn frame(&mut self, width: u32, height: u32, scale: f32) -> UiFrame {
        if std::mem::take(&mut self.restyle) {
            self.cascade();
        }
        self.layout(width, height, scale);
        self.update_scroll_limits();
        // Lists make and bind the rows now on screen, which then need laying out.
        if self.update_lists() {
            if std::mem::take(&mut self.restyle) {
                self.cascade();
            }
            self.layout(width, height, scale);
            self.update_scroll_limits();
        }
        if self.place_menu() {
            if std::mem::take(&mut self.restyle) {
                self.cascade();
            }
            self.layout(width, height, scale);
        }
        self.refresh_fields();
        let scale = self.viewport.2;
        let (focused, caret_on, time) = (self.input.focused, self.caret_on(), self.input.time);
        let mut quads = Vec::new();
        // An atlas that filled up starts again part way through, so the
        // glyphs drawn before then are drawn once more.
        for _ in 0..2 {
            quads.clear();
            self.atlas.restarted = false;
            let mut draw = Draw {
                tree: &mut self.tree,
                atlas: &mut self.atlas,
                scale,
                quads: &mut quads,
                focused,
                caret_on,
                time,
                raised: Vec::new(),
                lifting: false,
            };
            draw.all(self.root);
            if !self.atlas.restarted {
                break;
            }
        }
        self.pointer.clicks.clear();
        self.pointer.double_clicks.clear();
        self.pointer.link_clicks.clear();
        self.pointer.context_clicks.clear();
        self.menu_choices.clear();
        self.row_drops.clear();
        self.element_drops.clear();
        self.clear_selection_changes();
        self.clear_input_events();
        UiFrame { quads, atlas: self.atlas.snapshot() }
    }

    fn layout(&mut self, width: u32, height: u32, scale: f32) {
        let scale = scale.max(0.25);
        let viewport = (width as f32 / scale, height as f32 / scale, scale);
        if viewport.2 != self.viewport.2 {
            self.reshape(self.root);
        }
        self.viewport = viewport;
        // The cascade may have replaced the root's style, so its size is checked every frame.
        let size = Size { width: Dimension::length(viewport.0), height: Dimension::length(viewport.1) };
        if self.tree.style(self.root).is_ok_and(|style| style.size != size) {
            let style = taffy::Style { size, ..self.tree.style(self.root).cloned().unwrap_or_default() };
            let _ = self.tree.set_style(self.root, style);
        }
        let (fonts, layouts) = (&mut self.fonts, &mut self.layouts);
        let space = Size { width: AvailableSpace::Definite(viewport.0), height: AvailableSpace::Definite(viewport.1) };
        let measured = self.tree.compute_layout_with_measure(self.root, space, |input, _, element, style| {
            measure(input, element, style, fonts, layouts, scale)
        });
        measured.expect("the root is in this tree");
    }

    /// Text is shaped for the display scale, so a new scale shapes it again.
    fn reshape(&mut self, node: taffy::NodeId) {
        for child in self.tree.children(node).unwrap_or_default() {
            self.reshape(child);
        }
        let Some(text) = self.tree.get_node_context_mut(node).and_then(|e| e.text.as_mut()) else { return };
        text.shaped = None;
        let _ = self.tree.mark_dirty(node);
    }
}
