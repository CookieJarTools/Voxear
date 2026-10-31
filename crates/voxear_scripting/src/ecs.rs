//! The ECS from C#: components registered by name and layout, chunk queries
//! that hand scripts pointers into chunk memory, and entity commands.
//!
//! C# components are `unmanaged` structs living in the same chunks as Rust
//! components, so a system reads and writes them through spans with no
//! copies. Scripts may read any component that is plain data; they may
//! write their own and `Transform`, and add and remove only their own, since
//! the engine's other components keep invariants scripts cannot see.
//!
//! Structural changes (adding or removing a component, destroying an
//! entity) move rows between chunks, which would leave a span pointing at
//! the wrong row. So while a system runs, or a query is being walked, they
//! wait in a queue and apply once it is done: see [`commands_defer`].

use std::{
    alloc::Layout,
    cell::{Cell, RefCell},
    slice,
    sync::atomic::AtomicU64,
};

use voxear_ecs::{ComponentId, Entity, World};
use voxear_scene::{Behaviours, Destroying, EditorOnly, Name, Transform};

use crate::interop::{push_obj_destroy, utf8, with_world};

/// The id of the world lent to scripts, which they read straight from here
/// to tell when the component ids they keep belong to a world now gone.
pub(crate) static CURRENT_WORLD: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// How deep the systems and query walks under way go; commands wait
    /// while it is above zero.
    static DEFER: Cell<u32> = const { Cell::new(0) };
    static COMMANDS: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
}

/// A structural change waiting for the systems and walks under way.
enum Command {
    Add(Entity, ComponentId, Vec<u8>),
    Remove(Entity, ComponentId),
    Destroy(Entity),
}

/// What a C# query asks for: the components it hands out, in order, with
/// its filters. Mirrors `QueryDesc` in Voxear.Core.
#[repr(C)]
pub(crate) struct QueryDesc {
    pub fetch: *const u32,
    pub fetch_count: i32,
    pub with: *const u32,
    pub with_count: i32,
    pub without: *const u32,
    pub without_count: i32,
    pub changed: *const u32,
    pub changed_count: i32,
    /// For `changed`: only chunks written after this tick.
    pub since: u32,
}

/// Slots per chunk in what [`query_chunks`] writes, before the columns:
/// where the chunk is (archetype in the high half, chunk in the low), how
/// many rows it has, and its entities.
const CHUNK_HEADER: usize = 3;

/// The id of the component of this name, registered with this layout when
/// it is new, or -1 when it cannot be used from C#.
pub(crate) unsafe extern "C" fn component_register(name: *const u8, len: i32, size: i32, align: i32) -> i32 {
    let name = unsafe { utf8(name, len) }.into_owned();
    let Ok(layout) = Layout::from_size_align(size.max(0) as usize, align.max(1) as usize) else {
        tracing::error!(target: "script", "{name}: a size of {size} and alignment of {align} is not a layout");
        return -1;
    };
    with_world(|world| {
        let Some(id) = world.components().id_by_name(&name) else {
            return Some(world.register_dynamic_component(name, layout).index() as i32);
        };
        let info = world.components().info(id);
        let engine_own = info.type_id().is_some();
        let fits = if engine_own {
            !info.needs_drop() && info.layout().size() == layout.size() && info.layout().align() >= layout.align()
        } else {
            info.layout() == layout
        };
        if fits {
            return Some(id.index() as i32);
        }
        if engine_own {
            tracing::error!(target: "script", "{name} is the engine's own, and its C# struct does not match it");
        } else {
            tracing::error!(
                target: "script",
                "{name} changed its fields while the game ran, so it cannot be used until play mode starts again"
            );
        }
        Some(-1)
    })
    .unwrap_or(-1)
}

