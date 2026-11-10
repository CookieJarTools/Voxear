//! Terrains without end: ground made round the camera as it goes.
//!
//! A terrain without end keeps a square of tiles round the camera, its
//! window, `window_tiles` each way, in its heights, as a terrain of a set
//! size keeps all of its own; `Terrain::origin` is the tile its heights
//! start at, counted from its Obj either way. As the camera goes into
//! another tile the window goes with it, keeping the tiles still in it and
//! making those that came into it.
//!
//! Each tile is made alone, on the terrain's background threads, from the
//! layers that make ground without end (`LayerKind::endless`), with its
//! surfaces' shares and its grass: every layer is worked out from where
//! each sample is, so tiles made apart meet exactly. The tiles nearest the
//! camera come first, and those round it at once. Until a tile is made it
//! is a hole: drawn as nothing, and nothing stands on it.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
};

use crate::{
    Terrain,
    composite::{self, PAD, SIDE, Sources},
    file::{Endless, TerrainFile},
    generate::WorldGen,
    grass::{GrassMap, GrassTile, grow_tile, words},
    heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_LEN, TILE_QUADS},
    import::Greys,
    layers::Area,
    raster::Rasters,
    surfaces::{ClimateSource, WeightTile, Weights, weigh_tile},
};

/// The most tiles a window is across.
pub const MOST_TILES: u32 = 33;

/// How many tiles are made at once in the background, at most: enough to
/// keep every background thread busy, few enough that tiles asked for
/// later, nearer the camera, need not wait behind many far ones.
const MOST_MAKING: usize = 32;

/// How many tiles past its window a terrain keeps those it made, so going
/// back and forth over a tile's edge does not make them again.
const KEPT_PAST: i32 = 2;

/// How many tiles across a window `endless` keeps: as far as its distance
/// reaches each way from the camera's tile, and that tile.
pub fn window_tiles(endless: &Endless) -> u32 {
    let width = TILE_QUADS as f32 * endless.spacing;
    let reach = (endless.distance / width).ceil().max(1.0) as u32;
    (2 * reach + 1).min(MOST_TILES)
}

/// The tile `local`, in metres from the terrain's Obj along x and z, is in.
pub fn tile_at(spacing: f32, local: [f32; 2]) -> [i32; 2] {
    let width = TILE_QUADS as f32 * spacing;
    local.map(|v| (v / width).floor() as i32)
}

/// The first tile of `endless`'s window round `local`, in metres from the
/// terrain's Obj: the window with `local`'s tile at its middle.
pub fn origin_round(endless: &Endless, local: [f32; 2]) -> [i32; 2] {
    let half = (window_tiles(endless) / 2) as i32;
    tile_at(endless.spacing, local).map(|v| v - half)
}

/// The slot of tile `index` of `heights`' tiles, row after row: its own
/// place for a terrain of a set size; for one without end, whose window
/// starts at tile `origin`, a slot by where the tile is, so each keeps the
/// slot it came into as the window moves, and what keeps a copy of the
/// tiles elsewhere (on the GPU, in physics) sees only the tiles that came
/// in as changed.
pub fn slot_of(heights: &Heightmap, origin: Option<[i32; 2]>, index: usize) -> usize {
    let Some([ox, oz]) = origin else { return index };
    let (across, along) = (heights.tiles_x() as i32, heights.tiles_z() as i32);
    let (tx, tz) = (index as i32 % across, index as i32 / across);
    ((tz + oz).rem_euclid(along) * across + (tx + ox).rem_euclid(across)) as usize
}

/// A tile not made yet: every sample a hole's.
pub fn unmade() -> &'static Arc<HeightTile> {
    static UNMADE: OnceLock<Arc<HeightTile>> = OnceLock::new();
    UNMADE.get_or_init(|| {
        let holes = vec![u32::MAX; HOLE_WORDS].into_boxed_slice();
        Arc::new(HeightTile::flat(0.0).with_holes(Some(holes)))
    })
}

/// Whether `tile` is one not made yet.
pub fn is_unmade(tile: &Arc<HeightTile>) -> bool {
    Arc::ptr_eq(tile, unmade())
}

