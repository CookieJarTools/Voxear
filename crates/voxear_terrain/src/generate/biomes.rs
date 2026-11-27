//! Biomes: kinds of country, each a `.biome` file. A biome says where it
//! lies, by how warm and wet the world is there, how high over the sea and
//! how far sunk under the land round about, and what the ground there is made of: the blocks on top, under them and
//! deep down, on steep ground and along the shore. A world's generator
//! gives the climate; the world's biomes ([`Biomes`]) say which biome each
//! place is.
//!
//! ```toml
//! color = [0.93, 0.87, 0.6]      # on maps and previews
//! temperature = [22.0, 45.0]     # °C
//! humidity = [0.0, 0.25]         # 0 dry to 1 wet
//! height = [0.0, 400.0]          # metres over the sea; left out for any
//! sunk = [40.0, 1000.0]          # metres under the land round about: chasm floors; left out for any
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
//!
//! [land]                         # the lie of its land: see `land`
//! form = "tablelands"
//! height = 110.0
//!
//! [vines]
//! block = "Blocks/Vine.block"    # hangs under ledges, overhangs and cave roofs
//! amount = 0.3                   # how many of the places one could hang from have one, 0 to 1
//! length = [2, 7]                # blocks, from and to
//! depth = 40.0                   # how far under the ground's surface they still hang, in metres
//! ```
//!
//! A place belongs to the biome whose ranges it is nearest: within them,
//! or the least outside them, each measure counted in shares of how far
//! the world's climate strays (so a degree is not weighed against a tenth
//! of wetness as it stands). Among biomes a place is equally within, one
//! that keeps to a height wins, so peaks stand over whatever grows below
//! them, and one that keeps to sunk ground before one that does not, so a
//! chasm's floor grows its own; and then the narrowest: a small oasis
//! listed with a wide desert shows inside it. Borders wander: the climate is read a little off, by noise, so no
//! border is a contour of temperature and none runs straight.

use serde::{Deserialize, Serialize};

use super::{BiomeLand, RockLayers, Strata};
use crate::{
    blocks::chunk::{AIR, BlockId},
    noise,
};

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
    /// How far under the land round about its ground lies, in metres, from
    /// and to, as the generator's Sunk says: the floors of chasms and
    /// caverns. None for any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunk: Option<[f32; 2]>,
    /// What its ground is made of.
    pub ground: BiomeGround,
    /// The lie of its land: the generator's own, or its own hills,
    /// mountains, tables or pillars.
    #[serde(skip_serializing_if = "BiomeLand::is_keep")]
    pub land: BiomeLand,
    /// The vines that hang in it.
    #[serde(skip_serializing_if = "BiomeVines::is_none")]
    pub vines: BiomeVines,
    /// The colours its tinted blocks take (see `blocks::block::BlockTint`).
    #[serde(skip_serializing_if = "BiomeTints::is_none")]
    pub tints: BiomeTints,
}

/// The colours a biome's tinted blocks take, each a colour their own is
/// multiplied by, from 0 to 2 each part: white leaves a block as it is,
/// more than 1 brightens it. Its grass tint for grass, its foliage tint for
/// leaves; one left out is white. Where biomes meet, the tints fade from
/// one to another (see `blocks::generate::tints`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BiomeTints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grass: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foliage: Option<[f32; 3]>,
}

impl BiomeTints {
    /// The most a tint's part is.
    pub const MOST: f32 = 2.0;

    pub fn is_none(&self) -> bool {
        self.grass.is_none() && self.foliage.is_none()
    }

    /// Its grass tint and its foliage tint, white for one left out.
    pub fn colors(&self) -> [[f32; 3]; 2] {
        [self.grass.unwrap_or([1.0; 3]), self.foliage.unwrap_or([1.0; 3])]
    }
}

/// The vines of a biome: the block they are, one whose shape is a vine,
/// how many of the places a vine could hang from have one, how long they
/// hang, and how far under the ground's surface there still are any. A
/// vine hangs from the underside of the ground, under a ledge, an overhang
/// or a cave's roof, likelier beside a wall, and down through the open air
/// until it meets something.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BiomeVines {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// From 0 for none to 1 for every place.
    pub amount: f32,
    /// In blocks, from and to.
    pub length: [u32; 2],
    /// In metres under the ground's surface.
    pub depth: f32,
}

