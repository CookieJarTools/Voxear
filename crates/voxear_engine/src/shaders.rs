//! Custom shaders: `.shader` assets in Slang, which materials pick in place
//! of the standard one. A shader is a struct of properties and any of three
//! functions, all optional:
//!
//! ```slang
//! [Unlit]                       // only its colour and emission, unlit
//! [MaxOffset(0.3)]              // how far `vertex` moves anything, in metres
//! struct Water
//! {
//!     [Property, Color] float3 deep = float3(0.0, 0.15, 0.25);
//!     [Property, Range(0, 4)] float waveSpeed = 1.0;
//!     [Property] Picture ripples;
//! }
//!
//! void surface(Water water, SurfaceInput input, inout SurfaceOutput surface) { ... }
//! float3 vertex(Water water, VertexInput input) { ... }   // an offset, in world space
//! float3 light(Water water, LightInput light, SurfaceOutput surface) { ... }
//! ```
//!
//! This module reads what the renderer and the editor need to know of one:
//! its name, its properties with their defaults, what its functions are,
//! and how it is drawn ([`ShaderInfo`]). Slang itself compiles it, into the
//! renderer's own shaders (see `shader_code`).
//!
//! The engine has shaders of its own built in too ([`BuiltinShader`]),
//! which any material can pick as it picks a project's.

use std::{collections::BTreeSet, fmt, str::FromStr};

use anyhow::{Context, Result, bail, ensure};
use voxear_assets::AssetGuid;

/// How a material's file names a built-in shader: `builtin:toon`.
const BUILTIN: &str = "builtin:";

/// A shader the engine has built in, which any material can pick. Each is
/// compiled into the renderer's shaders only once a material uses it, so a
/// project pays nothing for those it leaves alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinShader {
    /// Light in flat steps, as a cartoon is painted.
    Toon,
    /// Soft, painted light, for the Storybook lighting look.
    Storybook,
    /// Pictures laid on by where the surface is, rather than by its UVs.
    Triplanar,
    /// Moving waves, ripples, and foam on the crests.
    Water,
    /// A figure of light with scrolling lines and a flicker.
    Hologram,
    /// Vertices swaying to and fro, for flags, signs and plants.
    Sway,
    /// Glowing molten rock under a drifting crust.
    Lava,
}

impl BuiltinShader {
    pub const ALL: [Self; 7] =
        [Self::Toon, Self::Storybook, Self::Triplanar, Self::Water, Self::Hologram, Self::Sway, Self::Lava];

    /// As a material's file names it, after `builtin:`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Toon => "toon",
            Self::Storybook => "storybook",
            Self::Triplanar => "triplanar",
            Self::Water => "water",
            Self::Hologram => "hologram",
            Self::Sway => "sway",
            Self::Lava => "lava",
        }
    }

    /// As the editor shows it, and its properties struct is named.
    pub fn title(self) -> &'static str {
        match self {
            Self::Toon => "Toon",
            Self::Storybook => "Storybook",
            Self::Triplanar => "Triplanar",
            Self::Water => "Water",
            Self::Hologram => "Hologram",
            Self::Sway => "Sway",
            Self::Lava => "Lava",
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            Self::Toon => include_str!("../shaders/toon.shader"),
            Self::Storybook => include_str!("../shaders/storybook.shader"),
            Self::Triplanar => include_str!("../shaders/triplanar.shader"),
            Self::Water => include_str!("../shaders/water.shader"),
            Self::Hologram => include_str!("../shaders/hologram.shader"),
            Self::Sway => include_str!("../shaders/sway.shader"),
            Self::Lava => include_str!("../shaders/lava.shader"),
        }
    }

    /// `builtin:toon`, as a material's file and the object picker name it.
    pub fn reference(self) -> String {
        format!("{BUILTIN}{}", self.name())
    }

    /// The built-in shader `reference` names, as `builtin:toon`.
    pub fn from_reference(reference: &str) -> Option<Self> {
        let name = reference.strip_prefix(BUILTIN)?;
        Self::ALL.into_iter().find(|shader| shader.name() == name)
    }
}

/// The shader a material picks in place of the standard one: one built in,
/// or one of the project's `.shader` assets. A material's file writes it as
/// `builtin:toon` or as the asset's GUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ShaderRef {
    Builtin(BuiltinShader),
    Asset(AssetGuid),
}

impl fmt::Display for ShaderRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(shader) => f.write_str(&shader.reference()),
            Self::Asset(guid) => write!(f, "{guid}"),
        }
    }
}

impl FromStr for ShaderRef {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        if text.starts_with(BUILTIN) {
            return BuiltinShader::from_reference(text).map(Self::Builtin).with_context(|| {
                let names: Vec<String> = BuiltinShader::ALL.iter().map(|shader| shader.reference()).collect();
                format!("{text} is not a built-in shader; they are {}", names.join(", "))
            });
        }
        Ok(Self::Asset(text.parse().with_context(|| format!("{text} is not a shader's GUID"))?))
    }
}

