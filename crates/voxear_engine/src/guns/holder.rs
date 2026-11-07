//! The Gun Holder: a character holding a gun in its own hands, in front of
//! its First-Person View's camera. Each frame the gun goes where it is
//! held, at the hip or up to the eye, with every layer of life on top as
//! springs: sway as the view turns, lag as the body moves, a bob at each
//! footfall, breathing and tremor. Near a wall it pulls back, and closer
//! still lifts. Then the hands reach the gun's grip and the other hand's
//! place on it, and close on them.

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat3, Mat4, Quat, Vec3};
use voxear_core::spring::{Spring, SpringFeel};
use voxear_core::time::Time;
use voxear_ecs::{Component, Entity, World};
use voxear_physics::scene::PhysicsScene;
use voxear_render::{ModelRenderer, Pose};
use voxear_scene::{Parent, world_matrix};

use super::actions::{ActionKind, Hand, plan};
use super::contact::{Shape, surface};
use super::gun::{Gun, GunKind, HandRole, Socket};
use super::hands::{Grip, close, keep_out, palm_of, seat_hand};
use super::mechanism::{GunPart, GunState, Known, PartRole};
use super::playing::{ActionPlay, HandAim, Throwing, advance, curve, minimum_jerk};
use crate::animation::MotionMatcherState;
use crate::animation::ik::{PoseEdit, aim_turn, turn_spine};
use crate::animation::rig::{Arm, Rig};
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
    /// Where a handgun's grip is held, from the camera, in metres: right, up
    /// and forward, the arms out. And how a gun is turned when held at the
    /// hip, in degrees.
    pub hip: [f32; 3],
    pub hip_turn: [f32; 3],
    /// Where a long gun's butt sits from the shooting shoulder, in metres in
    /// the body's space: in toward the chest, up and forward, so its stock
    /// rests in the shoulder in both first and third person.
    pub shoulder: [f32; 3],
    /// How long raising the gun to the eye takes, in seconds.
    pub aim_time: f32,
    /// How much it sways and lags, bobs with the steps and breathes: 1 as
    /// the gun's weight has it, 0 not at all.
    pub sway: f32,
    pub bob: f32,
    pub breathing: f32,
    /// Whether it pulls back from walls.
    pub wall_check: bool,
    /// Rounds it carries for reloading.
    pub spare: u32,
    /// Whether a bolt-action or pump is worked by itself after each shot.
    pub auto_cycle: bool,
    /// An action asked for, which starts when nothing else is under way.
    #[serde(skip)]
    pub request: Option<ActionKind>,
}

impl Component for GunHolder {}

impl Default for GunHolder {
    fn default() -> Self {
        Self {
            gun: None,
            aiming: false,
            hip: [0.16, -0.22, 0.2],
            hip_turn: [7.0, -2.0, -5.0],
            shoulder: [-0.06, -0.03, 0.06],
            aim_time: 0.22,
            sway: 1.0,
            bob: 1.0,
            breathing: 1.0,
            wall_check: true,
            spare: 90,
            auto_cycle: true,
            request: None,
        }
    }
}

/// What a holder keeps from frame to frame.
#[derive(Clone, Debug, PartialEq)]
pub struct GunHolderState {
    /// The action under way, the gun's tilt for it, and where each hand
    /// eases back to its hold from once an action ends.
    play: Option<ActionPlay>,
    tilt: Spring<Vec3>,
    /// How far back along the gun the support hand is planted.
    support: Option<Spring<f32>>,
    /// How far an action has brought the gun up from its hold to show its
    /// works, 0 to 1.
    working: Spring<f32>,
    returning: [Option<(Vec3, f32)>; 2],
    /// Where the gun was last frame, for finding the pouches on it.
    held: Option<Mat4>,
    /// Seconds since a self-loader's last shot worked its action, and
    /// seconds until a manual action is worked after one.
    cycling: Option<f32>,
    cycle_after: Option<f32>,
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
    /// Shots: when the last left, in the game's time, how many in this
    /// burst, the kicks still to show, and the pattern's way, wandering
    /// shot to shot.
    last_shot: Option<f64>,
    burst: u32,
    kicks: u32,
    pattern: f32,
    /// How far the shots have climbed the view, in degrees, not yet
    /// recovered; and the tilt the view was left at, to tell the player's
    /// own pull-down from it.
    climb: f32,
    left_pitch: Option<f32>,
    /// The last shot's line, for scripts to read after firing.
    pub(crate) shot: Option<Shot>,
    seed: u64,
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
            play: None,
            tilt: Spring::new(Vec3::ZERO, SpringFeel::new(2.4, 0.7, 0.0)),
            support: None,
            working: Spring::new(0.0, SpringFeel::new(1.7, 0.78, 0.0)),
            returning: [None; 2],
            held: None,
            cycling: None,
            cycle_after: None,
            aim: Spring::new(0.0, SpringFeel::halflife(0.05)),
            sway: Spring::new(Vec3::ZERO, SpringFeel::new(2.2, 0.45, 0.0)),
            lag: Spring::new(Vec3::ZERO, SpringFeel::new(2.0, 0.55, 0.0)),
            bob: Spring::new(Vec3::ZERO, SpringFeel::new(3.2, 0.4, 0.0)),
            wall: Spring::new(0.0, SpringFeel::halflife(0.06)),
            reach: Spring::new(0.0, SpringFeel::halflife(0.08)),
            view: None,
            last_shot: None,
            burst: 0,
            kicks: 0,
            pattern: 0.0,
            climb: 0.0,
            left_pitch: None,
            shot: None,
            seed: 0x9e37_79b9_7f4a_7c15,
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

    /// The action under way, if any.
    pub fn acting(&self) -> Option<ActionKind> {
        self.play.as_ref().map(|play| play.kind)
    }

    /// Stops the action under way where it is: the hands ease back to the
    /// gun from wherever they were.
    pub fn cancel(&mut self) {
        if let Some(play) = self.play.take() {
            for (hand, at) in play.hands.iter().enumerate() {
                self.returning[hand] = at.map(|at| (at, 0.0));
            }
        }
    }