impl Default for BiomeVines {
    fn default() -> Self {
        Self { block: None, amount: 0.3, length: [2, 7], depth: 40.0 }
    }
}

impl BiomeVines {
    pub fn is_none(&self) -> bool {
        self.block.is_none()
    }
}

impl Default for BiomeDef {
    fn default() -> Self {
        Self {
            color: [0.45, 0.62, 0.3],
            temperature: [5.0, 22.0],
            humidity: [0.3, 0.7],
            height: None,
            sunk: None,
            ground: BiomeGround::default(),
            land: BiomeLand::default(),
            vines: BiomeVines::default(),
            tints: BiomeTints::default(),
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
    /// Its rock in layers, shown on cliffs and walls: none for its deep
    /// block all the way down (see `strata`).
    #[serde(skip_serializing_if = "RockLayers::is_none")]
    pub rock_layers: RockLayers,
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
            rock_layers: RockLayers::default(),
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
        let sunk = self.sunk.map_or(0.0, |range| Self::outside(range, climate.sunk, spreads.sunk));
        warmth * warmth + wetness * wetness + height * height + sunk * sunk
    }

    /// How much of the climate its ranges take in: the narrower, the less.
    fn reach(&self, spreads: Spreads) -> f32 {
        let span = |range: [f32; 2], spread: f32| ((range[1] - range[0]).abs() / spread.max(1e-6)).min(8.0);
        // One that keeps to a height comes before any that does not: peaks over whatever grows below them.
        let height = self.height.map_or(1000.0, |range| span(range, spreads.height));
        // And one that keeps to sunk ground before any that does not: a chasm's floor grows its own.
        let sunk = self.sunk.map_or(1000.0, |range| span(range, spreads.sunk));
        span(self.temperature, spreads.temperature) * span(self.humidity, spreads.humidity) * height * sunk
    }
}

/// A place's climate: how warm, how wet, how high over the sea, and how
/// far sunk under the land round about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub temperature: f32,
    pub humidity: f32,
    pub above_sea: f32,
    pub sunk: f32,
}

/// What one share of each measure is, for weighing them against each other.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spreads {
    temperature: f32,
    humidity: f32,
    height: f32,
    sunk: f32,
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
        let (temperature, humidity) = (temperature_range.max(1.0), humidity_range.max(0.05));
        let spreads = Spreads { temperature, humidity, height: 200.0, sunk: 40.0 };
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

    /// Which biome a region whose climate is `climate` is: the one it is
    /// nearest, as [`Biomes::of`] says, among those that do not keep to a
    /// height or to sunk ground, which lie over regions rather than making
    /// their own (see `regions`). Among all where every one keeps to some.
    pub fn of_region(&self, climate: Place) -> Option<usize> {
        let own = |biome: &BiomeDef| biome.height.is_none() && biome.sunk.is_none();
        if !self.list.iter().any(own) {
            return self.of(climate);
        }
        let mut best: Option<(usize, f32, f32)> = None;
        for (at, biome) in self.list.iter().enumerate().filter(|(_, biome)| own(biome)) {
            let (away, reach) = (biome.away(climate, self.spreads), biome.reach(self.spreads));
            let nearer = best.is_none_or(|(_, least, narrowest)| away < least || (away == least && reach < narrowest));
            if nearer {
                best = Some((at, away, reach));
            }
        }
        best.map(|(at, ..)| at)
    }

    /// The biome keeping to a height or to sunk ground that a place with
    /// `climate` is within every range of, where there is one: the
    /// narrowest such, as among biomes a place is equally within.
    pub fn over(&self, climate: Place) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (at, biome) in self.list.iter().enumerate() {
            if biome.height.is_none() && biome.sunk.is_none() || biome.away(climate, self.spreads) > 0.0 {
                continue;
            }
            let reach = biome.reach(self.spreads);
            if best.is_none_or(|(_, narrowest)| reach < narrowest) {
                best = Some((at, reach));
            }
        }
        best.map(|(at, _)| at)
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
            sunk: climate.sunk,
        })
    }
}

