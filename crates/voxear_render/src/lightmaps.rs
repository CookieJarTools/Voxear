//! Baked lighting: lightmaps on the Objs that keep still, and light probes
//! for everything else, as a level's bake made them (see `lightmapper` for
//! the bake itself).
//!
//! - An Obj with a [`Lightmap`] is still for lighting: a bake traces the
//!   light reaching each texel of its meshes' lightmap UVs and keeps it in
//!   one of the level's lightmaps, which shading reads in place of the probes
//!   round the camera. Its `cast` says whether it shades and bounces light
//!   in the bake, `receive` whether it takes a lightmap or is lit by the light
//!   probes, as a moving Obj is.
//! - A [`LightProbeGroup`] places light probes, points that keep the light
//!   reaching them from every way as nine spherical harmonics a colour
//!   (L2), by hand or filling its box. Each Obj without a lightmap is lit by
//!   the probes round it, blended for where it is each frame.
//! - A [`Light`](crate::Light)'s `mode` says what of it is baked: nothing
//!   (`Realtime`), its bounce (`Mixed`), or its bounce and its direct light
//!   on lightmapped surfaces (`Baked`).
//! - The level's `[post.global_illumination] source` says whether the bake
//!   lights it, the probes round the camera, or both, each then holding the
//!   light the other does not, so none is counted twice.
//!
//! What a bake made is put on the world by the engine as it loads the level:
//! [`BakedLighting`], a resource with the lightmaps and probes, and a
//! [`LightmapBake`] on each Obj, saying where in which lightmap each of its
//! parts is.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ash::vk;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity};
use voxear_rhi::{Bindless, Buffer, Gpu, MemoryLocation, TextureId};

use crate::assets::Assets;
use crate::gpu::{
    BAKED_LIGHTMAP, BAKED_PROBES, BAKED_RECEIVES, GpuBakedInstance, GpuInstance, INSTANCE_IMPOSTOR,
    INSTANCE_NO_BAKE_CAST, INSTANCE_RAYS_ONLY, NO_TEXTURE,
};
use crate::scene::Owner;

/// Marks an Obj's Mesh Renderer or Model Renderer as still for lighting, so
/// a bake gives it a lightmap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lightmap {
    /// Multiplies the level's texels a metre for this Obj: 2 for twice the
    /// detail each way, 0.5 for half.
    pub scale: f32,
    /// Whether it shades and bounces light in the bake.
    pub cast: bool,
    /// Whether it takes a lightmap; without, the light probes light it.
    pub receive: bool,
}

impl Component for Lightmap {}

impl Default for Lightmap {
    fn default() -> Self {
        Self { scale: 1.0, cast: true, receive: true }
    }
}

/// What of a light a bake keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightMode {
    /// Nothing: it lights in real time, and only the probes round the camera
    /// take its bounce. For lights that move or change.
    Realtime,
    /// Its bounce: it lights directly in real time, with its shadows and
    /// shine, and the bake keeps the light it bounces.
    #[default]
    Mixed,
    /// Its bounce, and its direct light on lightmapped surfaces, which then
    /// leave it out; moving Objs are still lit by it in real time.
    Baked,
}

impl LightMode {
    pub const ALL: [Self; 3] = [Self::Realtime, Self::Mixed, Self::Baked];

    pub fn name(self) -> &'static str {
        match self {
            Self::Realtime => "Realtime",
            Self::Mixed => "Mixed",
            Self::Baked => "Baked",
        }
    }

    /// Its number in the shaders and the frame's light order: baked first.
    pub(crate) fn order(self) -> u32 {
        match self {
            Self::Baked => 0,
            Self::Mixed => 1,
            Self::Realtime => 2,
        }
    }
}

/// Places light probes: the points in `points`, in the Obj's own space, and
/// with a `fill`, a grid filling the Obj's unit cube as its Transform
/// places, turns and sizes it.
#[derive(Clone, Debug, PartialEq)]
pub struct LightProbeGroup {
    pub points: Vec<Vec3>,
    pub fill: Option<ProbeFill>,
}

impl Component for LightProbeGroup {}

impl Default for LightProbeGroup {
    fn default() -> Self {
        Self { points: Vec::new(), fill: Some(ProbeFill::default()) }
    }
}

