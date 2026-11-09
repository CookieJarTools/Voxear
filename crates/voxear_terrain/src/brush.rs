//! Sculpting brushes: each dab changes a Sculpt layer's raster round a
//! point, most at its middle and easing to nothing at its edge, by how
//! the ground stands there now (the layers below, then the raster).
//!
//! A brush paints as long as it is held, a dab each frame, so how far a dab
//! moves the ground scales with how long it was held (`amount`, in seconds):
//! the same stroke does the same at any frame rate.

use crate::{
    composite::{Compositor, Pictures, Sources},
    file::TerrainFile,
    heights::{TILE_QUADS, TILE_SAMPLES},
    layers::{Area, smoothstep},
    noise::{self, NoiseStyle},
    raster::Rasters,
};

/// What a brush does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BrushKind {
    /// Raises the ground, or lowers it held inverted (Shift).
    #[default]
    Raise,
    /// Evens out bumps.
    Smooth,
    /// Brings the ground to the height under the brush where the stroke began.
    Flatten,
    /// Brings the ground to the brush's height.
    SetHeight,
    /// Cuts the ground into steps.
    Terrace,
    /// Roughens the ground with noise.
    Noise,
    /// Cuts holes in the ground, or fills them held inverted (Shift): a
    /// way into a cave or a building below, where nothing is drawn and
    /// nothing stands.
    Holes,
    /// Digs the ground away under the brush, up and down too, into a Dig
    /// layer (see `dig`), or fills it back held inverted (Shift).
    Dig,
    /// Heaps ground up under the brush into a Dig layer, or digs it away
    /// held inverted.
    Fill,
}

impl BrushKind {
    pub const ALL: [BrushKind; 9] = [
        BrushKind::Raise,
        BrushKind::Smooth,
        BrushKind::Flatten,
        BrushKind::SetHeight,
        BrushKind::Terrace,
        BrushKind::Noise,
        BrushKind::Holes,
        BrushKind::Dig,
        BrushKind::Fill,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BrushKind::Raise => "Raise or Lower",
            BrushKind::Smooth => "Smooth",
            BrushKind::Flatten => "Flatten",
            BrushKind::SetHeight => "Set Height",
            BrushKind::Terrace => "Terrace",
            BrushKind::Noise => "Noise",
            BrushKind::Holes => "Holes",
            BrushKind::Dig => "Dig",
            BrushKind::Fill => "Fill",
        }
    }

    /// Whether it paints into a Sculpt layer: Holes cuts the terrain's own
    /// raster of holes, whatever layer is picked, and Dig and Fill go in a
    /// Dig layer.
    pub fn sculpts(self) -> bool {
        !matches!(self, BrushKind::Holes | BrushKind::Dig | BrushKind::Fill)
    }

    /// Whether it digs or fills, into a Dig layer.
    pub fn digs(self) -> bool {
        matches!(self, BrushKind::Dig | BrushKind::Fill)
    }
}

/// A brush: what it does, how wide (`radius`, in metres) and how strong
/// (0 to 1), the height Set Height brings the ground to, the steps Terrace
/// cuts, and the size of Noise's bumps, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    pub kind: BrushKind,
    pub radius: f32,
    pub strength: f32,
    pub height: f32,
    pub step: f32,
    pub noise_size: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self { kind: BrushKind::Raise, radius: 8.0, strength: 0.5, height: 0.0, step: 2.0, noise_size: 4.0 }
    }
}

/// How fast Raise moves the ground at full strength, in metres a second.
const RAISE_SPEED: f32 = 6.0;

/// How fast the brushes that bring the ground to a height get there at full
/// strength: the share of the way they go in a second.
const PULL: f32 = 8.0;

