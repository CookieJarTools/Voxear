//! A world of blocks' own overhangs and caves, as the nodes that make
//! them: what such a world's ground is made from while its generator has
//! no Volume, and what Move to Generator writes into the generator's graph.
//!
//! The generator's own ground is made solid, pushed by an Overhangs node,
//! and has Tunnels and Chambers taken out of it:
//!
//! ```text
//! Solid ─ Overhangs ─┬──────────────────────── Combine (Subtract) ─ Output
//!                    ├─ Tunnels ──┬─ Combine (Union) ─┘
//!                    └─ Chambers ─┘
//! ```
//!
//! The nodes' noise is seeded as a world's own was, so moving a world's
//! numbers into its generator leaves its ground where it was.

use super::{
    graph::{Graph, NodeId},
    nodes::{Chambers, Combine, NodeKind, Output, Overhangs, Solid, Tunnels},
};

/// How much past a generator's seed a world of blocks' own overhangs,
/// tunnels and chambers are seeded.
const OVERHANGS_SEED: u32 = 41;
const TUNNELS_SEED: u32 = 53;
const CHAMBERS_SEED: u32 = 79;

/// A world of blocks' own overhangs and caves: how far its surface is
/// pushed in and out, in metres; how much of it is hollowed, from 0 for
/// none to 1; and how far under the surface its caves keep, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnVoxels {
    pub overhangs: f32,
    pub caves: f32,
    pub roof: f32,
}

impl Graph {
    /// Wires `own` into its Output's Volume as nodes, in place of whatever
    /// was wired there: false, and nothing done, for a graph with no Output.
    ///
    /// As `rows`, every node is added whatever its amount, and the amounts
    /// are shown as rows of the generator's own: Overhangs, Tunnels and
    /// Chambers. Otherwise a node whose amount is nothing is left out.
    pub fn add_voxels(&mut self, own: OwnVoxels, rows: bool) -> bool {
        let Some(output) = self.output().map(|node| node.id) else { return false };
        // Laid out in a row under everything there.
        let below = self.nodes.iter().map(|node| node.at[1]).fold(0.0, f32::max) + 280.0;
        let mut ground = self.add(NodeKind::Solid(Solid::default()), [0.0, below]);
        let mut wired = true;
        let mut wire = |graph: &mut Graph, from: NodeId, to: NodeId, input: usize| {
            wired &= graph.connect(from, 0, to, input);
        };
        if rows || own.overhangs > 0.0 {
            let node = Overhangs { amount: own.overhangs.max(0.0), seed: OVERHANGS_SEED, ..Overhangs::default() };
            let overhangs = self.add(NodeKind::Overhangs(node), [240.0, below]);
            wire(self, ground, overhangs, Overhangs::GROUND);
            ground = overhangs;
            if rows {
                self.expose(overhangs, Overhangs::AMOUNT, "Overhangs");
            }
        }
        let mut whole = ground;
        if rows || own.caves > 0.0 {
            let (amount, roof) = (own.caves.clamp(0.0, 1.0), own.roof.max(0.0));
            let tunnels = Tunnels { amount, roof, seed: TUNNELS_SEED, ..Tunnels::default() };
            let chambers = Chambers { amount, roof, seed: CHAMBERS_SEED, ..Chambers::default() };
            let tunnels = self.add(NodeKind::Tunnels(tunnels), [480.0, below + 120.0]);
            let chambers = self.add(NodeKind::Chambers(chambers), [480.0, below + 300.0]);
            let caves = self.add(NodeKind::Combine(Combine::default()), [720.0, below + 200.0]);
            let cut = self.add(NodeKind::Combine(Combine::subtract()), [960.0, below]);
            wire(self, ground, tunnels, Tunnels::GROUND);
            wire(self, ground, chambers, Chambers::GROUND);
            wire(self, tunnels, caves, Combine::A);
            wire(self, chambers, caves, Combine::B);
            wire(self, ground, cut, Combine::A);
            wire(self, caves, cut, Combine::B);
            whole = cut;
            if rows {
                self.expose(tunnels, Tunnels::AMOUNT, "Tunnels");
                self.expose(chambers, Chambers::AMOUNT, "Chambers");
            }
        }
        wire(self, whole, output, Output::VOLUME);
        debug_assert!(wired, "a world's own overhangs and caves wire up");
        wired
    }

    /// The nodes that make its Volume and nothing else: those its Output
    /// reads through its Volume, and not through its height or its climate.
    pub fn volume_nodes(&self) -> Vec<NodeId> {
        let Some(output) = self.output() else { return Vec::new() };
        let source = |input: usize| self.link_into(output.id, input).map(|link| link.from);
        let Some(volume) = source(Output::VOLUME) else { return Vec::new() };
        let ground: Vec<NodeId> =
            (0..output.kind.inputs().len()).filter(|&input| input != Output::VOLUME).filter_map(source).collect();
        let nodes = self.nodes.iter().map(|node| node.id);
        nodes.filter(|&node| self.reads(volume, node) && !ground.iter().any(|&from| self.reads(from, node))).collect()
    }

