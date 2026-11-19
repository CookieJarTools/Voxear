//! Skinning: each frame, the vertices of every part of a model a `Pose`
//! poses are moved by their bones on the GPU (`shaders/skinning.slang`),
//! once for all views. Each such part keeps three runs of posed vertices in
//! the shared vertex buffer, written in turn, so last frame's stay for its
//! motion while a frame still in flight reads them; its instance then reads
//! its posed vertices in place of its mesh's (`instanceVertices` in
//! `shaders/common.slang`), in every pass that draws it.
//!
//! A part whose every vertex is bound whole to one bone, as a box model's
//! are, is not skinned at all: its bones' matrices are all it needs, and
//! each pass that draws it moves each vertex by its one bone as it reads it
//! (`instanceVertex`). So it keeps no posed vertices, and every Obj drawing
//! the same model shares one mesh.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use voxear_core::math::{Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, Gpu, MemoryLocation, vk};
use voxear_scene::Destroying;

use crate::{
    assets::{Assets, MeshHandle},
    gpu::{GpuBuffers, GpuInstance, Push},
    graph::FrameGraph,
    model_renderer::ModelRenderer,
    pipelines::Pipelines,
    scene::Owner,
};

/// How many bone matrices a frame's buffer holds at first, across every
/// posed part; it grows to what a frame needs, up to `MOST_SKIN_MATRICES`.
const FIRST_SKIN_MATRICES: usize = 16 * 1024;
/// The most bone matrices a frame poses with: 64 MB a frame in flight.
const MOST_SKIN_MATRICES: usize = 1024 * 1024;
/// How far past its bones a skinned mesh's vertices may lie, in metres,
/// which culling allows for.
const FLESH: f32 = 0.3;

/// Where each of a model's bones is this frame, in the model's space, in
/// its animation's skeleton's order: set by the animator, and what moves
/// the model's skinned parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    pub bones: Arc<[Mat4]>,
}

impl Component for Pose {}

/// The bones of a model's skeleton hidden from the cameras that draw it, in
/// its skeleton's order: skin bound a quarter or more to them is left out of
/// what a camera sees, though it still casts its shadow. Set each frame by what
/// hides them, as a First-Person View hides the head its camera looks out
/// of, and gone the next unless set again. The bones themselves stay where
/// the pose has them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HiddenBones {
    pub bones: Vec<bool>,
}

impl Component for HiddenBones {}

/// The bones of a model's skeleton a script has switched off, in its
/// skeleton's order: the skin bound to them, and to every bone under them,
/// is drawn nowhere, shadows included, as it is drawn in to the bone that
/// was switched off. Kept until switched on again; the bones themselves
/// stay where the pose has them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BonesOff {
    pub off: Vec<bool>,
}

impl Component for BonesOff {}

impl BonesOff {
    /// Whether each bone is drawn nowhere: switched off itself, or under
    /// one that is. Parents come before their children in a skeleton.
    pub fn reach(&self, skeleton: &crate::Skeleton) -> Vec<Option<usize>> {
        let mut gone: Vec<Option<usize>> = Vec::with_capacity(skeleton.bones.len());
        for (index, bone) in skeleton.bones.iter().enumerate() {
            let own = self.off.get(index).copied().unwrap_or(false).then_some(index);
            gone.push(own.or_else(|| bone.parent.and_then(|parent| gone.get(parent).copied().flatten())));
        }
        gone
    }
}

/// How a part is posed this frame: where its posed vertices are this frame
/// and last, or, posed by its bones where it is drawn, where its bones'
/// matrices are this frame and last and its mesh's skin; and how far its
/// pose reaches past its mesh's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Posed {
    pub now: u32,
    pub before: u32,
    pub reach: f32,
    pub by_bones: Option<ByBones>,
}

/// A part posed by its bones where it is drawn: where its matrices start
/// in this frame's buffer, and in last frame's (`None` when it was not
/// posed then), and where its mesh's skin starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ByBones {
    pub matrices: u32,
    pub before: Option<u32>,
    pub skin: u32,
}

