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
    joins::{BACK, Joins},
    level::Level,
    made::{DETAIL, Made, RoomRules},
    rooms::{NOTHING, Room, RoomExit, greater, lesser},
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

/// How many rooms a room may be joined to: the four beside it in its own
/// layer, then the four beside it one layer down, then one layer up.
pub(crate) const SLOTS: usize = 12;

/// How many pieces a tunnel's line is made of.
const PIECES: usize = 16;

/// The steepest a tunnel is held to where it has no limit: none.
pub(crate) const NO_LIMIT: f32 = 1.0e6;

/// How far under a tunnel's middle its floor lies, of half its width.
const FLOOR: f32 = 0.55;

/// How far a hillside must fall across a mouth for it to be one, as a share
/// of how high its room is.
const MOUTH_STEEP: f32 = 0.45;

/// How many places near its own a mouth may stand at, for the steepest.
const SPOTS: usize = 5;

/// The eight ways round a place, along the ground.
const RING: [V3; 8] = [
    [1.0, 0.0, 0.0],
    [0.707_106_77, 0.0, 0.707_106_77],
    [0.0, 0.0, 1.0],
    [-0.707_106_77, 0.0, 0.707_106_77],
    [-1.0, 0.0, 0.0],
    [-0.707_106_77, 0.0, -0.707_106_77],
    [0.0, 0.0, -1.0],
    [0.707_106_77, 0.0, -0.707_106_77],
];

/// A network's values, as its kernel reads them.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub rooms: Vec<Room>,
    pub layout: CaveLayout,
    pub spacing: f32,
    pub depth: f32,
    /// Half a tunnel's width, the narrowest and the widest.
    pub radius: f32,
    pub radius_to: f32,
    pub winding: f32,
    /// The steepest a tunnel runs, as rise over run, where there is room to wind it longer.
    pub slope: f32,
    /// How many tunnels have a dead end off them, and how many joins make loops, 0 to 1.
    pub branches: f32,
    pub loops: f32,
    pub rough: f32,
    pub roof: f32,
    pub entrances: f32,
    /// How many of the ways up to the surface are great open rooms, a mouth in the ground, 0 to 1.
    pub mouths: f32,
    pub main: u32,
    pub side: u32,
    /// How many layers of rooms an endless network has, one under another, and how far apart they lie.
    pub layers: u32,
    pub layer_depth: f32,
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
    /// The regions it may be joined to: the four beside it, then those one layer down, then one layer up.
    pub beside: [Region; SLOTS],
    /// Whether a tunnel joins it to each of those.
    pub links: [bool; SLOTS],
    /// The exit each of those tunnels leaves by.
    pub toward: [Option<usize>; SLOTS],
    /// The way to each of them, along its own floor.
    pub ways: [V3; SLOTS],
    /// Which of its shapes it has, a bit each.
    pub present: u64,
    /// The room itself, where it is made by rules.
    pub made: Option<Arc<Made>>,
    /// Where between its least and greatest depth it lies, 0 to 1.
    pub deep: f32,
    pub entrance: bool,
    /// Whether the room is itself its way up: a great mouth in a hillside, its floor level with the ground below
    /// it and the hill over the rest of it, its downhill wall open.
    pub mouth: bool,
    /// For a mouth, how high its middle lies, where the hillside has been found steep enough for one, and the
    /// place on the hillside below it its passage comes out at.
    pub seat: Option<f32>,
    pub out: Option<V3>,
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

/// A tunnel: its line, the way up along it, half its width (at its widest,
/// and at each place along its line) and whether it is kept under the
/// ground's roof.
#[derive(Clone, Debug)]
pub(crate) struct Tunnel {
    pub line: [V3; PIECES + 1],
    pub up: V3,
    pub radius: f32,
    pub radii: [f32; PIECES + 1],
    /// How far under its line its floor lies: the same all along, however wide it is.
    pub floor: f32,
    /// Each piece's middle, and the square of how far from it anything of the piece reaches, roughness and
    /// all: a place further off than that is left out of the piece's sums.
    pub near: [(V3, f32); PIECES],
    pub roofed: bool,
    /// Whether its walls are left smooth: a mouth's passage, whose roughness would break the thin ground over its
    /// end into loose blocks.
    pub smooth: bool,
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
    /// The furthest any of its walls is pushed in or out, in metres.
    pub roughest: f32,
    /// Whether it has rooms made by rules, and so fine detail on its walls.
    pub detailed: bool,
    pub level: Option<Level>,
}

