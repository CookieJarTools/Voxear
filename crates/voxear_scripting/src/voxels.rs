//! Voxel Volumes for C#: `Voxels.Carve` and `Voxels.Fill` in Voxear.Core
//! change every endless Voxel Volume a ball falls in, `Voxels.Box`,
//! `Voxels.Ramp`, `Voxels.Area` and `Voxels.Cover` those their shapes
//! reach, `Voxels.Print` prints a Voxel Print into those its box reaches,
//! `Voxels.Dig` and `Voxels.Place` set a voxel at a time, `Voxels.Raycast`
//! finds where a ray meets their ground, `Voxels.Cut` and `Voxels.Slice`
//! cut their grass and fell their trees, `Voxels.WaterAt` the water of
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

/// A shaped edit a script asked for (`Voxels.Box` and the rest), placed in
/// the world: its shape (0 a ball, 1 a box, 2 a bank, 3 an area, 4 a
/// cover), where it is, a ball's radius, a box's size and turn in degrees
/// about X, Y and Z, an area's foot and top, a cover's depth, what it
/// clears over its top, an outline's points along X and Z, what fills it
/// (-1 for air, else `VoxelMaterial` as Voxear.Core numbers it), and its
/// tint, as sRGB.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelShapeRequest {
    pub shape: i32,
    pub at: [f32; 3],
    pub radius: f32,
    pub size: [f32; 3],
    pub rotation: [f32; 3],
    pub low: f32,
    pub high: f32,
    pub depth: f32,
    pub clear: f32,
    pub points: Vec<[f32; 2]>,
    pub fill: i32,
    pub tint: Option<[u8; 3]>,
}

/// What answers `Voxels.Box` and the rest: how many volumes the edit
/// reached, or -1 for a shape or a ground there is none of.
pub type VoxelShapes = fn(&mut World, &VoxelShapeRequest) -> i32;

static SHAPES: OnceLock<VoxelShapes> = OnceLock::new();

/// Sets what answers `Voxels.Box` and the rest; the first set is kept.
pub fn set_voxel_shapes(shapes: VoxelShapes) {
    let _ = SHAPES.set(shapes);
}

/// The numbers a shaped edit is sent as, before its outline's points.
const SHAPE_NUMBERS: usize = 14;

/// A tint as sent: none for 0, else its red, green and blue in the low
/// three bytes and 1 over them.
fn tint_of(tint: u32) -> Option<[u8; 3]> {
    (tint >> 24 != 0).then(|| [(tint >> 16) as u8, (tint >> 8) as u8, tint as u8])
}

/// `Voxels.Box` and the rest: the shape, then `count` numbers (where it
/// is, a box's size, its turn, an area's foot and top, a cover's depth,
/// what it clears, a ball's radius, then an outline's points), what fills
/// it and its tint; how many volumes it reached, or -1.
pub(crate) unsafe extern "C" fn voxel_shape(shape: i32, numbers: *const f32, count: i32, fill: i32, tint: u32) -> i32 {
    let Some(shapes) = SHAPES.get() else { return -1 };
    if numbers.is_null() || count < SHAPE_NUMBERS as i32 {
        return -1;
    }
    let n = unsafe { std::slice::from_raw_parts(numbers, count as usize) };
    let request = VoxelShapeRequest {
        shape,
        at: [n[0], n[1], n[2]],
        size: [n[3], n[4], n[5]],
        rotation: [n[6], n[7], n[8]],
        low: n[9],
        high: n[10],
        depth: n[11],
        clear: n[12],
        radius: n[13],
        points: n[SHAPE_NUMBERS..].chunks_exact(2).map(|p| [p[0], p[1]]).collect(),
        fill,
        tint: tint_of(tint),
    };
    with_world(|world| Some(shapes(world, &request))).unwrap_or(-1)
}

/// What answers `Voxels.Dig`, `Voxels.Place` and `Voxels.Restore`: the
/// points of the world, what they are set to (-1 dug out, -2 let go, else a
/// `VoxelMaterial` as Voxear.Core numbers it) and a tint; how many voxels
/// were set, or -1 for a ground there is none of.
pub type VoxelSets = fn(&mut World, &[[f32; 3]], i32, Option<[u8; 3]>) -> i32;

