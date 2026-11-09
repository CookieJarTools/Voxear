//! Plants for C#: `Plant` in Voxear.Core is one of a Foliage's plants, by
//! its Obj and its number (see `voxear_scene::plants`). A script asks where
//! one stands, takes it away, or cuts it down into its Foliage's Cut
//! prefab; `Plants.Query` finds those near a point; a ray that meets a
//! trunk says which plant it was.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Plants, Transform, world_matrix};

use crate::interop::{instantiate, with_world};

/// A plant as C# reads it: its Obj and its number.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlantRef {
    pub obj: u64,
    pub number: u32,
    pub _pad: u32,
}

const _: () = assert!(size_of::<PlantRef>() == 16, "PlantRef in Voxear.Core is 16 bytes");

/// Where plant `number` of an Obj's plants stands in the world, and its
/// size, while it is there.
fn standing(world: &World, entity: Entity, number: u32) -> Option<(Mat4, f32)> {
    let plants = world.get::<Plants>(entity)?;
    if !plants.has(number) {
        return None;
    }
    let plant = plants.plants[number as usize];
    let model = world_matrix(world, entity);
    Some((plant.matrix(model), plant.scale * model.to_scale_rotation_translation().0.max_element()))
}

/// Whether a plant stands, and if so where in the world, and how large.
pub(crate) unsafe extern "C" fn plant_info(obj: u64, number: u32, at: *mut [f32; 3], size: *mut f32) -> i32 {
    let Some((matrix, scale)) = with_world(|world| standing(world, Entity::from_bits(obj), number)) else { return 0 };
    if !at.is_null() {
        unsafe { *at = matrix.w_axis.truncate().to_array() };
    }
    if !size.is_null() {
        unsafe { *size = scale };
    }
    1
}

/// Takes a plant away, leaving nothing; whether it was there.
pub(crate) unsafe extern "C" fn plant_remove(obj: u64, number: u32) -> i32 {
    with_world(|world| Some(world.get_mut::<Plants>(Entity::from_bits(obj))?.remove(number))).unwrap_or(false) as i32
}

/// Cuts a plant down: takes it away and makes its Foliage's Cut prefab
/// where it stood, turned as it was and as large, with its behaviours,
/// whose handles come back as `prefab_instantiate`'s do. The prefab's
/// root goes in `out_root`, or nothing for a Foliage without one. -1 when
/// the plant was not there.
pub(crate) unsafe extern "C" fn plant_cut(obj: u64, number: u32, out_root: *mut u64) -> i32 {
    let entity = Entity::from_bits(obj);
    let taken = with_world(|world| {
        let (matrix, size) = standing(world, entity, number)?;
        let plants = world.get_mut::<Plants>(entity)?;
        plants.remove(number);
        Some((matrix, size, plants.cut.clone()))
    });
    let Some((matrix, size, cut)) = taken else { return -1 };
    let Some(reference) = cut else {
        if !out_root.is_null() {
            unsafe { out_root.write_unaligned(0) };
        }
        return 0;
    };
    let (_, rotation, position) = matrix.to_scale_rotation_translation();
    let placement = Transform::new(position, rotation, Vec3::ONE);
    let spawn = move |services: &mut dyn crate::services::ScriptServices, world: &mut World, parent, placement| {
        let prefab = services.load_prefab(&reference)?;
        let made = services.spawn_prefab(world, prefab, parent, placement)?;
        if let Some(transform) = world.get_mut::<Transform>(made.root) {
            transform.scale *= size;
        }
        Ok(made)
    };
    unsafe { instantiate("plants can only be cut down", &placement, 1, 0, out_root, spawn) }
}

/// The plants standing within `radius` metres of `centre`, nearest first,
/// as many as `capacity` into `out`; returns how many there are in all.
pub(crate) unsafe extern "C" fn plants_near(
    centre: *const [f32; 3],
    radius: f32,
    out: *mut PlantRef,
    capacity: i32,
) -> i32 {
    if centre.is_null() || radius.is_nan() || radius < 0.0 {
        return 0;
    }
    let centre = Vec3::from(unsafe { *centre });
    let found = with_world(|world| {
        let mut found: Vec<(f32, PlantRef)> = Vec::new();
        let all = world.query_ref::<(Entity, &Plants)>().without::<voxear_scene::Destroying>();
        for (entity, plants) in all {
            let model = world_matrix(world, entity);
            for (number, plant) in plants.standing() {
                let distance = plant.matrix(model).w_axis.truncate().distance(centre);
                if distance <= radius {
                    found.push((distance, PlantRef { obj: entity.to_bits(), number, _pad: 0 }));
                }
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(found)
    })
    .unwrap_or_default();
    if !out.is_null() {
        for (index, (_, plant)) in found.iter().take(capacity.max(0) as usize).enumerate() {
            unsafe { out.add(index).write_unaligned(*plant) };
        }
    }
    found.len() as i32
}
