//! A network of caves: rooms set in the regions of a grid, turned so their
//! exits face the rooms they join, and winding tunnels from exit to exit.
//!
//! Everything a place in space can be hollowed by comes from the region it
//! is in and those round it, and each of those is worked out from the
//! region's own numbers and the seed alone. So a chunk made by itself
//! gathers what a chunk beside it gathers, and the two agree where they
//! meet. What is gathered is joined by the greater of each, never
//! smoothly: a smooth join of two different gatherings would differ.
//!
//! A room lies some way under the ground at its own middle, so gathering a
//! region asks the generator's ground for a height at each room round it.
//! That is the costly part, and the same for every cell of the region, so
//! what is gathered is kept a while (`gathered`): kept or made again, it is
//! the same numbers.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use super::{
    grid::{Grid, Region, Stand, V3, add, cross, dot, sub, times, unit},
    level::Level,
    rooms::{NOTHING, Room, greater, lesser},
};
use crate::{
    generate::eval::{Givens, Program},
    noise::gradient3,
};

/// How a network's rooms are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaveLayout {
    /// Rooms in every direction without end, a region each.
    #[default]
    Endless,
    /// A bounded cave: a main path of rooms with side rooms off it.
    Level,
}

/// How many pieces a tunnel's line is made of.
const PIECES: usize = 16;

/// How likely a region of an endless network has a room, and two rooms
/// side by side a tunnel between them.
const ROOMED: f32 = 0.9;
const JOINED: f32 = 0.7;

/// How far under a tunnel's middle its floor lies, of half its width.
const FLOOR: f32 = 0.55;

/// A network's values, as its kernel reads them.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub rooms: Vec<Room>,
    pub layout: CaveLayout,
    pub spacing: f32,
    pub depth: f32,
    /// Half a tunnel's width.
    pub radius: f32,
    pub winding: f32,
    pub rough: f32,
    pub roof: f32,
    pub entrances: f32,
    pub main: u32,
    pub side: u32,
    pub seed: u32,
}

/// A box in space, along the axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub low: V3,
    pub high: V3,
}

impl Bounds {
    pub const NONE: Bounds = Bounds { low: [f32::MAX; 3], high: [f32::MIN; 3] };

    pub fn round(at: V3, reach: f32) -> Self {
        Self { low: at.map(|v| v - reach), high: at.map(|v| v + reach) }
    }

    pub fn with(self, other: Bounds) -> Self {
        let mut all = self;
        for axis in 0..3 {
            all.low[axis] = lesser(all.low[axis], other.low[axis]);
            all.high[axis] = greater(all.high[axis], other.high[axis]);
        }
        all
    }

    pub fn holds(&self, p: V3) -> bool {
        (0..3).all(|axis| p[axis] >= self.low[axis] && p[axis] <= self.high[axis])
    }

}

/// A whole number from `seed` and `values`, the same on every machine.
pub(crate) fn mix(seed: u32, values: &[i32]) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    for value in values {
        h = (h ^ *value as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
    }
    h
}

/// A number from 0 up to 1 from a whole one.
pub(crate) fn share(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

/// A room of a region before its height is known: which room, where it
/// stands, how it is turned, and what it is joined to.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub region: Region,
    pub room: usize,
    pub stand: Stand,
    pub turns: u8,
    pub mirrored: bool,
    pub beside: [Region; 4],
    /// Whether a tunnel joins it to each of the regions beside it.
    pub links: [bool; 4],
    /// The exit each of those tunnels leaves by.
    pub toward: [Option<usize>; 4],
    /// The way to each region beside it, along its own floor.
    pub ways: [V3; 4],
    /// Which of its shapes it has, a bit each.
    pub present: u64,
    /// Where between its least and greatest depth it lies, 0 to 1.
    pub deep: f32,
    pub entrance: bool,
}