/// The blocks of one of a world's biomes, by the numbers the world gives
/// them: air for one the biome leaves to the world.
#[derive(Clone, Debug, PartialEq)]
pub struct BiomeBlocks {
    pub top: BlockId,
    pub under: BlockId,
    pub under_depth: u32,
    pub deep: BlockId,
    pub steep: BlockId,
    /// As rise over run.
    pub steep_slope: f32,
    pub shore: BlockId,
    pub shore_height: f32,
    /// Its vines: their block (air for none), how many of the places one
    /// could hang from have one, how long they hang in blocks, and how far
    /// under the surface they still hang, in metres.
    pub vine: BlockId,
    pub vine_amount: f32,
    pub vine_length: [u32; 2],
    pub vine_depth: f32,
    /// Its rock in layers, where it has any.
    pub strata: Option<std::sync::Arc<Strata>>,
}

impl Default for BiomeBlocks {
    fn default() -> Self {
        Self {
            top: AIR,
            under: AIR,
            under_depth: 3,
            deep: AIR,
            steep: AIR,
            steep_slope: 0.84,
            shore: AIR,
            shore_height: 2.0,
            vine: AIR,
            vine_amount: 0.0,
            vine_length: [2, 7],
            vine_depth: 40.0,
            strata: None,
        }
    }
}

impl BiomeBlocks {
    /// These with a biome's vines, their block numbered by `number`.
    pub fn with_vines(self, vines: &BiomeVines, mut number: impl FnMut(&str) -> BlockId) -> Self {
        let vine = vines.block.as_deref().map_or(AIR, &mut number);
        let [from, to] = vines.length;
        Self {
            vine,
            vine_amount: if vine == AIR { 0.0 } else { vines.amount.clamp(0.0, 1.0) },
            vine_length: [from.min(to).max(1), from.max(to).max(1)],
            vine_depth: vines.depth.max(0.0),
            ..self
        }
    }

    /// A biome's ground with each block it names numbered by `number`: air
    /// for one it does not name or the world cannot number.
    pub fn of(ground: &BiomeGround, mut number: impl FnMut(&str) -> BlockId) -> Self {
        let strata = Strata::of(&ground.rock_layers, &mut number).map(std::sync::Arc::new);
        let mut block = |named: &Option<String>| named.as_deref().map_or(AIR, &mut number);
        Self {
            strata,
            top: block(&ground.top),
            under: block(&ground.under),
            under_depth: ground.under_depth,
            deep: block(&ground.deep),
            steep: block(&ground.steep),
            steep_slope: slope_of(ground.steep_slope),
            shore: block(&ground.shore),
            shore_height: ground.shore_height.max(0.0),
            ..Self::default()
        }
    }

    /// The top and under blocks of ground `above_sea` metres over the sea
    /// that rises `slope` for every metre along: its shore's near the sea,
    /// its steep ground's where it is steep, its own elsewhere. Air for one
    /// left to the world.
    pub fn at(&self, above_sea: f32, slope: f32) -> (BlockId, BlockId) {
        if self.steep != AIR && slope >= self.steep_slope {
            (self.steep, self.steep)
        } else if self.shore != AIR && above_sea.abs() <= self.shore_height {
            (self.shore, if self.under == AIR { self.shore } else { self.under })
        } else {
            (self.top, self.under)
        }
    }
}

/// How much ground at `degrees` rises for every metre along, with sums and
/// products alone: the tangent, good to a part in a thousand under 80°.
pub(crate) fn slope_of(degrees: f32) -> f32 {
    let x = degrees.clamp(0.0, 85.0) * (std::f32::consts::PI / 180.0);
    let x2 = x * x;
    // Sine and cosine by their series, as far as they matter here.
    let sin = x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
    let cos = 1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0)));
    sin / cos.max(1e-3)
}

/// A world's biomes with the blocks of each, as the world numbers them.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBiomes {
    pub biomes: Biomes,
    pub blocks: Vec<BiomeBlocks>,
    /// Where they keep to regions, the regions: one biome across each
    /// (see `regions`). None, and each place is the biome its climate is
    /// nearest.
    pub regions: Option<std::sync::Arc<super::Regions>>,
    /// Its painted maps, whose biome map is chosen before the climate rules
    /// where it is painted (see `painted`).
    pub painted: Option<super::painted::Painted>,
}

