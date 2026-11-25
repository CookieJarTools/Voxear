//! What a Camera can be set to beyond its lens's shape (`lens`): how its
//! field of view is measured, a physical camera's focal length and sensor,
//! how far it sees, where on the screen it draws and in what order, which
//! layers it sees, what fills where nothing is drawn, its own
//! post-processing, and cameras stacked over another; and the views the
//! game draws from them all (`camera_views`).
//!
//! Every setting starts as a plain camera has it, and costs nothing then.

use voxear_core::math::{Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};

use crate::{Camera, Projection};

/// Which way a perspective camera's field of view is measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FovAxis {
    /// From the bottom of the picture to the top: a wider screen sees more
    /// at the sides.
    #[default]
    Vertical,
    /// From side to side: a taller screen sees more above and below.
    Horizontal,
}

impl FovAxis {
    pub const ALL: [Self; 2] = [Self::Vertical, Self::Horizontal];

    pub fn name(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// How a physical camera's sensor is fitted to a picture of another shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateFit {
    /// The sensor's height fills the picture's height.
    Vertical,
    /// The sensor's width fills the picture's width.
    Horizontal,
    /// The whole sensor shows, the picture seeing more one way.
    #[default]
    Fill,
    /// The picture is filled, part of the sensor cut off one way.
    Overscan,
    /// The picture takes the sensor's own shape, stretched.
    None,
}

impl GateFit {
    pub const ALL: [Self; 5] = [Self::Vertical, Self::Horizontal, Self::Fill, Self::Overscan, Self::None];

    pub fn name(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
            Self::Fill => "fill",
            Self::Overscan => "overscan",
            Self::None => "none",
        }
    }
}

/// A camera set as a real one is: its lens's focal length and its sensor,
/// which make its field of view, and its lens shifted off the middle.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicalLens {
    /// In millimetres: longer sees less.
    pub focal_length: f32,
    /// The sensor's width and height, in millimetres: 36 × 24 is full frame.
    pub sensor: [f32; 2],
    pub gate_fit: GateFit,
    /// The lens moved off the middle, as a share of the sensor across and
    /// up: 0.5 moves the picture half its width.
    pub shift: [f32; 2],
}

impl Default for PhysicalLens {
    fn default() -> Self {
        Self { focal_length: 50.0, sensor: [36.0, 24.0], gate_fit: GateFit::Fill, shift: [0.0; 2] }
    }
}

impl PhysicalLens {
    /// The field of view up and down, in radians, for a picture of `aspect`.
    pub fn fov_y(&self, aspect: f32) -> f32 {
        let [width, height] = self.sensor.map(|side| side.max(0.1));
        let focal = self.focal_length.max(0.1);
        let sensor_aspect = width / height;
        // The sensor's height as the picture's height takes it.
        let height = match self.gate_fit {
            GateFit::Vertical | GateFit::None => height,
            GateFit::Horizontal => width / aspect,
            GateFit::Fill => if aspect >= sensor_aspect { height } else { width / aspect },
            GateFit::Overscan => if aspect >= sensor_aspect { width / aspect } else { height },
        };
        2.0 * (height * 0.5 / focal).atan()
    }

    /// The focal length that sees `fov_y` up and down on a sensor `height`
    /// millimetres high.
    pub fn focal_length_for(fov_y: f32, height: f32) -> f32 {
        height * 0.5 / (fov_y * 0.5).tan().max(1e-4)
    }
}

/// What fills the picture where the camera sees nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum Background {
    /// The sky, as the level's lighting draws it.
    #[default]
    Sky,
    /// One colour, sRGB from 0 to 1.
    Color { color: [f32; 3] },
    /// Nothing: what was drawn there before shows, as a camera drawn over
    /// another's picture.
    Nothing,
}

impl Background {
    pub const NAMES: [&str; 3] = ["sky", "color", "nothing"];

    pub fn index(self) -> usize {
        match self {
            Self::Sky => 0,
            Self::Color { .. } => 1,
            Self::Nothing => 2,
        }
    }
}

/// Where a camera's picture goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum CameraTarget {
    /// A display, by its number from 0, the game's own window.
    Display { display: u8 },
    /// A picture of this many pixels, which materials and UI show
    /// (`CameraPictures`).
    Picture { width: u32, height: u32 },
}

impl Default for CameraTarget {
    fn default() -> Self {
        Self::Display { display: 0 }
    }
}

