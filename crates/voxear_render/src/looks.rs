//! Lighting looks: the sky, the sun and the post-processing chosen together
//! for a mood, so a scene starts from a finished look and changes only what
//! it wants. A scene picks one in its post-processing, `[post] look =
//! "storybook"`; whatever the scene sets itself, its own `[sky]` or any
//! `[post]` setting, wins over the look's.
//!
//! - **Soft Overcast**: hazy, bright and calm, like daylight through big
//!   windows on a cloudy day. The sky is the main light and the sun weak and
//!   warm, with soft shadows, light shafts in thin blue mist, bounced light,
//!   and a soft filmic finish.
//! - **Storybook**: a warm sun breaking through a canopy into a cool forest,
//!   with rich colour and soft shapes, as in It Takes Two, Kena or Ori.
//!   Art-directed rather than accurate: shadows tinted warm, teal mist that
//!   layers the scene, tinted occlusion and golden highlights.
//! - **Golden Hour**: a low, warm sun in thick, dusty air, as in Arc Raiders:
//!   the haze glows round the sun and swallows the distance in warm light,
//!   light shafts cut through it, shadows are lifted to a warm brown rather
//!   than black, and the grade pushes highlights to cream and shade towards
//!   teal, with a soft glare and fringing as through a real lens.
//!
//! After Arc Raiders' places: **Overgrown Dam** (humid green-gold haze over
//! lush ruins), **Sand-Buried City** (a harsh sun in thick orange dust),
//! **Rusted Spaceport** (flat, cool industrial overcast), **Cold Mountain
//! Pass** (a crisp sun over blue valley haze), **Snowbound Peak** (white,
//! cold and bright), **Night Raid** (moonlight and lamps in thin mist) and
//! **Electrical Storm** (a dark green-grey sky and heavy air).
//!
//! After Hogwarts Legacy's: **Candlelit Castle** (warm window light and
//! dust in dark halls), **Dark Forest** (teal gloom cut by shafts), **Snowy
//! Village** (soft winter light, warm windows glowing), **Highland Summer**
//! (bright, rich and painterly) and **Enchanted Night** (a violet moonlit
//! sky full of stars).

use anyhow::{Result, bail};
use voxear_core::math::Vec3;

use crate::{
    post::{
        AmbientOcclusion, Bloom, Exposure, Fog, GlobalIllumination, Grading, PostProcess, Tonemap, Tonemapper,
        Vignette,
    },
    scene::{Clouds, Sky, SkyMode, temperature_color},
    shadows::{SHADOW_DISTANCE, SHADOW_SPLIT},
};

/// A look by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookName {
    SoftOvercast,
    Storybook,
    GoldenHour,
    OvergrownDam,
    SandBuriedCity,
    RustedSpaceport,
    ColdMountainPass,
    SnowboundPeak,
    NightRaid,
    ElectricalStorm,
    CandlelitCastle,
    DarkForest,
    SnowyVillage,
    HighlandSummer,
    EnchantedNight,
}

impl LookName {
    /// Every look, in the order menus list them, each with the name files
    /// write and the name people read.
    pub const ALL: [(LookName, &'static str, &'static str); 15] = [
        (LookName::GoldenHour, "golden_hour", "Golden Hour"),
        (LookName::Storybook, "storybook", "Storybook"),
        (LookName::SoftOvercast, "soft_overcast", "Soft Overcast"),
        (LookName::OvergrownDam, "overgrown_dam", "Overgrown Dam"),
        (LookName::SandBuriedCity, "sand_buried_city", "Sand-Buried City"),
        (LookName::RustedSpaceport, "rusted_spaceport", "Rusted Spaceport"),
        (LookName::ColdMountainPass, "cold_mountain_pass", "Cold Mountain Pass"),
        (LookName::SnowboundPeak, "snowbound_peak", "Snowbound Peak"),
        (LookName::NightRaid, "night_raid", "Night Raid"),
        (LookName::ElectricalStorm, "electrical_storm", "Electrical Storm"),
        (LookName::CandlelitCastle, "candlelit_castle", "Candlelit Castle"),
        (LookName::DarkForest, "dark_forest", "Dark Forest"),
        (LookName::SnowyVillage, "snowy_village", "Snowy Village"),
        (LookName::HighlandSummer, "highland_summer", "Highland Summer"),
        (LookName::EnchantedNight, "enchanted_night", "Enchanted Night"),
    ];

