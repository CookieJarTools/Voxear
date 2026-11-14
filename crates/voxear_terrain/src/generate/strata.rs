//! A biome's rock in layers: bands of blocks one over another, each so many
//! metres thick, repeating up and down, tilted a little one way and
//! wandering up and down a few metres across the land. A cliff, a canyon's
//! wall or a cave cut through the rock shows the bands running level along
//! it, as layered rock does, in place of one block from top to bottom.
//!
//! ```toml
//! [ground.rock_layers]
//! tilt = 2.0        # degrees: how far the layers lean, all one way
//! wander = 6.0      # metres: how far they rise and fall across the land
//!
//! [[ground.rock_layers.layers]]
//! block = "Blocks/Sandstone.block"
//! thickness = 12.0  # metres
//!
//! [[ground.rock_layers.layers]]
//! block = "Blocks/Red Sandstone.block"
//! thickness = 4.0
//! ```
//!
//! The layers are measured from the sea, the first listed lowest, and
//! repeat over and under. The way they lean is the biome's own, picked from
//! its layers, so every world gives the same biome the same rock. Arithmetic
//! alone, so every machine lays the same bands.

use serde::{Deserialize, Serialize};

use crate::{blocks::chunk::BlockId, noise};

/// A biome's rock layers as its file has them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RockLayers {
    /// How far the layers lean, in degrees.
    pub tilt: f32,
    /// How far they rise and fall across the land, in metres.
    pub wander: f32,
    pub layers: Vec<RockLayer>,
}

impl Default for RockLayers {
    fn default() -> Self {
        Self { tilt: 2.0, wander: 6.0, layers: Vec::new() }
    }
}

impl RockLayers {
    pub fn is_none(&self) -> bool {
        self.layers.is_empty()
    }
}

/// One layer: its block, by reference to a `.block`, and how thick it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RockLayer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// In metres.
    pub thickness: f32,
}

impl Default for RockLayer {
    fn default() -> Self {
        Self { block: None, thickness: 8.0 }
    }
}

/// Rock layers by a world's numbers for their blocks, ready to ask which
/// block lies at a height.
#[derive(Clone, Debug, PartialEq)]
pub struct Strata {
    /// Each band's block and the height its top lies at within one repeat,
    /// lowest first.
    bands: Vec<(BlockId, f32)>,
    /// How tall one repeat of every band is, in metres.
    period: f32,
    /// How far the layers rise for every metre along x and along z.
    lean: [f32; 2],
    wander: f32,
    seed: u32,
}

/// How far apart the rises and falls of the layers' wander are, in metres.
const WANDER_SIZE: f32 = 450.0;

impl Strata {
    /// `layers` with each block numbered by `number`; none where no layer
    /// has both a block the world numbers and some thickness.
    pub fn of(layers: &RockLayers, mut number: impl FnMut(&str) -> BlockId) -> Option<Self> {
        let mut bands = Vec::new();
        let mut top = 0.0;
        let mut key = Vec::new();
        for layer in &layers.layers {
            let thick = if layer.thickness.is_finite() { layer.thickness.clamp(0.0, 1000.0) } else { 0.0 };
            let block = layer.block.as_deref().map_or(0, &mut number);
            if thick <= 0.0 || block == 0 {
                continue;
            }
            top += thick;
            bands.push((block, top));
            key.extend(block.to_le_bytes());
            key.extend(thick.to_bits().to_le_bytes());
        }
        if bands.is_empty() {
            return None;
        }
        let seed = crate::layers::stable_hash(&key) as u32;
        // One of sixteen ways round, picked by the layers, and the rise for every metre that way.
        let way = WAYS[(seed % WAYS.len() as u32) as usize];
        let rise = super::biomes::slope_of(layers.tilt.clamp(0.0, 45.0));
        Some(Self {
            bands,
            period: top,
            lean: [way[0] * rise, way[1] * rise],
            wander: layers.wander.clamp(0.0, 200.0),
            seed,
        })
    }

