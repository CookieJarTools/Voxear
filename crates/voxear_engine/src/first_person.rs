//! The First-Person View: a camera in a character's own head. The mouse,
//! or a script, turns the view; the Obj it is on turns with it, and the
//! upper body leans into where the view looks. The camera rides the head
//! bone, steadied so the head's bob reaches it only in part, and the head
//! itself is hidden so the camera does not see inside it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec2, Vec3};
use voxear_core::spring::{Spring, SpringFeel};
use voxear_core::time::Time;
use voxear_ecs::{Component, Entity, World};
use voxear_render::{Camera, ModelRenderer, Pose};
use voxear_scene::{Parent, Transform, world_matrix};

use crate::animation::ik::{PoseEdit, turn_spine};
use crate::animation::rig::Rig;

/// The most the upper body leans into the view, in degrees.
const MOST_LEAN: f32 = 40.0;
/// The most the upper body turns from the legs to face the view, in
/// radians: about 80 degrees.
const MOST_TWIST: f32 = 1.4;
/// How near a first-person camera sees, in metres: what is held at the eye
/// and the hands working before it come closer than a camera usually
/// looks, and would be cut off.
const NEAR: f32 = 0.01;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FirstPersonView {
    /// The Obj whose model is the body; the first Model Renderer under
    /// the Obj when none. Set by scripts.
    #[serde(skip)]
    pub body: Option<Entity>,
    /// The camera it places; the first Camera under the Obj, or else the
    /// first anywhere, when none. Set by scripts.
    #[serde(skip)]
    pub camera: Option<Entity>,
    /// Where it looks, in degrees: the turn about up from +Z, and the tilt,
    /// up positive.
    pub yaw: f32,
    pub pitch: f32,
    /// Whether the mouse turns it, by `sensitivity` degrees a pixel.
    pub mouse_look: bool,
    pub sensitivity: f32,
    /// How far it may tilt down and up, in degrees.
    pub pitch_limits: [f32; 2],
    /// How much of the head's bob reaches the camera: 0 none, 1 all of it.
    pub bob: f32,
    /// Where the eyes are from the head bone, in metres, in the body's
    /// space: up and forward.
    pub eyes: [f32; 3],
    /// Whether the Obj it is on turns to face where the view does.
    pub turn_obj: bool,
    /// How much of the tilt the upper body leans into, 0 to 1.
    pub lean: f32,
    /// How much of the way the upper body turns to face where the view
    /// looks when the legs have not yet, 0 to 1.
    pub twist: f32,
    /// Whether the head and neck are hidden from the camera, so it does not
    /// see inside them; they still cast their shadow.
    pub hide_head: bool,
}

impl Component for FirstPersonView {}

impl Default for FirstPersonView {
    fn default() -> Self {
        Self {
            body: None,
            camera: None,
            yaw: 0.0,
            pitch: 0.0,
            mouse_look: true,
            sensitivity: 0.12,
            pitch_limits: [-85.0, 85.0],
            bob: 0.35,
            eyes: [0.0, 0.07, 0.11],
            turn_obj: true,
            lean: 0.5,
            twist: 1.0,
            hide_head: true,
        }
    }
}

impl FirstPersonView {
    /// Where it looks, as a turn in the world.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw.to_radians()) * Quat::from_rotation_x(-self.pitch.to_radians())
    }
}

/// What the view keeps from frame to frame: where the eyes were, steadied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstPersonState {
    /// The eyes' place relative to the Obj, following the head on a spring.
    eyes: Spring<Vec3>,
    /// A jolt to the camera's turn, as a shot's kick gives it, springing back:
    /// angles about its across, up and forward, in radians.
    punch: Spring<Vec3>,
    /// How far the eyes glance down from where the view looks, toward what
    /// the hands are doing, in radians, and how far they are asked to.
    glance: Spring<f32>,
    glance_goal: f32,
    /// Where the eyes are asked to be next frame instead of in the head,
    /// relative to the Obj, and how much: by clips holding something, filmed
    /// from somewhere else.
    framed: Option<(Vec3, f32)>,
    started: bool,
    /// Where the eyes are and look this frame, in the world, glances aside:
    /// the camera's place for the player's own view, and for anyone else's,
    /// what they hold is held before.
    pub eye: Mat4,
}