impl WorldBiomes {
    /// The biome painted at `(x, z)`, by its place in the list: the nearest
    /// pixel of a place moved a little by noise, so painted biomes meet with
    /// ragged edges.
    fn painted_at(&self, x: f32, z: f32) -> Option<usize> {
        let maps = self.painted.as_ref()?.maps();
        if maps.biome.is_empty() {
            return None;
        }
        let spacing = maps.biome.spacing;
        let wander = |part: u32| {
            let size = spacing * 3.0;
            let noise = crate::noise::fractal(crate::noise::NoiseStyle::Fractal, x / size, z / size, 2, part);
            (noise * 2.0 - 1.0) * spacing * 1.5
        };
        let value = maps.biome.nearest(x + wander(0x51), z + wander(0x7a))?;
        let index = value.round();
        (index >= 0.0 && (index as usize) < self.blocks.len().max(self.biomes.list().len())).then_some(index as usize)
    }

    /// Whether any of them has blocks for steep ground, so slopes are worth
    /// working out.
    pub fn any_steep(&self) -> bool {
        self.blocks.iter().any(|blocks| blocks.steep != AIR)
    }

    /// Whether any of them keeps to sunk ground, so how far the ground is
    /// sunk is worth asking.
    pub fn any_sunk(&self) -> bool {
        self.biomes.list().iter().any(|biome| biome.sunk.is_some())
    }

    /// Whether it makes the ground `other` makes: the same biomes in the
    /// same places, of the same blocks, whatever colours they show on maps.
    pub fn same_ground(&self, other: &Self) -> bool {
        let lies = |biome: &BiomeDef| (biome.temperature, biome.humidity, biome.height, biome.sunk, biome.land);
        let (ours, theirs) = (&self.biomes, &other.biomes);
        self.blocks == other.blocks
            && (ours.spreads, ours.wander, ours.wander_size, ours.seed)
                == (theirs.spreads, theirs.wander, theirs.wander_size, theirs.seed)
            && ours.list.iter().map(lies).eq(theirs.list.iter().map(lies))
            && self.regions == other.regions
    }

    /// The blocks of the biome the place at `(x, z)` is, with `climate`
    /// there; none where the world lists no biomes.
    pub fn at(&self, x: f32, z: f32, climate: Place) -> Option<&BiomeBlocks> {
        self.index_at(x, z, climate).and_then(|biome| self.blocks.get(biome))
    }

    /// How far over the lowest level the floor of the area the place at
    /// `(x, z)`, `height` metres high, is in stands, in metres, where the
    /// world's land lies in levels; nothing elsewhere. What keeps to a
    /// height over the sea, a block's cover or a structure's site, counts
    /// it from the floor. Never so far that ground over the lowest level is
    /// counted under it, as a ramp down from a high level is.
    pub fn lift(&self, x: f32, z: f32, height: f32) -> f32 {
        let Some(areas) = self.regions.as_ref().and_then(|regions| regions.areas()) else { return 0.0 };
        let floor = areas.floor_at(x, z) - areas.plan.low;
        floor.min(height - areas.plan.low).max(0.0)
    }

    /// Which of them the place at `(x, z)` is, by its place in the list: by
    /// its region where they keep to regions, else by its climate.
    pub fn index_at(&self, x: f32, z: f32, climate: Place) -> Option<usize> {
        if let Some(index) = self.painted_at(x, z) {
            return Some(index);
        }
        match &self.regions {
            Some(regions) => regions.at(x, z, climate),
            None => self.biomes.at(x, z, climate),
        }
    }
}