    /// The graph without the nodes that make its Volume alone, their wires
    /// and their rows: the ground's own graph.
    pub fn without_volume(&self) -> Graph {
        let mut ground = self.clone();
        for node in self.volume_nodes() {
            ground.remove(node);
        }
        ground
    }

    /// Takes `other`'s Volume for its own, in place of any it had: the
    /// nodes that make `other`'s Volume alone, under new numbers, laid out
    /// under everything here, with the wires between them, the wire into
    /// the Output and their rows. A wire into them from `other`'s ground is
    /// let go, as that ground is not here. False, and nothing done, where
    /// `other` has none or this graph no Output.
    pub fn take_volume(&mut self, other: &Graph) -> bool {
        let moved = other.volume_nodes();
        let (Some(output), Some(theirs)) = (self.output().map(|node| node.id), other.output().map(|node| node.id))
        else {
            return false;
        };
        if moved.is_empty() {
            return false;
        }
        for node in self.volume_nodes() {
            self.remove(node);
        }
        self.disconnect(output, Output::VOLUME);
        let places = || moved.iter().filter_map(|&node| other.node(node));
        let top = places().map(|node| node.at[1]).fold(f32::MAX, f32::min);
        let below = self.nodes.iter().map(|node| node.at[1]).fold(0.0, f32::max) + 280.0;
        let mut numbers = vec![(theirs, output)];
        for node in places() {
            numbers.push((node.id, self.add(node.kind.clone(), [node.at[0], node.at[1] - top + below])));
        }
        let now = |was: NodeId| numbers.iter().find(|(old, _)| *old == was).map(|(_, new)| *new);
        for link in other.links.iter().filter(|link| moved.contains(&link.from)) {
            if let (Some(from), Some(to)) = (now(link.from), now(link.to)) {
                self.connect(from, link.output, to, link.input);
            }
        }
        for shown in other.exposed.iter().filter(|shown| moved.contains(&shown.node)) {
            if let Some(node) = now(shown.node) {
                self.expose(node, shown.value, &shown.label);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Cells, Value, WorldGen,
        nodes::{Combining, Constant, Noise, Noise3d},
    };

    const SEA: f32 = 5.0;

    /// Hills of noise wired to an Output, whose height `high` sets: the node making them, and the Output.
    fn hills(high: f32) -> (Graph, NodeId, NodeId) {
        let mut graph = Graph::default();
        let noise = Noise { size: 60.0, octaves: 1, high, ..Noise::default() };
        let ground = graph.add(NodeKind::Noise(noise), [0.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [600.0, 0.0]);
        assert!(graph.connect(ground, 0, output, Output::HEIGHT));
        (graph, ground, output)
    }

    fn world(graph: Graph) -> WorldGen {
        let world = WorldGen::new(3, 1.0, SEA, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        world
    }

    /// A block of cells through the hills' ground, a column after another.
    fn cells() -> Vec<[f32; 3]> {
        (0..40)
            .flat_map(|i| (0..60).map(move |j| [i as f32 * 7.5 - 90.0, j as f32 * 1.5 - 25.0, i as f32 * 3.1]))
            .collect()
    }

    /// How far into `world`'s ground each of `cells` is, of `size` metres, and the same with the caves left whole.
    fn asked(world: &WorldGen, cells: &[[f32; 3]], size: f32, coarse: bool) -> (Vec<f32>, Vec<f32>) {
        let volume = world.volume().expect("a Volume");
        let [x, y, z] = [0, 1, 2].map(|axis| cells.iter().map(|cell| cell[axis]).collect::<Vec<f32>>());
        let mut heights = vec![0.0; cells.len()];
        world.heights(&x, &z, &mut heights);
        let carried = volume.carried(&x, &z, &heights);
        let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
        let (mut inside, mut ground) = (vec![0.0; cells.len()], vec![0.0; cells.len()]);
        volume.densities(Cells { x: &x, y: &y, z: &z, carried: &rows, size }, coarse, &mut inside, &mut ground);
        (inside, ground)
    }

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    /// The hills made solid: the Solid node, wired to the Output's Volume.
    fn solid_hills(high: f32) -> (Graph, NodeId, NodeId) {
        let (mut graph, _, output) = hills(high);
        let solid = graph.add(NodeKind::Solid(Solid::default()), [200.0, 200.0]);
        assert!(graph.connect(solid, 0, output, Output::VOLUME));
        (graph, solid, output)
    }

    #[test]
    fn solid_is_how_far_under_a_height_each_cell_is_flat_and_round() {
        // With nothing wired in, the generator's own ground.
        let (graph, solid, output) = solid_hills(40.0);
        let made = world(graph.clone());
        let volume = made.volume().unwrap();
        assert!(volume.plain() && volume.reach() == 0.0);
        let cells = cells();
        let (inside, ground) = asked(&made, &cells, 1.0, false);
        let under: Vec<f32> = cells.iter().map(|&[x, y, z]| made.height_at(x, z) - y).collect();
        assert_eq!(bits(&inside), bits(&under));
        assert_eq!(bits(&ground), bits(&under), "no caves in it");
        assert!(under.iter().any(|&v| v > 1.0) && under.iter().any(|&v| v < -1.0));
        // Round a planet the surface lies as high over the planet's sea as the height is over the sea level.
        let radius = 300.0;
        let on = [[180.0, -200.0, 133.0], [0.0, 300.0, 0.0], [-290.0, 20.0, 70.0]];
        for at in on {
            let far = at.iter().map(|v| v * v).sum::<f32>().sqrt();
            let surface = at.map(|v| v / far * radius);
            let height = made.height_round(surface);
            let carried = volume.carried_round(&[surface], &[height], radius);
            let (mut inside, mut ground) = ([0.0], [0.0]);
            let cell = Cells { x: &at[0..1], y: &at[1..2], z: &at[2..3], carried: &carried.column(0), size: 1.0 };
            volume.densities_round(cell, radius, false, &mut inside, &mut ground);
            let expected = radius + height - SEA - (at[0] * at[0] + at[1] * at[1] + at[2] * at[2]).sqrt();
            assert_eq!(inside[0].to_bits(), expected.to_bits(), "at {at:?}");
        }
        // The same height wired in by hand is the same ground; another height is its own, and no plain ground.
        let ground_from = graph.link_into(output, Output::HEIGHT).unwrap().from;
        let mut wired = graph.clone();
        assert!(wired.connect(ground_from, 0, solid, Solid::HEIGHT));
        let wired = world(wired);
        assert!(wired.volume().unwrap().plain());
        assert_eq!(bits(&asked(&wired, &cells, 1.0, false).0), bits(&under));
        let mut other = graph;
        let level = other.add(NodeKind::Constant(Constant { value: 12.0 }), [0.0, 300.0]);
        assert!(other.connect(level, 0, solid, Solid::HEIGHT));
        let other = world(other);
        assert!(!other.volume().unwrap().plain());
        let (inside, _) = asked(&other, &cells, 1.0, false);
        assert_eq!(bits(&inside), bits(&cells.iter().map(|cell| 12.0 - cell[1]).collect::<Vec<_>>()));
    }

    #[test]
    fn noise_3d_is_noise_through_space_between_its_low_and_high() {
        let (mut graph, _, output) = hills(40.0);
        let noise = Noise3d { size: 25.0, low: -3.0, high: 9.0, ..Noise3d::default() };
        let rock = graph.add(NodeKind::Noise3d(noise.clone()), [0.0, 300.0]);
        assert!(graph.connect(rock, 0, output, Output::VOLUME));
        let made = world(graph.clone());
        let cells = cells();
        let (inside, _) = asked(&made, &cells, 1.0, false);
        assert!(inside.iter().all(|v| (-3.0..=9.0).contains(v)));
        let (least, most) = inside.iter().fold((f32::MAX, f32::MIN), |(low, high), &v| (low.min(v), high.max(v)));
        assert!(least < 1.5 && most > 4.5, "from {least} to {most}");
        // It changes up a column as it does along the ground, and is the same each time.
        assert!(inside[..60].windows(2).any(|pair| pair[0] != pair[1]));
        assert_eq!(bits(&asked(&made, &cells, 1.0, false).0), bits(&inside));
        // Solid at any height, so read as far over the ground as a Volume ever is.
        assert_eq!(made.volume().unwrap().reach(), 64.0);
        // Another node's noise is its own; so is another seed's.
        let mut again = graph.clone();
        let second = again.add(NodeKind::Noise3d(noise), [0.0, 500.0]);
        assert!(again.connect(second, 0, output, Output::VOLUME));
        assert_ne!(bits(&asked(&world(again), &cells, 1.0, false).0), bits(&inside));
        assert_ne!(bits(&asked(&made.with_seed(4), &cells, 1.0, false).0), bits(&inside));
        // The generator's scale stretches the ground, not what is in it.
        assert_eq!(bits(&asked(&made.with_scale(2.0), &cells, 1.0, false).0), bits(&inside));
    }

    /// The hills made solid and pushed by an Overhangs node: the graph, and the node.
    fn leaning(high: f32, overhangs: Overhangs) -> (Graph, NodeId) {
        let (mut graph, solid, output) = solid_hills(high);
        let node = graph.add(NodeKind::Overhangs(overhangs), [400.0, 200.0]);
        assert!(graph.connect(solid, 0, node, Overhangs::GROUND) && graph.connect(node, 0, output, Output::VOLUME));
        (graph, node)
    }

    #[test]
    fn overhangs_push_steep_ground_and_leave_gentle_ground_and_far_cells_alone() {
        let overhangs = Overhangs { amount: 6.0, size: 14.0, octaves: 2, seed: 0 };
        // Hills up to 150 m high and 60 m across: cliffs in places, level ground in others.
        let (graph, node) = leaning(150.0, overhangs.clone());
        let made = world(graph.clone());
        let volume = made.volume().unwrap();
        assert!(!volume.plain() && volume.reach() == 6.0);
        let plain = world(solid_hills(150.0).0);
        let cells: Vec<[f32; 3]> = (0..3000)
            .map(|i| {
                let (x, z) = ((i % 60) as f32 * 4.3, (i / 60) as f32 * 5.9);
                [x, plain.height_at(x, z) + (i % 7) as f32 - 3.0, z]
            })
            .collect();
        let (pushed, _) = asked(&made, &cells, 1.0, false);
        let (was, _) = asked(&plain, &cells, 1.0, false);
        let climb = |x: f32, z: f32| {
            let along = plain.height_at(x + 5.6, z) - plain.height_at(x - 5.6, z);
            let across = plain.height_at(x, z + 5.6) - plain.height_at(x, z - 5.6);
            (along * along + across * across).sqrt() / 11.2
        };
        let (mut left, mut moved, mut most) = (0, 0, 0.0f32);
        for (at, cell) in cells.iter().enumerate() {
            let (steep, by) = (climb(cell[0], cell[2]), pushed[at] - was[at]);
            most = most.max(by.abs());
            if steep < 0.3 {
                assert_eq!(by, 0.0, "gentle ground pushed at {cell:?}, climbing {steep}");
                left += 1;
            }
            moved += usize::from(steep > 1.5 && by.abs() > 0.5);
        }
        assert!(left > 30 && moved > 100, "{left} cells of gentle ground, {moved} of cliffs pushed");
        assert!(most <= 6.0 && most > 2.0, "pushed {most} m at most");
        // Level ground is not pushed at all, however far it could be.
        let (mut flat, _) = leaning(0.0, overhangs.clone());
        assert_eq!(
            bits(&asked(&world(flat.clone()), &cells, 1.0, false).0),
            bits(&asked(&world(solid_hills(0.0).0), &cells, 1.0, false).0)
        );
        // Through a mask wired in it pushes where the mask says: everywhere, or nowhere.
        let mask = flat.add(NodeKind::Constant(Constant { value: 1.0 }), [200.0, 400.0]);
        let node_flat = flat.nodes.iter().find(|node| matches!(node.kind, NodeKind::Overhangs(_))).unwrap().id;
        assert!(flat.connect(mask, 0, node_flat, Overhangs::MASK));
        let level: Vec<[f32; 3]> = cells.iter().map(|cell| [cell[0], (cell[1] % 3.0) - 1.0, cell[2]]).collect();
        let everywhere = asked(&world(flat.clone()), &level, 1.0, false).0;
        let unpushed = asked(&world(solid_hills(0.0).0), &level, 1.0, false).0;
        assert!(everywhere.iter().zip(&unpushed).filter(|(pushed, was)| pushed != was).count() > 1000);
        flat.node_mut(mask).unwrap().kind.set_value(0, Value::Number(0.0));
        assert_eq!(bits(&asked(&world(flat), &level, 1.0, false).0), bits(&unpushed));
        // Cells too large to hold its bulges are left as they were; smaller ones hold the larger octave alone.
        assert_eq!(bits(&asked(&made, &cells, 8.0, true).0), bits(&asked(&plain, &cells, 8.0, true).0));
        let half = asked(&made, &cells, 4.0, false).0;
        assert_ne!(bits(&half), bits(&pushed));
        assert_ne!(bits(&half), bits(&was));
        // Pushing by nothing is the ground as it was, plain still; and a seed of its own moves the bulges.
        let mut none = graph.clone();
        none.node_mut(node).unwrap().kind.set_value(Overhangs::AMOUNT, Value::Number(0.0));
        let none = world(none);
        assert!(none.volume().unwrap().plain());
        assert_eq!(bits(&asked(&none, &cells, 1.0, false).0), bits(&was));
        let mut seeded = graph;
        seeded.node_mut(node).unwrap().kind.set_value(3, Value::Whole(41));
        assert_ne!(bits(&asked(&world(seeded), &cells, 1.0, false).0), bits(&pushed));
        // Round a planet the same ground leans too, no further than its amount.
        let radius = 500.0;
        let on: Vec<[f32; 3]> = (0..400)
            .map(|i| {
                let way =
                    [(i as f32 * 0.37).fract() - 0.5, (i as f32 * 0.61).fract() - 0.5, (i as f32 * 0.83).fract() - 0.4];
                let long = way.iter().map(|v| v * v).sum::<f32>().sqrt();
                way.map(|v| v / long * radius)
            })
            .collect();
        let round = |world: &WorldGen| -> Vec<f32> {
            let volume = world.volume().unwrap();
            let mut heights = vec![0.0; on.len()];
            world.heights_round(&on, &mut heights);
            let carried = volume.carried_round(&on, &heights, radius);
            let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
            // Each cell on its column's own surface.
            let out: Vec<[f32; 3]> =
                (0..on.len()).map(|i| on[i].map(|v| v / radius * (radius + heights[i] - SEA))).collect();
            let [x, y, z] = [0, 1, 2].map(|axis| out.iter().map(|cell| cell[axis]).collect::<Vec<f32>>());
            let (mut inside, mut ground) = (vec![0.0; on.len()], vec![0.0; on.len()]);
            volume.densities_round(
                Cells { x: &x, y: &y, z: &z, carried: &rows, size: 1.0 },
                radius,
                false,
                &mut inside,
                &mut ground,
            );
            inside
        };
        let (leaning, standing) = (round(&made), round(&plain));
        let by: Vec<f32> = leaning.iter().zip(&standing).map(|(pushed, was)| (pushed - was).abs()).collect();
        assert!(by.iter().all(|by| *by <= 6.0) && by.iter().any(|by| *by > 0.5) && by.contains(&0.0));
    }

    /// The hills made solid with caves of `kind` taken out of them: the graph, and the caves' node.
    fn hollowed(kind: NodeKind) -> (Graph, NodeId) {
        let (mut graph, solid, output) = solid_hills(40.0);
        let caves = graph.add(kind, [400.0, 300.0]);
        let cut = graph.add(NodeKind::Combine(Combine::subtract()), [600.0, 200.0]);
        assert!(graph.connect(solid, 0, caves, 0) && graph.connect(solid, 0, cut, Combine::A));
        assert!(graph.connect(caves, 0, cut, Combine::B) && graph.connect(cut, 0, output, Output::VOLUME));
        (graph, caves)
    }

    #[test]
    fn tunnels_and_chambers_hollow_the_ground_under_their_roof() {
        let kinds = [
            NodeKind::Tunnels(Tunnels { amount: 0.6, roof: 4.0, ..Tunnels::default() }),
            NodeKind::Chambers(Chambers { amount: 0.6, roof: 4.0, ..Chambers::default() }),
        ];
        // Cells from over the hills down to 90 m under the sea.
        let cells: Vec<[f32; 3]> = (0..60)
            .flat_map(|i| (0..130).map(move |j| [i as f32 * 5.5, 40.0 - j as f32, i as f32 * 2.9 + 11.0]))
            .collect();
        let plain = world(solid_hills(40.0).0);
        let (was, _) = asked(&plain, &cells, 1.0, false);
        for kind in kinds {
            let name = kind.title();
            let (graph, node) = hollowed(kind.clone());
            let made = world(graph.clone());
            assert!(made.volume().unwrap().plain(), "{name}: the ground under the caves is the height's");
            let (inside, ground) = asked(&made, &cells, 1.0, false);
            assert_eq!(bits(&ground), bits(&was), "{name}: the ground with its caves left whole");
            let hollow =
                |inside: &[f32]| inside.iter().zip(&was).filter(|(inside, was)| **inside <= 0.0 && **was > 0.0).count();
            let caves = hollow(&inside);
            let of = was.iter().filter(|was| **was > 0.0).count();
            assert!(caves > of / 200 && caves < of / 3, "{name}: {caves} of {of} cells of ground hollow");
            // Nothing within the roof of the surface is hollow, and the air is the air it was.
            for (at, (&inside, &was)) in inside.iter().zip(&was).enumerate() {
                assert!(inside <= was, "{name}: nothing added");
                assert!(was > 4.0 || (inside > 0.0) == (was > 0.0), "{name}: hollow {was} m under the surface at {at}");
            }
            // A coarse asking leaves the caves out.
            assert_eq!(bits(&asked(&made, &cells, 4.0, true).0), bits(&was));
            // More of them with a greater amount, and none at nothing.
            let amount = |amount: f32| {
                let mut graph = graph.clone();
                graph.node_mut(node).unwrap().kind.set_value(0, Value::Number(amount));
                asked(&world(graph), &cells, 1.0, false).0
            };
            assert!(hollow(&amount(1.0)) > caves && hollow(&amount(0.2)) < caves, "{name}");
            assert_eq!(bits(&amount(0.0)), bits(&was), "{name}");
            // A seed of its own puts them elsewhere; another node's are its own too.
            let mut seeded = graph.clone();
            seeded.node_mut(node).unwrap().kind.set_value(3, Value::Whole(9));
            assert_ne!(bits(&asked(&world(seeded), &cells, 1.0, false).0), bits(&inside), "{name}");
            let mut again = graph.clone();
            let cut = again.links.iter().find(|link| link.from == node).unwrap().to;
            let second = again.add(kind, [400.0, 500.0]);
            let solid = again.link_into(node, 0).unwrap().from;
            assert!(again.connect(solid, 0, second, 0) && again.connect(second, 0, cut, Combine::B));
            assert_ne!(bits(&asked(&world(again), &cells, 1.0, false).0), bits(&inside), "{name}");
        }
    }

    #[test]
    fn combine_joins_two_volumes_takes_one_from_the_other_or_runs_them_together() {
        // Ground 10 m high and ground `second` m high, combined: a cell `y` m up is 10 - y into one, and
        // `second` - y into the other.
        let combined = |operation: Combining, second: f32, y: f32| {
            let (mut graph, _, output) = hills(40.0);
            let solid = |graph: &mut Graph, height: f32| {
                let level = graph.add(NodeKind::Constant(Constant { value: height }), [0.0, 300.0]);
                let solid = graph.add(NodeKind::Solid(Solid::default()), [200.0, 300.0]);
                assert!(graph.connect(level, 0, solid, Solid::HEIGHT));
                solid
            };
            let (a, b) = (solid(&mut graph, 10.0), solid(&mut graph, second));
            let combine = graph.add(NodeKind::Combine(Combine { operation, blend: 4.0 }), [400.0, 300.0]);
            assert!(graph.connect(a, 0, combine, Combine::A) && graph.connect(b, 0, combine, Combine::B));
            assert!(graph.connect(combine, 0, output, Output::VOLUME));
            let made = world(graph);
            let (inside, ground) = asked(&made, &[[5.0, y, 5.0]], 1.0, false);
            let (coarse, _) = asked(&made, &[[5.0, y, 5.0]], 1.0, true);
            (inside[0], ground[0], coarse[0])
        };
        // Solid wherever either is.
        assert_eq!(combined(Combining::Union, 4.0, 6.0), (4.0, 4.0, 4.0));
        assert_eq!(combined(Combining::Union, 14.0, 12.0), (2.0, 2.0, 2.0));
        // B taken out of A: solid above B's ground and under A's; what is taken is a cave, left whole in the
        // ground under the caves and at coarse levels.
        assert_eq!(combined(Combining::Subtract, 4.0, 6.0), (2.0, 4.0, 4.0));
        assert_eq!(combined(Combining::Subtract, 4.0, 3.0), (-1.0, 7.0, 7.0));
        assert_eq!(combined(Combining::Subtract, 4.0, 11.0), (-1.0, -1.0, -1.0));
        // Run together: the union where the two are further apart than the blend, a little more where nearer.
        assert_eq!(combined(Combining::Smooth, 4.0, 6.0).0, 4.0);
        assert_eq!(combined(Combining::Smooth, 8.0, 6.0).0, 4.25);
        assert_eq!(combined(Combining::Smooth, 10.0, 9.0).0, 2.0);
        // With one input wired it is that Volume; a Subtract with nothing to take from is nothing.
        let (mut graph, solid, output) = solid_hills(40.0);
        let lone = graph.add(NodeKind::Combine(Combine::default()), [400.0, 300.0]);
        assert!(graph.connect(solid, 0, lone, Combine::B) && graph.connect(lone, 0, output, Output::VOLUME));
        let cells = cells();
        let plain = asked(&world(solid_hills(40.0).0), &cells, 1.0, false).0;
        assert_eq!(bits(&asked(&world(graph.clone()), &cells, 1.0, false).0), bits(&plain));
        graph.node_mut(lone).unwrap().kind.set_value(0, Value::Choice(1));
        assert!(asked(&world(graph), &cells, 1.0, false).0.iter().all(|inside| *inside < -1.0e6));
    }

    #[test]
    fn a_worlds_own_overhangs_and_caves_wire_into_a_graph_as_nodes_and_rows() {
        let (graph, _, output) = hills(40.0);
        let own = OwnVoxels { overhangs: 5.0, caves: 0.4, roof: 6.0 };
        // As rows: every node, each amount shown on the generator.
        let mut shown = graph.clone();
        assert!(shown.add_voxels(own, true));
        let labels: Vec<&str> = shown.exposed.iter().map(|exposed| exposed.label.as_str()).collect();
        assert_eq!(labels, ["Overhangs", "Tunnels", "Chambers"]);
        let values: Vec<Option<Value>> = (0..3).map(|row| shown.exposed_value(row)).collect();
        assert_eq!(values, [Some(Value::Number(5.0)), Some(Value::Number(0.4)), Some(Value::Number(0.4))]);
        assert!(shown.link_into(output, Output::VOLUME).is_some());
        // Without rows it makes the same ground, to the bit.
        let mut plain = graph.clone();
        assert!(plain.add_voxels(own, false));
        assert!(plain.exposed.is_empty());
        let cells = cells();
        let (inside, ground) = asked(&world(shown.clone()), &cells, 1.0, false);
        assert_eq!(bits(&asked(&world(plain), &cells, 1.0, false).0), bits(&inside));
        assert!(inside.iter().zip(&ground).any(|(inside, ground)| inside < ground), "caves in it");
        // With no overhangs and no caves it is the generator's ground made solid, and as rows the nodes are
        // there to be turned up.
        let none = OwnVoxels { overhangs: 0.0, caves: 0.0, roof: 6.0 };
        let (mut bare, mut rows) = (graph.clone(), graph.clone());
        assert!(bare.add_voxels(none, false) && rows.add_voxels(none, true));
        assert_eq!(bare.nodes.len(), graph.nodes.len() + 1, "a Solid node alone");
        assert_eq!(rows.nodes.len(), graph.nodes.len() + 6);
        let solid = asked(&world(solid_hills(40.0).0), &cells, 1.0, false).0;
        for made in [world(bare), world(rows.clone())] {
            assert!(made.volume().unwrap().plain());
            assert_eq!(bits(&asked(&made, &cells, 1.0, false).0), bits(&solid));
        }
        let turned_up = world(rows).with_exposed(1, Value::Number(0.8)).expect("the Tunnels row");
        assert_ne!(bits(&asked(&turned_up, &cells, 1.0, false).0), bits(&solid));
        // Written to a file and read back, the nodes and rows are as they were.
        let made = world(shown);
        let text = made.to_text();
        for kind in ["solid", "overhangs", "tunnels", "chambers", "combine"] {
            assert!(text.contains(&format!("kind = \"{kind}\"")), "{kind} in {text}");
        }
        assert_eq!(WorldGen::parse(&text).unwrap(), made);
        // A graph with no Output has nothing to wire them into.
        assert!(!Graph::default().add_voxels(own, true));
    }

    #[test]
    fn a_starter_with_overhangs_and_caves_is_that_starter_still_and_another_picked_keeps_them() {
        let starters = crate::generate::starters();
        let (hills, canyons) = (&starters[1], &starters[4]);
        let mut graph = hills.graph.clone();
        assert!(graph.add_voxels(OwnVoxels { overhangs: 9.0, caves: 0.3, roof: 3.0 }, true));
        let made = WorldGen::new(5, 2.0, hills.sea_level, graph.clone());
        assert_eq!(made.starter(), Some("Rolling Hills"));
        assert_eq!(graph.volume_nodes().len(), 6);
        assert_eq!(graph.without_volume(), hills.graph, "the ground's graph is the starter's");
        // A wire of its own in the ground is a graph of its own, with or without a Volume.
        let mut own = graph.clone();
        let output = own.output().unwrap().id;
        assert!(own.disconnect(output, Output::HEIGHT));
        assert_eq!(made.with_graph(own).starter(), None);
        // Another starter picked: its ground, with the same nodes making the Volume, their rows after its own.
        let picked = canyons.reshapes(&made);
        assert!(picked.faults().is_empty(), "{:?}", picked.faults());
        assert_eq!((picked.starter(), picked.seed(), picked.scale()), (Some("Canyons"), 5, 2.0));
        assert_eq!(picked.sea_level(), canyons.sea_level);
        let rows = |world: &WorldGen| -> Vec<(String, Option<Value>)> {
            let graph = world.graph();
            (0..graph.exposed.len()).map(|row| (graph.exposed[row].label.clone(), graph.exposed_value(row))).collect()
        };
        let (before, after) = (rows(&made), rows(&picked));
        assert_eq!(after[..canyons.graph.exposed.len()], rows(&canyons.world(5, 2.0))[..]);
        assert_eq!(after[canyons.graph.exposed.len()..], before[hills.graph.exposed.len()..]);
        assert_eq!(after.last(), Some(&("Chambers".to_string(), Some(Value::Number(0.3)))));
        let kinds = |graph: &Graph| -> Vec<NodeKind> {
            graph.volume_nodes().into_iter().filter_map(|node| graph.node(node)).map(|node| node.kind.clone()).collect()
        };
        assert_eq!(kinds(picked.graph()), kinds(&graph));
        let volume = picked.volume().expect("its Volume came with it");
        assert!(!volume.plain() && volume.caved());
        // The nodes lie under the starter's own, and a starter picked for a generator with no Volume has none.
        let lowest = canyons.graph.nodes.iter().map(|node| node.at[1]).fold(0.0, f32::max);
        let moved = picked.graph().volume_nodes();
        assert!(moved.iter().all(|&node| picked.graph().node(node).unwrap().at[1] > lowest));
        assert!(canyons.reshapes(&hills.world(1, 1.0)).volume().is_none());
    }

    #[test]
    fn a_starters_ground_takes_overhangs_and_caves_flat_and_round_and_a_cell_alone_is_the_cell_in_a_batch() {
        // Each starter's graph under the Volume's nodes: the Overhangs' own mask reads the ground again round
        // every column, through the starter's warps, masks and gullies.
        // The five that are ground alone: the cave level has a Volume of its own.
        for starter in crate::generate::starters().into_iter().take(5) {
            let name = starter.name;
            let mut graph = starter.graph.clone();
            assert!(graph.add_voxels(OwnVoxels { overhangs: 6.0, caves: 0.5, roof: 3.0 }, false), "{name}");
            let made = WorldGen::new(7, 1.0, starter.sea_level, graph);
            assert!(made.faults().is_empty(), "{name}: {:?}", made.faults());
            let volume = made.volume().expect("a Volume");
            assert!(!volume.plain() && volume.caved() && volume.reach() >= 6.0, "{name}");
            // Flat: columns of cells through the ground, from 30 m under it to 15 m over it.
            let cells: Vec<[f32; 3]> = (0..10)
                .flat_map(|i| {
                    let (x, z) = (i as f32 * 137.5 - 600.0, i as f32 * 71.3 + 40.0);
                    let ground = made.height_at(x, z);
                    (0..30).map(move |j| [x, ground + j as f32 * 1.5 - 30.0, z])
                })
                .collect();
            let (inside, ground) = asked(&made, &cells, 1.0, false);
            for (at, cell) in cells.iter().enumerate() {
                let (one, one_ground) = asked(&made, &[*cell], 1.0, false);
                assert_eq!(one[0].to_bits(), inside[at].to_bits(), "{name} at {cell:?}");
                assert_eq!(one_ground[0].to_bits(), ground[at].to_bits(), "{name}, its ground at {cell:?}");
            }
            for found in [&inside, &ground] {
                assert!(found.iter().any(|&v| v > 1.0) && found.iter().any(|&v| v < -1.0), "{name}: ground and air");
            }
            // Round a planet: cells along ways out from its middle, about its surface.
            let radius = 400.0;
            let ways: Vec<[f32; 3]> = (0..60)
                .map(|i| {
                    let i = i as f32;
                    let way = [(i * 0.37).fract() - 0.5, (i * 0.61).fract() - 0.5, (i * 0.83).fract() - 0.4];
                    let long = way.iter().map(|v| v * v).sum::<f32>().sqrt();
                    way.map(|v| v / long)
                })
                .collect();
            let on: Vec<[f32; 3]> = ways.iter().map(|way| way.map(|v| v * radius)).collect();
            let mut heights = vec![0.0; on.len()];
            made.heights_round(&on, &mut heights);
            let carried = volume.carried_round(&on, &heights, radius);
            let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
            let out = |i: usize| radius + heights[i] - made.sea_level() + 7.0 - (i % 19) as f32;
            let [x, y, z] = [0, 1, 2].map(|axis| (0..on.len()).map(|i| ways[i][axis] * out(i)).collect::<Vec<f32>>());
            let (mut inside, mut ground) = (vec![0.0; on.len()], vec![0.0; on.len()]);
            let all = Cells { x: &x, y: &y, z: &z, carried: &rows, size: 1.0 };
            volume.densities_round(all, radius, false, &mut inside, &mut ground);
            for i in 0..on.len() {
                let (mut one, mut one_ground) = ([0.0], [0.0]);
                let cell = Cells { x: &x[i..=i], y: &y[i..=i], z: &z[i..=i], carried: &carried.column(i), size: 1.0 };
                volume.densities_round(cell, radius, false, &mut one, &mut one_ground);
                assert_eq!(one[0].to_bits(), inside[i].to_bits(), "{name}, round a planet at {i}");
                assert_eq!(one_ground[0].to_bits(), ground[i].to_bits(), "{name}, its ground round a planet at {i}");
            }
            assert!(inside.iter().any(|&v| v > 0.0) && inside.iter().any(|&v| v < 0.0), "{name}: round ground and air");
        }
    }
}
