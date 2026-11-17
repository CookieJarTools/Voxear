//! Finding a way over a world's places.
//!
//! A way between places of one region is found place by place. A longer
//! one is found first over the regions ([`coarse`]), each entered where
//! the edge to it is crossed, and then place by place inside the regions
//! that coarse way went through, so the search over places looks at little
//! more than the way itself. A way much longer than [`PathAsk::refine`]
//! chunks is found place by place only for its first part; what walks it
//! asks again as it nears the end.
//!
//! Places are stepped between eight ways ([`step`](super::step)): a step
//! along an axis costs a block, one cornerwise √2, each times the cost of
//! the area stepped into, and half a block more for each block up or down.
//! The way found is then pulled tight ([`tight`](super::tight)) into a few
//! corners to walk straight between.

use std::{cell::RefCell, cmp::Reverse, collections::BinaryHeap};

use voxear_terrain::coord_hash::CoordMap;

use super::{BlockNav, PlaceKind, PlaceRef, around_index, jump_step, straight_step, tight, walking::Walker};
use crate::AreaCosts;

/// √2: how much further a cornerwise step goes than one along an axis.
const DIAGONAL: f32 = std::f32::consts::SQRT_2;
/// What a block of climbing or dropping adds to a step.
const UPWARD: f32 = 0.5;
/// What a jump adds to the blocks it crosses.
const JUMPING: f32 = 1.5;
/// The most regions a coarse search looks at.
const MOST_REGIONS: usize = 60_000;
/// How much a search over places in many regions leans towards the goal,
/// by default (see [`PathAsk::greed`]): over rolling land a way of 500
/// blocks costs some 2% more than the cheapest, and is found looking at a
/// tenth as many places.
pub const GREED: f32 = 1.25;

/// A way asked for.
#[derive(Clone, Debug)]
pub struct PathAsk {
    /// From where, and to where, in blocks from the world's corner: each
    /// near a place to stand, or the way begins or ends at the place
    /// nearest it.
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub walker: Walker,
    pub costs: AreaCosts,
    /// How many chunks of a long way are found place by place: the rest is
    /// known only to be there. `None` for all of it.
    pub refine: Option<u32>,
    /// The most places the search may look at.
    pub most: usize,
    /// How far from `to`, across, a place to end at may be, in blocks;
    /// and up or down.
    pub across: f32,
    pub reach: f32,
    /// Boxes giving the places in them another area, as Nav Areas do.
    pub boxes: Vec<crate::AreaBox>,
    /// How much a search over many regions leans towards the goal: 1 finds
    /// the cheapest way there is; more finds one sooner that may cost a
    /// little more, never more than this many times the cheapest.
    pub greed: f32,
}

impl PathAsk {
    pub fn new(from: [f32; 3], to: [f32; 3], walker: Walker) -> Self {
        Self {
            from,
            to,
            walker,
            costs: AreaCosts::default(),
            refine: None,
            most: 400_000,
            across: 3.0,
            reach: 4.0,
            boxes: Vec::new(),
            greed: GREED,
        }
    }
}

/// Whether a way was found, and how much of one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathStatus {
    /// None: there is no place to start from.
    #[default]
    None,
    /// All the way to the place nearest the goal.
    Complete,
    /// As near the goal as there is a way to: the goal cannot be reached,
    /// or not within what the search may look at.
    Partial,
    /// The first part of a way that goes on to the goal: ask again near its
    /// end for the rest.
    Continues,
}

/// A way found.
#[derive(Clone, Debug, Default)]
pub struct FoundPath {
    pub status: PathStatus,
    /// The points to walk straight between, the first where it begins, in
    /// blocks from the world's corner, on the floor.
    pub corners: Vec<[f32; 3]>,
    /// Every place along it, in turn.
    pub places: Vec<PlaceRef>,
    /// What it costs, by the costs it was found with.
    pub cost: f32,
    /// The chunks it goes through, so a change to one can be noticed.
    pub chunks: Vec<[i32; 3]>,
    /// The version of the navigation it was found in.
    pub version: u64,
    /// How many places and regions were looked at.
    pub looked: usize,
    /// For each corner, whether it is jumped to from the one before.
    pub jumps: Vec<bool>,
    /// The shut doors along it, each the block of its lower half, in turn.
    pub doors: Vec<[i32; 3]>,
}

