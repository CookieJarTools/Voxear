use std::{marker::PhantomData, slice};

use crate::{
    archetype::Archetype,
    component::{Component, ComponentId, ComponentRegistry},
    entity::Entity,
    world::World,
};

/// Read/write sets of a query, used to reject aliasing such as `(&mut A, &A)`.
#[derive(Default, Debug)]
pub struct Access {
    reads: Vec<ComponentId>,
    writes: Vec<ComponentId>,
}

impl Access {
    pub fn read(&mut self, id: ComponentId) {
        assert!(!self.writes.contains(&id), "query reads and writes the same component ({id:?})");
        self.reads.push(id);
    }

    pub fn write(&mut self, id: ComponentId) {
        assert!(
            !self.writes.contains(&id) && !self.reads.contains(&id),
            "query writes a component it also accesses elsewhere ({id:?})"
        );
        self.writes.push(id);
    }

    pub fn reads(&self) -> &[ComponentId] {
        &self.reads
    }

    pub fn writes(&self) -> &[ComponentId] {
        &self.writes
    }
}

/// Something a query can fetch per entity: `&T`, `&mut T`, `Option<&T>`,
/// [`Entity`], or a tuple of those.
///
/// # Safety
/// `access` must report every component that `fetch` touches, and `fetch`
/// must only hand out pointers into columns of a matching archetype.
pub unsafe trait QueryData {
    type State: Copy + 'static;
    type Fetch: Copy;
    type Item<'w>;
    type Slice<'w>;

    fn init(registry: &mut ComponentRegistry) -> Self::State;
    fn lookup(registry: &ComponentRegistry) -> Option<Self::State>;
    fn access(state: &Self::State, access: &mut Access);
    fn matches(state: &Self::State, archetype: &Archetype) -> bool;

    /// # Safety
    /// The archetype must match and `chunk` must be in bounds.
    unsafe fn fetch(state: &Self::State, archetype: &Archetype, chunk: usize, tick: u32) -> Self::Fetch;

    /// # Safety
    /// `row` must be in bounds of the fetched chunk, and the caller must not
    /// create aliasing mutable items.
    unsafe fn item<'w>(fetch: Self::Fetch, row: usize) -> Self::Item<'w>;

    /// # Safety
    /// `len` must be the fetched chunk's length.
    unsafe fn slice<'w>(fetch: Self::Fetch, len: usize) -> Self::Slice<'w>;
}

/// Query data that never writes, so it can run on `&World`.
///
/// # Safety
/// Implementors must only read.
pub unsafe trait ReadOnlyQueryData: QueryData {}

unsafe impl<T: Component> QueryData for &T {
    type State = ComponentId;
    type Fetch = *const T;
    type Item<'w> = &'w T;
    type Slice<'w> = &'w [T];

    fn init(registry: &mut ComponentRegistry) -> ComponentId {
        registry.register::<T>()
    }

    fn lookup(registry: &ComponentRegistry) -> Option<ComponentId> {
        registry.id_of::<T>()
    }

    fn access(state: &ComponentId, access: &mut Access) {
        access.read(*state);
    }

    fn matches(state: &ComponentId, archetype: &Archetype) -> bool {
        archetype.has(*state)
    }

    unsafe fn fetch(state: &ComponentId, archetype: &Archetype, chunk: usize, _tick: u32) -> *const T {
        let column = archetype.column_index(*state).expect("matched archetype has column");
        archetype.column_base(chunk, column).cast()
    }

    unsafe fn item<'w>(fetch: *const T, row: usize) -> &'w T {
        unsafe { &*fetch.add(row) }
    }

    unsafe fn slice<'w>(fetch: *const T, len: usize) -> &'w [T] {
        unsafe { slice::from_raw_parts(fetch, len) }
    }
}

unsafe impl<T: Component> ReadOnlyQueryData for &T {}

unsafe impl<T: Component> QueryData for &mut T {
    type State = ComponentId;
    type Fetch = *mut T;
    type Item<'w> = &'w mut T;
    type Slice<'w> = &'w mut [T];

