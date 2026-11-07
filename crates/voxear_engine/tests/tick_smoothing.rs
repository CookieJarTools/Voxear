//! Smoothing between ticks: an Obj moved only on ticks, shown on a display
//! faster than the tick rate (headless).

use std::sync::{Arc, Mutex};

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::Engine;
use voxear_scene::Transform;

/// A tick's walk at 3 metres a second, sixty ticks a second.
const STEP: f32 = 3.0 / 60.0;
const FRAME: f32 = 1.0 / 144.0;

/// An engine walking an Obj along X on each tick; where each tick found it.
fn walker() -> (Engine, Entity, Arc<Mutex<Vec<f32>>>) {
    let mut engine = Engine::new();
    let walker = engine.world.spawn((Transform::from_position(Vec3::ZERO),));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let found = seen.clone();
    engine.schedule.add_system("FixedUpdate", "walk", move |world| {
        let Some(transform) = world.get_mut::<Transform>(walker) else { return };
        found.lock().unwrap().push(transform.position.x);
        transform.position.x += STEP;
    });
    (engine, walker, seen)
}

fn x(engine: &Engine, entity: Entity) -> f32 {
    engine.world.get::<Transform>(entity).unwrap().position.x
}

#[test]
fn an_obj_moved_on_ticks_moves_evenly_between_them() {
    let (mut engine, walker, seen) = walker();
    // Turned in Update every frame, as a player's view turns, its walk is still smoothed.
    let mut yaw = 0.0f32;
    engine.schedule.add_system("Update", "turn", move |world| {
        yaw += 0.01;
        if let Some(transform) = world.get_mut::<Transform>(walker) {
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    });
    let mut shown = Vec::new();
    for _ in 0..300 {
        engine.tick(FRAME);
        shown.push(x(&engine, walker));
    }
    let even = STEP * 60.0 * FRAME;
    for (frame, pair) in shown.windows(2).enumerate().skip(10) {
        let moved = pair[1] - pair[0];
        assert!((moved - even).abs() < even * 0.2, "frame {frame} moved {moved}, not about {even}");
    }
    // A tick behind at most, never ahead.
    let ticks = seen.lock().unwrap().len() as f32;
    let truth = ticks * STEP;
    let last = *shown.last().unwrap();
    assert!(last <= truth + 1e-4 && truth - last <= STEP + 1e-4, "shown {last}, the ticks left it at {truth}");
}

#[test]
fn ticks_move_it_from_where_they_left_it() {
    let (mut engine, _, seen) = walker();
    for _ in 0..300 {
        engine.tick(FRAME);
    }
    let seen = seen.lock().unwrap();
    for (tick, pair) in seen.windows(2).enumerate() {
        assert!((pair[1] - pair[0] - STEP).abs() < 1e-4, "tick {tick} found it at {} after {}", pair[1], pair[0]);
    }
}

#[test]
fn an_obj_put_somewhere_in_update_is_there_for_the_next_tick() {
    let (mut engine, walker, seen) = walker();
    let frame = Arc::new(Mutex::new(0));
    let counted = frame.clone();
    engine.schedule.add_system("Update", "teleport", move |world| {
        let mut frame = counted.lock().unwrap();
        *frame += 1;
        if *frame == 100
            && let Some(transform) = world.get_mut::<Transform>(walker)
        {
            transform.position = Vec3::new(100.0, 0.0, 0.0);
        }
    });
    for _ in 0..100 {
        engine.tick(FRAME);
    }
    let ticked = seen.lock().unwrap().len();
    assert_eq!(x(&engine, walker), 100.0);
    while seen.lock().unwrap().len() == ticked {
        engine.tick(FRAME);
    }
    assert_eq!(seen.lock().unwrap()[ticked], 100.0, "the next tick walks on from where it was put");
    let shown = x(&engine, walker);
    assert!((100.0..=100.0 + STEP).contains(&shown), "shown at {shown}");
}

#[test]
fn in_edit_mode_nothing_is_smoothed() {
    let (mut engine, walker, seen) = walker();
    engine.set_edit_mode(true);
    for _ in 0..50 {
        engine.tick(FRAME);
    }
    let ticks = seen.lock().unwrap().len() as f32;
    assert!((x(&engine, walker) - ticks * STEP).abs() < 1e-4, "where the ticks left it");
}