    pub fn parse(name: &str) -> Result<Self> {
        match Self::ALL.iter().find(|(_, key, _)| *key == name) {
            Some(&(look, ..)) => Ok(look),
            None => {
                let known: Vec<&str> = Self::ALL.iter().map(|(_, key, _)| *key).collect();
                bail!("no look `{name}` ({} exist)", known.join(", "))
            }
        }
    }

    /// The name files write, as `look = "golden_hour"`.
    pub fn key(self) -> &'static str {
        Self::ALL.iter().find(|(look, ..)| *look == self).map_or("", |(_, key, _)| key)
    }

    /// The name menus show.
    pub fn title(self) -> &'static str {
        Self::ALL.iter().find(|(look, ..)| *look == self).map_or("", |(.., title)| title)
    }
}

/// What a look does to the level's sun, as a world resource: it changes
/// the first directional light's colour and strength, and gives it a
/// softness and a shadow tint unless the light has its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunLook {
    /// Linear, multiplying the light's colour.
    pub tint: Vec3,
    /// Multiplying the light's intensity.
    pub strength: f32,
    /// The sun's width in degrees, where the light keeps the default.
    pub softness: f32,
    /// Linear, where the light has none of its own.
    pub shadow_tint: Vec3,
    /// How far from the camera the sun casts shadows, in metres, and how
    /// its four cascades share that (see `SHADOW_SPLIT`).
    pub shadow_distance: f32,
    pub shadow_split: f32,
}

/// What the level is drawn with while a look pack is worn, as a world
/// resource: its own sky, sun look and post-processing with the pack's
/// laid over them, and what the pack sets of how blocks' faces are drawn.
/// The level's own stay in the world as they are, for the editor and
/// scripts to read and change; a frame is drawn with these in their place.
/// The engine works them out whenever the level's or the pack changes
/// (`voxear_engine::look_packs`), and takes the resource away with the
/// pack.
#[derive(Clone, Copy, Debug)]
pub struct WornLook {
    pub sky: Sky,
    /// None where neither the level nor the pack changes the sun.
    pub sun: Option<SunLook>,
    pub post: PostProcess,
    /// How many small cubes every world of blocks' faces are drawn as, and
    /// how deep; none for each world's own.
    pub voxels: Option<crate::blocks::BlockVoxels>,
    /// What the pack sets of how water looks, wherever it is.
    pub water: WaterLook,
}

/// What a look pack sets of how water looks: the sea, every Pool of water
/// and the water of worlds of blocks. Each is the water's own where the
/// pack sets none. Colours are sRGB, as an Ocean's and a Pool's are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterLook {
    /// The water's own colour: what it fades into where it is deep.
    pub deep: Option<[f32; 3]>,
    /// The colour light takes through the thin tops of waves and ripples.
    pub scatter: Option<[f32; 3]>,
    /// What a metre of water lets through, by colour.
    pub clarity: Option<[f32; 3]>,
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: Option<f32>,
    /// How much foam the sea's breaking crests have.
    pub foam: Option<f32>,
}

impl WaterLook {
    /// The sea as it looks under the pack.
    pub fn over_ocean(&self, ocean: crate::Ocean) -> crate::Ocean {
        crate::Ocean {
            deep: self.deep.unwrap_or(ocean.deep),
            scatter: self.scatter.unwrap_or(ocean.scatter),
            clarity: self.clarity.unwrap_or(ocean.clarity),
            murk: self.murk.unwrap_or(ocean.murk),
            foam: self.foam.unwrap_or(ocean.foam),
            ..ocean
        }
    }