    /// How far the layers are lifted at `(x, z)`, in metres in the
    /// generator's world: by their lean and their wander.
    pub fn lift(&self, x: f32, z: f32) -> f32 {
        let wander = if self.wander > 0.0 {
            self.wander * noise::gradient(x / WANDER_SIZE, z / WANDER_SIZE, self.seed)
        } else {
            0.0
        };
        x * self.lean[0] + z * self.lean[1] + wander
    }

    /// The block of the layer at `height` metres, lifted as [`Strata::lift`]
    /// says, so `height` is the place's own height less the lift there.
    pub fn block(&self, height: f32) -> BlockId {
        let within = height.rem_euclid(self.period);
        self.bands.iter().find(|(_, top)| within < *top).map_or(self.bands[0].0, |(block, _)| *block)
    }

    /// Every block of its layers.
    pub fn blocks(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.bands.iter().map(|(block, _)| *block)
    }

    /// The thinnest of its bands, in metres.
    pub fn thinnest(&self) -> f32 {
        let mut below = 0.0;
        let mut least = f32::MAX;
        for (_, top) in &self.bands {
            least = least.min(top - below);
            below = *top;
        }
        least
    }
}

/// Sixteen ways round, each a unit along x and z, without trigonometry.
const WAYS: [[f32; 2]; 16] = [
    [1.0, 0.0],
    [0.92388, 0.38268],
    [0.70711, 0.70711],
    [0.38268, 0.92388],
    [0.0, 1.0],
    [-0.38268, 0.92388],
    [-0.70711, 0.70711],
    [-0.92388, 0.38268],
    [-1.0, 0.0],
    [-0.92388, -0.38268],
    [-0.70711, -0.70711],
    [-0.38268, -0.92388],
    [0.0, -1.0],
    [0.38268, -0.92388],
    [0.70711, -0.70711],
    [0.92388, -0.38268],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn layers(bands: &[(&str, f32)]) -> RockLayers {
        let layers =
            bands.iter().map(|(name, thickness)| RockLayer { block: Some(name.to_string()), thickness: *thickness });
        RockLayers { tilt: 0.0, wander: 0.0, layers: layers.collect() }
    }

    fn number(name: &str) -> BlockId {
        match name {
            "Sandstone" => 4,
            "Shale" => 5,
            "Limestone" => 6,
            _ => 0,
        }
    }

    #[test]
    fn rock_lies_in_its_layers_level_one_over_another_repeating_up_and_down() {
        let strata = Strata::of(&layers(&[("Sandstone", 10.0), ("Shale", 3.0), ("Limestone", 7.0)]), number).unwrap();
        let at = |height: f32| strata.block(height - strata.lift(50.0, 80.0));
        assert_eq!([at(0.5), at(9.5), at(10.5), at(12.5), at(13.5), at(19.5)], [4, 4, 5, 5, 6, 6]);
        // Repeating: 20 m over and under is the same rock.
        assert_eq!([at(20.5), at(30.5), at(-0.5), at(-8.0)], [4, 5, 6, 5]);
        assert_eq!(strata.thinnest(), 3.0);
        // Level: the same rock at the same height anywhere.
        assert_eq!(strata.block(5.0 - strata.lift(-900.0, 400.0)), 4);
    }

    #[test]
    fn layers_lean_and_wander_but_only_a_little_and_none_of_unnumbered_blocks() {
        let mut leaning = layers(&[("Sandstone", 10.0), ("Shale", 3.0)]);
        (leaning.tilt, leaning.wander) = (2.0, 6.0);
        let strata = Strata::of(&leaning, number).unwrap();
        // A kilometre off, the layers stand up to tan(2°) of it and their wander higher or lower.
        let rise = (strata.lift(1000.0, 0.0) - strata.lift(0.0, 0.0)).abs();
        assert!(rise < 1000.0 * 0.035 + 12.0, "{rise}");
        let lifts: Vec<f32> = (0..50).map(|k| strata.lift(k as f32 * 37.0, 0.0)).collect();
        assert!(lifts.windows(2).all(|pair| (pair[1] - pair[0]).abs() < 3.0), "no step in the layers along the land");
        assert!(Strata::of(&layers(&[("Granite", 10.0)]), number).is_none());
        assert!(Strata::of(&layers(&[("Sandstone", 0.0)]), number).is_none());
    }
}
