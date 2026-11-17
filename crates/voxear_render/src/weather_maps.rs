//! What the weather has left lying by place, and what keeps it off: three
//! small grids round the viewer, which the engine fills as the game runs
//! (`voxear_engine::weather_maps`) and every frame reads.
//!
//! - **Lying:** how wet the ground is, how much snow lies and how much has
//!   fallen lately, how much of the sea is frozen, and how warm the air is
//!   at a height, in cells tens of metres across. A surface's own height
//!   picks how much of the snow it keeps: one storm leaves the peak white
//!   and the valley under it wet (`weatherSurface` in
//!   `shaders/shading.slang`).
//! - **Cover:** how high the highest thing that stops rain is over each
//!   half metre near the viewer: a roof, a tree, an overhang, the ground.
//!   What is under it stays dry and free of snow, and rain and snow kept out
//!   from under roofs (a Particle System's `sheltered`) end on it.
//! - **Sea floor:** how deep a world of blocks' sea is near the viewer, so
//!   its swell calms in the shallows and breaks on the shore
//!   (`seaSunk` in `shaders/water.slang`, and [`crate::sea::SeaSampler`]).
//! - **Far ice:** how much of the sea is frozen over far off, in cells
//!   hundreds of metres across, out to the far view: past the lying grid a
//!   frozen sea goes on frozen, the lying grid's ice blended into it over
//!   its last cells, so no line shows where the lying grid ends.
//!
//! Each is in the world's space, its corner moved as the world's middle
//! moves. Without one the frame goes by the level's weather alone, as
//! before: the same everywhere, under roofs too.

use std::sync::Arc;

use voxear_core::math::{Vec2, Vec3};

/// How many cells each grid has along a side.
pub const LYING_SIDE: usize = 32;
pub const COVER_SIDE: usize = 96;
pub const FLOOR_SIDE: usize = 64;
pub const ICE_SIDE: usize = 32;
/// How wide a cell of each is, in metres.
pub const LYING_CELL: f32 = 32.0;
pub const COVER_CELL: f32 = 0.5;
pub const FLOOR_CELL: f32 = 4.0;
pub const ICE_CELL: f32 = 512.0;
/// Over how many of its last cells the lying grid's ice is blended into the
/// far ice's.
pub const ICE_BLEND_CELLS: f32 = 2.0;
/// The deepest a sea floor's grid tells of, in metres: deeper is as deep.
pub const FLOOR_REACH: f32 = 16.0;
/// A cell with nothing over it.
pub const OPEN: f32 = -1.0e30;
/// How much colder the air is a metre higher, in °C.
pub const LAPSE: f32 = 0.0065;

/// The words the shaders read before the grids: each grid's side (0 for
/// none), cell, corner along x and z, and where its cells start (see
/// [`WeatherMaps::words`]).
pub const HEADER: usize = 20;
/// Every word of the frame's maps, at the most.
pub const WORDS: usize =
    HEADER + LYING_SIDE * LYING_SIDE * 3 + COVER_SIDE * COVER_SIDE + FLOOR_SIDE * FLOOR_SIDE + ICE_SIDE * ICE_SIDE;

/// What the weather has left lying in a cell: each amount from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lying {
    /// How wet the ground is from the rain.
    pub wet: f32,
    /// How much snow lies at the cell's own height.
    pub snow: f32,
    /// How much snow or rain has fallen lately: what lies, as snow, on what
    /// is cold enough to keep it, higher up in the cell.
    pub fallen: f32,
    /// How much of the sea is frozen over.
    pub ice: f32,
    /// How warm the open air is at `height`, in °C, and that height, in
    /// the world.
    pub celsius: f32,
    pub height: f32,
}

impl Lying {
    /// How much snow lies at `height`, in the world: the cell's own at its
    /// height, more of what fell where it is colder, none where it is warm,
    /// and snow that never melts where it is bitter.
    pub fn snow_at(&self, height: f32) -> f32 {
        let celsius = self.celsius + LAPSE * (self.height - height);
        let cold = 1.0 - smooth(0.0, 2.0, celsius);
        let kept = self.snow.max(self.fallen * cold) * (1.0 - smooth(1.5, 4.0, celsius));
        kept.max(lasting_snow(celsius)).clamp(0.0, 1.0)
    }

