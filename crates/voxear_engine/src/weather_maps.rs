//! What the weather has left lying by place, what keeps it off, and how
//! deep the sea is, round the viewer: the renderer's
//! [`WeatherMaps`](voxear_render::weather_maps::WeatherMaps), filled a
//! share at a time each frame.
//!
//! - **Lying:** every cell, tens of metres across, keeps how wet its ground
//!   is, how much snow lies on it and has fallen lately, and how much of the
//!   sea is frozen, worked out from the weather there ([`weather_at`]: the
//!   level's own and every storm's) and how warm the open air is there
//!   (`crate::heat::open_air`). Rain soaks the ground and it dries, faster
//!   where it is warm and dry; snow settles where it is below freezing and
//!   melts where it is not; where it is bitter, snow lies whatever the
//!   weather; and the sea freezes where the air over it is a couple of
//!   degrees below freezing, and thaws above it. A cell seen for the first
//!   time starts as the weather would have left it, so a cold sea is frozen
//!   as the level loads.
//! - **Cover:** the height of the highest thing over each half metre near
//!   the viewer: the highest ground block of every world of blocks, and
//!   what any collider that does not move meets a ray down. Rain and snow
//!   end on it, and what is under it stays dry.
//! - **Sea floor:** how deep a world of blocks' sea is, from its top down
//!   to its highest ground block, every few metres round the viewer.
//!
//! Nothing here is saved or sent: every machine works it out for itself.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::{DVec2, DVec3, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::sea::SeaLevel;
pub use voxear_render::weather_maps::{
    COVER_CELL, COVER_SIDE, FLOOR_CELL, FLOOR_REACH, FLOOR_SIDE, Grid, LYING_CELL, LYING_SIDE, Lying, OPEN,
    WeatherMaps, lasting_snow,
};
use voxear_scene::{Destroying, Transform, WorldOrigin, WorldTransform};
use voxear_terrain::blocks::{BlockWorld, chunk::CHUNK, is_ground};

use crate::storms::{SeaNow, Storms, WeatherAt, weather_at};

/// How many cells of each grid are worked out a frame, at the most: those
/// not yet known first, then the rest in turn.
const LYING_EACH_FRAME: usize = 64;
const COVER_EACH_FRAME: usize = 1024;
const FLOOR_EACH_FRAME: usize = 256;
/// How far over and under the viewer cover is looked for, in metres.
const COVER_REACH: f32 = 40.0;
/// How far over and under the viewer the ground is looked for, for a lying
/// cell's height, in metres.
const GROUND_REACH: f32 = 300.0;
/// How far from the sea's top the viewer may be for its floor to be looked
/// for, in metres.
const FLOOR_FROM: f32 = 250.0;

/// How many seconds rain takes to soak dry ground, and wet ground to dry
/// where it is mild; snow to settle as deep as it will and to melt, and to
/// melt in heavy rain; what has fallen to be forgotten; and the sea to
/// freeze over and to thaw.
const SOAK: f32 = 25.0;
const DRY: f32 = 180.0;
const SETTLE: f32 = 90.0;
const MELT: f32 = 240.0;
const WASH: f32 = 40.0;
const FORGET: f32 = 1200.0;
const FREEZE: f32 = 300.0;
const THAW: f32 = 240.0;
/// The sea freezes over where the air over it is this cold, in °C, and
/// thaws where it is this warm; between, it stays as it is.
pub const SEA_FREEZES: f32 = -2.0;
pub const SEA_THAWS: f32 = 0.5;
/// Snow melts above this, in °C.
const MELTS_ABOVE: f32 = 0.5;
/// The most seconds one cell is moved on at once.
const LONGEST_STEP: f32 = 30.0;

/// A grid that follows the viewer, held to whole cells of the level, so a
/// cell keeps what it holds as the viewer moves on.
struct Rolling<T> {
    side: usize,
    cell: f32,
    /// The cell of the level at its corner.
    anchor: Option<[i64; 2]>,
    cells: Vec<T>,
    known: Vec<bool>,
    next: usize,
}

impl<T: Copy + Default> Rolling<T> {
    fn new(side: usize, cell: f32) -> Self {
        Self {
            side,
            cell,
            anchor: None,
            cells: vec![T::default(); side * side],
            known: vec![false; side * side],
            next: 0,
        }
    }

