//! Rendering components on Objs, and extraction of what to draw from the
//! world each frame.

use voxear_core::math::{Mat3, Mat4, Quat, Vec3, Vec4, camera::lh::proj::vulkan};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    assets::{MaterialHandle, MeshHandle},
    gpu::{
        CLUSTER_FAR, CLUSTER_GRID, DEBUG_VIEW_NONE, GpuFrame, GpuInstance, GpuLight, LIGHT_POINT, LIGHT_SPOT,
        MAX_CLUSTER_LIGHTS, NO_TEXTURE, cols, vec3,
    },
    post::{AntiAliasingMode, PostProcess, taa_jitter},
    shadows::{self, CASCADES},
};

/// Renders the scene from this Obj, looking along its +Z like Unity.
#[derive(Clone, Copy, Debug)]
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
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub kind: LightKind,
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub spot_inner_degrees: f32,
    pub spot_outer_degrees: f32,
}
impl Component for Light {}

impl Light {
    pub fn directional(color: Vec3, intensity: f32) -> Self {
        Self {
            kind: LightKind::Directional,
            color,
            intensity,
            range: 0.0,
            spot_inner_degrees: 0.0,
            spot_outer_degrees: 0.0,
        }
    }

    pub fn point(color: Vec3, intensity: f32, range: f32) -> Self {
        Self { kind: LightKind::Point, color, intensity, range, spot_inner_degrees: 0.0, spot_outer_degrees: 0.0 }
    }

    pub fn spot(color: Vec3, intensity: f32, range: f32, inner_degrees: f32, outer_degrees: f32) -> Self {
        Self {
            kind: LightKind::Spot,
            color,
            intensity,
            range,
            spot_inner_degrees: inner_degrees,
            spot_outer_degrees: outer_degrees,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MeshRenderer {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}
impl Component for MeshRenderer {}

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
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self { debug_view: 0, lod_threshold: 1.0, lod_fade_speed: 4.0, post_override: None }
    }
}

/// Sky colours (linear) and ambient light, as a world resource. Without one
/// the scene gets a clear midday.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub ground: Vec3,
    /// Scales the ambient light the sky gives the scene.
    pub ambient: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            zenith: Vec3::new(0.23, 0.45, 0.85),
            horizon: Vec3::new(0.62, 0.72, 0.86),
            ground: Vec3::new(0.22, 0.2, 0.18),
            ambient: 1.0,
        }
    }
}

/// `frame_index` picks TAA's sub-pixel camera offset.
pub fn extract(
    world: &World,
    width: u32,
    height: u32,
    time: f32,
    frame_index: u64,
    options: &ViewOptions,
) -> ExtractedFrame {
    let aspect = width as f32 / height.max(1) as f32;
    let (camera, position, rotation) = world
        .query_ref::<(&Camera, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .next()
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
    // Debug views show their colours as they are.
    let post = if options.debug_view != DEBUG_VIEW_NONE {
        PostProcess::debug_view()
    } else {
        options.post_override.or_else(|| world.resource::<PostProcess>().copied()).unwrap_or_default()
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
        post: post.to_gpu(sky.horizon),
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
        ..Default::default()
    };

    let mut lights = Vec::new();
    let mut sun_found = false;
    for (light, transform) in world.query_ref::<(&Light, &Transform)>().without::<Destroying>() {
        let forward = transform.rotation * Vec3::Z;
        let radiance = light.color * light.intensity;
        match light.kind {
            LightKind::Directional if !sun_found => {
                sun_found = true;
                frame.sun_direction = vec3(forward);
                frame.sun_color = vec3(radiance);
            }
            LightKind::Directional => {}
            LightKind::Point | LightKind::Spot => lights.push(GpuLight {
                position: vec3(transform.position),
                range: light.range,
                color: vec3(radiance),
                kind: if light.kind == LightKind::Spot { LIGHT_SPOT } else { LIGHT_POINT },
                direction: vec3(forward),
                cos_outer: (light.spot_outer_degrees.to_radians() * 0.5).cos(),
                cos_inner: (light.spot_inner_degrees.to_radians() * 0.5).cos(),
                _pad: [0.0; 3],
            }),
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
                _pad: [0; 2],
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
