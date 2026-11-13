//! Portals in the game: Objs going through them, cameras carried through
//! them, and their far sides made ready (the component, its maths and how
//! a far side is drawn are in `voxear_render::portals`).
//!
//! **Going through.** A Character Controller, a Rigidbody or a
//! [`PortalTraveller`] whose Obj's place crosses an open Portal's opening
//! comes out of the Portal it leads to, at the same spot of that opening,
//! turned and moving as the turn between the two Portals makes it.
//! [`carry_objs`] finds the crossings from where each Obj was when last
//! looked at: after each physics step, from where the tick found it, and
//! after Update and after animation, from where the frame's scripts found
//! it ([`look`] notes those places). So however an Obj is moved it goes
//! through in the frame it crosses, and one only shown part-way between
//! two ticks is not taken for one that moved. Physics, the poses
//! shown between ticks and the renderer are each told, so nothing slides,
//! sweeps or smears from one place to the other. Each crossing is kept in
//! [`PortalEvents`] for the frame, which scripts hear as `Portal.Passed`.
//!
//! **Cameras.** A camera goes through a Portal when its own place crosses
//! the opening, not when what it films does. A camera under a body goes
//! with it; until then, and for a camera that follows its subject from
//! behind, [`carry_views`] finds the Portal whose opening lies between the
//! camera and its subject and gives the camera a [`ViewCarry`]: it looks
//! from the other side of that Portal, so it sees its subject through the
//! opening for as long as it would in a world where the two places were
//! joined there. The renderer is told whenever where a camera looks from
//! goes through a Portal, and carries its history through with it.

use std::collections::HashMap;

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_physics::scene::{CharacterController, PhysicsScene, Rigidbody};
use voxear_render::{
    Camera, CameraFollows, Carry, Portal, PortalFar, PortalPose, PortalSide, PortalTraveller, Renderer, ViewCarry,
    portals::crossing,
};
use voxear_scene::{Destroying, Layer, Parent, Switched, Transform, WorldTransform};

/// How far past its opening's edges a Portal still takes an Obj whose own
/// place crosses its plane, in metres: feet on its sill, a shoulder past its
/// side.
const MARGIN: f32 = 0.25;

/// An Obj that went through a Portal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalPassed {
    pub obj: Entity,
    /// The Portal it went in at, at which face, and the one it came out of.
    pub from: Entity,
    pub side: PortalSide,
    pub to: Entity,
    /// What going through did to its place, its turn and its speed.
    pub carry: Carry,
}

/// The Objs that went through Portals this frame, in order, as a world's
/// resource: emptied as each frame begins.
#[derive(Clone, Debug, Default)]
pub struct PortalEvents {
    pub passed: Vec<PortalPassed>,
}

/// Where each Obj that can go through a Portal was when last looked at.
#[derive(Default)]
struct PortalTracks {
    last: HashMap<Entity, Vec3>,
}

/// What each camera's view did last frame: where its Obj was, and the
/// Portal its view was carried through.
#[derive(Default)]
struct PortalViews {
    cameras: HashMap<Entity, (Vec3, Option<Carry>)>,
    /// What the renderer is yet to be told: each camera whose view went
    /// through a Portal, by which move.
    cuts: Vec<(Entity, Mat4)>,
    /// And each Obj that went through one, with those under it.
    objs: Vec<(Entity, Mat4)>,
}

/// An open Portal that leads somewhere, as going through it needs it.
#[derive(Clone, Copy)]
struct Way {
    entity: Entity,
    to: Entity,
    pose: PortalPose,
    portal: Portal,
    carry: Carry,
}

