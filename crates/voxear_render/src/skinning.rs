//! Skinning: each frame, the vertices of every part of a model a `Pose`
//! poses are moved by their bones on the GPU (`shaders/skinning.slang`),
//! once for all views, in one dispatch for every part. Each such part keeps
//! three runs of posed vertices in a pool of their own, written in turn, so
//! last frame's stay for its motion while a frame still in flight reads
//! them; its instance then reads its posed vertices in place of its mesh's
//! (`instanceVertex` in `shaders/common.slang`), in every pass that draws it.
//!
//! What it costs is held to a budget:
//! - A part whose every vertex is bound whole to one bone, as a box model's
//!   are, is not skinned at all: its bones' matrices are all it needs, and
//!   each pass that draws it moves each vertex by its bone as it reads it.
//! - A part that finds no room in the pool is posed the same way, by up to
//!   four bones a vertex, where it is drawn: dearer in each pass, but never
//!   drawn at rest.
//! - A part no camera can see, nor see the shadow of, is not skinned again
//!   until one can: it keeps the pose it was last skinned in.
//! - Objs whose poses are one and the same (`Arc::ptr_eq`), as a crowd
//!   sharing its walk, are skinned once and share their posed vertices.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use voxear_core::math::{Mat4, Quat, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Gpu, MemoryLocation, vk};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    assets::{Assets, MeshHandle},
    gpu::{GpuBuffers, GpuInstance, Push},
    graph::FrameGraph,
    model_renderer::ModelRenderer,
    pipelines::Pipelines,
    scene::{Camera, Owner},
};

/// How many posed vertices the pool holds, unless the project says
/// otherwise (see [`PosedVertexRoom`]): about forty characters of 8,000
/// vertices, three runs each, 52 MB.
pub const DEFAULT_POSED_VERTICES: u32 = 1024 * 1024;
/// The least and most the pool may be given: none poses every skinned part
/// where it is drawn.
pub const MIN_POSED_VERTICES: u32 = 0;
pub const MAX_POSED_VERTICES: u32 = 16 * 1024 * 1024;
/// How far, in metres, a part out of every camera's sight is still skinned,
/// and how far past the edge of a camera's view: its shadow may fall there.
const SEEN_MARGIN: f32 = 12.0;

/// How many vertices the project gives posed vertices' pool
/// (`[quality] posed_vertices`); [`DEFAULT_POSED_VERTICES`] where none is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PosedVertexRoom(pub u32);

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
/// posed then), where its mesh's skin starts, and whether its vertices are
/// weighed between up to four bones rather than bound whole to one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ByBones {
    pub matrices: u32,
    pub before: Option<u32>,
    pub skin: u32,
    pub weighted: bool,
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
                instance.bones_weighted = u32::from(bones.weighted);
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

/// A part's three runs of posed vertices, how many vertices each, the run
/// last written and the frame it was, and the last frame it was posed.
struct Slot {
    offset: u64,
    count: u64,
    run: u64,
    written: u64,
    used: u64,
}

/// One part's skinning this frame: its mesh's vertices, its skin, its
/// bones' matrices among the frame's, where its posed vertices go, how
/// many, and the first of the dispatch's thread groups that are its.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Job {
    source: u32,
    skin: u32,
    matrices: u32,
    output: u32,
    count: u32,
    first_group: u32,
    _pad: [u32; 2],
}

/// Vertices a thread group of the skinning shader poses.
const GROUP: u32 = 64;
/// The most thread groups a dispatch has across; more go in rows.
const GROUPS_ACROSS: u32 = 65535;

/// A frame in flight's bone matrices, three rows each and a fourth saying
/// whether the bone is hidden, how many they hold; its skinning jobs and
/// each thread group's job, how many each holds; and the pointers the
/// skinning shader reads.
struct FrameMatrices {
    matrices: Buffer,
    capacity: usize,
    jobs: Buffer,
    job_capacity: usize,
    groups: Buffer,
    group_capacity: usize,
    pointers: Buffer,
}

/// Posed vertices' own pool: the buffer, its room, and runs given back but
/// still read by a frame in flight, with the frame they are free after.
struct PosedPool {
    vertices: Buffer,
    space: crate::alloc::RangeAllocator,
    retiring: Vec<(u64, u64, u64)>,
}