fn unmade_weights() -> &'static Arc<WeightTile> {
    static UNMADE: OnceLock<Arc<WeightTile>> = OnceLock::new();
    UNMADE.get_or_init(|| Arc::new(WeightTile::all(0)))
}

fn unmade_grass(words: usize) -> Arc<GrassTile> {
    static UNMADE: OnceLock<Mutex<HashMap<usize, Arc<GrassTile>>>> = OnceLock::new();
    let kept = UNMADE.get_or_init(Default::default);
    let mut kept = kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.entry(words).or_insert_with(|| Arc::new(GrassTile::from_samples(vec![0; TILE_LEN * words]))).clone()
}

/// What a terrain without end's tiles are made from, its own, for the
/// background threads to share: its file, the pictures and generators its
/// layers name, what its brushes painted, and where its climate comes from
/// (see `surfaces::ClimateSource`).
#[derive(Clone, Default)]
pub struct Making {
    pub file: TerrainFile,
    pub pictures: HashMap<String, Arc<Greys>>,
    pub worlds: HashMap<String, Arc<WorldGen>>,
    pub rasters: Arc<Rasters>,
    pub climate: (Option<Arc<WorldGen>>, [f32; 2]),
}

impl Making {
    /// Whether it makes the same ground as `other`.
    fn same(&self, other: &Making) -> bool {
        let same_map = |a: &HashMap<String, Arc<WorldGen>>, b: &HashMap<String, Arc<WorldGen>>| {
            a.len() == b.len() && a.iter().all(|(key, world)| b.get(key).is_some_and(|other| Arc::ptr_eq(world, other)))
        };
        let same_pictures = self.pictures.len() == other.pictures.len()
            && self.pictures.iter().all(|(key, greys)| other.pictures.get(key).is_some_and(|o| Arc::ptr_eq(greys, o)));
        let same_climate = match (&self.climate.0, &other.climate.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        self.file == other.file
            && Arc::ptr_eq(&self.rasters, &other.rasters)
            && same_map(&self.worlds, &other.worlds)
            && same_pictures
            && same_climate
            && self.climate.1 == other.climate.1
    }
}

/// One tile of a terrain without end, made: its heights, and its
/// surfaces' shares and its grass where it has them.
#[derive(Clone, Debug)]
pub struct Made {
    pub heights: Arc<HeightTile>,
    pub weights: Option<Arc<WeightTile>>,
    pub grass: Option<Arc<GrassTile>>,
}

/// Tile `tile` of the terrain `making` makes, counted from its Obj either
/// way.
pub fn make_tile(making: &Making, tile: [i32; 2]) -> Made {
    let find = |reference: &str| making.pictures.get(reference).cloned();
    let world = |reference: &str| making.worlds.get(reference).cloned();
    let sources = Sources { pictures: &find, worlds: &world, rasters: &making.rasters };
    let (tx, tz) = (i64::from(tile[0]), i64::from(tile[1]));
    let padded = composite::padded_tile(&making.file, &sources, (tx, tz));
    let heights = HeightTile::from_samples(composite::inner(&padded));
    // The padded ground read by the shares and grass, as far as a sample past the tile.
    let quads = i64::from(TILE_QUADS);
    let first = [tx * quads, tz * quads];
    let (pad, side) = (i64::from(PAD), i64::from(SIDE));
    let sample = |x: i64, z: i64| {
        let (i, j) = ((x - first[0] + pad).clamp(0, side - 1), (z - first[1] + pad).clamp(0, side - 1));
        padded[(j * side + i) as usize]
    };
    let file = &making.file;
    let climate = ClimateSource { world: making.climate.0.as_deref(), position: making.climate.1 };
    let weights = (!file.surfaces.is_empty()).then(|| weigh_tile(file, &sample, &making.rasters, climate, first));
    let grass = (!file.grass.is_empty()).then(|| grow_tile(file, (&sample, weights.as_ref()), &making.rasters, first));
    Made { heights: Arc::new(heights), weights: weights.map(Arc::new), grass: grass.map(Arc::new) }
}

/// Tile `tile`'s shares and grass made again over the heights it has,
/// `around` it the heights of the tiles round it, row after row from the one
/// before it on both ways, where they are made: as after its surfaces or
/// grass changed.
fn reshare(making: &Making, tile: [i32; 2], around: &[Option<Arc<HeightTile>>]) -> Made {
    let own = around[4].clone().expect("a tile shared out again has heights");
    let quads = i64::from(TILE_QUADS);
    let first = [i64::from(tile[0]) * quads, i64::from(tile[1]) * quads];
    // Past the tiles round it, or where one is not made, its own edge.
    let sample = |x: i64, z: i64| {
        let (lx, lz) = (x - first[0], z - first[1]);
        let (gx, gz) = (lx.div_euclid(quads).clamp(-1, 1), lz.div_euclid(quads).clamp(-1, 1));
        let (gx, gz) = if lx == quads { (0, gz) } else { (gx, gz) };
        let (gx, gz) = if lz == quads { (gx, 0) } else { (gx, gz) };
        match &around[((gz + 1) * 3 + gx + 1) as usize] {
            Some(near) => {
                let (sx, sz) = ((lx - gx * quads).clamp(0, quads), (lz - gz * quads).clamp(0, quads));
                near.get(sx as u32, sz as u32)
            }
            None => own.get(lx.clamp(0, quads) as u32, lz.clamp(0, quads) as u32),
        }
    };
    let file = &making.file;
    let climate = ClimateSource { world: making.climate.0.as_deref(), position: making.climate.1 };
    let weights = (!file.surfaces.is_empty()).then(|| weigh_tile(file, &sample, &making.rasters, climate, first));
    let grass = (!file.grass.is_empty()).then(|| grow_tile(file, (&sample, weights.as_ref()), &making.rasters, first));
    Made { heights: own, weights: weights.map(Arc::new), grass: grass.map(Arc::new) }
}

/// What changed in what a terrain without end's tiles are made from, and
/// so what is made again.
#[derive(Debug, PartialEq)]
enum Change {
    /// Only its surfaces or grass: each tile's shares and grass, over the
    /// heights it has.
    Looks,
    /// Only layers that reach no further than these areas, in metres from
    /// its Obj: the tiles under them.
    Areas(Vec<Area>),
    /// Anything else: every tile.
    All,
}

/// What changed from `old` to `new`.
fn change_between(old: &Making, new: &Making) -> Change {
    let (a, b) = (&old.file, &new.file);
    let sources_same = Making { file: a.clone(), ..new.clone() }.same(old);
    let ground_same = |a: &TerrainFile, b: &TerrainFile| {
        (a.base, a.endless, a.holes, a.size, a.resolution) == (b.base, b.endless, b.holes, b.size, b.resolution)
    };
    if !sources_same || !ground_same(a, b) {
        return Change::All;
    }
    if a.layers == b.layers {
        return Change::Looks;
    }
    if a.surfaces != b.surfaces || a.grass != b.grass {
        return Change::All;
    }
    // The layers changed, added or taken away between those the same before and after, where each reaches.
    let (a, b) = (&a.layers, &b.layers);
    let before = a.iter().zip(b).take_while(|(was, now)| was == now).count();
    let after = a[before..].iter().rev().zip(b[before..].iter().rev()).take_while(|(was, now)| was == now).count();
    let changed = a[before..a.len() - after].iter().chain(&b[before..b.len() - after]);
    let areas: Option<Vec<Area>> = changed.map(|layer| layer.kind.area()).collect();
    areas.map_or(Change::All, Change::Areas)
}

/// One tile kept: what was made, and from which of what tiles were made
/// from its heights and its shares and grass are, by number.
struct Kept {
    made: Made,
    ground: u64,
    looks: u64,
}

/// What a background thread did: made a tile whole, or only its shares and
/// grass again; the numbers of what its heights and looks were made from;
/// which tile; and what it made.
struct Finished {
    whole: bool,
    ground: u64,
    looks: u64,
    at: [i32; 2],
    made: Made,
}

/// Keeps a terrain without end's window round the camera: the tiles it
/// made, those being made in the background, and what they are made from.
pub struct Streamer {
    making: Option<Arc<Making>>,
    /// What tiles' heights, and their shares and grass, are made from now,
    /// by number. A tile made from earlier is shown until it is made again.
    ground: u64,
    looks: u64,
    /// The number of what tiles are made from now, either part, which a
    /// background thread checks before it starts.
    latest: Arc<AtomicU64>,
    made: HashMap<[i32; 2], Kept>,
    asked: HashSet<[i32; 2]>,
    /// How many tiles of the window wait to be asked for.
    waiting: usize,
    making_now: Arc<AtomicUsize>,
    send: mpsc::Sender<Finished>,
    finished: Mutex<mpsc::Receiver<Finished>>,
}

impl Default for Streamer {
    fn default() -> Self {
        let (send, finished) = mpsc::channel();
        Self {
            making: None,
            ground: 0,
            looks: 0,
            latest: Arc::new(AtomicU64::new(0)),
            made: HashMap::new(),
            asked: HashSet::new(),
            waiting: 0,
            making_now: Arc::new(AtomicUsize::new(0)),
            send,
            finished: Mutex::new(finished),
        }
    }
}

impl Streamer {
    /// Whether every tile of the window is made from what it is made from
    /// now, and none is being made.
    pub fn settled(&self) -> bool {
        self.waiting == 0 && self.asked.is_empty() && self.making_now.load(Ordering::Acquire) == 0
    }

