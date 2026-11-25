//! Voxel Volumes for C#: `Voxels.Carve` and `Voxels.Fill` in Voxear.Core
//! change every endless Voxel Volume a ball falls in, and `Voxels.Raycast`
//! finds where a ray meets their ground. The engine answers, as
//! only it keeps the volumes (see `voxear_engine::voxel_volumes`).

use std::sync::OnceLock;

use voxear_ecs::{Entity, World};

use crate::interop::with_world;

/// What answers `Voxels.Carve` and `Voxels.Fill`: a ball round a place of
/// the world, its radius in metres, carved out for `fill` -1 or else filled
/// with that ground (`VoxelMaterial` in Voxear.Core, as numbered there);
/// how many volumes it reached.
pub type VoxelEdits = fn(&mut World, [f32; 3], f32, i32) -> i32;

static EDITS: OnceLock<VoxelEdits> = OnceLock::new();

/// Sets what answers `Voxels.Carve` and `Voxels.Fill`; the first set is kept.
pub fn set_voxel_edits(edits: VoxelEdits) {
    let _ = EDITS.set(edits);
}

/// `Voxels.Carve` and `Voxels.Fill`: how many volumes the ball reached, or
/// -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_edit(at: *const [f32; 3], radius: f32, fill: i32) -> i32 {
    let Some(edits) = EDITS.get() else { return -1 };
    if at.is_null() {
        return -1;
    }
    let at = unsafe { *at };
    with_world(|world| Some(edits(world, at, radius, fill))).unwrap_or(-1)
}

/// What answers `Voxels.Raycast`: where a ray from a place of the world
/// along a way, within a reach in metres, meets an endless Voxel Volume's
/// ground: the volume, the point, which way the face it met looks, and how
/// far.
pub type VoxelRaycast = fn(&World, [f32; 3], [f32; 3], f32) -> Option<(Entity, [f32; 3], [f32; 3], f32)>;

static RAYCAST: OnceLock<VoxelRaycast> = OnceLock::new();

/// Sets what answers `Voxels.Raycast`; the first set is kept.
pub fn set_voxel_raycast(raycast: VoxelRaycast) {
    let _ = RAYCAST.set(raycast);
}

/// `Voxels.Raycast`: 1 with the point, the face's way and the distance in
/// `hit` and the volume's Obj in `obj`; 0 where the ray meets none; -1
/// while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_raycast(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    reach: f32,
    hit: *mut [f32; 7],
    obj: *mut u64,
) -> i32 {
    let Some(raycast) = RAYCAST.get() else { return -1 };
    if origin.is_null() || direction.is_null() || hit.is_null() || obj.is_null() {
        return -1;
    }
    let (origin, direction) = unsafe { (*origin, *direction) };
    let found = with_world(|world| Some(raycast(world, origin, direction, reach))).unwrap_or(None);
    let Some((entity, point, normal, distance)) = found else { return 0 };
    unsafe {
        *hit = [point[0], point[1], point[2], normal[0], normal[1], normal[2], distance];
        *obj = entity.to_bits();
    }
    1
}
