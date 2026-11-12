//! Rendering components on Objs, and extraction of what to draw from the
//! world each frame.

use voxear_core::math::{Mat3, Mat4, Quat, Vec3, Vec4, camera::lh::proj::vulkan};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::TextureId;
use voxear_scene::{Destroying, Hidden, Parent, Switch, Switched, Transform, WorldTransform};

use crate::{
    assets::{MaterialHandle, MeshHandle},
    lod_group::{LodCamera, LodChoices},
    gpu::{
        CLUSTER_FAR, CLUSTER_GRID, DEBUG_VIEW_AMBIENT_OCCLUSION, DEBUG_VIEW_GLOBAL_ILLUMINATION, DEBUG_VIEW_NONE, GpuFrame,
        GpuInstance, GpuLight, GpuLighting, LIGHT_DISC, LIGHT_POINT, LIGHT_RECTANGLE, LIGHT_SPOT, LIGHT_TUBE,
        LOCAL_SHADOW_LIGHTS, LOCAL_SHADOW_VIEWS, MAX_CLUSTER_LIGHTS, NO_SHADOW, NO_TEXTURE, SKY_ATMOSPHERE, SKY_GRADIENT,
        cols, vec3,
    },
    post::{AmbientOcclusion, AntiAliasingMode, GlobalIllumination, PostProcess, taa_jitter},
    shadows::{self, CASCADES},
};

/// Renders the scene from this Obj, looking along its +Z like Unity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub fov_y_degrees: f32,
    pub near: f32,
    pub exposure: f32,
}
impl Component for Camera {}