    /// A number from -1 to 1, the next of the holder's own run.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}

/// A shot's line: where it starts, at the eye, and the way it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Fires the holder's gun, when it is ready to fire again by its rate and
/// not held against a wall: the view climbs by the gun's recoil, the gun
/// and camera kick, and the shot's line comes back, from the eye along the
/// view, spread as much as the hold is loose.
pub fn fire(world: &mut World, entity: Entity) -> Option<Shot> {
    if !crate::modules::has(world, crate::modules::Module::Guns) {
        return None;
    }
    let holder = world.get::<GunHolder>(entity)?.clone();
    let gun_entity = holder.gun.or_else(|| first_under(world, entity, |w, e| w.has::<Gun>(e)))?;
    let gun = world.get::<Gun>(gun_entity)?.clone();
    let now = world.resource::<Time>().map_or(0.0, |time| time.elapsed);
    let mut state = world.get::<GunHolderState>(entity).cloned().unwrap_or_else(GunHolderState::new);
    let interval = 60.0 / f64::from(gun.fire_rate.max(1.0));
    if state.last_shot.is_some_and(|last| now - last < interval - 1e-4)
        || state.wall.value > 0.6
        || state.play.is_some()
    {
        return None;
    }
    // Only a gun ready to fire fires: a round chambered, or an open bolt cocked over a fed magazine.
    let mut mechanism = super::mechanism::state_of(world, gun_entity)?;
    if !mechanism.ready(gun.kind) {
        return None;
    }
    let dry = mechanism.shoot(gun.kind);
    // A self-loader throws its case out as it works; an en-bloc clip pings out after the last round.
    if gun.kind.self_loading() && gun.kind != GunKind::TopBreakRevolver {
        let (held, seed) = (state.held.unwrap_or(Mat4::IDENTITY), state.random());
        super::leftovers::throw_case(world, &mechanism, held, state.velocity, now, seed);
    }
    if dry && gun.kind.feed() == super::mechanism::Feed::EnBloc {
        let held = state.held.unwrap_or(Mat4::IDENTITY);
        let (up, right) = (held.transform_vector3(Vec3::Y), held.transform_vector3(Vec3::X));
        let ping = state.velocity + up * 3.2 + right * 0.6;
        let size = Vec3::new(0.012, 0.05, 0.06);
        super::leftovers::throw(world, &mechanism, PartRole::Clip, "En-Bloc Clip", ping, right * 15.0, size, now);
    }
    mechanism.known_chamber = if gun.kind.self_loading() { mechanism.known_chamber } else { Known::Exact };
    world.insert(gun_entity, mechanism);
    state.cycling = gun.kind.self_loading().then_some(0.0);
    if !gun.kind.self_loading() && holder.auto_cycle {
        state.cycle_after = Some(0.16);
    }
    // A burst runs on while shots follow close behind one another.
    if state.last_shot.is_none_or(|last| now - last > interval * 2.5 + 0.1) {
        state.burst = 0;
    }
    state.last_shot = Some(now);
    state.burst += 1;
    state.kicks += 1;

    // Where shots go climbs as the gun kicks, the first shot hardest, and wanders aside in a pattern of its own.
    let heft = gun.heft();
    let first = if state.burst == 1 { 1.35 } else { 1.0 };
    let up = 2.4 * gun.recoil * (1.0 - 0.55 * heft) * first;
    state.pattern = (state.pattern + state.random() * 0.45).clamp(-1.2, 1.2);
    let aside = state.pattern.sin() * up * 0.45;
    let aimed = state.aimed();
    let view = world.get_mut::<FirstPersonView>(entity)?;
    view.pitch = (view.pitch + up).clamp(view.pitch_limits[0], view.pitch_limits[1]);
    view.yaw += aside;
    state.climb += up;
    state.left_pitch = Some(view.pitch);
    // The climb is the shot's, not a turn of the view the gun should lag.
    if let Some((yaw, pitch)) = state.view {
        state.view = Some((yaw + aside, pitch + up));
    }
    let rotation = view.rotation();

    // The line: from the camera, along the view, within a cone that narrows as the gun comes up to the eye.
    // From the view's eyes, which the camera is at for this machine's player and others' heads are for theirs.
    let eye = world.get::<crate::first_person::FirstPersonState>(entity).map(|state| state.eye.w_axis.truncate());
    let origin = eye.unwrap_or_else(|| world_matrix(world, entity).w_axis.truncate() + Vec3::Y * 1.6);
    let spread = (2.2 - 2.05 * aimed).to_radians();
    let (a, b) = (state.random(), state.random());
    let off = Quat::from_euler(EulerRot::YXZ, a * spread, b * spread, 0.0);
    let shot = Shot { origin, direction: (rotation * off * Vec3::Z).normalize() };
    state.shot = Some(shot);
    world.insert(entity, state);
    Some(shot)
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
        // The gun is held before the view's eyes: the camera for this machine's player, their head for others.
        let Some(eye) = world.get::<crate::first_person::FirstPersonState>(entity).map(|state| state.eye) else {
            continue;
        };
        let mut state = world.get::<GunHolderState>(entity).cloned().unwrap_or_else(GunHolderState::new);
        recover(world, entity, &gun, &mut state, delta);
        for _ in 0..state.kicks {
            let kick = gun.recoil * (1.0 - 0.5 * gun.heft());
            let aside = state.random() * 0.25 * kick;
            crate::first_person::punch(world, entity, Vec3::new(-0.9 * kick, aside, 0.0));
        }
        let view = world.get::<FirstPersonView>(entity).cloned().unwrap_or(view);
        let body = view.body.or_else(|| first_under(world, entity, |w, e| w.has::<ModelRenderer>(e)));

        // The action asked for starts, if there is one to do; a manual action is worked a moment after a shot.
        let Some(mut mechanism) = super::mechanism::state_of(world, gun_entity) else { continue };
        let mut holder = holder;
        if let Some(after) = state.cycle_after.as_mut() {
            *after -= delta;
            if *after <= 0.0 {
                state.cycle_after = None;
                holder.request = holder.request.or(Some(ActionKind::Cycle));
            }
        }
        if let Some(kind) = holder.request.take().filter(|_| state.play.is_none()) {
            let beats = plan(kind, gun.kind, &mechanism, holder.spare);
            if !beats.is_empty() {
                state.play = Some(ActionPlay::new(kind, beats));
                state.returning = [None; 2];
            }
        }
        if let Some(stored) = world.get_mut::<GunHolder>(entity) {
            stored.request = None;
        }
        let pouches = pouches(world, body, state.held);
        let mut spare = holder.spare;
        let mut aim = None;
        let mut tilt = Vec3::ZERO;
        if let Some(mut play) = state.play.take() {
            let now = world.resource::<Time>().map_or(0.0, |time| time.elapsed);
            let throwing =
                Throwing { held: state.held.unwrap_or(eye), velocity: state.velocity, now, seed: state.random() };
            match advance(world, &mut play, &gun, &mut mechanism, &mut spare, pouches, throwing, delta) {
                Some((hand, angle)) => {
                    (aim, tilt) = (Some(hand), angle);
                    // The beats' knocks shake the gun, and the eyes glance down at what the hands are doing.
                    let (spin, push) = std::mem::take(&mut play.knock);
                    state.sway.kick(spin);
                    state.lag.push(push, 0.03);
                    let down = state.held.map_or(0.0, |held| glance_at(eye, held, &gun));
                    crate::first_person::glance(world, entity, down);
                    state.play = Some(play);
                }
                None => {
                    let (spin, push) = std::mem::take(&mut play.knock);
                    state.sway.kick(spin);
                    state.lag.push(push, 0.03);
                    for (hand, at) in play.hands.iter().enumerate() {
                        state.returning[hand] = at.map(|at| (at, 0.0));
                    }
                }
            }
        }
        if let Some(stored) = world.get_mut::<GunHolder>(entity) {
            stored.spare = spare;
        }
        cycle(world, &gun, &mut mechanism, &mut state, delta);
        super::leftovers::hide_templates(world, &mechanism);
        super::leftovers::settle(world, world.resource::<Time>().map_or(0.0, |time| time.elapsed));
        world.insert(gun_entity, mechanism.clone());
        super::mechanism::show_parts(world, &mechanism);
        if let Some(play) = &state.play {
            super::playing::carry_now(world, &mechanism, play);
        }
        state.tilt.update(tilt, delta);

        if let Some(body) = body {
            blade(world, body, &gun);
        }
        let mut held = place_gun(world, entity, &holder, &gun, &view, eye, body, &mut state, delta);
        set_world(world, gun_entity, held.w_axis.truncate(), held.to_scale_rotation_translation().1);
        if let Some(body) = body {
            let aims = hand_aims(&gun, aim, &mut state, delta);
            let shapes = super::contact::shapes_now(world, gun_entity, &[]);
            let avoid = super::contact::shapes_now(world, gun_entity, &in_hand(world, &mechanism, state.play.as_ref()));
            let (posed, planted) = (world.get::<Pose>(body).cloned(), state.support);
            let hold = |at| Held { gun: &gun, at, shapes: &shapes, avoid: &avoid };
            let short = hands_on(world, body, &hold(held), eye, aims, &mut state.support, delta);
            // Short of reach, the gun comes nearer at once, so the hands never let go of it; it stays nearer,
            // drifting back out slowly once they meet it.
            if short > 0.002
                && let Some(posed) = posed
            {
                held = Mat4::from_translation(eye.transform_vector3(Vec3::NEG_Z) * short) * held;
                set_world(world, gun_entity, held.w_axis.truncate(), held.to_scale_rotation_translation().1);
                (state.support, _) = (planted, world.insert(body, posed));
                hands_on(world, body, &hold(held), eye, aims, &mut state.support, delta);
            }
            let wanted = (state.reach.goal() + short - 0.05 * delta).clamp(0.0, 0.45);
            state.reach.update(wanted, delta);
        }
        state.held = Some(held);
        world.insert(entity, state);
    }
}

