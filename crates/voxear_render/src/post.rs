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
use voxear_rhi::TextureId;

/// Every post-processing setting. `Default` is the Voxear Look.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PostProcess {
    pub ambient_occlusion: AmbientOcclusion,
    pub contact_shadows: ContactShadows,
    pub global_illumination: GlobalIllumination,
    pub reflections: Reflections,
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

/// Real-time global illumination from a clipmap of voxels round the camera
/// (see `gi`): light bounced off nearby surfaces, and the sky's light kept
/// out of rooms and from under things, in place of the plain sky ambient.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalIllumination {
    pub enabled: bool,
    /// How bright the bounced light is: 1 as measured, more for a brighter,
    /// more stylised bounce.
    pub strength: f32,
    /// The finest voxels' size, in metres; each of the three grids round the
    /// camera is 64 voxels across, the next with voxels twice the size. The
    /// radiance cache's probes are two voxels apart, 48 across. With ray
    /// tracing, the probes are two of these apart too.
    pub voxel_size: f32,
    /// Advanced: trace the probes' rays against the scene's triangles where
    /// the GPU has ray tracing, so no light leaks through walls; off, voxels
    /// light the scene on every GPU.
    pub ray_tracing: bool,
}

impl Default for GlobalIllumination {
    fn default() -> Self {
        Self { enabled: false, strength: 1.0, voxel_size: 0.5, ray_tracing: true }
    }
}

/// Screen-space reflections: what lies on screen, mirrored in smooth
/// surfaces, from last frame's picture. They need TAA, whose history they
/// read; without it, and where a ray finds nothing, surfaces reflect the sky.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Reflections {
    pub enabled: bool,
    /// How far rays go, in metres.
    pub max_distance: f32,
    /// How thick things in the depth buffer count as, in metres.
    pub thickness: f32,
    /// Surfaces rougher than this, from 0 to 1, show no screen-space
    /// reflections, fading out towards it.
    pub max_roughness: f32,
    /// Advanced: the steps each ray takes.
    pub steps: u32,
}

impl Default for Reflections {
    fn default() -> Self {
        Self { enabled: true, max_distance: 30.0, thickness: 0.3, max_roughness: 0.6, steps: 32 }
    }
}

/// Contact shadows: the small shadows the sun casts where things touch,
/// which its shadow map is too coarse to hold, found by marching a short way
/// towards the sun through the depth buffer from each pixel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactShadows {
    pub enabled: bool,
    /// How far each pixel looks towards the sun, in metres.
    pub length: f32,
    /// How thick something in the depth buffer counts as, in metres: what
    /// lies farther behind it than this does not shadow.
    pub thickness: f32,
    /// Advanced: how many steps the march takes.
    pub steps: u32,
}