    fn init(registry: &mut ComponentRegistry) -> ComponentId {
        registry.register::<T>()
    }

    fn lookup(registry: &ComponentRegistry) -> Option<ComponentId> {
        registry.id_of::<T>()
    }

    fn access(state: &ComponentId, access: &mut Access) {
        access.write(*state);
    }

    fn matches(state: &ComponentId, archetype: &Archetype) -> bool {
        archetype.has(*state)
    }

    unsafe fn fetch(state: &ComponentId, archetype: &Archetype, chunk: usize, tick: u32) -> *mut T {
        let column = archetype.column_index(*state).expect("matched archetype has column");
        archetype.mark_changed(chunk, column, tick);
        archetype.column_base(chunk, column).cast()
    }

    unsafe fn item<'w>(fetch: *mut T, row: usize) -> &'w mut T {
        unsafe { &mut *fetch.add(row) }
    }

    unsafe fn slice<'w>(fetch: *mut T, len: usize) -> &'w mut [T] {
        unsafe { slice::from_raw_parts_mut(fetch, len) }
    }
}

unsafe impl<T: Component> QueryData for Option<&T> {
    type State = ComponentId;
    type Fetch = *const T;
    type Item<'w> = Option<&'w T>;
    type Slice<'w> = Option<&'w [T]>;

    fn init(registry: &mut ComponentRegistry) -> ComponentId {
        registry.register::<T>()
    }

    fn lookup(registry: &ComponentRegistry) -> Option<ComponentId> {
        registry.id_of::<T>()
    }

    fn access(state: &ComponentId, access: &mut Access) {
        access.read(*state);
    }

    fn matches(_: &ComponentId, _: &Archetype) -> bool {
        true
    }

    unsafe fn fetch(state: &ComponentId, archetype: &Archetype, chunk: usize, _tick: u32) -> *const T {
        match archetype.column_index(*state) {
            Some(column) => archetype.column_base(chunk, column).cast(),
            None => std::ptr::null(),
        }
    }

    unsafe fn item<'w>(fetch: *const T, row: usize) -> Option<&'w T> {
        (!fetch.is_null()).then(|| unsafe { &*fetch.add(row) })
    }

    unsafe fn slice<'w>(fetch: *const T, len: usize) -> Option<&'w [T]> {
        (!fetch.is_null()).then(|| unsafe { slice::from_raw_parts(fetch, len) })
    }
}

unsafe impl<T: Component> ReadOnlyQueryData for Option<&T> {}

unsafe impl QueryData for Entity {
    type State = ();
    type Fetch = *const Entity;
    type Item<'w> = Entity;
    type Slice<'w> = &'w [Entity];

    fn init(_: &mut ComponentRegistry) {}

    fn lookup(_: &ComponentRegistry) -> Option<()> {
        Some(())
    }

    fn access(_: &(), _: &mut Access) {}

    fn matches(_: &(), _: &Archetype) -> bool {
        true
    }

    unsafe fn fetch(_: &(), archetype: &Archetype, chunk: usize, _tick: u32) -> *const Entity {
        archetype.entities_ptr(chunk)
    }

    unsafe fn item<'w>(fetch: *const Entity, row: usize) -> Self::Item<'w> {
        unsafe { *fetch.add(row) }
    }

    unsafe fn slice<'w>(fetch: *const Entity, len: usize) -> &'w [Entity] {
        unsafe { slice::from_raw_parts(fetch, len) }
    }
}

unsafe impl ReadOnlyQueryData for Entity {}

