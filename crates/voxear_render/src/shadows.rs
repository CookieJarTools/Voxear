//! Cascaded shadow maps for the sun, and local lights' shadows.
//!
//! Each cascade covers a slice of the view frustum with a square orthographic
//! projection fitted to the slice's bounding sphere. The sphere's size does
//! not change as the camera turns, and its centre is snapped to whole shadow
//! texels, so shadow edges stay still instead of shimmering.
//!
//! A local light casting a shadow takes views of an atlas of its own
//! ([`local_views`]): a spot light one, along its cone, and any other light
//! six, the faces of a cube round it. Each is a perspective view from the
//! light, a little wider than its cone or its face, so the filter that
//! softens the shadow's edge stays inside the view at its rim.

use voxear_core::math::{I64Vec3, Mat4, Quat, Vec3, camera::lh::proj::vulkan, camera::lh::view::look_to_mat4};

pub const CASCADES: usize = 4;
/// Pixels per cascade side; the atlas holds the four cascades in a 2×2 grid.
pub const CASCADE_RESOLUTION: u32 = 2048;
pub const ATLAS_RESOLUTION: u32 = CASCADE_RESOLUTION * 2;
/// How far from the camera the sun casts shadows, unless the lighting says
/// (`SunLook::shadow_distance`).
pub const SHADOW_DISTANCE: f32 = 80.0;
/// How the cascades share that distance, unless the lighting says: a blend
/// between logarithmic spacing (1), which keeps the near cascades small and
/// sharp, and even spacing (0), which gives the far ones more.
pub const SHADOW_SPLIT: f32 = 0.85;

#[derive(Clone, Copy, Debug)]
pub struct Cascade {
    pub view_proj: Mat4,
    /// Camera depth where this cascade ends.
    pub far: f32,
    /// World-space size of one shadow texel.
    pub texel: f32,
    /// Inward planes for culling casters; the near side is left open so
    /// casters between the sun and the slice still cast.
    pub planes: [[f32; 4]; 6],
}

pub struct View {
    pub position: Vec3,
    pub rotation: Quat,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    /// Where the world's middle is from the level's (see
    /// `voxear_scene::WorldOrigin`): the sun's cascades snap to texels
    /// measured from the level's middle, repeating every `ORIGIN_WRAP`
    /// metres, so the world's middle moving shifts no shadow's edge.
    pub origin: I64Vec3,
}

/// The way each face of a light's cube looks, and which way is up in it.
pub const CUBE_FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y),
    (Vec3::NEG_X, Vec3::Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];

/// Texels round a local shadow view's rim beyond its cone or face, which
/// the shaders keep their filter inside.
pub const LOCAL_SHADOW_BORDER: f32 = 2.0;

/// A local light as its shadow is drawn: where it is, how far its shadow
/// reaches, and how near to it things start to cast (past its own shape and
/// the glass or flame round it).
pub struct LocalLight {
    pub position: Vec3,
    pub rotation: Quat,
    pub range: f32,
    pub near: f32,
    /// A spot light's cone, across, in degrees; `None` for a cube.
    pub cone_degrees: Option<f32>,
}

/// The views a local light's shadow is drawn in: one along a spot light's
/// cone, or the six faces of a cube in `CUBE_FACES`' order.
pub fn local_views(light: &LocalLight) -> Vec<Mat4> {
    let size = crate::gpu::LOCAL_SHADOW_TILE as f32;
    let grow = (size + 2.0 * LOCAL_SHADOW_BORDER) / size;
    let far = light.range.max(light.near * 2.0);
    let projection = |tan_half: f32| vulkan::perspective(2.0 * (tan_half * grow).atan(), 1.0, light.near, far);
    match light.cone_degrees {
        Some(cone) => {
            let tan_half = (cone.clamp(1.0, 160.0).to_radians() * 0.5).tan();
            let (forward, up) = (light.rotation * Vec3::Z, light.rotation * Vec3::Y);
            vec![projection(tan_half) * look_to_mat4(light.position, forward, up)]
        }
        None => CUBE_FACES
            .iter()
            .map(|&(forward, up)| projection(1.0) * look_to_mat4(light.position, forward, up))
            .collect(),
    }
}

