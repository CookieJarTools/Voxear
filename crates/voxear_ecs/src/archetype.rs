use std::{
    alloc::{self, Layout},
    ptr::{self, NonNull},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    component::{ComponentId, ComponentRegistry, DropFn},
    entity::Entity,
};

/// Bytes per chunk, matching Unity's DOTS. Archetypes whose single entity is
/// larger than this get bigger chunks.
pub const CHUNK_SIZE: usize = 16 * 1024;
const MIN_CHUNK_ALIGN: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct ArchetypeId(pub(crate) u32);

impl ArchetypeId {
    pub const EMPTY: ArchetypeId = ArchetypeId(0);

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug)]
struct Column {
    offset: usize,
    size: usize,
    drop: Option<DropFn>,
}

struct Chunk {
    data: NonNull<u8>,
    len: usize,
    /// Change tick of the last write access, per column.
    versions: Box<[AtomicU32]>,
}

/// All entities with exactly the same component set. Rows are packed densely:
/// every chunk except the last one is full.
pub struct Archetype {
    id: ArchetypeId,
    components: Box<[ComponentId]>,
    columns: Box<[Column]>,
    capacity: usize,
    chunk_layout: Layout,
    chunks: Vec<Chunk>,
    len: usize,
}

// Rust components are Send + Sync and runtime components are plain bytes.
unsafe impl Send for Archetype {}
unsafe impl Sync for Archetype {}

