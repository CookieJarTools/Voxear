//! Shader packs: programs of a pack's own that run over the picture as the
//! frame is drawn, chosen by whoever plays, as a look pack is. A pack is a
//! folder, `Name.shaderpack`, holding its manifest, `pack.toml`, and its
//! programs as `.slang` files:
//!
//! ```toml
//! format = 1
//! description = "Long warm evenings and rays through the leaves."
//!
//! [options]
//! rays = { kind = "toggle", default = true, label = "Rays through leaves" }
//! strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength" }
//!
//! [[pass]]
//! name = "Rays"
//! program = "rays.slang"
//! at = "after_fog"
//! reads = ["depth", "color"]
//! writes = { target = "rays", format = "rgba16f", scale = 0.5 }
//!
//! [[pass]]
//! name = "Grade"
//! program = "grade.slang"
//! at = "final"
//! reads = ["color", "rays"]
//! ```
//!
//! Each pass is a full-screen program run at one [`PackPoint`] of the
//! frame. It reads the frame's images its `reads` lists ([`PassReads`]) and
//! the pack's own targets written before it, and writes the frame's colour
//! there or a target of its own. A program defines `float4 run(PassInput
//! input)` against `shaders/shader_pack.slang`; [`wrapper`] wraps it in a
//! file that defines the pack's options and targets.
//!
//! The engine reads a pack ([`PackManifest::parse`]), compiles its programs
//! ([`compile`]) and wears it by putting a [`WornShaderPack`] in the world;
//! the renderer makes its pipelines and draws its passes from the next
//! frame. Without one, nothing of this runs and the frame is as it was.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    pipelines::{HDR_FORMAT, LDR_FORMAT},
};

/// The `pack.toml` format this version reads.
pub const PACK_FORMAT: u32 = 1;

/// What a pack's folder's name ends with.
pub const PACK_EXTENSION: &str = "shaderpack";

/// The manifest in a pack's folder.
pub const MANIFEST: &str = "pack.toml";

/// The most a pack has of each: passes, targets of its own, and options.
pub const MAX_PASSES: usize = 16;
pub const MAX_TARGETS: usize = 8;
pub const MAX_OPTIONS: usize = 32;

/// The engine's own packs: what each is named by after `builtin:`, what a
/// menu calls it, and its files.
pub const BUILTIN: [(&str, &str, &[(&str, &str)]); 4] = [
    (
        "evening",
        "Evening",
        &[
            ("pack.toml", include_str!("../shader_packs/evening.shaderpack/pack.toml")),
            ("rays.slang", include_str!("../shader_packs/evening.shaderpack/rays.slang")),
            ("light.slang", include_str!("../shader_packs/evening.shaderpack/light.slang")),
            ("grade.slang", include_str!("../shader_packs/evening.shaderpack/grade.slang")),
            ("faces.slang", include_str!("../shader_packs/evening.shaderpack/faces.slang")),
            ("water.slang", include_str!("../shader_packs/evening.shaderpack/water.slang")),
            ("sky.slang", include_str!("../shader_packs/evening.shaderpack/sky.slang")),
            ("models.slang", include_str!("../shader_packs/evening.shaderpack/models.slang")),
            ("shadows.slang", include_str!("../shader_packs/evening.shaderpack/shadows.slang")),
        ],
    ),
    (
        "vivid",
        "Vivid",
        &[
            ("pack.toml", include_str!("../shader_packs/vivid.shaderpack/pack.toml")),
            ("grade.slang", include_str!("../shader_packs/vivid.shaderpack/grade.slang")),
            ("sky.slang", include_str!("../shader_packs/vivid.shaderpack/sky.slang")),
        ],
    ),
    (
        "radiant",
        "Radiant",
        &[
            ("pack.toml", include_str!("../shader_packs/radiant.shaderpack/pack.toml")),
            ("air.slang", include_str!("../shader_packs/radiant.shaderpack/air.slang")),
            ("light.slang", include_str!("../shader_packs/radiant.shaderpack/light.slang")),
            ("grade.slang", include_str!("../shader_packs/radiant.shaderpack/grade.slang")),
            ("faces.slang", include_str!("../shader_packs/radiant.shaderpack/faces.slang")),
            ("water.slang", include_str!("../shader_packs/radiant.shaderpack/water.slang")),
            ("sky.slang", include_str!("../shader_packs/radiant.shaderpack/sky.slang")),
            ("models.slang", include_str!("../shader_packs/radiant.shaderpack/models.slang")),
            ("window.slang", include_str!("../shader_packs/radiant.shaderpack/window.slang")),
        ],
    ),
    (
        "pixel",
        "Pixel",
        &[
            ("pack.toml", include_str!("../shader_packs/pixel.shaderpack/pack.toml")),
            ("bands.slang", include_str!("../shader_packs/pixel.shaderpack/bands.slang")),
            ("faces.slang", include_str!("../shader_packs/pixel.shaderpack/faces.slang")),
            ("models.slang", include_str!("../shader_packs/pixel.shaderpack/models.slang")),
            ("sky.slang", include_str!("../shader_packs/pixel.shaderpack/sky.slang")),
            ("grade.slang", include_str!("../shader_packs/pixel.shaderpack/grade.slang")),
            ("outline.slang", include_str!("../shader_packs/pixel.shaderpack/outline.slang")),
        ],
    ),
];

/// Where a built-in pack's files are in a source checkout, to be watched
/// and read again as they change while the engine is worked on.
pub fn builtin_folder(name: &str) -> Option<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("shader_packs").join(format!("{name}.{PACK_EXTENSION}"));
    folder.join(MANIFEST).is_file().then_some(folder)
}

/// One of the points of the frame a pass runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackPoint {
    /// Once the opaque surfaces and the sky are lit, before water and glass,
    /// at the size the scene is drawn.
    AfterLighting,
    /// Once the fog is laid over the scene, before smoke and particles.
    AfterFog,
    /// Once TAA has resolved the frame, before bloom and exposure, at the
    /// size the picture is shown.
    AfterTaa,
    /// In place of the engine's tonemap, its grade and a look pack's
    /// shader: a pass that writes the colour here reads the scene's light
    /// after TAA, and the glow (`bloom`), and writes the picture as shown,
    /// from 0 to 1.
    Tonemap,
    /// Over the finished picture, after the tonemap, the grade and a look
    /// pack's shader, before sharpening and the UI: its colours are as
    /// shown, from 0 to 1.
    Final,
}

impl PackPoint {
    pub const ALL: [Self; 5] = [Self::AfterLighting, Self::AfterFog, Self::AfterTaa, Self::Tonemap, Self::Final];

    /// As a manifest writes it.
    pub fn key(self) -> &'static str {
        match self {
            Self::AfterLighting => "after_lighting",
            Self::AfterFog => "after_fog",
            Self::AfterTaa => "after_taa",
            Self::Tonemap => "tonemap",
            Self::Final => "final",
        }
    }

    /// As the Inspector shows it.
    pub fn title(self) -> &'static str {
        match self {
            Self::AfterLighting => "After lighting",
            Self::AfterFog => "After fog",
            Self::AfterTaa => "After TAA",
            Self::Tonemap => "Tonemap",
            Self::Final => "Final",
        }
    }

    /// Whether its colour is the scene's own image, written in place, so
    /// what is drawn after it draws over the pack's work.
    fn in_place(self) -> bool {
        matches!(self, Self::AfterLighting | Self::AfterFog)
    }
}

/// What a target of a pack's own holds a pixel as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetFormat {
    /// Four channels of light, as the frame's own colour is.
    #[default]
    Rgba16f,
    /// Four channels from 0 to 1, a byte each.
    Rgba8,
    /// Two channels of numbers.
    Rg16f,
    /// One channel of a number.
    R16f,
}

impl TargetFormat {
    fn vk(self) -> vk::Format {
        match self {
            Self::Rgba16f => vk::Format::R16G16B16A16_SFLOAT,
            Self::Rgba8 => vk::Format::R8G8B8A8_UNORM,
            Self::Rg16f => vk::Format::R16G16_SFLOAT,
            Self::R16f => vk::Format::R16_SFLOAT,
        }
    }
}

/// How large a target of a pack's own is beside the frame's images at the
/// point that writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetScale {
    Full,
    Half,
    Quarter,
}

impl TargetScale {
    fn of(scale: f32) -> Result<Self> {
        Ok(match scale {
            1.0 => Self::Full,
            0.5 => Self::Half,
            0.25 => Self::Quarter,
            _ => bail!("`scale` is 1, 0.5 or 0.25, not {scale}"),
        })
    }

    pub fn share(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Half => 0.5,
            Self::Quarter => 0.25,
        }
    }

    fn of_extent(self, extent: vk::Extent2D) -> vk::Extent2D {
        let divisor = match self {
            Self::Full => 1,
            Self::Half => 2,
            Self::Quarter => 4,
        };
        vk::Extent2D { width: extent.width.div_ceil(divisor).max(1), height: extent.height.div_ceil(divisor).max(1) }
    }
}

/// The frame's images a pass reads, as its `reads` lists them. What a pass
/// does not list it cannot read: its program has no function for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PassReads {
    /// The frame's colour at the pass's point.
    pub color: bool,
    pub depth: bool,
    /// Each pixel's normal and roughness, which the depth passes write when
    /// a pass reads them.
    pub normals: bool,
    pub motion: bool,
    /// Last frame's colour, after TAA.
    pub history: bool,
    /// The sky light and block light round the camera, in a world of blocks.
    pub block_light: bool,
    /// The glow of bright light, as the engine's bloom has worked it out:
    /// at the tonemap and after.
    pub bloom: bool,
    /// What each pixel's surface is, which the depth passes write beside
    /// its normal when a pass reads them.
    pub surface_ids: bool,
    /// The sun's shadow, at any point of the world: its virtual shadow
    /// pages or cascades, for shafts of light marched through the air.
    pub shadow: bool,
    /// The pack's own targets, by their place among [`PackManifest::targets`].
    pub targets: Vec<usize>,
}

/// The engine's images, as `reads` names them, and which part of
/// [`PassReads`] each is, with the functions a program reads it by.
const ENGINE_READS: [(&str, &str, &[&str]); 9] = [
    ("color", "PACK_READS_COLOR", &["sceneColor", "sceneColorAt", "colorSize"]),
    ("depth", "PACK_READS_DEPTH", &["sceneDepth", "isSky", "sceneDistance", "scenePosition"]),
    ("normals", "PACK_READS_NORMALS", &["sceneNormal", "sceneRoughness"]),
    ("motion", "PACK_READS_MOTION", &["sceneMotion", "lastUv"]),
    ("history", "PACK_READS_HISTORY", &["lastColor"]),
    ("block_light", "PACK_READS_BLOCK_LIGHT", &["lightAround", "hasBlockLight", "blockGlow"]),
    ("bloom", "PACK_READS_BLOOM", &["sceneBloom"]),
    ("surface_id", "PACK_READS_SURFACE_ID", &["sceneSurfaceId"]),
    ("shadow", "PACK_READS_SHADOW", &["sunVisibility", "sunShadowsReach"]),
];

impl PassReads {
    fn engine(&self) -> [bool; 9] {
        let Self { color, depth, normals, motion, history, block_light, bloom, surface_ids, shadow, .. } = *self;
        [color, depth, normals, motion, history, block_light, bloom, surface_ids, shadow]
    }

    /// The names `reads` gives them, for the Inspector.
    pub fn names(&self, manifest: &PackManifest) -> Vec<String> {
        let engine = ENGINE_READS.iter().zip(self.engine()).filter(|(_, read)| *read).map(|((name, ..), _)| *name);
        let targets = self.targets.iter().map(|&target| manifest.targets[target].name.as_str());
        engine.chain(targets).map(str::to_string).collect()
    }
}

/// What a pass writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassWrites {
    /// The frame's colour at its point.
    Color,
    /// A target of the pack's own, by its place among
    /// [`PackManifest::targets`].
    Target(usize),
}

/// One pass of a pack.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackPass {
    pub name: String,
    /// Its program's file, in the pack's folder.
    pub program: String,
    pub point: PackPoint,
    pub reads: PassReads,
    pub writes: PassWrites,
}

/// A target of a pack's own: an image a pass writes and later passes read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackTarget {
    pub name: String,
    pub format: TargetFormat,
    pub scale: TargetScale,
    /// Kept from frame to frame, so a pass reads last frame's as well
    /// (`target.last(uv)`).
    pub kept: bool,
    /// The point of the pass that writes it, which sizes it.
    pub point: PackPoint,
}

/// An option of a pack's, which whoever plays sets in its menu.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PackOption {
    /// As the manifest and the programs name it.
    pub key: String,
    pub label: String,
    /// A line for its tooltip.
    pub about: String,
    #[serde(flatten)]
    pub kind: OptionKind,
}

/// What an option is, and what it is when no one has set it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OptionKind {
    /// A slider between two numbers.
    Number {
        low: f32,
        high: f32,
        default: f32,
    },
    Toggle {
        default: bool,
    },
    /// One of a few, by its place among them.
    Choice {
        choices: Vec<String>,
        default: usize,
    },
    /// A colour, picked from its swatch: sRGB from 0 to 1.
    Color {
        default: [f32; 3],
    },
}