impl FoundPath {
    /// Whether any chunk it goes through has changed since it was found.
    pub fn is_stale(&self, nav: &BlockNav) -> bool {
        nav.version() != self.version
            && self.chunks.iter().any(|key| {
                nav.slot_of(*key).and_then(|slot| nav.slot(slot)).is_none_or(|slot| slot.changed > self.version)
            })
    }
}

/// A way over `nav`'s places, as `ask` asks.
pub fn find_path(nav: &BlockNav, ask: &PathAsk) -> FoundPath {
    let walker = ask.walker;
    let none = FoundPath { version: nav.version(), ..FoundPath::default() };
    let Some(start) = nav.nearest(ask.from, walker, 1.0, 2.0) else { return none };
    let goal = nav.nearest(ask.to, walker, ask.across, ask.reach);
    let class = nav.class_of(walker);
    let target = goal.and_then(|goal| nav.point(goal)).unwrap_or(ask.to);
    // A coarse way first, unless the two are in one region.
    let mut corridor = None;
    let mut looked = 0;
    // Whether the way is found only part of the way, and whether the rest is there.
    let (mut short, mut goes_on) = (false, false);
    if let Some(class) = class {
        let same = goal
            .is_some_and(|goal| goal.slot == start.slot && nav.region_of(goal, class) == nav.region_of(start, class));
        if !same {
            let found =
                coarse(nav, class, start, goal.map(|goal| (goal.slot, nav.region_of(goal, class))), target, ask);
            looked += found.looked;
            let mut nodes = found.nodes;
            // A long way is found place by place for its first part.
            let cut = ask.refine.and_then(|chunks| {
                let longest = chunks.max(1) as f32 * 32.0;
                found.along.iter().position(|far| *far > longest)
            });
            if let Some(cut) = cut {
                nodes.truncate(cut.max(1));
                (short, goes_on) = (true, found.reached);
            }
            // Where the way leaves the last region kept: the crossing into the next.
            let next = found.crossings.get(nodes.len() - 1).copied();
            corridor = Some(Corridor { nodes, ends_short: short, next });
        }
    }
    let mut found = SCRATCH
        .with(|scratch| fine(&mut scratch.borrow_mut(), nav, class, start, goal, target, corridor.as_ref(), ask));
    looked += found.looked;
    // The regions are a walker's that does everything; one that does less may find no way through them, so it
    // looks again without them, as far as it may.
    if !found.reached && goal.is_some() && corridor.as_ref().is_some_and(|corridor| !corridor.ends_short) {
        let free = SCRATCH.with(|scratch| fine(&mut scratch.borrow_mut(), nav, None, start, goal, target, None, ask));
        looked += free.looked;
        if free.reached {
            found = free;
        }
    }
    let fine = found;
    let status = match (fine.reached, short, fine.places.is_empty()) {
        (_, _, true) => PathStatus::None,
        (true, true, _) if goes_on => PathStatus::Continues,
        (true, false, _) if goal.is_some() => PathStatus::Complete,
        _ => PathStatus::Partial,
    };
    let kept = tight::pull(nav, &fine.places, walker, &|at| cost_of(nav, ask, at));
    let corners = kept.iter().filter_map(|&at| nav.point(fine.places[at])).collect();
    // A corner is jumped to where the place before it is more than a column away.
    let apart = |a: PlaceRef, b: PlaceRef| match (nav.cell(a), nav.cell(b)) {
        (Some(a), Some(b)) => (a[0] - b[0]).abs().max((a[2] - b[2]).abs()) > 1,
        _ => false,
    };
    let jumps = kept
        .iter()
        .enumerate()
        .map(|(at, &place)| at > 0 && kept[at - 1] + 1 == place && apart(fine.places[place - 1], fine.places[place]))
        .collect();
    let doors = fine
        .places
        .iter()
        .filter(|place| nav.place(**place).is_some_and(|(place, _)| place.kind == PlaceKind::Door))
        .filter_map(|place| nav.cell(*place))
        .collect();
    let mut chunks: Vec<[i32; 3]> = Vec::new();
    for place in &fine.places {
        if let Some(slot) = nav.slot(place.slot)
            && chunks.last() != Some(&slot.key)
            && !chunks.contains(&slot.key)
        {
            chunks.push(slot.key);
        }
    }
    FoundPath {
        status,
        corners,
        places: fine.places,
        cost: fine.cost,
        chunks,
        version: nav.version(),
        looked,
        jumps,
        doors,
    }
}

