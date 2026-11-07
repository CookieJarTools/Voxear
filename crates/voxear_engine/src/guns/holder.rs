//! The Gun Holder: a character holding a gun in its own hands, in front of
//! its First-Person View's camera. Each frame the gun goes where it is
//! held, at the hip or up to the eye, with every layer of life on top as
//! springs: sway as the view turns, lag as the body moves, a bob at each
//! footfall, breathing and tremor. Near a wall it pulls back, and closer
//! still lifts. Then the hands reach the gun's grip and the other hand's
//! place on it, and close on them.

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat4, Quat, Vec3};
use voxear_core::spring::{Spring, SpringFeel};
use voxear_core::time::Time;
use voxear_ecs::{Component, Entity, World};
use voxear_physics::scene::PhysicsScene;
use voxear_render::{Camera, ModelRenderer, Pose};
use voxear_scene::{Parent, world_matrix};

use super::gun::{Gun, HandRole, Socket};
use super::hands::{Grip, close};
use crate::animation::MotionMatcherState;
use crate::animation::ik::{PoseEdit, aim_hand, reach_hand, turn_spine};
use crate::animation::rig::Rig;
use crate::first_person::{FirstPersonView, first_under, set_world};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GunHolder {
    /// The gun held; the first Gun under the Obj when none. Set by scripts.
    #[serde(skip)]
    pub gun: Option<Entity>,
    /// Whether it aims down the sights, rather than holding at the hip.
    #[serde(skip)]
    pub aiming: bool,
    /// Where the gun's grip is held at the hip, from the camera, in metres:
    /// right, up, forward. And its turn there, in degrees.
    pub hip: [f32; 3],
    pub hip_turn: [f32; 3],
    /// How long raising the gun to the eye takes, in seconds.
    pub aim_time: f32,
    /// How much it sways and lags, bobs with the steps and breathes: 1 as
    /// the gun's weight has it, 0 not at all.
    pub sway: f32,
    pub bob: f32,
    pub breathing: f32,
    /// Whether it pulls back from walls.
    pub wall_check: bool,
}

impl Component for GunHolder {}

impl Default for GunHolder {
    fn default() -> Self {
        Self {
            gun: None,
            aiming: false,
            hip: [0.17, -0.2, 0.36],
            hip_turn: [2.0, -3.0, -6.0],
            aim_time: 0.22,
            sway: 1.0,
            bob: 1.0,
            breathing: 1.0,
            wall_check: true,
        }
    }
}

/// What a holder keeps from frame to frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GunHolderState {
    aim: Spring<f32>,
    /// The turn the gun lags the view by, as angles about the camera's
    /// across, up and forward, in radians.
    sway: Spring<Vec3>,
    /// How far the gun lags the body's moves, in the camera's space.
    lag: Spring<Vec3>,
    /// The bob of the steps, in the camera's space.
    bob: Spring<Vec3>,
    /// How far into a wall the gun would go, 0 to 1.
    wall: Spring<f32>,
    /// How much nearer the body the gun comes so the arms can reach it.
    reach: Spring<f32>,
    view: Option<(f32, f32)>,
    at: Option<Vec3>,
    velocity: Vec3,
    feet: [bool; 2],
    stepped: f32,
    left_foot: bool,
    time: f32,
}

impl Component for GunHolderState {}

impl GunHolderState {
    fn new() -> Self {
        Self {
            aim: Spring::new(0.0, SpringFeel::halflife(0.05)),
            sway: Spring::new(Vec3::ZERO, SpringFeel::new(2.2, 0.45, 0.0)),
            lag: Spring::new(Vec3::ZERO, SpringFeel::new(2.0, 0.55, 0.0)),
            bob: Spring::new(Vec3::ZERO, SpringFeel::new(3.2, 0.4, 0.0)),
            wall: Spring::new(0.0, SpringFeel::halflife(0.06)),
            reach: Spring::new(0.0, SpringFeel::halflife(0.08)),
            view: None,
            at: None,
            velocity: Vec3::ZERO,
            feet: [false; 2],
            stepped: 0.0,
            left_foot: false,
            time: 0.0,
        }
    }

    /// How far raised to the eye it is, 0 at the hip to 1 aiming.
    pub fn aimed(&self) -> f32 {
        self.aim.value.clamp(0.0, 1.0)
    }
}

