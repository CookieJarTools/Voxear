//! Gizmos in the Scene view, drawn with the same shapes as the handles and
//! under them: a small sun for each light and a small camera for each camera,
//! where the Obj is, so they can be seen and clicked; and for the selected
//! one, what it reaches: a camera's frustum, a directional light's direction,
//! a spot light's cone and a point light's range. The Gizmos button shows or
//! hides them all, and its menu lights' or cameras'. The selected Obj's
//! Collider, Character Controller and Collider 2D are drawn in green, as
//! Unity's are, and its joint's anchor, a hinge's axis, and a line to what
//! it holds to in orange. Each Audio Source shows a speaker, bright while
//! the game plays it and dim otherwise, and the selected 3D one the
//! distances it is at full volume within and silent past, as rings.

use voxear_core::math::{Mat4, Quat, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    audio::{AudioSource, SoundSpace},
    physics::{CharacterController, Collider, ColliderMesh, ColliderShape, Joint, JointType, Vehicle, Wheel},
    physics2d::{Collider2D, ColliderShape2D},
};
use voxear_render::{Camera, DryArea, DryShape, Light, LightKind, Pool, PostVolume, VolumeShape, WindShape, WindZone};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_terrain::generate::{CaveExit, CaveMode, CaveShape, RoomForm};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{Markers, ViewCamera, ring};

/// A marker's size, in logical pixels: the sun's disc and the camera's body.
const MARKER: f32 = 5.0;
const LIGHT: u32 = 0xf2c94c;
const CAMERA: u32 = 0xc8ccd1;
/// An Audio Source's speaker while it plays, and while it does not.
const SOUND: u32 = 0x6fc3ff;
const QUIET: u32 = 0x7d8590;
/// A speaker's size, in logical pixels.
const SPEAKER: f32 = 16.0;
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
/// Pools' footprints, Dry Areas' shapes and Streams' Paths.
const WATER: u32 = 0x4fc4e8;
/// A Path that is not a Stream's, and how far apart a Stream's chevrons are, in metres.
const PATH: u32 = 0xe8c14f;
const FLOW_MARKS: f32 = 4.0;
/// The most triangles of a Mesh collider drawn, so that a big mesh does not
/// slow the Scene view; the rest are left out.
const MESH_TRIANGLES: usize = 4096;
const JOINT: u32 = 0xf2a14c;
/// A gun's sockets and parts.
const GUN: u32 = 0xe86f6f;
/// How big a joint's anchor and a hinge's axis are drawn, on screen.
const ANCHOR: f32 = 8.0;
const AXIS: f32 = 36.0;
/// A Cave Shape that carves and one that fills, selected and not, and a Cave Exit.
const CAVE: u32 = 0xf2a14c;
const CAVE_DIM: u32 = 0x8f6a3d;
const CAVE_FILL: u32 = 0xc8ccd1;
const CAVE_FILL_DIM: u32 = 0x767b82;
const CAVE_EXIT: u32 = 0x7fe3a1;
const CAVE_EXIT_DIM: u32 = 0x4f8a64;
/// How long a Cave Exit's arrow is, and how wide its ring, in metres.
const EXIT_ARROW: f32 = 3.0;
const EXIT_RING: f32 = 2.0;
/// How many pieces a capsule's half circle is drawn in.
const CAP_SEGMENTS: usize = 16;

