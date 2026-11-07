//! Smoothing between ticks. An Obj moved in FixedUpdate, or by physics,
//! moves only on ticks: on a display faster than the tick rate, most frames
//! would show it standing still and a few show it jump. So between ticks it
//! is shown part-way from where the tick before last left it to where the
//! last one did, by how far the frame is into the next tick: a tick behind,
//! but moving evenly. Update, LateUpdate and drawing see it there; ticks,
//! physics and the network see where the ticks left it, put back before
//! they run.
//!
//! What the ticks moved is found from the chunks whose Transforms they
//! wrote, so Objs that never move cost nothing. Whatever is written over an
//! Obj while it is shown, as a teleport in Update, is kept as written.

use std::collections::HashMap;

use voxear_ecs::{Entity, World};
use voxear_scene::Transform;

/// Ticked frames an Obj may stand still before it is no longer watched.
const STILL_FRAMES: u32 = 120;
/// Further than this, in metres, in a tick, it is shown there at once, as a
/// teleport should be.
const SNAP: f32 = 4.0;

/// The Objs the ticks move, and how far the frame is into the next tick.
#[derive(Default)]
pub struct TickSmoothing {
    objs: HashMap<Entity, Smoothed>,
    /// The change tick before this frame's ticks.
    since: u32,
    /// How far the frame is into the next tick, from 0 to 1.
    share: f32,
    shown: bool,
}

#[derive(Clone, Copy)]
struct Smoothed {
    /// Where it was before the last tick.
    before: Transform,
    /// Where the last tick left it.
    after: Transform,
    /// Where it is shown.
    shown: Transform,
    still: u32,
}

impl Smoothed {
    fn at(transform: Transform) -> Self {
        Self { before: transform, after: transform, shown: transform, still: 0 }
    }

    fn between(&self, share: f32) -> Transform {
        let (before, after) = (&self.before, &self.after);
        if before.position.distance(after.position) > SNAP {
            return *after;
        }
        Transform::new(
            before.position.lerp(after.position, share),
            before.rotation.slerp(after.rotation, share),
            before.scale.lerp(after.scale, share),
        )
    }

    /// Where the ticks left it, but for what was written over what was shown.
    fn put_back(&mut self, now: Transform) -> Transform {
        if now.position != self.shown.position {
            (self.before.position, self.after.position) = (now.position, now.position);
        }
        if now.rotation != self.shown.rotation {
            (self.before.rotation, self.after.rotation) = (now.rotation, now.rotation);
        }
        if now.scale != self.shown.scale {
            (self.before.scale, self.after.scale) = (now.scale, now.scale);
        }
        self.after
    }
}

/// Puts each Obj shown between ticks back where the ticks left it; true if
/// any was shown.
pub fn hide(world: &mut World) -> bool {
    let Some(mut smoothing) = world.remove_resource::<TickSmoothing>() else { return false };
    let shown = std::mem::take(&mut smoothing.shown);
    if shown {
        smoothing.objs.retain(|&entity, obj| {
            let Some(&now) = world.get::<Transform>(entity) else { return false };
            let back = obj.put_back(now);
            if back != now
                && let Some(transform) = world.get_mut::<Transform>(entity)
            {
                *transform = back;
            }
            true
        });
    }
    world.insert_resource(smoothing);
    shown
}

/// Shows each Obj the ticks move part-way between its last two ticks.
pub fn show(world: &mut World) {
    let Some(mut smoothing) = world.remove_resource::<TickSmoothing>() else { return };
    let share = smoothing.share;
    for (&entity, obj) in &mut smoothing.objs {
        obj.shown = obj.after;
        if obj.before == obj.after {
            continue;
        }
        let between = obj.between(share);
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            *transform = between;
            obj.shown = between;
        }
    }
    smoothing.shown = true;
    world.insert_resource(smoothing);
}

/// Before a frame's ticks: each Obj back where they left it, or where it
/// was put since, as by the server, and the point to tell what they move
/// from.
pub fn begin_ticks(world: &mut World) {
    hide(world);
    let since = world.change_tick();
    world.advance_tick();
    let mut smoothing = world.remove_resource::<TickSmoothing>().unwrap_or_default();
    smoothing.since = since;
    smoothing.objs.retain(|&entity, obj| {
        let Some(&now) = world.get::<Transform>(entity) else { return false };
        if now != obj.after {
            *obj = Smoothed { still: obj.still, ..Smoothed::at(now) };
        }
        true
    });
    world.insert_resource(smoothing);
}

/// Before each tick: where each Obj is, for the tick to move it from.
pub fn before_tick(world: &mut World) {
    let Some(mut smoothing) = world.remove_resource::<TickSmoothing>() else { return };
    for (&entity, obj) in &mut smoothing.objs {
        if let Some(&now) = world.get::<Transform>(entity) {
            obj.before = now;
        }
    }
    world.insert_resource(smoothing);
}

/// After a frame's ticks, `share` of the way into the next: watches what
/// they moved and shows it between its last two ticks.
pub fn end_ticks(world: &mut World, ticked: bool, share: f32) {
    let Some(mut smoothing) = world.remove_resource::<TickSmoothing>() else { return };
    if ticked {
        let moved = world.query_ref::<(Entity, &Transform)>().changed::<Transform>(smoothing.since);
        for (entity, &transform) in moved {
            smoothing.objs.entry(entity).or_insert_with(|| Smoothed::at(transform));
        }
        smoothing.objs.retain(|&entity, obj| {
            let Some(&now) = world.get::<Transform>(entity) else { return false };
            obj.after = now;
            obj.still = if obj.before == now { obj.still + 1 } else { 0 };
            obj.still < STILL_FRAMES
        });
    }
    smoothing.share = share.clamp(0.0, 1.0);
    world.insert_resource(smoothing);
    show(world);
}