/// The regions a coarse way goes through, which a search over places keeps
/// to.
struct Corridor {
    /// Each region, as its chunk's number and its own.
    nodes: Vec<(u32, u16)>,
    /// Whether it stops short of the goal, so the search over places ends on
    /// reaching its last region.
    ends_short: bool,
    /// Where the coarse way leaves its last region, to head for there.
    next: Option<[f32; 3]>,
}

/// What a coarse search found.
struct Coarse {
    nodes: Vec<(u32, u16)>,
    /// Where each edge along it is crossed, and how far along the way each
    /// region is entered.
    crossings: Vec<[f32; 3]>,
    along: Vec<f32>,
    reached: bool,
    looked: usize,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (x * x + z * z).sqrt() + y.abs() * UPWARD
}

/// The least a region's areas cost to cross, by `costs`; infinite where it
/// has none that may be entered.
fn region_cost(areas: u32, costs: &AreaCosts) -> f32 {
    let mut least = f32::INFINITY;
    let mut bits = areas;
    while bits != 0 {
        let area = bits.trailing_zeros();
        least = least.min(costs.of(area as u8));
        bits &= bits - 1;
    }
    least
}

/// A way over regions from `start`'s to `goal` (a chunk and region), or
/// towards `target` where the goal is unknown: as near it as there is a way
/// to where it cannot be reached.
fn coarse(
    nav: &BlockNav,
    class: usize,
    start: PlaceRef,
    goal: Option<(u32, u16)>,
    target: [f32; 3],
    ask: &PathAsk,
) -> Coarse {
    struct Node {
        g: f32,
        parent: u32,
        slot: u32,
        region: u16,
        entry: [f32; 3],
        closed: bool,
    }
    let least = ask.costs.least();
    let mut nodes: Vec<Node> = Vec::new();
    let mut found: CoordMap<u64, u32> = CoordMap::default();
    let mut open: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    let begin = (start.slot, nav.region_of(start, class));
    let entry = nav.point(start).unwrap_or(ask.from);
    let key = |slot: u32, region: u16| u64::from(slot) << 16 | u64::from(region);
    nodes.push(Node { g: 0.0, parent: u32::MAX, slot: begin.0, region: begin.1, entry, closed: false });
    found.insert(key(begin.0, begin.1), 0);
    open.push(Reverse((distance(entry, target).to_bits(), 0)));
    let mut best = (distance(entry, target), 0u32);
    let mut reached = None;
    let mut looked = 0;
    while let Some(Reverse((_, at))) = open.pop() {
        let node = &nodes[at as usize];
        if node.closed {
            continue;
        }
        let (g, slot_at, region, entry) = (node.g, node.slot, node.region, node.entry);
        nodes[at as usize].closed = true;
        if Some((slot_at, region)) == goal {
            reached = Some(at);
            break;
        }
        looked += 1;
        if looked > MOST_REGIONS {
            break;
        }
        let Some(slot) = nav.slot(slot_at) else { continue };
        let Some(regions) = slot.regions.get(class) else { continue };
        let here = regions.regions.get(usize::from(region)).map_or(1.0, |region| region_cost(region.areas, &ask.costs));
        let outer = slot.outer.get(class);
        let edges = regions.inner.from(region).iter().chain(outer.map_or(&[][..], |outer| outer.from(region)));
        for edge in edges {
            let to_slot = match usize::from(edge.to) == around_index([0, 0, 0]) {
                true => slot_at,
                false => slot.around[usize::from(edge.to)],
            };
            let Some(other) = nav.slot(to_slot) else { continue };
            let Some(to_region) =
                other.regions.get(class).and_then(|regions| regions.regions.get(usize::from(edge.region)))
            else {
                continue;
            };
            if !region_cost(to_region.areas, &ask.costs).is_finite() {
                continue;
            }
            let cost = g + distance(entry, edge.crossing) * here.max(least);
            let key = key(to_slot, edge.region);
            let index = match found.get(&key) {
                Some(&index) if nodes[index as usize].closed || nodes[index as usize].g <= cost => continue,
                Some(&index) => index,
                None => {
                    nodes.push(Node {
                        g: f32::INFINITY,
                        parent: 0,
                        slot: to_slot,
                        region: edge.region,
                        entry: edge.crossing,
                        closed: false,
                    });
                    found.insert(key, nodes.len() as u32 - 1);
                    nodes.len() as u32 - 1
                }
            };
            let node = &mut nodes[index as usize];
            (node.g, node.parent, node.entry) = (cost, at, edge.crossing);
            let ahead = distance(edge.crossing, target) * least;
            if ahead < best.0 {
                best = (ahead, index);
            }
            open.push(Reverse(((cost + ahead).to_bits(), index)));
        }
    }
    let end = reached.unwrap_or(best.1);
    let mut chain = vec![end];
    while let Some(&last) = chain.last() {
        let parent = nodes[last as usize].parent;
        if parent == u32::MAX {
            break;
        }
        chain.push(parent);
    }
    chain.reverse();
    let mut along = Vec::with_capacity(chain.len());
    let mut crossings = Vec::with_capacity(chain.len());
    let mut walked = 0.0;
    let mut last = entry;
    for (at, &index) in chain.iter().enumerate() {
        let node = &nodes[index as usize];
        if at > 0 {
            walked += distance(last, node.entry);
            crossings.push(node.entry);
            last = node.entry;
        }
        along.push(walked);
    }
    Coarse {
        nodes: chain.iter().map(|&index| (nodes[index as usize].slot, nodes[index as usize].region)).collect(),
        crossings,
        along,
        reached: reached.is_some(),
        looked,
    }
}