    /// Brings `terrain`'s window round `viewer`, in metres from its Obj
    /// along x and z, made by `making`: the tiles made in the background
    /// since taken in, those round the viewer made at once, and the rest
    /// asked for, nearest first. True if its ground changed. A terrain
    /// whose file is not without end is left as it is.
    pub fn update(&mut self, terrain: &mut Terrain, making: Making, viewer: [f32; 2]) -> bool {
        let Some(endless) = making.file.endless else { return false };
        self.follow(making);
        let making = self.making.clone().expect("kept by follow");
        self.take_finished();
        let count = window_tiles(&endless) as i32;
        let origin = origin_round(&endless, viewer);
        let middle = tile_at(endless.spacing, viewer);
        // Round the viewer, at once, those not made from anything yet: the ground it stands on, and what it sees
        // nearest.
        let near: Vec<[i32; 2]> = (-1..=1)
            .flat_map(|z| (-1..=1).map(move |x| [middle[0] + x, middle[1] + z]))
            .filter(|at| !self.made.contains_key(at))
            .collect();
        for (at, made) in near.iter().zip(crate::workers::each(&near, |&at| make_tile(&making, at))) {
            self.made.insert(*at, Kept { made, ground: self.ground, looks: self.looks });
            self.asked.remove(at);
        }
        // The rest in the background, nearest first, a few at a time.
        let stale =
            |kept: Option<&Kept>| kept.is_none_or(|kept| kept.ground != self.ground || kept.looks != self.looks);
        let mut wanted: Vec<[i32; 2]> = (0..count)
            .flat_map(|z| (0..count).map(move |x| [origin[0] + x, origin[1] + z]))
            .filter(|at| stale(self.made.get(at)) && !self.asked.contains(at))
            .collect();
        wanted.sort_by_key(|at| (at[0] - middle[0]).pow(2) + (at[1] - middle[1]).pow(2));
        let room = MOST_MAKING.saturating_sub(self.making_now.load(Ordering::Acquire));
        self.waiting = wanted.len().saturating_sub(room);
        for at in wanted.into_iter().take(room) {
            self.ask(&making, at);
        }
        // Tiles far behind are let go.
        let keeps = |at: &[i32; 2]| {
            (origin[0] - KEPT_PAST..origin[0] + count + KEPT_PAST).contains(&at[0])
                && (origin[1] - KEPT_PAST..origin[1] + count + KEPT_PAST).contains(&at[1])
        };
        self.made.retain(|at, _| keeps(at));
        self.asked.retain(|at| keeps(at));
        self.show(terrain, &making.file, origin, count as u32)
    }

