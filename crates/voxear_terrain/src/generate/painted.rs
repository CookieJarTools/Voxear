//! Painted maps: land, height and biome painted over a world's generator,
//! kept in tiles only where painted, at a coarse spacing, without end like
//! the world. The generator reads them as it makes the ground: painted land
//! is blended into the landmass before the coast curve, so it grows a real
//! coast and painting the other way sinks it; painted height is fed in
//! before ridges, erosion and gullies, so a painted mountain gets peaks and
//! valleys of the generator's own; a painted biome is chosen before the
//! climate rules, with ragged edges.
//!
//! Maps are in the generator's world, in metres. A world's maps are held by
//! a [`Painted`] handle its generator reads through, so a stroke changes the
//! ground with nothing compiled again; the world makes again only the
//! columns under what changed (see [`PaintedMaps::changed_since`]).

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::{Result, ensure};

/// How many pixels a tile is along each side.
pub const TILE: usize = 64;

/// Which map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintKind {
    Land,
    Height,
    Biome,
}

impl PaintKind {
    pub const ALL: [PaintKind; 3] = [PaintKind::Land, PaintKind::Height, PaintKind::Biome];

    pub fn name(self) -> &'static str {
        match self {
            PaintKind::Land => "Land",
            PaintKind::Height => "Height",
            PaintKind::Biome => "Biome",
        }
    }

    /// The spacing a map of this kind is kept at, in metres a pixel.
    pub fn spacing(self) -> f32 {
        match self {
            PaintKind::Land | PaintKind::Height => 8.0,
            PaintKind::Biome => 4.0,
        }
    }
}

/// One map: its pixels' spacing, in metres, and its tiles where painted, each
/// its pixels a row along x after another; not a number where unpainted.
#[derive(Clone, Debug)]
pub struct PaintedMap {
    pub spacing: f32,
    tiles: HashMap<[i32; 2], Arc<Vec<f32>>>,
}