    /// How wet a surface at `height` is: the cell's wetness, less where it
    /// is so cold that what fell lies as snow.
    pub fn wet_at(&self, height: f32) -> f32 {
        let celsius = self.celsius + LAPSE * (self.height - height);
        (self.wet * smooth(-1.0, 1.0, celsius)).clamp(0.0, 1.0)
    }
}

/// How much snow lies all year where the open air is `celsius` warm: none
/// above 4 °C below freezing, nearly all of it 9 °C below.
pub fn lasting_snow(celsius: f32) -> f32 {
    0.9 * (1.0 - smooth(-9.0, -4.0, celsius))
}

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A square of cells over part of the world: its corner along x and z, in
/// the world, how wide a cell is, how many along a side, and the cells, a
/// row of x at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    pub corner: Vec2,
    pub cell: f32,
    pub side: usize,
    pub cells: Arc<[T]>,
}

impl<T: Copy> Grid<T> {
    /// The cell `x`, `z` in the world is in; none outside the grid.
    pub fn nearest(&self, x: f32, z: f32) -> Option<T> {
        let at = (Vec2::new(x, z) - self.corner) / self.cell.max(1e-6);
        let (i, j) = (at.x.floor(), at.y.floor());
        let inside = (0.0..self.side as f32).contains(&i) && (0.0..self.side as f32).contains(&j);
        inside.then(|| self.cells[j as usize * self.side + i as usize])
    }

    /// The four cells round `x`, `z` and how far it is between them along x
    /// and z, each kept within the grid; none outside it.
    fn round(&self, x: f32, z: f32) -> Option<([T; 4], Vec2)> {
        let at = (Vec2::new(x, z) - self.corner) / self.cell.max(1e-6);
        let side = self.side as f32;
        if !(0.0..side).contains(&at.x) || !(0.0..side).contains(&at.y) {
            return None;
        }
        let at = at - 0.5;
        let low = at.floor();
        let part = at - low;
        let last = self.side as i64 - 1;
        let cell = |i: f32, j: f32| {
            let (i, j) = ((i as i64).clamp(0, last) as usize, (j as i64).clamp(0, last) as usize);
            self.cells[j * self.side + i]
        };
        Some((
            [cell(low.x, low.y), cell(low.x + 1.0, low.y), cell(low.x, low.y + 1.0), cell(low.x + 1.0, low.y + 1.0)],
            part,
        ))
    }

    /// A number from the cells round `x`, `z`, blended between them as far
    /// as the point is from each; none outside the grid.
    pub fn blended(&self, x: f32, z: f32, value: impl Fn(T) -> f32) -> Option<f32> {
        let ([a, b, c, d], part) = self.round(x, z)?;
        let along = |a: T, b: T| value(a) + (value(b) - value(a)) * part.x;
        let (low, high) = (along(a, b), along(c, d));
        Some(low + (high - low) * part.y)
    }

    /// As [`Grid::blended`], and past the grid's edge as at its edge.
    pub fn blended_on(&self, x: f32, z: f32, value: impl Fn(T) -> f32) -> f32 {
        let last = (self.side as f32 * self.cell).max(0.0) * (1.0 - 1e-6);
        let (x, z) = ((x - self.corner.x).clamp(0.0, last), (z - self.corner.y).clamp(0.0, last));
        self.blended(self.corner.x + x, self.corner.y + z, value).unwrap_or(0.0)
    }

    /// How far inside the grid `x`, `z` is, from 0 at its edge and beyond to
    /// 1 `cells` cells in.
    pub fn inside(&self, x: f32, z: f32, cells: f32) -> f32 {
        let at = (Vec2::new(x, z) - self.corner) / self.cell.max(1e-6);
        let side = self.side as f32;
        (at.x.min(at.y).min(side - at.x).min(side - at.y) / cells.max(1e-6)).clamp(0.0, 1.0)
    }
}

/// The weather's grids this frame, a resource.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeatherMaps {
    pub lying: Option<Grid<Lying>>,
    pub cover: Option<Grid<f32>>,
    pub floor: Option<Grid<f32>>,
    /// How much of the sea is frozen over far off, past the lying grid.
    pub ice: Option<Grid<f32>>,
}