    /// Moves it so `around`, in the level, is at its middle: the cells still
    /// in it keep what they hold, and the rest are to be worked out.
    fn follow(&mut self, around: DVec2) {
        let half = (self.side / 2) as i64;
        let cell = f64::from(self.cell);
        let anchor = [(around.x / cell).floor() as i64 - half, (around.y / cell).floor() as i64 - half];
        if self.anchor == Some(anchor) {
            return;
        }
        let side = self.side as i64;
        let mut cells = vec![T::default(); self.side * self.side];
        let mut known = vec![false; self.side * self.side];
        if let Some(old) = self.anchor {
            for j in 0..side {
                for i in 0..side {
                    let (oi, oj) = (anchor[0] + i - old[0], anchor[1] + j - old[1]);
                    if (0..side).contains(&oi) && (0..side).contains(&oj) {
                        let (from, to) = ((oj * side + oi) as usize, (j * side + i) as usize);
                        (cells[to], known[to]) = (self.cells[from], self.known[from]);
                    }
                }
            }
        }
        (self.anchor, self.cells, self.known) = (Some(anchor), cells, known);
    }

    /// The middle of cell `index`, in the level.
    fn middle(&self, index: usize) -> DVec2 {
        let anchor = self.anchor.unwrap_or_default();
        let (i, j) = ((index % self.side) as i64, (index / self.side) as i64);
        DVec2::new((anchor[0] + i) as f64 + 0.5, (anchor[1] + j) as f64 + 0.5) * f64::from(self.cell)
    }

    /// Works out up to `budget` cells by `work`, given each one's middle in
    /// the level and what it held, if it was known: those not yet known
    /// first, then the rest in turn.
    fn refresh(&mut self, budget: usize, mut work: impl FnMut(DVec2, Option<T>) -> T) {
        let mut done = 0;
        for index in 0..self.cells.len() {
            if done >= budget {
                return;
            }
            if !self.known[index] {
                self.cells[index] = work(self.middle(index), None);
                self.known[index] = true;
                done += 1;
            }
        }
        for _ in done..budget.min(self.cells.len()) {
            let index = self.next;
            self.next = (self.next + 1) % self.cells.len();
            self.cells[index] = work(self.middle(index), Some(self.cells[index]));
        }
    }

    /// Whether every cell has been worked out at least once.
    fn all_known(&self) -> bool {
        self.known.iter().all(|&known| known)
    }

    /// As the renderer reads it: its corner in the world, where the level's
    /// middle is at `origin`.
    fn grid<U>(&self, origin: DVec3, cell: impl Fn(T) -> U) -> Grid<U> {
        let anchor = self.anchor.unwrap_or_default();
        let corner =
            DVec2::new(anchor[0] as f64, anchor[1] as f64) * f64::from(self.cell) - DVec2::new(origin.x, origin.z);
        Grid {
            corner: corner.as_vec2(),
            cell: self.cell,
            side: self.side,
            cells: self.cells.iter().map(|&c| cell(c)).collect(),
        }
    }
}

/// A lying cell as kept: what lies, and when it was last moved on, on the
/// maps' own clock.
#[derive(Clone, Copy, Default)]
struct Kept {
    lying: Lying,
    at: f64,
}

/// Each chunk's columns' highest ground block, a chunk of a world of blocks
/// at a time, kept while the chunk is the same: from 1 for its lowest
/// block, 0 for a column of none.
#[derive(Default)]
struct Tops {
    chunks: HashMap<(u64, [i32; 3]), (Arc<voxear_terrain::blocks::chunk::Chunk>, Arc<[u8]>, u64)>,
    frame: u64,
}

impl Tops {
    /// The highest ground block in column `x`, `z` of `blocks`, in blocks
    /// from its corner, from block `from` down to block `to`: a chunk at a
    /// time, so the chunk `from` is in counts whole.
    fn top(&mut self, owner: Entity, blocks: &BlockWorld, [x, z]: [i32; 2], from: i32, to: i32) -> Option<i32> {
        let side = CHUNK as i32;
        let (cx, cz) = (x.div_euclid(side), z.div_euclid(side));
        let (lx, lz) = (x.rem_euclid(side) as usize, z.rem_euclid(side) as usize);
        for cy in (to.div_euclid(side)..=from.div_euclid(side)).rev() {
            let key = [cx, cy, cz];
            let Some(chunk) = blocks.blocks.chunk(key) else { continue };
            let tops = self.of(owner, key, chunk, &blocks.faces);
            let top = tops[lz * CHUNK as usize + lx];
            if top > 0 {
                return Some(cy * side + i32::from(top) - 1);
            }
        }
        None
    }

