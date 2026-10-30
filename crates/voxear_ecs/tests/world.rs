use std::{
    alloc::Layout,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use voxear_ecs::{CHUNK_SIZE, Component, Entity, Schedule, World};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Position(f32, f32, f32);
impl Component for Position {}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Velocity(f32, f32, f32);
impl Component for Velocity {}

#[derive(Debug, PartialEq)]
struct Name(String);
impl Component for Name {}

struct Frozen;
impl Component for Frozen {}

struct DropCounter(Arc<AtomicUsize>);
impl Component for DropCounter {}
impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn spawn_get_and_despawn() {
    let mut world = World::new();
    let a = world.spawn((Position(1.0, 2.0, 3.0), Name("a".into())));
    let b = world.spawn(Position(4.0, 5.0, 6.0));

    assert_eq!(world.get::<Position>(a), Some(&Position(1.0, 2.0, 3.0)));
    assert_eq!(world.get::<Name>(a).map(|n| n.0.as_str()), Some("a"));
    assert_eq!(world.get::<Name>(b), None);
    assert_eq!(world.entity_count(), 2);

    assert!(world.despawn(a));
    assert!(!world.despawn(a));
    assert!(!world.is_alive(a));
    assert_eq!(world.get::<Position>(b), Some(&Position(4.0, 5.0, 6.0)));

    let c = world.spawn(Position(0.0, 0.0, 0.0));
    assert_eq!(c.index(), a.index(), "index is reused");
    assert_ne!(c, a, "but the generation differs");
    assert_eq!(world.get::<Position>(a), None);
}

#[test]
fn entity_bits_round_trip() {
    let mut world = World::new();
    let e = world.spawn(Position(0.0, 0.0, 0.0));
    assert_eq!(Entity::from_bits(e.to_bits()), e);
    assert!(Entity::NULL.is_null());
}

#[test]
fn insert_and_remove_move_between_archetypes() {
    let mut world = World::new();
    let e = world.spawn(Position(1.0, 0.0, 0.0));
    world.insert(e, (Velocity(0.0, 1.0, 0.0), Name("mover".into())));
    assert_eq!(world.get::<Velocity>(e), Some(&Velocity(0.0, 1.0, 0.0)));
    assert_eq!(world.get::<Position>(e), Some(&Position(1.0, 0.0, 0.0)));

    world.insert(e, Name("renamed".into()));
    assert_eq!(world.get::<Name>(e).map(|n| n.0.as_str()), Some("renamed"));

    assert_eq!(world.remove::<Velocity>(e), Some(Velocity(0.0, 1.0, 0.0)));
    assert!(!world.has::<Velocity>(e));
    assert_eq!(world.get::<Name>(e).map(|n| n.0.as_str()), Some("renamed"));
    assert_eq!(world.remove::<Velocity>(e), None);
}

#[test]
fn many_entities_span_chunks_and_stay_consistent() {
    let mut world = World::new();
    let entities: Vec<_> = (0..10_000).map(|i| world.spawn((Position(i as f32, 0.0, 0.0), Velocity(1.0, 0.0, 0.0)))).collect();
    let archetype = world.archetypes().iter().find(|a| a.len() == 10_000).expect("archetype");
    assert!(archetype.chunk_count() > 1);
    assert!(archetype.chunk_capacity() * (size_of::<Position>() + size_of::<Velocity>()) <= CHUNK_SIZE);

    // Despawn every third entity; the swap-remove must keep every survivor intact.
    for e in entities.iter().step_by(3) {
        world.despawn(*e);
    }
    for (i, e) in entities.iter().enumerate() {
        if i % 3 == 0 {
            assert!(!world.is_alive(*e));
        } else {
            assert_eq!(world.get::<Position>(*e), Some(&Position(i as f32, 0.0, 0.0)));
        }
    }
}

#[test]
fn query_iterates_and_mutates() {
    let mut world = World::new();
    for i in 0..100 {
        world.spawn((Position(i as f32, 0.0, 0.0), Velocity(1.0, 2.0, 3.0)));
    }
    world.spawn(Position(-1.0, 0.0, 0.0));

    for (position, velocity) in world.query::<(&mut Position, &Velocity)>() {
        position.0 += velocity.0;
        position.1 += velocity.1;
    }

    let mut moved = 0;
    for (entity, position, velocity) in world.query::<(Entity, &Position, Option<&Velocity>)>() {
        assert!(world_alive_hint(entity));
        if velocity.is_some() {
            assert_eq!(position.1, 2.0);
            moved += 1;
        } else {
            assert_eq!(position.0, -1.0);
        }
    }
    assert_eq!(moved, 100);
    assert_eq!(world.query::<&Position>().count(), 101);
    assert_eq!(world.query_ref::<&Velocity>().count(), 100);
}

fn world_alive_hint(entity: Entity) -> bool {
    !entity.is_null()
}

#[test]
fn chunk_iteration_gives_whole_slices() {
    let mut world = World::new();
    for i in 0..5_000 {
        world.spawn((Position(i as f32, 0.0, 0.0), Velocity(2.0, 0.0, 0.0)));
    }
    let mut total = 0;
    world.query::<(&mut Position, &Velocity)>().for_each_chunk(|(positions, velocities)| {
        assert_eq!(positions.len(), velocities.len());
        for (p, v) in positions.iter_mut().zip(velocities) {
            p.0 += v.0;
        }
        total += positions.len();
    });
    assert_eq!(total, 5_000);
    let sum: f32 = world.query_ref::<&Position>().into_iter().map(|p| p.0).sum();
    assert_eq!(sum, (0..5_000).map(|i| i as f32 + 2.0).sum::<f32>());
}

#[test]
fn with_without_filters() {
    let mut world = World::new();
    world.spawn((Position(0.0, 0.0, 0.0), Frozen));
    world.spawn(Position(0.0, 0.0, 0.0));
    world.spawn(Position(0.0, 0.0, 0.0));
    assert_eq!(world.query::<&Position>().with::<Frozen>().count(), 1);
    assert_eq!(world.query::<&Position>().without::<Frozen>().count(), 2);
    assert_eq!(world.query::<&Position>().with::<Velocity>().count(), 0);
}

#[test]
fn changed_filter_tracks_writes_per_chunk() {
    let mut world = World::new();
    let a = world.spawn(Position(0.0, 0.0, 0.0));
    world.spawn((Position(0.0, 0.0, 0.0), Velocity(0.0, 0.0, 0.0)));
    let since = world.advance_tick();
    assert_eq!(world.query::<&Position>().changed::<Position>(since).count(), 0);

    world.advance_tick();
    world.get_mut::<Position>(a).unwrap().0 = 5.0;
    assert_eq!(world.query::<&Position>().changed::<Position>(since).count(), 1);
}

#[test]
#[should_panic(expected = "reads and writes")]
fn aliasing_query_panics() {
    let mut world = World::new();
    world.spawn(Position(0.0, 0.0, 0.0));
    let _ = world.query::<(&mut Position, &Position)>();
}

#[test]
#[should_panic(expected = "more than once")]
fn duplicate_bundle_panics() {
    let mut world = World::new();
    world.spawn((Position(0.0, 0.0, 0.0), Position(1.0, 1.0, 1.0)));
}

#[test]
fn components_are_dropped_exactly_once() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let mut world = World::new();
        let a = world.spawn((Position(0.0, 0.0, 0.0), DropCounter(drops.clone())));
        let b = world.spawn(DropCounter(drops.clone()));
        world.spawn(DropCounter(drops.clone()));

        world.despawn(a);
        assert_eq!(drops.load(Ordering::SeqCst), 1);

        world.insert(b, DropCounter(drops.clone()));
        assert_eq!(drops.load(Ordering::SeqCst), 2, "replaced value is dropped");

        world.insert(b, Velocity(0.0, 0.0, 0.0));
        assert_eq!(drops.load(Ordering::SeqCst), 2, "moving archetypes does not drop");

        let removed = world.remove::<DropCounter>(b);
        assert_eq!(drops.load(Ordering::SeqCst), 2);
        drop(removed);
        assert_eq!(drops.load(Ordering::SeqCst), 3);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 4, "world drop releases the rest");
}

