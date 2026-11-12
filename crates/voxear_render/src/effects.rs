//! Particle effects: several emitters played as one, each the settings of a
//! Particle System, so an explosion is its flash, its fireball, its smoke,
//! its sparks and what it throws, started together. A Particle System's
//! **Effect** picks one in place of its own settings: a starter, built in
//! here from the textures made here, or a project's `.particles` asset,
//! which the engine reads into [`ParticleEffects`].
//!
//! The textures particles are drawn with: one built in, or a project's
//! picture, which the engine loads and lists in [`ParticlePictures`]. The
//! built-in ones are made once, on the CPU, when the renderer starts
//! ([`BuiltinTexture::picture`]): white shapes whose colour comes from the
//! system that draws them, most of them four different ones to a picture so
//! no two particles need look alike.

use std::{collections::HashMap, fmt, str::FromStr, sync::Arc, sync::OnceLock};

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::curve::{Curve, Gradient};
use voxear_rhi::TextureId;

use crate::particle_meshes::{BuiltinMesh, ParticleMesh};
use crate::particles::{EmitShape, ParticleBlend, ParticleCollision, ParticleSpace, ParticleSystem, ParticleTrails};

/// What a built-in texture's or starter's reference starts with, as a file
/// writes it: `builtin:puff`, `builtin:explosion`.
const BUILTIN: &str = "builtin:";

/// A texture every project has, made by the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BuiltinTexture {
    /// A soft round dot, drawn without a picture: sparks, drops, flakes.
    #[default]
    Dot,
    /// A ragged puff: smoke, dust, steam.
    Puff,
    /// A tongue of flame.
    Flame,
    /// A thin ring: a shockwave, a ripple.
    Ring,
    /// A burst of rays: a muzzle flash, a glint.
    Flare,
    /// A hard-edged chip: debris, mud, clots.
    Shard,
}

impl BuiltinTexture {
    pub const ALL: [Self; 6] = [Self::Dot, Self::Puff, Self::Flame, Self::Ring, Self::Flare, Self::Shard];
    /// Those that are pictures, in the order the renderer keeps them.
    pub const PICTURES: [Self; 5] = [Self::Puff, Self::Flame, Self::Ring, Self::Flare, Self::Shard];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dot => "Dot",
            Self::Puff => "Puff",
            Self::Flame => "Flame",
            Self::Ring => "Ring",
            Self::Flare => "Flare",
            Self::Shard => "Shard",
        }
    }

    /// As a file names it: `builtin:puff`.
    pub fn reference(self) -> String {
        format!("{BUILTIN}{}", self.name().to_lowercase())
    }

    pub fn from_reference(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|texture| texture.reference() == text)
    }

    /// How many different shapes its picture holds, across and down: each
    /// particle is drawn with one of them.
    pub fn tiles(self) -> [u32; 2] {
        match self {
            Self::Dot | Self::Ring => [1, 1],
            Self::Puff | Self::Flame | Self::Flare | Self::Shard => [2, 2],
        }
    }

    /// Its picture: how many pixels a side, and its pixels, white or near
    /// it with the shape in their alpha. None for the Dot, which has no
    /// picture.
    pub fn picture(self) -> Option<(u32, Vec<u8>)> {
        let [across, down] = self.tiles();
        let tile = PICTURE_SIDE / across.max(down);
        let mut pixels = vec![0u8; (PICTURE_SIDE * PICTURE_SIDE * 4) as usize];
        if self == Self::Dot {
            return None;
        }
        for y in 0..PICTURE_SIDE {
            for x in 0..PICTURE_SIDE {
                let which = (x / tile + y / tile * across) as f32;
                // Where in its tile, from -1 to 1 each way, up being up.
                let at = [
                    ((x % tile) as f32 + 0.5) / tile as f32 * 2.0 - 1.0,
                    1.0 - ((y % tile) as f32 + 0.5) / tile as f32 * 2.0,
                ];
                let [shade, alpha] = match self {
                    Self::Puff => puff(at, which),
                    Self::Flame => flame(at, which),
                    Self::Ring => ring(at),
                    Self::Flare => flare(at, which),
                    Self::Shard => shard(at, which),
                    Self::Dot => [0.0, 0.0],
                };
                // Nothing at a tile's edge, so one tile never shows in the next.
                let edge = (1.0 - at[0].abs().max(at[1].abs())) * tile as f32 * 0.5;
                let alpha = alpha.clamp(0.0, 1.0) * ((edge - 1.0) / 2.0).clamp(0.0, 1.0);
                let grey = (shade.clamp(0.0, 1.0) * 255.0) as u8;
                let i = ((y * PICTURE_SIDE + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[grey, grey, grey, (alpha * 255.0) as u8]);
            }
        }
        Some((PICTURE_SIDE, pixels))
    }
}