    fn of(
        &mut self,
        owner: Entity,
        key: [i32; 3],
        chunk: &Arc<voxear_terrain::blocks::chunk::Chunk>,
        faces: &[voxear_terrain::blocks::BlockFaces],
    ) -> Arc<[u8]> {
        let frame = self.frame;
        if let Some((kept, tops, used)) = self.chunks.get_mut(&(owner.to_bits(), key))
            && Arc::ptr_eq(kept, chunk)
        {
            *used = frame;
            return tops.clone();
        }
        let side = CHUNK as usize;
        let mut tops = vec![0u8; side * side];
        if !chunk.is_air() {
            for z in 0..side {
                for x in 0..side {
                    let found = (0..side).rev().find(|&y| is_ground(faces, chunk.get(x as u32, y as u32, z as u32)));
                    tops[z * side + x] = found.map_or(0, |y| y as u8 + 1);
                }
            }
        }
        let tops: Arc<[u8]> = tops.into();
        self.chunks.insert((owner.to_bits(), key), (chunk.clone(), tops.clone(), frame));
        tops
    }

    /// Lets go of chunks not looked at for a while, which may have been
    /// changed or let go by their world.
    fn forget_old(&mut self) {
        self.frame += 1;
        if self.frame % 240 == 0 {
            let frame = self.frame;
            self.chunks.retain(|_, (.., used)| frame - *used < 600);
        }
    }
}

/// The grids as they are kept between frames, a world resource; and the
/// slabs of ice that bear what stands on a frozen sea, by their cell of the
/// lying grid.
struct MapsKept {
    lying: Rolling<Kept>,
    cover: Rolling<f32>,
    floor: Rolling<f32>,
    tops: Tops,
    clock: f64,
    slabs: HashMap<[i64; 2], Entity>,
}

impl Default for MapsKept {
    fn default() -> Self {
        Self {
            lying: Rolling::new(LYING_SIDE, LYING_CELL),
            cover: Rolling::new(COVER_SIDE, COVER_CELL),
            floor: Rolling::new(FLOOR_SIDE, FLOOR_CELL),
            tops: Tops::default(),
            clock: 0.0,
            slabs: HashMap::new(),
        }
    }
}

/// A world of blocks, where its corner is, and its sea's top in the world
/// where it has one.
struct Blocks<'a> {
    entity: Entity,
    blocks: &'a BlockWorld,
    corner: Vec3,
    sea: Option<f32>,
}

fn block_worlds(world: &World) -> Vec<Blocks<'_>> {
    let mut found: Vec<Blocks> = world
        .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>, Option<&SeaLevel>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, blocks, local, placed, sea)| {
            let corner = voxear_terrain::corner(local, placed);
            Blocks { entity, blocks, corner, sea: sea.map(|sea| corner.y + sea.0) }
        })
        .collect();
    found.sort_by_key(|blocks| blocks.entity.to_bits());
    found
}

/// The highest ground over `at`, in the world, from `over` metres above it
/// down to `under` below it, of every world of blocks: none where there is
/// none.
fn ground_top(tops: &mut Tops, worlds: &[Blocks], at: Vec3, over: f32, under: f32) -> Option<f32> {
    let mut highest: Option<f32> = None;
    for blocks in worlds {
        let size = blocks.blocks.block_size.max(0.01);
        let local = at - blocks.corner;
        let column = [(local.x / size).floor() as i32, (local.z / size).floor() as i32];
        let from = ((local.y + over) / size).floor() as i32;
        let to = ((local.y - under) / size).floor() as i32;
        if let Some(top) = tops.top(blocks.entity, blocks.blocks, column, from, to) {
            let height = blocks.corner.y + (top + 1) as f32 * size;
            highest = Some(highest.map_or(height, |h| h.max(height)));
        }
    }
    highest
}