/// Brings the view back down from the shots' climb once they stop, all but
/// what the player pulled down themself; a bolt-action recovers none, as
/// its shooter takes aim again for each shot.
fn recover(world: &mut World, entity: Entity, gun: &Gun, state: &mut GunHolderState, delta: f32) {
    let now = world.resource::<Time>().map_or(0.0, |time| time.elapsed);
    let quiet = state.last_shot.is_none_or(|last| now - last > 60.0 / f64::from(gun.fire_rate.max(1.0)) + 0.08);
    let Some(view) = world.get_mut::<FirstPersonView>(entity) else { return };
    if let Some(left) = state.left_pitch {
        let own = view.pitch - left;
        if own < 0.0 {
            state.climb = (state.climb + own).max(0.0);
        }
    }
    if quiet && gun.kind != super::GunKind::BoltActionRifle && state.climb > 0.0 {
        let back = state.climb * (1.0 - (-delta * 9.0).exp());
        view.pitch -= back;
        state.climb -= back;
    }
    state.left_pitch = Some(view.pitch);
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
    // An action other than working the bolt takes the gun down from the eye; working the bolt only half lifts it.
    let acting = state.play.as_ref().map(|play| play.kind);
    let busy = acting.is_some_and(|kind| kind != ActionKind::Cycle);
    let aim = state.aim.update(if holder.aiming && !busy { 1.0 - wall } else { 0.0 }, delta).clamp(0.0, 1.0);
    let lift = match acting {
        None => 0.0,
        Some(ActionKind::Cycle) => 0.4,
        Some(_) => 1.0,
    };
    let working = state.working.update(lift * (1.0 - aim), delta).max(0.0);

    // At the hip, and up to the eye with the sights on the camera's line, blended; it dips a little on the way up.
    let [x, y, z] = holder.hip_turn.map(f32::to_radians);
    let hip = Mat4::from_rotation_translation(Quat::from_euler(EulerRot::XYZ, x, y, z), Vec3::from(holder.hip));
    let sighted = Mat4::from_translation(Vec3::Z * gun.eye_relief) * gun.sight_frame().inverse();
    let (_, hip_turn, mut hip_at) = hip.to_scale_rotation_translation();
    // A long gun's butt sits in the shoulder; a handgun is held out in front.
    let shouldered = body.filter(|_| !gun.kind.is_handgun()).and_then(|body| pocket(world, body, holder));
    match shouldered {
        Some(pocket) => hip_at = eye.inverse().transform_point3(pocket) - hip_turn * Vec3::from(gun.butt),
        None => hip_at.z += 0.18,
    }
    let (_, aim_turn, aim_at) = sighted.to_scale_rotation_translation();
    let mut at = hip_at.lerp(aim_at, ease(aim)) + Vec3::Y * (-0.012 * (aim * std::f32::consts::PI).sin());
    let mut turn = hip_turn.slerp(aim_turn, ease(aim));

    // Near a wall it comes back toward the body, and closer still lifts to high ready.
    at += Vec3::new(0.0, 0.06, -0.22) * wall;
    turn = Quat::from_rotation_x(-55f32.to_radians() * smooth((wall - 0.4) / 0.6)) * turn;

    // An action brings the works up before the eyes, the muzzle tipped up and aside so the butt clears the
    // chest, and tilts the gun toward the work about them.
    let tilt = state.tilt.value * (std::f32::consts::PI / 180.0);
    let worked = Quat::from_euler(EulerRot::ZYX, -tilt.x, tilt.y, -tilt.z);
    let shown = Quat::from_euler(EulerRot::YXZ, -0.32 * working, -0.1 * working, 0.0);
    let works = turn * shown * worked * gun.works();
    at += (WORK_AT - works - at) * working;
    turn = turn * shown * worked;
    at.z -= state.reach.value;

    // Each shot kicks the gun up and back about the grip, rolling it the other way each time.
    let steady = 1.0 - 0.72 * aim;
    let kick = gun.recoil * (1.0 - 0.5 * heft);
    for _ in 0..std::mem::take(&mut state.kicks) {
        let roll = if state.burst.is_multiple_of(2) { 1.0 } else { -1.0 };
        let aside = state.random() * 1.5 * kick;
        state.sway.kick(Vec3::new(-7.0 * kick, aside, roll * 3.0 * kick) * (1.0 - 0.35 * aim));
        state.lag.push(Vec3::new(0.0, 0.01, -0.09) * kick, 0.025);
    }
    // Sway: the gun lags the view's turns, heavier guns more, yaw rolling it too; aiming steadies it.
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

/// Where an action brings a gun's works, from the eyes: right, up and
/// forward, in metres.
const WORK_AT: Vec3 = Vec3::new(0.07, -0.2, 0.42);

/// How far the eyes glance down to see a gun's works, held at `held`:
/// enough to bring them well up from the bottom of the view.
fn glance_at(eye: Mat4, held: Mat4, gun: &Gun) -> f32 {
    let works = eye.inverse().transform_point3(held.transform_point3(gun.works()));
    ((-works.y).atan2(works.z.max(0.05)) - 0.3).clamp(0.0, 0.3)
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

/// Works a self-loader's action after each shot, its bolt or slide flying
/// back and home again, held back if the gun ran dry.
fn cycle(world: &World, gun: &Gun, mechanism: &mut GunState, state: &mut GunHolderState, delta: f32) {
    let Some(t) = state.cycling.as_mut() else { return };
    *t += delta;
    let period = (60.0 / gun.fire_rate.max(60.0)).min(0.12) * 0.9;
    let s = (*t / period).min(1.0);
    let place = if mechanism.open { s.min(0.5) * 2.0 } else { (s * std::f32::consts::PI).sin() };
    let role = match gun.kind {
        GunKind::SlidePistol => PartRole::Slide,
        GunKind::TogglePistol => PartRole::Toggle,
        _ => PartRole::Bolt,
    };
    if let Some(entry) =
        mechanism.parts.iter_mut().find(|(e, ..)| world.get::<GunPart>(*e).is_some_and(|p| p.role == role))
    {
        entry.1 = place;
    }
    if s >= 1.0 {
        state.cycling = None;
    }
}

/// Where each hand's pouch is, in the gun's space as it was last frame:
/// at the front of each hip.
fn pouches(world: &World, body: Option<Entity>, held: Option<Mat4>) -> [Vec3; 2] {
    let fallback = [Vec3::new(-0.25, -0.35, -0.25), Vec3::new(0.2, -0.35, -0.3)];
    let (Some(body), Some(held)) = (body, held) else { return fallback };
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else {
        return fallback;
    };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body), Rig::find(&animation.skeleton)) else { return fallback };
    let into_gun = held.inverse() * world_matrix(world, body);
    let hips = pose.bones[rig.hips].w_axis.truncate();
    [Vec3::new(-0.17, -0.02, 0.12), Vec3::new(0.17, -0.02, 0.12)].map(|side| into_gun.transform_point3(hips + side))
}