impl Default for Camera {
    fn default() -> Self {
        Self { fov_y_degrees: 60.0, near: 0.1, exposure: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// Shines along the Obj's +Z, like a sun.
    Directional,
    Point,
    /// Shines along the Obj's +Z.
    Spot,
    /// A rectangle that shines from its face along the Obj's +Z, as a window
    /// or a panel light does: `width` along +X and `height` along +Y.
    Rectangle,
    /// A disc of `radius` that shines from its face along the Obj's +Z.
    Disc,
    /// A glowing capsule, as a fluorescent tube is: `length` along the Obj's
    /// +X, and `radius` round it.
    Tube,
}

impl LightKind {
    /// Whether it shines from a shape rather than a point.
    pub fn has_shape(self) -> bool {
        matches!(self, Self::Rectangle | Self::Disc | Self::Tube)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub kind: LightKind,
    /// Linear. With a `temperature`, a filter over the colour of that
    /// temperature: white leaves it as it is.
    pub color: Vec3,
    /// The colour of a light this hot, in kelvin, as Unity's Filter and
    /// Temperature: 1900 a candle, 2700 a household bulb, 4000 moonlight's
    /// look, 5500 the midday sun, 6500 daylight, 10000 a clear blue sky.
    /// Its brightness stays the same as it changes. `None` lights in `color`
    /// alone.
    pub temperature: Option<f32>,
    /// For a shape, what a point light of this intensity would give straight
    /// ahead of it from far away, so changing its size does not change how
    /// much light it gives.
    pub intensity: f32,
    /// How far it reaches, from the nearest point of its shape.
    pub range: f32,
    pub spot_inner_degrees: f32,
    pub spot_outer_degrees: f32,
    /// A rectangle's size, in metres.
    pub width: f32,
    pub height: f32,
    /// A disc's or a tube's radius, in metres.
    pub radius: f32,
    /// A tube's length, end to end, in metres.
    pub length: f32,
    /// A picture the light shines through, as Unity's cookies: projected
    /// over a spot light's cone, or tiled over the world by a directional
    /// light every `cookie_size` metres. Its colours tint the light.
    pub cookie: Option<TextureId>,
    pub cookie_size: f32,
    /// How wide across, in degrees, the light's source looks from what it
    /// shines on, which softens its shadow's edges the farther they fall
    /// from what casts them; 0 for hard edges. The sun is about half a
    /// degree. Only the sun's shadows soften yet.
    pub softness: f32,
    /// Whether a point, spot, area or tube light casts shadows, as Unity's
    /// Shadow Type; on by default. Each that does draws what is round it
    /// again (six times over for any but a spot light), so the nearest
    /// `LOCAL_SHADOW_LIGHTS` to the camera cast them in a frame. The sun
    /// always casts its own.
    pub shadows: bool,
    /// Linear: the colour the sun's shadows take, as a share of its light,
    /// as light through a canopy or off warm ground fills them; black, the
    /// default, leaves them as they are.
    pub shadow_tint: Vec3,
    /// An IES profile (see `Renderer::add_light_profile`): how bright a real
    /// fitting is in each direction, for a point or spot light.
    pub profile: Option<TextureId>,
    /// What a directional light is in the sky: the sun, or the moon, which
    /// the sky draws as its disc, and under which the sky is a night sky.
    pub body: SkyBody,
}

/// What a directional light shows as in the sky.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkyBody {
    /// Daylight: the sun's disc, and the sky lit as by day.
    #[default]
    Sun,
    /// Moonlight: the moon's disc, and the sky as dark as at night, so the
    /// stars show, however bright the light is made to show the scene.
    Moon,
}
impl Component for Light {}

impl Light {
    /// The colour it lights in, linear: its colour, filtering its
    /// temperature's when it has one.
    pub fn light_color(&self) -> Vec3 {
        self.temperature.map_or(self.color, |kelvin| self.color * temperature_color(kelvin))
    }

    pub fn directional(color: Vec3, intensity: f32) -> Self {
        Self { kind: LightKind::Directional, ..Self::shaped(color, intensity, 0.0) }
    }

    pub fn point(color: Vec3, intensity: f32, range: f32) -> Self {
        Self::shaped(color, intensity, range)
    }

    pub fn spot(color: Vec3, intensity: f32, range: f32, inner_degrees: f32, outer_degrees: f32) -> Self {
        Self {
            kind: LightKind::Spot,
            spot_inner_degrees: inner_degrees,
            spot_outer_degrees: outer_degrees,
            ..Self::shaped(color, intensity, range)
        }
    }

    pub fn rectangle(color: Vec3, intensity: f32, range: f32, width: f32, height: f32) -> Self {
        Self { kind: LightKind::Rectangle, width, height, ..Self::shaped(color, intensity, range) }
    }

    pub fn disc(color: Vec3, intensity: f32, range: f32, radius: f32) -> Self {
        Self { kind: LightKind::Disc, radius, ..Self::shaped(color, intensity, range) }
    }

    pub fn tube(color: Vec3, intensity: f32, range: f32, length: f32, radius: f32) -> Self {
        Self { kind: LightKind::Tube, length, radius, ..Self::shaped(color, intensity, range) }
    }

    /// A point light with every other setting at nothing, to build the
    /// others from.
    fn shaped(color: Vec3, intensity: f32, range: f32) -> Self {
        Self {
            kind: LightKind::Point,
            color,
            temperature: None,
            intensity,
            range,
            spot_inner_degrees: 0.0,
            spot_outer_degrees: 0.0,
            width: 0.0,
            height: 0.0,
            radius: 0.0,
            length: 0.0,
            cookie: None,
            cookie_size: 10.0,
            softness: 0.5,
            shadows: true,
            shadow_tint: Vec3::ZERO,
            profile: None,
            body: SkyBody::Sun,
        }
    }
}

/// The linear colour of a black body at `kelvin`, as lights of that
/// temperature shine, scaled to a luminance of 1 so only its colour
/// changes: orange at 1900, about white at 6500, blue past it. From Kim et
/// al.'s fit of the Planckian locus, held to 1667 to 25000 kelvin.
pub fn temperature_color(kelvin: f32) -> Vec3 {
    let t = f64::from(kelvin.clamp(1667.0, 25000.0));
    let (t2, t3) = (t * t, t * t * t);
    let x = match t {
        t if t <= 4000.0 => -0.2661239e9 / t3 - 0.2343589e6 / t2 + 0.8776956e3 / t + 0.179910,
        _ => -3.0258469e9 / t3 + 2.1070379e6 / t2 + 0.2226347e3 / t + 0.240390,
    };
    let (x2, x3) = (x * x, x * x * x);
    let y = match t {
        t if t <= 2222.0 => -1.1063814 * x3 - 1.34811020 * x2 + 2.18555832 * x - 0.20219683,
        t if t <= 4000.0 => -0.9549476 * x3 - 1.37418593 * x2 + 2.09137015 * x - 0.16748867,
        _ => 3.0817580 * x3 - 5.87338670 * x2 + 3.75112997 * x - 0.37001483,
    };
    // CIE XYZ at a luminance of 1, into linear sRGB.
    let (big_x, big_z) = (x / y, (1.0 - x - y) / y);
    let r = 3.2406 * big_x - 1.5372 - 0.4986 * big_z;
    let g = -0.9689 * big_x + 1.8758 + 0.0415 * big_z;
    let b = 0.0557 * big_x - 0.2040 + 1.0570 * big_z;
    Vec3::new(r as f32, g as f32, b as f32).max(Vec3::ZERO)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshRenderer {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    /// Shifts which LOD of its mesh is drawn, as Unity's LOD Selection Bias
    /// does: 1 allows twice the error, so simpler LODs come sooner, and -1
    /// half, so they come later. 0 draws them as the engine picks.
    pub lod_bias: f32,
    /// A LOD of its mesh drawn at every distance, as Unity's Forced LOD.
    pub forced_lod: Option<u8>,
    /// Where each LOD of its mesh ends, as a LOD Group's levels do: the
    /// share of the screen's height, from 0 to 1, each shows down to, the
    /// next showing below it. `None` leaves it to the engine, which switches
    /// by how much detail would show (and `lod_bias`).
    pub lod_transitions: Option<[f32; MAX_TRANSITIONS]>,
}
impl Component for MeshRenderer {}

/// How many LODs a renderer can say the end of: every one but the last.
pub const MAX_TRANSITIONS: usize = crate::lod::MAX_LODS - 1;

impl MeshRenderer {
    /// A mesh drawn with a material, its LODs picked by distance.
    pub fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self { mesh, material, lod_bias: 0.0, forced_lod: None, lod_transitions: None }
    }

    /// Its LOD settings as the GPU reads them: flags with the forced LOD in,
    /// the bias, and where each LOD ends when it says.
    pub(crate) fn lod_bits(&self) -> (u32, f32, [[f32; 4]; 2]) {
        let forced = self.forced_lod.map_or(0, |lod| u32::from(lod) + 1) << crate::gpu::INSTANCE_LOD_SHIFT;
        let Some(ends) = self.lod_transitions else { return (forced, self.lod_bias, [[0.0; 4]; 2]) };
        let mut screen = [[0.0; 4]; 2];
        for (at, end) in ends.into_iter().enumerate() {
            screen[at / 4][at % 4] = end;
        }
        (forced | crate::gpu::INSTANCE_LOD_SCREEN, self.lod_bias, screen)
    }
}

/// Everything the GPU needs for one view of the world.
pub struct ExtractedFrame {
    pub frame: GpuFrame,
    pub instances: Vec<GpuInstance>,
    pub lights: Vec<GpuLight>,
    /// A directional light exists, so the sun's shadow cascades are needed.
    pub has_sun: bool,
    /// Each local light casting a shadow this frame, in its list's order:
    /// its first view of the local shadow atlas and how many it takes.
    pub local_shadows: Vec<(u32, u32)>,
    /// What each instance was made from, for tracking motion between frames.
    pub entities: Vec<Owner>,
    pub post: PostProcess,
    /// The size the scene is drawn at: the view's own, or less when TAA
    /// upscales it (see `AntiAliasing::render_scale`).
    pub render_size: (u32, u32),
    /// The first Ocean, its Obj's height, and its sea now.
    pub ocean: Option<(crate::Ocean, f32, crate::SeaState)>,
    /// The Pools and Dry Areas nearest the camera.
    pub water_bodies: crate::water::WaterBodies,
    /// The Track Makers over a terrain (see `tracks`).
    pub track_makers: Vec<crate::tracks::Maker>,
    /// Where the world's middle is from the level's, in whole metres (see
    /// `voxear_scene::WorldOrigin`).
    pub origin: voxear_core::math::I64Vec3,
}

/// What an instance was made from: its Obj, for a plant of a Foliage its
/// place among the Foliage's plants, and for a part of a Model Renderer
/// which part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Owner {
    pub entity: Entity,
    pub plant: Option<u32>,
    pub part: Option<u32>,
}

/// What the game draws of what it may leave out, as its project's modules
/// decide: a world's resource, all of it when there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderFeatures {
    /// Foliage's plants.
    pub vegetation: bool,
    /// Oceans, Pools and Streams.
    pub water: bool,
    /// Terrains.
    pub terrain: bool,
}