/// A room in the ground.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    pub plan: Plan,
    pub at: V3,
    /// Its own x, y and z in space.
    pub axes: [V3; 3],
    pub bounds: Bounds,
}

/// A tunnel: its line, the way up along it, half its width, and whether it
/// is kept under the ground's roof.
#[derive(Clone, Debug)]
pub(crate) struct Tunnel {
    pub line: [V3; PIECES + 1],
    pub up: V3,
    pub radius: f32,
    pub roofed: bool,
    pub bounds: Bounds,
}

/// The rooms and tunnels that can reach into one region, or all of a
/// level's.
#[derive(Clone, Debug, Default)]
pub(crate) struct Gathered {
    pub rooms: Vec<Placed>,
    pub tunnels: Vec<Tunnel>,
    pub bounds: Option<Bounds>,
}

/// A network compiled: its values, the ground it lies under, and for a
/// level its rooms laid out.
#[derive(Debug)]
pub(crate) struct Network {
    pub set: Settings,
    /// The generator's own ground, for the heights rooms lie under.
    pub ground: Arc<Program>,
    /// What tells it from every other network, for what is kept of it.
    pub id: u64,
    pub level: Option<Level>,
}

thread_local! {
    /// What was last gathered on this thread, by network and region.
    static GATHERED: RefCell<Vec<(u64, Region, Rc<Gathered>)>> = const { RefCell::new(Vec::new()) };
    /// The heights levels were last found to lie at, by network.
    static LEVELS: RefCell<Vec<(u64, f32)>> = const { RefCell::new(Vec::new()) };
}

/// How many gatherings a thread keeps.
const KEPT: usize = 24;

impl Settings {
    /// The room with the furthest reach's, in metres.
    pub fn reach(&self) -> f32 {
        self.rooms.iter().map(Room::reach).fold(1.0, greater)
    }

    /// How far a tunnel may stray from the straight line between its ends
    /// and a wall from where its shape puts it, in metres.
    pub fn stray(&self) -> f32 {
        self.radius + self.rough + 2.0
    }

    /// How far from its region's middle a room may be moved, of half the
    /// region, so it and its tunnels' ends stay inside.
    fn loose(&self) -> f32 {
        let free = 1.0 - 2.0 * (self.reach() + self.stray()) / self.spacing;
        greater(lesser(free, 0.8), 0.0)
    }

    pub fn room(&self, pick: f32) -> usize {
        let total: f32 = self.rooms.iter().map(|room| greater(room.weight, 0.0)).sum();
        let mut left = pick * total;
        for (at, room) in self.rooms.iter().enumerate() {
            left -= greater(room.weight, 0.0);
            if left < 0.0 {
                return at;
            }
        }
        0
    }
}

/// `v`, a way or place in a room's space, as the room turned `turns`
/// quarter turns and `mirrored` has it.
fn turned(v: V3, turns: u8, mirrored: bool) -> V3 {
    let x = if mirrored { -v[0] } else { v[0] };
    let (x, z) = match turns % 4 {
        0 => (x, v[2]),
        1 => (-v[2], x),
        2 => (-x, -v[2]),
        _ => (v[2], -x),
    };
    [x, v[1], z]
}

impl Network {
    /// A network of these values under `ground`, known from every other by `id`: a level laid out.
    pub fn new(set: Settings, ground: Arc<Program>, id: u64) -> Self {
        let mut network = Network { set, ground, id, level: None };
        if network.set.layout == CaveLayout::Level {
            network.level = Some(Level::laid(&network));
        }
        network
    }

    /// The grid its regions lie on.
    pub fn grid(&self) -> Grid {
        Grid { spacing: self.set.spacing }
    }

    /// The ground's height over each of `feet`.
    pub fn grounds(&self, feet: &[V3]) -> Vec<f32> {
        let mut heights = vec![0.0; feet.len()];
        if feet.is_empty() {
            return heights;
        }
        let [x, z] = [0, 2].map(|axis| feet.iter().map(|foot| foot[axis]).collect::<Vec<f32>>());
        self.ground.run(feet.len(), Givens { x: &x, z: &z, ..Givens::default() }, &mut [&mut heights]);
        heights
    }

