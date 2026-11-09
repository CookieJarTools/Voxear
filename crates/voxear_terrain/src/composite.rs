//! Making a terrain's ground from its layers.
//!
//! Each tile is made on its own, on worker threads, from the base up
//! through every layer, with a border of `PAD` samples round it so what
//! looks at its neighbours (smoothing, a slope mask) sees past the tile's
//! edge. Only the tiles a change reaches are made again; the rest are kept.
//!
//! A terrain's holes are cut into each tile as it is made, from their
//! raster.
//!
//! While one layer is being edited (a stamp dragged, a sculpt layer
//! painted), the ground below it is kept for each tile it reaches, so each
//! change makes only that layer and the ones above it again.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    file::TerrainFile,
    heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_LEN, TILE_QUADS, TILE_SAMPLES},
    import::Greys,
    layers::{Area, Layer, LayerKind, hash_layers, smoothstep},
    noise,
    raster::Rasters,
};

/// How many samples past a tile's edge it is made with.
pub const PAD: u32 = 16;

/// How many samples a padded tile is across.
const SIDE: u32 = TILE_SAMPLES + 2 * PAD;

/// What finds a picture by the reference a layer has.
pub type Pictures<'a> = &'a (dyn Fn(&str) -> Option<Arc<Greys>> + Sync);

/// What layers read from outside the file: pictures by their references,
/// and the sculpt layers' rasters.
pub struct Sources<'a> {
    pub pictures: Pictures<'a>,
    pub rasters: &'a Rasters,
}

/// Which tiles to make again.
#[derive(Clone, Debug, PartialEq)]
pub enum Dirty {
    All,
    /// The tiles under these rectangles, in metres from the corner.
    Areas(Vec<Area>),
    Tiles(Vec<(u32, u32)>),
}

/// Makes terrains' ground from their layers, keeping the ground below a
/// layer being edited.
#[derive(Default)]
pub struct Compositor {
    editing: Option<(usize, u64)>,
    below: Mutex<Kept>,
}

/// Each tile's padded ground below the layer being edited.
type Kept = HashMap<(u32, u32), Arc<[f32]>>;

impl Compositor {
    /// Keeps the ground below layer `layer` from now on, as it is edited, or
    /// stops with `None`.
    pub fn edit(&mut self, file: &TerrainFile, layer: Option<usize>) {
        let wanted = layer.map(|layer| (layer, below_hash(file, layer)));
        if wanted != self.editing {
            self.editing = wanted;
            self.below.lock().expect("never poisoned").clear();
        }
    }

    /// The layer being edited, if any.
    pub fn editing(&self) -> Option<usize> {
        self.editing.map(|(layer, _)| layer)
    }

