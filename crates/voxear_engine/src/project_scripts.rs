//! Project Settings as scripts reach them (`ProjectSettings` in
//! Voxear.Core): each section the game started with, read as JSON laid out
//! as `voxear.toml` writes it, through the `Post` call. A game's options
//! change the portals', the water trails' and navigation's while it runs,
//! from the next frame; the time steps, the floating origin, the quality
//! budgets and the network are fixed once the game has started, and read
//! only.

use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use voxear_ecs::World;

use crate::{
    PortalSettings, ProjectFile, QualitySettings, TimeSettings, WaterTrailSettings, navigation::NavigationSettings,
    net::NetworkSettings, origin::OriginSettings,
};

/// The project's settings the game runs by, as a world resource, with what
/// scripts changed of those they may.
#[derive(Clone, Debug)]
pub struct ProjectInEffect {
    pub time: TimeSettings,
    pub floating_origin: OriginSettings,
    pub quality: QualitySettings,
    pub portals: PortalSettings,
    pub water_trails: WaterTrailSettings,
    pub network: NetworkSettings,
    pub navigation: NavigationSettings,
}

impl ProjectInEffect {
    pub fn of(file: &ProjectFile) -> Self {
        Self {
            time: file.time,
            floating_origin: file.floating_origin.clone(),
            quality: file.quality.clone(),
            portals: file.portals,
            water_trails: file.water_trails,
            network: file.network.clone(),
            navigation: file.navigation.clone(),
        }
    }
}

/// The section `name` names, as JSON with every setting.
pub fn section(world: &World, name: &str) -> Result<String> {
    let settings = world.resource::<ProjectInEffect>().context("the game has not started")?;
    let value = match name {
        "time" => serde_json::to_value(settings.time)?,
        "floating_origin" => serde_json::to_value(&settings.floating_origin)?,
        "quality" => serde_json::to_value(&settings.quality)?,
        "portals" => serde_json::to_value(settings.portals)?,
        "water_trails" => serde_json::to_value(settings.water_trails)?,
        "network" => serde_json::to_value(&settings.network)?,
        "navigation" => serde_json::to_value(&settings.navigation)?,
        other => bail!("no section `{other}` of Project Settings"),
    };
    Ok(value.to_string())
}

/// Changes the settings `json` gives of a section scripts may change,
/// `{"section": "portals", "fields": {...}}`, and puts them where the game
/// reads them. Nothing changes where any is refused.
pub fn change(world: &mut World, json: &str) -> Result<()> {
    let asked: serde_json::Value = serde_json::from_str(json).context("reading the settings")?;
    let name = asked.get("section").and_then(|name| name.as_str()).context("no section named")?;
    let fields = asked.get("fields").context("no settings given")?;
    let mut settings = world.resource::<ProjectInEffect>().cloned().context("the game has not started")?;
    match name {
        "portals" => {
            settings.portals = laid_over(&settings.portals, fields)?;
            settings.portals.put(world);
        }
        "water_trails" => {
            settings.water_trails = laid_over(&settings.water_trails, fields)?;
            settings.water_trails.put(world);
        }
        "navigation" => {
            settings.navigation = laid_over(&settings.navigation, fields)?;
            settings.navigation.put(world);
        }
        "time" | "floating_origin" | "quality" | "network" => {
            bail!("Project Settings' {name} are fixed once the game has started")
        }
        other => bail!("no section `{other}` of Project Settings"),
    }
    world.insert_resource(settings);
    Ok(())
}

/// `settings` with `fields` written over them.
fn laid_over<T: Serialize + DeserializeOwned>(settings: &T, fields: &serde_json::Value) -> Result<T> {
    let mut now = serde_json::to_value(settings)?;
    let (Some(now), Some(fields)) = (now.as_object_mut(), fields.as_object()) else {
        bail!("the settings are not a table of them")
    };
    for (key, value) in fields {
        now.insert(key.clone(), value.clone());
    }
    serde_json::from_value(serde_json::Value::Object(now.clone())).context("changing the settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_every_section_and_change_only_those_a_running_game_can() {
        let mut world = World::new();
        assert!(section(&world, "portals").is_err(), "not before the game starts");
        let file: ProjectFile = toml::from_str("[project]\nname = \"Test\"").unwrap();
        world.insert_resource(ProjectInEffect::of(&file));
        let portals: serde_json::Value = serde_json::from_str(&section(&world, "portals").unwrap()).unwrap();
        assert!(portals["view_distance"].as_f64().is_some(), "{portals}");
        let time: serde_json::Value = serde_json::from_str(&section(&world, "time").unwrap()).unwrap();
        assert!(time["fixed_step"].as_f64().is_some(), "{time}");

        change(&mut world, r#"{"section":"portals","fields":{"view_distance":40}}"#).unwrap();
        let quality = world.resource::<voxear_render::portals::PortalQuality>().unwrap();
        assert_eq!(quality.view_distance, 40.0, "where the renderer reads it");
        change(&mut world, r#"{"section":"navigation","fields":{"road_cost":0.25}}"#).unwrap();
        assert_eq!(world.resource::<ProjectInEffect>().unwrap().navigation.road_cost, 0.25);
        change(&mut world, r#"{"section":"water_trails","fields":{"quality":"low"}}"#).unwrap();
        assert!(change(&mut world, r#"{"section":"time","fields":{"fixed_step":0.01}}"#).is_err(), "fixed");
        assert!(change(&mut world, r#"{"section":"portals","fields":{"glow":1}}"#).is_err(), "no such setting");
        assert_eq!(world.resource::<ProjectInEffect>().unwrap().portals.view_distance, 40.0, "kept");
    }
}