/// A camera's own way of finishing its picture, in place of the level's
/// post-processing where set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraFinish {
    /// Post-processing at all: off, the picture is exposed and shown
    /// as lit, with none of the level's effects.
    pub post: bool,
    /// Anti-aliasing in place of the post-processing's, or theirs.
    pub anti_aliasing: Option<crate::AntiAliasingMode>,
    /// Light brighter than the screen shows kept, for bloom and the
    /// tonemap; off, it is cut at white before post-processing.
    pub hdr: bool,
    /// Whether what is hidden behind what is nearer is left out unseen.
    pub occlusion_culling: bool,
}

impl Default for CameraFinish {
    fn default() -> Self {
        Self { post: true, anti_aliasing: None, hdr: true, occlusion_culling: true }
    }
}

/// A camera drawn over another's picture, after it, in the same place on
/// the same screen: a first-person view's weapon or hands, or a UI drawn in
/// the world, never cut into by the world. Its background is nothing, and
/// what it draws covers the camera's under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraOver(pub Entity);
impl Component for CameraOver {}

/// A camera's picture shown on this Obj's mesh, as a screen, a mirror or a
/// map table shows one: the camera draws into a picture of its own
/// (`CameraTarget::Picture`), which takes the place of the mesh's material,
/// glowing as a screen does, `brightness` times as bright as the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraScreen {
    pub camera: Entity,
    pub brightness: f32,
}
impl Component for CameraScreen {}

/// One view the game draws: through which camera, into which part of its
/// output, in pixels, and over what is there when it is drawn over another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub camera: Entity,
    /// Left, top, width and height in the output, in pixels.
    pub rect: [u32; 4],
    pub target: CameraTarget,
    /// Drawn over what is under it: where it draws nothing, that shows.
    pub over: bool,
}

/// Every camera the game draws through, switched on, in the order they are
/// drawn: by target, then by priority, lowest first, each with the cameras
/// stacked over it right after; each in its viewport's part of `size`, or
/// of its own picture. A camera stacked over one that is off is not drawn.
pub fn camera_views(world: &World, size: (u32, u32)) -> Vec<CameraView> {
    let switched = voxear_scene::Switched::of(world);
    let cameras: Vec<(Entity, Camera, Option<Entity>)> = world
        .query_ref::<(Entity, &Camera)>()
        .without::<voxear_scene::Destroying>()
        .without::<voxear_scene::EditorOnly>()
        .into_iter()
        .filter(|(entity, _)| switched.is_on(world, *entity, voxear_scene::Switch::Camera))
        .map(|(entity, camera)| (entity, *camera, world.get::<CameraOver>(entity).map(|over| over.0)))
        .collect();
    let mut bases: Vec<&(Entity, Camera, Option<Entity>)> = cameras.iter().filter(|(.., over)| over.is_none()).collect();
    bases.sort_by_key(|(entity, camera, _)| (target_order(camera.target), camera.priority, entity.index()));
    let mut views = Vec::new();
    for (entity, camera, _) in bases {
        let area = match camera.target {
            CameraTarget::Display { .. } => size,
            CameraTarget::Picture { width, height } => (width.max(1), height.max(1)),
        };
        let rect = viewport_rect(camera.viewport, area);
        if rect[2] == 0 || rect[3] == 0 {
            continue;
        }
        let over = matches!(camera.background, Background::Nothing) && !views.is_empty();
        views.push(CameraView { camera: *entity, rect, target: camera.target, over });
        // Those stacked over it, by their own priority.
        let mut stacked: Vec<&(Entity, Camera, Option<Entity>)> =
            cameras.iter().filter(|(.., over)| *over == Some(*entity)).collect();
        stacked.sort_by_key(|(stacked, camera, _)| (camera.priority, stacked.index()));
        views.extend(stacked.into_iter().map(|(stacked, ..)| CameraView {
            camera: *stacked,
            rect,
            target: camera.target,
            over: true,
        }));
    }
    views
}

fn target_order(target: CameraTarget) -> (u8, u8) {
    match target {
        // Pictures first, so a camera on screen shows this frame's.
        CameraTarget::Picture { .. } => (0, 0),
        CameraTarget::Display { display } => (1, display),
    }
}

/// A viewport, left, bottom, width and height as shares of the screen from
/// 0 to 1, as pixels of `size`: left, top, width and height.
pub fn viewport_rect([x, y, w, h]: [f32; 4], (width, height): (u32, u32)) -> [u32; 4] {
    let (x0, x1) = (x.clamp(0.0, 1.0), (x + w).clamp(0.0, 1.0));
    let (y0, y1) = (y.clamp(0.0, 1.0), (y + h).clamp(0.0, 1.0));
    let px = |share: f32, of: u32| (share * of as f32).round() as u32;
    // The viewport's y counts up from the bottom; the picture's rows down from the top.
    let (left, right) = (px(x0, width), px(x1, width));
    let (top, bottom) = (height - px(y1, height), height - px(y0, height));
    [left, top, right.saturating_sub(left), bottom.saturating_sub(top)]
}