/// Every Portal an Obj can go through now: open, on, leading to a Portal
/// that is there, and with its far side made.
fn ways(world: &World) -> Vec<Way> {
    let switched = Switched::of(world);
    world
        .query_ref::<(Entity, &Portal, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, portal, ..)| portal.open && switched.is_active(*entity))
        .filter(|(entity, ..)| world.get::<PortalFar>(*entity).is_none_or(|far| far.ready))
        .filter_map(|(entity, portal, transform, placed)| {
            let to = portal.to.filter(|to| *to != entity && world.has::<Portal>(*to) && !world.has::<Destroying>(*to))?;
            let pose = PortalPose::of(transform, placed);
            let carry = Carry::between(&pose, &PortalPose::of_obj(world, to)?);
            Some(Way { entity, to, pose, portal: *portal, carry })
        })
        .collect()
}

/// Where an Obj is in the world.
fn place(world: &World, entity: Entity) -> Vec3 {
    voxear_scene::world_matrix(world, entity).w_axis.truncate()
}

/// Whether `entity` is `root` or under it.
fn under(world: &World, entity: Entity, root: Entity) -> bool {
    let mut at = Some(entity);
    for _ in 0..64 {
        match at {
            Some(here) if here == root => return true,
            Some(here) => at = world.get::<Parent>(here).map(|parent| parent.0),
            None => return false,
        }
    }
    false
}

/// Sets an Obj down where `carry` takes it, turned with it, whatever it is
/// under.
fn set_down(world: &mut World, entity: Entity, carry: &Carry) {
    let (scale, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
    let carried = Mat4::from_scale_rotation_translation(scale, carry.turn(rotation), carry.point(position));
    let parent = world.get::<Parent>(entity).map(|parent| voxear_scene::world_matrix(world, parent.0));
    let local = parent.map_or(carried, |parent| parent.inverse() * carried);
    let (_, rotation, position) = local.to_scale_rotation_translation();
    if let Some(transform) = world.get_mut::<Transform>(entity) {
        (transform.position, transform.rotation) = (position, rotation);
    }
}

/// The far side of a Portal near enough to show it, which the world is
/// made round as it is round the camera, less far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalExit {
    /// The Portal whose far side it is.
    pub portal: Entity,
    /// Where the Portal it leads to stands.
    pub at: Vec3,
    /// How far round it a world without end makes its own blocks, and how
    /// far its far view reaches from there.
    pub distance: f32,
    pub far: f32,
}

/// The far sides being made this tick, as a world's resource, and how many
/// ticks each has had its ground.
#[derive(Clone, Debug, Default)]
struct PortalExits {
    exits: Vec<PortalExit>,
    made_for: HashMap<Entity, u32>,
}

/// The far sides the world is made round this tick (see [`find_exits`]).
pub fn exits(world: &World) -> Vec<PortalExit> {
    world.resource::<PortalExits>().map(|exits| exits.exits.clone()).unwrap_or_default()
}

/// How much further from a Portal than it shows its far side that far side
/// starts to be made, in metres, so it is there by the time it shows; and
/// how much further again one being made is kept, so a step back and forth
/// at the edge makes nothing twice.
const MADE_BEFORE: f32 = 24.0;
const KEPT_PAST: f32 = 32.0;

/// How far round a Portal's far side the ground must be made, with what
/// stands on it, before the Portal opens: room to come out and look round.
const READY_ROUND: f32 = 40.0;

/// How many ticks a far side's ground must have stood before the Portal
/// opens: physics takes it up the tick after it is made.
const READY_TICKS: u32 = 3;

/// How long a Portal's mist takes to clear once its far side is ready, in
/// seconds.
const CLEARS_IN: f32 = 0.4;