/// A grid of probes filling a Light Probe Group's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeFill {
    /// How far apart the probes are, in metres; the box's sides each have
    /// one, and as many between as fit.
    pub spacing: f32,
}

impl Default for ProbeFill {
    fn default() -> Self {
        Self { spacing: 2.0 }
    }
}

/// The most probes a group fills its box with, so a large box at a small
/// spacing cannot ask for millions.
pub const MOST_FILLED_PROBES: usize = 4096;

impl LightProbeGroup {
    /// Where its probes are in the world, for an Obj placed by `matrix`:
    /// its points, then its grid's, each probe once.
    pub fn probes(&self, matrix: &voxear_core::math::Mat4) -> Vec<Vec3> {
        let mut found: Vec<Vec3> = self.points.iter().map(|&p| matrix.transform_point3(p)).collect();
        if let Some(fill) = self.fill {
            let size = Vec3::new(matrix.x_axis.length(), matrix.y_axis.length(), matrix.z_axis.length());
            let spacing = fill.spacing.max(0.05);
            let mut counts = (size / spacing).floor().as_uvec3() + 1;
            while (counts.x * counts.y * counts.z) as usize > MOST_FILLED_PROBES {
                counts = (counts.as_vec3() * 0.8).ceil().as_uvec3().max(voxear_core::math::UVec3::ONE);
            }
            for z in 0..counts.z {
                for y in 0..counts.y {
                    for x in 0..counts.x {
                        let along = |i: u32, n: u32| if n > 1 { i as f32 / (n - 1) as f32 - 0.5 } else { 0.0 };
                        let local = Vec3::new(along(x, counts.x), along(y, counts.y), along(z, counts.z));
                        found.push(matrix.transform_point3(local));
                    }
                }
            }
        }
        found
    }
}

/// Where in the level's lightmaps an Obj's parts are, as its bake put them:
/// one for each part of its Model Renderer, or one for its Mesh Renderer;
/// none for a part the bake gave no lightmap.
#[derive(Clone, Debug, PartialEq)]
pub struct LightmapBake(pub Arc<[Option<LightmapPlace>]>);

impl Component for LightmapBake {}

/// A part's place in a lightmap: which, and how its lightmap UVs are scaled
/// and moved into it (x and y scale, z and w offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapPlace {
    pub atlas: u32,
    pub scale_offset: [f32; 4],
}

/// One of a bake's lightmaps: its light, BC6H, and with a directional bake
/// which way its light mostly comes from, BC7, both `width` by `height`.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedAtlas {
    pub width: u32,
    pub height: u32,
    pub light: Vec<u8>,
    pub directions: Option<Vec<u8>>,
}

/// The light reaching a point from every way, as nine spherical harmonics a
/// colour (L2), already turned into the light a surface facing each way
/// takes (convolved with its cosine and divided by pi): shading reads it as
/// it reads the probes round the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProbeLight(pub [[f32; 3]; 9]);

/// Each band's share of the light a surface takes, the cosine's harmonics
/// over pi: 1, 2/3 and 1/4.
pub const BAND_SHARES: [f32; 3] = [1.0, 2.0 / 3.0, 0.25];

/// The nine real spherical harmonics, L2, at a direction, in the order the
/// shaders keep them.
pub fn harmonics(d: Vec3) -> [f32; 9] {
    [
        0.282_095,
        0.488_603 * d.y,
        0.488_603 * d.z,
        0.488_603 * d.x,
        1.092_548 * d.x * d.y,
        1.092_548 * d.y * d.z,
        0.315_392 * (3.0 * d.z * d.z - 1.0),
        1.092_548 * d.x * d.z,
        0.546_274 * (d.x * d.x - d.y * d.y),
    ]
}

impl ProbeLight {
    /// The light a surface facing `normal` takes.
    pub fn irradiance(&self, normal: Vec3) -> Vec3 {
        let y = harmonics(normal.normalize_or(Vec3::Y));
        let sum = self.0.iter().zip(y).fold(Vec3::ZERO, |sum, (c, y)| sum + Vec3::from(*c) * y);
        sum.max(Vec3::ZERO)
    }