/// A world's biomes as the GPU picks among them and lays their ground
/// (`biomeOf` and `biomeGround` in `voxear_render/shaders/voxel_world.slang`):
/// `words`, and the blocks they name, by their place in `blocks`, whose
/// colours the GPU is given beside them.
///
/// `words` holds how many biomes, how far and over how many metres their
/// borders wander, the seed, the spreads of warmth, wetness and height,
/// how many blocks, where the biomes' records start, where the painted
/// biome map starts (`u32::MAX` for none), a word for where the blocks'
/// colours start, which the renderer sets, and how large a block is, in
/// metres; then each biome's record of
/// [`GPU_BIOME_WORDS`]: its warmth, wetness and height (both `f32::MIN`
/// and `f32::MAX` for any), how much of the climate its ranges take in,
/// its top, under, deep, steep and shore blocks' places (`u32::MAX` for
/// none), how deep its under block goes in metres, how steep its steep
/// ground is, as rise over run, and how far over and under the sea its
/// shore is, and where its rock layers' words start (`u32::MAX` for none,
/// see [`super::Strata::gpu_words`]); then the painted biome map's words,
/// then the rock layers'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuBiomes {
    pub words: Vec<u32>,
    pub blocks: Vec<BlockId>,
}

/// The words of one biome's record in [`GpuBiomes::words`].
pub const GPU_BIOME_WORDS: usize = 16;