/// The gizmos for every light, camera and Audio Source in view whose
/// markers show, in the view's own coordinates (from its top left).
/// `playing` says whether the game is playing an Obj's Audio Source.
pub fn shapes(
    world: &World,
    camera: &ViewCamera,
    selected: Option<Entity>,
    markers: Markers,
    playing: &dyn Fn(Entity) -> bool,
) -> Vec<Shape> {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    let mut shapes = Vec::new();
    let marked = world
        .query_ref::<(Entity, &Transform, Option<&WorldTransform>, Option<&Light>, Option<&Camera>, Option<&AudioSource>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    // An Obj that is off shows no icons, as in Unity.
    let switched = voxear_scene::Switched::of(world);
    for (entity, transform, placed, light, lens, sound) in marked {
        let (light, lens) = (light.filter(|_| markers.lights), lens.filter(|_| markers.cameras));
        let sound = sound.filter(|_| markers.sounds);
        if (light.is_none() && lens.is_none() && sound.is_none()) || !switched.is_active(entity) {
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
        } else if sound.is_some() {
            let color = Color::hex(if playing(entity) { SOUND } else { QUIET });
            shapes.push(Shape::Icon { name: "audio", center: at, size: SPEAKER, color });
        }
        if let Some(source) = sound.filter(|source| chosen && source.space == SoundSpace::ThreeD) {
            sound_reach(&mut shapes, camera, source, position);
        }
    }
    if markers.colliders
        && let Some(entity) = selected
    {
        colliders(&mut shapes, world, camera, entity);
    }
    cave_rooms(&mut shapes, world, camera, selected);
    if let Some(entity) = selected {
        post_volume(&mut shapes, world, camera, entity);
        ui_panel(&mut shapes, world, camera, entity);
        wind_zone(&mut shapes, world, camera, entity);
        water_body(&mut shapes, world, camera, entity);
        path_curve(&mut shapes, world, camera, entity);
        gun_marks(&mut shapes, world, camera, entity);
    }
    shapes
}

/// The selected Gun's sockets, or a Gun Part's travel. A gun's grip and
/// support are drawn as crosses where the wrists go, its sight line from
/// the rear sight on past the front, and its muzzle, magazine well and
/// port as dots; a part's handle is drawn along the path it moves, all the
/// way.
fn gun_marks(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    use voxear_engine::guns::{Gun, mechanism::GunPart};
    let color = Color::hex(GUN);
    let placed = voxear_scene::world_matrix(world, entity);
    if let Some(gun) = world.get::<Gun>(entity) {
        let point = |p: [f32; 3]| placed.transform_point3(Vec3::from(p));
        for wrist in [gun.grip.position, gun.support.position] {
            let at = point(wrist);
            let size = camera.world_size(at, ANCHOR);
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                line(shapes, camera, at - axis * size, at + axis * size, color);
            }
        }
        let (rear, front) = (point(gun.rear_sight), point(gun.front_sight));
        line(shapes, camera, rear, front + (front - rear) * 0.6, color.with_alpha(200));
        for dot in [gun.muzzle, gun.feed, gun.port, gun.butt] {
            let at = point(dot);
            if let Some(on) = camera.project(at) {
                let local = [on[0] - camera.rect[0], on[1] - camera.rect[1]];
                shapes.push(Shape::Circle { center: local, radius: 3.0, fill: color, border: None });
            }
        }
    }
    // A part's handle moves with the part, through every place it can stand.
    let Some(part) = world.get::<GunPart>(entity) else { return };
    let Some(gun) = world.get::<voxear_scene::Parent>(entity).map(|parent| parent.0) else { return };
    let into_world = voxear_scene::world_matrix(world, gun);
    let steps = 24;
    let path: Vec<[f32; 2]> = (0..=steps)
        .filter_map(|step| {
            let place = part.full() * step as f32 / steps as f32;
            camera.project(into_world.transform_point3(part.moved(place).transform_point3(Vec3::from(part.handle))))
        })
        .collect();
    polyline(shapes, camera, &path, color);
}