static SETS: OnceLock<VoxelSets> = OnceLock::new();

/// Sets what answers `Voxels.Dig` and the rest; the first set is kept.
pub fn set_voxel_sets(sets: VoxelSets) {
    let _ = SETS.set(sets);
}

/// `Voxels.Dig`, `Voxels.Place` and `Voxels.Restore`.
pub(crate) unsafe extern "C" fn voxel_set(points: *const [f32; 3], count: i32, to: i32, tint: u32) -> i32 {
    let Some(sets) = SETS.get() else { return -1 };
    if points.is_null() || count <= 0 {
        return 0;
    }
    let points = unsafe { std::slice::from_raw_parts(points, count as usize) }.to_vec();
    with_world(|world| Some(sets(world, &points, to, tint_of(tint)))).unwrap_or(-1)
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

/// What answers `Voxels.WaterLook`: given a look's number (0 smooth, 1
/// voxels), draws every endless Voxel Volume's rivers and lakes so, and
/// given -1 only asks; gives the first one's look's number, if there is one.
pub type VoxelWaterLookCall = fn(&mut World, i32) -> Option<i32>;

static WATER_LOOK: OnceLock<VoxelWaterLookCall> = OnceLock::new();

/// Sets what answers `Voxels.WaterLook`; the first set is kept.
pub fn set_voxel_water_look(look: VoxelWaterLookCall) {
    let _ = WATER_LOOK.set(look);
}

/// `Voxels.WaterLook`: `look` 0 or 1 draws the water so, -1 asks; the first
/// endless volume's look's number, -1 where there is none or scripts are not
/// running.
pub(crate) unsafe extern "C" fn voxel_water_look(look: i32) -> i32 {
    let Some(call) = WATER_LOOK.get() else { return -1 };
    with_world(|world| call(world, look)).unwrap_or(-1)
}

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
    /// Whether it is fitted to the ground, and its footing (`VoxelMaterial`
    /// as Voxear.Core numbers it, -1 for none), and its tint.
    pub fit: bool,
    pub footing: i32,
    pub tint: Option<[u8; 3]>,
}

thread_local! {
    static PRINTS: RefCell<Vec<VoxelPrintRequest>> = const { RefCell::new(Vec::new()) };
}

/// The prints scripts asked for since this was last called, in order.
pub fn take_voxel_prints() -> Vec<VoxelPrintRequest> {
    PRINTS.with_borrow_mut(std::mem::take)
}

/// `Voxels.Print`: 1 once the print is asked for, printed after the frame;
/// 0 for no print named. Fitted where `fit` is 1, on a footing of `footing`
/// (-1 for none), in `tint` (as `voxel_shape` takes it).
pub(crate) unsafe extern "C" fn voxel_print(
    print: *const u8,
    length: i32,
    at: *const [f32; 3],
    rotation: *const [f32; 3],
    fit: i32,
    footing: i32,
    tint: u32,
) -> i32 {
    if print.is_null() || length <= 0 || at.is_null() || rotation.is_null() {
        return 0;
    }
    let print = unsafe { crate::interop::utf8(print, length) }.into_owned();
    let (at, rotation) = unsafe { (*at, *rotation) };
    let request = VoxelPrintRequest { print, at, rotation, fit: fit == 1, footing, tint: tint_of(tint) };
    PRINTS.with_borrow_mut(|prints| prints.push(request));
    1
}

/// A cut a script asked for (`Voxels.Cut` and `Voxels.Slice`), placed in
/// the world: at points, each the voxel there; a ball round a place; or a
/// blade's slice through a place square to a way, as far as a radius in
/// metres; and which way across what comes loose falls, none for away from
/// the cut.
#[derive(Clone, Debug, PartialEq)]
pub enum VoxelCutRequest {
    Points(Vec<[f32; 3]>),
    Ball { at: [f32; 3], radius: f32 },
    Slice { at: [f32; 3], across: [f32; 3], radius: f32 },
}

