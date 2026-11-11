//! Foliage in great numbers, and pushing through it.
//!
//! A **Foliage** component on an Obj holds one plant, its mesh and
//! material, and every place it grows, thousands of them without an Obj
//! each: placed by hand, or scattered over an area by density. The renderer
//! builds each plant's instance once, in chunks `CHUNK` metres across, and
//! each frame adds the chunks within the plants' draw distance to the
//! frame's instances, so the GPU culls them against the view and the depth
//! pyramid, picks their LODs, and moves them in the wind as any mesh. Near
//! their draw distance plants fade out by dithering, and past their shadow
//! distance they cast no shadow.
//!
//! **Pushing through foliage**: characters, vehicles and anything else with
//! a Foliage Interaction bend grass and plants out of their way, and the
//! plants spring back after them, as The Vegetation Engine's interaction
//! does in Unity.
//!
//! Pushes are kept in a grid of cells round the camera, `PUSH_CELLS` each
//! way `PUSH_CELL` metres apart, in a buffer kept from frame to frame. Each
//! frame, **Foliage pushes** (`shaders/foliage_push.slang`) lets every
//! cell's push spring back a little and stamps where each pusher is now, so
//! a trail stays behind something running through a field. Plants read the
//! grid where they stand (`shaders/wind.slang`) and lean away. A world cell
//! keeps its place in the grid as the camera moves, as the GI's probes do;
//! pushes left from far away have sprung back long before the camera comes
//! round to their cells again.

use std::{collections::HashMap, ops::Range, sync::Arc};

use serde::{Deserialize, Serialize};
use voxear_core::math::{I64Vec3, Mat3, Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::vk;
pub use voxear_scene::plants::{Plant, PlantSet, Plants, scatter};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    MaterialHandle, MeshHandle,
    gpu::{GpuInstance, GpuPushes, INSTANCE_IMPOSTOR, INSTANCE_NO_SHADOW, MAX_PUSHERS, PUSH_CELL, PUSH_CELLS, Push},
    graph::{BufferAccess, BufferId, FrameGraph},
    passes::bind,
    pipelines::Pipelines,
    scene::{ExtractedFrame, Owner},
};

/// How wide each chunk of plants is, in metres: plants are added to the
/// frame, and cast shadows or not, a chunk at a time.
const CHUNK: f32 = 16.0;

/// One plant, many times over: a mesh and a material, drawn wherever the
/// Obj's `Plants` grow (see `voxear_scene::plants`), but for those taken
/// away, which are skipped, so nothing is built again. The plants share
/// one LOD, wind and push setting each, from their mesh and material.
#[derive(Clone, Debug, PartialEq)]
pub struct Foliage {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    /// How far from the camera plants are drawn, in metres.
    pub draw_distance: f32,
    /// Over how many metres short of the draw distance they fade out.
    pub fade_distance: f32,
    /// How far from the camera they cast shadows, in metres; 0 for none.
    pub shadow_distance: f32,
    /// The colours plants are tinted between, as Unity's grass is, in
    /// sRGB: each plant takes some of each by noise over the level, in
    /// patches `color_spread` metres across. White leaves them as made.
    pub healthy_color: [f32; 3],
    pub dry_color: [f32; 3],
    pub color_spread: f32,
    /// Whether its far plants are drawn as impostors, flat pictures of them
    /// that turn to face the camera (see `impostors`), and from how far, in
    /// metres, when it says so.
    pub impostors: Impostors,
    pub impostor_distance: f32,
}

impl Component for Foliage {}

/// When a Foliage's plants are drawn as impostors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Impostors {
    /// As the engine chooses: for plants as large as trees, past a third of
    /// the draw distance, and never for grass.
    #[default]
    Auto,
    /// Never: always the plant's own mesh.
    Off,
    /// From the Foliage's `impostor_distance`.
    From,
}

impl Impostors {
    pub const ALL: [Impostors; 3] = [Impostors::Auto, Impostors::Off, Impostors::From];

    pub fn name(self) -> &'static str {
        match self {
            Impostors::Auto => "Auto",
            Impostors::Off => "Off",
            Impostors::From => "From distance",
        }
    }
}