/// A colour option's value as its one number holds it: its sRGB channels
/// to 8 bits each, red highest, which a float holds exactly.
pub fn color_value(srgb: [f32; 3]) -> f32 {
    let [r, g, b] = srgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u32);
    ((r << 16) | (g << 8) | b) as f32
}

/// The sRGB colour a colour option's value holds.
pub fn value_color(value: f32) -> [f32; 3] {
    let bits = value.round().clamp(0.0, 16_777_215.0) as u32;
    [bits >> 16, bits >> 8, bits].map(|c| (c & 255) as f32 / 255.0)
}

impl PackOption {
    /// Its value as the programs read it when no one has set it.
    pub fn default_value(&self) -> f32 {
        match &self.kind {
            OptionKind::Number { default, .. } => *default,
            OptionKind::Toggle { default } => f32::from(u8::from(*default)),
            OptionKind::Choice { default, .. } => *default as f32,
            OptionKind::Color { default } => color_value(*default),
        }
    }

    /// A value set for it, kept to what it can be.
    pub fn kept(&self, value: f32) -> f32 {
        match &self.kind {
            OptionKind::Number { low, high, .. } => value.clamp(*low, *high),
            OptionKind::Toggle { .. } => f32::from(u8::from(value >= 0.5)),
            OptionKind::Choice { choices, .. } => value.round().clamp(0.0, choices.len().saturating_sub(1) as f32),
            OptionKind::Color { .. } => value.round().clamp(0.0, 16_777_215.0),
        }
    }
}

/// A pack's manifest, read and checked: everything it says it does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackManifest {
    /// A line for the menu a player picks it from.
    pub description: String,
    pub options: Vec<PackOption>,
    /// In the order they run: by their points, then as the manifest lists
    /// them.
    pub passes: Vec<PackPass>,
    pub targets: Vec<PackTarget>,
    /// Its surface programs, which light kinds of geometry in place of the
    /// engine.
    pub surfaces: PackSurfaces,
    /// How the sun's shadows are drawn and read under it.
    pub shadows: PackShadows,
    /// The scene drawn in pixels under it, enlarged with hard edges (see
    /// `pixels`), in place of the settings' own.
    pub pixels: Option<PackPixels>,
}

/// What a pack asks of drawing in pixels (`[pixels]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackPixels {
    pub size: PackPixelSize,
    /// Keeps an orthographic camera on the grid of its pixels.
    pub snap: bool,
}

/// Screen pixels each drawn pixel covers, under a pack: a whole number, or a
/// number option's, by its place among the pack's options, so a player picks
/// it. 1 or less leaves the settings' own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackPixelSize {
    Fixed(u32),
    Option(usize),
}

impl PackPixels {
    /// The settings it puts in place under `options`, the pack's values in
    /// its manifest's order: none where its size is 1 or less.
    pub fn settings(&self, options: &[f32]) -> Option<crate::Pixels> {
        let size = match self.size {
            PackPixelSize::Fixed(size) => size,
            PackPixelSize::Option(at) => options.get(at).map_or(1, |value| value.round().max(0.0) as u32),
        };
        let size = size.min(crate::pixels::MOST_PIXEL_SIZE);
        (size > 1).then_some(crate::Pixels { enabled: true, size, height: 0, snap: self.snap })
    }
}

/// What a pack sets of the sun's shadows: each in place of the level's
/// lighting where it is set, and a program that reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackShadows {
    /// A `.slang` file in its folder that defines `float run(ShadowSurface
    /// surface, ShadowLighting light)`: how much of the sun reaches a point,
    /// in place of the engine's soft shadow (see
    /// `shaders/shader_pack_surface.slang`).
    pub filter: Option<String>,
    /// How wide the sun is, in degrees: the wider, the softer its shadows
    /// grow away from what casts them.
    pub softness: Option<f32>,
    /// How far from the camera the sun's cascades reach, in metres.
    pub distance: Option<f32>,
    /// The colour shade is lifted by, as the sky's light fills it.
    pub tint: Option<[f32; 3]>,
    /// Virtual shadow maps or cascades.
    pub maps: Option<crate::shadows::SunShadows>,
    /// How many cascades share the distance, 1 to 4: fewer draw less, each
    /// less finely.
    pub cascades: Option<u32>,
    /// How the cascades share it, from 0, alike, to 1, each twice the one
    /// before, the nearest small and sharp.
    pub split: Option<f32>,
    /// Leaves draw the colour the sun's light takes through them into the
    /// glass shadow atlas, as `light.through` gives a filter, and the light
    /// a filter lets through them is tinted by it.
    pub leaves: bool,
}

/// The engine's shader modules that read the sun's shadow (`sunShadow` in
/// `shaders/shading.slang`), which a pack's shadow filter is compiled into.
pub const SHADOW_MODULES: [&str; 9] =
    ["forward", "blocks", "terrain", "grass", "meshlets", "dice", "glass", "water", "particles"];

/// A pack's surface programs, each a `.slang` file in its folder that
/// defines `float3 run(BlockSurface surface, BlockLighting light)` (see
/// `shaders/shader_pack_surface.slang`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PackSurfaces {
    /// Block faces, and leaves too where there is no `leaves`.
    pub blocks: Option<String>,
    /// Faces of leaves, and sheets of leaves and of vines.
    pub leaves: Option<String>,
    /// Water: the sea, Pools, Streams and worlds of blocks' water; its
    /// program defines `float3 run(WaterSurface surface, WaterLighting
    /// light)`.
    pub water: Option<String>,
    /// The sky seen behind everything, and the clouds and far weather in
    /// front of it; its program defines `float3 run(SkySurface surface,
    /// SkyLighting light)`.
    pub sky: Option<String>,
    /// Models: characters, props, creatures, anything drawn from a mesh; its
    /// program defines `float3 run(ModelSurface surface, ModelLighting
    /// light)`.
    pub models: Option<String>,
}

impl PackSurfaces {
    /// Its programs, as the manifest names them.
    pub fn named(&self) -> Vec<(&'static str, &str)> {
        let named = [
            ("blocks", &self.blocks),
            ("leaves", &self.leaves),
            ("water", &self.water),
            ("sky", &self.sky),
            ("models", &self.models),
        ];
        named.into_iter().filter_map(|(key, program)| program.as_deref().map(|program| (key, program))).collect()
    }
}