/// Where a camera looks from, which way, and the cosine of half the widest
/// angle it sees, corner to corner.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Viewer {
    pub at: Vec3,
    pub ahead: Vec3,
    pub half_angle: f32,
}

impl Viewer {
    /// Whether it may see a sphere, or the shadow it casts nearby.
    fn sees(&self, centre: Vec3, radius: f32) -> bool {
        let to = centre - self.at;
        let far = to.length();
        let reach = radius + SEEN_MARGIN;
        if far <= reach {
            return true;
        }
        let off = to.dot(self.ahead).clamp(-far, far) / far;
        off.acos() <= self.half_angle + (reach / far).min(1.0).asin()
    }
}

/// The cameras the views look through, as `cameras` names them (`None` for
/// the first switched on), each with the widest its view is across to up;
/// `None` when what they see cannot be told, as through a Portal.
pub(crate) fn viewers(world: &World, cameras: &[(Option<Entity>, f32)]) -> Option<Vec<Viewer>> {
    if world.query_ref::<&crate::portals::Portal>().into_iter().next().is_some() {
        return None;
    }
    let found: Vec<(Entity, Camera, Vec3, Quat)> = world
        .query_ref::<(Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| voxear_scene::is_on(world, *entity, voxear_scene::Switch::Camera))
        .map(|(entity, camera, local, placed)| {
            let (position, rotation) = match placed {
                Some(placed) => {
                    let (_, rotation, position) = placed.0.to_scale_rotation_translation();
                    (position, rotation)
                }
                None => (local.position, local.rotation),
            };
            (entity, *camera, position, rotation)
        })
        .collect();
    let mut viewers = Vec::with_capacity(cameras.len());
    for (chosen, aspect) in cameras {
        let camera = found.iter().find(|(entity, ..)| chosen.is_none_or(|chosen| chosen == *entity));
        let Some((_, camera, at, rotation)) = camera else { return None };
        let tan = (camera.fov_y_degrees.to_radians() * 0.5).tan();
        let half_angle = (tan * (1.0 + aspect * aspect).sqrt()).atan();
        viewers.push(Viewer { at: *at, ahead: *rotation * Vec3::Z, half_angle });
    }
    Some(viewers)
}

#[derive(Default)]
pub(crate) struct Skinning {
    slots: HashMap<Owner, Slot>,
    pool: Option<PosedPool>,
    /// The frame last prepared.
    last_frame: Option<u64>,
    /// How many matrix rows last frame wrote, to make room for as many.
    last_rows: usize,
    jobs: Vec<Job>,
    groups: Vec<u32>,
    /// This frame's posed parts, for each view to mark its instances.
    pub posed: Arc<HashMap<Owner, Posed>>,
    matrices: Vec<FrameMatrices>,
    /// The frame slot last prepared, whose matrices are last frame's.
    last_slot: Option<usize>,
    /// The slot this frame's are in.
    slot: usize,
    counts: crate::PosedCounts,
    warned: bool,
}

/// A part's pose this frame, by its mesh, its pose and its skin: Objs
/// posed alike share it.
type Shared = (MeshHandle, usize, u32);

