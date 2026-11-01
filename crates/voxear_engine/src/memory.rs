//! A snapshot of the engine's memory, part by part, for the memory profiler
//! (see `voxear_core::memory`): the process as the system counts it, Rust's
//! heap, the ECS's chunks by archetype, each GPU allocation by name with the
//! blocks' unused room, and the managed heap.

use voxear_core::memory::{self, Snapshot};
use voxear_ecs::CHUNK_SIZE;
use voxear_render::Renderer;

use crate::Engine;

/// Memory now, labelled `label`.
pub fn snapshot(label: &str, engine: &Engine, renderer: Option<&Renderer>) -> Snapshot {
    let mut snapshot = Snapshot::new(label);
    if let Some(resident) = memory::resident() {
        snapshot.add("Process", "Resident", resident);
    }
    let (heap, _) = memory::heap();
    if heap > 0 {
        snapshot.add("Native heap", "Rust", heap);
    }
    let world = &engine.world;
    for archetype in world.archetypes().iter().filter(|a| a.chunk_count() > 0) {
        let names: Vec<&str> = archetype
            .components()
            .iter()
            .map(|&id| {
                let name = world.components().info(id).name();
                let name = name.rsplit("::").next().unwrap_or(name);
                name.rsplit('.').next().unwrap_or(name)
            })
            .collect();
        snapshot.add("ECS chunks", &names.join(" · "), (archetype.chunk_count() * CHUNK_SIZE) as u64);
    }
    if let Some(renderer) = renderer {
        let gpu = renderer.gpu_memory();
        let allocated: u64 = gpu.allocations.iter().map(|(_, bytes)| bytes).sum();
        for (name, bytes) in gpu.allocations {
            snapshot.add("GPU", &name, bytes);
        }
        snapshot.add("GPU", "Unused in its blocks", gpu.capacity.saturating_sub(allocated));
    }
    if let Some(scripts) = engine.scripts() {
        let managed = scripts.managed_memory();
        snapshot.add("Managed", "In use", managed.heap);
        snapshot.add("Managed", "Committed, unused", managed.committed.saturating_sub(managed.heap));
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_scene::Transform;

    #[test]
    fn a_snapshot_counts_the_worlds_chunks() {
        let mut engine = Engine::new();
        for _ in 0..3 {
            engine.world.spawn((Transform::IDENTITY,));
        }
        let taken = snapshot("Start", &engine, None);
        let chunks: Vec<_> = taken.entries.iter().filter(|e| e.category == "ECS chunks").collect();
        assert!(chunks.iter().any(|e| e.name == "Transform" && e.bytes == CHUNK_SIZE as u64), "{chunks:?}");
        assert_eq!(taken.label, "Start");
    }
}