    /// Whether an endless network has a room in `region`.
    fn roomed(&self, region: Region) -> bool {
        share(mix(self.set.seed, &[region[0], region[1], region[2], 1])) < ROOMED
    }

    /// Whether a tunnel joins the rooms of two regions side by side, both
    /// with a room: the same asked from either.
    fn joined(&self, a: Region, b: Region) -> bool {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        share(mix(self.set.seed, &[low[0], low[1], low[2], high[0], high[1], high[2], 2])) < JOINED
    }

    /// The room of `region` in an endless network, before its height: none
    /// where the region has none.
    pub fn plan(&self, grid: &Grid, region: Region) -> Option<Plan> {
        if !self.roomed(region) {
            return None;
        }
        let set = &self.set;
        let at = |part: i32| share(mix(set.seed, &[region[0], region[1], region[2], part]));
        let beside = grid.beside(region);
        let links = beside.map(|next| self.roomed(next) && self.joined(region, next));
        let loose = set.loose();
        let jitter = [(at(3) * 2.0 - 1.0) * loose, (at(4) * 2.0 - 1.0) * loose];
        let entrance = at(8) < set.entrances;
        Some(self.planned(grid, region, set.room(at(5)), jitter, links, at(6), entrance))
    }

    /// A room of `region` joined as `links` say, turned so its exits face
    /// what it is joined to.
    #[allow(clippy::too_many_arguments)]
    pub fn planned(
        &self,
        grid: &Grid,
        region: Region,
        room: usize,
        jitter: [f32; 2],
        links: [bool; 4],
        deep: f32,
        entrance: bool,
    ) -> Plan {
        let set = &self.set;
        let stand = grid.stand(region, jitter);
        let beside = grid.beside(region);
        let middle = grid.stand(region, [0.0, 0.0]).foot;
        let ways = beside.map(|next| {
            let away = sub(grid.stand(next, [0.0, 0.0]).foot, middle);
            unit([dot(away, stand.x), 0.0, dot(away, stand.z)], [1.0, 0.0, 0.0])
        });
        let exits = &set.rooms[room].exits;
        // Each way of turning it, from one the region picks: the first whose exits face its tunnels best.
        let first = mix(set.seed, &[region[0], region[1], region[2], 7]) % 8;
        let mut best: Option<(f32, u8, bool, [Option<usize>; 4])> = None;
        for step in 0..8 {
            let way = (first + step) % 8;
            let (turns, mirrored) = ((way % 4) as u8, way >= 4);
            let mut used = vec![false; exits.len()];
            let (mut score, mut toward) = (0.0, [None; 4]);
            for slot in (0..4).filter(|&slot| links[slot]) {
                let facing = |exit: usize| dot(turned(exits[exit].way, turns, mirrored), ways[slot]);
                let mut pick: Option<(f32, usize)> = None;
                // An exit no other tunnel has, while there is one.
                for spare in [true, false] {
                    for exit in (0..exits.len()).filter(|&exit| !(spare && used[exit])) {
                        let faces = facing(exit);
                        if pick.is_none_or(|(most, _)| faces > most) {
                            pick = Some((faces, exit));
                        }
                    }
                    if pick.is_some() {
                        break;
                    }
                }
                if let Some((faces, exit)) = pick {
                    used[exit] = true;
                    toward[slot] = Some(exit);
                    score += faces;
                }
            }
            if best.is_none_or(|(most, ..)| score > most) {
                best = Some((score, turns, mirrored, toward));
            }
        }
        let (_, turns, mirrored, toward) = best.unwrap_or((0.0, 0, false, [None; 4]));
        let shapes = &set.rooms[room].shapes;
        let mut present = 0u64;
        for (at, shape) in shapes.iter().enumerate().take(64) {
            let has = share(mix(set.seed, &[region[0], region[1], region[2], 16 + at as i32])) < shape.chance;
            present |= u64::from(has) << at;
        }
        Plan { region, room, stand, turns, mirrored, beside, links, toward, ways, present, deep, entrance }
    }