impl Default for ContactShadows {
    fn default() -> Self {
        Self { enabled: true, length: 0.3, thickness: 0.15, steps: 12 }
    }
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
    /// With TAA, the share of the output's width and height the scene is
    /// drawn at, from 0.25 to 1, as Unreal's screen percentage: TAA then
    /// upscales it, building the full picture from several frames' jittered
    /// samples. 0.67 draws under half the pixels and costs far less, for a
    /// little softness. Without TAA, the scene is drawn at full size.
    pub render_scale: f32,
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
    /// sRGB: the glow's colour, at the same brightness; white leaves it as
    /// the light is.
    pub tint: [f32; 3],
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
    /// brightening by default, so nights stay dark; rooms lit only through
    /// their windows want `max_ev` raised to 6 or more, and `night` keeps
    /// their nights dark when it is.
    pub min_ev: f32,
    pub max_ev: f32,
    /// Advanced: how quickly exposure adapts, in stops a second, towards
    /// brighter and towards darker scenes, easing in over the last stop and
    /// a half. Eyes adapt to light in seconds and to the dark far slower.
    pub speed_up: f32,
    pub speed_down: f32,
    /// How dark a dark scene stays, as an eye sees it: the darker it is,
    /// the less auto exposure brightens it, so night stays night however far
    /// `max_ev` lets it brighten. 1 is as the eye sees; 0 brightens dark
    /// scenes as far as daylit ones.
    pub night: f32,
    /// Advanced: the part of the picture's brightness exposure is set by, as
    /// percentiles of it: the brightest 30% bar the brightest 5% by default,
    /// so sunlit parts are exposed well and shadows stay dark, as cameras
    /// and Unreal do. Lower both to lift shadows.
    pub low_percent: f32,
    pub high_percent: f32,
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
    /// A colour grading lookup table, applied last: a picture of N squares
    /// of N×N side by side (see `Renderer::add_lut`). Scenes name it as
    /// `lut = "Looks/Warm.png"`, which the engine loads and sets here.
    #[serde(skip)]
    pub lut: Option<TextureId>,
    /// How much of the lookup table's grade shows, from 0 to 1.
    pub lut_contribution: f32,
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
        Self { mode: AntiAliasingMode::Taa, sharpness: 0.25, taa_history: 0.9, taa_jitter: 1.0, render_scale: 1.0 }
    }
}

impl Default for Bloom {
    fn default() -> Self {
        Self { enabled: true, intensity: 0.04, scatter: 0.7, tint: [1.0; 3] }
    }
}

impl Default for Exposure {
    fn default() -> Self {
        Self {
            auto: true,
            compensation: 0.0,
            min_ev: -3.0,
            max_ev: 1.0,
            speed_up: 3.0,
            speed_down: 1.0,
            night: 1.0,
            low_percent: 70.0,
            high_percent: 95.0,
        }
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
            lut: None,
            lut_contribution: 1.0,
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
    /// The Voxear Look: ambient occlusion, contact shadows, TAA with light sharpening, soft
    /// bloom, eye adaptation that keeps nights dark, AgX with a little punch,
    /// a soft vignette and a touch of atmospheric fog.
    pub fn voxear() -> Self {
        Self {
            ambient_occlusion: AmbientOcclusion::default(),
            contact_shadows: ContactShadows::default(),
            global_illumination: GlobalIllumination::default(),
            reflections: Reflections::default(),
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
            contact_shadows: ContactShadows { enabled: false, ..ContactShadows::default() },
            global_illumination: GlobalIllumination { enabled: false, ..GlobalIllumination::default() },
            reflections: Reflections { enabled: false, ..Reflections::default() },
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
        // A look's post-processing, or a preset's, with the table's settings on top; a preset wins over a look.
        let look = match table.get("look") {
            Some(toml::Value::String(name)) => Some(crate::looks::Look::of(crate::looks::LookName::parse(name)?)),
            Some(_) => bail!("`look` must be a name such as \"storybook\""),
            None => None,
        };
        let preset = match table.get("preset") {
            Some(toml::Value::String(name)) => Self::preset(name)?,
            Some(_) => bail!("`preset` must be a name such as \"voxear\""),
            None => look.map_or_else(Self::voxear, |look| look.post),
        };
        let mut merged = toml::Table::try_from(preset)?;
        let mut overrides = table.clone();
        overrides.remove("preset");
        overrides.remove("look");
        // A lookup table is an asset, which the engine loads and sets; it is not read here.
        if let Some(toml::Value::Table(grading)) = overrides.get_mut("grading") {
            grading.remove("lut");
        }
        merge(&mut merged, overrides);
        toml::Value::Table(merged).try_into().context("reading [post]")
    }
}

/// TAA's sub-pixel camera offset for a frame, in pixels: a Halton (2, 3)
/// sequence `phases` frames long, which covers the pixel evenly in few
/// frames. Eight suit drawing at full size; upscaling needs more, as each
/// drawn pixel covers several of the output's.
pub(crate) fn taa_jitter(frame_index: u64, scale: f32, phases: u64) -> [f32; 2] {
    let index = frame_index % phases.max(1) + 1;
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
            // The renderer names the image once it has made it.
            contact_texture: crate::gpu::NO_TEXTURE,
            contact_length: self.contact_shadows.length.max(0.0),
            contact_thickness: self.contact_shadows.thickness.max(0.001),
            contact_steps: self.contact_shadows.steps.clamp(1, 64),
            // The renderer names the table once it knows it.
            lut_texture: crate::gpu::NO_TEXTURE,
            _lut_pad0: 0,
            lut_contribution: self.grading.lut_contribution.clamp(0.0, 1.0),
            _lut_pad: 0,
            // The renderer names the image once it has made it.
            reflection_texture: crate::gpu::NO_TEXTURE,
            reflection_distance: self.reflections.max_distance.max(0.1),
            reflection_thickness: self.reflections.thickness.max(0.01),
            reflection_steps: self.reflections.steps.clamp(4, 128),
            reflection_max_roughness: self.reflections.max_roughness.clamp(0.01, 1.0),
            exposure_low_percent: self.exposure.low_percent.clamp(0.0, 99.0),
            exposure_high_percent: self.exposure.high_percent.clamp(1.0, 100.0),
            _exposure_pad: 0,
            bloom_tint: linear(self.bloom.tint),
            exposure_night: self.exposure.night.clamp(0.0, 1.0),
        }
    }
}

