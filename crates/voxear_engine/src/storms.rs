//! Storms: weather with a place and a size, that drifts, grows and dies, so
//! one is seen coming over the sea and walked out of.
//!
//! A [`Storm`] lies over a round part of the level. How hard it blows at a
//! point is its strength now, fading towards its reach; what falls there is
//! the storm's kind, which left to itself is whatever the open air's
//! temperature at that point makes it (`crate::heat::open_air`): rain where
//! it is mild, with thunder when the storm is strong, sleet round freezing,
//! snow and blizzards below it, and blown sand where it is hot and dry. So
//! one storm rains in a valley and snows on the peak over it.
//!
//! **Natural storms** are worked out from the world's seed and the storms'
//! clock alone ([`natural`]): every machine with the same two has the same
//! storms, with nothing sent between them. **Made storms** are started by
//! scripts and kept in [`Storms`], a world resource, with the clock: a
//! handful of numbers each, which is all a save or a joining player needs.
//!
//! Each frame [`update`] turns the clock, moves the made storms, and gives
//! the renderer what lies near the viewer: the weather over each place
//! (`voxear_render::weather::WeatherPlaces`, drawn from afar as cloud and a
//! curtain under it), what that adds where the viewer stands
//! (`Weather::set_here`), and the sea under it
//! (`voxear_render::sea::RoughSea`), with its whirlpools: those strong
//! storms raise over deep water, the great ones that stand at seeded places
//! in a generated sea, and those scripts make.

mod natural;
pub(crate) mod scripts;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{DVec2, Vec2, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{
    sea::{RoughSea, SeaLevel, SeaLook, SeaSampler, Whirlpool as DrawnWhirlpool},
    weather::{Weather, WeatherAmounts, WeatherPlace, WeatherPlaces},
};
use voxear_scene::{Destroying, Transform, WorldOrigin, WorldTransform};
use voxear_terrain::generate::WorldGen;

pub use natural::{GENERATION, natural_storms, natural_whirlpools};

use crate::{Content, blocks::BlockWorldAsset};

/// What a storm is told to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StormKind {
    /// Whatever the temperature makes it at each place: rain, thunder,
    /// sleet, snow, a blizzard, or blown sand where it is hot and dry.
    #[default]
    ByTemperature,
    /// Rain wherever it is, with no thunder.
    Rain,
    /// Rain with thunder and lightning.
    Thunderstorm,
    /// Driving snow, whatever the temperature.
    Blizzard,
    /// A gale full of blown sand.
    Sandstorm,
    /// A bank of thick fog, still and quiet.
    FogBank,
}

impl StormKind {
    pub const ALL: [Self; 6] =
        [Self::ByTemperature, Self::Rain, Self::Thunderstorm, Self::Blizzard, Self::Sandstorm, Self::FogBank];

    pub fn name(self) -> &'static str {
        match self {
            Self::ByTemperature => "By Temperature",
            Self::Rain => "Rain",
            Self::Thunderstorm => "Thunderstorm",
            Self::Blizzard => "Blizzard",
            Self::Sandstorm => "Sandstorm",
            Self::FogBank => "Fog Bank",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&kind| kind == self).unwrap_or(0)
    }
}

/// What the weather is at a place, by name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Falling {
    #[default]
    Nothing,
    Rain,
    Thunderstorm,
    Sleet,
    Snow,
    Blizzard,
    Sandstorm,
    Fog,
}

impl Falling {
    pub const ALL: [Self; 8] = [
        Self::Nothing,
        Self::Rain,
        Self::Thunderstorm,
        Self::Sleet,
        Self::Snow,
        Self::Blizzard,
        Self::Sandstorm,
        Self::Fog,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&falling| falling == self).unwrap_or(0)
    }

    /// What weather adding `amounts` is called.
    pub fn of(amounts: WeatherAmounts) -> Self {
        let WeatherAmounts { rain, snow, dust, fog, wind, lightning, .. } = amounts;
        match () {
            () if dust > 0.3 => Self::Sandstorm,
            () if rain > 0.12 && snow > 0.12 => Self::Sleet,
            () if snow > 0.6 && wind > 0.7 => Self::Blizzard,
            () if snow > 0.05 => Self::Snow,
            () if rain > 0.05 && lightning > 0.2 => Self::Thunderstorm,
            () if rain > 0.05 => Self::Rain,
            () if fog > 0.5 => Self::Fog,
            () => Self::Nothing,
        }
    }
}

