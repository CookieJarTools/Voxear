//! A cave level: a bounded cave laid out once from the seed. A main path
//! of rooms walks from region to region of a flat grid without crossing
//! itself, side rooms branch off it, and a tunnel joins each room to the
//! one it was reached from. So every room can be walked to from the first.
//!
//! It is laid out in a space of its own, its first room's floor at no
//! height; where the ground puts that floor is found when ground is made
//! (`Network::level_height`).

use super::{
    grid::{Grid, Region, V3},
    network::{Bounds, Gathered, Network, mix, share},
    rooms::NOTHING,
};

/// How much higher or lower than the room it was reached from a room may
/// lie, of the grid's spacing; and how far above or below the first room
/// any may, of the level's depth, so none comes near the ground's top.
const CLIMB: f32 = 0.12;
const DRIFT: f32 = 0.5;

/// The four ways from a region to those beside it, in the order
/// `Grid::beside` gives them.
const WAYS: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

/// A level laid out.
#[derive(Clone, Debug)]
pub(crate) struct Level {
    /// Its rooms, the main path's first and in order, then the side rooms,
    /// and its tunnels.
    pub all: Gathered,
    /// The foot of its first room: where the ground it lies under is asked.
    pub first: V3,
    /// How many of its rooms are the main path.
    pub main: usize,
    spacing: f32,
    /// The regions its lists cover, from `low`, `wide` along x by `deep`
    /// along z, and for each the rooms and tunnels that reach into it:
    /// rooms by their place, tunnels by theirs past the rooms.
    low: [i32; 2],
    wide: usize,
    deep: usize,
    lists: Vec<Vec<u16>>,
}