impl Skinning {
    /// Works out this frame's skinning: each posed part's bones' matrices,
    /// written for frame slot `slot`, and a skinned part's posed vertices'
    /// room, kept from frame to frame. Parts no longer posed give theirs back.
    /// `viewers` are the cameras that may see them, `None` for any.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        assets: &Assets,
        world: &World,
        frame: u64,
        slot: usize,
        viewers: Option<&[Viewer]>,
    ) -> Result<()> {
        self.jobs.clear();
        self.groups.clear();
        self.make_pool(gpu, world, frame)?;
        self.last_slot = (self.slot != slot && self.matrices.len() > self.slot).then_some(self.slot);
        self.slot = slot;
        // Last frame's posed parts, whose matrices are in last frame's buffer, where their motion starts.
        let last = std::mem::take(&mut self.posed);
        let was_last = self.last_slot.is_some() && frame.checked_sub(1) == self.last_frame;
        self.last_frame = Some(frame);
        let mut posed = HashMap::with_capacity(last.len());
        let mut shared: HashMap<Shared, Posed> = HashMap::new();
        let mut rows: Vec<[f32; 4]> = Vec::with_capacity(self.last_rows);
        let mut counts = crate::PosedCounts::default();
        let mut meshes: HashMap<MeshHandle, Option<MeshSkin>> = HashMap::new();
        let query = world.query_ref::<(
            Entity,
            &ModelRenderer,
            &Pose,
            Option<&HiddenBones>,
            Option<&BonesOff>,
            Option<&WorldTransform>,
        )>();
        for (entity, model, pose, hidden, off, placed) in query.without::<Destroying>() {
            let Some(animation) = &model.animation else { continue };
            // Hidden or switched-off bones make its pose its own.
            let alone = hidden.is_some() || off.is_some();
            let hidden = |joint: usize| hidden.is_some_and(|hidden| hidden.bones.get(joint).copied().unwrap_or(false));
            // The bone each switched-off bone's skin is drawn in to, its own or the one above it switched off.
            let gone = off.map(|off| off.reach(&animation.skeleton)).unwrap_or_default();
            let placed = placed.map_or_else(|| world.get::<Transform>(entity).map(Transform::matrix), |p| Some(p.0));
            let scale = placed.map_or(1.0, |m| m.to_scale_rotation_translation().0.abs().max_element());
            for (index, part) in model.parts.iter().enumerate() {
                let Some(skin) = part.skin.and_then(|skin| animation.skins.get(skin as usize)) else { continue };
                let mesh = *meshes.entry(part.mesh).or_insert_with(|| {
                    let ((skin, count), source) = (assets.skin_of(part.mesh)?, assets.vertices_of(part.mesh)?);
                    let (centre, radius) = assets.mesh_sphere(part.mesh).unwrap_or((Vec3::ZERO, 0.0));
                    Some(MeshSkin { skin, count, source, rigid: assets.is_rigid_skin(part.mesh), centre, radius })
                });
                let Some(mesh) = mesh else { continue };
                let owner = Owner { entity, plant: None, part: Some(index as u32), far_side: false };
                let key = (part.mesh, Arc::as_ptr(&pose.bones) as *const Mat4 as usize, part.skin.unwrap_or(0));
                if !alone && let Some(same) = shared.get(&key) {
                    posed.insert(owner, *same);
                    counts.shared += 1;
                    if let Some(slot) = self.slots.get_mut(&owner) {
                        slot.used = frame;
                    }
                    continue;
                }
                if rows.len() / 4 + skin.joints.len() > MOST_SKIN_MATRICES {
                    counts.at_rest += 1;
                    continue;
                }
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
                let by_bones = |weighted: bool| {
                    let before = match was_last {
                        true => last.get(&owner).and_then(|posed| posed.by_bones).map(|bones| bones.matrices),
                        false => None,
                    };
                    Posed { now: 0, before: 0, reach, by_bones: Some(ByBones { matrices, before, skin: mesh.skin, weighted }) }
                };
                let chosen = if mesh.rigid {
                    counts.by_bones += 1;
                    by_bones(false)
                } else {
                    // Out of every camera's sight, a part skinned before keeps the pose it was skinned in.
                    let seen = viewers.is_none_or(|viewers| {
                        let centre = placed.map_or(mesh.centre, |m| m.transform_point3(mesh.centre));
                        let radius = (mesh.radius + reach) * scale;
                        viewers.iter().any(|viewer| viewer.sees(centre, radius))
                    });
                    match self.skinned(gpu, owner, u64::from(mesh.count), frame, seen) {
                        Some((now, before, write)) => {
                            if write {
                                let (source, skin, count) = (mesh.source, mesh.skin, mesh.count);
                                let first_group = self.groups.len() as u32;
                                let job = self.jobs.len() as u32;
                                self.groups.extend(std::iter::repeat_n(job, count.div_ceil(GROUP) as usize));
                                self.jobs.push(Job { source, skin, matrices, output: now, count, first_group, _pad: [0; 2] });
                                counts.skinned += 1;
                            } else {
                                rows.truncate(matrices as usize * 4);
                                counts.unseen += 1;
                            }
                            Posed { now, before, reach, by_bones: None }
                        }
                        // No room in the pool: posed by its bones where it is drawn.
                        None => {
                            counts.weighted += 1;
                            by_bones(true)
                        }
                    }
                };
                if !alone {
                    shared.insert(key, chosen);
                }
                posed.insert(owner, chosen);
            }
        }
        // Parts no longer posed give their room back, once frames in flight are done with it.
        let gone: Vec<Owner> = self.slots.iter().filter(|(_, s)| s.used != frame).map(|(&o, _)| o).collect();
        for owner in gone {
            if let (Some(slot), Some(pool)) = (self.slots.remove(&owner), &mut self.pool) {
                pool.retiring.push((frame + FRAMES_IN_FLIGHT as u64, slot.offset, slot.count * 3));
            }
        }
        self.last_rows = rows.len();
        self.make_room(gpu, slot, (rows.len() / 4).max(1))?;
        let kept = &self.matrices[slot];
        kept.matrices.write(0, &rows);
        kept.jobs.write(0, &self.jobs);
        kept.groups.write(0, &self.groups);
        let table = GpuBuffers {
            vertices: assets.vertices.address,
            skins: assets.skins.address,
            skin_matrices: kept.matrices.address,
            posed_vertices: self.pool.as_ref().map_or(0, |pool| pool.vertices.address),
            skin_jobs: kept.jobs.address,
            skin_groups: kept.groups.address,
            ..bytemuck::Zeroable::zeroed()
        };
        kept.pointers.write(0, &[table]);
        self.posed = Arc::new(posed);
        self.counts = counts;
        if counts.at_rest > 0 && !self.warned {
            self.warned = true;
            tracing::warn!("{} posed parts are drawn at rest: no room for their bones", counts.at_rest);
        }
        Ok(())
    }

    /// A skinned part's runs of posed vertices this frame: where it is
    /// drawn from now and last frame, and whether to skin it, which it is
    /// when `seen` or never skinned yet. `None` when the pool has no room.
    fn skinned(&mut self, gpu: &Gpu, owner: Owner, count: u64, frame: u64, seen: bool) -> Option<(u32, u32, bool)> {
        let _ = gpu;
        let pool = self.pool.as_mut()?;
        if self.slots.get(&owner).is_some_and(|slot| slot.count != count) {
            let old = self.slots.remove(&owner)?;
            pool.retiring.push((frame + FRAMES_IN_FLIGHT as u64, old.offset, old.count * 3));
        }
        if !self.slots.contains_key(&owner) {
            let offset = pool.space.alloc(count * 3)?;
            // Never written yet, so no last frame's pose to move from.
            self.slots.insert(owner, Slot { offset, count, run: 0, written: u64::MAX, used: frame });
        }
        let slot = self.slots.get_mut(&owner)?;
        slot.used = frame;
        let at = |run: u64| (slot.offset + run * slot.count) as u32;
        if !seen && slot.written != u64::MAX {
            // Unseen, it stands in the pose last skinned, not moving.
            return Some((at(slot.run), at(slot.run), false));
        }
        // The next run in turn: neither of the two frames in flight reads it.
        let moved_on = frame.checked_sub(1) == Some(slot.written);
        let before = at(slot.run);
        slot.run = (slot.run + 1) % 3;
        slot.written = frame;
        let now = at(slot.run);
        Some((now, if moved_on { before } else { now }, true))
    }

    /// Makes the posed vertices' pool as large as the project gives it,
    /// made again, with every part skinned afresh, when that changes.
    fn make_pool(&mut self, gpu: &Gpu, world: &World, frame: u64) -> Result<()> {
        let wanted = world.resource::<PosedVertexRoom>().map_or(DEFAULT_POSED_VERTICES, |room| room.0);
        let wanted = u64::from(wanted.clamp(MIN_POSED_VERTICES, MAX_POSED_VERTICES));
        if self.pool.is_none() && wanted == 0 {
            return Ok(());
        }
        if let Some(pool) = &mut self.pool {
            if pool.space.capacity() == wanted {
                pool.retiring.retain(|&(after, offset, length)| {
                    let done = frame >= after;
                    if done {
                        pool.space.free(offset, length);
                    }
                    !done
                });
                return Ok(());
            }
            // Its size changed: nothing in flight may read the old one as it goes.
            gpu.wait_idle();
            if let Some(old) = self.pool.take() {
                gpu.destroy_buffer(old.vertices);
            }
            self.slots.clear();
        }
        if wanted == 0 {
            return Ok(());
        }
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let size = wanted * size_of::<crate::Vertex>() as u64;
        let vertices = gpu.create_buffer("posed vertices", size, storage, MemoryLocation::GpuOnly)?;
        let space = crate::alloc::RangeAllocator::new(wanted);
        self.pool = Some(PosedPool { vertices, space, retiring: Vec::new() });
        Ok(())
    }

    /// Makes slot `slot`'s matrices hold at least `needed`, and its jobs and
    /// thread groups this frame's, made again larger when they do not: its
    /// last frame is done, so nothing reads them.
    fn make_room(&mut self, gpu: &Gpu, slot: usize, needed: usize) -> Result<()> {
        let host = MemoryLocation::CpuToGpu;
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let make = |name: &str, size: u64| gpu.create_buffer(name, size, storage, host);
        while self.matrices.len() <= slot {
            let at = self.matrices.len();
            self.matrices.push(FrameMatrices {
                matrices: make(&format!("bone matrices {at}"), FIRST_SKIN_MATRICES as u64 * 64)?,
                capacity: FIRST_SKIN_MATRICES,
                jobs: make(&format!("skinning jobs {at}"), 1024 * size_of::<Job>() as u64)?,
                job_capacity: 1024,
                groups: make(&format!("skinning groups {at}"), 16 * 1024 * 4)?,
                group_capacity: 16 * 1024,
                pointers: make(&format!("skinning pointers {at}"), size_of::<GpuBuffers>() as u64)?,
            });
        }
        let kept = &mut self.matrices[slot];
        let grow = |buffer: &mut Buffer, capacity: &mut usize, needed: usize, each: usize, name: &str| -> Result<()> {
            if *capacity < needed {
                let more = needed.next_power_of_two();
                let grown = gpu.create_buffer(&format!("{name} {slot}"), (more * each) as u64, storage, host)?;
                gpu.destroy_buffer(std::mem::replace(buffer, grown));
                *capacity = more;
            }
            Ok(())
        };
        grow(&mut kept.matrices, &mut kept.capacity, needed.min(MOST_SKIN_MATRICES), 64, "bone matrices")?;
        grow(&mut kept.jobs, &mut kept.job_capacity, self.jobs.len(), size_of::<Job>(), "skinning jobs")?;
        grow(&mut kept.groups, &mut kept.group_capacity, self.groups.len(), 4, "skinning groups")?;
        Ok(())
    }

    /// How many parts this frame skinned, posed by their bones, and left at
    /// rest, and the rest of what the Profiler shows of posing.
    pub fn counts(&self) -> crate::PosedCounts {
        self.counts
    }

    /// Where this frame's bones' matrices are, and last frame's (this
    /// frame's again when there were none), for the passes that draw.
    pub fn matrices(&self) -> (u64, u64) {
        let now = self.matrices.get(self.slot).map_or(0, |kept| kept.matrices.address);
        let before = self.last_slot.and_then(|slot| self.matrices.get(slot)).map_or(now, |kept| kept.matrices.address);
        (now, before)
    }

    /// Where posed vertices are, for the passes that draw.
    pub fn posed_vertices(&self) -> u64 {
        self.pool.as_ref().map_or(0, |pool| pool.vertices.address)
    }

    /// Records this frame's skinning, before anything draws: one dispatch,
    /// a thread group for each 64 vertices of every part.
    pub fn record(&self, fg: &mut FrameGraph<'_>, pipelines: Pipelines, slot: usize) {
        if self.jobs.is_empty() {
            return;
        }
        let groups = self.groups.len() as u32;
        let pointers = self.matrices[slot].pointers.address;
        fg.add_pass("Skinning", &[], &[], move |ctx| {
            let push = Push { buffers: pointers, size: [groups, 0], ..Push::default() };
            crate::passes::bind(ctx, pipelines.skinning, vk::PipelineBindPoint::COMPUTE, &push);
            let (across, rows) = (groups.min(GROUPS_ACROSS), groups.div_ceil(GROUPS_ACROSS));
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, across, rows, 1) };
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

    /// Frees the matrices' buffers and the pool.
    pub fn destroy(&mut self, gpu: &Gpu) {
        for kept in self.matrices.drain(..) {
            for buffer in [kept.matrices, kept.jobs, kept.groups, kept.pointers] {
                gpu.destroy_buffer(buffer);
            }
        }
        if let Some(pool) = self.pool.take() {
            gpu.destroy_buffer(pool.vertices);
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