/// A place looked at in a search over places.
#[derive(Clone, Copy)]
struct Node {
    g: f32,
    parent: u32,
    at: PlaceRef,
    closed: bool,
}

const UNSEEN: Node =
    Node { g: f32::INFINITY, parent: u32::MAX, at: PlaceRef { slot: 0, index: 0, x: 0, z: 0 }, closed: false };

/// Where a chunk's places begin among a search's nodes, and which of its
/// regions the search may enter.
#[derive(Clone, Copy, Default)]
struct Mark {
    stamp: u32,
    /// [`super::NONE`] where none of its places may be entered.
    base: u32,
    allowed: (u32, u32),
}

/// What a search over places keeps between searches on one thread, so it
/// makes nothing anew.
#[derive(Default)]
struct Scratch {
    stamp: u32,
    marks: Vec<Mark>,
    nodes: Vec<Node>,
    allowed: Vec<u16>,
    open: BinaryHeap<Reverse<(u64, u32)>>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

/// A way over places found.
struct Fine {
    places: Vec<PlaceRef>,
    reached: bool,
    cost: f32,
    looked: usize,
}

impl Scratch {
    /// The node of place `at`, its chunk's nodes made where it is first
    /// reached; `None` where the search may not enter it.
    #[inline]
    fn node(&mut self, nav: &BlockNav, at: PlaceRef, class: Option<usize>, corridor: Option<&Corridor>) -> Option<u32> {
        let mark = self.marks[at.slot as usize];
        let mark = if mark.stamp == self.stamp {
            mark
        } else {
            let mark = self.mark(nav, at.slot, corridor);
            self.marks[at.slot as usize] = mark;
            mark
        };
        if mark.base == super::NONE {
            return None;
        }
        if corridor.is_some() {
            let region = nav.region_of(at, class?);
            if !self.allowed[mark.allowed.0 as usize..mark.allowed.1 as usize].contains(&region) {
                return None;
            }
        }
        Some(mark.base + u32::from(at.index))
    }

