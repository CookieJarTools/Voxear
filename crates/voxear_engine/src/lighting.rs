//! Lighting settings: a scene's sky, how its sun looks and its
//! post-processing, kept in a `.lighting` asset of their own, as Unity's
//! Lighting Settings asset. A scene names one, `lighting = "Lighting/Dusk.lighting"`,
//! and it is applied as the scene loads; scripts apply one with
//! `Lighting.Apply`. Whatever the scene sets in its own `[sky]` and `[post]`
//! wins over its lighting's.
//!
//! A lighting file can start from one of the built-in looks and change only
//! what it wants:
//!
//! ```toml
//! format = 1
//! look = "golden_hour"
//!
//! [sky]
//! haze = 2.5
//!
//! [sun]
//! temperature = 4500
//! strength = 1.2
//!
//! [post.bloom]
//! intensity = 0.2
//! ```
//!
//! Each look is also built in as lighting of its own: `builtin:golden_hour`,
//! `builtin:storybook` and `builtin:soft_overcast`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_render::{
    Look, LookName, Sky, SunLook, temperature_color,
    weather::{Weather, WeatherAmounts, WeatherKind},
};

use crate::{
    content::linear,
    scene_file::{SkyData, sky_data, sky_from_data},
};

/// The `.lighting` format this version writes and reads.
pub const LIGHTING_FORMAT: u32 = 1;

/// A `.lighting` file as written: a look to start from, if any, and the
/// sky, sun and post-processing settings that differ from it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightingFile {
    #[serde(default = "lighting_format")]
    pub format: u32,
    /// A built-in look to start from: `"golden_hour"`, `"storybook"` or
    /// `"soft_overcast"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// The sky's settings, as a scene's `[sky]` writes them, over the
    /// look's sky or the default one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sky: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun: Option<SunData>,
    /// Post-processing, as a scene's `[post]` writes it, over the look's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<toml::Table>,
    /// The weather the level is in once lit by these settings, as
    /// [`WeatherData`] writes it; clear when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<toml::Table>,
}

/// The weather lighting settings put a level in, as `[weather]` writes it:
/// a kind by name, or with `kind = "custom"` the amounts beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeatherData {
    pub kind: WeatherKind,
    pub cloud: f32,
    pub darkness: f32,
    pub fog: f32,
    pub wind: f32,
    pub rain: f32,
    pub snow: f32,
    pub lightning: f32,
}

impl WeatherData {
    /// The amounts written beside the kind, which a custom kind's are.
    pub fn amounts(&self) -> WeatherAmounts {
        let Self { cloud, darkness, fog, wind, rain, snow, lightning, .. } = *self;
        WeatherAmounts { cloud, darkness, fog, wind, rain, snow, lightning }
    }

    /// The weather it describes, settled.
    pub fn weather(&self) -> Weather {
        match self.kind {
            WeatherKind::Custom => Weather::custom(self.amounts()),
            kind => Weather::of(kind),
        }
    }
}

fn lighting_format() -> u32 {
    LIGHTING_FORMAT
}

/// How the level's sun looks, whatever light it is: what changes its colour
/// and strength, and what it gets where the light leaves its own at the
/// default. Each setting not written keeps the look's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SunData {
    /// Tints the sun to the colour of this temperature, in kelvin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// sRGB: tints the sun's colour, over its temperature's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[f32; 3]>,
    /// Multiplies the sun's intensity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,
    /// How wide the sun looks, in degrees, where the light keeps the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub softness: Option<f32>,
    /// sRGB: the colour its shadows take, where the light has none of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_tint: Option<[f32; 3]>,
    /// How far from the camera it casts shadows, in metres. Nearer keeps
    /// the shadows by the camera sharper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_distance: Option<f32>,
    /// How its four shadow cascades share that distance, from 0, evenly, to
    /// 1, the nearest small and sharp and the farthest wide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_split: Option<f32>,
}