/// The air is hot and dry enough for a storm left to itself to be sand:
/// this warm, in °C, and no wetter than this.
const SAND_FROM: f32 = 28.0;
const SAND_DRY: f32 = 0.25;
/// Between these, in °C, what falls turns from snow to rain.
const SNOW_BELOW: f32 = 0.0;
const RAIN_ABOVE: f32 = 2.0;

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a storm of `kind` blowing `strength` hard, from 0 to 1, adds to the
/// weather where the open air is `celsius` warm and `humidity` wet.
pub fn amounts(kind: StormKind, strength: f32, celsius: f32, humidity: f32) -> WeatherAmounts {
    let s = strength.clamp(0.0, 1.0);
    if s <= 0.0 {
        return WeatherAmounts::default();
    }
    // Its sky comes on before what falls from it.
    let sky = smooth(0.0, 0.25, s);
    let sand = WeatherAmounts { cloud: 0.4 * sky, darkness: 0.35 * s, wind: s, dust: s, ..Default::default() };
    // How much of what falls is snow: all of it below freezing, none a little over.
    let cold = |celsius: f32| 1.0 - smooth(SNOW_BELOW, RAIN_ABOVE, celsius);
    let falls = |cold: f32, thunder: f32| WeatherAmounts {
        cloud: sky,
        darkness: sky * (0.3 + 0.45 * s) * (1.0 - 0.4 * cold),
        // A blizzard is a whiteout.
        fog: sky * (0.1 + 0.2 * s + cold * 0.55 * s),
        wind: sky * (0.15 + 0.85 * s),
        rain: s * (1.0 - cold),
        snow: s * cold,
        lightning: thunder * (1.0 - cold),
        dust: 0.0,
    };
    match kind {
        StormKind::ByTemperature if celsius >= SAND_FROM && humidity <= SAND_DRY => sand,
        StormKind::ByTemperature => {
            // Thunder in a strong storm, the more where it is hot and wet.
            let sultry = if celsius > 24.0 && humidity > 0.6 { 1.0 } else { 0.8 };
            falls(cold(celsius), smooth(0.55, 0.9, s) * sultry)
        }
        StormKind::Rain => falls(0.0, 0.0),
        StormKind::Thunderstorm => falls(0.0, 0.3 + 0.7 * s),
        StormKind::Blizzard => falls(1.0, 0.0),
        StormKind::Sandstorm => sand,
        StormKind::FogBank => WeatherAmounts { cloud: 0.8 * sky, darkness: 0.1 * s, fog: s, ..Default::default() },
    }
}

/// How rough a storm of `kind` blowing `strength` hard makes the sea.
fn rough(kind: StormKind, strength: f32) -> f32 {
    match kind {
        StormKind::FogBank => 0.0,
        StormKind::Sandstorm => 0.3 * strength,
        _ => strength,
    }
}

/// A storm: where its middle is, how far it reaches and how hard it blows
/// at its height, how it moves, and how long it lives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storm {
    /// Its number: a made storm's from 1 up, a natural one's from its place
    /// and time with the top bit set.
    pub id: u32,
    pub kind: StormKind,
    /// Its middle, in metres from the level's middle along x and z.
    pub middle: [f64; 2],
    /// How far it reaches, in metres.
    pub radius: f32,
    /// How hard it blows at its height, from 0 to 1.
    pub strength: f32,
    /// How fast it drifts, in metres a second along x and z.
    pub velocity: [f32; 2],
    /// How much of its reach its calm eye takes, from 0 for none.
    pub eye: f32,
    /// When it began, on the storms' clock, in seconds; how long it takes
    /// to come on and to die away; and how long it lives in all, without
    /// end where that is not finite.
    pub born: f64,
    pub grows: f32,
    pub fades: f32,
    #[serde(with = "without_end")]
    pub lasts: f32,
    /// Places it makes for one after another, at the speed it drifts, in
    /// metres from the level's middle; it stands at the last.
    pub path: Vec<[f64; 2]>,
}

impl Default for Storm {
    fn default() -> Self {
        Self {
            id: 0,
            kind: StormKind::ByTemperature,
            middle: [0.0; 2],
            radius: 400.0,
            strength: 1.0,
            velocity: [0.0; 2],
            eye: 0.0,
            born: 0.0,
            grows: 60.0,
            fades: 90.0,
            lasts: f32::INFINITY,
            path: Vec::new(),
        }
    }
}

/// A life without end is written as 0, as files hold no number for it.
mod without_end {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(lasts: &f32, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_f32(if lasts.is_finite() { *lasts } else { 0.0 })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<f32, D::Error> {
        let lasts = f32::deserialize(from)?;
        Ok(if lasts > 0.0 { lasts } else { f32::INFINITY })
    }
}

/// A natural storm's number has this bit, a made one's never.
pub const NATURAL: u32 = 0x8000_0000;

impl Storm {
    /// Whether it was worked out from the world's seed, not made.
    pub fn is_natural(&self) -> bool {
        self.id & NATURAL != 0
    }

    /// How hard it blows at its middle at `clock`, from 0 to 1: coming on
    /// from when it began, and dying away to the end of its life.
    pub fn strength_at(&self, clock: f64) -> f32 {
        let age = (clock - self.born) as f32;
        if age < 0.0 || age >= self.lasts {
            return 0.0;
        }
        let on = if self.grows > 0.0 { smooth(0.0, self.grows, age) } else { 1.0 };
        let off = if self.lasts.is_finite() && self.fades > 0.0 { smooth(0.0, self.fades, self.lasts - age) } else { 1.0 };
        self.strength.clamp(0.0, 1.0) * on * off
    }

    /// Whether its life is over at `clock`.
    pub fn is_over(&self, clock: f64) -> bool {
        (clock - self.born) as f32 >= self.lasts
    }

    /// How far it reaches at `clock`: it spreads as it comes on.
    pub fn radius_at(&self, clock: f64) -> f32 {
        let age = (clock - self.born) as f32;
        let spread = if self.grows > 0.0 { 0.6 + 0.4 * smooth(0.0, self.grows, age) } else { 1.0 };
        self.radius.max(1.0) * spread
    }