impl BuiltinTexture {
    /// The Puff's two maps for being lit from six sides, as its picture's
    /// pixels: in the first, how bright each pixel is lit from the right,
    /// from above and from behind, with the puff's shape in alpha; in the
    /// second, from the left, from below and from in front. The puff is
    /// taken for a ball of smoke as thick as it is opaque: a side lights
    /// the half that faces it and the folds that catch it, less the further
    /// the light has come through the ball; light from behind comes through
    /// where it is thin; and its thin edges are brighter than its folds.
    /// None for the others, which are lit flat.
    pub fn light_maps(self) -> Option<(u32, [Vec<u8>; 2])> {
        if self != Self::Puff {
            return None;
        }
        let [across, down] = self.tiles();
        let tile = PICTURE_SIDE / across.max(down);
        let mut maps = [vec![0u8; (PICTURE_SIDE * PICTURE_SIDE * 4) as usize], vec![0u8; (PICTURE_SIDE * PICTURE_SIDE * 4) as usize]];
        // How thick the puff is at a place in a tile, from 0 to 1, and how opaque.
        let mound = |x: u32, y: u32, which: f32| {
            let at = [(x as f32 + 0.5) / tile as f32 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / tile as f32 * 2.0];
            let [_, alpha] = puff(at, which);
            let edge = (1.0 - at[0].abs().max(at[1].abs())) * tile as f32 * 0.5;
            let alpha = alpha.clamp(0.0, 1.0) * ((edge - 1.0) / 2.0).clamp(0.0, 1.0);
            (alpha.sqrt() * (1.0 - (at[0] * at[0] + at[1] * at[1]).min(1.0)).sqrt().max(0.2), alpha)
        };
        for y in 0..PICTURE_SIDE {
            for x in 0..PICTURE_SIDE {
                let which = (x / tile + y / tile * across) as f32;
                let (tx, ty) = (x % tile, y % tile);
                let (thick, alpha) = mound(tx, ty, which);
                // Which way its surface faces there, from how its thickness changes across and down the picture.
                let near = |dx: i32, dy: i32| {
                    let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
                    let inside = (0..tile as i32).contains(&nx) && (0..tile as i32).contains(&ny);
                    if inside { mound(nx as u32, ny as u32, which).0 } else { 0.0 }
                };
                let slope = tile as f32 * 0.09;
                let (rise_right, rise_up) = ((near(2, 0) - near(-2, 0)) * slope, (near(0, -2) - near(0, 2)) * slope);
                let long = (rise_right * rise_right + rise_up * rise_up + 1.0).sqrt();
                let (facing_right, facing_up, facing_front) = (-rise_right / long, -rise_up / long, 1.0 / long);
                // It is lit as a ball of smoke is: on the side toward the light, its folds catching it or not, and
                // darker the further the light has come through it; and brighter all over where it is thin.
                let at = [(tx as f32 + 0.5) / tile as f32 * 2.0 - 1.0, 1.0 - (ty as f32 + 0.5) / tile as f32 * 2.0];
                let (bx, by) = (at[0] / 0.85, at[1] / 0.85);
                let round = (1.0 - (bx * bx + by * by).min(1.0)).sqrt();
                let thin = (-3.0 * thick).exp();
                let side = |toward: f32, across: f32, facing: f32| {
                    // How far into the ball the light has come by here, from the side it comes from.
                    let come = ((1.0 - across * across).max(0.0).sqrt() - toward).clamp(0.0, 2.0);
                    let lit = (0.5 + 0.5 * (toward * 0.8 + facing * 0.6).clamp(-1.0, 1.0)).powf(1.3);
                    (lit * (0.15 + 0.85 * (-1.1 * come).exp()) + thin * 0.25).clamp(0.0, 1.0)
                };
                let (right, left) = (side(bx, by, facing_right), side(-bx, by, -facing_right));
                let (top, bottom) = (side(by, bx, facing_up), side(-by, bx, -facing_up));
                let front = ((0.3 + 0.7 * round) * (0.6 + 0.4 * facing_front)).clamp(0.0, 1.0);
                let back = ((-2.2 * thick).exp() * 0.9 + 0.05).clamp(0.0, 1.0);
                let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u8;
                let i = ((y * PICTURE_SIDE + x) * 4) as usize;
                maps[0][i..i + 4].copy_from_slice(&[byte(right), byte(top), byte(back), byte(alpha)]);
                maps[1][i..i + 4].copy_from_slice(&[byte(left), byte(bottom), byte(front), 255]);
            }
        }
        Some((PICTURE_SIDE, maps))
    }
}

/// How many pixels a side a built-in texture's picture has.
const PICTURE_SIDE: u32 = 256;

/// A number from 0 to 1 for a whole-numbered place.
fn lattice(x: i32, y: i32, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ salt.wrapping_mul(83492791);
    h = h.wrapping_mul(747796405).wrapping_add(2891336453);
    h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277803737);
    ((h >> 22) ^ h) as f32 / u32::MAX as f32
}