impl WorldBiomes {
    /// Its biomes for the GPU, each block a biome leaves to the world its
    /// world's own: `own` its top, under and deep blocks, `own_depth` how
    /// many blocks deep its under block goes; its blocks `block_size`
    /// metres a side.
    pub fn gpu(&self, own: [BlockId; 3], own_depth: u32, block_size: f32) -> GpuBiomes {
        let mut blocks: Vec<BlockId> = Vec::new();
        let mut place = |block: BlockId| -> u32 {
            if block == AIR {
                return u32::MAX;
            }
            match blocks.iter().position(|&known| known == block) {
                Some(at) => at as u32,
                None => {
                    blocks.push(block);
                    (blocks.len() - 1) as u32
                }
            }
        };
        let biomes = &self.biomes;
        let list = biomes.list();
        let mut records = Vec::with_capacity(list.len() * GPU_BIOME_WORDS);
        // Each biome's rock layers, where it has any, placed after the painted map.
        let mut strata: Vec<(usize, Vec<u32>)> = Vec::new();
        for (at, biome) in list.iter().enumerate() {
            let ground = self.blocks.get(at).cloned().unwrap_or_default();
            let or = |block: BlockId, own: BlockId| if block == AIR { own } else { block };
            let (low, high) = biome.height.map_or((f32::MIN, f32::MAX), |[low, high]| (low, high));
            let depth = if ground.under == AIR { own_depth } else { ground.under_depth } as f32 * block_size;
            records.extend([
                biome.temperature[0].to_bits(),
                biome.temperature[1].to_bits(),
                biome.humidity[0].to_bits(),
                biome.humidity[1].to_bits(),
                low.to_bits(),
                high.to_bits(),
                biome.reach(biomes.spreads).to_bits(),
                place(or(ground.top, own[0])),
                place(or(ground.under, own[1])),
                place(or(ground.deep, own[2])),
                place(ground.steep),
                ground.steep_slope.to_bits(),
                place(ground.shore),
                ground.shore_height.to_bits(),
                depth.to_bits(),
                u32::MAX,
            ]);
            if let Some(layers) = &ground.strata {
                strata.push((records.len() - 1, layers.gpu_words(&mut place)));
            }
        }
        let painted = self.painted.as_ref().map(|painted| painted.maps()).filter(|maps| !maps.biome.is_empty());
        let header = 12;
        let painted_at = painted.as_ref().map_or(u32::MAX, |_| (header + records.len()) as u32);
        let spreads = biomes.spreads;
        let mut words = vec![
            list.len() as u32,
            biomes.wander.to_bits(),
            biomes.wander_size.to_bits(),
            biomes.seed,
            spreads.temperature.to_bits(),
            spreads.humidity.to_bits(),
            spreads.height.to_bits(),
            blocks.len() as u32,
            header as u32,
            painted_at,
            0,
            block_size.to_bits(),
        ];
        words.extend(records);
        if let Some(maps) = painted {
            words.extend(maps.biome.gpu_words());
        }
        for (record, layers) in strata {
            words[header + record] = words.len() as u32;
            words.extend(layers);
        }
        GpuBiomes { words, blocks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_biomes_blocks_are_its_shores_near_the_sea_its_steep_grounds_where_steep_and_its_own_elsewhere() {
        let ground = BiomeGround {
            top: Some("grass".into()),
            under: Some("dirt".into()),
            steep: Some("rock".into()),
            shore: Some("sand".into()),
            deep: Some("missing".into()),
            ..BiomeGround::default()
        };
        let number =
            |name: &str| ["", "grass", "dirt", "rock", "sand"].iter().position(|n| *n == name).unwrap_or(0) as BlockId;
        let blocks = BiomeBlocks::of(&ground, number);
        assert_eq!((blocks.top, blocks.under, blocks.steep, blocks.shore, blocks.deep), (1, 2, 3, 4, AIR));
        // 40 degrees is 0.839 in 1.
        assert!((blocks.steep_slope - 0.8391).abs() < 1e-3, "{}", blocks.steep_slope);
        assert!((slope_of(45.0) - 1.0).abs() < 1e-3 && (slope_of(70.0) - 2.7475).abs() < 3e-3);
        assert_eq!(blocks.at(30.0, 0.2), (1, 2), "grass over dirt on gentle ground");
        assert_eq!(blocks.at(30.0, 1.0), (3, 3), "rock where it is steep");
        assert_eq!(blocks.at(1.5, 0.2), (4, 2), "sand by the sea, and just under it");
        assert_eq!(blocks.at(-1.5, 0.2), (4, 2));
        assert_eq!(blocks.at(1.5, 1.0), (3, 3), "steep ground by the sea is rock still");
        // One that names nothing leaves all to the world.
        assert_eq!(BiomeBlocks::of(&BiomeGround::default(), number).at(1.0, 2.0), (AIR, AIR));
    }

    fn biome(temperature: [f32; 2], humidity: [f32; 2], height: Option<[f32; 2]>) -> BiomeDef {
        BiomeDef { temperature, humidity, height, ..BiomeDef::default() }
    }

    fn place(temperature: f32, humidity: f32, above_sea: f32) -> Place {
        Place { temperature, humidity, above_sea, sunk: 0.0 }
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
    fn a_biome_kept_to_sunk_ground_is_a_chasms_floor_and_nowhere_else() {
        let forest = biome([8.0, 22.0], [0.4, 1.0], None);
        let desert = biome([22.0, 45.0], [0.0, 0.25], None);
        let hollow = BiomeDef { sunk: Some([40.0, 2000.0]), ..biome([-40.0, 45.0], [0.0, 1.0], None) };
        let biomes = Biomes::new(vec![forest, desert, hollow], 15.0, 0.3, 1);
        let sunk = |sunk: f32, temperature: f32| Place { sunk, ..place(temperature, 0.6, 20.0) };
        // Down a chasm, whatever the climate over it; on the land over it, and part way down, the land's own.
        assert_eq!(biomes.of(sunk(120.0, 15.0)), Some(2));
        assert_eq!(biomes.of(sunk(60.0, 35.0)), Some(2));
        assert_eq!(biomes.of(sunk(0.0, 15.0)), Some(0));
        assert_eq!(biomes.of(sunk(10.0, 15.0)), Some(0));
        let text = biomes.list()[2].to_text();
        assert!(text.contains("sunk"), "{text}");
        assert_eq!(BiomeDef::parse(&text).unwrap(), biomes.list()[2]);
    }

    #[test]
    fn a_worlds_biomes_make_the_same_ground_whatever_their_colours_and_other_ground_once_they_lie_elsewhere() {
        let of = |list: Vec<BiomeDef>, top: BlockId| WorldBiomes {
            painted: None,
            regions: None,
            blocks: list.iter().map(|_| BiomeBlocks { top, ..BiomeBlocks::default() }).collect(),
            biomes: Biomes::new(list, 15.0, 0.3, 1),
        };
        let cool = biome([-40.0, 15.0], [0.0, 1.0], None);
        let world = of(vec![cool.clone()], 4);
        let recoloured = of(vec![BiomeDef { color: [1.0, 0.0, 0.0], ..cool.clone() }], 4);
        assert!(world.same_ground(&recoloured) && world != recoloured);
        assert!(!world.same_ground(&of(vec![cool.clone()], 5)), "another block on top");
        assert!(!world.same_ground(&of(vec![BiomeDef { height: Some([0.0, 100.0]), ..cool.clone() }], 4)));
        assert!(!world.same_ground(&of(vec![cool.clone(), cool], 4)), "one biome more");
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