    /// How hard it blows at a place `distance` metres from its middle.
    pub fn strength_over(&self, clock: f64, distance: f32) -> f32 {
        self.strength_at(clock) * self.place(clock, WeatherAmounts::default(), Vec2::ZERO).share(distance)
    }

    /// The weather over its place, adding `amounts` at its middle, which is
    /// at `middle` in the world.
    fn place(&self, clock: f64, amounts: WeatherAmounts, middle: Vec2) -> WeatherPlace {
        let strength = self.strength_at(clock);
        WeatherPlace { middle, radius: self.radius_at(clock), eye: self.eye, amounts, rough: rough(self.kind, strength) }
    }

    /// Moves a made storm on by `delta` seconds: along its path where it
    /// has one, else as it drifts.
    fn drift(&mut self, delta: f32) {
        let speed = Vec2::from(self.velocity).length();
        let Some(&next) = self.path.first() else {
            self.middle[0] += f64::from(self.velocity[0] * delta);
            self.middle[1] += f64::from(self.velocity[1] * delta);
            return;
        };
        let to = DVec2::from(next) - DVec2::from(self.middle);
        let step = f64::from(speed * delta);
        if to.length() <= step || speed <= 0.0 {
            // There: on to the next, or standing at the last.
            if speed > 0.0 {
                self.middle = next;
                self.path.remove(0);
            }
            if self.path.is_empty() {
                self.velocity = [0.0; 2];
            }
            return;
        }
        let along = to.normalize();
        self.velocity = (along * f64::from(speed)).as_vec2().to_array();
        self.middle = (DVec2::from(self.middle) + along * step).to_array();
    }
}

/// How the level's storms and its sea in rough weather are set up: the
/// level's first, a component on any Obj. Without one there are no natural
/// storms, and the sea under a made one is drawn as small cubes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StormRules {
    /// Whether storms come and go by themselves.
    pub natural: bool,
    /// How often, from 0 for never to 1.
    pub frequency: f32,
    /// How far the largest reaches, in metres.
    pub largest: f32,
    /// Whether great storms stand for ever at places of their own, each
    /// round a calm eye.
    pub standing: bool,
    /// What tells this level's storms from another's, with its generator's
    /// seed.
    pub seed: u32,
    /// How a rough sea is drawn.
    pub sea_look: SeaLook,
    /// How far the deepest trough of the roughest sea lies under a crest,
    /// in metres.
    pub swell: f32,
    /// Whether strong storms raise whirlpools over deep water.
    pub whirlpools: bool,
    /// Whether great whirlpools stand at places of their own in a generated
    /// sea.
    pub maelstroms: bool,
    /// How much warm seas glow at night where the water breaks, from 0.
    pub glow: f32,
}

impl Default for StormRules {
    fn default() -> Self {
        Self {
            natural: true,
            frequency: 0.35,
            largest: 900.0,
            standing: true,
            seed: 0,
            sea_look: SeaLook::default(),
            swell: 3.0,
            whirlpools: true,
            maelstroms: true,
            glow: 0.6,
        }
    }
}

impl Component for StormRules {}

/// A whirlpool a script made: its number, where its middle is, in metres
/// from the level's middle along x and z, how far across its funnel reaches
/// and how deep its middle is drawn down, in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MadeWhirlpool {
    pub id: u32,
    pub middle: [f64; 2],
    pub radius: f32,
    pub depth: f32,
}

/// What scripts hear of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StormEvent {
    /// A storm reached the viewer.
    Arrived(u32),
    /// A storm left the viewer, or died over them.
    Left(u32),
    /// Something floating was drawn down a whirlpool's middle.
    Swallowed(Entity),
}

/// How hard a storm must blow on the viewer to have arrived, and how
/// little to have left.
const ARRIVES: f32 = 0.12;
const LEAVES: f32 = 0.06;
/// How far from the viewer storms are looked for, in metres: as far as
/// they are drawn.
pub const SEEN_FROM: f64 = 9000.0;
/// How far from the viewer whirlpools are looked for, in metres.
const POOLS_FROM: f64 = 1500.0;

/// The level's storms, a world resource: the clock they run on, in seconds,
/// the same on every machine; those scripts made; and, this frame, every
/// storm near the viewer.
#[derive(Clone, Debug, Default)]
pub struct Storms {
    pub clock: f64,
    pub made: Vec<Storm>,
    pub whirlpools: Vec<MadeWhirlpool>,
    next: u32,
    /// Every storm near the viewer this frame, natural and made, and where
    /// the viewer is, in metres from the level's middle.
    near: Vec<Storm>,
    around: Option<DVec2>,
    /// Those blowing on the viewer.
    over: Vec<u32>,
    events: Vec<StormEvent>,
    /// The seed natural storms are worked out from, and the generator that
    /// knows how deep the sea is and where, with where the level's middle
    /// is in it.
    seed: u32,
    generator: Option<(Arc<WorldGen>, [f64; 2])>,
    rules: Option<StormRules>,
    /// The rules scripts asked for, where no Obj holds any.
    asked: Option<StormRules>,
}

/// What a save keeps of the storms, and what a player joining is sent: the
/// clock and what scripts made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StormsSaved {
    pub clock: f64,
    pub made: Vec<Storm>,
    pub whirlpools: Vec<MadeWhirlpool>,
    pub next: u32,
}