/// `pack.toml` as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    #[serde(default = "pack_format")]
    format: u32,
    #[serde(default)]
    description: String,
    #[serde(default)]
    options: std::collections::BTreeMap<String, OptionData>,
    #[serde(default, rename = "pass")]
    passes: Vec<PassData>,
    #[serde(default)]
    surface: SurfaceData,
    #[serde(default)]
    shadows: ShadowsData,
    #[serde(default)]
    pixels: Option<PixelsData>,
    #[serde(default)]
    engine: Option<toml::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PixelsData {
    size: PixelSizeData,
    #[serde(default = "yes")]
    snap: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PixelSizeData {
    Fixed(u32),
    Option(String),
}

fn pack_format() -> u32 {
    PACK_FORMAT
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum OptionData {
    Number {
        range: [f32; 2],
        default: f32,
        #[serde(default)]
        label: String,
        #[serde(default)]
        about: String,
    },
    Toggle {
        default: bool,
        #[serde(default)]
        label: String,
        #[serde(default)]
        about: String,
    },
    Choice {
        choices: Vec<String>,
        default: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        about: String,
    },
    Color {
        default: [f32; 3],
        #[serde(default)]
        label: String,
        #[serde(default)]
        about: String,
    },
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceData {
    #[serde(default)]
    blocks: Option<String>,
    #[serde(default)]
    leaves: Option<String>,
    #[serde(default)]
    water: Option<String>,
    #[serde(default)]
    models: Option<String>,
    #[serde(default)]
    sky: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShadowsData {
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    softness: Option<f32>,
    #[serde(default)]
    distance: Option<f32>,
    #[serde(default)]
    tint: Option<[f32; 3]>,
    #[serde(default)]
    maps: Option<String>,
    #[serde(default)]
    leaves: bool,
    #[serde(default)]
    cascades: Option<u32>,
    #[serde(default)]
    split: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PassData {
    name: String,
    program: String,
    at: PackPoint,
    #[serde(default)]
    reads: Vec<String>,
    #[serde(default)]
    writes: Option<WritesData>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WritesData {
    target: String,
    #[serde(default)]
    format: TargetFormat,
    #[serde(default = "full_scale")]
    scale: f32,
    #[serde(default)]
    kept: bool,
}

fn full_scale() -> f32 {
    1.0
}

/// Words a program cannot take for an option's or a target's name: the
/// engine's images, and Slang's own words a name might be.
const RESERVED: [&str; 31] = [
    "color",
    "depth",
    "normals",
    "motion",
    "history",
    "block_light",
    "surface_id",
    "frame",
    "options",
    "input",
    "run",
    "push",
    "textures",
    "samplers",
    "in",
    "out",
    "inout",
    "if",
    "else",
    "for",
    "while",
    "return",
    "struct",
    "float",
    "int",
    "uint",
    "bool",
    "true",
    "false",
    "static",
    "const",
];

/// Whether `name` can name an option or a target in a program: small
/// letters, numbers and `_`, starting with a letter.
fn check_name(name: &str, what: &str) -> Result<()> {
    let mut letters = name.chars();
    let starts = letters.next().is_some_and(|first| first.is_ascii_lowercase());
    let rest = letters.all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '_');
    ensure!(starts && rest, "the {what} `{name}` is named with small letters, numbers and `_`, starting with a letter");
    ensure!(!RESERVED.contains(&name), "the {what} `{name}` takes a name the programs already have");
    Ok(())
}

/// A program's file: a `.slang` file in the pack's own folder.
/// `[pixels] size`, checked: a whole number of screen pixels, or the name of
/// a number option among `options`.
fn pixel_size(data: &PixelSizeData, options: &[PackOption]) -> Result<PackPixelSize> {
    let most = crate::pixels::MOST_PIXEL_SIZE;
    match data {
        PixelSizeData::Fixed(size) => {
            ensure!((1..=most).contains(size), "size: from 1 to {most} screen pixels, not {size}");
            Ok(PackPixelSize::Fixed(*size))
        }
        PixelSizeData::Option(name) => {
            let at = options.iter().position(|option| option.key == *name);
            let at = at.with_context(|| format!("size: no option `{name}`; name a number option, or give a number"))?;
            ensure!(
                matches!(options[at].kind, OptionKind::Number { .. }),
                "size: the option `{name}` is not a number"
            );
            Ok(PackPixelSize::Option(at))
        }
    }
}

/// The `[shadows]` table, checked.
fn shadows(data: &ShadowsData) -> Result<PackShadows> {
    if let Some(filter) = &data.filter {
        check_program(filter).context("filter")?;
    }
    if let Some(softness) = data.softness {
        ensure!((0.0..=20.0).contains(&softness), "softness: the sun is from 0 to 20 degrees wide, not {softness}");
    }
    if let Some(distance) = data.distance {
        ensure!((5.0..=1000.0).contains(&distance), "distance: from 5 to 1000 metres, not {distance}");
    }
    if let Some(tint) = data.tint {
        ensure!(tint.iter().all(|c| c.is_finite() && *c >= 0.0), "tint: a colour of three numbers from 0");
    }
    if let Some(count) = data.cascades {
        ensure!((1..=4).contains(&count), "cascades: from 1 to 4, not {count}");
    }
    if let Some(split) = data.split {
        ensure!((0.0..=1.0).contains(&split), "split: from 0 to 1, not {split}");
    }
    let maps = match &data.maps {
        Some(name) => {
            let maps = crate::shadows::SunShadows::named(name);
            Some(maps.with_context(|| format!("maps: `virtual` or `cascades`, not `{name}`"))?)
        }
        None => None,
    };
    Ok(PackShadows {
        filter: data.filter.clone(),
        softness: data.softness,
        distance: data.distance,
        tint: data.tint,
        maps,
        leaves: data.leaves,
        cascades: data.cascades,
        split: data.split,
    })
}

/// The pack's programs and the files of its own they include, as many deep
/// as they go, each by its place in the pack's folder, read by `read`: what
/// the pack is compiled from, and what a built game reads of it. A file it
/// includes that `read` has not, as the engine's own, is left to the
/// compiler.
pub fn program_files(manifest: &PackManifest, read: &mut dyn FnMut(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let mut waiting: Vec<String> = manifest.programs().into_iter().map(str::to_string).collect();
    while let Some(name) = waiting.pop() {
        if found.iter().any(|(seen, _)| *seen == name) {
            continue;
        }
        let Some(text) = read(&name) else { continue };
        let beside = name.rsplit_once('/').map_or("", |(folder, _)| folder);
        for line in text.lines() {
            let Some(included) = line.trim().strip_prefix("#include") else { continue };
            let included = included.trim().trim_matches(|c| c == '"' || c == '<' || c == '>');
            let place = if beside.is_empty() { included.to_string() } else { format!("{beside}/{included}") };
            if check_program(&place).is_ok() {
                waiting.push(place);
            }
        }
        found.push((name, text));
    }
    found.sort();
    found
}

fn check_program(program: &str) -> Result<()> {
    let path = Path::new(program);
    let inside = path.components().all(|part| matches!(part, std::path::Component::Normal(_)));
    ensure!(inside && !program.is_empty(), "the program `{program}` is not a file in the pack's folder");
    ensure!(path.extension().is_some_and(|e| e == "slang"), "the program `{program}` is not a .slang file");
    Ok(())
}

impl PackManifest {
    /// Reads a pack's manifest, and checks it can be drawn.
    pub fn parse(text: &str) -> Result<Self> {
        let file: ManifestFile = toml::from_str(text)?;
        ensure!(file.format <= PACK_FORMAT, "written by a newer Voxear (format {})", file.format);
        let surface = &file.surface;
        let surfaces = PackSurfaces {
            blocks: surface.blocks.clone(),
            leaves: surface.leaves.clone(),
            water: surface.water.clone(),
            sky: surface.sky.clone(),
            models: surface.models.clone(),
        };
        for (key, program) in surfaces.named() {
            check_program(program).with_context(|| format!("[surface] {key}"))?;
        }
        let shadows = shadows(&file.shadows).context("[shadows]")?;
        ensure!(file.engine.is_none(), "[engine]: a pack does not set the engine's own passes yet");
        // As the file lists them, which a menu keeps.
        let order = option_order(text);
        let mut written: Vec<(String, OptionData)> = file.options.into_iter().collect();
        written.sort_by_key(|(key, _)| order.iter().position(|named| named == key).unwrap_or(usize::MAX));
        let options = written.into_iter().map(|(key, data)| option(key, data)).collect::<Result<Vec<_>>>()?;
        let pixels = match &file.pixels {
            Some(data) => Some(PackPixels { size: pixel_size(&data.size, &options).context("[pixels]")?, snap: data.snap }),
            None => None,
        };
        ensure!(options.len() <= MAX_OPTIONS, "{} options are more than the {MAX_OPTIONS} a pack has", options.len());
        ensure!(file.passes.len() <= MAX_PASSES, "{} passes are more than {MAX_PASSES}", file.passes.len());
        ensure!(
            !file.passes.is_empty()
                || !surfaces.named().is_empty()
                || shadows != PackShadows::default()
                || pixels.is_some(),
            "the pack has no [[pass]], no [surface] programs, no [shadows] and no [pixels]"
        );
        // Every target is named by the pass that writes it.
        let mut targets: Vec<PackTarget> = Vec::new();
        for pass in &file.passes {
            let Some(writes) = &pass.writes else { continue };
            check_name(&writes.target, "target")?;
            ensure!(
                targets.iter().all(|target| target.name != writes.target),
                "the target `{}` is written by two passes; each target is written by one",
                writes.target
            );
            let scale = TargetScale::of(writes.scale).with_context(|| format!("the pass `{}`", pass.name))?;
            let (name, format, kept, point) = (writes.target.clone(), writes.format, writes.kept, pass.at);
            targets.push(PackTarget { name, format, scale, kept, point });
        }
        ensure!(targets.len() <= MAX_TARGETS, "{} targets are more than {MAX_TARGETS}", targets.len());
        // The order they run in: by their points, as the manifest lists them within one.
        let mut ordered: Vec<(usize, PassData)> = file.passes.into_iter().enumerate().collect();
        ordered.sort_by_key(|(at, pass)| (pass.at, *at));
        let mut passes = Vec::new();
        for (_, pass) in ordered {
            ensure!(!pass.name.trim().is_empty(), "a pass has no name");
            check_program(&pass.program).with_context(|| format!("the pass `{}`", pass.name))?;
            let writes = match &pass.writes {
                Some(writes) => {
                    PassWrites::Target(targets.iter().position(|target| target.name == writes.target).expect("named"))
                }
                None => PassWrites::Color,
            };
            let mut reads = PassReads::default();
            for read in &pass.reads {
                match read.as_str() {
                    "color" => reads.color = true,
                    "depth" => reads.depth = true,
                    "normals" => reads.normals = true,
                    "motion" => reads.motion = true,
                    "history" => reads.history = true,
                    "block_light" => reads.block_light = true,
                    "surface_id" => reads.surface_ids = true,
                    "shadow" => reads.shadow = true,
                    "bloom" => {
                        ensure!(
                            pass.at >= PackPoint::Tonemap,
                            "the pass `{}` reads `bloom`, which is there from the tonemap on",
                            pass.name
                        );
                        reads.bloom = true;
                    }
                    name => {
                        let Some(target) = targets.iter().position(|target| target.name == name) else {
                            bail!(
                                "the pass `{}` reads `{name}`, which is neither one of the frame's images (color, \
                                 depth, normals, motion, history, block_light, bloom, surface_id, shadow) nor a target a \
                                 pass writes",
                                pass.name
                            );
                        };
                        ensure!(
                            writes != PassWrites::Target(target) || targets[target].kept,
                            "the pass `{}` reads the target it writes; one that is `kept` reads last frame's",
                            pass.name
                        );
                        let written =
                            passes.iter().any(|before: &PackPass| before.writes == PassWrites::Target(target));
                        ensure!(
                            written || targets[target].kept,
                            "the pass `{}` reads `{name}` before the pass that writes it; a target that is `kept` \
                             can be read before, as last frame's",
                            pass.name
                        );
                        reads.targets.push(target);
                    }
                }
            }
            passes.push(PackPass { name: pass.name, program: pass.program, point: pass.at, reads, writes });
        }
        let tonemaps =
            passes.iter().filter(|pass| pass.point == PackPoint::Tonemap && pass.writes == PassWrites::Color);
        ensure!(tonemaps.count() <= 1, "two passes write the colour at the tonemap; one takes the engine's place");
        Ok(Self { description: file.description, options, passes, targets, surfaces, shadows, pixels })
    }

    /// Every program the pack names, each once: its passes', its surface
    /// programs and its shadow filter.
    pub fn programs(&self) -> Vec<&str> {
        let passes = self.passes.iter().map(|pass| pass.program.as_str());
        let surfaces = self.surfaces.named().into_iter().map(|(_, program)| program);
        let mut named: Vec<&str> = passes.chain(surfaces).chain(self.shadows.filter.as_deref()).collect();
        let mut seen = std::collections::HashSet::new();
        named.retain(|program| seen.insert(*program));
        named
    }

    /// The engine's shader modules the pack's programs are compiled into: its
    /// surface programs' and, for its shadow filter, every one that reads the
    /// sun's shadow.
    pub fn engine_modules(&self) -> Vec<&'static str> {
        let mut modules: Vec<&'static str> = SURFACE_MODULES
            .iter()
            .filter(|(module, _, _)| self.surfaces.compiles_into(module))
            .map(|(module, _, _)| *module)
            .collect();
        if self.shadows.filter.is_some() {
            modules.extend(SHADOW_MODULES.iter().filter(|module| !modules.contains(module)).collect::<Vec<_>>());
        }
        modules
    }

    /// One of the engine's own packs, by its name.
    pub fn builtin(name: &str) -> Result<Self> {
        let files = builtin_files(name)?;
        let text = files.iter().find(|(file, _)| *file == MANIFEST).map(|(_, text)| *text).unwrap_or_default();
        Self::parse(text).with_context(|| format!("reading the built-in shader pack {name}"))
    }

    /// Each option's value when no one has set it.
    pub fn default_values(&self) -> [f32; MAX_OPTIONS] {
        let mut values = [0.0; MAX_OPTIONS];
        for (value, option) in values.iter_mut().zip(&self.options) {
            *value = option.default_value();
        }
        values
    }

    /// Whether a pass runs at `point`.
    pub fn runs_at(&self, point: PackPoint) -> bool {
        self.passes.iter().any(|pass| pass.point == point)
    }
}

/// A built-in pack's files, by its name.
pub fn builtin_files(name: &str) -> Result<&'static [(&'static str, &'static str)]> {
    match BUILTIN.iter().find(|(key, ..)| *key == name) {
        Some((.., files)) => Ok(files),
        None => bail!("no built-in shader pack named `{name}`"),
    }
}

/// The options' names in the order the manifest writes them.
fn option_order(text: &str) -> Vec<String> {
    let Ok(document) = text.parse::<toml_edit::DocumentMut>() else { return Vec::new() };
    let Some(options) = document.get("options").and_then(toml_edit::Item::as_table_like) else { return Vec::new() };
    options.iter().map(|(key, _)| key.to_string()).collect()
}

fn option(key: String, data: OptionData) -> Result<PackOption> {
    check_name(&key, "option")?;
    let named = |label: String| if label.is_empty() { key.replace('_', " ") } else { label };
    Ok(match data {
        OptionData::Number { range: [low, high], default, label, about } => {
            ensure!(low < high, "the option `{key}`'s range runs from {low} up to {high}");
            ensure!((low..=high).contains(&default), "the option `{key}`'s default {default} is outside its range");
            PackOption { label: named(label), about, kind: OptionKind::Number { low, high, default }, key }
        }
        OptionData::Toggle { default, label, about } => {
            PackOption { label: named(label), about, kind: OptionKind::Toggle { default }, key }
        }
        OptionData::Choice { choices, default, label, about } => {
            ensure!(!choices.is_empty(), "the option `{key}` has no choices");
            let Some(chosen) = choices.iter().position(|choice| *choice == default) else {
                bail!("the option `{key}`'s default `{default}` is not one of its choices");
            };
            let names: Vec<String> = choices.iter().map(|choice| choice_constant(&key, choice)).collect();
            let distinct = names.iter().enumerate().all(|(at, name)| !names[..at].contains(name));
            ensure!(distinct, "the option `{key}` has two choices the programs would name alike");
            PackOption { label: named(label), about, kind: OptionKind::Choice { choices, default: chosen }, key }
        }
        OptionData::Color { default, label, about } => {
            ensure!(default.iter().all(|c| (0.0..=1.0).contains(c)), "the option `{key}`'s colour is sRGB from 0 to 1");
            PackOption { label: named(label), about, kind: OptionKind::Color { default }, key }
        }
    })
}

/// What a program names a choice by: `QUALITY_HIGH` for the choice `High`
/// of the option `quality`.
pub fn choice_constant(key: &str, choice: &str) -> String {
    let choice: String =
        choice.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("{}_{choice}", key.to_ascii_uppercase())
}

/// The file Voxear compiles for one pass: what its program reads, the
/// pack's options and targets as the program names them, the program, and
/// the fragment shader that runs it.
pub fn wrapper(manifest: &PackManifest, pass: &PackPass) -> String {
    let mut code = format!(
        "// Made by Voxear for the pass `{}` of a shader pack, round its program {}.\n\n",
        pass.name.replace('\n', " "),
        pass.program
    );
    for ((_, define, _), read) in ENGINE_READS.iter().zip(pass.reads.engine()) {
        code.push_str(&format!("#define {define} {}\n", u8::from(read)));
    }
    code.push_str("#include \"shader_pack.slang\"\n\n");
    code.push_str(&options_code(manifest));
    for (slot, target) in manifest.targets.iter().enumerate() {
        code.push_str(&format!("static const PackTarget {} = {{ {slot} }};\n", target.name));
    }
    code.push_str(&format!("\n#include \"{}\"\n\n", pass.program));
    code.push_str(
        "[shader(\"fragment\")]\nfloat4 packFragment(PackVertex v) : SV_Target\n{\n    return run(packInput(v));\n}\n",
    );
    code
}

/// The pack's options as its programs read them: an `Options` struct, a
/// constant for each choice, and `options()`, which reads them from the
/// frame.
fn options_code(manifest: &PackManifest) -> String {
    let mut code = String::new();
    code.push_str("struct Options\n{\n");
    for option in &manifest.options {
        let kind = match option.kind {
            OptionKind::Number { .. } => "float",
            OptionKind::Toggle { .. } => "bool",
            OptionKind::Choice { .. } => "uint",
            OptionKind::Color { .. } => "float3",
        };
        code.push_str(&format!("    {kind} {};\n", option.key));
    }
    if manifest.options.is_empty() {
        code.push_str("    uint none;\n");
    }
    code.push_str("};\n\n");
    for option in &manifest.options {
        if let OptionKind::Choice { choices, .. } = &option.kind {
            for (at, choice) in choices.iter().enumerate() {
                code.push_str(&format!("static const uint {} = {at};\n", choice_constant(&option.key, choice)));
            }
        }
    }
    code.push_str("\n// The pack's options as whoever plays has set them.\nOptions options()\n{\n    Options o;\n");
    if manifest.options.is_empty() {
        code.push_str("    o.none = 0;\n");
    }
    for (at, option) in manifest.options.iter().enumerate() {
        let read = format!("frame()->options[{}].{}", at / 4, ["x", "y", "z", "w"][at % 4]);
        let value = match option.kind {
            OptionKind::Number { .. } => read,
            OptionKind::Toggle { .. } => format!("{read} >= 0.5"),
            OptionKind::Choice { .. } => format!("uint({read} + 0.5)"),
            // Linear, from the sRGB its 24 bits hold.
            OptionKind::Color { .. } => format!("packOptionColor({read})"),
        };
        code.push_str(&format!("    o.{} = {value};\n", option.key));
    }
    code.push_str("    return o;\n}\n\n");
    code
}

/// The engine's shader modules a pack's surface programs are compiled
/// into: each module's name, what it is compiled with defined, and the file
/// it then includes, which [`surface_wrapper`] writes.
pub const SURFACE_MODULES: [(&str, &str, &str); 6] = [
    ("blocks", "VOXEAR_PACK_BLOCKS", "project_pack_blocks.slang"),
    ("water", "VOXEAR_PACK_WATER", "project_pack_water.slang"),
    ("sky", "VOXEAR_PACK_SKY", "project_pack_sky.slang"),
    // Models, drawn by vertices, by mesh shaders and diced into pieces.
    ("forward", "VOXEAR_PACK_MODELS", "project_pack_models.slang"),
    ("meshlets", "VOXEAR_PACK_MODELS", "project_pack_models.slang"),
    ("dice", "VOXEAR_PACK_MODELS", "project_pack_models.slang"),
];

impl PackSurfaces {
    /// Whether the pack has a program for the engine's module `module`.
    pub fn compiles_into(&self, module: &str) -> bool {
        match module {
            "blocks" => self.blocks.is_some() || self.leaves.is_some(),
            "water" => self.water.is_some(),
            "sky" => self.sky.is_some(),
            "forward" | "meshlets" | "dice" => self.models.is_some(),
            _ => false,
        }
    }
}

/// The file Voxear compiles into its own shader `module` for a pack's
/// surface programs (`shaders/shading.slang` includes it for block faces
/// as `project_pack_blocks.slang`, `shaders/water.slang` for water as
/// `project_pack_water.slang`, `shaders/sky.slang` for the sky as
/// `project_pack_sky.slang`): the pack's options, each program in a
/// namespace of its own by its file's place as `place` gives it, and the
/// function the engine calls. For block faces, `packBlockColor` lights a
/// face with the leaves' program where it is leaves and there is one, else
/// the blocks' program, else as the engine does; for water,
/// `packWaterColor`; for the sky, `packSkyColor`.
pub fn surface_wrapper(manifest: &PackManifest, module: &str, place: &dyn Fn(&str) -> PathBuf) -> String {
    let included = |program: &str| place(program).to_string_lossy().replace('\\', "/");
    let surfaces = &manifest.surfaces;
    let mut code = String::from("// Made by Voxear for a shader pack's surface programs.\n\n");
    code.push_str("#include \"shader_pack_surface.slang\"\n#include \"project_pack_options.slang\"\n\n");
    let programs = match module {
        "water" => vec![("pack_water", &surfaces.water)],
        "sky" => vec![("pack_sky", &surfaces.sky)],
        "forward" | "meshlets" | "dice" => vec![("pack_models", &surfaces.models)],
        _ => vec![("pack_blocks", &surfaces.blocks), ("pack_leaves", &surfaces.leaves)],
    };
    for (space, program) in programs {
        if let Some(program) = program {
            code.push_str(&format!("namespace {space}\n{{\n#include \"{}\"\n}}\n\n", included(program)));
        }
    }
    let unworn = "    if (frame() == nullptr)\n    {\n        return light.engine;\n    }\n";
    let others = [
        ("water", "packWaterColor", "WaterSurface surface, WaterLighting light", "pack_water"),
        ("sky", "packSkyColor", "SkySurface surface, SkyLighting light", "pack_sky"),
        ("forward", "packModelColor", "ModelSurface surface, ModelLighting light", "pack_models"),
        ("meshlets", "packModelColor", "ModelSurface surface, ModelLighting light", "pack_models"),
        ("dice", "packModelColor", "ModelSurface surface, ModelLighting light", "pack_models"),
    ];
    for (named, function, types, space) in others {
        if module == named {
            code.push_str(&format!("float3 {function}({types})\n{{\n"));
            code.push_str(unworn);
            code.push_str(&format!("    return {space}::run(surface, light);\n}}\n"));
            return code;
        }
    }
    code.push_str("float3 packBlockColor(BlockSurface surface, BlockLighting light)\n{\n");
    code.push_str(unworn);
    if surfaces.leaves.is_some() {
        code.push_str("    if ((surface.kind & BLOCK_LEAVES) != 0)\n    {\n        return pack_leaves::run(surface, light);\n    }\n");
    }
    match surfaces.blocks {
        Some(_) => code.push_str("    return pack_blocks::run(surface, light);\n}\n"),
        None => code.push_str("    return light.engine;\n}\n"),
    }
    code
}

/// The pack's options as its programs compiled into the engine's modules
/// read them, written once for every file that includes it.
pub fn options_file(manifest: &PackManifest) -> String {
    let mut code = String::from("// Made by Voxear: a shader pack's options.\n\n#pragma once\n\n");
    code.push_str("#include \"shader_pack_surface.slang\"\n\n");
    code.push_str(&options_code(manifest));
    code
}

/// The file `shaders/shading.slang` includes as `project_pack_shadows.slang`
/// for a pack's shadow filter: the program in a namespace of its own, by
/// its file's place as `place` gives it, and `packSunShadow`, which the
/// engine's `sunShadow` calls.
pub fn shadow_wrapper(manifest: &PackManifest, place: &dyn Fn(&str) -> PathBuf) -> String {
    let mut code = String::from("// Made by Voxear for a shader pack's shadow filter.\n\n");
    code.push_str("#include \"shader_pack_surface.slang\"\n#include \"project_pack_options.slang\"\n\n");
    if let Some(filter) = &manifest.shadows.filter {
        let included = place(filter).to_string_lossy().replace('\\', "/");
        code.push_str(&format!("namespace pack_shadows\n{{\n#include \"{included}\"\n}}\n\n"));
    }
    code.push_str("float packSunShadow(ShadowSurface surface, ShadowLighting light)\n{\n");
    code.push_str("    if (frame() == nullptr)\n    {\n        return light.engine;\n    }\n");
    match manifest.shadows.filter {
        Some(_) => code.push_str("    return pack_shadows::run(surface, light);\n}\n"),
        None => code.push_str("    return light.engine;\n}\n"),
    }
    code
}

/// What the engine's shader `module` is compiled with for the pack: the
/// files of Voxear's making it includes, by name, and what is defined.
fn module_files(
    manifest: &PackManifest,
    module: &str,
    place: &dyn Fn(&str) -> PathBuf,
) -> (Vec<(String, String)>, Vec<&'static str>) {
    let mut files = vec![("project_pack_options.slang".to_string(), options_file(manifest))];
    let mut defines = Vec::new();
    for (named, define, file) in &SURFACE_MODULES {
        if *named == module && manifest.surfaces.compiles_into(module) {
            files.push((file.to_string(), surface_wrapper(manifest, module, place)));
            defines.push(*define);
        }
    }
    if manifest.shadows.filter.is_some() && SHADOW_MODULES.contains(&module) {
        files.push(("project_pack_shadows.slang".to_string(), shadow_wrapper(manifest, place)));
        defines.push("VOXEAR_PACK_SHADOWS");
    }
    (files, defines)
}

/// Where a pack's programs are read from.
#[derive(Clone, Debug)]
pub enum PackFiles {
    /// Its folder, which its programs may include other files of.
    Folder(PathBuf),
    /// Files of the engine's own, by name.
    Embedded(&'static [(&'static str, &'static str)]),
    /// Files read already, by name, as a built game's are from its bundles.
    Files(Arc<[(String, String)]>),
}

impl PackFiles {
    /// The text of one of its files.
    pub fn read(&self, name: &str) -> Result<String> {
        match self {
            Self::Folder(folder) => {
                let path = folder.join(name);
                std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
            }
            Self::Embedded(files) => match files.iter().find(|(file, _)| *file == name) {
                Some((_, text)) => Ok(text.to_string()),
                None => bail!("the pack has no file {name}"),
            },
            Self::Files(files) => match files.iter().find(|(file, _)| file == name) {
                Some((_, text)) => Ok(text.clone()),
                None => bail!("the pack has no file {name}"),
            },
        }
    }

    /// Its folder, for one read from a folder.
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Self::Folder(folder) => Some(folder),
            _ => None,
        }
    }
}

/// A pack read and compiled, as the renderer draws it.
#[derive(Debug)]
pub struct CompiledPack {
    pub manifest: PackManifest,
    /// Each pass's SPIR-V, in the order they run.
    pub passes: Vec<Arc<[u8]>>,
    /// The engine's own shader modules compiled again with the pack's
    /// surface programs, by name (`blocks`), drawn in place of its own.
    pub modules: Vec<(String, Arc<[u8]>)>,
    /// Which compile this is, so the renderer makes its pipelines again
    /// when a pack is worn or compiled again.
    pub generation: u64,
}

static GENERATIONS: AtomicU64 = AtomicU64::new(1);

/// Each pass's SPIR-V compiled before, by a hash of everything it was made
/// from, so a pack worn again compiles nothing.
fn compiled_before() -> &'static Mutex<HashMap<u64, Arc<[u8]>>> {
    static COMPILED: OnceLock<Mutex<HashMap<u64, Arc<[u8]>>>> = OnceLock::new();
    COMPILED.get_or_init(Mutex::default)
}

/// Where compiled programs are kept on disk, if anywhere: each by the hash
/// of everything it was made from, so a pack worn in an earlier run of the
/// game compiles nothing.
fn cache_folder() -> &'static Mutex<Option<PathBuf>> {
    static FOLDER: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    FOLDER.get_or_init(Mutex::default)
}

/// The most the compile cache on disk keeps; past it, what was used least
/// lately goes first.
pub const CACHE_LIMIT: u64 = 256 * 1024 * 1024;

/// Keeps compiled programs in `folder` from now on, a pack's compile
/// reading them from there before compiling; `None` keeps them for this run
/// alone. A game keeps them in its `ShaderPacks` folder, beside its saves;
/// the editor in the project's own folder.
pub fn keep_compiles_in(folder: Option<PathBuf>) {
    *cache_folder().lock().expect("not poisoned") = folder;
}

/// Folders of compiled programs read before compiling, as a game reads what
/// its build compiled of its packs (`ShaderPacks` beside its `Shaders`).
fn read_folders() -> &'static Mutex<Vec<PathBuf>> {
    static FOLDERS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    FOLDERS.get_or_init(Mutex::default)
}