/// The highest collider that does not move under `from`, in the world,
/// within `reach` metres down: what a ray straight down meets first,
/// passing by bodies that move and characters.
fn collider_under(world: &World, physics: &crate::physics::PhysicsScene, from: Vec3, reach: f32) -> Option<f32> {
    let mut start = from;
    let mut left = reach;
    for _ in 0..4 {
        let hit = physics.raycast(start, Vec3::NEG_Y, left, u32::MAX, false)?;
        let moves = world.get::<crate::physics::Rigidbody>(hit.obj).is_some_and(|body| !body.kinematic)
            || world.get::<crate::physics::CharacterController>(hit.obj).is_some();
        if !moves {
            return Some(hit.point.y);
        }
        start = hit.point - Vec3::Y * 0.05;
        left -= hit.distance + 0.05;
        if left <= 0.0 {
            return None;
        }
    }
    None
}

/// How much of a lying cell's sea must be frozen for its ice to bear what
/// stands on it, and how thick a slab of it is, in metres.
pub const ICE_BEARS: f32 = 0.95;
const SLAB: f32 = 0.6;

/// Keeps a slab of ice under every cell of frozen sea round the viewer and
/// every character, so they walk on it rather than swim under it: an Obj of
/// the engine's own with a box collider, in no scene. Its top is the sea's.
fn bear_on_ice(world: &mut World, kept: &mut MapsKept, sea: Option<f32>, viewer: Vec3) {
    let origin = WorldOrigin::of(world);
    let mut wanted: Vec<[i64; 2]> = Vec::new();
    if let (Some(top), Some(anchor)) = (sea, kept.lying.anchor) {
        let mut standers = vec![viewer];
        standers.extend(
            world
                .query_ref::<(&crate::physics::CharacterController, &Transform)>()
                .into_iter()
                .map(|(_, transform)| transform.position),
        );
        let cell = f64::from(LYING_CELL);
        for at in standers.into_iter().filter(|at| (at.y - top).abs() < 8.0) {
            let level = origin.to_level(at);
            let middle = [(level.x / cell).floor() as i64, (level.z / cell).floor() as i64];
            for (dx, dz) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dz| (dx, dz))) {
                let key = [middle[0] + dx, middle[1] + dz];
                let (i, j) = (key[0] - anchor[0], key[1] - anchor[1]);
                let side = LYING_SIDE as i64;
                if !(0..side).contains(&i) || !(0..side).contains(&j) || wanted.contains(&key) {
                    continue;
                }
                let index = (j * side + i) as usize;
                if kept.lying.known[index] && kept.lying.cells[index].lying.ice >= ICE_BEARS {
                    wanted.push(key);
                }
            }
        }
    }
    kept.slabs.retain(|key, entity| {
        let keep = wanted.contains(key) && world.is_alive(*entity);
        if !keep {
            world.despawn(*entity);
        }
        keep
    });
    let Some(top) = sea else { return };
    for key in wanted {
        let middle = DVec3::new(
            (key[0] as f64 + 0.5) * f64::from(LYING_CELL),
            0.0,
            (key[1] as f64 + 0.5) * f64::from(LYING_CELL),
        );
        let place =
            Vec3::new((middle.x - origin.0.x as f64) as f32, top - SLAB * 0.5, (middle.z - origin.0.z as f64) as f32);
        match kept.slabs.get(&key) {
            Some(&slab) => {
                if let Some(transform) = world.get_mut::<Transform>(slab)
                    && transform.position != place
                {
                    transform.position = place;
                }
            }
            None => {
                let collider = crate::physics::Collider {
                    size: [LYING_CELL, SLAB, LYING_CELL],
                    friction: 0.05,
                    ..Default::default()
                };
                let slab = world.spawn((
                    voxear_scene::Name("Sea Ice".into()),
                    collider,
                    Transform::from_position(place),
                    voxear_scene::EditorOnly,
                ));
                kept.slabs.insert(key, slab);
            }
        }
    }
}