impl Storms {
    /// What a save keeps.
    pub fn saved(&self) -> StormsSaved {
        StormsSaved { clock: self.clock, made: self.made.clone(), whirlpools: self.whirlpools.clone(), next: self.next }
    }

    /// The storms as a save kept them.
    pub fn restore(&mut self, saved: StormsSaved) {
        self.clock = saved.clock;
        self.made = saved.made;
        self.whirlpools = saved.whirlpools;
        self.next = saved.next;
        self.over.clear();
    }

    /// Starts a storm: its number.
    pub fn start(&mut self, storm: Storm) -> u32 {
        self.next = (self.next + 1) & !NATURAL;
        let id = self.next.max(1);
        self.made.push(Storm { id, born: self.clock, ..storm });
        id
    }

    /// The made storm with a number, to change.
    pub fn made_mut(&mut self, id: u32) -> Option<&mut Storm> {
        self.made.iter_mut().find(|storm| storm.id == id)
    }

    /// A storm near the viewer, or any made one, by its number.
    pub fn get(&self, id: u32) -> Option<&Storm> {
        self.made.iter().chain(&self.near).find(|storm| storm.id == id)
    }

    /// Has a made storm die away over `seconds` from now; false for one
    /// that is not there, or is natural.
    pub fn stop(&mut self, id: u32, seconds: f32) -> bool {
        let clock = self.clock;
        let Some(storm) = self.made_mut(id) else { return false };
        let age = (clock - storm.born) as f32;
        storm.fades = seconds.max(0.0);
        storm.lasts = age + storm.fades;
        true
    }

    /// Every storm near the viewer this frame, natural and made, and every
    /// made one wherever it is.
    pub fn all(&self) -> Vec<&Storm> {
        let far = self.made.iter().filter(|storm| !self.near.iter().any(|near| near.id == storm.id));
        self.near.iter().chain(far).collect()
    }

    /// The numbers of the storms blowing on the viewer.
    pub fn over_viewer(&self) -> &[u32] {
        &self.over
    }

    /// Makes a whirlpool: its number.
    pub fn add_whirlpool(&mut self, middle: [f64; 2], radius: f32, depth: f32) -> u32 {
        self.next = (self.next + 1) & !NATURAL;
        let id = self.next.max(1);
        self.whirlpools.push(MadeWhirlpool { id, middle, radius: radius.max(1.0), depth: depth.max(0.0) });
        id
    }

    /// Takes a made whirlpool away; false where there is none.
    pub fn remove_whirlpool(&mut self, id: u32) -> bool {
        let before = self.whirlpools.len();
        self.whirlpools.retain(|pool| pool.id != id);
        self.whirlpools.len() != before
    }

    /// What has happened since they were last taken.
    pub fn take_events(&mut self) -> Vec<StormEvent> {
        std::mem::take(&mut self.events)
    }

    /// The oldest thing that has happened and not been taken.
    pub fn next_event(&mut self) -> Option<StormEvent> {
        (!self.events.is_empty()).then(|| self.events.remove(0))
    }

    /// Every storm within `reach` metres of `around`, in metres from the
    /// level's middle, at the clock's time: the natural ones the rules
    /// allow and those made, the nearest first.
    fn storms_near(&self, around: DVec2, reach: f64) -> Vec<Storm> {
        let mut found: Vec<Storm> = match self.rules {
            Some(rules) if rules.natural => {
                let humidity = |at: DVec2| self.wetness(at);
                natural_storms(self.seed ^ rules.seed, &rules, self.clock, around, reach, &humidity)
            }
            _ => Vec::new(),
        };
        let live = |storm: &&Storm| {
            let far = DVec2::from(storm.middle).distance(around);
            storm.strength_at(self.clock) > 0.0 && far < reach + f64::from(storm.radius)
        };
        found.extend(self.made.iter().filter(live).cloned());
        found.sort_by(|a, b| {
            let far = |storm: &Storm| DVec2::from(storm.middle).distance(around) - f64::from(storm.radius);
            far(a).total_cmp(&far(b)).then(a.id.cmp(&b.id))
        });
        found
    }

    /// How wet the generator's climate is at a place, in metres from the
    /// level's middle; middling without a generator.
    fn wetness(&self, at: DVec2) -> (f32, f32) {
        self.generator.as_ref().map_or((15.0, 0.5), |(generator, offset)| {
            generator.climate((at.x + offset[0]) as f32, (at.y + offset[1]) as f32, generator.sea_level())
        })
    }

    /// How far under the sea the generator's ground is at a place, in
    /// metres from the level's middle: deep with no generator, as a sea
    /// built by hand is taken to be.
    fn sea_depth(&self, at: DVec2) -> f32 {
        self.generator.as_ref().map_or(f32::INFINITY, |(generator, offset)| {
            generator.sea_level() - generator.height_at((at.x + offset[0]) as f32, (at.y + offset[1]) as f32)
        })
    }
}

/// The weather at a point: what it adds there, what that is called, how
/// hard the hardest storm blows there and which it is, and how warm and wet
/// the open air is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherAt {
    pub amounts: WeatherAmounts,
    pub falling: Falling,
    pub strength: f32,
    pub storm: u32,
    pub celsius: f32,
    pub humidity: f32,
}