    fn mark(&mut self, nav: &BlockNav, slot: u32, corridor: Option<&Corridor>) -> Mark {
        let from = self.allowed.len() as u32;
        if let Some(corridor) = corridor {
            self.allowed.extend(corridor.nodes.iter().filter(|(kept, _)| *kept == slot).map(|(_, region)| *region));
            if self.allowed.len() as u32 == from {
                return Mark { stamp: self.stamp, base: super::NONE, allowed: (from, from) };
            }
        }
        let base = self.nodes.len() as u32;
        let count = nav.slot(slot).map_or(0, |slot| slot.chunk.len());
        self.nodes.resize(self.nodes.len() + count, UNSEEN);
        Mark { stamp: self.stamp, base, allowed: (from, self.allowed.len() as u32) }
    }
}

/// The search over places, from `start` to `goal` (or towards `target`),
/// kept to `corridor`'s regions where there is one.
#[allow(clippy::too_many_arguments)]
fn fine(
    scratch: &mut Scratch,
    nav: &BlockNav,
    class: Option<usize>,
    start: PlaceRef,
    goal: Option<PlaceRef>,
    target: [f32; 3],
    corridor: Option<&Corridor>,
    ask: &PathAsk,
) -> Fine {
    scratch.stamp = scratch.stamp.wrapping_add(1);
    if scratch.stamp == 0 {
        scratch.marks.iter_mut().for_each(|mark| *mark = Mark::default());
        scratch.stamp = 1;
    }
    scratch.marks.resize(nav.slot_count(), Mark::default());
    scratch.nodes.clear();
    scratch.allowed.clear();
    scratch.open.clear();
    let walker = ask.walker;
    // Across many regions, leaning towards the goal; within one or two, the cheapest way.
    let greed = if corridor.is_some_and(|corridor| corridor.nodes.len() > 2) { ask.greed.max(1.0) } else { 1.0 };
    let least = ask.costs.least() * greed;
    // Ending short of the goal, at the corridor's last region: heading for where the way goes on from it.
    let end_region =
        corridor.filter(|corridor| corridor.ends_short).and_then(|corridor| corridor.nodes.last().copied());
    let aim = end_region.and_then(|_| corridor.and_then(|corridor| corridor.next)).unwrap_or(target);
    let ahead = |at: [f32; 3]| {
        let (x, z) = ((at[0] - aim[0]).abs(), (at[2] - aim[2]).abs());
        ((x.max(z) - x.min(z)) + DIAGONAL * x.min(z)) * least + (at[1] - aim[1]).abs() * UPWARD
    };
    let Some(first) = scratch.node(nav, start, class, corridor) else {
        return Fine { places: Vec::new(), reached: false, cost: 0.0, looked: 0 };
    };
    let start_point = nav.point(start).unwrap_or(ask.from);
    scratch.nodes[first as usize] = Node { g: 0.0, parent: u32::MAX, at: start, closed: false };
    scratch.open.push(Reverse((key(ahead(start_point), ahead(start_point)), first)));
    let mut best = (ahead(start_point), first);
    let mut reached = None;
    let mut looked = 0;
    while let Some(Reverse((_, index))) = scratch.open.pop() {
        let node = scratch.nodes[index as usize];
        if node.closed {
            continue;
        }
        scratch.nodes[index as usize].closed = true;
        let at = node.at;
        let is_end = match end_region {
            Some(end) => class.is_some_and(|class| (at.slot, nav.region_of(at, class)) == end),
            None => Some(at) == goal,
        };
        if is_end {
            reached = Some(index);
            break;
        }
        looked += 1;
        if looked > ask.most {
            break;
        }
        let Some((place, cy)) = nav.place(at) else { continue };
        let feet = place.feet(cy);
        // Along the axes first: a step cornerwise needs both of those beside it on the level.
        let mut level = [false; 4];
        for (way, [dx, dz]) in super::regions::AXES.into_iter().enumerate() {
            let Some((to, to_place, to_cy)) = straight_step(nav, at, place, cy, dx, dz, walker) else { continue };
            let to_feet = to_place.feet(to_cy);
            let crossed = area_cost(nav, ask, to, to_place.area);
            // Cornerwise past it only where it is level and may be crossed, so no corner of ground nothing walks
            // on is cut.
            level[way] = to_feet == feet && crossed.is_finite();
            let cost = crossed + (to_feet - feet).abs() as f32 / 16.0 * UPWARD;
            scratch.reach(nav, class, corridor, index, node.g, to, cost, &ahead, &mut best);
        }
        // Over gaps, for a walker that jumps.
        if walker.jump > 0 {
            for [dx, dz] in super::regions::AXES {
                let Some((to, to_place, across)) = jump_step(nav, at, dx, dz, walker) else { continue };
                let cost = area_cost(nav, ask, to, to_place.area) * across as f32 + JUMPING;
                scratch.reach(nav, class, corridor, index, node.g, to, cost, &ahead, &mut best);
            }
        }
        for (dx, dz, a, b) in [(1, 1, 0, 2), (1, -1, 0, 3), (-1, 1, 1, 2), (-1, -1, 1, 3)] {
            if !level[a] || !level[b] {
                continue;
            }
            let Some((to, to_place, to_cy)) = straight_step(nav, at, place, cy, dx, dz, walker) else { continue };
            if to_place.feet(to_cy) != feet {
                continue;
            }
            let cost = DIAGONAL * area_cost(nav, ask, to, to_place.area);
            scratch.reach(nav, class, corridor, index, node.g, to, cost, &ahead, &mut best);
        }
    }
    let end = reached.unwrap_or(best.1);
    let mut places = Vec::new();
    let mut at = end;
    while at != u32::MAX {
        let node = scratch.nodes[at as usize];
        places.push(node.at);
        at = node.parent;
    }
    places.reverse();
    Fine { cost: scratch.nodes[end as usize].g, places, reached: reached.is_some(), looked }
}

/// What a block of place `at`, of area `area`, costs as `ask` costs it: as
/// its area, or the area of a box it is in.
#[inline]
fn area_cost(nav: &BlockNav, ask: &PathAsk, at: PlaceRef, area: u8) -> f32 {
    if ask.boxes.is_empty() {
        return ask.costs.of(area);
    }
    let boxed = nav.point(at).and_then(|point| ask.boxes.iter().rev().find(|area_box| area_box.holds(point)));
    ask.costs.of(boxed.map_or(area, |area_box| area_box.area))
}

/// What a block of place `at` costs as `ask` costs it.
fn cost_of(nav: &BlockNav, ask: &PathAsk, at: PlaceRef) -> f32 {
    nav.place(at).map_or(1.0, |(place, _)| area_cost(nav, ask, at, place.area))
}

/// What a node is ordered by in the open list: its whole guessed cost, and
/// on a tie, the nearer the goal the sooner.
#[inline]
fn key(whole: f32, ahead: f32) -> u64 {
    u64::from(whole.max(0.0).to_bits()) << 32 | u64::from(ahead.max(0.0).to_bits())
}

impl Scratch {
    /// Reaches place `to` from node `from` (cost `g` so far) at `step` more.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn reach(
        &mut self,
        nav: &BlockNav,
        class: Option<usize>,
        corridor: Option<&Corridor>,
        from: u32,
        g: f32,
        to: PlaceRef,
        step: f32,
        ahead: &impl Fn([f32; 3]) -> f32,
        best: &mut (f32, u32),
    ) {
        if !step.is_finite() {
            return;
        }
        let Some(index) = self.node(nav, to, class, corridor) else { return };
        let cost = g + step;
        let node = &mut self.nodes[index as usize];
        if node.closed || node.g <= cost {
            return;
        }
        *node = Node { g: cost, parent: from, at: to, closed: false };
        let Some(point) = nav.point(to) else { return };
        let guess = ahead(point);
        if guess < best.0 {
            *best = (guess, index);
        }
        self.open.push(Reverse((key(cost + guess, guess), index)));
    }
}