/// A piece of tree a cut took loose, as a script reads it: its Obj while it
/// is a body, where the middle of its logs is, its size in metres, and how
/// many voxels of logs and of leaves it has; and whether it is at rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelCutPiece {
    pub obj: Option<Entity>,
    pub place: [f32; 3],
    pub size: [f32; 3],
    pub logs: i32,
    pub leaves: i32,
    pub resting: bool,
}

impl VoxelCutPiece {
    /// Its numbers as sent: place, size, logs, leaves and 1 at rest.
    fn numbers(&self) -> [f32; 9] {
        let [x, y, z] = self.place;
        let [w, h, d] = self.size;
        [x, y, z, w, h, d, self.logs as f32, self.leaves as f32, f32::from(u8::from(self.resting))]
    }
}

/// What answers `Voxels.Cut` and `Voxels.Slice`: the cut, which way across
/// what comes loose falls, whether it falls (0) or is taken away at once
/// (1), and what becomes of it at rest (-1 as its volume says, 0 it stays
/// a body, 1 it is stamped back, 2 it is taken away). Gives how many
/// volumes it reached, how many columns' grass and voxels of logs and leaves
/// it cut, and how many pieces came loose; and the pieces.
pub type VoxelCuts =
    fn(&mut World, &VoxelCutRequest, Option<[f32; 3]>, i32, i32) -> ([i32; 4], Vec<VoxelCutPiece>, Vec<[f32; 3]>);

static CUTS: OnceLock<VoxelCuts> = OnceLock::new();

/// Sets what answers `Voxels.Cut` and `Voxels.Slice`; the first set is kept.
pub fn set_voxel_cuts(cuts: VoxelCuts) {
    let _ = CUTS.set(cuts);
}

/// `Voxels.Cut` and `Voxels.Slice`: `shape` 0 points (three numbers each),
/// 1 a ball (where and its radius), 2 a slice (where, the way it is square
/// to, and its radius); `toward` the way across what comes loose falls, or
/// null; `pieces` and `rest` as [`VoxelCuts`] takes them. Writes the
/// columns, the voxels, the pieces cut and the places grass was cut into
/// `done`, the first `room` pieces' numbers into `found` and their Objs into
/// `objs` (0 for one taken away), and the first `mown_room` places grass was
/// cut into `mown`. Gives how many volumes it reached, or -1.
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn voxel_cut(
    shape: i32,
    numbers: *const f32,
    count: i32,
    toward: *const [f32; 3],
    pieces: i32,
    rest: i32,
    done: *mut [i32; 4],
    found: *mut [f32; 9],
    objs: *mut u64,
    room: i32,
    mown: *mut [f32; 3],
    mown_room: i32,
) -> i32 {
    let Some(cuts) = CUTS.get() else { return -1 };
    if numbers.is_null() || count < 0 || done.is_null() {
        return -1;
    }
    let n = unsafe { std::slice::from_raw_parts(numbers, count as usize) };
    let request = match (shape, n.len()) {
        (0, _) => VoxelCutRequest::Points(n.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect()),
        (1, 4..) => VoxelCutRequest::Ball { at: [n[0], n[1], n[2]], radius: n[3].max(0.0) },
        (2, 7..) => VoxelCutRequest::Slice { at: [n[0], n[1], n[2]], across: [n[3], n[4], n[5]], radius: n[6].max(0.0) },
        _ => return -1,
    };
    let toward = (!toward.is_null()).then(|| unsafe { *toward });
    let Some(([reached, columns, voxels, felled], cut, places)) =
        with_world(|world| Some(cuts(world, &request, toward, pieces, rest)))
    else {
        return -1;
    };
    unsafe { *done = [columns, voxels, felled, places.len().min(i32::MAX as usize) as i32] };
    if !mown.is_null() && mown_room > 0 {
        for (i, place) in places.iter().take(mown_room as usize).enumerate() {
            unsafe { *mown.add(i) = *place };
        }
    }
    if !found.is_null() && !objs.is_null() && room > 0 {
        for (i, piece) in cut.iter().take(room as usize).enumerate() {
            unsafe {
                *found.add(i) = piece.numbers();
                *objs.add(i) = piece.obj.map_or(0, Entity::to_bits);
            }
        }
    }
    reached
}