thread_local! {
    /// What was last gathered on this thread, by network and region.
    static GATHERED: RefCell<Vec<(u64, Region, Rc<Gathered>)>> = const { RefCell::new(Vec::new()) };
    /// The rooms last made by rules on this thread, by network and region: a room is asked for by every
    /// region round its own, and is the same room each time.
    static MADE: RefCell<Vec<(u64, Region, Arc<Made>)>> = const { RefCell::new(Vec::new()) };
    /// The heights levels were last found to lie at, by network.
    static LEVELS: RefCell<Vec<(u64, f32)>> = const { RefCell::new(Vec::new()) };
}

/// How many gatherings a thread keeps, and how many rooms made by rules.
const KEPT: usize = 24;
const ROOMS_KEPT: usize = 256;

impl Settings {
    /// The room with the furthest reach's, in metres.
    pub fn reach(&self) -> f32 {
        self.rooms.iter().map(Room::reach).fold(1.0, greater)
    }

    /// How far a tunnel may stray from the straight line between its ends
    /// and a wall from where its shape puts it, in metres.
    pub fn stray(&self) -> f32 {
        greater(self.radius, self.radius_to) + self.rough + 2.0
    }

    /// The furthest any wall is pushed in or out: its own roughness, or that of a room made by rules.
    pub fn roughest(&self) -> f32 {
        self.rooms.iter().filter_map(|room| room.rules).map(|rules| rules.kept().roughest()).fold(self.rough, greater)
    }

    /// How far from its region's middle a room may be moved, of half the
    /// region, so it and its tunnels' ends stay inside.
    fn loose(&self) -> f32 {
        let free = 1.0 - 2.0 * (self.reach() + self.stray()) / self.spacing;
        if self.rooms.iter().any(|room| room.rules.is_some()) {
            // Rooms made by rules may reach into the regions beside theirs, and meet: only never past them,
            // which is as far as what is gathered for a region looks.
            let most = 2.8 - 2.0 * (self.reach() + self.stray()) / self.spacing;
            return greater(lesser(greater(free, lesser(most, 0.6)), 0.8), 0.0);
        }
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
        let roughest = set.roughest();
        let detailed = set.rooms.iter().any(|room| room.rules.is_some());
        let mut network = Network { set, ground, id, roughest, detailed, level: None };
        if network.set.layout == CaveLayout::Level {
            network.level = Some(Level::laid(&network));
        }
        network
    }

    /// The grid its regions lie on.
    pub fn grid(&self) -> Grid {
        Grid { spacing: self.set.spacing }
    }

