//! The glyph atlas: rasterised glyphs packed into one coverage texture, in
//! shelves (rows as tall as their tallest glyph).

use std::collections::HashMap;

use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source},
    zeno::{Angle, Cap, Fill, Format, Join, Mask, Stroke, Transform, Vector},
};

use std::sync::Arc;

use crate::AtlasImage;

const SIZE: u32 = 1024;
/// Empty pixels around each glyph, so bilinear lookups never bleed.
const PADDING: u32 = 1;
/// How finely an unhinted glyph is placed between pixels, each way.
pub(crate) const SUBPIXELS: u8 = 4;

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

/// A font, a glyph in it, the size in 1/64 pixels, its synthesis (the
/// slant in degrees and whether it is emboldened), and how it is placed:
/// 0 hinted on a whole pixel, or one more than its [`Subpixel`] numbered
/// across then down, unhinted.
type Key = (u64, u32, u16, u32, i8, bool, u8);

/// Where an unhinted glyph sits between pixels, across and down, in
/// [`SUBPIXELS`] of a pixel.
pub(crate) type Subpixel = [u8; 2];

/// What a font lacks and is faked for: italics slanted, bold thickened,
/// as parley asks when the family has no such face.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Synthesis {
    pub skew: Option<f32>,
    pub embolden: bool,
}
/// An icon's name, its size in pixels, and its stroke in 1/64 pixels.
type IconKey = (&'static str, u32, u32);

pub(crate) struct GlyphAtlas {
    pixels: Vec<u8>,
    glyphs: HashMap<Key, Option<AtlasGlyph>>,
    icons: HashMap<IconKey, Option<AtlasGlyph>>,
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
            icons: HashMap::new(),
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

    /// The glyph at `size` pixels, rasterised and packed on first use:
    /// hinted to sit on a whole pixel, or with `between` unhinted and that
    /// far between pixels, for a UI drawn at many sizes whose text should
    /// land in the same place at each. `None` for glyphs with no pixels,
    /// such as spaces.
    pub fn glyph(
        &mut self,
        font: (&[u8], u32, u64),
        glyph: u16,
        (size, between): (f32, Option<Subpixel>),
        coords: &[i16],
        synthesis: Synthesis,
    ) -> Option<AtlasGlyph> {
        let skew = synthesis.skew.unwrap_or_default().round() as i8;
        let place = between.map_or(0, |[x, y]| 1 + x + y * SUBPIXELS);
        let key = (font.2, font.1, glyph, (size * 64.0) as u32, skew, synthesis.embolden, place);
        if let Some(found) = self.glyphs.get(&key) {
            return *found;
        }
        let rasterised = self.rasterise(font, glyph, (size, between), coords, synthesis);
        let packed = rasterised.and_then(|(left, top, width, height, coverage)| {
            let (x, y) = self.pack(width, height, &coverage)?;
            Some(AtlasGlyph { left, top, width, height, x, y })
        });
        self.glyphs.insert(key, packed);
        packed
    }

    /// The icon `name` drawn `px` pixels square, its lines `stroke` pixels
    /// wide, rasterised and packed on first use. `None` for no such icon.
    pub fn icon(&mut self, name: &str, px: u32, stroke: f32) -> Option<AtlasGlyph> {
        let (name, parts) = crate::icons::parts(name)?;
        let key = (name, px, (stroke * 64.0) as u32);
        if let Some(found) = self.icons.get(&key) {
            return *found;
        }
        let scale = px as f32 / crate::icons::GRID;
        let mut coverage = vec![0u8; (px * px) as usize];
        for &(path, filled) in parts {
            let mut mask = Mask::new(path);
            if filled {
                mask.style(Fill::NonZero);
            } else {
                let mut line = Stroke::new(stroke);
                line.cap(Cap::Round).join(Join::Round).scale(false);
                mask.style(line);
            }
            let (part, _) = mask.transform(Some(Transform::scale(scale, scale))).size(px, px).render();
            coverage.iter_mut().zip(part).for_each(|(into, from)| *into = (*into).max(from));
        }
        let packed = (px > 0).then(|| self.pack(px, px, &coverage)).flatten().map(|(x, y)| AtlasGlyph {
            left: 0,
            top: 0,
            width: px,
            height: px,
            x,
            y,
        });
        self.icons.insert(key, packed);
        packed
    }

    /// Copies coverage into a free place, starting the atlas again when it
    /// is full, keeping only what is drawn from then on.
    fn pack(&mut self, width: u32, height: u32, coverage: &[u8]) -> Option<(u32, u32)> {
        let (x, y) = self.place(width, height).or_else(|| {
            self.clear();
            self.place(width, height)
        })?;
        for row in 0..height {
            let from = (row * width) as usize;
            let to = ((y + row) * SIZE + x) as usize;
            self.pixels[to..to + width as usize].copy_from_slice(&coverage[from..from + width as usize]);
        }
        self.dirty = true;
        Some((x, y))
    }

    fn rasterise(
        &mut self,
        (data, index, _): (&[u8], u32, u64),
        glyph: u16,
        (size, between): (f32, Option<Subpixel>),
        coords: &[i16],
        synthesis: Synthesis,
    ) -> Option<(i32, i32, u32, u32, Vec<u8>)> {
        let font = FontRef::from_index(data, index as usize)?;
        let hint = between.is_none();
        let mut scaler = self.scaler.builder(font).size(size).hint(hint).normalized_coords(coords).build();
        let mut render = Render::new(&[Source::Outline]);
        render.format(Format::Alpha);
        if let Some([x, y]) = between {
            // The outline's own space points up, the screen's down.
            let step = f32::from(SUBPIXELS);
            render.offset(Vector::new(f32::from(x) / step, -f32::from(y) / step));
        }
        if let Some(degrees) = synthesis.skew {
            render.transform(Some(Transform::skew(Angle::from_degrees(degrees), Angle::ZERO)));
        }
        if synthesis.embolden {
            render.embolden(size / 28.0);
        }
        let image = render.render(&mut scaler, glyph)?;
        let p = image.placement;
        // A space placed between pixels comes back a pixel of nothing.
        let inked = image.data.iter().any(|&coverage| coverage > 0);
        (p.width > 0 && p.height > 0 && inked).then_some((p.left, p.top, p.width, p.height, image.data))
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
        self.icons.clear();
        self.shelves.clear();
        self.dirty = true;
        self.restarted = true;
    }

    /// The atlas as it is now: the same shared image until glyphs are added,
    /// then a copy with the next version. `None` until a glyph is drawn.
    pub fn snapshot(&mut self) -> Option<Arc<AtlasImage>> {
        if std::mem::take(&mut self.dirty) && !(self.glyphs.is_empty() && self.icons.is_empty()) {
            let version = self.snapshot.as_ref().map_or(1, |s| s.version + 1);
            let image = AtlasImage { width: SIZE, height: SIZE, pixels: self.pixels.clone(), version };
            self.snapshot = Some(Arc::new(image));
        }
        self.snapshot.clone()
    }
}