macro_rules! impl_query_tuple {
    ($($name:ident),+) => {
        #[allow(non_snake_case)]
        unsafe impl<$($name: QueryData),+> QueryData for ($($name,)+) {
            type State = ($($name::State,)+);
            type Fetch = ($($name::Fetch,)+);
            type Item<'w> = ($($name::Item<'w>,)+);
            type Slice<'w> = ($($name::Slice<'w>,)+);

            fn init(registry: &mut ComponentRegistry) -> Self::State {
                ($($name::init(registry),)+)
            }

            fn lookup(registry: &ComponentRegistry) -> Option<Self::State> {
                Some(($($name::lookup(registry)?,)+))
            }

            fn access(state: &Self::State, access: &mut Access) {
                let ($($name,)+) = state;
                $($name::access($name, access);)+
            }

            fn matches(state: &Self::State, archetype: &Archetype) -> bool {
                let ($($name,)+) = state;
                $($name::matches($name, archetype))&&+
            }

            unsafe fn fetch(state: &Self::State, archetype: &Archetype, chunk: usize, tick: u32) -> Self::Fetch {
                let ($($name,)+) = state;
                unsafe { ($($name::fetch($name, archetype, chunk, tick),)+) }
            }

            unsafe fn item<'w>(fetch: Self::Fetch, row: usize) -> Self::Item<'w> {
                let ($($name,)+) = fetch;
                unsafe { ($($name::item($name, row),)+) }
            }

            unsafe fn slice<'w>(fetch: Self::Fetch, len: usize) -> Self::Slice<'w> {
                let ($($name,)+) = fetch;
                unsafe { ($($name::slice($name, len),)+) }
            }
        }

        unsafe impl<$($name: ReadOnlyQueryData),+> ReadOnlyQueryData for ($($name,)+) {}
    };
}

impl_query_tuple!(A);
impl_query_tuple!(A, B);
impl_query_tuple!(A, B, C);
impl_query_tuple!(A, B, C, D);
impl_query_tuple!(A, B, C, D, E);
impl_query_tuple!(A, B, C, D, E, F);
impl_query_tuple!(A, B, C, D, E, F, G);
impl_query_tuple!(A, B, C, D, E, F, G, H);
impl_query_tuple!(A, B, C, D, E, F, G, H, I);
impl_query_tuple!(A, B, C, D, E, F, G, H, I, J);

#[derive(Clone, Default)]
struct Filter {
    with: Vec<ComponentId>,
    without: Vec<ComponentId>,
    changed: Vec<(ComponentId, u32)>,
    /// A `with` component was never registered, so nothing can match.
    impossible: bool,
}

impl Filter {
    fn matches_archetype(&self, archetype: &Archetype) -> bool {
        !self.impossible
            && self.with.iter().all(|&c| archetype.has(c))
            && !self.without.iter().any(|&c| archetype.has(c))
            && self.changed.iter().all(|&(c, _)| archetype.has(c))
    }

    fn matches_chunk(&self, archetype: &Archetype, chunk: usize) -> bool {
        self.changed.iter().all(|&(c, since)| archetype.chunk_version(chunk, c).is_some_and(|v| v > since))
    }
}

/// A view over every entity that has the components in `Q`, refined with
/// [`with`](Query::with), [`without`](Query::without) and
/// [`changed`](Query::changed).
pub struct Query<'w, Q: QueryData> {
    world: &'w World,
    state: Option<Q::State>,
    filter: Filter,
}

impl<'w, Q: QueryData> Query<'w, Q> {
    pub(crate) fn new(world: &'w World, state: Option<Q::State>) -> Self {
        if let Some(state) = &state {
            Q::access(state, &mut Access::default());
        }
        Self { world, state, filter: Filter::default() }
    }

    /// Only entities that also have `T`.
    pub fn with<T: Component>(mut self) -> Self {
        match self.world.components().id_of::<T>() {
            Some(id) => self.filter.with.push(id),
            None => self.filter.impossible = true,
        }
        self
    }

    /// Only entities that do not have `T`.
    pub fn without<T: Component>(mut self) -> Self {
        if let Some(id) = self.world.components().id_of::<T>() {
            self.filter.without.push(id);
        }
        self
    }

    /// Only chunks where `T` was written after `since` (a value from
    /// [`World::change_tick`]). Granularity is per chunk, as in DOTS.
    pub fn changed<T: Component>(mut self, since: u32) -> Self {
        match self.world.components().id_of::<T>() {
            Some(id) => self.filter.changed.push((id, since)),
            None => self.filter.impossible = true,
        }
        self
    }