/// The weather at `at`, in the world.
pub fn weather_at(world: &World, at: Vec3) -> WeatherAt {
    let (celsius, humidity) = crate::heat::open_air(world, at);
    let level = world.resource::<Weather>().map(Weather::everywhere).unwrap_or_default();
    let mut found = WeatherAt { amounts: level, celsius, humidity, ..WeatherAt::default() };
    let Some(storms) = world.resource::<Storms>() else {
        return WeatherAt { falling: Falling::of(level), ..found };
    };
    let origin = WorldOrigin::of(world);
    let place = origin.to_level(at);
    let place = DVec2::new(place.x, place.z);
    // Those near the viewer are at hand; a point far from the viewer has its own looked for.
    let looked;
    let near = match storms.around.is_some_and(|around| around.distance(place) < SEEN_FROM * 0.5) {
        true => &storms.near,
        false => {
            looked = storms.storms_near(place, 0.0);
            &looked
        }
    };
    for storm in near {
        let distance = DVec2::from(storm.middle).distance(place) as f32;
        let strength = storm.strength_over(storms.clock, distance);
        if strength <= 0.0 {
            continue;
        }
        found.amounts = found.amounts.most(amounts(storm.kind, strength, celsius, humidity));
        if strength > found.strength {
            (found.strength, found.storm) = (strength, storm.id);
        }
    }
    found.falling = Falling::of(found.amounts);
    found
}

/// Gives each world of blocks with a generated sea its [`SeaLevel`], and
/// the storms the generator that knows the climate and the sea's depth.
pub fn prepare(world: &mut World, content: &mut Content) {
    let mut worlds: Vec<(Entity, BlockWorldAsset, Vec3)> = world
        .query_ref::<(Entity, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, asset, local, placed)| (entity, asset.clone(), voxear_terrain::corner(local, placed)))
        .collect();
    worlds.sort_by_key(|(entity, ..)| entity.to_bits());
    let origin = WorldOrigin::of(world).0.as_dvec3();
    let mut first = None;
    for (entity, asset, corner) in worlds {
        let generate = asset.file.generate.as_ref();
        let generator = crate::blocks::generator(content, &asset.file);
        let top = generate.zip(generator.as_deref()).and_then(|(generate, generator)| {
            voxear_terrain::blocks::generate::sea_top(&asset.file, generate, generator)
        });
        match (top, world.get::<SeaLevel>(entity).copied()) {
            (Some(top), had) if had != Some(SeaLevel(top)) => drop(world.insert(entity, SeaLevel(top))),
            (None, Some(_)) => drop(world.remove::<SeaLevel>(entity)),
            _ => {}
        }
        if let (None, Some(generator), Some(generate)) = (&first, generator, generate) {
            // Where the level's middle is in the generator's world.
            let level = corner.as_dvec3() + origin;
            let offset = [f64::from(generate.position[0]) - level.x, f64::from(generate.position[1]) - level.z];
            first = Some((generator, offset));
        }
    }
    let mut found: Vec<(Entity, StormRules)> =
        world.query_ref::<(Entity, &StormRules)>().without::<Destroying>().into_iter().map(|(e, r)| (e, *r)).collect();
    found.sort_by_key(|(entity, _)| entity.to_bits());
    let rules = found.first().map(|(_, rules)| *rules);
    if rules.is_none() && first.is_none() && world.resource::<Storms>().is_none() {
        return;
    }
    let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
    let rules = rules.or(storms.asked);
    storms.seed = first.as_ref().map_or(0, |(generator, _)| generator.seed());
    storms.generator = first;
    storms.rules = rules;
    world.insert_resource(storms);
}

/// The sea's swell and whirlpools this frame, as what floats asks about
/// them: a world resource.
pub struct SeaNow(pub SeaSampler);