impl SunData {
    /// Changes `sun` to what is written here, leaving the rest of it as it
    /// is: a temperature gives its tint, a tint colours that, and the
    /// others take the place of its own.
    pub fn lay_over(&self, sun: &mut SunLook) {
        if let Some(kelvin) = self.temperature {
            sun.tint = temperature_color(kelvin);
        }
        if let Some(tint) = self.tint {
            sun.tint *= Vec3::from(linear(tint));
        }
        sun.strength = self.strength.unwrap_or(sun.strength).max(0.0);
        sun.softness = self.softness.unwrap_or(sun.softness).clamp(0.0, 20.0);
        if let Some(shadow) = self.shadow_tint {
            sun.shadow_tint = Vec3::from(linear(shadow));
        }
        sun.shadow_distance = self.shadow_distance.unwrap_or(sun.shadow_distance).clamp(5.0, 1000.0);
        sun.shadow_split = self.shadow_split.unwrap_or(sun.shadow_split).clamp(0.0, 1.0);
    }
}

/// Lighting settings worked out: what a scene's environment starts from.
#[derive(Clone, Debug)]
pub struct Lighting {
    pub sky: Sky,
    pub sun: SunLook,
    /// Post-processing as a scene's `[post]` would write it, look and all.
    pub post: toml::Table,
    /// The weather the level is in once lit: clear where the settings write none.
    pub weather: WeatherData,
}

/// A sun look that changes nothing.
pub fn plain_sun() -> SunLook {
    SunLook {
        tint: Vec3::ONE,
        strength: 1.0,
        softness: 0.5,
        shadow_tint: Vec3::ZERO,
        shadow_distance: voxear_render::SHADOW_DISTANCE,
        shadow_split: voxear_render::SHADOW_SPLIT,
    }
}

impl LightingFile {
    /// One of the built-in looks as lighting of its own, by its name.
    pub fn builtin(name: &str) -> Result<Self> {
        LookName::parse(name)?;
        Ok(Self { look: Some(name.to_string()), ..Self::default() })
    }

    /// Its settings worked out over its look's.
    pub fn resolve(&self) -> Result<Lighting> {
        let look = self.look.as_deref().map(LookName::parse).transpose()?.map(Look::of);
        let mut sky = look.map_or_else(Sky::default, |look| look.sky);
        if let Some(over) = &self.sky {
            let mut table = toml::Table::try_from(sky_data(&sky)).context("writing the sky")?;
            merge(&mut table, over);
            let data: SkyData = table.try_into().context("reading the lighting's [sky]")?;
            sky = sky_from_data(&data);
        }
        let mut sun = look.map_or_else(plain_sun, |look| look.sun);
        if let Some(over) = self.sun {
            over.lay_over(&mut sun);
        }
        let mut post = toml::Table::new();
        if let Some(name) = &self.look {
            post.insert("look".into(), toml::Value::String(name.clone()));
        }
        if let Some(over) = &self.post {
            merge(&mut post, over);
        }
        let weather = match &self.weather {
            Some(table) => table.clone().try_into().context("reading the lighting's [weather]")?,
            None => WeatherData::default(),
        };
        Ok(Lighting { sky, sun, post, weather })
    }
}