impl FirstPersonState {
    fn new(eyes: Vec3) -> Self {
        Self {
            eyes: Spring::new(eyes, SpringFeel::halflife(0.06)),
            punch: Spring::new(Vec3::ZERO, SpringFeel::new(5.0, 0.55, 0.0)),
            glance: Spring::new(0.0, SpringFeel::new(1.6, 0.9, 0.0)),
            glance_goal: 0.0,
            framed: None,
            started: false,
            eye: Mat4::IDENTITY,
        }
    }
}

/// Has a view's eyes glance down by `angle` radians next frame, toward
/// what its hands are doing, easing back once it is no longer asked to;
/// where it aims is left as it was.
pub fn glance(world: &mut World, entity: Entity, angle: f32) {
    if let Some(state) = world.get_mut::<FirstPersonState>(entity) {
        state.glance_goal = angle;
    }
}

/// Has a view's eyes go to `at` next frame, relative to its Obj, `weight`
/// of the way from the head, easing there and back: where the clips
/// holding something were filmed from. Once it is no longer asked to, they go
/// back to the head.
pub fn frame_eyes(world: &mut World, entity: Entity, at: Vec3, weight: f32) {
    if let Some(state) = world.get_mut::<FirstPersonState>(entity) {
        state.framed = Some((at, weight.clamp(0.0, 1.0)));
    }
}

/// Jolts a view's camera by a spin, angles a second about its across, up
/// and forward, which springs back: a shot's kick, a landing, a blast.
pub fn punch(world: &mut World, entity: Entity, spin: Vec3) {
    let Some(state) = world.get_mut::<FirstPersonState>(entity) else { return };
    state.punch.push(spin, 0.03);
}

impl Component for FirstPersonState {}

/// Marks a view posed while editing too, where no other is: one the editor
/// previews what it holds in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PosedWhileEditing;

impl Component for PosedWhileEditing {}


/// The mouse as this frame moved it, for views that look with it: the game
/// puts it in the world each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseLook {
    pub delta: Vec2,
}

/// Turns each view by the mouse, and the Obj it is on with it: run in
/// Update, after the behaviours, so a script can turn it first and the
/// body faces where it looks this frame.
pub fn look(world: &mut World) {
    if crate::editing(world) {
        return;
    }
    let delta = world.resource::<MouseLook>().map_or(Vec2::ZERO, |mouse| mouse.delta);
    let views: Vec<Entity> = world.query_ref::<(Entity, &FirstPersonView)>().into_iter().map(|(e, _)| e).collect();
    let views: Vec<Entity> = views.into_iter().filter(|&entity| is_own(world, entity)).collect();
    for entity in views {
        let Some(view) = world.get_mut::<FirstPersonView>(entity) else { continue };
        if view.mouse_look {
            view.yaw += delta.x * view.sensitivity;
            view.pitch -= delta.y * view.sensitivity;
        }
        view.yaw = (view.yaw + 180.0).rem_euclid(360.0) - 180.0;
        view.pitch = view.pitch.clamp(view.pitch_limits[0], view.pitch_limits[1]);
        let (turn, yaw) = (view.turn_obj, view.yaw);
        if turn && let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.rotation = Quat::from_rotation_y(yaw.to_radians());
        }
    }
}

