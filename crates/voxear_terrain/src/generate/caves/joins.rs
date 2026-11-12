//! Which rooms of an endless network have a tunnel between them.
//!
//! The grid's regions, a room each, are taken sixteen at a time, four by
//! four: a district. Within a district every room is reached from every
//! other: of the regions the seed gives a room, the largest group that
//! touch keep theirs, and tunnels join them as a tree (the least joins
//! that reach them all, picked by numbers from the seed), with some more
//! besides, which make loops. Two districts side by side are joined
//! through one pair of rooms facing each other across their shared side,
//! picked from the pairs there are by a number both work out alike, and
//! through more where there are loops.
//!
//! A district is worked out from its own numbers and, for its sides, from
//! which regions of the four round it keep a room: so any region can ask
//! what it is joined to without anything else having been made.

use super::network::{mix, share};

/// How many regions a district is along each side.
pub(crate) const SIDE: i32 = 4;
const COUNT: usize = (SIDE * SIDE) as usize;

/// The four ways from a region to those beside it, in the order
/// `Grid::beside` gives them.
const WAYS: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

/// How likely the seed gives a region a room.
const ROOMED: f32 = 0.9;

/// What decides an endless network's joins.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Joins {
    pub seed: u32,
    /// How many of the joins a tree leaves out are made after all, 0 to 1.
    pub loops: f32,
}

/// The district a region `(x, z)` is in, and its place among the
/// district's regions.
fn district_of(cell: [i32; 2]) -> ([i32; 2], usize) {
    let district = [cell[0].div_euclid(SIDE), cell[1].div_euclid(SIDE)];
    (district, (cell[1].rem_euclid(SIDE) * SIDE + cell[0].rem_euclid(SIDE)) as usize)
}

/// The region at `at` among `district`'s.
fn cell_of(district: [i32; 2], at: usize) -> [i32; 2] {
    [district[0] * SIDE + at as i32 % SIDE, district[1] * SIDE + at as i32 / SIDE]
}

/// The region beside the one at `at` the way `way`, where it is in the
/// same district.
fn within(at: usize, way: usize) -> Option<usize> {
    let (x, z) = (at as i32 % SIDE + WAYS[way][0], at as i32 / SIDE + WAYS[way][1]);
    ((0..SIDE).contains(&x) && (0..SIDE).contains(&z)).then_some((z * SIDE + x) as usize)
}

impl Joins {
    /// A number for the pair of regions `a` and `b`, the same asked either
    /// way round.
    fn pair(&self, a: [i32; 2], b: [i32; 2], part: i32) -> u32 {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        mix(self.seed, &[low[0], low[1], high[0], high[1], part])
    }

    /// Which of `district`'s regions keep a room: of those the seed gives
    /// one, the largest group that touch, the first where two are as large.
    fn kept(&self, district: [i32; 2]) -> [bool; COUNT] {
        let given: [bool; COUNT] = std::array::from_fn(|at| {
            let [x, z] = cell_of(district, at);
            share(mix(self.seed, &[x, 0, z, 1])) < ROOMED
        });
        let mut group = [usize::MAX; COUNT];
        let mut best = (0, usize::MAX);
        for first in 0..COUNT {
            if !given[first] || group[first] != usize::MAX {
                continue;
            }
            group[first] = first;
            let (mut next, mut size) = (vec![first], 1);
            while let Some(at) = next.pop() {
                for to in (0..4).filter_map(|way| within(at, way)) {
                    if given[to] && group[to] == usize::MAX {
                        group[to] = first;
                        size += 1;
                        next.push(to);
                    }
                }
            }
            if size > best.0 {
                best = (size, first);
            }
        }
        std::array::from_fn(|at| group[at] == best.1 && best.1 != usize::MAX)
    }

    /// Whether the region `(x, z)` has a room, and whether a tunnel joins
    /// it to each of the four beside it: none where it has no room.
    pub fn links(&self, cell: [i32; 2]) -> Option<[bool; 4]> {
        let (district, own) = district_of(cell);
        let kept = self.kept(district);
        if !kept[own] {
            return None;
        }
        // Within the district: the joins between rooms side by side, the least first, each kept where it
        // reaches a room the ones before it do not. That is a tree; some of the rest are loops.
        let mut joins: Vec<(u32, usize, usize)> = Vec::new();
        for at in (0..COUNT).filter(|&at| kept[at]) {
            for to in [0, 2].into_iter().filter_map(|way| within(at, way)).filter(|&to| kept[to]) {
                joins.push((self.pair(cell_of(district, at), cell_of(district, to), 2), at, to));
            }
        }
        joins.sort_unstable();
        let mut root: [usize; COUNT] = std::array::from_fn(|at| at);
        let find = |root: &[usize; COUNT], mut at: usize| {
            while root[at] != at {
                at = root[at];
            }
            at
        };
        let mut links = [false; 4];
        let way_to = |to: usize| (0..4).find(|&way| within(own, way) == Some(to));
        for (_, at, to) in joins {
            let (a, b) = (find(&root, at), find(&root, to));
            let tree = a != b;
            if tree {
                root[a] = b;
            }
            let mine = if at == own { way_to(to) } else if to == own { way_to(at) } else { None };
            if let Some(way) = mine {
                let looped = share(self.pair(cell_of(district, at), cell_of(district, to), 3)) < self.loops;
                links[way] = tree || looped;
            }
        }
        // Across each side the region lies on: the one pair both districts pick, and some of the others.
        for way in (0..4).filter(|&way| within(own, way).is_none()) {
            let next = [district[0] + WAYS[way][0], district[1] + WAYS[way][1]];
            let theirs = self.kept(next);
            // The regions along the shared side, by how far along it they lie, and the one facing each.
            let facing = |along: i32| {
                let (mine, far) = match way {
                    0 => ([SIDE - 1, along], [0, along]),
                    1 => ([0, along], [SIDE - 1, along]),
                    2 => ([along, SIDE - 1], [along, 0]),
                    _ => ([along, 0], [along, SIDE - 1]),
                };
                ((mine[1] * SIDE + mine[0]) as usize, (far[1] * SIDE + far[0]) as usize)
            };
            let pairs: Vec<i32> = (0..SIDE)
                .filter(|&along| {
                    let (mine, far) = facing(along);
                    kept[mine] && theirs[far]
                })
                .collect();
            let Some(along) = (0..SIDE).find(|&along| facing(along).0 == own) else { continue };
            if pairs.is_empty() || !pairs.contains(&along) {
                continue;
            }
            let picked = pairs[self.pair(district, next, 4) as usize % pairs.len()];
            let (mine, far) = facing(along);
            let looped = share(self.pair(cell_of(district, mine), cell_of(next, far), 3)) < self.loops * 0.5;
            links[way] = picked == along || looped;
        }
        Some(links)
    }
}