/// Reads compiled programs from `folders` too, before compiling them: what
/// a game's build compiled of its packs ([`compile_into`]). Nothing is
/// written there.
pub fn read_compiles_from(folders: Vec<PathBuf>) {
    *read_folders().lock().expect("not poisoned") = folders;
}

/// A program kept in the cache on disk, or compiled by the game's build, if
/// it is there.
fn cached(hash: u64) -> Option<Arc<[u8]>> {
    let built = read_folders().lock().expect("not poisoned").clone();
    let found = built.iter().find_map(|folder| {
        let path = folder.join(format!("{hash:016x}.spv"));
        let spirv = std::fs::read(path).ok()?;
        whole(&spirv).then(|| Arc::from(spirv))
    });
    if found.is_some() {
        return found;
    }
    let folder = cache_folder().lock().expect("not poisoned").clone()?;
    cached_in(&folder, hash)
}

/// Whether `spirv` is a whole SPIR-V program, not one cut short by a crash:
/// whole words, starting with its magic number.
fn whole(spirv: &[u8]) -> bool {
    spirv.len() >= 20 && spirv.len() % 4 == 0 && spirv[..4] == [0x03, 0x02, 0x23, 0x07]
}

/// A program kept in the cache in `folder`, its file touched so it is the
/// last to go.
fn cached_in(folder: &Path, hash: u64) -> Option<Arc<[u8]>> {
    let path = folder.join(format!("{hash:016x}.spv"));
    let spirv = std::fs::read(&path).ok()?;
    if !whole(&spirv) {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    if let Ok(file) = std::fs::File::options().append(true).open(&path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    Some(spirv.into())
}

/// Keeps a program in the cache on disk, if there is one.
fn keep(hash: u64, spirv: &[u8]) {
    if let Some(folder) = cache_folder().lock().expect("not poisoned").clone() {
        keep_in(&folder, hash, spirv, CACHE_LIMIT);
    }
}

/// Keeps a program in the cache in `folder`, written whole or not at all,
/// and the cache within `limit` bytes.
fn keep_in(folder: &Path, hash: u64, spirv: &[u8], limit: u64) {
    if std::fs::create_dir_all(folder).is_err() {
        return;
    }
    let path = folder.join(format!("{hash:016x}.spv"));
    let part = folder.join(format!("{hash:016x}.{}.part", GENERATIONS.fetch_add(1, Ordering::Relaxed)));
    if std::fs::write(&part, spirv).is_err() || std::fs::rename(&part, &path).is_err() {
        let _ = std::fs::remove_file(&part);
        return;
    }
    trim_cache(folder, limit);
}

/// Takes the programs used least lately out of the cache in `folder` until
/// what is left fits in `limit` bytes.
fn trim_cache(folder: &Path, limit: u64) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            path.extension().is_some_and(|e| e == "spv").then_some(())?;
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), path))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort_by_key(|(when, ..)| *when);
    for (_, size, path) in files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