/// Holds every holder's gun and puts its hands on it: run in the pose
/// stage, after the First-Person View has placed the camera.
pub fn hold(world: &mut World) {
    if crate::editing(world) || !crate::modules::has(world, crate::modules::Module::Guns) {
        return;
    }
    let delta = world.resource::<Time>().map_or(1.0 / 60.0, |time| time.delta).max(1e-4);
    let holders: Vec<(Entity, GunHolder)> =
        world.query_ref::<(Entity, &GunHolder)>().into_iter().map(|(e, h)| (e, h.clone())).collect();
    for (entity, holder) in holders {
        let Some(gun_entity) = holder.gun.or_else(|| first_under(world, entity, |w, e| w.has::<Gun>(e))) else {
            continue;
        };
        let Some(gun) = world.get::<Gun>(gun_entity).cloned() else { continue };
        let Some(view) = world.get::<FirstPersonView>(entity).cloned() else { continue };
        let camera = view
            .camera
            .or_else(|| first_under(world, entity, |w, e| w.has::<Camera>(e)))
            .or_else(|| world.query_ref::<(Entity, &Camera)>().into_iter().map(|(e, _)| e).next());
        let Some(camera) = camera else { continue };
        let eye = world_matrix(world, camera);
        let mut state = world.get::<GunHolderState>(entity).copied().unwrap_or_else(GunHolderState::new);
        let body = view.body.or_else(|| first_under(world, entity, |w, e| w.has::<ModelRenderer>(e)));
        let held = place_gun(world, entity, &holder, &gun, &view, eye, body, &mut state, delta);
        set_world(world, gun_entity, held.w_axis.truncate(), held.to_scale_rotation_translation().1);
        if let Some(body) = body {
            // Short of reach, the gun comes nearer until the hands meet it, and drifts back out slowly once they do.
            let short = hands_on(world, body, &gun, held, eye);
            let wanted = (state.reach.goal() + short - 0.05 * delta).clamp(0.0, 0.45);
            state.reach.update(wanted, delta);
        }
        world.insert(entity, state);
    }
}

