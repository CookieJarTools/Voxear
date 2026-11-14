//! Post-processing as scripts reach it: `Post` in Voxear.Core reads the
//! level's settings as they are in effect and lays settings of its own over
//! them (`voxear_render::ScriptPost`), through the one call set here. Both
//! go as JSON laid out as a scene's `[post]` writes its settings.

use anyhow::{Context, Result, bail};
use voxear_ecs::World;
use voxear_render::{PostProcess, ScriptPost, WornLook};
use voxear_scripting::{POST_IN_EFFECT, POST_OVERRIDES, POST_RESET, POST_SET};

/// Sets what answers `Post` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_post_call(call);
}

fn call(world: &mut World, what: i32, text: &str) -> Result<String> {
    match what {
        POST_IN_EFFECT => Ok(serde_json::to_string(&in_effect(world))?),
        POST_OVERRIDES => {
            let overrides = world.resource::<ScriptPost>().map(|script| script.overrides().clone()).unwrap_or_default();
            Ok(serde_json::to_string(&overrides)?)
        }
        POST_SET => {
            let json: serde_json::Value = serde_json::from_str(text).context("reading the settings")?;
            let toml::Value::Table(overrides) = to_toml(&json).unwrap_or(toml::Value::Table(toml::Table::new())) else {
                bail!("the settings are not a table of sections");
            };
            set(world, overrides)?;
            Ok(String::new())
        }
        POST_RESET => {
            world.remove_resource::<ScriptPost>();
            Ok(String::new())
        }
        _ => bail!("no such call ({what})"),
    }
}

/// Lays `overrides`, as `[post]` writes settings, over the level's, in place
/// of any set before; none takes them all off.
pub fn set(world: &mut World, overrides: toml::Table) -> Result<()> {
    match overrides.is_empty() {
        true => drop(world.remove_resource::<ScriptPost>()),
        false => world.insert_resource(ScriptPost::new(overrides)?),
    }
    Ok(())
}

/// The level's post-processing as a frame is drawn with it, but for post
/// volumes, which are where the camera is: a look pack's where one is worn,
/// the level's own otherwise, and what scripts set over either.
pub fn in_effect(world: &World) -> PostProcess {
    let worn = world.resource::<WornLook>().map(|worn| worn.post);
    let level = worn.or_else(|| world.resource::<PostProcess>().copied()).unwrap_or_default();
    world.resource::<ScriptPost>().map_or(level, |script| script.over(level))
}

/// JSON as TOML: what is null is left out, as a setting not set.
fn to_toml(json: &serde_json::Value) -> Option<toml::Value> {
    use serde_json::Value;
    Some(match json {
        Value::Null => return None,
        Value::Bool(on) => toml::Value::Boolean(*on),
        Value::Number(number) => match number.as_i64() {
            Some(whole) => toml::Value::Integer(whole),
            None => toml::Value::Float(number.as_f64()?),
        },
        Value::String(text) => toml::Value::String(text.clone()),
        Value::Array(items) => toml::Value::Array(items.iter().filter_map(to_toml).collect()),
        Value::Object(fields) => {
            toml::Value::Table(fields.iter().filter_map(|(key, value)| Some((key.clone(), to_toml(value)?))).collect())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_and_lay_settings_over_the_level() {
        let mut world = World::new();
        world.insert_resource(PostProcess::cinematic());
        let seen: serde_json::Value = serde_json::from_str(&call(&mut world, POST_IN_EFFECT, "").unwrap()).unwrap();
        assert_eq!(seen["tonemap"]["mapper"], "aces", "the level's own");
        call(&mut world, POST_SET, r#"{"bloom":{"intensity":0.5,"tint":[1,0.5,0]},"fog":{"color":null}}"#).unwrap();
        let post = in_effect(&world);
        assert_eq!(post.bloom.intensity, 0.5);
        assert_eq!(post.bloom.tint, [1.0, 0.5, 0.0], "whole numbers read as numbers");
        assert_eq!(post.tonemap.mapper, voxear_render::Tonemapper::Aces, "the level's, where the script sets none");
        let written: serde_json::Value = serde_json::from_str(&call(&mut world, POST_OVERRIDES, "").unwrap()).unwrap();
        assert_eq!(written, serde_json::json!({"bloom": {"intensity": 0.5, "tint": [1, 0.5, 0]}, "fog": {}}));
        assert!(call(&mut world, POST_SET, r#"{"bloom":{"glow":1}}"#).is_err(), "a setting that does not exist");
        assert_eq!(in_effect(&world).bloom.intensity, 0.5, "kept after a refusal");
        call(&mut world, POST_RESET, "").unwrap();
        assert_eq!(in_effect(&world), PostProcess::cinematic());
    }
}