    /// The light arriving from every way, averaged: what a surface facing
    /// any way takes, give or take.
    pub fn average(&self) -> Vec3 {
        Vec3::from(self.0[0]) * 0.282_095
    }

    /// From harmonics of the light reaching the point, as a bake projects
    /// them: each band scaled to what a surface takes.
    pub fn from_radiance(coefficients: [[f32; 3]; 9]) -> Self {
        let band = |i: usize| match i {
            0 => 0,
            1..=3 => 1,
            _ => 2,
        };
        Self(std::array::from_fn(|i| coefficients[i].map(|c| c * BAND_SHARES[band(i)])))
    }

    fn add_scaled(&mut self, other: &ProbeLight, weight: f32) {
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            for (a, b) in mine.iter_mut().zip(theirs) {
                *a += b * weight;
            }
        }
    }

    /// Packed as the shaders read it: 27 floats in seven rows of four.
    pub(crate) fn rows(&self) -> [[f32; 4]; 7] {
        let mut flat = [0.0; 28];
        for (i, c) in self.0.iter().enumerate() {
            flat[i * 3..i * 3 + 3].copy_from_slice(c);
        }
        std::array::from_fn(|row| [flat[row * 4], flat[row * 4 + 1], flat[row * 4 + 2], flat[row * 4 + 3]])
    }
}

/// How many probes round a point light it.
const BLENDED_PROBES: usize = 8;

/// A bake's light probes: where each is, and its light.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedProbes {
    pub positions: Vec<Vec3>,
    pub lights: Vec<ProbeLight>,
}

impl BakedProbes {
    /// The light at `point`, blended from the probes round it: the nearest
    /// few, each weighed less the farther it is, down to nothing at the
    /// first left out, so the light changes smoothly as a point moves
    /// between them (Franke and Nielson's modified Shepard weights). None
    /// without probes.
    pub fn light_at(&self, point: Vec3) -> Option<ProbeLight> {
        if self.positions.is_empty() {
            return None;
        }
        let mut near: Vec<(f32, usize)> =
            self.positions.iter().enumerate().map(|(i, p)| (p.distance_squared(point), i)).collect();
        let kept = BLENDED_PROBES.min(near.len());
        if near.len() > kept {
            near.select_nth_unstable_by(kept, |a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        }
        let reach = if near.len() > kept {
            near[kept].0.sqrt()
        } else {
            near[..kept].iter().map(|n| n.0).fold(0.0, f32::max).sqrt() * 1.5 + 1e-3
        };
        let mut light = ProbeLight::default();
        let mut total = 0.0;
        for &(squared, index) in &near[..kept] {
            let distance = squared.sqrt();
            if distance < 1e-4 {
                return Some(self.lights[index]);
            }
            let weight = ((reach - distance).max(0.0) / (reach * distance)).powi(2);
            light.add_scaled(&self.lights[index], weight);
            total += weight;
        }
        if total <= 0.0 {
            let nearest = near[..kept].iter().min_by(|a, b| a.0.total_cmp(&b.0))?.1;
            return Some(self.lights[nearest]);
        }
        let mut blended = ProbeLight::default();
        blended.add_scaled(&light, 1.0 / total);
        Some(blended)
    }
}

/// A lighting bake under way, as a world resource the editor keeps, for
/// scripts to read: whether one is, and how far it has gone, from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightBakeState {
    pub baking: bool,
    pub progress: f32,
}

/// A level's bake, as a world resource: its lightmaps and light probes.
/// The renderer uploads its lightmaps once each time it changes, as told
/// by `id`.
#[derive(Clone, Debug, Default)]
pub struct BakedLighting {
    pub id: u64,
    pub atlases: Arc<Vec<BakedAtlas>>,
    pub probes: Arc<BakedProbes>,
}

impl BakedLighting {
    /// A bake given an id of its own, so the renderer knows it is new.
    pub fn new(atlases: Vec<BakedAtlas>, probes: BakedProbes) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { id, atlases: Arc::new(atlases), probes: Arc::new(probes) }
    }
}

/// The most cells a probe volume has along its longest side.
const VOLUME_CELLS: f32 = 48.0;