/// What answers `Voxels.Blast`: what was built in a ball round `at`, of
/// `radius` metres, dug out of every endless Voxel Volume it reaches, what
/// that leaves cut off or overloaded falling (`pieces` 0) or taken away (1),
/// what it cuts off thrown away from it at `throw` metres a second. Gives
/// how many volumes it reached and how many voxels it dug out.
pub type VoxelBlasts = fn(&mut World, [f32; 3], f32, i32, f32) -> [i32; 2];

static BLASTS: OnceLock<VoxelBlasts> = OnceLock::new();

/// Sets what answers `Voxels.Blast`; the first set is kept.
pub fn set_voxel_blasts(blasts: VoxelBlasts) {
    let _ = BLASTS.set(blasts);
}

/// `Voxels.Blast`: writes how many volumes it reached and voxels it dug
/// into `done`; gives the volumes, or -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_blast(at: *const [f32; 3], radius: f32, pieces: i32, throw: f32, done: *mut [i32; 2]) -> i32 {
    let Some(blasts) = BLASTS.get() else { return -1 };
    if at.is_null() || done.is_null() {
        return -1;
    }
    let at = unsafe { *at };
    let Some(made) = with_world(|world| Some(blasts(world, at, radius.max(0.0), pieces, throw.max(0.0)))) else {
        return -1;
    };
    unsafe { *done = made };
    made[0]
}

/// What answers `Voxels.Hang` and `Voxels.CutRope`: with `act` 0 a rope
/// tied at `numbers[0..3]` holding the point `numbers[3..6]`, `numbers[6]`
/// metres long and snapping at `numbers[7]` newtons (0 for as far as
/// between them, and never), giving its Obj; with `act` 1 rope `obj` cut,
/// giving it back where it was one not cut already.
pub type VoxelRopes = fn(&mut World, i32, [f32; 8], Option<Entity>) -> Option<Entity>;

static ROPES: OnceLock<VoxelRopes> = OnceLock::new();

/// Sets what answers `Voxels.Hang` and `Voxels.CutRope`; the first set is kept.
pub fn set_voxel_ropes(ropes: VoxelRopes) {
    let _ = ROPES.set(ropes);
}

/// `Voxels.Hang` (`act` 0) and `Voxels.CutRope` (1): the rope's Obj, or 0.
pub(crate) unsafe extern "C" fn voxel_rope(act: i32, numbers: *const [f32; 8], obj: u64) -> u64 {
    let Some(ropes) = ROPES.get() else { return 0 };
    let numbers = if numbers.is_null() { [0.0; 8] } else { unsafe { *numbers } };
    let obj = (obj != 0).then(|| Entity::from_bits(obj));
    with_world(|world| ropes(world, act, numbers, obj)).map_or(0, Entity::to_bits)
}

/// What answers `Voxels.Piece`, `Voxels.Stamp` and `Voxels.Remove`: what
/// felled piece an Obj is, and with `act` 1 it stamped back where it lies, 2
/// taken away; none for an Obj that is not one.
pub type VoxelPieces = fn(&mut World, Entity, i32) -> Option<VoxelCutPiece>;

static PIECES: OnceLock<VoxelPieces> = OnceLock::new();

/// Sets what answers `Voxels.Piece` and the rest; the first set is kept.
pub fn set_voxel_pieces(pieces: VoxelPieces) {
    let _ = PIECES.set(pieces);
}

/// `Voxels.Piece`, `Voxels.Stamp` and `Voxels.Remove`: 1 with the piece's
/// numbers in `info`, as it was before `act`; 0 for an Obj that is not a
/// felled piece; -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_piece(obj: u64, act: i32, info: *mut [f32; 9]) -> i32 {
    let Some(pieces) = PIECES.get() else { return -1 };
    if info.is_null() {
        return -1;
    }
    let Some(piece) = with_world(|world| pieces(world, Entity::from_bits(obj), act)) else { return 0 };
    unsafe { *info = piece.numbers() };
    1
}

/// What answers `Voxels.Raycast` meeting cover too: as [`VoxelRaycast`],
/// with what it meets (0 what is solid and the trees' leaves, 1 the grass,
/// cover and crops too), and whether what it met is cover.
pub type VoxelRaycastMeeting =
    fn(&World, [f32; 3], [f32; 3], f32, i32) -> Option<(Entity, [f32; 3], [f32; 3], f32, bool)>;