/// A lying cell `delta` seconds on, under the weather `at` gives, where
/// the air over the sea is `sea` warm; or, for a cell not yet known, as
/// that weather would have left it.
fn lie(was: Option<Lying>, at: &WeatherAt, sea: Option<f32>, height: f32, delta: f32) -> Lying {
    let WeatherAt { amounts, celsius, humidity, .. } = *at;
    let (rain, snowing) = (amounts.rain, amounts.snow);
    let lasting = lasting_snow(celsius);
    let snow_wanted = if rain > 0.0 && celsius > MELTS_ABOVE { 0.0 } else { (snowing * 1.25).min(1.0) }.max(lasting);
    let wet_wanted = if rain > 0.0 { (0.5 + rain).min(1.0) } else { 0.0 };
    let fallen_wanted = rain.max(snowing);
    let ice_wanted = sea.map(|sea| {
        if sea <= SEA_FREEZES {
            1.0
        } else if sea >= SEA_THAWS {
            0.0
        } else {
            was.map_or(0.0, |was| was.ice)
        }
    });
    let Some(was) = was else {
        return Lying {
            wet: wet_wanted,
            snow: snow_wanted,
            fallen: fallen_wanted,
            ice: ice_wanted.unwrap_or(0.0),
            celsius,
            height,
        };
    };
    let towards = |now: f32, wanted: f32, up: f32, down: f32| match wanted > now {
        true => (now + delta / up.max(1e-3)).min(wanted),
        false => (now - delta / down.max(1e-3)).max(wanted),
    };
    // Wet ground dries sooner where it is warm and the air is dry.
    let dries = DRY * (0.6 + humidity) / (1.0 + celsius.max(0.0) / 12.0);
    // Snow keeps where it is below freezing; above it, it melts, the sooner the warmer, and sooner still in rain.
    let melts = if celsius > MELTS_ABOVE || rain > 0.0 {
        let warm = MELT / (1.0 + (celsius - MELTS_ABOVE).max(0.0) / 3.0);
        warm + (WASH - warm) * rain
    } else {
        f32::INFINITY
    };
    let snow = towards(was.snow, snow_wanted, SETTLE, melts);
    // Melting snow wets the ground under it.
    let wet_wanted = if snow < was.snow { wet_wanted.max(0.6) } else { wet_wanted };
    Lying {
        wet: towards(was.wet, wet_wanted, SOAK, dries),
        snow,
        fallen: towards(was.fallen, fallen_wanted, SETTLE, FORGET),
        ice: ice_wanted.map_or(was.ice, |wanted| towards(was.ice, wanted, FREEZE, THAW)),
        celsius,
        height,
    }
}

/// Fills a share of the grids round the viewer and gives the renderer
/// them, and the sea's sampler its floor and ice. While the game is not
/// `playing` nothing lying changes but what is first seen. Without a
/// viewer, there are none.
pub fn update(world: &mut World, delta: f32, playing: bool) {
    let Some(viewer) = crate::terrain::viewer(world) else {
        world.remove_resource::<WeatherMaps>();
        return;
    };
    let mut kept = world.remove_resource::<MapsKept>().unwrap_or_default();
    if playing {
        kept.clock += f64::from(delta.max(0.0));
    }
    kept.tops.forget_old();
    let origin = WorldOrigin::of(world);
    let level = origin.to_level(viewer);
    let around = DVec2::new(level.x, level.z);
    let to_world =
        |middle: DVec2| Vec2::new((middle.x - origin.0.x as f64) as f32, (middle.y - origin.0.z as f64) as f32);
    let worlds = block_worlds(world);
    let physics = world.resource::<crate::physics::PhysicsScene>();
    let has_weather = world.resource::<crate::weather::Weather>().is_some() || world.resource::<Storms>().is_some();

    // Cover, over every half metre near the viewer.
    let covered = !worlds.is_empty() || physics.is_some();
    if covered {
        kept.cover.follow(around);
        let tops = &mut kept.tops;
        kept.cover.refresh(COVER_EACH_FRAME, |middle, _| {
            let xz = to_world(middle);
            let at = Vec3::new(xz.x, viewer.y, xz.y);
            let blocks = ground_top(tops, &worlds, at, COVER_REACH, COVER_REACH);
            let over = physics
                .and_then(|physics| collider_under(world, physics, at + Vec3::Y * COVER_REACH, COVER_REACH * 2.0));
            blocks.into_iter().chain(over).fold(OPEN, f32::max)
        });
    }

    // The sea's floor, every few metres, where the viewer is near a sea's top.
    let sea = worlds.iter().find_map(|blocks| blocks.sea).filter(|sea| (viewer.y - sea).abs() < FLOOR_FROM);
    if let Some(top) = sea {
        kept.floor.follow(around);
        let tops = &mut kept.tops;
        kept.floor.refresh(FLOOR_EACH_FRAME, |middle, _| {
            let xz = to_world(middle);
            let at = Vec3::new(xz.x, top - 0.01, xz.y);
            match ground_top(tops, &worlds, at, 0.0, FLOOR_REACH) {
                Some(floor) => (top - floor).clamp(0.0, FLOOR_REACH),
                None => FLOOR_REACH,
            }
        });
    }

    // What lies, a cell at a time.
    if has_weather {
        kept.lying.follow(around);
        let clock = kept.clock;
        let tops = &mut kept.tops;
        kept.lying.refresh(LYING_EACH_FRAME, |middle, was| {
            let xz = to_world(middle);
            let ground = Vec3::new(xz.x, viewer.y, xz.y);
            let height = ground_top(tops, &worlds, ground, GROUND_REACH, GROUND_REACH).unwrap_or(viewer.y);
            let at = weather_at(world, Vec3::new(xz.x, height, xz.y));
            let sea = sea.map(|top| crate::heat::open_air(world, Vec3::new(xz.x, top, xz.y)).0);
            let delta = was.map_or(0.0, |was| (clock - was.at).clamp(0.0, f64::from(LONGEST_STEP)) as f32);
            Kept { lying: lie(was.map(|was| was.lying), &at, sea, height, delta), at: clock }
        });
    }

    bear_on_ice(world, &mut kept, sea, viewer);

    let maps = WeatherMaps {
        lying: has_weather.then(|| kept.lying.grid(origin.0.as_dvec3(), |kept| kept.lying)),
        cover: covered.then(|| kept.cover.grid(origin.0.as_dvec3(), |height| height)),
        floor: sea.is_some().then(|| kept.floor.grid(origin.0.as_dvec3(), |depth| depth)),
    };
    if let Some(SeaNow(sampler)) = world.remove_resource::<SeaNow>() {
        world.insert_resource(SeaNow(sampler.with_maps(maps.clone())));
    }
    world.insert_resource(maps);
    world.insert_resource(kept);
}