    /// `plan`'s room where the ground over its foot is `ground` metres
    /// high, `rise` metres up from there.
    pub fn placed(&self, grid: &Grid, plan: Plan, ground: f32, rise: f32) -> Placed {
        let stand = plan.stand;
        let at = grid.lifted(&stand, ground, rise);
        let along = |v: V3| {
            let v = turned(v, plan.turns, plan.mirrored);
            add(add(times(stand.x, v[0]), times(stand.up, v[1])), times(stand.z, v[2]))
        };
        let axes = [along([1.0, 0.0, 0.0]), stand.up, along([0.0, 0.0, 1.0])];
        let bounds = Bounds::round(at, self.set.rooms[plan.room].reach() + self.set.rough);
        Placed { plan, at, axes, bounds }
    }

    /// How far under the ground at its middle an endless network's room
    /// lies: its floor, in metres.
    fn sunk(&self, plan: &Plan) -> f32 {
        -self.set.depth * (0.75 + 0.5 * plan.deep)
    }

    /// Everything of an endless network that can reach into `region`.
    fn gather(&self, grid: &Grid, region: Region) -> Gathered {
        // The region, the four beside it, and those beside them: each once, in this order.
        let mut regions = vec![region];
        regions.extend(grid.beside(region));
        for at in 1..5 {
            for next in grid.beside(regions[at]) {
                if !regions.contains(&next) {
                    regions.push(next);
                }
            }
        }
        let plans: Vec<Option<Plan>> = regions.iter().map(|&region| self.plan(grid, region)).collect();
        let inner = regions.len().min(5);
        // The ground over every room, and over where each inner room's entrance comes out, asked at once.
        let mut feet: Vec<V3> = plans.iter().flatten().map(|plan| plan.stand.foot).collect();
        let mouths: Vec<Option<V3>> = plans[..inner]
            .iter()
            .map(|plan| plan.as_ref().filter(|plan| plan.entrance).map(|plan| self.mouth(grid, plan)))
            .collect();
        feet.extend(mouths.iter().flatten());
        let heights = self.grounds(&feet);
        let mut heights = heights.into_iter();
        let placed: Vec<Option<Placed>> = plans
            .into_iter()
            .map(|plan| {
                let plan = plan?;
                let (ground, rise) = (heights.next().unwrap_or(0.0), self.sunk(&plan));
                Some(self.placed(grid, plan, ground, rise))
            })
            .collect();
        let find =
            |region: Region| regions.iter().position(|&known| known == region).and_then(|at| placed[at].as_ref());
        let mut gathered = Gathered::default();
        let mut pairs: Vec<(Region, Region)> = Vec::new();
        for room in placed[..inner].iter().flatten() {
            for slot in (0..4).filter(|&slot| room.plan.links[slot]) {
                let next = room.plan.beside[slot];
                let pair = if room.plan.region <= next { (room.plan.region, next) } else { (next, room.plan.region) };
                if pairs.contains(&pair) {
                    continue;
                }
                pairs.push(pair);
                if let (Some(a), Some(b)) = (find(pair.0), find(pair.1)) {
                    gathered.tunnels.extend(self.tunnel(a, b));
                }
            }
        }
        for (room, mouth) in placed[..inner].iter().zip(&mouths) {
            if let (Some(room), Some(mouth)) = (room, mouth) {
                let ground = heights.next().unwrap_or(0.0);
                gathered.tunnels.push(self.entrance(grid, room, *mouth, ground));
            }
        }
        gathered.rooms = placed.into_iter().take(inner).flatten().collect();
        gathered.bound();
        gathered
    }

