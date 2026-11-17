//! What stirs the water, for the trails the renderer draws on it
//! (`voxear_render::water_trails`), with nothing set up: every Character
//! Controller, so a swimmer leaves ripples and foam, and every body with a
//! Collider, so a boat or a crate thrown in does, each as wide as its body
//! where it cuts the surface. A Water Wake tunes what its Obj makes, gives
//! an Obj with no body a wake of its own, and switched off, stops it.
//!
//! Each frame notes, of what is near the viewer, each body whose height
//! reaches across the water's surface, or nearly, as the renderer finds
//! from where it is drawn when it goes in, comes out and moves across; and
//! keeps a grid of how the water's current runs round the viewer, worked
//! out a few cells a frame, which the trails are carried along by.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_physics::scene::{CharacterController, Collider, Rigidbody};
use voxear_render::{Stir, Stirs, TrailFlow, TrailQuality, WaterTrails, WaterWake, water_trails::FLOW_CELLS};
use voxear_scene::{Destroying, Switch, Switched, Transform, WorldTransform};

/// How far over or under the water's surface a body may be, past its own
/// height, and still be noted, in metres: so the frames it leaves the water
/// and comes back are seen, even falling fast, which a renderer needs a
/// frame of beforehand to tell how fast it goes in.
const NEAR_SURFACE: f32 = 2.5;
/// How wide a character's wake is, as many times its radius.
const CHARACTER_WAKE: f32 = 1.3;
/// How wide the wake is of an Obj with a Water Wake and nothing else to go
/// by, in metres.
const BARE_WAKE: f32 = 0.5;
/// How many cells of the current's grid are worked out a frame: those not
/// yet known first, then this many again in turn.
const FLOW_NEW: usize = 24;
const FLOW_AGAIN: usize = 4;

/// The grid of the water's current round the viewer, kept from frame to
/// frame: a cell keeps what it holds as the viewer moves on.
#[derive(Default)]
struct CurrentKept {
    /// The cell at its corner, in cells from the world's origin, and how
    /// wide a cell is.
    anchor: Option<[i64; 2]>,
    cell: f32,
    cells: Vec<Option<[f32; 2]>>,
    next: usize,
    /// Where the world's middle was, so the grid starts again when it moves.
    origin: voxear_core::math::I64Vec3,
}

impl voxear_ecs::Component for CurrentKept {}

/// Notes what stirs the water near the viewer this frame, and how the
/// water's current runs round it, for the renderer's trails.
pub fn update(world: &mut World) {
    let settings = world.resource::<WaterTrails>().copied().unwrap_or_default();
    let viewer = crate::terrain::viewer(world);
    let Some(viewer) = viewer.filter(|_| settings.quality != TrailQuality::Off) else {
        world.remove_resource::<Stirs>();
        world.remove_resource::<TrailFlow>();
        return;
    };
    let near = settings.area.clamp(WaterTrails::AREA.0, WaterTrails::AREA.1) * 0.75;
    let stirs = stirs(world, viewer, near);
    let level = stirs.iter().map(|stir| stir.surface).next();
    let mut kept = world.remove_resource::<CurrentKept>().unwrap_or_default();
    // The current only matters where something has stirred the water, at the height of what last did.
    if let Some(level) = level.or(kept.anchor.map(|_| viewer.y)) {
        let flow = follow_current(world, &mut kept, viewer, settings.area / FLOW_CELLS as f32, level);
        world.insert_resource(flow);
    }
    world.insert_resource(kept);
    world.insert_resource(Stirs(stirs));
}

/// A body's Water Wake, if it has one: `None` when it is switched off, as
/// the body then stirs nothing.
fn wake_of(world: &World, switched: &Switched, entity: Entity) -> Option<WaterWake> {
    match world.get::<WaterWake>(entity) {
        Some(wake) => switched.is_on(world, entity, Switch::WaterWake).then_some(*wake),
        None => Some(WaterWake::default()),
    }
}