impl PaintedMap {
    pub fn new(spacing: f32) -> Self {
        Self { spacing: spacing.max(0.5), tiles: HashMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn pixel(&self, [px, pz]: [i32; 2]) -> Option<f32> {
        let side = TILE as i32;
        let tile = self.tiles.get(&[px.div_euclid(side), pz.div_euclid(side)])?;
        let value = tile[(pz.rem_euclid(side) * side + px.rem_euclid(side)) as usize];
        (!value.is_nan()).then_some(value)
    }

    /// Its value at `(x, z)`, in metres: blended between the four pixels
    /// round it, unpainted ones counting as `none`; none where all four are.
    pub fn sample(&self, x: f32, z: f32, none: f32) -> Option<f32> {
        if self.tiles.is_empty() {
            return None;
        }
        let (fx, fz) = (x / self.spacing - 0.5, z / self.spacing - 0.5);
        let (x0, z0) = (fx.floor() as i32, fz.floor() as i32);
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let mut painted = false;
        let mut at = |px: i32, pz: i32| match self.pixel([px, pz]) {
            Some(value) => {
                painted = true;
                value
            }
            None => none,
        };
        let (a, b, c, d) = (at(x0, z0), at(x0 + 1, z0), at(x0, z0 + 1), at(x0 + 1, z0 + 1));
        let value = (a * (1.0 - tx) + b * tx) * (1.0 - tz) + (c * (1.0 - tx) + d * tx) * tz;
        painted.then_some(value)
    }

    /// The value of the pixel nearest `(x, z)`, where it is painted.
    pub fn nearest(&self, x: f32, z: f32) -> Option<f32> {
        if self.tiles.is_empty() {
            return None;
        }
        self.pixel([(x / self.spacing).floor() as i32, (z / self.spacing).floor() as i32])
    }

    /// Changes the pixels within `radius` metres of `at` by `stroke`, which is
    /// given each pixel's value (none where unpainted) and how much the brush
    /// covers it, from 1 in its middle to 0 at its edge as `falloff` says,
    /// and gives its new value (none to leave it unpainted). The box of
    /// metres it changed.
    pub fn paint(
        &mut self,
        at: [f32; 2],
        radius: f32,
        falloff: f32,
        stroke: &dyn Fn(Option<f32>, f32) -> Option<f32>,
    ) -> Option<[[f32; 2]; 2]> {
        let spacing = self.spacing;
        let low = [((at[0] - radius) / spacing).floor() as i32, ((at[1] - radius) / spacing).floor() as i32];
        let high = [((at[0] + radius) / spacing).ceil() as i32, ((at[1] + radius) / spacing).ceil() as i32];
        let side = TILE as i32;
        let mut changed = false;
        for pz in low[1]..=high[1] {
            for px in low[0]..=high[0] {
                let middle = [(px as f32 + 0.5) * spacing, (pz as f32 + 0.5) * spacing];
                let distance = ((middle[0] - at[0]).powi(2) + (middle[1] - at[1]).powi(2)).sqrt();
                if distance > radius {
                    continue;
                }
                // Whole out to where the falloff starts, then easing to nothing at the edge.
                let inner = radius * (1.0 - falloff.clamp(0.0, 1.0));
                let cover = match distance <= inner {
                    true => 1.0,
                    false => {
                        let t = ((distance - inner) / (radius - inner).max(1e-3)).clamp(0.0, 1.0);
                        1.0 - t * t * (3.0 - 2.0 * t)
                    }
                };
                let was = self.pixel([px, pz]);
                let now = stroke(was, cover);
                if now == was {
                    continue;
                }
                let key = [px.div_euclid(side), pz.div_euclid(side)];
                let tile = self.tiles.entry(key).or_insert_with(|| Arc::new(vec![f32::NAN; TILE * TILE]));
                Arc::make_mut(tile)[(pz.rem_euclid(side) * side + px.rem_euclid(side)) as usize] = now.unwrap_or(f32::NAN);
                changed = true;
            }
        }
        // Tiles left with nothing painted go.
        self.tiles.retain(|_, tile| tile.iter().any(|v| !v.is_nan()));
        changed.then(|| {
            let corner = |p: [i32; 2]| [p[0] as f32 * spacing, p[1] as f32 * spacing];
            [corner(low), corner([high[0] + 1, high[1] + 1])]
        })
    }

    /// A number for the tiles reaching the box of metres `[low, high]`, the
    /// same while they are: 0 where none is painted there.
    pub fn hash_within(&self, [low, high]: [[f32; 2]; 2]) -> u64 {
        let span = self.spacing * TILE as f32;
        let (l, h) = (low.map(|v| (v / span).floor() as i32), high.map(|v| (v / span).floor() as i32));
        let mut bytes: Vec<u8> = Vec::new();
        for tz in l[1]..=h[1] {
            for tx in l[0]..=h[0] {
                if let Some(tile) = self.tiles.get(&[tx, tz]) {
                    bytes.extend(tx.to_le_bytes());
                    bytes.extend(tz.to_le_bytes());
                    tile.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
                }
            }
        }
        if bytes.is_empty() { 0 } else { crate::layers::stable_hash(&bytes) }
    }

    fn write(&self, bytes: &mut Vec<u8>) {
        bytes.extend(self.spacing.to_le_bytes());
        bytes.extend((self.tiles.len() as u32).to_le_bytes());
        let mut keys: Vec<&[i32; 2]> = self.tiles.keys().collect();
        keys.sort_unstable();
        for key in keys {
            bytes.extend(key[0].to_le_bytes());
            bytes.extend(key[1].to_le_bytes());
            self.tiles[key].iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        }
    }

    fn read(bytes: &[u8], at: &mut usize) -> Result<Self> {
        let mut take = |count: usize| -> Result<&[u8]> {
            ensure!(*at + count <= bytes.len(), "a painted map cut short");
            *at += count;
            Ok(&bytes[*at - count..*at])
        };
        let spacing = f32::from_le_bytes(take(4)?.try_into()?);
        let count = u32::from_le_bytes(take(4)?.try_into()?);
        let mut map = PaintedMap::new(spacing);
        for _ in 0..count {
            let key = [i32::from_le_bytes(take(4)?.try_into()?), i32::from_le_bytes(take(4)?.try_into()?)];
            let pixels: Vec<f32> = take(TILE * TILE * 4)?.chunks(4).map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]])).collect();
            map.tiles.insert(key, Arc::new(pixels));
        }
        Ok(map)
    }
}

/// A world's three painted maps, a number counting each change, and what
/// each change reached, in metres, by that number.
#[derive(Clone, Debug)]
pub struct PaintedMaps {
    pub land: PaintedMap,
    pub height: PaintedMap,
    pub biome: PaintedMap,
    pub version: u64,
    changes: Vec<(u64, [[f32; 2]; 2])>,
}

impl Default for PaintedMaps {
    fn default() -> Self {
        Self {
            land: PaintedMap::new(PaintKind::Land.spacing()),
            height: PaintedMap::new(PaintKind::Height.spacing()),
            biome: PaintedMap::new(PaintKind::Biome.spacing()),
            version: 0,
            changes: Vec::new(),
        }
    }
}

/// What a file of painted maps starts with, and the version of what follows.
const MAGIC: &[u8; 4] = b"VXPM";
const VERSION: u8 = 1;
/// How many changes are remembered for those catching up.
const CHANGES_KEPT: usize = 256;