/// Each hand's aim this frame, left then right: the working hand's, and a
/// hand easing back to its hold after an action.
fn hand_aims(gun: &Gun, working: Option<HandAim>, state: &mut GunHolderState, delta: f32) -> [Option<HandAim>; 2] {
    let mut aims = [None; 2];
    for (index, hand) in [Hand::Left, Hand::Right].into_iter().enumerate() {
        if let Some(aim) = working.filter(|aim| aim.hand == hand) {
            aims[index] = Some(aim);
            continue;
        }
        if let Some((from, t)) = state.returning[index].as_mut() {
            *t += delta / 0.28;
            // Back to its hold, coming in to the grip from the side and to a fore-end from below, settling into it.
            let home = super::playing::socket(gun, hand);
            let (grip, towards) =
                if hand == Hand::Right { (Grip::TRIGGER, Vec3::X) } else { (Grip::SUPPORT, Vec3::NEG_Y) };
            let s = minimum_jerk(*t);
            let at = curve([*from, *from, home + towards * 0.06, home], s);
            aims[index] = Some(HandAim { hand, at, grip, settle: s, toward: towards });
            if *t >= 1.0 {
                state.returning[index] = None;
            }
        }
    }
    aims
}

/// Turns the chest for a long gun, its fore-end far out, bringing the
/// support shoulder forward to meet it: before the gun is placed, so its
/// butt goes to the shoulder where the shoulder ends up.
fn blade(world: &mut World, body: Entity, gun: &Gun) {
    if gun.kind.is_handgun() {
        return;
    }
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else { return };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body).cloned(), Rig::find(&animation.skeleton)) else { return };
    let mut edit = PoseEdit::new(&animation.skeleton, &pose.bones);
    let up = world_matrix(world, body).inverse().transform_vector3(Vec3::Y).normalize_or_zero();
    let bladed = (gun.support.position[2] - gun.grip.position[2]).clamp(0.0, 0.6) * 40.0;
    turn_spine(&mut edit, &rig.spine, Quat::from_axis_angle(up, bladed.to_radians()));
    world.insert(body, Pose { bones: edit.matrices().into() });
}

/// Where a long gun's butt goes, in the world: the body's shooting
/// shoulder, as posed now, moved by the holder's `shoulder` in the body's
/// space.
fn pocket(world: &World, body: Entity, holder: &GunHolder) -> Option<Vec3> {
    let animation = world.get::<ModelRenderer>(body)?.animation.clone()?;
    let rig = Rig::find(&animation.skeleton)?;
    let pose = world.get::<Pose>(body)?;
    let shoulder = pose.bones.get(rig.arms[1].upper)?.w_axis.truncate();
    Some(world_matrix(world, body).transform_point3(shoulder + Vec3::from(holder.shoulder)))
}

/// What a body's hands hold this frame: the gun, where it is held, and its
/// surfaces, in its own space: all of them, which fingers close on, and
/// those but the parts a hand has, which hands keep out of.
struct Held<'a> {
    gun: &'a Gun,
    at: Mat4,
    shapes: &'a [Shape],
    avoid: &'a [Shape],
}

