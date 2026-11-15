//! A dungeon's layout, planned once for each site: the graph of each floor
//! first (a threshold, two ways round to a pillared hall, set pieces, the
//! end), then each space's place under the land, the floors' heights that
//! keep every space under enough rock, and the passages and stairs between
//! them, routed round the spaces and kept under the land too.

use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use super::land::{Entrance, Land};
use super::{Dungeon, DungeonSize, EntranceKind, RoomRole, noise};
use crate::blocks::structure::Ground;

/// What a space of a dungeon is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpaceKind {
    /// The foot of the grand stair down from the entrance: the first look
    /// into the dungeon.
    Landing,
    /// A room where fights happen: a flat floor, a raised edge, two or
    /// three ways out.
    Room,
    /// A pillared hall: columns and pointed arches down both sides, statues
    /// in alcoves, a gallery over the aisles.
    Hall,
    /// A gorge: a deep cut in the rock, layered ledges, entered at its top
    /// through a carved archway, a stair down its wall to a floor of
    /// flagstones and rubble.
    Gorge,
    /// A chasm: a pit glowing red, walkways round its edge, a bridge over
    /// it to an obelisk.
    Chasm,
    /// A hall flooded with green water round carved monoliths on pillars, a
    /// relief on its far wall, stairs down into the water.
    Flooded,
    /// The boss's arena: round, a ring of columns, a throne on a dais.
    Arena,
    /// A round shaft down to the next floor, a stair winding round its wall
    /// past the well in its middle.
    Descent,
    /// A small room off the way where the loot is, round a glowing
    /// reliquary.
    Treasury,
    /// A small room to rest in, warmly lit.
    Shrine,
}

/// What a way between spaces is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WayKind {
    /// The grand stair from the entrance down to the first floor.
    Grand,
    /// A passage between spaces, a stair along it where they differ in
    /// height.
    Passage,
    /// A narrow way back from the end of a floor to its start, behind an
    /// iron gate.
    Shortcut,
}

/// What a light is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LightKind {
    /// Fire in an iron cage on a stand: warm, along the main path.
    Brazier,
    /// A cluster of blue crystal: cold, along side ways.
    Crystal,
    /// A set piece's own light: lava in a pit, a reliquary's glow.
    Glow,
}

/// A space of a dungeon as planned.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    pub kind: SpaceKind,
    /// Which floor it is on, from 0 for the first.
    pub floor: u32,
    /// The block in the middle of its floor: x, the floor block's height,
    /// z. Its open air starts a block over it.
    pub at: [i32; 3],
    /// How far it reaches from its middle along x and z, its walls in that.
    pub half: [i32; 2],
    /// How high it is open over its floor.
    pub height: i32,
    /// The floor block's height of its upper ways in, where it has any
    /// higher than its floor: a gorge's top, a shaft's top; else its floor.
    pub upper: i32,
    /// How far under its floor it goes: a chasm's pit, a flooded hall's
    /// water.
    pub below: i32,
    /// Whether its length runs along x.
    pub along_x: bool,
    /// Whether it is on the main path.
    pub main: bool,
    /// How far along its floor it stands, from 0 at its start to 1 at its end.
    pub progress: f32,
    pub seed: u32,
    /// Your own piece standing in it, by its place in the dungeon's rooms.
    pub piece: Option<usize>,
}

impl Space {
    /// The highest block of open air in it.
    pub fn top(&self) -> i32 {
        (self.at[1] + self.height).max(self.upper + 10)
    }

    /// The lowest block it reaches down to.
    pub fn bottom(&self) -> i32 {
        self.at[1] - self.below
    }

    pub fn low(&self) -> [i32; 2] {
        [self.at[0] - self.half[0], self.at[2] - self.half[1]]
    }

    pub fn high(&self) -> [i32; 2] {
        [self.at[0] + self.half[0], self.at[2] + self.half[1]]
    }

    /// Whether it is round: an arena or a shaft.
    pub fn round(&self) -> bool {
        matches!(self.kind, SpaceKind::Arena | SpaceKind::Descent)
    }
}

/// One block column of a way: where, the height of its walking surface in
/// half blocks (a whole block's top at an even number, a slab's at an odd),
/// how far from its middle line, and how far along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WayCell {
    pub at: [i32; 2],
    pub surface: i32,
    pub off: f32,
    pub along: f32,
}

/// A way between two spaces as planned, or from the entrance.
#[derive(Clone, Debug, PartialEq)]
pub struct Way {
    pub kind: WayKind,
    /// The spaces it joins: none for the entrance's end; `usize::MAX` for
    /// a way broken through to a cave of the world's.
    pub from: Option<usize>,
    pub to: usize,
    pub width: i32,
    pub height: i32,
    pub main: bool,
    /// Its middle line, blocks along x and z, with its walking surface's
    /// height in half blocks at each point.
    pub line: Vec<[f32; 3]>,
    /// Its columns, ordered by x and then z.
    pub cells: Vec<WayCell>,
    pub low: [i32; 2],
    pub high: [i32; 2],
    pub length: f32,
}

impl Way {
    /// Its column at `at`, if it has one there.
    pub fn cell(&self, at: [i32; 2]) -> Option<&WayCell> {
        if at[0] < self.low[0] || at[0] > self.high[0] || at[1] < self.low[1] || at[1] > self.high[1] {
            return None;
        }
        self.cells.binary_search_by(|cell| cell.at.cmp(&at)).ok().map(|i| &self.cells[i])
    }

    /// The point of its middle line `along` blocks along it, and the way it
    /// runs there.
    pub fn point(&self, along: f32) -> ([f32; 3], [f32; 2]) {
        let mut left = along.max(0.0);
        for pair in self.line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if length <= 1e-4 {
                continue;
            }
            let dir = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
            if left <= length {
                let t = left / length;
                return ([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t], dir);
            }
            left -= length;
        }
        let last = self.line.last().copied().unwrap_or([0.0; 3]);
        (last, [1.0, 0.0])
    }
}

/// A light of a dungeon: the block it stands in, and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSpot {
    pub at: [i32; 3],
    pub kind: LightKind,
}

/// A dungeon as laid out at one site: its entrance, its spaces and ways,
/// its lights, and the blocks it reaches from and to.
#[derive(Clone, Debug, PartialEq)]
pub struct DungeonLayout {
    pub entrance: Entrance,
    pub spaces: Vec<Space>,
    pub ways: Vec<Way>,
    pub lights: Vec<LightSpot>,
    pub floors: u32,
    /// How many ways its graph asked for, the grand stair with them: as
    /// many as it has, where every one found its way.
    pub joins: u32,
    pub low: [i32; 3],
    pub high: [i32; 3],
    /// How far down the shaft of a sinkhole entrance goes, to the first
    /// floor's landing; 0 for a gate.
    pub shaft_floor: i32,
    /// Cracks of daylight from a great hall's roof up through the rock to
    /// level land over it: the column of each, and the heights from its
    /// foot in the roof to the ground's top.
    pub shafts: Vec<([i32; 2], [i32; 2])>,
}

/// A dungeon's sizes in blocks, from its size and what its file sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sizes {
    pub passage: [i32; 2],
    pub stair: i32,
    pub room: [i32; 2],
    pub hall: [i32; 2],
    pub gorge: [i32; 2],
    pub chasm: [i32; 2],
    pub arena: [i32; 2],
    pub floor_gap: i32,
    /// How far apart the graph's places are, along the floor and across it.
    pub unit: [f32; 2],
    /// How many places along the floor its graph runs.
    pub steps: f32,
}

