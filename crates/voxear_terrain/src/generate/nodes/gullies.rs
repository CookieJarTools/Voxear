//! The Gullies node: the height wired in, with gullies cut down its slopes
//! as running water cuts them, by the maths a landmass cuts its own with
//! (see `landmass::Gullied`). In each of a few octaves, stripes run down
//! the slope the ground has there, cut deeper the steeper it is, each
//! octave half as far apart and following the ground as the last left it,
//! so small gullies run down the walls of larger ones and into them. Level
//! ground, crests and floors are left as they were; nothing is cut under
//! the generator's sea, and less towards it.
//!
//! In one thing it differs from a landmass: its octaves are joined (see
//! `Gullied::joined`). A landmass's small gullies meet a larger gully's
//! floor out of step from its two walls, which leaves a step in the ground
//! along the floor, a metre or more high. A landmass keeps them, since the
//! ground it makes is pinned; the node's gullies turn to run along the
//! floor as they reach it, and meet.
//!
//! The slope is read from the height a little way off each way (0.15 of
//! Spacing), so four more times than the place itself: everything behind
//! the node is worked out at each. Keep Spacing well under the width of the
//! hills the gullies run down: where the slope turns within a gully's
//! width, its stripes are sheared into arcs.
//!
//! Round a planet the height passes through as it is: gullies run down a
//! slope along flat ground.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around, node_seed};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
    landmass::{Erosion, Gullied},
};

/// Gullies cut `strength` deep (1 for a quarter as deep as they are apart,
/// on steep ground), the largest `spacing` metres apart, in `octaves`
/// octaves each half as far apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gullies {
    pub strength: f32,
    pub spacing: f32,
    pub octaves: u32,
}

impl Default for Gullies {
    fn default() -> Self {
        Self { strength: 0.5, spacing: 160.0, octaves: 3 }
    }
}

impl Gullies {
    /// Its Strength, among its values.
    pub const STRENGTH: usize = 0;
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("strength", "Strength", 0.0, f32::MAX, ""),
    ValueSpec::number("spacing", "Spacing", 1.0, f32::MAX, "m"),
    ValueSpec::whole("octaves", "Octaves", 1, 8),
];

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::field("height", "Height")];

impl Kind for Gullies {
    const TITLE: &'static str = "Gullies";
    const FOLDER: &'static str = "Erosion";

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
            0 => Value::Number(self.strength),
            1 => Value::Number(self.spacing),
            2 => Value::Whole(self.octaves),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.strength = number,
            (1, Value::Number(number)) => self.spacing = number,
            (2, Value::Whole(whole)) => self.octaves = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let Some(height) = cx.input(node, 0, at) else { return cx.constant(0.0) };
        let Places::Flat { x, z } = at else { return height };
        if self.strength <= 0.0 || self.octaves == 0 {
            return height;
        }
        let cut = Gullied {
            erosion: Erosion { strength: self.strength, spacing: self.spacing, octaves: self.octaves },
            scale: cx.world.scale,
            sea_level: cx.world.sea_level,
            seed: node_seed(cx.world, node),
            joined: true,
        };
        let [a, b, c, d] = around::four(cx, at, cut.step()).map(|from| cx.input(node, 0, from).unwrap_or(height));
        cx.run(Cut(cut), &[x, z, height, a, b, c, d])
    }
}

/// A height with gullies cut down it: from rows of x and z, the height
/// there, and the height a step off further along x, back along x, further
/// along z and back along z.
#[derive(Debug)]
struct Cut(Gullied);

impl Kernel for Cut {
    fn key(&self, key: &mut Key) {
        let Gullied { erosion, scale, sea_level, seed, joined } = self.0;
        key.name("gullies").number(erosion.strength).number(erosion.spacing).whole(erosion.octaves);
        key.number(scale).number(sea_level).whole(seed).whole(u32::from(joined));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let around = [inputs[3][at], inputs[4][at], inputs[5][at], inputs[6][at]];
            *out = self.0.cut(inputs[0][at], inputs[1][at], inputs[2][at], around);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Landmass, NodeKind, Preset, WorldGen,
        graph::NodeId,
        nodes::{Constant, Land, Noise, testing},
    };