impl WeatherMaps {
    /// What lies at `at`, in the world, blended between cells: snow by the
    /// height there. None outside the lying grid.
    pub fn lying_at(&self, at: Vec3) -> Option<(f32, f32, f32)> {
        let lying = self.lying.as_ref()?;
        let wet = lying.blended(at.x, at.z, |cell| cell.wet_at(at.y))?;
        let snow = lying.blended(at.x, at.z, |cell| cell.snow_at(at.y))?;
        let ice = lying.blended(at.x, at.z, |cell| cell.ice)?;
        Some((wet, snow, ice))
    }

    /// How much of the sea is frozen over at `x`, `z`, from 0 to 1, blended
    /// between cells: the lying grid's, and past it the far ice's, into
    /// which the lying grid's is blended over its last
    /// [`ICE_BLEND_CELLS`] cells. None outside the lying grid where there is
    /// no far ice.
    pub fn ice_at(&self, x: f32, z: f32) -> f32 {
        let near = self.lying.as_ref().and_then(|lying| lying.blended(x, z, |cell| cell.ice));
        let Some(far) = self.ice.as_ref().map(|ice| ice.blended_on(x, z, |ice| ice)) else {
            return near.unwrap_or(0.0);
        };
        match (near, self.lying.as_ref()) {
            (Some(near), Some(lying)) => far + (near - far) * lying.inside(x, z, ICE_BLEND_CELLS),
            _ => far,
        }
    }

    /// How high the highest thing over `x`, `z` is that stops rain, in the
    /// world; [`OPEN`] where nothing is, or the cover grid does not reach.
    pub fn cover_at(&self, x: f32, z: f32) -> f32 {
        self.cover.as_ref().and_then(|cover| cover.nearest(x, z)).unwrap_or(OPEN)
    }

    /// Whether something is over `at` that keeps the rain off: more than a
    /// little over it, so the top of a roof or the ground itself is out in
    /// the open.
    pub fn covered(&self, at: Vec3) -> bool {
        self.cover_at(at.x, at.z) > at.y + 0.2
    }

    /// How deep the sea is at `x`, `z`, in metres, blended between cells:
    /// nothing on land, at most [`FLOOR_REACH`]; none outside the grid.
    pub fn sea_depth(&self, x: f32, z: f32) -> Option<f32> {
        self.floor.as_ref()?.blended(x, z, |depth| depth)
    }