    /// The ground `file` makes: the `dirty` tiles made again, the rest kept
    /// from `previous` when it is as large and as fine. Keeps `previous`'s
    /// id when it is the same terrain.
    pub fn composite(
        &self,
        file: &TerrainFile,
        sources: &Sources,
        previous: Option<&Heightmap>,
        dirty: &Dirty,
    ) -> Heightmap {
        let (tiles, spacing) = (file.tiles(), file.spacing());
        let fits = previous.filter(|p| p.tiles_x() == tiles && p.tiles_z() == tiles && p.spacing() == spacing);
        let wanted: Vec<(u32, u32)> = match (dirty, fits) {
            (Dirty::All, _) | (_, None) => (0..tiles * tiles).map(|i| (i % tiles, i / tiles)).collect(),
            (Dirty::Tiles(list), Some(_)) => list.iter().copied().filter(|&(x, z)| x < tiles && z < tiles).collect(),
            (Dirty::Areas(areas), Some(_)) => tiles_under(areas, tiles, spacing),
        };
        // The ground below the edited layer is kept only while the layers under it are as they were.
        let below_layer =
            self.editing.filter(|&(layer, hash)| below_hash(file, layer) == hash && layer < file.layers.len());
        let made = Mutex::new(Vec::with_capacity(wanted.len()));
        let next = AtomicUsize::new(0);
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(wanted.len().max(1));
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    let mut padded = vec![0.0f32; (SIDE * SIDE) as usize];
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(tx, tz)) = wanted.get(at) else { break };
                        let start = match below_layer {
                            Some((layer, _)) => {
                                padded.copy_from_slice(&self.below_tile(file, sources, layer, tx, tz));
                                layer
                            }
                            None => {
                                padded.fill(file.base);
                                0
                            }
                        };
                        for layer in &file.layers[start..] {
                            apply(layer, &mut padded, (tx, tz), spacing, file, sources);
                        }
                        let tile = HeightTile::from_samples(inner(&padded)).with_holes(hole_bits(file, sources.rasters, tx, tz));
                        made.lock().expect("never poisoned").push((tx, tz, tile));
                    }
                });
            }
        });
        let made = made.into_inner().expect("never poisoned");
        match fits {
            Some(previous) => previous.with_tiles(made),
            None => {
                let mut ordered: Vec<Option<Arc<HeightTile>>> = vec![None; (tiles * tiles) as usize];
                for (x, z, tile) in made {
                    ordered[(z * tiles + x) as usize] = Some(Arc::new(tile));
                }
                let all = ordered.into_iter().map(|tile| tile.expect("every tile made")).collect();
                Heightmap::from_tiles(tiles, tiles, spacing, all)
            }
        }
    }

    /// The padded ground below layer `layer` on tile `(tx, tz)`, made once
    /// while it is edited.
    fn below_tile(&self, file: &TerrainFile, sources: &Sources, layer: usize, tx: u32, tz: u32) -> Arc<[f32]> {
        if let Some(kept) = self.below.lock().expect("never poisoned").get(&(tx, tz)) {
            return kept.clone();
        }
        let mut padded = vec![file.base; (SIDE * SIDE) as usize];
        for under in &file.layers[..layer] {
            apply(under, &mut padded, (tx, tz), file.spacing(), file, sources);
        }
        let kept: Arc<[f32]> = padded.into();
        self.below.lock().expect("never poisoned").insert((tx, tz), kept.clone());
        kept
    }

    /// The ground just below the edited layer at sample `(x, z)` of the
    /// whole terrain, as brushes read it.
    pub fn below_at(&self, file: &TerrainFile, sources: &Sources, x: i64, z: i64) -> f32 {
        let Some(layer) = self.editing() else { return file.base };
        let tiles = file.tiles();
        let quads = i64::from(tiles * TILE_QUADS);
        let (x, z) = (x.clamp(0, quads), z.clamp(0, quads));
        let tx = ((x / i64::from(TILE_QUADS)) as u32).min(tiles - 1);
        let tz = ((z / i64::from(TILE_QUADS)) as u32).min(tiles - 1);
        let padded = self.below_tile(file, sources, layer, tx, tz);
        let (px, pz) =
            (x - i64::from(tx * TILE_QUADS) + i64::from(PAD), z - i64::from(tz * TILE_QUADS) + i64::from(PAD));
        padded[(pz * i64::from(SIDE) + px) as usize]
    }
}

/// A hash of what is below layer `layer`, and how large and fine the
/// terrain is, for knowing when what was kept no longer holds.
fn below_hash(file: &TerrainFile, layer: usize) -> u64 {
    let below = &file.layers[..layer.min(file.layers.len())];
    let layers = hash_layers(file.base, below);
    crate::layers::stable_hash(format!("{} {} {layers}", file.size, file.resolution).as_bytes())
}