    #[test]
    fn gullies_cut_any_height_by_a_landmasss_maths_to_the_bit() {
        // Alpine with no gullies or bumps of its own, and a Gullies node after it with Alpine's gullies.
        let alpine = Preset::Alpine.landmass();
        let erosion = alpine.erosion;
        let bare = Landmass { erosion: Erosion { strength: 0.0, ..erosion }, bumps: 0.0, ..alpine.clone() };
        let gullies = Gullies { strength: erosion.strength, spacing: erosion.spacing, octaves: erosion.octaves };
        let world = testing::world_of(NodeKind::Gullies(gullies), 0, &[Some(NodeKind::Landmass(Land::from(&bare)))]);
        let world = WorldGen::new(1, 1.3, 20.0, world.graph().clone());
        let bare = Landmass { scale: 1.3, sea_level: 20.0, ..bare };
        // The node's gullies are its own, from its number: node 2 here.
        let seed = node_seed(crate::generate::compile::World { seed: 1, scale: 1.3, sea_level: 20.0 }, NodeId(2));
        let cut = Gullied { erosion, scale: 1.3, sea_level: 20.0, seed, joined: true };
        let step = cut.step();
        let (mut lower, mut cuts) = (0.0, 0);
        for i in 0..3000 {
            let (x, z) = ((i % 60) as f32 * 211.0 - 6000.0, (i / 60) as f32 * 197.0 - 5000.0);
            let at = |dx: f32, dz: f32| bare.height_at(x + dx, z + dz);
            let was = at(0.0, 0.0);
            let meant = cut.cut(x, z, was, [at(step, 0.0), at(-step, 0.0), at(0.0, step), at(0.0, -step)]);
            let made = world.height_at(x, z);
            assert_eq!(made.to_bits(), meant.to_bits(), "at {x}, {z}");
            assert!(made <= was + 1e-3, "raised from {was} to {made}");
            (lower, cuts) = (lower + was - made, cuts + usize::from(was - made > 0.5));
        }
        assert!(lower / 3000.0 > 0.5 && cuts > 300, "cut into the mountains: {} m on average", lower / 3000.0);
        // With the landmass's own seed in place of the node's, they are the landmass's own gullies: the ground
        // Alpine makes, less its bumps.
        let own = Gullied { erosion, scale: 1.3, sea_level: 20.0, seed: 1, joined: false };
        let whole = Landmass { scale: 1.3, sea_level: 20.0, bumps: 0.0, ..alpine };
        for i in 0..500 {
            let (x, z) = (i as f32 * 53.0 - 9000.0, i as f32 * 29.0 + 500.0);
            let at = |dx: f32, dz: f32| bare.height_at(x + dx, z + dz);
            let around = [at(step, 0.0), at(-step, 0.0), at(0.0, step), at(0.0, -step)];
            assert_eq!(own.cut(x, z, at(0.0, 0.0), around).to_bits(), whole.height_at(x, z).to_bits());
        }
    }

    #[test]
    fn a_nodes_gullies_join_where_a_landmasss_leave_steps_along_their_floors() {
        // Hills 600 m across with gullies 180 m apart in three octaves, walked a quarter of a metre at a time: the
        // most the ground rises or falls in one pace, joined and not.
        let hills = Noise { size: 600.0, octaves: 3, low: 0.0, high: 120.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills), 0, &[]);
        let erosion = Erosion { strength: 0.6, spacing: 180.0, octaves: 3 };
        let largest_pace = |joined: bool| {
            let cut = Gullied { erosion, scale: 1.0, sea_level: -1000.0, seed: 3, joined };
            let step = cut.step();
            let at = |x: f32, z: f32| {
                let h = |dx: f32, dz: f32| source.height_at(x + dx, z + dz);
                cut.cut(x, z, h(0.0, 0.0), [h(step, 0.0), h(-step, 0.0), h(0.0, step), h(0.0, -step)])
            };
            let (mut largest, mut last) = (0.0f32, at(0.0, 40.0));
            for i in 1..24_000 {
                let next = at(i as f32 * 0.25, 40.0 + i as f32 * 0.1);
                (largest, last) = (largest.max((next - last).abs()), next);
            }
            largest
        };
        let (joined, stepped) = (largest_pace(true), largest_pace(false));
        assert!(joined < 0.4, "joined, the ground changes by {joined} m in a pace at the most");
        assert!(stepped > 1.0 && stepped > joined * 4.0, "not joined, by {stepped} m: {joined} m joined");
    }

    #[test]
    fn gullies_leave_level_ground_and_planets_as_they_are() {
        let hills = Noise { size: 500.0, low: -30.0, high: 220.0, ..Noise::default() };
        let source = testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]);
        let gullies = |strength: f32| {
            let kind = NodeKind::Gullies(Gullies { strength, spacing: 120.0, octaves: 3 });
            testing::world_of(kind, 0, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (world, none) = (gullies(0.8), gullies(0.0));
        let (low, high) = testing::checked(&world);
        assert!(low >= -30.0 && high <= 220.0, "never raised, and never cut under the sea: {low} to {high}");
        let (was, cut) = (testing::flat(&source, 4000), testing::flat(&world, 4000));
        let mut lower = 0.0;
        for i in 0..4000 {
            // Only cut, by no more than a gully's depth; never under the sea from above it, nor at all under it.
            assert!(cut[i] <= was[i] && cut[i] >= was[i] - 0.8 * 120.0 * 0.25 * 2.0, "{} cut to {}", was[i], cut[i]);
            assert!(was[i] > 0.0 || cut[i] == was[i], "{} cut to {} under the sea", was[i], cut[i]);
            assert!(was[i] < 0.0 || cut[i] >= 0.0, "{} cut to {}, under the sea", was[i], cut[i]);
            lower += was[i] - cut[i];
        }
        assert!(lower / 4000.0 > 0.3, "{} m on average", lower / 4000.0);
        assert_eq!(testing::flat(&none, 4000), was, "no strength, no gullies");
        // Round a planet the height passes through, to the bit.
        assert_eq!(testing::round(&world, 4000), testing::round(&source, 4000));
        // Level ground has no slope to run down; nothing wired in is nothing.
        let level = testing::world_of(
            NodeKind::Gullies(Gullies::default()),
            0,
            &[Some(NodeKind::Constant(Constant { value: 50.0 }))],
        );
        assert_eq!(level.height_at(40.0, 40.0), 50.0);
        assert_eq!(testing::world_of(NodeKind::Gullies(Gullies::default()), 0, &[]).height_at(40.0, 40.0), 0.0);
        // Twice the scale, the same gullies twice as far apart in ground twice as wide: as deep as its slopes,
        // half as steep, let them be.
        let wide = world.with_scale(2.0);
        assert!(wide.height_at(600.0, 200.0) <= source.with_scale(2.0).height_at(600.0, 200.0));
    }
}
