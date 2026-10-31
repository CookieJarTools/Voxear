//! Gizmos in the Scene view, drawn with the same shapes as the handles and
//! under them: a small sun for each light and a small camera for each camera,
//! where the Obj is, so they can be seen and clicked; and for the selected
//! one, what it reaches: a camera's frustum, a directional light's direction,
//! a spot light's cone and a point light's range. The Gizmos button shows or
//! hides them all.

use voxear_core::math::{Mat4, Quat, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Light, LightKind};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{ViewCamera, ring};

/// A marker's size, in logical pixels: the sun's disc and the camera's body.
const MARKER: f32 = 5.0;
const LIGHT: u32 = 0xf2c94c;
const CAMERA: u32 = 0xc8ccd1;
/// The selected Obj's reach is drawn in the accent.
const REACH: u32 = 0x7988ff;
/// How far a camera's frustum is drawn, in metres.
const FRUSTUM_DEPTH: f32 = 3.0;
/// The aspect a camera's frustum is drawn with, which the view it renders
/// to decides, so the Game view's usual one.
const FRUSTUM_ASPECT: f32 = 16.0 / 9.0;
/// How long a directional light's direction is drawn, on screen.
const DIRECTION: f32 = 70.0;

/// The gizmos for every light and camera in view, in the view's own
/// coordinates (from its top left).
pub fn shapes(world: &World, camera: &ViewCamera, selected: Option<Entity>) -> Vec<Shape> {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    let mut shapes = Vec::new();
    let marked = world
        .query_ref::<(Entity, &Transform, Option<&WorldTransform>, Option<&Light>, Option<&Camera>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    for (entity, transform, placed, light, lens) in marked {
        if light.is_none() && lens.is_none() {
            continue;
        }
        let (position, rotation) = placement(transform, placed);
        let Some(at) = camera.project(position) else { continue };
        let at = local(at);
        let chosen = Some(entity) == selected;
        if let Some(light) = light {
            sun(&mut shapes, at, Color::hex(LIGHT));
            if chosen {
                light_reach(&mut shapes, camera, light, position, rotation);
            }
        } else if let Some(lens) = lens {
            camera_body(&mut shapes, at, Color::hex(CAMERA));
            if chosen {
                frustum(&mut shapes, camera, lens, position, rotation);
            }
        }
    }
    shapes
}

/// Where an Obj is and which way it faces in the world.
fn placement(transform: &Transform, placed: Option<&WorldTransform>) -> (Vec3, Quat) {
    match placed {
        Some(WorldTransform(matrix)) => {
            let (_, rotation, position) = Mat4::to_scale_rotation_translation(matrix);
            (position, rotation)
        }
        None => (transform.position, transform.rotation),
    }
}

/// A disc with eight rays, as lights are shown.
fn sun(shapes: &mut Vec<Shape>, at: [f32; 2], color: Color) {
    shapes.push(Shape::Circle { center: at, radius: MARKER - 1.0, fill: color, border: None });
    for ray in 0..8 {
        let angle = ray as f32 * std::f32::consts::FRAC_PI_4;
        let along = Vec2::new(angle.cos(), angle.sin());
        let (from, to) = (Vec2::from(at) + along * (MARKER + 1.5), Vec2::from(at) + along * (MARKER + 4.5));
        shapes.push(Shape::Line { from: from.into(), to: to.into(), width: 1.5, color });
    }
}

/// A box with a lens on its right, as cameras are shown.
fn camera_body(shapes: &mut Vec<Shape>, at: [f32; 2], color: Color) {
    let [x, y] = at;
    let (w, h) = (MARKER * 1.6, MARKER);
    let corners = [[x - w, y - h], [x + w * 0.4, y - h], [x + w * 0.4, y + h], [x - w, y + h]];
    for i in 0..4 {
        shapes.push(Shape::Line { from: corners[i], to: corners[(i + 1) % 4], width: 1.5, color });
    }
    let lens = [[x + w * 0.4, y - h * 0.2], [x + w, y - h * 0.8], [x + w, y + h * 0.8], [x + w * 0.4, y + h * 0.2]];
    for i in 0..3 {
        shapes.push(Shape::Line { from: lens[i], to: lens[i + 1], width: 1.5, color });
    }
}

/// What a light reaches: a directional light's direction, a spot light's
/// cone out to its range, and a point light's range as three rings.
fn light_reach(shapes: &mut Vec<Shape>, camera: &ViewCamera, light: &Light, position: Vec3, rotation: Quat) {
    let color = Color::hex(REACH);
    let forward = rotation * Vec3::Z;
    match light.kind {
        LightKind::Directional => {
            let tip = position + forward * camera.world_size(position, DIRECTION);
            line(shapes, camera, position, tip, color);
        }
        LightKind::Spot => {
            let radius = light.range * light.spot_outer_degrees.to_radians().tan();
            let end = position + forward * light.range;
            let across = forward.any_orthonormal_vector();
            for turn in 0..4 {
                let spoke = Quat::from_axis_angle(forward, turn as f32 * std::f32::consts::FRAC_PI_2) * across;
                line(shapes, camera, position, end + spoke * radius, color);
            }
            polyline(shapes, camera, &ring(camera, end, forward, radius), color);
        }
        LightKind::Point => {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                polyline(shapes, camera, &ring(camera, position, axis, light.range), color);
            }
        }
    }
}

/// A camera's frustum, a few metres deep: lines from the camera to the far
/// corners, and the far rectangle.
fn frustum(shapes: &mut Vec<Shape>, camera: &ViewCamera, lens: &Camera, position: Vec3, rotation: Quat) {
    let color = Color::hex(REACH);
    let half_height = (lens.fov_y_degrees.to_radians() * 0.5).tan() * FRUSTUM_DEPTH;
    let half_width = half_height * FRUSTUM_ASPECT;
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| position + rotation * Vec3::new(x * half_width, y * half_height, FRUSTUM_DEPTH));
    for i in 0..4 {
        line(shapes, camera, position, corners[i], color);
        line(shapes, camera, corners[i], corners[(i + 1) % 4], color);
    }
}

/// A line between two points in the world, left out when either is behind
/// the camera.
fn line(shapes: &mut Vec<Shape>, camera: &ViewCamera, from: Vec3, to: Vec3, color: Color) {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    if let (Some(from), Some(to)) = (camera.project(from), camera.project(to)) {
        shapes.push(Shape::Line { from: local(from), to: local(to), width: 1.5, color });
    }
}

/// Lines joining points already on screen, in order.
fn polyline(shapes: &mut Vec<Shape>, camera: &ViewCamera, points: &[[f32; 2]], color: Color) {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    for pair in points.windows(2) {
        shapes.push(Shape::Line { from: local(pair[0]), to: local(pair[1]), width: 1.5, color });
    }
}