/// How large a plant must be across, in metres, before the engine gives it
/// impostors of its own accord, and the nearest it draws them from.
const AUTO_IMPOSTOR_SIZE: f32 = 2.0;
const AUTO_IMPOSTOR_DISTANCE: f32 = 40.0;

impl Foliage {
    /// From how far its plants are impostors, if at all: as it says, or
    /// where the engine chooses, plants `AUTO_IMPOSTOR_SIZE` across or more
    /// from `AUTO_IMPOSTOR_DISTANCE` or past a third of the draw distance.
    fn impostors_from(&self, plants: &[Plant], mesh_radius: f32) -> Option<f32> {
        match self.impostors {
            Impostors::Off => None,
            Impostors::From => Some(self.impostor_distance.max(0.0)),
            Impostors::Auto => {
                let typical = plants.iter().map(|plant| plant.scale).sum::<f32>() / plants.len().max(1) as f32;
                (mesh_radius * 2.0 * typical >= AUTO_IMPOSTOR_SIZE)
                    .then(|| AUTO_IMPOSTOR_DISTANCE.max(self.draw_distance / 3.0))
            }
        }
    }

    /// What its instances are built from besides its plants and place: its
    /// mesh, material, distances and colours, to tell when to build again.
    fn look(&self) -> (MeshHandle, MaterialHandle, [u32; 10]) {
        let [a, b, c] = self.healthy_color.map(f32::to_bits);
        let [d, e, f] = self.dry_color.map(f32::to_bits);
        let distances = [self.draw_distance, self.fade_distance, self.shadow_distance].map(f32::to_bits);
        let [g, h, i] = distances;
        (self.mesh, self.material, [g, h, i, a, b, c, d, e, f, self.color_spread.to_bits()])
    }

    /// A plant's tint standing at `at` in a world whose middle is `origin`
    /// from the level's, packed as the GPU reads it (RGBA8 in an instance's
    /// `fade.z`), or 0 for none. Its patches are read from the level's middle,
    /// as the shaders' are (`levelPosition`), so they stay where they are as
    /// the world's middle moves.
    fn tint(&self, at: Vec3, origin: I64Vec3) -> u32 {
        if self.healthy_color == [1.0; 3] && self.dry_color == [1.0; 3] {
            return 0;
        }
        let spread = self.color_spread.max(0.1);
        let level = (at.as_dvec3() + origin.as_dvec3()).to_array();
        let wrap = crate::gpu::ORIGIN_WRAP;
        let level = level.map(|metres| (metres - (metres / wrap + 0.5).floor() * wrap) as f32);
        let dry = smooth_noise(level[0] / spread, level[2] / spread);
        let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        let [r, g, b] = std::array::from_fn(|i| {
            let mixed = linear(self.healthy_color[i]) * (1.0 - dry) + linear(self.dry_color[i]) * dry;
            (mixed.clamp(0.0, 1.0) * 255.0).round() as u32
        });
        r | g << 8 | b << 16 | 255 << 24
    }
}

/// Smooth value noise from 0 to 1, varying over about one unit.
fn smooth_noise(x: f32, z: f32) -> f32 {
    let hash = |x: f32, z: f32| {
        let n = (x * 127.1 + z * 311.7).sin() * 43_758.547;
        n - n.floor()
    };
    let (cx, cz) = (x.floor(), z.floor());
    let (fx, fz) = (x - cx, z - cz);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = lerp(hash(cx, cz), hash(cx + 1.0, cz), ux);
    let bottom = lerp(hash(cx, cz + 1.0), hash(cx + 1.0, cz + 1.0), ux);
    lerp(top, bottom, uz)
}

/// One chunk of a Foliage's plants: where it is, how far round it they
/// reach, and which of the built instances are in it.
struct Chunk {
    centre: Vec3,
    radius: f32,
    plants: Range<usize>,
}

/// A Foliage's instances, built once, kept until it changes.
struct Built {
    /// What it was built from: the plants' list, where the Obj was, its
    /// mesh and material and distances.
    plants: Arc<Vec<Plant>>,
    model: Mat4,
    look: (MeshHandle, MaterialHandle, [u32; 10]),
    chunks: Vec<Chunk>,
    instances: Vec<GpuInstance>,
    /// Each instance's plant, by its place in the Foliage's list.
    numbers: Vec<u32>,
    /// Each plant's impostor, in the same order, when it has one baked, and
    /// how far from the camera it takes over.
    impostors: Vec<GpuInstance>,
    impostor_from: f32,
    /// The impostor it was built with, to build again once one is baked.
    impostor: Option<crate::Impostor>,
}

