//! The distance-field atlas: each glyph once, at one size, as how far each
//! texel is from the glyph's edge. A UI drawn at many sizes draws its text
//! from it, as a panel in the world does, so the renderer can draw that
//! text sharp however near, far or slanted the panel is, where a glyph drawn
//! at one size blurs when it shows larger than that.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source},
    zeno::{Angle, Format, Transform},
};

use crate::{AtlasImage, atlas::Synthesis};

const SIZE: u32 = 1024;
/// Empty texels around each glyph, so neighbours do not bleed.
const PADDING: u32 = 1;
/// The size glyphs are made at, in the atlas's texels to the em.
pub(crate) const FIELD_SIZE: f32 = 48.0;
/// How far the distances reach either side of a glyph's edge, in texels.
pub(crate) const SPREAD: f32 = 8.0;
/// How many times finer each glyph is drawn, each way, for its distances to
/// be measured from.
const FINE: u32 = 2;
/// Farther than any distance measured, squared.
const FAR: f64 = 1e20;

/// A glyph in the atlas: its place there and its size, in texels, and
/// where its top left corner is from its pen at [`FIELD_SIZE`], left across
/// and up.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldGlyph {
    pub left: f32,
    pub top: f32,
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

/// A font, a glyph in it, its variation (hashed), and its synthesis: the
/// slant in degrees and whether it is emboldened. No size: one field draws
/// every size.
type Key = (u64, u32, u16, u64, i8, bool);

pub(crate) struct FieldAtlas {
    pixels: Vec<u8>,
    glyphs: HashMap<Key, Option<FieldGlyph>>,
    shelves: Vec<(u32, u32, u32)>,
    dirty: bool,
    /// The pixels as last handed out.
    snapshot: Option<Arc<AtlasImage>>,
    /// Set when the atlas filled up and was emptied.
    pub restarted: bool,
}

impl FieldAtlas {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; (SIZE * SIZE) as usize],
            glyphs: HashMap::new(),
            shelves: Vec::new(),
            dirty: true,
            snapshot: None,
            restarted: false,
        }
    }

    pub fn size(&self) -> u32 {
        SIZE
    }

    /// The glyph's field, made and packed on first use. `None` for glyphs
    /// with nothing to draw, such as spaces.
    pub fn glyph(
        &mut self,
        scaler: &mut ScaleContext,
        font: (&[u8], u32, u64),
        glyph: u16,
        coords: &[i16],
        synthesis: Synthesis,
    ) -> Option<FieldGlyph> {
        let skew = synthesis.skew.unwrap_or_default().round() as i8;
        let mut variation = DefaultHasher::new();
        coords.hash(&mut variation);
        let key = (font.2, font.1, glyph, variation.finish(), skew, synthesis.embolden);
        if let Some(found) = self.glyphs.get(&key) {
            return *found;
        }
        let made = field(scaler, font, glyph, coords, synthesis);
        let packed = made.and_then(|(left, top, width, height, texels)| {
            let (x, y) = self.pack(width, height, &texels)?;
            Some(FieldGlyph { left, top, width, height, x, y })
        });
        self.glyphs.insert(key, packed);
        packed
    }

    /// Copies texels into a free place, starting the atlas again when it is
    /// full, keeping only what is drawn from then on.
    fn pack(&mut self, width: u32, height: u32, texels: &[u8]) -> Option<(u32, u32)> {
        let (x, y) = self.place(width, height).or_else(|| {
            self.clear();
            self.place(width, height)
        })?;
        for row in 0..height {
            let from = (row * width) as usize;
            let to = ((y + row) * SIZE + x) as usize;
            self.pixels[to..to + width as usize].copy_from_slice(&texels[from..from + width as usize]);
        }
        self.dirty = true;
        Some((x, y))
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
    /// then a copy with the next version. `None` until a glyph is made.
    pub fn snapshot(&mut self) -> Option<Arc<AtlasImage>> {
        if std::mem::take(&mut self.dirty) && !self.glyphs.is_empty() {
            let version = self.snapshot.as_ref().map_or(1, |s| s.version + 1);
            let image = AtlasImage { width: SIZE, height: SIZE, pixels: self.pixels.clone(), version };
            self.snapshot = Some(Arc::new(image));
        }
        self.snapshot.clone()
    }
}

