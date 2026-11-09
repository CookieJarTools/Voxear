//! Shaders an element's background can be drawn with, named by its style:
//! `shader: shimmer`, one of the engine's built-in effects, or
//! `shader: url("Shaders/Glow.shader")`, a project's own UI shader by path
//! or GUID. A shader works out the colour of the background pixel by
//! pixel; the border, rounded corners and opacity stay as the style sets
//! them.
//!
//! The UI knows shaders only as [`UiShaders`] describes them: each one's
//! number, which the renderer's UI shader picks it by, and its properties,
//! whose values come from the element's custom properties of the same names
//! (`--speed: 2`), or else the shader's defaults. The engine adds a
//! project's own to the built-in effects ([`crate::Ui::set_shaders`]).

use std::collections::HashMap;

use cssparser::{Parser, ParserInput};

/// The first number the built-in effects are known by on the GPU; a
/// project's own shaders have numbers below it.
pub const BUILTIN_UI_SHADERS: u32 = 0x8000;

/// What one of a UI shader's properties holds, and so which of its four
/// numbers its value fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiShaderValue {
    Number,
    /// Two, three or four numbers.
    Vector(u8),
    /// A colour, made linear and premultiplied by its alpha.
    Color,
    /// On or off, as 1 or 0.
    Toggle,
}

/// One of a UI shader's properties: its name, without the `--` its custom
/// property is written with, what it holds, and its value when the element
/// gives none.
#[derive(Clone, Debug, PartialEq)]
pub struct UiShaderProperty {
    pub name: String,
    pub value: UiShaderValue,
    pub default: [f32; 4],
}

/// A shader the UI can draw a background with: the number the GPU picks it
/// by, and its properties in the order the GPU reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct UiShader {
    pub number: u32,
    pub properties: Vec<UiShaderProperty>,
}

/// The shaders a UI can draw with, by the names styles give them: the
/// engine's built-in effects, and whatever else is added.
#[derive(Clone, Debug)]
pub struct UiShaders {
    by_name: HashMap<String, UiShader>,
}

impl Default for UiShaders {
    /// The built-in effects, which every UI has.
    fn default() -> Self {
        let mut shaders = Self { by_name: HashMap::new() };
        let property = |name: &str, value, default| UiShaderProperty { name: name.to_string(), value, default };
        let number = |name: &str, default: f32| property(name, UiShaderValue::Number, [default, 0.0, 0.0, 0.0]);
        let color = |name: &str, [r, g, b, a]: [f32; 4]| {
            let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            property(name, UiShaderValue::Color, [linear(r) * a, linear(g) * a, linear(b) * a, a])
        };
        let effects = [
            ("shimmer", vec![number("speed", 0.6), number("width", 0.3), color("shine", [1.0, 1.0, 1.0, 0.35])]),
            (
                "stripes",
                vec![
                    color("stripe", [1.0, 1.0, 1.0, 0.16]),
                    number("spacing", 14.0),
                    number("speed", 24.0),
                    number("angle", 45.0),
                ],
            ),
            ("scanlines", vec![number("spacing", 3.0), number("strength", 0.25), number("flicker", 0.04)]),
            ("static", vec![number("amount", 0.12), number("speed", 24.0)]),
        ];
        for (index, (name, properties)) in effects.into_iter().enumerate() {
            shaders.insert(name, UiShader { number: BUILTIN_UI_SHADERS + index as u32, properties });
        }
        shaders
    }
}

impl UiShaders {
    /// Adds a shader by a name styles give it: a project's by its path, and
    /// again by its GUID.
    pub fn insert(&mut self, name: impl Into<String>, shader: UiShader) {
        self.by_name.insert(name.into(), shader);
    }

    pub fn get(&self, name: &str) -> Option<&UiShader> {
        self.by_name.get(name)
    }

    /// The built-in effects' names, in the order of their numbers.
    pub fn builtin_names() -> [&'static str; 4] {
        ["shimmer", "stripes", "scanlines", "static"]
    }
}

impl UiShader {
    /// Its properties' values for an element whose custom properties are
    /// `vars`, each as the GPU reads it; a property the element does not set,
    /// or sets to something that does not read, takes its default.
    pub fn values(&self, vars: &HashMap<String, String>) -> Vec<[f32; 4]> {
        let read = |property: &UiShaderProperty| {
            let written = vars.get(&format!("--{}", property.name))?;
            let written = crate::properties::substitute(written, vars, 0).ok()?;
            let mut input = ParserInput::new(&written);
            let mut parser = Parser::new(&mut input);
            let value = read_value(&mut parser, property.value)?;
            parser.expect_exhausted().ok()?;
            Some(value)
        };
        self.properties.iter().map(|property| read(property).unwrap_or(property.default)).collect()
    }
}

/// One value of `kind`, as the GPU reads it.
fn read_value(input: &mut Parser, kind: UiShaderValue) -> Option<[f32; 4]> {
    match kind {
        UiShaderValue::Number => Some([input.expect_number().ok()?, 0.0, 0.0, 0.0]),
        UiShaderValue::Toggle => {
            let on = match input.try_parse(|i| i.expect_number()) {
                Ok(number) => number != 0.0,
                Err(_) => input.expect_ident_matching("true").is_ok(),
            };
            Some([f32::from(u8::from(on)), 0.0, 0.0, 0.0])
        }
        UiShaderValue::Vector(count) => {
            let mut values = [0.0; 4];
            for slot in values.iter_mut().take(usize::from(count.clamp(2, 4))) {
                let _ = input.try_parse(|i| i.expect_comma());
                *slot = input.expect_number().ok()?;
            }
            Some(values)
        }
        UiShaderValue::Color => crate::properties::color(input).ok().map(|color| color.linear_premultiplied()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_elements_custom_properties_give_a_shaders_values_or_its_defaults() {
        let shaders = UiShaders::default();
        let stripes = shaders.get("stripes").expect("a built-in effect");
        assert_eq!(stripes.number, BUILTIN_UI_SHADERS + 1);
        let vars = HashMap::from([
            ("--spacing".to_string(), "20".to_string()),
            ("--stripe".to_string(), "var(--warning)".to_string()),
            ("--warning".to_string(), "#ffffff".to_string()),
            ("--speed".to_string(), "fast".to_string()),
        ]);
        let values = stripes.values(&vars);
        assert_eq!(values[0], [1.0, 1.0, 1.0, 1.0], "a colour, through a variable, made linear");
        assert_eq!(values[1][0], 20.0);
        assert_eq!(values[2][0], 24.0, "what does not read takes the default");
        assert_eq!(values[3][0], 45.0, "what is not set takes the default");
        assert!(shaders.get("glow").is_none());
    }
}
