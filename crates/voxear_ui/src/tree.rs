//! The element tree: taffy nodes, each carrying its element's identity
//! (type, name, classes and states), the style it was given, the style the
//! cascade worked out, and its text.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use parley::{FontContext, FontWeight, Layout, LayoutContext, StyleProperty};
use taffy::{AvailableSpace, Dimension, Display, Size, TaffyTree};

use crate::{
    Style, TextStyle, UiFrame,
    atlas::GlyphAtlas,
    draw::{Draw, measure},
    selector::{Compound, States},
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

/// A fill from `left` to `right`, and, given `bottom`, down to it as well,
/// as a colour picker's square runs from white to a hue and down to black.
/// The colours blend in sRGB, as people see them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub left: crate::Color,
    pub right: crate::Color,
    pub bottom: Option<crate::Color>,
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
    /// A fill over its background; see [`Gradient`].
    pub gradient: Option<Gradient>,
    /// Lines and circles drawn over its background.
    pub shapes: Vec<crate::Shape>,
    /// A window onto what was drawn under the UI; see [`Ui::set_see_through`].
    pub see_through: bool,
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
    /// The keyframes its `animation` plays, and since when.
    pub keyframes: Option<crate::animation::Playing>,
    /// The cascade has styled it at least once, so changes from now on may animate.
    pub styled: bool,
    /// The custom properties in effect, its own and inherited, for its children.
    pub vars: Rc<HashMap<String, String>>,
    /// How far it is drawn from where it is laid out, with everything in
    /// it, as an item sliding in a reorderable list is.
    pub translate: [f32; 2],
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
            gradient: None,
            shapes: Vec::new(),
            see_through: false,
            scroll: [0.0; 2],
            scroll_max: [0.0; 2],
            list: None,
            row_of: None,
            control: None,
            animations: Vec::new(),
            keyframes: None,
            styled: false,
            vars: Rc::default(),
            translate: [0.0; 2],
        }
    }
}

pub(crate) struct Text {
    pub content: String,
    /// The style it was last shaped with.
    pub style: TextStyle,
    /// Shaped at the display scale it was last laid out at, its lines
    /// rounded to whole pixels or not (see [`Ui::set_scalable`]).
    pub shaped: Option<((f32, bool), Layout<u16>)>,
    /// For rich text: how parts of it differ, and its links.
    pub spans: Vec<crate::Span>,
}

impl Text {
    pub fn new(content: &str) -> Self {
        Self { content: content.to_owned(), style: TextStyle::default(), shaped: None, spans: Vec::new() }
    }

    /// Shaped at physical size, so hinting and line breaks match the pixels
    /// it is drawn at, its lines' heights rounded to whole pixels there when
    /// `whole_pixels`.
    pub fn shaped(
        &mut self,
        fonts: &mut FontContext,
        layouts: &mut LayoutContext<u16>,
        (scale, whole_pixels): (f32, bool),
    ) -> &mut Layout<u16> {
        if self.shaped.as_ref().is_none_or(|(at, _)| *at != (scale, whole_pixels)) {
            let mut builder = layouts.ranged_builder(fonts, &self.content, scale, whole_pixels);
            builder.push_default(StyleProperty::FontSize(self.style.size));
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
            builder.push_default(StyleProperty::FontFamily(self.style.family.source().into()));
            builder.push_default(StyleProperty::LineHeight(self.style.parley_line_height()));
            crate::rich::push_spans(&mut builder, &self.spans, &self.style);
            self.shaped = Some(((scale, whole_pixels), builder.build(&self.content)));
        }
        &mut self.shaped.as_mut().expect("shaped above").1
    }
}

/// The least display scale a UI is laid out at, which keeps one framed
/// before it has a size from dividing by nothing. A UI Panel far off is
/// drawn at an eighth of its size, and laid out the same as up close, so
/// this is well below that.
pub(crate) const MIN_SCALE: f32 = 1.0 / 64.0;

