//! Post-processing as scripts reach it: `Post` in Voxear.Core reads the
//! level's settings as they are in effect and lays settings of its own over
//! them (`voxear_render::ScriptPost`), through the one call set here. Both
//! go as JSON laid out as a scene's `[post]` writes its settings. The same
//! call answers `Lighting.Shadows`: how the sun's and lamps' shadows are
//! drawn, read as a frame draws them and set over the level's
//! (`voxear_render::ScriptShadows`).

use anyhow::{Context, Result, bail, ensure};
use voxear_ecs::World;
use voxear_render::{LampShadows, PostProcess, ScriptPost, ScriptShadows, ShadowsInEffect, SunShadows, WornLook};
use voxear_scripting::{
    LEVEL_COMPONENT, POST_IN_EFFECT, POST_OVERRIDES, POST_RESET, POST_SET, PROJECT_CHANGE, PROJECT_SECTION,
    SHADOWS_IN_EFFECT, SHADOWS_RESET, SHADOWS_SET, SKY_IN_EFFECT, SKY_RESET, SKY_SET, SUN_LOOK, SUN_LOOK_RESET,
    SUN_LOOK_SET, WORLD_ASK,
};

use crate::{
    lighting::{SunData, merge},
    look_packs::{ScriptLook, set_script_look},
    scene_file::{SkyData, sky_data},
};

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
        SHADOWS_IN_EFFECT => {
            let shadows = ShadowsInEffect::of(world);
            let changed = world.resource::<ScriptShadows>().is_some();
            Ok(serde_json::json!({
                "sun": shadows.sun.name(),
                "lamps": shadows.lamps.name(),
                "distance": shadows.distance,
                "split": shadows.split,
                "changed": changed,
            })
            .to_string())
        }
        SHADOWS_SET => {
            let json: serde_json::Value = serde_json::from_str(text).context("reading the shadows")?;
            set_shadows(world, &json)?;
            Ok(String::new())
        }
        SHADOWS_RESET => {
            world.remove_resource::<ScriptShadows>();
            Ok(String::new())
        }
        SKY_IN_EFFECT => Ok(sky_json(&sky_in_effect(world)).to_string()),
        SKY_SET => {
            let json: serde_json::Value = serde_json::from_str(text).context("reading the sky")?;
            set_sky(world, &json)?;
            Ok(String::new())
        }
        SKY_RESET => {
            let script = world.resource::<ScriptLook>().cloned().unwrap_or_default();
            set_script_look(world, ScriptLook::new(None, script.sun));
            Ok(String::new())
        }
        SUN_LOOK => {
            let sun = world.resource::<ScriptLook>().and_then(|script| script.sun).unwrap_or_default();
            Ok(serde_json::to_string(&sun)?)
        }
        SUN_LOOK_SET => {
            let json: serde_json::Value = serde_json::from_str(text).context("reading the sun")?;
            set_sun_look(world, &json)?;
            Ok(String::new())
        }
        SUN_LOOK_RESET => {
            let script = world.resource::<ScriptLook>().cloned().unwrap_or_default();
            set_script_look(world, ScriptLook::new(script.sky, None));
            Ok(String::new())
        }
        // Answered with nothing where the level has none.
        LEVEL_COMPONENT => Ok(crate::script_components::level_component(world, text)?.unwrap_or_default()),
        PROJECT_SECTION => crate::project_scripts::section(world, text),
        PROJECT_CHANGE => {
            crate::project_scripts::change(world, text)?;
            Ok(String::new())
        }
        WORLD_ASK => crate::roads::answer(world, text),
        _ => bail!("no such call ({what})"),
    }
}

/// The sky as a frame draws it, weather aside: a look pack's and what
/// scripts set where either is laid over the level's, the level's own
/// otherwise.
pub fn sky_in_effect(world: &World) -> voxear_render::Sky {
    crate::look_packs::in_effect(world).sky
}

/// A sky as scripts read it: every setting, colours sRGB, laid out as
/// `[sky]` writes them.
pub fn sky_json(sky: &voxear_render::Sky) -> serde_json::Value {
    let data = sky_data(sky);
    serde_json::json!({
        "mode": data.mode,
        "zenith": data.zenith,
        "horizon": data.horizon,
        "ground": data.ground,
        "ambient": data.ambient,
        "stars": data.stars,
        "haze": data.haze,
        "haze_tint": data.haze_tint,
        "haze_glow": data.haze_glow,
        "distance_haze": data.distance_haze,
        "sun_size": data.sun_size,
        "picture_turn": data.picture_turn,
        "clouds": data.clouds,
    })
}

