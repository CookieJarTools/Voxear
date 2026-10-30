//! Measuring text for layout, and walking the laid-out tree into quads.

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem};
use taffy::{AvailableSpace, Display, LayoutInput, LayoutOutput, Size, TaffyTree};

use crate::{Quad, Style, atlas::GlyphAtlas, tree::Element};

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
    /// elements it is in.
    pub fn node(&mut self, node: taffy::NodeId, parent: (f32, f32), opacity: f32) {
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
        let mut opacity = opacity;
        if let Some(element) = self.tree.get_node_context_mut(node) {
            opacity *= element.computed.opacity;
            shape_quad(&element.computed, [x0, y0, x1 - x0, y1 - y0], self.scale, opacity, self.quads);
            let color = element.text_style.color.linear_premultiplied().map(|c| c * opacity);
            if let Some(text) = &mut element.text
                && let Some((_, shaped)) = &mut text.shaped
            {
                // Broken again at the width it ended up, which the last
                // measure may not have been.
                shaped.break_all_lines(Some((inner * self.scale).ceil()));
                glyph_quads(shaped, origin, color, self.atlas, self.quads);
            }
        }
        if opacity <= 0.0 {
            return;
        }
        for child in self.tree.children(node).unwrap_or_default() {
            self.node(child, at, opacity);
        }
    }

    /// A logical point on the physical pixel grid.
    fn snap(&self, x: f32, y: f32) -> [f32; 2] {
        [(x * self.scale).round(), (y * self.scale).round()]
    }
}

fn shape_quad(style: &Style, rect: [f32; 4], scale: f32, opacity: f32, quads: &mut Vec<Quad>) {
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
        radius: style.radius * scale,
        border,
        kind: Quad::SHAPE,
        ..Quad::default()
    });
}

/// Every glyph of a shaped text, on whole pixels: hinted glyphs are drawn
/// where the hinting expected them.
fn glyph_quads(layout: &Layout<()>, origin: [f32; 2], color: [f32; 4], atlas: &mut GlyphAtlas, quads: &mut Vec<Quad>) {
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
                    kind: Quad::GLYPH,
                    ..Quad::default()
                });
            }
        }
    }
}