/// A bake's probes' light resampled on a grid over them, as blended for each
/// cell's middle, keeping the first two bands (L1) of each colour: what
/// shading reads, cell by cell, for what has no baked light of its own,
/// fog, water, terrain, particles and the rays' hits, which would otherwise
/// take the open sky's light even deep indoors.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProbeVolume {
    pub origin: Vec3,
    pub cell: f32,
    pub size: [u32; 3],
    /// Three rows a cell, x fastest, then y, then z: red, green and blue,
    /// each its band 0 and its band 1's y, z and x, as `harmonics` orders them.
    pub rows: Vec<[f32; 4]>,
}

impl ProbeVolume {
    pub(crate) fn of(probes: &BakedProbes) -> Option<Self> {
        let first = *probes.positions.first()?;
        let (low, high) = probes.positions.iter().fold((first, first), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let extent = (high - low).max(Vec3::splat(0.5));
        let cell = (extent.max_element() / VOLUME_CELLS).max(0.25);
        let size = ((extent / cell).ceil().as_uvec3() + 1).max(voxear_core::math::UVec3::splat(2)).to_array();
        let mut rows = Vec::with_capacity((size[0] * size[1] * size[2] * 3) as usize);
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let at = low + Vec3::new(x as f32, y as f32, z as f32) * cell;
                    let light = probes.light_at(at).unwrap_or_default();
                    for c in 0..3 {
                        rows.push([light.0[0][c], light.0[1][c], light.0[2][c], light.0[3][c]]);
                    }
                }
            }
        }
        Some(Self { origin: low, cell, size, rows })
    }
}

/// The level's lightmaps on the GPU, uploaded once each time its bake
/// changes, its probes, and their volume.
#[derive(Default)]
pub(crate) struct BakedTextures {
    id: u64,
    /// Each lightmap's light and directions.
    textures: Vec<(TextureId, Option<TextureId>)>,
    probes: Option<Arc<BakedProbes>>,
    volume: Option<(Buffer, ProbeVolume)>,
    /// Volumes replaced, freed once the frames that read them are done.
    retired: Vec<(u64, Buffer)>,
}

/// What every view this frame lights its instances with from the bake.
#[derive(Clone, Default)]
pub(crate) struct BakedFrame {
    /// Each lightmap's light and directions, as texture numbers.
    pub textures: Arc<[(u32, u32)]>,
    pub probes: Option<Arc<BakedProbes>>,
    pub id: u64,
    /// A bake is tracing: what is not to be in it is marked.
    pub baking: bool,
    /// The probes' volume: its buffer, cells across, cell width and first
    /// cell's middle.
    pub volume: Option<(u64, [u32; 3], f32, [f32; 3])>,
}