/// Finds the far sides to make this tick: of each Portal that leads to one
/// that is there, while where the world is seen from is near enough for it
/// to show. Before the world's ground is brought round where it is seen
/// from, which takes them in.
pub(crate) fn find_exits(world: &mut World) {
    let was = world.remove_resource::<PortalExits>().unwrap_or_default();
    let Some(viewer) = crate::terrain::viewer(world) else { return };
    let quality = world.resource::<voxear_render::portals::PortalQuality>().copied().unwrap_or_default();
    let switched = Switched::of(world);
    let exits: Vec<PortalExit> = world
        .query_ref::<(Entity, &Portal, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, portal, ..)| portal.open && switched.is_active(*entity))
        .filter_map(|(entity, portal, transform, placed)| {
            let to = portal.to.filter(|to| *to != entity && world.has::<Portal>(*to) && !world.has::<Destroying>(*to))?;
            let pose = PortalPose::of(transform, placed);
            let kept = was.exits.iter().any(|exit| exit.portal == entity);
            let reach = portal.view_distance.max(0.0) + MADE_BEFORE + if kept { KEPT_PAST } else { 0.0 };
            let local = pose.local(viewer);
            let nearest = Vec3::new(local.x.clamp(-portal.size.x * 0.5, portal.size.x * 0.5), local.y.clamp(0.0, portal.size.y), 0.0);
            (local.distance(nearest) <= reach).then_some(())?;
            let at = PortalPose::of_obj(world, to)?.position;
            Some(PortalExit { portal: entity, at, distance: quality.far_side_distance, far: quality.far_side_far_view })
        })
        .collect();
    if exits.is_empty() && was.made_for.is_empty() {
        return;
    }
    world.insert_resource(PortalExits { exits, made_for: was.made_for });
}

/// Says of each Portal whether its far side is made (its [`PortalFar`]),
/// and clears its mist once it is. After the world's ground is brought
/// round where it is seen from. Where no world is without end, every far
/// side is there as it stands, and no Portal is given one.
pub(crate) fn update_far(world: &mut World, delta: f32) {
    let portals: Vec<(Entity, Option<PortalFar>)> = world
        .query_ref::<(Entity, &Portal, Option<&PortalFar>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _, far)| (entity, far.copied()))
        .collect();
    if portals.is_empty() {
        world.remove_resource::<PortalExits>();
        return;
    }
    let mut exits = world.remove_resource::<PortalExits>().unwrap_or_default();
    let mut made_for = HashMap::new();
    for (entity, was) in portals {
        let exit = exits.exits.iter().find(|exit| exit.portal == entity).copied();
        // Nothing streams at all: as it stands.
        let streams = exit.map_or_else(|| crate::blocks::made_round(world, Vec3::ZERO, 0.0), |exit| crate::blocks::made_round(world, exit.at, READY_ROUND));
        let now = match (streams, exit) {
            (None, _) => None,
            (Some(made), Some(_)) => {
                let ticks = if made { exits.made_for.get(&entity).map_or(1, |ticks| ticks + 1) } else { 0 };
                made_for.insert(entity, ticks);
                let ready = ticks >= READY_TICKS;
                let shown = match (ready, was) {
                    (true, Some(was)) => (was.shown + delta.max(0.0) / CLEARS_IN).min(1.0),
                    _ => 0.0,
                };
                Some(PortalFar { ready, shown })
            }
            // Too far for its far side to be made.
            (Some(_), None) => Some(PortalFar { ready: false, shown: 0.0 }),
        };
        match now {
            Some(far) if was != Some(far) => {
                world.insert(entity, far);
            }
            None if was.is_some() => {
                world.remove::<PortalFar>(entity);
            }
            _ => {}
        }
    }
    exits.made_for = made_for;
    world.insert_resource(exits);
}

/// Empties the frame's [`PortalEvents`]: at the start of each frame.
pub(crate) fn begin_frame(world: &mut World) {
    if let Some(events) = world.resource_mut::<PortalEvents>() {
        events.passed.clear();
    }
}

/// Every Obj that goes through Portals, with its layer.
fn travellers(world: &World) -> Vec<(Entity, u8)> {
    world
        .query_ref::<(Entity, &Transform, Option<&CharacterController>, Option<&Rigidbody>, Option<&PortalTraveller>, Option<&Layer>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, _, character, body, traveller, _)| character.is_some() || body.is_some() || traveller.is_some())
        .map(|(entity, .., layer)| (entity, layer.map_or(0, |layer| layer.0)))
        .collect()
}

