//! The glyph atlas: rasterised glyphs packed into one coverage texture, in
//! shelves (rows as tall as their tallest glyph).

use std::collections::HashMap;

use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source},
    zeno::Format,
};

use std::sync::Arc;

use crate::AtlasImage;

const SIZE: u32 = 1024;
/// Empty pixels around each glyph, so bilinear lookups never bleed.
const PADDING: u32 = 1;

/// A glyph rasterised at one size: where it sits relative to its pen
/// position, and where it is in the atlas.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AtlasGlyph {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

/// A font, a glyph in it, and the size in 1/64 pixels.
type Key = (u64, u32, u16, u32);

pub(crate) struct GlyphAtlas {
    pixels: Vec<u8>,
    glyphs: HashMap<Key, Option<AtlasGlyph>>,
    shelves: Vec<(u32, u32, u32)>,
    scaler: ScaleContext,
    dirty: bool,
    /// The pixels as last handed out.
    snapshot: Option<Arc<AtlasImage>>,
    /// Set when the atlas filled up and was emptied.
    pub restarted: bool,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; (SIZE * SIZE) as usize],
            glyphs: HashMap::new(),
            shelves: Vec::new(),
            scaler: ScaleContext::new(),
            dirty: true,
            snapshot: None,
            restarted: false,
        }
    }

    pub fn size(&self) -> u32 {
        SIZE
    }

    /// The glyph at `size` pixels, rasterised and packed on first use.
    /// `None` for glyphs with no pixels, such as spaces.
    pub fn glyph(&mut self, font: (&[u8], u32, u64), glyph: u16, size: f32, coords: &[i16]) -> Option<AtlasGlyph> {
        let key = (font.2, font.1, glyph, (size * 64.0) as u32);
        if let Some(found) = self.glyphs.get(&key) {
            return *found;
        }
        let rasterised = self.rasterise(font, glyph, size, coords);
        let packed = rasterised.and_then(|(left, top, width, height, coverage)| {
            let (x, y) = self.place(width, height).or_else(|| {
                // Full: start again, keeping only what is drawn from now on.
                self.clear();
                self.place(width, height)
            })?;
            for row in 0..height {
                let from = (row * width) as usize;
                let to = ((y + row) * SIZE + x) as usize;
                self.pixels[to..to + width as usize].copy_from_slice(&coverage[from..from + width as usize]);
            }
            self.dirty = true;
            Some(AtlasGlyph { left, top, width, height, x, y })
        });
        self.glyphs.insert(key, packed);
        packed
    }

    fn rasterise(
        &mut self,
        (data, index, _): (&[u8], u32, u64),
        glyph: u16,
        size: f32,
        coords: &[i16],
    ) -> Option<(i32, i32, u32, u32, Vec<u8>)> {
        let font = FontRef::from_index(data, index as usize)?;
        let mut scaler = self.scaler.builder(font).size(size).hint(true).normalized_coords(coords).build();
        let image = Render::new(&[Source::Outline]).format(Format::Alpha).render(&mut scaler, glyph)?;
        let p = image.placement;
        (p.width > 0 && p.height > 0).then_some((p.left, p.top, p.width, p.height, image.data))
    }

    /// First fit along the shelves, opening a new one below when none fits.
    fn place(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = (width + PADDING, height + PADDING);
        if let Some(shelf) = self.shelves.iter_mut().find(|(_, shelf_h, used)| h <= *shelf_h && used + w <= SIZE) {
            let x = shelf.2;
            shelf.2 += w;
            return Some((x, shelf.0));
        }
        let top = self.shelves.last().map_or(0, |(y, shelf_h, _)| y + shelf_h);
        if top + h > SIZE || w > SIZE {
            return None;
        }
        self.shelves.push((top, h, w));
        Some((0, top))
    }

    fn clear(&mut self) {
        self.pixels.fill(0);
        self.glyphs.clear();
        self.shelves.clear();
        self.dirty = true;
        self.restarted = true;
    }

    /// The atlas as it is now: the same shared image until glyphs are added,
    /// then a copy with the next version. `None` until a glyph is drawn.
    pub fn snapshot(&mut self) -> Option<Arc<AtlasImage>> {
        if std::mem::take(&mut self.dirty) && !self.glyphs.is_empty() {
            let version = self.snapshot.as_ref().map_or(1, |s| s.version + 1);
            let image = AtlasImage { width: SIZE, height: SIZE, pixels: self.pixels.clone(), version };
            self.snapshot = Some(Arc::new(image));
        }
        self.snapshot.clone()
    }
}