/// The way back from each of the four ways.
pub(crate) const BACK: [usize; 4] = [1, 0, 3, 2];

impl Joins {
    /// Whether a ramp leads down from the room of region `(x, z)` in the
    /// layer these joins are of to the room beside it, each of the four
    /// ways, in the layer under it, whose joins are `under`. In every
    /// district one such pair of rooms is picked from those there are, by
    /// a number of the district, so the two layers are one cave; and some
    /// of the others, where there are loops.
    pub fn ramps(&self, under: &Joins, cell: [i32; 2]) -> [bool; 4] {
        let (district, own) = district_of(cell);
        let (upper, lower) = (self.kept(district), under.kept(district));
        let mut ramps = [false; 4];
        if !upper[own] {
            return ramps;
        }
        // Every room with a room one layer down and one region along, in the same district.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for at in (0..COUNT).filter(|&at| upper[at]) {
            for way in 0..4 {
                if within(at, way).is_some_and(|to| lower[to]) {
                    pairs.push((at, way));
                }
            }
        }
        if pairs.is_empty() {
            return ramps;
        }
        let picked = pairs[mix(self.seed, &[district[0], district[1], 5]) as usize % pairs.len()];
        for way in 0..4 {
            let Some(to) = within(own, way).filter(|&to| lower[to]) else { continue };
            let more = share(self.pair(cell, cell_of(district, to), 6)) < self.loops * 0.3;
            ramps[way] = picked == (own, way) || more;
        }
        ramps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many rooms the regions from `low`, `wide` along each side, have, and how many of them are
    /// reached from the first through the joins.
    fn reached(joins: &Joins, low: [i32; 2], wide: i32) -> (usize, usize, usize) {
        let inside = |cell: [i32; 2]| (0..2).all(|axis| (low[axis]..low[axis] + wide).contains(&cell[axis]));
        let all = (0..wide * wide).map(|at| [low[0] + at % wide, low[1] + at / wide]);
        let cells: Vec<[i32; 2]> = all.filter(|&cell| joins.links(cell).is_some()).collect();
        let mut seen = vec![cells[0]];
        let (mut next, mut tunnels) = (vec![cells[0]], 0);
        while let Some(cell) = next.pop() {
            let links = joins.links(cell).expect("a room");
            for way in (0..4).filter(|&way| links[way]) {
                let to = [cell[0] + WAYS[way][0], cell[1] + WAYS[way][1]];
                // Both ends know of every tunnel.
                let back = [1, 0, 3, 2][way];
                assert!(joins.links(to).is_some_and(|links| links[back]), "{cell:?} to {to:?}");
                tunnels += 1;
                if inside(to) && !seen.contains(&to) {
                    seen.push(to);
                    next.push(to);
                }
            }
        }
        (cells.len(), seen.len(), tunnels)
    }

    #[test]
    fn every_room_of_a_district_is_reached_and_districts_join_where_both_agree() {
        for seed in 0..40 {
            let joins = Joins { seed, loops: 0.0 };
            // One district: a tree, a join fewer than its rooms inside it.
            let (rooms, seen, _) = reached(&joins, [-8, 4], SIDE);
            assert_eq!(rooms, seen, "seed {seed}");
            assert!(rooms >= 8, "most regions keep their room: {rooms}");
            // Sixteen districts: all one cave.
            let (rooms, seen, _) = reached(&joins, [-8, -8], SIDE * 4);
            assert_eq!(rooms, seen, "seed {seed}: districts apart");
        }
        // Two layers: in every district a ramp or more leads down, to a room there is.
        for seed in 0..40 {
            let (upper, lower) = (Joins { seed, loops: 0.0 }, Joins { seed: seed + 1000, loops: 0.0 });
            let mut down = 0;
            for at in 0..SIDE * SIDE {
                let cell = [4 + at % SIDE, -8 + at / SIDE];
                for way in (0..4).filter(|&way| upper.ramps(&lower, cell)[way]) {
                    down += 1;
                    assert!(upper.links(cell).is_some(), "a ramp from a room");
                    assert!(lower.links([cell[0] + WAYS[way][0], cell[1] + WAYS[way][1]]).is_some(), "to a room");
                }
            }
            assert_eq!(down, 1, "seed {seed}: one ramp a district with no loops");
        }
        // Loops add tunnels and take no room away.
        let (tree, looped) = (Joins { seed: 3, loops: 0.0 }, Joins { seed: 3, loops: 0.6 });
        let (a, b) = (reached(&tree, [0, 0], SIDE * 3), reached(&looped, [0, 0], SIDE * 3));
        assert!(a.0 == b.0 && b.2 > a.2 + 10, "{a:?} {b:?}");
    }
}