/// Notes where each Obj that goes through Portals is, for [`carry_objs`]
/// to tell what moves it from here on: before a tick moves them, and before
/// a frame's scripts do, when those the ticks move are shown part-way
/// between two ticks.
pub(crate) fn look(world: &mut World) {
    if world.query_ref::<&Portal>().into_iter().next().is_none() {
        world.remove_resource::<PortalTracks>();
        return;
    }
    let last = travellers(world).into_iter().map(|(entity, _)| (entity, place(world, entity))).collect();
    world.insert_resource(PortalTracks { last });
}

/// Takes each Obj that crossed a Portal's opening since it was last looked
/// at through to the Portal that one leads to. Without a Portal in the
/// world it does nothing and keeps nothing.
pub(crate) fn carry_objs(world: &mut World) {
    let ways = ways(world);
    if ways.is_empty() {
        world.remove_resource::<PortalTracks>();
        return;
    }
    let travellers = travellers(world);
    let mut tracks = world.remove_resource::<PortalTracks>().unwrap_or_default();
    let mut seen = HashMap::with_capacity(travellers.len());
    for (entity, layer) in travellers {
        let now = place(world, entity);
        let Some(&before) = tracks.last.get(&entity) else {
            seen.insert(entity, now);
            continue;
        };
        let through = ways.iter().find_map(|way| {
            let passes = way.portal.passes & (1 << (layer & 31)) != 0;
            let side = crossing(&way.pose, way.portal.size, before, now, MARGIN).filter(|_| passes)?;
            Some((*way, side))
        });
        let Some((way, side)) = through.filter(|_| before != now) else {
            seen.insert(entity, now);
            continue;
        };
        set_down(world, entity, &way.carry);
        if let Some(mut physics) = world.remove_resource::<PhysicsScene>() {
            physics.teleported(world, entity, way.carry.rotation);
            world.insert_resource(physics);
        }
        crate::tick_smoothing::carried(world, entity, &way.carry);
        seen.insert(entity, place(world, entity));
        let passed = PortalPassed { obj: entity, from: way.entity, side, to: way.to, carry: way.carry };
        match world.resource_mut::<PortalEvents>() {
            Some(events) => events.passed.push(passed),
            None => world.insert_resource(PortalEvents { passed: vec![passed] }),
        }
        // The renderer hears of it, and of everything under it, before the frame is drawn.
        let mut views = world.remove_resource::<PortalViews>().unwrap_or_default();
        let moved = way.carry.matrix();
        let below = world.query_ref::<(Entity, &Parent)>().into_iter().map(|(below, _)| below);
        let below: Vec<Entity> = below.filter(|below| *below != entity && under(world, *below, entity)).collect();
        views.objs.extend(std::iter::once(entity).chain(below).map(|obj| (obj, moved)));
        world.insert_resource(views);
    }
    tracks.last = seen;
    world.insert_resource(tracks);
}

/// The world's middle moved by `by`: where each Obj was moves with it, so
/// the move reads as no crossing.
pub(crate) fn shift(world: &mut World, by: Vec3) {
    if let Some(tracks) = world.resource_mut::<PortalTracks>() {
        for last in tracks.last.values_mut() {
            *last += by;
        }
    }
    if let Some(views) = world.resource_mut::<PortalViews>() {
        for (last, _) in views.cameras.values_mut() {
            *last += by;
        }
    }
}