    /// Takes `making` as what tiles are made from, if it is not already,
    /// and marks what it changed to be made again: the tiles a layer moved
    /// over, only the shares and grass where surfaces or grass changed, or
    /// everything.
    fn follow(&mut self, making: Making) {
        if self.making.as_ref().is_some_and(|kept| kept.same(&making)) {
            return;
        }
        let change = self.making.as_ref().map_or(Change::All, |kept| change_between(kept, &making));
        // Tiles as fine as another spacing are of no use at this one, even until made again.
        if self.making.as_ref().is_some_and(|kept| kept.file.spacing() != making.file.spacing()) {
            self.made.clear();
        }
        let number = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        let was = (self.ground, self.looks);
        let spacing = making.file.spacing();
        match change {
            Change::Looks => self.looks = number,
            Change::All => (self.ground, self.looks) = (number, number),
            Change::Areas(areas) => {
                (self.ground, self.looks) = (number, number);
                // Tiles no layer moved over are as they were, and are kept as made from this.
                let width = TILE_QUADS as f32 * spacing;
                let reach = (PAD + 1) as f32 * spacing;
                for (at, kept) in &mut self.made {
                    let [x0, z0] = [at[0] as f32 * width - reach, at[1] as f32 * width - reach];
                    let [x1, z1] = [x0 + width + 2.0 * reach, z0 + width + 2.0 * reach];
                    let under = areas.iter().any(|a| a[0] <= x1 && a[2] >= x0 && a[1] <= z1 && a[3] >= z0);
                    if !under && (kept.ground, kept.looks) == was {
                        (kept.ground, kept.looks) = (number, number);
                    }
                }
            }
        }
        self.making = Some(Arc::new(making));
        self.asked.clear();
    }