/// Where the gun goes this frame, in the world.
#[allow(clippy::too_many_arguments)]
fn place_gun(
    world: &World,
    entity: Entity,
    holder: &GunHolder,
    gun: &Gun,
    view: &FirstPersonView,
    eye: Mat4,
    body: Option<Entity>,
    state: &mut GunHolderState,
    delta: f32,
) -> Mat4 {
    state.time += delta;
    state.aim.feel = SpringFeel::halflife(holder.aim_time.max(0.02) / 4.0);
    let heft = gun.heft();
    // Into a wall: how far along the barrel something is.
    let reach = holder.hip[2] + gun.muzzle[2] + 0.1;
    let blocked = if holder.wall_check { wall(world, entity, eye, reach) } else { 0.0 };
    let wall = state.wall.update(blocked, delta).clamp(0.0, 1.0);
    let aim = state.aim.update(if holder.aiming { 1.0 - wall } else { 0.0 }, delta).clamp(0.0, 1.0);

    // At the hip, and up to the eye with the sights on the camera's line, blended; it dips a little on the way up.
    let [x, y, z] = holder.hip_turn.map(f32::to_radians);
    let hip = Mat4::from_rotation_translation(Quat::from_euler(EulerRot::XYZ, x, y, z), Vec3::from(holder.hip));
    let sighted = Mat4::from_translation(Vec3::Z * gun.eye_relief) * gun.sight_frame().inverse();
    let (_, hip_turn, hip_at) = hip.to_scale_rotation_translation();
    let (_, aim_turn, aim_at) = sighted.to_scale_rotation_translation();
    let mut at = hip_at.lerp(aim_at, ease(aim)) + Vec3::Y * (-0.012 * (aim * std::f32::consts::PI).sin());
    let mut turn = hip_turn.slerp(aim_turn, ease(aim));

    // Near a wall it comes back toward the body, and closer still lifts to high ready.
    at += Vec3::new(0.0, 0.06, -0.22) * wall;
    turn = Quat::from_rotation_x(-55f32.to_radians() * smooth((wall - 0.4) / 0.6)) * turn;
    at.z -= state.reach.value;

    // Sway: the gun lags the view's turns, heavier guns more, yaw rolling it too; aiming steadies it.
    let steady = 1.0 - 0.72 * aim;
    let (yaw, pitch) = (view.yaw, view.pitch);
    if let Some((last_yaw, last_pitch)) = state.view {
        let turned = Vec3::new(pitch - last_pitch, wrap(yaw - last_yaw), 0.0) * std::f32::consts::PI / 180.0;
        let spin = turned / delta;
        let push = (0.016 + 0.024 * heft) * holder.sway * steady;
        state.sway.kick(Vec3::new(spin.x, -spin.y, spin.y * 0.6) * push);
    }
    state.view = Some((yaw, pitch));
    state.sway.feel = SpringFeel::new(2.6 - 1.2 * heft, 0.42 + 0.1 * heft, 0.0);
    let sway = state.sway.update(Vec3::ZERO, delta);
    let sway_turn =
        Quat::from_euler(EulerRot::XYZ, sway.x.clamp(-0.3, 0.3), sway.y.clamp(-0.3, 0.3), sway.z.clamp(-0.4, 0.4));

    // Lag: the gun trails the body's moves, in the camera's space.
    let here = world_matrix(world, entity).w_axis.truncate();
    let velocity = state.at.map_or(Vec3::ZERO, |last| (here - last) / delta);
    state.at = Some(here);
    let into_eye = eye.inverse();
    let moved = into_eye.transform_vector3(velocity);
    let wanted = (-moved * (0.006 + 0.01 * heft) * holder.sway * steady).clamp_length_max(0.05);
    let lag = state.lag.update(wanted, delta);
    state.velocity = velocity;

    // Bob: a kick at each footfall, alternating sides, as hard as the walk is quick.
    let speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
    let feet = body.and_then(|body| world.get::<MotionMatcherState>(body)).map(MotionMatcherState::feet_down);
    let footfall = match feet {
        Some(feet) => {
            let fell = (feet[0] && !state.feet[0]) || (feet[1] && !state.feet[1]);
            state.feet = feet;
            fell.then_some(feet[0] && !feet[1])
        }
        None => {
            state.stepped += speed * delta;
            (state.stepped > 0.75).then(|| {
                state.stepped = 0.0;
                state.left_foot = !state.left_foot;
                state.left_foot
            })
        }
    };
    if let Some(left) = footfall {
        let side = if left { 1.0 } else { -1.0 };
        let hard = (speed / 4.0).clamp(0.15, 1.2) * holder.bob * (1.0 - 0.6 * aim);
        state.bob.kick(Vec3::new(side * 0.04, -0.16, -0.03) * hard);
    }
    let bob = state.bob.update(Vec3::ZERO, delta);
    let bob_roll = Quat::from_rotation_z(-bob.x * 2.5);

    // Breathing, slow and mostly up and down, and a faint tremor of sines that never line up.
    let t = state.time;
    let breath = (t * std::f32::consts::TAU * 0.24).sin() * 0.0022 * holder.breathing * (1.0 - 0.5 * aim);
    let tremor = Vec3::new(
        (t * 1.3).sin() * 0.6 + (t * 3.1).sin() * 0.25 + (t * 7.3).sin() * 0.08,
        (t * 0.9).sin() * 0.5 + (t * 2.3).sin() * 0.3 + (t * 5.9).sin() * 0.1,
        0.0,
    ) * (0.0012 + 0.0025 * heft)
        * holder.breathing
        * (1.0 - 0.6 * aim);
    let tremor_turn = Quat::from_euler(EulerRot::XYZ, tremor.x, tremor.y, 0.0);

    let local = Mat4::from_rotation_translation(
        bob_roll * sway_turn * tremor_turn * turn,
        at + lag + bob * 0.25 + Vec3::Y * breath,
    );
    eye * local
}

/// How far into a wall the gun held out `reach` from the eye would go, 0
/// to 1, missing the holder's own colliders.
fn wall(world: &World, entity: Entity, eye: Mat4, reach: f32) -> f32 {
    let Some(physics) = world.resource::<PhysicsScene>() else { return 0.0 };
    let (from, forward) = (eye.w_axis.truncate(), eye.transform_vector3(Vec3::Z).normalize_or_zero());
    let mut start = 0.0;
    for _ in 0..4 {
        let Some(hit) = physics.raycast(from + forward * start, forward, reach - start, u32::MAX, false) else {
            return 0.0;
        };
        if !is_under(world, hit.obj, entity) {
            return (1.0 - (start + hit.distance) / reach).clamp(0.0, 1.0);
        }
        start += hit.distance + 0.02;
        if start >= reach {
            return 0.0;
        }
    }
    0.0
}