/// The selected Obj's Path: its curve, and its edges as wide as it is; on a
/// Stream, in water's colour with chevrons every few metres pointing the
/// way it flows.
fn path_curve(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let Some(path) = world.get::<voxear_scene::Path>(entity) else { return };
    let stream = world.has::<voxear_render::Stream>(entity);
    let color = Color::hex(if stream { WATER } else { PATH });
    let faint = color.with_alpha(120);
    let samples = path.samples(voxear_scene::world_matrix(world, entity), 0.5);
    let side = |s: &voxear_scene::PathSample, sign: f32| {
        let across = Vec3::new(s.tangent.z, 0.0, -s.tangent.x).normalize_or(Vec3::X);
        s.position + across * s.width * 0.5 * sign
    };
    for pair in samples.windows(2) {
        line(shapes, camera, pair[0].position, pair[1].position, color);
        for sign in [-1.0, 1.0] {
            line(shapes, camera, side(&pair[0], sign), side(&pair[1], sign), faint);
        }
    }
    if !stream {
        return;
    }
    let mut next = FLOW_MARKS;
    for s in &samples {
        if s.along < next {
            continue;
        }
        next += FLOW_MARKS;
        let back = s.position - s.tangent * s.width * 0.2;
        for sign in [-1.0, 1.0] {
            let wing = (side(s, sign) - s.position) * 0.4;
            line(shapes, camera, back + wing, s.position, color);
        }
    }
}

/// Which way the selected Wind Zone blows: three arrows along its Obj's +Z,
/// level, as the wind is, the longer the stronger it blows.
fn wind_zone(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let Some(wind) = world.get::<WindZone>(entity) else { return };
    let (_, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
    // A Sphere's or a Box's reach over the sea.
    let half = Vec3::from(wind.size) * 0.5;
    match wind.shape {
        WindShape::Global => {}
        WindShape::Sphere => {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                polyline(shapes, camera, &ring(camera, position, rotation * axis, half.x), Color::hex(REACH));
            }
        }
        WindShape::Box => wire_box(shapes, camera, position, rotation, half, Color::hex(REACH)),
    }
    let along = rotation * Vec3::Z;
    let along = Vec3::new(along.x, 0.0, along.z).normalize_or(Vec3::Z);
    let across = Vec3::new(-along.z, 0.0, along.x);
    let size = camera.world_size(position, DIRECTION);
    let length = size * (0.5 + wind.strength.clamp(0.0, 3.0) * 0.5);
    let color = Color::hex(REACH);
    for offset in [-0.35, 0.0, 0.35] {
        let start = position + across * (size * offset) - along * (length * 0.5);
        let tip = start + along * length;
        line(shapes, camera, start, tip, color);
        for side in [-1.0, 1.0] {
            line(shapes, camera, tip, tip - along * (size * 0.15) + across * (size * 0.1 * side), color);
        }
    }
}