impl Default for RenderFeatures {
    fn default() -> Self {
        Self { vegetation: true, water: true, terrain: true }
    }
}

/// Per-view settings that are not part of the scene.
#[derive(Clone, Copy, Debug)]
pub struct ViewOptions {
    pub debug_view: u32,
    /// Largest LOD error allowed on screen, in pixels.
    pub lod_threshold: f32,
    /// LOD fades complete in `1 / lod_fade_speed` seconds.
    pub lod_fade_speed: f32,
    /// Replaces the scene's post-processing, e.g. to compare presets.
    pub post_override: Option<PostProcess>,
    /// The Obj whose camera to look through, rather than the first camera
    /// found, as the editor's Scene view does.
    pub camera: Option<voxear_ecs::Entity>,
    /// Whether debug draw's lines from behaviours' gizmos show, as in the
    /// editor's Scene views with their Gizmos button on, but not its Game view.
    pub gizmos: bool,
    /// Whether each draw is culled meshlet by meshlet, against the frustum,
    /// the meshlets' normal cones and the depth pyramid, rather than drawn as
    /// whole LODs. On by default; off, to compare.
    pub meshlet_culling: bool,
    /// Draws the scene at this share of the view's size, whatever its
    /// post-processing says, as `vox render --render-scale` does.
    pub render_scale: Option<f32>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            debug_view: 0,
            lod_threshold: 1.0,
            lod_fade_speed: 4.0,
            post_override: None,
            camera: None,
            gizmos: true,
            meshlet_culling: true,
            render_scale: None,
        }
    }
}

/// Sky colours (linear) and ambient light, as a world resource. Without one
/// the scene gets a clear midday.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub mode: SkyMode,
    /// A gradient sky's colours.
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub ground: Vec3,
    /// Scales the ambient light the sky gives the scene.
    pub ambient: f32,
    /// How bright the stars are: 1 as the night sky shows them, 0 for none.
    /// They show only when the sky is dark, as at night under the moon.
    pub stars: f32,
    /// The atmosphere's haze: how much dust and moisture hangs in the air,
    /// 1 as on a clear day, more for a hazy, golden or smoggy one.
    pub haze: f32,
    /// Linear: the colour the haze scatters, white for moisture's grey,
    /// warmer for dust, which takes blue out.
    pub haze_tint: Vec3,
    /// How much the haze glows round the sun rather than evenly, 0 to 0.95:
    /// higher gives a brighter, tighter glow looking towards the sun.
    pub haze_glow: f32,
    /// How quickly haze builds up with distance over the scene: things
    /// count as this many times as far for the haze over them.
    pub distance_haze: f32,
    /// The sun's disc across the sky, in degrees: half a degree as on Earth.
    pub sun_size: f32,
    /// The clouds in it; none by default.
    pub clouds: Clouds,
}

/// Clouds in the sky: a layer of them between two heights, drifting with the
/// wind, lit by the sun and the sky, and shading the ground under them (see
/// `clouds`). With no cover there are none, and they cost nothing.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clouds {
    /// How much of the sky they cover, from 0 for a clear sky to 1 for
    /// overcast.
    pub cover: f32,
    /// How high their undersides are over the level's ground, in metres.
    pub height: f32,
    /// How tall they are from their undersides to their tops, in metres.
    pub thickness: f32,
    /// How far across a cloud is, in metres.
    pub size: f32,
    /// How thick they are to look through: 1 for fair-weather clouds, less
    /// for thin ones the sky shows through, more for heavy ones.
    pub density: f32,
    /// How fast they drift, in metres a second, the way the level's wind
    /// blows.
    pub drift: f32,
    /// How dark their shadows on the ground are, from 0 for none to 1.
    pub shadows: f32,
    /// Soft clouds, or clouds built of cubes.
    pub kind: CloudKind,
    /// How wide one cube of a cloud built of them is, in metres.
    pub block: f32,
}

/// What a sky's clouds are made of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloudKind {
    /// Heaped clouds with soft, billowing edges.
    #[default]
    Soft,
    /// The same clouds built of cubes, each flat-sided and one colour a
    /// face, for a world of blocks.
    Blocks,
}

impl Default for Clouds {
    fn default() -> Self {
        Self {
            cover: 0.0,
            height: 1200.0,
            thickness: 700.0,
            size: 900.0,
            density: 1.0,
            drift: 8.0,
            shadows: 0.8,
            kind: CloudKind::Soft,
            block: 100.0,
        }
    }
}

impl Clouds {
    /// Clouds covering `cover` of the sky, the rest as they are by default.
    pub fn covering(cover: f32) -> Self {
        Self { cover, ..Self::default() }
    }

    /// Clouds built of cubes covering `cover` of the sky, the rest as they
    /// are by default.
    pub fn blocks(cover: f32) -> Self {
        Self { cover, kind: CloudKind::Blocks, ..Self::default() }
    }

    /// Whether there are none.
    pub fn is_none(&self) -> bool {
        self.cover <= 0.0
    }
}

/// What the sky is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkyMode {
    /// A gradient of the sky's own colours, from the horizon up and down.
    #[default]
    Gradient,
    /// The Earth's air, lit by the sun: blue by day, red and
    /// orange as the sun sets, with haze over distant things (see
    /// `atmosphere`).
    Atmosphere,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            mode: SkyMode::Gradient,
            zenith: Vec3::new(0.23, 0.45, 0.85),
            horizon: Vec3::new(0.62, 0.72, 0.86),
            ground: Vec3::new(0.22, 0.2, 0.18),
            ambient: 1.0,
            stars: 1.0,
            haze: 1.0,
            haze_tint: Vec3::ONE,
            haze_glow: 0.8,
            distance_haze: 10.0,
            sun_size: 0.5357,
            clouds: Clouds::default(),
        }
    }
}

