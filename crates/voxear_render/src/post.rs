//! Post-processing settings and the presets, including the default "Voxear
//! Look".
//!
//! A scene's `[post]` section picks a preset and overrides any setting:
//!
//! ```toml
//! [post]
//! preset = "voxear"          # or "neutral", "cinematic"
//!
//! [post.bloom]
//! intensity = 0.08
//!
//! [post.ambient_occlusion]
//! strength = 1.5
//! tint = [0.25, 0.15, 0.1]
//!
//! [post.anti_aliasing]
//! mode = "fxaa"
//! ```

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Every post-processing setting. `Default` is the Voxear Look.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PostProcess {
    pub ambient_occlusion: AmbientOcclusion,
    pub anti_aliasing: AntiAliasing,
    pub bloom: Bloom,
    pub exposure: Exposure,
    pub tonemap: Tonemap,
    pub grading: Grading,
    pub vignette: Vignette,
    pub fog: Fog,
    pub depth_of_field: DepthOfField,
    pub motion_blur: MotionBlur,
    /// Splits colours towards the screen edges, like a cheap lens. 0 is off.
    pub chromatic_aberration: f32,
    pub grain: Grain,
}

/// How hard ambient occlusion looks for what shades each pixel: how many
/// directions (slices) it searches, and how many steps along each side of
/// each. Every frame searches new directions, which blend over a few frames,
/// so the lower ones are smooth once the view is still.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AmbientOcclusionQuality {
    /// 1 slice of 4 steps.
    Low,
    /// 2 slices of 4 steps.
    Medium,
    /// 3 slices of 6 steps.
    High,
    /// 4 slices of 8 steps.
    Ultra,
}

impl AmbientOcclusionQuality {
    /// Slices, and steps along each side of each.
    pub fn slices_and_steps(self) -> (u32, u32) {
        match self {
            Self::Low => (1, 4),
            Self::Medium => (2, 4),
            Self::High => (3, 6),
            Self::Ultra => (4, 8),
        }
    }
}

