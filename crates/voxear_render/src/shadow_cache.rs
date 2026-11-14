//! Local lights' shadows of what stands still, kept from frame to frame.
//!
//! Each view keeps an atlas of its own beside the local shadow atlas, laid
//! out as it is, which holds for each light casting a shadow what stands
//! still in its reach: Objs that have not moved for `STILL_FRAMES` frames,
//! and worlds of blocks' chunks. A light keeps its slot of the atlases for
//! as long as it casts a shadow, and its slot is drawn into the kept atlas
//! only when it first takes it, when it stops after moving, or when what
//! stands still in its reach changes: an Obj stops or starts moving, comes
//! or goes, or a chunk's blocks change. Each frame then draws only what
//! moves, over a copy of the kept views in the local shadow atlas, and only
//! for the lights with something moving in their reach. Shading reads the
//! copy where one was made and the kept views where none was
//! (`localShadowKept` in `shaders/lights.slang`). A light that moves is
//! drawn whole into the local shadow atlas each frame, as without a cache.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Entity;

use crate::{
    MaterialHandle, MeshHandle,
    assets::Assets,
    blocks::{ChunkDraw, DrawnBlocks},
    gpu::{
        GpuInstance, GpuLight, GpuLocalShadows, INSTANCE_IMPOSTOR, INSTANCE_LOD_SCREEN, INSTANCE_NEW,
        INSTANCE_RAYS_ONLY, INSTANCE_STILL, LOCAL_SHADOW_LIGHTS, LOCAL_SHADOW_SLOT_VIEWS, Mat4Cols, cols,
    },
    scene::{ExtractedFrame, Owner},
    shadows::{self, LocalLight},
};

/// How many frames running an Obj stands just as it stood before local
/// lights keep its shadow: what only pauses is drawn each frame still, so
/// it does not draw their kept views again each time it stops.
pub const STILL_FRAMES: u16 = 16;

/// The local shadow cull that lists only what stands still, for the kept
/// atlas (`LOCAL_CULL_STILL` in `shaders/cull.slang`).
pub const LOCAL_CULL_STILL: u32 = 1;

/// A local light that may cast a shadow this frame: its place among the
/// frame's lights, its Obj, and how its shadow is drawn.
#[derive(Clone, Copy, Debug)]
pub struct ShadowLight {
    pub index: usize,
    pub entity: Entity,
    pub light: LocalLight,
}

/// What the cache knows of an Obj's instance: how it was last drawn, how
/// many frames running it has stood so, where it reaches, and whether
/// local lights keep its shadow.
#[derive(Clone, Copy)]
struct Kept {
    model: Mat4Cols,
    mesh: u32,
    material: u32,
    flags: u32,
    lod_bias: f32,
    still: u16,
    sphere: [f32; 4],
    kept: bool,
}

/// A world of blocks as its chunks were last frame, which is kept only while
/// it does not turn.
struct KeptBlocks {
    chunks: Arc<[ChunkDraw]>,
    corner: Vec3,
    block_size: f32,
    cast_shadows: bool,
}

/// A slot of the atlases: the light that last had it, and whether its kept
/// views hold what stands still round it now.
#[derive(Clone, Copy)]
struct Slot {
    entity: Entity,
    light: LocalLight,
    drawn: bool,
    used: u64,
}

/// What a view's local shadows draw this frame: each slot's light, where it
/// is and how far it reaches; which slots keep what stands still, a bit
/// each; and which of them draw it into the kept atlas this frame.
#[derive(Clone, Debug, Default)]
pub(crate) struct Lamps {
    pub slots: Vec<Option<(Vec3, f32)>>,
    pub cached: u32,
    pub redraw: u32,
}

impl Lamps {
    pub fn is_cached(&self, slot: usize) -> bool {
        self.cached >> slot & 1 != 0
    }

    pub fn redraws(&self, slot: usize) -> bool {
        self.redraw >> slot & 1 != 0
    }
}