/// Leans each view's upper body into its tilt, places its camera in the
/// head and hides the head: run in the pose stage.
pub fn place(world: &mut World) {
    let delta = world.resource::<Time>().map_or(1.0 / 60.0, |time| time.delta);
    let editing = crate::editing(world);
    let views: Vec<(Entity, FirstPersonView)> = world
        .query_ref::<(Entity, &FirstPersonView)>()
        .into_iter()
        .filter(|(entity, _)| !editing || world.has::<PosedWhileEditing>(*entity))
        .map(|(e, v)| (e, v.clone()))
        .collect();
    for (entity, view) in views {
        // Only this machine's own player looks out of their head; anyone else's keeps it.
        let own = is_own(world, entity);
        // The eyes follow the head on a spring from where the Obj is, so the head's bob is steadied; a view with
        // no body looks from the Obj itself.
        let obj = world_matrix(world, entity);
        // While clips hold something the eyes stay put on the body and the upper body does not lean, so the shoulders
        // keep their place under them, as the clips' were under their camera.
        let framed = world.get::<FirstPersonState>(entity).is_some_and(|state| state.framed.is_some());
        let posed = pose_body(world, entity, &view, own, framed).map(|eyes| obj.inverse().transform_point3(eyes));
        let state = world.get::<FirstPersonState>(entity).copied();
        let mut state = state.unwrap_or_else(|| FirstPersonState::new(posed.unwrap_or(Vec3::ZERO)));
        let eyes_now = match (posed, state.framed.take()) {
            (Some(eyes), Some((framed, weight))) => eyes.lerp(framed, weight),
            (posed, _) => posed.unwrap_or(Vec3::ZERO),
        };
        // The eyes start in the head the first time the body is posed, not on a spring up from the Obj's feet,
        // which would leave what is held before them far below the shoulders.
        if !state.started && posed.is_some() {
            state.eyes.snap(eyes_now);
            state.started = true;
        }
        let steadied = state.eyes.update(eyes_now, delta);
        let shown = steadied.lerp(eyes_now, view.bob.clamp(0.0, 1.0));
        let jolt = state.punch.update(Vec3::ZERO, delta);
        let jolt = Quat::from_euler(voxear_core::math::EulerRot::XYZ, jolt.x, jolt.y, jolt.z);
        // A glance turns the camera alone: what is held stays where it is, and the eyes look down at it.
        let down = state.glance.update(std::mem::take(&mut state.glance_goal), delta);
        state.eye = Mat4::from_rotation_translation(view.rotation() * jolt, obj.transform_point3(shown));
        let jolt = Quat::from_rotation_x(down) * jolt;
        world.insert(entity, state);
        if !own {
            continue;
        }
        // While editing, only a camera the view names is moved: never the level's own.
        let camera = view.camera.or_else(|| {
            let under = (!editing).then(|| first_under(world, entity, |w, e| w.has::<Camera>(e))).flatten();
            under.or_else(|| {
                (!editing).then(|| world.query_ref::<(Entity, &Camera)>().into_iter().map(|(e, _)| e).next()).flatten()
            })
        });
        if let Some(camera) = camera {
            set_world(world, camera, obj.transform_point3(shown), view.rotation() * jolt);
            // In play only, so a saved level keeps its camera as it was set.
            if let Some(lens) = world.get_mut::<Camera>(camera).filter(|_| !editing) {
                lens.near = lens.near.min(NEAR);
            }
        }
    }
}

/// Turns a view's body's upper body toward where the view looks and leans
/// it, hiding its head for this machine's own player, and gives back where
/// its eyes are in the world; none for a view with no body. While clips
/// hold something, `framed`, the eyes stay put low in the head and the upper
/// body does not lean, so the shoulders keep their place under them, as the
/// clips' did under their camera; and the upper chest and collarbones round
/// the eyes are hidden too, as the steps carry them up past the eyes.
fn pose_body(world: &mut World, entity: Entity, view: &FirstPersonView, own: bool, framed: bool) -> Option<Vec3> {
    let lean = if framed { 0.0 } else { view.lean };
    let body = body_of(world, entity, view)?;
    let animation = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone())?;
    let (pose, rig) = (world.get::<Pose>(body).cloned()?, Rig::find(&animation.skeleton)?);
    let mut edit = PoseEdit::new(&animation.skeleton, &pose.bones);
    let placed = world_matrix(world, body);
    let into_body = placed.inverse();
    // The upper body turns to face where the view looks, though the legs, turning at their own pace, lag
    // behind: the chest, its arms and what they hold stay with the eyes.
    let up = into_body.transform_vector3(Vec3::Y).normalize_or_zero();
    let looking = into_body.transform_vector3(view.rotation() * Vec3::Z);
    let shoulders = edit.position(rig.arms[1].upper) - edit.position(rig.arms[0].upper);
    let facing = shoulders.cross(up);
    let flat = |v: Vec3| (v - up * v.dot(up)).normalize_or_zero();
    let (facing, looking) = (flat(facing), flat(looking));
    if facing != Vec3::ZERO && looking != Vec3::ZERO {
        let behind = facing.cross(looking).dot(up).atan2(facing.dot(looking)) * view.twist.clamp(0.0, 1.0);
        turn_spine(&mut edit, &rig.spine, Quat::from_axis_angle(up, behind.clamp(-MOST_TWIST, MOST_TWIST)));
    }
    // The upper body leans about the body's own across, so it bends forward and back.
    let across = into_body.transform_vector3(view.rotation() * Vec3::X).normalize_or_zero();
    let lean = (-view.pitch * lean.clamp(0.0, 1.0)).clamp(-MOST_LEAN, MOST_LEAN).to_radians();
    if across != Vec3::ZERO {
        turn_spine(&mut edit, &rig.spine, Quat::from_axis_angle(across, lean));
    }
    let head = edit.matrices()[rig.head];
    let eyes_in_body = head.w_axis.truncate() + Vec3::from(view.eyes);
    world.insert(body, Pose { bones: edit.matrices().into() });
    // The neck goes with the head where the rig has one: a camera low in the head, as one held where a pack's clips
    // were filmed from, sits nearer the neck than the head.
    if view.hide_head && own {
        crate::animation::hide(world, body, &animation.skeleton, rig.neck.unwrap_or(rig.head));
        if framed {
            let round: Vec<usize> = rig.arms.iter().filter_map(|arm| arm.clavicle).chain([rig.chest()]).collect();
            crate::animation::hide_only(world, body, &animation.skeleton, &round);
        }
    }
    Some(placed.transform_point3(eyes_in_body))
}

