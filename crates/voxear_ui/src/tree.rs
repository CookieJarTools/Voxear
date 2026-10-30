//! The element tree: taffy nodes, each carrying its element's look and text.

use parley::{FontContext, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};
use taffy::{AvailableSpace, Dimension, Display, LayoutInput, LayoutOutput, Size, TaffyTree};

use crate::{Quad, Style, TextStyle, UiFrame, atlas::GlyphAtlas};

/// An element in a [`Ui`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(taffy::NodeId);

/// What each taffy node carries besides its layout.
#[derive(Default)]
struct Element {
    style: Style,
    text: Option<Text>,
}

struct Text {
    content: String,
    style: TextStyle,
    /// Shaped at the display scale it was last laid out at.
    shaped: Option<(f32, Layout<()>)>,
}

impl Text {
    /// Shaped at physical size, so hinting and line breaks match the pixels
    /// it is drawn at.
    fn shaped(&mut self, fonts: &mut FontContext, layouts: &mut LayoutContext<()>, scale: f32) -> &mut Layout<()> {
        if self.shaped.as_ref().is_none_or(|(at, _)| *at != scale) {
            let mut builder = layouts.ranged_builder(fonts, &self.content, scale, true);
            builder.push_default(StyleProperty::FontSize(self.style.size));
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
            builder.push_default(StyleProperty::FontFamily(self.style.family.source().into()));
            self.shaped = Some((scale, builder.build(&self.content)));
        }
        &mut self.shaped.as_mut().expect("shaped above").1
    }
}