/// A view's cache of what stands still in its local lights' reach.
#[derive(Default)]
pub(crate) struct ShadowCache {
    objs: HashMap<Owner, Kept>,
    spare: HashMap<Owner, Kept>,
    blocks: HashMap<Entity, KeptBlocks>,
    slots: [Option<Slot>; LOCAL_SHADOW_LIGHTS],
    /// Where what stands still changed since the slots were last given out.
    changed: Vec<[f32; 4]>,
    /// Everything changed, as when a material does.
    everything: bool,
    material_writes: u64,
}

impl ShadowCache {
    /// Marks the instances that stand still, which local lights keep the
    /// shadows of, among the first `count`, and notes where what stands
    /// still changed. Only Objs' instances stand still: a Foliage's plants,
    /// skinned instances, and those whose material moves its vertices or
    /// whose LOD follows the camera are drawn each frame.
    pub fn mark_still(&mut self, extracted: &mut ExtractedFrame, count: usize, assets: &Assets) {
        voxear_core::profile_scope!("Shadow Cache");
        if assets.material_writes() != self.material_writes {
            self.material_writes = assets.material_writes();
            self.everything = true;
        }
        let mut objs = std::mem::take(&mut self.spare);
        objs.clear();
        let instances = extracted.instances.iter_mut().zip(&extracted.entities).take(count);
        for (instance, owner) in instances.filter(|(instance, owner)| may_stand_still(instance, owner, assets)) {
            let before = self.objs.get(owner);
            let same = before.filter(|kept| {
                kept.model == instance.model
                    && kept.mesh == instance.mesh
                    && kept.material == instance.material
                    && kept.flags == instance.flags & !INSTANCE_NEW
                    && kept.lod_bias == instance.lod_bias
            });
            let still = same.map_or(0, |kept| kept.still.saturating_add(1));
            let kept = still >= STILL_FRAMES;
            let sphere = match same {
                Some(kept) => kept.sphere,
                None => sphere_of(instance, assets),
            };
            let was_kept = before.is_some_and(|before| before.kept);
            if was_kept != kept {
                self.changed.push(if kept { sphere } else { before.expect("was kept").sphere });
            }
            if kept {
                instance.flags |= INSTANCE_STILL;
            }
            let flags = instance.flags & !(INSTANCE_NEW | INSTANCE_STILL);
            let (model, mesh, material, lod_bias) =
                (instance.model, instance.mesh, instance.material, instance.lod_bias);
            objs.insert(*owner, Kept { model, mesh, material, flags, lod_bias, still, sphere, kept });
        }
        // Those kept that are gone.
        let gone = self.objs.iter().filter(|(owner, kept)| kept.kept && !objs.contains_key(owner));
        self.changed.extend(gone.map(|(_, kept)| kept.sphere));
        self.spare = std::mem::replace(&mut self.objs, objs);
    }

    /// Forgets what it knew of what stands still, for a frame it does not
    /// see: everything is drawn again when lights next keep it.
    pub fn forget(&mut self) {
        self.objs.clear();
        self.blocks.clear();
        self.changed.clear();
        self.everything = true;
    }