impl BakedTextures {
    /// Uploads the world's bake's lightmaps, when they have changed since
    /// the last frame, and frees those before.
    pub(crate) fn update(
        &mut self,
        world: &voxear_ecs::World,
        (gpu, bindless): (&Gpu, &mut Bindless),
        assets: &mut Assets,
        frame: u64,
    ) -> Result<()> {
        let finished: Vec<Buffer> = {
            let (done, kept): (Vec<_>, Vec<_>) = self.retired.drain(..).partition(|(at, _)| *at + 4 <= frame);
            self.retired = kept;
            done.into_iter().map(|(_, buffer)| buffer).collect()
        };
        for buffer in finished {
            gpu.destroy_buffer(buffer);
        }
        let baked = world.resource::<BakedLighting>();
        let id = baked.map_or(0, |baked| baked.id);
        if id == self.id {
            return Ok(());
        }
        if let Some((buffer, _)) = self.volume.take() {
            self.retired.push((frame, buffer));
        }
        for (light, directions) in self.textures.drain(..) {
            assets.free_texture(light, frame);
            if let Some(directions) = directions {
                assets.free_texture(directions, frame);
            }
        }
        self.id = id;
        self.probes = baked.map(|baked| baked.probes.clone()).filter(|probes| !probes.positions.is_empty());
        let Some(baked) = baked else { return Ok(()) };
        if let Some(volume) = ProbeVolume::of(&baked.probes) {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
            let size = (volume.rows.len() * 16) as u64;
            let buffer = gpu.create_buffer("light probe volume", size, usage, MemoryLocation::GpuOnly)?;
            gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&volume.rows))?;
            tracing::info!(
                "light probe volume: {:?} cells {:.2} m wide from {}",
                volume.size,
                volume.cell,
                volume.origin
            );
            self.volume = Some((buffer, volume));
        }
        for (index, atlas) in baked.atlases.iter().enumerate() {
            let size = (atlas.width, atlas.height);
            let blocks = (atlas.width.div_ceil(4) * atlas.height.div_ceil(4) * 16) as usize;
            if atlas.light.len() != blocks || atlas.directions.as_ref().is_some_and(|d| d.len() != blocks) {
                tracing::warn!("lightmap {index} is not the size it says, so is left out");
                continue;
            }
            let name = format!("lightmap {index}");
            let light = assets.add_texture_mips(
                gpu,
                bindless,
                &name,
                size,
                vk::Format::BC6H_UFLOAT_BLOCK,
                &[atlas.light.clone()],
            )?;
            let directions = match &atlas.directions {
                Some(bytes) => Some(assets.add_texture_mips(
                    gpu,
                    bindless,
                    &format!("{name} directions"),
                    size,
                    vk::Format::BC7_UNORM_BLOCK,
                    &[bytes.clone()],
                )?),
                None => None,
            };
            self.textures.push((light, directions));
        }
        Ok(())
    }

    pub(crate) fn frame(&self, baking: bool) -> BakedFrame {
        let textures =
            self.textures.iter().map(|(light, directions)| (light.0, directions.map_or(NO_TEXTURE, |d| d.0))).collect();
        let volume = self.volume.as_ref().map(|(buffer, v)| (buffer.address, v.size, v.cell, v.origin.to_array()));
        BakedFrame { textures, probes: self.probes.clone(), id: self.id, baking, volume }
    }

    /// Frees what it holds on the GPU but its textures, which the assets free.
    pub(crate) fn destroy(&mut self, gpu: &Gpu) {
        for buffer in self.volume.take().map(|(b, _)| b).into_iter().chain(self.retired.drain(..).map(|(_, b)| b)) {
            gpu.destroy_buffer(buffer);
        }
    }
}

/// A view's instances' baked light: a buffer each frame in flight, and the
/// light probes' light round each Obj lit by them, kept while it stands still.
#[derive(Default)]
pub(crate) struct ViewBaked {
    buffers: Vec<Option<(Buffer, usize)>>,
    near: HashMap<(Entity, u32), (Vec3, u64, ProbeLight)>,
}

/// How far an Obj lit by the light probes moves before their light round it
/// is blended again, in metres.
const PROBE_MOVE: f32 = 0.02;