impl TryFrom<String> for ShaderRef {
    type Error = anyhow::Error;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

impl From<ShaderRef> for String {
    fn from(shader: ShaderRef) -> Self {
        shader.to_string()
    }
}

/// A shader the renderer can draw with: built in, or one of the project's.
/// Its number in the renderer's dispatch (from 1; 0 is the standard
/// shader), its source, and what it declares, or why it could not be read.
#[derive(Clone, Debug)]
pub struct ProjectShader {
    pub reference: ShaderRef,
    /// Its asset's path, or `builtin:toon` for one built in.
    pub path: String,
    pub number: u32,
    pub source: String,
    pub info: std::result::Result<ShaderInfo, String>,
    /// Whether it is compiled into the renderer's shaders: every one of the
    /// project's own, and the built-in ones its materials use.
    pub used: bool,
}

/// The shaders materials can pick: the built-in ones first, in their order,
/// then the project's, in the order of their paths, so the same project
/// numbers them the same everywhere: in the editor, and in a game built
/// from it.
#[derive(Clone, Debug, Default)]
pub struct ShaderRegistry {
    pub shaders: Vec<ProjectShader>,
}

impl ShaderRegistry {
    /// Numbers and reads the built-in shaders and these of the project's,
    /// each its asset, path and source; `used` are the built-in ones that
    /// materials pick.
    pub fn new(mut found: Vec<(AssetGuid, String, String)>, used: &BTreeSet<BuiltinShader>) -> Self {
        found.sort_by(|a, b| a.1.cmp(&b.1));
        let builtin = BuiltinShader::ALL.into_iter().map(|shader| {
            (ShaderRef::Builtin(shader), shader.reference(), shader.source().to_string(), used.contains(&shader))
        });
        let project = found.into_iter().map(|(guid, path, source)| (ShaderRef::Asset(guid), path, source, true));
        let shaders = (1..)
            .zip(builtin.chain(project))
            .map(|(number, (reference, path, text, used))| {
                // A graph's file is its nodes, compiled here into the Slang a `.shader` holds.
                let source = if is_graph(&path) { graph_source(&text) } else { Ok(text) };
                let info = source.as_ref().map_err(String::clone).and_then(|s| read(s).map_err(|e| format!("{e:#}")));
                ProjectShader { reference, path, number, source: source.unwrap_or_default(), info, used }
            })
            .collect();
        Self { shaders }
    }

    /// A shader's number and what it declares, when there is such a shader
    /// and it reads.
    pub fn get(&self, reference: ShaderRef) -> Option<(u32, &ShaderInfo)> {
        let shader = self.shaders.iter().find(|shader| shader.reference == reference)?;
        Some((shader.number, shader.info.as_ref().ok()?))
    }

    /// The project's own shaders, without the built-in ones.
    pub fn project_shaders(&self) -> impl Iterator<Item = &ProjectShader> {
        self.shaders.iter().filter(|shader| matches!(shader.reference, ShaderRef::Asset(_)))
    }

    /// Whether any shader is compiled in: the project has one of its own, or
    /// a material picks a built-in one.
    pub fn any_used(&self) -> bool {
        self.shaders.iter().any(|shader| shader.used)
    }
}

/// The built-in shaders that materials pick, from what each picks.
pub fn builtins_used(picked: impl IntoIterator<Item = Option<ShaderRef>>) -> BTreeSet<BuiltinShader> {
    picked
        .into_iter()
        .filter_map(|shader| match shader {
            Some(ShaderRef::Builtin(builtin)) => Some(builtin),
            _ => None,
        })
        .collect()
}

/// What a property holds, and so how the material Inspector edits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    Float,
    Float2,
    Float3,
    Float4,
    /// An sRGB colour, picked from a swatch; `float3`, or `float4` with alpha.
    Color { alpha: bool },
    Toggle,
    /// A picture asset, sampled with `sample(uv)`.
    Picture,
}

impl PropertyKind {
    /// How many of its four values the property uses.
    pub fn values(self) -> usize {
        match self {
            Self::Float | Self::Toggle | Self::Picture => 1,
            Self::Float2 => 2,
            Self::Float3 | Self::Color { alpha: false } => 3,
            Self::Float4 | Self::Color { alpha: true } => 4,
        }
    }
}

/// One property of a shader, as its struct declares it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderProperty {
    pub name: String,
    pub kind: PropertyKind,
    /// Its value until a material sets it: as many as `kind` uses, the rest
    /// 0. A picture's default is none.
    pub default: [f32; 4],
    /// `[Range(min, max)]`: the least and most a number may be.
    pub range: Option<(f32, f32)>,
    /// `[Tooltip("...")]`: what it does, shown when the pointer rests on it.
    pub tooltip: Option<String>,
}

/// What a shader is, as its file declares it.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderInfo {
    /// Its properties struct's name, which its functions take.
    pub name: String,
    pub properties: Vec<ShaderProperty>,
    pub surface: bool,
    pub vertex: bool,
    pub light: bool,
    /// A UI shader: its `ui` function draws a UI element's background (see
    /// [`ui_dispatch_code`]). It draws UI or materials, never both.
    pub ui: bool,
    /// A look shader: its `look` function has the last word on the finished
    /// picture, where a look pack names it (see [`look_dispatch_code`]). It
    /// does that alone.
    pub look: bool,
    /// `[Unlit]`: drawn as its surface's colour and emission alone.
    pub unlit: bool,
    /// `[MaxOffset(metres)]`: how far its `vertex` may move a vertex, which
    /// culling allows for.
    pub max_offset: f32,
}

/// Whether the shader at `path` is a graph of nodes rather than Slang.
pub fn is_graph(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".shadergraph")
}

/// The Slang a shader graph's file compiles into, or what stops it, each
/// thing wrong with the node it is at, by its title.
pub fn graph_source(text: &str) -> Result<String, String> {
    let graph = voxear_shader_graph::Graph::from_toml(text).map_err(|error| format!("{error:#}"))?;
    match voxear_shader_graph::compile(&graph) {
        Ok(compiled) => Ok(compiled.source),
        Err(errors) => {
            let at = |error: &voxear_shader_graph::CompileError| {
                let node = error.node.and_then(|id| graph.node(id)).map(|node| node.kind.title());
                node.map_or(error.message.clone(), |title| format!("{title}: {}", error.message))
            };
            Err(errors.iter().map(at).collect::<Vec<_>>().join("; "))
        }
    }
}

