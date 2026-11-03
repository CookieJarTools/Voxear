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

use anyhow::{Result, bail};
use voxear_core::math::Vec3;

use crate::{
    post::{
        AmbientOcclusion, Bloom, Exposure, Fog, GlobalIllumination, Grading, PostProcess, Tonemap, Tonemapper, Vignette,
    },
    scene::{Sky, SkyMode},
};

/// A look by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookName {
    SoftOvercast,
    Storybook,
}

impl LookName {
    pub fn parse(name: &str) -> Result<Self> {
        Ok(match name {
            "soft_overcast" => Self::SoftOvercast,
            "storybook" => Self::Storybook,
            _ => bail!("no look `{name}` (soft_overcast and storybook exist)"),
        })
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
        }
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
            ..Sky::default()
        };
        // About 5200K, giving a few times the sky's light rather than ten.
        let sun = SunLook { tint: linear([1.0, 0.93, 0.83]), strength: 0.6, softness: 3.0, shadow_tint: Vec3::ZERO };
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
            ..Sky::default()
        };
        // About 4000K and pushed towards yellow-green, with shadows never black but warm.
        let sun = SunLook {
            tint: linear([1.0, 0.9, 0.62]),
            strength: 1.2,
            softness: 2.0,
            shadow_tint: linear([0.42, 0.3, 0.38]),
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
    }
}