impl Sizes {
    pub(crate) fn of(dungeon: &Dungeon) -> Self {
        let mut sizes = match dungeon.size {
            DungeonSize::Small => Sizes {
                passage: [5, 6],
                stair: 7,
                room: [18, 10],
                hall: [44, 22],
                gorge: [32, 20],
                chasm: [36, 18],
                arena: [32, 20],
                floor_gap: 22,
                unit: [38.0, 30.0],
                steps: 4.0,
            },
            DungeonSize::Medium => Sizes {
                passage: [5, 6],
                stair: 8,
                room: [22, 12],
                hall: [56, 28],
                gorge: [40, 26],
                chasm: [44, 24],
                arena: [40, 24],
                floor_gap: 26,
                unit: [46.0, 38.0],
                steps: 5.0,
            },
            DungeonSize::Large => Sizes {
                passage: [6, 7],
                stair: 10,
                room: [24, 14],
                hall: [64, 32],
                gorge: [48, 32],
                chasm: [52, 30],
                arena: [46, 30],
                floor_gap: 30,
                unit: [52.0, 42.0],
                steps: 6.0,
            },
        };
        let set = dungeon.sizes;
        let pair = |kept: &mut [i32; 2], given: [u32; 2], least: [i32; 2]| {
            for axis in 0..2 {
                if given[axis] > 0 {
                    kept[axis] = (given[axis] as i32).max(least[axis]);
                }
            }
        };
        pair(&mut sizes.passage, set.passage, [3, 3]);
        pair(&mut sizes.room, set.room, [10, 5]);
        pair(&mut sizes.hall, set.hall, [20, 10]);
        pair(&mut sizes.gorge, set.gorge, [16, 8]);
        pair(&mut sizes.chasm, set.chasm, [20, 8]);
        pair(&mut sizes.arena, set.arena, [16, 8]);
        if set.stair > 0 {
            sizes.stair = (set.stair as i32).max(3);
        }
        if set.floor_gap > 0 {
            sizes.floor_gap = (set.floor_gap as i32).max(12);
        }
        sizes
    }

    /// How far a floor reaches from its start, in blocks.
    pub(crate) fn floor_reach(&self) -> f32 {
        self.unit[0] * self.steps + self.hall[0] as f32 * 0.5 + self.unit[1] * 2.0
    }

    /// How far a gorge drops: what a floor's lower level is under its upper.
    fn drop(&self) -> i32 {
        self.gorge[1]
    }
}