/// Every Foliage's instances, built once and added to each frame a chunk
/// at a time.
#[derive(Default)]
pub(crate) struct FoliageCache {
    built: HashMap<Entity, Built>,
}

impl FoliageCache {
    /// Moves every Foliage's built plants by `by`, as the world's origin
    /// moves its Obj (see `voxear_scene::WorldOrigin`), so none is built
    /// again for it.
    pub fn shift(&mut self, by: Vec3) {
        let moved = by.extend(0.0);
        for built in self.built.values_mut() {
            built.model.w_axis += moved;
            for chunk in &mut built.chunks {
                chunk.centre += by;
            }
            for instance in built.instances.iter_mut().chain(&mut built.impostors) {
                for model in [&mut instance.model, &mut instance.prev_model] {
                    model[3] = (Vec4::from(model[3]) + moved).to_array();
                }
            }
        }
    }

    /// Adds the plants within their draw distance of `camera` to the frame,
    /// building the instances of any Foliage that changed. `sphere` gives a
    /// mesh's bounding sphere.
    pub fn gather(
        &mut self,
        world: &World,
        camera: Vec3,
        extracted: &mut ExtractedFrame,
        sphere: &dyn Fn(MeshHandle) -> Option<(Vec3, f32)>,
        impostors: &crate::impostors::ImpostorTable,
    ) {
        let origin = voxear_scene::WorldOrigin::of(world).0;
        let mut seen = Vec::new();
        let found = world
            .query_ref::<(Entity, &Foliage, &Plants, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>();
        // A Foliage switched off grows nothing, and what it had built goes, as for one taken away.
        let switched = voxear_scene::Switched::of(world);
        let on = |entity: Entity| switched.is_on(world, entity, voxear_scene::Switch::Foliage);
        let found = found.into_iter().filter(|(entity, ..)| on(*entity));
        for (entity, foliage, plants, transform, placed) in found {
            seen.push(entity);
            let model = placed.map_or_else(
                || Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
                |w| w.0,
            );
            let look = foliage.look();
            let Some(bounds) = sphere(foliage.mesh) else { continue };
            let impostor_from = foliage.impostors_from(&plants.plants, bounds.1);
            let impostor = match (impostor_from, impostors.quad) {
                (Some(_), quad) => impostors.get_or_ask(foliage.mesh, foliage.material).zip(quad),
                (None, _) => None,
            };
            let stale = self.built.get(&entity).is_none_or(|built| {
                !Arc::ptr_eq(&built.plants, &plants.plants)
                    || built.model != model
                    || built.look != look
                    || built.impostor != impostor.map(|(i, _)| i)
                    || built.impostor_from != impostor_from.unwrap_or(f32::MAX)
            });
            if stale {
                let from = impostor_from.unwrap_or(f32::MAX);
                let made = build(foliage, &plants.plants, model, look, bounds, impostor, from, origin);
                self.built.insert(entity, made);
            }
            let built = &self.built[&entity];
            let draw = foliage.draw_distance.max(0.0);
            for chunk in &built.chunks {
                let near = (chunk.centre.distance(camera) - chunk.radius).max(0.0);
                if near > draw {
                    continue;
                }
                let shadowless = near > foliage.shadow_distance;
                let far = chunk.centre.distance(camera) + chunk.radius;
                // Where the whole chunk is nearer than impostors start, no plant needs looking at on its own.
                let mixed = !built.impostors.is_empty() && far > built.impostor_from;
                for index in chunk.plants.clone() {
                    if plants.removed.contains(built.numbers[index]) {
                        continue;
                    }
                    let mut instance = built.instances[index];
                    if mixed {
                        let at = Vec3::new(instance.model[3][0], instance.model[3][1], instance.model[3][2]);
                        if at.distance(camera) > built.impostor_from {
                            instance = built.impostors[index];
                        }
                    }
                    if shadowless {
                        instance.flags |= INSTANCE_NO_SHADOW;
                    }
                    extracted.instances.push(instance);
                    extracted.entities.push(Owner { entity, plant: Some(built.numbers[index]), part: None });
                }
            }
        }
        self.built.retain(|entity, _| seen.contains(entity));
    }
}

/// A Foliage's instances, sorted into chunks.
fn build(
    foliage: &Foliage,
    plants: &Arc<Vec<Plant>>,
    model: Mat4,
    look: (MeshHandle, MaterialHandle, [u32; 10]),
    bounds: (Vec3, f32),
    impostor: Option<(crate::Impostor, MeshHandle)>,
    impostor_from: f32,
    origin: I64Vec3,
) -> Built {
    let (mesh_centre, mesh_radius) = bounds;
    let fade = [(foliage.draw_distance - foliage.fade_distance).max(0.0), foliage.draw_distance.max(0.0), 0.0, 0.0];
    let instance = |plant: &Plant| {
        let placed = plant.matrix(model);
        let normal = Mat3::from_mat4(placed).inverse().transpose();
        let tint = f32::from_bits(foliage.tint(placed.w_axis.truncate(), origin));
        let [near, far, ..] = fade;
        GpuInstance {
            model: placed.to_cols_array_2d(),
            prev_model: placed.to_cols_array_2d(),
            normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
            mesh: foliage.mesh.0,
            material: foliage.material.0,
            flags: 0,
            lod_bias: 0.0,
            lod_screen: [[0.0; 4]; 2],
            fade: [near, far, tint, 0.0],
            ..GpuInstance::default()
        }
    };
    // Each plant's chunk, by where it stands in the world.
    let placed: Vec<(usize, Vec3)> = plants
        .iter()
        .enumerate()
        .map(|(number, plant)| (number, model.transform_point3(Vec3::from(plant.position))))
        .collect();
    let mut keyed: Vec<((i32, i32), usize)> = placed
        .iter()
        .map(|&(number, at)| (((at.x / CHUNK).floor() as i32, (at.z / CHUNK).floor() as i32), number))
        .collect();
    keyed.sort_unstable();
    let scale = model.to_scale_rotation_translation().0.max_element();
    let mut built = Built {
        plants: plants.clone(),
        model,
        look,
        chunks: Vec::new(),
        instances: Vec::with_capacity(keyed.len()),
        numbers: Vec::with_capacity(keyed.len()),
        impostors: Vec::new(),
        impostor_from,
        impostor: impostor.map(|(i, _)| i),
    };
    for group in keyed.chunk_by(|a, b| a.0 == b.0) {
        let start = built.instances.len();
        let mut low = Vec3::splat(f32::MAX);
        let mut high = Vec3::splat(f32::MIN);
        let mut reach: f32 = 0.0;
        for &(_, number) in group {
            let plant = &plants[number];
            let instance = instance(plant);
            let centre = Mat4::from_cols_array_2d(&instance.model).transform_point3(mesh_centre);
            reach = reach.max(mesh_radius * plant.scale * scale);
            low = low.min(centre);
            high = high.max(centre);
            if let Some((impostor, quad)) = impostor {
                built.impostors.push(impostor_instance(&instance, impostor, quad));
            }
            built.instances.push(instance);
            built.numbers.push(number as u32);
        }
        let centre = (low + high) * 0.5;
        let radius = (high - low).length() * 0.5 + reach;
        built.chunks.push(Chunk { centre, radius, plants: start..built.instances.len() });
    }
    built
}

/// A plant's impostor in place of its mesh: the quad, sized to the sphere
/// its atlas pictured it in, there in the world, still turned as the plant
/// is so the impostor shows the side of it that faces the camera, and with
/// no shadow, as impostors are far off.
fn impostor_instance(plant: &GpuInstance, impostor: crate::Impostor, quad: MeshHandle) -> GpuInstance {
    let placed = Mat4::from_cols_array_2d(&plant.model);
    let (scale, rotation, _) = placed.to_scale_rotation_translation();
    let centre = placed.transform_point3(impostor.centre);
    let size = impostor.radius * scale.max_element();
    let model = Mat4::from_scale_rotation_translation(Vec3::splat(size), rotation, centre);
    let normal = Mat3::from_mat4(model).inverse().transpose();
    GpuInstance {
        model: model.to_cols_array_2d(),
        prev_model: model.to_cols_array_2d(),
        normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
        mesh: quad.0,
        material: impostor.material.0,
        flags: plant.flags | INSTANCE_NO_SHADOW | INSTANCE_IMPOSTOR,
        ..*plant
    }
}

/// Something that bends foliage out of its way as it passes through:
/// grass and plants within `radius` of the Obj lean away from it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FoliageInteraction {
    /// How far round the Obj plants are pushed, in metres.
    pub radius: f32,
    /// How hard: 1 bends grass well over, 0.3 just stirs it.
    pub strength: f32,
}

