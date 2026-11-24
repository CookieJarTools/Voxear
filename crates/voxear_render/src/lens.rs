//! A camera's lens: perspective, as an eye sees, things smaller the further
//! off they are; or orthographic, every ray parallel along the camera's
//! forward, things the same size however far, as a plan or a pixel-art room
//! seen from above is drawn.
//!
//! Both draw with reversed depth, 1 at the near plane. A perspective camera
//! sees without end, its depth `near / distance`; an orthographic one sees
//! [`ORTHO_DEPTH`] metres past its near plane, its depth falling evenly from
//! 1 to 0 over them. The shaders read which from the frame
//! (`isOrthographic`, `viewDepthOf`, `eyeRay` in `shaders/common.slang`).

use voxear_core::math::{Mat4, Quat, Vec3, Vec4, camera::lh::proj::vulkan};

/// How far past its near plane an orthographic camera sees, in metres.
pub const ORTHO_DEPTH: f32 = 4096.0;

/// How a camera draws what it sees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    /// Further off is smaller, as an eye sees: `fov_y_degrees` says how much.
    #[default]
    Perspective,
    /// Every ray parallel, the same size however far off: `size` says how
    /// much it sees.
    Orthographic,
}

impl Projection {
    pub const ALL: [Self; 2] = [Self::Perspective, Self::Orthographic];

    /// As a scene file and the calls write it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Perspective => "perspective",
            Self::Orthographic => "orthographic",
        }
    }

    /// From a scene file's or a call's name.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|projection| projection.name() == name)
    }
}

/// What a camera's projection needs: the shape of what it sees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lens {
    /// How much a perspective camera sees up and down, in radians.
    pub fov_y: f32,
    /// Width over height.
    pub aspect: f32,
    pub near: f32,
    /// For an orthographic camera, half the height it sees, in metres.
    pub ortho: Option<f32>,
}

impl Lens {
    /// The lens of `camera`, for a picture of `aspect`.
    pub fn of(camera: &crate::Camera, aspect: f32) -> Self {
        Self {
            fov_y: camera.fov_y_degrees.clamp(0.01, 179.0).to_radians(),
            aspect: aspect.max(1e-6),
            near: camera.near.max(1e-4),
            ortho: (camera.projection == Projection::Orthographic).then_some(camera.size.max(1e-3)),
        }
    }

    /// Half the height it sees at `distance` in front: the same everywhere
    /// for an orthographic camera.
    pub fn half_height(&self, distance: f32) -> f32 {
        self.ortho.unwrap_or_else(|| (self.fov_y * 0.5).tan() * distance)
    }

    /// The lens seeing `grow` times as far each way, across and up, round
    /// the same middle: a picture with a margin round it.
    pub fn grown(self, [across, up]: [f32; 2]) -> Self {
        let aspect = self.aspect * across / up;
        match self.ortho {
            Some(half) => Self { aspect, ortho: Some(half * up), ..self },
            None => Self { aspect, fov_y: 2.0 * ((self.fov_y * 0.5).tan() * up).atan(), ..self },
        }
    }

    /// The projection, reversed depth, Y as the perspective cameras flip it.
    pub fn projection(&self) -> Mat4 {
        let perspective = vulkan::perspective_infinite_reverse(self.fov_y, self.aspect, self.near);
        let Some(half) = self.ortho else { return perspective };
        let flip = perspective.y_axis.y.signum();
        let far = self.near + ORTHO_DEPTH;
        Mat4::from_cols(
            Vec4::new(1.0 / (half * self.aspect), 0.0, 0.0, 0.0),
            Vec4::new(0.0, flip / half, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -1.0 / ORTHO_DEPTH, 0.0),
            Vec4::new(0.0, 0.0, far / ORTHO_DEPTH, 1.0),
        )
    }

    /// `projection` moved by `offset` in the picture's -1 to 1 across and
    /// up, as TAA's jitter moves it.
    pub fn moved(&self, mut projection: Mat4, offset: [f32; 2]) -> Mat4 {
        // A perspective's w is the view depth, so its offset goes in the column depth multiplies; an orthographic
        // one's w is 1, so it goes in the last.
        let column = if self.ortho.is_some() { &mut projection.w_axis } else { &mut projection.z_axis };
        column.x += offset[0];
        column.y += offset[1];
        projection
    }

    /// The picture's pixels a metre of something seen square on: at a metre
    /// away for a perspective camera, whose pixels a metre fall as one over
    /// the distance; everywhere for an orthographic one. A frame's
    /// `lod_scale`.
    pub fn pixels_a_metre(&self, height: u32) -> f32 {
        height as f32 / (2.0 * self.half_height(1.0))
    }