/// A retained tree of UI elements. Build it in code or from markup
/// ([`load_markup`](Self::load_markup)), style it with sheets, change what
/// changes, and call [`frame`](Self::frame) each frame for what to draw.
pub struct Ui {
    pub(crate) tree: TaffyTree<Element>,
    pub(crate) root: taffy::NodeId,
    /// Laid out right to left, as the game's language reads.
    pub(crate) rtl: bool,
    pub(crate) fonts: FontContext,
    pub(crate) layouts: LayoutContext<u16>,
    pub(crate) atlas: GlyphAtlas,
    /// Logical size and display scale of the last frame.
    pub(crate) viewport: (f32, f32, f32),
    /// Elements the cascade styles again next frame, as something it reads
    /// changed, each with whether everything in it is styled again too.
    pub(crate) restyles: Vec<(taffy::NodeId, bool)>,
    /// The classes, and the ancestors in a state, that selectors in any
    /// sheet ask of an ancestor; see [`RuleIndex`](crate::sheet::RuleIndex).
    pub(crate) ancestor_classes: HashSet<String>,
    pub(crate) state_ancestors: Vec<Compound>,
    pub(crate) pointer: crate::pointer::Pointer,
    pub(crate) warnings: Vec<String>,
    /// The shaders backgrounds can be drawn with (see [`Ui::set_shaders`]).
    pub(crate) shaders: std::rc::Rc<crate::UiShaders>,
    /// Shader names styles gave that no shader has, warned about already.
    pub(crate) unknown_shaders: std::collections::HashSet<String>,
    /// Text and edges kept to whole pixels at the display scale, unless the
    /// UI is drawn alike at every scale (see [`Ui::set_scalable`]).
    pub(crate) whole_pixels: bool,
    /// The size it is laid out at, in logical pixels, whatever the area it
    /// is drawn in (see [`Ui::set_layout_size`]).
    pub(crate) layout_size: Option<[f32; 2]>,
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
    /// lists, drops onto elements, and drops from elsewhere onto lists'
    /// rows since the last frame; and the elements that drag and that take
    /// drops.
    pub(crate) drag: Option<crate::drag::Drag>,
    pub(crate) row_drops: Vec<crate::list::RowDrop>,
    pub(crate) element_drops: Vec<(taffy::NodeId, crate::drag::DragSource)>,
    pub(crate) row_landings: Vec<crate::list::RowLanding>,
    pub(crate) draggables: std::collections::HashSet<taffy::NodeId>,
    pub(crate) drop_targets: std::collections::HashSet<taffy::NodeId>,
    /// Elements shown in desktop windows of their own, the window the
    /// pointer is in, and one whose bar was pressed to move it; see
    /// [`desktop`](crate::desktop).
    pub(crate) desktop: Vec<crate::desktop::Shown>,
    pub(crate) pointer_window: Option<crate::DesktopWindow>,
    pub(crate) window_move: Option<crate::DesktopWindow>,
    /// Elements moved over a desktop window, each with where it came from.
    pub(crate) visitors: Vec<(taffy::NodeId, taffy::NodeId)>,
    /// Containers whose items drag into a new order, and their slides; see
    /// [`reorder`](crate::reorder).
    pub(crate) reorders: crate::reorder::Reorders,
    /// Key maps, by their element; see [`keymap`](crate::keymap).
    pub(crate) keymaps: HashMap<taffy::NodeId, crate::keymap::Keymap>,
    pub(crate) prompts: HashMap<taffy::NodeId, crate::prompt::Prompt>,
    /// The elements whose text is a key's, in the language the game is in,
    /// and whether any were bound since the last look (see
    /// [`Ui::set_text_key`]).
    text_keys: HashMap<taffy::NodeId, String>,
    text_keys_added: bool,
    /// What shows when the pointer rests on an element, and the key it is
    /// the text of, if any (see [`Ui::set_tooltip`]).
    pub(crate) tooltips: HashMap<taffy::NodeId, (String, Option<String>)>,
    /// A number's label being dragged to change it (see [`scrub`](crate::scrub)).
    pub(crate) scrub: Option<crate::scrub::Scrub>,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    pub fn new() -> Self {
        Self::with_fonts(FontContext::new())
    }

    /// A new UI that finds fonts as this one does, sharing what it found:
    /// finding the system's fonts afresh takes a while, which a UI made
    /// often, as a panel in the world is, should not.
    pub fn sharing_fonts(&self) -> Self {
        Self::with_fonts(self.fonts.clone())
    }