    /// What can reach into `region`: gathered now, or as it was kept.
    pub fn gathered(&self, grid: &Grid, region: Region) -> Rc<Gathered> {
        let kept = GATHERED.with_borrow(|kept| {
            kept.iter().find(|(id, known, _)| *id == self.id && *known == region).map(|kept| kept.2.clone())
        });
        if let Some(kept) = kept {
            return kept;
        }
        let made = Rc::new(self.gather(grid, region));
        GATHERED.with_borrow_mut(|kept| {
            if kept.len() >= KEPT {
                kept.remove(0);
            }
            kept.push((self.id, region, made.clone()));
        });
        made
    }

    /// How high a level's first room lies: `depth` under the ground over
    /// it.
    pub fn level_height(&self) -> f32 {
        let Some(level) = &self.level else { return 0.0 };
        let kept = LEVELS.with_borrow(|kept| kept.iter().find(|(id, _)| *id == self.id).map(|kept| kept.1));
        if let Some(kept) = kept {
            return kept;
        }
        let height = self.grounds(&[level.first])[0] - self.set.depth;
        LEVELS.with_borrow_mut(|kept| {
            if kept.len() >= KEPT {
                kept.remove(0);
            }
            kept.push((self.id, height));
        });
        height
    }

    /// Where a tunnel leaves `room` toward the region in its slot `slot`,
    /// and the way it leaves by.
    fn end(&self, room: &Placed, slot: usize) -> (V3, V3) {
        let own = &self.set.rooms[room.plan.room];
        let out = |v: V3| add(add(times(room.axes[0], v[0]), times(room.axes[1], v[1])), times(room.axes[2], v[2]));
        match room.plan.toward[slot] {
            Some(exit) => (add(room.at, out(own.exits[exit].at)), out(own.exits[exit].way)),
            // A room with no exits: from its middle, the way its neighbour lies.
            None => {
                let stand = &room.plan.stand;
                let way = room.plan.ways[slot];
                let away = add(times(stand.x, way[0]), times(stand.z, way[2]));
                (add(room.at, times(room.axes[1], 1.2)), away)
            }
        }
    }

    /// The tunnel between two rooms side by side, the lesser region's
    /// first: none where either does not know the other.
    pub fn tunnel(&self, a: &Placed, b: &Placed) -> Option<Tunnel> {
        let slot_a = a.plan.beside.iter().position(|&next| next == b.plan.region)?;
        let slot_b = b.plan.beside.iter().position(|&next| next == a.plan.region)?;
        let (from, to) = (self.end(a, slot_a), self.end(b, slot_b));
        let [x, y, z] = a.plan.region;
        let seed = mix(self.set.seed, &[x, y, z, b.plan.region[0], b.plan.region[1], b.plan.region[2], 9]);
        let up = unit(add(a.axes[1], b.axes[1]), a.axes[1]);
        Some(self.wound(from, to, up, seed, self.set.winding, true))
    }

    /// Where the entrance of `plan`'s room comes out: the foot of the
    /// column, some way off along its spare exit.
    fn mouth(&self, grid: &Grid, plan: &Plan) -> V3 {
        let way = self.spare(plan).1;
        let stand = &plan.stand;
        let far = lesser(greater(self.set.depth * 1.5, 12.0), self.set.spacing * 0.45);
        let over = add(stand.foot, add(times(stand.x, way[0] * far), times(stand.z, way[2] * far)));
        grid.foot(over).0
    }

    /// The exit an entrance leaves `plan`'s room by, and the way along the
    /// room's floor it goes: an exit no tunnel has, or its last.
    fn spare(&self, plan: &Plan) -> (Option<usize>, V3) {
        let exits = &self.set.rooms[plan.room].exits;
        let taken = |exit: usize| plan.toward.contains(&Some(exit));
        let exit = (0..exits.len()).find(|&exit| !taken(exit)).or(exits.len().checked_sub(1));
        let way = match exit {
            Some(exit) => turned(exits[exit].way, plan.turns, plan.mirrored),
            None => [1.0, 0.0, 0.0],
        };
        (exit, unit([way[0], 0.0, way[2]], [1.0, 0.0, 0.0]))
    }