impl PostProcess {
    /// These settings with `overrides`, written as `[post]` writes them,
    /// shown by `amount` from 0 to 1: numbers and colours in between,
    /// switches and choices once it is more than half.
    pub fn blended(&self, overrides: &toml::Table, amount: f32) -> Result<Self> {
        let base = toml::Table::try_from(self)?;
        let mut target = base.clone();
        merge(&mut target, overrides.clone());
        let mixed = mix(&toml::Value::Table(base), &toml::Value::Table(target), amount.clamp(0.0, 1.0));
        let mut blended: Self = mixed.try_into().context("reading the blended settings")?;
        // The lookup table is an asset, not a setting, so it stays as it was.
        blended.grading.lut = self.grading.lut;
        Ok(blended)
    }
}

/// `from` and `to` mixed by `amount`: numbers in between, each part of a
/// table or a list in turn, and anything else switching at half.
fn mix(from: &toml::Value, to: &toml::Value, amount: f32) -> toml::Value {
    use toml::Value;
    match (from, to) {
        (Value::Float(a), Value::Float(b)) => Value::Float(a + (b - a) * f64::from(amount)),
        (Value::Integer(a), Value::Integer(b)) => {
            Value::Integer((*a as f64 + (*b - *a) as f64 * f64::from(amount)).round() as i64)
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            Value::Array(a.iter().zip(b).map(|(a, b)| mix(a, b, amount)).collect())
        }
        (Value::Table(a), Value::Table(b)) => Value::Table(
            b.iter()
                .map(|(key, to)| (key.clone(), a.get(key).map_or_else(|| to.clone(), |from| mix(from, to, amount))))
                .collect(),
        ),
        _ if amount > 0.5 => to.clone(),
        _ => from.clone(),
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
        let offsets: Vec<[f32; 2]> = (0..8).map(|i| taa_jitter(i, 1.0, 8)).collect();
        assert!(offsets.iter().flatten().all(|v| (-0.5..0.5).contains(v)));
        let mean = offsets.iter().fold([0.0, 0.0], |sum, o| [sum[0] + o[0] / 8.0, sum[1] + o[1] / 8.0]);
        assert!(mean[0].abs() < 0.07 && mean[1].abs() < 0.07, "centred on the pixel: {mean:?}");
        assert_eq!(taa_jitter(8, 1.0, 8), offsets[0]);
        assert_eq!(taa_jitter(3, 0.5, 8), offsets[3].map(|v| v * 0.5));
    }
}