    /// The inward planes of what it sees, from `position` turned by
    /// `rotation`, `dot(n, p) + d >= 0` inside: four sides and the near
    /// plane, and an orthographic camera's far plane, or one that takes in
    /// everything.
    pub fn planes(&self, position: Vec3, rotation: Quat) -> [[f32; 4]; 6] {
        let forward = rotation * Vec3::Z;
        let right = rotation * Vec3::X;
        let up = rotation * Vec3::Y;
        let near_plane = [forward.x, forward.y, forward.z, -forward.dot(position) - self.near];
        let Some(half) = self.ortho else {
            let half_v = self.fov_y * 0.5;
            let half_h = (half_v.tan() * self.aspect).atan();
            let plane = |normal: Vec3| {
                let normal = normal.normalize();
                [normal.x, normal.y, normal.z, -normal.dot(position)]
            };
            return [
                plane(right * half_h.cos() + forward * half_h.sin()),
                plane(-right * half_h.cos() + forward * half_h.sin()),
                plane(up * half_v.cos() + forward * half_v.sin()),
                plane(-up * half_v.cos() + forward * half_v.sin()),
                near_plane,
                [0.0, 0.0, 0.0, 1.0],
            ];
        };
        // A side `extent` from the middle along `normal` facing in.
        let side = |normal: Vec3, extent: f32| [normal.x, normal.y, normal.z, extent - normal.dot(position)];
        let far = -forward;
        [
            side(right, half * self.aspect),
            side(-right, half * self.aspect),
            side(up, half),
            side(-up, half),
            near_plane,
            [far.x, far.y, far.z, forward.dot(position) + self.near + ORTHO_DEPTH],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ortho(half: f32) -> Lens {
        Lens { fov_y: 1.0, aspect: 2.0, near: 0.5, ortho: Some(half) }
    }

    fn inside(planes: &[[f32; 4]; 6], p: Vec3) -> bool {
        planes.iter().all(|pl| Vec3::new(pl[0], pl[1], pl[2]).dot(p) + pl[3] >= 0.0)
    }

    #[test]
    fn orthographic_depth_falls_evenly_from_the_near_plane() {
        let lens = ortho(5.0);
        let projection = lens.projection();
        let depth = |z: f32| {
            let clip = projection * Vec4::new(0.0, 0.0, z, 1.0);
            clip.z / clip.w
        };
        assert!((depth(0.5) - 1.0).abs() < 1e-6, "the near plane at 1: {}", depth(0.5));
        assert!(depth(0.5 + ORTHO_DEPTH).abs() < 1e-5, "the far plane at 0: {}", depth(0.5 + ORTHO_DEPTH));
        let middle = depth(0.5 + ORTHO_DEPTH * 0.5);
        assert!((middle - 0.5).abs() < 1e-5, "half way, half the depth: {middle}");
    }

    #[test]
    fn orthographic_size_is_the_same_however_far() {
        let lens = ortho(5.0);
        let projection = lens.projection();
        for z in [1.0, 10.0, 1000.0] {
            let clip = projection * Vec4::new(10.0, 5.0, z, 1.0);
            let ndc = clip.truncate() / clip.w;
            assert!((ndc.x - 1.0).abs() < 1e-5, "the right edge is 10 m across at {z} m: {}", ndc.x);
            assert!((ndc.y.abs() - 1.0).abs() < 1e-5, "the top is 5 m up at {z} m: {}", ndc.y);
        }
        assert_eq!(lens.pixels_a_metre(1000), 100.0);
    }

    #[test]
    fn orthographic_y_flips_as_the_perspective_one_does() {
        let lens = ortho(1.0);
        let perspective = Lens { ortho: None, ..lens }.projection();
        let up = |projection: Mat4, z: f32| (projection * Vec4::new(0.0, 0.5, z, 1.0)).y.signum();
        assert_eq!(up(lens.projection(), 3.0), up(perspective, 3.0));
    }

    #[test]
    fn orthographic_planes_take_in_a_box() {
        let lens = ortho(2.0);
        let planes = lens.planes(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
        let at = |x: f32, y: f32, z: f32| Vec3::new(1.0 + x, 2.0 + y, 3.0 + z);
        assert!(inside(&planes, at(3.9, 1.9, 1.0)), "inside, near a corner");
        assert!(inside(&planes, at(-3.9, -1.9, 3000.0)), "inside, far off");
        assert!(!inside(&planes, at(4.1, 0.0, 10.0)), "past the right side");
        assert!(!inside(&planes, at(0.0, 2.1, 10.0)), "above the top");
        assert!(!inside(&planes, at(0.0, 0.0, 0.4)), "nearer than the near plane");
        assert!(!inside(&planes, at(0.0, 0.0, ORTHO_DEPTH + 1.0)), "past the far plane");
    }

    #[test]
    fn a_grown_lens_sees_further_round_the_same_middle() {
        let lens = Lens { fov_y: 1.0, aspect: 1.5, near: 0.1, ortho: None };
        let grown = lens.grown([1.1, 1.2]);
        assert!((grown.half_height(1.0) / lens.half_height(1.0) - 1.2).abs() < 1e-5);
        assert!((grown.aspect * grown.half_height(1.0) / (lens.aspect * lens.half_height(1.0)) - 1.1).abs() < 1e-5);
        let grown = ortho(3.0).grown([1.0, 2.0]);
        assert_eq!(grown.ortho, Some(6.0));
    }

    #[test]
    fn moved_shifts_the_picture_alike_at_every_depth() {
        for lens in [ortho(3.0), Lens { ortho: None, ..ortho(3.0) }] {
            let moved = lens.moved(lens.projection(), [0.1, -0.2]);
            for z in [1.0, 50.0] {
                let p = Vec4::new(0.3, 0.2, z, 1.0);
                let (a, b) = (lens.projection() * p, moved * p);
                let shift = b.truncate() / b.w - a.truncate() / a.w;
                assert!((shift.x - 0.1).abs() < 1e-5 && (shift.y + 0.2).abs() < 1e-5, "{shift} at {z}");
            }
        }
    }
}