    /// The way up from `room` to the surface, coming out over `mouth`
    /// where the ground is `ground` metres high.
    fn entrance(&self, grid: &Grid, room: &Placed, mouth: V3, ground: f32) -> Tunnel {
        let (exit, way) = self.spare(&room.plan);
        let stand = &room.plan.stand;
        let away = add(times(stand.x, way[0]), times(stand.z, way[2]));
        let own = &self.set.rooms[room.plan.room];
        let out = |v: V3| add(add(times(room.axes[0], v[0]), times(room.axes[1], v[1])), times(room.axes[2], v[2]));
        let from = match exit {
            Some(exit) => add(room.at, out(own.exits[exit].at)),
            None => add(room.at, times(room.axes[1], 1.2)),
        };
        let (foot, up) = grid.foot(mouth);
        let stand = Stand { foot, up, ..*stand };
        // A little over the ground, so it comes out into the open; and still climbing the way it was going
        // rather than straight up, so its mouth is a slope to walk down and not a pit.
        let top = grid.lifted(&stand, ground, self.set.radius * 0.5);
        let down = unit(sub(times(up, -0.4), away), times(up, -1.0));
        let [x, y, z] = room.plan.region;
        let seed = mix(self.set.seed, &[x, y, z, 10]);
        self.wound((from, away), (top, down), room.axes[1], seed, self.set.winding * 0.5, false)
    }

    /// A tunnel from one end to another, each a place and the way out of
    /// it: leaving each the way it faces, and wound sideways between by
    /// amounts from `seed`.
    fn wound(&self, from: (V3, V3), to: (V3, V3), up: V3, seed: u32, winding: f32, roofed: bool) -> Tunnel {
        let radius = self.set.radius;
        // Its ends a little inside the rooms, so it opens into them.
        let (a, b) = (sub(from.0, times(from.1, radius * 0.5)), sub(to.0, times(to.1, radius * 0.5)));
        let across = sub(b, a);
        let length = dot(across, across).sqrt();
        let side = unit(cross(up, across), [1.0, 0.0, 0.0]);
        let pull = length * 0.5;
        let (leave, arrive) = (times(from.1, pull), times(to.1, -pull));
        let most = greater(winding, 0.0) * lesser(length * 0.15, self.set.spacing * 0.2);
        // How far aside and how far up it is pushed at each of four places along it, and not at all at its ends.
        let knot = |at: usize, part: i32| match at {
            1..=4 => share(mix(seed, &[at as i32, part])) * 2.0 - 1.0,
            _ => 0.0,
        };
        let pushed = |t: f32, part: i32| {
            let along = t * 5.0;
            let before = lesser(along.floor(), 4.0);
            let part_way = along - before;
            let eased = part_way * part_way * (3.0 - 2.0 * part_way);
            let (low, high) = (knot(before as usize, part), knot(before as usize + 1, part));
            // Least near its ends, so it leaves a room and comes into one nearly straight.
            (low + (high - low) * eased) * 4.0 * t * (1.0 - t)
        };
        let mut line = [[0.0; 3]; PIECES + 1];
        let mut bounds = Bounds::NONE;
        for (at, point) in line.iter_mut().enumerate() {
            let t = at as f32 / PIECES as f32;
            let (t2, t3) = (t * t, t * t * t);
            let weights = [2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, 3.0 * t2 - 2.0 * t3, t3 - t2];
            let straight = add(
                add(times(a, weights[0]), times(leave, weights[1])),
                add(times(b, weights[2]), times(arrive, weights[3])),
            );
            let aside = add(times(side, most * pushed(t, 0)), times(up, most * 0.25 * pushed(t, 1)));
            *point = add(straight, aside);
            bounds = bounds.with(Bounds::round(*point, radius + self.set.rough));
        }
        Tunnel { line, up, radius, roofed, bounds }
    }
}

