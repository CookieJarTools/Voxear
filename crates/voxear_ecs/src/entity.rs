use std::fmt;

use crate::archetype::ArchetypeId;

/// A generational handle to an entity. Stale handles never alias a newer entity
/// that reuses the same index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    /// Never refers to a live entity. Generation 0 is never handed out.
    pub const NULL: Entity = Entity { index: u32::MAX, generation: 0 };

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub const fn is_null(self) -> bool {
        self.generation == 0
    }

    /// Packs the handle into 64 bits; this is the representation C# sees.
    pub const fn to_bits(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }

    pub const fn from_bits(bits: u64) -> Self {
        Self { index: bits as u32, generation: (bits >> 32) as u32 }
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return f.write_str("Entity(null)");
        }
        write!(f, "Entity({}v{})", self.index, self.generation)
    }
}

/// A map keyed by entities, hashed quickly: an entity is already two numbers
/// no other live entity shares, so they are only mixed, not hashed as a
/// string would be.
pub type EntityMap<V> = std::collections::HashMap<Entity, V, std::hash::BuildHasherDefault<EntityHasher>>;

/// A set of entities, hashed as [`EntityMap`] does.
pub type EntitySet = std::collections::HashSet<Entity, std::hash::BuildHasherDefault<EntityHasher>>;

/// Hashes an [`Entity`] by mixing its index and generation.
#[derive(Default)]
pub struct EntityHasher(u64);

impl std::hash::Hasher for EntityHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntityLocation {
    pub archetype: ArchetypeId,
    pub chunk: u32,
    pub row: u32,
}

struct Slot {
    generation: u32,
    location: Option<EntityLocation>,
}

#[derive(Default)]
pub(crate) struct Entities {
    slots: Vec<Slot>,
    free: Vec<u32>,
    alive: u32,
}

impl Entities {
    /// The caller must give the new entity a location before anything else
    /// looks it up.
    pub fn alloc(&mut self) -> Entity {
        self.alive += 1;
        if let Some(index) = self.free.pop() {
            return Entity { index, generation: self.slots[index as usize].generation };
        }
        let index = u32::try_from(self.slots.len()).expect("entity index space exhausted");
        assert!(index != u32::MAX, "entity index space exhausted");
        self.slots.push(Slot { generation: 1, location: None });
        Entity { index, generation: 1 }
    }

    /// Frees the handle and returns where its data lived.
    pub fn free(&mut self, entity: Entity) -> Option<EntityLocation> {
        let slot = self.slots.get_mut(entity.index as usize)?;
        if slot.generation != entity.generation {
            return None;
        }
        let location = slot.location.take()?;
        slot.generation = slot.generation.checked_add(1).unwrap_or(1);
        self.free.push(entity.index);
        self.alive -= 1;
        Some(location)
    }

    pub fn location(&self, entity: Entity) -> Option<EntityLocation> {
        let slot = self.slots.get(entity.index as usize)?;
        if slot.generation != entity.generation {
            return None;
        }
        slot.location
    }

    pub fn set_location(&mut self, index: u32, location: EntityLocation) {
        self.slots[index as usize].location = Some(location);
    }

    pub fn len(&self) -> u32 {
        self.alive
    }
}