/// Reads a shader's struct, properties and functions from its source.
pub fn read(source: &str) -> Result<ShaderInfo> {
    let code = without_comments(source);
    let (attributes, name, body) = properties_struct(&code)?;
    let mut info = ShaderInfo {
        name: name.clone(),
        properties: Vec::new(),
        surface: declares(&code, "surface"),
        vertex: declares(&code, "vertex"),
        light: declares(&code, "light"),
        ui: declares(&code, "ui"),
        look: declares(&code, "look"),
        unlit: attributes.iter().any(|(attribute, _)| attribute == "Unlit"),
        max_offset: 0.0,
    };
    if let Some((_, arguments)) = attributes.iter().find(|(attribute, _)| attribute == "MaxOffset") {
        info.max_offset = number(arguments).with_context(|| format!("[MaxOffset] on {name} takes metres"))?.max(0.0);
    }
    if !info.surface && !info.vertex && !info.light && !info.unlit && !info.ui && !info.look {
        bail!("{name} has no surface, vertex, light, ui or look function");
    }
    if info.look && (info.surface || info.vertex || info.light || info.unlit || info.ui) {
        bail!("{name} draws the finished picture with its look function, and nothing else: it cannot have a surface, vertex, light or ui function, or be [Unlit]");
    }
    if info.ui && (info.surface || info.vertex || info.light || info.unlit) {
        bail!("{name} draws UI with its ui function, so it draws no materials: it cannot have a surface, vertex or light function, or be [Unlit]");
    }
    for field in body.split(';').map(str::trim).filter(|field| !field.is_empty()) {
        if let Some(property) = property(field).with_context(|| format!("in {name}: `{field}`"))? {
            info.properties.push(property);
        }
    }
    if info.look && info.properties.len() > voxear_render::LOOK_VALUES {
        bail!("{name} is a look shader, which has at most {} properties", voxear_render::LOOK_VALUES);
    }
    if info.look && info.properties.iter().any(|property| property.kind == PropertyKind::Picture) {
        bail!("{name} is a look shader, whose properties are numbers, vectors, colours and toggles, not pictures");
    }
    if info.ui && info.properties.iter().any(|property| property.kind == PropertyKind::Picture) {
        bail!("{name} draws UI, whose properties are numbers, vectors, colours and toggles, not pictures");
    }
    if info.properties.len() > MAX_PROPERTIES {
        bail!("{name} has {} properties; a shader has at most {MAX_PROPERTIES}", info.properties.len());
    }
    Ok(info)
}

/// The source with its comments blanked out, so they declare nothing.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("//") {
            rest = after.find('\n').map_or("", |end| &after[end..]);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.find("*/").map_or("", |end| &after[end + 2..]);
            out.push(' ');
        } else {
            let next = rest.chars().next().expect("not empty");
            out.push(next);
            rest = &rest[next.len_utf8()..];
        }
    }
    out
}

/// The first struct with a `[Property]` field, or else the first struct:
/// the attributes before it, its name and what is between its braces.
fn properties_struct(code: &str) -> Result<(Vec<(String, String)>, String, String)> {
    let mut structs = Vec::new();
    let mut at = 0;
    while let Some(found) = code[at..].find("struct ") {
        let start = at + found;
        at = start + 7;
        let before = code[..start].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let name: String = code[at..].trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let Some(open) = code[at..].find('{').map(|open| at + open) else { continue };
        let close = matching(code, open).with_context(|| format!("struct {name} has no closing brace"))?;
        structs.push((attributes_before(&code[..start]), name, code[open + 1..close].to_string()));
    }
    let marked = structs.iter().position(|(_, _, body)| body.contains("Property"));
    let chosen = marked.or((!structs.is_empty()).then_some(0));
    chosen.map(|at| structs.swap_remove(at)).context("a shader needs a struct of its properties, even an empty one")
}

/// Where the brace matching the one at `open` is.
fn matching(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (at, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `[...]` attributes right before the end of `code`, each its name and
/// what is in its brackets, nearest last.
fn attributes_before(code: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = code.trim_end();
    while let Some(inside) = rest.strip_suffix(']') {
        let Some(open) = inside.rfind('[') else { break };
        found.splice(0..0, split_attributes(&inside[open + 1..]));
        rest = inside[..open].trim_end();
    }
    found
}

/// `Property, Range(0, 1)` as each attribute and its arguments.
fn split_attributes(list: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (at, c) in list.char_indices().chain(std::iter::once((list.len(), ','))) {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let item = list[start..at].trim();
                if !item.is_empty() {
                    let (name, arguments) = match item.split_once('(') {
                        Some((name, arguments)) => (name.trim(), arguments.trim_end_matches(')').trim()),
                        None => (item, ""),
                    };
                    found.push((name.to_string(), arguments.to_string()));
                }
                start = at + 1;
            }
            _ => {}
        }
    }
    found
}

/// Whether the code declares a function of this name: its name, then `(`,
/// after a type.
fn declares(code: &str, function: &str) -> bool {
    let mut at = 0;
    while let Some(found) = code[at..].find(function) {
        let start = at + found;
        at = start + function.len();
        let before = code[..start].trim_end();
        let after = code[at..].trim_start();
        let word_before = before.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_');
        let joined = code[..start].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
        if after.starts_with('(') && word_before && !joined {
            return true;
        }
    }
    false
}