impl PaintedMaps {
    pub fn is_empty(&self) -> bool {
        self.land.is_empty() && self.height.is_empty() && self.biome.is_empty()
    }

    pub fn map(&self, kind: PaintKind) -> &PaintedMap {
        match kind {
            PaintKind::Land => &self.land,
            PaintKind::Height => &self.height,
            PaintKind::Biome => &self.biome,
        }
    }

    pub fn map_mut(&mut self, kind: PaintKind) -> &mut PaintedMap {
        match kind {
            PaintKind::Land => &mut self.land,
            PaintKind::Height => &mut self.height,
            PaintKind::Biome => &mut self.biome,
        }
    }

    /// Notes a change reaching the box of metres `area`, counting it.
    pub fn changed(&mut self, area: [[f32; 2]; 2]) {
        self.version += 1;
        self.changes.push((self.version, area));
        if self.changes.len() > CHANGES_KEPT {
            self.changes.remove(0);
        }
    }

    /// What changed since `version`: the boxes of metres each change reached,
    /// or none where too much did to remember.
    pub fn changed_since(&self, version: u64) -> Option<Vec<[[f32; 2]; 2]>> {
        if version == self.version {
            return Some(Vec::new());
        }
        let first = self.changes.first().map_or(self.version + 1, |(number, _)| *number);
        if version + 1 < first {
            return None;
        }
        Some(self.changes.iter().filter(|(number, _)| *number > version).map(|(_, area)| *area).collect())
    }

    /// A number for every map's tiles reaching the box of metres `area`.
    pub fn hash_within(&self, area: [[f32; 2]; 2]) -> u64 {
        let hashes = PaintKind::ALL.map(|kind| self.map(kind).hash_within(area));
        if hashes == [0; 3] { 0 } else { crate::layers::stable_hash(&hashes.map(u64::to_le_bytes).concat()) }
    }

    /// The maps as bytes, compressed, for the world's `.maps` file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(MAGIC);
        bytes.push(VERSION);
        for kind in PaintKind::ALL {
            self.map(kind).write(&mut bytes);
        }
        lz4_flex::compress_prepend_size(&bytes)
    }

    pub fn from_bytes(compressed: &[u8]) -> Result<Self> {
        let bytes = lz4_flex::decompress_size_prepended(compressed)?;
        ensure!(bytes.len() >= 5 && &bytes[..4] == MAGIC && bytes[4] == VERSION, "not painted maps this version reads");
        let mut at = 5;
        let land = PaintedMap::read(&bytes, &mut at)?;
        let height = PaintedMap::read(&bytes, &mut at)?;
        let biome = PaintedMap::read(&bytes, &mut at)?;
        ensure!(at == bytes.len(), "painted maps with more after them");
        Ok(Self { land, height, biome, version: 0, changes: Vec::new() })
    }
}

/// A world's painted maps, as its generator reads them: shared, and changed
/// in place by a stroke, so nothing is compiled again. Known by its `id`.
#[derive(Clone, Debug)]
pub struct Painted {
    pub id: u64,
    maps: Arc<RwLock<Arc<PaintedMaps>>>,
}

impl PartialEq for Painted {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Painted {
    pub fn new(id: u64, maps: PaintedMaps) -> Self {
        Self { id: id.max(1), maps: Arc::new(RwLock::new(Arc::new(maps))) }
    }