static RAYCAST_MEETING: OnceLock<VoxelRaycastMeeting> = OnceLock::new();

/// Sets what answers `Voxels.Raycast` meeting cover; the first set is kept.
pub fn set_voxel_raycast_meeting(raycast: VoxelRaycastMeeting) {
    let _ = RAYCAST_MEETING.set(raycast);
}

/// `Voxels.Raycast` with what it meets: 1 with the point, the face's way,
/// the distance and 1 where it met cover in `hit`, and the volume's Obj in
/// `obj`; 0 where the ray meets nothing; -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_raycast_meeting(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    reach: f32,
    meets: i32,
    hit: *mut [f32; 8],
    obj: *mut u64,
) -> i32 {
    let Some(raycast) = RAYCAST_MEETING.get() else { return -1 };
    if origin.is_null() || direction.is_null() || hit.is_null() || obj.is_null() {
        return -1;
    }
    let (origin, direction) = unsafe { (*origin, *direction) };
    let found = with_world(|world| Some(raycast(world, origin, direction, reach, meets))).unwrap_or(None);
    let Some((entity, point, normal, distance, cover)) = found else { return 0 };
    unsafe {
        *hit = [point[0], point[1], point[2], normal[0], normal[1], normal[2], distance, f32::from(u8::from(cover))];
        *obj = entity.to_bits();
    }
    1
}

/// What answers `Voxels.Felled` and `Voxels.Regrow`: given a felled
/// trees' rest's number (0 a body, 1 stamped; -1 only asks) and a regrow
/// time in seconds (under 0 only asks), sets them on every endless Voxel
/// Volume; gives the first one's, if there is one.
pub type VoxelCutting = fn(&mut World, i32, f32) -> Option<(i32, f32)>;

static CUTTING: OnceLock<VoxelCutting> = OnceLock::new();

/// Sets what answers `Voxels.Felled` and `Voxels.Regrow`; the first set is
/// kept.
pub fn set_voxel_cutting(cutting: VoxelCutting) {
    let _ = CUTTING.set(cutting);
}

/// `Voxels.Felled` and `Voxels.Regrow`: `felled` 0 or 1 sets the felled
/// trees' rest, `regrow` 0 or more the regrow time, -1 for either only
/// asks. 1 with the first endless volume's in `found`; 0 where there is
/// none; -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_cutting(felled: i32, regrow: f32, found: *mut [f32; 2]) -> i32 {
    let Some(cutting) = CUTTING.get() else { return -1 };
    if found.is_null() {
        return -1;
    }
    let Some((felled, regrow)) = with_world(|world| cutting(world, felled, regrow)) else { return 0 };
    unsafe { *found = [felled as f32, regrow] };
    1
}

/// What answers `Voxels.Felling`: given 1, sets how every endless Voxel
/// Volume's felled trees fall and rest to `numbers` (the push, how long
/// still to rest, how deep and to what share leaves are pressed, the wood's
/// density, and 1 for clippings), and either way gives the first one's.
pub type VoxelFellingCall = fn(&mut World, Option<[f32; 6]>) -> Option<[f32; 6]>;

static FELLING: OnceLock<VoxelFellingCall> = OnceLock::new();

/// Sets what answers `Voxels.Felling`; the first set is kept.
pub fn set_voxel_felling(felling: VoxelFellingCall) {
    let _ = FELLING.set(felling);
}

/// `Voxels.Felling`: `set` 1 sets every endless volume's to `numbers`, 0
/// only asks; 1 with the first one's in `numbers`, 0 where there is none,
/// -1 while scripts are not running.
pub(crate) unsafe extern "C" fn voxel_felling(set: i32, numbers: *mut [f32; 6]) -> i32 {
    let Some(felling) = FELLING.get() else { return -1 };
    if numbers.is_null() {
        return -1;
    }
    let given = (set == 1).then(|| unsafe { *numbers });
    let Some(found) = with_world(|world| felling(world, given)) else { return 0 };
    unsafe { *numbers = found };
    1
}