    /// The grids as the shaders read them (`weatherMaps` in
    /// `shaders/common.slang`): the lying, cover and sea floor grids' sides,
    /// cells and corners four words each, where each grid's cells start,
    /// and the far ice's four words after those; then the cells.
    pub fn words(&self) -> Vec<u32> {
        let mut words = vec![0u32; HEADER];
        let mut head = |slot: usize, side: usize, cell: f32, corner: Vec2, start: usize| {
            let at = if slot < 3 { slot * 4 } else { 16 };
            words[at] = side as u32;
            words[at + 1] = cell.to_bits();
            words[at + 2] = corner.x.to_bits();
            words[at + 3] = corner.y.to_bits();
            words[12 + slot] = start as u32;
        };
        let mut start = HEADER;
        let lying =
            self.lying.as_ref().filter(|grid| grid.side <= LYING_SIDE && grid.cells.len() >= grid.side * grid.side);
        let cover =
            self.cover.as_ref().filter(|grid| grid.side <= COVER_SIDE && grid.cells.len() >= grid.side * grid.side);
        let floor =
            self.floor.as_ref().filter(|grid| grid.side <= FLOOR_SIDE && grid.cells.len() >= grid.side * grid.side);
        let ice = self.ice.as_ref().filter(|grid| grid.side <= ICE_SIDE && grid.cells.len() >= grid.side * grid.side);
        if let Some(grid) = lying {
            head(0, grid.side, grid.cell, grid.corner, start);
            start += grid.side * grid.side * 3;
        }
        if let Some(grid) = cover {
            head(1, grid.side, grid.cell, grid.corner, start);
            start += grid.side * grid.side;
        }
        if let Some(grid) = floor {
            head(2, grid.side, grid.cell, grid.corner, start);
            start += grid.side * grid.side;
        }
        if let Some(grid) = ice {
            head(3, grid.side, grid.cell, grid.corner, start);
        }
        let unit = |amount: f32| (amount.clamp(0.0, 1.0) * 255.0).round() as u32;
        if let Some(grid) = lying {
            for cell in grid.cells.iter().take(grid.side * grid.side) {
                words.push(unit(cell.wet) | unit(cell.snow) << 8 | unit(cell.fallen) << 16 | unit(cell.ice) << 24);
                words.extend([cell.celsius.to_bits(), cell.height.to_bits()]);
            }
        }
        if let Some(grid) = cover {
            words.extend(grid.cells.iter().take(grid.side * grid.side).map(|height| height.to_bits()));
        }
        if let Some(grid) = floor {
            words.extend(
                grid.cells.iter().take(grid.side * grid.side).map(|depth| depth.clamp(0.0, FLOOR_REACH).to_bits()),
            );
        }
        if let Some(grid) = ice {
            words.extend(grid.cells.iter().take(grid.side * grid.side).map(|ice| ice.clamp(0.0, 1.0).to_bits()));
        }
        words
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid<T: Copy>(corner: Vec2, cell: f32, side: usize, value: impl Fn(usize, usize) -> T) -> Grid<T> {
        let cells: Vec<T> = (0..side * side).map(|n| value(n % side, n / side)).collect();
        Grid { corner, cell, side, cells: cells.into() }
    }

    #[test]
    fn snow_lies_by_height_more_where_it_is_colder_and_lasts_where_it_is_bitter() {
        // A storm that snowed and rained: at the cell's own height, 3 °C, it is wet and has no snow.
        let cell = Lying { wet: 0.9, snow: 0.0, fallen: 0.8, celsius: 3.0, height: 100.0, ..Lying::default() };
        assert_eq!(cell.snow_at(100.0), 0.0);
        assert!(cell.wet_at(100.0) > 0.8);
        // Eight hundred metres up it is two degrees below freezing: what fell lies there as snow, and nothing is wet.
        let high = 100.0 + 5.0 / LAPSE;
        assert!((cell.snow_at(high) - 0.8).abs() < 1e-3, "{}", cell.snow_at(high));
        assert_eq!(cell.wet_at(high), 0.0);
        // Far higher it is bitter, and snow lies whatever fell.
        let dry = Lying { fallen: 0.0, ..cell };
        assert_eq!(dry.snow_at(high), 0.0);
        assert!(dry.snow_at(100.0 + 15.0 / LAPSE) > 0.85);
        // Snow lying in the cell melts away below it, where it is warmer.
        let snowed = Lying { snow: 1.0, celsius: 0.0, ..cell };
        assert_eq!(snowed.snow_at(100.0), 1.0);
        assert_eq!(snowed.snow_at(100.0 - 5.0 / LAPSE), 0.0);
    }

    #[test]
    fn grids_blend_between_cells_and_tell_nothing_outside() {
        let floor = grid(Vec2::new(-8.0, -8.0), 4.0, 4, |i, _| i as f32 * 2.0);
        let maps = WeatherMaps { floor: Some(floor), ..WeatherMaps::default() };
        // Cell middles are at -6, -2, 2 and 6: half way between the second and third.
        assert_eq!(maps.sea_depth(0.0, 0.0), Some(3.0));
        assert_eq!(maps.sea_depth(-2.0, 5.0), Some(2.0));
        assert_eq!(maps.sea_depth(-7.9, 0.0), Some(0.0), "kept to the edge");
        assert_eq!(maps.sea_depth(8.5, 0.0), None);
        // Cover is a cell's own, not blended: under the roof, on it, and out in the open beside it.
        let cover = grid(Vec2::ZERO, 0.5, 4, |i, _| if i < 2 { 3.0 } else { OPEN });
        let maps = WeatherMaps { cover: Some(cover), ..WeatherMaps::default() };
        assert!(maps.covered(Vec3::new(0.2, 1.0, 0.2)));
        assert!(!maps.covered(Vec3::new(0.2, 3.0, 0.2)), "the roof's top is in the open");
        assert!(!maps.covered(Vec3::new(1.2, 1.0, 0.2)));
        assert!(!maps.covered(Vec3::new(9.0, 1.0, 0.2)), "beyond the grid");
    }

    #[test]
    fn the_maps_are_laid_out_for_the_shaders_each_after_the_last() {
        let lying = grid(Vec2::new(1.0, 2.0), 32.0, 2, |_, _| Lying {
            wet: 1.0,
            ice: 0.5,
            celsius: -3.0,
            height: 12.0,
            ..Lying::default()
        });
        let floor = grid(Vec2::new(-3.0, 4.0), 4.0, 3, |_, _| 40.0);
        let ice = grid(Vec2::new(-512.0, 0.0), 512.0, 2, |i, _| i as f32);
        let words = WeatherMaps { lying: Some(lying), cover: None, floor: Some(floor), ice: Some(ice) }.words();
        assert_eq!(words.len(), HEADER + 2 * 2 * 3 + 9 + 4);
        assert_eq!(&words[..4], &[2, 32f32.to_bits(), 1f32.to_bits(), 2f32.to_bits()]);
        assert_eq!(words[4], 0, "no cover");
        assert_eq!((words[12], words[14]), (HEADER as u32, HEADER as u32 + 12));
        // The far ice's after where the others start, its cells after theirs.
        assert_eq!(&words[16..20], &[2, 512f32.to_bits(), (-512f32).to_bits(), 0f32.to_bits()]);
        assert_eq!(words[15], HEADER as u32 + 21);
        assert_eq!(words[HEADER], 255 | 128 << 24);
        assert_eq!((f32::from_bits(words[HEADER + 1]), f32::from_bits(words[HEADER + 2])), (-3.0, 12.0));
        assert_eq!(f32::from_bits(words[HEADER + 12]), FLOOR_REACH, "kept within the reach");
        assert_eq!(
            words[HEADER + 21..].iter().map(|&word| f32::from_bits(word)).collect::<Vec<_>>(),
            [0.0, 1.0, 0.0, 1.0]
        );
        assert!(WORDS >= words.len());
    }

    #[test]
    fn a_frozen_sea_goes_on_frozen_past_the_lying_grid_into_the_far_ice_with_no_line_where_it_ends() {
        // The lying grid 256 m across round the middle, its sea frozen.
        let lying = grid(Vec2::new(-128.0, -128.0), 32.0, 8, |_, _| Lying { ice: 1.0, ..Lying::default() });
        let near = WeatherMaps { lying: Some(lying.clone()), ..WeatherMaps::default() };
        assert_eq!(near.ice_at(0.0, 0.0), 1.0);
        assert_eq!(near.ice_at(140.0, 0.0), 0.0, "with no far ice, none past the lying grid");
        // The far ice frozen all over: the sea goes on frozen past the lying grid, and past the far ice's own edge.
        let frozen = grid(Vec2::new(-1024.0, -1024.0), 512.0, 4, |_, _| 1.0);
        let maps = WeatherMaps { lying: Some(lying.clone()), ice: Some(frozen), ..WeatherMaps::default() };
        for (x, z) in [(0.0, 0.0), (120.0, 0.0), (140.0, 0.0), (-130.0, 200.0), (5000.0, 3000.0)] {
            assert_eq!(maps.ice_at(x, z), 1.0, "at {x}, {z}");
        }
        // The far ice open water: across the lying grid's edge westwards, from its frozen sea to the open water a
        // step at a time, with no line where the lying grid ends.
        let open = grid(Vec2::new(-1024.0, -1024.0), 512.0, 4, |_, _| 0.0);
        let maps = WeatherMaps { lying: Some(lying), ice: Some(open), ..WeatherMaps::default() };
        let steps: Vec<f32> = (0..=300).map(|x| maps.ice_at(-x as f32, 0.0)).collect();
        assert_eq!((steps[0], steps[140]), (1.0, 0.0));
        assert!(steps.windows(2).all(|pair| (pair[0] - pair[1]).abs() < 0.05), "{steps:?}");
    }
}