    /// Asks a background thread to make tile `at`: whole where its heights
    /// are stale, or else only its shares and grass, over its heights and
    /// those round it.
    fn ask(&mut self, making: &Arc<Making>, at: [i32; 2]) {
        let whole = self.made.get(&at).is_none_or(|kept| kept.ground != self.ground);
        let around: Option<Vec<Option<Arc<HeightTile>>>> = (!whole).then(|| {
            (-1..=1)
                .flat_map(|z| (-1..=1).map(move |x| [at[0] + x, at[1] + z]))
                .map(|near| self.made.get(&near).map(|kept| kept.made.heights.clone()))
                .collect()
        });
        self.asked.insert(at);
        self.making_now.fetch_add(1, Ordering::AcqRel);
        let (making, latest, making_now, send) =
            (making.clone(), self.latest.clone(), self.making_now.clone(), self.send.clone());
        let (ground, looks, number) = (self.ground, self.looks, self.ground.max(self.looks));
        crate::workers::spawn(move || {
            // Not made at all when what it would be made from has changed since it was asked for.
            if latest.load(Ordering::Acquire) == number {
                let made = match around {
                    None => make_tile(&making, at),
                    Some(around) => reshare(&making, at, &around),
                };
                let _ = send.send(Finished { whole, ground, looks, at, made });
            }
            making_now.fetch_sub(1, Ordering::AcqRel);
        });
    }

    /// Takes in what the background threads made: tiles made from what
    /// tiles are made from now, and older ones where nothing is made yet.
    fn take_finished(&mut self) {
        let finished = self.finished.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(done) = finished.try_recv() {
            let current = done.ground == self.ground && done.looks == self.looks;
            if current {
                self.asked.remove(&done.at);
            }
            match self.made.get_mut(&done.at) {
                // Its shares and grass again, over the heights it has.
                Some(kept) if !done.whole => {
                    if current && kept.ground == done.ground {
                        kept.made = done.made;
                        kept.looks = done.looks;
                    }
                }
                Some(kept) if current => *kept = Kept { made: done.made, ground: done.ground, looks: done.looks },
                Some(_) => {}
                None if done.whole => {
                    self.made.insert(done.at, Kept { made: done.made, ground: done.ground, looks: done.looks });
                }
                None => {}
            }
        }
    }