/// A field of the properties struct, when it is a `[Property]`.
fn property(field: &str) -> Result<Option<ShaderProperty>> {
    let mut attributes = Vec::new();
    let mut rest = field.trim();
    while let Some(inside) = rest.strip_prefix('[') {
        let close = inside.find(']').context("an attribute is not closed")?;
        attributes.extend(split_attributes(&inside[..close]));
        rest = inside[close + 1..].trim_start();
    }
    if !attributes.iter().any(|(name, _)| name == "Property") {
        return Ok(None);
    }
    let (declaration, default) = match rest.split_once('=') {
        Some((declaration, default)) => (declaration.trim(), Some(default.trim())),
        None => (rest, None),
    };
    let mut words = declaration.split_whitespace();
    let (Some(type_name), Some(name), None) = (words.next(), words.next(), words.next()) else {
        bail!("a property is a type and a name");
    };
    let color = attributes.iter().any(|(attribute, _)| attribute == "Color");
    let kind = match (type_name, color) {
        ("float", false) => PropertyKind::Float,
        ("float2", false) => PropertyKind::Float2,
        ("float3", false) => PropertyKind::Float3,
        ("float4", false) => PropertyKind::Float4,
        ("float3", true) => PropertyKind::Color { alpha: false },
        ("float4", true) => PropertyKind::Color { alpha: true },
        ("bool", false) => PropertyKind::Toggle,
        ("Picture", false) => PropertyKind::Picture,
        (other, true) => bail!("[Color] is for float3 or float4, not {other}"),
        (other, false) => bail!("a property is a float, float2, float3, float4, bool or Picture, not {other}"),
    };
    let mut values = [0.0; 4];
    if let Some(default) = default {
        let numbers = default_values(default, kind).with_context(|| format!("the default of {name}"))?;
        values[..numbers.len()].copy_from_slice(&numbers);
    }
    let range = attributes.iter().find(|(attribute, _)| attribute == "Range").map(|(_, arguments)| {
        let mut bounds = arguments.split(',').filter_map(|bound| number(bound).ok());
        (bounds.next().unwrap_or(0.0), bounds.next().unwrap_or(1.0))
    });
    let tooltip = attributes
        .iter()
        .find(|(attribute, _)| attribute == "Tooltip")
        .map(|(_, arguments)| arguments.trim_matches('"').to_string());
    Ok(Some(ShaderProperty { name: name.to_string(), kind, default: values, range, tooltip }))
}

/// A default as numbers: `1.0`, `true`, or `float3(0.1, 0.2, 0.3)`.
fn default_values(default: &str, kind: PropertyKind) -> Result<Vec<f32>> {
    match kind {
        PropertyKind::Toggle => match default {
            "true" => Ok(vec![1.0]),
            "false" => Ok(vec![0.0]),
            _ => bail!("a bool's default is true or false"),
        },
        PropertyKind::Picture => bail!("a Picture has no default; a material picks it"),
        _ => {
            let inside = match default.split_once('(') {
                Some((_, inside)) => inside.trim_end_matches(')'),
                None => default,
            };
            let numbers: Vec<f32> = inside.split(',').map(number).collect::<Result<_>>()?;
            match numbers.len() {
                1 => Ok(vec![numbers[0]; kind.values()]),
                count if count == kind.values() => Ok(numbers),
                count => bail!("{count} values for {} wanted", kind.values()),
            }
        }
    }
}

fn number(text: &str) -> Result<f32> {
    let text = text.trim().trim_end_matches(['f', 'F']);
    text.parse::<f32>().with_context(|| format!("`{text}` is not a number"))
}

/// The project's shaders as Slang for the renderer to compile in place of
/// its empty `project_shaders.slang`: each shader's source in a namespace of
/// its own, named for its number, a function reading each one's properties
/// from its material, and the functions that call the one a material picks
/// by its number. Shaders that do not read, and built-in ones no material
/// uses, are left out, so their materials draw as the standard shader.
pub fn dispatch_code(registry: &ShaderRegistry) -> String {
    let shaders: Vec<(&ProjectShader, &ShaderInfo)> = registry
        .shaders
        .iter()
        .filter(|shader| shader.used)
        .filter_map(|shader| Some((shader, shader.info.as_ref().ok()?)))
        .filter(|(_, info)| !info.ui && !info.look)
        .collect();
    let mut code = String::from("// Written by the engine from the project's .shader assets.\n\n#pragma once\n\n");
    for (shader, info) in &shaders {
        let number = shader.number;
        code += &format!("namespace shader_{number}\n{{\n#line 1 \"{}\"\n{}\n}}\n\n", shader.path, shader.source);
        let values = "buffers()->materialProperties + material * MATERIAL_PROPERTIES";
        code += &properties_reader(&format!("shader_{number}"), info, "uint material", values);
    }
    let calls = |wanted: fn(&ShaderInfo) -> bool, call: &dyn Fn(u32) -> String| -> String {
        let mut cases = String::new();
        for (shader, _) in shaders.iter().filter(|(_, info)| wanted(info)) {
            cases += &format!("    case {}:\n        {}\n", shader.number, call(shader.number));
        }
        cases
    };
    let switch = "    switch (buffers()->materials[material].shader)\n    {\n";
    code += "void customSurface(uint material, SurfaceInput input, inout SurfaceOutput surface)\n{\n";
    code += switch;
    code += &calls(|info| info.surface, &|n| {
        format!("shader_{n}::surface(shader_{n}_properties(material), input, surface);\n        break;")
    });
    code += "    default:\n        break;\n    }\n}\n\n";
    code += "float3 customVertex(uint material, VertexInput input)\n{\n";
    code += switch;
    code += &calls(|info| info.vertex, &|n| {
        format!("return shader_{n}::vertex(shader_{n}_properties(material), input);")
    });
    code += "    default:\n        return 0.0;\n    }\n}\n\n";
    code += "bool customUnlit(uint material)\n{\n";
    code += switch;
    code += &calls(|info| info.unlit, &|_| "return true;".to_string());
    code += "    default:\n        return false;\n    }\n}\n\n";
    code += "bool customLighting(uint material)\n{\n";
    code += switch;
    code += &calls(|info| info.light, &|_| "return true;".to_string());
    code += "    default:\n        return false;\n    }\n}\n\n";
    code += "float3 customLight(uint material, LightInput light, SurfaceOutput surface)\n{\n";
    code += switch;
    code += &calls(|info| info.light, &|n| {
        format!("return shader_{n}::light(shader_{n}_properties(material), light, surface);")
    });
    code += "    default:\n        return 0.0;\n    }\n}\n";
    code
}