/// Whether `obj` is `owner` or hangs under it.
fn is_under(world: &World, obj: Entity, owner: Entity) -> bool {
    let mut at = Some(obj);
    while let Some(here) = at {
        if here == owner {
            return true;
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    false
}

/// Puts the body's hands on the gun held at `held`: the shooting hand on
/// its grip, the other on its support, elbows down and out, fingers
/// closed. Gives back how far short the arms fell.
fn hands_on(world: &mut World, body: Entity, gun: &Gun, held: Mat4, eye: Mat4) -> f32 {
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else {
        return 0.0;
    };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body).cloned(), Rig::find(&animation.skeleton)) else {
        return 0.0;
    };
    let into_body = world_matrix(world, body).inverse();
    let mut edit = PoseEdit::new(&animation.skeleton, &pose.bones);
    let (right, left) = (&rig.arms[1], &rig.arms[0]);
    // A long gun's fore-end is far out: the chest turns, bringing the support shoulder forward to meet it.
    if !gun.kind.is_handgun() {
        let up = into_body.transform_vector3(Vec3::Y).normalize_or_zero();
        let bladed = (gun.support.position[2] - gun.grip.position[2]).clamp(0.0, 0.6) * 45.0;
        turn_spine(&mut edit, &rig.spine, Quat::from_axis_angle(up, bladed.to_radians()));
    }
    let down = into_body.transform_vector3(eye.transform_vector3(Vec3::NEG_Y));
    let out = into_body.transform_vector3(eye.transform_vector3(Vec3::X));
    let back = into_body.transform_vector3(eye.transform_vector3(Vec3::NEG_Z));
    // Each hand's wrist to its socket, then turned as the socket says a hand holds it there.
    let to_body = into_body * held;
    let place = |socket: &Socket, role: HandRole| {
        let (fingers, palm) = socket.hand(role);
        (
            to_body.transform_point3(Vec3::from(socket.position)),
            to_body.transform_vector3(fingers),
            to_body.transform_vector3(palm),
        )
    };
    let (at, fingers, palm) = place(&gun.grip, HandRole::Grip);
    let shoulder = edit.position(right.upper);
    let short_right = reach_hand(&mut edit, right, at, shoulder + down * 0.6 + out * 0.5 + back * 0.2, 0.35);
    aim_hand(&mut edit, right, fingers, palm);
    close(&mut edit, right, Grip::TRIGGER);
    let role = if gun.kind.is_handgun() { HandRole::HandgunSupport } else { HandRole::Support };
    let (at, fingers, palm) = place(&gun.support, role);
    // Out of reach, the support hand slides back along the barrel, as far as the grip, to where it can hold.
    let shoulder = edit.position(left.upper);
    let arm =
        edit.position(left.forearm).distance(shoulder) + edit.position(left.hand).distance(edit.position(left.forearm));
    let back_along = to_body.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
    let room = (gun.support.position[2] - gun.grip.position[2] - 0.08).max(0.0);
    let slide = (0..14).fold((0.0, room), |(low, high), _| {
        let mid = (low + high) / 2.0;
        if (at + back_along * low).distance(shoulder) <= arm * 0.97 {
            (low, low)
        } else if (at + back_along * mid).distance(shoulder) <= arm * 0.97 {
            (low, mid)
        } else {
            (mid, high)
        }
    });
    let at = at + back_along * slide.1;
    reach_hand(&mut edit, left, at, shoulder + down * 0.7 - out * 0.4 + back * 0.1, 0.45);
    aim_hand(&mut edit, left, fingers, palm);
    close(&mut edit, left, Grip::SUPPORT);
    world.insert(body, Pose { bones: edit.matrices().into() });
    short_right
}

/// Eases 0 to 1 in and out.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth(t: f32) -> f32 {
    ease(t)
}