#[test]
fn dynamic_components_share_chunk_memory() {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Health {
        current: f32,
        max: f32,
    }

    let mut world = World::new();
    let health = world.register_dynamic_component("Game.Health", Layout::new::<Health>());
    assert_eq!(world.register_dynamic_component("Game.Health", Layout::new::<Health>()), health);

    let e = world.spawn(Position(0.0, 0.0, 0.0));
    let value = Health { current: 50.0, max: 100.0 };
    assert!(unsafe { world.insert_by_id(e, health, (&raw const value).cast()) });

    let ptr = world.get_mut_by_id(e, health).unwrap();
    unsafe { ptr.cast::<Health>().as_mut().current = 75.0 };

    let archetype = world.archetypes().iter().find(|a| a.has(health)).unwrap();
    let column = archetype.column_ptr(0, health).unwrap();
    let span = unsafe { std::slice::from_raw_parts(column.cast::<Health>().as_ptr(), archetype.chunk_len(0)) };
    assert_eq!(span, &[Health { current: 75.0, max: 100.0 }]);

    assert!(world.remove_by_id(e, health));
    assert!(!world.has_id(e, health));
    assert_eq!(world.get::<Position>(e), Some(&Position(0.0, 0.0, 0.0)));
}

#[test]
fn zero_sized_tags_and_resources() {
    let mut world = World::new();
    let e = world.spawn((Frozen, Position(1.0, 1.0, 1.0)));
    assert!(world.has::<Frozen>(e));
    assert_eq!(world.query::<(&Frozen, &Position)>().count(), 1);

    world.insert_resource(42u32);
    *world.resource_mut::<u32>().unwrap() += 1;
    assert_eq!(world.resource::<u32>(), Some(&43));
    assert_eq!(world.remove_resource::<u32>(), Some(43));
}

#[test]
fn schedule_runs_stages_in_declared_order() {
    let mut world = World::new();
    world.insert_resource(Vec::<&'static str>::new());
    let mut schedule = Schedule::new(&["First", "Update", "Last"]);
    schedule.add_system("Last", "c", |w| w.resource_mut::<Vec<&str>>().unwrap().push("c"));
    schedule.add_system("First", "a", |w| w.resource_mut::<Vec<&str>>().unwrap().push("a"));
    schedule.add_system("Update", "b", |w| w.resource_mut::<Vec<&str>>().unwrap().push("b"));
    schedule.run(&mut world);
    assert_eq!(world.resource::<Vec<&str>>().unwrap(), &["a", "b", "c"]);
}