/// Writes the chunks a query matches, as many as fit in `capacity`, and
/// returns how many there are, or -1 when the query is not valid. Each
/// takes `3 + fetch_count` slots: its place, its row count, its entities,
/// then a pointer to each fetched component's column.
pub(crate) unsafe extern "C" fn query_chunks(query: *const QueryDesc, out: *mut isize, capacity: i32) -> i32 {
    let Some(query) = (unsafe { query.as_ref() }) else { return -1 };
    with_world(|world| {
        let count = world.components().len();
        let ids = |ptr: *const u32, len: i32| -> Option<Vec<ComponentId>> {
            if len <= 0 || ptr.is_null() {
                return Some(Vec::new());
            }
            let raw = unsafe { slice::from_raw_parts(ptr, len as usize) };
            raw.iter().map(|&id| ((id as usize) < count).then_some(ComponentId::from_index(id))).collect()
        };
        let (Some(fetch), Some(with), Some(without), Some(changed)) = (
            ids(query.fetch, query.fetch_count),
            ids(query.with, query.with_count),
            ids(query.without, query.without_count),
            ids(query.changed, query.changed_count),
        ) else {
            return Some(-1);
        };
        // The editor's own Objs, and Objs on their way out, are never seen.
        let hidden = [world.components().id_of::<EditorOnly>(), world.components().id_of::<Destroying>()];
        let stride = CHUNK_HEADER + fetch.len();
        let mut found = 0usize;
        for (index, archetype) in world.archetypes().iter().enumerate() {
            let has_all = fetch.iter().chain(&with).chain(&changed).all(|&id| archetype.has(id));
            let has_any = without.iter().chain(hidden.iter().flatten()).any(|&id| archetype.has(id));
            if !has_all || has_any {
                continue;
            }
            for chunk in 0..archetype.chunk_count() {
                let fresh = |&id: &ComponentId| archetype.chunk_version(chunk, id).is_some_and(|v| v > query.since);
                if !changed.iter().all(fresh) {
                    continue;
                }
                if !out.is_null() && found < capacity.max(0) as usize {
                    let row = unsafe { slice::from_raw_parts_mut(out.add(found * stride), stride) };
                    row[0] = ((index as u64) << 32 | chunk as u64) as isize;
                    row[1] = archetype.chunk_len(chunk) as isize;
                    row[2] = archetype.chunk_entities(chunk).as_ptr() as isize;
                    for (slot, &id) in row[CHUNK_HEADER..].iter_mut().zip(&fetch) {
                        *slot = archetype.column_ptr(chunk, id).map_or(0, |p| p.as_ptr() as isize);
                    }
                }
                found += 1;
            }
        }
        Some(found as i32)
    })
    .unwrap_or(-1)
}

/// Stamps a fetched component in a chunk as written now, as `Write<T>`
/// does before handing out its span, so `Changed` filters see it.
pub(crate) unsafe extern "C" fn chunk_written(place: u64, component: u32) {
    let (archetype, chunk) = ((place >> 32) as usize, (place & 0xffff_ffff) as usize);
    with_world(|world| {
        let tick = world.change_tick();
        let archetype = world.archetypes().get(archetype)?;
        archetype.mark_written(chunk, ComponentId::from_index(component), tick);
        Some(())
    });
}

/// A new entity with no components. Made at once, even while commands
/// wait, since an empty entity moves no other entity's row.
pub(crate) unsafe extern "C" fn entity_create() -> u64 {
    with_world(|world| Some(world.spawn_empty().to_bits())).unwrap_or(0)
}

/// Destroys an entity, or an Obj as `Obj.Destroy` does, with its children
/// and behaviours.
pub(crate) unsafe extern "C" fn entity_destroy(entity: u64) {
    let entity = Entity::from_bits(entity);
    let is_obj = with_world(|world| Some(world.has::<Name>(entity) || world.has::<Behaviours>(entity)));
    match is_obj {
        Some(true) => push_obj_destroy(entity),
        Some(false) => run(Command::Destroy(entity)),
        None => {}
    }
}

pub(crate) unsafe extern "C" fn entity_alive(entity: u64) -> i32 {
    with_world(|world| Some(i32::from(world.is_alive(Entity::from_bits(entity))))).unwrap_or(0)
}

pub(crate) unsafe extern "C" fn component_has(entity: u64, component: u32) -> i32 {
    with_world(|world| {
        let id = valid(world, component)?;
        Some(i32::from(world.has_id(Entity::from_bits(entity), id)))
    })
    .unwrap_or(0)
}

/// Copies a component's value out: 1 when it did, 0 when the entity has
/// none, and -1 when the component cannot be read this way.
pub(crate) unsafe extern "C" fn component_get(entity: u64, component: u32, out: *mut u8, size: i32) -> i32 {
    with_world(|world| {
        let id = sized(world, component, size)?;
        let Some(value) = world.get_by_id(Entity::from_bits(entity), id) else { return Some(0) };
        unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), out, size as usize) };
        Some(1)
    })
    .unwrap_or(-1)
}