impl Posed {
    /// Marks an instance as drawn posed.
    pub fn apply(&self, instance: &mut GpuInstance) {
        instance.reach = self.reach;
        match self.by_bones {
            Some(bones) => {
                (instance.posed, instance.posed_before) = (0, 0);
                instance.bones = bones.matrices + 1;
                instance.bones_before = bones.before.map_or(0, |before| before + 1);
                instance.skin = bones.skin;
            }
            None => (instance.posed, instance.posed_before) = (self.now + 1, self.before + 1),
        }
    }
}

/// What posing a skinned part needs of its mesh, the same for every Obj
/// drawing it: where its skin and vertices start, how many vertices, whether
/// it is bound whole to bones, and its bounding sphere.
#[derive(Clone, Copy)]
struct MeshSkin {
    skin: u32,
    count: u32,
    source: u32,
    rigid: bool,
    centre: Vec3,
    radius: f32,
}

/// A part's three runs of posed vertices, how many vertices each, and the
/// last frame they were written.
struct Slot {
    offset: u64,
    count: u64,
    written: u64,
}

/// One part's skinning this frame: its mesh's vertices, its skin, its
/// bones' matrices among the frame's, and where its posed vertices go.
#[derive(Clone, Copy)]
struct Job {
    source: u32,
    skin: u32,
    matrices: u32,
    output: u32,
    count: u32,
}

/// A frame in flight's bone matrices, three rows each and a fourth saying
/// whether the bone is hidden, how many they hold, and the pointers the
/// skinning shader reads.
struct FrameMatrices {
    matrices: Buffer,
    capacity: usize,
    pointers: Buffer,
}

#[derive(Default)]
pub(crate) struct Skinning {
    slots: HashMap<Owner, Slot>,
    /// The frame last prepared.
    last_frame: Option<u64>,
    /// How many matrix rows last frame wrote, to make room for as many.
    last_rows: usize,
    jobs: Vec<Job>,
    /// This frame's posed parts, for each view to mark its instances.
    pub posed: Arc<HashMap<Owner, Posed>>,
    matrices: Vec<FrameMatrices>,
    /// The frame slot last prepared, whose matrices are last frame's.
    last_slot: Option<usize>,
    /// The slot this frame's are in.
    slot: usize,
    /// How many posed parts were left at rest this frame, with no room for
    /// their matrices or posed vertices.
    pub left_at_rest: u32,
    warned: bool,
}