/// The plane `[n, d]`, `dot(n, p) + d >= 0` kept, that cuts away what is
/// further than `far` in front of a camera at `position` looking along
/// `forward`; one that cuts nothing for no far limit.
pub fn far_plane(position: Vec3, forward: Vec3, far: Option<f32>) -> [f32; 4] {
    match far {
        Some(far) => [-forward.x, -forward.y, -forward.z, forward.dot(position) + far],
        None => crate::gpu::NO_CUT,
    }
}

/// Whether a custom projection is orthographic: its w does not grow with
/// depth.
pub fn custom_is_orthographic(projection: &Mat4) -> bool {
    projection.row(3) == Vec4::new(0.0, 0.0, 0.0, 1.0)
}

/// The lens a projection of a camera's own has, as the passes read it,
/// and for an orthographic one how far past its near plane it sees.
/// Reverse depth is taken as `lens` makes it: a perspective's near plane
/// from its last column, as if it saw without end.
pub fn lens_of(matrix: &Mat4) -> (crate::Lens, f32) {
    let (across, up) = (matrix.x_axis.x.abs().max(1e-6), matrix.y_axis.y.abs().max(1e-6));
    let aspect = up / across;
    if custom_is_orthographic(matrix) {
        let (scale, offset) = (matrix.z_axis.z, matrix.w_axis.z);
        let near = if scale.abs() > 1e-12 { (1.0 - offset) / scale } else { 0.1 };
        let depth = if scale.abs() > 1e-12 { (1.0 / scale).abs() } else { crate::ORTHO_DEPTH };
        return (crate::Lens { fov_y: 1.0, aspect, near, ortho: Some(1.0 / up) }, depth);
    }
    let lens = crate::Lens { fov_y: 2.0 * (1.0 / up).atan(), aspect, near: matrix.w_axis.z.abs().max(1e-4), ortho: None };
    (lens, crate::ORTHO_DEPTH)
}

/// The layers a mask sees, a bit each: every one.
pub const EVERY_LAYER: u32 = u32::MAX;

/// Whether a camera with `mask` sees an Obj on `layer`.
pub fn sees_layer(mask: u32, layer: u8) -> bool {
    mask & (1 << u32::from(layer.min(31))) != 0
}

impl Camera {
    /// The field of view up and down, in radians, for a picture of
    /// `aspect`: its physical lens's, or its own measured as `fov_axis`
    /// says.
    pub fn fov_y_for(&self, aspect: f32) -> f32 {
        if let Some(physical) = &self.physical {
            return physical.fov_y(aspect);
        }
        let fov = self.fov_y_degrees.clamp(0.01, 179.0).to_radians();
        match self.fov_axis {
            FovAxis::Vertical => fov,
            FovAxis::Horizontal => 2.0 * ((fov * 0.5).tan() / aspect.max(1e-6)).atan(),
        }
    }

    /// Whether it is orthographic, by its projection or its custom matrix.
    pub fn is_orthographic(&self) -> bool {
        match &self.custom_projection {
            Some(matrix) => custom_is_orthographic(matrix),
            None => self.projection == Projection::Orthographic,
        }
    }