/// A retained tree of UI elements. Build it once, change what changes, and
/// call [`frame`](Self::frame) each frame for what to draw.
pub struct Ui {
    tree: TaffyTree<Element>,
    root: taffy::NodeId,
    fonts: FontContext,
    layouts: LayoutContext<()>,
    atlas: GlyphAtlas,
    /// Logical size and display scale of the last frame.
    viewport: (f32, f32, f32),
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
        let root =
            tree.new_leaf_with_context(taffy::Style::default(), Element::default()).expect("a new tree has room");
        Self {
            tree,
            root,
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
            atlas: GlyphAtlas::new(),
            viewport: (0.0, 0.0, 0.0),
        }
    }

    /// The element covering the whole screen, which everything else is in.
    pub fn root(&self) -> NodeId {
        NodeId(self.root)
    }

    /// A box that holds other elements.
    pub fn panel(&mut self, parent: NodeId, style: Style) -> NodeId {
        self.add(parent, Element { style, text: None })
    }

    /// A line or paragraph of text, wrapping when its parent is narrower.
    pub fn label(&mut self, parent: NodeId, text: &str, style: TextStyle) -> NodeId {
        let text = Text { content: text.to_owned(), style, shaped: None };
        self.add(parent, Element { style: Style::default(), text: Some(text) })
    }

    fn add(&mut self, parent: NodeId, element: Element) -> NodeId {
        let node = self.tree.new_leaf_with_context(element.style.layout.clone(), element).expect("room for a node");
        self.tree.add_child(parent.0, node).expect("the parent is in this tree");
        NodeId(node)
    }

    /// Changes a label's text. Setting the same text again costs nothing, so
    /// this can be called every frame.
    pub fn set_text(&mut self, node: NodeId, text: &str) {
        let Some(label) = self.text_mut(node) else { return };
        if label.content == text {
            return;
        }
        label.content.clear();
        label.content.push_str(text);
        label.shaped = None;
        let _ = self.tree.mark_dirty(node.0);
    }

    pub fn set_text_style(&mut self, node: NodeId, style: TextStyle) {
        let Some(label) = self.text_mut(node) else { return };
        if label.style == style {
            return;
        }
        label.style = style;
        label.shaped = None;
        let _ = self.tree.mark_dirty(node.0);
    }

    fn text_mut(&mut self, node: NodeId) -> Option<&mut Text> {
        self.tree.get_node_context_mut(node.0)?.text.as_mut()
    }

    pub fn set_style(&mut self, node: NodeId, style: Style) {
        let _ = self.tree.set_style(node.0, style.layout.clone());
        if let Some(element) = self.tree.get_node_context_mut(node.0) {
            element.style = style;
        }
    }

    /// Hidden elements take no space and draw nothing, nor do their children.
    pub fn set_visible(&mut self, node: NodeId, visible: bool) {
        let Ok(style) = self.tree.style(node.0) else { return };
        let display = if visible { Display::Flex } else { Display::None };
        if style.display != display {
            let style = taffy::Style { display, ..style.clone() };
            let _ = self.tree.set_style(node.0, style);
        }
    }

    pub fn is_visible(&self, node: NodeId) -> bool {
        self.tree.style(node.0).is_ok_and(|s| s.display != Display::None)
    }

    /// Removes an element and everything in it.
    pub fn remove(&mut self, node: NodeId) {
        for child in self.tree.children(node.0).unwrap_or_default() {
            self.remove(NodeId(child));
        }
        let _ = self.tree.remove(node.0);
    }

    /// Where an element was laid out by the last frame: left, top, width and
    /// height in logical pixels from the screen's top left.
    pub fn rect(&self, node: NodeId) -> [f32; 4] {
        let Ok(layout) = self.tree.layout(node.0) else { return [0.0; 4] };
        let (mut x, mut y) = (layout.location.x, layout.location.y);
        let mut at = node.0;
        while let Some(parent) = self.tree.parent(at) {
            let location = self.tree.layout(parent).map(|l| l.location).unwrap_or_default();
            (x, y) = (x + location.x, y + location.y);
            at = parent;
        }
        [x, y, layout.size.width, layout.size.height]
    }

    /// Lays the tree out for a screen `width` by `height` physical pixels at
    /// `scale` physical pixels per logical one, and returns what to draw.
    pub fn frame(&mut self, width: u32, height: u32, scale: f32) -> UiFrame {
        self.layout(width, height, scale);
        let scale = self.viewport.2;
        let mut quads = Vec::new();
        // An atlas that filled up starts again part way through, so the
        // glyphs drawn before then are drawn once more.
        for _ in 0..2 {
            quads.clear();
            self.atlas.restarted = false;
            let mut draw = Draw { tree: &mut self.tree, atlas: &mut self.atlas, scale, quads: &mut quads };
            draw.node(self.root, (0.0, 0.0));
            if !self.atlas.restarted {
                break;
            }
        }
        UiFrame { quads, atlas: self.atlas.take_upload() }
    }

    fn layout(&mut self, width: u32, height: u32, scale: f32) {
        let scale = scale.max(0.25);
        let viewport = (width as f32 / scale, height as f32 / scale, scale);
        if viewport != self.viewport {
            if viewport.2 != self.viewport.2 {
                self.reshape(self.root);
            }
            let size = Size { width: Dimension::length(viewport.0), height: Dimension::length(viewport.1) };
            let style = taffy::Style { size, ..self.tree.style(self.root).cloned().unwrap_or_default() };
            let _ = self.tree.set_style(self.root, style);
            self.viewport = viewport;
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

/// How big a leaf is: its text, broken to fit, or nothing.
fn measure(
    input: LayoutInput,
    element: Option<&mut Element>,
    style: &taffy::Style,
    fonts: &mut FontContext,
    layouts: &mut LayoutContext<()>,
    scale: f32,
) -> LayoutOutput {
    let Some(text) = element.and_then(|e| e.text.as_mut()) else {
        return taffy::compute_leaf_layout(input, style, |_, _| 0.0, |_, _| Size::ZERO);
    };
    taffy::compute_leaf_layout(
        input,
        style,
        |_, _| 0.0,
        |known, available| {
            let max = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                AvailableSpace::MinContent => Some(0.0),
                AvailableSpace::MaxContent => None,
            });
            let layout = text.shaped(fonts, layouts, scale);
            layout.break_all_lines(max.map(|w| w * scale));
            Size {
                width: known.width.unwrap_or(layout.width() / scale),
                height: known.height.unwrap_or(layout.height() / scale),
            }
        },
    )
}

