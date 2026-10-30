use std::{
    alloc::Layout,
    any::{Any, TypeId},
    borrow::Cow,
    collections::HashMap,
    ptr::{self, NonNull},
};

use crate::{
    archetype::{Archetype, ArchetypeId},
    bundle::Bundle,
    component::{Component, ComponentId, ComponentRegistry},
    entity::{Entities, Entity, EntityLocation},
    query::{Query, QueryData, ReadOnlyQueryData},
};

/// Owns every entity, component and resource.
pub struct World {
    entities: Entities,
    components: ComponentRegistry,
    archetypes: Vec<Archetype>,
    archetype_index: HashMap<Box<[ComponentId]>, ArchetypeId>,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    change_tick: u32,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        let components = ComponentRegistry::default();
        let empty = Archetype::new(ArchetypeId::EMPTY, &[], &components);
        Self {
            entities: Entities::default(),
            components,
            archetypes: vec![empty],
            archetype_index: HashMap::from([(Box::default(), ArchetypeId::EMPTY)]),
            resources: HashMap::new(),
            change_tick: 1,
        }
    }

    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        let ids = self.bundle_ids::<B>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        let archetype = self.archetype_for(&sorted);
        let entity = self.entities.alloc();
        let tick = self.change_tick;

        let archetype = &mut self.archetypes[archetype.index()];
        let (chunk, row) = archetype.alloc(entity);
        unsafe {
            bundle.write(&mut |index, src| {
                let column = archetype.column_index(ids[index]).expect("bundle column exists");
                let dst = archetype.ptr(chunk as usize, column, row as usize);
                ptr::copy_nonoverlapping(src, dst, archetype.column_size(column));
            });
        }
        archetype.mark_all_changed(chunk as usize, tick);

        let location = EntityLocation { archetype: archetype.id(), chunk, row };
        self.entities.set_location(entity.index(), location);
        entity
    }

    pub fn spawn_empty(&mut self) -> Entity {
        self.spawn(())
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(location) = self.entities.free(entity) else { return false };
        let archetype = &mut self.archetypes[location.archetype.index()];
        let moved = unsafe { archetype.swap_remove(location.chunk as usize, location.row as usize, true, self.change_tick) };
        if let Some(moved) = moved {
            self.entities.set_location(moved.index(), location);
        }
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.location(entity).is_some()
    }

    pub fn entity_count(&self) -> u32 {
        self.entities.len()
    }

    /// Adds the bundle's components, replacing any the entity already has.
    /// Returns `false` (and drops the bundle) if the entity is dead.
    pub fn insert<B: Bundle>(&mut self, entity: Entity, bundle: B) -> bool {
        let Some(location) = self.entities.location(entity) else { return false };
        let ids = self.bundle_ids::<B>();
        let source = &self.archetypes[location.archetype.index()];
        let existed: Vec<bool> = ids.iter().map(|&id| source.has(id)).collect();

        let location = if existed.iter().all(|&e| e) {
            location
        } else {
            let mut target: Vec<_> = source.components().to_vec();
            target.extend(ids.iter().zip(&existed).filter(|(_, e)| !**e).map(|(id, _)| *id));
            target.sort_unstable();
            let target = self.archetype_for(&target);
            self.move_entity(entity, location, target, true)
        };

        let tick = self.change_tick;
        let archetype = &mut self.archetypes[location.archetype.index()];
        let (chunk, row) = (location.chunk as usize, location.row as usize);
        unsafe {
            bundle.write(&mut |index, src| {
                let column = archetype.column_index(ids[index]).expect("bundle column exists");
                let dst = archetype.ptr(chunk, column, row);
                if existed[index] && let Some(drop_fn) = archetype.column_drop(column) {
                    drop_fn(dst);
                }
                ptr::copy_nonoverlapping(src, dst, archetype.column_size(column));
                archetype.mark_changed(chunk, column, tick);
            });
        }
        true
    }

    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        let id = self.components.id_of::<T>()?;
        let location = self.entities.location(entity)?;
        let archetype = &self.archetypes[location.archetype.index()];
        let column = archetype.column_index(id)?;
        let value = unsafe { archetype.ptr(location.chunk as usize, column, location.row as usize).cast::<T>().read() };
        let target = self.archetype_without(location.archetype, id);
        self.move_entity(entity, location, target, false);
        Some(value)
    }

    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.components.id_of::<T>().is_some_and(|id| self.has_id(entity, id))
    }

    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let ptr = self.get_by_id(entity, self.components.id_of::<T>()?)?;
        Some(unsafe { ptr.cast::<T>().as_ref() })
    }

    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let ptr = self.get_mut_by_id(entity, self.components.id_of::<T>()?)?;
        Some(unsafe { ptr.cast::<T>().as_mut() })
    }

    /// Iterates entities matching `Q`, e.g. `world.query::<(&mut Position, &Velocity)>()`.
    pub fn query<Q: QueryData>(&mut self) -> Query<'_, Q> {
        let state = Q::init(&mut self.components);
        Query::new(self, Some(state))
    }

    /// Read-only query that only needs `&World`.
    pub fn query_ref<Q: ReadOnlyQueryData>(&self) -> Query<'_, Q> {
        Query::new(self, Q::lookup(&self.components))
    }

    pub fn components(&self) -> &ComponentRegistry {
        &self.components
    }

    pub fn register_component<T: Component>(&mut self) -> ComponentId {
        self.components.register::<T>()
    }

    pub fn register_named_component<T: Component>(&mut self, name: impl Into<Cow<'static, str>>) -> ComponentId {
        self.components.register_named::<T>(name)
    }

    /// See [`ComponentRegistry::register_dynamic`].
    pub fn register_dynamic_component(&mut self, name: impl Into<Cow<'static, str>>, layout: Layout) -> ComponentId {
        self.components.register_dynamic(name, layout)
    }

    pub fn archetypes(&self) -> &[Archetype] {
        &self.archetypes
    }

    pub fn has_id(&self, entity: Entity, id: ComponentId) -> bool {
        self.entities.location(entity).is_some_and(|l| self.archetypes[l.archetype.index()].has(id))
    }

    pub fn get_by_id(&self, entity: Entity, id: ComponentId) -> Option<NonNull<u8>> {
        let location = self.entities.location(entity)?;
        let archetype = &self.archetypes[location.archetype.index()];
        let column = archetype.column_index(id)?;
        NonNull::new(unsafe { archetype.ptr(location.chunk as usize, column, location.row as usize) })
    }

    pub fn get_mut_by_id(&mut self, entity: Entity, id: ComponentId) -> Option<NonNull<u8>> {
        let location = self.entities.location(entity)?;
        let archetype = &self.archetypes[location.archetype.index()];
        let column = archetype.column_index(id)?;
        archetype.mark_changed(location.chunk as usize, column, self.change_tick);
        NonNull::new(unsafe { archetype.ptr(location.chunk as usize, column, location.row as usize) })
    }

    /// Inserts a component from raw bytes, replacing an existing value.
    ///
    /// # Safety
    /// `src` must point to a valid value of the component's type; ownership
    /// moves into the world.
    pub unsafe fn insert_by_id(&mut self, entity: Entity, id: ComponentId, src: *const u8) -> bool {
        let Some(mut location) = self.entities.location(entity) else { return false };
        let existed = self.archetypes[location.archetype.index()].has(id);
        if !existed {
            let mut target = self.archetypes[location.archetype.index()].components().to_vec();
            target.push(id);
            target.sort_unstable();
            let target = self.archetype_for(&target);
            location = self.move_entity(entity, location, target, true);
        }
        let archetype = &self.archetypes[location.archetype.index()];
        let column = archetype.column_index(id).expect("column exists after insert");
        unsafe {
            let dst = archetype.ptr(location.chunk as usize, column, location.row as usize);
            if existed && let Some(drop_fn) = archetype.column_drop(column) {
                drop_fn(dst);
            }
            ptr::copy_nonoverlapping(src, dst, archetype.column_size(column));
        }
        archetype.mark_changed(location.chunk as usize, column, self.change_tick);
        true
    }

    /// Removes and drops a component.
    pub fn remove_by_id(&mut self, entity: Entity, id: ComponentId) -> bool {
        let Some(location) = self.entities.location(entity) else { return false };
        if !self.archetypes[location.archetype.index()].has(id) {
            return false;
        }
        let target = self.archetype_without(location.archetype, id);
        self.move_entity(entity, location, target, true);
        true
    }

    pub fn insert_resource<R: Send + Sync + 'static>(&mut self, resource: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(resource));
    }

    pub fn resource<R: Send + Sync + 'static>(&self) -> Option<&R> {
        self.resources.get(&TypeId::of::<R>())?.downcast_ref()
    }

    pub fn resource_mut<R: Send + Sync + 'static>(&mut self) -> Option<&mut R> {
        self.resources.get_mut(&TypeId::of::<R>())?.downcast_mut()
    }

    pub fn remove_resource<R: Send + Sync + 'static>(&mut self) -> Option<R> {
        self.resources.remove(&TypeId::of::<R>())?.downcast().ok().map(|r| *r)
    }

    /// The tick stamped on chunks written from now on.
    pub fn change_tick(&self) -> u32 {
        self.change_tick
    }

    /// Starts a new change tick; the schedule calls this after each system so
    /// change filters can tell which writes came after a given point.
    pub fn advance_tick(&mut self) -> u32 {
        self.change_tick = self.change_tick.wrapping_add(1).max(1);
        self.change_tick
    }

    fn bundle_ids<B: Bundle>(&mut self) -> Vec<ComponentId> {
        let mut ids = Vec::new();
        B::component_ids(&mut self.components, &mut ids);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        if let Some(pair) = sorted.windows(2).find(|w| w[0] == w[1]) {
            panic!("bundle contains `{}` more than once", self.components.info(pair[0]).name());
        }
        ids
    }

    fn archetype_for(&mut self, sorted: &[ComponentId]) -> ArchetypeId {
        if let Some(&id) = self.archetype_index.get(sorted) {
            return id;
        }
        let id = ArchetypeId(u32::try_from(self.archetypes.len()).expect("too many archetypes"));
        self.archetypes.push(Archetype::new(id, sorted, &self.components));
        self.archetype_index.insert(sorted.into(), id);
        id
    }

    fn archetype_without(&mut self, archetype: ArchetypeId, removed: ComponentId) -> ArchetypeId {
        let target: Vec<_> =
            self.archetypes[archetype.index()].components().iter().copied().filter(|&c| c != removed).collect();
        self.archetype_for(&target)
    }

    /// Moves an entity's row to another archetype, copying shared columns.
    /// Columns missing from the target are dropped when `drop_removed`,
    /// otherwise the caller has already moved them out. Columns new in the
    /// target are left uninitialised for the caller to write.
    fn move_entity(&mut self, entity: Entity, from: EntityLocation, to: ArchetypeId, drop_removed: bool) -> EntityLocation {
        debug_assert_ne!(from.archetype, to);
        let tick = self.change_tick;
        let (source, target) = pair_mut(&mut self.archetypes, from.archetype.index(), to.index());
        let (chunk, row) = target.alloc(entity);
        let (src_chunk, src_row) = (from.chunk as usize, from.row as usize);

        for column in 0..source.components().len() {
            let id = source.components()[column];
            unsafe {
                let src = source.ptr(src_chunk, column, src_row);
                match target.column_index(id) {
                    Some(dst_column) => {
                        let dst = target.ptr(chunk as usize, dst_column, row as usize);
                        ptr::copy_nonoverlapping(src, dst, source.column_size(column));
                    }
                    None if drop_removed => {
                        if let Some(drop_fn) = source.column_drop(column) {
                            drop_fn(src);
                        }
                    }
                    None => {}
                }
            }
        }
        target.mark_all_changed(chunk as usize, tick);

        if let Some(moved) = unsafe { source.swap_remove(src_chunk, src_row, false, tick) } {
            self.entities.set_location(moved.index(), from);
        }
        let location = EntityLocation { archetype: to, chunk, row };
        self.entities.set_location(entity.index(), location);
        location
    }
}

fn pair_mut<T>(items: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert_ne!(a, b);
    if a < b {
        let (left, right) = items.split_at_mut(b);
        (&mut left[a], &mut right[0])
    } else {
        let (left, right) = items.split_at_mut(a);
        (&mut right[0], &mut left[b])
    }
}
