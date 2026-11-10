//! The Climate Mask node: 1 where the temperature and humidity wired in
//! lie within its ranges, easing to nothing outside them. It says where a
//! kind of country is by how warm and wet the world is there. Biome sets
//! both ranges to those of one of eight kinds of country; changing either
//! range by hand shows Custom.
//!
//! Wire a Climate node's Temperature and Humidity into it. An input with
//! nothing wired in counts for any, so a mask of temperature alone needs
//! only that wire.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, around};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        graph::NodeId,
    },
    surfaces::Biome,
};

/// The coldest and hottest a climate mask's rows go, in °C: a biome with
/// no range of temperatures reaches both.
const COLDEST: f32 = -100.0;
const HOTTEST: f32 = 100.0;

/// A mask of where it is from `temperature[0]` to `temperature[1]` °C warm
/// and from `humidity[0]` to `humidity[1]` wet, eased in over
/// `temperature_feather` °C and `humidity_feather` outside them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClimateMask {
    pub temperature: [f32; 2],
    pub humidity: [f32; 2],
    pub temperature_feather: f32,
    pub humidity_feather: f32,
}

impl Default for ClimateMask {
    fn default() -> Self {
        Self::of(Biome::Forest)
    }
}

impl ClimateMask {
    /// Its inputs, among its inputs.
    pub const TEMPERATURE: usize = 0;
    pub const HUMIDITY: usize = 1;

    /// The mask of where `biome` is: its ranges, eased in as a terrain's
    /// surfaces ease theirs, over 3 °C and 0.08 of humidity.
    pub fn of(biome: Biome) -> Self {
        let (temperature, humidity) = Self::ranges(biome);
        Self { temperature, humidity, temperature_feather: 3.0, humidity_feather: 0.08 }
    }

    /// A biome's ranges as rows keep them: any temperature as coldest to
    /// hottest, any humidity as 0 to 1.
    fn ranges(biome: Biome) -> ([f32; 2], [f32; 2]) {
        let (temperature, humidity) = biome.ranges();
        (temperature.unwrap_or([COLDEST, HOTTEST]), humidity.unwrap_or([0.0, 1.0]))
    }

    /// The biome its ranges are, if they are one's.
    pub fn biome(&self) -> Option<Biome> {
        Biome::ALL.into_iter().find(|&biome| Self::ranges(biome) == (self.temperature, self.humidity))
    }
}

const BIOMES: &[&str] =
    &["Snow", "Tundra", "Taiga", "Grassland", "Forest", "Desert", "Savanna", "Rainforest", "Custom"];

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("biome", "Biome", BIOMES),
    ValueSpec::number("temperature_min", "Temperature min", COLDEST, HOTTEST, "°C"),
    ValueSpec::number("temperature_max", "Temperature max", COLDEST, HOTTEST, "°C"),
    ValueSpec::number("humidity_min", "Humidity min", 0.0, 1.0, ""),
    ValueSpec::number("humidity_max", "Humidity max", 0.0, 1.0, ""),
    ValueSpec::number("temperature_feather", "Temperature feather", 0.0, HOTTEST, "°C").advanced(),
    ValueSpec::number("humidity_feather", "Humidity feather", 0.0, 1.0, "").advanced(),
];

const INPUTS: &[Port] = &[Port::field("temperature", "Temperature"), Port::field("humidity", "Humidity")];

const OUTPUTS: &[Port] = &[Port::field("mask", "Mask")];

impl Kind for ClimateMask {
    const TITLE: &'static str = "Climate Mask";
    const FOLDER: &'static str = "Masks";

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
            0 => Value::Choice(
                self.biome().and_then(|biome| Biome::ALL.iter().position(|&b| b == biome)).unwrap_or(Biome::ALL.len()),
            ),
            1 => Value::Number(self.temperature[0]),
            2 => Value::Number(self.temperature[1]),
            3 => Value::Number(self.humidity[0]),
            4 => Value::Number(self.humidity[1]),
            5 => Value::Number(self.temperature_feather),
            6 => Value::Number(self.humidity_feather),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            // A biome's ranges; Custom leaves them as they are.
            (0, Value::Choice(choice)) => {
                if let Some(&biome) = Biome::ALL.get(choice) {
                    (self.temperature, self.humidity) = Self::ranges(biome);
                }
            }
            (1, Value::Number(number)) => self.temperature[0] = number,
            (2, Value::Number(number)) => self.temperature[1] = number,
            (3, Value::Number(number)) => self.humidity[0] = number,
            (4, Value::Number(number)) => self.humidity[1] = number,
            (5, Value::Number(number)) => self.temperature_feather = number,
            (6, Value::Number(number)) => self.humidity_feather = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let warmth = cx.input(node, Self::TEMPERATURE, at);
        let wetness = cx.input(node, Self::HUMIDITY, at);
        let range = |[low, high]: [f32; 2], feather: f32| Range { low, high, feather };
        let warm = warmth.map(|row| cx.run(range(self.temperature, self.temperature_feather), &[row]));
        let wet = wetness.map(|row| cx.run(range(self.humidity, self.humidity_feather), &[row]));
        match (warm, wet) {
            (Some(warm), Some(wet)) => cx.run(Both, &[warm, wet]),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => cx.constant(1.0),
        }
    }
}