impl ViewBaked {
    /// Gives each instance its baked light: its part's place in a lightmap,
    /// or the light probes' light round it, and while a bake traces, marks
    /// what is not to shade or bounce light in it. Writes them into this
    /// frame's buffer and returns where it is, or 0 where none is needed.
    /// `offset` is where the view's own space has its middle in the world's.
    pub(crate) fn assign(
        &mut self,
        (gpu, world, assets): (&Gpu, &voxear_ecs::World, &Assets),
        frame: &BakedFrame,
        (instances, owners): (&mut [GpuInstance], &[Owner]),
        (needed, texels_per_metre, offset): (bool, f32, Vec3),
        slot: usize,
    ) -> Result<u64> {
        if !needed && !frame.baking {
            return Ok(0);
        }
        let mut rows: Vec<GpuBakedInstance> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (instance, owner) in instances.iter_mut().zip(owners) {
            instance.baked = 0;
            // The ground's and worlds' of blocks own meshes for rays stand still and shade the bake; plants do not.
            if owner.plant.is_some() || instance.flags & (INSTANCE_RAYS_ONLY | INSTANCE_IMPOSTOR) != 0 {
                if frame.baking && owner.plant.is_some() {
                    instance.flags |= INSTANCE_NO_BAKE_CAST;
                }
                continue;
            }
            let part = owner.part.unwrap_or(0);
            let lightmap = world.get::<Lightmap>(owner.entity);
            if frame.baking && !lightmap.is_some_and(|lightmap| lightmap.cast) {
                instance.flags |= INSTANCE_NO_BAKE_CAST;
            }
            let mesh = crate::MeshHandle(instance.mesh);
            let model = Mat4::from_cols_array_2d(&instance.model);
            let receives = lightmap.filter(|l| l.receive && instance.posed == 0 && assets.has_lightmap_uvs(mesh));
            let mut row = GpuBakedInstance { lightmap: NO_TEXTURE, directions: NO_TEXTURE, ..Default::default() };
            if let Some(lightmap) = receives {
                row.flags |= BAKED_RECEIVES;
                row.density = density(assets, mesh, &model, texels_per_metre * lightmap.scale.max(0.0));
            }
            let bake = receives.and_then(|_| world.get::<LightmapBake>(owner.entity));
            let place = bake.and_then(|bake| bake.0.get(part as usize).copied().flatten());
            match place.and_then(|place| frame.textures.get(place.atlas as usize).map(|t| (place, *t))) {
                Some((place, (light, directions))) => {
                    row.flags |= BAKED_LIGHTMAP;
                    (row.lightmap, row.directions, row.scale_offset) = (light, directions, place.scale_offset);
                }
                None => {
                    if let Some(probes) = &frame.probes {
                        let centre = assets.mesh_sphere(mesh).map_or(Vec3::ZERO, |(centre, _)| centre);
                        let at = model.transform_point3(centre) + offset;
                        let key = (owner.entity, part + if owner.far_side { 1 << 31 } else { 0 });
                        seen.insert(key);
                        let kept =
                            self.near.get(&key).filter(|(was, id, _)| *id == frame.id && was.distance(at) < PROBE_MOVE);
                        let light = match kept {
                            Some((.., light)) => *light,
                            None => {
                                let light = probes.light_at(at).unwrap_or_default();
                                self.near.insert(key, (at, frame.id, light));
                                light
                            }
                        };
                        row.flags |= BAKED_PROBES;
                        row.sh = light.rows();
                    }
                }
            }
            if row.flags == 0 {
                continue;
            }
            rows.push(row);
            instance.baked = rows.len() as u32;
        }
        self.near.retain(|key, _| seen.contains(key));
        if rows.is_empty() {
            return Ok(0);
        }
        while self.buffers.len() <= slot {
            self.buffers.push(None);
        }
        let room = self.buffers[slot].as_ref().map_or(0, |(_, room)| *room);
        if room < rows.len() {
            if let Some((old, _)) = self.buffers[slot].take() {
                gpu.destroy_buffer(old);
            }
            let room = rows.len().next_power_of_two().max(64);
            let size = (room * size_of::<GpuBakedInstance>()) as u64;
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let buffer = gpu.create_buffer("baked instances", size, usage, MemoryLocation::CpuToGpu)?;
            self.buffers[slot] = Some((buffer, room));
        }
        let (buffer, _) = self.buffers[slot].as_ref().expect("made above");
        buffer.write(0, &rows);
        Ok(buffer.address)
    }

    /// Its buffers, to free once frames in flight are done with them.
    pub(crate) fn take_buffers(&mut self) -> Vec<Buffer> {
        self.buffers.drain(..).flatten().map(|(buffer, _)| buffer).collect()
    }
}

/// Lightmap texels across a mesh's lightmap UVs' square at `texels_per_metre`,
/// as a bake gives it, roughly, from its areas and how `model` sizes it.
fn density(assets: &Assets, mesh: crate::MeshHandle, model: &Mat4, texels_per_metre: f32) -> f32 {
    let Some((uv, surface)) = assets.lightmap_areas(mesh) else { return 0.0 };
    let (scale, ..) = model.to_scale_rotation_translation();
    let stretch = (scale.x * scale.y * scale.z).abs().powf(2.0 / 3.0);
    ((surface * stretch / uv.max(1e-9)).sqrt() * texels_per_metre).max(1.0)
}

#[cfg(test)]
mod tests {
    use voxear_core::math::{Mat4, Quat};

    use super::*;