/// Smooth noise from 0 to 1, about one unit across a bump.
fn noise(x: f32, y: f32, salt: u32) -> f32 {
    let (cx, cy) = (x.floor(), y.floor());
    let (fx, fy) = (x - cx, y - cy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (cx, cy) = (cx as i32, cy as i32);
    let top = lattice(cx, cy, salt) + (lattice(cx + 1, cy, salt) - lattice(cx, cy, salt)) * ux;
    let bottom = lattice(cx, cy + 1, salt) + (lattice(cx + 1, cy + 1, salt) - lattice(cx, cy + 1, salt)) * ux;
    top + (bottom - top) * uy
}

/// Noise in four sizes, each half the one before and half as strong: from 0 to 1.
fn billows(x: f32, y: f32, salt: u32) -> f32 {
    let mut total = 0.0;
    let (mut size, mut strength) = (1.0, 0.5);
    for octave in 0..4 {
        total += noise(x * size, y * size, salt + octave) * strength;
        size *= 2.0;
        strength *= 0.5;
    }
    total / 0.9375
}

/// 0 below `from`, 1 above `to`, eased between; the other way round when `from` is the greater.
fn eased(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A ragged puff: a ball whose edge noise eats into, thinner in patches.
fn puff([x, y]: [f32; 2], which: f32) -> [f32; 2] {
    let salt = 11 + which as u32 * 17;
    let out = (x * x + y * y).sqrt();
    let ragged = out + (billows(x * 1.6 + 5.0, y * 1.6 + 9.0, salt) - 0.5) * 0.7;
    let body = eased(0.92, 0.25, ragged);
    let patches = billows(x * 3.0 + 31.0, y * 3.0 + 7.0, salt + 5);
    [0.82 + 0.18 * patches, body * (0.45 + 0.75 * patches) * eased(1.0, 0.8, out)]
}

/// A tongue of flame: round at its foot, widest a little above it, narrowing to a wavering tip.
fn flame([x, y]: [f32; 2], which: f32) -> [f32; 2] {
    let salt = 101 + which as u32 * 23;
    let up = (y + 1.0) * 0.5;
    let sway = (billows(y * 1.4 + 3.0, which * 7.0, salt) - 0.5) * 0.7 * up;
    let width = 0.6 * (up * 5.0).min(1.0).sqrt() * (1.0 - up).powf(0.75) + 0.02;
    let across = eased(width, width * 0.2, (x - sway).abs());
    let licked = billows(x * 2.5 + 13.0, y * 2.0 - 4.0, salt + 3);
    let tip = eased(0.95, 0.35, up + (licked - 0.5) * 0.6);
    [1.0, across * tip * eased(0.0, 0.12, up) * (0.6 + 0.6 * licked)]
}

/// A thin ring, a little uneven round its length.
fn ring([x, y]: [f32; 2]) -> [f32; 2] {
    let out = (x * x + y * y).sqrt();
    let turn = y.atan2(x);
    let uneven = 0.75 + 0.5 * noise(turn.cos() * 2.5 + 8.0, turn.sin() * 2.5 + 8.0, 301);
    let band = (-((out - 0.8) / 0.07).powi(2)).exp();
    [1.0, band * uneven]
}

/// A burst of rays out of a bright middle.
fn flare([x, y]: [f32; 2], which: f32) -> [f32; 2] {
    let salt = 401 + which as u32 * 29;
    let out = (x * x + y * y).sqrt();
    let turn = y.atan2(x);
    // Round the middle, so the rays meet themselves where the turn comes round.
    let rays = noise(turn.cos() * 3.5 + 20.0, turn.sin() * 3.5 + 20.0, salt).powi(3) * 2.2;
    let fine = noise(turn.cos() * 9.0 + 40.0, turn.sin() * 9.0 + 40.0, salt + 1).powi(2);
    let middle = (-out * out * 14.0).exp();
    let reach = (rays + fine * 0.5) * (-out * 3.2).exp() * 2.0;
    [1.0, (middle + reach) * eased(1.0, 0.6, out)]
}

/// A hard-edged chip: what five or six straight cuts leave, each at its own distance from the middle, shaded
/// across.
fn shard([x, y]: [f32; 2], which: f32) -> [f32; 2] {
    let salt = 601 + which as u32 * 31;
    let cuts = 5 + which as i32 % 2;
    let mut inside = f32::INFINITY;
    for cut in 0..cuts {
        let turn = (cut as f32 + lattice(cut, 1, salt) * 0.7) / cuts as f32 * std::f32::consts::TAU;
        let far = 0.3 + 0.4 * lattice(cut, 2, salt);
        inside = inside.min(far - (x * turn.cos() + y * turn.sin()));
    }
    let shade = 0.55 + 0.45 * noise(x * 2.0 + 3.0, y * 2.0 + 3.0, salt + 2);
    [shade, eased(0.0, 0.05, inside)]
}

/// The texture a Particle System's particles are drawn with: one built in,
/// or one of the project's pictures. A file writes it as `builtin:puff` or
/// as the picture's GUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ParticleTexture {
    Builtin(BuiltinTexture),
    Asset(AssetGuid),
}

impl Default for ParticleTexture {
    fn default() -> Self {
        Self::Builtin(BuiltinTexture::Dot)
    }
}

impl ParticleTexture {
    pub(crate) fn is_dot(&self) -> bool {
        *self == Self::default()
    }

    /// Whether it is the built-in texture `texture`.
    pub fn is_builtin(&self, texture: BuiltinTexture) -> bool {
        *self == Self::Builtin(texture)
    }
}

impl fmt::Display for ParticleTexture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(texture) => f.write_str(&texture.reference()),
            Self::Asset(guid) => write!(f, "{guid}"),
        }
    }
}

impl FromStr for ParticleTexture {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if text.starts_with(BUILTIN) {
            let names: Vec<String> = BuiltinTexture::ALL.iter().map(|texture| texture.reference()).collect();
            let unknown = || format!("{text} is not a built-in particle texture; they are {}", names.join(", "));
            return BuiltinTexture::from_reference(text).map(Self::Builtin).ok_or_else(unknown);
        }
        text.parse().map(Self::Asset).map_err(|_| format!("{text} is not a picture's GUID"))
    }
}

impl TryFrom<String> for ParticleTexture {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

impl From<ParticleTexture> for String {
    fn from(texture: ParticleTexture) -> Self {
        texture.to_string()
    }
}

/// The project's pictures that Particle Systems draw with, as the renderer
/// has them: a resource of the world, kept by whoever loads pictures. One
/// not listed yet draws as the Dot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticlePictures(pub HashMap<AssetGuid, TextureId>);

/// An effect every project has: several emitters made from the built-in
/// textures, to use as it is or to start one's own from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Starter {
    Fire,
    Smoke,
    Sparks,
    Dust,
    Rain,
    Snow,
    Blood,
    MuzzleFlash,
    Explosion,
    Magic,
    Mud,
}

impl Starter {
    pub const ALL: [Self; 11] = [
        Self::Fire,
        Self::Smoke,
        Self::Sparks,
        Self::Dust,
        Self::Rain,
        Self::Snow,
        Self::Blood,
        Self::MuzzleFlash,
        Self::Explosion,
        Self::Magic,
        Self::Mud,
    ];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fire => "Fire",
            Self::Smoke => "Smoke",
            Self::Sparks => "Sparks",
            Self::Dust => "Dust",
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Blood => "Blood",
            Self::MuzzleFlash => "Muzzle Flash",
            Self::Explosion => "Explosion",
            Self::Magic => "Magic",
            Self::Mud => "Mud",
        }
    }

    /// As a file names it: `builtin:muzzle_flash`.
    pub fn reference(self) -> String {
        format!("{BUILTIN}{}", self.name().to_lowercase().replace(' ', "_"))
    }

    pub fn from_reference(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|starter| starter.reference() == text)
    }

    /// Its emitters, made once.
    pub fn effect(self) -> &'static ParticleEffect {
        static MADE: OnceLock<Vec<ParticleEffect>> = OnceLock::new();
        let made = MADE.get_or_init(|| Self::ALL.iter().map(|starter| starter.make()).collect());
        &made[Self::ALL.iter().position(|starter| *starter == self).unwrap_or(0)]
    }
}

/// The effect a Particle System plays in place of its own settings: a
/// starter, or one of the project's `.particles` assets. A file writes it
/// as `builtin:explosion` or as the asset's GUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum EffectRef {
    Builtin(Starter),
    Asset(AssetGuid),
}

impl fmt::Display for EffectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(starter) => f.write_str(&starter.reference()),
            Self::Asset(guid) => write!(f, "{guid}"),
        }
    }
}

impl FromStr for EffectRef {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if text.starts_with(BUILTIN) {
            let names: Vec<String> = Starter::ALL.iter().map(|starter| starter.reference()).collect();
            let unknown = || format!("{text} is not a built-in particle effect; they are {}", names.join(", "));
            return Starter::from_reference(text).map(Self::Builtin).ok_or_else(unknown);
        }
        text.parse().map(Self::Asset).map_err(|_| format!("{text} is not a particle effect's GUID"))
    }
}