/// What the weather has left lying at `at`, in the world, as the maps say:
/// how wet, how much snow, and how much of the sea is frozen over. None
/// where the lying grid does not reach.
pub fn lying_at(world: &World, at: Vec3) -> Option<(f32, f32, f32)> {
    world.resource::<WeatherMaps>()?.lying_at(at)
}

/// Whether something keeps the rain off `at`, in the world.
pub fn covered(world: &World, at: Vec3) -> bool {
    world.resource::<WeatherMaps>().is_some_and(|maps| maps.covered(at))
}

/// What the weather has left on the ground at a place: how wet it is, how
/// much snow lies there and how much of the sea is frozen, each from 0 to
/// 1, and whether something over it keeps the weather off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundAt {
    pub wet: f32,
    pub snow: f32,
    pub ice: f32,
    pub sheltered: bool,
}

/// What the weather has left on the ground at `at`, in the world: by place
/// within the lying grid, else the level's own weather's; dry and clear of
/// snow under a roof.
pub fn ground_at(world: &World, at: Vec3) -> GroundAt {
    let sheltered = covered(world, at);
    let (wet, snow, ice) = lying_at(world, at).unwrap_or_else(|| {
        let ground = world.resource::<crate::weather::Weather>().map(|weather| weather.ground()).unwrap_or_default();
        (ground.wet, ground.snow, 0.0)
    });
    match sheltered {
        true => GroundAt { ice, sheltered, ..GroundAt::default() },
        false => GroundAt { wet, snow, ice, sheltered },
    }
}

