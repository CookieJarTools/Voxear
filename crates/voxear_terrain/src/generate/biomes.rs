//! Biomes: kinds of country, each a `.biome` file. A biome says where it
//! lies, by how warm and wet the world is there and how high over the sea,
//! and what the ground there is made of: the blocks on top, under them and
//! deep down, on steep ground and along the shore. A world's generator
//! gives the climate; the world's biomes ([`Biomes`]) say which biome each
//! place is.
//!
//! ```toml
//! color = [0.93, 0.87, 0.6]      # on maps and previews
//! temperature = [22.0, 45.0]     # °C
//! humidity = [0.0, 0.25]         # 0 dry to 1 wet
//! height = [0.0, 400.0]          # metres over the sea; left out for any
//!
//! [ground]
//! top = "Blocks/Sand.block"
//! under = "Blocks/Sandstone.block"
//! under_depth = 4                # blocks of `under` below the top one
//! deep = "Blocks/Stone.block"
//! steep = "Blocks/Sandstone.block"  # where the ground is steeper than `steep_slope`
//! steep_slope = 40.0             # degrees
//! shore = "Blocks/Sand.block"    # within `shore_height` metres of the sea
//! shore_height = 2.0
//! ```
//!
//! A place belongs to the biome whose ranges it is nearest: within them,
//! or the least outside them, each measure counted in shares of how far
//! the world's climate strays (so a degree is not weighed against a tenth
//! of wetness as it stands). Among biomes a place is equally within, one
//! that keeps to a height wins, so peaks stand over whatever grows below
//! them, and then the narrowest: a small oasis listed with a wide desert
//! shows inside it. Borders wander: the climate is read a little off, by noise, so no
//! border is a contour of temperature and none runs straight.

use serde::{Deserialize, Serialize};

use crate::noise;

/// What a `.biome` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BiomeDef {
    /// Its colour on maps and previews.
    pub color: [f32; 3],
    /// How warm it is there, in °C, from and to.
    pub temperature: [f32; 2],
    /// How wet, from 0 for dry to 1 for wet.
    pub humidity: [f32; 2],
    /// How high over the sea, in metres; none for any height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<[f32; 2]>,
    /// What its ground is made of.
    pub ground: BiomeGround,
}

impl Default for BiomeDef {
    fn default() -> Self {
        Self {
            color: [0.45, 0.62, 0.3],
            temperature: [5.0, 22.0],
            humidity: [0.3, 0.7],
            height: None,
            ground: BiomeGround::default(),
        }
    }
}

/// The blocks a biome's ground is made of, each a reference to a `.block`;
/// one left out is the world's own for that place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BiomeGround {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<String>,
    /// How many blocks of `under` lie below the top one.
    pub under_depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep: Option<String>,
    /// On ground steeper than `steep_slope`, in place of the top and under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steep: Option<String>,
    /// In degrees.
    pub steep_slope: f32,
    /// Within `shore_height` metres of the sea, over and under it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shore: Option<String>,
    pub shore_height: f32,
}

impl Default for BiomeGround {
    fn default() -> Self {
        Self {
            top: None,
            under: None,
            under_depth: 3,
            deep: None,
            steep: None,
            steep_slope: 40.0,
            shore: None,
            shore_height: 2.0,
        }
    }
}

impl BiomeDef {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// How far `value` lies outside `range`, in shares of `spread`; none
    /// within it.
    fn outside(range: [f32; 2], value: f32, spread: f32) -> f32 {
        let (low, high) = (range[0].min(range[1]), range[0].max(range[1]));
        ((low - value).max(value - high)).max(0.0) / spread.max(1e-6)
    }

    /// How far a place lies outside its ranges, squared: nothing within
    /// them all.
    fn away(&self, climate: Place, spreads: Spreads) -> f32 {
        let warmth = Self::outside(self.temperature, climate.temperature, spreads.temperature);
        let wetness = Self::outside(self.humidity, climate.humidity, spreads.humidity);
        let height = self.height.map_or(0.0, |range| Self::outside(range, climate.above_sea, spreads.height));
        warmth * warmth + wetness * wetness + height * height
    }