    /// A Pool as it looks under the pack: one of water, as lava keeps its own.
    pub fn over_pool(&self, pool: crate::Pool) -> crate::Pool {
        if pool.liquid != crate::Liquid::Water {
            return pool;
        }
        crate::Pool {
            deep: self.deep.unwrap_or(pool.deep),
            scatter: self.scatter.unwrap_or(pool.scatter),
            clarity: self.clarity.unwrap_or(pool.clarity),
            murk: self.murk.unwrap_or(pool.murk),
            ..pool
        }
    }

    /// The colour worlds of blocks' water is drawn in under the pack,
    /// linear, with 1 after it; all 0 for each block's own.
    pub fn block_water(&self) -> [f32; 4] {
        let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        self.deep.map_or([0.0; 4], |[r, g, b]| [linear(r), linear(g), linear(b), 1.0])
    }
}

/// A look's settings.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub post: PostProcess,
    pub sky: Sky,
    pub sun: SunLook,
}

/// sRGB to linear, for colours written as an artist picks them.
fn linear(srgb: [f32; 3]) -> Vec3 {
    let channel = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    Vec3::new(channel(srgb[0]), channel(srgb[1]), channel(srgb[2]))
}

impl Look {
    pub fn of(name: LookName) -> Self {
        match name {
            LookName::SoftOvercast => Self::soft_overcast(),
            LookName::Storybook => Self::storybook(),
            LookName::GoldenHour => Self::golden_hour(),
            LookName::OvergrownDam => Self::overgrown_dam(),
            LookName::SandBuriedCity => Self::sand_buried_city(),
            LookName::RustedSpaceport => Self::rusted_spaceport(),
            LookName::ColdMountainPass => Self::cold_mountain_pass(),
            LookName::SnowboundPeak => Self::snowbound_peak(),
            LookName::NightRaid => Self::night_raid(),
            LookName::ElectricalStorm => Self::electrical_storm(),
            LookName::CandlelitCastle => Self::candlelit_castle(),
            LookName::DarkForest => Self::dark_forest(),
            LookName::SnowyVillage => Self::snowy_village(),
            LookName::HighlandSummer => Self::highland_summer(),
            LookName::EnchantedNight => Self::enchanted_night(),
        }
    }