/// `light_direction` is the way the light travels. The cascades reach out
/// to `distance` from the camera, split between them as `split` says (see
/// `SHADOW_SPLIT`).
pub fn cascades(view: &View, light_direction: Vec3, distance: f32, split: f32) -> [Cascade; CASCADES] {
    let forward = view.rotation * Vec3::Z;
    let right = view.rotation * Vec3::X;
    let up = view.rotation * Vec3::Y;
    let tan_v = (view.fov_y * 0.5).tan();
    let tan_h = tan_v * view.aspect;
    let direction = light_direction.normalize();
    let light_up = if direction.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let rotation_only = look_to_mat4(Vec3::ZERO, direction, light_up);

    // A camera seeing from very close, as a first-person one does, splits its cascades from a usual near
    // distance, not from in front of its nose.
    let start = view.near.max(SPLIT_NEAR);
    let mut near = view.near;
    std::array::from_fn(|i| {
        let far = split_at(start, i + 1, distance, split);
        let corners = [near, far].into_iter().flat_map(|d| {
            [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(sx, sy)| view.position + forward * d + right * (sx * d * tan_h) + up * (sy * d * tan_v))
        });
        let corners: Vec<Vec3> = corners.collect();
        let mut center = corners.iter().sum::<Vec3>() / corners.len() as f32;
        let radius = corners.iter().map(|c| c.distance(center)).fold(0.0, f32::max);
        // Quantise the size so it never changes as the camera turns.
        let radius = (radius * 16.0).ceil() / 16.0;
        let texel = radius * 2.0 / CASCADE_RESOLUTION as f32;

        // On texels counted from the level's middle, in f64, as the world's middle may be far from it; repeating
        // every `ORIGIN_WRAP` metres, as the shaders' patterns do.
        let wrapped = view.origin.as_dvec3().to_array().map(|metres| metres.rem_euclid(crate::gpu::ORIGIN_WRAP));
        let level = rotation_only.as_dmat4().transform_vector3(voxear_core::math::DVec3::from_array(wrapped));
        let mut snapped = rotation_only.transform_point3(center);
        let snap = |v: f32, by: f64| (((f64::from(v) + by) / f64::from(texel)).floor() * f64::from(texel) - by) as f32;
        snapped.x = snap(snapped.x, level.x);
        snapped.y = snap(snapped.y, level.y);
        center = rotation_only.inverse().transform_point3(snapped);

        let eye = center - direction * radius;
        let light_view = look_to_mat4(eye, direction, light_up);
        let projection = vulkan::orthographic(-radius, radius, -radius, radius, 0.0, radius * 2.0);
        let basis = light_view.inverse();
        let (light_right, light_up_axis) = (basis.x_axis.truncate(), basis.y_axis.truncate());
        let plane = |n: Vec3, offset: f32| [n.x, n.y, n.z, offset];
        let cascade = Cascade {
            view_proj: projection * light_view,
            far,
            texel,
            planes: [
                plane(light_right, -light_right.dot(center) + radius),
                plane(-light_right, light_right.dot(center) + radius),
                plane(light_up_axis, -light_up_axis.dot(center) + radius),
                plane(-light_up_axis, light_up_axis.dot(center) + radius),
                plane(-direction, direction.dot(center) + radius),
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        near = far;
        cascade
    })
}

/// "Practical" split scheme: mostly logarithmic, so near cascades are small
/// and sharp, with some even spacing so the far ones are not starved.
/// The nearest a camera's cascades split from, in metres.
const SPLIT_NEAR: f32 = 0.1;

fn split_at(near: f32, index: usize, distance: f32, split: f32) -> f32 {
    let distance = distance.max(near * 2.0);
    let t = index as f32 / CASCADES as f32;
    let log = near * (distance / near).powf(t);
    let even = near + (distance - near) * t;
    let split = split.clamp(0.0, 1.0);
    split * log + (1.0 - split) * even
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            position: Vec3::new(0.0, 2.0, -5.0),
            rotation: Quat::IDENTITY,
            fov_y: 1.0,
            aspect: 16.0 / 9.0,
            near: 0.1,
            origin: I64Vec3::ZERO,
        }
    }

    fn inside(planes: &[[f32; 4]; 6], p: Vec3) -> bool {
        planes.iter().all(|pl| Vec3::new(pl[0], pl[1], pl[2]).dot(p) + pl[3] >= 0.0)
    }

    #[test]
    fn splits_grow_and_end_at_the_shadow_distance() {
        let cascades = cascades(&view(), Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        for pair in cascades.windows(2) {
            assert!(pair[1].far > pair[0].far);
            assert!(pair[1].texel > pair[0].texel);
        }
        assert!((cascades[CASCADES - 1].far - SHADOW_DISTANCE).abs() < 1e-3);
    }

    #[test]
    fn the_lighting_sets_how_far_shadows_reach_and_how_the_cascades_share_it() {
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let (near, far) = (cascades(&view(), sun, 30.0, 0.85), cascades(&view(), sun, 200.0, 0.85));
        assert!((near[CASCADES - 1].far - 30.0).abs() < 1e-3 && (far[CASCADES - 1].far - 200.0).abs() < 1e-3);
        assert!(near[0].texel < far[0].texel, "a shorter reach gives sharper shadows near the camera");
        let (even, logarithmic) = (cascades(&view(), sun, 80.0, 0.0), cascades(&view(), sun, 80.0, 1.0));
        assert!((even[0].far - 20.0).abs() < 0.1, "even splits share the distance alike: {}", even[0].far);
        assert!(logarithmic[0].far < 1.0, "logarithmic ones keep the first cascade small: {}", logarithmic[0].far);
    }

    #[test]
    fn points_in_a_slice_land_inside_its_shadow_map() {
        let view = view();
        let cascades = cascades(&view, Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        let point = view.position + Vec3::Z * 3.0;
        let clip = cascades[0].view_proj * point.extend(1.0);
        assert!(clip.x.abs() < 1.0 && clip.y.abs() < 1.0, "{clip:?}");
        assert!((0.0..=1.0).contains(&clip.z));
        assert!(inside(&cascades[0].planes, point));
        // Casters between the sun and the slice must survive culling.
        assert!(inside(&cascades[0].planes, point + Vec3::new(-0.3, 1.0, -0.4) * 200.0));
        assert!(!inside(&cascades[0].planes, point + Vec3::X * 500.0));
    }

    #[test]
    fn a_cube_sees_all_round_a_light_and_a_spot_along_its_cone() {
        let light =
            LocalLight { position: Vec3::new(1.0, 2.0, 3.0), rotation: Quat::IDENTITY, range: 10.0, near: 0.1, cone_degrees: None };
        let faces = local_views(&light);
        assert_eq!(faces.len(), 6);
        for direction in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z, Vec3::new(0.4, -0.7, 0.6)] {
            let point = light.position + direction.normalize() * 4.0;
            let seen = faces.iter().filter(|face| {
                let clip = **face * point.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                clip.w > 0.0 && ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)
            });
            assert!(seen.count() >= 1, "some face sees {direction}");
        }
        // Nearer is less deep, so a caster in front hides what lies behind it.
        let depth = |at: Vec3| {
            let clip = faces[4] * at.extend(1.0);
            clip.z / clip.w
        };
        assert!(depth(light.position + Vec3::Z * 2.0) < depth(light.position + Vec3::Z * 6.0));

        let spot = LocalLight { cone_degrees: Some(60.0), ..light };
        let cone = local_views(&spot);
        assert_eq!(cone.len(), 1);
        let ahead = cone[0] * (spot.position + Vec3::Z * 5.0).extend(1.0);
        assert!((ahead.x / ahead.w).abs() < 1e-4 && (ahead.y / ahead.w).abs() < 1e-4, "straight ahead is the middle");
        let outside = cone[0] * (spot.position + Vec3::new(1.0, 0.0, 1.0) * 5.0).extend(1.0);
        assert!((outside.x / outside.w).abs() > 1.0, "45 degrees off is outside a 60 degree cone");
    }

    #[test]
    fn cascades_stay_still_when_the_camera_only_turns() {
        let mut a = view();
        let first = cascades(&a, Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        a.rotation = Quat::from_rotation_y(0.0001);
        let second = cascades(&a, Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        assert_eq!(first[0].texel, second[0].texel);
    }

    #[test]
    fn cascades_keep_their_texels_where_they_were_as_the_worlds_middle_moves() {
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let near = cascades(&view(), sun, SHADOW_DISTANCE, SHADOW_SPLIT);
        // The world moved 3 km back along x: the camera with it, the middle 3 km out.
        let by = Vec3::new(-3072.0, 0.0, 0.0);
        let moved = View { position: view().position + by, origin: I64Vec3::new(3072, 0, 0), ..view() };
        let far = cascades(&moved, sun, SHADOW_DISTANCE, SHADOW_SPLIT);
        for (near, far) in near.iter().zip(&far) {
            let (point, moved_point) = (Vec3::new(1.3, 0.2, 4.1), Vec3::new(1.3, 0.2, 4.1) + by);
            let (a, b) = (near.view_proj.project_point3(point), far.view_proj.project_point3(moved_point));
            // A texel is 2/2048 across: a fifth of one is float rounding, not a texel's snap.
            assert!(a.distance(b) < 2e-4, "the same point on the same texel: {a} and {b}");
        }
    }
}