/// Whether every cell of the grids the level has has been worked out once:
/// for tests, and for a picture taken as a level loads.
pub fn settled(world: &World) -> bool {
    world.resource::<MapsKept>().is_some_and(|kept| {
        let maps = world.resource::<WeatherMaps>();
        let lying = maps.is_none_or(|maps| maps.lying.is_none()) || kept.lying.all_known();
        let cover = maps.is_none_or(|maps| maps.cover.is_none()) || kept.cover.all_known();
        let floor = maps.is_none_or(|maps| maps.floor.is_none()) || kept.floor.all_known();
        lying && cover && floor
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use voxear_render::weather::WeatherAmounts;

    fn weather(rain: f32, snow: f32, celsius: f32) -> WeatherAt {
        WeatherAt {
            amounts: WeatherAmounts { rain, snow, ..Default::default() },
            celsius,
            humidity: 0.5,
            ..WeatherAt::default()
        }
    }

    #[test]
    fn rain_soaks_and_dries_snow_settles_below_freezing_and_melts_above_and_the_sea_freezes() {
        // Rain on dry ground soaks it in half a minute, and it dries after.
        let dry = lie(None, &weather(0.0, 0.0, 12.0), None, 0.0, 0.0);
        assert_eq!((dry.wet, dry.snow), (0.0, 0.0));
        let mut ground = dry;
        for _ in 0..30 {
            ground = lie(Some(ground), &weather(0.6, 0.0, 12.0), None, 0.0, 1.0);
        }
        assert!(ground.wet > 0.9, "{ground:?}");
        let soaked = ground;
        for _ in 0..40 {
            ground = lie(Some(ground), &weather(0.0, 0.0, 12.0), None, 0.0, 1.0);
        }
        assert!(ground.wet < soaked.wet && ground.wet > 0.5, "drying over a minute or two: {ground:?}");
        let mut hot = soaked;
        for _ in 0..40 {
            hot = lie(Some(hot), &weather(0.0, 0.0, 32.0), None, 0.0, 1.0);
        }
        assert!(hot.wet < ground.wet, "sooner where it is hot");

        // Snow settles, and keeps below freezing once it stops.
        let mut snowy = dry;
        for _ in 0..120 {
            snowy = lie(Some(snowy), &weather(0.0, 0.8, -3.0), None, 0.0, 1.0);
        }
        assert_eq!(snowy.snow, 1.0);
        for _ in 0..600 {
            snowy = lie(Some(snowy), &weather(0.0, 0.0, -3.0), None, 0.0, 1.0);
        }
        assert_eq!(snowy.snow, 1.0, "kept in the cold");
        // A thaw melts it, and its water wets the ground.
        let mut thaw = snowy;
        for _ in 0..60 {
            thaw = lie(Some(thaw), &weather(0.0, 0.0, 8.0), None, 0.0, 1.0);
        }
        assert!(thaw.snow < 0.6 && thaw.wet > 0.3, "{thaw:?}");
        // Where it is bitter, snow lies with nothing falling, from the first.
        assert!(lie(None, &weather(0.0, 0.0, -12.0), None, 0.0, 0.0).snow > 0.8);

        // A sea under freezing air is frozen from the first, and one that warms thaws over minutes.
        let frozen = lie(None, &weather(0.0, 0.0, -6.0), Some(-6.0), 0.0, 0.0);
        assert_eq!(frozen.ice, 1.0);
        assert_eq!(lie(None, &weather(0.0, 0.0, 10.0), Some(10.0), 0.0, 0.0).ice, 0.0);
        let half = lie(Some(frozen), &weather(0.0, 0.0, 4.0), Some(4.0), 0.0, THAW / 2.0);
        assert!((half.ice - 0.5).abs() < 1e-3);
        // Between freezing and thawing it stays as it was.
        assert_eq!(lie(Some(half), &weather(0.0, 0.0, -1.0), Some(-1.0), 0.0, 60.0).ice, half.ice);
    }

    use voxear_render::{Camera, weather::WeatherKind};
    use voxear_terrain::blocks::{BlockFaces, BlockMap};

    /// A yard of ground forty blocks across, a block high, with a roof four
    /// blocks over it across one corner, ten blocks a side, and a camera
    /// under the roof.
    pub(crate) fn roofed_yard() -> World {
        let mut world = World::new();
        let mut map = BlockMap::default();
        for x in -20..20 {
            for z in -20..20 {
                map.set([x, 0, z], 1);
            }
        }
        for x in 0..10 {
            for z in 0..10 {
                map.set([x, 4, z], 1);
            }
        }
        world.spawn((BlockWorld::new(map, &[BlockFaces::default(); 2], 1.0), Transform::default()));
        world.spawn((Camera::default(), Transform::from_position(Vec3::new(5.0, 2.0, 5.0))));
        world
    }

    pub(crate) fn settle(world: &mut World) {
        for _ in 0..100 {
            update(world, 0.0, true);
            if settled(world) {
                return;
            }
        }
        panic!("the weather's grids were never all worked out");
    }

    #[test]
    fn a_roof_keeps_the_rain_off_what_is_under_it_and_the_ground_beside_it_is_wet() {
        let mut world = roofed_yard();
        world.insert_resource(crate::weather::Weather::of(WeatherKind::Rain));
        settle(&mut world);
        let maps = world.resource::<WeatherMaps>().unwrap();
        assert_eq!(maps.cover_at(5.2, 5.2), 5.0, "the roof's top");
        assert_eq!(maps.cover_at(15.2, 5.2), 1.0, "the yard's");
        assert_eq!(maps.cover_at(40.0, 5.2), OPEN, "past the blocks");
        assert!(covered(&world, Vec3::new(5.2, 1.0, 5.2)) && !covered(&world, Vec3::new(15.2, 1.0, 5.2)));
        assert!(!covered(&world, Vec3::new(5.2, 5.0, 5.2)), "the roof's top is out in it");
        let (under, out) = (ground_at(&world, Vec3::new(5.2, 1.0, 5.2)), ground_at(&world, Vec3::new(15.2, 1.0, 5.2)));
        assert_eq!((under.wet, under.sheltered), (0.0, true));
        assert!(out.wet > 0.9 && !out.sheltered, "{out:?}");
        // A sea floor only where there is a sea.
        assert!(maps.floor.is_none());
    }

    #[test]
    fn a_storm_wets_the_ground_under_it_and_none_beside_and_snow_lies_where_it_is_bitter() {
        let mut world = roofed_yard();
        let mut storms = Storms::default();
        storms.start(crate::storms::Storm {
            middle: [-300.0, 0.0],
            radius: 150.0,
            grows: 0.0,
            kind: crate::storms::StormKind::Rain,
            ..Default::default()
        });
        world.insert_resource(storms);
        settle(&mut world);
        let wet = |world: &World, x: f32| lying_at(world, Vec3::new(x, 1.0, 0.0)).unwrap().0;
        assert!(wet(&world, -300.0) > 0.9, "under the storm: {}", wet(&world, -300.0));
        assert_eq!(wet(&world, 300.0), 0.0, "far from it");
        assert!(lying_at(&world, Vec3::new(5000.0, 1.0, 0.0)).is_none(), "past the grid");

        // It moves on: the ground it left dries over minutes, not at once.
        world.resource_mut::<Storms>().unwrap().made[0].middle = [3000.0, 0.0];
        for _ in 0..30 {
            update(&mut world, 1.0, true);
        }
        let drying = wet(&world, -300.0);
        assert!(drying > 0.5 && drying < 0.95, "{drying}");

        // A bitter climate: snow lies from the first, out in the open; none under the roof.
        let mut world = roofed_yard();
        world.spawn(crate::heat::Climate { celsius: -14.0, day_swing: 0.0, ..Default::default() });
        world.insert_resource(crate::weather::Weather::default());
        settle(&mut world);
        assert!(ground_at(&world, Vec3::new(15.2, 1.0, 5.2)).snow > 0.8);
        assert_eq!(ground_at(&world, Vec3::new(5.2, 1.0, 5.2)).snow, 0.0);
    }

    #[test]
    fn a_rolling_grid_keeps_its_cells_as_it_follows_and_works_out_the_new_ones_first() {
        let mut grid: Rolling<f32> = Rolling::new(4, 2.0);
        grid.follow(DVec2::new(0.0, 0.0));
        let mut asked = 0;
        grid.refresh(100, |middle, was| {
            asked += 1;
            assert!(was.is_none());
            middle.x as f32
        });
        assert_eq!(asked, 16, "all sixteen, none twice");
        assert!(grid.all_known());
        assert_eq!(grid.middle(0), DVec2::new(-3.0, -3.0));
        // One cell along: three columns kept, one to work out, and that one first.
        grid.follow(DVec2::new(2.5, 0.0));
        assert!(!grid.all_known());
        let mut new = Vec::new();
        grid.refresh(4, |middle, was| {
            if was.is_none() {
                new.push(middle);
            }
            middle.x as f32
        });
        assert_eq!(new.len(), 4);
        assert!(new.iter().all(|middle| middle.x == 5.0));
        let out = grid.grid(DVec3::new(1.0, 0.0, 0.0), |v| v);
        assert_eq!(out.corner, Vec2::new(-3.0, -4.0));
        assert_eq!(out.nearest(-2.9, -3.9), Some(-1.0));
    }
}