    fn golden_hour() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.6, max_ev: 3.0, ..base.exposure },
            // Soft highlights that roll off to cream rather than clip, at a gentle contrast.
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.2, saturation: 1.25 },
            // The low sun through the air warms the light already: the grade only lifts the shade a little towards
            // teal against it, and pushes highlights to cream.
            grading: Grading {
                temperature: 0.04,
                saturation: 1.05,
                lift: [0.004, 0.007, 0.009],
                gain: [1.04, 1.0, 0.95],
                ..base.grading
            },
            // A wide glare round the sun and bright edges.
            bloom: Bloom { intensity: 0.16, scatter: 0.92, tint: [1.0, 0.93, 0.82], ..base.bloom },
            vignette: Vignette { intensity: 0.28, ..base.vignette },
            // Thick, low dust that the sun's shafts cut through, glowing towards the sun.
            fog: Fog {
                density: 0.005,
                height_falloff: 0.08,
                sun_glow: 0.9,
                volumetric: true,
                volumetric_distance: 96.0,
                anisotropy: 0.75,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.2, 0.12, 0.08], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.15, ..base.global_illumination },
            chromatic_aberration: 0.1,
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Atmosphere,
            ambient: 1.0,
            // Dusty air: twice a clear day's haze, a little warm, glowing tightly round the sun.
            haze: 1.5,
            haze_tint: linear([1.0, 0.96, 0.9]),
            haze_glow: 0.85,
            distance_haze: 16.0,
            sun_size: 0.8,
            // High, scattered clouds for the low sun to catch.
            clouds: Clouds { height: 1800.0, ..Clouds::covering(0.35) },
            ..Sky::default()
        };
        // The air reddens a low sun already; a touch warmer than white keeps it golden as it climbs. Warm brown in
        // its shadows.
        let sun = SunLook {
            tint: temperature_color(5000.0),
            strength: 1.4,
            softness: 1.0,
            shadow_tint: linear([0.12, 0.09, 0.07]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn soft_overcast() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            // Narrow, for interiors, so bright windows blow out and bloom into the frame.
            exposure: Exposure { compensation: 0.3, min_ev: -1.0, max_ev: 1.5, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.0, saturation: 1.0 },
            grading: Grading {
                saturation: 0.9,
                // Shadows a little towards teal, highlights a little warm.
                lift: [-0.01, 0.005, 0.015],
                gain: [1.03, 1.0, 0.97],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.06, scatter: 0.85, ..base.bloom },
            vignette: Vignette { intensity: 0.15, ..base.vignette },
            fog: Fog { density: 0.006, color: Some([0.78, 0.83, 0.9]), volumetric: true, anisotropy: 0.7, ..base.fog },
            ambient_occlusion: AmbientOcclusion { strength: 0.8, ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.66, 0.74, 0.86]),
            horizon: linear([0.83, 0.87, 0.92]),
            ground: linear([0.5, 0.49, 0.47]),
            ambient: 1.6,
            // A sky all but full of cloud. Its light is an overcast sky's already, so the clouds take no more of
            // the sun.
            clouds: Clouds { shadows: 0.0, ..Clouds::covering(0.9) },
            ..Sky::default()
        };
        // About 5200K, giving a few times the sky's light rather than ten.
        let sun = SunLook {
            tint: linear([1.0, 0.93, 0.83]),
            strength: 0.6,
            softness: 3.0,
            shadow_tint: Vec3::ZERO,
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn storybook() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            grading: Grading {
                saturation: 1.15,
                // Shadows lifted towards teal, highlights pushed towards gold.
                lift: [0.0, 0.02, 0.03],
                gain: [1.05, 1.0, 0.9],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.06, scatter: 0.8, tint: [1.0, 0.92, 0.78], ..base.bloom },
            vignette: Vignette { intensity: 0.25, ..base.vignette },
            fog: Fog { density: 0.012, height_falloff: 0.08, color: Some([0.48, 0.72, 0.72]), ..base.fog },
            ambient_occlusion: AmbientOcclusion { strength: 1.3, tint: [0.32, 0.2, 0.3], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.2, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.38, 0.64, 0.68]),
            horizon: linear([0.66, 0.8, 0.74]),
            ground: linear([0.45, 0.34, 0.24]),
            ambient: 1.2,
            // Fair-weather clouds.
            clouds: Clouds::covering(0.4),
            ..Sky::default()
        };
        // About 4000K and pushed towards yellow-green, with shadows never black but warm.
        let sun = SunLook {
            tint: linear([1.0, 0.9, 0.62]),
            strength: 1.2,
            softness: 2.0,
            shadow_tint: linear([0.42, 0.3, 0.38]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }
    // ---------------------------------------------------------------- After Arc Raiders' places

    fn overgrown_dam() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.3, max_ev: 3.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.1, saturation: 1.15 },
            grading: Grading {
                saturation: 1.05,
                lift: [0.0, 0.008, 0.006],
                gain: [1.02, 1.02, 0.94],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.12, scatter: 0.9, tint: [1.0, 0.97, 0.85], ..base.bloom },
            vignette: Vignette { intensity: 0.2, ..base.vignette },
            fog: Fog {
                density: 0.006,
                height_falloff: 0.1,
                color: Some([0.72, 0.78, 0.62]),
                sun_glow: 0.7,
                volumetric: true,
                anisotropy: 0.65,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.12, 0.16, 0.08], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.1, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Atmosphere,
            ambient: 1.0,
            haze: 2.0,
            haze_tint: linear([0.95, 1.0, 0.86]),
            haze_glow: 0.75,
            distance_haze: 24.0,
            // Heaped clouds in the humid air.
            clouds: Clouds::covering(0.5),
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(5200.0),
            strength: 1.3,
            softness: 1.0,
            shadow_tint: linear([0.08, 0.11, 0.07]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn sand_buried_city() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: -0.2, max_ev: 2.5, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.15, saturation: 1.05 },
            grading: Grading {
                temperature: 0.08,
                contrast: 1.05,
                saturation: 0.95,
                lift: [0.012, 0.006, 0.0],
                gain: [1.05, 0.99, 0.9],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.14, scatter: 0.92, tint: [1.0, 0.9, 0.75], ..base.bloom },
            vignette: Vignette { intensity: 0.25, ..base.vignette },
            fog: Fog {
                density: 0.012,
                height_falloff: 0.06,
                color: Some([0.86, 0.68, 0.48]),
                sun_glow: 0.95,
                volumetric: true,
                anisotropy: 0.8,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.22, 0.13, 0.07], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.2, ..base.global_illumination },
            chromatic_aberration: 0.12,
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Atmosphere,
            ambient: 0.9,
            haze: 3.0,
            haze_tint: linear([1.0, 0.84, 0.64]),
            haze_glow: 0.9,
            distance_haze: 30.0,
            sun_size: 1.2,
            // A few thin clouds, high over the dust.
            clouds: Clouds { height: 2500.0, thickness: 400.0, density: 0.6, ..Clouds::covering(0.15) },
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(5600.0),
            strength: 1.6,
            softness: 0.6,
            shadow_tint: linear([0.16, 0.1, 0.06]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn rusted_spaceport() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.2, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.2, saturation: 0.95 },
            grading: Grading {
                temperature: -0.04,
                saturation: 0.85,
                lift: [-0.005, 0.006, 0.012],
                gain: [1.02, 1.0, 0.97],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.06, scatter: 0.8, ..base.bloom },
            vignette: Vignette { intensity: 0.22, ..base.vignette },
            fog: Fog {
                density: 0.01,
                height_falloff: 0.05,
                color: Some([0.62, 0.66, 0.68]),
                sun_glow: 0.3,
                volumetric: true,
                anisotropy: 0.4,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { strength: 1.2, ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.55, 0.6, 0.64]),
            horizon: linear([0.72, 0.74, 0.74]),
            ground: linear([0.36, 0.33, 0.3]),
            ambient: 1.4,
            // Overcast, the light already what it lets through.
            clouds: Clouds { shadows: 0.0, ..Clouds::covering(0.95) },
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(6800.0),
            strength: 0.5,
            softness: 4.0,
            shadow_tint: Vec3::ZERO,
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn cold_mountain_pass() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.1, max_ev: 3.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.15, saturation: 1.1 },
            grading: Grading {
                temperature: -0.05,
                saturation: 1.05,
                lift: [-0.004, 0.004, 0.016],
                gain: [1.04, 1.0, 0.96],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.1, scatter: 0.88, ..base.bloom },
            vignette: Vignette { intensity: 0.18, ..base.vignette },
            fog: Fog {
                density: 0.004,
                height_falloff: 0.12,
                color: Some([0.62, 0.74, 0.9]),
                sun_glow: 0.6,
                volumetric: true,
                anisotropy: 0.6,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.06, 0.09, 0.16], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Atmosphere,
            ambient: 1.1,
            haze: 1.2,
            haze_tint: linear([0.84, 0.92, 1.0]),
            haze_glow: 0.7,
            distance_haze: 40.0,
            // A few clouds in clear air.
            clouds: Clouds::covering(0.3),
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(4800.0),
            strength: 1.4,
            softness: 0.8,
            shadow_tint: linear([0.08, 0.12, 0.22]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn snowbound_peak() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: -0.3, min_ev: -2.0, max_ev: 1.5, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.05, saturation: 0.95 },
            grading: Grading {
                temperature: -0.07,
                saturation: 0.85,
                lift: [0.0, 0.006, 0.014],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.1, scatter: 0.9, tint: [0.92, 0.96, 1.0], ..base.bloom },
            vignette: Vignette { intensity: 0.15, ..base.vignette },
            fog: Fog {
                density: 0.015,
                height_falloff: 0.06,
                color: Some([0.86, 0.9, 0.95]),
                sun_glow: 0.4,
                volumetric: true,
                anisotropy: 0.5,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { strength: 0.8, tint: [0.06, 0.1, 0.18], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.2, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.72, 0.8, 0.9]),
            horizon: linear([0.9, 0.92, 0.95]),
            ground: linear([0.82, 0.84, 0.88]),
            ambient: 2.0,
            // Broken cloud.
            clouds: Clouds::covering(0.55),
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(7000.0),
            strength: 0.7,
            softness: 4.0,
            shadow_tint: Vec3::ZERO,
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn night_raid() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.2, max_ev: 6.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.2, saturation: 1.05 },
            grading: Grading {
                temperature: -0.1,
                saturation: 0.9,
                lift: [0.0, 0.004, 0.012],
                gain: [0.96, 1.0, 1.06],
                ..base.grading
            },
            // Lamps glow through the mist.
            bloom: Bloom { intensity: 0.18, scatter: 0.9, ..base.bloom },
            vignette: Vignette { intensity: 0.3, ..base.vignette },
            fog: Fog {
                density: 0.008,
                height_falloff: 0.08,
                color: Some([0.14, 0.18, 0.28]),
                sun_glow: 0.2,
                volumetric: true,
                anisotropy: 0.6,
                ..base.fog
            },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.02, 0.03, 0.07]),
            horizon: linear([0.07, 0.09, 0.15]),
            ground: linear([0.02, 0.02, 0.03]),
            ambient: 0.3,
            stars: 1.5,
            // A few clouds for the moon to light.
            clouds: Clouds::covering(0.3),
            ..Sky::default()
        };
        // The moon: faint, blue and soft.
        let sun = SunLook {
            tint: temperature_color(9500.0),
            strength: 0.12,
            softness: 1.5,
            shadow_tint: linear([0.02, 0.03, 0.06]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn electrical_storm() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.3, max_ev: 4.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.3, saturation: 0.9 },
            grading: Grading {
                temperature: -0.03,
                tint: -0.05,
                saturation: 0.8,
                lift: [-0.004, 0.008, 0.008],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.14, scatter: 0.9, tint: [0.85, 1.0, 0.95], ..base.bloom },
            vignette: Vignette { intensity: 0.35, ..base.vignette },
            fog: Fog {
                density: 0.02,
                height_falloff: 0.04,
                color: Some([0.34, 0.4, 0.4]),
                sun_glow: 0.2,
                volumetric: true,
                anisotropy: 0.3,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { strength: 1.3, ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            chromatic_aberration: 0.15,
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.18, 0.22, 0.24]),
            horizon: linear([0.34, 0.4, 0.38]),
            ground: linear([0.16, 0.16, 0.15]),
            ambient: 0.9,
            // A low, heavy ceiling of cloud, the light already what it lets through.
            clouds: Clouds { height: 800.0, thickness: 1500.0, density: 2.0, shadows: 0.0, ..Clouds::covering(1.0) },
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(7500.0),
            strength: 0.25,
            softness: 6.0,
            shadow_tint: Vec3::ZERO,
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    // ---------------------------------------------------------------- After Hogwarts Legacy's places

    fn candlelit_castle() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            // Free to brighten a hall lit through its windows and by candles.
            exposure: Exposure { compensation: 0.2, max_ev: 6.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.25, saturation: 1.15 },
            grading: Grading {
                temperature: 0.08,
                saturation: 1.05,
                lift: [0.012, 0.006, 0.0],
                gain: [1.06, 1.0, 0.9],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.2, scatter: 0.9, tint: [1.0, 0.85, 0.6], ..base.bloom },
            vignette: Vignette { intensity: 0.35, ..base.vignette },
            // Dust hanging in the window light.
            fog: Fog {
                density: 0.01,
                height_falloff: 0.02,
                color: Some([0.6, 0.46, 0.3]),
                sun_glow: 0.8,
                volumetric: true,
                anisotropy: 0.75,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { strength: 1.3, tint: [0.2, 0.1, 0.05], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.2, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.32, 0.36, 0.46]),
            horizon: linear([0.62, 0.5, 0.38]),
            ground: linear([0.2, 0.15, 0.1]),
            ambient: 0.4,
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(3200.0),
            strength: 1.0,
            softness: 1.5,
            shadow_tint: linear([0.1, 0.06, 0.04]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn dark_forest() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: -0.1, max_ev: 5.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.2, saturation: 1.1 },
            grading: Grading {
                temperature: -0.04,
                saturation: 1.0,
                lift: [-0.004, 0.012, 0.012],
                gain: [1.0, 1.02, 0.96],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.12, scatter: 0.88, tint: [0.9, 1.0, 0.9], ..base.bloom },
            vignette: Vignette { intensity: 0.4, ..base.vignette },
            fog: Fog {
                density: 0.025,
                height_falloff: 0.1,
                color: Some([0.24, 0.38, 0.36]),
                sun_glow: 0.7,
                volumetric: true,
                anisotropy: 0.8,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { strength: 1.3, tint: [0.04, 0.1, 0.08], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.2, 0.3, 0.32]),
            horizon: linear([0.32, 0.44, 0.4]),
            ground: linear([0.1, 0.12, 0.08]),
            ambient: 0.6,
            // Heavy cloud, the gloom under it the look's own.
            clouds: Clouds { shadows: 0.0, ..Clouds::covering(0.75) },
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(4600.0),
            strength: 0.9,
            softness: 1.0,
            shadow_tint: linear([0.03, 0.07, 0.06]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn snowy_village() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: -0.2, max_ev: 3.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.05, saturation: 1.05 },
            grading: Grading {
                temperature: -0.06,
                saturation: 0.95,
                lift: [0.0, 0.006, 0.014],
                ..base.grading
            },
            // Windows glow warm against the cold.
            bloom: Bloom { intensity: 0.16, scatter: 0.9, tint: [1.0, 0.86, 0.66], ..base.bloom },
            vignette: Vignette { intensity: 0.22, ..base.vignette },
            fog: Fog {
                density: 0.01,
                height_falloff: 0.08,
                color: Some([0.78, 0.84, 0.92]),
                sun_glow: 0.3,
                volumetric: true,
                anisotropy: 0.5,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.06, 0.08, 0.16], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.15, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.6, 0.68, 0.8]),
            horizon: linear([0.84, 0.86, 0.9]),
            ground: linear([0.8, 0.82, 0.86]),
            ambient: 1.5,
            // A winter sky nearly full of cloud, the light already soft.
            clouds: Clouds { shadows: 0.0, ..Clouds::covering(0.8) },
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(6500.0),
            strength: 0.5,
            softness: 3.0,
            shadow_tint: Vec3::ZERO,
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn highland_summer() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.2, max_ev: 2.5, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.1, saturation: 1.25 },
            grading: Grading {
                saturation: 1.12,
                lift: [0.0, 0.006, 0.012],
                gain: [1.03, 1.01, 0.96],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.08, scatter: 0.85, ..base.bloom },
            vignette: Vignette { intensity: 0.15, ..base.vignette },
            fog: Fog {
                density: 0.003,
                height_falloff: 0.1,
                color: Some([0.7, 0.8, 0.92]),
                sun_glow: 0.5,
                ..base.fog
            },
            ambient_occlusion: AmbientOcclusion { tint: [0.08, 0.1, 0.16], ..base.ambient_occlusion },
            global_illumination: GlobalIllumination { enabled: true, strength: 1.15, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Atmosphere,
            ambient: 1.1,
            haze: 1.0,
            haze_tint: linear([0.94, 0.97, 1.0]),
            haze_glow: 0.6,
            distance_haze: 50.0,
            // Fair-weather clouds.
            clouds: Clouds::covering(0.4),
            ..Sky::default()
        };
        let sun = SunLook {
            tint: temperature_color(5600.0),
            strength: 1.3,
            softness: 0.8,
            shadow_tint: linear([0.06, 0.08, 0.14]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }

    fn enchanted_night() -> Self {
        let base = PostProcess::voxear();
        let post = PostProcess {
            exposure: Exposure { compensation: 0.3, max_ev: 6.0, ..base.exposure },
            tonemap: Tonemap { mapper: Tonemapper::Agx, contrast: 1.15, saturation: 1.2 },
            grading: Grading {
                temperature: -0.08,
                tint: 0.06,
                saturation: 1.05,
                lift: [0.006, 0.0, 0.016],
                gain: [0.98, 0.97, 1.06],
                ..base.grading
            },
            bloom: Bloom { intensity: 0.24, scatter: 0.92, tint: [0.85, 0.8, 1.0], ..base.bloom },
            vignette: Vignette { intensity: 0.3, ..base.vignette },
            fog: Fog {
                density: 0.01,
                height_falloff: 0.08,
                color: Some([0.22, 0.2, 0.42]),
                sun_glow: 0.4,
                volumetric: true,
                anisotropy: 0.65,
                ..base.fog
            },
            global_illumination: GlobalIllumination { enabled: true, ..base.global_illumination },
            ..base
        };
        let sky = Sky {
            mode: SkyMode::Gradient,
            zenith: linear([0.04, 0.03, 0.12]),
            horizon: linear([0.14, 0.11, 0.26]),
            ground: linear([0.03, 0.02, 0.05]),
            ambient: 0.4,
            stars: 2.0,
            // A few clouds for the moon to light.
            clouds: Clouds::covering(0.25),
            ..Sky::default()
        };
        let sun = SunLook {
            tint: linear([0.72, 0.74, 1.0]),
            strength: 0.18,
            softness: 1.5,
            shadow_tint: linear([0.04, 0.03, 0.09]),
            shadow_distance: SHADOW_DISTANCE,
            shadow_split: SHADOW_SPLIT,
        };
        Self { post, sky, sun }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_are_found_by_name_and_differ() {
        let overcast = Look::of(LookName::parse("soft_overcast").unwrap());
        let storybook = Look::of(LookName::parse("storybook").unwrap());
        assert!(LookName::parse("noir").is_err());
        assert!(overcast.post.fog.volumetric && !storybook.post.fog.volumetric, "storybook's light shafts are faint");
        assert!(overcast.sun.strength < storybook.sun.strength, "overcast's sky is the main light");
        assert_eq!(overcast.sun.shadow_tint, Vec3::ZERO);
        assert!(storybook.sun.shadow_tint.x > 0.0, "storybook's shadows are never grey");
        let golden = Look::of(LookName::parse("golden_hour").unwrap());
        assert_eq!(golden.sky.mode, SkyMode::Atmosphere, "golden hour's light comes through the air");
        assert!(golden.sky.haze > 1.0 && golden.post.fog.volumetric, "in dusty air, with light shafts");
    }

    #[test]
    fn every_look_reads_back_by_its_name_and_names_are_its_own() {
        for (look, key, title) in LookName::ALL {
            assert_eq!(LookName::parse(key).unwrap(), look);
            assert_eq!((look.key(), look.title()), (key, title));
            let made = Look::of(look);
            assert!(made.sun.strength > 0.0 && made.sky.ambient > 0.0, "{title} lights something");
        }
        let mut keys: Vec<&str> = LookName::ALL.iter().map(|(_, key, _)| *key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), LookName::ALL.len(), "no two looks share a name");
        let night = Look::of(LookName::NightRaid);
        assert!(night.sun.strength < 0.3 && night.sky.stars > 0.0, "a night look is lit by a faint moon under stars");
    }
}