/// Every Cave Shape's outline and every Cave Exit's way out, so a room shows
/// whole while one of its shapes is worked on: those that carve in one
/// colour, those that fill in another, the selected Obj's brighter and with
/// its inner margin drawn inside it.
fn cave_rooms(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, selected: Option<Entity>) {
    for (entity, shape) in world.query_ref::<(Entity, &CaveShape)>().without::<Destroying>() {
        let placed = voxear_scene::world_matrix(world, entity);
        let chosen = selected == Some(entity);
        let color = Color::hex(match (shape.mode, chosen) {
            (CaveMode::Carve, true) => CAVE,
            (CaveMode::Carve, false) => CAVE_DIM,
            (CaveMode::Fill, true) => CAVE_FILL,
            (CaveMode::Fill, false) => CAVE_FILL_DIM,
        });
        let size = Vec3::from(shape.size);
        cave_outline(shapes, camera, &placed, shape.shape, size, color);
        // How far in the walls' roughness may come: the same form, that much smaller every way, in metres.
        let scale = Vec3::new(placed.x_axis.length(), placed.y_axis.length(), placed.z_axis.length());
        let inner = size - Vec3::splat(shape.margin * 2.0) / scale.max(Vec3::splat(1.0e-4));
        if chosen && shape.margin > 0.0 && inner.min_element() > 0.0 {
            cave_outline(shapes, camera, &placed, shape.shape, inner, Color::hex(CAVE_DIM));
        }
    }
    for (entity, _) in world.query_ref::<(Entity, &CaveExit)>().without::<Destroying>() {
        let (_, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
        let color = Color::hex(if selected == Some(entity) { CAVE_EXIT } else { CAVE_EXIT_DIM });
        let (way, side, up) = (rotation * Vec3::Z, rotation * Vec3::X, rotation * Vec3::Y);
        let tip = position + way * EXIT_ARROW;
        polyline(shapes, camera, &ring(camera, position, way, EXIT_RING), color);
        line(shapes, camera, position, tip, color);
        for barb in [side, -side, up, -up] {
            line(shapes, camera, tip, tip - way * 0.6 + barb * 0.35, color);
        }
    }
}

/// A Cave Shape's form `size` across, as its Obj's matrix places it: a
/// box's edges, or a ball's three rounds, a half ball's level one whole and
/// its upright two as arches.
fn cave_outline(shapes: &mut Vec<Shape>, camera: &ViewCamera, placed: &Mat4, form: RoomForm, size: Vec3, color: Color) {
    let at = |local: Vec3| placed.transform_point3(local);
    let half = size * 0.5;
    if form == RoomForm::Box {
        let corner = |i: usize| {
            let sign = |bit: usize| if i & bit == 0 { -1.0 } else { 1.0 };
            at(half * Vec3::new(sign(1), sign(2), sign(4)))
        };
        for i in 0..8 {
            for bit in [1, 2, 4] {
                if i & bit == 0 {
                    line(shapes, camera, corner(i), corner(i | bit), color);
                }
            }
        }
        return;
    }
    // A half ball stands on its Obj and is as high as its size; a ball is round its Obj.
    let dome = form == RoomForm::HalfBall;
    let radii = Vec3::new(half.x, if dome { size.y } else { half.y }, half.z);
    let round = |a: Vec3, b: Vec3, whole: bool| {
        let pieces = if whole { 2 * CAP_SEGMENTS } else { CAP_SEGMENTS };
        let points: Option<Vec<[f32; 2]>> = (0..=pieces)
            .map(|i| {
                let angle = i as f32 / CAP_SEGMENTS as f32 * std::f32::consts::PI;
                camera.project(at((a * angle.cos() + b * angle.sin()) * radii))
            })
            .collect();
        points
    };
    for points in [round(Vec3::X, Vec3::Z, true), round(Vec3::X, Vec3::Y, !dome), round(Vec3::Z, Vec3::Y, !dome)] {
        if let Some(points) = points {
            polyline(shapes, camera, &points, color);
        }
    }
}

/// The selected Pool's footprint, flat at its surface, and the selected Dry
/// Area's shape, turned with their Obj.
fn water_body(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let (_, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
    let color = Color::hex(WATER);
    if let Some(pool) = world.get::<Pool>(entity) {
        let half = Vec3::new(pool.size[0] * 0.5, 0.0, pool.size[1] * 0.5);
        wire_box(shapes, camera, position, rotation, half, color);
    }
    let Some(dry) = world.get::<DryArea>(entity) else { return };
    let half = Vec3::from(dry.size) * 0.5;
    match dry.shape {
        DryShape::Box => wire_box(shapes, camera, position, rotation, half, color),
        DryShape::Sphere => {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                polyline(shapes, camera, &ring(camera, position, rotation * axis, half.x), color);
            }
        }
        DryShape::Capsule => {
            let reach = (half.y - half.x).max(0.0);
            for end in [-reach, 0.0, reach] {
                polyline(shapes, camera, &ring(camera, position + rotation * Vec3::new(0.0, end, 0.0), rotation * Vec3::Y, half.x), color);
            }
            for side in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let edge = rotation * (side * half.x);
                line(shapes, camera, position + edge + rotation * Vec3::new(0.0, -reach, 0.0), position + edge + rotation * Vec3::new(0.0, reach, 0.0), color);
            }
        }
        // The mesh shows its own shape where the Obj draws it.
        DryShape::Mesh => {}
    }
}

/// How far the selected Audio Source carries: a sphere of three rings at
/// its min distance, inside which it is at full volume, and a fainter one at
/// its max, past which it is silent.
fn sound_reach(shapes: &mut Vec<Shape>, camera: &ViewCamera, source: &AudioSource, position: Vec3) {
    let faint = Color { a: 110, ..Color::hex(SOUND) };
    for (radius, color) in [(source.min_distance, Color::hex(SOUND)), (source.max_distance, faint)] {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            polyline(shapes, camera, &ring(camera, position, axis, radius), color);
        }
    }
}

