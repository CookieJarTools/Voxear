//! Gizmos in the Scene view, drawn with the same shapes as the handles and
//! under them: a small sun for each light and a small camera for each camera,
//! where the Obj is, so they can be seen and clicked; and for the selected
//! one, what it reaches: a camera's frustum, a directional light's direction,
//! a spot light's cone and a point light's range. The Gizmos button shows or
//! hides them all, and its menu lights' or cameras'. The selected Obj's
//! Collider, Character Controller and Collider 2D are drawn in green, as
//! Unity's are.

use voxear_core::math::{Mat4, Quat, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    physics::{CharacterController, Collider, ColliderShape},
    physics2d::{Collider2D, ColliderShape2D},
};
use voxear_render::{Camera, Light, LightKind};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{Markers, ViewCamera, ring};

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
const COLLIDER: u32 = 0x91f48b;
/// How many pieces a capsule's half circle is drawn in.
const CAP_SEGMENTS: usize = 16;

/// The gizmos for every light and camera in view whose markers show, in the
/// view's own coordinates (from its top left).
pub fn shapes(world: &World, camera: &ViewCamera, selected: Option<Entity>, markers: Markers) -> Vec<Shape> {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    let mut shapes = Vec::new();
    let marked = world
        .query_ref::<(Entity, &Transform, Option<&WorldTransform>, Option<&Light>, Option<&Camera>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    for (entity, transform, placed, light, lens) in marked {
        let (light, lens) = (light.filter(|_| markers.lights), lens.filter(|_| markers.cameras));
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
    if markers.colliders
        && let Some(entity) = selected
    {
        colliders(&mut shapes, world, camera, entity);
    }
    shapes
}

/// The selected Obj's Collider, where physics puts it, and its Character
/// Controller's capsule, standing on the Obj's position.
fn colliders(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let Some(transform) = world.get::<Transform>(entity) else { return };
    let (scale, rotation, position) = match world.get::<WorldTransform>(entity) {
        Some(WorldTransform(matrix)) => matrix.to_scale_rotation_translation(),
        None => (transform.scale, transform.rotation, transform.position),
    };
    let color = Color::hex(COLLIDER);
    let scale = scale.abs().max(Vec3::splat(1e-4));
    if let Some(collider) = world.get::<Collider>(entity) {
        let middle = position + rotation * (Vec3::from(collider.center) * scale);
        match collider.shape {
            ColliderShape::Box => {
                wire_box(shapes, camera, middle, rotation, Vec3::from(collider.size) * scale * 0.5, color)
            }
            ColliderShape::Sphere => {
                let radius = collider.radius * scale.max_element();
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    polyline(shapes, camera, &ring(camera, middle, rotation * axis, radius), color);
                }
            }
            ColliderShape::Capsule => {
                let radius = collider.radius * scale.x.max(scale.z);
                let half = (collider.height * scale.y * 0.5 - radius).max(0.01);
                capsule(shapes, camera, middle, rotation, half, radius, color);
            }
        }
    }
    if let Some(character) = world.get::<CharacterController>(entity) {
        let half = (character.height * 0.5 - character.radius).max(0.01);
        let middle = position + Vec3::Y * (half + character.radius);
        capsule(shapes, camera, middle, Quat::IDENTITY, half, character.radius, color);
    }
    if let Some(collider) = world.get::<Collider2D>(entity) {
        flat_collider(shapes, camera, collider, position, rotation, scale, color);
    }
}

/// A Collider 2D flat in the XY plane, where 2D physics puts it: turned
/// only about Z.
fn flat_collider(
    shapes: &mut Vec<Shape>,
    camera: &ViewCamera,
    collider: &Collider2D,
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
    color: Color,
) {
    let across = rotation * Vec3::X;
    let turn = Quat::from_rotation_z(across.y.atan2(across.x));
    let flat = Vec3::new(scale.x, scale.y, 1.0);
    let middle = position + turn * (Vec2::from(collider.offset).extend(0.0) * flat);
    match collider.shape {
        ColliderShape2D::Box => {
            let half = Vec2::from(collider.size) * Vec2::new(scale.x, scale.y) * 0.5;
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| middle + turn * Vec3::new(x * half.x, y * half.y, 0.0));
            for i in 0..4 {
                line(shapes, camera, corners[i], corners[(i + 1) % 4], color);
            }
        }
        ColliderShape2D::Circle => {
            let radius = collider.radius * scale.x.max(scale.y);
            polyline(shapes, camera, &ring(camera, middle, Vec3::Z, radius), color);
        }
        ColliderShape2D::Capsule => {
            let radius = collider.radius * scale.x;
            let half = (collider.height * scale.y * 0.5 - radius).max(0.01);
            let (up, right) = (turn * Vec3::Y, turn * Vec3::X);
            for end in [-1.0, 1.0] {
                let at = middle + up * half * end;
                let cap: Vec<[f32; 2]> = (0..=CAP_SEGMENTS)
                    .filter_map(|i| {
                        let angle = i as f32 / CAP_SEGMENTS as f32 * std::f32::consts::PI;
                        camera.project(at + (right * angle.cos() + up * end * angle.sin()) * radius)
                    })
                    .collect();
                polyline(shapes, camera, &cap, color);
            }
            for side in [right, -right] {
                line(shapes, camera, middle + side * radius - up * half, middle + side * radius + up * half, color);
            }
        }
    }
}