/// Walks the laid-out tree into quads, parents before children.
struct Draw<'a> {
    tree: &'a mut TaffyTree<Element>,
    atlas: &'a mut GlyphAtlas,
    scale: f32,
    quads: &'a mut Vec<Quad>,
}

impl Draw<'_> {
    fn node(&mut self, node: taffy::NodeId, parent: (f32, f32)) {
        if self.tree.style(node).is_ok_and(|s| s.display == Display::None) {
            return;
        }
        let layout = *self.tree.layout(node).expect("laid out");
        let at = (parent.0 + layout.location.x, parent.1 + layout.location.y);
        let [x0, y0] = self.snap(at.0, at.1);
        let [x1, y1] = self.snap(at.0 + layout.size.width, at.1 + layout.size.height);
        let inset = (layout.border.left + layout.padding.left, layout.border.top + layout.padding.top);
        let origin = self.snap(at.0 + inset.0, at.1 + inset.1);
        let inner = layout.size.width - inset.0 - layout.border.right - layout.padding.right;
        if let Some(element) = self.tree.get_node_context_mut(node) {
            shape_quad(&element.style, [x0, y0, x1 - x0, y1 - y0], self.scale, self.quads);
            if let Some(text) = &mut element.text
                && let Some((_, shaped)) = &mut text.shaped
            {
                // Broken again at the width it ended up, which the last
                // measure may not have been.
                shaped.break_all_lines(Some((inner * self.scale).ceil()));
                glyph_quads(shaped, origin, text.style.color.linear_premultiplied(), self.atlas, self.quads);
            }
        }
        for child in self.tree.children(node).unwrap_or_default() {
            self.node(child, at);
        }
    }

    /// A logical point on the physical pixel grid.
    fn snap(&self, x: f32, y: f32) -> [f32; 2] {
        [(x * self.scale).round(), (y * self.scale).round()]
    }
}

fn shape_quad(style: &Style, rect: [f32; 4], scale: f32, quads: &mut Vec<Quad>) {
    let border = style.border.filter(|(width, _)| *width > 0.0);
    if style.background.is_none() && border.is_none() {
        return;
    }
    let (border, border_color) =
        border.map_or((0.0, [0.0; 4]), |(w, c)| ((w * scale).round().max(1.0), c.linear_premultiplied()));
    quads.push(Quad {
        rect,
        color: style.background.map_or([0.0; 4], |c| c.linear_premultiplied()),
        border_color,
        radius: style.radius * scale,
        border,
        kind: Quad::SHAPE,
        ..Quad::default()
    });
}

/// Every glyph of a shaped text, on whole pixels: hinted glyphs are drawn
/// where the hinting expected them.
fn glyph_quads(layout: &Layout<()>, origin: [f32; 2], color: [f32; 4], atlas: &mut GlyphAtlas, quads: &mut Vec<Quad>) {
    let size = atlas.size() as f32;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let font = run.font();
            let font = (font.data.data(), font.index, font.data.id());
            let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
            for glyph in glyph_run.positioned_glyphs() {
                let Some(found) = atlas.glyph(font, glyph.id as u16, run.font_size(), &coords) else { continue };
                let pen = [(origin[0] + glyph.x).round(), (origin[1] + glyph.y).round()];
                let (x, y) = (found.x as f32, found.y as f32);
                let (w, h) = (found.width as f32, found.height as f32);
                quads.push(Quad {
                    rect: [pen[0] + found.left as f32, pen[1] - found.top as f32, w, h],
                    uv: [x / size, y / size, (x + w) / size, (y + h) / size],
                    color,
                    kind: Quad::GLYPH,
                    ..Quad::default()
                });
            }
        }
    }
}