/// What stirs the water within `near` metres of `viewer` along the ground,
/// nearest first: characters, bodies with Colliders, and Objs with a Water
/// Wake and neither.
fn stirs(world: &World, viewer: Vec3, near: f32) -> Vec<Stir> {
    let switched = Switched::of(world);
    let close = |at: Vec3| (at - viewer).with_y(0.0).length_squared() < near * near;
    let mut found = Vec::new();
    // Characters, from their feet up: treading water from as deep as their waist, wholly as deep as their chest.
    let characters = world.query_ref::<(Entity, &CharacterController)>().without::<Destroying>();
    for (entity, controller) in characters {
        let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        if !switched.is_active(entity) || !close(at) {
            continue;
        }
        let Some(wake) = wake_of(world, &switched, entity) else { continue };
        let tall = controller.height.max(0.1);
        let Some(water) = crate::water::sample(world, at + Vec3::Y * tall * 0.5) else { continue };
        let over = water.height - at.y;
        let radius = if wake.size > 0.0 { wake.size * 0.5 } else { controller.radius.max(0.1) * CHARACTER_WAKE };
        let bob = ((over - tall * 0.4) / (tall * 0.2)).clamp(0.0, 1.0);
        let stir = stir_of(entity, ([Vec3::ZERO; 2], radius), (water.height, [0.0, tall], at.y), water.flow, wake, bob);
        found.extend(stir);
    }
    // Bodies with Colliders, as wide as the box round the Collider where it cuts the water, long ones end to end.
    let bodies = world.query_ref::<(Entity, &Collider, &Rigidbody)>().without::<Destroying>();
    for (entity, collider, _) in bodies {
        if world.has::<CharacterController>(entity) || !switched.is_active(entity) {
            continue;
        }
        let to_world: Mat4 = voxear_scene::world_matrix(world, entity);
        let place = to_world.w_axis.truncate();
        if !close(place) || collider.trigger {
            continue;
        }
        let Some(wake) = wake_of(world, &switched, entity) else { continue };
        let (middle, reach) = crate::water::collider_box(world, entity, collider);
        let centre = to_world.transform_point3(middle);
        let Some(water) = crate::water::sample(world, centre) else { continue };
        let axes =
            [Vec3::X * reach.x, Vec3::Y * reach.y, Vec3::Z * reach.z].map(|axis| to_world.transform_vector3(axis));
        let high = axes.iter().map(|axis| axis.y.abs()).sum::<f32>();
        // Its footprint's long way and its width, as the box's sides lie across the water.
        let sides = axes.map(|axis| axis.with_y(0.0));
        let (long, wide) = longest_across(sides);
        let radius = if wake.size > 0.0 { wake.size * 0.5 } else { wide.max(0.05) };
        let ends = match long.length() > radius {
            true => {
                let way = long.normalize() * (long.length() - radius);
                [centre - place + way, centre - place - way]
            }
            false => [centre - place; 2],
        };
        let reaches = [centre.y - high - place.y, centre.y + high - place.y];
        found.extend(stir_of(entity, (ends, radius), (water.height, reaches, place.y), water.flow, wake, 0.0));
    }
    // Objs with a Water Wake and no body to go by, as wide as it says, as high as wide.
    let bare = world.query_ref::<(Entity, &WaterWake)>().without::<Destroying>();
    for (entity, wake) in bare {
        if world.has::<CharacterController>(entity) || world.has::<Collider>(entity) || !switched.is_active(entity) {
            continue;
        }
        if !switched.is_on(world, entity, Switch::WaterWake) {
            continue;
        }
        let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        let Some(water) = close(at).then(|| crate::water::sample(world, at)).flatten() else { continue };
        let radius = if wake.size > 0.0 { wake.size * 0.5 } else { BARE_WAKE * 0.5 };
        let stir =
            stir_of(entity, ([Vec3::ZERO; 2], radius), (water.height, [-radius, radius], at.y), water.flow, *wake, 0.0);
        found.extend(stir);
    }
    found.sort_by(|a, b| {
        let from = |stir: &Stir| (stir.ends[0] - viewer).length_squared();
        from(a).total_cmp(&from(b))
    });
    found
}

/// The longest of a box's three sides across the water, and the half
/// width of the footprint across it: the longest of the other two's reach
/// across that way.
fn longest_across(sides: [Vec3; 3]) -> (Vec3, f32) {
    let long = sides.into_iter().max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec3::ZERO);
    let across = long.cross(Vec3::Y).normalize_or_zero();
    let wide = match across == Vec3::ZERO {
        true => sides.iter().map(|side| side.length()).fold(0.0, f32::max),
        false => sides.iter().map(|side| side.dot(across).abs()).sum(),
    };
    (long, wide)
}

/// A body stirring the water, if it reaches up to the surface or nearly:
/// its ends from its Obj's place and its radius, the water's height, how
/// far below and above its place it reaches, and its place's height.
fn stir_of(
    entity: Entity,
    (ends, radius): ([Vec3; 2], f32),
    (surface, reach, at): (f32, [f32; 2], f32),
    flow: Vec3,
    wake: WaterWake,
    bob: f32,
) -> Option<Stir> {
    let (bottom, top) = (at + reach[0], at + reach[1]);
    if surface < bottom - NEAR_SURFACE || surface > top + NEAR_SURFACE || (wake.strength <= 0.0 && wake.foam <= 0.0) {
        return None;
    }
    Some(Stir { entity, ends, radius, surface, reach, flow, strength: wake.strength, foam: wake.foam, bob })
}