    pub fn iter(&mut self) -> QueryIter<'_, Q> {
        QueryIter::new(self.world, self.state, self.filter.clone())
    }

    /// Runs `f` once per chunk with whole component slices: the fastest path,
    /// and the shape SIMD and job code wants.
    pub fn for_each_chunk(&mut self, mut f: impl FnMut(Q::Slice<'_>)) {
        let Some(state) = self.state else { return };
        let tick = self.world.change_tick();
        for archetype in self.world.archetypes() {
            if !Q::matches(&state, archetype) || !self.filter.matches_archetype(archetype) {
                continue;
            }
            for chunk in 0..archetype.chunk_count() {
                if !self.filter.matches_chunk(archetype, chunk) {
                    continue;
                }
                let fetch = unsafe { Q::fetch(&state, archetype, chunk, tick) };
                f(unsafe { Q::slice(fetch, archetype.chunk_len(chunk)) });
            }
        }
    }

    pub fn count(&self) -> usize {
        let Some(state) = self.state else { return 0 };
        let mut count = 0;
        for archetype in self.world.archetypes() {
            if !Q::matches(&state, archetype) || !self.filter.matches_archetype(archetype) {
                continue;
            }
            count += (0..archetype.chunk_count())
                .filter(|&c| self.filter.matches_chunk(archetype, c))
                .map(|c| archetype.chunk_len(c))
                .sum::<usize>();
        }
        count
    }

    /// The only match, or `None` if there are zero or several.
    pub fn single(&mut self) -> Option<Q::Item<'_>> {
        let mut iter = self.iter();
        let first = iter.next()?;
        iter.next().is_none().then_some(first)
    }
}

impl<'w, Q: QueryData> IntoIterator for Query<'w, Q> {
    type Item = Q::Item<'w>;
    type IntoIter = QueryIter<'w, Q>;

    fn into_iter(self) -> QueryIter<'w, Q> {
        QueryIter::new(self.world, self.state, self.filter)
    }
}

pub struct QueryIter<'w, Q: QueryData> {
    world: &'w World,
    state: Option<Q::State>,
    filter: Filter,
    archetype: usize,
    chunk: usize,
    row: usize,
    len: usize,
    fetch: Option<Q::Fetch>,
    _marker: PhantomData<fn() -> Q>,
}

impl<'w, Q: QueryData> QueryIter<'w, Q> {
    fn new(world: &'w World, state: Option<Q::State>, filter: Filter) -> Self {
        Self { world, state, filter, archetype: 0, chunk: 0, row: 0, len: 0, fetch: None, _marker: PhantomData }
    }

    /// Moves to the next non-empty matching chunk.
    fn advance_chunk(&mut self) -> bool {
        let Some(state) = self.state else { return false };
        let archetypes = self.world.archetypes();
        while self.archetype < archetypes.len() {
            let archetype = &archetypes[self.archetype];
            if Q::matches(&state, archetype) && self.filter.matches_archetype(archetype) {
                while self.chunk < archetype.chunk_count() {
                    let chunk = self.chunk;
                    self.chunk += 1;
                    if !self.filter.matches_chunk(archetype, chunk) {
                        continue;
                    }
                    self.fetch = Some(unsafe { Q::fetch(&state, archetype, chunk, self.world.change_tick()) });
                    self.row = 0;
                    self.len = archetype.chunk_len(chunk);
                    return true;
                }
            }
            self.archetype += 1;
            self.chunk = 0;
        }
        false
    }
}

impl<'w, Q: QueryData> Iterator for QueryIter<'w, Q> {
    type Item = Q::Item<'w>;

    fn next(&mut self) -> Option<Q::Item<'w>> {
        while self.row >= self.len {
            if !self.advance_chunk() {
                return None;
            }
        }
        let item = unsafe { Q::item(self.fetch?, self.row) };
        self.row += 1;
        Some(item)
    }
}