impl TryFrom<String> for EffectRef {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

impl From<EffectRef> for String {
    fn from(effect: EffectRef) -> Self {
        effect.to_string()
    }
}

/// One of an effect's emitters: what it is called, and its settings, those
/// of a Particle System. Its own Effect, if it names one, is not played.
#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    pub name: String,
    pub system: ParticleSystem,
}

/// An effect: its emitters, played together from when their Particle System
/// starts, each after its own Delay. A `.particles` file is one of these,
/// an `[[emitter]]` table each: its name, then a Particle System's settings
/// as a scene writes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleEffect {
    pub emitters: Vec<Emitter>,
}

/// The most emitters an effect plays.
pub const MAX_EMITTERS: usize = 16;

impl ParticleEffect {
    /// Reads a `.particles` file's text.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let mut file: toml::Table = text.parse().map_err(|error: toml::de::Error| error.to_string())?;
        let tables = match file.remove("emitter") {
            Some(toml::Value::Array(tables)) => tables,
            Some(_) => return Err("emitter is not a list of [[emitter]] tables".into()),
            None => Vec::new(),
        };
        if let Some(key) = file.keys().find(|key| key.as_str() != "format") {
            return Err(format!("unknown key {key}"));
        }
        let mut emitters = Vec::with_capacity(tables.len());
        for (index, table) in tables.into_iter().enumerate() {
            let toml::Value::Table(mut table) = table else { return Err("an emitter is not a table".into()) };
            let name = match table.remove("name") {
                Some(toml::Value::String(name)) => name,
                Some(_) => return Err(format!("emitter {}: its name is not text", index + 1)),
                None => format!("Emitter {}", index + 1),
            };
            let system: ParticleSystem = table.try_into().map_err(|error| format!("emitter {name}: {error}"))?;
            emitters.push(Emitter { name, system: ParticleSystem { effect: None, ..system } });
        }
        Ok(Self { emitters })
    }

    /// Its text as a `.particles` file: each emitter's name, then what of
    /// its settings is not as a new Particle System has it.
    pub fn to_toml(&self) -> Result<String, String> {
        let mut text = String::from("# Voxear particle effect\nformat = 1\n");
        let fresh = toml::Table::try_from(ParticleSystem::default()).map_err(|error| error.to_string())?;
        for emitter in &self.emitters {
            let all = toml::Table::try_from(emitter.system).map_err(|error| error.to_string())?;
            let mut table = toml::Table::new();
            table.insert("name".into(), toml::Value::String(emitter.name.clone()));
            table.extend(all.into_iter().filter(|(key, value)| fresh.get(key) != Some(value)));
            let written = toml::to_string(&table).map_err(|error| error.to_string())?;
            text.push_str(&format!("\n[[emitter]]\n{written}"));
        }
        Ok(text)
    }
}

/// The project's effects, as read from its `.particles` assets: a resource
/// of the world, kept by whoever reads them. A Particle System whose effect
/// is not here yet sends out nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleEffects(pub HashMap<AssetGuid, Arc<ParticleEffect>>);

/// The systems `system` runs: its effect's emitters, when it names one, or
/// itself; none while its effect is not read yet.
pub(crate) fn emitters<'a>(
    library: Option<&'a ParticleEffects>,
    system: &'a ParticleSystem,
) -> impl Iterator<Item = &'a ParticleSystem> {
    let (own, effect): (Option<&ParticleSystem>, &[Emitter]) = match system.effect {
        None => (Some(system), &[]),
        Some(EffectRef::Builtin(starter)) => (None, &starter.effect().emitters),
        Some(EffectRef::Asset(guid)) => {
            let found = library.and_then(|library| library.0.get(&guid));
            (None, found.map_or(&[], |effect| &effect.emitters))
        }
    };
    own.into_iter().chain(effect.iter().take(MAX_EMITTERS).map(|emitter| &emitter.system))
}

/// How long after it starts `system` has nothing left out, in seconds: once
/// the last of its emitters' particles has died. None while any of them
/// loops, or its effect is not read yet.
pub fn ends_after(library: Option<&ParticleEffects>, system: &ParticleSystem) -> Option<f32> {
    let mut end: Option<f32> = None;
    for emitter in emitters(library, system) {
        if emitter.looping {
            return None;
        }
        let longest = emitter.lifetime[0].max(emitter.lifetime[1]).max(0.0);
        let ends = emitter.delay.max(0.0) + emitter.duration.max(0.0) + longest;
        end = Some(end.map_or(ends, |end| end.max(ends)));
    }
    end
}

/// How long the longest lived of `system`'s particles lives, in seconds, over
/// all its emitters: how long after it is stopped it has nothing left out.
/// None while its effect is not read yet.
pub fn longest_life(library: Option<&ParticleEffects>, system: &ParticleSystem) -> Option<f32> {
    let lives = emitters(library, system).map(|emitter| emitter.lifetime[0].max(emitter.lifetime[1]).max(0.0));
    lives.reduce(f32::max)
}

/// Keys for a curve that fades in over the first `rise` of a life, then out to nothing by its end.
fn swell(rise: f32) -> Curve {
    Curve::new(&[[0.0, 0.0], [rise, 1.0], [1.0, 0.0]])
}