    /// How much of the climate its ranges take in: the narrower, the less.
    fn reach(&self, spreads: Spreads) -> f32 {
        let span = |range: [f32; 2], spread: f32| ((range[1] - range[0]).abs() / spread.max(1e-6)).min(8.0);
        // One that keeps to a height comes before any that does not: peaks over whatever grows below them.
        let height = self.height.map_or(1000.0, |range| span(range, spreads.height));
        span(self.temperature, spreads.temperature) * span(self.humidity, spreads.humidity) * height
    }
}

/// A place's climate: how warm, how wet, and how high over the sea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub temperature: f32,
    pub humidity: f32,
    pub above_sea: f32,
}

/// What one share of each measure is, for weighing them against each other.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spreads {
    temperature: f32,
    humidity: f32,
    height: f32,
}

/// A world's biomes, in the order it lists them, and how its borders
/// wander.
#[derive(Clone, Debug, PartialEq)]
pub struct Biomes {
    list: Vec<BiomeDef>,
    spreads: Spreads,
    /// How far borders wander, in shares of a measure's spread, and over
    /// how many metres.
    wander: f32,
    wander_size: f32,
    seed: u32,
}

impl Biomes {
    /// `list`, for a world whose temperature strays `temperature_range` °C
    /// and whose humidity `humidity_range` either side of its average, made
    /// from `seed`.
    pub fn new(list: Vec<BiomeDef>, temperature_range: f32, humidity_range: f32, seed: u32) -> Self {
        let spreads =
            Spreads { temperature: temperature_range.max(1.0), humidity: humidity_range.max(0.05), height: 200.0 };
        Self { list, spreads, wander: 0.12, wander_size: 48.0, seed: seed ^ 0xB10E_5EED }
    }

    /// Borders that wander `wander` of a measure's spread (0 for none) over
    /// `size` metres.
    pub fn wandering(mut self, wander: f32, size: f32) -> Self {
        (self.wander, self.wander_size) = (wander.max(0.0), size.max(1.0));
        self
    }

