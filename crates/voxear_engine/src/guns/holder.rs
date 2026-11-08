//! The Gun Holder: a character holding a gun in its own hands, in front of
//! its First-Person View's camera. Each frame the gun goes where it is
//! held, at the hip or up to the eye, with every layer of life on top as
//! springs: sway as the view turns, lag as the body moves, a bob at each
//! footfall, breathing and tremor. Near a wall it pulls back, and closer
//! still lifts. Then the hands reach the gun's grip and the other hand's
//! place on it, and close on them.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat3, Mat4, Quat, Vec3};
use voxear_core::spring::{Spring, SpringFeel};
use voxear_core::time::Time;
use voxear_ecs::{Component, Entity, World};
use voxear_physics::scene::PhysicsScene;
use voxear_render::{ModelAnimation, ModelRenderer, Pose};
use voxear_scene::{Parent, world_matrix};

use super::actions::{ActionId, ActionKind, Hand};
use super::contact::{Shape, surface};
use super::gun::{Gun, GunKind, HandRole, Socket};
use super::clip_hold::ClipPose;
use super::handling::{PackClips, Poses};
use super::hands::{Grip, close_by, finger_bends, keep_out, palm_of, seat_hand};
use super::mechanism::{GunPart, GunState, Known, PartRole};
use super::playing::{ActionPlay, HandAim, Throwing, advance, curve, minimum_jerk};
use super::preview::HoldPreview;
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
    /// Whether it sprints, for a gun held by clips that has a sprint clip.
    #[serde(skip)]
    pub sprinting: bool,
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
    /// Whether a gun held by clips is shown as its clips alone have it, with
    /// nothing on top: no sway, bob, breathing, kick of the camera or pull
    /// back from walls. For comparing with the clips as made.
    pub base_only: bool,
    /// Rounds it carries for reloading.
    pub spare: u32,
    /// Whether a bolt-action or pump is worked by itself after each shot.
    pub auto_cycle: bool,
    /// An action asked for, which starts when nothing else is under way.
    #[serde(skip)]
    pub request: Option<ActionId>,
}

impl Component for GunHolder {}

