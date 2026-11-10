//! The Cave Network node: caves made of rooms joined by winding tunnels,
//! as a Volume of how many metres inside one each place is. Taken out of
//! the ground by a Combine node's Subtract, as Tunnels and Chambers are.
//!
//! Its rooms are prefabs of Cave Shape and Cave Exit Objs, each kept here
//! as the numbers the editor read from it (see `caves::Room`); with none
//! listed it places a plain chamber. Laid out without end, a room to
//! every region `Spacing` metres across, `Depth` under the ground, some
//! with a way up to the surface; or as one level, a main path of rooms
//! with side rooms, `Depth` under the ground at its first room.
//!
//! Everything but an entrance keeps `Roof` metres under the ground wired
//! in: with nothing wired in, under the generator's own.
//!
//! Its sizes are in metres whatever the generator's scale, which stretches
//! the ground sideways only.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Solid, Value, ValueSpec, noise3d::seed_for};
use crate::generate::{
    caves::{CaveLayout, Network, Room, Settings},
    compile::{Cells, Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// A network of cave rooms and the tunnels between them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaveNetwork {
    pub layout: Laid,
    /// How far apart its rooms lie, in metres: the size of a region.
    pub spacing: f32,
    /// How far under the ground its rooms' floors lie, in metres.
    pub depth: f32,
    pub tunnel_width: f32,
    /// How far its tunnels wind from the straight way, 0 to 1.
    pub winding: f32,
    /// How far its walls are pushed in and out, in metres.
    pub roughness: f32,
    /// How many of an endless network's rooms have a way up to the
    /// surface, 0 to 1.
    pub entrances: f32,
    /// How many rooms a level's main path has, and how many lie off it.
    pub main_rooms: u32,
    pub side_rooms: u32,
    pub roof: f32,
    pub seed: u32,
    /// Its rooms: kept last, as a file wants tables after values.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rooms: Vec<Room>,
}

/// A network's layout, as a file names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Laid {
    #[default]
    Endless,
    Level,
}

impl Laid {
    pub const ALL: [Laid; 2] = [Laid::Endless, Laid::Level];
}

impl From<Laid> for CaveLayout {
    fn from(laid: Laid) -> Self {
        match laid {
            Laid::Endless => CaveLayout::Endless,
            Laid::Level => CaveLayout::Level,
        }
    }
}

impl Default for CaveNetwork {
    fn default() -> Self {
        Self {
            layout: Laid::Endless,
            spacing: 64.0,
            depth: 24.0,
            tunnel_width: 4.0,
            winding: 0.6,
            roughness: 0.8,
            entrances: 0.25,
            main_rooms: 8,
            side_rooms: 4,
            roof: 3.0,
            seed: 0,
            rooms: Vec::new(),
        }
    }
}

impl CaveNetwork {
    /// Its ground, among its inputs.
    pub const GROUND: usize = 0;