/// An angle in degrees brought into -180..180.
fn wrap(angle: f32) -> f32 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_render::ModelAnimation;
    use voxear_scene::Transform;

    use super::*;
    use crate::animation::rig::tests::body;
    use crate::first_person::{self, MouseLook};
    use crate::guns::GunKind;

    struct Scene {
        world: World,
        player: Entity,
        body: Entity,
        camera: Entity,
        gun: Entity,
    }

    /// A player with a body, a view, a camera and a gun of `kind` under it.
    fn scene_with(kind: GunKind) -> Scene {
        let mut world = World::new();
        let skeleton = body(|part, side| match side {
            Some(left) => format!("{part}.{}", if left { "L" } else { "R" }),
            None => part.to_string(),
        });
        let rest = skeleton.model_matrices(&skeleton.rest());
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
        let player = world.spawn((Transform::default(),));
        let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
        let body = world.spawn((Transform::default(), Parent(player), model, Pose { bones: rest.into() }));
        let camera = world.spawn((Transform::default(), Parent(player), Camera::default()));
        let gun = world.spawn((Transform::default(), Parent(player), Gun::preset(kind)));
        world.insert(
            player,
            (FirstPersonView { mouse_look: false, ..FirstPersonView::default() }, GunHolder::default()),
        );
        world.insert_resource(MouseLook::default());
        Scene { world, player, body, camera, gun }
    }

    fn scene() -> Scene {
        scene_with(GunKind::BoltActionRifle)
    }

    fn frame(scene: &mut Scene, rest: &[Mat4]) {
        scene.world.insert(scene.body, Pose { bones: rest.to_vec().into() });
        first_person::look(&mut scene.world);
        first_person::place(&mut scene.world);
        hold(&mut scene.world);
    }

    fn rest_of(scene: &Scene) -> Vec<Mat4> {
        scene.world.get::<Pose>(scene.body).unwrap().bones.to_vec()
    }

    #[test]
    fn a_gun_is_held_at_the_hip_with_both_hands_on_it_then_raised_to_the_eye() {
        let mut scene = scene_with(GunKind::OpenBoltSmg);
        let rest = rest_of(&scene);
        for _ in 0..30 {
            frame(&mut scene, &rest);
        }
        let eye = world_matrix(&scene.world, scene.camera);
        let held = world_matrix(&scene.world, scene.gun);
        let at_hip = (eye.inverse() * held).w_axis.truncate();
        let hip = Vec3::from(GunHolder::default().hip);
        // Where the hip hold puts it, but as near the body as the arms need to reach it.
        assert!((at_hip.x - hip.x).abs() < 0.02 && (at_hip.y - hip.y).abs() < 0.08, "at the hip: {at_hip}");
        assert!(at_hip.z <= hip.z + 0.01 && at_hip.z > 0.05, "at the hip: {at_hip}");
        // Both wrists are on their sockets.
        let gun = scene.world.get::<Gun>(scene.gun).unwrap().clone();
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        let bones = scene.world.get::<Pose>(scene.body).unwrap().bones.clone();
        let placed = world_matrix(&scene.world, scene.body);
        let wrist = |arm: usize| placed.transform_point3(bones[rig.arms[arm].hand].w_axis.truncate());
        let socket = |s: &Socket| held.transform_point3(Vec3::from(s.position));
        assert!(wrist(1).distance(socket(&gun.grip)) < 0.02, "the right hand is on the grip");
        // The left hand is on the fore-end, slid back along it as far as the arm needs.
        let (support, grip) = (socket(&gun.support), socket(&gun.grip));
        let along = held.transform_vector3(Vec3::Z).normalize();
        let off_line = (wrist(0) - support) - along * (wrist(0) - support).dot(along);
        assert!(off_line.length() < 0.03, "on the fore-end's line: {}", off_line.length());
        assert!((wrist(0) - grip).dot(along) > 0.05, "ahead of the grip");

        scene.world.get_mut::<GunHolder>(scene.player).unwrap().aiming = true;
        for _ in 0..60 {
            frame(&mut scene, &rest);
        }
        let eye = world_matrix(&scene.world, scene.camera);
        let sights = world_matrix(&scene.world, scene.gun) * gun.sight_frame();
        let along = sights.transform_vector3(Vec3::Z).normalize();
        assert!(along.dot(eye.transform_vector3(Vec3::Z).normalize()) > 0.999, "the sights look where the eye does");
        let rear = eye.inverse().transform_point3(sights.w_axis.truncate());
        assert!(rear.truncate().length() < 0.01, "on the eye's line: {rear}");
        assert!((rear.z - gun.eye_relief).abs() < 0.01, "at the eye relief: {rear}");
    }

    #[test]
    fn turning_the_view_sways_the_gun_behind_it_and_it_settles() {
        let mut scene = scene();
        let rest = rest_of(&scene);
        for _ in 0..30 {
            frame(&mut scene, &rest);
        }
        let held = |scene: &Scene| {
            let local = world_matrix(&scene.world, scene.camera).inverse() * world_matrix(&scene.world, scene.gun);
            local.to_scale_rotation_translation().1
        };
        let settled = held(&scene);
        scene.world.get_mut::<FirstPersonView>(scene.player).unwrap().yaw += 25.0;
        let swung = (0..10)
            .map(|_| {
                frame(&mut scene, &rest);
                held(&scene).angle_between(settled)
            })
            .fold(0.0, f32::max);
        assert!(swung > 0.03, "it lags the turn: {swung}");
        for _ in 0..180 {
            frame(&mut scene, &rest);
        }
        assert!(held(&scene).angle_between(settled) < 0.01, "and settles");
    }
}