/// What a camera films: the Obj its [`CameraFollows`] names, or else the
/// nearest Obj it is under that goes through Portals.
fn subject(world: &World, camera: Entity) -> Option<Entity> {
    if let Some(follows) = world.get::<CameraFollows>(camera) {
        return world.has::<Transform>(follows.0).then_some(follows.0);
    }
    let mut at = world.get::<Parent>(camera).map(|parent| parent.0);
    for _ in 0..64 {
        let here = at?;
        if world.has::<CharacterController>(here) || world.has::<Rigidbody>(here) || world.has::<PortalTraveller>(here) {
            return Some(here);
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    None
}

/// How far above its Obj's place a subject is looked at, in metres: a body's
/// middle, not its feet, which stand on a Portal's sill.
const SUBJECT_HEIGHT: f32 = 1.0;

/// Gives each camera whose subject is on the other side of a Portal's
/// opening from it a [`ViewCarry`] through that Portal, takes it from those
/// that need none, and notes for the renderer each camera whose view went
/// through a Portal this frame. After LateUpdate, when cameras are where
/// their scripts put them.
pub(crate) fn carry_views(world: &mut World) {
    let ways = ways(world);
    let mut views = world.remove_resource::<PortalViews>().unwrap_or_default();
    if ways.is_empty() && views.cameras.is_empty() {
        return;
    }
    let cameras: Vec<Entity> =
        world.query_ref::<(Entity, &Camera, &Transform)>().without::<Destroying>().into_iter().map(|(e, ..)| e).collect();
    let passed: Vec<PortalPassed> = world.resource::<PortalEvents>().map(|events| events.passed.clone()).unwrap_or_default();
    let mut kept = HashMap::with_capacity(cameras.len());
    for camera in cameras {
        let here = place(world, camera);
        let subject = subject(world, camera);
        // The Portal between the camera and what it films, walking from the subject to the camera: in at one face
        // of it, the camera looks from where coming out of the other puts it.
        let carry = subject.and_then(|subject| {
            let up = voxear_scene::world_matrix(world, subject).y_axis.truncate().normalize_or_zero();
            let filmed = place(world, subject) + up * SUBJECT_HEIGHT;
            ways.iter().find(|way| crossing(&way.pose, way.portal.size, filmed, here, MARGIN).is_some()).map(|way| way.carry)
        });
        match carry {
            Some(carry) => {
                world.insert(camera, ViewCarry(carry));
            }
            None => {
                world.remove::<ViewCarry>(camera);
            }
        }
        // Where it looks from went through a Portal when its Obj did, with its subject, or its carry changed: the
        // three together are one move, and none at all when they undo each other.
        if let Some(&(before, old)) = views.cameras.get(&camera) {
            let went = passed.iter().rev().find(|passed| subject == Some(passed.obj) || under(world, camera, passed.obj));
            // Only if the camera went with it: one that lags behind its subject is still where it was.
            let went = went.map(|passed| passed.carry).filter(|carry| carry.point(before).distance(here) < before.distance(here));
            let matrix = |carry: Option<Carry>| carry.map_or(Mat4::IDENTITY, |carry| carry.matrix());
            if went.is_some() || old != carry {
                let moved = matrix(carry) * matrix(went) * matrix(old).inverse();
                // Carried there and back is no move, as near as moves between far places come out.
                if !moved.abs_diff_eq(Mat4::IDENTITY, 0.05) {
                    views.cuts.push((camera, moved));
                }
            }
        }
        kept.insert(camera, (here, carry));
    }
    views.cameras = kept;
    world.insert_resource(views);
}

/// Tells the renderer of each camera whose view went through a Portal this
/// frame, and each Obj that did, before it draws the frame.
pub(crate) fn tell_renderer(world: &mut World, renderer: Option<&mut Renderer>) {
    let Some(views) = world.resource_mut::<PortalViews>() else { return };
    let (cuts, objs) = (std::mem::take(&mut views.cuts), std::mem::take(&mut views.objs));
    let Some(renderer) = renderer else { return };
    for (camera, moved) in &cuts {
        renderer.carry_view(*camera, *moved);
    }
    renderer.carry_objs(&objs);
}