/// One dab of `brush` on raster `raster` at `at` (metres from the corner),
/// held for `amount` seconds, `inverted` (Shift) to lower rather than raise.
/// `flatten_to` is the height Flatten brings the ground to. The compositor
/// must be editing the Sculpt layer whose raster this is; `pictures` are
/// what the layers below read. Returns the area it changed.
pub fn dab(
    brush: &Brush,
    at: [f32; 2],
    amount: f32,
    inverted: bool,
    flatten_to: f32,
    (file, compositor, pictures): (&TerrainFile, &Compositor, Pictures),
    (rasters, raster): (&mut Rasters, u32),
) -> Area {
    let spacing = file.spacing();
    let radius = brush.radius.max(spacing);
    let quads = i64::from(file.tiles() * TILE_QUADS);
    // The samples the dab reaches, and one more each way for Smooth to look at.
    let first = |v: f32| (((v - radius) / spacing).floor() as i64 - 2).clamp(0, quads);
    let last = |v: f32| (((v + radius) / spacing).ceil() as i64 + 2).clamp(0, quads);
    let (x0, x1, z0, z1) = (first(at[0]), last(at[0]), first(at[1]), last(at[1]));
    let width = (x1 - x0 + 1) as usize;
    let current: Vec<f32> = {
        let sources = Sources { pictures, rasters };
        let mut current = Vec::with_capacity(width * (z1 - z0 + 1) as usize);
        for z in z0..=z1 {
            for x in x0..=x1 {
                current.push(compositor.below_at(file, &sources, x, z) + rasters.sample(raster, x, z));
            }
        }
        current
    };
    let here = |x: i64, z: i64| current[((z - z0) as usize) * width + (x - x0) as usize];
    let strength = brush.strength.clamp(0.0, 1.0);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let (px, pz) = (x as f32 * spacing, z as f32 * spacing);
            let distance = ((px - at[0]).powi(2) + (pz - at[1]).powi(2)).sqrt();
            let falloff = 1.0 - smoothstep(0.0, 1.0, distance / radius);
            if falloff <= 0.0 {
                continue;
            }
            let height = here(x, z);
            let pull = (strength * falloff * amount * PULL).min(1.0);
            let change = match brush.kind {
                BrushKind::Raise => {
                    let way = if inverted { -1.0 } else { 1.0 };
                    way * strength * falloff * amount * RAISE_SPEED
                }
                BrushKind::Smooth => {
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for dz in -2..=2 {
                        for dx in -2..=2 {
                            let (sx, sz) = ((x + dx).clamp(x0, x1), (z + dz).clamp(z0, z1));
                            sum += here(sx, sz);
                            count += 1.0;
                        }
                    }
                    (sum / count - height) * pull
                }
                BrushKind::Flatten => (flatten_to - height) * pull,
                BrushKind::SetHeight => (brush.height - height) * pull,
                BrushKind::Terrace => {
                    let step = brush.step.max(0.01);
                    ((height / step).round() * step - height) * pull
                }
                BrushKind::Noise => {
                    let size = brush.noise_size.max(0.1);
                    let bump = noise::fractal(NoiseStyle::Fractal, px / size, pz / size, 3, 0x5EED) * 2.0 - 1.0;
                    let way = if inverted { -1.0 } else { 1.0 };
                    way * bump * strength * falloff * amount * RAISE_SPEED * 0.5
                }
                // Cut by `cut`, into the raster of holes.
                BrushKind::Holes | BrushKind::Dig | BrushKind::Fill => 0.0,
            };
            if change != 0.0 {
                add(rasters, raster, x, z, change);
            }
        }
    }
    [x0 as f32 * spacing, z0 as f32 * spacing, x1 as f32 * spacing, z1 as f32 * spacing]
}