impl Component for FoliageInteraction {}

impl Default for FoliageInteraction {
    fn default() -> Self {
        Self { radius: 0.6, strength: 1.0 }
    }
}

/// The pushers as shaders read them, the nearest to the camera first, at
/// most `MAX_PUSHERS`, with the grid placed round the camera.
pub(crate) fn pushes(camera: Vec3, mut pushers: Vec<(Vec3, FoliageInteraction)>) -> GpuPushes {
    pushers.sort_by(|a, b| a.0.distance_squared(camera).total_cmp(&b.0.distance_squared(camera)));
    let mut gpu = GpuPushes::default();
    for (slot, (at, push)) in pushers.iter().take(MAX_PUSHERS).enumerate() {
        gpu.pushers[slot] = [at.x, at.y, at.z, push.radius.max(0.01)];
        gpu.strengths[slot / 4][slot % 4] = push.strength.max(0.0);
    }
    gpu.count = pushers.len().min(MAX_PUSHERS) as u32;
    let half = (PUSH_CELLS / 2) as i32;
    gpu.origin = [(camera.x / PUSH_CELL).floor() as i32 - half, (camera.z / PUSH_CELL).floor() as i32 - half];
    gpu.cell = PUSH_CELL;
    gpu
}

/// Records the pass that springs the pushes back and stamps this frame's.
pub(crate) fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), cells: BufferId) {
    fg.add_pass("Foliage pushes", &[], &[(cells, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.foliage_push, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, (PUSH_CELLS * PUSH_CELLS).div_ceil(64), 1, 1) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scattering_covers_the_area_evenly_and_the_same_way_each_time() {
        let plants = scatter([10.0, 20.0], 2.0, 7, [0.8, 1.2]);
        assert!((plants.len() as f32 - 400.0).abs() < 40.0, "about two to a square metre: {}", plants.len());
        assert!(plants.iter().all(|p| p.position[0].abs() <= 5.0 && p.position[2].abs() <= 10.0), "inside the area");
        assert!(plants.iter().all(|p| (0.8..=1.2).contains(&p.scale)));
        let left = plants.iter().filter(|p| p.position[0] < 0.0).count() as f32;
        assert!((left / plants.len() as f32 - 0.5).abs() < 0.1, "spread across it, not bunched");
        assert_eq!(plants, scatter([10.0, 20.0], 2.0, 7, [0.8, 1.2]), "the same seed, the same plants");
        assert_ne!(plants, scatter([10.0, 20.0], 2.0, 8, [0.8, 1.2]));
    }

    #[test]
    fn the_nearest_pushers_are_kept_and_the_grid_centres_on_the_camera() {
        let push = FoliageInteraction::default();
        let far: Vec<_> = (0..MAX_PUSHERS + 4).map(|i| (Vec3::new(100.0 + i as f32, 0.0, 0.0), push)).collect();
        let near = (Vec3::new(1.0, 0.0, 1.0), FoliageInteraction { radius: 2.0, strength: 0.5 });
        let gpu = pushes(Vec3::new(10.2, 3.0, -4.9), far.into_iter().chain([near]).collect());
        assert_eq!(gpu.count as usize, MAX_PUSHERS);
        assert_eq!(gpu.pushers[0], [1.0, 0.0, 1.0, 2.0], "the nearest first");
        assert_eq!(gpu.strengths[0][0], 0.5);
        let half = (PUSH_CELLS / 2) as i32;
        assert_eq!(gpu.origin, [20 - half, -10 - half]);
    }
}