/// `frame_index` picks TAA's sub-pixel camera offset. `sphere` gives a
/// mesh's bounding sphere, which LOD Groups measure their size from.
pub fn extract(
    world: &World,
    (width, height): (u32, u32),
    (time, frame_index): (f32, u64),
    options: &ViewOptions,
    sphere: &dyn Fn(MeshHandle) -> Option<(Vec3, f32)>,
) -> ExtractedFrame {
    let aspect = width as f32 / height.max(1) as f32;
    // Nothing switched off draws, lights or looks: neither an Obj that is off nor a component.
    let switched = Switched::of(world);
    let on = |entity: Entity, switch: Switch| switched.is_on(world, entity, switch);
    let (camera, position, rotation) = world
        .query_ref::<(voxear_ecs::Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| on(*entity, Switch::Camera))
        .find(|(entity, ..)| options.camera.is_none_or(|chosen| chosen == *entity))
        .map(|(_, camera, local, world)| (camera, local, world))
        .map(|(camera, local, world)| {
            let (_, rotation, position) =
                world.map_or((local.scale, local.rotation, local.position), |w| w.0.to_scale_rotation_translation());
            (*camera, position, rotation)
        })
        .unwrap_or((Camera::default(), Vec3::new(0.0, 1.0, -5.0), Quat::IDENTITY));

    let sky = world.resource::<Sky>().copied().unwrap_or_default();
    let fov = camera.fov_y_degrees.to_radians();
    let view = Mat4::from_rotation_translation(rotation, position).inverse();
    let projection = vulkan::perspective_infinite_reverse(fov, aspect, camera.near);
    // The lights' depth slices start from a usual near distance even for a camera seeing from very close.
    let cluster_near = camera.near.max(0.1);
    let unjittered = projection * view;
    // Debug views show their colours as they are, the ambient occlusion view
    // the scene's own occlusion, on even where the scene turns it off.
    let scene_post = options.post_override.or_else(|| world.resource::<PostProcess>().copied()).unwrap_or_default();
    // Post volumes blend in where the camera is, unless a view sets its own settings.
    let scene_post = match options.post_override {
        Some(_) => scene_post,
        None => crate::post_volume::blend_volumes(world, scene_post, position),
    };
    let post = match options.debug_view {
        DEBUG_VIEW_NONE => scene_post,
        DEBUG_VIEW_AMBIENT_OCCLUSION => PostProcess {
            ambient_occlusion: AmbientOcclusion { enabled: true, ..scene_post.ambient_occlusion },
            ..PostProcess::debug_view()
        },
        DEBUG_VIEW_GLOBAL_ILLUMINATION => PostProcess {
            global_illumination: GlobalIllumination { enabled: true, ..scene_post.global_illumination },
            ..PostProcess::debug_view()
        },
        _ => PostProcess::debug_view(),
    };
    // With TAA, the scene may be drawn smaller and upscaled; LODs are still picked for the view's own size.
    let output_height = height;
    let taa = post.anti_aliasing.mode == AntiAliasingMode::Taa;
    let scale = match taa {
        true => options.render_scale.unwrap_or(post.anti_aliasing.render_scale).clamp(0.25, 1.0),
        false => 1.0,
    };
    let (width, height) = render_size((width, height), scale);
    // Each drawn pixel covers 1 / scale² of the output's, so the jitter needs as many more phases to reach them.
    let phases = ((8.0 / (scale * scale)).ceil() as u64).clamp(8, 64);
    let jitter = match taa {
        true => taa_jitter(frame_index, post.anti_aliasing.taa_jitter, phases),
        false => [0.0; 2],
    };
    // Offsetting the column that multiplies view depth shifts the image by a constant amount in NDC.
    let mut jittered_projection = projection;
    jittered_projection.z_axis.x += jitter[0] * 2.0 / width as f32;
    jittered_projection.z_axis.y += jitter[1] * 2.0 / height as f32;
    let view_proj = jittered_projection * view;

    let mut gpu_post = post.to_gpu(sky.horizon);
    gpu_post.ao_unproject = [1.0 / projection.x_axis.x, 1.0 / projection.y_axis.y];
    // The fog's base is measured from the level's ground, wherever the world's middle has moved to.
    let origin = voxear_scene::WorldOrigin::of(world).0;
    gpu_post.fog_base_height = (f64::from(gpu_post.fog_base_height) - origin.y as f64) as f32;
    gpu_post.lut_texture = post.grading.lut.map_or(NO_TEXTURE, |lut| lut.0);
    let mut frame = GpuFrame {
        view_proj: cols(view_proj),
        planes: frustum_planes(position, rotation, fov, aspect, camera.near),
        camera_position: vec3(position),
        camera_forward: vec3(rotation * Vec3::Z),
        near: camera.near,
        lod_scale: output_height as f32 / (2.0 * (fov * 0.5).tan()),
        lod_threshold: options.lod_threshold,
        debug_view: options.debug_view,
        hiz_texture: NO_TEXTURE,
        shadow_texture: NO_TEXTURE,
        view: cols(view),
        unjittered_view_proj: cols(unjittered),
        jitter,
        post: gpu_post,
        cluster_grid: CLUSTER_GRID,
        cluster_max_lights: MAX_CLUSTER_LIGHTS,
        cluster_scale: CLUSTER_GRID[2] as f32 / (CLUSTER_FAR / cluster_near).ln(),
        cluster_bias: CLUSTER_GRID[2] as f32 * cluster_near.ln() / (CLUSTER_FAR / cluster_near).ln(),
        tile_size: [width as f32 / CLUSTER_GRID[0] as f32, height as f32 / CLUSTER_GRID[1] as f32],
        lod_fade_speed: options.lod_fade_speed,
        time,
        sun_direction: vec3(Vec3::new(0.3, -0.8, 0.5).normalize()),
        sun_color: [0.0; 3],
        exposure: camera.exposure,
        sky_zenith: vec3(sky.zenith),
        sky_horizon: vec3(sky.horizon),
        ground_color: vec3(sky.ground),
        ambient: sky.ambient,
        stars: sky.stars.max(0.0),
        viewport: [width, height],
        inv_view_proj: cols(view_proj.inverse()),
        lighting: GpuLighting {
            sky: match sky.mode {
                SkyMode::Gradient => SKY_GRADIENT,
                SkyMode::Atmosphere => SKY_ATMOSPHERE,
            },
            atmosphere: crate::atmosphere::of_sky(&sky),
            clouds: crate::clouds::gpu(&sky.clouds),
            sun_cookie: NO_TEXTURE,
            transmittance: NO_TEXTURE,
            multi_scattering: NO_TEXTURE,
            sky_view: NO_TEXTURE,
            aerial_perspective: NO_TEXTURE,
            ..GpuLighting::default()
        },
        origin: crate::gpu::frame_origin(origin),
        ..Default::default()
    };

    // What LOD Groups hide from this camera is neither drawn nor lit.
    let lods = LodChoices::new(world, LodCamera::new(position, camera.fov_y_degrees), sphere);
    let mut lights = Vec::new();
    // The local lights that may cast shadows, by their index in `lights`.
    let mut shadowed = Vec::new();
    let mut sun_found = false;
    let shining = world.query_ref::<(Entity, &Light, &Transform)>().without::<Destroying>();
    let sun_look = world.resource::<crate::looks::SunLook>().copied();
    let lit = shining.into_iter().filter(|(entity, ..)| !lods.hidden.contains(entity) && on(*entity, Switch::Light));
    for (_, light, transform) in lit {
        // A look changes the sun's colour and strength, and gives what the light leaves at its default.
        let looked;
        let light = match (light.kind, sun_look) {
            (LightKind::Directional, Some(look)) => {
                looked = Light {
                    color: light.light_color() * look.tint,
                    temperature: None,
                    intensity: light.intensity * look.strength,
                    softness: if light.softness == 0.5 { look.softness } else { light.softness },
                    shadow_tint: if light.shadow_tint == Vec3::ZERO { look.shadow_tint } else { light.shadow_tint },
                    ..*light
                };
                &looked
            }
            _ => light,
        };
        let forward = transform.rotation * Vec3::Z;
        let radiance = light.light_color() * light.intensity;
        match light.kind {
            LightKind::Directional if !sun_found => {
                sun_found = true;
                frame.sun_direction = vec3(forward);
                frame.sun_color = vec3(radiance);
                frame.lighting.sun_softness = (light.softness.clamp(0.0, 20.0).to_radians() * 0.5).tan();
                frame.lighting.sun_shadow_tint = vec3(light.shadow_tint.max(Vec3::ZERO));
                frame.lighting.moon = u32::from(light.body == SkyBody::Moon);
                if let Some(cookie) = light.cookie {
                    frame.lighting.sun_cookie = cookie.0;
                    frame.lighting.sun_cookie_size = light.cookie_size;
                    frame.lighting.sun_right = vec3(transform.rotation * Vec3::X);
                    frame.lighting.sun_up = vec3(transform.rotation * Vec3::Y);
                }
            }
            LightKind::Directional => {}
            _ => {
                let gpu = gpu_light(light, transform, radiance);
                if light.shadows && light.range > 0.0 {
                    shadowed.push((lights.len(), shadow_light(light, transform, &gpu)));
                }
                lights.push(gpu);
            }
        }
    }
    frame.light_count = lights.len() as u32;
    let local_shadows = local_shadows(&mut frame, &mut lights, shadowed);
    // What pushes through foliage, the nearest the camera first.
    let pushing = world.query_ref::<(Entity, &crate::FoliageInteraction, &Transform, Option<&WorldTransform>)>();
    let pushers = pushing
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| on(*entity, Switch::FoliageInteraction))
        .map(|(_, push, transform, at)| (at.map_or(transform.position, |w| w.0.w_axis.truncate()), *push))
        .collect();
    frame.pushes = crate::foliage::pushes(position, pushers);
    // What presses tracks into the ground, each with how high it is above the ground under it.
    let pressing = world.query_ref::<(Entity, &crate::TrackMaker, &Transform, Option<&WorldTransform>)>();
    let track_makers = pressing
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| on(*entity, Switch::TrackMaker))
        .filter_map(|(entity, maker, transform, at)| {
            let at = at.map_or(transform.position, |w| w.0.w_axis.truncate());
            let ground = voxear_terrain::ground_at(world, at.x, at.z)?;
            Some((entity, at, at.y - ground.height, *maker))
        })
        .collect();
    // The first Global Wind Zone that is on blows over the whole level.
    let zones = world.query_ref::<(Entity, &crate::WindZone, &Transform)>().without::<Destroying>();
    let global = |(entity, zone, _): &(Entity, &crate::WindZone, &Transform)| {
        zone.shape == crate::WindShape::Global && on(*entity, Switch::WindZone)
    };
    let zone = zones.into_iter().find(global).map(|(_, zone, transform)| (*zone, transform.rotation));
    if let Some((zone, rotation)) = zone {
        frame.wind = zone.gpu(rotation);
    }
    // The first Ocean is the sea, at its Obj's height: as its wind has eased it where the engine runs, or as its wind
    // makes it now.
    let seas = world.query_ref::<(&crate::Ocean, &Transform, Option<&WorldTransform>, Option<&crate::SeaNow>)>().without::<Destroying>();
    let ocean = seas.into_iter().next().map(|(ocean, transform, at, now)| {
        let (_, rotation, position) = at.map_or((Vec3::ONE, transform.rotation, transform.position), |w| {
            w.0.to_scale_rotation_translation()
        });
        let blown = || ocean.sea_blown(rotation, zone.map(|(zone, along)| (along, zone.strength)));
        (*ocean, position.y, now.map_or_else(blown, |now| now.0))
    });
    let water_bodies = water_bodies(world, position);
    // Lava lights what is round it.
    lights.extend(crate::water_bodies::lava_lights(&water_bodies, position));
    // And Particle Systems that cast a light: a fire's flicker, an explosion's flash.
    lights.extend(crate::particles::lights(world, position, crate::particles::time_of(world, time)));
    frame.light_count = lights.len() as u32;
    // And the first Seasons turns all its plants.
    let seasons = world.query_ref::<(Entity, &crate::Seasons)>().without::<Destroying>();
    if let Some((_, seasons)) = seasons.into_iter().find(|(entity, _)| on(*entity, Switch::Seasons)) {
        frame.seasons = seasons.gpu();
    }

    if sun_found {
        let view = shadows::View { position, rotation, fov_y: fov, aspect, near: camera.near, origin };
        let (distance, split) = sun_look.map_or((shadows::SHADOW_DISTANCE, shadows::SHADOW_SPLIT), |look| {
            (look.shadow_distance, look.shadow_split)
        });
        let cascades = shadows::cascades(&view, Vec3::from(frame.sun_direction), distance, split);
        for (i, cascade) in cascades.iter().enumerate().take(CASCADES) {
            frame.cascade_view_proj[i] = cols(cascade.view_proj);
            frame.cascade_planes[i] = cascade.planes;
            frame.cascade_far[i] = cascade.far;
            frame.cascade_texel[i] = cascade.texel;
        }
    }

    let mut entities = Vec::new();
    let any_hidden = world.query_ref::<&Hidden>().into_iter().next().is_some();
    let shown = |entity: Entity| !lods.hidden.contains(&entity) && !(any_hidden && hidden(world, entity));
    let instances: Vec<GpuInstance> = world
        .query_ref::<(Entity, &MeshRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| shown(*entity) && on(*entity, Switch::Renderer))
        .map(|(entity, renderer, transform, world_transform)| {
            entities.push(Owner { entity, plant: None, part: None });
            let model = world_transform.map_or_else(
                || Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
                |w| w.0,
            );
            let normal = Mat3::from_mat4(model).inverse().transpose();
            GpuInstance {
                model: cols(model),
                prev_model: cols(model),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                mesh: renderer.mesh.0,
                material: renderer.material.0,
                flags: renderer.lod_bits().0,
                lod_bias: renderer.lod_bits().1,
                lod_screen: renderer.lod_bits().2,
                fade: [0.0; 4],
                ..GpuInstance::default()
            }
        })
        .collect();
    let mut instances = instances;
    // Each part of a model an Obj draws whole, where the model puts it.
    let models = world.query_ref::<(Entity, &crate::ModelRenderer, &Transform, Option<&WorldTransform>)>();
    for (entity, drawn, transform, world_transform) in models.without::<Destroying>() {
        if !shown(entity) || !on(entity, Switch::Model) {
            continue;
        }
        let obj = world_transform.map_or_else(
            || Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
            |w| w.0,
        );
        let lod = MeshRenderer { lod_bias: drawn.lod_bias, ..MeshRenderer::new(MeshHandle(0), MaterialHandle(0)) };
        let (flags, lod_bias, lod_screen) = lod.lod_bits();
        for (index, part) in drawn.parts.iter().enumerate() {
            entities.push(Owner { entity, plant: None, part: Some(index as u32) });
            let model = obj * part.transform;
            let normal = Mat3::from_mat4(model).inverse().transpose();
            instances.push(GpuInstance {
                model: cols(model),
                prev_model: cols(model),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                mesh: part.mesh.0,
                material: part.material.0,
                flags,
                lod_bias,
                lod_screen,
                fade: [0.0; 4],
                ..GpuInstance::default()
            });
        }
    }
    frame.instance_count = instances.len() as u32;

    ExtractedFrame {
        frame,
        instances,
        lights,
        has_sun: sun_found,
        local_shadows,
        entities,
        post,
        render_size: (width, height),
        ocean,
        water_bodies,
        track_makers,
        origin: voxear_scene::WorldOrigin::of(world).0,
    }
}