    fn with_fonts(fonts: FontContext) -> Self {
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
            rtl: false,
            fonts,
            layouts: LayoutContext::new(),
            atlas: GlyphAtlas::new(),
            viewport: (0.0, 0.0, 0.0),
            restyles: vec![(root, true)],
            ancestor_classes: HashSet::new(),
            state_ancestors: Vec::new(),
            pointer: Default::default(),
            warnings: Vec::new(),
            shaders: Default::default(),
            unknown_shaders: Default::default(),
            whole_pixels: true,
            layout_size: None,
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
            row_landings: Vec::new(),
            draggables: Default::default(),
            drop_targets: Default::default(),
            desktop: Vec::new(),
            pointer_window: None,
            window_move: None,
            visitors: Vec::new(),
            reorders: Default::default(),
            keymaps: HashMap::new(),
            prompts: HashMap::new(),
            text_keys: HashMap::new(),
            text_keys_added: false,
            tooltips: HashMap::new(),
            scrub: None,
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

    /// Fills an element with a gradient over its background, rounded as its
    /// corners are, or takes it away with `None`.
    pub fn set_gradient(&mut self, node: NodeId, gradient: Option<Gradient>) {
        if let Some(element) = self.element_mut(node) {
            element.gradient = gradient;
        }
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

    /// The shapes drawn on an element.
    pub fn shapes(&self, node: NodeId) -> &[crate::Shape] {
        self.element(node).map_or(&[], |element| element.shapes.as_slice())
    }

    /// Makes an element a window onto what the renderer drew under the UI,
    /// such as a 3D view: the UI drawn before it is cut away inside it, so
    /// what lies under a floating window does not show through its view.
    pub fn set_see_through(&mut self, node: NodeId, see_through: bool) {
        if let Some(element) = self.element_mut(node) {
            element.see_through = see_through;
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
        self.restyle(node, true);
        NodeId(node)
    }

    pub(crate) fn element(&self, node: NodeId) -> Option<&Element> {
        self.tree.get_node_context(node.0)
    }

    pub(crate) fn element_mut(&mut self, node: NodeId) -> Option<&mut Element> {
        self.tree.get_node_context_mut(node.0)
    }

    /// Binds an element's text to a key, such as `menu.play`, for whoever
    /// keeps the game's languages to show that key's text in it, now and
    /// whenever the language changes (see [`text_keys`](Self::text_keys)).
    /// `None` unbinds it, leaving the text it has.
    pub fn set_text_key(&mut self, node: NodeId, key: Option<&str>) {
        match key {
            Some(key) => {
                self.text_keys.insert(node.0, key.to_string());
                self.text_keys_added = true;
            }
            None => drop(self.text_keys.remove(&node.0)),
        }
    }

    /// The key an element's text is bound to.
    pub fn text_key(&self, node: NodeId) -> Option<&str> {
        self.text_keys.get(&node.0).map(String::as_str)
    }

    /// Every element whose text is bound to a key, and the key.
    pub fn text_keys(&self) -> Vec<(NodeId, String)> {
        self.text_keys.iter().map(|(&node, key)| (NodeId(node), key.clone())).collect()
    }

    /// Whether elements were bound to keys since the last call, so their
    /// text is still to be shown.
    pub fn take_text_keys_added(&mut self) -> bool {
        std::mem::take(&mut self.text_keys_added)
    }

    /// Gives an element a tooltip, what shows when the pointer rests on it,
    /// for whoever shows the UI to show, as the editor and a game's UI do
    /// (see [`hovered_tooltip`](Self::hovered_tooltip)). `key` names the
    /// string table key it is the text of, to follow the language as
    /// [`set_text_key`](Self::set_text_key) does. Empty text and no key
    /// take it away.
    pub fn set_tooltip(&mut self, node: NodeId, text: &str, key: Option<&str>) {
        if text.is_empty() && key.is_none() {
            self.tooltips.remove(&node.0);
            return;
        }
        self.tooltips.insert(node.0, (text.to_string(), key.map(str::to_string)));
        if key.is_some() {
            self.text_keys_added = true;
        }
    }

    pub fn tooltip(&self, node: NodeId) -> Option<&str> {
        self.tooltips.get(&node.0).map(|(text, _)| text.as_str())
    }

    /// Every element whose tooltip is a key's text, and the key.
    pub fn tooltip_keys(&self) -> Vec<(NodeId, String)> {
        let keyed = self.tooltips.iter().filter_map(|(&node, (_, key))| Some((NodeId(node), key.clone()?)));
        keyed.collect()
    }

    /// Changes an element's text. Setting the same text again costs nothing,
    /// so this can be called every frame.
    pub fn set_text(&mut self, node: NodeId, text: &str) {
        // A prompt's text is the action it is for.
        if self.is_prompt(node) {
            return self.set_prompt_action(node, text);
        }
        if let Some(edit) = self.editing(node) {
            if edit.editor.raw_text() != text {
                edit.editor.set_text(text);
                edit.forget_history();
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
            self.restyle(node.0, false);
        }
    }

    /// The style an element starts from, before sheets.
    pub fn set_style(&mut self, node: NodeId, style: Style) {
        if let Some(element) = self.element_mut(node)
            && element.base != style
        {
            element.base = style;
            self.restyle(node.0, false);
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
            self.restyle(node.0, false);
        }
    }

    /// Lays the whole UI out right to left, as Arabic and Hebrew read, or
    /// back: rows run from the right, left and right swap in margins,
    /// padding, borders and places, and text starts at the right. An
    /// element styled `direction: ltr` keeps left to right, with what is in
    /// it, as a key map does.
    pub fn set_right_to_left(&mut self, rtl: bool) {
        if self.rtl != rtl {
            self.rtl = rtl;
            self.restyle(self.root, true);
        }
    }

    pub fn is_right_to_left(&self) -> bool {
        self.rtl
    }

    /// Styles `node` and everything in it with `sheet`. Sheets nearer an
    /// element win over those further up when rules are equally specific.
    pub fn add_sheet(&mut self, node: NodeId, sheet: StyleSheet) {
        self.warn(sheet.warnings().to_vec());
        self.note_ancestor_keys(&sheet);
        if let Some(element) = self.element_mut(node) {
            element.sheets.push(Rc::new(sheet));
            self.restyle(node.0, true);
        }
    }

    /// Has the next cascade style `node` again, and everything in it when
    /// `deep`. A UI changed over and over with no frame between, as a game's
    /// is while it is not shown, is styled whole instead.
    pub(crate) fn restyle(&mut self, node: taffy::NodeId, deep: bool) {
        const HELD: usize = 4096;
        if self.restyles.len() >= HELD {
            self.restyles.clear();
            self.restyles.push((self.root, true));
        }
        self.restyles.push((node, deep));
    }

    /// Notes the classes and states `sheet` asks of ancestors, so changing
    /// one on an element restyles what is inside it.
    pub(crate) fn note_ancestor_keys(&mut self, sheet: &StyleSheet) {
        self.ancestor_classes.extend(sheet.index.ancestor_classes.iter().cloned());
        for compound in &sheet.index.state_ancestors {
            if !self.state_ancestors.contains(compound) {
                self.state_ancestors.push(compound.clone());
            }
        }
    }

    /// Takes away every sheet `node` was given, for loading them again.
    pub fn clear_sheets(&mut self, node: NodeId) {
        if let Some(element) = self.element_mut(node) {
            element.sheets.clear();
            self.restyle(node.0, true);
        }
    }

    pub fn set_name(&mut self, node: NodeId, name: &str) {
        if let Some(element) = self.element_mut(node) {
            element.name = Some(name.to_string());
            self.restyle(node.0, true);
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
        let deep = self.ancestor_classes.contains(class);
        self.restyle(node.0, deep);
    }

    pub fn has_class(&self, node: NodeId, class: &str) -> bool {
        self.element(node).is_some_and(|e| e.classes.iter().any(|c| c == class))
    }

    pub(crate) fn set_state(&mut self, node: NodeId, state: States, on: bool) {
        let Some(element) = self.element_mut(node) else { return };
        if element.states.contains(state) != on {
            element.states.set(state, on);
            let deep = self.state_reaches_inside(node.0, state);
            self.restyle(node.0, deep);
        }
    }

    /// Whether a selector asks `state` of an element like `node` as an
    /// ancestor, as `Toggle:hover .toggle-box` asks it of a Toggle, so that
    /// changing it restyles what is inside `node`. The pointer sets `:hover`
    /// and `:active` on everything from the root down, most of which no
    /// selector asks it of.
    fn state_reaches_inside(&self, node: taffy::NodeId, state: States) -> bool {
        let Some(element) = self.tree.get_node_context(node) else { return false };
        let (tag, name, classes) = (element.kind.tag(), element.name.as_deref(), &element.classes);
        let asks = |compound: &&Compound| compound.states.intersects(state);
        self.state_ancestors.iter().filter(asks).any(|compound| compound.could_match(tag, name, classes))
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
            self.restyle(node.0, false);
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

    /// Moves an element, and everything in it, to the end of `parent`.
    /// Nothing moves into itself or into what is inside it.
    pub fn move_into(&mut self, node: NodeId, parent: NodeId) {
        if self.is_within(parent, node) {
            return;
        }
        if let Some(old) = self.tree.parent(node.0) {
            let _ = self.tree.remove_child(old, node.0);
        }
        let _ = self.tree.add_child(parent.0, node.0);
        // What is above it, which selectors see, changed.
        self.restyle(node.0, true);
    }

    /// Removes an element and everything in it.
    pub fn remove(&mut self, node: NodeId) {
        if !self.visitors.is_empty() {
            self.send_visitors_home(node.0);
        }
        self.visitors.retain(|&(visitor, home)| visitor != node.0 && home != node.0);
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
        self.forget_scrub(node.0);
        self.draggables.remove(&node.0);
        self.drop_targets.remove(&node.0);
        self.reorders.forget(node.0);
        self.forget_keymap(node.0);
        self.forget_prompt(node.0);
        self.text_keys.remove(&node.0);
        self.tooltips.remove(&node.0);
        self.lists.retain(|&n| n != node.0);
        self.scrollers.retain(|&n| n != node.0);
        self.fields.retain(|&n| n != node.0);
        self.desktop.retain(|shown| shown.node != node.0);
        if self.input.focused == Some(node.0) {
            self.input.focused = None;
        }
        if self.input.dragging == Some(node.0) {
            self.input.dragging = None;
        }
    }

    /// An element's style as the last frame's cascade worked it out.
    pub fn style(&self, node: NodeId) -> Option<&Style> {
        self.element(node).map(|e| &e.computed)
    }

    /// Whether anything is mid-transition or playing keyframes, so a caller
    /// that only redraws on change knows to keep drawing.
    pub fn is_animating(&self) -> bool {
        self.animating(self.root)
    }

    fn animating(&self, node: taffy::NodeId) -> bool {
        let now = self.input.time;
        let busy = self.tree.get_node_context(node).is_some_and(|e| {
            !e.animations.is_empty() || e.keyframes.as_ref().is_some_and(|played| !played.finished(now))
        });
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

    /// The shaders backgrounds can be drawn with, by the names styles give
    /// them: the built-in effects, which a new UI has, and a project's own,
    /// which the engine adds.
    pub fn set_shaders(&mut self, shaders: std::rc::Rc<crate::UiShaders>) {
        if !std::rc::Rc::ptr_eq(&self.shaders, &shaders) {
            self.shaders = shaders;
            self.unknown_shaders.clear();
        }
    }

    /// Draws this UI alike at every display scale, as one drawn at many
    /// sizes needs: a UI Panel in the world is drawn small far off and large
    /// up close, and its text wraps the same and nothing in it moves as it
    /// goes from one to the other. Its text, edges and borders are then
    /// placed where its layout puts them, its glyphs unhinted and placed to
    /// a quarter of a pixel, rather than kept to whole pixels at the scale
    /// it is drawn at, which a UI drawn at one size looks crisper for.
    pub fn set_scalable(&mut self, scalable: bool) {
        if self.whole_pixels == scalable {
            self.whole_pixels = !scalable;
            self.reshape(self.root);
        }
    }

    /// Lays this UI out at `size` logical pixels, whatever the area it is
    /// drawn in, or at that area's size for `None`. A UI Panel's texture is
    /// a whole number of pixels, a little larger than its layout at most
    /// scales, which its UI keeps to rather than stretching to fill.
    pub fn set_layout_size(&mut self, size: Option<[f32; 2]>) {
        self.layout_size = size;
    }

    /// Says once for each, until the shaders change, that a style named a
    /// shader there is none of.
    pub(crate) fn warn_unknown_shaders(&mut self, names: Vec<String>) {
        let new: Vec<String> = names.into_iter().filter(|name| self.unknown_shaders.insert(name.clone())).collect();
        self.warn(new.into_iter().map(|name| format!("there is no UI shader '{name}'; its background is drawn plain")).collect());
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
    /// forgotten after this. What is shown in desktop windows of its own is
    /// left out, for [`window_frame`](Self::window_frame) to draw.
    pub fn frame(&mut self, width: u32, height: u32, scale: f32) -> UiFrame {
        if !self.restyles.is_empty() {
            self.cascade();
        }
        self.layout(width, height, scale);
        self.update_scroll_limits();
        // Lists make and bind the rows now on screen, which then need laying out.
        let listed = {
            voxear_core::profile_scope!("Lists");
            self.update_lists()
        };
        if listed {
            if !self.restyles.is_empty() {
                self.cascade();
            }
            self.layout(width, height, scale);
            self.update_scroll_limits();
        }
        if self.place_menu() {
            if !self.restyles.is_empty() {
                self.cascade();
            }
            self.layout(width, height, scale);
        }
        // Key maps place their boards and labels by where they were laid out.
        if self.arrange_keymaps() {
            if !self.restyles.is_empty() {
                self.cascade();
            }
            self.layout(width, height, scale);
        }
        self.refresh_fields();
        let scale = self.viewport.2;
        let (focused, caret_on, time) = (self.input.focused, self.caret_on(), self.input.time);
        let mut quads = Vec::new();
        let (mut values, mut unknown) = (Vec::new(), Vec::new());
        let skip: Vec<taffy::NodeId> = self.desktop.iter().map(|shown| shown.node).collect();
        voxear_core::profile_scope!("Draw");
        // An atlas that filled up starts again part way through, so the
        // glyphs drawn before then are drawn once more.
        for _ in 0..2 {
            quads.clear();
            values.clear();
            self.atlas.restarted = false;
            let mut draw = Draw {
                tree: &mut self.tree,
                atlas: &mut self.atlas,
                scale,
                whole_pixels: self.whole_pixels,
                quads: &mut quads,
                focused,
                caret_on,
                time,
                raised: Vec::new(),
                lifting: false,
                skip: skip.clone(),
                shaders: &self.shaders,
                values: &mut values,
                unknown_shaders: &mut unknown,
            };
            draw.all(self.root);
            if !self.atlas.restarted {
                break;
            }
        }
        self.warn_unknown_shaders(unknown);
        self.pointer.clicks.clear();
        self.pointer.double_clicks.clear();
        self.pointer.link_clicks.clear();
        self.pointer.context_clicks.clear();
        self.menu_choices.clear();
        self.row_drops.clear();
        self.reorders.frame_done();
        self.element_drops.clear();
        self.row_landings.clear();
        self.clear_selection_changes();
        self.clear_input_events();
        UiFrame { quads, values, atlas: self.atlas.snapshot(), fields: self.atlas.fields_snapshot() }
    }

    /// Lays the tree out and lets each list make and bind the rows now on
    /// screen, without drawing, for lists whose rows are filled by code that
    /// cannot run inside [`frame`](Self::frame), as a game's scripts. The
    /// frame after lays out again what that code changed.
    pub fn prepare(&mut self, width: u32, height: u32, scale: f32) {
        if !self.restyles.is_empty() {
            self.cascade();
        }
        self.layout(width, height, scale);
        self.update_scroll_limits();
        self.update_lists();
    }

    fn layout(&mut self, width: u32, height: u32, scale: f32) {
        self.compute_layout(width, height, scale);
        // A desktop window's element goes at the screen's top left, which only a layout says how far that is.
        if self.place_windows() {
            if !self.restyles.is_empty() {
                self.cascade();
            }
            self.compute_layout(width, height, scale);
        }
    }

    fn compute_layout(&mut self, width: u32, height: u32, scale: f32) {
        voxear_core::profile_scope!("Layout");
        let scale = scale.max(MIN_SCALE);
        let [width, height] = self.layout_size.unwrap_or([width as f32 / scale, height as f32 / scale]);
        let viewport = (width, height, scale);
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
        let shaping = (scale, self.whole_pixels);
        let measured = self.tree.compute_layout_with_measure(self.root, space, |input, _, element, style| {
            measure(input, element, style, fonts, layouts, shaping)
        });
        measured.expect("the root is in this tree");
    }

    /// Text is shaped for the display scale, so a new scale shapes it again.
    pub(crate) fn reshape(&mut self, node: taffy::NodeId) {
        for child in self.tree.children(node).unwrap_or_default() {
            self.reshape(child);
        }
        let Some(text) = self.tree.get_node_context_mut(node).and_then(|e| e.text.as_mut()) else { return };
        text.shaped = None;
        let _ = self.tree.mark_dirty(node);
    }
}