/// Cuts holes in the ground within `radius` metres of `at` (metres from
/// the corner), or with `fill` (Shift) fills them, in the terrain's raster
/// of holes, which must have a number (`TerrainFile::holes`). Each sample
/// is wholly a hole's or not, so the edge is as sharp as the samples are
/// fine. Returns the area it changed.
pub fn cut(file: &TerrainFile, rasters: &mut Rasters, at: [f32; 2], radius: f32, fill: bool) -> Area {
    let spacing = file.spacing();
    let radius = radius.max(spacing * 0.5);
    let quads = i64::from(file.tiles() * TILE_QUADS);
    let first = |v: f32| (((v - radius) / spacing).floor() as i64).clamp(0, quads);
    let last = |v: f32| (((v + radius) / spacing).ceil() as i64).clamp(0, quads);
    let (x0, x1, z0, z1) = (first(at[0]), last(at[0]), first(at[1]), last(at[1]));
    let raster = file.holes;
    if raster != 0 {
        let goal = if fill { 0.0 } else { 1.0 };
        for z in z0..=z1 {
            for x in x0..=x1 {
                let distance = ((x as f32 * spacing - at[0]).powi(2) + (z as f32 * spacing - at[1]).powi(2)).sqrt();
                let change = goal - rasters.sample(raster, x, z);
                if distance <= radius && change != 0.0 {
                    add(rasters, raster, x, z, change);
                }
            }
        }
    }
    [x0 as f32 * spacing, z0 as f32 * spacing, x1 as f32 * spacing, z1 as f32 * spacing]
}

