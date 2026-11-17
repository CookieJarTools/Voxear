//! Regions: a chunk's places gathered into groups that a walker can get
//! about within without leaving the chunk, and the coarse graph of edges
//! between them.
//!
//! Two places are in one region when a walker steps from one to the other
//! and back, a step at a time, inside the chunk: so every place of a region
//! is reached from every other. A step that goes one way only (a drop too
//! high to climb back) joins two regions with an edge that way; so does a
//! step out of the chunk into a region of the chunk beside it. Each edge
//! keeps where it is crossed, the middle of the steps that make it, so a
//! search over regions measures a way by where it would really go.

use std::collections::HashMap;

use voxear_terrain::blocks::chunk::CHUNK;

use super::{BlockNav, NavChunk, OneChunk, PlaceRef, around_index, step, walking::Walker};

/// A place a walker does not fit in: in no region of its.
pub const NO_REGION: u16 = u16::MAX;

/// The four ways along the axes, as `(dx, dz)`.
pub const AXES: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

/// One region: where its middle is, in blocks from the world's corner, how
/// many places it has, and the areas they are, a bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Region {
    pub middle: [f32; 3],
    pub places: u16,
    pub areas: u32,
}

/// An edge from a region: to region `region` of the chunk at around index
/// `to` (see [`around_index`]; 13 for its own chunk), crossed at
/// `crossing`, in blocks from the world's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub to: u8,
    pub region: u16,
    pub crossing: [f32; 3],
}

/// Edges, gathered by the region they leave.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edges {
    start: Box<[u32]>,
    list: Box<[Edge]>,
}

impl Edges {
    /// The edges from region `region`.
    #[inline]
    pub fn from(&self, region: u16) -> &[Edge] {
        let at = usize::from(region);
        match self.start.get(at..at + 2) {
            Some(&[from, to]) => &self.list[from as usize..to as usize],
            _ => &[],
        }
    }

    /// How many edges there are.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Edges of `regions` regions from steps each `(from, to around index,
    /// to region, where)`: the steps between the same two regions made one
    /// edge, crossed at their middle.
    fn gather(regions: usize, steps: impl IntoIterator<Item = (u16, u8, u16, [f32; 3])>) -> Self {
        let mut joined: HashMap<(u16, u8, u16), ([f32; 3], u32)> = HashMap::new();
        for (from, to, region, at) in steps {
            let (sum, count) = joined.entry((from, to, region)).or_insert(([0.0; 3], 0));
            for axis in 0..3 {
                sum[axis] += at[axis];
            }
            *count += 1;
        }
        let mut edges: Vec<(u16, Edge)> = joined
            .into_iter()
            .map(|((from, to, region), (sum, count))| {
                (from, Edge { to, region, crossing: sum.map(|v| v / count as f32) })
            })
            .collect();
        edges.sort_by_key(|(from, edge)| (*from, edge.to, edge.region));
        let mut start = vec![0u32; regions + 1];
        for (from, _) in &edges {
            start[usize::from(*from) + 1] += 1;
        }
        for at in 1..start.len() {
            start[at] += start[at - 1];
        }
        Self { start: start.into_boxed_slice(), list: edges.into_iter().map(|(_, edge)| edge).collect() }
    }
}

/// A chunk's regions for one walker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Regions {
    /// Each place's region, [`NO_REGION`] where the walker does not fit.
    of: Box<[u16]>,
    pub regions: Box<[Region]>,
    /// Edges to other regions of the same chunk, those a step joins one way
    /// only.
    pub inner: Edges,
}

impl Regions {
    /// The region of place `index`.
    #[inline]
    pub fn of(&self, index: u16) -> u16 {
        self.of.get(usize::from(index)).copied().unwrap_or(NO_REGION)
    }