/// The selected Obj's Collider, where physics puts it, and its Character
/// Controller's capsule, standing on the Obj's position.
/// A post volume's shape: its box or sphere, and round it where its
/// settings start to blend in, fainter. A global one has no shape to draw.
fn post_volume(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let Some(volume) = world.get::<PostVolume>(entity) else { return };
    let matrix = voxear_scene::world_matrix(world, entity);
    let (scale, rotation, position) = matrix.to_scale_rotation_translation();
    let scale = scale.abs().max(Vec3::splat(1e-4));
    let color = Color::hex(REACH);
    let faint = Color { a: (u16::from(color.a) * 2 / 5) as u8, ..color };
    match volume.shape {
        VolumeShape::Global => {}
        VolumeShape::Box => {
            wire_box(shapes, camera, position, rotation, scale * 0.5, color);
            if volume.blend_distance > 0.0 {
                wire_box(shapes, camera, position, rotation, scale * 0.5 + volume.blend_distance, faint);
            }
        }
        VolumeShape::Sphere => {
            let radius = scale.max_element() * 0.5;
            for (reach, shade) in [(radius, color), (radius + volume.blend_distance, faint)] {
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    polyline(shapes, camera, &ring(camera, position, rotation * axis, reach), shade);
                }
            }
        }
    }
}

/// A UI Panel's rectangle, as it shows to the view's camera.
fn ui_panel(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, entity: Entity) {
    let Some(panel) = world.get::<voxear_render::UiPanel>(entity) else { return };
    let matrix = voxear_scene::world_matrix(world, entity);
    let rect = voxear_render::PanelRect::of(panel, matrix, camera.position);
    let corners = [rect.origin, rect.origin + rect.across, rect.origin + rect.across + rect.down, rect.origin + rect.down];
    let points: Option<Vec<[f32; 2]>> = corners.iter().chain(&corners[..1]).map(|&corner| camera.project(corner)).collect();
    if let Some(points) = points {
        polyline(shapes, camera, &points, Color::hex(REACH));
    }
}

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
            // Its mesh's triangles, once the engine has given them.
            ColliderShape::Mesh => {
                let Some(mesh) = world.get::<ColliderMesh>(entity) else { return };
                let matrix = voxear_scene::world_matrix(world, entity);
                let at = |index: u32| mesh.points.get(index as usize).map(|p| matrix.transform_point3(*p));
                for &[a, b, c] in mesh.indices.as_chunks::<3>().0.iter().take(MESH_TRIANGLES) {
                    let (Some(a), Some(b), Some(c)) = (at(a), at(b), at(c)) else {
                        continue;
                    };
                    for (from, to) in [(a, b), (b, c), (c, a)] {
                        line(shapes, camera, from, to, color);
                    }
                }
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
    if let Some(joint) = world.get::<Joint>(entity) {
        let placed = Mat4::from_scale_rotation_translation(scale, rotation, position);
        joint_marks(shapes, world, camera, joint, placed);
    }
    if let Some(wheel) = world.get::<Wheel>(entity) {
        wheel_marks(shapes, camera, wheel, position, rotation, color);
    }
    // A Vehicle selected shows each of its wheels.
    if world.has::<Vehicle>(entity) {
        let wheels = world.query_ref::<(Entity, &Wheel, &voxear_scene::Parent)>();
        for (child, wheel, _) in wheels.into_iter().filter(|(_, _, parent)| parent.0 == entity) {
            let (_, turn, at) = voxear_scene::world_matrix(world, child).to_scale_rotation_translation();
            wheel_marks(shapes, camera, wheel, at, turn, color);
        }
    }
}

