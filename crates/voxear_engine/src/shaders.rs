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

use anyhow::{Context, Result, bail};
use voxear_assets::AssetGuid;

/// One of the project's shaders: its asset, its number in the renderer's
/// dispatch (from 1; 0 is the standard shader), its source, and what it
/// declares, or why it could not be read.
#[derive(Clone, Debug)]
pub struct ProjectShader {
    pub guid: AssetGuid,
    pub path: String,
    pub number: u32,
    pub source: String,
    pub info: std::result::Result<ShaderInfo, String>,
}

/// The project's shaders, numbered in the order of their paths, so the same
/// project numbers them the same everywhere: in the editor, and in a game
/// built from it.
#[derive(Clone, Debug, Default)]
pub struct ShaderRegistry {
    pub shaders: Vec<ProjectShader>,
}

impl ShaderRegistry {
    /// Numbers and reads these shaders, each its asset, path and source.
    pub fn new(mut found: Vec<(AssetGuid, String, String)>) -> Self {
        found.sort_by(|a, b| a.1.cmp(&b.1));
        let shaders = (1..)
            .zip(found)
            .map(|(number, (guid, path, source))| {
                let info = read(&source).map_err(|error| format!("{error:#}"));
                ProjectShader { guid, path, number, source, info }
            })
            .collect();
        Self { shaders }
    }

    /// A shader's number and what it declares, when the project has it and
    /// it reads.
    pub fn get(&self, guid: AssetGuid) -> Option<(u32, &ShaderInfo)> {
        let shader = self.shaders.iter().find(|shader| shader.guid == guid)?;
        Some((shader.number, shader.info.as_ref().ok()?))
    }
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
    /// `[Unlit]`: drawn as its surface's colour and emission alone.
    pub unlit: bool,
    /// `[MaxOffset(metres)]`: how far its `vertex` may move a vertex, which
    /// culling allows for.
    pub max_offset: f32,
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
        unlit: attributes.iter().any(|(attribute, _)| attribute == "Unlit"),
        max_offset: 0.0,
    };
    if let Some((_, arguments)) = attributes.iter().find(|(attribute, _)| attribute == "MaxOffset") {
        info.max_offset = number(arguments).with_context(|| format!("[MaxOffset] on {name} takes metres"))?.max(0.0);
    }
    if !info.surface && !info.vertex && !info.light && !info.unlit {
        bail!("{name} has no surface, vertex or light function");
    }
    for field in body.split(';').map(str::trim).filter(|field| !field.is_empty()) {
        if let Some(property) = property(field).with_context(|| format!("in {name}: `{field}`"))? {
            info.properties.push(property);
        }
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
/// by its number. Shaders that do not read are left out, so their
/// materials draw as the standard shader.
pub fn dispatch_code(registry: &ShaderRegistry) -> String {
    let shaders: Vec<(&ProjectShader, &ShaderInfo)> =
        registry.shaders.iter().filter_map(|shader| Some((shader, shader.info.as_ref().ok()?))).collect();
    let mut code = String::from("// Written by the engine from the project's .shader assets.\n\n#pragma once\n\n");
    for (shader, info) in &shaders {
        let number = shader.number;
        code += &format!("namespace shader_{number}\n{{\n#line 1 \"{}\"\n{}\n}}\n\n", shader.path, shader.source);
        code += &format!(
            "shader_{number}::{name} shader_{number}_properties(uint material)\n{{\n    \
             float4* values = buffers()->materialProperties + material * MATERIAL_PROPERTIES;\n    \
             shader_{number}::{name} properties;\n",
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
        code += "    return properties;\n}\n\n";
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
    fn an_unlit_shader_needs_no_function_and_a_shader_with_nothing_is_refused() {
        let unlit = read("[Unlit]\nstruct Glow { [Property, Color] float3 tint = 1.0; }").unwrap();
        assert!(unlit.unlit && !unlit.surface);
        assert_eq!(unlit.properties[0].default, [1.0, 1.0, 1.0, 0.0], "one value for every channel");
        assert!(read("struct Nothing { }").is_err());
        assert!(read("void surface(int a) {}").is_err(), "no properties struct");
        assert!(read("struct S { [Property] double x; }\nvoid surface(S s) {}").is_err(), "an unknown type");
    }

    #[test]
    fn shaders_are_numbered_by_path_from_one_and_one_that_does_not_read_is_kept_with_its_error() {
        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let registry = ShaderRegistry::new(vec![
            (guid(1), "Shaders/Water.shader".into(), WATER.into()),
            (guid(2), "Shaders/Broken.shader".into(), "struct Nothing { }".into()),
        ]);
        let numbers: Vec<(&str, u32)> = registry.shaders.iter().map(|s| (s.path.as_str(), s.number)).collect();
        assert_eq!(numbers, [("Shaders/Broken.shader", 1), ("Shaders/Water.shader", 2)]);
        assert!(registry.shaders[0].info.is_err());
        assert_eq!(registry.get(guid(1)).map(|(number, info)| (number, info.name.as_str())), Some((2, "Water")));
        assert!(registry.get(guid(2)).is_none(), "one that does not read draws as the standard shader");
    }

    #[test]
    fn the_dispatch_calls_each_shaders_functions_by_its_number() {
        let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
        let unlit = "[Unlit]\nstruct Glow { [Property, Color] float3 tint = 1.0; }";
        let registry = ShaderRegistry::new(vec![
            (guid(1), "Shaders/Water.shader".into(), WATER.into()),
            (guid(2), "Shaders/Glow.shader".into(), unlit.into()),
        ]);
        let code = dispatch_code(&registry);
        assert!(code.contains("namespace shader_2\n{\n#line 1 \"Shaders/Water.shader\""), "{code}");
        assert!(code.contains("properties.deep = values[0].xyz;"));
        assert!(code.contains("properties.ripples.index = asuint(values[2].x);"));
        assert!(code.contains("properties.foam = values[3].x > 0.5;"));
        assert!(code.contains("case 2:\n        shader_2::surface(shader_2_properties(material), input, surface);"));
        assert!(code.contains("case 2:\n        return shader_2::vertex("));
        assert!(code.contains("case 1:\n        return true;"), "Glow is unlit");
        assert!(!code.contains("shader_1::surface"), "Glow has no surface function");
    }

    #[test]
    fn a_function_named_in_a_comment_or_called_is_not_declared() {
        let info = read("// float3 light(...)\nstruct S { }\nvoid surface(S s) { float x = other.vertex(1); }").unwrap();
        assert!(info.surface && !info.light && !info.vertex);
    }
}
