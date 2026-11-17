//! Text drawn on the CPU into a coverage picture, for pictures made before a
//! UI can draw: the splash a window shows while the renderer gets ready.

use parley::{FontContext, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};

use crate::{
    Family,
    atlas::{GlyphAtlas, Synthesis},
    fonts::FontStack,
};

/// Lays out and rasterises lines of text in Voxear's own faces.
pub struct TextRaster {
    fonts: FontContext,
    layouts: LayoutContext<u16>,
    atlas: GlyphAtlas,
}

/// A line of text as how much of each pixel it covers, row by row from the
/// top left, 0 to 255.
#[derive(Clone, Debug, Default)]
pub struct TextCoverage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Default for TextRaster {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRaster {
    pub fn new() -> Self {
        Self { fonts: FontContext::new(), layouts: LayoutContext::new(), atlas: GlyphAtlas::new() }
    }

    /// `text` on one line, its font `size` pixels, at `weight` (400 regular,
    /// 600 semibold).
    pub fn line(&mut self, text: &str, size: f32, weight: f32) -> TextCoverage {
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
        builder.push_default(StyleProperty::FontFamily(FontStack::default().families(Family::Sans)));
        let mut layout: Layout<u16> = builder.build(text);
        layout.break_all_lines(None);
        let (width, height) = (layout.width().ceil().max(0.0) as u32, layout.height().ceil().max(0.0) as u32);
        let mut pixels = vec![0u8; (width * height) as usize];
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
                let run = glyph_run.run();
                let font = run.font();
                let font = (font.data.data(), font.index, font.data.id());
                let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
                let synthesis = run.synthesis();
                let synthesis = Synthesis { skew: synthesis.skew(), embolden: synthesis.embolden() };
                for glyph in glyph_run.positioned_glyphs() {
                    let found = self.atlas.glyph(font, glyph.id as u16, run.font_size(), &coords, synthesis);
                    let Some(found) = found else { continue };
                    let left = glyph.x.round() as i32 + found.left;
                    let top = glyph.y.round() as i32 - found.top;
                    for row in 0..found.height {
                        for column in 0..found.width {
                            let (x, y) = (left + column as i32, top + row as i32);
                            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                                continue;
                            }
                            let at = (y as u32 * width + x as u32) as usize;
                            pixels[at] = pixels[at].max(self.atlas.coverage(found.x + column, found.y + row));
                        }
                    }
                }
            }
        }
        TextCoverage { width, height, pixels }
    }
}