// What `values` reads or writes, as `FirstPersonView` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const YAW: i32 = 3;
const PITCH: i32 = 4;
const MOUSE_LOOK: i32 = 5;
const SENSITIVITY: i32 = 6;
const PITCH_LIMITS: i32 = 7;
const BOB: i32 = 8;
const EYES: i32 = 9;
const TURN_OBJ: i32 = 10;
const LEAN: i32 = 11;
const HIDE_HEAD: i32 = 12;
const BODY: i32 = 13;
const CAMERA: i32 = 14;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::FirstPersonView, values);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    use crate::cameras::{obj_value, value_obj};
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let flag = |on: bool| number(f32::from(u8::from(on)));
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<FirstPersonView>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<FirstPersonView>(entity) {
                world.insert(entity, FirstPersonView::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<FirstPersonView>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let view = world.get_mut::<FirstPersonView>(entity)?;
    let Some(v) = written else {
        return match what {
            YAW => Some(number(view.yaw)),
            PITCH => Some(number(view.pitch)),
            MOUSE_LOOK => Some(flag(view.mouse_look)),
            SENSITIVITY => Some(number(view.sensitivity)),
            PITCH_LIMITS => Some([view.pitch_limits[0], view.pitch_limits[1], 0.0, 0.0]),
            BOB => Some(number(view.bob)),
            EYES => Some([view.eyes[0], view.eyes[1], view.eyes[2], 0.0]),
            TURN_OBJ => Some(flag(view.turn_obj)),
            LEAN => Some(number(view.lean)),
            HIDE_HEAD => Some(flag(view.hide_head)),
            BODY => Some(obj_value(view.body)),
            CAMERA => Some(obj_value(view.camera)),
            _ => None,
        };
    };
    match what {
        YAW => view.yaw = v[0],
        PITCH => view.pitch = v[0].clamp(view.pitch_limits[0], view.pitch_limits[1]),
        MOUSE_LOOK => view.mouse_look = v[0] != 0.0,
        SENSITIVITY => view.sensitivity = v[0].max(0.0),
        PITCH_LIMITS => view.pitch_limits = [v[0].clamp(-90.0, 0.0), v[1].clamp(0.0, 90.0)],
        BOB => view.bob = v[0].clamp(0.0, 1.0),
        EYES => view.eyes = [v[0], v[1], v[2]],
        TURN_OBJ => view.turn_obj = v[0] != 0.0,
        LEAN => view.lean = v[0].clamp(0.0, 1.0),
        HIDE_HEAD => view.hide_head = v[0] != 0.0,
        BODY => view.body = value_obj(v),
        CAMERA => view.camera = value_obj(v),
        _ => return None,
    }
    Some([0.0; 4])
}

/// Whether a view is this machine's own player's: always offline, and in
/// networked play, a view on an Obj this machine's player owns, or on one
/// not shared at all.
pub fn is_own(world: &World, entity: Entity) -> bool {
    let Some(session) = world.resource::<crate::net::NetSession>() else { return true };
    let mut at = Some(entity);
    while let Some(here) = at {
        if let Some(networked) = world.get::<crate::net::Networked>(here) {
            return networked.owner.is_some() && networked.owner == session.local_player();
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    true
}

/// Puts an Obj at a place and turn in the world, through its parent.
pub(crate) fn set_world(world: &mut World, entity: Entity, position: Vec3, rotation: Quat) {
    let parent = world.get::<Parent>(entity).map(|parent| parent.0);
    let into_parent = parent.map_or(Mat4::IDENTITY, |parent| world_matrix(world, parent).inverse());
    let local = into_parent * Mat4::from_rotation_translation(rotation, position);
    if let Some(transform) = world.get_mut::<Transform>(entity) {
        let (_, rotation, position) = local.to_scale_rotation_translation();
        (transform.position, transform.rotation) = (position, rotation);
    }
}

/// The body a view looks out of: the one it names, or else the first model
/// under its Obj with a body's bones, a head and arms, so a skinned model
/// it holds is not taken for it.
pub(crate) fn body_of(world: &World, entity: Entity, view: &FirstPersonView) -> Option<Entity> {
    view.body.or_else(|| first_under(world, entity, is_body))
}

/// Whether an Obj draws a model whose skeleton is a humanoid's.
fn is_body(world: &World, entity: Entity) -> bool {
    let skeleton = world.get::<ModelRenderer>(entity).and_then(|model| model.animation.as_ref());
    skeleton.is_some_and(|animation| crate::animation::rig::Rig::find(&animation.skeleton).is_some())
}

/// The first Obj under `entity`, itself included, that `is` picks, going
/// down by children in order.
pub(crate) fn first_under(world: &World, entity: Entity, is: impl Fn(&World, Entity) -> bool + Copy) -> Option<Entity> {
    if is(world, entity) {
        return Some(entity);
    }
    let mut children: Vec<Entity> = world
        .query_ref::<(Entity, &Parent)>()
        .into_iter()
        .filter(|(_, parent)| parent.0 == entity)
        .map(|(child, _)| child)
        .collect();
    children.sort_by_key(|child| child.index());
    children.into_iter().find_map(|child| first_under(world, child, is))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_render::ModelAnimation;
    use voxear_scene::Name;

    use super::*;
    use crate::animation::rig::tests::body;

    /// A player Obj with a body under it and a camera, and a view on the player.
    fn player(world: &mut World) -> (Entity, Entity, Entity) {
        let skeleton = body(|part, side| match side {
            Some(left) => format!("{part}.{}", if left { "L" } else { "R" }),
            None => part.to_string(),
        });
        let rest = skeleton.model_matrices(&skeleton.rest());
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
        let player = world.spawn((Name("Player".into()), Transform::from_position(Vec3::new(2.0, 0.0, 0.0))));
        let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
        let body = world.spawn((Transform::default(), Parent(player), model, Pose { bones: rest.into() }));
        let camera = world.spawn((Transform::default(), Camera::default()));
        world.insert(player, FirstPersonView::default());
        (player, body, camera)
    }

    #[test]
    fn the_mouse_turns_the_view_and_the_obj_and_the_camera_rides_the_head() {
        let mut world = World::new();
        let (player, body, camera) = player(&mut world);
        world.insert_resource(MouseLook { delta: Vec2::new(750.0, -200.0) });
        look(&mut world);
        let view = world.get::<FirstPersonView>(player).unwrap().clone();
        assert!((view.yaw - 90.0).abs() < 1e-3 && (view.pitch - 24.0).abs() < 1e-3, "{} {}", view.yaw, view.pitch);
        let turned = world.get::<Transform>(player).unwrap().rotation;
        assert!(
            turned.angle_between(Quat::from_rotation_y(90f32.to_radians())) < 1e-3,
            "the player faces the view: {turned}"
        );

        world.insert_resource(MouseLook { delta: Vec2::ZERO });
        place(&mut world);
        let eye = world.get::<Transform>(camera).unwrap();
        let skeleton_head = world_matrix(&world, body).transform_point3(Vec3::new(0.0, 1.65, 0.0));
        assert!(
            eye.position.distance(skeleton_head) < 0.2,
            "the camera is at the head: {} {}",
            eye.position,
            skeleton_head
        );
        assert!(eye.rotation.angle_between(view.rotation()) < 1e-4, "and looks where the view does");
        let near = world.get::<Camera>(camera).unwrap().near;
        assert!(near <= 0.01, "and sees from close enough not to cut off the hands: {near}");
        let head =
            Rig::find(&world.get::<ModelRenderer>(body).unwrap().animation.as_ref().unwrap().skeleton).unwrap().head;
        let hidden = world.get::<voxear_render::HiddenBones>(body).expect("the head is hidden");
        let rig = Rig::find(&world.get::<ModelRenderer>(body).unwrap().animation.as_ref().unwrap().skeleton).unwrap();
        assert!(hidden.bones[head] && hidden.bones[rig.neck.unwrap()], "the head and neck are hidden from the camera");
        assert!(world.get::<Pose>(body).unwrap().bones[head].x_axis.length() > 0.5, "though it stays where it is");
    }

    #[test]
    fn the_eyes_start_at_the_head_once_the_body_is_posed() {
        let mut world = World::new();
        let (_, body, camera) = player(&mut world);
        // A body not posed yet, as on the frame it is made: the eyes have no head to be in.
        let posed = world.remove::<Pose>(body).unwrap();
        place(&mut world);
        world.insert(body, posed);
        place(&mut world);
        let eye = world.get::<Transform>(camera).unwrap().position;
        let head = world_matrix(&world, body).transform_point3(Vec3::new(0.0, 1.65, 0.0));
        assert!(eye.distance(head) < 0.2, "the camera is at the head at once, not rising from the feet: {eye} {head}");
    }

    #[test]
    fn the_upper_body_turns_to_face_the_view_though_the_legs_lag() {
        let mut world = World::new();
        let (player, body, _) = player(&mut world);
        world.get_mut::<FirstPersonView>(player).unwrap().turn_obj = false;
        // The legs, and all above them, still turned 50 degrees aside, as a body turning at its own pace is.
        let animation = world.get::<ModelRenderer>(body).unwrap().animation.clone().unwrap();
        let mut skewed = animation.skeleton.rest();
        skewed[0].rotation = Quat::from_rotation_y(50f32.to_radians());
        world.insert(body, Pose { bones: animation.skeleton.model_matrices(&skewed).into() });
        place(&mut world);
        let rig = Rig::find(&animation.skeleton).unwrap();
        let bones = &world.get::<Pose>(body).unwrap().bones;
        let across = bones[rig.arms[1].upper].w_axis.truncate() - bones[rig.arms[0].upper].w_axis.truncate();
        let facing = across.cross(Vec3::Y).normalize();
        assert!(facing.dot(Vec3::Z) > 0.99, "the shoulders face where the view looks: {facing}");
    }

    #[test]
    fn the_upper_body_leans_into_the_tilt_and_nothing_moves_while_editing() {
        let mut world = World::new();
        let (player, body, _) = player(&mut world);
        world.get_mut::<FirstPersonView>(player).unwrap().pitch = -40.0;
        world.get_mut::<FirstPersonView>(player).unwrap().hide_head = false;
        let before = world.get::<Pose>(body).unwrap().bones[4].w_axis.truncate();
        place(&mut world);
        let after = world.get::<Pose>(body).unwrap().bones[4].w_axis.truncate();
        assert!(after.z > before.z + 0.05, "looking down leans forward: {before} {after}");

        world.insert_resource(crate::EditMode(true));
        world.insert_resource(MouseLook { delta: Vec2::new(500.0, 0.0) });
        let yaw = world.get::<FirstPersonView>(player).unwrap().yaw;
        look(&mut world);
        assert_eq!(world.get::<FirstPersonView>(player).unwrap().yaw, yaw, "the editor's mouse is its own");
    }
}
