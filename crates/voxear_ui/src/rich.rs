//! Rich text: a label whose words differ in weight, slant, face or colour,
//! with links in it, as a page of documentation needs. The text is one
//! string; spans over parts of it say how those parts differ. Colours for
//! links, inline code and highlights come from the label's text style
//! (`link-color`, `code-background` and `mark-color` in sheets), so a theme
//! sets them once; a span may also have a colour of its own, as each part of
//! a stack frame does in the Console.

use std::ops::Range;

use parley::{Cluster, FontStyle, FontWeight, RangedBuilder, StyleProperty};

use crate::{
    NodeId, Style, TextStyle, Ui,
    tree::{Element, Kind, Text},
};

/// How part of a rich label differs from the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// Bold.
    pub strong: bool,
    /// Italic.
    pub emphasis: bool,
    /// Monospaced, on a tinted ground.
    pub code: bool,
    /// Struck through.
    pub strike: bool,
    /// Highlighted.
    pub mark: bool,
    /// A link that goes nowhere yet, such as a page not written: faded and
    /// underlined rather than in the link colour.
    pub broken: bool,
}

/// A stretch of a rich label, by byte range, with its marks and the link it
/// is, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub marks: Marks,
    pub link: Option<String>,
    /// Its own colour, over the label's and a link's.
    pub color: Option<crate::Color>,
}

/// Text with spans over it; build it with [`push`](Self::push) and
/// [`push_link`](Self::push_link).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `text`, marked as `marks` say. Plain text needs no span.
    pub fn push(&mut self, text: &str, marks: Marks) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        if marks != Marks::default() {
            self.spans.push(Span { range: start..self.text.len(), marks, link: None, color: None });
        }
        self
    }

    /// Adds `text` in a colour of its own.
    pub fn push_colored(&mut self, text: &str, color: crate::Color) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.spans.push(Span {
            range: start..self.text.len(),
            marks: Marks::default(),
            link: None,
            color: Some(color),
        });
        self
    }

    /// Adds `text` in a colour of its own, as a link to `url`.
    pub fn push_link_colored(&mut self, text: &str, url: &str, color: crate::Color) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        let link = Some(url.to_string());
        self.spans.push(Span { range: start..self.text.len(), marks: Marks::default(), link, color: Some(color) });
        self
    }

    /// Adds `text` as a link to `url`, which [`Ui::clicked_link`] reports.
    pub fn push_link(&mut self, text: &str, url: &str, marks: Marks) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        self.spans.push(Span { range: start..self.text.len(), marks, link: Some(url.to_string()), color: None });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl From<&str> for RichText {
    fn from(text: &str) -> Self {
        Self { text: text.to_string(), spans: Vec::new() }
    }
}

/// Gives each span its style while shaping. Each gets its own brush, its
/// index plus one, so the runs split at every span and drawing can tell
/// which span a run belongs to.
pub(crate) fn push_spans(builder: &mut RangedBuilder<'_, u16>, spans: &[Span], style: &TextStyle) {
    for (index, span) in spans.iter().enumerate() {
        let range = span.range.clone();
        builder.push(StyleProperty::Brush(index as u16 + 1), range.clone());
        let marks = span.marks;
        if marks.strong {
            builder.push(StyleProperty::FontWeight(FontWeight::new((style.weight + 200.0).min(800.0))), range.clone());
        }
        if marks.emphasis {
            builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
        }
        if marks.code {
            builder.push(StyleProperty::FontFamily(crate::Family::Mono.source().into()), range.clone());
            builder.push(StyleProperty::FontSize(style.size * 0.92), range.clone());
        }
        if marks.strike {
            builder.push(StyleProperty::Strikethrough(true), range.clone());
        }
        if marks.broken {
            builder.push(StyleProperty::Underline(true), range);
        }
    }
}

impl Ui {
    /// A label of rich text, wrapping like any other, whose text style comes
    /// from its parent and sheets.
    pub fn rich_label(&mut self, parent: NodeId, text: &RichText) -> NodeId {
        let node =
            self.add(parent, Element { text: Some(Text::new("")), ..Element::new(Kind::Label, Style::default()) });
        self.set_rich_text(node, text);
        node
    }

    /// Changes a label's text and spans. The same again costs nothing.
    pub fn set_rich_text(&mut self, node: NodeId, rich: &RichText) {
        let Some(text) = self.element_mut(node).and_then(|e| e.text.as_mut()) else { return };
        if text.content == rich.text && text.spans == rich.spans {
            return;
        }
        text.content.clone_from(&rich.text);
        text.spans.clone_from(&rich.spans);
        text.shaped = None;
        let _ = self.tree.mark_dirty(node.0);
    }

    /// The link clicked in `node`, or in anything in it, since the last frame.
    pub fn clicked_link(&self, node: NodeId) -> Option<&str> {
        self.pointer
            .link_clicks
            .iter()
            .find(|(target, _)| self.is_within(NodeId(*target), node))
            .map(|(_, url)| url.as_str())
    }

    /// Where a link's first letter is drawn in a rich label, from its last
    /// layout, as `[x, y, width, height]`: so a test, or a tour, can point at
    /// the link to click it.
    pub fn link_rect(&self, node: NodeId, url: &str) -> Option<[f32; 4]> {
        let text = self.element(node)?.text.as_ref()?;
        let span = text.spans.iter().find(|span| span.link.as_deref() == Some(url))?;
        let ((scale, _), layout) = text.shaped.as_ref()?;
        let [x, y, _, height] = self.content_box(node);
        let spare = ((height * scale - layout.height()).max(0.0) / 2.0).round();
        let cluster = Cluster::from_byte_index(layout, span.range.start)?;
        let line = *cluster.line().metrics();
        let left = cluster.visual_offset()?;
        let top = line.block_min_coord + spare;
        let size = [cluster.advance(), line.block_max_coord - line.block_min_coord];
        Some([x + left / scale, y + top / scale, size[0] / scale, size[1] / scale])
    }

    /// The link under a point in a rich label, from its last layout.
    pub(crate) fn link_at(&self, node: NodeId, point: (f32, f32)) -> Option<String> {
        let text = self.element(node)?.text.as_ref()?;
        if !text.spans.iter().any(|span| span.link.is_some()) {
            return None;
        }
        let ((scale, _), layout) = text.shaped.as_ref()?;
        let [x, y, _, height] = self.content_box(node);
        // Drawn centred down a taller box, as the drawing does.
        let spare = ((height * scale - layout.height()).max(0.0) / 2.0).round();
        let (local_x, local_y) = ((point.0 - x) * scale, (point.1 - y) * scale - spare);
        let (cluster, _) = Cluster::from_point_exact(layout, local_x, local_y)?;
        let index = cluster.text_range().start;
        text.spans.iter().find(|span| span.range.contains(&index)).and_then(|span| span.link.clone())
    }
}