impl Skinning {
    /// Works out this frame's skinning: each posed part's bones' matrices,
    /// written for frame slot `slot`, and a skinned part's posed vertices'
    /// room, kept from frame to frame. Parts no longer posed give theirs back.
    pub fn prepare(&mut self, gpu: &Gpu, assets: &mut Assets, world: &World, frame: u64, slot: usize) -> Result<()> {
        self.jobs.clear();
        self.last_slot = (self.slot != slot && self.matrices.len() > self.slot).then_some(self.slot);
        self.slot = slot;
        // Last frame's posed parts, whose matrices are in last frame's buffer, where their motion starts.
        let last = std::mem::take(&mut self.posed);
        let was_last = self.last_slot.is_some() && frame.checked_sub(1) == self.last_frame;
        self.last_frame = Some(frame);
        let mut posed = HashMap::with_capacity(last.len());
        let mut rows: Vec<[f32; 4]> = Vec::with_capacity(self.last_rows);
        let mut left_at_rest = 0u32;
        let mut meshes: HashMap<MeshHandle, Option<MeshSkin>> = HashMap::new();
        let query = world.query_ref::<(Entity, &ModelRenderer, &Pose, Option<&HiddenBones>, Option<&BonesOff>)>();
        for (entity, model, pose, hidden, off) in query.without::<Destroying>() {
            let Some(animation) = &model.animation else { continue };
            let hidden = |joint: usize| hidden.is_some_and(|hidden| hidden.bones.get(joint).copied().unwrap_or(false));
            // The bone each switched-off bone's skin is drawn in to, its own or the one above it switched off.
            let gone = off.map(|off| off.reach(&animation.skeleton)).unwrap_or_default();
            for (index, part) in model.parts.iter().enumerate() {
                let Some(skin) = part.skin.and_then(|skin| animation.skins.get(skin as usize)) else { continue };
                let mesh = *meshes.entry(part.mesh).or_insert_with(|| {
                    let ((skin, count), source) = (assets.skin_of(part.mesh)?, assets.vertices_of(part.mesh)?);
                    let (centre, radius) = assets.mesh_sphere(part.mesh).unwrap_or((Vec3::ZERO, 0.0));
                    Some(MeshSkin { skin, count, source, rigid: assets.is_rigid_skin(part.mesh), centre, radius })
                });
                let Some(mesh) = mesh else { continue };
                if rows.len() / 4 + skin.joints.len() > MOST_SKIN_MATRICES {
                    left_at_rest += 1;
                    continue;
                }
                let owner = Owner { entity, plant: None, part: Some(index as u32), far_side: false };
                // Each joint's matrix: where its bone is now, from where it was when the mesh was bound to it.
                let matrices = (rows.len() / 4) as u32;
                let mut far = 0.0f32;
                for (&joint, bind) in skin.joints.iter().zip(&skin.inverse_binds) {
                    let bone = pose.bones.get(joint).copied().unwrap_or(Mat4::IDENTITY);
                    // Drawn in to a point, so its triangles cover nothing, in any pass.
                    let m = match gone.get(joint).copied().flatten() {
                        Some(off) => {
                            let at = pose.bones.get(off).copied().unwrap_or(bone).w_axis;
                            Mat4::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, at).transpose()
                        }
                        None => (bone * *bind).transpose(),
                    };
                    let shown = if hidden(joint) { 1.0 } else { 0.0 };
                    rows.extend([
                        m.x_axis.to_array(),
                        m.y_axis.to_array(),
                        m.z_axis.to_array(),
                        [shown, 0.0, 0.0, 0.0],
                    ]);
                    far = far.max(bone.w_axis.truncate().distance(mesh.centre));
                }
                let reach = (far + FLESH - mesh.radius).max(0.0);
                if mesh.rigid {
                    let before = match was_last {
                        true => last.get(&owner).and_then(|posed| posed.by_bones).map(|bones| bones.matrices),
                        false => None,
                    };
                    let by_bones = Some(ByBones { matrices, before, skin: mesh.skin });
                    posed.insert(owner, Posed { now: 0, before: 0, reach, by_bones });
                    continue;
                }
                let Some(kept) = self.slot(gpu, assets, owner, u64::from(mesh.count), frame) else {
                    left_at_rest += 1;
                    rows.truncate(matrices as usize * 4);
                    continue;
                };
                let run = |k: u64| (kept.offset + (k % 3) * kept.count) as u32;
                let now = run(frame);
                let before = if frame.checked_sub(1) == Some(kept.written) { run(frame + 2) } else { now };
                kept.written = frame;
                posed.insert(owner, Posed { now, before, reach, by_bones: None });
                let (source, skin, count) = (mesh.source, mesh.skin, mesh.count);
                self.jobs.push(Job { source, skin, matrices, output: now, count });
            }
        }
        // Parts no longer posed give their room back, once frames in flight are done with it.
        let gone: Vec<Owner> = self.slots.iter().filter(|(_, s)| s.written != frame).map(|(&o, _)| o).collect();
        for owner in gone {
            if let Some(slot) = self.slots.remove(&owner) {
                assets.release_posed(slot.offset, slot.count * 3, frame);
            }
        }
        self.last_rows = rows.len();
        let needed = (rows.len() / 4).max(1);
        self.make_room(gpu, slot, needed)?;
        let FrameMatrices { matrices, pointers, .. } = &self.matrices[slot];
        matrices.write(0, &rows);
        let table = GpuBuffers {
            vertices: assets.vertices.address,
            skins: assets.skins.address,
            skin_matrices: matrices.address,
            ..bytemuck::Zeroable::zeroed()
        };
        pointers.write(0, &[table]);
        self.posed = Arc::new(posed);
        self.left_at_rest = left_at_rest;
        if left_at_rest > 0 && !self.warned {
            self.warned = true;
            tracing::warn!("{left_at_rest} posed parts are drawn at rest: no room for their bones or posed vertices");
        }
        Ok(())
    }

    /// Makes slot `slot`'s matrices hold at least `needed`, made again
    /// larger when they do not: its last frame is done, so nothing reads them.
    fn make_room(&mut self, gpu: &Gpu, slot: usize, needed: usize) -> Result<()> {
        let host = MemoryLocation::CpuToGpu;
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        while self.matrices.len() <= slot {
            let at = self.matrices.len();
            let matrices = gpu.create_buffer(&format!("bone matrices {at}"), FIRST_SKIN_MATRICES as u64 * 64, storage, host)?;
            let size = size_of::<GpuBuffers>() as u64;
            let pointers = gpu.create_buffer(&format!("skinning pointers {at}"), size, storage, host)?;
            self.matrices.push(FrameMatrices { matrices, capacity: FIRST_SKIN_MATRICES, pointers });
        }
        let kept = &mut self.matrices[slot];
        if kept.capacity < needed {
            let capacity = needed.next_power_of_two().min(MOST_SKIN_MATRICES);
            let grown = gpu.create_buffer(&format!("bone matrices {slot}"), capacity as u64 * 64, storage, host)?;
            gpu.destroy_buffer(std::mem::replace(&mut kept.matrices, grown));
            kept.capacity = capacity;
        }
        Ok(())
    }

    /// How many parts this frame skinned, posed by their bones, and left at
    /// rest.
    pub fn counts(&self) -> crate::PosedCounts {
        let by_bones = self.posed.values().filter(|posed| posed.by_bones.is_some()).count() as u32;
        crate::PosedCounts { skinned: self.jobs.len() as u32, by_bones, at_rest: self.left_at_rest }
    }

    /// Where this frame's bones' matrices are, and last frame's (this
    /// frame's again when there were none), for the passes that draw.
    pub fn matrices(&self) -> (u64, u64) {
        let now = self.matrices.get(self.slot).map_or(0, |kept| kept.matrices.address);
        let before = self.last_slot.and_then(|slot| self.matrices.get(slot)).map_or(now, |kept| kept.matrices.address);
        (now, before)
    }

    /// The part's slot, made, or made again when its mesh's vertices
    /// changed in number; `None` when there is no room.
    fn slot(&mut self, gpu: &Gpu, assets: &mut Assets, owner: Owner, count: u64, frame: u64) -> Option<&mut Slot> {
        if self.slots.get(&owner).is_some_and(|slot| slot.count != count) {
            let old = self.slots.remove(&owner)?;
            assets.release_posed(old.offset, old.count * 3, frame);
        }
        if !self.slots.contains_key(&owner) {
            let offset = assets.reserve_posed(gpu, count * 3)?;
            // Never written yet, so no last frame's pose to move from.
            self.slots.insert(owner, Slot { offset, count, written: u64::MAX });
        }
        self.slots.get_mut(&owner)
    }

    /// Records this frame's skinning, before anything draws.
    pub fn record(&self, fg: &mut FrameGraph<'_>, pipelines: Pipelines, slot: usize) {
        if self.jobs.is_empty() {
            return;
        }
        let jobs = self.jobs.clone();
        let pointers = self.matrices[slot].pointers.address;
        fg.add_pass("Skinning", &[], &[], move |ctx| {
            for job in &jobs {
                let push = Push {
                    buffers: pointers,
                    textures: [job.source, job.skin, job.matrices, job.output],
                    size: [job.count, 0],
                    ..Push::default()
                };
                crate::passes::bind(ctx, pipelines.skinning, vk::PipelineBindPoint::COMPUTE, &push);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, job.count.div_ceil(64), 1, 1) };
            }
            // Every pass after reads the posed vertices as it reads any.
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        });
    }

    /// Frees the matrices' buffers.
    pub fn destroy(&mut self, gpu: &Gpu) {
        for kept in self.matrices.drain(..) {
            gpu.destroy_buffer(kept.matrices);
            gpu.destroy_buffer(kept.pointers);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bone, BonePose, Skeleton};

    #[test]
    fn a_bone_switched_off_takes_the_bones_under_it() {
        let bone = |name: &str, parent| Bone { name: name.into(), parent, rest: BonePose::default() };
        let skeleton = Skeleton {
            bones: vec![bone("Body", None), bone("Head", Some(0)), bone("Jaw", Some(1)), bone("Arm", Some(0))],
        };
        let off = BonesOff { off: vec![false, true, false, false] };
        assert_eq!(off.reach(&skeleton), vec![None, Some(1), Some(1), None], "drawn in to the head");
        assert!(BonesOff::default().reach(&skeleton).iter().all(Option::is_none));
    }
}