/// One `slangc` run of a pack's compile: what it is called in an error, the
/// files it writes beside its source (that first), the engine's shader it
/// compiles in place of that source, if any, and what it defines.
struct Job<'a> {
    label: String,
    files: Vec<(String, String)>,
    engine_module: Option<&'a str>,
    defines: Vec<&'static str>,
}

/// Compiles a pack's programs, one `slangc` a pass and one for the engine's
/// block shader with its surface programs, all at once: what takes a few
/// seconds the first time and nothing after. An error names the pass or the
/// program, and Slang's message names the file and the line.
pub fn compile(manifest: &PackManifest, files: &PackFiles) -> Result<CompiledPack> {
    compile_with(manifest, files, None)
}

/// As [`compile`], each program kept in `keep` too, where it is read from
/// before it is compiled: as a game's build compiles its packs, for the game
/// to read as it runs (see [`read_compiles_from`]).
pub fn compile_into(manifest: &PackManifest, files: &PackFiles, keep: &Path) -> Result<CompiledPack> {
    compile_with(manifest, files, Some(keep))
}

fn compile_with(manifest: &PackManifest, files: &PackFiles, keep: Option<&Path>) -> Result<CompiledPack> {
    let engine =
        crate::hot_reload::engine_shader_dir().context("the engine's shader sources are not installed beside it")?;
    // Everything a program's SPIR-V is made from, for the hash it is kept by: the engine's shaders and the pack's,
    // by their names, not where they are, so a game built on one machine reads on another what its build compiled.
    let named = |path: &Path| path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(&engine).with_context(|| format!("reading {}", engine.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "slang") {
            sources.push(format!("{}\n{}", named(&path), std::fs::read_to_string(&path)?));
        }
    }
    let mut written: Vec<(String, String)> = Vec::new();
    match files {
        PackFiles::Folder(_) => {}
        PackFiles::Embedded(list) => {
            written.extend(list.iter().map(|(name, text)| (name.to_string(), text.to_string())));
        }
        PackFiles::Files(list) => {
            for (name, text) in list.iter() {
                check_program(name)?;
                written.push((name.clone(), text.clone()));
            }
        }
    }
    // The pack's programs and what they include of its own, wherever they are read from, so a game's build and the
    // game hash it alike.
    let programs = program_files(manifest, &mut |name| files.read(name).ok());
    sources.extend(programs.iter().map(|(name, text)| format!("{name}\n{text}")));
    sources.sort();
    let sources = sources.join("\n");
    let work = std::env::temp_dir().join(format!("voxear-shader-packs-{}", std::process::id()));
    let mut jobs: Vec<Job> = manifest
        .passes
        .iter()
        .enumerate()
        .map(|(at, pass)| Job {
            label: format!("the pass `{}`", pass.name),
            files: vec![(format!("voxear_pass_{at}.slang"), wrapper(manifest, pass))],
            engine_module: None,
            defines: Vec::new(),
        })
        .collect();
    for module in manifest.engine_modules() {
        // Named by the programs' own names here, for the hash; by where they are once written, below.
        let (files, defines) = module_files(manifest, module, &|program| PathBuf::from(program));
        let label = format!("its programs compiled into {module}");
        jobs.push(Job { label, files, engine_module: Some(module), defines });
    }
    // A few at once: the engine's modules are large, and each compile takes a core and a lot of memory.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let cores = std::thread::available_parallelism().map_or(4, |cores| cores.get());
    let workers = (cores / 2).clamp(2, 8).min(jobs.len());
    let mut results: Vec<Option<Result<Arc<[u8]>>>> = (0..jobs.len()).map(|_| None).collect();
    let finished: Vec<Vec<(usize, Result<Arc<[u8]>>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let (work, engine, written, sources) = (&work, &engine, &written, &sources);
                let (jobs, next) = (&jobs, &next);
                scope.spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(at) else { break };
                        let places = (work.as_path(), engine.as_path(), written.as_slice(), sources.as_str(), keep);
                        done.push((at, compile_job(job, (manifest, files), places)));
                    }
                    done
                })
            })
            .collect();
        handles.into_iter().map(|worker| worker.join().unwrap_or_default()).collect()
    });
    for (at, result) in finished.into_iter().flatten() {
        results[at] = Some(result);
    }
    let results: Vec<Result<Arc<[u8]>>> = results
        .into_iter()
        .map(|result| result.unwrap_or_else(|| bail!("the shader compiler thread panicked")))
        .collect();
    let mut compiled = results.into_iter().collect::<Result<Vec<_>>>()?;
    // The passes' first, then the engine's modules.
    let modules: Vec<(String, Arc<[u8]>)> = jobs[manifest.passes.len()..]
        .iter()
        .zip(compiled.split_off(manifest.passes.len()))
        .filter_map(|(job, spirv)| job.engine_module.map(|module| (module.to_string(), spirv)))
        .collect();
    let generation = GENERATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(CompiledPack { manifest: manifest.clone(), passes: compiled, modules, generation })
}

