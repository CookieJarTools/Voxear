//! Rendering components on Objs, and extraction of what to draw from the
//! world each frame.

use voxear_core::math::{Mat3, Mat4, Quat, Vec3, Vec4, camera::lh::proj::vulkan};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::TextureId;
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    assets::{MaterialHandle, MeshHandle},
    lod_group::{LodCamera, LodChoices},
    gpu::{
        CLUSTER_FAR, CLUSTER_GRID, DEBUG_VIEW_AMBIENT_OCCLUSION, DEBUG_VIEW_NONE, GpuFrame, GpuInstance, GpuLight, GpuLighting,
        LIGHT_DISC, LIGHT_POINT, LIGHT_RECTANGLE, LIGHT_SPOT, LIGHT_TUBE, MAX_CLUSTER_LIGHTS, NO_TEXTURE, SKY_ATMOSPHERE, SKY_GRADIENT, cols, vec3,
    },
    post::{AmbientOcclusion, AntiAliasingMode, PostProcess, taa_jitter},
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
    pub color: Vec3,
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
    /// degree. Only the sun casts shadows yet.
    pub softness: f32,
    /// An IES profile (see `Renderer::add_light_profile`): how bright a real
    /// fitting is in each direction, for a point or spot light.
    pub profile: Option<TextureId>,
}
impl Component for Light {}

impl Light {
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
            profile: None,
        }
    }
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
    /// The entity behind each instance, for tracking motion between frames.
    pub entities: Vec<Entity>,
    pub post: PostProcess,
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
}

/// What the sky is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkyMode {
    /// A gradient of the sky's own colours, from the horizon up and down.
    #[default]
    Gradient,
    /// The air of a planet like Earth, lit by the sun: blue by day, red and
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
    let (camera, position, rotation) = world
        .query_ref::<(voxear_ecs::Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
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
    let unjittered = projection * view;
    // Debug views show their colours as they are, the ambient occlusion view
    // the scene's own occlusion, on even where the scene turns it off.
    let scene_post = options.post_override.or_else(|| world.resource::<PostProcess>().copied()).unwrap_or_default();
    let post = match options.debug_view {
        DEBUG_VIEW_NONE => scene_post,
        DEBUG_VIEW_AMBIENT_OCCLUSION => PostProcess {
            ambient_occlusion: AmbientOcclusion { enabled: true, ..scene_post.ambient_occlusion },
            ..PostProcess::debug_view()
        },
        _ => PostProcess::debug_view(),
    };
    let jitter = match post.anti_aliasing.mode {
        AntiAliasingMode::Taa => taa_jitter(frame_index, post.anti_aliasing.taa_jitter),
        _ => [0.0; 2],
    };
    // Offsetting the column that multiplies view depth shifts the image by a constant amount in NDC.
    let mut jittered_projection = projection;
    jittered_projection.z_axis.x += jitter[0] * 2.0 / width as f32;
    jittered_projection.z_axis.y += jitter[1] * 2.0 / height as f32;
    let view_proj = jittered_projection * view;

    let mut gpu_post = post.to_gpu(sky.horizon);
    gpu_post.ao_unproject = [1.0 / projection.x_axis.x, 1.0 / projection.y_axis.y];
    let mut frame = GpuFrame {
        view_proj: cols(view_proj),
        planes: frustum_planes(position, rotation, fov, aspect, camera.near),
        camera_position: vec3(position),
        camera_forward: vec3(rotation * Vec3::Z),
        near: camera.near,
        lod_scale: height as f32 / (2.0 * (fov * 0.5).tan()),
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
        cluster_scale: CLUSTER_GRID[2] as f32 / (CLUSTER_FAR / camera.near).ln(),
        cluster_bias: CLUSTER_GRID[2] as f32 * camera.near.ln() / (CLUSTER_FAR / camera.near).ln(),
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
        viewport: [width, height],
        inv_view_proj: cols(view_proj.inverse()),
        lighting: GpuLighting {
            sky: match sky.mode {
                SkyMode::Gradient => SKY_GRADIENT,
                SkyMode::Atmosphere => SKY_ATMOSPHERE,
            },
            atmosphere: crate::atmosphere::earth(),
            sun_cookie: NO_TEXTURE,
            transmittance: NO_TEXTURE,
            multi_scattering: NO_TEXTURE,
            sky_view: NO_TEXTURE,
            aerial_perspective: NO_TEXTURE,
            ..GpuLighting::default()
        },
        ..Default::default()
    };

    // What LOD Groups hide from this camera is neither drawn nor lit.
    let lods = LodChoices::new(world, LodCamera::new(position, camera.fov_y_degrees), sphere);
    let mut lights = Vec::new();
    let mut sun_found = false;
    let shining = world.query_ref::<(Entity, &Light, &Transform)>().without::<Destroying>();
    for (_, light, transform) in shining.into_iter().filter(|(entity, ..)| !lods.hidden.contains(entity)) {
        let forward = transform.rotation * Vec3::Z;
        let radiance = light.color * light.intensity;
        match light.kind {
            LightKind::Directional if !sun_found => {
                sun_found = true;
                frame.sun_direction = vec3(forward);
                frame.sun_color = vec3(radiance);
                frame.lighting.sun_softness = (light.softness.clamp(0.0, 20.0).to_radians() * 0.5).tan();
                if let Some(cookie) = light.cookie {
                    frame.lighting.sun_cookie = cookie.0;
                    frame.lighting.sun_cookie_size = light.cookie_size;
                    frame.lighting.sun_right = vec3(transform.rotation * Vec3::X);
                    frame.lighting.sun_up = vec3(transform.rotation * Vec3::Y);
                }
            }
            LightKind::Directional => {}
            _ => lights.push(gpu_light(light, transform, radiance)),
        }
    }
    frame.light_count = lights.len() as u32;

    if sun_found {
        let view = shadows::View { position, rotation, fov_y: fov, aspect, near: camera.near };
        let cascades = shadows::cascades(&view, Vec3::from(frame.sun_direction));
        for (i, cascade) in cascades.iter().enumerate().take(CASCADES) {
            frame.cascade_view_proj[i] = cols(cascade.view_proj);
            frame.cascade_planes[i] = cascade.planes;
            frame.cascade_far[i] = cascade.far;
            frame.cascade_texel[i] = cascade.texel;
        }
    }

    let mut entities = Vec::new();
    let instances: Vec<GpuInstance> = world
        .query_ref::<(Entity, &MeshRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| !lods.hidden.contains(entity))
        .map(|(entity, renderer, transform, world_transform)| {
            entities.push(entity);
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
            }
        })
        .collect();
    frame.instance_count = instances.len() as u32;

    ExtractedFrame { frame, instances, lights, has_sun: sun_found, entities, post }
}

/// Inward-facing side and near planes of an infinite perspective frustum.
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
    }
}

#[cfg(test)]
mod tests {
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
