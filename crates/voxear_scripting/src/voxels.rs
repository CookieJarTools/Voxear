//! Voxel Volumes for C#: `Voxels.Carve` and `Voxels.Fill` in Voxear.Core
//! change every endless Voxel Volume a ball falls in. The engine answers, as
//! only it keeps the volumes (see `voxear_engine::voxel_volumes`).

use std::sync::OnceLock;

use voxear_ecs::World;

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
