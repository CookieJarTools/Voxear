//! Skinning: each frame, the vertices of every part of a model a `Pose`
//! poses are moved by their bones on the GPU (`shaders/skinning.slang`),
//! once for all views. Each such part keeps three runs of posed vertices in
//! the shared vertex buffer, written in turn, so last frame's stay for its
//! motion while a frame still in flight reads them; its instance then reads
//! its posed vertices in place of its mesh's (`instanceVertices` in
//! `shaders/common.slang`), in every pass that draws it.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, Gpu, MemoryLocation, vk};
use voxear_scene::Destroying;

use crate::{
    assets::Assets,
    gpu::{GpuBuffers, GpuInstance, Push},
    graph::FrameGraph,
    model_renderer::ModelRenderer,
    pipelines::Pipelines,
    scene::Owner,
};

/// How many bone matrices a frame poses with, across every skinned part.
const MAX_SKIN_MATRICES: u64 = 16 * 1024;
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

/// Where a part's posed vertices are this frame and last, and how far its
/// pose reaches past its mesh's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Posed {
    pub now: u32,
    pub before: u32,
    pub reach: f32,
}

impl Posed {
    /// Marks an instance as drawn from its posed vertices.
    pub fn apply(&self, instance: &mut GpuInstance) {
        (instance.posed, instance.posed_before, instance.reach) = (self.now + 1, self.before + 1, self.reach);
    }
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

#[derive(Default)]
pub(crate) struct Skinning {
    slots: HashMap<Owner, Slot>,
    jobs: Vec<Job>,
    /// This frame's posed parts, for each view to mark its instances.
    pub posed: Arc<HashMap<Owner, Posed>>,
    /// Each frame in flight's bone matrices, three rows each and a fourth
    /// saying whether the bone is hidden, and the pointers the skinning
    /// shader reads.
    matrices: Vec<(Buffer, Buffer)>,
}

impl Skinning {
    /// Works out this frame's skinning: each posed part's bones' matrices,
    /// written for frame slot `slot`, and its posed vertices' room, kept
    /// from frame to frame. Parts no longer posed give theirs back.
    pub fn prepare(&mut self, gpu: &Gpu, assets: &mut Assets, world: &World, frame: u64, slot: usize) -> Result<()> {
        while self.matrices.len() <= slot {
            let host = MemoryLocation::CpuToGpu;
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let size = MAX_SKIN_MATRICES * 64;
            let matrices = gpu.create_buffer(&format!("bone matrices {}", self.matrices.len()), size, storage, host)?;
            let pointers = gpu.create_buffer(
                &format!("skinning pointers {}", self.matrices.len()),
                size_of::<GpuBuffers>() as u64,
                storage,
                host,
            )?;
            self.matrices.push((matrices, pointers));
        }
        self.jobs.clear();
        let mut posed = HashMap::new();
        let mut rows: Vec<[f32; 4]> = Vec::new();
        let query = world.query_ref::<(Entity, &ModelRenderer, &Pose)>();
        for (entity, model, pose) in query.without::<Destroying>() {
            let Some(animation) = &model.animation else { continue };
            let hidden = world.get::<HiddenBones>(entity);
            let hidden = |joint: usize| hidden.is_some_and(|hidden| hidden.bones.get(joint).copied().unwrap_or(false));
            for (index, part) in model.parts.iter().enumerate() {
                let Some(skin) = part.skin.and_then(|skin| animation.skins.get(skin as usize)) else { continue };
                let (Some((skin_offset, count)), Some(source)) =
                    (assets.skin_of(part.mesh), assets.vertices_of(part.mesh))
                else {
                    continue;
                };
                if rows.len() / 4 + skin.joints.len() > MAX_SKIN_MATRICES as usize {
                    break;
                }
                let owner = Owner { entity, plant: None, part: Some(index as u32) };
                let Some(kept) = self.slot(assets, owner, u64::from(count), frame) else { continue };
                // Each joint's matrix: where its bone is now, from where it was when the mesh was bound to it.
                let matrices = (rows.len() / 4) as u32;
                let mut far = 0.0f32;
                let (centre, radius) = assets.mesh_sphere(part.mesh).unwrap_or((Vec3::ZERO, 0.0));
                for (&joint, bind) in skin.joints.iter().zip(&skin.inverse_binds) {
                    let bone = pose.bones.get(joint).copied().unwrap_or(Mat4::IDENTITY);
                    let m = (bone * *bind).transpose();
                    let shown = if hidden(joint) { 1.0 } else { 0.0 };
                    rows.extend([
                        m.x_axis.to_array(),
                        m.y_axis.to_array(),
                        m.z_axis.to_array(),
                        [shown, 0.0, 0.0, 0.0],
                    ]);
                    far = far.max(bone.w_axis.truncate().distance(centre));
                }
                let run = |k: u64| (kept.offset + (k % 3) * kept.count) as u32;
                let now = run(frame);
                let before = if frame.checked_sub(1) == Some(kept.written) { run(frame + 2) } else { now };
                kept.written = frame;
                let reach = (far + FLESH - radius).max(0.0);
                posed.insert(owner, Posed { now, before, reach });
                self.jobs.push(Job { source, skin: skin_offset, matrices, output: now, count });
            }
        }
        // Parts no longer posed give their room back, once frames in flight are done with it.
        let gone: Vec<Owner> = self.slots.iter().filter(|(_, s)| s.written != frame).map(|(&o, _)| o).collect();
        for owner in gone {
            if let Some(slot) = self.slots.remove(&owner) {
                assets.release_posed(slot.offset, slot.count * 3, frame);
            }
        }
        let (matrices, pointers) = &self.matrices[slot];
        matrices.write(0, &rows);
        let table = GpuBuffers {
            vertices: assets.vertices.address,
            skins: assets.skins.address,
            skin_matrices: matrices.address,
            ..bytemuck::Zeroable::zeroed()
        };
        pointers.write(0, &[table]);
        self.posed = Arc::new(posed);
        Ok(())
    }

    /// The part's slot, made, or made again when its mesh's vertices
    /// changed in number; `None` when there is no room.
    fn slot(&mut self, assets: &mut Assets, owner: Owner, count: u64, frame: u64) -> Option<&mut Slot> {
        if self.slots.get(&owner).is_some_and(|slot| slot.count != count) {
            let old = self.slots.remove(&owner)?;
            assets.release_posed(old.offset, old.count * 3, frame);
        }
        if !self.slots.contains_key(&owner) {
            let offset = assets.reserve_posed(count * 3)?;
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
        let pointers = self.matrices[slot].1.address;
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
        for (matrices, pointers) in self.matrices.drain(..) {
            gpu.destroy_buffer(matrices);
            gpu.destroy_buffer(pointers);
        }
    }
}