/// Moves the grid of the current to round `viewer`, cells `cell` wide,
/// works out some of its cells at `level`, and gives it as the renderer
/// reads it: unknown cells as still water.
fn follow_current(world: &World, kept: &mut CurrentKept, viewer: Vec3, cell: f32, level: f32) -> TrailFlow {
    let side = FLOW_CELLS as i64;
    let origin = voxear_scene::WorldOrigin::of(world).0;
    let anchor = [(viewer.x / cell).floor() as i64 - side / 2, (viewer.z / cell).floor() as i64 - side / 2];
    if kept.cell != cell || kept.origin != origin || kept.cells.len() != FLOW_CELLS * FLOW_CELLS {
        (kept.anchor, kept.cell, kept.origin) = (None, cell, origin);
        kept.cells = vec![None; FLOW_CELLS * FLOW_CELLS];
    }
    if let Some(old) = kept.anchor.filter(|old| *old != anchor) {
        let mut moved = vec![None; FLOW_CELLS * FLOW_CELLS];
        for j in 0..side {
            for i in 0..side {
                let (oi, oj) = (anchor[0] + i - old[0], anchor[1] + j - old[1]);
                if (0..side).contains(&oi) && (0..side).contains(&oj) {
                    moved[(j * side + i) as usize] = kept.cells[(oj * side + oi) as usize];
                }
            }
        }
        kept.cells = moved;
    }
    kept.anchor = Some(anchor);
    let middle = |index: usize| {
        let (i, j) = ((index % FLOW_CELLS) as i64, (index / FLOW_CELLS) as i64);
        Vec3::new(((anchor[0] + i) as f32 + 0.5) * cell, level - 0.2, ((anchor[1] + j) as f32 + 0.5) * cell)
    };
    let current = |index: usize| {
        let flow = crate::water::current(world, middle(index));
        Some([flow.x, flow.z])
    };
    let mut done = 0;
    for index in 0..kept.cells.len() {
        if done >= FLOW_NEW {
            break;
        }
        if kept.cells[index].is_none() {
            kept.cells[index] = current(index);
            done += 1;
        }
    }
    for _ in 0..FLOW_AGAIN {
        let index = kept.next;
        kept.next = (kept.next + 1) % kept.cells.len();
        kept.cells[index] = current(index);
    }
    TrailFlow {
        corner: [anchor[0] as f32 * cell, anchor[1] as f32 * cell],
        cell,
        cells: kept.cells.iter().map(|flow| flow.unwrap_or([0.0; 2])).collect(),
    }
}

/// Sets what answers `WaterWake` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::WaterWake, values);
}

// What `values` reads or writes, as `WaterWake` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const SIZE: i32 = 3;
const STRENGTH: i32 = 4;
const FOAM: i32 = 5;
const ENABLED: i32 = 6;

/// Reads value `what` of `entity`'s Water Wake, or with `written`, writes
/// it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<WaterWake>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<WaterWake>(entity) {
                world.insert(entity, WaterWake::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<WaterWake>(entity).map(|_| [0.0; 4]),
        (ENABLED, None) if world.has::<WaterWake>(entity) => {
            let on = voxear_scene::is_component_on(world, entity, Switch::WaterWake);
            return Some(number(f32::from(u8::from(on))));
        }
        (ENABLED, Some([x, ..])) if world.has::<WaterWake>(entity) => {
            voxear_scene::set_component_on(world, entity, Switch::WaterWake, x != 0.0);
            return Some([0.0; 4]);
        }
        _ => {}
    }
    let wake = world.get_mut::<WaterWake>(entity)?;
    match written {
        None => Some(number(match what {
            SIZE => wake.size,
            STRENGTH => wake.strength,
            FOAM => wake.foam,
            _ => return None,
        })),
        Some([x, ..]) => {
            match what {
                SIZE => wake.size = x.clamp(0.0, 100.0),
                STRENGTH => wake.strength = x.clamp(0.0, 10.0),
                FOAM => wake.foam = x.clamp(0.0, 10.0),
                _ => return None,
            }
            Some([0.0; 4])
        }
    }
}

#[cfg(test)]
mod tests {
    use voxear_physics::scene::ColliderShape;
    use voxear_render::Pool;

    use super::*;

    /// A world with a Pool of still water 10 m across, its top at 0.
    fn pond() -> World {
        let mut world = World::new();
        let pool = Pool { size: [10.0, 10.0], depth: 4.0, ..Pool::default() };
        world.spawn((pool, Transform::from_position(Vec3::ZERO)));
        world.insert_resource(crate::terrain::TerrainViewer(Vec3::new(0.0, 2.0, 0.0)));
        world
    }