/// A Wheel as a ring about its axle, its Obj's X, and a line up and down
/// its suspension's travel.
fn wheel_marks(shapes: &mut Vec<Shape>, camera: &ViewCamera, wheel: &Wheel, at: Vec3, turn: Quat, color: Color) {
    polyline(shapes, camera, &ring(camera, at, turn * Vec3::X, wheel.radius), color);
    let half = turn * Vec3::Y * (wheel.suspension * 0.5);
    line(shapes, camera, at - half, at + half, color.with_alpha(160));
}

/// A joint's anchor as a small cross, a hinge's axis through it, and a
/// line to the Obj it holds to.
fn joint_marks(shapes: &mut Vec<Shape>, world: &World, camera: &ViewCamera, joint: &Joint, placed: Mat4) {
    let color = Color::hex(JOINT);
    let at = placed.transform_point3(Vec3::from(joint.anchor));
    let size = camera.world_size(at, ANCHOR);
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        line(shapes, camera, at - axis * size, at + axis * size, color);
    }
    if joint.kind == JointType::Hinge {
        let (_, rotation, _) = placed.to_scale_rotation_translation();
        let along = (rotation * Vec3::from(joint.axis)).normalize_or(Vec3::Y) * camera.world_size(at, AXIS);
        line(shapes, camera, at - along, at + along, color);
    }
    if let Some(other) = joint.connected.filter(|&other| world.is_alive(other)) {
        let there = voxear_scene::world_matrix(world, other).w_axis.truncate();
        line(shapes, camera, at, there, color.with_alpha(160));
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
/// cone out to its range, and a point light's range as three rings. A
/// shape is drawn as it is, with a rectangle's or a disc's facing as a line
/// out of its middle, and its range as a ring round it.
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
        LightKind::Rectangle => {
            let (right, up) = (rotation * Vec3::X * (light.width * 0.5), rotation * Vec3::Y * (light.height * 0.5));
            let corners = [position - right - up, position + right - up, position + right + up, position - right + up];
            for (i, &corner) in corners.iter().enumerate() {
                line(shapes, camera, corner, corners[(i + 1) % 4], color);
            }
            facing(shapes, camera, position, forward, light.range, color);
        }
        LightKind::Disc => {
            polyline(shapes, camera, &ring(camera, position, forward, light.radius), color);
            facing(shapes, camera, position, forward, light.range, color);
        }
        LightKind::Tube => {
            let along = rotation * Vec3::X * (light.length * 0.5);
            line(shapes, camera, position - along, position + along, color);
            for end in [position - along, position + along] {
                polyline(shapes, camera, &ring(camera, end, along.normalize_or_zero(), light.radius), color);
            }
            polyline(shapes, camera, &ring(camera, position, rotation * Vec3::Y, light.range), color);
        }
    }
}