/// One of a pack's compiles: a pass, or one of the engine's modules with
/// the pack's programs in it; kept by a hash of all it is made from.
fn compile_job(
    job: &Job<'_>,
    (manifest, files): (&PackManifest, &PackFiles),
    (work, engine, written, sources, keep): (&Path, &Path, &[(String, String)], &str, Option<&Path>),
) -> Result<Arc<[u8]>> {
    let named: Vec<String> = job.files.iter().map(|(name, text)| format!("{name}\n{text}")).collect();
    let module = job.engine_module.unwrap_or_default();
    let hash = stable_hash(&[&named.join("\n"), module, &job.defines.join(" "), sources]);
    if let Some(spirv) = compiled_before().lock().expect("not poisoned").get(&hash) {
        return Ok(spirv.clone());
    }
    if let Some(spirv) = keep.and_then(|folder| cached_in(folder, hash)).or_else(|| cached(hash)) {
        compiled_before().lock().expect("not poisoned").insert(hash, spirv.clone());
        if let Some(folder) = keep {
            keep_in(folder, hash, &spirv, u64::MAX);
        }
        return Ok(spirv);
    }
    // A folder of its own, as the same pack may compile on two threads at once.
    let dir = work.join(format!("{hash:016x}-{}", GENERATIONS.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir)?;
    for (name, text) in written {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    let folder = files.folder().map_or_else(|| dir.clone(), Path::to_path_buf);
    let made = match job.engine_module {
        // The files of Voxear's making name the pack's programs where they are.
        Some(module) => module_files(manifest, module, &|program| folder.join(program)).0,
        None => job.files.clone(),
    };
    for (name, text) in &made {
        std::fs::write(dir.join(name), text)?;
    }
    let source = match job.engine_module {
        Some(module) => engine.join(format!("{module}.slang")),
        None => dir.join(&job.files[0].0),
    };
    let spirv = compile_program(&source, &[&dir, &folder, engine], &job.defines, &dir.join("out.spv"))
        .with_context(|| job.label.clone())?;
    let spirv: Arc<[u8]> = spirv.into();
    compiled_before().lock().expect("not poisoned").insert(hash, spirv.clone());
    match keep {
        Some(folder) => keep_in(folder, hash, &spirv, u64::MAX),
        None => self::keep(hash, &spirv),
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(spirv)
}

/// One program compiled to SPIR-V by `slangc`, its error with a word on what
/// a pass forgot to read where that is what it is.
fn compile_program(source: &Path, includes: &[&Path], defines: &[&str], target: &Path) -> Result<Vec<u8>> {
    let mut command = Command::new(crate::hot_reload::slangc());
    command.arg(source).args(crate::hot_reload::SLANG_ARGS);
    for define in defines {
        command.arg("-D").arg(define);
    }
    for include in includes {
        command.arg("-I").arg(include);
    }
    let output = command.arg("-o").arg(target).output().context("running the Slang compiler")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let hint = ENGINE_READS.iter().find_map(|(name, _, functions)| {
            functions
                .iter()
                .any(|function| message.contains(&format!("undefined identifier '{function}'")))
                .then(|| format!("\n(a pass that reads the frame's {name} lists `{name}` in its `reads`)"))
        });
        bail!("{message}{}", hint.unwrap_or_default());
    }
    Ok(std::fs::read(target)?)
}

/// A hash that is the same from run to run.
fn stable_hash(parts: &[&str]) -> u64 {
    // FNV-1a.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in part.bytes().chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

/// The leaf light's format: the colour the sun takes through the leaves
/// over a point, and the nearest leaf's depth.
pub const LEAF_LIGHT_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// The leaf light's size, its four cascades laid out as the sun's shadow
/// atlas lays them.
/// (`LEAF_LIGHT_SIZE` in `shaders/shading.slang`).
pub const LEAF_LIGHT_SIZE: u32 = 1024;

/// An image for the leaf light, this frame.
pub(crate) fn leaf_light_image(fg: &mut FrameGraph<'_>, (gpu, bindless): (&Gpu, &mut Bindless)) -> Result<ImageId> {
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let desc = ImageDesc::new(LEAF_LIGHT_SIZE, LEAF_LIGHT_SIZE, LEAF_LIGHT_FORMAT, usage);
    fg.create_image(gpu, bindless, "leaf light", desc)
}

/// **Leaf light**: the leaves of worlds of blocks each of the sun's
/// cascades sees, the colour they let its light through multiplied where
/// they stand one behind another and the nearest's depth, for a shader pack
/// whose `[shadows] leaves` lets some of the sun through them.
pub(crate) fn record_leaf_light(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (crate::pipelines::Pipelines, Push),
    image: ImageId,
    view: crate::blocks::ViewBlocks,
) {
    let reads: Vec<_> = view.pools.iter().map(|&id| (id, crate::graph::BufferAccess::ShaderRead)).collect();
    fg.add_pass("Leaf light", &[(image, ImageAccess::ColorAttachment)], &reads, move |ctx| {
        // Where nothing is drawn, all the light passes, from as far as can be.
        let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [1.0, 1.0, 1.0, 65504.0] } };
        let target = vk::RenderingAttachmentInfo::default()
            .image_view(ctx.view(image))
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear);
        let size = vk::Extent2D { width: LEAF_LIGHT_SIZE, height: LEAF_LIGHT_SIZE };
        crate::passes::begin_rendering(ctx, size, &[target], None);
        crate::passes::bind(ctx, pipelines.block_leaf_light, vk::PipelineBindPoint::GRAPHICS, &push);
        let tile = LEAF_LIGHT_SIZE / 2;
        for cascade in 0..crate::shadows::CASCADES as u32 {
            let corner = vk::Offset2D { x: (cascade % 2 * tile) as i32, y: (cascade / 2 * tile) as i32 };
            crate::passes::set_viewport(ctx, corner, vk::Extent2D { width: tile, height: tile });
            crate::blocks::draw_for_cascade(ctx, &view, &push, cascade);
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// The shader pack the world wears, as a resource: the renderer draws its
/// passes from the next frame.
#[derive(Clone, Debug)]
pub struct WornShaderPack {
    pub pack: Arc<CompiledPack>,
    /// Each option's value, in the manifest's order.
    pub options: [f32; MAX_OPTIONS],
    /// The level's clock: the time of day, 0 to 24, and the day it is.
    pub time_of_day: f32,
    pub day: u32,
}

impl WornShaderPack {
    pub fn new(pack: Arc<CompiledPack>) -> Self {
        let options = pack.manifest.default_values();
        Self { pack, options, time_of_day: 12.0, day: 0 }
    }
}

/// The frame as a pack's programs see it; mirrors `PackFrame` in
/// `shaders/shader_pack.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuPackFrame {
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
    pub view_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub last_view_projection: [[f32; 4]; 4],
    pub camera_position: [f32; 3],
    pub time: f32,
    pub camera_forward: [f32; 3],
    pub near: f32,
    pub jitter: [f32; 2],
    pub drawn_size: [u32; 2],
    pub shown_size: [u32; 2],
    pub frame_index: u32,
    pub day: u32,
    pub sun_direction: [f32; 3],
    pub sun_strength: f32,
    pub sun_color: [f32; 3],
    pub time_of_day: f32,
    pub sun_light: [f32; 3],
    pub night: f32,
    pub cloud: f32,
    pub rain: f32,
    pub snow: f32,
    pub fog: f32,
    pub wind: f32,
    pub lightning: f32,
    pub wetness: f32,
    pub snow_cover: f32,
    pub wind_direction: [f32; 2],
    pub exposure: f32,
    pub bloom_scale: f32,
    pub options: [f32; MAX_OPTIONS],
    pub depth_texture: u32,
    pub surface_texture: u32,
    pub motion_texture: u32,
    pub last_color_texture: u32,
    pub targets: [u32; MAX_TARGETS],
    pub last_targets: [u32; MAX_TARGETS],
    /// What each pixel's surface is, for a pass that reads it.
    pub surface_id_texture: u32,
    pub pad2: u32,
    pub sky_light: u64,
    pub light_window: u64,
    pub light_bytes: u64,
    pub display_peak: f32,
    /// The leaf light's texture, `NO_TEXTURE` without one.
    pub leaf_light: u32,
    /// Where the sun's shadow maps are described ([`GpuPackShadows`]), for
    /// a pass that reads `shadow`; 0 for none.
    pub sun_shadows: u64,
    /// For an orthographic camera, half the height it sees and how far past
    /// its near plane, as the engine's frame has them; 0 for a perspective one.
    pub ortho: f32,
    pub ortho_depth: f32,
}

/// Where in the pack's frame buffer the sun's shadow maps are written,
/// after the frame.
pub(crate) const SHADOWS_OFFSET: u64 = size_of::<GpuPackFrame>().next_multiple_of(16) as u64;

/// The sun's shadow maps as a pass that reads `shadow` reads them; mirrors
/// `PackSunShadows` in `shaders/shader_pack_frame.slang`. Written beside
/// the pack's frame only when a pass reads them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuPackShadows {
    pub right: [f32; 3],
    pub finest_texel: f32,
    pub up: [f32; 3],
    pub depth_min: f32,
    pub direction: [f32; 3],
    pub depth_range: f32,
    pub table: u64,
    pub pool: u32,
    pub moving_pool: u32,
    pub atlas: u32,
    pub cascade_texels: u32,
    pub bias: f32,
    pub lod_scale: f32,
    pub pad: [u32; 2],
    pub origins: [[i32; 4]; crate::vsm::LEVELS],
    pub cascade_view_projection: [[[f32; 4]; 4]; crate::shadows::CASCADES],
    pub cascade_far: [f32; 4],
    pub cascade_texel: [f32; 4],
}

impl GpuPackShadows {
    /// The sun's shadow maps of the engine's frame, its cascades' tiles
    /// `cascade_texels` across; `None` where it draws neither its pages nor
    /// its cascades.
    pub fn of(frame: &crate::gpu::GpuFrame, cascade_texels: u32) -> Option<Self> {
        let vsm = &frame.vsm;
        if vsm.pool == NO_TEXTURE && frame.shadow_texture == NO_TEXTURE {
            return None;
        }
        Some(Self {
            right: vsm.right,
            finest_texel: vsm.finest_texel,
            up: vsm.up,
            depth_min: vsm.depth_min,
            direction: vsm.direction,
            depth_range: vsm.depth_range,
            table: vsm.table,
            pool: vsm.pool,
            moving_pool: vsm.moving_pool,
            atlas: if vsm.pool == NO_TEXTURE { frame.shadow_texture } else { NO_TEXTURE },
            cascade_texels,
            bias: vsm.bias,
            lod_scale: frame.lod_scale,
            pad: [0; 2],
            origins: vsm.origins,
            cascade_view_projection: frame.cascade_view_proj,
            cascade_far: frame.cascade_far,
            cascade_texel: frame.cascade_texel,
        })
    }
}

/// What a view's frame gives a pack's frame beyond the engine's own.
pub(crate) struct FrameFacts {
    /// The sky's light's own colour, and how strong.
    pub sun_own: [f32; 4],
    pub weather: Option<crate::weather::Weather>,
    /// What the scene's light is multiplied by to show it, as last frame's
    /// exposure left it.
    pub exposure: f32,
    pub drawn: vk::Extent2D,
    pub shown: vk::Extent2D,
    /// The sun's and sky's light under the atmosphere, and the block light
    /// window and its bytes; 0 for none.
    pub addresses: [u64; 3],
}

impl GpuPackFrame {
    /// The frame as a pack reads it, from the engine's own; its images are
    /// named by [`PackRecorder::prepare`].
    pub fn of(frame: &crate::gpu::GpuFrame, facts: FrameFacts) -> Self {
        use voxear_core::math::{Mat4, Vec3};
        let view = Mat4::from_cols_array_2d(&frame.view);
        let projection = Mat4::from_cols_array_2d(&frame.view_proj) * view.inverse();
        let way = -Vec3::from(frame.sun_direction);
        let way = if way.length_squared() > 1e-8 { way.normalize() } else { Vec3::Y };
        let weather = facts.weather.unwrap_or_default();
        let (now, ground) = (weather.now(), weather.ground());
        Self {
            view: frame.view,
            projection: projection.to_cols_array_2d(),
            view_projection: frame.view_proj,
            inverse_view_projection: frame.inv_view_proj,
            last_view_projection: frame.prev_view_proj,
            camera_position: frame.camera_position,
            time: frame.time,
            camera_forward: frame.camera_forward,
            near: frame.near,
            jitter: frame.jitter,
            drawn_size: [facts.drawn.width, facts.drawn.height],
            shown_size: [facts.shown.width, facts.shown.height],
            frame_index: frame.frame_index,
            sun_direction: way.to_array(),
            sun_strength: facts.sun_own[3],
            sun_color: [facts.sun_own[0], facts.sun_own[1], facts.sun_own[2]],
            sun_light: frame.sun_color,
            night: frame.lighting.moon as f32,
            cloud: now.cloud,
            rain: now.rain,
            snow: now.snow,
            fog: now.fog,
            wind: now.wind,
            lightning: weather.flash(),
            wetness: ground.wet,
            snow_cover: ground.snow,
            wind_direction: frame.wind.direction,
            exposure: facts.exposure,
            display_peak: frame.post.display_peak.max(1.0),
            leaf_light: NO_TEXTURE,
            sun_shadows: 0,
            ortho: frame.ortho,
            ortho_depth: frame.ortho_depth,
            sky_light: facts.addresses[0],
            light_window: facts.addresses[1],
            light_bytes: facts.addresses[2],
            ..Self::default()
        }
    }
}

impl GpuPackFrame {
    /// The frame with the worn pack's options and the level's clock, and
    /// no images: as a view through a Portal gives its surface programs.
    pub fn worn(frame: &crate::gpu::GpuFrame, facts: FrameFacts, worn: &WornShaderPack) -> Self {
        Self {
            options: worn.options,
            time_of_day: worn.time_of_day,
            day: worn.day,
            depth_texture: NO_TEXTURE,
            surface_texture: NO_TEXTURE,
            surface_id_texture: NO_TEXTURE,
            motion_texture: NO_TEXTURE,
            last_color_texture: NO_TEXTURE,
            targets: [NO_TEXTURE; MAX_TARGETS],
            last_targets: [NO_TEXTURE; MAX_TARGETS],
            ..Self::of(frame, facts)
        }
    }
}

/// A pack's pipelines, one a pass, made from its SPIR-V when it is worn.
pub(crate) struct PackPipelines {
    pub generation: u64,
    pub pipelines: Vec<vk::Pipeline>,
}

impl PackPipelines {
    /// Each pass's pipeline, into the format of what it writes.
    pub fn new(gpu: &Gpu, bindless: &Bindless, pack: &CompiledPack) -> Result<Self> {
        let mut pipelines = Vec::new();
        for (pass, spirv) in pack.manifest.passes.iter().zip(&pack.passes) {
            let made = (|| {
                let module = voxear_rhi::ShaderModule::new(gpu, &format!("{}.slang", pass.name), spirv)?;
                let format = match pass.writes {
                    PassWrites::Color if pass.point >= PackPoint::Tonemap => LDR_FORMAT,
                    PassWrites::Color => HDR_FORMAT,
                    PassWrites::Target(target) => pack.manifest.targets[target].format.vk(),
                };
                let desc = voxear_rhi::GraphicsPipelineDesc {
                    color_formats: &[format],
                    cull: vk::CullModeFlags::NONE,
                    ..voxear_rhi::GraphicsPipelineDesc::new(&pass.name, &module, c"packVertex", c"packFragment")
                };
                let pipeline = voxear_rhi::create_graphics_pipeline(gpu, bindless.pipeline_layout, &desc);
                module.destroy(gpu);
                pipeline
            })();
            match made {
                Ok(pipeline) => pipelines.push(pipeline),
                Err(error) => {
                    Self { generation: pack.generation, pipelines }.destroy(gpu);
                    return Err(error.context(format!("making the pass `{}`", pass.name)));
                }
            }
        }
        Ok(Self { generation: pack.generation, pipelines })
    }

    /// The caller makes sure the GPU is done with them.
    pub fn destroy(self, gpu: &Gpu) {
        for pipeline in self.pipelines {
            unsafe { gpu.device.destroy_pipeline(pipeline, None) };
        }
    }
}

/// A worn pack as one frame draws it: its passes and their pipelines.
#[derive(Clone)]
pub(crate) struct PackDraw {
    pub pack: Arc<CompiledPack>,
    pub pipelines: Arc<[vk::Pipeline]>,
    pub worn: WornShaderPack,
}

impl PackDraw {
    /// Whether a pass reads each pixel's normal, which the depth passes
    /// write only when something asks.
    pub fn reads_normals(&self) -> bool {
        self.pack.manifest.passes.iter().any(|pass| pass.reads.normals)
    }

    /// Whether a pass reads what each pixel's surface is, which the depth
    /// passes write only when something asks.
    pub fn reads_surface_ids(&self) -> bool {
        self.pack.manifest.passes.iter().any(|pass| pass.reads.surface_ids)
    }

    /// Whether a pass reads the sun's shadow, which its frame then points
    /// at.
    pub fn reads_shadow(&self) -> bool {
        self.pack.manifest.passes.iter().any(|pass| pass.reads.shadow)
    }
}

/// The images a view's frame gives a pack, and the sizes it draws at.
pub(crate) struct PackImages {
    pub depth: ImageId,
    pub motion: ImageId,
    pub surface: Option<ImageId>,
    pub surface_ids: Option<ImageId>,
    /// The sun's shadow maps this frame: its pages' pools, or its cascades'
    /// atlas; read by a pass that reads `shadow`.
    pub sun_shadows: Vec<ImageId>,
    /// The size the scene is drawn at, the size after TAA, and the size the
    /// picture is shown at.
    pub drawn: vk::Extent2D,
    pub resolved: vk::Extent2D,
    pub shown: vk::Extent2D,
    /// Which of a pair of kept images this frame writes.
    pub frame_index: u64,
}

/// Records a pack's passes at each point of a view's frame, its images all
/// made up front ([`PackRecorder::prepare`]), so the passes need nothing
/// more of the renderer as they are recorded.
#[derive(Clone)]
pub(crate) struct PackRecorder {
    draw: PackDraw,
    push: Push,
    depth: ImageId,
    motion: ImageId,
    surface: Option<ImageId>,
    surface_ids: Option<ImageId>,
    sun_shadows: Vec<ImageId>,
    /// Each target this frame, and last frame's for those it keeps.
    targets: Vec<(ImageId, Option<ImageId>)>,
    /// Last frame's colour after TAA, and this frame's to copy it into.
    history: Option<(ImageId, Option<ImageId>)>,
    /// What a pass writing the colour in place reads it from.
    copy: Option<ImageId>,
    /// Two images each the passes after TAA and the final ones write the
    /// colour into, one after the other.
    resolved: Option<[ImageId; 2]>,
    finals: Option<[ImageId; 2]>,
}

impl PackRecorder {
    /// Makes every image the pack's passes use this frame, and its frame's
    /// fields that name them.
    pub fn prepare(
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        draw: PackDraw,
        images: &PackImages,
        frame: &mut GpuPackFrame,
    ) -> Result<Self> {
        let manifest = &draw.pack.manifest;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let sized = |extent: vk::Extent2D, format| ImageDesc::new(extent.width, extent.height, format, usage);
        let size_at = |point: PackPoint| match point {
            PackPoint::AfterLighting | PackPoint::AfterFog => images.drawn,
            PackPoint::AfterTaa => images.resolved,
            PackPoint::Tonemap | PackPoint::Final => images.shown,
        };
        let which = (images.frame_index % 2) as usize;
        let mut targets = Vec::new();
        for target in &manifest.targets {
            let desc = sized(target.scale.of_extent(size_at(target.point)), target.format.vk());
            let made = match target.kept {
                true => {
                    let named = |at: usize| format!("pack target {} {at}", target.name);
                    let current = fg.history_image(gpu, bindless, &named(which), desc)?;
                    let last = fg.history_image(gpu, bindless, &named(1 - which), desc)?;
                    (current, (!fg.is_new(last)).then_some(last))
                }
                false => (fg.create_image(gpu, bindless, &format!("pack target {}", target.name), desc)?, None),
            };
            targets.push(made);
        }
        let reads_history = manifest.passes.iter().any(|pass| pass.reads.history);
        let history = match reads_history {
            true => {
                let desc = ImageDesc {
                    usage: usage | vk::ImageUsageFlags::TRANSFER_DST,
                    ..sized(images.resolved, HDR_FORMAT)
                };
                let current = fg.history_image(gpu, bindless, &format!("pack history {which}"), desc)?;
                let last = fg.history_image(gpu, bindless, &format!("pack history {}", 1 - which), desc)?;
                Some((current, (!fg.is_new(last)).then_some(last)))
            }
            false => None,
        };
        let writes_color = |point: PackPoint| {
            manifest.passes.iter().any(|pass| pass.point == point && pass.writes == PassWrites::Color)
        };
        let copy = match writes_color(PackPoint::AfterLighting) || writes_color(PackPoint::AfterFog) {
            true => {
                let desc =
                    ImageDesc { usage: usage | vk::ImageUsageFlags::TRANSFER_DST, ..sized(images.drawn, HDR_FORMAT) };
                Some(fg.create_image(gpu, bindless, "pack colour copy", desc)?)
            }
            false => None,
        };
        let pair =
            |fg: &mut FrameGraph<'_>, bindless: &mut Bindless, name: &str, desc: ImageDesc| -> Result<[ImageId; 2]> {
                Ok([
                    fg.create_image(gpu, bindless, &format!("{name} 0"), desc)?,
                    fg.create_image(gpu, bindless, &format!("{name} 1"), desc)?,
                ])
            };
        let resolved = match writes_color(PackPoint::AfterTaa) {
            true => Some(pair(fg, bindless, "pack after TAA", sized(images.resolved, HDR_FORMAT))?),
            false => None,
        };
        let finals = match writes_color(PackPoint::Final) {
            true => Some(pair(fg, bindless, "pack final", sized(images.shown, LDR_FORMAT))?),
            false => None,
        };
        let texture =
            |fg: &FrameGraph<'_>, image: Option<ImageId>| image.map_or(NO_TEXTURE, |image| fg.texture(image).0);
        let reads = |read: fn(&PassReads) -> bool| manifest.passes.iter().any(|pass| read(&pass.reads));
        frame.depth_texture = if reads(|r| r.depth) { fg.texture(images.depth).0 } else { NO_TEXTURE };
        frame.motion_texture = if reads(|r| r.motion) { fg.texture(images.motion).0 } else { NO_TEXTURE };
        frame.surface_texture = if reads(|r| r.normals) { texture(fg, images.surface) } else { NO_TEXTURE };
        frame.surface_id_texture = if reads(|r| r.surface_ids) { texture(fg, images.surface_ids) } else { NO_TEXTURE };
        frame.last_color_texture = texture(fg, history.and_then(|(_, last)| last));
        frame.targets = [NO_TEXTURE; MAX_TARGETS];
        frame.last_targets = [NO_TEXTURE; MAX_TARGETS];
        for (slot, (current, last)) in targets.iter().enumerate() {
            frame.targets[slot] = fg.texture(*current).0;
            frame.last_targets[slot] = texture(fg, *last);
        }
        frame.options = draw.worn.options;
        frame.time_of_day = draw.worn.time_of_day;
        frame.day = draw.worn.day;
        Ok(Self {
            draw,
            push: Push::default(),
            depth: images.depth,
            motion: images.motion,
            surface: images.surface,
            surface_ids: images.surface_ids,
            sun_shadows: images.sun_shadows.clone(),
            targets,
            history,
            copy,
            resolved,
            finals,
        })
    }

    /// Points the passes at the frame they read, once it has a buffer.
    pub fn set_push(&mut self, push: Push) {
        self.push = push;
    }

    /// Whether any pass runs at `point`.
    pub fn runs_at(&self, point: PackPoint) -> bool {
        self.draw.pack.manifest.runs_at(point)
    }

    /// Whether a pass at `point` writes the colour: at the tonemap, in place
    /// of the engine's.
    pub fn writes_color_at(&self, point: PackPoint) -> bool {
        let manifest = &self.draw.pack.manifest;
        manifest.passes.iter().any(|pass| pass.point == point && pass.writes == PassWrites::Color)
    }

    /// Records the pack's passes at `point` over `color`, the frame's colour
    /// there; returns the colour after them, which at a point whose colour
    /// is written in place is `color` itself. `bloom` is the engine's glow,
    /// from the tonemap on; at the tonemap, `shown` is the image a pass that
    /// writes the colour writes the picture into.
    pub fn record(
        &self,
        fg: &mut FrameGraph<'_>,
        point: PackPoint,
        color: ImageId,
        (bloom, shown): (Option<ImageId>, Option<ImageId>),
    ) -> ImageId {
        if point == PackPoint::AfterTaa
            && let Some((current, _)) = self.history
        {
            copy_image(fg, "Pack history", color, current);
        }
        let manifest = &self.draw.pack.manifest;
        let mut color = color;
        let mut written = 0;
        for (index, pass) in manifest.passes.iter().enumerate().filter(|(_, pass)| pass.point == point) {
            let pipeline = self.draw.pipelines[index];
            // What the pass reads the colour from, and what it writes.
            let (source, target) = match pass.writes {
                PassWrites::Target(target) => (color, self.targets[target].0),
                PassWrites::Color if point.in_place() => {
                    let copy = self.copy.expect("made for a pass that writes the colour in place");
                    copy_image(fg, "Pack colour copy", color, copy);
                    (copy, color)
                }
                PassWrites::Color if point == PackPoint::Tonemap => {
                    (color, shown.expect("given the image the picture is written into at the tonemap"))
                }
                PassWrites::Color => {
                    let pair = match point {
                        PackPoint::Final => self.finals,
                        _ => self.resolved,
                    };
                    let next = pair.expect("made for a pass that writes the colour")[written % 2];
                    written += 1;
                    (color, next)
                }
            };
            let mut images = vec![(target, ImageAccess::ColorAttachment)];
            let mut sampled = |image: ImageId| {
                if image != target && images.iter().all(|(seen, _)| *seen != image) {
                    images.push((image, ImageAccess::Sampled));
                }
            };
            if pass.reads.color {
                sampled(source);
            }
            if pass.reads.depth {
                sampled(self.depth);
            }
            if pass.reads.motion {
                sampled(self.motion);
            }
            if pass.reads.normals
                && let Some(surface) = self.surface
            {
                sampled(surface);
            }
            if pass.reads.surface_ids
                && let Some(ids) = self.surface_ids
            {
                sampled(ids);
            }
            if pass.reads.shadow {
                for &map in &self.sun_shadows {
                    sampled(map);
                }
            }
            if pass.reads.history
                && let Some((_, Some(last))) = self.history
            {
                sampled(last);
            }
            if pass.reads.bloom
                && let Some(bloom) = bloom
            {
                sampled(bloom);
            }
            for &read in &pass.reads.targets {
                let (current, last) = self.targets[read];
                if PassWrites::Target(read) != pass.writes {
                    sampled(current);
                }
                if let Some(last) = last {
                    sampled(last);
                }
            }
            let source_texture = if pass.reads.color { fg.texture(source).0 } else { NO_TEXTURE };
            let extent = fg.image_desc(target).extent();
            let bloom_texture = bloom.filter(|_| pass.reads.bloom).map_or(NO_TEXTURE, |bloom| fg.texture(bloom).0);
            let push = Push {
                texture: source_texture,
                max_draws: index as u32,
                size: [extent.width, extent.height],
                textures: [bloom_texture, 0, 0, 0],
                ..self.push
            };
            fg.add_pass(&pass.name, &images, &[], move |ctx| {
                crate::post_passes::draw(ctx, pipeline, &push, target, false);
            });
            if pass.writes == PassWrites::Color && !point.in_place() {
                color = target;
            }
        }
        color
    }
}

/// Copies all of one image into another of the same size and format.
fn copy_image(fg: &mut FrameGraph<'_>, name: &str, from: ImageId, to: ImageId) {
    let images = [(from, ImageAccess::TransferSrc), (to, ImageAccess::TransferDst)];
    fg.add_pass(name, &images, &[], move |ctx| {
        let extent = ctx.extent(from);
        let layers = vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
        let region = vk::ImageCopy::default().src_subresource(layers).dst_subresource(layers).extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        });
        unsafe {
            ctx.gpu.device.cmd_copy_image(
                ctx.cmd,
                ctx.image(from),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ctx.image(to),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = r#"
format = 1
description = "A test."

[options]
strength = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Strength", about = "How strong." }
rays = { kind = "toggle", default = true }
quality = { kind = "choice", choices = ["Low", "High"], default = "High" }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color", "rays"]

[[pass]]
name = "Rays"
program = "rays.slang"
at = "after_fog"
reads = ["depth", "color"]
writes = { target = "rays", format = "rgba16f", scale = 0.5 }
"#;

    #[test]
    fn a_manifest_reads_its_options_in_order_and_its_passes_in_the_order_they_run() {
        let manifest = PackManifest::parse(PACK).unwrap();
        let keys: Vec<&str> = manifest.options.iter().map(|option| option.key.as_str()).collect();
        assert_eq!(keys, ["strength", "rays", "quality"], "as the file lists them");
        assert_eq!(manifest.options[1].label, "rays", "named after its key with no label");
        assert_eq!(manifest.options[0].about, "How strong.");
        assert_eq!(manifest.default_values()[..3], [1.0, 1.0, 1.0]);
        let names: Vec<&str> = manifest.passes.iter().map(|pass| pass.name.as_str()).collect();
        assert_eq!(names, ["Rays", "Grade"], "after fog comes before the final picture");
        assert_eq!(manifest.passes[0].writes, PassWrites::Target(0));
        assert_eq!(manifest.passes[1].reads.targets, [0]);
        assert_eq!(manifest.targets[0].scale, TargetScale::Half);
        assert_eq!(manifest.targets[0].point, PackPoint::AfterFog);
        assert_eq!(manifest.passes[1].reads.names(&manifest), ["color", "rays"]);
        assert!(manifest.runs_at(PackPoint::Final) && !manifest.runs_at(PackPoint::AfterTaa));
    }

    #[test]
    fn an_option_set_is_kept_to_what_it_can_be() {
        let manifest = PackManifest::parse(PACK).unwrap();
        let [strength, rays, quality] = [&manifest.options[0], &manifest.options[1], &manifest.options[2]];
        assert_eq!((strength.kept(5.0), strength.kept(-1.0), strength.kept(0.4)), (2.0, 0.0, 0.4));
        assert_eq!((rays.kept(0.2), rays.kept(0.9)), (0.0, 1.0));
        assert_eq!((quality.kept(7.0), quality.kept(0.3)), (1.0, 0.0));
    }

    #[test]
    fn pixels_follow_a_number_option_or_a_number_and_one_leaves_the_settings_own() {
        let text = "[options]\nsize = { kind = \"number\", range = [1.0, 8.0], default = 4.0 }\n[pixels]\nsize = \"size\"\n";
        let manifest = PackManifest::parse(text).unwrap();
        let pixels = manifest.pixels.unwrap();
        assert_eq!(pixels, PackPixels { size: PackPixelSize::Option(0), snap: true });
        assert_eq!(pixels.settings(&[6.0]).map(|p| (p.size, p.snap)), Some((6, true)));
        assert_eq!(pixels.settings(&[1.0]), None, "a size of 1 leaves the settings' own");
        let fixed = PackManifest::parse("[pixels]\nsize = 3\nsnap = false\n").unwrap().pixels.unwrap();
        assert_eq!(fixed.settings(&[]).map(|p| (p.size, p.snap)), Some((3, false)));
        for (bad, says) in [
            ("[pixels]\nsize = \"none\"\n", "no option `none`"),
            ("[pixels]\nsize = 40\n", "from 1 to 32"),
            ("[options]\non = { kind = \"toggle\", default = true }\n[pixels]\nsize = \"on\"\n", "not a number"),
        ] {
            let error = format!("{:#}", PackManifest::parse(bad).expect_err(bad));
            assert!(error.contains(says), "{bad}: {error}");
        }
    }

    #[test]
    fn a_colour_option_keeps_its_srgb_in_one_number_and_reads_as_linear() {
        let text = "[options]\nshade = { kind = \"color\", default = [0.2, 0.4, 1.0] }\n[pixels]\nsize = 2\n";
        let manifest = PackManifest::parse(text).unwrap();
        let option = &manifest.options[0];
        let value = option.default_value();
        let back = value_color(value);
        for (a, b) in back.iter().zip([0.2, 0.4, 1.0]) {
            assert!((a - b).abs() <= 0.5 / 255.0, "{back:?}");
        }
        assert_eq!(value_color(color_value([1.0, 0.0, 0.5])), [1.0, 0.0, 128.0 / 255.0]);
        assert_eq!(option.kept(2.0e7), 16_777_215.0, "kept to 24 bits");
        assert!(options_code(&manifest).contains("float3 shade;"));
        assert!(options_code(&manifest).contains("o.shade = packOptionColor(frame()->options[0].x);"));
    }

    #[test]
    fn a_manifest_that_cannot_be_drawn_says_why() {
        let pass = "[[pass]]\nname = \"A\"\nprogram = \"a.slang\"\nat = \"final\"\n";
        let fails = |text: &str, says: &str| {
            let error = format!("{:#}", PackManifest::parse(text).expect_err(text));
            assert!(error.contains(says), "{text}: {error}");
        };
        fails("format = 1", "no [[pass]]");
        fails("format = 9", "newer Voxear");
        fails(&format!("{pass}reads = [\"shade\"]"), "neither one of the frame's images");
        fails(&format!("{pass}reads = [\"rays\"]\n{pass}writes = {{ target = \"rays\" }}"), "before the pass");
        fails(&format!("{pass}writes = {{ target = \"rays\" }}\nreads = [\"rays\"]"), "reads the target it writes");
        fails(&format!("{pass}writes = {{ target = \"rays\", scale = 0.3 }}"), "1, 0.5 or 0.25");
        fails(&format!("{pass}writes = {{ target = \"Rays\" }}"), "small letters");
        fails(&format!("{pass}writes = {{ target = \"depth\" }}"), "already have");
        fails(&pass.replace("a.slang", "../a.slang"), "not a file in the pack's folder");
        fails(&pass.replace("a.slang", "a.hlsl"), "not a .slang file");
        fails(&pass.replace("final", "after_sky"), "unknown variant");
        fails("[surface]\nmodels = \"../m.slang\"", "[surface] models");
        fails("[surface]\nsky = \"../s.slang\"", "[surface] sky");
        let tonemap = pass.replace("final", "tonemap");
        let second = tonemap.replace("\"A\"", "\"B\"");
        fails(&format!("{tonemap}{second}"), "two passes write the colour at the tonemap");
        fails(&format!("{}reads = [\"bloom\"]\n", pass.replace("final", "after_taa")), "from the tonemap on");
        fails("[surface]\nblocks = \"../b.slang\"", "not a file in the pack's folder");
        fails("[surface]\nshadows = \"s.slang\"", "unknown field");
        fails("[shadows]\nfilter = \"../s.slang\"", "not a file in the pack's folder");
        fails("[shadows]\nsoftness = 30.0", "0 to 20 degrees");
        fails("[shadows]\ndistance = 2.0", "5 to 1000 metres");
        fails("[shadows]\ntint = [-1.0, 0.0, 0.0]", "tint");
        fails("[shadows]\nmaps = \"many\"", "`virtual` or `cascades`");
        fails("[shadows]\ncascades = 5", "from 1 to 4");
        fails("[shadows]\nsplit = 1.5", "from 0 to 1");
        fails("[shadows]\nblur = 1.0", "unknown field");
        fails(&format!("[options]\nx = {{ kind = \"number\", range = [1.0, 0.0], default = 0.5 }}\n{pass}"), "range");
        fails(
            &format!("[options]\nx = {{ kind = \"choice\", choices = [\"A\"], default = \"B\" }}\n{pass}"),
            "default",
        );
        fails(&format!("[options]\nx = {{ kind = \"toggle\", default = true, size = 2 }}\n{pass}"), "unknown field");
        // A kept target can be read before the pass that writes it, as last frame's, and by that pass itself.
        let kept = format!("{pass}reads = [\"trail\"]\nwrites = {{ target = \"trail\", kept = true }}");
        assert!(PackManifest::parse(&kept).is_ok());
    }

    #[test]
    fn a_wrapper_defines_what_the_pass_reads_and_the_packs_options_and_targets() {
        let manifest = PackManifest::parse(PACK).unwrap();
        let rays = wrapper(&manifest, &manifest.passes[0]);
        assert!(rays.contains("#define PACK_READS_DEPTH 1") && rays.contains("#define PACK_READS_NORMALS 0"));
        assert!(rays.contains("    float strength;\n    bool rays;\n    uint quality;"));
        assert!(rays.contains("o.quality = uint(frame()->options[0].z + 0.5);"));
        assert!(rays.contains("static const uint QUALITY_HIGH = 1;"));
        assert!(rays.contains("static const PackTarget rays = { 0 };"));
        assert!(rays.contains("#include \"rays.slang\"") && rays.contains("return run(packInput(v));"));
        assert_eq!(choice_constant("look", "Hand-drawn 2"), "LOOK_HAND_DRAWN_2");
    }

    #[test]
    fn a_pack_of_surface_programs_alone_lights_leaves_with_its_own_program_and_the_rest_with_the_blocks() {
        let manifest = PackManifest::parse(
            "[options]\nwarm = { kind = \"toggle\", default = true }\n[surface]\nblocks = \"faces.slang\"\nleaves = \"leaves.slang\"\n",
        )
        .unwrap();
        assert!(manifest.passes.is_empty());
        assert_eq!(manifest.surfaces.named(), [("blocks", "faces.slang"), ("leaves", "leaves.slang")]);
        let wrapped = surface_wrapper(&manifest, "blocks", &|program| Path::new("/p").join(program));
        assert!(wrapped.contains("#include \"project_pack_options.slang\""));
        let options = options_file(&manifest);
        assert!(options.contains("#pragma once") && options.contains("    bool warm;"));
        assert!(wrapped.contains("namespace pack_blocks\n{\n#include \"/p/faces.slang\"\n}"));
        assert!(wrapped.contains("return pack_leaves::run(surface, light);"));
        assert!(wrapped.contains("return pack_blocks::run(surface, light);"));
        // Leaves alone: every other face as the engine lights it.
        let leaves = PackManifest::parse("[surface]\nleaves = \"leaves.slang\"\n").unwrap();
        let wrapped = surface_wrapper(&leaves, "blocks", &|program| PathBuf::from(program));
        assert!(wrapped.contains("pack_leaves::run") && wrapped.ends_with("    return light.engine;\n}\n"));
    }

    #[test]
    fn a_packs_shadows_set_the_suns_and_its_filter_is_compiled_into_every_module_that_reads_them() {
        let manifest = PackManifest::parse(
            "[shadows]\nfilter = \"soft.slang\"\nsoftness = 2.0\ndistance = 150.0\ntint = [0.1, 0.1, 0.2]\nmaps = \"cascades\"\n",
        )
        .unwrap();
        let shadows = &manifest.shadows;
        assert_eq!(shadows.filter.as_deref(), Some("soft.slang"));
        assert_eq!((shadows.softness, shadows.distance, shadows.tint), (Some(2.0), Some(150.0), Some([0.1, 0.1, 0.2])));
        assert_eq!(shadows.maps, Some(crate::shadows::SunShadows::Cascades));
        let layout = PackManifest::parse("[shadows]\ncascades = 2\nsplit = 0.4\n").unwrap();
        assert_eq!((layout.shadows.cascades, layout.shadows.split), (Some(2), Some(0.4)));
        assert_eq!(manifest.engine_modules(), SHADOW_MODULES);
        let (files, defines) = module_files(&manifest, "water", &|program| Path::new("/p").join(program));
        assert_eq!(defines, ["VOXEAR_PACK_SHADOWS"]);
        let wrapped = &files.iter().find(|(name, _)| name == "project_pack_shadows.slang").unwrap().1;
        assert!(wrapped.contains("namespace pack_shadows\n{\n#include \"/p/soft.slang\"\n}"));
        assert!(wrapped.contains("return pack_shadows::run(surface, light);"));
        // With water's own program too, one module takes both.
        let both = PackManifest::parse("[surface]\nwater = \"w.slang\"\n[shadows]\nfilter = \"s.slang\"\n").unwrap();
        let (files, defines) = module_files(&both, "water", &|program| PathBuf::from(program));
        assert_eq!(defines, ["VOXEAR_PACK_WATER", "VOXEAR_PACK_SHADOWS"]);
        assert_eq!(files.len(), 3, "the options, water's and the shadows'");
        assert_eq!(both.engine_modules().iter().filter(|module| **module == "water").count(), 1);
        // Settings alone compile nothing.
        let settings = PackManifest::parse("[shadows]\nsoftness = 0.0\nleaves = true\n").unwrap();
        assert!(settings.engine_modules().is_empty() && settings.shadows.leaves);
        assert!(!manifest.shadows.leaves, "leaves let nothing through unless the pack says so");
    }

    #[test]
    fn compiled_programs_are_kept_on_disk_whole_and_the_least_used_go_first_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let program = |words: u32| {
            let mut spirv = vec![0x03, 0x02, 0x23, 0x07];
            spirv.resize(words as usize * 4, 0);
            spirv
        };
        keep_in(dir.path(), 1, &program(100), 1000);
        assert_eq!(cached_in(dir.path(), 1).map(|spirv| spirv.len()), Some(400));
        assert!(cached_in(dir.path(), 2).is_none());
        // One cut short, or not SPIR-V at all, is not read, and goes.
        std::fs::write(dir.path().join(format!("{:016x}.spv", 3u64)), [0x03, 0x02, 0x23]).unwrap();
        assert!(cached_in(dir.path(), 3).is_none() && !dir.path().join(format!("{:016x}.spv", 3u64)).exists());
        // Past the limit, what was used least lately goes: the first, read again, stays.
        std::thread::sleep(std::time::Duration::from_millis(20));
        keep_in(dir.path(), 4, &program(100), 1000);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(cached_in(dir.path(), 1).is_some());
        std::thread::sleep(std::time::Duration::from_millis(20));
        keep_in(dir.path(), 5, &program(100), 1000);
        assert!(cached_in(dir.path(), 1).is_some() && cached_in(dir.path(), 5).is_some());
        assert!(cached_in(dir.path(), 4).is_none(), "the one used least lately went");
        let parts = std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok());
        assert!(parts.into_iter().all(|e| e.path().extension().is_some_and(|x| x == "spv")), "no half-written file");
    }

    #[test]
    fn a_packs_programs_are_read_with_the_files_of_its_own_they_include_as_deep_as_they_go() {
        let manifest = PackManifest::parse(
            "[surface]\nblocks = \"faces.slang\"\n[[pass]]\nname = \"A\"\nprogram = \"a.slang\"\nat = \"final\"\n",
        )
        .unwrap();
        let files = [
            ("a.slang", "#include \"lib/b.slang\"\nfloat4 run(PassInput input) { return 0; }"),
            ("lib/b.slang", "#include \"c.slang\"\n#include \"shader_pack.slang\"\n"),
            ("lib/c.slang", "// last\n"),
            ("faces.slang", "#include \"lib/c.slang\"\n#include \"../outside.slang\"\n"),
            ("unused.slang", "// not included\n"),
        ];
        let mut asked = Vec::new();
        let read = program_files(&manifest, &mut |name| {
            asked.push(name.to_string());
            files.iter().find(|(file, _)| *file == name).map(|(_, text)| text.to_string())
        });
        let names: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["a.slang", "faces.slang", "lib/b.slang", "lib/c.slang"], "each once, nothing outside");
        assert!(asked.contains(&"lib/shader_pack.slang".to_string()), "the engine's own, asked for and left");
        assert!(!asked.iter().any(|name| name.contains("..")), "nothing outside the pack is asked for");
    }

    #[test]
    fn the_frame_a_pack_reads_is_laid_out_as_the_shader_has_it() {
        // Five matrices, then ten rows of sixteen bytes, the options, the images and five pointers.
        assert_eq!(size_of::<GpuPackFrame>(), 5 * 64 + 10 * 16 + 4 * MAX_OPTIONS + 16 + 8 * MAX_TARGETS + 8 + 48);
        // The sun's space, the table, textures and the rest (scalar layout, as the shaders are compiled), each level's
        // window and the cascades.
        assert_eq!(size_of::<GpuPackShadows>(), 48 + 8 + 32 + 16 * crate::vsm::LEVELS + 64 * 4 + 32);
        // As shader_pack_frame.slang names the pages' layout.
        let header = include_str!("../shaders/shader_pack_frame.slang");
        for (name, value) in [
            ("PACK_VSM_LEVELS", crate::vsm::LEVELS as u32),
            ("PACK_VSM_PAGE", crate::vsm::PAGE),
            ("PACK_VSM_BORDER", crate::vsm::BORDER),
            ("PACK_VSM_POOL_SLOTS", crate::vsm::POOL_SLOTS),
            ("PACK_VSM_MOVING_POOL_SLOTS", crate::vsm::MOVING_POOL_SLOTS),
            ("PACK_VSM_WINDOW", crate::vsm::WINDOW as u32),
            ("PACK_CASCADES", crate::shadows::CASCADES as u32),
        ] {
            assert!(header.contains(&format!("{name} = {value};")), "{name} is {value}");
        }
    }

    #[test]
    fn every_built_in_pack_reads_and_says_what_it_is() {
        for (key, title, _) in BUILTIN {
            let manifest = PackManifest::builtin(key).unwrap_or_else(|error| panic!("{key}: {error:#}"));
            assert!(!title.is_empty() && manifest.description.len() > 20, "{key} says what it is");
            let programs = manifest.passes.iter().map(|pass| pass.program.as_str());
            let surfaces = manifest.surfaces.named().into_iter().map(|(_, program)| program);
            for program in programs.chain(surfaces).chain(manifest.shadows.filter.as_deref()) {
                builtin_files(key).unwrap().iter().find(|(name, _)| *name == program).expect("its program");
            }
        }
        assert!(PackManifest::builtin("gloomy").is_err());
    }
}