    /// Puts the window starting at tile `origin`, `count` tiles across,
    /// into `terrain`: the tiles made, holes where none is yet. True if its
    /// ground changed.
    fn show(&self, terrain: &mut Terrain, file: &TerrainFile, origin: [i32; 2], count: u32) -> bool {
        let spacing = file.spacing();
        let words = (!file.grass.is_empty()).then(|| words(file.grass.len()));
        let mut heights = Vec::with_capacity((count * count) as usize);
        let mut weights = Vec::with_capacity(heights.capacity());
        let mut grass = Vec::with_capacity(heights.capacity());
        for z in 0..count as i32 {
            for x in 0..count as i32 {
                let made = self.made.get(&[origin[0] + x, origin[1] + z]).map(|kept| &kept.made);
                heights.push(made.map_or_else(|| unmade().clone(), |made| made.heights.clone()));
                // Shares and grass made with other surfaces and grass than these are left out until made again.
                let shares = made.and_then(|made| made.weights.clone()).filter(|_| !file.surfaces.is_empty());
                weights.push(shares.unwrap_or_else(|| unmade_weights().clone()));
                if let Some(words) = words {
                    let grown =
                        made.and_then(|made| made.grass.clone()).filter(|g| g.samples().len() == TILE_LEN * words);
                    grass.push(grown.unwrap_or_else(|| unmade_grass(words)));
                }
            }
        }
        let same_tiles = |a: &[Arc<HeightTile>], b: &[Arc<HeightTile>]| a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b));
        let fits = terrain.origin == Some(origin)
            && terrain.heights.tiles_x() == count
            && terrain.heights.tiles_z() == count
            && terrain.heights.spacing() == spacing;
        let same = fits
            && same_tiles(terrain.heights.tiles(), &heights)
            && match (&terrain.weights, file.surfaces.is_empty()) {
                (Some(kept), false) => kept.tiles().iter().zip(&weights).all(|(a, b)| Arc::ptr_eq(a, b)),
                (kept, empty) => kept.is_none() && empty,
            }
            && match (&terrain.grass, words) {
                (Some(kept), Some(words)) => {
                    kept.words() == words && kept.tiles().iter().zip(&grass).all(|(a, b)| Arc::ptr_eq(a, b))
                }
                (kept, words) => kept.is_none() && words.is_none(),
            };
        if same {
            return false;
        }
        // The same terrain, its id kept, where it is as large and as fine.
        let keeps_id = terrain.heights.tiles_x() == count
            && terrain.heights.tiles_z() == count
            && terrain.heights.spacing() == spacing;
        terrain.heights = Arc::new(if keeps_id {
            terrain.heights.with_shared_tiles(heights)
        } else {
            Heightmap::from_tiles(count, count, spacing, heights)
        });
        terrain.weights = (!file.surfaces.is_empty()).then(|| Arc::new(Weights::from_tiles(count, count, weights)));
        terrain.grass = words.map(|words| Arc::new(GrassMap::from_tiles(count, count, words, grass)));
        terrain.dug = None;
        terrain.origin = Some(origin);
        true
    }
}