    #[test]
    fn a_groups_fill_reaches_its_boxs_sides() {
        let group = LightProbeGroup { points: vec![Vec3::new(0.0, 0.4, 0.0)], fill: Some(ProbeFill { spacing: 2.0 }) };
        let matrix = Mat4::from_scale_rotation_translation(Vec3::new(4.0, 2.0, 1.0), Quat::IDENTITY, Vec3::X * 10.0);
        let probes = group.probes(&matrix);
        assert_eq!(probes.len(), 1 + 3 * 2 * 1);
        assert_eq!(probes[0], Vec3::new(10.0, 0.8, 0.0), "its own point, placed with it");
        assert!(probes.contains(&Vec3::new(8.0, -1.0, 0.0)) && probes.contains(&Vec3::new(12.0, 1.0, 0.0)));
        let huge = Mat4::from_scale(Vec3::splat(1000.0));
        assert!(LightProbeGroup::default().probes(&huge).len() <= MOST_FILLED_PROBES);
    }

    #[test]
    fn light_from_one_way_lights_what_faces_it() {
        // Radiance of 1 from straight up, over a small cone, projected as a bake does.
        let mut coefficients = [[0.0; 3]; 9];
        let samples = 4000;
        for i in 0..samples {
            let y = 1.0 - (i as f32 + 0.5) * 2.0 / samples as f32;
            let ring = (1.0 - y * y).max(0.0).sqrt();
            let angle = i as f32 * 2.399_963;
            let d = Vec3::new(angle.cos() * ring, y, angle.sin() * ring);
            let radiance = if d.y > 0.0 { 1.0 } else { 0.0 };
            for (c, h) in coefficients.iter_mut().zip(harmonics(d)) {
                c.iter_mut().for_each(|c| *c += radiance * h * 4.0 * std::f32::consts::PI / samples as f32);
            }
        }
        let light = ProbeLight::from_radiance(coefficients);
        // A sky of 1 over the top half: a floor takes 1, a ceiling nothing, a wall a half.
        assert!((light.irradiance(Vec3::Y).x - 1.0).abs() < 0.08, "{}", light.irradiance(Vec3::Y));
        assert!(light.irradiance(-Vec3::Y).x < 0.08, "{}", light.irradiance(-Vec3::Y));
        assert!((light.irradiance(Vec3::X).x - 0.5).abs() < 0.05, "{}", light.irradiance(Vec3::X));
    }

    #[test]
    fn probes_blend_smoothly_and_meet_each_probe_at_it() {
        let lit = |v: f32| ProbeLight(std::array::from_fn(|i| if i == 0 { [v; 3] } else { [0.0; 3] }));
        let positions: Vec<Vec3> = (0..20).map(|i| Vec3::new(i as f32, 0.0, (i % 3) as f32)).collect();
        let lights = (0..20).map(|i| lit(i as f32)).collect();
        let probes = BakedProbes { positions: positions.clone(), lights };
        assert_eq!(probes.light_at(positions[5]).unwrap(), lit(5.0), "a probe's own light where it is");
        let mut last = probes.light_at(Vec3::new(4.0, 0.0, 1.0)).unwrap().0[0][0];
        for step in 1..200 {
            let point = Vec3::new(4.0 + step as f32 * 0.01, 0.0, 1.0);
            let now = probes.light_at(point).unwrap().0[0][0];
            assert!((now - last).abs() < 0.12, "a jump from {last} to {now} at {point}");
            last = now;
        }
        assert!(BakedProbes::default().light_at(Vec3::ZERO).is_none());
    }

    #[test]
    fn a_probe_volume_covers_the_probes_and_keeps_their_light_where_they_are() {
        let lit = |v: f32| ProbeLight(std::array::from_fn(|i| if i < 4 { [v, v * 0.5, 0.0] } else { [9.0; 3] }));
        let positions = vec![Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 4.0, 6.0)];
        let probes = BakedProbes { positions, lights: vec![lit(1.0), lit(3.0), lit(2.0)] };
        let volume = ProbeVolume::of(&probes).unwrap();
        assert_eq!(volume.origin, Vec3::ZERO);
        let [x, y, z] = volume.size;
        assert!(
            x as f32 * volume.cell >= 10.0 && y >= 2 && z as f32 * volume.cell >= 6.0,
            "{:?} at {}",
            volume.size,
            volume.cell
        );
        assert_eq!(volume.rows.len() as u32, x * y * z * 3);
        assert_eq!(volume.rows[0], [1.0, 1.0, 1.0, 1.0], "red's L0 and L1 at the first probe");
        assert_eq!(volume.rows[1], [0.5; 4], "green's");
        assert!(ProbeVolume::of(&BakedProbes::default()).is_none());
    }
}
