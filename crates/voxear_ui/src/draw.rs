//! Measuring text for layout, and walking the laid-out tree into quads.

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem};
use taffy::{AvailableSpace, Display, LayoutInput, LayoutOutput, Size, TaffyTree};

use crate::{
    Overflow, Quad, Style,
    atlas::GlyphAtlas,
    scroll::{BAR, BAR_INSET, thumb},
    tree::Element,
};

/// How big a leaf is: its text, broken to fit, or nothing.
pub(crate) fn measure(
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
pub(crate) struct Draw<'a> {
    pub tree: &'a mut TaffyTree<Element>,
    pub atlas: &'a mut GlyphAtlas,
    pub scale: f32,
    pub quads: &'a mut Vec<Quad>,
}

impl Draw<'_> {
    /// Draws `node` at `parent`'s position, faded by the opacity of the
    /// elements it is in and kept within `clip` (physical pixels).
    pub fn node(&mut self, node: taffy::NodeId, parent: (f32, f32), opacity: f32, clip: [f32; 4]) {
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
        let outside = x1 <= clip[0] || y1 <= clip[1] || x0 >= clip[2] || y0 >= clip[3];
        let Some(element) = self.tree.get_node_context_mut(node) else { return };
        let opacity = opacity * element.computed.opacity;
        let (overflow, scroll, scroll_max, bar) =
            (element.computed.overflow, element.scroll, element.scroll_max, element.computed.scrollbar);
        if !outside {
            shape_quad(&element.computed, [x0, y0, x1 - x0, y1 - y0], self.scale, opacity, clip, self.quads);
            let color = element.text_style.color.linear_premultiplied().map(|c| c * opacity);
            if let Some(text) = &mut element.text
                && let Some((_, shaped)) = &mut text.shaped
            {
                // Broken again at the width it ended up, which the last
                // measure may not have been.
                shaped.break_all_lines(Some((inner * self.scale).ceil()));
                glyph_quads(shaped, origin, color, clip, self.atlas, self.quads);
            }
        }
        // What it clips cannot show when it is itself out of sight.
        if opacity <= 0.0 || (outside && overflow != Overflow::Visible) {
            return;
        }
        let padding_box = [
            at.0 + layout.border.left,
            at.1 + layout.border.top,
            layout.size.width - layout.border.left - layout.border.right,
            layout.size.height - layout.border.top - layout.border.bottom,
        ];
        let child_clip = if overflow == Overflow::Visible {
            clip
        } else {
            let [l, t] = self.snap(padding_box[0], padding_box[1]);
            let [r, b] = self.snap(padding_box[0] + padding_box[2], padding_box[1] + padding_box[3]);
            [clip[0].max(l), clip[1].max(t), clip[2].min(r), clip[3].min(b)]
        };
        let origin = (at.0 - scroll[0], at.1 - scroll[1]);
        for child in self.tree.children(node).unwrap_or_default() {
            self.node(child, origin, opacity, child_clip);
        }
        if overflow == Overflow::Scroll
            && let Some((top, length)) = thumb(padding_box[3], scroll_max[1], scroll[1])
        {
            let x = padding_box[0] + padding_box[2] - BAR - BAR_INSET;
            let [l, t] = self.snap(x, padding_box[1] + top);
            let [r, b] = self.snap(x + BAR, padding_box[1] + top + length);
            let style = Style { background: Some(bar), radius: BAR / 2.0, ..Style::default() };
            shape_quad(&style, [l, t, r - l, b - t], self.scale, opacity, child_clip, self.quads);
        }
    }

    /// A logical point on the physical pixel grid.
    fn snap(&self, x: f32, y: f32) -> [f32; 2] {
        [(x * self.scale).round(), (y * self.scale).round()]
    }
}

fn shape_quad(style: &Style, rect: [f32; 4], scale: f32, opacity: f32, clip: [f32; 4], quads: &mut Vec<Quad>) {
    let border = style.border.filter(|(width, color)| *width > 0.0 && color.a > 0);
    let background = style.background.filter(|color| color.a > 0);
    if (background.is_none() && border.is_none()) || opacity <= 0.0 {
        return;
    }
    let fade = |color: crate::Color| color.linear_premultiplied().map(|c| c * opacity);
    let (border, border_color) = border.map_or((0.0, [0.0; 4]), |(w, c)| ((w * scale).round().max(1.0), fade(c)));
    quads.push(Quad {
        rect,
        color: background.map_or([0.0; 4], fade),
        border_color,
        clip,
        radius: style.radius * scale,
        border,
        kind: Quad::SHAPE,
        ..Quad::default()
    });
}

/// Every glyph of a shaped text, on whole pixels: hinted glyphs are drawn
/// where the hinting expected them.
fn glyph_quads(
    layout: &Layout<()>,
    origin: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
    atlas: &mut GlyphAtlas,
    quads: &mut Vec<Quad>,
) {
    if color[3] <= 0.0 {
        return;
    }
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
                    clip,
                    kind: Quad::GLYPH,
                    ..Quad::default()
                });
            }
        }
    }
}