    #[test]
    fn a_swimmer_and_a_boat_stir_the_water_and_what_stands_clear_of_it_does_not() {
        let mut world = pond();
        let swimmer =
            world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(1.0, -1.2, 0.0))));
        let walker = world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(2.0, 4.0, 0.0))));
        let hull = Collider { shape: ColliderShape::Box, size: [1.0, 0.6, 4.0], ..Collider::default() };
        let boat = world.spawn((hull, Rigidbody::default(), Transform::from_position(Vec3::new(-2.0, 0.1, 0.0))));
        update(&mut world);
        let stirs = &world.resource::<Stirs>().expect("noted").0;
        let of = |entity: Entity| stirs.iter().find(|stir| stir.entity == entity);
        assert!(of(walker).is_none(), "a character up on the bank stirs nothing");
        let swimming = of(swimmer).expect("the swimmer");
        assert_eq!(swimming.surface, 0.0);
        assert!(swimming.bob > 0.9, "treading water: {}", swimming.bob);
        let sailing = of(boat).expect("the boat");
        assert!((sailing.radius - 0.5).abs() < 1e-4, "half as wide as its hull: {}", sailing.radius);
        assert!((sailing.ends[0].distance(sailing.ends[1]) - 3.0).abs() < 1e-3, "end to end along its length");
        assert_eq!(sailing.bob, 0.0);
        assert_eq!(stirs[0].entity, swimmer, "the nearest first");
    }

    #[test]
    fn a_water_wake_tunes_a_body_gives_one_without_a_body_its_own_and_switched_off_stops_it() {
        let mut world = pond();
        let wide = WaterWake { size: 3.0, strength: 0.5, foam: 0.0 };
        let raft =
            world.spawn((CharacterController::default(), wide, Transform::from_position(Vec3::new(1.0, -0.5, 0.0))));
        let float = world.spawn((WaterWake::default(), Transform::from_position(Vec3::new(-1.0, 0.0, 1.0))));
        let quiet = world.spawn((
            CharacterController::default(),
            WaterWake::default(),
            Transform::from_position(Vec3::new(0.0, -0.5, 2.0)),
        ));
        voxear_scene::set_component_on(&mut world, quiet, Switch::WaterWake, false);
        update(&mut world);
        let stirs = &world.resource::<Stirs>().expect("noted").0;
        let of = |entity: Entity| stirs.iter().find(|stir| stir.entity == entity);
        let tuned = of(raft).expect("tuned");
        assert_eq!((tuned.radius, tuned.strength, tuned.foam), (1.5, 0.5, 0.0));
        assert_eq!(of(float).expect("a wake of its own").radius, BARE_WAKE * 0.5);
        assert!(of(quiet).is_none(), "switched off");
    }

    #[test]
    fn scripts_add_tune_and_switch_off_a_water_wake() {
        let mut world = World::new();
        let obj = world.spawn(Transform::default());
        assert_eq!(values(&mut world, obj, HAS, None), Some([0.0; 4]));
        assert_eq!(values(&mut world, obj, SIZE, None), None, "none to read");
        values(&mut world, obj, ADD, Some([0.0; 4]));
        values(&mut world, obj, SIZE, Some([2.5, 0.0, 0.0, 0.0]));
        values(&mut world, obj, FOAM, Some([-1.0, 0.0, 0.0, 0.0]));
        assert_eq!(*world.get::<WaterWake>(obj).expect("added"), WaterWake { size: 2.5, strength: 1.0, foam: 0.0 });
        assert_eq!(values(&mut world, obj, ENABLED, None), Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, ENABLED, Some([0.0; 4]));
        assert!(!voxear_scene::is_component_on(&world, obj, Switch::WaterWake));
        assert!(values(&mut world, obj, REMOVE, Some([0.0; 4])).is_some());
        assert!(!world.has::<WaterWake>(obj));
    }

    #[test]
    fn trails_off_note_nothing() {
        let mut world = pond();
        world.spawn((CharacterController::default(), Transform::from_position(Vec3::new(1.0, -1.2, 0.0))));
        world.insert_resource(WaterTrails { quality: TrailQuality::Off, ..WaterTrails::default() });
        update(&mut world);
        assert!(world.resource::<Stirs>().is_none());
    }

    #[test]
    fn the_current_is_kept_round_the_viewer_and_moves_with_it() {
        let world = pond();
        let mut kept = CurrentKept::default();
        let first = follow_current(&world, &mut kept, Vec3::ZERO, 4.0, 0.0);
        assert_eq!(first.corner, [-32.0, -32.0]);
        assert_eq!(first.cells.len(), FLOW_CELLS * FLOW_CELLS);
        assert!(first.cells.iter().all(|cell| *cell == [0.0; 2]), "a Pool lies still");
        let known = kept.cells.iter().filter(|cell| cell.is_some()).count();
        assert_eq!(known, FLOW_NEW, "a few cells a frame");
        let moved = follow_current(&world, &mut kept, Vec3::new(8.0, 0.0, 0.0), 4.0, 0.0);
        assert_eq!(moved.corner, [-24.0, -32.0]);
    }
}