/// A function `{namespace}_properties` that reads a shader's properties from
/// `values`, a `float4*` worked out from what it takes, `argument`.
fn properties_reader(namespace: &str, info: &ShaderInfo, argument: &str, values: &str) -> String {
    let mut code = format!(
        "{namespace}::{name} {namespace}_properties({argument})\n{{\n    float4* values = {values};\n    \
         {namespace}::{name} properties;\n",
        name = info.name
    );
    for (slot, property) in info.properties.iter().enumerate().take(MAX_PROPERTIES) {
        let value = match property.kind {
            PropertyKind::Float => format!("values[{slot}].x"),
            PropertyKind::Float2 => format!("values[{slot}].xy"),
            PropertyKind::Float3 | PropertyKind::Color { alpha: false } => format!("values[{slot}].xyz"),
            PropertyKind::Float4 | PropertyKind::Color { alpha: true } => format!("values[{slot}]"),
            PropertyKind::Toggle => format!("values[{slot}].x > 0.5"),
            PropertyKind::Picture => {
                code += &format!("    properties.{}.index = asuint(values[{slot}].x);\n", property.name);
                continue;
            }
        };
        code += &format!("    properties.{} = {value};\n", property.name);
    }
    code + "    return properties;\n}\n\n"
}

/// The code a project's UI shaders compile into the UI's shader as
/// (`project_ui_shaders.slang`; see `shaders/ui_custom.slang`): each in a
/// namespace of its own, named for its number, a function reading its
/// properties from the UI's values, where a shaded background's quad says
/// they start, and `customUi`, which calls the one the quad picks by its
/// number. Empty for a project without any, which compiles to none.
pub fn ui_dispatch_code(registry: &ShaderRegistry) -> String {
    let shaders: Vec<(&ProjectShader, &ShaderInfo)> = registry
        .shaders
        .iter()
        .filter_map(|shader| Some((shader, shader.info.as_ref().ok()?)))
        .filter(|(_, info)| info.ui)
        .collect();
    if shaders.is_empty() {
        return String::new();
    }
    let mut code = String::from("// Written by the engine from the project's .shader assets that draw UI.\n\n#pragma once\n\n");
    for (shader, info) in &shaders {
        let number = shader.number;
        code += &format!("namespace ui_shader_{number}\n{{\n#line 1 \"{}\"\n{}\n}}\n\n", shader.path, shader.source);
        code += &properties_reader(&format!("ui_shader_{number}"), info, "uint first", "buffers()->uiValues + first");
    }
    code += "float4 customUi(uint shader, uint first, UiInput input)\n{\n    switch (shader)\n    {\n";
    for (shader, _) in &shaders {
        let n = shader.number;
        code += &format!("    case {n}:\n        return ui_shader_{n}::ui(ui_shader_{n}_properties(first), input);\n");
    }
    code + "    default:\n        return input.background;\n    }\n}\n"
}

/// The code a project's look shaders compile into the post-processing's
/// shader as (`project_look_shaders.slang`; see `shaders/look_custom.slang`):
/// each in a namespace of its own, named for its number, a function reading
/// its properties from the frame's look values, and `customLook`, which
/// calls the one the frame names by its number. Empty for a project
/// without any, which compiles to none.
pub fn look_dispatch_code(registry: &ShaderRegistry) -> String {
    let shaders: Vec<(&ProjectShader, &ShaderInfo)> = registry
        .shaders
        .iter()
        .filter_map(|shader| Some((shader, shader.info.as_ref().ok()?)))
        .filter(|(_, info)| info.look)
        .collect();
    if shaders.is_empty() {
        return String::new();
    }
    let mut code = String::from("// Written by the engine from the project's .shader assets that draw the picture.\n\n#pragma once\n\n");
    for (shader, info) in &shaders {
        let number = shader.number;
        code += &format!("namespace look_shader_{number}\n{{\n#line 1 \"{}\"\n{}\n}}\n\n", shader.path, shader.source);
        let reader = properties_reader(&format!("look_shader_{number}"), info, "", "push.frame->post.lookValues");
        // The values are the frame's own array, not a pointer into a buffer.
        code += &reader.replace("float4* values = ", "float4 values[LOOK_VALUES] = ");
    }
    code += "float3 customLook(uint shader, LookInput input)\n{\n    switch (shader)\n    {\n";
    for (shader, _) in &shaders {
        let n = shader.number;
        code += &format!("    case {n}:\n        return look_shader_{n}::look(look_shader_{n}_properties(), input);\n");
    }
    code + "    default:\n        return input.color;\n    }\n}\n"
}

/// The look shader `reference` names, a project path or GUID, as a frame is
/// drawn with it: its number, and its properties' values, each its default
/// unless `values` gives one by its name, as a number, a list of numbers or
/// true or false. Colours are written sRGB and made linear.
pub fn look_shader(
    registry: &ShaderRegistry,
    found: &ShaderRef,
    values: &toml::Table,
) -> Result<voxear_render::LookShader> {
    let shader = registry.shaders.iter().find(|shader| shader.reference == *found).context("no such shader")?;
    let info = shader.info.as_ref().map_err(|error| anyhow::anyhow!("{}: {error}", shader.path))?;
    ensure!(info.look, "{} has no look function, so it cannot draw a look pack's picture", shader.path);
    let named = |name: &str| info.properties.iter().any(|property| property.name == name);
    if let Some(stray) = values.keys().find(|name| !named(name)) {
        bail!("{} has no property named {stray}", shader.path);
    }
    let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let mut slots = [[0.0; 4]; voxear_render::LOOK_VALUES];
    for (slot, property) in slots.iter_mut().zip(&info.properties) {
        let mut value = property.default;
        match values.get(&property.name) {
            Some(toml::Value::Boolean(on)) => value = [f32::from(u8::from(*on)), 0.0, 0.0, 0.0],
            Some(toml::Value::Array(parts)) => {
                let numbers = parts.iter().map(|part| part.as_float().or_else(|| part.as_integer().map(|n| n as f64)));
                for (place, number) in value.iter_mut().zip(numbers) {
                    *place = number.with_context(|| format!("{}'s {} takes numbers", shader.path, property.name))? as f32;
                }
            }
            Some(other) => {
                let number = other.as_float().or_else(|| other.as_integer().map(|n| n as f64));
                let number = number.with_context(|| format!("{}'s {} takes a number", shader.path, property.name))?;
                value = [number as f32; 4];
            }
            None => {}
        }
        if let PropertyKind::Color { alpha } = property.kind {
            value = [linear(value[0]), linear(value[1]), linear(value[2]), if alpha { value[3] } else { 1.0 }];
        }
        *slot = value;
    }
    Ok(voxear_render::LookShader { number: shader.number, values: slots })
}