/// How much of a mask from `low` to `high`, eased in over `feather`,
/// covers each of a row.
#[derive(Debug)]
struct Range {
    low: f32,
    high: f32,
    feather: f32,
}

impl Kernel for Range {
    fn key(&self, key: &mut Key) {
        key.name("climate mask").number(self.low).number(self.high).number(self.feather);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, &value) in out.iter_mut().zip(inputs[0]) {
            *out = around::within(self.low, self.high, self.feather, value);
        }
    }
}

/// Two masks as one: where both cover.
#[derive(Debug)]
struct Both;

impl Kernel for Both {
    fn key(&self, key: &mut Key) {
        key.name("climate mask both");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            *out = inputs[0][at] * inputs[1][at];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Noise, testing},
    };

    #[test]
    fn a_climate_mask_covers_its_ranges_of_temperature_and_humidity() {
        let warmth = Noise { size: 400.0, low: -30.0, high: 40.0, ..Noise::default() };
        let wetness = Noise { size: 300.0, low: 0.0, high: 1.0, ..Noise::default() };
        let sources = [Some(NodeKind::Noise(warmth.clone())), Some(NodeKind::Noise(wetness.clone()))];
        let mask = |biome: Biome| testing::world_of(NodeKind::ClimateMask(ClimateMask::of(biome)), 0, &sources);
        let world = mask(Biome::Grassland);
        assert_eq!(testing::checked(&world), (0.0, 1.0));
        // The weather under the mask, from its two fields' previews: nodes 1 and 2 of every graph here.
        let (xs, zs) = testing::flat_places(4000);
        let previews = world.previews(&xs, &zs);
        let (warmths, wets) = (&previews[0].values, &previews[1].values);
        let warm_only = NodeKind::ClimateMask(ClimateMask::of(Biome::Grassland));
        let warm_only = testing::world_of(warm_only, 0, &[sources[0].clone()]);
        let covered = testing::flat(&world, 4000);
        for i in 0..4000 {
            // Grassland: 4 to 22 °C and 0.25 to 0.55 wet, eased in over 3 °C and 0.08.
            let (warm, wet) = (warmths[i], wets[i]);
            let inside = (4.0..22.0).contains(&warm) && (0.25..0.55).contains(&wet);
            let outside = warm <= 1.0 || warm >= 25.0 || wet <= 0.17 || wet >= 0.63;
            assert!(!inside || covered[i] == 1.0, "{} at {warm} °C, {wet}", covered[i]);
            assert!(!outside || covered[i] == 0.0, "{} at {warm} °C, {wet}", covered[i]);
        }
        assert!(covered.iter().any(|&mask| mask > 0.0 && mask < 1.0), "and eased in between");
        // With only a temperature wired in, any humidity counts.
        let by_warmth = testing::flat(&warm_only, 4000);
        assert!((0..4000).all(|i| !(4.0..22.0).contains(&warmths[i]) || by_warmth[i] == 1.0));
        assert!((0..4000).all(|i| by_warmth[i] >= covered[i]));
        // Snow has no range of humidity, so it is wherever it is cold; with nothing wired in, a mask is everywhere.
        let snow = testing::flat(&mask(Biome::Snow), 4000);
        assert!((0..4000).all(|i| (snow[i] == 1.0) == (warmths[i] < -2.0) || warmths[i] > -2.0 && warmths[i] < 1.0));
        assert_eq!(testing::world_of(NodeKind::ClimateMask(ClimateMask::default()), 0, &[]).height_at(0.0, 0.0), 1.0);
        // Round a planet it masks the climate it is given there, the same bits in any batch.
        assert_eq!(testing::round_range(&world), (0.0, 1.0));
        // Each biome's ranges are found again by its mask, and a range changed by hand is Custom.
        for (at, biome) in Biome::ALL.into_iter().enumerate() {
            let mut kind = NodeKind::ClimateMask(ClimateMask::of(Biome::Desert));
            kind.set_value(0, Value::Choice(at));
            assert_eq!(kind, NodeKind::ClimateMask(ClimateMask::of(biome)));
            assert_eq!(kind.value(0), Some(Value::Choice(at)), "{biome:?}");
            assert!(kind.set_value(3, Value::Number(0.33)));
            assert_eq!(kind.value(0), Some(Value::Choice(Biome::ALL.len())), "Custom");
            assert!(!kind.set_value(0, Value::Choice(Biome::ALL.len())), "which leaves it as it is");
        }
    }
}