impl Level {
    /// The level `net`'s values lay out.
    pub fn laid(net: &Network) -> Level {
        let set = &net.set;
        let grid = Grid { spacing: set.spacing };
        let roll = |step: i32, part: i32| mix(set.seed, &[step, part, 31]);
        // The walk: each step to a free region beside the last, keeping on the way it was going more often
        // than not, and not into a region with no way on while there are rooms to come.
        let mut cells: Vec<[i32; 2]> = vec![[0, 0]];
        let mut from: Vec<Option<usize>> = vec![None];
        let mut heights = vec![0.0f32];
        let drift = set.depth * DRIFT;
        let risen =
            |from: f32, roll: u32| (from + (share(roll) * 2.0 - 1.0) * CLIMB * set.spacing).clamp(-drift, drift);
        let taken = |cells: &[[i32; 2]], cell: [i32; 2]| cells.contains(&cell);
        let step = |cell: [i32; 2], way: usize| [cell[0] + WAYS[way][0], cell[1] + WAYS[way][1]];
        let mut heading = (roll(0, 0) % 4) as usize;
        let main = set.main.clamp(1, 64) as usize;
        for at in 1..main {
            let last = cells[at - 1];
            let weight = |way: usize| {
                let next = step(last, way);
                if taken(&cells, next) {
                    return 0;
                }
                let open = (0..4).any(|on| !taken(&cells, step(next, on)));
                match (open || at + 1 == main, way == heading) {
                    (false, _) => 0,
                    (true, true) => 3,
                    (true, false) => 1,
                }
            };
            let total: u32 = (0..4).map(weight).sum();
            if total == 0 {
                break;
            }
            let mut left = roll(at as i32, 1) % total;
            let way = (0..4).find(|&way| {
                let weight = weight(way);
                let found = left < weight;
                left = left.saturating_sub(weight);
                found
            });
            let way = way.unwrap_or(heading);
            heading = way;
            cells.push(step(last, way));
            from.push(Some(at - 1));
            heights.push(risen(heights[at - 1], roll(at as i32, 2)));
        }
        let main = cells.len();
        // Side rooms: each off a room there is, a few tries at one with a free region beside it.
        for side in 0..set.side.min(64) as i32 {
            for attempt in 0..8 {
                let off = (roll(1000 + side * 8 + attempt, 3) % cells.len() as u32) as usize;
                let next = step(cells[off], (roll(1000 + side * 8 + attempt, 4) % 4) as usize);
                if taken(&cells, next) {
                    continue;
                }
                cells.push(next);
                from.push(Some(off));
                heights.push(risen(heights[off], roll(1000 + side * 8 + attempt, 5)));
                break;
            }
        }
        // Each room joined to the one it was reached from and to those reached from it.
        let region = |cell: [i32; 2]| -> Region { [cell[0], 0, cell[1]] };
        // And, for loops, to some of the rooms beside it that it was not.
        let looped = |a: usize, b: usize| {
            let (low, high) = (a.min(b) as i32, a.max(b) as i32);
            share(mix(set.seed, &[low, high, 33])) < set.loops
        };
        let joined = |a: usize, b: usize| from[a] == Some(b) || from[b] == Some(a) || looped(a, b);
        let loose = (1.0 - 2.0 * (set.reach() + set.stray()) / set.spacing).clamp(0.0, 0.6);
        let mut all = Gathered::default();
        for (at, &cell) in cells.iter().enumerate() {
            let links = [0, 1, 2, 3].map(|way| {
                let next = step(cell, way);
                cells.iter().position(|&other| other == next).is_some_and(|other| joined(at, other))
            });
            let part = |part: i32| share(roll(at as i32, 10 + part));
            let jitter = [(part(0) * 2.0 - 1.0) * loose, (part(1) * 2.0 - 1.0) * loose];
            let plan = net.planned(&grid, region(cell), set.room(part(2)), jitter, links, 0.5, false);
            all.rooms.push(net.placed(&grid, plan, heights[at], 0.0));
        }
        for at in 1..all.rooms.len() {
            let Some(off) = from[at] else { continue };
            let (a, b) = (&all.rooms[off], &all.rooms[at]);
            let (a, b) = if a.plan.region <= b.plan.region { (a, b) } else { (b, a) };
            let tunnel = net.tunnel(a, b);
            all.tunnels.extend(tunnel);
        }
        for at in 0..all.rooms.len() {
            for way in [0, 2] {
                let next = step(cells[at], way);
                let Some(other) = cells.iter().position(|&cell| cell == next) else { continue };
                if from[at] == Some(other) || from[other] == Some(at) || !looped(at, other) {
                    continue;
                }
                let (a, b) = (&all.rooms[at], &all.rooms[other]);
                let (a, b) = if a.plan.region <= b.plan.region { (a, b) } else { (b, a) };
                let tunnel = net.tunnel(a, b);
                all.tunnels.extend(tunnel);
            }
        }
        all.bound();
        let first = all.rooms[0].plan.stand.foot;
        // For each region round the level, what reaches into it.
        let (mut low, mut high) = ([i32::MAX; 2], [i32::MIN; 2]);
        for cell in &cells {
            for axis in 0..2 {
                (low[axis], high[axis]) = (low[axis].min(cell[axis] - 1), high[axis].max(cell[axis] + 1));
            }
        }
        let (wide, deep) = ((high[0] - low[0] + 1) as usize, (high[1] - low[1] + 1) as usize);
        let boxes: Vec<Bounds> =
            all.rooms.iter().map(|room| room.bounds).chain(all.tunnels.iter().map(|t| t.bounds)).collect();
        let mut lists = Vec::with_capacity(wide * deep);
        for z in 0..deep {
            for x in 0..wide {
                let (x0, z0) = ((low[0] + x as i32) as f32 * set.spacing, (low[1] + z as i32) as f32 * set.spacing);
                let reaches = |bounds: &Bounds| {
                    bounds.high[0] >= x0
                        && bounds.low[0] <= x0 + set.spacing
                        && bounds.high[2] >= z0
                        && bounds.low[2] <= z0 + set.spacing
                };
                lists.push((0..boxes.len()).filter(|&at| reaches(&boxes[at])).map(|at| at as u16).collect());
            }
        }
        Level { all, first, main, spacing: set.spacing, low, wide, deep, lists }
    }

    /// How far inside its hollows `p` is, a place in the level's own
    /// space, for cells `size` metres across (see [`Gathered::inside`]).
    pub fn inside(&self, net: &Network, p: V3, size: f32) -> (f32, f32) {
        let cell = |v: f32, low: i32| (v / self.spacing).floor() as i64 - i64::from(low);
        let (x, z) = (cell(p[0], self.low[0]), cell(p[2], self.low[1]));
        if x < 0 || z < 0 || x >= self.wide as i64 || z >= self.deep as i64 {
            return (NOTHING, NOTHING);
        }
        let list = &self.lists[z as usize * self.wide + x as usize];
        if list.is_empty() {
            return (NOTHING, NOTHING);
        }
        self.all.inside(net, p, size, Some(list))
    }
}