/// Writes a component's value in place: 1 when it did, 0 when the entity
/// has none, and -1 when scripts cannot write it.
pub(crate) unsafe extern "C" fn component_set(entity: u64, component: u32, value: *const u8, size: i32) -> i32 {
    with_world(|world| {
        let id = sized(world, component, size)?;
        if !writable(world, id) {
            tracing::error!(target: "script", "{} is the engine's own; scripts can read it but not write it", name(world, id));
            return Some(-1);
        }
        let Some(slot) = world.get_mut_by_id(Entity::from_bits(entity), id) else { return Some(0) };
        unsafe { std::ptr::copy_nonoverlapping(value, slot.as_ptr(), size as usize) };
        Some(1)
    })
    .unwrap_or(-1)
}

/// Adds a component, or replaces its value: 1 when done or waiting to be,
/// and -1 when scripts cannot add it.
pub(crate) unsafe extern "C" fn component_add(entity: u64, component: u32, value: *const u8, size: i32) -> i32 {
    let entity = Entity::from_bits(entity);
    let id = with_world(|world| {
        let id = sized(world, component, size)?;
        if !scripts_own(world, id) {
            tracing::error!(target: "script", "{} is the engine's own; scripts cannot add it", name(world, id));
            return None;
        }
        Some(id)
    });
    let Some(id) = id else { return -1 };
    let bytes = unsafe { slice::from_raw_parts(value, size as usize) }.to_vec();
    run(Command::Add(entity, id, bytes));
    1
}

/// Removes a component: 1 when done or waiting to be, and -1 when scripts
/// cannot remove it.
pub(crate) unsafe extern "C" fn component_remove(entity: u64, component: u32) -> i32 {
    let entity = Entity::from_bits(entity);
    let id = with_world(|world| {
        let id = valid(world, component)?;
        if !scripts_own(world, id) {
            tracing::error!(target: "script", "{} is the engine's own; scripts cannot remove it", name(world, id));
            return None;
        }
        Some(id)
    });
    let Some(id) = id else { return -1 };
    run(Command::Remove(entity, id));
    1
}

/// `begin` 1: commands wait from now on. 0: one wait ends, and when none is
/// left, the commands waiting apply in the order they came.
pub(crate) unsafe extern "C" fn commands_defer(begin: i32) {
    if begin != 0 {
        DEFER.set(DEFER.get() + 1);
        return;
    }
    let depth = DEFER.get().saturating_sub(1);
    DEFER.set(depth);
    if depth == 0 {
        apply_commands();
    }
}

/// The world's change tick, which writes are stamped with; with `advance`,
/// a new tick starts after it is read, as after each system.
pub(crate) unsafe extern "C" fn change_tick(advance: i32) -> u32 {
    with_world(|world| {
        let tick = world.change_tick();
        if advance != 0 {
            world.advance_tick();
        }
        Some(tick)
    })
    .unwrap_or(0)
}

/// Ends every wait, as after the systems ran even when one forgot to end
/// its own, and applies what waited.
pub(crate) fn settle_commands() {
    DEFER.set(0);
    apply_commands();
}

fn run(command: Command) {
    if DEFER.get() > 0 {
        COMMANDS.with_borrow_mut(|commands| commands.push(command));
        return;
    }
    with_world(|world| {
        apply(world, command);
        Some(())
    });
}

fn apply_commands() {
    let commands = COMMANDS.take();
    if commands.is_empty() {
        return;
    }
    with_world(|world| {
        for command in commands {
            apply(world, command);
        }
        Some(())
    });
}

fn apply(world: &mut World, command: Command) {
    match command {
        // The bytes came from a C# struct of the component's own layout.
        Command::Add(entity, id, bytes) => _ = unsafe { world.insert_by_id(entity, id, bytes.as_ptr()) },
        Command::Remove(entity, id) => _ = world.remove_by_id(entity, id),
        Command::Destroy(entity) => _ = world.despawn(entity),
    }
}

fn valid(world: &World, component: u32) -> Option<ComponentId> {
    ((component as usize) < world.components().len()).then_some(ComponentId::from_index(component))
}

/// The component, when it is plain data of `size` bytes.
fn sized(world: &World, component: u32, size: i32) -> Option<ComponentId> {
    let id = valid(world, component)?;
    let info = world.components().info(id);
    (!info.needs_drop() && info.layout().size() == size.max(0) as usize).then_some(id)
}

/// Components scripts made, which they may add and remove.
fn scripts_own(world: &World, id: ComponentId) -> bool {
    world.components().info(id).type_id().is_none()
}

/// What scripts may write: their own components, and the Transform.
fn writable(world: &World, id: ComponentId) -> bool {
    scripts_own(world, id) || world.components().id_of::<Transform>() == Some(id)
}

fn name(world: &World, id: ComponentId) -> String {
    world.components().info(id).name().to_owned()
}