/// Adds `change` to sample `(x, z)` of a raster, in every tile that holds
/// it: a sample on a tile's edge is in the tile beside it too.
pub(crate) fn add(rasters: &mut Rasters, raster: u32, x: i64, z: i64, change: f32) {
    let quads = i64::from(TILE_QUADS);
    let tiles_along = |v: i64| {
        let tile = v / quads;
        // An edge sample belongs to the tile before as its last, and to its own as its first.
        if v % quads == 0 && tile > 0 { vec![tile - 1, tile] } else { vec![tile] }
    };
    for tz in tiles_along(z) {
        for tx in tiles_along(x) {
            let (sx, sz) = ((x - tx * quads) as u32, (z - tz * quads) as u32);
            if sx >= TILE_SAMPLES || sz >= TILE_SAMPLES || tx < 0 || tz < 0 {
                continue;
            }
            rasters.tile_mut(raster, tx as u32, tz as u32)[(sz * TILE_SAMPLES + sx) as usize] += change;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        composite::Dirty,
        layers::{Layer, LayerKind},
    };
    use voxear_core::math::Vec3;

    fn none(_: &str) -> Option<std::sync::Arc<crate::import::Greys>> {
        None
    }

    fn sculpted() -> TerrainFile {
        let slope = Layer::new(LayerKind::Spline {
            points: vec![[0.0, 0.0, 256.0], [512.0, 20.0, 256.0]],
            width: 2000.0,
            falloff: 1.0,
            dig: None,
        });
        TerrainFile {
            size: 512.0,
            resolution: 513,
            layers: vec![slope, Layer::new(LayerKind::Sculpt { raster: 1 })],
            ..TerrainFile::default()
        }
    }

    #[test]
    fn raise_lifts_the_ground_most_in_the_middle_and_shift_lowers_it() {
        let file = sculpted();
        let mut compositor = Compositor::default();
        compositor.edit(&file, Some(1));
        let mut rasters = Rasters::default();
        let brush = Brush { radius: 10.0, strength: 1.0, ..Brush::default() };
        // A dab on the line between two tiles, which share their edge.
        let at = [256.0, 100.0];
        let area = dab(&brush, at, 0.5, false, 0.0, (&file, &compositor, &none), (&mut rasters, 1));
        assert!(area[0] < 246.0 && area[2] > 266.0);
        let middle = rasters.sample(1, 256, 100);
        assert!((middle - 3.0).abs() < 1e-3, "{middle}");
        assert!(rasters.sample(1, 260, 100) < middle && rasters.sample(1, 260, 100) > 0.0);
        assert_eq!(rasters.sample(1, 280, 100), 0.0, "nothing past its edge");
        assert_eq!(
            rasters.tile(1, 0, 0).unwrap()[(100 * TILE_SAMPLES + 256) as usize],
            middle,
            "the tile before has the edge too"
        );
        dab(&brush, at, 0.5, true, 0.0, (&file, &compositor, &none), (&mut rasters, 1));
        assert!(rasters.sample(1, 256, 100).abs() < 1e-4, "lowered back");
    }

    #[test]
    fn flatten_and_set_height_bring_the_ground_to_a_height_and_smooth_evens_a_bump() {
        let file = sculpted();
        let mut compositor = Compositor::default();
        compositor.edit(&file, Some(1));
        let mut rasters = Rasters::default();
        let pictures = &none;
        let at = [100.0, 100.0];
        let set = Brush { kind: BrushKind::SetHeight, height: 12.0, radius: 6.0, strength: 1.0, ..Brush::default() };
        for _ in 0..20 {
            dab(&set, at, 0.1, false, 0.0, (&file, &compositor, pictures), (&mut rasters, 1));
        }
        let sources = Sources { pictures, rasters: &rasters };
        let ground = compositor.composite(&file, &sources, None, &Dirty::All);
        assert!((ground.height_at(100.0, 100.0) - 12.0).abs() < 0.05, "{}", ground.height_at(100.0, 100.0));
        // The slope below was about 3.9 m there; Flatten to 2 m pulls it down too.
        let flatten = Brush { kind: BrushKind::Flatten, radius: 6.0, strength: 1.0, ..Brush::default() };
        for _ in 0..20 {
            dab(&flatten, at, 0.1, false, 2.0, (&file, &compositor, pictures), (&mut rasters, 1));
        }
        let sources = Sources { pictures, rasters: &rasters };
        let ground = compositor.composite(&file, &sources, None, &Dirty::All);
        assert!((ground.height_at(100.0, 100.0) - 2.0).abs() < 0.05);
        // A spike smoothed away.
        let mut spiky = Rasters::default();
        spiky.tile_mut(1, 0, 0)[(200 * TILE_SAMPLES + 200) as usize] = 10.0;
        let smooth = Brush { kind: BrushKind::Smooth, radius: 4.0, strength: 1.0, ..Brush::default() };
        dab(&smooth, [200.0, 200.0], 0.2, false, 0.0, (&file, &compositor, pictures), (&mut spiky, 1));
        assert!(spiky.sample(1, 200, 200) < 5.0);
    }

    #[test]
    fn holes_cut_the_ground_away_for_rays_and_what_stands_and_fill_again() {
        let mut file = sculpted();
        file.holes = 7;
        let mut rasters = Rasters::default();
        // On the line between two tiles, which share it.
        cut(&file, &mut rasters, [256.0, 300.0], 3.0, false);
        let compositor = Compositor::default();
        let sources = Sources { pictures: &none, rasters: &rasters };
        let ground = compositor.composite(&file, &sources, None, &Dirty::All);
        assert!(ground.is_hole(256, 300) && ground.is_hole(254, 300) && !ground.is_hole(260, 300));
        assert!(ground.tile(0, 1).is_hole(256, 44) && ground.tile(1, 1).is_hole(0, 44), "both tiles hold the edge");
        assert!(ground.cut_at(256.5, 300.5) && !ground.cut_at(250.0, 300.0));
        let down = crate::ray::raycast(&ground, Vec3::new(256.2, 50.0, 300.7), Vec3::NEG_Y, 100.0);
        assert!(down.is_none(), "through the hole");
        assert!(crate::ray::raycast(&ground, Vec3::new(240.0, 50.0, 300.0), Vec3::NEG_Y, 100.0).is_some());
        cut(&file, &mut rasters, [256.0, 300.0], 3.0, true);
        let sources = Sources { pictures: &none, rasters: &rasters };
        let filled = compositor.composite(&file, &sources, Some(&ground), &Dirty::Areas(vec![[250.0, 295.0, 262.0, 305.0]]));
        assert!(!filled.has_holes());
    }
}