/// A glyph's field: where its corner is from its pen, its size in texels,
/// and each texel's distance from its edge, 128 on the edge, more inside,
/// and 0 or 255 at [`SPREAD`] texels and beyond. Drawn [`FINE`] times
/// finer, its overlapping outlines filled as one, measured there and
/// averaged down.
fn field(
    scaler: &mut ScaleContext,
    (data, index, _): (&[u8], u32, u64),
    glyph: u16,
    coords: &[i16],
    synthesis: Synthesis,
) -> Option<(f32, f32, u32, u32, Vec<u8>)> {
    let font = FontRef::from_index(data, index as usize)?;
    let size = FIELD_SIZE * FINE as f32;
    let mut scaler = scaler.builder(font).size(size).hint(false).normalized_coords(coords).build();
    let mut render = Render::new(&[Source::Outline]);
    render.format(Format::Alpha);
    if let Some(degrees) = synthesis.skew {
        render.transform(Some(Transform::skew(Angle::from_degrees(degrees), Angle::ZERO)));
    }
    if synthesis.embolden {
        render.embolden(size / 28.0);
    }
    let image = render.render(&mut scaler, glyph)?;
    let p = image.placement;
    if p.width == 0 || p.height == 0 || image.data.iter().all(|&coverage| coverage == 0) {
        return None;
    }
    // Room for the distances outside it, and a whole number of texels.
    let margin = SPREAD as u32 * FINE;
    let (width, height) = ((p.width + 2 * margin).div_ceil(FINE), (p.height + 2 * margin).div_ceil(FINE));
    let fine = [(width * FINE) as usize, (height * FINE) as usize];
    let mut coverage = vec![0.0f64; fine[0] * fine[1]];
    for (row, line) in image.data.chunks_exact(p.width as usize).enumerate() {
        let at = (row + margin as usize) * fine[0] + margin as usize;
        coverage[at..at + line.len()].iter_mut().zip(line).for_each(|(into, &c)| *into = f64::from(c) / 255.0);
    }
    let distances = signed_distances(&coverage, fine);
    let mut texels = vec![0u8; (width * height) as usize];
    let block = FINE as usize;
    for (at, texel) in texels.iter_mut().enumerate() {
        let (x, y) = (at % width as usize * block, at / width as usize * block);
        let fine_at = |(dx, dy): (usize, usize)| distances[(y + dy) * fine[0] + x + dx];
        let sum: f64 = (0..block).flat_map(|dy| (0..block).map(move |dx| (dx, dy))).map(fine_at).sum();
        let texels_in = sum / (block * block) as f64 / f64::from(FINE);
        *texel = ((0.5 + texels_in / (2.0 * f64::from(SPREAD))).clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    let left = (p.left - margin as i32) as f32 / FINE as f32;
    let top = (p.top + margin as i32) as f32 / FINE as f32;
    Some((left, top, width, height, texels))
}

/// How far each pixel is from the edge of what `coverage` covers, in
/// pixels, more inside, measured between pixels' middles: a covered pixel
/// beside an uncovered one is a pixel in, the uncovered one a pixel out,
/// and the edge halfway; a pixel half covered is on the edge.
fn signed_distances(coverage: &[f64], [width, height]: [usize; 2]) -> Vec<f64> {
    let mut to_inside = vec![FAR; coverage.len()];
    let mut to_outside = vec![0.0; coverage.len()];
    for (at, &c) in coverage.iter().enumerate() {
        if c >= 1.0 {
            (to_inside[at], to_outside[at]) = (0.0, FAR);
        } else if c > 0.0 {
            let between = 0.5 - c;
            to_inside[at] = if between > 0.0 { between * between } else { 0.0 };
            to_outside[at] = if between < 0.0 { between * between } else { 0.0 };
        }
    }
    squared_distances(&mut to_inside, width, height);
    squared_distances(&mut to_outside, width, height);
    to_outside.iter().zip(&to_inside).map(|(outside, inside)| outside.sqrt() - inside.sqrt()).collect()
}

/// Turns squared distances to features (0 there, [`FAR`] elsewhere) into
/// squared distances to the nearest, along each row and then each column.
fn squared_distances(grid: &mut [f64], width: usize, height: usize) {
    let longest = width.max(height);
    let (mut line, mut nearest) = (vec![0.0; longest], vec![0.0; longest]);
    let (mut v, mut z) = (vec![0usize; longest], vec![0.0; longest + 1]);
    for y in 0..height {
        line[..width].copy_from_slice(&grid[y * width..(y + 1) * width]);
        along(&line[..width], &mut nearest[..width], &mut v, &mut z);
        grid[y * width..(y + 1) * width].copy_from_slice(&nearest[..width]);
    }
    for x in 0..width {
        (0..height).for_each(|y| line[y] = grid[y * width + x]);
        along(&line[..height], &mut nearest[..height], &mut v, &mut z);
        (0..height).for_each(|y| grid[y * width + x] = nearest[y]);
    }
}

/// One line of squared distances: at each point, the least over every
/// point of the distance between them squared plus what is already there,
/// as the lower envelope of the parabolas rooted at each. `v` holds the
/// envelope's parabolas and `z` where each takes over.
fn along(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..f.len() {
        let qf = q as f64;
        let crossing = |r: usize| (f[q] - f[r] + qf * qf - (r * r) as f64) / (qf - r as f64) / 2.0;
        let mut s = crossing(v[k]);
        while s <= z[k] {
            k -= 1;
            s = crossing(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    let mut k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f64;
        while z[k + 1] < qf {
            k += 1;
        }
        let r = v[k] as f64;
        *out = (qf - r) * (qf - r) + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_distances_is_to_the_nearest_feature() {
        let f = [FAR, FAR, 0.0, FAR, FAR, FAR, 0.0, FAR];
        let mut d = [0.0; 8];
        let (mut v, mut z) = ([0; 8], [0.0; 9]);
        along(&f, &mut d, &mut v, &mut z);
        assert_eq!(d, [4.0, 1.0, 0.0, 1.0, 4.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_square_is_positive_inside_and_negative_outside_by_how_far() {
        let (width, height) = (12, 12);
        let covered = |at: usize| (4..8).contains(&(at % width)) && (4..8).contains(&(at / width));
        let coverage: Vec<f64> = (0..width * height).map(|at| if covered(at) { 1.0 } else { 0.0 }).collect();
        let d = signed_distances(&coverage, [width, height]);
        let at = |x: usize, y: usize| d[y * width + x];
        assert_eq!([at(1, 6), at(3, 6), at(4, 6), at(5, 6)], [-3.0, -1.0, 1.0, 2.0], "the edge halfway between 3 and 4");
        assert_eq!(at(2, 2), -(8.0f64.sqrt()), "past the corner, as far as the corner");
    }

    #[test]
    fn a_half_covered_pixel_is_on_the_edge() {
        let coverage = [0.0, 0.0, 0.5, 1.0, 1.0];
        let d = signed_distances(&coverage, [5, 1]);
        assert_eq!(d, vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
    }
}