/// A box's twelve edges, about its middle.
fn wire_box(shapes: &mut Vec<Shape>, camera: &ViewCamera, middle: Vec3, rotation: Quat, half: Vec3, color: Color) {
    let corner = |i: usize| {
        let sign = |bit: usize| if i & bit == 0 { -1.0 } else { 1.0 };
        middle + rotation * (half * Vec3::new(sign(1), sign(2), sign(4)))
    };
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                line(shapes, camera, corner(i), corner(i | bit), color);
            }
        }
    }
}

/// A capsule upright in its rotation: a ring at each end of its middle,
/// four lines between them, and the half circles of its caps.
fn capsule(
    shapes: &mut Vec<Shape>,
    camera: &ViewCamera,
    middle: Vec3,
    rotation: Quat,
    half: f32,
    radius: f32,
    color: Color,
) {
    let (up, right, forward) = (rotation * Vec3::Y, rotation * Vec3::X, rotation * Vec3::Z);
    for end in [-1.0, 1.0] {
        let at = middle + up * half * end;
        polyline(shapes, camera, &ring(camera, at, up, radius), color);
        for across in [right, forward] {
            let cap: Vec<[f32; 2]> = (0..=CAP_SEGMENTS)
                .filter_map(|i| {
                    let angle = i as f32 / CAP_SEGMENTS as f32 * std::f32::consts::PI;
                    camera.project(at + (across * angle.cos() + up * end * angle.sin()) * radius)
                })
                .collect();
            polyline(shapes, camera, &cap, color);
        }
    }
    for side in [right, -right, forward, -forward] {
        line(shapes, camera, middle + side * radius - up * half, middle + side * radius + up * half, color);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera ten metres back, looking at the middle of the world, and an
    /// Obj there.
    fn looking_at_an_obj() -> (World, ViewCamera, Entity) {
        let mut world = World::new();
        let transform = Transform::new(Vec3::new(0.0, 0.0, -10.0), Quat::IDENTITY, Vec3::ONE);
        let lens = world.spawn((Camera::default(), transform));
        let camera = ViewCamera::new(&world, lens, [0.0, 0.0, 800.0, 600.0]).unwrap();
        let obj = world.spawn((Transform::IDENTITY,));
        (world, camera, obj)
    }

    fn green_lines(shapes: &[Shape]) -> usize {
        let green = Color::hex(COLLIDER);
        shapes.iter().filter(|shape| matches!(shape, Shape::Line { color, .. } if *color == green)).count()
    }

    #[test]
    fn the_selected_objs_colliders_are_drawn_in_green() {
        let (mut world, camera, obj) = looking_at_an_obj();
        let shown = Markers { lights: true, cameras: true, colliders: true };
        world.insert(obj, Collider::default());
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), shown)), 12, "a box's edges");
        assert_eq!(green_lines(&shapes(&world, &camera, None, shown)), 0, "only the selected Obj's");
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), Markers::NONE)), 0, "nor with gizmos hidden");

        world.insert(obj, Collider { shape: ColliderShape::Sphere, ..Collider::default() });
        let sphere = green_lines(&shapes(&world, &camera, Some(obj), shown));
        world.insert(obj, Collider { shape: ColliderShape::Capsule, ..Collider::default() });
        let capsule = green_lines(&shapes(&world, &camera, Some(obj), shown));
        assert!(sphere > 12 && capsule > sphere, "three rings, then two rings, four caps and four sides");

        world.remove::<Collider>(obj);
        world.insert(obj, CharacterController::default());
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), shown)), capsule, "a character is a capsule");

        world.remove::<CharacterController>(obj);
        world.insert(obj, Collider2D::default());
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), shown)), 4, "a 2D box is flat");
    }
}