/// The tiles any of `areas` reach, with the border what looks at its
/// neighbours reads past them.
fn tiles_under(areas: &[Area], tiles: u32, spacing: f32) -> Vec<(u32, u32)> {
    let mut found = Vec::new();
    let reach = PAD as f32 * spacing;
    let width = TILE_QUADS as f32 * spacing;
    for &[x0, z0, x1, z1] in areas {
        let first = |v: f32| ((v - reach) / width).floor().max(0.0) as u32;
        let last = |v: f32| (((v + reach) / width).floor().max(0.0) as u32).min(tiles - 1);
        for tz in first(z0).min(tiles - 1)..=last(z1) {
            for tx in first(x0).min(tiles - 1)..=last(x1) {
                if !found.contains(&(tx, tz)) {
                    found.push((tx, tz));
                }
            }
        }
    }
    found
}

/// Tile `(tx, tz)`'s holes, from the terrain's raster of them: a sample a
/// hole's where it is more than a half.
pub fn hole_bits(file: &TerrainFile, rasters: &Rasters, tx: u32, tz: u32) -> Option<Box<[u32]>> {
    if file.holes == 0 {
        return None;
    }
    let tile = rasters.tile(file.holes, tx, tz)?;
    let mut bits = vec![0u32; HOLE_WORDS];
    for (index, &value) in tile.iter().enumerate() {
        if value > 0.5 {
            bits[index / 32] |= 1 << (index % 32);
        }
    }
    Some(bits.into())
}

/// A padded tile's own samples.
fn inner(padded: &[f32]) -> Vec<f32> {
    let mut samples = Vec::with_capacity(TILE_LEN);
    for z in 0..TILE_SAMPLES {
        let row = ((z + PAD) * SIDE + PAD) as usize;
        samples.extend_from_slice(&padded[row..row + TILE_SAMPLES as usize]);
    }
    samples
}

/// Puts one layer on a padded tile.
fn apply(layer: &Layer, padded: &mut [f32], (tx, tz): (u32, u32), spacing: f32, file: &TerrainFile, sources: &Sources) {
    // Digging is done into the ground every layer makes, after (see `dig::dig_layers`).
    if !layer.enabled || layer.opacity <= 0.0 || layer.kind.digs() {
        return;
    }
    let (ox, oz) = (i64::from(tx * TILE_QUADS) - i64::from(PAD), i64::from(tz * TILE_QUADS) - i64::from(PAD));
    let quads = i64::from(file.tiles() * TILE_QUADS);
    // Each padded sample's place in metres, clamped to the terrain, as heights past its edge repeat its edge.
    let place = |i: u32, j: u32| {
        let (x, z) = ((ox + i64::from(i)).clamp(0, quads), (oz + i64::from(j)).clamp(0, quads));
        (x, z, x as f32 * spacing, z as f32 * spacing)
    };
    if let Some([x0, z0, x1, z1]) = layer.kind.area() {
        let tile_area = [
            ox as f32 * spacing,
            oz as f32 * spacing,
            (ox + i64::from(SIDE)) as f32 * spacing,
            (oz + i64::from(SIDE)) as f32 * spacing,
        ];
        if x1 < tile_area[0] || z1 < tile_area[1] || x0 > tile_area[2] || z0 > tile_area[3] {
            return;
        }
    }
    // The ground below, for masks and the layers that work on it.
    let below = padded.to_vec();
    let rise = |i: u32, j: u32| {
        let at = |i: u32, j: u32| below[(j.min(SIDE - 1) * SIDE + i.min(SIDE - 1)) as usize];
        let dx = at(i + 1, j) - at(i.saturating_sub(1), j);
        let dz = at(i, j + 1) - at(i, j.saturating_sub(1));
        (dx * dx + dz * dz).sqrt() / (2.0 * spacing)
    };
    let picture = |reference: &Option<String>| reference.as_deref().and_then(|r| (sources.pictures)(r));
    let terrain_size = file.size;
    let blurred = match layer.kind {
        LayerKind::Smooth { radius } => Some(blur(&below, (radius / spacing).round() as u32)),
        _ => None,
    };
    let import = match &layer.kind {
        LayerKind::Import { picture: reference, .. } => picture(reference),
        _ => None,
    };
    let stamp = match &layer.kind {
        LayerKind::Stamp { picture: reference, .. } => picture(reference),
        _ => None,
    };
    for j in 0..SIDE {
        for i in 0..SIDE {
            let index = (j * SIDE + i) as usize;
            let (gx, gz, x, z) = place(i, j);
            let h = below[index];
            let (value, weight) = match &layer.kind {
                LayerKind::Import { height, .. } => match &import {
                    Some(greys) => (greys.at(x / terrain_size, 1.0 - z / terrain_size) * height, 1.0),
                    None => (0.0, 0.0),
                },
                LayerKind::Stamp { position, rotation, size, height, falloff, .. } => {
                    stamp_at((x, z), *position, *rotation, *size, *falloff, stamp.as_deref())
                        .map_or((0.0, 0.0), |(grey, weight)| (grey * height, weight))
                }
                LayerKind::Noise { style, scale, height, octaves, seed } => {
                    let scale = scale.max(0.01);
                    (noise::fractal(*style, x / scale, z / scale, *octaves, *seed) * height, 1.0)
                }
                LayerKind::Sculpt { raster } => (sources.rasters.sample(*raster, gx, gz), 1.0),
                LayerKind::Spline { points, width, falloff, .. } => spline_at((x, z), points, *width, *falloff),
                LayerKind::Terrace { step, sharpness } => (terrace(h, *step, *sharpness), 1.0),
                LayerKind::Smooth { .. } => (blurred.as_ref().map_or(h, |b| b[index]), 1.0),
                LayerKind::Dig { .. } => (h, 0.0),
            };
            if weight <= 0.0 {
                continue;
            }
            let masked = if layer.mask.is_none() { 1.0 } else { layer.mask.weight(h, rise(i, j)) };
            let share = (weight * layer.opacity * masked).clamp(0.0, 1.0);
            padded[index] = h + (layer.blend.apply(h, value) - h) * share;
        }
    }
}