/// Lays the sky's settings `json` gives, as `[sky]` writes them, over those
/// scripts set before. Nothing changes where any is refused: one there is
/// not, or the sky's picture, which is the level's own.
pub fn set_sky(world: &mut World, json: &serde_json::Value) -> Result<()> {
    let Some(toml::Value::Table(over)) = to_toml(json) else { bail!("the sky is not a table of settings") };
    ensure!(
        !over.contains_key("picture"),
        "the sky's picture is the level's own: apply lighting settings to change it"
    );
    let script = world.resource::<ScriptLook>().cloned().unwrap_or_default();
    let mut sky = script.sky.clone().unwrap_or_default();
    merge(&mut sky, &over);
    // Checked over the default sky, so every setting written is one there is.
    let mut table = toml::Table::try_from(sky_data(&voxear_render::Sky::default())).context("writing the sky")?;
    merge(&mut table, &sky);
    let _: SkyData = table.try_into().context("reading the sky")?;
    set_script_look(world, ScriptLook::new(Some(sky), script.sun));
    Ok(())
}

/// Sets how the sun looks over the level's, as `[sun]` writes it, in place
/// of what scripts set before: its temperature, tint, strength, softness,
/// shadow tint and canopy. Its shadows are `Lighting.Shadows`'.
pub fn set_sun_look(world: &mut World, json: &serde_json::Value) -> Result<()> {
    let fields = json.as_object().context("the sun is not a table of settings")?;
    const LOOKS: [&str; 6] = ["temperature", "tint", "strength", "softness", "shadow_tint", "canopy"];
    if let Some(other) = fields.keys().find(|key| !LOOKS.contains(&key.as_str())) {
        bail!("no sun setting `{other}`");
    }
    let sun: SunData = serde_json::from_value(json.clone()).context("reading the sun")?;
    let script = world.resource::<ScriptLook>().cloned().unwrap_or_default();
    let sun = (sun != SunData::default()).then_some(sun);
    set_script_look(world, ScriptLook::new(script.sky, sun));
    Ok(())
}