/// Ground-truth ambient occlusion (GTAO): creases, corners and contacts
/// darken, as less of the sky reaches into them. It shades only the sky's
/// ambient light, never the sun or lamps.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AmbientOcclusion {
    pub enabled: bool,
    /// How far, in metres, a surface looks for what shades it.
    pub radius: f32,
    /// 1 is as measured; higher darkens more, 0 leaves the light alone.
    pub strength: f32,
    /// sRGB: the colour fully occluded light takes on. Black only darkens;
    /// a warm brown or a purple tints contact shadows, as stylised looks do.
    pub tint: [f32; 3],
    pub quality: AmbientOcclusionQuality,
    /// Searches every pixel, rather than one of each 2x2 a frame with the
    /// temporal pass filling in the rest. Sharper in fine detail, and about
    /// four times the cost.
    pub full_resolution: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AntiAliasingMode {
    Off,
    /// Fast, single pass, slightly soft. Good for low-end GPUs.
    Fxaa,
    /// Temporal: smooth edges and stable detail, using motion vectors.
    Taa,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AntiAliasing {
    pub mode: AntiAliasingMode,
    /// Contrast-adaptive sharpening after anti-aliasing, 0 to 1.
    pub sharpness: f32,
    /// Advanced: how much of last frame TAA keeps, 0.8 to 0.97. Higher is
    /// smoother and slower to react.
    pub taa_history: f32,
    /// Advanced: sub-pixel jitter spread, in pixels.
    pub taa_jitter: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bloom {
    pub enabled: bool,
    /// How much of the image is replaced by its blurred glow. Physically
    /// based, so no threshold: bright things simply glow more.
    pub intensity: f32,
    /// Advanced: 0 keeps the glow tight, 1 spreads it wide.
    pub scatter: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Exposure {
    /// Adapt to the scene's brightness, like an eye.
    pub auto: bool,
    /// Brighter or darker than metered, in stops (EV).
    pub compensation: f32,
    /// Advanced: the most auto exposure may darken (negative) or brighten
    /// (positive) the image, in stops. Darkening is allowed further than
    /// brightening by default, so nights stay dark.
    pub min_ev: f32,
    pub max_ev: f32,
    /// Advanced: adaptation speed in stops per second, towards brighter and
    /// towards darker scenes.
    pub speed_up: f32,
    pub speed_down: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tonemapper {
    /// Graceful highlights and natural colour (Blender's default). Voxear's choice.
    Agx,
    /// The filmic ACES curve: punchy and contrasty.
    Aces,
    /// Khronos PBR Neutral: keeps material colours true, for product shots.
    Neutral,
    Reinhard,
    /// Clamp only, for debugging.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tonemap {
    pub mapper: Tonemapper,
    /// AgX look: contrast (1 is AgX's own) and saturation (1 unchanged).
    pub contrast: f32,
    pub saturation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grading {
    /// White balance: negative is cooler (blue), positive warmer (orange).
    pub temperature: f32,
    /// Negative is greener, positive more magenta.
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
    /// Advanced: lift (shadows), gamma (midtones) and gain (highlights) per
    /// channel, applied after tonemapping.
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vignette {
    pub intensity: f32,
    /// Advanced: softness of the falloff, and 1 for round or 0 for the
    /// screen's own shape.
    pub smoothness: f32,
    pub roundness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fog {
    pub enabled: bool,
    pub density: f32,
    /// Advanced: how fast fog thins with height, and where it is densest.
    pub height_falloff: f32,
    pub base_height: f32,
    /// sRGB; the sky's horizon colour when left out.
    pub color: Option<[f32; 3]>,
    /// How strongly fog glows towards the sun.
    pub sun_glow: f32,
    /// Lights the fog near the camera as a volume, so the sun's shadows cut
    /// shafts through it and lamps glow in it. Beyond `volumetric_distance`
    /// metres the fog carries on as it is without.
    pub volumetric: bool,
    pub volumetric_distance: f32,
    /// How much of the light the fog scatters onward, from -1 (back towards
    /// the light) through 0 (evenly) to 1: higher brightens the fog looking
    /// towards a light, as haze does.
    pub anisotropy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DepthOfField {
    pub enabled: bool,
    /// Distance in metres that is in perfect focus.
    pub focus_distance: f32,
    /// Lens f-stop: smaller numbers blur more.
    pub aperture: f32,
    /// Advanced: the largest blur radius, in pixels at 1080p.
    pub max_blur: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionBlur {
    pub enabled: bool,
    /// Fraction of the frame's motion that blurs, like a camera shutter.
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grain {
    pub intensity: f32,
    /// Advanced: grain size in pixels.
    pub size: f32,
}

impl Default for PostProcess {
    fn default() -> Self {
        Self::voxear()
    }
}

impl Default for AmbientOcclusion {
    fn default() -> Self {
        Self {
            enabled: true,
            radius: 0.75,
            strength: 1.0,
            tint: [0.0; 3],
            quality: AmbientOcclusionQuality::Medium,
            full_resolution: false,
        }
    }
}

impl Default for AntiAliasing {
    fn default() -> Self {
        Self { mode: AntiAliasingMode::Taa, sharpness: 0.25, taa_history: 0.9, taa_jitter: 1.0 }
    }
}

impl Default for Bloom {
    fn default() -> Self {
        Self { enabled: true, intensity: 0.04, scatter: 0.7 }
    }
}

impl Default for Exposure {
    fn default() -> Self {
        Self { auto: true, compensation: 0.0, min_ev: -3.0, max_ev: 1.0, speed_up: 2.0, speed_down: 1.0 }
    }
}

impl Default for Tonemap {
    fn default() -> Self {
        Self { mapper: Tonemapper::Agx, contrast: 1.1, saturation: 1.15 }
    }
}

impl Default for Grading {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            tint: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            lift: [0.0; 3],
            gamma: [1.0; 3],
            gain: [1.0; 3],
        }
    }
}

impl Default for Vignette {
    fn default() -> Self {
        Self { intensity: 0.2, smoothness: 0.4, roundness: 1.0 }
    }
}

impl Default for Fog {
    fn default() -> Self {
        Self {
            enabled: true,
            density: 0.004,
            height_falloff: 0.12,
            base_height: 0.0,
            color: None,
            sun_glow: 0.5,
            volumetric: false,
            volumetric_distance: 64.0,
            anisotropy: 0.6,
        }
    }
}

impl Default for DepthOfField {
    fn default() -> Self {
        Self { enabled: false, focus_distance: 10.0, aperture: 2.8, max_blur: 12.0 }
    }
}

impl Default for MotionBlur {
    fn default() -> Self {
        Self { enabled: false, intensity: 0.5 }
    }
}

impl Default for Grain {
    fn default() -> Self {
        Self { intensity: 0.0, size: 1.5 }
    }
}

impl PostProcess {
    /// The Voxear Look: ambient occlusion, TAA with light sharpening, soft
    /// bloom, eye adaptation that keeps nights dark, AgX with a little punch,
    /// a soft vignette and a touch of atmospheric fog.
    pub fn voxear() -> Self {
        Self {
            ambient_occlusion: AmbientOcclusion::default(),
            anti_aliasing: AntiAliasing::default(),
            bloom: Bloom::default(),
            // Metering for middle grey looks dull on bright days; half a stop lifts it.
            exposure: Exposure { compensation: 0.5, ..Exposure::default() },
            tonemap: Tonemap::default(),
            grading: Grading::default(),
            vignette: Vignette::default(),
            fog: Fog::default(),
            depth_of_field: DepthOfField::default(),
            motion_blur: MotionBlur::default(),
            chromatic_aberration: 0.0,
            grain: Grain::default(),
        }
    }

    /// Only what keeps the image correct: TAA, AgX with no look, fixed
    /// exposure, and no stylistic effects. A clean base to build on.
    pub fn neutral() -> Self {
        Self {
            anti_aliasing: AntiAliasing { sharpness: 0.0, ..AntiAliasing::default() },
            bloom: Bloom { enabled: false, ..Bloom::default() },
            exposure: Exposure { auto: false, ..Exposure::default() },
            tonemap: Tonemap { contrast: 1.0, saturation: 1.0, ..Tonemap::default() },
            vignette: Vignette { intensity: 0.0, ..Vignette::default() },
            fog: Fog { enabled: false, ..Fog::default() },
            ..Self::voxear()
        }
    }

    /// A filmic camera: ACES contrast, stronger bloom and vignette, a little
    /// grain and lens fringing, and motion blur.
    pub fn cinematic() -> Self {
        Self {
            bloom: Bloom { intensity: 0.07, ..Bloom::default() },
            tonemap: Tonemap { mapper: Tonemapper::Aces, ..Tonemap::default() },
            vignette: Vignette { intensity: 0.35, ..Vignette::default() },
            motion_blur: MotionBlur { enabled: true, ..MotionBlur::default() },
            chromatic_aberration: 0.15,
            grain: Grain { intensity: 0.05, ..Grain::default() },
            ..Self::voxear()
        }
    }

    /// Everything off and no tonemapping, so debug views show raw colours.
    pub(crate) fn debug_view() -> Self {
        Self {
            ambient_occlusion: AmbientOcclusion { enabled: false, ..AmbientOcclusion::default() },
            anti_aliasing: AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() },
            exposure: Exposure { auto: false, ..Exposure::default() },
            tonemap: Tonemap { mapper: Tonemapper::None, ..Tonemap::default() },
            ..Self::neutral()
        }
    }

    pub fn preset(name: &str) -> Result<Self> {
        Ok(match name {
            "voxear" => Self::voxear(),
            "neutral" => Self::neutral(),
            "cinematic" => Self::cinematic(),
            _ => bail!("no post-processing preset `{name}` (voxear, neutral and cinematic exist)"),
        })
    }

    /// Reads a `[post]` table: the named preset (Voxear by default) with the
    /// table's settings layered on top.
    pub fn from_toml(table: &toml::Table) -> Result<Self> {
        let preset = match table.get("preset") {
            Some(toml::Value::String(name)) => Self::preset(name)?,
            Some(_) => bail!("`preset` must be a name such as \"voxear\""),
            None => Self::voxear(),
        };
        let mut merged = toml::Table::try_from(preset)?;
        let mut overrides = table.clone();
        overrides.remove("preset");
        merge(&mut merged, overrides);
        toml::Value::Table(merged).try_into().context("reading [post]")
    }
}

/// TAA's sub-pixel camera offset for a frame, in pixels: an eight-frame
/// Halton (2, 3) sequence, which covers the pixel evenly in few frames.
pub(crate) fn taa_jitter(frame_index: u64, scale: f32) -> [f32; 2] {
    let index = frame_index % 8 + 1;
    [(halton(index, 2) - 0.5) * scale, (halton(index, 3) - 0.5) * scale]
}

fn halton(mut index: u64, base: u64) -> f32 {
    let (mut fraction, mut result) = (1.0, 0.0);
    while index > 0 {
        fraction /= base as f32;
        result += fraction * (index % base) as f32;
        index /= base;
    }
    result
}

impl PostProcess {
    /// What shaders read. `horizon` (linear) colours fog when no colour is set.
    pub(crate) fn to_gpu(self, horizon: voxear_core::math::Vec3) -> crate::gpu::GpuPost {
        let linear = |c: [f32; 3]| c.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
        let tonemapper = match self.tonemap.mapper {
            Tonemapper::Agx => 0,
            Tonemapper::Aces => 1,
            Tonemapper::Neutral => 2,
            Tonemapper::Reinhard => 3,
            Tonemapper::None => 4,
        };
        let enabled = |on: bool, value: f32| if on { value } else { 0.0 };
        let (slices, steps) = self.ambient_occlusion.quality.slices_and_steps();
        crate::gpu::GpuPost {
            bloom_intensity: enabled(self.bloom.enabled, self.bloom.intensity),
            sharpness: self.anti_aliasing.sharpness,
            taa_history: self.anti_aliasing.taa_history,
            chromatic_aberration: self.chromatic_aberration,
            exposure_compensation: self.exposure.compensation,
            exposure_min: self.exposure.min_ev,
            exposure_max: self.exposure.max_ev,
            auto_exposure: u32::from(self.exposure.auto),
            exposure_speed_up: self.exposure.speed_up,
            exposure_speed_down: self.exposure.speed_down,
            tonemapper,
            fxaa: u32::from(self.anti_aliasing.mode == AntiAliasingMode::Fxaa),
            tonemap_contrast: self.tonemap.contrast,
            tonemap_saturation: self.tonemap.saturation,
            temperature: self.grading.temperature,
            tint: self.grading.tint,
            grading_contrast: self.grading.contrast,
            grading_saturation: self.grading.saturation,
            vignette: self.vignette.intensity,
            vignette_smoothness: self.vignette.smoothness,
            lift: self.grading.lift,
            vignette_roundness: self.vignette.roundness,
            gamma: self.grading.gamma,
            grain: self.grain.intensity,
            gain: self.grading.gain,
            grain_size: self.grain.size,
            fog_color: self.fog.color.map_or(horizon.to_array(), linear),
            fog_density: enabled(self.fog.enabled, self.fog.density),
            fog_height_falloff: self.fog.height_falloff,
            fog_base_height: self.fog.base_height,
            fog_sun_glow: self.fog.sun_glow,
            bloom_scatter: self.bloom.scatter,
            dof_focus: self.depth_of_field.focus_distance,
            dof_aperture: self.depth_of_field.aperture,
            dof_max_blur: enabled(self.depth_of_field.enabled, self.depth_of_field.max_blur),
            motion_blur: enabled(self.motion_blur.enabled, self.motion_blur.intensity),
            ao_tint: linear(self.ambient_occlusion.tint),
            ao_strength: self.ambient_occlusion.strength.max(0.0),
            ao_radius: self.ambient_occlusion.radius,
            ao_slices: slices,
            ao_steps: steps,
            // The renderer names the image, and how to turn pixels back into view space, once it knows them.
            ao_texture: crate::gpu::NO_TEXTURE,
            ao_unproject: [0.0; 2],
            ao_reset: 0,
            _ao_pad: 0,
            fog_anisotropy: self.fog.anisotropy.clamp(-0.95, 0.95),
            fog_volume_far: self.fog.volumetric_distance.max(1.0),
            // The renderer names the volume once it has made it.
            fog_volume: crate::gpu::NO_TEXTURE,
            fog_volume_reset: 0,
        }
    }
}

fn merge(base: &mut toml::Table, overrides: toml::Table) {
    for (key, value) in overrides {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(value)) => merge(base, value),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_overrides_layer() {
        let table: toml::Table = toml::from_str("preset = \"cinematic\"\n[bloom]\nintensity = 0.2\n").unwrap();
        let post = PostProcess::from_toml(&table).unwrap();
        assert_eq!(post.tonemap.mapper, Tonemapper::Aces, "from the preset");
        assert_eq!(post.bloom.intensity, 0.2, "overridden");
        assert!(post.bloom.enabled, "untouched fields keep the preset's value");
    }

    #[test]
    fn an_empty_section_is_the_voxear_look() {
        assert_eq!(PostProcess::from_toml(&toml::Table::new()).unwrap(), PostProcess::voxear());
        assert!(PostProcess::from_toml(&toml::from_str("preset = \"nope\"").unwrap()).is_err());
        assert!(PostProcess::from_toml(&toml::from_str("[bloom]\nglow = 1.0").unwrap()).is_err(), "typos are caught");
    }

    #[test]
    fn ambient_occlusion_reads_its_quality_and_tint() {
        let table = "[ambient_occlusion]\nquality = \"ultra\"\ntint = [1.0, 0.5, 0.0]\nstrength = 0.0\n";
        let post = PostProcess::from_toml(&toml::from_str(table).unwrap()).unwrap();
        assert_eq!(post.ambient_occlusion.quality, AmbientOcclusionQuality::Ultra);
        assert!(post.ambient_occlusion.enabled, "on in the Voxear Look");
        let gpu = post.to_gpu(voxear_core::math::Vec3::ONE);
        assert_eq!((gpu.ao_slices, gpu.ao_steps), (4, 8));
        assert!((gpu.ao_tint[1] - 0.214).abs() < 1e-3, "tints are sRGB, and shaders read them linear");
        assert_eq!(gpu.ao_strength, 0.0);
        assert!(PostProcess::from_toml(&toml::from_str("[ambient_occlusion]\nquality = 3").unwrap()).is_err());
        assert!(!PostProcess::debug_view().ambient_occlusion.enabled, "debug views show surfaces as they are");
    }

    #[test]
    fn taa_jitter_covers_the_pixel_and_repeats() {
        let offsets: Vec<[f32; 2]> = (0..8).map(|i| taa_jitter(i, 1.0)).collect();
        assert!(offsets.iter().flatten().all(|v| (-0.5..0.5).contains(v)));
        let mean = offsets.iter().fold([0.0, 0.0], |sum, o| [sum[0] + o[0] / 8.0, sum[1] + o[1] / 8.0]);
        assert!(mean[0].abs() < 0.07 && mean[1].abs() < 0.07, "centred on the pixel: {mean:?}");
        assert_eq!(taa_jitter(8, 1.0), offsets[0]);
        assert_eq!(taa_jitter(3, 0.5), offsets[3].map(|v| v * 0.5));
    }
}