/// A stamp at `(x, z)`: its grey there, 0 to 1, and how much of it counts,
/// past its edge none.
fn stamp_at(
    (x, z): (f32, f32),
    position: [f32; 2],
    rotation: f32,
    size: [f32; 2],
    falloff: f32,
    picture: Option<&Greys>,
) -> Option<(f32, f32)> {
    let (sin, cos) = rotation.to_radians().sin_cos();
    let (dx, dz) = (x - position[0], z - position[1]);
    // Into the stamp's own frame, turned back by its rotation.
    let (lx, lz) = (dx * cos - dz * sin, dx * sin + dz * cos);
    let (u, v) = (lx / size[0].max(1e-3) + 0.5, lz / size[1].max(1e-3) + 0.5);
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return None;
    }
    let edge = u.min(1.0 - u).min(v).min(1.0 - v) * 2.0;
    let weight = smoothstep(0.0, falloff.clamp(0.0, 1.0), edge);
    let grey = match picture {
        // The picture's top is the stamp's far side, along +z, as from above.
        Some(greys) => greys.at(u, 1.0 - v),
        None => {
            let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
            1.0 - smoothstep(0.0, 1.0, r)
        }
    };
    Some((grey, weight))
}

/// A spline at `(x, z)`: the height of the nearest point along it, and how
/// much it counts there.
fn spline_at((x, z): (f32, f32), points: &[[f32; 3]], width: f32, falloff: f32) -> (f32, f32) {
    let (mut best, mut height) = (f32::MAX, 0.0);
    for pair in points.windows(2) {
        let ([ax, ay, az], [bx, by, bz]) = (pair[0], pair[1]);
        let (abx, abz) = (bx - ax, bz - az);
        let length = abx * abx + abz * abz;
        let t = if length > 0.0 { (((x - ax) * abx + (z - az) * abz) / length).clamp(0.0, 1.0) } else { 0.0 };
        let (px, pz) = (ax + abx * t, az + abz * t);
        let distance = ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
        if distance < best {
            (best, height) = (distance, ay + (by - ay) * t);
        }
    }
    if points.len() == 1 {
        let [px, py, pz] = points[0];
        (best, height) = (((x - px).powi(2) + (z - pz).powi(2)).sqrt(), py);
    }
    let half = width * 0.5;
    let weight = 1.0 - smoothstep(half, half + falloff.max(1e-3), best);
    (height, weight)
}