    /// The maps as they are now.
    pub fn maps(&self) -> Arc<PaintedMaps> {
        self.maps.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Changes the maps with `change`.
    pub fn change<R>(&self, change: impl FnOnce(&mut PaintedMaps) -> R) -> R {
        let mut held = self.maps.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        change(Arc::make_mut(&mut held))
    }

    /// Puts `maps` in place of the maps, as an undone stroke does.
    pub fn set(&self, maps: Arc<PaintedMaps>) {
        let mut held = self.maps.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let version = held.version + 1;
        let mut maps = maps;
        Arc::make_mut(&mut maps).version = version;
        // Everything that differs counts as changed: all of what either holds.
        let reach = |maps: &PaintedMaps| -> Option<[[f32; 2]; 2]> {
            let mut boxes = PaintKind::ALL.into_iter().filter_map(|kind| maps.map(kind).bounds());
            let first = boxes.next()?;
            Some(boxes.fold(first, |a, b| [[a[0][0].min(b[0][0]), a[0][1].min(b[0][1])], [a[1][0].max(b[1][0]), a[1][1].max(b[1][1])]]))
        };
        for area in [reach(&held), reach(&maps)].into_iter().flatten() {
            Arc::make_mut(&mut maps).changes.push((version, area));
        }
        *held = maps;
    }

    /// Whether `other` is the same handle.
    pub fn same(&self, other: &Painted) -> bool {
        Arc::ptr_eq(&self.maps, &other.maps)
    }
}

impl PaintedMap {
    /// The box of metres its tiles cover; none with none.
    pub fn bounds(&self) -> Option<[[f32; 2]; 2]> {
        let span = self.spacing * TILE as f32;
        let mut keys = self.tiles.keys();
        let first = *keys.next()?;
        let (mut low, mut high) = (first, first);
        for key in keys {
            low = [low[0].min(key[0]), low[1].min(key[1])];
            high = [high[0].max(key[0]), high[1].max(key[1])];
        }
        Some([low.map(|v| v as f32 * span), high.map(|v| (v + 1) as f32 * span)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_is_painted_where_the_brush_goes_eased_at_its_edge_and_erased_back_to_nothing() {
        let mut map = PaintedMap::new(8.0);
        assert_eq!(map.sample(100.0, 100.0, 0.0), None);
        let area = map.paint([100.0, 100.0], 40.0, 0.5, &|was, cover| Some(was.unwrap_or(0.0) + cover));
        assert!(area.is_some());
        assert!((map.sample(100.0, 100.0, 0.0).unwrap() - 1.0).abs() < 0.05);
        let edge = map.sample(134.0, 100.0, 0.0).unwrap();
        assert!(edge > 0.0 && edge < 0.6, "{edge}");
        assert_eq!(map.sample(300.0, 100.0, 0.0), None);
        // Painted again where it is, it changes nothing and reaches nothing.
        assert!(map.paint([100.0, 100.0], 40.0, 0.5, &|was, _| was).is_none());
        map.paint([100.0, 100.0], 60.0, 0.0, &|_, _| None);
        assert!(map.is_empty(), "erased tiles go");
    }

    #[test]
    fn painted_height_and_land_raise_the_generators_ground_and_painting_changes_it_with_nothing_compiled_again() {
        use crate::blocks::generate::{BlockGenerate, PaintedRef, ground_generator};
        let world = crate::generate::Preset::RollingHills.world();
        let bare = BlockGenerate { world: Some("hills".into()), ..BlockGenerate::default() };
        let painted = Painted::new(7, PaintedMaps::default());
        let generate = BlockGenerate { painted: PaintedRef(Some(painted.clone())), ..bare.clone() };
        let at = |generate: &BlockGenerate, x: f32, z: f32| ground_generator(generate, &world, 1.0).height_at(x, z);
        let before = at(&generate, 500.0, 500.0);
        assert_eq!(before, at(&bare, 500.0, 500.0), "nothing painted, nothing changed");
        let first = ground_generator(&generate, &world, 1.0);
        // A mountain painted there: the ground rises, from the same generator, nothing compiled again.
        painted.change(|maps| {
            let area = maps.height.paint([500.0, 500.0], 200.0, 0.5, &|_, cover| Some(cover));
            maps.changed(area.unwrap());
        });
        let raised = at(&generate, 500.0, 500.0);
        assert!(raised > before + 10.0, "{before} to {raised}");
        assert!(std::sync::Arc::ptr_eq(&first, &ground_generator(&generate, &world, 1.0)));
        assert_eq!(at(&generate, 5000.0, 5000.0), at(&bare, 5000.0, 5000.0), "unpainted ground as it was");
        // Land painted away sinks it under the sea.
        painted.change(|maps| {
            maps.height.paint([500.0, 500.0], 300.0, 0.0, &|_, _| None);
            maps.land.paint([500.0, 500.0], 300.0, 0.0, &|_, _| Some(-1.0));
        });
        assert!(at(&generate, 500.0, 500.0) < world.sea_level(), "sunk");
    }

    #[test]
    fn maps_go_to_bytes_and_back_and_remember_what_changed() {
        let mut maps = PaintedMaps::default();
        let area = maps.land.paint([0.0, 0.0], 20.0, 0.0, &|_, _| Some(0.75)).unwrap();
        maps.changed(area);
        maps.biome.paint([500.0, -300.0], 10.0, 0.0, &|_, _| Some(3.0));
        let read = PaintedMaps::from_bytes(&maps.to_bytes()).unwrap();
        assert_eq!(read.land.sample(0.0, 0.0, 0.0), maps.land.sample(0.0, 0.0, 0.0));
        assert_eq!(read.biome.nearest(500.0, -300.0), Some(3.0));
        assert_eq!(maps.changed_since(0), Some(vec![area]));
        assert_eq!(maps.changed_since(1), Some(Vec::new()));
        assert_ne!(maps.hash_within(area), 0);
        assert_eq!(maps.hash_within([[5000.0, 5000.0], [5100.0, 5100.0]]), 0);
    }
}