/// Writes `over`'s settings into `base`, table by table, so a setting
/// `over` leaves out keeps `base`'s.
pub fn merge(base: &mut toml::Table, over: &toml::Table) {
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(more)) => merge(inner, more),
            _ => drop(base.insert(key.clone(), value.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_starts_from_its_look_and_changes_only_what_it_writes() {
        let text = "format = 1\nlook = \"golden_hour\"\n[sky]\nhaze = 2.5\n[sun]\nstrength = 2.0\n[post.bloom]\nintensity = 0.3";
        let file: LightingFile = toml::from_str(text).unwrap();
        let lighting = file.resolve().unwrap();
        let golden = Look::of(LookName::GoldenHour);
        assert_eq!(lighting.sky.haze, 2.5, "the lighting's own haze");
        assert_eq!(lighting.sky.mode, golden.sky.mode, "the look's sky otherwise");
        assert!((lighting.sky.distance_haze - golden.sky.distance_haze).abs() < 1e-4);
        assert_eq!((lighting.sun.strength, lighting.sun.softness), (2.0, golden.sun.softness));
        assert_eq!(lighting.post["look"].as_str(), Some("golden_hour"));
        assert_eq!(lighting.post["bloom"]["intensity"].as_float(), Some(0.3));
    }

    #[test]
    fn lighting_sets_the_weather_by_kind_or_by_amounts_and_is_clear_without() {
        let plain: LightingFile = toml::from_str("look = \"golden_hour\"").unwrap();
        assert_eq!(plain.resolve().unwrap().weather.weather(), Weather::default(), "clear unless it says");
        let rainy: LightingFile = toml::from_str("[weather]\nkind = \"rain\"").unwrap();
        let rain = rainy.resolve().unwrap().weather.weather();
        assert_eq!((rain.kind(), rain.now()), (WeatherKind::Rain, WeatherKind::Rain.amounts()));
        assert!(rain.settled(), "at once");
        let own: LightingFile = toml::from_str("[weather]\nkind = \"custom\"\nfog = 0.8\nsnow = 0.25").unwrap();
        let custom = own.resolve().unwrap().weather.weather();
        assert_eq!((custom.now().fog, custom.now().snow, custom.now().rain), (0.8, 0.25, 0.0));
        // A kind's own amounts are its own, whatever is written beside it.
        let both: LightingFile = toml::from_str("[weather]\nkind = \"fog\"\nrain = 1.0").unwrap();
        assert_eq!(both.resolve().unwrap().weather.weather().now().rain, 0.0);
        let misspelt: LightingFile = toml::from_str("[weather]\nkind = \"drizzle\"").unwrap();
        assert!(misspelt.resolve().is_err());
    }

    #[test]
    fn lighting_changes_its_looks_clouds_one_setting_at_a_time() {
        let highland = Look::of(LookName::HighlandSummer).sky.clouds;
        assert!(highland.cover > 0.0, "the look has clouds");
        let file: LightingFile = toml::from_str("look = \"highland_summer\"\n[sky.clouds]\ndrift = 30.0").unwrap();
        let clouds = file.resolve().unwrap().sky.clouds;
        assert_eq!((clouds.drift, clouds.cover, clouds.height), (30.0, highland.cover, highland.height));
        // A clear sky under a look.
        let clear: LightingFile = toml::from_str("look = \"highland_summer\"\n[sky.clouds]\ncover = 0.0").unwrap();
        assert!(clear.resolve().unwrap().sky.clouds.is_none());
        assert!(Look::of(LookName::CandlelitCastle).sky.clouds.is_none(), "none indoors");
    }

    #[test]
    fn a_sun_temperature_tints_it_and_builtin_lighting_is_a_look() {
        let file: LightingFile = toml::from_str("[sun]\ntemperature = 1900").unwrap();
        let sun = file.resolve().unwrap().sun;
        assert!(sun.tint.x > sun.tint.z * 3.0, "a candle-warm sun: {}", sun.tint);
        let builtin = LightingFile::builtin("storybook").unwrap().resolve().unwrap();
        assert_eq!(builtin.sun, Look::of(LookName::Storybook).sun);
        assert!(LightingFile::builtin("gloomy").is_err());
    }

    #[test]
    fn the_sun_s_shadows_reach_as_far_as_the_lighting_says() {
        let plain = LightingFile::default().resolve().unwrap().sun;
        let defaults = (voxear_render::SHADOW_DISTANCE, voxear_render::SHADOW_SPLIT);
        assert_eq!((plain.shadow_distance, plain.shadow_split), defaults);
        let file: LightingFile = toml::from_str("[sun]\nshadow_distance = 40.0\nshadow_split = 0.6").unwrap();
        let sun = file.resolve().unwrap().sun;
        assert_eq!((sun.shadow_distance, sun.shadow_split), (40.0, 0.6));
        let wild: LightingFile = toml::from_str("[sun]\nshadow_distance = -3.0\nshadow_split = 4.0").unwrap();
        let sun = wild.resolve().unwrap().sun;
        assert_eq!((sun.shadow_distance, sun.shadow_split), (5.0, 1.0), "kept to what works");
    }

    #[test]
    fn merging_keeps_what_the_override_leaves_out() {
        let mut base: toml::Table =
            toml::from_str("look = \"storybook\"\n[bloom]\nintensity = 0.1\nscatter = 0.8").unwrap();
        merge(&mut base, &toml::from_str("[bloom]\nintensity = 0.4").unwrap());
        assert_eq!(base["bloom"]["intensity"].as_float(), Some(0.4));
        assert_eq!(base["bloom"]["scatter"].as_float(), Some(0.8));
        assert_eq!(base["look"].as_str(), Some("storybook"));
    }
}
