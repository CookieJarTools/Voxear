//! Wearing a terrain's ground down as weather does, for its Erosion layer:
//! rain running off it cuts channels and drops what it washed away where it
//! slows, in fans at the foot of slopes and along valley floors, and ground
//! too steep to stand slumps down to where it can.
//!
//! The renderer does the work on the GPU (`voxear_render::erosion`), once,
//! in the editor, over the ground every layer below the Erosion layer
//! makes. What it changed is kept in the layer's raster, as a Sculpt
//! layer's brushes' changes are, so a game reads the ground as made, and it
//! is the same on every machine.

use serde::{Deserialize, Serialize};

use crate::{
    heights::{Heightmap, TILE_QUADS, TILE_SAMPLES},
    raster::Rasters,
};

/// How an Erosion layer wears its ground down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weather {
    /// How much rain falls each step, in millimetres.
    pub rain: f32,
    /// How many steps it rains and runs off: longer wears deeper.
    pub steps: u32,
    /// How readily running water picks up ground, 0 to 1.
    pub wear: f32,
    /// The steepest loose ground stands, in degrees: steeper slumps.
    pub slump: f32,
}

impl Default for Weather {
    fn default() -> Self {
        Self { rain: 2.0, steps: 400, wear: 0.3, slump: 50.0 }
    }
}

/// The most cells a side erosion works on: a terrain with more samples is
/// eroded on a coarser grid, and what changed stretched over it.
pub const MOST_CELLS: u32 = 2048;

/// A terrain's ground as erosion works on it: every `stride`th sample each
/// way, `cells` of them, `spacing` metres apart, row after row along z.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub heights: Vec<f32>,
    pub cells: [u32; 2],
    pub spacing: f32,
    pub stride: u32,
}

/// `heights` as a grid to erode, its holes' ground too.
pub fn grid(heights: &Heightmap) -> Grid {
    let [qx, qz] = heights.quads();
    let stride = qx.max(qz).div_ceil(MOST_CELLS - 1).max(1);
    let cells = [qx / stride + 1, qz / stride + 1];
    let mut grid = Vec::with_capacity((cells[0] * cells[1]) as usize);
    for z in 0..cells[1] {
        for x in 0..cells[0] {
            grid.push(heights.sample(i64::from(x * stride), i64::from(z * stride)));
        }
    }
    Grid { heights: grid, cells, spacing: heights.spacing() * stride as f32, stride }
}

/// What erosion changed, `eroded` being `grid`'s heights after it, as
/// raster `raster` of `rasters` for a terrain of `heights`' size: each
/// sample's change, read between the grid's cells where it is coarser.
pub fn keep(rasters: &mut Rasters, raster: u32, heights: &Heightmap, grid: &Grid, eroded: &[f32]) {
    let [cx, cz] = grid.cells;
    let change = |x: u32, z: u32| {
        let i = (z.min(cz - 1) * cx + x.min(cx - 1)) as usize;
        eroded[i] - grid.heights[i]
    };
    let stride = grid.stride as f32;
    for tz in 0..heights.tiles_z() {
        for tx in 0..heights.tiles_x() {
            let tile = rasters.tile_mut(raster, tx, tz);
            for sz in 0..TILE_SAMPLES {
                for sx in 0..TILE_SAMPLES {
                    let (gx, gz) = ((tx * TILE_QUADS + sx) as f32 / stride, (tz * TILE_QUADS + sz) as f32 / stride);
                    let (x0, z0) = (gx.floor() as u32, gz.floor() as u32);
                    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
                    let top = change(x0, z0) + (change(x0 + 1, z0) - change(x0, z0)) * fx;
                    let bottom = change(x0, z0 + 1) + (change(x0 + 1, z0 + 1) - change(x0, z0 + 1)) * fx;
                    tile[(sz * TILE_SAMPLES + sx) as usize] = top + (bottom - top) * fz;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_goes_to_a_grid_and_what_erosion_changed_comes_back_sample_by_sample() {
        let heights = Heightmap::from_fn(2, 1, 0.5, |x, z| x * 0.1 + z * 0.02);
        let ground = grid(&heights);
        assert_eq!((ground.cells, ground.stride, ground.spacing), ([513, 257], 1, 0.5));
        assert_eq!(ground.heights[513 * 3 + 7], heights.sample(7, 3));
        // Lowered by a metre at one cell, and nothing elsewhere.
        let mut eroded = ground.heights.clone();
        eroded[513 * 3 + 300] -= 1.0;
        let mut rasters = Rasters::default();
        keep(&mut rasters, 4, &heights, &ground, &eroded);
        assert_eq!(rasters.sample(4, 300, 3), -1.0);
        assert_eq!(rasters.sample(4, 301, 3), 0.0);
        // A terrain too large for one grid is eroded on a coarser one, and the change spread between its cells.
        let large = Heightmap::flat(9, 9, 1.0, 0.0);
        let coarse = grid(&large);
        assert_eq!(coarse.stride, 2);
        assert!(coarse.cells[0] <= MOST_CELLS);
        let mut lowered = coarse.heights.clone();
        lowered[(coarse.cells[0] * 10 + 10) as usize] -= 2.0;
        let mut rasters = Rasters::default();
        keep(&mut rasters, 1, &large, &coarse, &lowered);
        assert_eq!(rasters.sample(1, 20, 20), -2.0);
        assert_eq!(rasters.sample(1, 21, 20), -1.0, "half way to the next cell");
    }
}