impl Tunnel {
    /// How far inside it `p` is, in metres: round, with a flat floor.
    fn inside(&self, p: V3) -> f32 {
        let mut hollow = NOTHING;
        for piece in self.line.windows(2) {
            let (a, along) = (piece[0], sub(piece[1], piece[0]));
            let length = dot(along, along);
            let t = if length > 0.0 { dot(sub(p, a), along) / length } else { 0.0 };
            let t = greater(lesser(t, 1.0), 0.0);
            let away = sub(p, add(a, times(along, t)));
            let round = self.radius - dot(away, away).sqrt();
            let floor = dot(away, self.up) + self.radius * FLOOR;
            hollow = greater(hollow, lesser(round, floor));
        }
        hollow
    }
}

impl Gathered {
    /// Works out the box that holds it all.
    pub fn bound(&mut self) {
        let boxes = self.rooms.iter().map(|room| room.bounds).chain(self.tunnels.iter().map(|tunnel| tunnel.bounds));
        self.bounds = boxes.reduce(Bounds::with);
    }

    /// How far inside its hollows `p` is, in metres, for cells `size`
    /// metres across: inside what is kept under the ground's roof, then
    /// inside what is not (entrances). `only` picks which rooms and tunnels
    /// are looked at, rooms first, for what has many.
    pub fn inside(&self, net: &Network, p: V3, size: f32, only: Option<&[u16]>) -> (f32, f32) {
        if !self.bounds.is_some_and(|bounds| bounds.holds(p)) {
            return (NOTHING, NOTHING);
        }
        let rough = net.set.rough;
        let plain = self.measured(net, p, None, only);
        // Roughness moves a wall no further than itself: further off than that and a cell, it changes nothing.
        let near = |hollow: f32| hollow.abs() <= rough + size;
        if rough <= 0.0 || !(near(plain.0) || near(plain.1)) {
            return plain;
        }
        let grain = 4.0 + 2.0 * rough;
        let wobble = gradient3(p[0] / grain, p[1] / grain, p[2] / grain, net.set.seed.wrapping_add(11)) * rough;
        self.measured(net, p, Some(wobble), only)
    }

    fn measured(&self, net: &Network, p: V3, wobble: Option<f32>, only: Option<&[u16]>) -> (f32, f32) {
        let (mut roofed, mut open) = (NOTHING, NOTHING);
        let mut room = |room: &Placed| {
            if !room.bounds.holds(p) {
                return;
            }
            let from = sub(p, room.at);
            let own = [dot(from, room.axes[0]), dot(from, room.axes[1]), dot(from, room.axes[2])];
            roofed = greater(roofed, net.set.rooms[room.plan.room].inside(own, room.plan.present, wobble));
        };
        match only {
            Some(only) => only.iter().filter_map(|&at| self.rooms.get(usize::from(at))).for_each(&mut room),
            None => self.rooms.iter().for_each(&mut room),
        }
        let mut tunnel = |tunnel: &Tunnel| {
            if !tunnel.bounds.holds(p) {
                return;
            }
            // A tunnel's walls come in no further than a quarter of its half width.
            let moved = wobble.map_or(0.0, |wobble| greater(wobble, -tunnel.radius * 0.25));
            let hollow = tunnel.inside(p) + moved;
            match tunnel.roofed {
                true => roofed = greater(roofed, hollow),
                false => open = greater(open, hollow),
            }
        };
        let count = self.rooms.len();
        match only {
            Some(only) => only
                .iter()
                .filter_map(|&at| self.tunnels.get(usize::from(at).checked_sub(count)?))
                .for_each(&mut tunnel),
            None => self.tunnels.iter().for_each(&mut tunnel),
        }
        (roofed, open)
    }
}