/// A place reached from `from` by `walker`, chosen by `seed`: no further
/// than `radius` blocks away by the way there, and at least a third of
/// that, where there is one. For wandering.
pub fn random_near(
    nav: &BlockNav,
    from: PlaceRef,
    walker: Walker,
    costs: &AreaCosts,
    radius: f32,
    seed: u32,
) -> Option<PlaceRef> {
    let origin = nav.point(from)?;
    let mut seen: CoordMap<(u32, u16), ()> = CoordMap::default();
    let mut queue = std::collections::VecDeque::from([from]);
    seen.insert((from.slot, from.index), ());
    let mut far_enough = Vec::new();
    let mut near = Vec::new();
    while let Some(at) = queue.pop_front() {
        if seen.len() > 20_000 {
            break;
        }
        for [dx, dz] in super::regions::AXES {
            let Some(to) = nav.step(at, dx, dz, walker) else { continue };
            let Some((place, _)) = nav.place(to) else { continue };
            if !costs.allows(place.area) || seen.contains_key(&(to.slot, to.index)) {
                continue;
            }
            let Some(point) = nav.point(to) else { continue };
            let far = ((point[0] - origin[0]).powi(2) + (point[2] - origin[2]).powi(2)).sqrt();
            if far > radius {
                continue;
            }
            seen.insert((to.slot, to.index), ());
            queue.push_back(to);
            if far >= radius / 3.0 { far_enough.push(to) } else { near.push(to) }
        }
    }
    let pick = if far_enough.is_empty() { &near } else { &far_enough };
    (!pick.is_empty()).then(|| pick[seed as usize % pick.len()])
}

#[cfg(test)]
mod tests;