/// Lays the shadow settings `json` gives (`sun`, `lamps`, `distance` and
/// `split`) over those scripts set before; the rest stay as they were.
/// Nothing changes where any is refused.
pub fn set_shadows(world: &mut World, json: &serde_json::Value) -> Result<()> {
    let mut shadows = world.resource::<ScriptShadows>().copied().unwrap_or_default();
    let fields = json.as_object().context("the shadows are not a table of settings")?;
    for (key, value) in fields {
        match key.as_str() {
            "sun" => {
                let name = value.as_str().context("`sun` is not a name")?;
                shadows.sun = Some(SunShadows::named(name).with_context(|| format!("no sun shadows `{name}`"))?);
            }
            "lamps" => {
                let name = value.as_str().context("`lamps` is not a name")?;
                shadows.lamps = Some(LampShadows::named(name).with_context(|| format!("no lamp shadows `{name}`"))?);
            }
            "distance" => {
                shadows.distance =
                    Some((value.as_f64().context("`distance` is not a number")? as f32).clamp(5.0, 1000.0))
            }
            "split" => {
                shadows.split = Some((value.as_f64().context("`split` is not a number")? as f32).clamp(0.0, 1.0))
            }
            other => bail!("no shadow setting `{other}`"),
        }
    }
    world.insert_resource(shadows);
    Ok(())
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

    #[test]
    fn scripts_read_and_set_the_shadows_over_the_level() {
        let mut world = World::new();
        let mut sun = crate::lighting::plain_sun();
        sun.shadow_distance = 120.0;
        world.insert_resource(sun);
        let seen = |world: &mut World| -> serde_json::Value {
            serde_json::from_str(&call(world, SHADOWS_IN_EFFECT, "").unwrap()).unwrap()
        };
        let level = seen(&mut world);
        assert_eq!((level["sun"].as_str(), level["lamps"].as_str()), (Some("virtual"), Some("virtual")));
        assert_eq!((level["distance"].as_f64(), level["changed"].as_bool()), (Some(120.0), Some(false)));

        call(&mut world, SHADOWS_SET, r#"{"sun":"cascades","distance":60}"#).unwrap();
        call(&mut world, SHADOWS_SET, r#"{"lamps":"atlas"}"#).unwrap();
        let now = seen(&mut world);
        assert_eq!((now["sun"].as_str(), now["lamps"].as_str()), (Some("cascades"), Some("atlas")), "both kept");
        assert_eq!((now["distance"].as_f64(), now["changed"].as_bool()), (Some(60.0), Some(true)));
        assert!(call(&mut world, SHADOWS_SET, r#"{"sun":"soft"}"#).is_err(), "a way that does not exist");
        assert!(call(&mut world, SHADOWS_SET, r#"{"sharpness":1}"#).is_err(), "a setting that does not exist");
        assert_eq!(seen(&mut world)["sun"].as_str(), Some("cascades"), "kept after a refusal");

        call(&mut world, SHADOWS_RESET, "").unwrap();
        assert_eq!(seen(&mut world), level, "the level's again");
    }

    #[test]
    fn scripts_lay_the_sky_and_the_suns_look_over_the_levels_and_take_them_off() {
        let mut world = World::new();
        let sky = voxear_render::Sky { haze: 2.0, ..Default::default() };
        world.insert_resource(sky);
        world.insert_resource(crate::lighting::plain_sun());
        let seen = |world: &mut World| -> serde_json::Value {
            serde_json::from_str(&call(world, SKY_IN_EFFECT, "").unwrap()).unwrap()
        };
        let level = seen(&mut world);
        assert_eq!((level["haze"].as_f64(), level["mode"].as_str()), (Some(2.0), Some("gradient")));
        assert_eq!(level["clouds"]["kind"].as_str(), Some("soft"), "every setting, clouds and all");

        call(&mut world, SKY_SET, r#"{"mode":"atmosphere","clouds":{"cover":0.6}}"#).unwrap();
        call(&mut world, SKY_SET, r#"{"clouds":{"kind":"blocks"}}"#).unwrap();
        let now = seen(&mut world);
        assert_eq!((now["mode"].as_str(), now["haze"].as_f64()), (Some("atmosphere"), Some(2.0)), "the level's haze");
        let cover = now["clouds"]["cover"].as_f64().unwrap_or_default();
        assert!((cover - 0.6).abs() < 1e-6 && now["clouds"]["kind"] == "blocks", "{now}");
        assert!(call(&mut world, SKY_SET, r#"{"glow":1}"#).is_err(), "a setting that does not exist");
        assert!(call(&mut world, SKY_SET, r#"{"picture":"x"}"#).is_err(), "the level's picture");
        crate::look_packs::update(&mut world);
        let worn = world.resource::<WornLook>().expect("drawn with what scripts set");
        assert_eq!(
            (worn.sky.mode, worn.sky.clouds.kind),
            (voxear_render::SkyMode::Atmosphere, voxear_render::CloudKind::Blocks)
        );

        call(&mut world, SUN_LOOK_SET, r#"{"temperature":3000,"strength":2}"#).unwrap();
        assert!(call(&mut world, SUN_LOOK_SET, r#"{"shadows":"cascades"}"#).is_err(), "shadows are Lighting.Shadows'");
        let sun: serde_json::Value = serde_json::from_str(&call(&mut world, SUN_LOOK, "").unwrap()).unwrap();
        assert_eq!(sun, serde_json::json!({"temperature": 3000.0, "strength": 2.0}));
        crate::look_packs::update(&mut world);
        let worn = world.resource::<WornLook>().and_then(|worn| worn.sun).unwrap();
        assert_eq!(worn.strength, 2.0);
        assert!(worn.tint.z < worn.tint.x, "warm at 3000 K");

        call(&mut world, SKY_RESET, "").unwrap();
        assert_eq!(seen(&mut world), level, "the level's sky again");
        call(&mut world, SUN_LOOK_RESET, "").unwrap();
        crate::look_packs::update(&mut world);
        assert!(world.resource::<WornLook>().is_none(), "drawn as the level is");
    }
}
