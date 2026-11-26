//! Voxel Volumes for C#: `Voxels.Carve` and `Voxels.Fill` in Voxear.Core
//! change every endless Voxel Volume a ball falls in, `Voxels.Print`
//! prints a Voxel Print into those its box reaches, `Voxels.Raycast`
//! finds where a ray meets their ground, `Voxels.WaterAt` the water of
//! their rivers and lakes at a place, and `Voxels.Sea` turns their
//! generators' seas on and off. The engine answers, as
//! only it keeps the volumes (see `voxear_engine::voxel_volumes`); a print
//! is printed after the frame, when the engine has its project's files.

use std::cell::RefCell;
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

/// What answers `Voxels.WaterAt`: the water of an endless Voxel Volume's
/// rivers and lakes at a place of the world: the volume, how high its
/// surface stands, and its current along X, Y and Z, in metres a second.
pub type VoxelWaterAt = fn(&World, [f32; 3]) -> Option<(Entity, f32, [f32; 3])>;

/// What answers `Voxels.Sea`: given 0 or 1, turns every endless Voxel
/// Volume's generator's sea off or on, and given -1 only asks; gives the
/// height of the sea one has now, if any has.
pub type VoxelSea = fn(&mut World, i32) -> Option<f32>;

static WATER_AT: OnceLock<VoxelWaterAt> = OnceLock::new();
static SEA: OnceLock<VoxelSea> = OnceLock::new();

/// Sets what answers `Voxels.WaterAt` and `Voxels.Sea`; the first set is kept.
pub fn set_voxel_water(water_at: VoxelWaterAt, sea: VoxelSea) {
    let _ = WATER_AT.set(water_at);
    let _ = SEA.set(sea);
}

/// `Voxels.WaterAt`: 1 with the surface's height and the current in
/// `water` and the volume's Obj in `obj`; 0 where there is no such water;
/// -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_water(at: *const [f32; 3], water: *mut [f32; 4], obj: *mut u64) -> i32 {
    let Some(water_at) = WATER_AT.get() else { return -1 };
    if at.is_null() || water.is_null() || obj.is_null() {
        return -1;
    }
    let at = unsafe { *at };
    let Some((entity, height, current)) = with_world(|world| Some(water_at(world, at))).unwrap_or(None) else {
        return 0;
    };
    unsafe {
        *water = [height, current[0], current[1], current[2]];
        *obj = entity.to_bits();
    }
    1
}

/// `Voxels.Sea`: `on` 0 or 1 turns the seas off or on, -1 asks; 1 with the
/// sea's height in `level` where a volume has one now, 0 where none has,
/// -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_sea(on: i32, level: *mut f32) -> i32 {
    let Some(sea) = SEA.get() else { return -1 };
    if level.is_null() {
        return -1;
    }
    match with_world(|world| Some(sea(world, on))).unwrap_or(None) {
        Some(height) => {
            unsafe { *level = height };
            1
        }
        None => 0,
    }
}

/// A print a script asked for: the `.voxprint` by GUID or path, where its
/// model's origin goes in the world, and how it is turned, in degrees about
/// X, Y and Z.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelPrintRequest {
    pub print: String,
    pub at: [f32; 3],
    pub rotation: [f32; 3],
}

thread_local! {
    static PRINTS: RefCell<Vec<VoxelPrintRequest>> = const { RefCell::new(Vec::new()) };
}

/// The prints scripts asked for since this was last called, in order.
pub fn take_voxel_prints() -> Vec<VoxelPrintRequest> {
    PRINTS.with_borrow_mut(std::mem::take)
}

/// `Voxels.Print`: 1 once the print is asked for, printed after the frame;
/// 0 for no print named.
pub(crate) unsafe extern "C" fn voxel_print(
    print: *const u8,
    length: i32,
    at: *const [f32; 3],
    rotation: *const [f32; 3],
) -> i32 {
    if print.is_null() || length <= 0 || at.is_null() || rotation.is_null() {
        return 0;
    }
    let print = unsafe { crate::interop::utf8(print, length) }.into_owned();
    let (at, rotation) = unsafe { (*at, *rotation) };
    PRINTS.with_borrow_mut(|prints| prints.push(VoxelPrintRequest { print, at, rotation }));
    1
}