    /// How many regions there are.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// The regions of `chunk`, chunk `key`, for `walker`.
    pub fn of_chunk(key: [i32; 3], chunk: &NavChunk, walker: Walker) -> Self {
        let ground = OneChunk { chunk, slot: 0, cy: key[1] };
        let side = CHUNK as i32;
        let corner = [key[0] * side, key[1] * side, key[2] * side];
        let mut of = vec![NO_REGION; chunk.len()];
        let mut regions: Vec<Region> = Vec::new();
        let mut one_way: Vec<(PlaceRef, PlaceRef)> = Vec::new();
        let mut queue: Vec<PlaceRef> = Vec::new();
        let fits = |index: u16| {
            let place = chunk.place(index);
            place.top(key[1]) - place.feet(key[1]) >= i32::from(walker.height)
        };
        let point = |at: PlaceRef| {
            let place = chunk.place(at.index);
            [
                (corner[0] + i32::from(at.x)) as f32 + 0.5,
                place.feet(key[1]) as f32 / 16.0,
                (corner[2] + i32::from(at.z)) as f32 + 0.5,
            ]
        };
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                let first = chunk.first_of(x, z);
                for n in 0..chunk.column(x, z).len() {
                    let index = first + n as u16;
                    if of[usize::from(index)] != NO_REGION || !fits(index) {
                        continue;
                    }
                    let region = regions.len() as u16;
                    if region == NO_REGION {
                        break;
                    }
                    let seed = PlaceRef { slot: 0, index, x: x as u8, z: z as u8 };
                    of[usize::from(index)] = region;
                    queue.push(seed);
                    let mut gathered = Region::default();
                    let mut sum = [0.0f64; 3];
                    while let Some(at) = queue.pop() {
                        let here = point(at);
                        for axis in 0..3 {
                            sum[axis] += f64::from(here[axis]);
                        }
                        gathered.places = gathered.places.saturating_add(1);
                        gathered.areas |= 1 << (chunk.place(at.index).area & 31);
                        for [dx, dz] in AXES {
                            let Some((to, _)) = step(&ground, at, dx, dz, walker) else { continue };
                            let back = step(&ground, to, -dx, -dz, walker).map(|(back, _)| back);
                            if back != Some(at) {
                                one_way.push((at, to));
                                continue;
                            }
                            if of[usize::from(to.index)] == NO_REGION {
                                of[usize::from(to.index)] = region;
                                queue.push(to);
                            }
                        }
                    }
                    let count = f64::from(gathered.places.max(1));
                    gathered.middle = sum.map(|v| (v / count) as f32);
                    regions.push(gathered);
                }
            }
        }
        let here = around_index([0, 0, 0]) as u8;
        let inner = one_way.into_iter().filter_map(|(from, to)| {
            let (a, b) = (of[usize::from(from.index)], of[usize::from(to.index)]);
            (a != b && b != NO_REGION).then(|| (a, here, b, middle(point(from), point(to))))
        });
        let inner = Edges::gather(regions.len(), inner);
        Self { of: of.into_boxed_slice(), regions: regions.into_boxed_slice(), inner }
    }
}

/// The point half way between two.
fn middle(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

/// The edges from chunk `slot`'s regions for walker `class` to those of the
/// chunks round it: a step out of it from any place near its sides, top or
/// bottom, into a region of another.
pub fn outer_edges(nav: &BlockNav, slot: u32, class: usize) -> Edges {
    let Some(own) = nav.slot(slot) else { return Edges::default() };
    let Some(regions) = own.regions.get(class) else { return Edges::default() };
    let walker = nav.walkers()[class];
    let (down, up) = walker.reach();
    let last = CHUNK as i32 - 1;
    let chunk = &own.chunk;
    let mut steps = Vec::new();
    for z in 0..CHUNK {
        for x in 0..CHUNK {
            let side = x == 0 || z == 0 || x == CHUNK - 1 || z == CHUNK - 1;
            let first = chunk.first_of(x, z);
            for (n, place) in chunk.column(x, z).iter().enumerate() {
                let index = first + n as u16;
                let region = regions.of(index);
                let y = i32::from(place.y);
                if region == NO_REGION || !side && y >= down && y <= last - up {
                    continue;
                }
                let at = PlaceRef { slot, index, x: x as u8, z: z as u8 };
                for [dx, dz] in AXES {
                    let Some((to, _)) = step(nav, at, dx, dz, walker) else { continue };
                    if to.slot == slot {
                        continue;
                    }
                    let to_region = nav.region_of(to, class);
                    let (Some(other), Some(from_point), Some(to_point)) =
                        (nav.slot(to.slot), nav.point(at), nav.point(to))
                    else {
                        continue;
                    };
                    if to_region == NO_REGION {
                        continue;
                    }
                    let d = [0, 1, 2].map(|axis| other.key[axis] - own.key[axis]);
                    steps.push((region, around_index(d) as u8, to_region, middle(from_point, to_point)));
                }
            }
        }
    }
    Edges::gather(regions.len(), steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{
        places::ColumnBuilder,
        tests::{FACES, STONE, floor},
        walking::WalkTable,
    };

    #[test]
    fn a_floor_is_one_region_a_wall_across_it_makes_two_and_a_drop_joins_them_one_way() {
        let table = WalkTable::of(&FACES);
        let walker = Walker::blocks(2);
        let regions =
            |blocks| Regions::of_chunk([0, 0, 0], &ColumnBuilder::new(&blocks, &table).build([0, 0, 0]), walker);
        let open = regions(floor());
        assert_eq!(open.len(), 1);
        assert_eq!(open.regions[0].places, 256);

        let mut walled = floor();
        for z in 0..16 {
            blocks_up(&mut walled, [8, 1, z], 3);
        }
        assert_eq!(regions(walled).len(), 3, "either side, and the wall's top");

        // A shelf three blocks up along one side: dropped from, never climbed.
        let mut shelf = floor();
        for z in 0..16 {
            for x in 0..4 {
                blocks_up(&mut shelf, [x, 1, z], 3);
            }
        }
        let shelf = regions(shelf);
        assert_eq!(shelf.len(), 2);
        let high = shelf.regions.iter().position(|region| region.middle[1] > 3.0).unwrap() as u16;
        let edges = shelf.inner.from(high);
        assert_eq!(edges.len(), 1, "down off the shelf");
        assert!(shelf.inner.from(1 - high).is_empty(), "never up");
    }

    fn blocks_up(blocks: &mut voxear_terrain::blocks::BlockMap, at: [i32; 3], high: i32) {
        for up in 0..high {
            blocks.set([at[0], at[1] + up, at[2]], STONE);
        }
    }
}