/// Puts the body's hands on the gun: the shooting hand's palm on its grip,
/// the other's on its support, planted where it can hold, elbows down and
/// out, fingers closed round what they hold, before the eyes at `eye`.
/// Gives back how far short the shooting hand fell.
fn hands_on(
    world: &mut World,
    body: Entity,
    held: &Held,
    eye: Mat4,
    aims: [Option<HandAim>; 2],
    support: &mut Option<Spring<f32>>,
    delta: f32,
) -> f32 {
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else {
        return 0.0;
    };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body).cloned(), Rig::find(&animation.skeleton)) else {
        return 0.0;
    };
    let into_body = world_matrix(world, body).inverse();
    let mut edit = PoseEdit::new(&animation.skeleton, &pose.bones);
    let (right, left) = (&rig.arms[1], &rig.arms[0]);
    let way = |v: Vec3| into_body.transform_vector3(eye.transform_vector3(v));
    let (down, out, back) = (way(Vec3::NEG_Y), way(Vec3::X), way(Vec3::NEG_Z));
    let to_body = into_body * held.at;
    let shapes: Vec<Shape> = held.shapes.iter().map(|shape| shape.moved(to_body)).collect();
    let avoid: Vec<Shape> = held.avoid.iter().map(|shape| shape.moved(to_body)).collect();
    let placing = Placing { gun: held.gun, to_body, shapes: held.shapes };

    let (contact, axes) = placing.aimed(aims[1], &held.gun.grip, HandRole::Grip, 0.0);
    let shoulder = edit.position(right.upper);
    let pole = shoulder + down * 0.6 + out * 0.5 + back * 0.2;
    let short = seat_clear(&mut edit, right, (contact, axes), (pole, 0.15), &avoid, working(aims[1]));
    close(&mut edit, right, aims[1].map_or(Grip::TRIGGER, |aim| aim.grip), &shapes);

    let shoulder = edit.position(left.upper);
    let pole = shoulder + down * 0.7 - out * 0.4 + back * 0.1;
    let role = if held.gun.kind.is_handgun() { HandRole::HandgunSupport } else { HandRole::Support };
    let slide = match aims[0] {
        // A hand working lets go of where it was planted, and plants afresh once it is back.
        Some(_) => {
            *support = None;
            0.0
        }
        None => planted(support, placing.slide_needed(&edit, left, role, shoulder), delta),
    };
    let (contact, axes) = placing.aimed(aims[0], &held.gun.support, role, slide);
    seat_clear(&mut edit, left, (contact, axes), (pole, 0.2), &avoid, working(aims[0]));
    close(&mut edit, left, aims[0].map_or(Grip::SUPPORT, |aim| aim.grip), &shapes);
    world.insert(body, Pose { bones: edit.matrices().into() });
    if aims[1].is_some() { 0.0 } else { short }
}

/// The parts a hand has now: the one the beat under way drives, and the
/// one it carries.
fn in_hand(world: &World, mechanism: &GunState, play: Option<&ActionPlay>) -> Vec<Entity> {
    let Some(beat) = play.and_then(ActionPlay::beat) else { return Vec::new() };
    let roles = [beat.carry, beat.drive.map(|(role, ..)| role)];
    let has = |entity: Entity| world.get::<GunPart>(entity).is_some_and(|part| roles.contains(&Some(part.role)));
    mechanism.parts.iter().map(|&(entity, ..)| entity).filter(|&entity| has(entity)).collect()
}

/// How much a hand is working rather than resting on its hold, 0 to 1.
fn working(aim: Option<HandAim>) -> f32 {
    aim.map_or(0.0, |aim| 1.0 - aim.settle.clamp(0.0, 1.0))
}

/// Seats a hand, its elbow toward the pole and its collarbone taking its
/// share, and while it works moves it out of the gun as far as it went in,
/// and a little more, a few times over: so its palm, wrist and knuckles stay out of the gun
/// all along a reach. A hand resting is left as its hold puts it. Gives
/// back how far short it fell.
fn seat_clear(
    edit: &mut PoseEdit,
    arm: &Arm,
    (contact, axes): (Vec3, (Vec3, Vec3)),
    (pole, clavicle): (Vec3, f32),
    shapes: &[Shape],
    working: f32,
) -> f32 {
    let before = edit.local.clone();
    let mut contact = contact;
    let mut short = seat_hand(edit, arm, contact, axes, pole, clavicle);
    // A little past each push, so out of one box against another it gets clear of both in a few goes.
    for _ in 0..5 {
        let push = keep_out(edit, arm, shapes) * working * 1.1;
        if push.length() < 0.001 {
            break;
        }
        contact += push;
        edit.local.clone_from(&before);
        short = seat_hand(edit, arm, contact, axes, pole, clavicle);
    }
    short
}

/// Where hands go on a gun, in the body's space.
struct Placing<'a> {
    gun: &'a Gun,
    to_body: Mat4,
    shapes: &'a [Shape],
}