    /// Notes where worlds of blocks' chunks changed since last frame. A
    /// world that turns, as blocks that came loose do, is drawn each frame,
    /// and one that comes, goes or moves changes everything.
    pub fn note_blocks(&mut self, drawn: &[DrawnBlocks]) {
        let mut worlds = HashMap::with_capacity(drawn.len());
        for blocks in drawn.iter().filter(|blocks| blocks.turn.is_none()) {
            let now = KeptBlocks {
                chunks: blocks.chunks.clone(),
                corner: blocks.corner,
                block_size: blocks.block_size,
                cast_shadows: blocks.cast_shadows,
            };
            match self.blocks.remove(&blocks.entity) {
                Some(before) if Arc::ptr_eq(&before.chunks, &now.chunks) && same_place(&before, &now) => {}
                Some(before) if same_place(&before, &now) => {
                    let had: HashMap<[i32; 4], &ChunkDraw> = before.chunks.iter().map(|c| (c.key, c)).collect();
                    for chunk in now.chunks.iter() {
                        if had.get(&chunk.key).is_none_or(|was| *was != chunk) {
                            self.changed.push(chunk_sphere(now.corner, chunk));
                        }
                    }
                    let has: std::collections::HashSet<[i32; 4]> = now.chunks.iter().map(|c| c.key).collect();
                    let gone = before.chunks.iter().filter(|chunk| !has.contains(&chunk.key));
                    self.changed.extend(gone.map(|chunk| chunk_sphere(before.corner, chunk)));
                }
                _ => self.everything = true,
            }
            worlds.insert(blocks.entity, now);
        }
        if !self.blocks.is_empty() {
            self.everything = true;
        }
        self.blocks = worlds;
    }

    /// Gives this frame's lights casting shadows their slots of the atlases,
    /// writes their views and reach into the frame and each its first view,
    /// and says which slots keep what stands still and which draw it again.
    /// A light keeps its slot from frame to frame; one new to the atlas
    /// takes a slot no light has this frame, the one longest unused. With
    /// `caching` off, or a kept atlas just made (`fresh`), nothing is kept
    /// from before. Returns the slots, and each one's first view and how
    /// many its light takes, none for a slot without one.
    pub fn give_slots(
        &mut self,
        lights: &[ShadowLight],
        (shadows, gpu_lights): (&mut GpuLocalShadows, &mut [GpuLight]),
        (caching, fresh, frame_index): (bool, bool, u64),
    ) -> (Lamps, Vec<(u32, u32)>) {
        let mut taken: [Option<usize>; LOCAL_SHADOW_LIGHTS] = [None; LOCAL_SHADOW_LIGHTS];
        let mut placed = vec![false; lights.len()];
        for (at, light) in lights.iter().enumerate() {
            let kept = self.slots.iter().position(|slot| slot.is_some_and(|slot| slot.entity == light.entity));
            if let Some(slot) = kept.filter(|&slot| taken[slot].is_none()) {
                taken[slot] = Some(at);
                placed[at] = true;
            }
        }
        for at in (0..lights.len()).filter(|&at| !placed[at]) {
            let free = (0..LOCAL_SHADOW_LIGHTS).filter(|&slot| taken[slot].is_none());
            let slot = free.min_by_key(|&slot| self.slots[slot].map_or(0, |slot| slot.used + 1)).expect("a slot each");
            taken[slot] = Some(at);
        }

        // What changed round a slot's light, kept or not this frame, is drawn again when it next keeps it.
        let everything = std::mem::take(&mut self.everything) || !caching || fresh;
        let changed = std::mem::take(&mut self.changed);
        for slot in self.slots.iter_mut().flatten() {
            let near = |sphere: &[f32; 4]| {
                Vec3::new(sphere[0], sphere[1], sphere[2]).distance(slot.light.position) < sphere[3] + slot.light.range
            };
            if everything || changed.iter().any(near) {
                slot.drawn = false;
            }
        }

        shadows.casters = [[0.0, 0.0, 0.0, -1.0e9]; LOCAL_SHADOW_LIGHTS];
        let count = taken.iter().rposition(Option::is_some).map_or(0, |last| last + 1);
        let mut lamps = Lamps { slots: vec![None; count], ..Lamps::default() };
        let mut views_of: Vec<(u32, u32)> = (0..count as u32).map(|slot| (slot * LOCAL_SHADOW_SLOT_VIEWS, 0)).collect();
        for (slot, at) in taken.iter().enumerate().filter_map(|(slot, at)| Some((slot, (*at)?))) {
            let ShadowLight { index, entity, light } = lights[at];
            let before = self.slots[slot];
            let same = before.is_some_and(|before| before.entity == entity && before.light == light);
            let new = before.is_none_or(|before| before.entity != entity);
            // A light that moved since last frame is drawn whole, as it may move again.
            let cached = caching && (same || new);
            let drawn = cached && same && before.is_some_and(|before| before.drawn);
            if cached {
                lamps.cached |= 1 << slot;
                if !drawn {
                    lamps.redraw |= 1 << slot;
                }
            }
            self.slots[slot] = Some(Slot { entity, light, drawn: cached, used: frame_index });
            let first = slot as u32 * LOCAL_SHADOW_SLOT_VIEWS;
            let views = shadows::local_views(&light);
            for (face, view) in views.iter().enumerate() {
                shadows.view_proj[first as usize + face] = cols(*view);
            }
            shadows.casters[slot] = [light.position.x, light.position.y, light.position.z, light.range];
            gpu_lights[index].shadow = first;
            views_of[slot] = (first, views.len() as u32);
            lamps.slots[slot] = Some((light.position, light.range));
        }
        shadows.count = count as u32;
        shadows.cached = lamps.cached;
        (lamps, views_of)
    }
}