/// Holes where nothing is made yet, for a terrain without end before its
/// first tile is: a window of `file`'s round its Obj, with its corner tile.
pub fn unmade_window(file: &TerrainFile) -> Option<(Heightmap, [i32; 2])> {
    let endless = file.endless?;
    let count = window_tiles(&endless);
    let tiles = vec![unmade().clone(); (count * count) as usize];
    Some((Heightmap::from_tiles(count, count, endless.spacing, tiles), origin_round(&endless, [0.0, 0.0])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generate::Preset,
        layers::{Layer, LayerKind},
        surfaces::{Cover, Surface},
    };

    fn endless_file(spacing: f32, distance: f32) -> Making {
        let generate = Layer::new(LayerKind::Generate { world: Some("hills".into()), position: [300.0, -200.0] });
        let mut rock = Surface::new(None);
        rock.cover = Cover::Rules;
        rock.mask.slope = Some([20.0, 90.0]);
        rock.breakup = 2.0;
        let file = TerrainFile {
            layers: vec![generate],
            surfaces: vec![Surface::new(None), rock],
            endless: Some(Endless { distance, spacing }),
            ..TerrainFile::default()
        };
        let mut worlds = HashMap::new();
        worlds.insert("hills".to_string(), Arc::new(Preset::Alpine.world()));
        Making { file, worlds, ..Making::default() }
    }

    #[test]
    fn a_tile_keeps_its_slot_as_the_window_moves() {
        let heights = Heightmap::flat(3, 3, 1.0, 0.0);
        assert_eq!(slot_of(&heights, None, 7), 7);
        // Tile (4, -2) of the world is the window's (1, 0) from (3, -2), and its (0, 1) from (4, -3).
        assert_eq!(slot_of(&heights, Some([3, -2]), 1), slot_of(&heights, Some([4, -3]), 3));
        let slots: HashSet<usize> = (0..9).map(|index| slot_of(&heights, Some([-5, 7]), index)).collect();
        assert_eq!(slots.len(), 9, "every slot once");
    }

    #[test]
    fn a_window_reaches_as_far_as_its_distance_round_the_cameras_tile() {
        let endless = Endless { distance: 1500.0, spacing: 1.0 };
        assert_eq!(window_tiles(&endless), 13, "six tiles of 256 m each way, and the camera's");
        assert_eq!(window_tiles(&Endless { distance: 100.0, spacing: 1.0 }), 3);
        assert_eq!(window_tiles(&Endless { distance: 100_000.0, spacing: 0.25 }), MOST_TILES);
        assert_eq!(tile_at(1.0, [-0.5, 256.0]), [-1, 1]);
        assert_eq!(origin_round(&endless, [10.0, -10.0]), [-6, -7]);
    }

    #[test]
    fn tiles_made_apart_meet_exactly_and_their_shares_follow_the_ground_across_the_edge() {
        let making = endless_file(4.0, 1000.0);
        let (left, right) = (make_tile(&making, [-3, 5]), make_tile(&making, [-2, 5]));
        let (below, above) = (make_tile(&making, [-3, 4]), make_tile(&making, [-3, 5]));
        let last = TILE_QUADS;
        for i in 0..=last {
            assert_eq!(left.heights.get(last, i), right.heights.get(0, i), "the edge along z, sample {i}");
            assert_eq!(below.heights.get(i, last), above.heights.get(i, 0), "the edge along x, sample {i}");
            let (a, b) = (left.weights.as_ref().unwrap(), right.weights.as_ref().unwrap());
            assert_eq!(a.shares(last, i), b.shares(0, i), "shares on the edge, sample {i}");
        }
        // The same ground as a terrain of a set size made from the same layers, where both reach.
        let set = TerrainFile { size: 2048.0, resolution: 513, endless: None, ..making.file.clone() };
        let find = |reference: &str| making.worlds.get(reference).cloned();
        let sources = Sources { pictures: &composite::no_pictures, worlds: &find, rasters: &Rasters::default() };
        let whole = composite::Compositor::default().composite(&set, &sources, None, &composite::Dirty::All);
        let corner = make_tile(&making, [1, 0]);
        for i in (0..TILE_QUADS).step_by(17) {
            assert_eq!(corner.heights.get(i, 9), whole.tile(1, 0).get(i, 9));
        }
        // And some of it rock, where it is steep.
        let rocky = (0..TILE_LEN as u32).filter(|i| left.weights.as_ref().unwrap().shares(i % 257, i / 257)[0].0 == 1);
        assert!(rocky.count() > 0);
    }

    #[test]
    fn a_change_remakes_only_what_it_reaches() {
        let making = endless_file(4.0, 1000.0);
        let mut terrain = Terrain { origin: Some([0, 0]), ..Terrain::new(unmade_window(&making.file).unwrap().0) };
        let mut streamer = Streamer::default();
        streamer.update(&mut terrain, making.clone(), [10.0, 10.0]);
        let before = terrain.clone();
        let settle = |streamer: &mut Streamer, terrain: &mut Terrain, making: &Making| {
            for _ in 0..1000 {
                streamer.update(terrain, making.clone(), [10.0, 10.0]);
                if streamer.settled() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("never settled");
        };
        // A stamp added on tile (1, 1), 1024 m a side: that tile and those its edge is near made again, the rest kept.
        let mut stamped = making.clone();
        let stamp = LayerKind::Stamp {
            picture: None,
            position: [1536.0, 1536.0],
            rotation: 0.0,
            size: [200.0, 200.0],
            height: 30.0,
            falloff: 0.5,
        };
        stamped.file.layers.push(Layer::new(stamp));
        assert!(matches!(change_between(&making, &stamped), Change::Areas(_)));
        settle(&mut streamer, &mut terrain, &stamped);
        let kept = |window: &Terrain, x: u32, z: u32| Arc::ptr_eq(window.heights.tile(x, z), before.heights.tile(x, z));
        assert!(!kept(&terrain, 2, 2), "tile (1, 1), third in the window from (-1, -1)");
        assert!(kept(&terrain, 0, 0) && kept(&terrain, 1, 1), "tiles the stamp is far from");
        // A surface's rule changed: shares made again, over the same heights.
        let mut steeper = stamped.clone();
        steeper.file.surfaces[1].mask.slope = Some([40.0, 90.0]);
        assert_eq!(change_between(&stamped, &steeper), Change::Looks);
        let heights = terrain.heights.clone();
        let shares = terrain.weights.clone().unwrap();
        settle(&mut streamer, &mut terrain, &steeper);
        assert!(heights.tiles().iter().zip(terrain.heights.tiles()).all(|(a, b)| Arc::ptr_eq(a, b)));
        let rock = |weights: &Weights| {
            weights.tiles().iter().map(|tile| tile.samples().iter().filter(|s| s[0] & 0xFF == 1).count()).sum::<usize>()
        };
        assert!(rock(terrain.weights.as_ref().unwrap()) < rock(&shares), "less of it steep enough for rock");
        // The same as made whole, across a tile's edge too.
        let whole = make_tile(&steeper, [0, 0]);
        assert_eq!(terrain.weights.as_ref().unwrap().tile(1, 1).samples(), whole.weights.unwrap().samples());
    }

    #[test]
    fn a_window_follows_the_camera_keeping_the_tiles_still_in_it() {
        let making = endless_file(4.0, 1000.0);
        let count = window_tiles(&making.file.endless.unwrap());
        assert_eq!(count, 3);
        let (window, origin) = unmade_window(&making.file).unwrap();
        let mut terrain = Terrain { origin: Some(origin), ..Terrain::new(window) };
        let mut streamer = Streamer::default();
        // Round the camera, the ground made at once; the 3 by 3 window is all of that.
        assert!(streamer.update(&mut terrain, making.clone(), [10.0, 10.0]));
        assert_eq!(terrain.origin, Some([-1, -1]));
        assert!(terrain.heights.tiles().iter().all(|tile| !is_unmade(tile)));
        assert!(!terrain.heights.has_holes());
        let id = terrain.heights.id();
        let kept = terrain.heights.tile(1, 1).clone();
        // A tile on: the window goes with it, the tiles still in it kept, those come into it made.
        let width = TILE_QUADS as f32 * 4.0;
        assert!(streamer.update(&mut terrain, making.clone(), [10.0 + width, 10.0]));
        assert_eq!(terrain.origin, Some([0, -1]));
        assert_eq!(terrain.heights.id(), id, "the same terrain, moved");
        assert!(Arc::ptr_eq(terrain.heights.tile(0, 1), &kept));
        let ground = terrain.heights.height_at(width + 8.0, width + 8.0);
        let made = make_tile(&making, [1, 0]);
        assert!((ground - made.heights.get(2, 2)).abs() < 1e-3);
        assert!(!streamer.update(&mut terrain, making.clone(), [12.0 + width, 10.0]), "nothing changed");
        // A layer changed: every tile is made again from it, the old ones shown till then.
        let mut changed = making.clone();
        if let LayerKind::Generate { position, .. } = &mut changed.file.layers[0].kind {
            position[0] += 1000.0;
        }
        streamer.update(&mut terrain, changed.clone(), [12.0 + width, 10.0]);
        for _ in 0..500 {
            if streamer.settled() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            streamer.update(&mut terrain, changed.clone(), [12.0 + width, 10.0]);
        }
        streamer.update(&mut terrain, changed.clone(), [12.0 + width, 10.0]);
        assert!(streamer.settled());
        assert_eq!(terrain.heights.tile(1, 1).get(3, 3), make_tile(&changed, [1, 0]).heights.get(3, 3));
    }
}