    /// Makes it a physical camera with a lens of `focal_length` millimetres
    /// on a full-frame sensor: the easy way in.
    pub fn with_focal_length(mut self, focal_length: f32) -> Self {
        self.physical = Some(PhysicalLens { focal_length, ..PhysicalLens::default() });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horizontal_field_of_view_is_as_wide_on_every_screen() {
        let camera = Camera { fov_y_degrees: 90.0, fov_axis: FovAxis::Horizontal, ..Camera::default() };
        for aspect in [1.0f32, 16.0 / 9.0, 21.0 / 9.0] {
            let across = 2.0 * ((camera.fov_y_for(aspect) * 0.5).tan() * aspect).atan();
            assert!((across.to_degrees() - 90.0).abs() < 1e-3, "{across} at {aspect}");
        }
        assert!((Camera::default().fov_y_for(2.0).to_degrees() - 60.0).abs() < 1e-4);
    }

    #[test]
    fn a_physical_lens_sees_as_its_focal_length_and_sensor_say() {
        // A 50 mm lens on a 24 mm high sensor sees 2 atan(12 / 50), about 27 degrees, up and down.
        let lens = PhysicalLens::default();
        let fov = lens.fov_y(1.5).to_degrees();
        assert!((fov - 26.99).abs() < 0.05, "{fov}");
        // Fill on a squarer picture fits the sensor's width, seeing more up and down; overscan the other way.
        assert!(lens.fov_y(1.0) > lens.fov_y(1.5));
        let over = PhysicalLens { gate_fit: GateFit::Overscan, ..lens };
        assert!(over.fov_y(1.0) < lens.fov_y(1.0) && (over.fov_y(1.0) - lens.fov_y(1.5)).abs() < 1e-5);
        let focal = PhysicalLens::focal_length_for(lens.fov_y(1.5), 24.0);
        assert!((focal - 50.0).abs() < 1e-3);
    }

    #[test]
    fn a_viewport_is_counted_up_from_the_bottom_left() {
        assert_eq!(viewport_rect([0.0, 0.0, 1.0, 1.0], (800, 600)), [0, 0, 800, 600]);
        // The left half, and the top right quarter.
        assert_eq!(viewport_rect([0.0, 0.0, 0.5, 1.0], (800, 600)), [0, 0, 400, 600]);
        assert_eq!(viewport_rect([0.5, 0.5, 0.5, 0.5], (800, 600)), [400, 0, 400, 300]);
        assert_eq!(viewport_rect([0.9, 0.0, 0.5, 1.0], (800, 600)), [720, 0, 80, 600], "kept on the screen");
    }

    #[test]
    fn cameras_are_drawn_by_priority_with_those_stacked_over_each_right_after() {
        let mut world = World::new();
        let place = voxear_scene::Transform::IDENTITY;
        let left = Camera { viewport: [0.0, 0.0, 0.5, 1.0], priority: 0, ..Camera::default() };
        let right = Camera { viewport: [0.5, 0.0, 0.5, 1.0], priority: -1, ..Camera::default() };
        let (left, right) = (world.spawn((left, place)), world.spawn((right, place)));
        let hands = world.spawn((Camera { priority: 5, background: Background::Nothing, ..Camera::default() }, place));
        world.insert(hands, CameraOver(left));
        let mirror = Camera { target: CameraTarget::Picture { width: 256, height: 128 }, priority: 9, ..Camera::default() };
        let mirror = world.spawn((mirror, place));
        let views = camera_views(&world, (800, 600));
        let order: Vec<Entity> = views.iter().map(|view| view.camera).collect();
        assert_eq!(order, [mirror, right, left, hands], "pictures first, then by priority, stacked after their base");
        assert_eq!(views[0].rect, [0, 0, 256, 128]);
        assert_eq!(views[3].rect, views[2].rect, "a stacked camera draws where its base does");
        assert!(views[3].over && !views[2].over && !views[1].over);
    }

    #[test]
    fn a_mask_sees_its_layers_only() {
        assert!(sees_layer(EVERY_LAYER, 31) && sees_layer(0b101, 2) && !sees_layer(0b101, 1));
    }

    #[test]
    fn a_far_plane_keeps_what_is_nearer() {
        let plane = far_plane(Vec3::new(0.0, 0.0, 1.0), Vec3::Z, Some(10.0));
        let keeps = |p: Vec3| Vec3::new(plane[0], plane[1], plane[2]).dot(p) + plane[3] >= 0.0;
        assert!(keeps(Vec3::new(5.0, 3.0, 10.9)) && !keeps(Vec3::new(0.0, 0.0, 11.1)));
        assert!(custom_is_orthographic(&crate::Lens { fov_y: 1.0, aspect: 1.0, near: 0.1, ortho: Some(2.0) }.projection()));
    }

    #[test]
    fn a_custom_projection_is_read_back_as_the_lens_that_made_it() {
        for made in [
            crate::Lens { fov_y: 0.9, aspect: 1.6, near: 0.3, ortho: None },
            crate::Lens { fov_y: 1.0, aspect: 1.6, near: 0.3, ortho: Some(4.0) },
        ] {
            let (read, depth) = lens_of(&made.projection());
            assert!((read.aspect - made.aspect).abs() < 1e-4 && (read.near - made.near).abs() < 1e-3, "{read:?}");
            match made.ortho {
                Some(half) => {
                    assert!((read.ortho.unwrap() - half).abs() < 1e-4);
                    assert!((depth - crate::ORTHO_DEPTH).abs() < 0.5, "{depth}");
                }
                None => assert!((read.fov_y - made.fov_y).abs() < 1e-4 && read.ortho.is_none()),
            }
        }
    }
}