/// Whether an instance can stand still for local lights to keep its shadow.
fn may_stand_still(instance: &GpuInstance, owner: &Owner, assets: &Assets) -> bool {
    owner.plant.is_none()
        && instance.posed == 0
        && instance.flags & (INSTANCE_RAYS_ONLY | INSTANCE_LOD_SCREEN | INSTANCE_IMPOSTOR) == 0
        && !assets.moves_vertices(MaterialHandle(instance.material))
}

/// The sphere an instance lies in, in the world.
fn sphere_of(instance: &GpuInstance, assets: &Assets) -> [f32; 4] {
    let model = Mat4::from_cols_array_2d(&instance.model);
    let (center, radius) = assets.mesh_sphere(MeshHandle(instance.mesh)).unwrap_or((Vec3::ZERO, 0.0));
    let scale =
        model.x_axis.truncate().length().max(model.y_axis.truncate().length()).max(model.z_axis.truncate().length());
    let center = model.transform_point3(center);
    [center.x, center.y, center.z, radius * scale + instance.reach]
}

/// Whether a world of blocks lies where it did, its blocks as large, and
/// casts as it did.
fn same_place(before: &KeptBlocks, now: &KeptBlocks) -> bool {
    before.corner == now.corner && before.block_size == now.block_size && before.cast_shadows == now.cast_shadows
}