impl Default for GunHolder {
    fn default() -> Self {
        Self {
            gun: None,
            aiming: false,
            sprinting: false,
            hip: [0.16, -0.22, 0.2],
            hip_turn: [7.0, -2.0, -5.0],
            shoulder: [-0.06, -0.03, 0.06],
            aim_time: 0.22,
            sway: 1.0,
            bob: 1.0,
            breathing: 1.0,
            wall_check: true,
            base_only: false,
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
    /// How the hands hold, from frame to frame.
    hands: Hands,
    /// Where the shooting shoulder is, as the eyes see it, steadied.
    pocket: Option<Spring<Vec3>>,
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
    /// How much each holding clip holds the gun now: idle, walking,
    /// running and sprinting.
    held_by: [Spring<f32>; 4],
    /// How far into its loop each holding clip is, from its first frame
    /// when it came in.
    clocks: [f32; 4],
    /// Which holding clip leads now, and the clip starting a run or a
    /// sprint while it plays.
    leading: usize,
    starting: Option<Starting>,
    /// Where the eyes stay while clips hold the gun, and how far out they
    /// hold it, worked out once for its handling.
    seen_from: Option<SeenFrom>,
    /// Where the last action played from clips left the gun and hands, how
    /// long ago it let go of them, and over how long the hold blends back
    /// from there to the holding clips.
    /// And which hands are on their way back to their hold from a clip let
    /// go of early, left then right.
    left_off: Option<(ClipPose, f32, f32, [bool; 2])>,
    /// Where each arms clip played as an action's last part lets go, a hand
    /// snapping back onto its hold, and which hands, at the Hand Return it
    /// was found for.
    snaps: Vec<(String, f32, Option<(f32, [bool; 2])>)>,
}

impl Component for GunHolderState {}

impl GunHolderState {
    fn new() -> Self {
        Self {
            play: None,
            tilt: Spring::new(Vec3::ZERO, SpringFeel::new(2.4, 0.7, 0.0)),
            hands: Hands::default(),
            pocket: None,
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
            held_by: [Spring::new(0.0, SpringFeel::halflife(0.2)); 4],
            clocks: [0.0; 4],
            leading: 0,
            starting: None,
            seen_from: None,
            left_off: None,
            snaps: Vec::new(),
        }
    }

    /// How far raised to the eye it is, 0 at the hip to 1 aiming.
    pub fn aimed(&self) -> f32 {
        self.aim.value.clamp(0.0, 1.0)
    }

    /// The built-in action under way, or the one the gun's own action
    /// under way takes the place of.
    pub fn acting(&self) -> Option<ActionKind> {
        self.play.as_ref().and_then(|play| play.kind)
    }

    /// The action under way, as it was asked for.
    pub fn doing(&self) -> Option<&ActionId> {
        self.play.as_ref().map(|play| &play.id)
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

/// Fires the holder's gun, when it is ready to fire again, by its rate or
/// with its action worked, and not held against a wall: the view climbs by the gun's recoil, the gun
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
    // A self-loader fires as fast as its rate; a bolt or pump as soon as it has been worked, which is its rate.
    let interval = 60.0 / f64::from(gun.fire_rate.max(1.0));
    let too_soon = gun.kind.self_loading() && state.last_shot.is_some_and(|last| now - last < interval - 1e-4);
    if too_soon || state.wall.value > 0.6 || state.play.is_some() {
        return None;
    }
    // Only a gun ready to fire fires: a round chambered, or an open bolt cocked over a fed magazine.
    let mut mechanism = super::mechanism::state_of(world, gun_entity)?;
    if !mechanism.ready(gun.kind) {
        return None;
    }
    let dry = mechanism.shoot(gun.kind);
    // A preview throws nothing away, to lie about the edited level.
    let quiet = world.has::<HoldPreview>(entity);
    // A self-loader throws its case out as it works; an en-bloc clip pings out after the last round.
    if !quiet && gun.kind.self_loading() && gun.kind != GunKind::TopBreakRevolver {
        let (held, seed) = (state.held.unwrap_or(Mat4::IDENTITY), state.random());
        super::leftovers::throw_case(world, &mechanism, held, state.velocity, now, seed);
    }
    if !quiet && dry && gun.kind.feed() == super::mechanism::Feed::EnBloc {
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
        // A shot played from clips has the bolt worked once its clips are done.
        let pack = &super::handling::handling_of(world, &gun).pack;
        let fired = [&pack.fire, &pack.gun_fire].map(|clip| super::gun_clips::length(world, clip));
        state.cycle_after = Some(fired.into_iter().fold(0.16, f32::max));
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
    if !crate::modules::has(world, crate::modules::Module::Guns) {
        return;
    }
    let delta = world.resource::<Time>().map_or(1.0 / 60.0, |time| time.delta).max(1e-4);
    let holders: Vec<(Entity, GunHolder)> = world
        .query_ref::<(Entity, &GunHolder)>()
        .into_iter()
        .filter(|(entity, _)| super::preview::runs(world, *entity))
        .map(|(e, h)| (e, h.clone()))
        .collect();
    for (entity, holder) in holders {
        let Some(gun_entity) = holder.gun.or_else(|| first_under(world, entity, |w, e| w.has::<Gun>(e))) else {
            continue;
        };
        let Some(gun) = world.get::<Gun>(gun_entity).cloned() else { continue };
        let Some(view) = world.get::<FirstPersonView>(entity).cloned() else { continue };
        let preview = world.get::<HoldPreview>(entity).cloned();
        let handling = super::handling::handling_of(world, &gun);
        let poses = &handling.poses;
        // The hands hold where the handling moves them to.
        let gun = poses.held(&gun);
        // The gun is held before the view's eyes: the camera for this machine's player, their head for others.
        let Some(eye) = world.get::<crate::first_person::FirstPersonState>(entity).map(|state| state.eye) else {
            continue;
        };
        let mut state = world.get::<GunHolderState>(entity).cloned().unwrap_or_else(GunHolderState::new);
        recover(world, entity, &gun, &mut state, delta);
        for _ in 0..if holder.base_only { 0 } else { state.kicks } {
            let kick = gun.recoil * (1.0 - 0.5 * gun.heft());
            let aside = state.random() * 0.25 * kick;
            crate::first_person::punch(world, entity, Vec3::new(-0.9 * kick, aside, 0.0));
        }
        let view = world.get::<FirstPersonView>(entity).cloned().unwrap_or(view);
        let body = crate::first_person::body_of(world, entity, &view);

        // The action asked for starts, if there is one to do; a manual action is worked a moment after a shot.
        let Some(mut mechanism) = super::mechanism::state_of(world, gun_entity) else { continue };
        let mut holder = holder;
        if let Some(after) = state.cycle_after.as_mut() {
            *after -= delta;
            if *after <= 0.0 {
                state.cycle_after = None;
                holder.request = holder.request.or(Some(ActionKind::Cycle.into()));
            }
        }
        let clips = world.resource::<crate::animation::Clips>();
        if let Some(id) = holder.request.take().filter(|_| state.play.is_none())
            && let Some(play) = ActionPlay::start(&id, gun.kind, &handling, &mechanism, holder.spare, clips)
        {
            state.play = Some(ActionPlay { quiet: preview.is_some(), ..play });
            state.returning = [None; 2];
        }
        if let Some(stored) = world.get_mut::<GunHolder>(entity) {
            stored.request = None;
        }
        let pouches = pouches(world, body, state.held, poses);
        let mut spare = holder.spare;
        let mut aim = None;
        let mut tilt = Vec3::ZERO;
        // A preview shown at a time in an action is that action replayed from its start to then, every frame, with
        // every spring at rest where it is going: so the same time always looks the same.
        let mut delta = delta;
        if let Some(HoldPreview { start, at: Some((id, t)) }) = &preview {
            mechanism = start.state(gun.kind, &mechanism);
            let clips = world.resource::<crate::animation::Clips>();
            let played = ActionPlay::start(id, gun.kind, &handling, &mechanism, spare, clips);
            state.play = played.map(|play| ActionPlay { quiet: true, ..play });
            // Replayed every frame, so the rounds it loads come from spares of its own, not the holder's.
            let mut spent = spare;
            let shown = state.play.as_mut().and_then(|play| {
                scrub(world, play, (&gun, &mut mechanism, &mut spent), pouches, state.held.unwrap_or(eye), *t)
            });
            if let Some((hand, angle)) = shown {
                (aim, tilt) = (hand, angle);
            } else {
                state.play = None;
            }
            (state.returning, delta) = ([None; 2], 10.0);
            state.time = t - delta;
        } else if let Some(mut play) = state.play.take() {
            let now = world.resource::<Time>().map_or(0.0, |time| time.elapsed);
            let throwing =
                Throwing { held: state.held.unwrap_or(eye), velocity: state.velocity, now, seed: state.random() };
            match advance(world, &mut play, &gun, &mut mechanism, &mut spare, pouches, throwing, delta) {
                Some((hand, angle)) => {
                    (aim, tilt) = (hand, angle);
                    // The beats' knocks shake the gun, and the eyes glance down at what the hands are doing; a
                    // part played from clips brings the work before the eyes itself, framed as its clips are.
                    let (spin, push) = std::mem::take(&mut play.knock);
                    state.sway.kick(spin);
                    state.lag.push(push, 0.03);
                    let glances = play.part().is_none();
                    let down = state.held.filter(|_| glances).map_or(0.0, |held| glance_at(eye, held, &gun));
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
        let now = world.resource::<Time>().map_or(0.0, |time| time.elapsed);
        let since_shot = state.last_shot.map(|last| (now - last) as f32);
        super::gun_clips::pose_gun(world, gun_entity, &handling.pack, state.play.as_ref(), since_shot);
        super::mechanism::show_parts(world, gun_entity, &mechanism);
        if let Some(play) = &state.play {
            super::playing::carry_now(world, &mechanism, play);
        }
        state.tilt.update(tilt, delta);

        // A gun held by clips stands as they hold it, the upper body still under the eyes: not bladed.
        if let Some(body) = body.filter(|_| !handling.pack.holds()) {
            blade(world, body, &gun);
        }
        // A clip the action is based on moves the body, before the gun is placed by it, and gives how the gun is
        // carried and the elbows bend.
        let given = match (body, state.play.clone()) {
            (Some(body), Some(play)) => super::base_clip::play(world, body, &play, eye).unwrap_or_default(),
            _ => Default::default(),
        };
        // A gun from an animation pack is held where its clips have it, and kicks as its shot's clip does; the clips
        // are made to fit the body first, so they pose its arms as their own.
        let model = body.and_then(|body| world.get::<ModelRenderer>(body));
        let animation = model.and_then(|model| model.animation.clone());
        if let Some(animation) = &animation {
            super::clip_hold::fit_clips(world, &handling.pack, state.play.as_ref(), animation);
        }
        let clipped = held_by_clips(world, &handling.pack, &holder, &mut state, delta, animation.as_ref());
        // The upper body stands as the clips' stance has it, still under the eyes: the idle and the walk move the hips
        // and legs alone, so the arms hang from shoulders where the clips' were.
        if let Some(((pose, _), body)) = clipped.as_ref().zip(body) {
            stand_body(world, body, &pose.stance);
        }
        // While clips hold it, the eyes stay put on the body, as still as the camera the clips were filmed from:
        // where it was from the clip's shoulders, or at the eyes, worked out on the body at rest.
        if let Some(((_, weight), body)) = clipped.as_ref().zip(body)
            && let Some(at) = steady_eyes(world, entity, body, &gun.handling, &handling.pack, &view, &mut state)
        {
            // The camera's own bob with the steps, as much as the handling says, none shown as the clips alone.
            let amount = if holder.base_only { 0.0 } else { handling.pack.head_bob };
            let obj = world_matrix(world, entity).to_scale_rotation_translation().1;
            let bobbed = obj.inverse() * eye.transform_vector3(state.bob.value * amount);
            crate::first_person::frame_eyes(world, entity, at + bobbed, *weight);
        }
        let aimed = state.aimed() > 0.5;
        let kick = since_shot.and_then(|since| super::clip_hold::fire_kick(world, &handling.pack, since, aimed));
        // Tipped up or down, the gun turns with the view but swings about the shoulders, which it is held from, not
        // about the eyes above them: so the arms reach it alike however far the view tips.
        let swung = state.seen_from.as_ref().map_or(Vec3::ZERO, |seen| {
            let tipped = Quat::from_rotation_x(-view.pitch.to_radians());
            tipped.inverse() * -seen.from_middle + seen.from_middle
        });
        // Seen as filmed by a body whose arms reach less far than the clips', or further, the hold comes as much
        // nearer the eyes: as the camera is from the shoulders, so the arms reach it bent as the clips' are.
        let scaled = |gun: Mat4| state.seen_from.as_ref().map_or(gun, |seen| seen.scaled(gun));
        let clip_gun = clipped.as_ref().map(|(pose, _)| Mat4::from_translation(swung) * scaled(pose.gun));
        let rear_sight = Vec3::from(gun.rear_sight);
        let by_clips = ByClips {
            gun: clip_gun.zip(clipped.as_ref()).map(|(gun, (_, weight))| (gun, *weight)),
            kick,
            kicks: !handling.pack.fire.is_empty(),
            relief: state.seen_from.as_ref().map(|seen| seen.scaled(seen.gun).transform_point3(rear_sight).z),
            layers: [handling.pack.look_sway, handling.pack.move_sway, handling.pack.bob, handling.pack.breathing],
        };
        let mut held = place_gun(
            world,
            entity,
            (&holder, poses),
            &gun,
            &view,
            (eye, given.gun, by_clips),
            body,
            &mut state,
            delta,
        );
        set_world(world, gun_entity, held.w_axis.truncate(), held.to_scale_rotation_translation().1);
        if let Some(body) = body {
            let aims = hand_aims(&gun, aim, poses, &mut state, delta);
            let shapes = super::contact::shapes_now(world, gun_entity, &[], &[]);
            let leaving = in_hand(world, &mechanism, state.play.as_ref());
            let leaving_bones = super::mechanism::bones_of(world, gun_entity, &leaving);
            let avoid = super::contact::shapes_now(world, gun_entity, &leaving, &leaving_bones);
            let gun_body = super::contact::body_shapes(world, gun_entity);
            // A hand on its way back from a clip let go of early keeps out of the gun on its own side, most so half
            // way.
            let returning = state.left_off.as_ref().map_or([0.0; 2], |(_, since, over, hands)| {
                let way = (std::f32::consts::PI * (since / over.max(1e-3)).clamp(0.0, 1.0)).sin();
                hands.map(|hand| if hand { way } else { 0.0 })
            });
            let (posed, planted) = (world.get::<Pose>(body).cloned(), state.hands);
            let elbows = given.elbows;
            let clip = clipped.as_ref().zip(clip_gun);
            let hold = |at| Held {
                gun: &gun,
                at,
                shapes: &shapes,
                avoid: &avoid,
                body: &gun_body,
                poses,
                elbows,
                clip,
                returning,
            };
            let short = hands_on(world, body, &hold(held), eye, aims, &mut state.hands, delta);
            // Short of reach, the gun comes nearer at once, so the hands never let go of it; it stays nearer,
            // drifting back out slowly once they meet it. Held by clips it stays where they have it: the arms reach
            // for it, and a hand short of it stops short.
            let short = if clip.is_some() { 0.0 } else { short };
            if short > 0.002
                && let Some(posed) = posed
            {
                held = Mat4::from_translation(eye.transform_vector3(Vec3::NEG_Z) * short) * held;
                set_world(world, gun_entity, held.w_axis.truncate(), held.to_scale_rotation_translation().1);
                (state.hands, _) = (planted, world.insert(body, posed));
                hands_on(world, body, &hold(held), eye, aims, &mut state.hands, delta);
            }
            let wanted = (state.reach.goal() + short - 0.05 * delta).clamp(0.0, 0.45);
            state.reach.update(wanted, delta);
        }
        state.held = Some(held);
        world.insert(entity, state);
    }
}

/// Plays an action from its start to `t` seconds in, in small steps, as it
/// would have played: where its hand is then and how the gun is tilted, or
/// none once it is over.
fn scrub(
    world: &mut World,
    play: &mut ActionPlay,
    (gun, mechanism, spare): (&Gun, &mut GunState, &mut u32),
    pouches: [Vec3; 2],
    held: Mat4,
    t: f32,
) -> Option<(Option<HandAim>, Vec3)> {
    let throwing = Throwing { held, velocity: Vec3::ZERO, now: 0.0, seed: 0.0 };
    let mut left = t.max(0.0);
    loop {
        let step = left.min(1.0 / 120.0);
        let shown = advance(world, play, gun, mechanism, spare, pouches, throwing, step);
        left -= step;
        if shown.is_none() || left <= 1e-6 {
            return shown;
        }
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
    (holder, poses): (&GunHolder, &Poses),
    gun: &Gun,
    view: &FirstPersonView,
    (eye, carried, by_clips): (Mat4, Option<(Mat4, f32)>, ByClips),
    body: Option<Entity>,
    state: &mut GunHolderState,
    delta: f32,
) -> Mat4 {
    state.time += delta;
    state.aim.feel = SpringFeel::halflife(holder.aim_time.max(0.02) / 4.0);
    let heft = gun.heft();
    // Into a wall: how far along the barrel something is.
    let reach = holder.hip[2] + gun.muzzle[2] + 0.1;
    let blocked = if holder.wall_check && !holder.base_only { wall(world, entity, eye, reach) } else { 0.0 };
    let wall = state.wall.update(blocked, delta).clamp(0.0, 1.0);
    // An action takes the gun down from the eye and lifts it before them, as far as it says: working the bolt
    // keeps it at the eye and only half lifts it.
    let (lift, busy) = state.play.as_ref().map_or((0.0, false), |play| (play.lift, play.lowers_aim));
    let aim = state.aim.update(if holder.aiming && !busy { 1.0 - wall } else { 0.0 }, delta).clamp(0.0, 1.0);
    let working = state.working.update(lift * (1.0 - aim), delta).max(0.0);

    // At the hip, and up to the eye with the sights on the camera's line, blended; it dips a little on the way up.
    // The handling's poses are offsets: on the hip hold, and on the hold at the eye, about the eye, so its cant
    // turns the gun about the line of the sights.
    let add = |a: [f32; 3], b: [f32; 3]| Vec3::from(a) + Vec3::from(b);
    let [x, y, z] = add(holder.hip_turn, poses.hip.turn).to_array().map(f32::to_radians);
    let hip = Mat4::from_rotation_translation(Quat::from_euler(EulerRot::XYZ, x, y, z), add(holder.hip, poses.hip.at));
    let aimed = Mat4::from_rotation_translation(
        poses.aim.rotation() * Quat::from_rotation_z(-poses.cant.to_radians()),
        Vec3::from(poses.aim.at),
    );
    // Held by clips, it comes up no nearer than they hold it, its sights onto the camera's line from across, as a
    // pack aims by bringing its camera down onto the sights: so the hands stay as far ahead of the eye as the clips
    // have them, or further with more eye relief.
    let relief = match (by_clips.gun, by_clips.relief) {
        (Some((_, weight)), Some(relief)) => gun.eye_relief + (relief - gun.eye_relief).max(0.0) * weight,
        _ => gun.eye_relief,
    };
    let sighted = aimed * Mat4::from_translation(Vec3::Z * relief) * gun.sight_frame().inverse();
    let (_, mut hip_turn, mut hip_at) = hip.to_scale_rotation_translation();
    // A long gun's butt sits in the shoulder; a handgun is held out in front.
    // The shoulder as the eyes see it, steadied: the body's steps and landings move it far more than the eyes,
    // which are steadied too, and the gun would bob in the view with it.
    let shouldered =
        body.filter(|_| !gun.kind.is_handgun()).and_then(|body| pocket(world, body, holder, poses)).map(|pocket| {
            let seen = eye.inverse().transform_point3(pocket);
            let steadied = state.pocket.get_or_insert_with(|| Spring::new(seen, SpringFeel::halflife(0.3)));
            // Far off, as when landing or spawning, it is a move to follow at once, not a bob to steady.
            if seen.distance(steadied.value) > 0.12 {
                steadied.snap(seen);
            }
            steadied.update(seen, delta)
        });
    match shouldered {
        Some(pocket) => hip_at = pocket - hip_turn * Vec3::from(gun.butt) + Vec3::from(poses.hip.at),
        None => hip_at.z += 0.18,
    }
    // Held by clips, it is where they carry it, as much as they hold it.
    if let Some((clipped, weight)) = by_clips.gun {
        let (_, turn, at) = clipped.to_scale_rotation_translation();
        (hip_at, hip_turn) = (hip_at.lerp(at, weight), hip_turn.slerp(turn, weight));
    }
    let (_, aim_turn, aim_at) = sighted.to_scale_rotation_translation();
    let mut at = hip_at.lerp(aim_at, ease(aim)) + Vec3::Y * (-0.012 * (aim * std::f32::consts::PI).sin());
    let mut turn = hip_turn.slerp(aim_turn, ease(aim));

    // Near a wall it comes back toward the body, and closer still lifts to high ready.
    at += Vec3::new(0.0, 0.06, -0.22) * wall;
    turn = Quat::from_rotation_x(-55f32.to_radians() * smooth((wall - 0.4) / 0.6)) * turn;

    // A clip the action is based on carries the gun from here as its gun hand moves, in place of the action's own
    // lift and tilts, as much as it is followed.
    let clipped = carried.map(|(moved, weight)| (moved * Mat4::from_rotation_translation(turn, at), weight));

    // An action brings the works up before the eyes, the muzzle tipped up and aside so the butt clears the
    // chest, and tilts the gun toward the work about them.
    let tilt = state.tilt.value * (std::f32::consts::PI / 180.0);
    let worked = Quat::from_euler(EulerRot::ZYX, -tilt.x, tilt.y, -tilt.z);
    let action = state.play.as_ref().map(|play| play.work).unwrap_or_default();
    let offset = Quat::IDENTITY.slerp(poses.work.rotation() * action.rotation(), working.min(1.0));
    let shown = Quat::from_euler(EulerRot::YXZ, -0.32 * working, -0.1 * working, 0.0) * offset;
    let work_at = WORK_AT + Vec3::from(poses.work.at) + Vec3::from(action.at);
    let works = turn * shown * worked * gun.works();
    at += (work_at - works - at) * working;
    turn = turn * shown * worked;
    if let Some((clipped, weight)) = clipped {
        let (_, clip_turn, clip_at) = clipped.to_scale_rotation_translation();
        (at, turn) = (at.lerp(clip_at, weight), turn.slerp(clip_turn, weight));
    }
    at.z -= state.reach.value;
    // A shot's clip kicks the gun as it goes, in the gun's own space, in place of its own kick.
    if let Some(kicked) = by_clips.kick {
        let kicked = Mat4::from_rotation_translation(turn, at) * kicked;
        (_, turn, at) = kicked.to_scale_rotation_translation();
    }

    // Each layer on top as much as the clips holding the gun say, the gun's own up at the eye, where they do not
    // hold it; none at all shown as the clips alone have it.
    let held = by_clips.gun.map_or(0.0, |(_, weight)| weight) * (1.0 - aim);
    let layer = |amount: f32| if holder.base_only { 0.0 } else { 1.0 + (amount - 1.0) * held };
    let [looking, moving, bobbing, breathing] = by_clips.layers.map(layer);
    // Each shot kicks the gun up and back about the grip, rolling it the other way each time.
    let steady = 1.0 - 0.72 * aim;
    let kick = gun.recoil * (1.0 - 0.5 * heft);
    let kicks = std::mem::take(&mut state.kicks);
    for _ in 0..if by_clips.kicks || holder.base_only { 0 } else { kicks } {
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
        let push = (0.016 + 0.024 * heft) * holder.sway * steady * looking;
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
    let wanted = (-moved * (0.006 + 0.01 * heft) * holder.sway * steady * moving).clamp_length_max(0.05);
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
    // The steps' bob as a whole, which the camera's own bob reads too; the gun bobs as much as its layer says.
    let bob = state.bob.update(Vec3::ZERO, delta) * bobbing;
    let bob_roll = Quat::from_rotation_z(-bob.x * 2.5);

    // Breathing, slow and mostly up and down, and a faint tremor of sines that never line up.
    let t = state.time;
    let breathing = holder.breathing * breathing;
    let breath = (t * std::f32::consts::TAU * 0.24).sin() * 0.0022 * breathing * (1.0 - 0.5 * aim);
    let tremor = Vec3::new(
        (t * 1.3).sin() * 0.6 + (t * 3.1).sin() * 0.25 + (t * 7.3).sin() * 0.08,
        (t * 0.9).sin() * 0.5 + (t * 2.3).sin() * 0.3 + (t * 5.9).sin() * 0.1,
        0.0,
    ) * (0.0012 + 0.0025 * heft)
        * breathing
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
fn pouches(world: &World, body: Option<Entity>, held: Option<Mat4>, poses: &Poses) -> [Vec3; 2] {
    let fallback = [Vec3::new(-0.25, -0.35, -0.25), Vec3::new(0.2, -0.35, -0.3)];
    let (Some(body), Some(held)) = (body, held) else { return fallback };
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else {
        return fallback;
    };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body), Rig::find(&animation.skeleton)) else { return fallback };
    let into_gun = held.inverse() * world_matrix(world, body);
    let hips = pose.bones[rig.hips].w_axis.truncate();
    let sides = [Vec3::new(-0.17, -0.02, 0.12), Vec3::new(0.17, -0.02, 0.12)];
    std::array::from_fn(|hand| into_gun.transform_point3(hips + sides[hand] + Vec3::from(poses.pouches[hand])))
}

/// Each hand's aim this frame, left then right: the working hand's, and a
/// hand easing back to its hold after an action.
fn hand_aims(
    gun: &Gun,
    working: Option<HandAim>,
    poses: &Poses,
    state: &mut GunHolderState,
    delta: f32,
) -> [Option<HandAim>; 2] {
    let mut aims = [None; 2];
    for (index, hand) in [Hand::Left, Hand::Right].into_iter().enumerate() {
        if let Some(aim) = working.filter(|aim| aim.hand == hand) {
            aims[index] = Some(aim);
            continue;
        }
        if let Some((from, t)) = state.returning[index].as_mut() {
            *t += delta * poses.return_pace.max(0.05) / 0.28;
            // Back to its hold, coming in to the grip from the side and to a fore-end from below, settling into it.
            let home = super::playing::socket(gun, hand);
            let (grip, towards) = if hand == Hand::Right {
                (rest_grip(poses, hand), Vec3::X)
            } else {
                (rest_grip(poses, hand), Vec3::NEG_Y)
            };
            let s = minimum_jerk(*t);
            let at = curve([*from, *from, home + towards * 0.06, home], s);
            aims[index] = Some(HandAim { hand, at, grip, settle: s, toward: towards, turn: Quat::IDENTITY });
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
fn pocket(world: &World, body: Entity, holder: &GunHolder, poses: &Poses) -> Option<Vec3> {
    let animation = world.get::<ModelRenderer>(body)?.animation.clone()?;
    let rig = Rig::find(&animation.skeleton)?;
    let pose = world.get::<Pose>(body)?;
    let shoulder = pose.bones.get(rig.arms[1].upper)?.w_axis.truncate();
    let offset = Vec3::from(holder.shoulder) + Vec3::from(poses.shoulder);
    Some(world_matrix(world, body).transform_point3(shoulder + offset))
}

/// The grip a hand holds the gun with at rest: as the handling has it, or
/// round the grip and under the fore-end as usual.
fn rest_grip(poses: &Poses, hand: Hand) -> Grip {
    match hand {
        Hand::Right => poses.rest_grip.unwrap_or(Grip::TRIGGER),
        Hand::Left => poses.rest_support.unwrap_or(Grip::SUPPORT),
    }
}

/// What a body's hands hold this frame: the gun, where it is held, and its
/// surfaces, in its own space: all of them, which fingers close on, and
/// those but the parts a hand has, which hands keep out of.
struct Held<'a> {
    gun: &'a Gun,
    at: Mat4,
    shapes: &'a [Shape],
    avoid: &'a [Shape],
    poses: &'a Poses,
    /// Which way a clip the action is based on bends each elbow, left then
    /// right, in the body's space, and how much they follow it.
    elbows: Option<([Vec3; 2], f32)>,
    /// Where clips holding the gun have the hands and pose the arms, and how
    /// much they hold it; and where they put the gun, in the eyes' space.
    clip: Option<(&'a (ClipPose, f32), Mat4)>,
    /// The gun's body, its moving parts left out (see
    /// `contact::body_shapes`), which fingers the clips pose and a hand on
    /// its way back keep out of.
    body: &'a [Shape],
    /// How much each hand on its way back to its hold, from a clip let go
    /// of early, keeps out of the gun's body, left then right.
    returning: [f32; 2],
}

/// Stands a body's upper body as a clip's stance has it, for this frame.
fn stand_body(world: &mut World, body: Entity, stance: &[Quat; 3]) {
    let Some(animation) = world.get::<ModelRenderer>(body).and_then(|model| model.animation.clone()) else { return };
    let (Some(pose), Some(rig)) = (world.get::<Pose>(body).cloned(), Rig::find(&animation.skeleton)) else { return };
    let skeleton = &animation.skeleton;
    let rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
    let mut edit = PoseEdit::new(skeleton, &pose.bones);
    super::clip_hold::stand(&mut edit, &rig, &rest, stance);
    world.insert(body, Pose { bones: edit.matrices().into() });
}

/// Where the eyes stay relative to the holder's Obj while clips hold the
/// gun, turned with where the view looks: for a handling seen as filmed,
/// as far from the middle of the body's shoulders as the clips' camera was
/// from theirs, scaled to the body's arms; otherwise at the eyes. Both are
/// worked out on the body at rest, once for the handling `handling`, so the
/// body's walk and run do not carry the camera.
fn steady_eyes(
    world: &World,
    entity: Entity,
    body: Entity,
    handling: &str,
    pack: &PackClips,
    view: &FirstPersonView,
    state: &mut GunHolderState,
) -> Option<Vec3> {
    if state.seen_from.as_ref().is_none_or(|seen| seen.handling != handling) {
        let animation = world.get::<ModelRenderer>(body)?.animation.clone()?;
        let skeleton = &animation.skeleton;
        let rig = Rig::find(skeleton)?;
        let rest = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
        // The clips' eyes, from the first clip that holds the gun as it stands, and the body at rest standing as it does.
        let clip = [&pack.idle, &pack.walk, &pack.run, &pack.sprint].into_iter().find(|clip| !clip.is_empty())?;
        let carrier = (pack.carrier.as_str(), &pack.carrier_offset);
        let filmed = super::clip_hold::clip_pose(world, clip, 0.0, true, carrier, Vec3::from(pack.eyes), None)?;
        let mut standing = PoseEdit::new(skeleton, &skeleton.model_matrices(&skeleton.rest()));
        super::clip_hold::stand(&mut standing, &rig, &rest, &filmed.stance);
        let middle = (standing.position(rig.arms[0].upper) + standing.position(rig.arms[1].upper)) * 0.5;
        let (from_middle, scale) = match pack.camera {
            super::handling::PackCamera::Filmed => {
                // Their space's origin, so back from their shoulders the way the shoulders are from it.
                let reach = |arm: usize| super::hands::arm_reach(&rest, &rig.arms[arm]);
                let scale = (reach(0)? + reach(1)?) / (filmed.hands[0].reach + filmed.hands[1].reach).max(1e-3);
                (-(filmed.shoulders[0] + filmed.shoulders[1]) * 0.5 * scale, scale)
            }
            super::handling::PackCamera::Eyes => (rest.position(rig.head) + Vec3::from(view.eyes) - middle, 1.0),
        };
        let handling = handling.to_string();
        state.seen_from = Some(SeenFrom { handling, middle, from_middle, gun: filmed.gun, scale });
    }
    let seen = state.seen_from.as_ref()?;
    let (obj, placed) = (world_matrix(world, entity), world_matrix(world, body));
    let eyes = placed.transform_point3(seen.middle) + Quat::from_rotation_y(view.yaw.to_radians()) * seen.from_middle;
    Some(obj.inverse().transform_point3(eyes))
}

/// Where the eyes stay while clips hold the gun, and how far out the clips
/// hold it, worked out on the body at rest once for a handling.
#[derive(Clone, Debug, PartialEq)]
struct SeenFrom {
    /// The handling's path.
    handling: String,
    /// The middle of the body's shoulders at rest, in its model's space.
    middle: Vec3,
    /// The eyes from there, level.
    from_middle: Vec3,
    /// The gun as the clips hold it standing, in their camera's space.
    gun: Mat4,
    /// How far the body's arms reach for each metre the clips' do, seen as
    /// filmed: the clips' hold comes as much nearer the eyes, or further,
    /// so the arms bend as the clips' do; 1 at the eyes.
    scale: f32,
}

impl SeenFrom {
    /// `gun`, a place in the clips' camera's space, brought as much nearer
    /// the eyes as the body's arms are shorter than the clips'.
    fn scaled(&self, gun: Mat4) -> Mat4 {
        let (_, turn, at) = gun.to_scale_rotation_translation();
        Mat4::from_rotation_translation(turn, at * self.scale)
    }
}

/// Where the arms clip `arms` lets go as an action's last part, a hand
/// snapping back onto its hold (see `clip_hold::snaps_back_at`): found once
/// for each clip and Hand Return, once the clip is read.
fn snap_of(state: &mut GunHolderState, world: &World, pack: &PackClips, arms: &str) -> Option<(f32, [bool; 2])> {
    let least = pack.hand_return;
    if let Some((_, _, at)) = state.snaps.iter().find(|(known, was, _)| known == arms && *was == least) {
        return *at;
    }
    let carrier = (pack.carrier.as_str(), &pack.carrier_offset);
    let at = super::clip_hold::snaps_back_at(world, arms, carrier, Vec3::from(pack.eyes), least)?;
    state.snaps.push((arms.to_string(), least, at));
    at
}

/// A clip starting a run or a sprint, playing over the holding clips: which
/// of them it starts, how far into it it is, and once it is over or left,
/// how long it has been going out and over how long.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Starting {
    held: usize,
    at: f32,
    out: Option<(f32, f32)>,
}

/// What clips holding the gun give its place: where they carry it in the
/// eyes' space, and how much; how a shot's clip moves it now, in its own
/// space; whether a shot's clip kicks it in place of its own kick; and how
/// far ahead of the eye they hold its rear sight standing, the nearest it
/// comes aimed.
struct ByClips {
    gun: Option<(Mat4, f32)>,
    kick: Option<Mat4>,
    kicks: bool,
    relief: Option<f32>,
    /// How much the gun sways as the view turns, lags as the holder moves,
    /// bobs with the steps and breathes while clips hold it (see `PackClips`).
    layers: [f32; 4],
}

/// Where a gun from an animation pack is held now, and how much its clips
/// hold it: by the holding clips, weighted by how fast the holder goes and
/// whether it sprints, eased from one to another; and over them, as an
/// action played from clips comes in and goes, by its part's arms clip.
fn held_by_clips(
    world: &World,
    pack: &PackClips,
    holder: &GunHolder,
    state: &mut GunHolderState,
    delta: f32,
    body: Option<&Arc<ModelAnimation>>,
) -> Option<(ClipPose, f32)> {
    let eyes = Vec3::from(pack.eyes);
    let speed = Vec3::new(state.velocity.x, 0.0, state.velocity.z).length();
    let wanted = super::clip_hold::holding_weights(pack, speed, holder.sprinting);
    // The holding clips ease from one to another on springs, setting off and settling smoothly; held by none yet,
    // they hold as wanted at once.
    let held = state.held_by.iter().map(|weight| weight.value).sum::<f32>() > 1e-3;
    for (shown, wanted) in state.held_by.iter_mut().zip(wanted) {
        shown.feel = SpringFeel::halflife(pack.blend.max(0.01));
        match held {
            true => _ = shown.update(wanted, delta),
            false => shown.snap(wanted),
        }
    }
    let weights = state.held_by.map(|weight| weight.value.clamp(0.0, 1.0));
    // Setting off into a run or a sprint from standing or walking, its start clip plays over the holding clips where
    // the pack has one, and its loop starts from the first frame once the start is over, where the start leaves off.
    let leading = (0..4).max_by(|&a, &b| wanted[a].total_cmp(&wanted[b])).unwrap_or(0);
    if leading != state.leading {
        let starts = leading >= 2 && state.leading < 2 && !super::clip_hold::start_of(pack, leading).is_empty();
        match state.starting.as_mut() {
            _ if starts => state.starting = Some(Starting { held: leading, at: 0.0, out: None }),
            // Left before it is over, it goes out as slowly as any clip.
            Some(start) if start.out.is_none() => start.out = Some((0.0, pack.blend)),
            _ => {}
        }
        state.leading = leading;
    }
    let carrier = (pack.carrier.as_str(), &pack.carrier_offset);
    let start = state.starting.as_mut().and_then(|start| {
        let reference = super::clip_hold::start_of(pack, start.held);
        let length = super::gun_clips::length(world, reference);
        match start.out.as_mut() {
            None => {
                start.at += delta;
                // Over, it gives way at once to its loop's first frame, the pose it ends in.
                if start.at >= length {
                    start.out = Some((0.0, pack.blend * 0.25));
                }
            }
            Some((since, _)) => *since += delta,
        }
        let weight = match start.out {
            None => super::clip_hold::comes_in(start.at, pack.blend * 0.5),
            Some((since, over)) => super::clip_hold::goes_out(since, over),
        };
        let pose = super::clip_hold::clip_pose(world, reference, start.at.min(length), false, carrier, eyes, body)?;
        Some((pose, weight))
    });
    if state.starting.is_some_and(|start| start.out.is_some_and(|(since, over)| since >= over)) {
        state.starting = None;
    }
    // Each holding clip plays from its first frame as it comes in, a loop being started held there until its start
    // is over.
    for (held, clock) in state.clocks.iter_mut().enumerate() {
        let waiting = state.starting.is_some_and(|start| start.held == held && start.out.is_none());
        *clock = if weights[held] < 0.01 || waiting { 0.0 } else { *clock + delta };
    }
    let holding = super::clip_hold::holding_pose(world, pack, weights, state.clocks, body);
    let holding = match (holding, start) {
        (Some(holding), Some((start, weight))) => Some(holding.lerp(&start, weight)),
        (holding, _) => holding,
    };
    let now = state.play.as_ref().and_then(|play| {
        let (part, at) = play.part().filter(|(part, _)| !part.arms.is_empty())?;
        Some((part.arms.clone(), at, play.on_last_beat(), play.progress().0))
    });
    // A pack's clips end as they began, held as the holding clips hold the gun: each plays to its very end, and only
    // once it is over does the hold blend back from where it left the gun and hands, so its last moves keep their pace.
    // But an action's last clip that snaps a hand back onto its hold at its very end lets go as the hand sets off,
    // and the hand eases back over the Hand Return instead.
    let part = now.and_then(|(arms, at, last, elapsed)| {
        let snaps = if last { snap_of(state, world, pack, &arms) } else { None };
        let (from, since, over, hands) = match snaps {
            Some((from, hands)) if at >= from => (from, at - from, pack.hand_return.max(pack.blend), hands),
            _ => (at, 0.0, pack.blend, [false; 2]),
        };
        let pose = super::clip_hold::clip_pose(world, &arms, from, false, carrier, eyes, body)?;
        let weight = super::clip_hold::comes_in(elapsed, pack.blend) * super::clip_hold::goes_out(since, over);
        Some((pose, since, over, hands, weight))
    });
    let part = match part {
        Some((pose, since, over, hands, weight)) => {
            state.left_off = Some((pose.clone(), since, over, hands));
            Some((pose, weight))
        }
        None => state.left_off.as_mut().and_then(|(pose, since, over, _)| {
            *since += delta;
            let weight = super::clip_hold::goes_out(*since, *over);
            (weight > 0.0).then(|| (pose.clone(), weight))
        }),
    };
    if part.is_none() {
        state.left_off = None;
    }
    match (holding, part) {
        (Some(holding), Some((part, fade))) => Some((holding.lerp(&part, fade), 1.0)),
        (Some(holding), None) => Some((holding, 1.0)),
        (None, Some((part, fade))) => Some((part, fade)),
        (None, None) => None,
    }
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
    hands: &mut Hands,
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
    let gun_body: Vec<Shape> = held.body.iter().map(|shape| shape.moved(to_body)).collect();
    // A hand working keeps out of the gun; so does one the clips let go of on its way back to its hold, round the
    // gun's body rather than through it.
    let keeping = |hand: usize| match held.returning[hand] > 0.0 {
        true => (&gun_body, held.returning[hand]),
        false => (&avoid, working(aims[hand])),
    };
    let placing = Placing { gun: held.gun, to_body, shapes: held.shapes };
    // Clips holding the gun have each hand where they say, but a hand working an action of beats.
    let clipped = |hand: usize| {
        let held = held.clip.filter(|_| aims[hand].is_none());
        held.map(|((pose, weight), _)| (pose.hands[hand], *weight))
    };
    // Each hand comes into a new hold or beat rather than jumping to it, in the gun's space; a hand the clips
    // hold follows them as they have it, however fast they move it, as they ease from one to another themselves.
    let to_gun = to_body.inverse();
    let mut eased = |hand: usize, (contact, (fingers, palm)): (Vec3, (Vec3, Vec3))| {
        let turn = hand_turn(to_gun.transform_vector3(fingers), to_gun.transform_vector3(palm));
        let at = to_gun.transform_point3(contact);
        let (at, turn) = match clipped(hand) {
            Some((_, weight)) if weight > 0.999 => follow(&mut hands.placed[hand], at, turn, delta),
            _ => come_into(&mut hands.placed[hand], at, turn, delta),
        };
        let way = |v: Vec3| to_body.transform_vector3(turn * v).normalize_or_zero();
        (to_body.transform_point3(at), (way(Vec3::X), way(Vec3::Y)))
    };
    // A hand built otherwise than the clip's holds with its knuckles where the clip's are, so its fingers close
    // on what the clip's close on: the middle of its palm goes as far from there as it is on this hand.
    let palm_to_knuckles = [0, 1].map(|hand| super::hands::knuckles_from_palm(&edit, &rig.arms[hand]));
    // A hand the clips pose keeps the turn their pose gives it, carried as the gun has moved from where they hold it:
    // the frame its knuckles make tells a hand built otherwise turned otherwise, its thumb over the top.
    let by_clip = |hand: usize, (contact, (fingers, palm)): (Vec3, (Vec3, Vec3)), posed: Option<(Vec3, Vec3)>| {
        let Some((by, weight)) = clipped(hand) else { return (contact, (fingers, palm)) };
        let turned = (to_body.transform_vector3(by.fingers), to_body.transform_vector3(by.palm));
        let (clip_fingers, clip_palm) = posed.unwrap_or(turned);
        let held_at = match palm_to_knuckles[hand] {
            Some(offset) => {
                to_body.transform_point3(by.knuckles) - super::hands::frame_of(clip_fingers, clip_palm) * offset
            }
            None => to_body.transform_point3(by.contact),
        };
        let way = |own: Vec3, wanted: Vec3| own.lerp(wanted, weight).normalize_or(own);
        (contact.lerp(held_at, weight), (way(fingers, clip_fingers), way(palm, clip_palm)))
    };
    // A hand the clips hold has its arm posed as theirs first, bone by bone out to the fingertips, turned with the
    // eyes, so it bends and twists as theirs does; it then reaches only the rest of the way onto the gun, as far as a
    // body built otherwise needs, its elbow staying where the clip's pose has it.
    let eyes = (into_body * eye).to_scale_rotation_translation().1;
    let turn_of = |m: Mat4| m.to_scale_rotation_translation().1;
    let posed_by_clip = |edit: &mut PoseEdit, hand: usize| {
        let ((pose, weight), clip_gun) = held.clip.filter(|_| aims[hand].is_none())?;
        let arms = pose.arms.as_ref()?;
        let arm = &rig.arms[hand];
        super::clip_hold::pose_arm(edit, &animation.skeleton, arm, &arms[hand], eyes, *weight);
        let moved = turn_of(to_body) * turn_of(into_body * eye * clip_gun).inverse();
        let (fingers, palm) = crate::animation::ik::hand_axes(edit, arm)?;
        Some((edit.position(arm.forearm), (moved * fingers, moved * palm)))
    };
    // Fingers the clip posed are kept as it has them, and how far they bend from straight is kept, so a hand going
    // on to a beat of its own closes from there.
    let at_rest = PoseEdit::new(&animation.skeleton, &animation.skeleton.model_matrices(&animation.skeleton.rest()));
    let kept = |edit: &PoseEdit, arm: &Arm| {
        let (now, rest) = (finger_bends(edit, arm), finger_bends(&at_rest, arm));
        Some(std::array::from_fn(|f| std::array::from_fn(|j| now[f][j] - rest[f][j])))
    };

    let shoulder = edit.position(right.upper);
    let posed = posed_by_clip(&mut edit, 1);
    let grip_at = placing.aimed(aims[1], &held.gun.grip, HandRole::Grip, 0.0);
    let (contact, axes) = eased(1, by_clip(1, grip_at, posed.map(|(_, axes)| axes)));
    let own = || bent(held, 1, shoulder, shoulder + down * 0.6 + out * 0.5 + back * 0.2);
    let pole = posed.map_or_else(own, |(elbow, _)| elbow);
    let side = to_body.transform_vector3(Vec3::X).normalize_or_zero();
    let (keep, push) = keeping(1);
    let short = seat_clear(&mut edit, right, (contact, axes), (pole, 0.15), (keep, side), push);
    let grip = aims[1].map_or(rest_grip(held.poses, Hand::Right), |aim| aim.grip);
    match posed {
        Some(_) => {
            super::hands::keep_fingers_out(&mut edit, right, &gun_body);
            hands.fingers[1] = kept(&edit, right);
        }
        None => close_by(&mut edit, right, super::hands::bends_of(grip), &shapes, &mut hands.fingers[1], delta),
    }

    let shoulder = edit.position(left.upper);
    let posed = posed_by_clip(&mut edit, 0);
    let own = || bent(held, 0, shoulder, shoulder + down * 0.7 - out * 0.4 + back * 0.1);
    let pole = posed.map_or_else(own, |(elbow, _)| elbow);
    let role = if held.gun.kind.is_handgun() { HandRole::HandgunSupport } else { HandRole::Support };
    let slide = match aims[0] {
        // A hand working lets go of where it was planted, and plants afresh once it is back.
        Some(_) => {
            hands.support = None;
            0.0
        }
        None if clipped(0).is_some() => 0.0,
        None => planted(&mut hands.support, placing.slide_needed(&edit, left, role, shoulder), delta),
    };
    // Where clips hold the gun, the support hand stays where they have it: the gun comes within its reach.
    let target = by_clip(0, placing.aimed(aims[0], &held.gun.support, role, slide), posed.map(|(_, axes)| axes));
    let (contact, axes) = eased(0, target);
    let (keep, push) = keeping(0);
    seat_clear(&mut edit, left, (contact, axes), (pole, 0.2), (keep, -side), push);
    let grip = aims[0].map_or(rest_grip(held.poses, Hand::Left), |aim| aim.grip);
    match posed {
        Some(_) => {
            super::hands::keep_fingers_out(&mut edit, left, &gun_body);
            hands.fingers[0] = kept(&edit, left);
        }
        None => close_by(&mut edit, left, super::hands::bends_of(grip), &shapes, &mut hands.fingers[0], delta),
    }
    world.insert(body, Pose { bones: edit.matrices().into() });
    if aims[1].is_some() { 0.0 } else { short }
}

/// Where an arm's elbow bends toward: its own `pole`, or as far toward the
/// way a clip the action is based on bends it as it is followed.
fn bent(held: &Held, arm: usize, shoulder: Vec3, pole: Vec3) -> Vec3 {
    let Some((ways, weight)) = held.elbows else { return pole };
    let clipped = shoulder + ways[arm] * (pole - shoulder).length();
    pole.lerp(clipped, weight)
}

/// The parts a hand has now: the one the beat under way drives, and the
/// one it carries.
fn in_hand(world: &World, mechanism: &GunState, play: Option<&ActionPlay>) -> Vec<Entity> {
    let Some(beat) = play.and_then(ActionPlay::beat) else { return Vec::new() };
    let roles = [beat.carry, beat.drive.map(|drive| drive.part)];
    let has = |entity: Entity| world.get::<GunPart>(entity).is_some_and(|part| roles.contains(&Some(part.role)));
    mechanism.parts.iter().map(|&(entity, ..)| entity).filter(|&entity| has(entity)).collect()
}

/// How the hands hold from frame to frame: where the support hand is
/// planted, and each hand's place and turn on the gun and its fingers'
/// bends, which come into new ones rather than jump to them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Hands {
    support: Option<Spring<f32>>,
    placed: [Option<Eased>; 2],
    fingers: [Option<[[f32; 3]; 5]>; 2],
}

/// A hand's place and turn on the gun as last shown: where it was to be,
/// how fast that was moving, and how far off it is shown, easing away.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Eased {
    at: Vec3,
    velocity: Vec3,
    turn: Quat,
    offset: Vec3,
    turned: Quat,
}

/// How long a jump in a hand's hold takes to ease half away, in seconds.
const EASE: f32 = 0.06;

/// Where a hand is shown, given where it is to be: exactly there while
/// that moves smoothly, as a hand riding a bolt or resting on its hold;
/// but where it jumps, to a new beat or back to rest, the hand carries on
/// from where it was and eases into it.
fn come_into(eased: &mut Option<Eased>, at: Vec3, turn: Quat, delta: f32) -> (Vec3, Quat) {
    let Some(last) = eased.as_mut() else {
        *eased = Some(Eased { at, velocity: Vec3::ZERO, turn, offset: Vec3::ZERO, turned: Quat::IDENTITY });
        return (at, turn);
    };
    let jump = at - (last.at + last.velocity * delta);
    if jump.length() > 0.01 {
        last.offset -= jump;
    } else {
        last.velocity = (at - last.at) / delta.max(1e-4);
    }
    if last.turn.angle_between(turn) > 6f32.to_radians() {
        last.turned = last.turned * last.turn * turn.inverse();
    }
    let fade = 0.5f32.powf(delta / EASE);
    (last.at, last.turn, last.offset, last.turned) =
        (at, turn, last.offset * fade, last.turned.slerp(Quat::IDENTITY, 1.0 - fade));
    (at + last.offset, (last.turned * turn).normalize())
}

/// Where a hand is shown when it goes where it is to be, kept so a later
/// jump eases from there.
fn follow(eased: &mut Option<Eased>, at: Vec3, turn: Quat, delta: f32) -> (Vec3, Quat) {
    let velocity = eased.map_or(Vec3::ZERO, |last| (at - last.at) / delta.max(1e-4));
    *eased = Some(Eased { at, velocity, turn, offset: Vec3::ZERO, turned: Quat::IDENTITY });
    (at, turn)
}

/// The turn that points X along `fingers` and Y along `palm`.
fn hand_turn(fingers: Vec3, palm: Vec3) -> Quat {
    let fingers = fingers.normalize_or(Vec3::Z);
    let palm = (palm - fingers * palm.dot(fingers)).normalize_or(Vec3::Y);
    Quat::from_mat3(&Mat3::from_cols(fingers, palm, fingers.cross(palm)))
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
    (shapes, side): (&[Shape], Vec3),
    working: f32,
) -> f32 {
    let before = edit.local.clone();
    let mut contact = contact;
    let mut short = seat_hand(edit, arm, contact, axes, pole, clavicle);
    // A little past each push, so out of one box against another it gets clear of both in a few goes.
    for _ in 0..5 {
        let push = keep_out(edit, arm, shapes, side) * working * 1.1;
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
        let (fingers, palm, contact) = if let Some(toward) = aim.toward.try_normalize() {
            let palm = -toward;
            let fingers = palm.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::Z);
            (fingers, palm, aim.at + toward * 0.02 - fingers * 0.05)
        } else {
            let bore = Vec3::new(0.0, 0.03, aim.at.z);
            let side = if aim.hand == Hand::Right { Vec3::NEG_X } else { Vec3::X };
            let palm = (bore - aim.at).try_normalize().unwrap_or(side);
            let fingers = (Vec3::Z - palm * palm.z).normalize_or(Vec3::Z);
            (fingers, palm, aim.at - fingers * 0.06 - palm * 0.015)
        };
        // Turned as the beat says, about the point it holds.
        let (fingers, palm) = (aim.turn * fingers, aim.turn * palm);
        let contact = aim.at + aim.turn * (contact - aim.at);
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
/// as the body moves, goes back only as far as it must, and forward again
/// only once it could hold well forward of where it is, as after a stumble
/// that pulled it back.
fn planted(support: &mut Option<Spring<f32>>, needed: f32, delta: f32) -> f32 {
    let spring = support.get_or_insert_with(|| Spring::new(0.0, SpringFeel::halflife(0.08)));
    let held = spring.goal();
    let goal = if needed > held + 0.01 {
        needed
    } else if needed < held - 0.06 {
        needed + 0.02
    } else {
        held
    };
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
        scene.world.insert(scene.gun, GunShapes { pieces: pieces.to_vec(), ..GunShapes::default() });
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
            let shapes = shapes_now(&scene.world, scene.gun, &[], &[]);
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

    #[test]
    fn a_handlings_poses_move_the_hold_by_exactly_their_offsets() {
        use crate::guns::handling::{GunHandling, Place, put_handling};
        let mut scene = scene();
        let rest = rest_of(&scene);
        let seen =
            |scene: &Scene| world_matrix(&scene.world, scene.camera).inverse() * world_matrix(&scene.world, scene.gun);
        // Still, not breathing, so where it is held is all that moves it.
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().breathing = 0.0;
        for _ in 0..90 {
            frame(&mut scene, &rest);
        }
        let usual = seen(&scene);
        let mut handling = GunHandling::default();
        handling.poses.hip = Place { at: [0.02, -0.03, 0.01], turn: [0.0; 3] };
        handling.poses.cant = 12.0;
        put_handling(&mut scene.world, "Tuned", std::sync::Arc::new(handling));
        scene.world.get_mut::<Gun>(scene.gun).unwrap().handling = "Tuned".into();
        for _ in 0..90 {
            frame(&mut scene, &rest);
        }
        let moved = seen(&scene).w_axis.truncate() - usual.w_axis.truncate();
        assert!((moved - Vec3::new(0.02, -0.03, 0.01)).length() < 0.002, "the hip hold moves by its offset: {moved}");
        // Raised to the eye, it cants its top to the right about the sights' line, which stays on the eye's.
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().aiming = true;
        for _ in 0..90 {
            frame(&mut scene, &rest);
        }
        let gun = scene.world.get::<Gun>(scene.gun).unwrap().clone();
        let sights = seen(&scene) * gun.sight_frame();
        let up = sights.transform_vector3(Vec3::Y).normalize();
        assert!((up.x.atan2(up.y).to_degrees() - 12.0).abs() < 0.5, "canted twelve degrees: {up}");
        let rear = sights.w_axis.truncate();
        assert!(rear.truncate().length() < 0.01, "the sights still on the eye's line: {rear}");
    }

    #[test]
    fn while_editing_only_a_previewed_holder_is_held_and_it_shows_a_time_in_an_action_the_same_each_time() {
        use crate::guns::preview::{HoldPreview, PreviewStart};
        let mut scene = scene();
        let rest = rest_of(&scene);
        scene.world.insert_resource(crate::EditMode(true));
        let was = *scene.world.get::<voxear_scene::Transform>(scene.gun).unwrap();
        for _ in 0..5 {
            frame(&mut scene, &rest);
        }
        assert!(!scene.world.has::<GunHolderState>(scene.player), "a level's own holder is left alone while editing");
        assert_eq!(*scene.world.get::<voxear_scene::Transform>(scene.gun).unwrap(), was);

        let shown_at = |scene: &mut Scene, t: f32| {
            let at = Some((ActionId::Builtin(ActionKind::Reload), t));
            scene.world.insert(scene.player, HoldPreview { start: PreviewStart::Dry, at });
            frame(scene, &rest);
            let gun = world_matrix(&scene.world, scene.gun);
            let parts = scene.world.get::<GunState>(scene.gun).unwrap().clone();
            (gun, parts.rounds, scene.world.get::<GunHolderState>(scene.player).unwrap().acting())
        };
        let (first, rounds, acting) = shown_at(&mut scene, 0.6);
        assert_eq!((rounds, acting), (0, Some(ActionKind::Reload)), "partway through reloading from dry");
        shown_at(&mut scene, 1.4);
        let (again, ..) = shown_at(&mut scene, 0.6);
        assert!(again.abs_diff_eq(first, 1e-4), "the same time looks the same: {first} {again}");
        let (_, rounds, acting) = shown_at(&mut scene, 20.0);
        assert_eq!((rounds, acting), (4, None), "past its end it shows the gun reloaded, the hands back on it");
        let leftovers = scene.world.query_ref::<&super::super::leftovers::Leftover>().into_iter().count();
        assert_eq!(leftovers, 0, "a preview throws nothing away");
        let spare = scene.world.get::<GunHolder>(scene.player).unwrap().spare;
        assert_eq!(spare, GunHolder::default().spare, "nor spends the holder's rounds, however often it is shown");
    }

    #[test]
    fn a_clip_an_action_is_based_on_carries_the_gun_and_bends_the_body_on_top_of_the_action() {
        use crate::animation::{ClipSource, Clips};
        use crate::guns::base_clip::BaseClip;
        use crate::guns::handling::{ActionOffsets, GunHandling, put_handling};
        use crate::guns::preview::{HoldPreview, PreviewStart};
        use voxear_render::{BonePose, Clip};
        let mut scene = scene();
        // The body's own file has two clips a second long: standing still, and lifting the gun hand 10 cm and
        // bending the chest 20 degrees forward halfway through.
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap().skeleton.clone();
        let rig = Rig::find(&skeleton).unwrap();
        let (frames, rest) = (61, skeleton.rest());
        let still: Vec<BonePose> = (0..frames).flat_map(|_| rest.clone()).collect();
        let lift = (0..frames).flat_map(|frame| {
            let up = (std::f32::consts::PI * frame as f32 / 60.0).sin();
            let mut pose = rest.clone();
            pose[rig.arms[1].hand].translation.y += 0.1 * up;
            pose[rig.chest()].rotation = Quat::from_rotation_x(20f32.to_radians() * up) * pose[rig.chest()].rotation;
            pose
        });
        let clips = vec![
            Clip { name: "Still".into(), frames, poses: still },
            Clip { name: "Lift".into(), frames, poses: lift.collect() },
        ];
        let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips });
        scene.world.get_mut::<ModelRenderer>(scene.body).unwrap().animation = Some(animation.clone());
        let mut found = Clips::default();
        for (index, name) in ["Still", "Lift"].into_iter().enumerate() {
            let source = ClipSource { animation: animation.clone(), index, settings: Default::default() };
            found.put(&format!("Body#clip:{name}"), source);
        }
        scene.world.insert_resource(found);
        scene.world.insert_resource(crate::EditMode(true));
        let posed = rest_of(&scene);

        // Halfway through cycling the bolt, the gun about the eye and the chest as posed.
        let kind = GunKind::BoltActionRifle;
        let halfway = |scene: &mut Scene, base: Option<(&str, [f32; 3])>| {
            let base = base.map(|(clip, [body, gun, arms])| BaseClip {
                clip: format!("Body#clip:{clip}"),
                body,
                gun,
                arms,
                ..BaseClip::default()
            });
            let mut handling = GunHandling::default();
            handling.offsets.push(ActionOffsets { action: ActionKind::Cycle, clip: base, ..ActionOffsets::default() });
            let start = PreviewStart::Full.state(kind, &GunState::loaded(kind));
            let id = ActionId::Builtin(ActionKind::Cycle);
            let length = ActionPlay::start(&id, kind, &handling, &start, 10, None).unwrap().progress().1;
            put_handling(&mut scene.world, "Based", Arc::new(handling));
            scene.world.get_mut::<Gun>(scene.gun).unwrap().handling = "Based".into();
            scene.world.insert(scene.player, HoldPreview { start: PreviewStart::Full, at: Some((id, length * 0.5)) });
            frame(scene, &posed);
            let gun = world_matrix(&scene.world, scene.camera).inverse() * world_matrix(&scene.world, scene.gun);
            let chest = scene.world.get::<Pose>(scene.body).unwrap().bones[rig.chest()];
            (gun, chest.to_scale_rotation_translation().1)
        };
        let (none, none_chest) = halfway(&mut scene, None);
        let (unfollowed, unfollowed_chest) = halfway(&mut scene, Some(("Lift", [0.0; 3])));
        assert!(unfollowed.abs_diff_eq(none, 1e-4), "a clip followed not at all changes nothing");
        assert!(unfollowed_chest.angle_between(none_chest) < 1e-3);

        // The gun goes as the clip's gun hand goes, from where it is held.
        let (still, _) = halfway(&mut scene, Some(("Still", [0.0, 1.0, 0.0])));
        let (lifted, _) = halfway(&mut scene, Some(("Lift", [0.0, 1.0, 0.0])));
        let rise = lifted.w_axis.truncate() - still.w_axis.truncate();
        assert!((rise - Vec3::new(0.0, 0.1, 0.0)).length() < 0.015, "lifted with the clip's hand: {rise}");
        assert!(lifted.to_scale_rotation_translation().1.angle_between(still.to_scale_rotation_translation().1) < 0.02);

        // The chest bends as the clip bends it, on top of how it is posed.
        let (_, still_chest) = halfway(&mut scene, Some(("Still", [1.0, 0.0, 0.0])));
        let (_, bent_chest) = halfway(&mut scene, Some(("Lift", [1.0, 0.0, 0.0])));
        let bent = bent_chest.angle_between(still_chest).to_degrees();
        assert!((bent - 20.0).abs() < 1.0, "bent as the clip bends it: {bent}");
    }

    #[test]
    fn the_support_hand_stays_planted_and_comes_forward_again_once_a_stumble_that_pulled_it_back_is_over() {
        let mut support = None;
        let settle = |support: &mut Option<Spring<f32>>, needed: f32| {
            (0..60).map(|_| planted(support, needed, 1.0 / 60.0)).last()
        };
        assert!(settle(&mut support, 0.05).unwrap() > 0.045, "back as far as the arm needs");
        assert!((settle(&mut support, 0.03).unwrap() - 0.05).abs() < 1e-3, "and planted there, not creeping forward");
        assert!(settle(&mut support, 0.25).unwrap() > 0.24, "a stumble pulls it far back");
        assert!(settle(&mut support, 0.03).unwrap() < 0.06, "and once it is over it comes forward again: {support:?}");
    }

    #[test]
    fn after_the_body_drops_as_on_landing_the_gun_is_back_at_the_shoulder_and_the_support_hand_forward() {
        let mut scene = scene();
        let rest = rest_of(&scene);
        // Half a metre up, as when spawned in the air, then down.
        let raised: Vec<Mat4> = rest.iter().map(|bone| Mat4::from_translation(Vec3::Y * 0.5) * *bone).collect();
        for _ in 0..20 {
            frame(&mut scene, &raised);
        }
        for _ in 0..90 {
            frame(&mut scene, &rest);
        }
        let held = world_matrix(&scene.world, scene.gun);
        let gun = scene.world.get::<Gun>(scene.gun).unwrap().clone();
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        let bones = scene.world.get::<Pose>(scene.body).unwrap().bones.clone();
        let placed = world_matrix(&scene.world, scene.body);
        let shoulder = placed.transform_point3(bones[rig.arms[1].upper].w_axis.truncate());
        let butt = held.transform_point3(Vec3::from(gun.butt));
        assert!(butt.distance(shoulder) < 0.15, "the butt back at the shoulder: {butt} {shoulder}");
        let slide = scene.world.get::<GunHolderState>(scene.player).unwrap().hands.support.map_or(0.0, |s| s.value);
        // Stuck where the drop pulled it, it would be as far back as it goes, 0.25.
        assert!(slide < 0.2, "the support hand forward on the fore-end again: {slide}");
    }

    #[test]
    fn a_long_gun_stays_steady_in_the_view_as_the_body_bobs_under_it() {
        let mut scene = scene();
        let rest = rest_of(&scene);
        for _ in 0..30 {
            frame(&mut scene, &rest);
        }
        // The whole body bobs five centimetres up and down twice a second, as a walk does; the eyes, steadied, less.
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for step in 0..120 {
            let bob = (step as f32 / 60.0 * std::f32::consts::TAU * 2.0).sin() * 0.05;
            let bobbed: Vec<Mat4> = rest.iter().map(|bone| Mat4::from_translation(Vec3::Y * bob) * *bone).collect();
            frame(&mut scene, &bobbed);
            let seen = world_matrix(&scene.world, scene.camera).inverse() * world_matrix(&scene.world, scene.gun);
            if step >= 60 {
                (low, high) = (low.min(seen.w_axis.y), high.max(seen.w_axis.y));
            }
        }
        assert!(high - low < 0.02, "it bobs {} in the view", high - low);
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
        scene.world.insert(scene.gun, GunShapes { pieces, ..GunShapes::default() });
        let rest = rest_of(&scene);
        let mut time = Time { delta: 1.0 / 60.0, ..Time::default() };
        for _ in 0..10 {
            step(&mut scene, &rest, &mut time);
        }
        if let Some(state) = scene.world.get_mut::<GunState>(scene.gun) {
            (state.rounds, state.chamber) = (0, Chamber::Empty);
        }
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::Reload.into());
        let skeleton = scene.world.get::<ModelRenderer>(scene.body).unwrap().animation.clone().unwrap();
        let rig = Rig::find(&skeleton.skeleton).unwrap();
        let mut worst: f32 = 0.0;
        let mut worked = 0;
        // Where the right palm is on the gun each frame, to see it never jumps: a quick reach moves it far in a
        // frame, but only a jump changes how far in one frame.
        let palm_on_gun = |scene: &Scene| {
            let edit = PoseEdit::new(&skeleton.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
            let into_gun = world_matrix(&scene.world, scene.gun).inverse() * world_matrix(&scene.world, scene.body);
            into_gun.transform_point3(palm_of(&edit, &rig.arms[1]).unwrap().point)
        };
        let (mut last, mut moved, mut leap) = (palm_on_gun(&scene), Vec3::ZERO, 0.0f32);
        for _ in 0..240 {
            step(&mut scene, &rest, &mut time);
            let now = palm_on_gun(&scene);
            (leap, moved, last) = (leap.max((now - last - moved).length()), now - last, now);
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
                shapes_now(&scene.world, scene.gun, &leaving, &[]).iter().map(|shape| shape.moved(to_body)).collect();
            let edit = PoseEdit::new(&skeleton.skeleton, &scene.world.get::<Pose>(scene.body).unwrap().bones);
            worst = worst.max(keep_out(&edit, &rig.arms[1], &avoid, to_body.transform_vector3(Vec3::X)).length());
            worked += 1;
        }
        assert!(worked > 60, "the right hand worked the reload: {worked} frames");
        assert!(worst < 0.004, "and kept out of the gun: {worst}");
        assert!(leap < 0.03, "never jumping from beat to beat: {leap}");
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

        // Shot dry, then reloaded: a clip's five rounds from the spare, each fired as soon as the bolt is worked.
        for _ in 0..5 {
            let mut waited = 0;
            while fire(&mut scene.world, scene.player).is_none() {
                step(&mut scene, &rest, &mut time);
                waited += 1;
            }
            assert!(waited < 10, "the bolt was worked within its 70 frames, and it fires again: waited {waited}");
            for _ in 0..70 {
                step(&mut scene, &rest, &mut time);
            }
        }
        let state = scene.world.get::<GunState>(scene.gun).unwrap().clone();
        assert!(!state.ready(GunKind::BoltActionRifle) && state.rounds == 0, "{state:?}");
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::Reload.into());
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
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::CheckMagazine.into());
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
        scene.world.get_mut::<GunHolder>(scene.player).unwrap().request = Some(ActionKind::CheckMagazine.into());
        for _ in 0..90 {
            step(&mut scene, &rest, &mut time);
        }
        assert_eq!(scene.world.get::<GunState>(scene.gun).unwrap().known_feed, Known::Rough);
    }
}