impl Starter {
    fn make(self) -> ParticleEffect {
        let named = |name: &str, system: ParticleSystem| Emitter { name: name.into(), system };
        // What most glowing emitters share, and most lit ones.
        let glow = ParticleSystem { blend: ParticleBlend::Additive, lit: false, ..ParticleSystem::default() };
        let haze = ParticleSystem { space: ParticleSpace::World, ..ParticleSystem::default() };
        let hot = Gradient::new(&[
            [0.0, 1.0, 0.85, 0.5],
            [0.3, 1.0, 0.45, 0.08],
            [0.7, 0.7, 0.1, 0.02],
            [1.0, 0.2, 0.02, 0.0],
        ]);
        let puff = ParticleTexture::Builtin(BuiltinTexture::Puff);
        let flame = ParticleTexture::Builtin(BuiltinTexture::Flame);
        let shard = ParticleTexture::Builtin(BuiltinTexture::Shard);
        let flare = ParticleTexture::Builtin(BuiltinTexture::Flare);
        let ring = ParticleTexture::Builtin(BuiltinTexture::Ring);
        let smoke = ParticleSystem {
            texture: puff,
            rate: 12.0,
            lifetime: [3.5, 5.0],
            speed: [0.8, 1.3],
            size: [0.5, 0.8],
            size_end: 4.0,
            color: [0.2, 0.2, 0.21, 0.8],
            opacity_curve: swell(0.12),
            radius: 0.15,
            angle: 10.0,
            gravity: -0.07,
            drag: 0.3,
            spin: 25.0,
            softness: 0.6,
            wind: 1.0,
            turbulence: 0.45,
            turbulence_size: 2.0,
            forces: 1.0,
            ..haze
        };
        let sparks = ParticleSystem {
            rate: 0.0,
            burst: 40,
            looping: false,
            duration: 1.0,
            lifetime: [0.5, 1.3],
            speed: [3.0, 8.0],
            size: [0.03, 0.05],
            // Each draws the way it has come behind it, round a bounce as much as along a fall.
            trails: ParticleTrails::Particles,
            trail_time: 0.07,
            trail_width: 0.9,
            color: [1.0, 0.75, 0.35, 1.0],
            color_gradient: Gradient::new(&[[0.0, 1.0, 0.9, 0.6], [0.4, 1.0, 0.5, 0.1], [1.0, 0.6, 0.08, 0.02]]),
            opacity_curve: Curve::new(&[[0.0, 1.0], [0.7, 1.0], [1.0, 0.0]]),
            intensity: 6.0,
            angle: 35.0,
            gravity: 1.0,
            drag: 0.6,
            softness: 0.02,
            space: ParticleSpace::World,
            collision: ParticleCollision::Depth,
            bounce: 0.45,
            friction: 0.3,
            ..glow
        };
        let emitters = match self {
            Self::Fire => vec![
                named(
                    "Flames",
                    ParticleSystem {
                        texture: flame,
                        rate: 30.0,
                        lifetime: [0.6, 1.0],
                        speed: [0.9, 1.7],
                        size: [0.7, 1.05],
                        size_end: 0.5,
                        color_gradient: Gradient::new(&[
                            [0.0, 1.0, 0.6, 0.2],
                            [0.35, 1.0, 0.33, 0.06],
                            [0.75, 0.6, 0.1, 0.02],
                            [1.0, 0.15, 0.02, 0.0],
                        ]),
                        opacity_curve: swell(0.15),
                        intensity: 1.6,
                        light: 5.0,
                        light_range: 7.0,
                        radius: 0.16,
                        angle: 7.0,
                        gravity: -0.12,
                        softness: 0.25,
                        wind: 0.12,
                        turbulence: 0.25,
                        turbulence_size: 0.5,
                        ..glow
                    },
                ),
                named("Smoke", ParticleSystem { delay: 0.3, ..smoke }),
                named(
                    "Embers",
                    ParticleSystem {
                        rate: 10.0,
                        burst: 0,
                        looping: true,
                        duration: 5.0,
                        lifetime: [1.5, 3.0],
                        speed: [1.0, 2.4],
                        angle: 18.0,
                        radius: 0.2,
                        gravity: -0.1,
                        drag: 0.4,
                        wind: 0.6,
                        turbulence: 1.2,
                        turbulence_size: 0.8,
                        collision: ParticleCollision::Off,
                        trail_time: 0.25,
                        ..sparks
                    },
                ),
            ],
            Self::Smoke => vec![named(
                "Smoke",
                ParticleSystem { rate: 22.0, size: [0.7, 1.0], color: [0.32, 0.32, 0.33, 0.9], radius: 0.3, ..smoke },
            )],
            Self::Sparks => vec![named(
                "Sparks",
                ParticleSystem { rate: 90.0, burst: 0, looping: true, duration: 5.0, angle: 25.0, ..sparks },
            )],
            Self::Dust => vec![named(
                "Dust",
                ParticleSystem {
                    texture: puff,
                    rate: 10.0,
                    lifetime: [5.0, 8.0],
                    speed: [0.0, 0.25],
                    size: [0.9, 1.6],
                    size_end: 2.2,
                    color: [0.5, 0.42, 0.3, 0.32],
                    opacity_curve: swell(0.25),
                    shape: EmitShape::Box,
                    box_size: [6.0, 0.4, 6.0],
                    spin: 12.0,
                    softness: 0.8,
                    wind: 0.6,
                    turbulence: 0.3,
                    turbulence_size: 3.0,
                    forces: 1.0,
                    ..haze
                },
            )],
            Self::Rain => vec![named(
                "Rain",
                ParticleSystem {
                    rate: 1800.0,
                    lifetime: [1.4, 1.4],
                    // Out of a box over its Obj, downwards.
                    speed: [-11.0, -9.0],
                    size: [0.012, 0.018],
                    stretch: 0.045,
                    color: [0.75, 0.82, 0.9, 0.55],
                    color_end: [0.75, 0.82, 0.9, 0.55],
                    shape: EmitShape::Box,
                    box_size: [14.0, 0.5, 14.0],
                    gravity: 0.5,
                    softness: 0.0,
                    wind: 0.5,
                    collision: ParticleCollision::Depth,
                    life_lost: 1.0,
                    ..haze
                },
            )],
            Self::Snow => vec![named(
                "Snow",
                ParticleSystem {
                    rate: 320.0,
                    lifetime: [9.0, 11.0],
                    speed: [-1.4, -0.9],
                    size: [0.02, 0.04],
                    color: [1.0, 1.0, 1.0, 0.9],
                    color_end: [1.0, 1.0, 1.0, 0.9],
                    shape: EmitShape::Box,
                    box_size: [14.0, 0.5, 14.0],
                    softness: 0.0,
                    wind: 0.5,
                    turbulence: 0.5,
                    turbulence_size: 2.0,
                    collision: ParticleCollision::Depth,
                    bounce: 0.0,
                    friction: 1.0,
                    life_lost: 0.8,
                    ..haze
                },
            )],
            Self::Blood => vec![
                named(
                    "Spurt",
                    ParticleSystem {
                        texture: shard,
                        rate: 0.0,
                        burst: 26,
                        looping: false,
                        duration: 1.0,
                        lifetime: [1.5, 2.8],
                        speed: [1.5, 4.5],
                        size: [0.03, 0.08],
                        color: [0.25, 0.005, 0.005, 1.0],
                        color_end: [0.12, 0.0, 0.0, 1.0],
                        opacity_curve: Curve::new(&[[0.0, 1.0], [0.8, 1.0], [1.0, 0.0]]),
                        angle: 30.0,
                        gravity: 1.0,
                        drag: 0.4,
                        spin: 200.0,
                        softness: 0.0,
                        collision: ParticleCollision::Depth,
                        bounce: 0.0,
                        friction: 1.0,
                        ..haze
                    },
                ),
                named(
                    "Mist",
                    ParticleSystem {
                        texture: puff,
                        rate: 0.0,
                        burst: 6,
                        looping: false,
                        duration: 1.0,
                        lifetime: [0.4, 0.7],
                        speed: [0.5, 1.5],
                        size: [0.15, 0.25],
                        size_end: 2.5,
                        color: [0.3, 0.01, 0.01, 0.5],
                        angle: 30.0,
                        drag: 3.0,
                        softness: 0.2,
                        ..haze
                    },
                ),
            ],
            Self::MuzzleFlash => vec![
                named(
                    "Flash",
                    ParticleSystem {
                        texture: flare,
                        rate: 0.0,
                        burst: 3,
                        looping: false,
                        duration: 0.2,
                        lifetime: [0.04, 0.07],
                        speed: [0.0, 0.4],
                        size: [0.25, 0.42],
                        size_end: 1.4,
                        color: [1.0, 0.85, 0.55, 1.0],
                        color_end: [1.0, 0.5, 0.15, 0.0],
                        intensity: 10.0,
                        light: 14.0,
                        light_range: 6.0,
                        angle: 4.0,
                        radius: 0.01,
                        softness: 0.05,
                        ..glow
                    },
                ),
                named(
                    "Sparks",
                    ParticleSystem {
                        burst: 9,
                        duration: 0.2,
                        lifetime: [0.12, 0.3],
                        speed: [6.0, 14.0],
                        angle: 14.0,
                        drag: 4.0,
                        collision: ParticleCollision::Off,
                        ..sparks
                    },
                ),
                named(
                    "Smoke",
                    ParticleSystem {
                        rate: 0.0,
                        burst: 3,
                        looping: false,
                        duration: 0.2,
                        lifetime: [0.7, 1.2],
                        speed: [0.6, 1.6],
                        size: [0.12, 0.2],
                        size_end: 4.0,
                        color: [0.5, 0.5, 0.5, 0.3],
                        angle: 12.0,
                        radius: 0.01,
                        drag: 3.0,
                        gravity: -0.03,
                        softness: 0.15,
                        ..smoke
                    },
                ),
            ],
            Self::Explosion => vec![
                named(
                    "Flash",
                    ParticleSystem {
                        texture: flare,
                        rate: 0.0,
                        burst: 2,
                        looping: false,
                        duration: 3.0,
                        lifetime: [0.08, 0.12],
                        speed: [0.0, 0.0],
                        size: [3.5, 4.5],
                        size_end: 1.5,
                        color: [1.0, 0.9, 0.7, 1.0],
                        color_end: [1.0, 0.6, 0.2, 0.0],
                        intensity: 14.0,
                        shape: EmitShape::Point,
                        softness: 0.5,
                        ..glow
                    },
                ),
                named(
                    "Fireball",
                    ParticleSystem {
                        texture: puff,
                        rate: 0.0,
                        burst: 26,
                        looping: false,
                        duration: 3.0,
                        lifetime: [0.5, 0.95],
                        speed: [2.0, 6.0],
                        size: [1.0, 1.7],
                        size_end: 1.8,
                        color_gradient: hot,
                        opacity_curve: Curve::new(&[[0.0, 1.0], [0.5, 0.8], [1.0, 0.0]]),
                        intensity: 5.0,
                        light: 60.0,
                        light_range: 18.0,
                        shape: EmitShape::Sphere,
                        radius: 0.4,
                        gravity: -0.15,
                        drag: 3.0,
                        spin: 60.0,
                        softness: 0.5,
                        ..glow
                    },
                ),
                named(
                    "Smoke",
                    ParticleSystem {
                        rate: 0.0,
                        burst: 28,
                        looping: false,
                        duration: 3.0,
                        delay: 0.08,
                        lifetime: [2.5, 4.5],
                        speed: [1.5, 5.0],
                        size: [1.2, 2.0],
                        size_end: 2.6,
                        color: [0.1, 0.1, 0.1, 0.7],
                        opacity_curve: swell(0.08),
                        shape: EmitShape::Sphere,
                        radius: 0.5,
                        gravity: -0.04,
                        drag: 2.2,
                        wind: 0.5,
                        forces: 0.0,
                        ..smoke
                    },
                ),
                named(
                    "Sparks",
                    ParticleSystem {
                        burst: 70,
                        duration: 3.0,
                        lifetime: [0.6, 1.5],
                        speed: [6.0, 17.0],
                        shape: EmitShape::Sphere,
                        radius: 0.2,
                        ..sparks
                    },
                ),
                named(
                    "Debris",
                    ParticleSystem {
                        mesh: Some(ParticleMesh::Builtin(BuiltinMesh::Chunk)),
                        rate: 0.0,
                        burst: 26,
                        looping: false,
                        duration: 3.0,
                        lifetime: [1.8, 3.0],
                        speed: [4.0, 11.0],
                        size: [0.08, 0.22],
                        color: [0.16, 0.14, 0.12, 1.0],
                        color_end: [0.16, 0.14, 0.12, 1.0],
                        opacity_curve: Curve::new(&[[0.0, 1.0], [0.85, 1.0], [1.0, 0.0]]),
                        shape: EmitShape::Sphere,
                        radius: 0.3,
                        gravity: 1.0,
                        drag: 0.2,
                        spin: 420.0,
                        softness: 0.0,
                        collision: ParticleCollision::Depth,
                        bounce: 0.35,
                        friction: 0.45,
                        ..haze
                    },
                ),
                named(
                    "Shockwave",
                    ParticleSystem {
                        texture: ring,
                        rate: 0.0,
                        burst: 1,
                        looping: false,
                        duration: 3.0,
                        lifetime: [0.28, 0.28],
                        speed: [0.0, 0.0],
                        size: [1.0, 1.0],
                        size_end: 9.0,
                        color: [1.0, 0.9, 0.8, 0.5],
                        color_end: [1.0, 0.9, 0.8, 0.0],
                        intensity: 2.0,
                        shape: EmitShape::Point,
                        softness: 0.5,
                        ..glow
                    },
                ),
            ],
            Self::Magic => vec![
                named(
                    "Motes",
                    ParticleSystem {
                        rate: 70.0,
                        lifetime: [1.0, 2.0],
                        speed: [0.2, 0.7],
                        size: [0.03, 0.07],
                        color_gradient: Gradient::new(&[
                            [0.0, 0.5, 0.95, 1.0],
                            [0.6, 0.45, 0.3, 1.0],
                            [1.0, 0.25, 0.05, 0.6],
                        ]),
                        opacity_curve: swell(0.2),
                        intensity: 6.0,
                        shape: EmitShape::Sphere,
                        radius: 0.5,
                        gravity: -0.05,
                        softness: 0.05,
                        turbulence: 1.4,
                        turbulence_size: 0.6,
                        forces: 1.0,
                        // Each mote a thread of light along the way the swirls have taken it.
                        trails: ParticleTrails::Particles,
                        trail_time: 0.25,
                        trail_width: 0.6,
                        ..glow
                    },
                ),
                named(
                    "Glow",
                    ParticleSystem {
                        texture: flare,
                        rate: 5.0,
                        lifetime: [0.5, 0.9],
                        speed: [0.0, 0.1],
                        size: [0.6, 1.0],
                        size_end: 1.6,
                        color: [0.4, 0.6, 1.0, 0.6],
                        opacity_curve: swell(0.3),
                        intensity: 3.0,
                        light: 4.0,
                        light_range: 6.0,
                        shape: EmitShape::Point,
                        spin: 40.0,
                        softness: 0.3,
                        ..glow
                    },
                ),
                named(
                    "Pulse",
                    ParticleSystem {
                        texture: ring,
                        rate: 1.2,
                        lifetime: [0.8, 0.8],
                        speed: [0.0, 0.0],
                        size: [0.3, 0.3],
                        size_end: 8.0,
                        color: [0.5, 0.7, 1.0, 0.5],
                        color_end: [0.4, 0.3, 1.0, 0.0],
                        intensity: 3.0,
                        shape: EmitShape::Point,
                        softness: 0.3,
                        ..glow
                    },
                ),
            ],
            Self::Mud => vec![
                named(
                    "Clods",
                    ParticleSystem {
                        mesh: Some(ParticleMesh::Builtin(BuiltinMesh::Chunk)),
                        rate: 0.0,
                        burst: 34,
                        looping: false,
                        duration: 2.0,
                        lifetime: [1.5, 2.6],
                        speed: [2.5, 7.0],
                        size: [0.06, 0.17],
                        color: [0.11, 0.075, 0.045, 1.0],
                        color_end: [0.11, 0.075, 0.045, 1.0],
                        opacity_curve: Curve::new(&[[0.0, 1.0], [0.85, 1.0], [1.0, 0.0]]),
                        angle: 40.0,
                        radius: 0.2,
                        gravity: 1.0,
                        drag: 0.3,
                        spin: 300.0,
                        softness: 0.0,
                        collision: ParticleCollision::Depth,
                        bounce: 0.08,
                        friction: 0.9,
                        ..haze
                    },
                ),
                named(
                    "Spatter",
                    ParticleSystem {
                        rate: 0.0,
                        burst: 60,
                        looping: false,
                        duration: 2.0,
                        lifetime: [0.6, 1.2],
                        speed: [3.0, 9.0],
                        size: [0.015, 0.03],
                        stretch: 0.02,
                        color: [0.13, 0.09, 0.055, 1.0],
                        color_end: [0.13, 0.09, 0.055, 1.0],
                        angle: 50.0,
                        radius: 0.2,
                        gravity: 1.0,
                        softness: 0.0,
                        collision: ParticleCollision::Depth,
                        life_lost: 1.0,
                        ..haze
                    },
                ),
                named(
                    "Dust",
                    ParticleSystem {
                        texture: puff,
                        rate: 0.0,
                        burst: 6,
                        looping: false,
                        duration: 2.0,
                        lifetime: [0.8, 1.4],
                        speed: [0.8, 2.2],
                        size: [0.3, 0.5],
                        size_end: 3.0,
                        color: [0.3, 0.24, 0.17, 0.4],
                        angle: 45.0,
                        drag: 2.5,
                        softness: 0.3,
                        wind: 0.5,
                        ..haze
                    },
                ),
            ],
        };
        ParticleEffect { emitters }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_read_back_as_written_and_say_what_is_wrong() {
        for texture in BuiltinTexture::ALL {
            assert_eq!(texture.reference().parse(), Ok(ParticleTexture::Builtin(texture)));
        }
        for starter in Starter::ALL {
            assert_eq!(starter.reference().parse(), Ok(EffectRef::Builtin(starter)));
        }
        assert_eq!(Starter::MuzzleFlash.reference(), "builtin:muzzle_flash");
        let guid = AssetGuid::new_random();
        assert_eq!(guid.to_string().parse(), Ok(EffectRef::Asset(guid)));
        assert_eq!(guid.to_string().parse(), Ok(ParticleTexture::Asset(guid)));
        assert!("builtin:nothing".parse::<EffectRef>().unwrap_err().contains("builtin:fire"));
        assert!("not a guid".parse::<ParticleTexture>().is_err());
    }

    #[test]
    fn built_in_textures_are_shapes_in_alpha_that_never_reach_their_tiles_edges() {
        assert!(BuiltinTexture::Dot.picture().is_none(), "the Dot is drawn without one");
        for texture in BuiltinTexture::PICTURES {
            let (side, pixels) = texture.picture().unwrap();
            let alpha = |x: u32, y: u32| pixels[((y * side + x) * 4 + 3) as usize];
            let [across, down] = texture.tiles();
            let tile = side / across.max(down);
            let covered = pixels.chunks(4).filter(|pixel| pixel[3] > 40).count() as f32 / (side * side) as f32;
            assert!((0.03..0.7).contains(&covered), "{}: {covered} of it covered", texture.name());
            for edge in 0..side {
                for line in (0..=side).step_by(tile as usize) {
                    let line = line.min(side - 1);
                    assert_eq!((alpha(edge, line), alpha(line, edge)), (0, 0), "{} at {edge}, {line}", texture.name());
                }
            }
        }
    }

    /// With `VOXEAR_TEST_OUTPUT` set, the built-in textures side by side on grey, to look at.
    #[test]
    fn built_in_textures_can_be_looked_at() {
        let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") else { return };
        let side = PICTURE_SIDE as usize;
        let mut sheet = vec![90u8; side * BuiltinTexture::PICTURES.len() * side * 3];
        for (slot, texture) in BuiltinTexture::PICTURES.into_iter().enumerate() {
            let (_, pixels) = texture.picture().unwrap();
            for (i, pixel) in pixels.chunks(4).enumerate() {
                let at = ((i / side) * side * BuiltinTexture::PICTURES.len() + slot * side + i % side) * 3;
                let alpha = f32::from(pixel[3]) / 255.0;
                let shown = (f32::from(pixel[0]) * alpha + 90.0 * (1.0 - alpha)) as u8;
                sheet[at..at + 3].copy_from_slice(&[shown; 3]);
            }
        }
        let file = std::fs::File::create(std::path::Path::new(&dir).join("particle textures.png")).unwrap();
        let mut encoder = png::Encoder::new(file, (side * BuiltinTexture::PICTURES.len()) as u32, side as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.write_header().unwrap().write_image_data(&sheet).unwrap();
    }

    #[test]
    fn the_puffs_light_maps_are_brighter_on_the_side_the_light_comes_from() {
        assert!(BuiltinTexture::Flame.light_maps().is_none(), "only the Puff is lit from six sides");
        let (side, [first, second]) = BuiltinTexture::Puff.light_maps().unwrap();
        // The first puff's tile: its right half against its left, lit from the right and from the left.
        let tile = side / 2;
        let lit = |map: &[u8], channel: usize, columns: std::ops::Range<u32>| {
            let rows = tile / 4..tile * 3 / 4;
            let pixels = rows.flat_map(|y| columns.clone().map(move |x| (x, y)));
            pixels.map(|(x, y)| u64::from(map[((y * side + x) * 4) as usize + channel])).sum::<u64>()
        };
        let (left, right) = (tile / 8..tile / 2, tile / 2..tile * 7 / 8);
        assert!(lit(&first, 0, right.clone()) > lit(&first, 0, left.clone()) * 5 / 4, "from the right, its right");
        assert!(lit(&second, 0, left.clone()) > lit(&second, 0, right.clone()) * 5 / 4, "from the left, its left");
        // Light from behind comes through its thin edge more than its thick middle.
        let middle = u32::from(first[(((tile / 2) * side + tile / 2) * 4) as usize + 2]);
        assert!(middle < 120, "little comes through its middle: {middle}");
        // Its shape is the Puff's own.
        let (_, flat) = BuiltinTexture::Puff.picture().unwrap();
        assert!(flat.chunks(4).zip(first.chunks(4)).all(|(flat, lit)| flat[3] == lit[3]));
    }

    #[test]
    fn an_effect_reads_back_as_written_with_only_what_is_not_as_new() {
        let effect = Starter::Explosion.effect();
        assert_eq!(effect.emitters.len(), 6);
        let text = effect.to_toml().unwrap();
        assert!(text.contains("name = \"Fireball\"") && text.contains("texture = \"builtin:puff\""), "{text}");
        assert!(!text.contains("softness = 0.3"), "what is as a new system has it is left out: {text}");
        assert_eq!(&ParticleEffect::from_toml(&text).unwrap(), effect);
        let named = ParticleEffect::from_toml("[[emitter]]\nrate = 3.0\n[[emitter]]\nname = \"Second\"").unwrap();
        assert_eq!((named.emitters[0].name.as_str(), named.emitters[1].name.as_str()), ("Emitter 1", "Second"));
        assert!(ParticleEffect::from_toml("[[emitter]]\nspeeed = 1.0").unwrap_err().contains("speeed"));
        assert!(ParticleEffect::from_toml("emitters = 2").is_err());
        // An emitter does not play an effect of its own.
        let nested = ParticleEffect::from_toml("[[emitter]]\neffect = \"builtin:fire\"").unwrap();
        assert_eq!(nested.emitters[0].system.effect, None);
    }

    #[test]
    fn a_system_runs_its_effects_emitters_or_itself() {
        let own = ParticleSystem::default();
        assert_eq!(emitters(None, &own).count(), 1);
        let fire = ParticleSystem { effect: Some(EffectRef::Builtin(Starter::Fire)), ..own };
        let names: Vec<_> = Starter::Fire.effect().emitters.iter().map(|emitter| emitter.name.as_str()).collect();
        assert_eq!((emitters(None, &fire).count(), names), (3, vec!["Flames", "Smoke", "Embers"]));
        let guid = AssetGuid::new_random();
        let asset = ParticleSystem { effect: Some(EffectRef::Asset(guid)), ..own };
        assert_eq!(emitters(None, &asset).count(), 0, "nothing until its effect is read");
        let library = ParticleEffects(HashMap::from([(guid, Arc::new(Starter::Mud.effect().clone()))]));
        assert_eq!(emitters(Some(&library), &asset).count(), 3);
        // A fire burns on; an explosion is over once its smoke has gone: its delay, its round, and its longest life.
        assert_eq!(ends_after(None, &fire), None);
        let explosion = ParticleSystem { effect: Some(EffectRef::Builtin(Starter::Explosion)), ..own };
        assert_eq!(ends_after(None, &explosion), Some(0.08 + 3.0 + 4.5));
        assert_eq!(ends_after(None, &asset), None, "not read yet");
        // Every starter is simulated or not as its emitters need, and none asks for more than a system may have.
        for starter in Starter::ALL {
            for emitter in &starter.effect().emitters {
                let room =
                    if emitter.system.is_simulated() { emitter.system.capacity() } else { emitter.system.slots() };
                assert!(room > 0 && room <= 4096, "{} {}: room for {room}", starter.name(), emitter.name);
            }
        }
    }
}