/// The shaders the UI can draw backgrounds with: the built-in effects, and
/// the project's UI shaders, each by its path and by its GUID, as a style's
/// `shader: url("…")` names it. Colours' defaults are made linear and
/// premultiplied, as the UI's values are.
pub fn ui_shaders(registry: &ShaderRegistry) -> voxear_ui::UiShaders {
    use voxear_ui::{UiShader, UiShaderProperty, UiShaderValue};
    let mut shaders = voxear_ui::UiShaders::default();
    let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    for shader in &registry.shaders {
        let Some(info) = shader.info.as_ref().ok().filter(|info| info.ui) else { continue };
        let properties = info
            .properties
            .iter()
            .map(|property| {
                let [r, g, b, a] = property.default;
                let (value, default) = match property.kind {
                    PropertyKind::Float => (UiShaderValue::Number, property.default),
                    PropertyKind::Float2 => (UiShaderValue::Vector(2), property.default),
                    PropertyKind::Float3 => (UiShaderValue::Vector(3), property.default),
                    PropertyKind::Float4 => (UiShaderValue::Vector(4), property.default),
                    PropertyKind::Toggle => (UiShaderValue::Toggle, property.default),
                    PropertyKind::Color { alpha } => {
                        let a = if alpha { a } else { 1.0 };
                        (UiShaderValue::Color, [linear(r) * a, linear(g) * a, linear(b) * a, a])
                    }
                    PropertyKind::Picture => (UiShaderValue::Number, [0.0; 4]),
                };
                UiShaderProperty { name: property.name.clone(), value, default }
            })
            .collect();
        let made = UiShader { number: shader.number, properties };
        if let ShaderRef::Asset(guid) = shader.reference {
            shaders.insert(guid.to_string(), made.clone());
        }
        shaders.insert(shader.path.clone(), made);
    }
    shaders
}