/// `h` cut into steps `step` high, their risers as sharp as `sharpness`.
fn terrace(h: f32, step: f32, sharpness: f32) -> f32 {
    if step <= 1e-4 {
        return h;
    }
    let f = h / step;
    let floor = f.floor();
    let width = (1.0 - sharpness.clamp(0.0, 1.0)).max(1e-3);
    let t = smoothstep(0.5 - width * 0.5, 0.5 + width * 0.5, f - floor);
    (floor + t) * step
}

/// A padded tile blurred over `radius` samples each way: a box blur along
/// rows, then columns, twice, close to a Gaussian.
fn blur(samples: &[f32], radius: u32) -> Vec<f32> {
    let radius = (radius / 2).clamp(1, PAD / 2) as i64;
    let side = i64::from(SIDE);
    let mut a = samples.to_vec();
    let mut b = vec![0.0f32; a.len()];
    for _ in 0..2 {
        box_pass(&a, &mut b, side, radius, true);
        box_pass(&b, &mut a, side, radius, false);
    }
    a
}

fn box_pass(from: &[f32], to: &mut [f32], side: i64, radius: i64, along_rows: bool) {
    let count = (2 * radius + 1) as f32;
    for line in 0..side {
        for at in 0..side {
            let mut sum = 0.0;
            for k in -radius..=radius {
                let along = (at + k).clamp(0, side - 1);
                let (x, z) = if along_rows { (along, line) } else { (line, along) };
                sum += from[(z * side + x) as usize];
            }
            let (x, z) = if along_rows { (at, line) } else { (line, at) };
            to[(z * side + x) as usize] = sum / count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{Blend, Mask};

    fn file(layers: Vec<Layer>) -> TerrainFile {
        TerrainFile { size: 256.0, resolution: 257, layers, ..TerrainFile::default() }
    }

    fn none(_: &str) -> Option<Arc<Greys>> {
        None
    }

    fn make(file: &TerrainFile, rasters: &Rasters) -> Heightmap {
        let sources = Sources { pictures: &none, rasters };
        Compositor::default().composite(file, &sources, None, &Dirty::All)
    }

    #[test]
    fn layers_stack_from_the_base_up_each_by_its_blend_and_opacity() {
        let mut stamp = Layer::new(LayerKind::Stamp {
            picture: None,
            position: [128.0, 128.0],
            rotation: 0.0,
            size: [40.0, 40.0],
            height: 10.0,
            falloff: 0.0,
        });
        let mut terrain = file(vec![stamp.clone()]);
        terrain.base = 2.0;
        let rasters = Rasters::default();
        let ground = make(&terrain, &rasters);
        // The rounded hill's top in the middle, nothing past its edge.
        assert!((ground.height_at(128.0, 128.0) - 12.0).abs() < 1e-3);
        assert_eq!(ground.height_at(10.0, 10.0), 2.0);
        // Half as strong at half opacity; subtracted, a pit.
        stamp.opacity = 0.5;
        assert!((make(&file(vec![stamp.clone()]), &rasters).height_at(128.0, 128.0) - 5.0).abs() < 1e-3);
        stamp.opacity = 1.0;
        stamp.blend = Blend::Subtract;
        assert!((make(&file(vec![stamp.clone()]), &rasters).height_at(128.0, 128.0) + 10.0).abs() < 1e-3);
        // Switched off, as though it were not there.
        stamp.enabled = false;
        assert_eq!(make(&file(vec![stamp]), &rasters).height_at(128.0, 128.0), 0.0);
    }

    #[test]
    fn a_spline_lays_the_ground_level_along_it_and_eases_beside() {
        let mut road = Layer::new(LayerKind::Spline {
            points: vec![[20.0, 5.0, 128.0], [230.0, 5.0, 128.0]],
            width: 8.0,
            falloff: 4.0,
            dig: None,
        });
        road.blend = Blend::Set;
        let hills =
            Layer::new(LayerKind::Noise { style: Default::default(), scale: 40.0, height: 20.0, octaves: 3, seed: 1 });
        let ground = make(&file(vec![hills, road]), &Rasters::default());
        assert!((ground.height_at(100.0, 128.0) - 5.0).abs() < 1e-3, "on the road");
        assert!((ground.height_at(100.0, 131.9) - 5.0).abs() < 1e-3, "at its edge");
        assert!((ground.height_at(100.0, 140.0) - 5.0).abs() > 0.05, "off it, the hills");
    }

    #[test]
    fn a_mask_keeps_a_layer_to_the_heights_it_names() {
        // Ground rising 50 m along x, then 3 m added only above 30 m.
        let tilt = Layer::new(LayerKind::Spline {
            points: vec![[0.0, 0.0, 128.0], [256.0, 50.0, 128.0]],
            width: 600.0,
            falloff: 1.0,
            dig: None,
        });
        let mut raise = Layer::new(LayerKind::Stamp {
            picture: None,
            position: [128.0, 128.0],
            rotation: 0.0,
            size: [1000.0, 1000.0],
            height: 3.0,
            falloff: 0.0,
        });
        raise.mask = Mask { height: Some([30.0, 100.0]), slope: None, feather: 0.0 };
        let ground = make(&file(vec![tilt, raise]), &Rasters::default());
        assert!((ground.height_at(50.0, 128.0) - 50.0 / 256.0 * 50.0).abs() < 0.2, "below 30 m, as it was");
        assert!(ground.height_at(200.0, 128.0) > 200.0 / 256.0 * 50.0 + 2.0, "above 30 m, raised");
    }

    #[test]
    fn a_sculpt_layer_adds_its_raster_and_terraces_and_smoothing_work_on_the_ground_below() {
        let mut rasters = Rasters::default();
        rasters.tile_mut(7, 0, 0)[(100 * TILE_SAMPLES + 100) as usize] = 4.0;
        let sculpt = Layer::new(LayerKind::Sculpt { raster: 7 });
        let ground = make(&file(vec![sculpt.clone()]), &rasters);
        assert_eq!(ground.sample(100, 100), 4.0);
        let smooth = Layer::new(LayerKind::Smooth { radius: 4.0 });
        let smoothed = make(&file(vec![sculpt, smooth]), &rasters);
        assert!(smoothed.sample(100, 100) < 4.0 && smoothed.sample(101, 100) > 0.0, "the spike spread round");
        // Sharp terraces round to the nearest step.
        assert_eq!(terrace(6.6, 2.0, 1.0), 6.0);
        assert_eq!(terrace(7.4, 2.0, 1.0), 8.0);
    }

    #[test]
    fn only_the_tiles_a_change_reaches_are_made_again_and_editing_a_layer_keeps_the_ground_below() {
        let hills =
            Layer::new(LayerKind::Noise { style: Default::default(), scale: 50.0, height: 10.0, octaves: 3, seed: 2 });
        let stamp = |x: f32| {
            Layer::new(LayerKind::Stamp {
                picture: None,
                position: [x, 64.0],
                rotation: 0.0,
                size: [20.0, 20.0],
                height: 5.0,
                falloff: 0.0,
            })
        };
        let mut terrain =
            TerrainFile { size: 1024.0, resolution: 1025, layers: vec![hills, stamp(64.0)], ..TerrainFile::default() };
        let rasters = Rasters::default();
        let sources = Sources { pictures: &none, rasters: &rasters };
        let mut compositor = Compositor::default();
        let first = compositor.composite(&terrain, &sources, None, &Dirty::All);
        // Dragged along, the stamp's tiles before and after are made again, the rest kept.
        compositor.edit(&terrain, Some(1));
        let before = terrain.layers[1].kind.area().unwrap();
        terrain.layers[1] = stamp(80.0);
        let after = terrain.layers[1].kind.area().unwrap();
        let moved = compositor.composite(&terrain, &sources, Some(&first), &Dirty::Areas(vec![before, after]));
        assert_eq!(moved.id(), first.id());
        assert!(Arc::ptr_eq(moved.tile(3, 3), first.tile(3, 3)), "far tiles kept");
        assert!(!Arc::ptr_eq(moved.tile(0, 0), first.tile(0, 0)));
        // As made from nothing.
        let whole = Compositor::default().composite(&terrain, &sources, None, &Dirty::All);
        assert_eq!(moved.tile(0, 0).samples(), whole.tile(0, 0).samples());
        // Where the stamp was first, the ground below it is the hills alone.
        assert!((first.sample(64, 64) - compositor.below_at(&terrain, &sources, 64, 64) - 5.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod timing {
    use super::*;
    use crate::layers::Blend;

    /// How long a large terrain takes to make from a few layers, and a
    /// brush-sized change to it; run with `--ignored --nocapture` in release.
    #[test]
    #[ignore]
    fn a_large_terrain_is_made_quickly() {
        let hills = Layer::new(LayerKind::Noise { style: crate::noise::NoiseStyle::Ridged, scale: 600.0, height: 120.0, octaves: 7, seed: 3 });
        let crater = Layer::new(LayerKind::Stamp { picture: None, position: [900.0, 900.0], rotation: 0.0, size: [60.0, 60.0], height: -8.0, falloff: 0.3 });
        let mut road = Layer::new(LayerKind::Spline { points: vec![[100.0, 20.0, 100.0], [1500.0, 30.0, 1800.0]], width: 10.0, falloff: 6.0, dig: None });
        road.blend = Blend::Set;
        let smooth = Layer::new(LayerKind::Smooth { radius: 2.0 });
        let file = TerrainFile { size: 2048.0, resolution: 4097, layers: vec![hills, crater, road, smooth], ..TerrainFile::default() };
        let rasters = Rasters::default();
        let none = |_: &str| None;
        let sources = Sources { pictures: &none, rasters: &rasters };
        let compositor = Compositor::default();
        let start = std::time::Instant::now();
        let whole = compositor.composite(&file, &sources, None, &Dirty::All);
        let all = start.elapsed();
        let start = std::time::Instant::now();
        let _ = compositor.composite(&file, &sources, Some(&whole), &Dirty::Areas(vec![[880.0, 880.0, 920.0, 920.0]]));
        let some = start.elapsed();
        eprintln!("256 tiles from 4 layers: {all:?}; a 40 m change: {some:?}");

        // Grass everywhere, rock on slopes and snow up high, their edges broken up, and their shares worked out.
        use crate::surfaces::{Cover, Surface, weigh};
        let rules = |mask: crate::layers::Mask| Surface { cover: Cover::Rules, mask, breakup: 3.0, ..Surface::new(None) };
        let rock = rules(crate::layers::Mask { height: None, slope: Some([30.0, 90.0]), feather: 5.0 });
        let snow = rules(crate::layers::Mask { height: Some([80.0, 1000.0]), slope: None, feather: 6.0 });
        let file = TerrainFile { surfaces: vec![Surface::new(None), rock, snow], ..file };
        let start = std::time::Instant::now();
        let shares = weigh(&file, &whole, &rasters, None, &Dirty::All).unwrap();
        let all = start.elapsed();
        let start = std::time::Instant::now();
        let _ = weigh(&file, &whole, &rasters, Some(&shares), &Dirty::Areas(vec![[880.0, 880.0, 920.0, 920.0]]));
        let some = start.elapsed();
        eprintln!("shares of 3 surfaces over 256 tiles: {all:?}; over a 40 m change: {some:?}");
    }
}