    pub fn list(&self) -> &[BiomeDef] {
        &self.list
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Which biome a place with `climate` is, among the list: none for an
    /// empty list.
    pub fn of(&self, climate: Place) -> Option<usize> {
        let mut best: Option<(usize, f32, f32)> = None;
        for (at, biome) in self.list.iter().enumerate() {
            let (away, reach) = (biome.away(climate, self.spreads), biome.reach(self.spreads));
            let nearer = best.is_none_or(|(_, least, narrowest)| away < least || (away == least && reach < narrowest));
            if nearer {
                best = Some((at, away, reach));
            }
        }
        best.map(|(at, ..)| at)
    }

    /// Which biome the place at `(x, z)`, in metres in the generator's
    /// world, is, with the generator's `climate` there: read a little off
    /// by noise, so borders wander.
    pub fn at(&self, x: f32, z: f32, climate: Place) -> Option<usize> {
        if self.wander <= 0.0 {
            return self.of(climate);
        }
        let (u, v) = (x / self.wander_size, z / self.wander_size);
        let off = |seed: u32| {
            let wide = noise::gradient(u, v, self.seed ^ seed);
            let fine = noise::gradient(u * 3.7, v * 3.7, self.seed ^ seed ^ 0x9E37_79B9);
            (wide + 0.4 * fine) * self.wander
        };
        self.of(Place {
            temperature: climate.temperature + off(1) * self.spreads.temperature,
            humidity: climate.humidity + off(2) * self.spreads.humidity,
            above_sea: climate.above_sea,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn biome(temperature: [f32; 2], humidity: [f32; 2], height: Option<[f32; 2]>) -> BiomeDef {
        BiomeDef { temperature, humidity, height, ..BiomeDef::default() }
    }

    fn place(temperature: f32, humidity: f32, above_sea: f32) -> Place {
        Place { temperature, humidity, above_sea }
    }

    #[test]
    fn a_biome_is_written_and_read_back_and_an_unknown_key_is_refused() {
        let mut desert = biome([22.0, 45.0], [0.0, 0.25], Some([0.0, 400.0]));
        desert.ground.top = Some("Blocks/Sand.block".into());
        desert.ground.steep = Some("Blocks/Sandstone.block".into());
        assert_eq!(BiomeDef::parse(&desert.to_text()).unwrap(), desert);
        assert_eq!(BiomeDef::parse("").unwrap(), BiomeDef::default());
        assert!(BiomeDef::parse("colour = [1.0, 0.0, 0.0]").is_err());
        // Blocks left out are not written.
        assert!(!BiomeDef::default().to_text().contains("top"));
    }

    #[test]
    fn a_place_is_the_biome_it_is_within_or_nearest_and_the_narrowest_of_those_it_is_within() {
        let (desert, forest, snow) = (
            biome([22.0, 45.0], [0.0, 0.25], None),
            biome([8.0, 22.0], [0.4, 1.0], None),
            biome([-40.0, -2.0], [0.0, 1.0], None),
        );
        let oasis = biome([25.0, 35.0], [0.1, 0.2], None);
        let peaks = biome([-40.0, 45.0], [0.0, 1.0], Some([900.0, 4000.0]));
        let biomes = Biomes::new(vec![desert, forest, snow, oasis, peaks], 15.0, 0.3, 1);
        assert_eq!(biomes.of(place(30.0, 0.05, 20.0)), Some(0), "hot and dry");
        assert_eq!(biomes.of(place(15.0, 0.7, 20.0)), Some(1), "mild and wet");
        assert_eq!(biomes.of(place(-10.0, 0.5, 20.0)), Some(2), "cold");
        // Within the desert and the oasis: the narrower.
        assert_eq!(biomes.of(place(30.0, 0.15, 20.0)), Some(3));
        // Within none: the nearest. Just warmer than the forest and far too wet for the desert.
        assert_eq!(biomes.of(place(23.0, 0.8, 20.0)), Some(1));
        // And just too dry for the forest, far too cool for the desert, far too warm for snow.
        assert_eq!(biomes.of(place(12.0, 0.35, 20.0)), Some(1));
        // High up, the peaks, whatever the climate; the forest again below them.
        assert_eq!(biomes.of(place(15.0, 0.7, 1200.0)), Some(4));
        assert_eq!(biomes.of(place(15.0, 0.7, 850.0)), Some(1));
        assert_eq!(Biomes::new(Vec::new(), 15.0, 0.3, 1).of(place(0.0, 0.5, 0.0)), None);
    }

    #[test]
    fn borders_wander_by_noise_the_same_for_a_seed_and_never_far_from_where_the_climate_puts_them() {
        let (cool, warm) = (biome([-40.0, 15.0], [0.0, 1.0], None), biome([15.0, 45.0], [0.0, 1.0], None));
        let biomes = Biomes::new(vec![cool, warm], 15.0, 0.3, 7);
        // Temperature rising a degree every 100 m along x: the border is at x = 1500 without wandering.
        let climate = |x: f32| place(x / 100.0, 0.5, 10.0);
        let border = |z: f32| {
            let first_warm = (0..3000).map(|x| x as f32).find(|&x| biomes.at(x, z, climate(x)) == Some(1));
            first_warm.expect("warm somewhere along the line")
        };
        let borders: Vec<f32> = (0..40).map(|i| border(i as f32 * 25.0)).collect();
        let (low, high) = borders.iter().fold((f32::MAX, f32::MIN), |(l, h), &b| (l.min(b), h.max(b)));
        assert!(high - low > 60.0, "the border wanders: from {low} to {high}");
        // No further than its wander: 0.12 of 15 °C at a degree every 100 m, and the finer noise's 0.4 more.
        assert!(low > 1500.0 - 260.0 && high < 1500.0 + 260.0, "from {low} to {high}");
        // The same again for the seed, another line for another.
        assert_eq!(borders, (0..40).map(|i| border(i as f32 * 25.0)).collect::<Vec<_>>());
        let other = Biomes::new(biomes.list().to_vec(), 15.0, 0.3, 8);
        assert!(
            (0..3000)
                .any(|x| other.at(x as f32, 0.0, climate(x as f32)) != biomes.at(x as f32, 0.0, climate(x as f32)))
        );
        // Without wandering, straight where the climate puts it.
        let straight = biomes.clone().wandering(0.0, 48.0);
        assert_eq!(straight.at(1499.0, 300.0, climate(1499.0)), Some(0));
        assert_eq!(straight.at(1501.0, 300.0, climate(1501.0)), Some(1));
    }
}