/// Inward-facing side and near planes of an infinite perspective frustum.
/// Whether `entity` or an Obj it is under is `Hidden`.
pub(crate) fn hidden(world: &World, entity: Entity) -> bool {
    let mut at = Some(entity);
    // As deep as any hierarchy goes, and no further should one loop.
    for _ in 0..64 {
        let Some(here) = at else { return false };
        if world.has::<Hidden>(here) {
            return true;
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    false
}

fn frustum_planes(position: Vec3, rotation: Quat, fov_y: f32, aspect: f32, near: f32) -> [[f32; 4]; 6] {
    let forward = rotation * Vec3::Z;
    let right = rotation * Vec3::X;
    let up = rotation * Vec3::Y;
    let half_v = fov_y * 0.5;
    let half_h = (half_v.tan() * aspect).atan();
    let plane = |normal: Vec3| {
        let normal = normal.normalize();
        [normal.x, normal.y, normal.z, -normal.dot(position)]
    };
    let near_plane = [forward.x, forward.y, forward.z, -forward.dot(position) - near];
    [
        plane(right * half_h.cos() + forward * half_h.sin()),
        plane(-right * half_h.cos() + forward * half_h.sin()),
        plane(up * half_v.cos() + forward * half_v.sin()),
        plane(-up * half_v.cos() + forward * half_v.sin()),
        near_plane,
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The size a view of `size` is drawn at, at `scale` of it, at least a
/// pixel each way.
pub(crate) fn render_size((width, height): (u32, u32), scale: f32) -> (u32, u32) {
    let scaled = |value: u32| ((value as f32 * scale).round() as u32).clamp(1, value.max(1));
    (scaled(width), scaled(height))
}

/// A local light as its shadow is drawn. It starts to cast a little past its
/// own shape, so the bulb, flame or tube drawn round it does not shut its
/// light in.
fn shadow_light(light: &Light, transform: &Transform, gpu: &GpuLight) -> shadows::LocalLight {
    let extent = match light.kind {
        LightKind::Rectangle => Vec3::new(gpu.size[0], gpu.size[1], 0.0).length(),
        LightKind::Disc => gpu.size[0],
        LightKind::Tube => gpu.size[0] + gpu.size[1],
        _ => 0.0,
    };
    shadows::LocalLight {
        position: transform.position,
        rotation: transform.rotation,
        range: light.range + extent,
        near: (extent * 1.1).max(0.1),
        cone_degrees: (light.kind == LightKind::Spot).then_some(light.spot_outer_degrees),
    }
}

/// Gives the lights that cast shadows this frame their views of the atlas:
/// those whose reach the camera is nearest first, up to
/// `LOCAL_SHADOW_LIGHTS` of them and as many as the views go round. The
/// rest light without a shadow this frame. Returns each one's first view
/// and how many it takes.
fn local_shadows(
    frame: &mut GpuFrame,
    lights: &mut [GpuLight],
    mut shadowed: Vec<(usize, shadows::LocalLight)>,
) -> Vec<(u32, u32)> {
    let camera = Vec3::from(frame.camera_position);
    let visible = |light: &shadows::LocalLight| {
        frame.planes.iter().all(|p| Vec3::new(p[0], p[1], p[2]).dot(light.position) + p[3] >= -light.range)
    };
    shadowed.retain(|(_, light)| visible(light));
    shadowed.sort_by(|(_, a), (_, b)| {
        let reach = |light: &shadows::LocalLight| camera.distance(light.position) - light.range;
        reach(a).total_cmp(&reach(b))
    });
    let mut views = 0;
    let mut drawn = Vec::new();
    for (index, light) in shadowed {
        let faces = shadows::local_views(&light);
        if drawn.len() == LOCAL_SHADOW_LIGHTS || views + faces.len() > LOCAL_SHADOW_VIEWS {
            continue;
        }
        lights[index].shadow = views as u32;
        frame.local_shadows.casters[drawn.len()] = [light.position.x, light.position.y, light.position.z, light.range];
        drawn.push((views as u32, faces.len() as u32));
        for face in faces {
            frame.local_shadows.view_proj[views] = cols(face);
            views += 1;
        }
    }
    frame.local_shadows.count = drawn.len() as u32;
    drawn
}

/// A local light as the shaders read it. A shape's size is halved, as they
/// measure from its middle, and a disc's radius doubled into both halves,
/// as it is drawn as a polygon of that size (see `shaders/lights.slang`).
fn gpu_light(light: &Light, transform: &Transform, radiance: Vec3) -> GpuLight {
    let scale = |size: f32| size.max(0.0);
    let (kind, size) = match light.kind {
        LightKind::Spot => (LIGHT_SPOT, [0.0; 2]),
        LightKind::Rectangle => (LIGHT_RECTANGLE, [scale(light.width) * 0.5, scale(light.height) * 0.5]),
        LightKind::Disc => (LIGHT_DISC, [scale(light.radius); 2]),
        LightKind::Tube => (LIGHT_TUBE, [scale(light.length) * 0.5, scale(light.radius)]),
        LightKind::Point | LightKind::Directional => (LIGHT_POINT, [0.0; 2]),
    };
    GpuLight {
        position: vec3(transform.position),
        range: light.range,
        color: vec3(radiance),
        kind,
        direction: vec3(transform.rotation * Vec3::Z),
        cos_outer: (light.spot_outer_degrees.to_radians() * 0.5).cos(),
        right: vec3(transform.rotation * Vec3::X),
        cos_inner: (light.spot_inner_degrees.to_radians() * 0.5).cos(),
        size,
        cookie: light.cookie.filter(|_| light.kind == LightKind::Spot).map_or(NO_TEXTURE, |cookie| cookie.0),
        profile: light
            .profile
            .filter(|_| matches!(light.kind, LightKind::Point | LightKind::Spot))
            .map_or(NO_TEXTURE, |profile| profile.0),
        shadow: NO_SHADOW,
        _pad: [0; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::temperature_color;

    #[test]
    fn a_candle_is_orange_daylight_white_and_a_blue_sky_blue() {
        let luminance = |c: voxear_core::math::Vec3| 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
        let candle = temperature_color(1900.0);
        assert!(candle.x > candle.y * 1.5 && candle.y > candle.z * 2.0, "a candle is orange: {candle}");
        let daylight = temperature_color(6500.0);
        assert!((daylight - voxear_core::math::Vec3::ONE).abs().max_element() < 0.06, "daylight is white: {daylight}");
        let sky = temperature_color(10000.0);
        assert!(sky.z > sky.x * 1.3, "a clear sky is blue: {sky}");
        for kelvin in [1900.0, 2700.0, 6500.0, 10000.0] {
            let bright = luminance(temperature_color(kelvin));
            assert!((bright - 1.0).abs() < 0.02, "only the colour changes, not the brightness: {bright} at {kelvin}");
        }
    }

    use super::*;

    fn inside(planes: &[[f32; 4]; 6], point: Vec3, radius: f32) -> bool {
        planes.iter().all(|p| Vec3::new(p[0], p[1], p[2]).dot(point) + p[3] >= -radius)
    }

    #[test]
    fn frustum_keeps_what_is_in_front_and_drops_the_rest() {
        let planes = frustum_planes(Vec3::ZERO, Quat::IDENTITY, 60f32.to_radians(), 16.0 / 9.0, 0.1);
        assert!(inside(&planes, Vec3::new(0.0, 0.0, 10.0), 0.5));
        assert!(inside(&planes, Vec3::new(0.0, 0.0, 10_000.0), 0.5), "the far plane is at infinity");
        assert!(!inside(&planes, Vec3::new(0.0, 0.0, -10.0), 0.5), "behind the camera");
        assert!(!inside(&planes, Vec3::new(50.0, 0.0, 10.0), 0.5), "far to the right");
        assert!(!inside(&planes, Vec3::new(0.0, 20.0, 10.0), 0.5), "far above");
        assert!(inside(&planes, Vec3::new(5.9, 0.0, 10.0), 0.5), "just inside the right edge");
    }

    #[test]
    fn camera_looks_along_positive_z() {
        let view = Mat4::from_rotation_translation(Quat::IDENTITY, Vec3::ZERO).inverse();
        let clip = vulkan::perspective_infinite_reverse(1.0, 1.0, 0.1) * view * Vec4::new(0.0, 1.0, 5.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(clip.w > 0.0, "points in front have positive w");
        assert!(ndc.y < 0.0, "up in the world is up on screen, which is -Y in Vulkan");
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "reverse-Z depth lies in (0, 1)");
    }
}

/// The Pools, the Streams and then the Dry Areas nearest `camera`, as many
/// as a frame draws, and the points along the Streams.
fn water_bodies(world: &World, camera: Vec3) -> crate::water::WaterBodies {
    use crate::water_bodies::{DryArea, MAX_STREAM_POINTS, MAX_WATER_BODIES, Pool, Stream};
    let placed = |transform: &Transform, at: Option<&WorldTransform>| {
        at.map_or((transform.position, transform.rotation), |w| {
            let (_, rotation, position) = w.0.to_scale_rotation_translation();
            (position, rotation)
        })
    };
    let mut pools: Vec<(f32, crate::gpu::GpuWaterBody)> = world
        .query_ref::<(&Pool, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(pool, transform, at)| {
            let (position, rotation) = placed(transform, at);
            (position.distance_squared(camera), pool.gpu(position, rotation))
        })
        .collect();
    // Sphere and Box Wind Zones go with the Dry Areas, which the shaders tell apart by their kind.
    type Dry = (f32, crate::gpu::GpuWaterBody, Option<crate::MeshHandle>, Option<crate::gpu::GpuWaterBody>);
    let switched = Switched::of(world);
    let gusts: Vec<Dry> = world
        .query_ref::<(Entity, &crate::WindZone, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, zone, ..)| zone.shape != crate::WindShape::Global && switched.is_on(world, *entity, Switch::WindZone))
        .map(|(_, zone, transform, at)| {
            let (position, rotation) = placed(transform, at);
            (position.distance_squared(camera), zone.gpu_local(position, rotation), None, None)
        })
        .collect();
    // A mesh Dry Area takes its own mesh or its Obj's Mesh Renderer's; with neither it keeps nothing dry. Each brings
    // its bilge, if it has one.
    let mut dries: Vec<Dry> = world
        .query_ref::<(&DryArea, &Transform, Option<&WorldTransform>, Option<&MeshRenderer>)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(dry, transform, at, drawn)| {
            let (position, rotation) = placed(transform, at);
            let near = position.distance_squared(camera);
            match dry.shape {
                crate::DryShape::Mesh => {
                    let mesh = dry.mesh.or(drawn.map(|renderer| renderer.mesh))?;
                    let model = at.map_or_else(|| transform.matrix(), |w| w.0);
                    Some((near, DryArea::gpu_mesh(mesh, model), Some(mesh), dry.gpu_bilge(model)))
                }
                _ => {
                    let bilge = dry.gpu_bilge(Mat4::from_rotation_translation(rotation, position));
                    Some((near, dry.gpu(position, rotation), None, bilge))
                }
            }
        })
        .collect();
    // The water a Stream can run into: the sea, at its level, and each Pool of water.
    let sea = world.query_ref::<(&crate::Ocean, &Transform, Option<&WorldTransform>)>().into_iter().next().map(|(_, t, at)| placed(t, at).0.y);
    let ponds: Vec<(Pool, Vec3, Quat)> = world
        .query_ref::<(&Pool, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(pool, ..)| pool.liquid == crate::Liquid::Water)
        .map(|(pool, t, at)| {
            let (position, rotation) = placed(t, at);
            (*pool, position, rotation)
        })
        .collect();
    // Each Stream's points along its Path, by how near the nearest of them is; a Stream of water running into a Pool
    // or the sea eases into it at its mouth.
    let mut streams: Vec<(f32, Stream, Vec<crate::water_bodies::StreamPoint>, Option<usize>)> = world
        .query_ref::<(&Stream, &voxear_scene::Path, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(stream, path, transform, at)| {
            let mut points = stream.points(path, at.map_or_else(|| transform.matrix(), |w| w.0));
            let mut pours = None;
            if let Some(end) = points.last().map(|p| p.position).filter(|_| stream.liquid == crate::Liquid::Water) {
                let pond = ponds.iter().find_map(|(pool, position, rotation)| {
                    pool.surface_over(*position, *rotation, end).filter(|surface| end.y < surface + 1.0)
                });
                let meets = pond.or(sea.filter(|level| end.y < level + 1.0));
                if let Some(level) = meets {
                    pours = crate::water_bodies::run_into(&mut points, level);
                }
            }
            let near = points.iter().map(|p| p.position.distance_squared(camera)).fold(f32::INFINITY, f32::min);
            (near, *stream, points, pours)
        })
        .collect();
    // Where Streams of water run into each other.
    let mut joining: Vec<crate::water_bodies::Joining> = streams
        .iter_mut()
        .map(|(_, stream, points, pours)| (stream.liquid == crate::Liquid::Water, std::mem::take(points), *pours))
        .collect();
    crate::water_bodies::join_streams(&mut joining);
    for ((_, _, points, pours), (_, joined, poured)) in streams.iter_mut().zip(joining) {
        (*points, *pours) = (joined, poured);
    }
    pools.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The blocks' water the camera is in, first: a Pool over the same place is found after it, and is the one seen.
    if let Some(body) = crate::water_bodies::block_water(world, camera) {
        pools.insert(0, (0.0, body));
    }
    streams.sort_by(|a, b| a.0.total_cmp(&b.0));
    dries.extend(gusts);
    dries.sort_by(|a, b| a.0.total_cmp(&b.0));
    pools.truncate(MAX_WATER_BODIES / 2);
    streams.truncate(MAX_WATER_BODIES / 4);
    // Bilges are Pools, listed after the others, and find their Dry Area once it is listed.
    let bilges = MAX_WATER_BODIES / 2 - pools.len();
    let mut bilges: Vec<(usize, crate::gpu::GpuWaterBody)> =
        dries.iter().enumerate().filter_map(|(at, dry)| Some((at, dry.3?))).take(bilges).collect();
    dries.truncate(MAX_WATER_BODIES - pools.len() - bilges.len() - streams.len());
    bilges.retain(|(at, _)| *at < dries.len());
    let mut bodies: Vec<crate::gpu::GpuWaterBody> = pools.iter().map(|(_, body)| *body).chain(bilges.iter().map(|(_, body)| *body)).collect();
    let listed = (pools.len(), bodies.len());
    let (mut counts, mut points) = (Vec::new(), Vec::new());
    for (_, stream, along, pours) in &streams {
        let room = MAX_STREAM_POINTS - points.len();
        if along.len() < 2 || along.len() > room {
            continue;
        }
        bodies.push(stream.gpu(points.len() as u32, along.len() as u32, *pours));
        counts.push(along.len() as u32);
        points.extend(along.iter().map(|p| p.gpu()));
    }
    let first_dry = bodies.len() as u32;
    for (body, (dry, _)) in bodies[listed.0..listed.1].iter_mut().zip(&bilges) {
        body.shape = first_dry + *dry as u32 + 1;
    }
    let dry_meshes = (first_dry..).zip(&dries).filter_map(|(body, (_, _, mesh, _))| mesh.map(|mesh| (body, mesh))).collect();
    bodies.extend(dries.into_iter().map(|(_, body, ..)| body));
    crate::water::WaterBodies { bodies, pools: listed.1 as u32, streams: counts, points, dry_meshes }
}