    /// The noise its walls are roughened by at `p`, each about -1 to 1: lumps, and where it has rooms made
    /// by rules, detail of a block or two for them and its tunnels.
    pub fn noise(&self, p: V3) -> [f32; 2] {
        // Coarser the rougher the walls, so a wall pushed far is pushed far over a long way, and no hollow
        // opens apart from its room.
        let grain = 4.0 + 2.0 * self.roughest;
        let lumps = gradient3(p[0] / grain, p[1] / grain, p[2] / grain, self.set.seed.wrapping_add(11));
        let detail = match self.detailed {
            true => gradient3(p[0] / DETAIL, p[1] / DETAIL, p[2] / DETAIL, self.set.seed.wrapping_add(12)),
            false => 0.0,
        };
        [lumps, detail]
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

    /// The exits of `plan`'s room, in the room's own space.
    pub fn exits<'a>(&'a self, plan: &'a Plan) -> &'a [RoomExit] {
        match &plan.made {
            Some(made) => &made.exits,
            None => &self.set.rooms[plan.room].exits,
        }
    }

    /// How far inside the hollow of `plan`'s room `own` is, a place in the room's space, its walls moved
    /// by `noise`, about -1 to 1, where it is given.
    pub fn room_inside(&self, plan: &Plan, own: V3, noise: Option<[f32; 2]>) -> f32 {
        match &plan.made {
            Some(made) => made.inside(own, noise),
            None => self.set.rooms[plan.room].inside(own, plan.present, noise.map(|noise| noise[0] * self.set.rough)),
        }
    }

    /// No place of `plan`'s room is further than this from its origin.
    pub fn room_reach(&self, plan: &Plan) -> f32 {
        match &plan.made {
            Some(made) => made.reach(),
            None => self.set.rooms[plan.room].reach() + self.set.rough,
        }
    }

    /// The room of `region` in an endless network, before its height: none
    /// where the region has none.
    pub fn plan(&self, grid: &Grid, region: Region) -> Option<Plan> {
        let set = &self.set;
        let (cell, layer) = ([region[0], region[2]], region[1]);
        if layer < 0 || layer >= set.layers as i32 {
            return None;
        }
        let own = self.joins(layer);
        let beside = own.links(cell)?;
        let mut links = [false; SLOTS];
        links[..4].copy_from_slice(&beside);
        // Ramps down to the layer under it, and up from it to rooms that have a ramp down to it.
        if layer + 1 < set.layers as i32 {
            links[4..8].copy_from_slice(&own.ramps(&self.joins(layer + 1), cell));
        }
        if layer > 0 {
            let over = self.joins(layer - 1);
            let flat = grid.beside(region);
            for way in 0..4 {
                links[8 + way] = over.ramps(&own, [flat[way][0], flat[way][2]])[BACK[way]];
            }
        }
        let at = |part: i32| share(mix(set.seed, &[region[0], region[1], region[2], part]));
        let loose = set.loose();
        let jitter = [(at(3) * 2.0 - 1.0) * loose, (at(4) * 2.0 - 1.0) * loose];
        // Only the topmost layer has ways up to the surface; some of them are the room itself, a mouth.
        let entrance = layer == 0 && at(8) < set.entrances;
        let mouth = entrance && at(9) < set.mouths;
        let mut plan = self.planned(grid, region, set.room(at(5)), jitter, links, at(6), (entrance, mouth));
        plan.mouth = mouth;
        Some(plan)
    }

    /// What decides the joins of layer `layer`: the topmost by the network's own seed, as a network of one
    /// layer is.
    fn joins(&self, layer: i32) -> Joins {
        let seed = if layer == 0 { self.set.seed } else { mix(self.set.seed, &[layer, 71]) };
        Joins { seed, loops: self.set.loops }
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
        links: [bool; SLOTS],
        deep: f32,
        (entrance, mouth): (bool, bool),
    ) -> Plan {
        let set = &self.set;
        let stand = grid.stand(region, jitter);
        let flat = grid.beside(region);
        let beside: [Region; SLOTS] = std::array::from_fn(|slot| {
            let [i, layer, j] = flat[slot % 4];
            [i, layer + [0, 1, -1][slot / 4], j]
        });
        let middle = grid.stand(region, [0.0, 0.0]).foot;
        let ways = beside.map(|next| {
            let away = sub(grid.stand(next, [0.0, 0.0]).foot, middle);
            unit([dot(away, stand.x), 0.0, dot(away, stand.z)], [1.0, 0.0, 0.0])
        });
        let made = set.rooms[room].rules.map(|rules| {
            // A mouth hangs no rock from its roof: where its passage opens the roof away, the rock would float.
            if mouth {
                let rules = RoomRules { hanging: 0, ..rules };
                return Arc::new(Made::of(&rules, mix(set.seed, &[region[0], region[1], region[2], 40])));
            }
            let known = |kept: &(u64, Region, Arc<Made>)| kept.0 == self.id && kept.1 == region;
            let kept = MADE.with_borrow(|kept| kept.iter().find(|kept| known(kept)).map(|kept| kept.2.clone()));
            kept.unwrap_or_else(|| {
                let made = Arc::new(Made::of(&rules, mix(set.seed, &[region[0], region[1], region[2], 40])));
                MADE.with_borrow_mut(|kept| {
                    if kept.len() >= ROOMS_KEPT {
                        kept.remove(0);
                    }
                    kept.push((self.id, region, made.clone()));
                });
                made
            })
        });
        let exits: &[RoomExit] = match &made {
            Some(made) => &made.exits,
            None => &set.rooms[room].exits,
        };
        // Each way of turning it, from one the region picks: the first whose exits face its tunnels best.
        let first = mix(set.seed, &[region[0], region[1], region[2], 7]) % 8;
        let mut best: Option<(f32, u8, bool, [Option<usize>; SLOTS])> = None;
        for step in 0..8 {
            let way = (first + step) % 8;
            let (turns, mirrored) = ((way % 4) as u8, way >= 4);
            let mut used = vec![false; exits.len()];
            let (mut score, mut toward) = (0.0, [None; SLOTS]);
            for slot in (0..SLOTS).filter(|&slot| links[slot]) {
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
        let (_, turns, mirrored, toward) = best.unwrap_or((0.0, 0, false, [None; SLOTS]));
        let shapes = &set.rooms[room].shapes;
        let mut present = 0u64;
        for (at, shape) in shapes.iter().enumerate().take(64) {
            let has = share(mix(set.seed, &[region[0], region[1], region[2], 16 + at as i32])) < shape.chance;
            present |= u64::from(has) << at;
        }
        let (mouth, seat, out) = (false, None, None);
        Plan {
            region,
            room,
            stand,
            turns,
            mirrored,
            beside,
            links,
            toward,
            ways,
            present,
            made,
            deep,
            entrance,
            mouth,
            seat,
            out,
        }
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
        let bounds = match &plan.made {
            // A made room's own box, as the room is turned: it lies along the axes whichever way that is.
            Some(made) => {
                let [low, high] = made.bounds();
                let mut bounds = Bounds::NONE;
                for corner in 0..8 {
                    let pick = |axis: usize| if corner >> axis & 1 == 0 { low[axis] } else { high[axis] };
                    let v = [pick(0), pick(1), pick(2)];
                    let p = add(at, add(add(times(axes[0], v[0]), times(axes[1], v[1])), times(axes[2], v[2])));
                    bounds = bounds.with(Bounds { low: p, high: p });
                }
                bounds
            }
            None => Bounds::round(at, self.room_reach(&plan)),
        };
        Placed { plan, at, axes, bounds }
    }

    /// How far under the ground at its middle an endless network's room
    /// lies: its floor, in metres.
    pub fn sunk(&self, plan: &Plan) -> f32 {
        -self.set.depth * (0.75 + 0.5 * plan.deep) - plan.region[1] as f32 * self.set.layer_depth
    }

    /// The room of `region` in an endless network, a mouth seated in its
    /// hillside (see `seat_mouths`): none where the region has none.
    pub fn seated(&self, grid: &Grid, region: Region) -> Option<Plan> {
        let mut plans = [self.plan(grid, region)];
        self.seat_mouths(&mut plans);
        let [plan] = plans;
        plan
    }

    /// How far over the ground `ground` metres high at its foot `plan`'s
    /// room lies: a mouth where it is seated, else as deep as it is sunk.
    pub fn rise(&self, plan: &Plan, ground: f32) -> f32 {
        plan.seat.map_or_else(|| self.sunk(plan), |seat| seat - ground)
    }

    /// Makes each of `plans` that would be a mouth one where the ground
    /// round it falls away steeply enough on one side, its floor level with
    /// the ground there and the room moved into the hill; and an ordinary
    /// room with a way up where it does not.
    fn seat_mouths(&self, plans: &mut [Option<Plan>]) {
        let mouth = |at: &usize| plans[*at].as_ref().is_some_and(|plan| plan.mouth);
        let mouths: Vec<usize> = (0..plans.len()).filter(mouth).collect();
        if mouths.is_empty() {
            return;
        }
        // The ground at a few places a mouth may stand, each with eight places round it as far out as most of
        // the room reaches, asked at once.
        let spots = |plan: &Plan| -> [V3; SPOTS] {
            let near = self.set.spacing * 0.15;
            let ways = [[0.0, 0.0], [1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]];
            ways.map(|[x, z]| add(plan.stand.foot, [x * near, 0.0, z * near]))
        };
        let ring = |plan: &Plan, spot: V3| -> [V3; 9] {
            let far = self.room_reach(plan) * 0.8;
            std::array::from_fn(|k| if k == 0 { spot } else { add(spot, times(RING[k - 1], far)) })
        };
        let places: Vec<V3> = mouths
            .iter()
            .flat_map(|&at| {
                let plan = plans[at].as_ref().expect("a mouth");
                spots(plan).into_iter().flat_map(move |spot| ring(plan, spot))
            })
            .collect();
        let heights = self.grounds(&places);
        for (n, &at) in mouths.iter().enumerate() {
            let plan = plans[at].as_mut().expect("a mouth");
            // The spot where the ground falls furthest to one side, and that side.
            let mut best: Option<(f32, V3, usize, f32)> = None;
            for (k, spot) in spots(plan).into_iter().enumerate() {
                let ground = &heights[(n * SPOTS + k) * 9..(n * SPOTS + k) * 9 + 9];
                let low = (1..9).min_by(|&a, &b| ground[a].total_cmp(&ground[b])).unwrap_or(1);
                let fall = ground[0] - ground[low];
                if best.is_none_or(|(most, ..)| fall > most) {
                    best = Some((fall, spot, low, ground[low]));
                }
            }
            let tall = self.room_height(plan);
            let Some((_, spot, low, below)) = best.filter(|(fall, ..)| *fall >= tall * MOUTH_STEEP) else {
                plan.mouth = false;
                continue;
            };
            // Into the hill, away from where it falls, and level with the ground there.
            let into = times(RING[low - 1], -self.room_reach(plan) * 0.2);
            // Kept within the box any room of its region may stand in, so the regions round it find it.
            let [x, _, z] = plan.region;
            let (spread, spacing) = (self.set.loose() * 0.5 * self.set.spacing, self.set.spacing);
            let held = |v: f32, cell: i32| {
                let middle = (cell as f32 + 0.5) * spacing;
                v.clamp(middle - spread, middle + spread)
            };
            let foot = add(spot, into);
            plan.stand.foot = [held(foot[0], x), foot[1], held(foot[2], z)];
            plan.seat = Some(below - self.room_floor(plan));
            let out = add(spot, times(RING[low - 1], self.room_reach(plan) * 0.8));
            plan.out = Some([out[0], below, out[2]]);
        }
    }

    /// How far over its middle `plan`'s room's floor lies, where it can be
    /// stood on, in metres.
    fn room_floor(&self, plan: &Plan) -> f32 {
        plan.made.as_ref().map_or(0.0, |made| made.stood())
    }

    /// How high `plan`'s room is, in metres.
    fn room_height(&self, plan: &Plan) -> f32 {
        match &plan.made {
            Some(made) => made.bounds()[1][1],
            None => self.set.rooms[plan.room].reach(),
        }
    }

    /// Everything of an endless network that can reach into `region`.
    fn gather(&self, grid: &Grid, region: Region) -> Gathered {
        // The region, the four beside it, and those beside them: each once, in this order. Then the same of
        // every layer under the first, a layer after another.
        let region = [region[0], 0, region[2]];
        let mut cells = vec![region];
        cells.extend(grid.beside(region));
        for at in 1..5 {
            for next in grid.beside(cells[at]) {
                if !cells.contains(&next) {
                    cells.push(next);
                }
            }
        }
        let layers = self.set.layers.max(1) as i32;
        let regions: Vec<Region> =
            (0..layers).flat_map(|layer| cells.iter().map(move |&[i, _, j]| [i, layer, j])).collect();
        // The rooms that can reach into the region itself: its own and the eight round it, of each layer. A great
        // hall near a corner of its region reaches over the corner, into the region across it.
        let within = |at: usize| {
            let [i, _, j] = regions[at];
            (i - region[0]).abs() <= 1 && (j - region[2]).abs() <= 1
        };
        let mut plans: Vec<Option<Plan>> = regions.iter().map(|&region| self.plan(grid, region)).collect();
        self.seat_mouths(&mut plans);
        // The ground over every room, and over where each inner room's entrance comes out, asked at once.
        let mut feet: Vec<V3> = plans.iter().flatten().map(|plan| plan.stand.foot).collect();
        let mouths: Vec<Option<V3>> = plans
            .iter()
            .enumerate()
            .map(|(at, plan)| {
                let way_up = |plan: &&Plan| within(at) && plan.entrance && !plan.mouth;
                plan.as_ref().filter(way_up).map(|plan| self.mouth(grid, plan))
            })
            .collect();
        feet.extend(mouths.iter().flatten());
        let heights = self.grounds(&feet);
        let mut heights = heights.into_iter();
        let placed: Vec<Option<Placed>> = plans
            .into_iter()
            .map(|plan| {
                let plan = plan?;
                let ground = heights.next().unwrap_or(0.0);
                let rise = self.rise(&plan, ground);
                Some(self.placed(grid, plan, ground, rise))
            })
            .collect();
        let find =
            |region: Region| regions.iter().position(|&known| known == region).and_then(|at| placed[at].as_ref());
        let mut gathered = Gathered::default();
        let mut pairs: Vec<(Region, Region)> = Vec::new();
        let inner = || placed.iter().enumerate().filter(|(at, _)| within(*at)).filter_map(|(_, room)| room.as_ref());
        for room in inner() {
            for slot in (0..SLOTS).filter(|&slot| room.plan.links[slot]) {
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
        for (room, mouth) in placed.iter().zip(&mouths) {
            if let (Some(room), Some(mouth)) = (room, mouth) {
                let ground = heights.next().unwrap_or(0.0);
                gathered.tunnels.push(self.entrance(grid, room, *mouth, ground));
            }
        }
        // Each mouth's passage out to its hillside: of every room placed, as one may reach two regions off.
        gathered.tunnels.extend(placed.iter().flatten().filter_map(|room| self.passage(room)));
        gathered.rooms = inner().cloned().collect();
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
        let exits = self.exits(&room.plan);
        let out = |v: V3| add(add(times(room.axes[0], v[0]), times(room.axes[1], v[1])), times(room.axes[2], v[2]));
        match room.plan.toward[slot] {
            Some(exit) => (add(room.at, out(exits[exit].at)), out(exits[exit].way)),
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
    /// first, and the dead end off it where it has one: none where either
    /// room does not know the other.
    pub fn tunnel(&self, a: &Placed, b: &Placed) -> Vec<Tunnel> {
        let slots = |a: &Placed, b: &Placed| a.plan.beside.iter().position(|&next| next == b.plan.region);
        let (Some(slot_a), Some(slot_b)) = (slots(a, b), slots(b, a)) else { return Vec::new() };
        let (from, to) = (self.end(a, slot_a), self.end(b, slot_b));
        let [x, y, z] = a.plan.region;
        let set = &self.set;
        let seed = mix(set.seed, &[x, y, z, b.plan.region[0], b.plan.region[1], b.plan.region[2], 9]);
        let up = unit(add(a.axes[1], b.axes[1]), a.axes[1]);
        // As wide as the seed says between its narrowest and its widest.
        let radius = match set.radius_to > set.radius {
            true => set.radius + (set.radius_to - set.radius) * share(mix(seed, &[12])),
            false => set.radius,
        };
        let ramp = a.plan.region[1] != b.plan.region[1];
        let main = self.wound(from, to, up, seed, set.winding, true, radius, ramp);
        if share(mix(seed, &[13])) >= set.branches {
            return vec![main];
        }
        // A dead end: off one side from somewhere along its middle, a little narrower, a fifth of a region
        // long or so.
        let at = 5 + (mix(seed, &[14]) % 7) as usize;
        let along = unit(sub(main.line[at + 1], main.line[at - 1]), [1.0, 0.0, 0.0]);
        let side = if mix(seed, &[15]) % 2 == 0 { 1.0 } else { -1.0 };
        let aside = times(unit(cross(up, along), [0.0, 0.0, 1.0]), side);
        let long = set.spacing * (0.12 + 0.14 * share(mix(seed, &[16])));
        let lean = (share(mix(seed, &[17])) - 0.5) * 0.8;
        let end = add(main.line[at], add(times(aside, long), times(along, long * lean)));
        let narrow = greater(radius * 0.8, 0.75);
        let (from, to) = ((main.line[at], aside), (end, times(aside, -1.0)));
        let spur = self.wound(from, to, up, mix(seed, &[18]), set.winding, true, narrow, false);
        vec![main, spur]
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
        let exits = self.exits(plan);
        let taken = |exit: usize| plan.toward.contains(&Some(exit));
        let exit = (0..exits.len()).find(|&exit| !taken(exit)).or(exits.len().checked_sub(1));
        let way = match exit {
            Some(exit) => turned(exits[exit].way, plan.turns, plan.mirrored),
            None => [1.0, 0.0, 0.0],
        };
        (exit, unit([way[0], 0.0, way[2]], [1.0, 0.0, 0.0]))
    }

    /// A mouth's passage: wide, open and nearly level, from its floor out to
    /// the hillside below it, so the hall opens in the hill's side.
    fn passage(&self, room: &Placed) -> Option<Tunnel> {
        let out = room.plan.out?;
        let tall = self.room_height(&room.plan);
        let radius = greater(self.set.radius_to, tall * 0.3);
        let floor = room.at[1] + self.room_floor(&room.plan);
        let from = [room.at[0], floor + radius * FLOOR, room.at[2]];
        // Out past where the hillside comes down to the floor, into the open.
        let along = unit([out[0] - from[0], 0.0, out[2] - from[2]], [1.0, 0.0, 0.0]);
        let to = add([out[0], out[1] + radius * FLOOR, out[2]], times(along, radius));
        let [x, y, z] = room.plan.region;
        let seed = mix(self.set.seed, &[x, y, z, 11]);
        let winding = self.set.winding * 0.3;
        let passage = self.wound((from, along), (to, along), room.axes[1], seed, winding, false, radius, false);
        Some(Tunnel { smooth: true, ..passage })
    }

    /// The way up from `room` to the surface, coming out over `mouth`
    /// where the ground is `ground` metres high.
    fn entrance(&self, grid: &Grid, room: &Placed, mouth: V3, ground: f32) -> Tunnel {
        let (exit, way) = self.spare(&room.plan);
        let stand = &room.plan.stand;
        let away = add(times(stand.x, way[0]), times(stand.z, way[2]));
        let exits = self.exits(&room.plan);
        let out = |v: V3| add(add(times(room.axes[0], v[0]), times(room.axes[1], v[1])), times(room.axes[2], v[2]));
        let from = match exit {
            Some(exit) => add(room.at, out(exits[exit].at)),
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
        self.wound((from, away), (top, down), room.axes[1], seed, self.set.winding * 0.5, false, self.set.radius, false)
    }

    /// A tunnel from one end to another, each a place and the way out of
    /// it: leaving each the way it faces, and wound sideways between by
    /// amounts from `seed`. One that would run steeper than the network's
    /// limit is wound further, so it is longer and climbs more gently, as
    /// far as a region leaves it room to.
    #[allow(clippy::too_many_arguments)]
    fn wound(
        &self,
        from: (V3, V3),
        to: (V3, V3),
        up: V3,
        seed: u32,
        winding: f32,
        roofed: bool,
        radius: f32,
        ramp: bool,
    ) -> Tunnel {
        // Its ends a little inside the rooms, so it opens into them.
        let (a, b) = (sub(from.0, times(from.1, radius * 0.5)), sub(to.0, times(to.1, radius * 0.5)));
        let across = sub(b, a);
        let length = dot(across, across).sqrt();
        let side = unit(cross(up, across), [1.0, 0.0, 0.0]);
        let pull = length * 0.5;
        let (leave, arrive) = (times(from.1, pull), times(to.1, -pull));
        // A ramp, from one layer to the next, may wind over nearly half a region: it has far to climb.
        let widest = self.set.spacing * if ramp { 0.45 } else { 0.2 };
        let mut most = greater(winding, 0.0) * lesser(length * 0.15, widest);
        // How far aside and how far up it is pushed at each of four places along it, and not at all at its ends.
        // A ramp is pushed to one side and then the other, which is what makes a way longest.
        let knot = |at: usize, part: i32| match at {
            1..=4 if ramp && part == 0 => {
                let far = 0.75 + 0.25 * share(mix(seed, &[at as i32, part]));
                if at % 2 == 0 { far } else { -far }
            }
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
        let lay = |line: &mut [V3; PIECES + 1], most: f32| {
            for (at, point) in line.iter_mut().enumerate() {
                let t = at as f32 / PIECES as f32;
                let (t2, t3) = (t * t, t * t * t);
                let weights = [2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, 3.0 * t2 - 2.0 * t3, t3 - t2];
                let straight = add(
                    add(times(a, weights[0]), times(leave, weights[1])),
                    add(times(b, weights[2]), times(arrive, weights[3])),
                );
                if ramp {
                    // A ramp is laid level first, at the height it starts from, and made to climb after.
                    let level = add(straight, times(up, dot(a, up) - dot(straight, up)));
                    *point = add(level, times(side, most * pushed(t, 0)));
                    continue;
                }
                let aside = add(times(side, most * pushed(t, 0)), times(up, most * 0.25 * pushed(t, 1)));
                *point = add(straight, aside);
            }
        };
        // A ramp climbs by the ground it covers, the same over every metre of its way but for a level piece at
        // each end: so no part of it is steeper than the rest, as a way that eased in and out would be in its
        // middle.
        let climbed = |line: &mut [V3; PIECES + 1]| {
            let mut run = [0.0f32; PIECES + 1];
            for at in 1..=PIECES {
                let step = sub(line[at], line[at - 1]);
                run[at] = run[at - 1] + dot(step, step).sqrt();
            }
            let (first, last) = (run[1], run[PIECES - 1]);
            for at in 0..=PIECES {
                let even = greater(lesser((run[at] - first) / greater(last - first, 1.0e-3), 1.0), 0.0);
                line[at] = add(line[at], times(up, dot(across, up) * even));
            }
        };
        lay(&mut line, most);
        if ramp {
            climbed(&mut line);
        }
        if roofed && self.set.slope < NO_LIMIT {
            // How far it runs along the ground against how far it climbs: wound wider while that is too steep.
            let climb = dot(across, up);
            let climb = if climb < 0.0 { -climb } else { climb };
            for _ in 0..if ramp { 10 } else { 6 } {
                let runs = line.windows(2).map(|piece| {
                    let step = sub(piece[1], piece[0]);
                    let level = sub(step, times(up, dot(step, up)));
                    (dot(level, level).sqrt(), dot(step, up))
                });
                // A ramp is held by its steepest piece; any other tunnel by its climb over its whole run.
                let gentle = match ramp {
                    true => runs.clone().all(|(run, rise)| (if rise < 0.0 { -rise } else { rise }) <= self.set.slope * run),
                    false => climb <= self.set.slope * runs.map(|(run, _)| run).sum::<f32>(),
                };
                if gentle || most >= widest {
                    break;
                }
                most = lesser(greater(most * 1.6, length * 0.06), widest);
                lay(&mut line, most);
                if ramp {
                    climbed(&mut line);
                }
            }
        }
        // Where a network's tunnels are of many widths, each also swells and narrows along its way, and
        // opens out at its ends, into the rooms.
        let mut radii = [radius; PIECES + 1];
        if self.set.radius_to > self.set.radius {
            for (at, wide) in radii.iter_mut().enumerate() {
                let t = at as f32 / PIECES as f32;
                let end = 1.0 - 4.0 * t * (1.0 - t);
                let flared = 1.0 + 0.4 * end * end * end * end;
                *wide = greater(radius * (1.0 + 0.35 * pushed(t, 2)) * flared, 0.75);
            }
        }
        let fattest = radii.iter().copied().fold(radius, greater);
        let mut bounds = Bounds::NONE;
        for point in &line {
            bounds = bounds.with(Bounds::round(*point, fattest + self.set.rough + 0.5));
        }
        let near = std::array::from_fn(|at| {
            let (a, b) = (line[at], line[at + 1]);
            let along = sub(b, a);
            let reach = dot(along, along).sqrt() * 0.5 + greater(radii[at], radii[at + 1]) + self.set.rough + 1.0;
            (add(a, times(along, 0.5)), reach * reach)
        });
        Tunnel { line, up, radius: fattest, radii, floor: radius * FLOOR, near, roofed, smooth: false, bounds }
    }
}

impl Tunnel {
    /// How far inside it `p` is, in metres: round, with a flat floor.
    fn inside(&self, p: V3) -> f32 {
        let mut hollow = NOTHING;
        for (at, piece) in self.line.windows(2).enumerate() {
            let from = sub(p, self.near[at].0);
            if dot(from, from) > self.near[at].1 {
                continue;
            }
            let (a, along) = (piece[0], sub(piece[1], piece[0]));
            let length = dot(along, along);
            let t = if length > 0.0 { dot(sub(p, a), along) / length } else { 0.0 };
            let t = greater(lesser(t, 1.0), 0.0);
            let away = sub(p, add(a, times(along, t)));
            let radius = self.radii[at] + (self.radii[at + 1] - self.radii[at]) * t;
            let round = radius - dot(away, away).sqrt();
            let floor = dot(away, self.up) + self.floor;
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

    /// The rooms and tunnels that reach over the column at `x` and `z`:
    /// rooms by their place, tunnels by theirs past the rooms, as
    /// [`Gathered::inside`] takes them.
    pub fn over(&self, x: f32, z: f32, list: &mut Vec<u16>) {
        list.clear();
        let holds = |bounds: &Bounds| {
            x >= bounds.low[0] && x <= bounds.high[0] && z >= bounds.low[2] && z <= bounds.high[2]
        };
        let boxes = self.rooms.iter().map(|room| &room.bounds).chain(self.tunnels.iter().map(|tunnel| &tunnel.bounds));
        list.extend(boxes.enumerate().filter(|(_, bounds)| holds(bounds)).map(|(at, _)| at as u16));
    }

    /// How far inside its hollows `p` is, in metres, for cells `size`
    /// metres across: inside what is kept under the ground's roof, then
    /// inside what is not (entrances). `only` picks which rooms and tunnels
    /// are looked at, rooms first, for what has many.
    pub fn inside(&self, net: &Network, p: V3, size: f32, only: Option<&[u16]>) -> (f32, f32) {
        if !self.bounds.is_some_and(|bounds| bounds.holds(p)) {
            return (NOTHING, NOTHING);
        }
        let rough = net.roughest;
        let plain = self.measured(net, p, None, only);
        // Roughness moves a wall no further than itself: further off than that and a cell, it changes nothing.
        let near = |hollow: f32| hollow.abs() <= rough + size;
        if rough <= 0.0 || !(near(plain.0) || near(plain.1)) {
            return plain;
        }
        self.measured(net, p, Some(net.noise(p)), only)
    }

    /// The same with its walls moved by `noise`, about -1 to 1, where it is given.
    fn measured(&self, net: &Network, p: V3, noise: Option<[f32; 2]>, only: Option<&[u16]>) -> (f32, f32) {
        let (mut roofed, mut open) = (NOTHING, NOTHING);
        let mut room = |room: &Placed| {
            if !room.bounds.holds(p) {
                return;
            }
            let from = sub(p, room.at);
            let own = [dot(from, room.axes[0]), dot(from, room.axes[1]), dot(from, room.axes[2])];
            let inside = net.room_inside(&room.plan, own, noise);
            // A mouth is open to the sky; every other room keeps under the ground's roof.
            match room.plan.mouth {
                true => open = greater(open, inside),
                false => roofed = greater(roofed, inside),
            }
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
            let moved = noise.filter(|_| !tunnel.smooth).map_or(0.0, |[lumps, detail]| {
                greater(lumps * net.set.rough + detail * 0.35, -tunnel.radius * 0.25)
            });
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