/// A place in a floor's graph before it is laid out.
#[derive(Clone, Copy, Debug)]
struct Node {
    kind: SpaceKind,
    /// Places along the floor and across it.
    place: [f32; 2],
    /// 0 on the floor's upper level, 1 on its lower.
    level: u8,
    main: bool,
    /// Whether it may be left out where the land has no room for it.
    optional: bool,
    progress: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Join {
    Main,
    Side,
    Shortcut,
}

/// A floor's graph: its places, the first its start, and the joins between
/// them.
fn graph(size: DungeonSize, last: bool, floor: u32, seed: u32) -> (Vec<Node>, Vec<(usize, usize, Join)>) {
    let pick = |salt: u32, kinds: &[SpaceKind]| {
        kinds[(noise::hash(floor as i32, salt as i32, 7, seed) % kinds.len() as u32) as usize]
    };
    let end = if last { SpaceKind::Arena } else { SpaceKind::Descent };
    let start = if floor == 0 { SpaceKind::Landing } else { SpaceKind::Descent };
    let node =
        |kind, place: [f32; 2], level, main, optional, progress| Node { kind, place, level, main, optional, progress };
    use Join::{Main, Shortcut, Side};
    use SpaceKind::*;
    match size {
        DungeonSize::Small => {
            let nodes = vec![
                node(start, [0.0, 0.0], 0, true, false, 0.0),
                node(Room, [1.0, 0.0], 0, true, false, 0.25),
                node(Gorge, [2.0, 0.6], 0, true, false, 0.5),
                node(Hall, [3.0, 0.0], 1, true, false, 0.75),
                node(end, [4.0, 0.0], 1, true, false, 1.0),
                node(Treasury, [1.0, 1.3], 0, false, true, 0.3),
            ];
            let joins = vec![
                (0, 1, Main),
                (1, 2, Main),
                (2, 3, Main),
                (1, 3, Main),
                (3, 4, Main),
                (1, 5, Side),
                (4, 0, Shortcut),
            ];
            (nodes, joins)
        }
        DungeonSize::Medium | DungeonSize::Large => {
            let flooded = pick(1, &[Flooded, Flooded, Hall]);
            let chasm = pick(2, &[Chasm, Chasm, Room]);
            let large = size == DungeonSize::Large;
            let mut nodes = vec![
                node(start, [0.0, 0.0], 0, true, false, 0.0),
                node(Room, [1.0, 0.0], 0, true, false, 0.2),
                node(Gorge, [2.0, 0.9], 0, true, false, 0.4),
                node(Room, [2.0, -0.9], 0, true, true, 0.4),
                node(Hall, [3.0, 0.0], 1, true, false, 0.6),
                node(flooded, [4.0, 0.9], 1, true, true, 0.8),
                node(chasm, [4.0, -0.9], 1, true, true, 0.8),
                node(end, [if large { 6.0 } else { 5.0 }, 0.0], 1, true, false, 1.0),
                node(Treasury, [2.0, -1.9], 0, false, true, 0.45),
                node(Treasury, [4.0, 1.9], 1, false, true, 0.85),
            ];
            let mut joins = vec![
                (0, 1, Main),
                (1, 2, Main),
                (1, 3, Main),
                (2, 4, Main),
                (3, 4, Main),
                (4, 5, Main),
                (4, 6, Main),
                (3, 8, Side),
                (5, 9, Side),
                (7, 0, Shortcut),
            ];
            if large {
                nodes.push(node(Hall, [5.0, 0.0], 1, true, false, 0.9));
                nodes.push(node(Shrine, [5.0, 1.3], 1, false, true, 0.92));
                nodes.push(node(Treasury, [4.0, -1.9], 1, false, true, 0.85));
                joins.extend([(5, 10, Main), (6, 10, Main), (10, 7, Main), (10, 11, Side), (6, 12, Side)]);
            } else {
                joins.extend([(5, 7, Main), (6, 7, Main)]);
            }
            (nodes, joins)
        }
    }
}

/// A space's size by its kind: half along x and z with its length along
/// the floor's way, its height, and how far it goes under its floor.
fn shape(kind: SpaceKind, sizes: &Sizes) -> ([i32; 2], i32, i32) {
    let half = |v: i32| v / 2;
    match kind {
        SpaceKind::Landing => ([half(sizes.room[0]) - 1, half(sizes.room[0]) - 1], sizes.room[1], 0),
        SpaceKind::Room => ([half(sizes.room[0]), half(sizes.room[0])], sizes.room[1], 0),
        SpaceKind::Hall => ([half(sizes.hall[0]), half(sizes.hall[0]) / 2 + 1], sizes.hall[1], 0),
        SpaceKind::Gorge => ([half(sizes.gorge[0]), half(sizes.gorge[0]) / 3 + 2], 10, 0),
        SpaceKind::Chasm => ([half(sizes.chasm[0]), half(sizes.chasm[0])], sizes.chasm[1], sizes.chasm[1]),
        SpaceKind::Flooded => ([half(sizes.hall[0]) - 4, half(sizes.hall[0]) / 2 + 1], sizes.hall[1] * 3 / 4, 4),
        SpaceKind::Arena => ([half(sizes.arena[0]), half(sizes.arena[0])], sizes.arena[1], 0),
        SpaceKind::Descent => ([10, 10], 10, 0),
        SpaceKind::Treasury | SpaceKind::Shrine => ([6, 6], 8, 0),
    }
}

/// The role your own pieces for a space of `kind` have.
fn role_of(kind: SpaceKind) -> Option<RoomRole> {
    match kind {
        SpaceKind::Room => Some(RoomRole::Any),
        SpaceKind::Hall => Some(RoomRole::Hall),
        SpaceKind::Treasury => Some(RoomRole::Loot),
        SpaceKind::Arena => Some(RoomRole::Boss),
        _ => None,
    }
}

/// A floor laid out at one turn: its spaces' places relative to the first
/// floor's upper level, and its joins.
#[derive(Clone)]
struct Floor {
    spaces: Vec<Space>,
    /// For each of its places, its space: in this floor's own, or the floor
    /// over's shaft down (`usize::MAX`).
    joins: Vec<(usize, usize, Join)>,
    nodes: Vec<Node>,
}

/// Plans a dungeon at block `site`, its entrance round it: none where the
/// land offers no entrance, or no room under it for even one floor over the
/// world's lowest block, `bottom`.
pub(crate) fn plan(
    dungeon: &Dungeon,
    ground: &Ground,
    site: [i32; 2],
    seed: u32,
    bottom: i32,
) -> Option<DungeonLayout> {
    let sizes = Sizes::of(dungeon);
    let land = Land::new(ground);
    let entrance = land.entrance(site, &dungeon.entrances, super::ENTRANCE_SEARCH, seed)?;
    let cover = dungeon.cover.max(3) as i32;
    let [fewest, most] =
        [dungeon.floors[0].min(dungeon.floors[1]).max(1), dungeon.floors[0].max(dungeon.floors[1]).max(1)];
    let wanted = fewest + noise::hash(site[0], site[1], 3, seed) % (most - fewest + 1);
    for floors in (1..=wanted.min(4)).rev() {
        if let Some(layout) = plan_floors(dungeon, &sizes, &land, entrance, floors, cover, seed, bottom) {
            return Some(layout);
        }
    }
    None
}

/// The heights each floor's levels are at, relative to the first floor's
/// upper level: its upper, then its lower.
fn levels(sizes: &Sizes, floor: u32) -> [i32; 2] {
    let upper = -(floor as i32) * (sizes.drop() + sizes.floor_gap);
    [upper, upper - sizes.drop()]
}

#[allow(clippy::too_many_arguments)]
fn plan_floors(
    dungeon: &Dungeon,
    sizes: &Sizes,
    land: &Land,
    entrance: Entrance,
    floors: u32,
    cover: i32,
    seed: u32,
    bottom: i32,
) -> Option<DungeonLayout> {
    let into = [entrance.into[0] as f32, entrance.into[1] as f32];
    let sinkhole = entrance.kind == EntranceKind::Sinkhole;
    // How far the grand stair falls, first a guess, then as far as the floors under the land need it to.
    let mut fall = cover + sizes.room[1] + 6;
    let mut tries = 0;
    loop {
        tries += 1;
        let start = if sinkhole {
            // At the foot of the sinkhole's shaft, beside it.
            let along = 11.0 + (sizes.room[0] / 2) as f32;
            [entrance.at[0] as f32 + into[0] * along, entrance.at[2] as f32 + into[1] * along]
        } else {
            let along = 3.0 + grand_length(fall) + (sizes.room[0] / 2 - 1) as f32;
            [entrance.at[0] as f32 + into[0] * along, entrance.at[2] as f32 + into[1] * along]
        };
        let laid = lay_floors(dungeon, sizes, land, start, into, floors, cover, seed)?;
        // The first floor's upper level as high as every space lets it be, and no higher than the stair's fall.
        let mut level = entrance.at[1] - fall;
        for floor in &laid {
            for space in &floor.spaces {
                let ceiling = land.lowest_over(space.low(), space.high()) - cover - space.top();
                level = level.min(ceiling);
            }
        }
        let need = entrance.at[1] - level;
        if need > fall + 2 && tries < 5 {
            fall = need;
            continue;
        }
        // Nothing under the world's lowest blocks.
        let lowest = laid.iter().flat_map(|floor| floor.spaces.iter()).map(Space::bottom).min().unwrap_or(0);
        if level + lowest - 4 < bottom {
            return None;
        }
        return Some(finish(dungeon, sizes, land, entrance, laid, level, cover, seed));
    }
}

/// How long a grand stair falling `fall` blocks runs: a block down for every
/// two along, a landing of four every ten blocks down.
fn grand_length(fall: i32) -> f32 {
    let fall = fall.max(0) as f32;
    fall * 2.0 + (fall / 10.0).floor() * 4.0 + 6.0
}

/// Each floor's spaces laid out from block `start`, the first floor's
/// running `into`, each later one from the shaft down at the end of the
/// floor over it, each turned the way that puts it under the highest land,
/// heights relative to the first floor's upper level.
#[allow(clippy::too_many_arguments)]
fn lay_floors(
    dungeon: &Dungeon,
    sizes: &Sizes,
    land: &Land,
    start: [f32; 2],
    into: [f32; 2],
    floors: u32,
    cover: i32,
    seed: u32,
) -> Option<Vec<Floor>> {
    let mut laid: Vec<Floor> = Vec::new();
    let mut origin = start;
    for floor in 0..floors {
        let last = floor + 1 == floors;
        let (nodes, joins) = graph(dungeon.size, last, floor, seed);
        // The first floor runs on from the grand stair, turned a little at most; a later one any way round.
        let base = into[1].atan2(into[0]);
        let quarter = std::f32::consts::FRAC_PI_4;
        let angles: Vec<f32> = if floor == 0 {
            [0.0, 0.5, -0.5, 1.0, -1.0].iter().map(|turn| base + turn * quarter).collect()
        } else {
            (0..8).map(|turn| turn as f32 * quarter).collect()
        };
        let mut best: Option<(i32, Floor)> = None;
        for &angle in &angles {
            for flip in [false, true] {
                let dir = [angle.cos(), angle.sin()];
                let side = if flip { [dir[1], -dir[0]] } else { [-dir[1], dir[0]] };
                let mut candidate = place_floor(dungeon, sizes, &nodes, &joins, floor, origin, dir, side, seed);
                let earlier: Vec<&Space> = laid.iter().flat_map(|floor| floor.spaces.iter()).collect();
                separate(&mut candidate.spaces, &earlier);
                // How high its first floor's level could be under the land: the best turn lets it be highest.
                let mut score = i32::MAX;
                for (at, space) in candidate.spaces.iter().enumerate() {
                    if floor > 0 && at == 0 {
                        continue;
                    }
                    let ceiling = land.lowest_over(space.low(), space.high()) - cover - space.top();
                    if !candidate.nodes[at].optional {
                        score = score.min(ceiling);
                    }
                }
                if best.as_ref().is_none_or(|(least, _)| score > *least) {
                    best = Some((score, candidate));
                }
            }
        }
        let (score, mut chosen) = best?;
        // A space that may be left out, and needs the whole floor far deeper than the rest of it does, is.
        drop_deep(&mut chosen, land, cover, score);
        origin = chosen
            .spaces
            .iter()
            .zip(&chosen.nodes)
            .find(|(_, node)| node.kind == SpaceKind::Descent && node.progress >= 1.0)
            .map_or(origin, |(space, _)| [space.at[0] as f32, space.at[2] as f32]);
        laid.push(chosen);
    }
    Some(laid)
}

/// A floor's nodes put in place, from `origin`, along `dir` and across
/// `side`.
#[allow(clippy::too_many_arguments)]
fn place_floor(
    dungeon: &Dungeon,
    sizes: &Sizes,
    nodes: &[Node],
    joins: &[(usize, usize, Join)],
    floor: u32,
    origin: [f32; 2],
    dir: [f32; 2],
    side: [f32; 2],
    seed: u32,
) -> Floor {
    let levels = levels(sizes, floor);
    let along_x = dir[0].abs() >= dir[1].abs();
    let mut spaces = Vec::new();
    for (at, node) in nodes.iter().enumerate() {
        let jitter = |salt: i32| (noise::unit(at as i32, floor as i32, salt, seed) - 0.5) * 12.0;
        let (a, s) = (node.place[0] * sizes.unit[0], node.place[1] * sizes.unit[1]);
        let x = origin[0] + dir[0] * a + side[0] * s + if at == 0 { 0.0 } else { jitter(1) };
        let z = origin[1] + dir[1] * a + side[1] * s + if at == 0 { 0.0 } else { jitter(2) };
        let (half, height, below) = shape(node.kind, sizes);
        let level = levels[node.level as usize];
        // A gorge's top is the upper level, its floor the lower; a shaft's top the floor over's lower level.
        let (floor_level, upper) = match node.kind {
            SpaceKind::Gorge => (levels[1], levels[0]),
            SpaceKind::Descent if at == 0 && floor > 0 => {
                let over = self::levels(sizes, floor - 1);
                (levels[0], over[1])
            }
            SpaceKind::Descent => {
                let under = self::levels(sizes, floor + 1);
                (under[0], level)
            }
            _ => (level, level),
        };
        // The long way of a gorge runs from the places before it to those after; of a hall along the floor.
        let along = match node.kind {
            SpaceKind::Gorge => {
                let toward = joins
                    .iter()
                    .filter(|(a, b, _)| *a == at || *b == at)
                    .map(|(a, b, _)| if *a == at { *b } else { *a });
                let mut extent = [0.0f32; 2];
                for other in toward {
                    let place = nodes[other].place;
                    let d = [place[0] - node.place[0], place[1] - node.place[1]];
                    let world = [
                        dir[0] * d[0] * sizes.unit[0] + side[0] * d[1] * sizes.unit[1],
                        dir[1] * d[0] * sizes.unit[0] + side[1] * d[1] * sizes.unit[1],
                    ];
                    extent[0] += world[0].abs();
                    extent[1] += world[1].abs();
                }
                extent[0] >= extent[1]
            }
            _ => along_x,
        };
        let half = if along { half } else { [half[1], half[0]] };
        let space_seed = noise::hash(at as i32, floor as i32, 11, seed);
        let mut space = Space {
            kind: node.kind,
            floor,
            at: [x.round() as i32, floor_level, z.round() as i32],
            half,
            height,
            upper,
            below,
            along_x: along,
            main: node.main,
            progress: node.progress,
            seed: space_seed,
            piece: None,
        };
        // Your own piece for its role, where you have one: the space as large as the piece and a little.
        if let Some(role) = role_of(node.kind) {
            space.piece = pick_piece(dungeon, role, node.progress, space_seed);
            if let Some(piece) = space.piece.map(|at| &dungeon.rooms[at].1.piece) {
                let [px, py, pz] = piece.size.map(|v| v as i32);
                space.half = [space.half[0].max(px / 2 + 2), space.half[1].max(pz / 2 + 2)];
                space.height = space.height.max(py + 2);
            }
        }
        spaces.push(space);
    }
    Floor { spaces, joins: joins.to_vec(), nodes: nodes.to_vec() }
}

/// One of your own pieces of `role` for a space `progress` along its floor,
/// by weight among those whose zone it is in.
fn pick_piece(dungeon: &Dungeon, role: RoomRole, progress: f32, seed: u32) -> Option<usize> {
    let fits = |at: &usize| {
        let (kind_role, kind) = &dungeon.rooms[*at];
        let zone = kind.zone;
        *kind_role == role && progress >= zone[0].min(zone[1]) - 1e-4 && progress <= zone[0].max(zone[1]) + 1e-4
    };
    let total: f32 = (0..dungeon.rooms.len()).filter(fits).map(|at| dungeon.rooms[at].1.weight.max(0.0)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut left = (seed >> 8) as f32 / 16_777_216.0 * total;
    (0..dungeon.rooms.len()).filter(fits).find(|&at| {
        left -= dungeon.rooms[at].1.weight.max(0.0);
        left < 0.0
    })
}

/// Pushes `spaces` apart until none meets another, or one of `earlier`,
/// with a wall of rock between: in plan, where their heights meet too. The
/// first of them, the floor's start, stays where it is.
fn separate(spaces: &mut [Space], earlier: &[&Space]) {
    // Room for a passage between any two, with rock either side of it.
    const GAP: i32 = 16;
    let meets = |a: &Space, b: &Space| -> Option<[i32; 2]> {
        let over_y = a.bottom() - GAP < b.top() && b.bottom() - GAP < a.top();
        if !over_y {
            return None;
        }
        let ox = (a.half[0] + b.half[0] + GAP) - (a.at[0] - b.at[0]).abs();
        let oz = (a.half[1] + b.half[1] + GAP) - (a.at[2] - b.at[2]).abs();
        (ox > 0 && oz > 0).then_some([ox, oz])
    };
    for _ in 0..48 {
        let mut moved = false;
        for i in 0..spaces.len() {
            for other in earlier {
                if let Some([ox, oz]) = meets(&spaces[i], other) {
                    if i == 0 {
                        continue;
                    }
                    push(&mut spaces[i], other.at, ox, oz, 1.0);
                    moved = true;
                }
            }
            for j in i + 1..spaces.len() {
                let Some([ox, oz]) = meets(&spaces[i], &spaces[j]) else { continue };
                let (a, b) = (spaces[i].at, spaces[j].at);
                if i == 0 {
                    push(&mut spaces[j], a, ox, oz, 1.0);
                } else {
                    push(&mut spaces[j], a, ox, oz, 0.5);
                    push(&mut spaces[i], b, ox, oz, 0.5);
                }
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

/// Moves `space` away from `from` by `share` of the lesser of the overlaps.
fn push(space: &mut Space, from: [i32; 3], ox: i32, oz: i32, share: f32) {
    if ox <= oz {
        let way = if space.at[0] >= from[0] { 1 } else { -1 };
        space.at[0] += way * ((ox as f32 * share).ceil() as i32).max(1);
    } else {
        let way = if space.at[2] >= from[2] { 1 } else { -1 };
        space.at[2] += way * ((oz as f32 * share).ceil() as i32).max(1);
    }
}

/// Leaves out each space of `floor` that may be left out and would need the
/// floor 24 blocks or more deeper than its others do (`score`), with its
/// joins; keeping at least one way on past the hall.
fn drop_deep(floor: &mut Floor, land: &Land, cover: i32, score: i32) {
    let mut gone = vec![false; floor.nodes.len()];
    for (at, space) in floor.spaces.iter().enumerate() {
        if !floor.nodes[at].optional {
            continue;
        }
        let ceiling = land.lowest_over(space.low(), space.high()) - cover - space.top();
        gone[at] = ceiling < score - 24;
    }
    // A place left with no join to the rest goes too: loot off a room that went.
    loop {
        let mut changed = false;
        for at in 1..floor.nodes.len() {
            if gone[at] {
                continue;
            }
            let joined = floor
                .joins
                .iter()
                .any(|(a, b, join)| *join != Join::Shortcut && ((*a == at && !gone[*b]) || (*b == at && !gone[*a])));
            if !joined {
                gone[at] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Both ways on from the hall gone: the nearer to fitting stays.
    let ways_on: Vec<usize> = (0..floor.nodes.len())
        .filter(|&at| floor.nodes[at].main && floor.nodes[at].optional && floor.nodes[at].progress > 0.7)
        .collect();
    if !ways_on.is_empty() && ways_on.iter().all(|&at| gone[at]) {
        gone[ways_on[0]] = false;
    }
    floor.joins.retain(|(a, b, _)| !gone[*a] && !gone[*b]);
    // Kept in place (joins name places by index): a space left out takes no room and is built as nothing.
    for (at, gone) in gone.iter().enumerate() {
        if *gone {
            floor.spaces[at].half = [0, 0];
            floor.spaces[at].height = -1;
        }
    }
}

/// The layout with every height made whole from the first floor's upper
/// level, `level`: the spaces, the grand stair, the passages routed round
/// the spaces and under the land, the shortcuts, and the lights.
#[allow(clippy::too_many_arguments)]
fn finish(
    dungeon: &Dungeon,
    sizes: &Sizes,
    land: &Land,
    entrance: Entrance,
    laid: Vec<Floor>,
    level: i32,
    cover: i32,
    seed: u32,
) -> DungeonLayout {
    let floors = laid.len() as u32;
    let mut spaces: Vec<Space> = Vec::new();
    let mut joins: Vec<(usize, usize, Join)> = Vec::new();
    let mut previous_end: Option<usize> = None;
    for floor in laid {
        let mut index = Vec::with_capacity(floor.spaces.len());
        for (at, mut space) in floor.spaces.into_iter().enumerate() {
            // A later floor's start is the floor over's shaft down.
            if at == 0
                && let Some(end) = previous_end
            {
                index.push(end);
                continue;
            }
            space.at[1] += level;
            space.upper += level;
            index.push(spaces.len());
            spaces.push(space);
        }
        for (a, b, join) in floor.joins {
            joins.push((index[a], index[b], join));
        }
        previous_end = floor
            .nodes
            .iter()
            .position(|node| node.progress >= 1.0 && node.kind == SpaceKind::Descent)
            .map(|at| index[at]);
    }
    let live = |space: &Space| space.height >= 0;
    let mut ways = Vec::new();
    // The grand stair, from the entrance's threshold into the land to the landing.
    let sinkhole = entrance.kind == EntranceKind::Sinkhole;
    let landing = &spaces[0];
    let into = entrance.into;
    let shaft_floor = if sinkhole { landing.at[1] } else { 0 };
    let from = if sinkhole {
        [
            entrance.at[0] as f32 + into[0] as f32 * 9.0,
            entrance.at[2] as f32 + into[1] as f32 * 9.0,
            ((landing.at[1] + 1) * 2) as f32,
        ]
    } else {
        [
            entrance.at[0] as f32 + into[0] as f32 * 1.0,
            entrance.at[2] as f32 + into[1] as f32 * 1.0,
            ((entrance.at[1] + 1) * 2) as f32,
        ]
    };
    let door = door_of(landing, [from[0], from[1]]);
    let to = [door.0[0] - door.1[0] as f32 * 2.0, door.0[1] - door.1[1] as f32 * 2.0, ((landing.at[1] + 1) * 2) as f32];
    ways.push(make_way(WayKind::Grand, None, 0, sizes.stair, sizes.stair + 2, true, vec![from, to]));
    for &(a, b, join) in &joins {
        if !live(&spaces[a]) || !live(&spaces[b]) || a == b {
            continue;
        }
        let (width, height) = match join {
            Join::Shortcut => (3, 4),
            _ => (sizes.passage[0], sizes.passage[1]),
        };
        let kind = if join == Join::Shortcut { WayKind::Shortcut } else { WayKind::Passage };
        if let Some(line) = route(&spaces, a, b, width, height, land, cover, seed) {
            ways.push(make_way(kind, Some(a), b, width, height, join == Join::Main, line));
        }
    }
    if let Some(way) = into_caves(dungeon, &spaces, land.ground()) {
        ways.push(way);
    }
    let shafts = if dungeon.shafts { shafts(&spaces, land) } else { Vec::new() };
    let wanted = 1 + joins.iter().filter(|(a, b, _)| a != b && live(&spaces[*a]) && live(&spaces[*b])).count() as u32;
    let lights = lights(&spaces, &ways, dungeon);
    let mut low = [i32::MAX; 3];
    let mut high = [i32::MIN; 3];
    let mut grow = |lo: [i32; 3], hi: [i32; 3]| {
        for axis in 0..3 {
            low[axis] = low[axis].min(lo[axis]);
            high[axis] = high[axis].max(hi[axis]);
        }
    };
    for space in spaces.iter().filter(|space| live(space)) {
        let pad = 6;
        grow(
            [space.at[0] - space.half[0] - pad, space.bottom() - 4, space.at[2] - space.half[1] - pad],
            [space.at[0] + space.half[0] + pad, space.top() + pad, space.at[2] + space.half[1] + pad],
        );
    }
    for way in &ways {
        let ys = way.line.iter().map(|p| (p[2] / 2.0) as i32);
        let (ylo, yhi) = (ys.clone().min().unwrap_or(0), ys.max().unwrap_or(0));
        grow([way.low[0] - 2, ylo - 3, way.low[1] - 2], [way.high[0] + 2, yhi + way.height + 4, way.high[1] + 2]);
    }
    // The entrance and what stands over it.
    let e = entrance.at;
    grow([e[0] - 24, e[1] - 6, e[2] - 24], [e[0] + 24, e[1] + 30, e[2] + 24]);
    for (at, [foot, top]) in &shafts {
        grow([at[0] - 3, *foot, at[1] - 3], [at[0] + 3, top + 2, at[1] + 3]);
    }
    DungeonLayout { entrance, spaces, ways, lights, floors, joins: wanted, low, high, shaft_floor, shafts }
}

/// Where daylight falls into the great halls: over the middle of each hall
/// and flooded hall of the first floor, where the land over it lies level
/// for 12 blocks round and no more than 48 blocks of rock over its roof.
fn shafts(spaces: &[Space], land: &Land) -> Vec<([i32; 2], [i32; 2])> {
    let mut shafts = Vec::new();
    for space in spaces.iter().filter(|space| space.height >= 0 && space.floor == 0) {
        if !matches!(space.kind, SpaceKind::Hall | SpaceKind::Flooded) {
            continue;
        }
        let at = [space.at[0], space.at[2]];
        let top = land.top(at);
        let level = [[12, 0], [-12, 0], [0, 12], [0, -12]]
            .iter()
            .all(|d| (land.top([at[0] + d[0], at[1] + d[1]]) - top).abs() <= 2);
        let roof = space.at[1] + space.height;
        if level && top - roof <= 48 && top > land.sea + 1 {
            shafts.push((at, [roof - 2, top + 1]));
        }
    }
    shafts
}

/// A rough passage from the space nearest a cave of the world's generator
/// to that cave room's floor, where one lies within the dungeon's `caves`
/// blocks of a space and the way climbs or falls no steeper than it runs
/// along: so the dungeon and the caves are one. None where none does.
fn into_caves(dungeon: &Dungeon, spaces: &[Space], ground: &Ground) -> Option<Way> {
    let caves = ground.world.caves().filter(|_| dungeon.caves > 0)?;
    let first = spaces.first()?;
    let (size, [px, pz]) = (ground.size, ground.position);
    let metres = |[x, y, z]: [i32; 3]| [px + (x as f32 + 0.5) * size, y as f32 * size, pz + (z as f32 + 0.5) * size];
    let block = |[x, y, z]: [f32; 3]| {
        [((x - px) / size).floor() as i32, (y / size).floor() as i32, ((z - pz) / size).floor() as i32]
    };
    let near = caves.rooms_near(metres(first.at), (dungeon.caves as f32 + 400.0) * size);
    let mut best: Option<(f32, usize, [i32; 3])> = None;
    for (at, space) in spaces.iter().enumerate().filter(|(_, space)| space.height >= 0) {
        for cave in &near {
            let floor = block(cave.floor);
            let (dx, dy, dz) = (floor[0] - space.at[0], floor[1] - space.at[1], floor[2] - space.at[2]);
            let along = ((dx * dx + dz * dz) as f32).sqrt();
            let past = along - space.half[0].max(space.half[1]) as f32;
            if past > dungeon.caves as f32 || dy.abs() as f32 > along {
                continue;
            }
            if best.is_none_or(|(least, ..)| past < least) {
                best = Some((past, at, floor));
            }
        }
    }
    let (_, at, floor) = best?;
    let space = &spaces[at];
    let from = [space.at[0] as f32 + 0.5, space.at[2] as f32 + 0.5, ((space.at[1] + 1) * 2) as f32];
    let to = [floor[0] as f32 + 0.5, floor[2] as f32 + 0.5, ((floor[1] + 1) * 2) as f32];
    let length = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
    let count = length.ceil().max(1.0) as usize;
    let line = (0..=count)
        .map(|i| {
            let t = i as f32 / count as f32;
            [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t, from[2] + (to[2] - from[2]) * t]
        })
        .collect();
    // Its far end is no space of the dungeon's.
    Some(make_way(WayKind::Passage, Some(at), usize::MAX, 4, 4, false, line))
}

/// Where a way from `toward` meets `space`'s wall: the middle of the side
/// facing it, a little towards it, and the way out of that side.
fn door_of(space: &Space, toward: [f32; 2]) -> ([f32; 2], [i32; 2]) {
    let (cx, cz) = (space.at[0] as f32, space.at[2] as f32);
    let (dx, dz) = (toward[0] - cx, toward[1] - cz);
    let (hx, hz) = (space.half[0] as f32, space.half[1] as f32);
    if dx.abs() * hz >= dz.abs() * hx {
        let way = if dx >= 0.0 { 1 } else { -1 };
        let z = cz + (dz * 0.3).clamp(-(hz - 4.0).max(0.0), (hz - 4.0).max(0.0));
        ([cx + hx * way as f32, z], [way, 0])
    } else {
        let way = if dz >= 0.0 { 1 } else { -1 };
        let x = cx + (dx * 0.3).clamp(-(hx - 4.0).max(0.0), (hx - 4.0).max(0.0));
        ([x, cz + hz * way as f32], [0, way])
    }
}

/// A way's middle line from space `a` to space `b`, `width` wide and
/// `height` high, round every other space whose heights it would meet and
/// under the land: from each door straight out, then round on a grid of
/// two blocks, the cheapest way with few turns. None where there is no way.
#[allow(clippy::too_many_arguments)]
fn route(
    spaces: &[Space],
    a: usize,
    b: usize,
    width: i32,
    height: i32,
    land: &Land,
    cover: i32,
    seed: u32,
) -> Option<Vec<[f32; 3]>> {
    let (sa, sb) = (&spaces[a], &spaces[b]);
    let (da, out_a) = door_of(sa, [sb.at[0] as f32, sb.at[2] as f32]);
    let (db, out_b) = door_of(sb, [sa.at[0] as f32, sa.at[2] as f32]);
    // Each end's level: the nearer pair of the two spaces' levels.
    let mut pair = (sa.at[1], sb.at[1]);
    let mut best = i32::MAX;
    for la in [sa.at[1], sa.upper] {
        for lb in [sb.at[1], sb.upper] {
            if (la - lb).abs() < best {
                best = (la - lb).abs();
                pair = (la, lb);
            }
        }
    }
    let (ya, yb) = pair;
    let reach = (width / 2 + 5) as f32;
    let start = [da[0] + out_a[0] as f32 * reach, da[1] + out_a[1] as f32 * reach];
    let end = [db[0] + out_b[0] as f32 * reach, db[1] + out_b[1] as f32 * reach];
    const CELL: f32 = 2.0;
    let pad = 48.0;
    let low = [start[0].min(end[0]) - pad, start[1].min(end[1]) - pad];
    let high = [start[0].max(end[0]) + pad, start[1].max(end[1]) + pad];
    let count = [((high[0] - low[0]) / CELL) as i32 + 1, ((high[1] - low[1]) / CELL) as i32 + 1];
    let index = |c: [i32; 2]| (c[1] * count[0] + c[0]) as usize;
    let cell_of = |p: [f32; 2]| [((p[0] - low[0]) / CELL).round() as i32, ((p[1] - low[1]) / CELL).round() as i32];
    let place = |c: [i32; 2]| [low[0] + c[0] as f32 * CELL, low[1] + c[1] as f32 * CELL];
    let (ylo, yhi) = (ya.min(yb), ya.max(yb));
    let room = (width / 2 + 2) as f32;
    // How dear each cell is: through a space it never goes; under too little land it goes only at a cost.
    let cost = |c: [i32; 2]| -> Option<f32> {
        let p = place(c);
        for space in spaces.iter().filter(|space| space.height >= 0) {
            if space.bottom() - 4 > yhi + height || space.top() + 4 < ylo {
                continue;
            }
            let inside = (p[0] - space.at[0] as f32).abs() <= space.half[0] as f32 + room
                && (p[1] - space.at[2] as f32).abs() <= space.half[1] as f32 + room;
            if inside {
                return None;
            }
        }
        let at = [p[0].round() as i32, p[1].round() as i32];
        let under = land.lowest(at) < yhi + height + cover;
        Some(1.0 + if under { 40.0 } else { 0.0 } + noise::unit(at[0], at[1], 5, seed) * 0.4)
    };
    let (s, e) = (cell_of(start), cell_of(end));
    if s[0] < 0
        || s[1] < 0
        || e[0] < 0
        || e[1] < 0
        || s[0] >= count[0]
        || s[1] >= count[1]
        || e[0] >= count[0]
        || e[1] >= count[1]
    {
        return None;
    }
    let total = (count[0] * count[1]) as usize;
    let mut spent = vec![f32::INFINITY; total];
    let mut came = vec![u32::MAX; total];
    let mut heap = BinaryHeap::new();
    let guess = |c: [i32; 2]| (((c[0] - e[0]).abs() + (c[1] - e[1]).abs()) as f32) * 1.0;
    spent[index(s)] = 0.0;
    heap.push(Entry(guess(s), s, [0, 0]));
    let steps: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    let mut found = false;
    let mut visits = 0;
    while let Some(Entry(_, c, from)) = heap.pop() {
        visits += 1;
        if c == e {
            found = true;
            break;
        }
        if visits > 200_000 {
            break;
        }
        let here = spent[index(c)];
        for step in steps {
            let n = [c[0] + step[0], c[1] + step[1]];
            if n[0] < 0 || n[1] < 0 || n[0] >= count[0] || n[1] >= count[1] {
                continue;
            }
            // Its ends may lie beside a space's wall: the door's own cells are always open.
            let Some(price) = (if n == e || n == s { Some(1.0) } else { cost(n) }) else { continue };
            let turn = if from != [0, 0] && from != step { 0.6 } else { 0.0 };
            let next = here + price + turn;
            if next < spent[index(n)] {
                spent[index(n)] = next;
                came[index(n)] = index(c) as u32;
                heap.push(Entry(next + guess(n), n, step));
            }
        }
    }
    if !found {
        return None;
    }
    let mut cells = vec![e];
    let mut at = index(e);
    while came[at] != u32::MAX {
        at = came[at] as usize;
        cells.push([(at as i32) % count[0], (at as i32) / count[0]]);
    }
    cells.reverse();
    // Only the corners kept, then each corner cut, so it bends rather than turns square.
    let mut corners: Vec<[f32; 2]> = vec![place(cells[0])];
    for i in 1..cells.len() - 1 {
        let (p, c, n) = (cells[i - 1], cells[i], cells[i + 1]);
        if [c[0] - p[0], c[1] - p[1]] != [n[0] - c[0], n[1] - c[1]] {
            corners.push(place(c));
        }
    }
    corners.push(place(*cells.last().expect("a cell")));
    let mut points = vec![[da[0] - out_a[0] as f32 * 2.0, da[1] - out_a[1] as f32 * 2.0], start];
    let cut = corners.len() > 2;
    for i in 0..corners.len() {
        if cut && i > 0 && i + 1 < corners.len() {
            let (p, c, n) = (corners[i - 1], corners[i], corners[i + 1]);
            let toward = |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            let lp = ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2)).sqrt();
            let ln = ((c[0] - n[0]).powi(2) + (c[1] - n[1]).powi(2)).sqrt();
            let r = 4.0f32.min(lp * 0.4).min(ln * 0.4);
            points.push(toward(c, p, r / lp.max(1e-3)));
            points.push(toward(c, n, r / ln.max(1e-3)));
        } else {
            points.push(corners[i]);
        }
    }
    points.push(end);
    points.push([db[0] - out_b[0] as f32 * 2.0, db[1] - out_b[1] as f32 * 2.0]);
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01);
    // The walking surface along it: level at each end, a stair between, in half blocks.
    let length: f32 =
        points.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
    let (sa_half, sb_half) = ((ya + 1) * 2, (yb + 1) * 2);
    let mut along = 0.0;
    let mut line = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            along += ((p[0] - points[i - 1][0]).powi(2) + (p[1] - points[i - 1][1]).powi(2)).sqrt();
        }
        line.push([p[0], p[1], surface_at(along, length, sa_half, sb_half) as f32]);
    }
    Some(line)
}

/// The walking surface `along` blocks along a way `length` long, from
/// `from` to `to` half blocks: level for a few blocks at each end, then a
/// half block down or up for every block along, a landing four blocks long
/// every twenty half blocks; steeper where it has not the room.
pub(crate) fn surface_at(along: f32, length: f32, from: i32, to: i32) -> i32 {
    let steps = (to - from).abs();
    if steps == 0 {
        return from;
    }
    let way = (to - from).signum();
    let pad = 4.0f32.min(length * 0.2);
    let room = (length - pad * 2.0).max(1.0);
    let run = steps as f32 + (steps / 20) as f32 * 4.0;
    let done = if room >= run {
        let begin = pad + (room - run) * 0.5;
        let u = (along - begin).clamp(0.0, run);
        let flight = (u / 24.0).floor();
        let rest = u - flight * 24.0;
        (flight as i32 * 20 + rest.min(20.0).floor() as i32).min(steps)
    } else {
        let u = (along - pad).clamp(0.0, room);
        ((u / room) * steps as f32).floor() as i32
    };
    from + way * done.min(steps)
}

/// A cell of the routing grid waiting to be looked at: what it is guessed
/// to cost to the end through it, the cell, and the step that came to it.
#[derive(Clone, Copy)]
struct Entry(f32, [i32; 2], [i32; 2]);

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.total_cmp(&self.0)
    }
}

/// A way along `line`, its columns found: every block within half its
/// width of the line, each with the walking surface of the line's nearest
/// point.
fn make_way(
    kind: WayKind,
    from: Option<usize>,
    to: usize,
    width: i32,
    height: i32,
    main: bool,
    mut line: Vec<[f32; 3]>,
) -> Way {
    // A grand stair's surface falls along it as a way's does.
    if kind == WayKind::Grand && line.len() == 2 {
        let length = ((line[1][0] - line[0][0]).powi(2) + (line[1][1] - line[0][1]).powi(2)).sqrt();
        let (from_s, to_s) = (line[0][2] as i32, line[1][2] as i32);
        let count = length.ceil().max(1.0) as usize;
        let mut points = Vec::with_capacity(count + 1);
        for i in 0..=count {
            let t = i as f32 / count as f32;
            let along = length * t;
            points.push([
                line[0][0] + (line[1][0] - line[0][0]) * t,
                line[0][1] + (line[1][1] - line[0][1]) * t,
                surface_at(along, length, from_s, to_s) as f32,
            ]);
        }
        line = points;
    }
    // As far out as its walls may wander.
    let r = width as f32 * 0.5 + 2.0;
    let mut cells: HashMap<[i32; 2], WayCell> = HashMap::new();
    let mut along = 0.0;
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let lo = [(a[0].min(b[0]) - r - 1.0).floor() as i32, (a[1].min(b[1]) - r - 1.0).floor() as i32];
        let hi = [(a[0].max(b[0]) + r + 1.0).ceil() as i32, (a[1].max(b[1]) + r + 1.0).ceil() as i32];
        for z in lo[1]..=hi[1] {
            for x in lo[0]..=hi[0] {
                let p = [x as f32 + 0.5, z as f32 + 0.5];
                let (d, t) = if length < 1e-4 {
                    (((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt(), 0.0)
                } else {
                    let t = (((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / (length * length))
                        .clamp(0.0, 1.0);
                    let q = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    (((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt(), t)
                };
                if d > r {
                    continue;
                }
                let surface = (a[2] + (b[2] - a[2]) * t).round() as i32;
                let cell = WayCell { at: [x, z], surface, off: d, along: along + length * t };
                let keep = cells.get(&[x, z]).is_none_or(|kept| d < kept.off);
                if keep {
                    cells.insert([x, z], cell);
                }
            }
        }
        along += length;
    }
    // A stair's surface steps with the line's: the nearest point's, but a cell beside a step takes the step it
    // stands beside, so a tread runs straight across the way.
    let mut cells: Vec<WayCell> = cells.into_values().collect();
    cells.sort_by_key(|cell| cell.at);
    let low = [cells.iter().map(|c| c.at[0]).min().unwrap_or(0), cells.iter().map(|c| c.at[1]).min().unwrap_or(0)];
    let high = [cells.iter().map(|c| c.at[0]).max().unwrap_or(0), cells.iter().map(|c| c.at[1]).max().unwrap_or(0)];
    Way { kind, from, to, width, height, main, line, cells, low, high, length: along }
}

/// Where the lights go: braziers along each main way every fourteen blocks,
/// on alternate sides, crystals along the side ways every eighteen, and each
/// space's own.
fn lights(spaces: &[Space], ways: &[Way], dungeon: &Dungeon) -> Vec<LightSpot> {
    let mut lights = Vec::new();
    if dungeon.passes.lights <= 0.0 {
        return lights;
    }
    let spacing = |base: f32| base / dungeon.passes.lights.clamp(0.25, 1.0);
    for way in ways {
        if way.kind == WayKind::Shortcut {
            continue;
        }
        let (kind, every) =
            if way.main { (LightKind::Brazier, spacing(14.0)) } else { (LightKind::Crystal, spacing(18.0)) };
        let mut along = 6.0;
        let mut side = 1.0;
        while along < way.length - 4.0 {
            let (p, dir) = way.point(along);
            let off = way.width as f32 * 0.5 - 0.8;
            let at = [(p[0] + -dir[1] * off * side).floor() as i32, (p[1] + dir[0] * off * side).floor() as i32];
            let surface = way.cell(at).map_or(p[2] as i32, |cell| cell.surface);
            // Only where the floor is a whole block: never on a stair's half step.
            if surface % 2 == 0 {
                lights.push(LightSpot { at: [at[0], surface / 2, at[1]], kind });
            }
            along += every;
            side = -side;
        }
    }
    for space in spaces.iter().filter(|space| space.height >= 0) {
        let floor = space.at[1] + 1;
        let [hx, hz] = space.half;
        let corners = [[-1, -1], [1, -1], [1, 1], [-1, 1]];
        match space.kind {
            SpaceKind::Room | SpaceKind::Landing | SpaceKind::Shrine | SpaceKind::Treasury => {
                for [sx, sz] in corners {
                    lights.push(LightSpot {
                        at: [space.at[0] + sx * (hx - 3), floor, space.at[2] + sz * (hz - 3)],
                        kind: LightKind::Brazier,
                    });
                }
            }
            SpaceKind::Hall | SpaceKind::Flooded => {
                // Pools of light down both sides, dark between.
                let long = if space.along_x { hx } else { hz };
                let across = if space.along_x { hz } else { hx };
                let mut a = -long + 6;
                while a <= long - 6 {
                    for s in [-1, 1] {
                        let (x, z) = if space.along_x { (a, s * (across - 6)) } else { (s * (across - 6), a) };
                        let kind =
                            if space.kind == SpaceKind::Flooded { LightKind::Crystal } else { LightKind::Brazier };
                        let y = floor;
                        lights.push(LightSpot { at: [space.at[0] + x, y, space.at[2] + z], kind });
                    }
                    a += 16;
                }
            }
            SpaceKind::Arena => {
                let r = hx as f32 - 4.0;
                for i in 0..8 {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_4 + std::f32::consts::FRAC_PI_8;
                    lights.push(LightSpot {
                        at: [space.at[0] + (angle.cos() * r) as i32, floor, space.at[2] + (angle.sin() * r) as i32],
                        kind: LightKind::Brazier,
                    });
                }
            }
            SpaceKind::Chasm => {
                lights.push(LightSpot { at: [space.at[0], space.bottom() + 1, space.at[2]], kind: LightKind::Glow });
            }
            SpaceKind::Gorge => {
                lights.push(LightSpot { at: [space.at[0], floor, space.at[2]], kind: LightKind::Crystal });
            }
            SpaceKind::Descent => {
                lights.push(LightSpot { at: [space.at[0], floor, space.at[2]], kind: LightKind::Glow });
            }
        }
    }
    lights
}

/// The layouts planned, by site, shared by every thread: a site is asked
/// about by every chunk column it reaches.
type Layouts = Mutex<HashMap<u64, Arc<OnceLock<Option<Arc<DungeonLayout>>>>>>;
static LAYOUTS: LazyLock<Layouts> = LazyLock::new(Layouts::default);
const LAYOUTS_KEPT: usize = 128;

/// The layout kept for `key`, planned by `make` the first time any thread
/// asks.
pub(crate) fn cached_layout(key: u64, make: impl FnOnce() -> Option<DungeonLayout>) -> Option<Arc<DungeonLayout>> {
    let cell = {
        let mut kept = LAYOUTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if kept.len() >= LAYOUTS_KEPT && !kept.contains_key(&key) {
            kept.clear();
        }
        kept.entry(key).or_default().clone()
    };
    cell.get_or_init(|| make().map(Arc::new)).clone()
}

/// What tells one dungeon at one site from every other: its settings, the
/// world, where its blocks lie in the world, the site and the world's
/// lowest block.
pub(crate) fn layout_key(dungeon: &Dungeon, ground: &Ground, site: [i32; 2], bottom: i32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ground.world.hash().hash(&mut hasher);
    ground.size.to_bits().hash(&mut hasher);
    ground.position.map(f32::to_bits).hash(&mut hasher);
    ground.halved.hash(&mut hasher);
    site.hash(&mut hasher);
    bottom.hash(&mut hasher);
    (dungeon.theme, dungeon.size, dungeon.floors, &dungeon.entrances, dungeon.cover, dungeon.caves, dungeon.shafts)
        .hash(&mut hasher);
    dungeon.blocks.hash(&mut hasher);
    for value in [dungeon.passes.ruin, dungeon.passes.wear, dungeon.passes.overgrowth, dungeon.passes.lights] {
        value.to_bits().hash(&mut hasher);
    }
    format!("{:?}", dungeon.sizes).hash(&mut hasher);
    for (role, kind) in &dungeon.rooms {
        (role, kind.most, kind.weight.to_bits(), kind.zone.map(f32::to_bits), kind.piece.size).hash(&mut hasher);
        kind.piece.blocks.hash(&mut hasher);
    }
    if let Some(entrance) = &dungeon.entrance {
        (entrance.size, &entrance.blocks).hash(&mut hasher);
    }
    hasher.finish()
}
