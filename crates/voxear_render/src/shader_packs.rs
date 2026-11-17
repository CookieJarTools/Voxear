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
pub const BUILTIN: [(&str, &str, &[(&str, &str)]); 1] = [(
    "evening",
    "Evening",
    &[
        ("pack.toml", include_str!("../shader_packs/evening.shaderpack/pack.toml")),
        ("rays.slang", include_str!("../shader_packs/evening.shaderpack/rays.slang")),
        ("light.slang", include_str!("../shader_packs/evening.shaderpack/light.slang")),
        ("grade.slang", include_str!("../shader_packs/evening.shaderpack/grade.slang")),
    ],
)];

/// Where a built-in pack's files are in a source checkout, to be watched
/// and read again as they change while the engine is worked on.
pub fn builtin_folder(name: &str) -> Option<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("shader_packs").join(format!("{name}.{PACK_EXTENSION}"));
    folder.join(MANIFEST).is_file().then_some(folder)
}

/// One of the four points of the frame a pass runs at.
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
    /// Over the finished picture, after the tonemap, the grade and a look
    /// pack's shader, before sharpening and the UI: its colours are as
    /// shown, from 0 to 1.
    Final,
}

impl PackPoint {
    pub const ALL: [Self; 4] = [Self::AfterLighting, Self::AfterFog, Self::AfterTaa, Self::Final];

    /// As a manifest writes it.
    pub fn key(self) -> &'static str {
        match self {
            Self::AfterLighting => "after_lighting",
            Self::AfterFog => "after_fog",
            Self::AfterTaa => "after_taa",
            Self::Final => "final",
        }
    }

    /// As the Inspector shows it.
    pub fn title(self) -> &'static str {
        match self {
            Self::AfterLighting => "After lighting",
            Self::AfterFog => "After fog",
            Self::AfterTaa => "After TAA",
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
    /// The pack's own targets, by their place among [`PackManifest::targets`].
    pub targets: Vec<usize>,
}

/// The engine's images, as `reads` names them, and which part of
/// [`PassReads`] each is, with the functions a program reads it by.
const ENGINE_READS: [(&str, &str, &[&str]); 6] = [
    ("color", "PACK_READS_COLOR", &["sceneColor", "sceneColorAt", "colorSize"]),
    ("depth", "PACK_READS_DEPTH", &["sceneDepth", "isSky", "sceneDistance", "scenePosition"]),
    ("normals", "PACK_READS_NORMALS", &["sceneNormal", "sceneRoughness"]),
    ("motion", "PACK_READS_MOTION", &["sceneMotion", "lastUv"]),
    ("history", "PACK_READS_HISTORY", &["lastColor"]),
    ("block_light", "PACK_READS_BLOCK_LIGHT", &["lightAround", "hasBlockLight", "blockGlow"]),
];

impl PassReads {
    fn engine(&self) -> [bool; 6] {
        [self.color, self.depth, self.normals, self.motion, self.history, self.block_light]
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
}

impl PackOption {
    /// Its value as the programs read it when no one has set it.
    pub fn default_value(&self) -> f32 {
        match &self.kind {
            OptionKind::Number { default, .. } => *default,
            OptionKind::Toggle { default } => f32::from(u8::from(*default)),
            OptionKind::Choice { default, .. } => *default as f32,
        }
    }