/// The sphere a chunk lies in, in the world, from its world's corner.
fn chunk_sphere(corner: Vec3, chunk: &ChunkDraw) -> [f32; 4] {
    let [low, high] = chunk.bounds;
    let middle = corner + (low + high) * 0.5;
    [middle.x, middle.y, middle.z, (high - low).length() * 0.5]
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    fn light(x: f32) -> LocalLight {
        LocalLight {
            position: Vec3::new(x, 2.0, 0.0),
            rotation: Quat::IDENTITY,
            range: 5.0,
            near: 0.1,
            cone_degrees: None,
        }
    }

    /// Gives `lights` their slots: what each slot draws, the slots' views,
    /// and each light's first view.
    fn give(
        cache: &mut ShadowCache,
        lights: &[(Entity, LocalLight)],
        caching: bool,
        frame: u64,
    ) -> (Lamps, Vec<(u32, u32)>, Vec<u32>) {
        let listed: Vec<ShadowLight> =
            lights.iter().enumerate().map(|(index, &(entity, light))| ShadowLight { index, entity, light }).collect();
        let mut shadows = GpuLocalShadows::default();
        let mut gpu_lights = vec![GpuLight::default(); lights.len()];
        let (lamps, views) = cache.give_slots(&listed, (&mut shadows, &mut gpu_lights), (caching, frame == 1, frame));
        assert_eq!(shadows.cached, lamps.cached);
        (lamps, views, gpu_lights.iter().map(|light| light.shadow).collect())
    }

    #[test]
    fn a_still_light_keeps_its_slot_and_draws_its_kept_views_once() {
        let mut world = voxear_ecs::World::new();
        let (a, b) = (world.spawn(()), world.spawn(()));
        let mut cache = ShadowCache::default();
        let (first, views, _) = give(&mut cache, &[(a, light(0.0)), (b, light(20.0))], true, 1);
        assert_eq!((first.cached, first.redraw), (0b11, 0b11), "both kept, both drawn at first");
        assert_eq!(views, vec![(0, 6), (6, 6)]);
        // Listed the other way round, each keeps its slot all the same.
        let (second, _, firsts) = give(&mut cache, &[(b, light(20.0)), (a, light(0.0))], true, 2);
        assert_eq!((second.cached, second.redraw), (0b11, 0), "nothing drawn again");
        assert_eq!(firsts, vec![6, 0]);
    }

    #[test]
    fn a_light_that_moves_is_drawn_whole_and_kept_again_once_it_stops() {
        let mut world = voxear_ecs::World::new();
        let a = world.spawn(());
        let mut cache = ShadowCache::default();
        give(&mut cache, &[(a, light(0.0))], true, 1);
        let (moved, ..) = give(&mut cache, &[(a, light(1.0))], true, 2);
        assert_eq!((moved.cached, moved.redraw), (0, 0), "drawn whole while it moves");
        let (stopped, ..) = give(&mut cache, &[(a, light(1.0))], true, 3);
        assert_eq!((stopped.cached, stopped.redraw), (1, 1), "kept, and drawn into the kept atlas, once still");
        let (after, ..) = give(&mut cache, &[(a, light(1.0))], true, 4);
        assert_eq!((after.cached, after.redraw), (1, 0));
    }

    #[test]
    fn a_change_in_reach_draws_only_the_lights_round_it_again() {
        let mut world = voxear_ecs::World::new();
        let (a, b) = (world.spawn(()), world.spawn(()));
        let mut cache = ShadowCache::default();
        let lights = [(a, light(0.0)), (b, light(20.0))];
        give(&mut cache, &lights, true, 1);
        cache.changed.push([1.0, 0.0, 0.0, 0.5]);
        let (after, ..) = give(&mut cache, &lights, true, 2);
        assert_eq!((after.cached, after.redraw), (0b11, 0b01), "only the light by the change");
        // Off, nothing is kept.
        let (off, ..) = give(&mut cache, &lights, false, 3);
        assert_eq!((off.cached, off.redraw), (0, 0));
        let (on, ..) = give(&mut cache, &lights, true, 4);
        assert_eq!(on.redraw, 0b11, "back on, everything is drawn again");
    }

    #[test]
    fn a_light_new_to_the_atlas_takes_the_slot_longest_unused() {
        let mut world = voxear_ecs::World::new();
        let lamps: Vec<Entity> = (0..LOCAL_SHADOW_LIGHTS + 1).map(|_| world.spawn(())).collect();
        let mut cache = ShadowCache::default();
        let all: Vec<(Entity, LocalLight)> =
            lamps.iter().enumerate().map(|(i, &e)| (e, light(i as f32 * 20.0))).collect();
        give(&mut cache, &all[..LOCAL_SHADOW_LIGHTS], true, 1);
        // The first light is left out a frame, then the last comes and takes the slot it left.
        give(&mut cache, &all[1..LOCAL_SHADOW_LIGHTS], true, 2);
        let (lamps_now, _, firsts) = give(&mut cache, &all[1..], true, 3);
        assert_eq!(*firsts.last().unwrap(), 0, "the first light's slot");
        assert_eq!(lamps_now.redraw, 1, "only it is drawn");
    }
}