/// Turns the storms' clock by `delta` seconds while `playing`, moves the
/// made storms, and gives the renderer the weather over each place near
/// the viewer, what it adds where the viewer stands, and the sea under it.
pub fn update(world: &mut World, delta: f32, playing: bool) {
    let Some(mut storms) = world.remove_resource::<Storms>() else { return };
    if playing {
        storms.clock += f64::from(delta.max(0.0));
        let clock = storms.clock;
        for storm in &mut storms.made {
            storm.drift(delta);
        }
        storms.made.retain(|storm| !storm.is_over(clock));
    }
    let rules = storms.rules.unwrap_or(StormRules { natural: false, ..StormRules::default() });
    let viewer = crate::terrain::viewer(world);
    let origin = WorldOrigin::of(world);
    let to_world = |at: [f64; 2]| Vec2::new((at[0] - origin.0.x as f64) as f32, (at[1] - origin.0.z as f64) as f32);
    let clock = storms.clock;
    let around = viewer.map(|viewer| {
        let level = origin.to_level(viewer);
        DVec2::new(level.x, level.z)
    });
    storms.near = around.map_or_else(Vec::new, |around| storms.storms_near(around, SEEN_FROM));
    storms.around = around;

    // The weather over each place, as warm as the open air at its middle makes it, and what it adds on the viewer.
    let mut places = Vec::with_capacity(storms.near.len());
    let mut here = WeatherAmounts::default();
    let mut over = Vec::new();
    for storm in &storms.near {
        let middle = to_world(storm.middle);
        let height = viewer.map_or(0.0, |viewer| viewer.y);
        let (celsius, humidity) = crate::heat::open_air(world, Vec3::new(middle.x, height, middle.y));
        let strength = storm.strength_at(clock);
        let place = storm.place(clock, amounts(storm.kind, strength, celsius, humidity), middle);
        if let Some(viewer) = viewer {
            let on_viewer = strength * place.share(middle.distance(Vec2::new(viewer.x, viewer.z)));
            let (celsius, humidity) = crate::heat::open_air(world, viewer);
            here = here.most(amounts(storm.kind, on_viewer, celsius, humidity));
            let was = storms.over.contains(&storm.id);
            if on_viewer > if was { LEAVES } else { ARRIVES } {
                over.push(storm.id);
            }
        }
        places.push(place);
    }
    for &id in over.iter().filter(|id| !storms.over.contains(id)) {
        storms.events.push(StormEvent::Arrived(id));
    }
    for &id in storms.over.iter().filter(|id| !over.contains(id)) {
        storms.events.push(StormEvent::Left(id));
    }
    storms.over = over;
    if let Some(weather) = world.resource_mut::<Weather>() {
        weather.set_here(here);
    } else if !here.is_none() {
        let mut weather = Weather::default();
        weather.set_here(here);
        world.insert_resource(weather);
    }

    // The sea: its whirlpools near the viewer, the nearest first.
    let mut pools: Vec<(f64, DrawnWhirlpool)> = Vec::new();
    if let Some(around) = around {
        let mut add = |middle: [f64; 2], radius: f32, depth: f32| {
            let far = DVec2::from(middle).distance(around);
            if far < POOLS_FROM + f64::from(radius) && depth > 0.0 {
                pools.push((far, DrawnWhirlpool { middle: to_world(middle), radius, depth }));
            }
        };
        for pool in &storms.whirlpools {
            add(pool.middle, pool.radius, pool.depth);
        }
        let seed = storms.seed ^ rules.seed;
        let deep = |at: DVec2| storms.sea_depth(at);
        if rules.maelstroms && storms.generator.is_some() {
            for pool in natural::natural_whirlpools(seed, around, POOLS_FROM, &deep) {
                add(pool.middle, pool.radius, pool.depth);
            }
        }
        if rules.whirlpools {
            for storm in &storms.near {
                if let Some(pool) = natural::storm_whirlpool(storm, clock, &deep) {
                    add(pool.middle, pool.radius, pool.depth);
                }
            }
        }
    }
    pools.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Warm seas glow at night where the water breaks.
    let glow = match (rules.glow > 0.0, viewer) {
        (true, Some(viewer)) => {
            let (celsius, _) = crate::heat::open_air(world, viewer);
            let night = crate::heat::climate(world).map_or(0.0, |climate| {
                let hour = climate.time_of_day;
                smooth(19.0, 21.0, hour).max(1.0 - smooth(4.5, 6.0, hour))
            });
            rules.glow * night * smooth(16.0, 24.0, celsius)
        }
        _ => 0.0,
    };
    let wind = world
        .query_ref::<(&voxear_render::WindZone, &Transform)>()
        .into_iter()
        .next()
        .map(|(_, transform)| transform.rotation * Vec3::Z);
    let heading = wind.map_or(0.0, |wind| wind.z.atan2(wind.x));
    let sea = RoughSea {
        look: rules.sea_look,
        height: rules.swell.max(0.0),
        everywhere: world.resource::<Weather>().map_or(0.0, |weather| {
            let level = weather.everywhere();
            (level.wind * (0.4 + 0.6 * level.rain.max(level.snow))).clamp(0.0, 1.0)
        }),
        heading,
        time: clock,
        whirlpools: pools.into_iter().map(|(_, pool)| pool).collect(),
        glow,
    };
    let sampler = sea.sampler(&places, [origin.0.x as f64, origin.0.z as f64]);
    match sampler.is_calm() {
        true => drop(world.remove_resource::<SeaNow>()),
        false => world.insert_resource(SeaNow(sampler)),
    }
    world.insert_resource(sea);
    world.insert_resource(WeatherPlaces(places));
    world.insert_resource(storms);
}

/// Starts a storm over `at`, in the world, reaching `radius` metres and
/// blowing `strength` hard: its number. The easy way in; change what
/// [`Storms::made_mut`] gives for the rest.
pub fn start(world: &mut World, at: Vec3, radius: f32, strength: f32, kind: StormKind) -> u32 {
    let level = WorldOrigin::of(world).to_level(at);
    let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
    let storm = Storm {
        kind,
        middle: [level.x, level.z],
        radius: radius.max(1.0),
        strength: strength.clamp(0.0, 1.0),
        ..Storm::default()
    };
    let id = storms.start(storm);
    world.insert_resource(storms);
    id
}