/// The way an area light faces, as a line out of its middle, and how far it
/// reaches that way, as a ring.
fn facing(shapes: &mut Vec<Shape>, camera: &ViewCamera, position: Vec3, forward: Vec3, range: f32, color: Color) {
    let tip = position + forward * camera.world_size(position, DIRECTION);
    line(shapes, camera, position, tip, color);
    polyline(shapes, camera, &ring(camera, position, forward, range), color);
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
    fn audio_sources_show_a_speaker_lit_while_playing_and_the_selected_ones_reach() {
        let (mut world, camera, obj) = looking_at_an_obj();
        let shown = Markers { lights: false, cameras: false, sounds: true, colliders: false };
        world.insert(obj, AudioSource::default());
        let speaker = |drawn: &[Shape]| {
            drawn.iter().find_map(|shape| match shape {
                Shape::Icon { name: "audio", color, .. } => Some(*color),
                _ => None,
            })
        };
        assert_eq!(speaker(&shapes(&world, &camera, None, shown, &|_| false)), Some(Color::hex(QUIET)));
        assert_eq!(speaker(&shapes(&world, &camera, None, shown, &|e| e == obj)), Some(Color::hex(SOUND)), "playing");
        assert_eq!(speaker(&shapes(&world, &camera, None, Markers::NONE, &|_| false)), None, "hidden");

        // Selected, a 3D source's min and max distances are rings; a 2D one has none.
        let rings = |world: &World| {
            let drawn = shapes(world, &camera, Some(obj), shown, &|_| false);
            let blue = Color::hex(SOUND);
            let tinted = |color: &Color| (color.r, color.g, color.b) == (blue.r, blue.g, blue.b);
            drawn.iter().filter(|shape| matches!(shape, Shape::Line { color, .. } if tinted(color))).count()
        };
        assert!(rings(&world) > 0);
        world.get_mut::<AudioSource>(obj).unwrap().space = SoundSpace::TwoD;
        assert_eq!(rings(&world), 0);
    }

    #[test]
    fn a_selected_gun_shows_its_sockets_and_a_part_its_handles_path() {
        use voxear_engine::guns::{Gun, mechanism::GunPart};
        let (mut world, camera, obj) = looking_at_an_obj();
        world.insert(obj, Gun::default());
        let red = Color::hex(GUN);
        let marks = |drawn: &[Shape]| {
            drawn.iter().filter(|shape| matches!(shape, Shape::Line { color, .. } | Shape::Circle { fill: color, .. } if color.r == red.r && color.g == red.g)).count()
        };
        let drawn = shapes(&world, &camera, Some(obj), Markers::NONE, &|_| false);
        assert_eq!(marks(&drawn), 3 + 3 + 1 + 4, "two crosses, the sight line and four dots");
        assert_eq!(marks(&shapes(&world, &camera, None, Markers::NONE, &|_| false)), 0, "only the selected gun's");
        let bolt = world.spawn((Transform::IDENTITY, voxear_scene::Parent(obj), GunPart::turn_bolt([0.05, 0.05, 0.1])));
        assert_eq!(marks(&shapes(&world, &camera, Some(bolt), Markers::NONE, &|_| false)), 24, "its handle's path");
    }

    #[test]
    fn the_selected_objs_colliders_are_drawn_in_green() {
        let (mut world, camera, obj) = looking_at_an_obj();
        let shown = Markers { lights: true, cameras: true, sounds: true, colliders: true };
        world.insert(obj, Collider::default());
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), shown, &|_| false)), 12, "a box's edges");
        assert_eq!(green_lines(&shapes(&world, &camera, None, shown, &|_| false)), 0, "only the selected Obj's");
        assert_eq!(
            green_lines(&shapes(&world, &camera, Some(obj), Markers::NONE, &|_| false)),
            0,
            "nor with gizmos hidden"
        );

        world.insert(obj, Collider { shape: ColliderShape::Sphere, ..Collider::default() });
        let sphere = green_lines(&shapes(&world, &camera, Some(obj), shown, &|_| false));
        world.insert(obj, Collider { shape: ColliderShape::Capsule, ..Collider::default() });
        let capsule = green_lines(&shapes(&world, &camera, Some(obj), shown, &|_| false));
        assert!(sphere > 12 && capsule > sphere, "three rings, then two rings, four caps and four sides");

        world.remove::<Collider>(obj);
        world.insert(obj, CharacterController::default());
        assert_eq!(
            green_lines(&shapes(&world, &camera, Some(obj), shown, &|_| false)),
            capsule,
            "a character is a capsule"
        );

        world.remove::<CharacterController>(obj);
        world.insert(obj, Collider2D::default());
        assert_eq!(green_lines(&shapes(&world, &camera, Some(obj), shown, &|_| false)), 4, "a 2D box is flat");

        // A hinge: three lines of its anchor's cross, its axis, and one to what it holds to.
        let post = world.spawn((Transform::from_position(Vec3::new(1.0, 0.0, 0.0)),));
        world.insert(obj, Joint { kind: JointType::Hinge, connected: Some(post), ..Joint::default() });
        let orange = Color::hex(JOINT);
        let drawn = shapes(&world, &camera, Some(obj), shown, &|_| false);
        let joint_lines = drawn
            .iter()
            .filter(|shape| matches!(shape, Shape::Line { color, .. } if color.r == orange.r && color.g == orange.g));
        assert_eq!(joint_lines.count(), 5);
    }
}