impl Archetype {
    /// `components` must be sorted and free of duplicates.
    pub(crate) fn new(id: ArchetypeId, components: &[ComponentId], registry: &ComponentRegistry) -> Self {
        debug_assert!(components.windows(2).all(|w| w[0] < w[1]));
        let infos: Vec<_> = components.iter().map(|&c| registry.info(c)).collect();

        let per_entity = size_of::<Entity>() + infos.iter().map(|i| i.layout().size()).sum::<usize>();
        let padding: usize = infos.iter().map(|i| i.layout().align()).sum();
        let align = infos.iter().map(|i| i.layout().align()).fold(MIN_CHUNK_ALIGN, usize::max);

        let mut chunk_bytes = CHUNK_SIZE;
        let mut capacity = CHUNK_SIZE.saturating_sub(padding) / per_entity;
        if capacity == 0 {
            capacity = 1;
            chunk_bytes = (per_entity + padding).next_multiple_of(align);
        }

        let mut offset = size_of::<Entity>() * capacity;
        let columns = infos
            .iter()
            .map(|info| {
                offset = offset.next_multiple_of(info.layout().align());
                let column = Column { offset, size: info.layout().size(), drop: info.drop_fn() };
                offset += column.size * capacity;
                column
            })
            .collect();
        debug_assert!(offset <= chunk_bytes);

        Self {
            id,
            components: components.into(),
            columns,
            capacity,
            chunk_layout: Layout::from_size_align(chunk_bytes, align).expect("invalid chunk layout"),
            chunks: Vec::new(),
            len: 0,
        }
    }

    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    pub fn components(&self) -> &[ComponentId] {
        &self.components
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Entities per chunk.
    pub fn chunk_capacity(&self) -> usize {
        self.capacity
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn chunk_len(&self, chunk: usize) -> usize {
        self.chunks[chunk].len
    }

    pub fn chunk_entities(&self, chunk: usize) -> &[Entity] {
        let chunk = &self.chunks[chunk];
        unsafe { std::slice::from_raw_parts(chunk.data.as_ptr().cast(), chunk.len) }
    }

    pub fn has(&self, id: ComponentId) -> bool {
        self.column_index(id).is_some()
    }

    pub fn column_index(&self, id: ComponentId) -> Option<usize> {
        self.components.binary_search(&id).ok()
    }

    /// Start of a component column inside a chunk; the column holds
    /// `chunk_len(chunk)` tightly packed values. This is what scripts read
    /// through spans without copying.
    pub fn column_ptr(&self, chunk: usize, id: ComponentId) -> Option<NonNull<u8>> {
        let column = &self.columns[self.column_index(id)?];
        let data = self.chunks.get(chunk)?.data;
        Some(unsafe { data.add(column.offset) })
    }

    /// Tick of the last write to `id` in `chunk`.
    pub fn chunk_version(&self, chunk: usize, id: ComponentId) -> Option<u32> {
        let column = self.column_index(id)?;
        Some(self.chunks.get(chunk)?.versions[column].load(Ordering::Relaxed))
    }

    /// Stamps `id` in `chunk` as written at `tick`, for code that writes
    /// through [`column_ptr`](Self::column_ptr), as scripts do.
    pub fn mark_written(&self, chunk: usize, id: ComponentId, tick: u32) {
        if let Some(column) = self.column_index(id)
            && chunk < self.chunks.len()
        {
            self.mark_changed(chunk, column, tick);
        }
    }

    pub(crate) fn mark_changed(&self, chunk: usize, column: usize, tick: u32) {
        self.chunks[chunk].versions[column].store(tick, Ordering::Relaxed);
    }

    pub(crate) fn mark_all_changed(&self, chunk: usize, tick: u32) {
        for version in &self.chunks[chunk].versions {
            version.store(tick, Ordering::Relaxed);
        }
    }

    pub(crate) fn column_size(&self, column: usize) -> usize {
        self.columns[column].size
    }

    pub(crate) fn column_drop(&self, column: usize) -> Option<DropFn> {
        self.columns[column].drop
    }

    /// # Safety
    /// `chunk` and `row` must be in bounds (`row` may equal the chunk length
    /// only while a freshly allocated row is being written).
    pub(crate) unsafe fn ptr(&self, chunk: usize, column: usize, row: usize) -> *mut u8 {
        let column = &self.columns[column];
        unsafe { self.chunks[chunk].data.as_ptr().add(column.offset + column.size * row) }
    }

    pub(crate) fn column_base(&self, chunk: usize, column: usize) -> *mut u8 {
        unsafe { self.chunks[chunk].data.as_ptr().add(self.columns[column].offset) }
    }

    pub(crate) fn entities_ptr(&self, chunk: usize) -> *const Entity {
        self.chunks[chunk].data.as_ptr().cast()
    }

    /// Reserves a row at the end of the archetype. The caller must write the
    /// entity and every column before the row is observed.
    pub(crate) fn alloc(&mut self, entity: Entity) -> (u32, u32) {
        if self.chunks.last().is_none_or(|c| c.len == self.capacity) {
            self.chunks.push(Chunk::new(self.chunk_layout, self.columns.len()));
        }
        let chunk_index = self.chunks.len() - 1;
        let chunk = &mut self.chunks[chunk_index];
        let row = chunk.len;
        unsafe { chunk.data.as_ptr().cast::<Entity>().add(row).write(entity) };
        chunk.len += 1;
        self.len += 1;
        (chunk_index as u32, row as u32)
    }

    /// Removes a row by moving the archetype's last row into it. Returns the
    /// entity that moved, whose location the caller must update.
    ///
    /// # Safety
    /// The row must be in bounds. With `drop == false` the caller must already
    /// have moved every component out of the row.
    pub(crate) unsafe fn swap_remove(&mut self, chunk: usize, row: usize, drop: bool, tick: u32) -> Option<Entity> {
        if drop {
            for column in 0..self.columns.len() {
                if let Some(drop_fn) = self.columns[column].drop {
                    unsafe { drop_fn(self.ptr(chunk, column, row)) };
                }
            }
        }

        let last_chunk = self.chunks.len() - 1;
        let last_row = self.chunks[last_chunk].len - 1;
        let moved = if (chunk, row) != (last_chunk, last_row) {
            unsafe {
                for column in 0..self.columns.len() {
                    let size = self.columns[column].size;
                    ptr::copy_nonoverlapping(
                        self.ptr(last_chunk, column, last_row),
                        self.ptr(chunk, column, row),
                        size,
                    );
                }
                let entities = self.chunks[chunk].data.as_ptr().cast::<Entity>();
                let moved = self.entities_ptr(last_chunk).add(last_row).read();
                entities.add(row).write(moved);
                self.mark_all_changed(chunk, tick);
                Some(moved)
            }
        } else {
            None
        };

        self.chunks[last_chunk].len -= 1;
        self.len -= 1;
        if self.chunks[last_chunk].len == 0 {
            let chunk = self.chunks.pop().expect("chunk exists");
            unsafe { alloc::dealloc(chunk.data.as_ptr(), self.chunk_layout) };
        }
        moved
    }
}

impl Chunk {
    fn new(layout: Layout, columns: usize) -> Self {
        let data = unsafe { alloc::alloc(layout) };
        let data = NonNull::new(data).unwrap_or_else(|| alloc::handle_alloc_error(layout));
        Self { data, len: 0, versions: (0..columns).map(|_| AtomicU32::new(0)).collect() }
    }
}

impl Drop for Archetype {
    fn drop(&mut self) {
        for chunk_index in 0..self.chunks.len() {
            for column in 0..self.columns.len() {
                let Some(drop_fn) = self.columns[column].drop else { continue };
                for row in 0..self.chunks[chunk_index].len {
                    unsafe { drop_fn(self.ptr(chunk_index, column, row)) };
                }
            }
            unsafe { alloc::dealloc(self.chunks[chunk_index].data.as_ptr(), self.chunk_layout) };
        }
    }
}