/// Makes a whirlpool over `at`, in the world: its number.
pub fn add_whirlpool(world: &mut World, at: Vec3, radius: f32, depth: f32) -> u32 {
    let level = WorldOrigin::of(world).to_level(at);
    let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
    let id = storms.add_whirlpool([level.x, level.z], radius, depth);
    world.insert_resource(storms);
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::{Camera, weather::WeatherKind};

    #[test]
    fn a_storms_kind_is_what_the_temperature_makes_it_where_it_blows() {
        let at = |celsius: f32, humidity: f32, strength: f32| {
            let adds = amounts(StormKind::ByTemperature, strength, celsius, humidity);
            (Falling::of(adds), adds)
        };
        let (mild, rain) = at(14.0, 0.6, 0.5);
        assert_eq!(mild, Falling::Rain);
        assert!(rain.rain == 0.5 && rain.snow == 0.0 && rain.lightning == 0.0 && rain.cloud == 1.0, "{rain:?}");
        let (strong, thunder) = at(14.0, 0.6, 1.0);
        assert_eq!(strong, Falling::Thunderstorm);
        assert!(thunder.lightning > 0.7 && thunder.wind == 1.0 && thunder.darkness > 0.6, "{thunder:?}");
        assert!(at(27.0, 0.8, 1.0).1.lightning > thunder.lightning, "more of it where it is hot and wet");
        assert_eq!(at(1.0, 0.5, 0.6).0, Falling::Sleet);
        let (cold, snow) = at(-4.0, 0.5, 0.5);
        assert_eq!(cold, Falling::Snow);
        assert!(snow.rain == 0.0 && snow.snow == 0.5 && snow.lightning == 0.0);
        let (bitter, blizzard) = at(-12.0, 0.5, 1.0);
        assert_eq!(bitter, Falling::Blizzard);
        assert!(blizzard.fog > 0.8 && blizzard.lightning == 0.0 && blizzard.darkness < thunder.darkness, "{blizzard:?}");
        let (desert, sand) = at(34.0, 0.1, 0.9);
        assert_eq!(desert, Falling::Sandstorm);
        assert!(sand.dust == 0.9 && sand.rain == 0.0 && sand.wind == 0.9);
        assert_eq!(at(34.0, 0.7, 0.9).0, Falling::Thunderstorm, "hot and wet, it rains");
        // One told what to be is that wherever it is.
        assert_eq!(Falling::of(amounts(StormKind::Blizzard, 1.0, 30.0, 0.1)), Falling::Blizzard);
        assert_eq!(Falling::of(amounts(StormKind::Sandstorm, 1.0, -5.0, 0.9)), Falling::Sandstorm);
        assert_eq!(Falling::of(amounts(StormKind::Rain, 1.0, -5.0, 0.9)), Falling::Rain);
        let fog = amounts(StormKind::FogBank, 1.0, 8.0, 0.9);
        assert_eq!((Falling::of(fog), fog.wind, fog.rain), (Falling::Fog, 0.0, 0.0));
        // No storm, no weather; and every amount keeps within 0 and 1.
        assert!(amounts(StormKind::ByTemperature, 0.0, 14.0, 0.6).is_none());
        for kind in StormKind::ALL {
            for step in 0..=10 {
                let adds = amounts(kind, step as f32 / 10.0, 10.0, 0.5).to_array();
                assert!(adds.iter().all(|amount| (0.0..=1.0).contains(amount)), "{kind:?}: {adds:?}");
            }
            assert_eq!(StormKind::ALL[kind.index()], kind);
        }
        assert_eq!(Falling::of(WeatherKind::Clear.amounts()), Falling::Nothing);
    }

    #[test]
    fn a_storm_comes_on_blows_and_dies_away_and_spreads_as_it_grows() {
        let storm = Storm { born: 100.0, grows: 60.0, fades: 120.0, lasts: 600.0, strength: 0.8, ..Storm::default() };
        assert_eq!((storm.strength_at(99.0), storm.strength_at(100.0)), (0.0, 0.0));
        assert!((storm.strength_at(130.0) - 0.4).abs() < 1e-4, "half on");
        assert_eq!(storm.strength_at(300.0), 0.8);
        assert!((storm.strength_at(640.0) - 0.4).abs() < 1e-4, "half gone");
        assert_eq!(storm.strength_at(700.0), 0.0);
        assert!(!storm.is_over(699.0) && storm.is_over(700.0));
        assert!(storm.radius_at(100.0) < storm.radius_at(160.0) && storm.radius_at(160.0) == storm.radius);
        // One with no end blows on, until it is stopped.
        let mut storms = Storms::default();
        storms.clock = 50.0;
        let id = storms.start(Storm { grows: 0.0, ..Storm::default() });
        assert_eq!((id, storms.made[0].born), (1, 50.0));
        storms.clock = 5000.0;
        assert_eq!(storms.get(id).unwrap().strength_at(storms.clock), 1.0);
        assert!(storms.stop(id, 20.0) && !storms.stop(77, 1.0));
        assert!((storms.get(id).unwrap().strength_at(5010.0) - 0.5).abs() < 1e-3);
        assert!(storms.get(id).unwrap().is_over(5020.0));
    }

    #[test]
    fn a_made_storm_drifts_and_follows_its_path_to_stand_at_its_end() {
        let mut storm = Storm { velocity: [3.0, 4.0], ..Storm::default() };
        storm.drift(10.0);
        assert_eq!(storm.middle, [30.0, 40.0]);
        storm.path = vec![[30.0, 140.0], [130.0, 140.0]];
        storm.drift(10.0);
        assert!((storm.middle[0] - 30.0).abs() < 1e-6 && (storm.middle[1] - 90.0).abs() < 1e-6, "north at 5 m/s");
        for _ in 0..40 {
            storm.drift(1.0);
        }
        assert_eq!((storm.middle, storm.path.len()), ([130.0, 140.0], 0), "at its path's end");
    }

    fn world_with_viewer(at: Vec3) -> World {
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(at)));
        world.insert_resource(Storms::default());
        world
    }

    #[test]
    fn a_storm_started_blows_on_the_viewer_as_it_reaches_them_and_scripts_hear_it_come_and_go() {
        let mut world = world_with_viewer(Vec3::new(0.0, 2.0, 0.0));
        // A kilometre west, 300 m across, coming east at 20 m/s.
        let id = start(&mut world, Vec3::new(-1000.0, 0.0, 0.0), 300.0, 1.0, StormKind::Thunderstorm);
        {
            let storms = world.resource_mut::<Storms>().unwrap();
            let storm = storms.made_mut(id).unwrap();
            (storm.velocity, storm.grows) = ([20.0, 0.0], 0.0);
        }
        update(&mut world, 0.0, true);
        assert!(world.resource::<Weather>().is_none_or(|weather| weather.now().is_none()), "not here yet");
        let places = world.resource::<WeatherPlaces>().unwrap().0.clone();
        assert_eq!(places.len(), 1);
        assert_eq!((places[0].middle, places[0].radius, places[0].rough), (Vec2::new(-1000.0, 0.0), 300.0, 1.0));
        assert!(places[0].amounts.rain == 1.0 && places[0].amounts.lightning > 0.9, "seen from afar as what it is");
        assert_eq!(weather_at(&world, Vec3::new(-1000.0, 0.0, 0.0)).falling, Falling::Thunderstorm);
        assert_eq!(weather_at(&world, Vec3::ZERO).falling, Falling::Nothing);
        assert!(world.resource_mut::<Storms>().unwrap().take_events().is_empty());

        // Fifty seconds on its middle is over the viewer.
        for _ in 0..50 {
            update(&mut world, 1.0, true);
        }
        let here = world.resource::<Weather>().unwrap().now();
        assert!(here.rain == 1.0 && here.wind == 1.0 && here.lightning > 0.9, "{here:?}");
        assert_eq!(world.resource::<Weather>().unwrap().everywhere(), WeatherAmounts::default(), "the level's own is clear");
        let storms = world.resource_mut::<Storms>().unwrap();
        assert_eq!(storms.over_viewer(), &[id][..]);
        assert_eq!(storms.take_events(), vec![StormEvent::Arrived(id)]);
        let at = weather_at(&world, Vec3::new(0.0, 2.0, 0.0));
        assert_eq!((at.falling, at.storm, at.strength), (Falling::Thunderstorm, id, 1.0));
        // And the sea under it is rough, where it was flat.
        assert!(world.resource::<SeaNow>().is_some_and(|sea| sea.0.rough_at(0.0, 0.0) == 1.0));

        // It blows over: half a minute on it is gone from the viewer, and its rain with it.
        for _ in 0..30 {
            update(&mut world, 1.0, true);
        }
        assert!(world.resource::<Weather>().unwrap().now().is_none());
        assert_eq!(world.resource_mut::<Storms>().unwrap().take_events(), vec![StormEvent::Left(id)]);
        // While the game is not playing, storms stand still.
        let before = world.resource::<Storms>().unwrap().made[0].middle;
        update(&mut world, 5.0, false);
        assert_eq!(world.resource::<Storms>().unwrap().made[0].middle, before);
        // Stopped, it dies away and is forgotten.
        world.resource_mut::<Storms>().unwrap().stop(id, 2.0);
        update(&mut world, 3.0, true);
        assert!(world.resource::<Storms>().unwrap().made.is_empty());
        assert!(world.resource::<WeatherPlaces>().unwrap().0.is_empty());
    }

    #[test]
    fn storms_are_kept_in_a_save_as_the_clock_and_what_was_made() {
        let mut storms = Storms::default();
        storms.clock = 1234.5;
        let id = storms.start(Storm { middle: [10.0, -20.0], velocity: [1.0, 2.0], eye: 0.3, ..Storm::default() });
        let pool = storms.add_whirlpool([5.0, 6.0], 20.0, 6.0);
        let saved = storms.saved();
        let text = serde_json::to_string(&saved).unwrap();
        assert!(text.len() < 600, "small: {} bytes", text.len());
        let mut restored = Storms::default();
        restored.restore(serde_json::from_str(&text).unwrap());
        assert_eq!(restored.saved(), saved);
        assert_eq!((restored.get(id).unwrap().eye, restored.whirlpools[0].id), (0.3, pool));
        // The next made takes a number neither has.
        assert!(restored.start(Storm::default()) > pool);
        assert!(restored.remove_whirlpool(pool) && !restored.remove_whirlpool(pool));
    }

    #[test]
    fn a_made_whirlpool_is_in_the_sea_near_the_viewer_and_a_far_one_is_not() {
        let mut world = world_with_viewer(Vec3::new(0.0, 2.0, 0.0));
        add_whirlpool(&mut world, Vec3::new(40.0, 0.0, 10.0), 20.0, 6.0);
        add_whirlpool(&mut world, Vec3::new(40_000.0, 0.0, 10.0), 20.0, 6.0);
        update(&mut world, 0.1, true);
        let sea = world.resource::<RoughSea>().unwrap();
        assert_eq!(sea.whirlpools, vec![DrawnWhirlpool { middle: Vec2::new(40.0, 10.0), radius: 20.0, depth: 6.0 }]);
        let now = &world.resource::<SeaNow>().expect("a sea that is not calm").0;
        assert!((now.at(40.0, 10.0).sunk - 6.0).abs() < 0.01);
    }
}