    /// Its values as its kernel reads them, seeded `seed`: its rooms that
    /// have a shape, or a plain chamber; its regions wide enough to hold
    /// the largest of them and a tunnel's bends.
    fn settings(&self, seed: u32) -> Settings {
        let mut rooms: Vec<Room> = self.rooms.iter().filter(|room| !room.shapes.is_empty()).cloned().collect();
        if rooms.is_empty() {
            rooms.push(Room::chamber());
        }
        let mut set = Settings {
            rooms,
            layout: self.layout.into(),
            spacing: self.spacing,
            depth: self.depth.max(0.0),
            radius: (self.tunnel_width * 0.5).clamp(0.75, 64.0),
            winding: self.winding.clamp(0.0, 1.0),
            rough: self.roughness.clamp(0.0, 16.0),
            roof: self.roof.max(0.0),
            entrances: self.entrances.clamp(0.0, 1.0),
            main: self.main_rooms.clamp(1, 64),
            side: self.side_rooms.min(64),
            seed,
        };
        set.spacing = set.spacing.max(2.0 * (set.reach() + set.stray()) + 4.0).min(100_000.0);
        set
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("layout", "Layout", &["Endless", "Level"]),
    ValueSpec::number("spacing", "Spacing", 8.0, 10000.0, "m"),
    ValueSpec::number("depth", "Depth", 0.0, 10000.0, "m"),
    ValueSpec::number("tunnel_width", "Tunnel Width", 1.5, 64.0, "m"),
    ValueSpec::number("winding", "Winding", 0.0, 1.0, ""),
    ValueSpec::number("roughness", "Roughness", 0.0, 16.0, "m"),
    ValueSpec::number("entrances", "Entrances", 0.0, 1.0, ""),
    ValueSpec::whole("main_rooms", "Main Rooms", 1, 64),
    ValueSpec::whole("side_rooms", "Side Rooms", 0, 64),
    ValueSpec::number("roof", "Roof", 0.0, 10000.0, "m").advanced(),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::volume("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::volume("caves", "Caves")];

impl Kind for CaveNetwork {
    const TITLE: &'static str = "Cave Network";
    const FOLDER: &'static str = "Voxels";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(Laid::ALL.iter().position(|&laid| laid == self.layout).unwrap_or(0)),
            1 => Value::Number(self.spacing),
            2 => Value::Number(self.depth),
            3 => Value::Number(self.tunnel_width),
            4 => Value::Number(self.winding),
            5 => Value::Number(self.roughness),
            6 => Value::Number(self.entrances),
            7 => Value::Whole(self.main_rooms),
            8 => Value::Whole(self.side_rooms),
            9 => Value::Number(self.roof),
            10 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.layout = Laid::ALL[choice.min(Laid::ALL.len() - 1)],
            (1, Value::Number(number)) => self.spacing = number,
            (2, Value::Number(number)) => self.depth = number,
            (3, Value::Number(number)) => self.tunnel_width = number,
            (4, Value::Number(number)) => self.winding = number,
            (5, Value::Number(number)) => self.roughness = number,
            (6, Value::Number(number)) => self.entrances = number,
            (7, Value::Whole(whole)) => self.main_rooms = whole,
            (8, Value::Whole(whole)) => self.side_rooms = whole,
            (9, Value::Number(number)) => self.roof = number,
            (10, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        let ground = cx.volume(node, Self::GROUND, at).unwrap_or_else(|| Solid::of_ground(cx, at));
        let set = self.settings(seed_for(cx, node, self.seed));
        let (heights, round, sea_level) = (cx.own_ground(), cx.round(), cx.world.sea_level);
        // Everything that tells this network from another: what its kernel is known by, and what is kept of it.
        let mut key = Key::default();
        key.name("cave network").whole(u32::from(round)).number(sea_level);
        key.whole(heights.hash as u32).whole((heights.hash >> 32) as u32);
        key.whole(set.layout as u32).number(set.spacing).number(set.depth).number(set.radius);
        key.number(set.winding).number(set.rough).number(set.roof).number(set.entrances);
        key.whole(set.main).whole(set.side).whole(set.seed).whole(set.rooms.len() as u32);
        for room in &set.rooms {
            room.key(&mut key);
        }
        let network = Arc::new(Network::new(set, heights, round, sea_level, key.hash()));
        cx.note_caves(network.clone());
        let size = cx.size();
        let mut inputs = vec![ground, at.x, at.y, at.z, size];
        if round {
            inputs.push(cx.radius());
        }
        cx.run(Hollow { network, key: key.bytes() }, &inputs)
    }
}

/// How far inside a network's hollows each cell is, from rows of how far
/// into the ground it is, its x, y and z, the cells' size, and round a
/// planet its radius.
struct Hollow {
    network: Arc<Network>,
    key: Vec<u8>,
}

impl std::fmt::Debug for Hollow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hollow({:#x})", self.network.id)
    }
}

impl Kernel for Hollow {
    fn key(&self, key: &mut Key) {
        key.raw(&self.key);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let net = &*self.network;
        // What the cell before read, while this one is in the same region of the same planet.
        let mut grid = None;
        let mut held = None;
        let mut lifted = None;
        for (at, out) in out.iter_mut().enumerate() {
            // A cell or more over the ground nothing changes what a cell holds. Under it and over the roof an
            // entrance may still come up, so those cells are asked.
            let (under, size) = (inputs[0][at] - net.set.roof, inputs[4][at]);
            if inputs[0][at] <= -size {
                *out = under;
                continue;
            }
            let p = [inputs[1][at], inputs[2][at], inputs[3][at]];
            let radius = if net.round { inputs[5][at] } else { 0.0 };
            let (roofed, open) = match &net.level {
                Some(level) => {
                    let lift = match lifted {
                        Some((planet, lift)) if planet == radius.to_bits() => lift,
                        _ => net.level_height(radius),
                    };
                    lifted = Some((radius.to_bits(), lift));
                    level.inside(net, [p[0], p[1] - lift, p[2]], size)
                }
                None => {
                    let own = match grid {
                        Some((planet, own)) if planet == radius.to_bits() => own,
                        _ => net.grid(radius),
                    };
                    grid = Some((radius.to_bits(), own));
                    let region = own.region(p);
                    let gathered = match held.take() {
                        Some((planet, known, gathered)) if (planet, known) == (radius.to_bits(), region) => gathered,
                        _ => net.gathered(&own, radius, region),
                    };
                    let inside = gathered.inside(net, p, size, None);
                    held = Some((radius.to_bits(), region, gathered));
                    inside
                }
            };
            let kept = if roofed < under { roofed } else { under };
            *out = if kept > open { kept } else { open };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Carried, Cells as Asked, Graph, NodeKind, WorldGen,
        caves::{PlacedRoom, RoomExit, RoomForm, RoomShape},
        nodes::{Combine, Combining, Constant, Noise, Output},
    };

    /// A generator whose ground is hills (or level rock `flat` metres
    /// high) with `caves` taken out of it.
    fn world(caves: CaveNetwork, flat: Option<f32>, seed: u32) -> WorldGen {
        let mut graph = Graph::default();
        let height = match flat {
            Some(height) => graph.add(NodeKind::Constant(Constant { value: height }), [0.0, 0.0]),
            None => {
                graph.add(NodeKind::Noise(Noise { size: 300.0, low: 20.0, high: 90.0, ..Noise::default() }), [0.0, 0.0])
            }
        };
        let solid = graph.add(NodeKind::Solid(Solid::default()), [200.0, 0.0]);
        let network = graph.add(NodeKind::CaveNetwork(caves), [200.0, 200.0]);
        let take = Combine { operation: Combining::Subtract, ..Combine::default() };
        let take = graph.add(NodeKind::Combine(take), [400.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [600.0, 0.0]);
        assert!(graph.connect(height, 0, solid, Solid::HEIGHT) && graph.connect(height, 0, output, Output::HEIGHT));
        assert!(graph.connect(solid, 0, take, Combine::A) && graph.connect(network, 0, take, Combine::B));
        assert!(graph.connect(take, 0, output, Output::VOLUME));
        let world = WorldGen::new(seed, 1.0, 0.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    /// How far into the ground each of `cells` is, asked all at once.
    fn asked(world: &WorldGen, cells: &[[f32; 3]], planet: Option<f32>) -> Vec<f32> {
        let volume = world.volume().expect("a Volume");
        let (mut inside, mut ground) = (vec![0.0; cells.len()], vec![0.0; cells.len()]);
        let [x, y, z] = [0, 1, 2].map(|axis| cells.iter().map(|cell| cell[axis]).collect::<Vec<f32>>());
        let carried: Carried = match planet {
            None => {
                let heights: Vec<f32> = cells.iter().map(|cell| world.height_at(cell[0], cell[2])).collect();
                volume.carried(&x, &z, &heights)
            }
            Some(radius) => {
                let feet: Vec<[f32; 3]> = cells
                    .iter()
                    .map(|cell| {
                        let far = (cell[0] * cell[0] + cell[1] * cell[1] + cell[2] * cell[2]).sqrt();
                        cell.map(|v| v / far * radius)
                    })
                    .collect();
                let heights: Vec<f32> = feet.iter().map(|&foot| world.height_round(foot)).collect();
                volume.carried_round(&feet, &heights, radius)
            }
        };
        let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
        let cells = Asked { x: &x, y: &y, z: &z, carried: &rows, size: 1.0 };
        match planet {
            None => volume.densities(cells, false, &mut inside, &mut ground),
            Some(radius) => volume.densities_round(cells, radius, false, &mut inside, &mut ground),
        }
        inside
    }

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    /// Cells of a metre through the box from `low`, `count` along each
    /// axis, `step` metres apart.
    fn grid(low: [f32; 3], count: [usize; 3], step: f32) -> Vec<[f32; 3]> {
        let mut cells = Vec::with_capacity(count[0] * count[1] * count[2]);
        for z in 0..count[2] {
            for x in 0..count[0] {
                for y in 0..count[1] {
                    cells.push([low[0] + x as f32 * step, low[1] + y as f32 * step, low[2] + z as f32 * step]);
                }
            }
        }
        cells
    }

    fn open(world: &WorldGen, at: [f32; 3], planet: Option<f32>) -> bool {
        asked(world, &[at], planet)[0] < 0.0
    }

    #[test]
    fn a_network_hollows_rooms_and_tunnels_under_the_ground_and_none_without_one() {
        let caves = world(CaveNetwork::default(), None, 3);
        let rooms = caves.caves().expect("its caves").rooms_near(None, [0.0, 40.0, 0.0], 400.0);
        assert!(rooms.len() > 20, "{} rooms within 400 m", rooms.len());
        for room in &rooms {
            let ground = caves.height_at(room.at[0], room.at[2]);
            let under = ground - room.at[1];
            assert!((17.0..=31.0).contains(&under), "a room {under} m under the ground");
            assert!(open(&caves, room.floor.map(|v| v + 0.5), None), "open over its floor at {:?}", room.floor);
            assert!(!open(&caves, [room.floor[0], room.floor[1] - 1.5, room.floor[2]], None), "rock under it");
            // Along a tunnel's exit and a little way out, it is open.
            for exit in room.exits.iter().filter(|exit| exit.leads_to.is_some()) {
                let out = [0, 1, 2].map(|axis| exit.at[axis] + exit.way[axis] * 1.5);
                assert!(open(&caves, out, None), "open out of an exit at {out:?}");
            }
        }
        // Entrances come out into the open: over this ground some places just under its top are hollow.
        let mut mouths = 0;
        for i in 0..40000 {
            let (x, z) = ((i % 200) as f32 * 2.0 - 200.0, (i / 200) as f32 * 2.0 - 200.0);
            mouths += usize::from(open(&caves, [x, caves.height_at(x, z) - 0.5, z], None));
        }
        assert!(mouths > 8, "{mouths} places open at the ground's top over {} rooms", rooms.len());
        // Nothing is hollow over the roof, where no entrance is.
        let plain = world(CaveNetwork { entrances: 0.0, ..CaveNetwork::default() }, None, 3);
        for i in 0..4000 {
            let (x, z) = (i as f32 * 3.7 - 500.0, i as f32 * 2.9 - 300.0);
            let ground = plain.height_at(x, z);
            for up in [2.5, 1.0] {
                assert!(!open(&plain, [x, ground - up, z], None), "open {up} m under the ground at {x}, {z}");
            }
        }
    }

    #[test]
    fn a_cell_alone_is_the_cell_in_a_batch_flat_and_round_a_planet_and_other_seeds_other_caves() {
        for layout in Laid::ALL {
            let made = world(CaveNetwork { layout, ..CaveNetwork::default() }, None, 9);
            let middle = made.height_at(30.0, 30.0) - 24.0;
            let cells = grid([-70.0, middle - 14.0, -70.0], [29, 15, 29], 5.0);
            let batch = asked(&made, &cells, None);
            assert!(batch.iter().any(|inside| *inside < 0.0), "{layout:?}: hollows among the cells");
            // One at a time, backwards: whatever was kept from another region, the bits are the same.
            for (at, cell) in cells.iter().enumerate().rev().step_by(7) {
                assert_eq!(asked(&made, &[*cell], None)[0].to_bits(), batch[at].to_bits(), "{layout:?} at {cell:?}");
            }
            assert_ne!(
                bits(&asked(&world(CaveNetwork { layout, ..CaveNetwork::default() }, None, 10), &cells, None)),
                bits(&batch)
            );
            // Round a planet 400 m from its middle to its sea: cells under its top and under one of its edges.
            let radius = 400.0;
            for toward in [[0.0, 1.0, 0.0], [0.72, 0.69, 0.05]] {
                let far = radius + 55.0 - 24.0;
                let cells: Vec<[f32; 3]> = grid([-60.0, -12.0, -60.0], [25, 13, 25], 5.0)
                    .into_iter()
                    .map(|cell| [0, 1, 2].map(|axis| toward[axis] * far + cell[axis]))
                    .collect();
                let batch = asked(&made, &cells, Some(radius));
                if layout == Laid::Endless || toward[1] == 1.0 {
                    assert!(batch.iter().any(|inside| *inside < 0.0), "{layout:?}: hollows toward {toward:?}");
                }
                for (at, cell) in cells.iter().enumerate().rev().step_by(11) {
                    let alone = asked(&made, &[*cell], Some(radius))[0];
                    assert_eq!(alone.to_bits(), batch[at].to_bits(), "{layout:?} round a planet at {cell:?}");
                }
            }
        }
    }

    #[test]
    fn rooms_and_tunnels_are_whole_wherever_regions_meet() {
        // Across the lines where regions meet, the hollow never ends at the line: no cell well inside a hollow
        // has rock a metre away across one, as it would were the hollow cut off there.
        let set = CaveNetwork { roughness: 0.0, entrances: 0.5, ..CaveNetwork::default() };
        let made = world(set, Some(80.0), 5);
        let (mut walls, mut seen) = (0, 0);
        for line in -3..=3 {
            let edge = line as f32 * 64.0;
            for along in 0..400 {
                for up in 0..40 {
                    let (t, y) = (along as f32 * 1.0 - 200.0, 40.0 + up as f32);
                    for (near, far) in
                        [([edge - 0.5, y, t], [edge + 0.5, y, t]), ([t, y, edge - 0.5], [t, y, edge + 0.5])]
                    {
                        let pair = asked(&made, &[near, far], None);
                        if pair[0] < -1.3 || pair[1] < -1.3 {
                            seen += 1;
                            walls += usize::from((pair[0] < 0.0) != (pair[1] < 0.0));
                        }
                    }
                }
            }
        }
        assert!(seen > 30, "{seen} open cells along the lines");
        assert_eq!(walls, 0, "of {seen} open cells along the lines, {walls} end at one");
    }

    /// The places open to walk from `from` through `made`'s hollows, in
    /// cells of a metre inside the box from `low` to `high`.
    fn flooded(made: &WorldGen, from: [f32; 3], low: [f32; 3], high: [f32; 3]) -> impl Fn([f32; 3]) -> bool {
        let count = [0, 1, 2].map(|axis| (high[axis] - low[axis]).ceil() as usize + 1);
        let cells = grid(low, [count[0], count[1], count[2]], 1.0);
        let inside = asked(made, &cells, None);
        let place = move |cell: [usize; 3]| (cell[2] * count[0] + cell[0]) * count[1] + cell[1];
        let cell_of = move |at: [f32; 3]| {
            [0, 1, 2].map(|axis| ((at[axis] - low[axis]).round().max(0.0) as usize).min(count[axis] - 1))
        };
        let mut reached = vec![false; cells.len()];
        let mut edge = vec![cell_of(from)];
        reached[place(edge[0])] = true;
        while let Some(cell) = edge.pop() {
            for axis in 0..3 {
                for step in [-1i32, 1] {
                    let mut next = cell;
                    let moved = next[axis] as i32 + step;
                    if moved < 0 || moved >= count[axis] as i32 {
                        continue;
                    }
                    next[axis] = moved as usize;
                    if !reached[place(next)] && inside[place(next)] < 0.0 {
                        reached[place(next)] = true;
                        edge.push(next);
                    }
                }
            }
        }
        move |at: [f32; 3]| reached[place(cell_of(at))]
    }

    fn over(room: &PlacedRoom) -> [f32; 3] {
        [room.floor[0], room.floor[1] + 1.0, room.floor[2]]
    }

    #[test]
    fn every_room_of_a_level_is_reached_from_the_first_through_its_tunnels() {
        for seed in [1, 2, 7, 40] {
            let set = CaveNetwork {
                layout: Laid::Level,
                main_rooms: 9,
                side_rooms: 5,
                depth: 60.0,
                ..CaveNetwork::default()
            };
            let made = world(set, Some(100.0), seed);
            let caves = made.caves().expect("its caves");
            assert_eq!(caves.layout(), CaveLayout::Level);
            let rooms = caves.rooms(None);
            assert_eq!(rooms.len(), 14, "seed {seed}");
            assert!(rooms[..9].iter().all(|room| room.main) && rooms[9..].iter().all(|room| !room.main));
            assert!((rooms[0].at[1] - 40.0).abs() < 1e-3, "the first room 60 m under ground 100 m high");
            let (mut low, mut high) = ([f32::MAX; 3], [f32::MIN; 3]);
            for room in &rooms {
                for axis in 0..3 {
                    (low[axis], high[axis]) =
                        (low[axis].min(room.bounds[0][axis]), high[axis].max(room.bounds[1][axis]));
                }
                // Each tunnel's far end knows this room: exits come in pairs.
                for exit in &room.exits {
                    let Some(other) = exit.leads_to_index else { continue };
                    let back = rooms[other].exits.iter().any(|exit| exit.leads_to_index == room.index);
                    assert!(back, "room {:?} leads to {other}, which does not lead back", room.index);
                }
            }
            let low = low.map(|v| v - 12.0);
            let reached = flooded(&made, over(&rooms[0]), low, high.map(|v| v + 12.0));
            for room in &rooms {
                assert!(reached(over(room)), "seed {seed}: room {:?} at {:?} is cut off", room.index, room.at);
            }
            // Bounded: nothing is open outside the box round its rooms and tunnels.
            for i in 0..2000 {
                let (x, z) = (low[0] - 3.0 - (i % 40) as f32 * 9.0, low[2] + (i / 40) as f32 * 11.0);
                assert!(!open(&made, [x, rooms[0].at[1] + 2.0, z], None));
            }
        }
    }

    #[test]
    fn a_network_reads_its_rooms_shapes_turned_and_mirrored_and_writes_them_to_its_file() {
        // A room of a long hall with one way out, at its far end: wherever it is placed, the hall is open
        // from its middle to its exit, and its exit faces a room it is joined to.
        let hall =
            RoomShape { shape: RoomForm::Box, size: [18.0, 5.0, 6.0], at: [5.0, 2.5, 0.0], ..RoomShape::default() };
        let room = Room {
            prefab: Some("0123456789abcdef0123456789abcdef".into()),
            weight: 2.0,
            shapes: vec![hall],
            exits: vec![RoomExit { at: [14.0, 1.5, 0.0], way: [1.0, 0.0, 0.0] }],
        };
        let set =
            CaveNetwork { layout: Laid::Level, roughness: 0.0, rooms: vec![room.clone()], ..CaveNetwork::default() };
        let made = world(set, Some(100.0), 4);
        let rooms = made.caves().unwrap().rooms(None);
        let mut turns = Vec::new();
        for placed in &rooms {
            assert_eq!(placed.prefab, room.prefab);
            turns.push((placed.turns, placed.mirrored));
            let along = |far: f32| {
                [0, 1, 2].map(|axis| placed.at[axis] + placed.axes[0][axis] * far + placed.axes[1][axis] * 2.5)
            };
            for far in [-3.0, 0.0, 6.0, 13.0] {
                assert!(open(&made, along(far), None), "the hall of {:?}, {far} m along it", placed.index);
            }
            assert!(!open(&made, along(-6.0), None), "rock behind the hall of {:?}", placed.index);
            assert_eq!(placed.exits.len(), 1);
            let exit = &placed.exits[0];
            assert!(exit.leads_to_index.is_some(), "an exit unused in a room joined to another");
        }
        // The last room is joined to one other alone, and its one exit faces it.
        let last = rooms.last().unwrap();
        let other = &rooms[last.exits[0].leads_to_index.unwrap()];
        let facing =
            last.exits[0].way[0] * (other.at[0] - last.at[0]) + last.exits[0].way[2] * (other.at[2] - last.at[2]);
        assert!(facing > 0.0, "the last room's exit faces away from the room it leads to");
        turns.dedup();
        assert!(turns.len() > 1, "rooms turned different ways: {turns:?}");
        let text = made.to_text();
        assert!(text.contains("kind = \"cave_network\"") && text.contains("[[nodes.rooms.shapes]]"), "{text}");
        let read = WorldGen::parse(&text).unwrap();
        assert_eq!(read, made);
        assert_eq!(read.hash(), made.hash());
        // A room changed is other ground; a room listed with nothing read from it yet is left out.
        let mut wider = made.graph().clone();
        for node in &mut wider.nodes {
            if let Some(rooms) = node.kind.rooms_mut() {
                rooms[0].shapes[0].size[2] = 7.0;
                rooms.push(Room { prefab: Some("ffff".into()), ..Room::default() });
            }
        }
        let wider = made.with_graph(wider);
        assert_ne!(wider.hash(), made.hash());
        assert!(wider.caves().unwrap().rooms(None).iter().all(|placed| placed.room == 0));
    }
}