    /// A value set for it, kept to what it can be.
    pub fn kept(&self, value: f32) -> f32 {
        match &self.kind {
            OptionKind::Number { low, high, .. } => value.clamp(*low, *high),
            OptionKind::Toggle { .. } => f32::from(u8::from(value >= 0.5)),
            OptionKind::Choice { choices, .. } => value.round().clamp(0.0, choices.len().saturating_sub(1) as f32),
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
    surface: Option<toml::Value>,
    #[serde(default)]
    engine: Option<toml::Value>,
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
const RESERVED: [&str; 30] = [
    "color",
    "depth",
    "normals",
    "motion",
    "history",
    "block_light",
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
        ensure!(file.surface.is_none(), "[surface]: programs that light surfaces are not drawn yet, only passes");
        ensure!(file.engine.is_none(), "[engine]: a pack does not set the engine's own passes yet");
        // As the file lists them, which a menu keeps.
        let order = option_order(text);
        let mut written: Vec<(String, OptionData)> = file.options.into_iter().collect();
        written.sort_by_key(|(key, _)| order.iter().position(|named| named == key).unwrap_or(usize::MAX));
        let options = written.into_iter().map(|(key, data)| option(key, data)).collect::<Result<Vec<_>>>()?;
        ensure!(options.len() <= MAX_OPTIONS, "{} options are more than the {MAX_OPTIONS} a pack has", options.len());
        ensure!(file.passes.len() <= MAX_PASSES, "{} passes are more than {MAX_PASSES}", file.passes.len());
        ensure!(!file.passes.is_empty(), "the pack has no [[pass]]");
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
                    name => {
                        let Some(target) = targets.iter().position(|target| target.name == name) else {
                            bail!(
                                "the pass `{}` reads `{name}`, which is neither one of the frame's images (color, \
                                 depth, normals, motion, history, block_light) nor a target a pass writes",
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
        Ok(Self { description: file.description, options, passes, targets })
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
    code.push_str("struct Options\n{\n");
    for option in &manifest.options {
        let kind = match option.kind {
            OptionKind::Number { .. } => "float",
            OptionKind::Toggle { .. } => "bool",
            OptionKind::Choice { .. } => "uint",
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
        };
        code.push_str(&format!("    o.{} = {value};\n", option.key));
    }
    code.push_str("    return o;\n}\n\n");
    for (slot, target) in manifest.targets.iter().enumerate() {
        code.push_str(&format!("static const PackTarget {} = {{ {slot} }};\n", target.name));
    }
    code.push_str(&format!("\n#include \"{}\"\n\n", pass.program));
    code.push_str(
        "[shader(\"fragment\")]\nfloat4 packFragment(PackVertex v) : SV_Target\n{\n    return run(packInput(v));\n}\n",
    );
    code
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

/// Compiles a pack's programs, one `slangc` a pass, all at once: what
/// takes a few seconds the first time and nothing after. An error names
/// the pass and Slang's message names the file and the line.
pub fn compile(manifest: &PackManifest, files: &PackFiles) -> Result<CompiledPack> {
    let engine =
        crate::hot_reload::engine_shader_dir().context("the engine's shader sources are not installed beside it")?;
    let header = std::fs::read_to_string(engine.join("shader_pack.slang")).context("reading shader_pack.slang")?;
    // Everything a pass's SPIR-V is made from, for the hash it is kept by.
    let mut sources = vec![header];
    let mut embedded: Vec<(&str, &str)> = Vec::new();
    match files {
        PackFiles::Folder(folder) => {
            for entry in std::fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "slang") {
                    sources.push(format!("{}\n{}", path.display(), std::fs::read_to_string(&path)?));
                }
            }
        }
        PackFiles::Embedded(list) => {
            for (name, text) in *list {
                sources.push(format!("{name}\n{text}"));
                embedded.push((*name, *text));
            }
        }
        PackFiles::Files(list) => {
            for (name, text) in list.iter() {
                check_program(name)?;
                sources.push(format!("{name}\n{text}"));
                embedded.push((name.as_str(), text.as_str()));
            }
        }
    }
    sources.sort();
    let work = std::env::temp_dir().join(format!("voxear-shader-packs-{}", std::process::id()));
    let compiled: Vec<Result<Arc<[u8]>>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = manifest
            .passes
            .iter()
            .enumerate()
            .map(|(at, pass)| {
                let code = wrapper(manifest, pass);
                let hash = stable_hash(&[&code, &sources.join("\n")]);
                let (work, engine, embedded) = (&work, &engine, &embedded);
                scope.spawn(move || -> Result<Arc<[u8]>> {
                    if let Some(spirv) = compiled_before().lock().expect("not poisoned").get(&hash) {
                        return Ok(spirv.clone());
                    }
                    let dir = work.join(format!("{hash:016x}"));
                    std::fs::create_dir_all(&dir)?;
                    for (name, text) in embedded {
                        std::fs::write(dir.join(name), text)?;
                    }
                    let source = dir.join(format!("voxear_pass_{at}.slang"));
                    std::fs::write(&source, &code)?;
                    let folder = files.folder().map_or_else(|| dir.clone(), Path::to_path_buf);
                    let spirv = compile_program(&source, &[&folder, engine], &dir.join("pass.spv"))
                        .with_context(|| format!("the pass `{}`", pass.name))?;
                    let spirv: Arc<[u8]> = spirv.into();
                    compiled_before().lock().expect("not poisoned").insert(hash, spirv.clone());
                    let _ = std::fs::remove_dir_all(&dir);
                    Ok(spirv)
                })
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().unwrap_or_else(|_| bail!("the shader compiler thread panicked")))
            .collect()
    });
    let passes = compiled.into_iter().collect::<Result<Vec<_>>>()?;
    let generation = GENERATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(CompiledPack { manifest: manifest.clone(), passes, generation })
}

/// One wrapped program compiled to SPIR-V by `slangc`, its error with a
/// word on what a pass forgot to read where that is what it is.
fn compile_program(source: &Path, includes: &[&Path], target: &Path) -> Result<Vec<u8>> {
    let mut command = Command::new(crate::hot_reload::slangc());
    command.arg(source).args(crate::hot_reload::SLANG_ARGS);
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
    pub pad0: f32,
    pub options: [f32; MAX_OPTIONS],
    pub depth_texture: u32,
    pub surface_texture: u32,
    pub motion_texture: u32,
    pub last_color_texture: u32,
    pub targets: [u32; MAX_TARGETS],
    pub last_targets: [u32; MAX_TARGETS],
    pub sky_light: u64,
    pub light_window: u64,
    pub light_bytes: u64,
    pub pad1: u64,
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
            sky_light: facts.addresses[0],
            light_window: facts.addresses[1],
            light_bytes: facts.addresses[2],
            ..Self::default()
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
                    PassWrites::Color if pass.point == PackPoint::Final => LDR_FORMAT,
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
}

/// The images a view's frame gives a pack, and the sizes it draws at.
pub(crate) struct PackImages {
    pub depth: ImageId,
    pub motion: ImageId,
    pub surface: Option<ImageId>,
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
            PackPoint::Final => images.shown,
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

    /// Records the pack's passes at `point` over `color`, the frame's colour
    /// there; returns the colour after them, which at a point whose colour
    /// is written in place is `color` itself.
    pub fn record(&self, fg: &mut FrameGraph<'_>, point: PackPoint, color: ImageId) -> ImageId {
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
            if pass.reads.history
                && let Some((_, Some(last))) = self.history
            {
                sampled(last);
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
            let push = Push {
                texture: source_texture,
                max_draws: index as u32,
                size: [extent.width, extent.height],
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
    fn a_manifest_that_cannot_be_drawn_says_why() {
        let pass = "[[pass]]\nname = \"A\"\nprogram = \"a.slang\"\nat = \"final\"\n";
        let fails = |text: &str, says: &str| {
            let error = format!("{:#}", PackManifest::parse(text).expect_err(text));
            assert!(error.contains(says), "{text}: {error}");
        };
        fails("format = 1", "no [[pass]]");
        fails("format = 9", "newer Voxear");
        fails(&format!("{pass}reads = [\"shadow\"]"), "neither one of the frame's images");
        fails(&format!("{pass}reads = [\"rays\"]\n{pass}writes = {{ target = \"rays\" }}"), "before the pass");
        fails(&format!("{pass}writes = {{ target = \"rays\" }}\nreads = [\"rays\"]"), "reads the target it writes");
        fails(&format!("{pass}writes = {{ target = \"rays\", scale = 0.3 }}"), "1, 0.5 or 0.25");
        fails(&format!("{pass}writes = {{ target = \"Rays\" }}"), "small letters");
        fails(&format!("{pass}writes = {{ target = \"depth\" }}"), "already have");
        fails(&pass.replace("a.slang", "../a.slang"), "not a file in the pack's folder");
        fails(&pass.replace("a.slang", "a.hlsl"), "not a .slang file");
        fails(&pass.replace("final", "after_sky"), "unknown variant");
        fails(&format!("[surface]\nblocks = \"b.slang\"\n{pass}"), "[surface]");
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
    fn the_frame_a_pack_reads_is_laid_out_as_the_shader_has_it() {
        // Five matrices, then ten rows of sixteen bytes, the options, the images and four pointers.
        assert_eq!(size_of::<GpuPackFrame>(), 5 * 64 + 10 * 16 + 4 * MAX_OPTIONS + 16 + 8 * MAX_TARGETS + 32);
    }

    #[test]
    fn every_built_in_pack_reads_and_says_what_it_is() {
        for (key, title, _) in BUILTIN {
            let manifest = PackManifest::builtin(key).unwrap_or_else(|error| panic!("{key}: {error:#}"));
            assert!(!title.is_empty() && manifest.description.len() > 20, "{key} says what it is");
            for pass in &manifest.passes {
                builtin_files(key).unwrap().iter().find(|(name, _)| *name == pass.program).expect("its program");
            }
        }
        assert!(PackManifest::builtin("gloomy").is_err());
    }
}