impl Placing<'_> {
    /// A hand at rest on a socket, `slide` metres back along the gun: the
    /// middle of its palm where it holds, on the gun's surface, and the
    /// way its fingers point and palm faces.
    fn resting(&self, socket: &Socket, role: HandRole, slide: f32) -> (Vec3, (Vec3, Vec3)) {
        let (fingers, palm) = socket.hand(role);
        let at = Vec3::from(socket.position) + Vec3::NEG_Z * slide;
        let contact = surface(self.shapes, at, -palm);
        (self.to_body.transform_point3(contact), self.axes(fingers, palm))
    }

    /// A hand working, holding a part or a round where its aim says: its
    /// palm against the way it came in, fingers across the gun, so it
    /// pinches what it holds from that side without lying over the rest of
    /// the gun; with no way in, fingers along the gun and palm toward its
    /// bore.
    fn working(&self, aim: HandAim) -> (Vec3, (Vec3, Vec3)) {
        if let Some(toward) = aim.toward.try_normalize() {
            let palm = -toward;
            let fingers = palm.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::Z);
            let contact = aim.at + toward * 0.02 - fingers * 0.05;
            return (self.to_body.transform_point3(contact), self.axes(fingers, palm));
        }
        let bore = Vec3::new(0.0, 0.03, aim.at.z);
        let side = if aim.hand == Hand::Right { Vec3::NEG_X } else { Vec3::X };
        let palm = (bore - aim.at).try_normalize().unwrap_or(side);
        let fingers = (Vec3::Z - palm * palm.z).normalize_or(Vec3::Z);
        let contact = aim.at - fingers * 0.06 - palm * 0.015;
        (self.to_body.transform_point3(contact), self.axes(fingers, palm))
    }

    /// A hand as its aim has it, eased toward its rest on `socket` as far
    /// as it has settled; at rest with no aim.
    fn aimed(&self, aim: Option<HandAim>, socket: &Socket, role: HandRole, slide: f32) -> (Vec3, (Vec3, Vec3)) {
        let rest = self.resting(socket, role, slide);
        let Some(aim) = aim else { return rest };
        let work = self.working(aim);
        let t = aim.settle.clamp(0.0, 1.0);
        let turn =
            |(fingers, palm): (Vec3, Vec3)| Quat::from_mat3(&Mat3::from_cols(fingers, palm, fingers.cross(palm)));
        let (from, to) = (turn(work.1), turn(rest.1));
        let blended = from.slerp(to, t);
        (work.0.lerp(rest.0, t), (blended * Vec3::X, blended * Vec3::Y))
    }

    fn axes(&self, fingers: Vec3, palm: Vec3) -> (Vec3, Vec3) {
        let fingers = self.to_body.transform_vector3(fingers).normalize_or_zero();
        let palm = self.to_body.transform_vector3(palm);
        (fingers, (palm - fingers * palm.dot(fingers)).normalize_or_zero())
    }

    /// How far back along the gun, from its support socket toward its grip,
    /// the support hand must hold for its arm to reach from `shoulder`.
    fn slide_needed(&self, edit: &PoseEdit, arm: &Arm, role: HandRole, shoulder: Vec3) -> f32 {
        let length = edit.position(arm.forearm).distance(shoulder)
            + edit.position(arm.hand).distance(edit.position(arm.forearm));
        let room = (self.gun.support.position[2] - self.gun.grip.position[2] - 0.08).max(0.0);
        let reaches = |slide: f32| {
            let (contact, (fingers, palm)) = self.resting(&self.gun.support, role, slide);
            let offset = match (aim_turn(edit, arm, fingers, palm), palm_of(edit, arm)) {
                (Some(turn), Some(now)) => turn * (now.point - edit.position(arm.hand)),
                _ => Vec3::ZERO,
            };
            (contact - offset).distance(shoulder) <= length * 0.97
        };
        if reaches(0.0) {
            return 0.0;
        }
        let (mut low, mut high) = (0.0, room);
        for _ in 0..14 {
            let middle = (low + high) / 2.0;
            if reaches(middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        high
    }
}

/// Where the support hand holds, as far back along the gun as it is
/// planted: it stays there while the arm can reach, so it does not creep
/// as the body moves, and goes back only as far as it must. It plants
/// afresh, as far forward as it can, each time it comes back from an
/// action.
fn planted(support: &mut Option<Spring<f32>>, needed: f32, delta: f32) -> f32 {
    let spring = support.get_or_insert_with(|| Spring::new(0.0, SpringFeel::halflife(0.08)));
    let goal = if needed > spring.goal() + 0.01 { needed } else { spring.goal() };
    spring.update(goal, delta)
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
pub(super) mod tests_support {
    pub use super::tests::{Scene, frame, rest_of, scene_with};
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;

    use voxear_render::ModelAnimation;
    use voxear_scene::Transform;

    use super::*;
    use crate::animation::rig::tests::body;
    use crate::first_person::{self, MouseLook};
    use crate::guns::GunKind;

    pub struct Scene {
        pub world: World,
        pub player: Entity,
        pub body: Entity,
        pub camera: Entity,
        pub gun: Entity,
    }

    /// A player with a body, a view, a camera and a gun of `kind` under it.
    pub fn scene_with(kind: GunKind) -> Scene {
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
        let camera = world.spawn((Transform::default(), Parent(player), voxear_render::Camera::default()));
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

    pub fn frame(scene: &mut Scene, rest: &[Mat4]) {
        scene.world.insert(scene.body, Pose { bones: rest.to_vec().into() });
        first_person::look(&mut scene.world);
        first_person::place(&mut scene.world);
        hold(&mut scene.world);
    }

    pub fn rest_of(scene: &Scene) -> Vec<Mat4> {
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
        // An SMG's butt sits at the right shoulder, its muzzle out in front of the eye.
        let gun_now = scene.world.get::<Gun>(scene.gun).unwrap().clone();
        let butt = held.transform_point3(Vec3::from(gun_now.butt));
        let body_skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let right = Rig::find(&body_skeleton.skeleton).unwrap().arms[1].upper;
        let shoulder = world_matrix(&scene.world, scene.body)
            .transform_point3(scene.world.get::<Pose>(scene.body).unwrap().bones[right].w_axis.truncate());
        assert!(butt.distance(shoulder) < 0.15, "the butt at the shoulder: {butt} {shoulder}");
        let muzzle = (eye.inverse() * held).transform_point3(Vec3::from(gun_now.muzzle));
        assert!(muzzle.z > 0.3, "the muzzle out in front: {muzzle}");
        // Both palms are on their sockets: with no surfaces to the gun, right where they say.
        let gun = scene.world.get::<Gun>(scene.gun).unwrap().clone();
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        let bones = scene.world.get::<Pose>(scene.body).unwrap().bones.clone();
        let placed = world_matrix(&scene.world, scene.body);
        let edit = PoseEdit::new(&skeleton.skeleton, &bones);
        let palm = |arm: usize| placed.transform_point3(palm_of(&edit, &rig.arms[arm]).unwrap().point);
        let socket = |s: &Socket| held.transform_point3(Vec3::from(s.position));
        assert!(palm(1).distance(socket(&gun.grip)) < 0.005, "the right palm is on the grip");
        // The left palm is on the fore-end, slid back along it as far as the arm needs.
        let (support, grip) = (socket(&gun.support), socket(&gun.grip));
        let along = held.transform_vector3(Vec3::Z).normalize();
        let off_line = (palm(0) - support) - along * (palm(0) - support).dot(along);
        assert!(off_line.length() < 0.005, "on the fore-end's line: {}", off_line.length());
        assert!((palm(0) - grip).dot(along) > 0.05, "ahead of the grip");

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

    #[test]
    fn palms_stay_on_the_guns_surfaces_and_fingers_out_of_it_as_the_body_walks_and_turns() {
        use crate::guns::contact::{GunShapes, nearest, shapes_now};
        let mut scene = scene();
        // A rifle's wrist, stock and fore-end, each a unit cube scaled, as the Rifle the editor makes has them.
        let blocks = [
            ([0.0, -0.03, -0.11], [0.03, 0.042, 0.18]),
            ([0.0, -0.035, -0.28], [0.042, 0.13, 0.16]),
            ([0.0, -0.005, 0.32], [0.045, 0.06, 0.72]),
        ];
        let pieces = blocks.map(|(at, size)| {
            let transform = Transform::new(Vec3::from(at), Quat::IDENTITY, Vec3::from(size));
            (scene.world.spawn((transform, Parent(scene.gun))), Vec3::splat(-0.5), Vec3::splat(0.5))
        });
        scene.world.insert(scene.gun, GunShapes { pieces: pieces.to_vec() });
        let rest = rest_of(&scene);
        for _ in 0..30 {
            frame(&mut scene, &rest);
        }
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        // Each palm in the gun's space, how far it is from the gun's surfaces, and how far in the deepest finger goes.
        let measure = |scene: &Scene| {
            let bones = scene.world.get::<Pose>(scene.body).unwrap().bones.clone();
            let edit = PoseEdit::new(&skeleton.skeleton, &bones);
            let into_gun = world_matrix(&scene.world, scene.gun).inverse() * world_matrix(&scene.world, scene.body);
            let shapes = shapes_now(&scene.world, scene.gun, &[]);
            let palms = rig.arms.each_ref().map(|arm| into_gun.transform_point3(palm_of(&edit, arm).unwrap().point));
            let off = palms.map(|palm| nearest(&shapes, palm));
            let deepest =
                rig.arms.iter().flat_map(|arm| arm.fingers.iter().flatten()).fold(f32::INFINITY, |least, &bone| {
                    least.min(nearest(&shapes, into_gun.transform_point3(edit.position(bone))))
                });
            (palms, off, deepest)
        };
        let (first, off, deepest) = measure(&scene);
        assert!(off.iter().all(|&off| off < 0.003), "palms on the surfaces: {off:?}");
        assert!(deepest > 0.0, "no finger's joint inside the gun: {deepest}");
        for step in 0..120 {
            if let Some(transform) = scene.world.get_mut::<Transform>(scene.player) {
                transform.position += Vec3::new(0.0, 0.0, 0.025);
            }
            scene.world.get_mut::<FirstPersonView>(scene.player).unwrap().yaw += if step < 60 { 0.8 } else { -0.8 };
            frame(&mut scene, &rest);
            let (palms, off, deepest) = measure(&scene);
            for (hand, (palm, was)) in palms.iter().zip(&first).enumerate() {
                assert!(palm.distance(*was) < 0.003, "hand {hand} stays where it holds, step {step}: {palm} {was}");
            }
            assert!(off.iter().all(|&off| off < 0.003) && deepest > 0.0, "step {step}: {off:?} {deepest}");
        }
    }
}

#[cfg(test)]
mod firing {
    use super::tests_support::*;
    use super::*;
    use crate::guns::GunKind;

    #[test]
    fn shots_come_at_the_guns_rate_climb_the_view_and_an_smg_recovers_what_the_player_left() {
        let mut scene = scene_with(GunKind::OpenBoltSmg);
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        let step = |scene: &mut Scene, time: &mut Time| {
            time.elapsed += f64::from(time.delta);
            scene.world.insert_resource(*time);
            frame(scene, &rest);
        };
        for _ in 0..20 {
            step(&mut scene, &mut time);
        }
        let start = scene.world.get::<FirstPersonView>(scene.player).unwrap().pitch;
        // Held for half a second at 550 a minute: about 4 shots, not 30.
        let mut shots = 0;
        for _ in 0..30 {
            if let Some(shot) = fire(&mut scene.world, scene.player) {
                shots += 1;
                assert!((shot.direction.length() - 1.0).abs() < 1e-4);
            }
            step(&mut scene, &mut time);
        }
        assert!((4..=6).contains(&shots), "{shots} shots");
        let climbed = scene.world.get::<FirstPersonView>(scene.player).unwrap().pitch;
        assert!(climbed > start + 2.0, "the view climbs: {start} to {climbed}");
        for _ in 0..90 {
            step(&mut scene, &mut time);
        }
        let back = scene.world.get::<FirstPersonView>(scene.player).unwrap().pitch;
        assert!((back - start).abs() < 0.2, "and comes back down: {back} from {start}");
    }

    #[test]
    fn a_bolt_action_keeps_its_climb_and_the_players_pull_down_is_theirs() {
        let mut scene = scene_with(GunKind::BoltActionRifle);
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        for _ in 0..10 {
            time.elapsed += f64::from(time.delta);
            scene.world.insert_resource(time);
            frame(&mut scene, &rest);
        }
        let start = scene.world.get::<FirstPersonView>(scene.player).unwrap().pitch;
        assert!(fire(&mut scene.world, scene.player).is_some());
        assert!(fire(&mut scene.world, scene.player).is_none(), "not again until the bolt is worked");
        for _ in 0..60 {
            time.elapsed += f64::from(time.delta);
            scene.world.insert_resource(time);
            frame(&mut scene, &rest);
        }
        let after = scene.world.get::<FirstPersonView>(scene.player).unwrap().pitch;
        assert!(after > start + 1.5, "a bolt-action does not bring the view back: {start} {after}");
    }
}

#[cfg(test)]
mod acting {
    use voxear_scene::Transform;

    use super::tests_support::*;
    use super::*;
    use crate::guns::GunKind;
    use crate::guns::mechanism::{Chamber, GunPart, PartRole};

    fn step(scene: &mut Scene, rest: &[Mat4], time: &mut Time) {
        time.elapsed += f64::from(time.delta);
        scene.world.insert_resource(*time);
        frame(scene, rest);
    }

    /// A rifle with a turn-bolt, a stripper clip and a case under it, each
    /// a cube to look at.
    fn rifle() -> (Scene, Entity) {
        let mut scene = scene_with(GunKind::BoltActionRifle);
        let look = voxear_render::MeshRenderer::new(voxear_render::MeshHandle(0), voxear_render::MaterialHandle(0));
        scene.world.spawn((
            Transform::from_position(Vec3::new(0.03, 0.05, 0.12)),
            Parent(scene.gun),
            GunPart::of(PartRole::Case, [0.03, 0.05, 0.12]),
            look,
        ));
        let bolt = scene.world.spawn((
            Transform::from_position(Vec3::new(0.0, 0.05, 0.1)),
            Parent(scene.gun),
            GunPart::turn_bolt([0.05, 0.05, 0.1]),
        ));
        scene.world.spawn((
            Transform::from_position(Vec3::new(0.0, 0.08, 0.12)),
            Parent(scene.gun),
            GunPart::of(PartRole::Clip, [0.0, 0.09, 0.12]),
            look,
        ));
        (scene, bolt)
    }

    #[test]
    fn a_reload_keeps_the_working_hand_out_of_the_gun() {
        use crate::guns::contact::{GunShapes, Shape, shapes_now};
        use crate::guns::hands::keep_out;
        let (mut scene, bolt) = rifle();
        // The Rifle the editor makes: its wrist, stock, fore-end and receiver, and its bolt's handle out to the right.
        let blocks = [
            ([0.0, -0.03, -0.11], [0.03, 0.042, 0.18]),
            ([0.0, -0.035, -0.28], [0.042, 0.13, 0.16]),
            ([0.0, -0.005, 0.32], [0.045, 0.06, 0.72]),
            ([0.0, 0.04, 0.08], [0.034, 0.034, 0.22]),
        ];
        let mut pieces: Vec<(Entity, Vec3, Vec3)> = blocks
            .map(|(at, size)| {
                let transform = Transform::new(Vec3::from(at), Quat::IDENTITY, Vec3::from(size));
                (scene.world.spawn((transform, Parent(scene.gun))), Vec3::splat(-0.5), Vec3::splat(0.5))
            })
            .to_vec();
        pieces.push((bolt, Vec3::new(0.015, -0.0075, -0.0075), Vec3::new(0.075, 0.0075, 0.0075)));
        scene.world.insert(scene.gun, GunShapes { pieces });
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        for _ in 0..10 {
            step(&mut scene, &rest, &mut time);
        }
        if let Some(state) = scene.world.get_mut::<GunState>(scene.gun) {
            (state.rounds, state.chamber) = (0, Chamber::Empty);
        }
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::Reload);
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        let mut worst: f32 = 0.0;
        let mut worked = 0;
        for _ in 0..240 {
            step(&mut scene, &rest, &mut time);
            let state = scene.world.get::<GunHolderState>(scene.player).unwrap();
            // The right hand working, past the first moments of easing out of its hold.
            let Some(play) = state.play.as_ref().filter(|play| {
                play.beat().is_some_and(|beat| beat.hand == Hand::Right && !(play.rested && play.t < beat.reach * 0.5))
            }) else {
                continue;
            };
            let mechanism = scene.world.get::<GunState>(scene.gun).unwrap();
            let leaving = in_hand(&scene.world, mechanism, Some(play));
            let to_body = world_matrix(&scene.world, scene.body).inverse() * world_matrix(&scene.world, scene.gun);
            let avoid: Vec<Shape> =
                shapes_now(&scene.world, scene.gun, &leaving).iter().map(|shape| shape.moved(to_body)).collect();
            let edit = PoseEdit::new(&skeleton.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
            worst = worst.max(keep_out(&edit, &rig.arms[1], &avoid).length());
            worked += 1;
        }
        assert!(worked > 60, "the right hand worked the reload: {worked} frames");
        assert!(worst < 0.004, "and kept out of the gun: {worst}");
    }

    #[test]
    fn a_bolt_action_works_its_bolt_after_each_shot_and_reloads_from_a_clip_when_empty() {
        let (mut scene, bolt) = rifle();
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        for _ in 0..10 {
            step(&mut scene, &rest, &mut time);
        }
        assert!(fire(&mut scene.world, scene.player).is_some());
        // The bolt works by itself: it lifts and moves back, then all comes home.
        let mut furthest: f32 = 0.0;
        for _ in 0..70 {
            step(&mut scene, &rest, &mut time);
            let state = scene.world.get::<GunState>(scene.gun).unwrap();
            furthest = furthest.max(state.parts.iter().find(|(e, ..)| *e == bolt).unwrap().1);
        }
        assert!(furthest > 1.9, "the bolt went all the way back: {furthest}");
        let state = scene.world.get::<GunState>(scene.gun).unwrap();
        assert!(state.ready(GunKind::BoltActionRifle) && state.rounds == 4, "{state:?}");
        let named = |world: &World, name: &str| {
            world
                .query_ref::<(&voxear_scene::Name, &super::super::leftovers::Leftover)>()
                .into_iter()
                .filter(|(n, _)| n.0 == name)
                .count()
        };
        assert_eq!(named(&scene.world, "Spent Case"), 1, "the bolt threw the case out");
        assert_eq!(state.parts.iter().find(|(e, ..)| *e == bolt).unwrap().1, 0.0, "the bolt home");

        // Shot dry, then reloaded: a clip's five rounds from the spare.
        for _ in 0..5 {
            while fire(&mut scene.world, scene.player).is_none() {
                step(&mut scene, &rest, &mut time);
            }
            for _ in 0..70 {
                step(&mut scene, &rest, &mut time);
            }
        }
        let state = scene.world.get::<GunState>(scene.gun).unwrap().clone();
        assert!(!state.ready(GunKind::BoltActionRifle) && state.rounds == 0, "{state:?}");
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::Reload);
        step(&mut scene, &rest, &mut time);
        assert_eq!(scene.world.get::<GunHolderState>(scene.player).unwrap().acting(), Some(ActionKind::Reload));
        // The works come up before the eyes, which glance down at them: where the camera shows them, as a slope.
        let shown = |scene: &Scene| {
            let works = scene.world.get::<Gun>(scene.gun).unwrap().works();
            let local = world_matrix(&scene.world, scene.camera).inverse() * world_matrix(&scene.world, scene.gun);
            let works = local.transform_point3(works);
            voxear_core::math::Vec2::new(works.x, works.y) / works.z
        };
        let held = shown(&scene);
        for _ in 0..40 {
            step(&mut scene, &rest, &mut time);
        }
        let working = shown(&scene);
        assert!(working.y > held.y + 0.15 && working.length() < 0.45, "in view: {held} {working}");
        for _ in 0..200 {
            step(&mut scene, &rest, &mut time);
        }
        let state = scene.world.get::<GunState>(scene.gun).unwrap();
        assert!(
            state.ready(GunKind::BoltActionRifle) && state.rounds == 4 && state.chamber == Chamber::Live,
            "{state:?}"
        );
        assert_eq!(scene.world.get::<GunHolder>(scene.player).unwrap().spare, 85);
        assert_eq!(named(&scene.world, "Stripper Clip"), 1, "the emptied clip was flicked off");
        assert_eq!(
            named(&scene.world, "Spent Case"),
            6,
            "a case for each of the five shots, and the last ejected by the reload"
        );
        assert_eq!(scene.world.get::<GunHolderState>(scene.player).unwrap().acting(), None);
    }

    #[test]
    fn a_magazine_check_shows_the_holder_roughly_what_is_left_and_can_be_stopped() {
        let mut scene = scene_with(GunKind::OpenBoltSmg);
        scene.world.spawn((
            Transform::from_position(Vec3::new(0.0, -0.1, 0.12)),
            Parent(scene.gun),
            GunPart::of(PartRole::Magazine, [0.0, -0.15, 0.12]),
        ));
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        for _ in 0..5 {
            step(&mut scene, &rest, &mut time);
        }
        scene.world.get_mut::<GunState>(scene.gun).unwrap().known_feed = Known::Unknown;
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::CheckMagazine);
        for _ in 0..15 {
            step(&mut scene, &rest, &mut time);
        }
        assert!(fire(&mut scene.world, scene.player).is_none(), "no firing mid-check");
        scene.world.get_mut::<GunHolderState>(scene.player).unwrap().cancel();
        step(&mut scene, &rest, &mut time);
        assert_eq!(
            scene.world.get::<GunState>(scene.gun).unwrap().known_feed,
            Known::Unknown,
            "stopped before the look"
        );
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::CheckMagazine);
        for _ in 0..90 {
            step(&mut scene, &rest, &mut time);
        }
        assert_eq!(scene.world.get::<GunState>(scene.gun).unwrap().known_feed, Known::Rough);
    }
}