/// The most properties a shader's materials can set: each is one of a
/// material's `MATERIAL_PROPERTIES` on the GPU.
pub const MAX_PROPERTIES: usize = voxear_render::MATERIAL_PROPERTIES;

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: &str = r#"
        // Water that ripples.
        [MaxOffset(0.3)]
        struct Water
        {
            [Property, Color] float3 deep = float3(0.0, 0.15, 0.25);
            [Property, Range(0, 4), Tooltip("How fast the waves go")] float waveSpeed = 1.0;
            [Property] Picture ripples;
            [Property] bool foam = true;
            float hidden; // not a property
        }

        void surface(Water water, SurfaceInput input, inout SurfaceOutput surface)
        {
            surface.color = water.deep * water.ripples.sample(input.uv).rgb;
        }

        float3 vertex(Water water, VertexInput input)
        {
            return float3(0.0, sin(input.time * water.waveSpeed) * 0.3, 0.0);
        }
    "#;

    #[test]
    fn a_shader_reads_its_properties_and_functions() {
        let info = read(WATER).unwrap();
        assert_eq!(info.name, "Water");
        assert!(info.surface && info.vertex && !info.light && !info.unlit);
        assert_eq!(info.max_offset, 0.3);
        let names: Vec<&str> = info.properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["deep", "waveSpeed", "ripples", "foam"], "the fields marked as properties, in order");
        assert_eq!(info.properties[0].kind, PropertyKind::Color { alpha: false });
        assert_eq!(info.properties[0].default, [0.0, 0.15, 0.25, 0.0]);
        assert_eq!(info.properties[1].range, Some((0.0, 4.0)));
        assert_eq!(info.properties[1].tooltip.as_deref(), Some("How fast the waves go"));
        assert_eq!(info.properties[2].kind, PropertyKind::Picture);
        assert_eq!(info.properties[3].default[0], 1.0);
    }

    #[test]
    fn a_shader_graph_is_compiled_into_the_registry_and_what_stops_it_is_kept_as_its_error() {
        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let good = voxear_shader_graph::starters::lit("Glow").to_toml();
        let mut broken = voxear_shader_graph::starters::lit("Broken");
        let light = broken.add(voxear_shader_graph::NodeKind::Input { input: voxear_shader_graph::Input::LightColor }, [0.0; 2]);
        let out = broken.nodes.iter().find(|n| n.kind == voxear_shader_graph::NodeKind::SurfaceOutput).unwrap().id;
        broken.link(light, 0, out, 5);
        let registry = ShaderRegistry::new(
            vec![
                (guid(1), "Shaders/Glow.shadergraph".into(), good),
                (guid(2), "Shaders/Broken.shadergraph".into(), broken.to_toml()),
            ],
            &BTreeSet::new(),
        );
        let (_, info) = registry.get(ShaderRef::Asset(guid(1))).expect("compiled and read");
        assert_eq!((info.name.as_str(), info.properties.len()), ("Glow", 2));
        let failed = registry.shaders.iter().find(|s| s.path.ends_with("Broken.shadergraph")).unwrap();
        let error = failed.info.as_ref().unwrap_err();
        assert!(error.starts_with("Light Color: ") && error.contains("Surface"), "{error}");
        assert!(is_graph("A/B.ShaderGraph") && !is_graph("A/B.shader"));
    }

    #[test]
    fn an_unlit_shader_needs_no_function_and_a_shader_with_nothing_is_refused() {
        let unlit = read("[Unlit]\nstruct Glow { [Property, Color] float3 tint = 1.0; }").unwrap();
        assert!(unlit.unlit && !unlit.surface);
        assert_eq!(unlit.properties[0].default, [1.0, 1.0, 1.0, 0.0], "one value for every channel");
        assert!(read("struct Nothing { }").is_err());
        assert!(read("void surface(int a) {}").is_err(), "no properties struct");
        assert!(read("struct S { [Property] double x; }\nvoid surface(S s) {}").is_err(), "an unknown type");
    }

    /// The number the project's first shader gets, after the built-in ones.
    const FIRST: u32 = BuiltinShader::ALL.len() as u32 + 1;

    #[test]
    fn shaders_are_numbered_by_path_after_the_built_in_ones_and_one_that_does_not_read_is_kept_with_its_error() {
        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let registry = ShaderRegistry::new(
            vec![
                (guid(1), "Shaders/Water.shader".into(), WATER.into()),
                (guid(2), "Shaders/Broken.shader".into(), "struct Nothing { }".into()),
            ],
            &BTreeSet::new(),
        );
        let numbers: Vec<(&str, u32)> = registry.project_shaders().map(|s| (s.path.as_str(), s.number)).collect();
        assert_eq!(numbers, [("Shaders/Broken.shader", FIRST), ("Shaders/Water.shader", FIRST + 1)]);
        let water = registry.get(ShaderRef::Asset(guid(1))).map(|(number, info)| (number, info.name.as_str()));
        assert_eq!(water, Some((FIRST + 1, "Water")));
        let broken = registry.get(ShaderRef::Asset(guid(2)));
        assert!(broken.is_none(), "one that does not read draws as the standard shader");
        assert_eq!(registry.get(ShaderRef::Builtin(BuiltinShader::Toon)).map(|(number, _)| number), Some(1));
    }

    #[test]
    fn every_built_in_shader_reads_and_its_struct_is_named_as_its_title() {
        for shader in BuiltinShader::ALL {
            let info = read(shader.source()).unwrap_or_else(|error| panic!("{}: {error:#}", shader.title()));
            assert_eq!(info.name, shader.title());
            assert!(!info.properties.is_empty(), "{} has settings", shader.title());
            assert_eq!(BuiltinShader::from_reference(&shader.reference()), Some(shader));
            let reference: ShaderRef = shader.reference().parse().unwrap();
            assert_eq!(reference, ShaderRef::Builtin(shader));
        }
        assert!("builtin:nothing".parse::<ShaderRef>().is_err());
        let toon = read(BuiltinShader::Toon.source()).unwrap();
        assert!(toon.surface && toon.light && !toon.unlit);
        assert!(read(BuiltinShader::Hologram.source()).unwrap().unlit);
        assert!(read(BuiltinShader::Water.source()).unwrap().max_offset >= 2.0, "waves up to 2 metres high");
    }

    #[test]
    fn only_the_built_in_shaders_materials_use_are_compiled_in() {
        let none = ShaderRegistry::new(Vec::new(), &BTreeSet::new());
        assert!(!none.any_used());
        assert!(!dispatch_code(&none).contains("namespace shader_"));
        let used = builtins_used([Some(ShaderRef::Builtin(BuiltinShader::Water)), None]);
        let water = ShaderRegistry::new(Vec::new(), &used);
        let code = dispatch_code(&water);
        assert!(water.any_used());
        assert!(code.contains("namespace shader_4\n{\n#line 1 \"builtin:water\""), "{code}");
        assert!(!code.contains("namespace shader_1\n"), "Toon is left out");
    }

    #[test]
    fn the_dispatch_calls_each_shaders_functions_by_its_number() {
        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let unlit = "[Unlit]\nstruct Glow { [Property, Color] float3 tint = 1.0; }";
        let registry = ShaderRegistry::new(
            vec![
                (guid(1), "Shaders/Water.shader".into(), WATER.into()),
                (guid(2), "Shaders/Glow.shader".into(), unlit.into()),
            ],
            &BTreeSet::new(),
        );
        let (glow, water) = (FIRST, FIRST + 1);
        let code = dispatch_code(&registry);
        assert!(code.contains(&format!("namespace shader_{water}\n{{\n#line 1 \"Shaders/Water.shader\"")), "{code}");
        assert!(code.contains("properties.deep = values[0].xyz;"));
        assert!(code.contains("properties.ripples.index = asuint(values[2].x);"));
        assert!(code.contains("properties.foam = values[3].x > 0.5;"));
        let surface =
            format!("case {water}:\n        shader_{water}::surface(shader_{water}_properties(material), input, surface);");
        assert!(code.contains(&surface));
        assert!(code.contains(&format!("case {water}:\n        return shader_{water}::vertex(")));
        assert!(code.contains(&format!("case {glow}:\n        return true;")), "Glow is unlit");
        assert!(!code.contains(&format!("shader_{glow}::surface")), "Glow has no surface function");
    }

    const SCAN: &str = r#"
        // Scanlines for a terminal.
        struct Scan
        {
            [Property, Color] float4 tint = float4(0.5, 1.0, 0.5, 1.0);
            [Property] float spacing = 3.0;
            [Property] bool flicker = true;
        }
        float4 ui(Scan scan, UiInput input) { return input.background * scan.tint; }
    "#;

    const PRINT: &str = r#"
        struct Print
        {
            [Property, Range(2, 16)] float steps = 6.0;
            [Property, Color] float3 paper = float3(1.0, 0.5, 0.0);
            [Property] bool lined = true;
        }
        float3 look(Print print, LookInput input) { return floor(input.color * print.steps) / print.steps * print.paper; }
    "#;

    #[test]
    fn a_look_shader_draws_the_picture_alone_and_a_pack_gives_its_properties_by_name() {
        let print = read(PRINT).unwrap();
        assert!(print.look && !print.ui && !print.surface && !print.unlit);
        let mixed = "struct Both { [Property] float x = 1.0; }\nfloat4 ui(Both b, UiInput i) { return 0.0; }\nfloat3 look(Both b, LookInput i) { return 0.0; }";
        assert!(read(mixed).unwrap_err().to_string().contains("nothing else"));
        let pictured = "struct Pictured { [Property] Picture pattern; }\nfloat3 look(Pictured p, LookInput i) { return 0.0; }";
        assert!(read(pictured).unwrap_err().to_string().contains("not pictures"));
        let many: String = (0..9).map(|n| format!("[Property] float p{n} = 0.0; ")).collect();
        let crowded = format!("struct Crowded {{ {many} }}\nfloat3 look(Crowded c, LookInput i) {{ return 0.0; }}");
        assert!(read(&crowded).unwrap_err().to_string().contains("at most 8"));

        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let registry = ShaderRegistry::new(
            vec![(guid(1), "Shaders/Print.shader".into(), PRINT.into()), (guid(2), "Shaders/Water.shader".into(), WATER.into())],
            &BTreeSet::new(),
        );
        let (number, water) = (FIRST, FIRST + 1);
        // Neither materials nor the UI are given it; the picture's dispatch calls it by its number.
        assert!(!dispatch_code(&registry).contains(&format!("shader_{number}")), "materials leave it out");
        assert!(ui_dispatch_code(&registry).is_empty());
        let code = look_dispatch_code(&registry);
        assert!(code.contains(&format!("namespace look_shader_{number}\n{{\n#line 1 \"Shaders/Print.shader\"")), "{code}");
        assert!(code.contains("float4 values[LOOK_VALUES] = push.frame->post.lookValues;"), "{code}");
        assert!(code.contains("properties.lined = values[2].x > 0.5;"));
        let call = format!("case {number}:\n        return look_shader_{number}::look(look_shader_{number}_properties(), input);");
        assert!(code.contains(&call), "{code}");
        assert!(!code.contains(&format!("look_shader_{water}")), "the water draws materials");
        assert!(look_dispatch_code(&ShaderRegistry::new(Vec::new(), &BTreeSet::new())).is_empty());

        // As a frame is drawn with it: its defaults, a colour made linear, and what a pack writes over them.
        let shader = ShaderRef::Asset(guid(1));
        let plain = look_shader(&registry, &shader, &toml::Table::new()).unwrap();
        assert_eq!((plain.number, plain.values[0][0], plain.values[2][0]), (number, 6.0, 1.0));
        assert!((plain.values[1][1] - 0.214).abs() < 0.002 && plain.values[1][3] == 1.0, "{:?}", plain.values[1]);
        let written: toml::Table = toml::from_str("steps = 4\npaper = [1.0, 1.0, 1.0]\nlined = false").unwrap();
        let set = look_shader(&registry, &shader, &written).unwrap();
        assert_eq!((set.values[0][0], set.values[1], set.values[2][0]), (4.0, [1.0; 4], 0.0));
        // A property it has not, a shader that draws materials, and one there is not, are refused.
        let stray: toml::Table = toml::from_str("ink = 1.0").unwrap();
        assert!(look_shader(&registry, &shader, &stray).unwrap_err().to_string().contains("no property named ink"));
        let materials = look_shader(&registry, &ShaderRef::Asset(guid(2)), &toml::Table::new());
        assert!(materials.unwrap_err().to_string().contains("no look function"));
        assert!(look_shader(&registry, &ShaderRef::Asset(guid(3)), &toml::Table::new()).is_err());
    }

    #[test]
    fn a_ui_shader_draws_ui_alone_and_is_dispatched_by_the_ui() {
        let scan = read(SCAN).unwrap();
        assert!(scan.ui && !scan.surface && !scan.unlit);
        let mixed = "struct Both { [Property] float x = 1.0; }\nvoid surface(Both b, SurfaceInput i, inout SurfaceOutput s) {}\nfloat4 ui(Both b, UiInput i) { return 0.0; }";
        assert!(read(mixed).unwrap_err().to_string().contains("draws UI"));
        let pictured = "struct Pictured { [Property] Picture pattern; }\nfloat4 ui(Pictured p, UiInput i) { return 0.0; }";
        assert!(read(pictured).unwrap_err().to_string().contains("not pictures"));

        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let registry = ShaderRegistry::new(
            vec![(guid(1), "Shaders/Scan.shader".into(), SCAN.into()), (guid(2), "Shaders/Water.shader".into(), WATER.into())],
            &BTreeSet::new(),
        );
        let (scan_number, water) = (FIRST, FIRST + 1);
        assert!(!dispatch_code(&registry).contains(&format!("shader_{scan_number}")), "materials leave it out");
        let code = ui_dispatch_code(&registry);
        assert!(code.contains(&format!("namespace ui_shader_{scan_number}\n{{\n#line 1 \"Shaders/Scan.shader\"")), "{code}");
        assert!(code.contains("float4* values = buffers()->uiValues + first;"));
        assert!(code.contains("properties.flicker = values[2].x > 0.5;"));
        let call = format!("case {scan_number}:\n        return ui_shader_{scan_number}::ui(ui_shader_{scan_number}_properties(first), input);");
        assert!(code.contains(&call), "{code}");
        assert!(!code.contains(&format!("ui_shader_{water}")), "the water draws materials");
        assert!(ui_dispatch_code(&ShaderRegistry::new(Vec::new(), &BTreeSet::new())).is_empty());

        let table = ui_shaders(&registry);
        let by_path = table.get("Shaders/Scan.shader").expect("by its path");
        assert_eq!(table.get(&guid(1).to_string()), Some(by_path), "and by its GUID");
        assert_eq!(by_path.number, scan_number);
        let tint = by_path.properties[0].default;
        assert!((tint[0] - 0.214).abs() < 0.01 && tint[1] == 1.0, "a colour made linear: {tint:?}");
        assert!(table.get("shimmer").is_some(), "with the built-in effects");
        assert!(table.get("Shaders/Water.shader").is_none());
    }

    #[test]
    fn a_function_named_in_a_comment_or_called_is_not_declared() {
        let info = read("// float3 light(...)\nstruct S { }\nvoid surface(S s) { float x = other.vertex(1); }").unwrap();
        assert!(info.surface && !info.light && !info.vertex);
    }
}
