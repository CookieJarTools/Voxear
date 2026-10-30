//! Voxear's data-oriented core.
//!
//! An archetype ECS in the spirit of Unity DOTS: entities with the same set of
//! components share an [`Archetype`], whose data lives in fixed-size chunks
//! ([`CHUNK_SIZE`]) laid out as structure-of-arrays. Every Obj, scene and
//! engine system sits on top of this.
//!
//! Components can be Rust types ([`Component`]) or plain-old-data registered at
//! runtime by name and layout, which is how C# `unmanaged` structs share the
//! same chunk memory with zero copies.

mod archetype;
mod bundle;
mod component;
mod entity;
mod query;
mod schedule;
mod world;

pub use archetype::{Archetype, ArchetypeId, CHUNK_SIZE};
pub use bundle::Bundle;
pub use component::{Component, ComponentId, ComponentInfo, ComponentRegistry};
pub use entity::Entity;
pub use query::{Access, Query, QueryData, QueryIter, ReadOnlyQueryData};
pub use schedule::{Schedule, System};
pub use world::World;
