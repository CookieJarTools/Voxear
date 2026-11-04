//! Cascaded shadow maps for the sun.
//!
//! Each cascade covers a slice of the view frustum with a square orthographic
//! projection fitted to the slice's bounding sphere. The sphere's size does
//! not change as the camera turns, and its centre is snapped to whole shadow
//! texels, so shadow edges stay still instead of shimmering.

use voxear_core::math::{Mat4, Quat, Vec3, camera::lh::proj::vulkan, camera::lh::view::look_to_mat4};

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

    let mut near = view.near;
    std::array::from_fn(|i| {
        let far = split_at(view.near, i + 1, distance, split);
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

        let mut snapped = rotation_only.transform_point3(center);
        snapped.x = (snapped.x / texel).floor() * texel;
        snapped.y = (snapped.y / texel).floor() * texel;
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
    fn cascades_stay_still_when_the_camera_only_turns() {
        let mut a = view();
        let first = cascades(&a, Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        a.rotation = Quat::from_rotation_y(0.0001);
        let second = cascades(&a, Vec3::new(0.3, -1.0, 0.4), SHADOW_DISTANCE, SHADOW_SPLIT);
        assert_eq!(first[0].texel, second[0].texel);
    }
}
