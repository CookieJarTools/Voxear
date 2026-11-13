//! Trees, saplings and items lying in the world, for C#: `Trees`,
//! `Saplings` and `ItemDrops` in Voxear.Core. A log is mined a slice at a
//! time and a tree comes down once one is cut through; a sapling takes
//! root and grows; what a tree breaks into lies on the ground until taken
//! up. The engine does each (`voxear_engine::trees`, `saplings`,
//! `item_drops`) through `ScriptServices`.

use crate::{
    interop::{utf8, with_world},
    services::with_services,
};

/// What `tree_call` did: nothing, a log a slice thinner, a log cut through
/// with nothing coming down, a tree come down, a sapling grown a stage.
pub const TREE_NOTHING: i32 = 0;
pub const TREE_THINNER: i32 = 1;
pub const TREE_CUT: i32 = 2;
pub const TREE_FELLED: i32 = 3;
pub const TREE_GREW: i32 = 4;

/// A tree that came down, as the engine tells scripts of it: the world's
/// Obj, where it was cut, the way it fell, where its middle landed, and
/// the Objs of the items it broke into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FelledInfo {
    pub world: u64,
    pub cut: [f32; 3],
    pub away: [f32; 3],
    pub landed: [f32; 3],
    pub drops: Vec<u64>,
}

/// A tree that came down, as `TreeFelledData` in Voxear.Core reads it:
/// its drops are `drop_count` of those written beside it, from
/// `first_drop`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FelledData {
    pub world: u64,
    pub cut: [f32; 3],
    pub away: [f32; 3],
    pub landed: [f32; 3],
    pub first_drop: i32,
    pub drop_count: i32,
}

const _: () = assert!(size_of::<FelledData>() == 56, "TreeFelledData in Voxear.Core is 56 bytes");

/// `Trees.Chop`, `Trees.Fell` and `Saplings.Grow`: see
/// `ScriptServices::tree_call`.
pub(crate) unsafe extern "C" fn tree_call(what: i32, at: *const [f32; 3], way: *const [f32; 3]) -> i32 {
    if at.is_null() || way.is_null() {
        return TREE_NOTHING;
    }
    let (at, way) = unsafe { (*at, *way) };
    with_world(|world| with_services(|services| services.tree_call(world, what, at, way))).unwrap_or(TREE_NOTHING)
}

/// `Trees.Dispatch`: the trees that came down since last asked, as many as
/// fit, and their drops' Objs; how many trees there were. Those not
/// written are lost: the caller asks with room enough.
pub(crate) unsafe extern "C" fn trees_felled(
    out: *mut FelledData,
    capacity: i32,
    drops: *mut u64,
    drop_capacity: i32,
) -> i32 {
    let felled = with_world(|world| with_services(|services| services.trees_felled(world))).unwrap_or_default();
    let mut written = 0usize;
    for (at, tree) in felled.iter().enumerate().take(capacity.max(0) as usize) {
        let first = written.min(drop_capacity.max(0) as usize);
        let room = (drop_capacity.max(0) as usize).saturating_sub(first);
        let count = tree.drops.len().min(room);
        if !drops.is_null() && count > 0 {
            unsafe { std::ptr::copy_nonoverlapping(tree.drops.as_ptr(), drops.add(first), count) };
        }
        written = first + count;
        if !out.is_null() {
            let data = FelledData {
                world: tree.world,
                cut: tree.cut,
                away: tree.away,
                landed: tree.landed,
                first_drop: first as i32,
                drop_count: count as i32,
            };
            unsafe { *out.add(at) = data };
        }
    }
    felled.len() as i32
}

/// `Saplings.Plant`: whether the sapling a `.block` names took root on the
/// block under `at`.
pub(crate) unsafe extern "C" fn sapling_plant(block: *const u8, len: i32, at: *const [f32; 3]) -> i32 {
    if block.is_null() || len <= 0 || at.is_null() {
        return 0;
    }
    let (named, at) = (unsafe { utf8(block, len) }, unsafe { *at });
    let planted = with_world(|world| with_services(|services| services.sapling_plant(world, &named, at)));
    i32::from(planted == Some(true))
}

/// `Saplings.All`: every sapling growing, as many as fit, each where it is
/// and how far grown, four numbers; how many there are.
pub(crate) unsafe extern "C" fn saplings(out: *mut [f32; 4], capacity: i32) -> i32 {
    let found = with_world(|world| with_services(|services| services.saplings(world))).unwrap_or_default();
    if !out.is_null() {
        for (at, (place, grown)) in found.iter().enumerate().take(capacity.max(0) as usize) {
            unsafe { *out.add(at) = [place[0], place[1], place[2], *grown] };
        }
    }
    found.len() as i32
}

/// `ItemDrops.Drop`: `count` of the item a data asset names dropped at
/// `at`, thrown at `velocity`: its Obj's bits, 0 where none.
pub(crate) unsafe extern "C" fn item_drop(
    item: *const u8,
    len: i32,
    count: i32,
    at: *const [f32; 3],
    velocity: *const [f32; 3],
) -> u64 {
    if item.is_null() || len <= 0 || count <= 0 || at.is_null() {
        return 0;
    }
    let named = unsafe { utf8(item, len) };
    let (at, velocity) = (unsafe { *at }, if velocity.is_null() { [0.0; 3] } else { unsafe { *velocity } });
    with_world(|world| with_services(|services| services.item_drop(world, &named, count as u32, at, velocity)))
        .unwrap_or(0)
}

/// `ItemDrops.Near`: the items lying within `radius` of `at` that can be
/// taken up, nearest first, as many as fit; how many there are.
pub(crate) unsafe extern "C" fn item_drops_near(at: *const [f32; 3], radius: f32, out: *mut u64, capacity: i32) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = unsafe { *at };
    let found =
        with_world(|world| with_services(|services| services.item_drops_near(world, at, radius))).unwrap_or_default();
    if !out.is_null() {
        for (at, drop) in found.iter().enumerate().take(capacity.max(0) as usize) {
            unsafe { *out.add(at) = *drop };
        }
    }
    found.len() as i32
}

/// `ItemDrop.Item`, `.Count`, `.Position` and `ItemDrops.Take`: the item
/// lying on `drop`, written into `buffer` as its reference when it fits,
/// with how many and where; taken up where `take` is not 0. The
/// reference's length, or -1 where it is no drop, or gone.
pub(crate) unsafe extern "C" fn item_drop_read(
    drop: u64,
    take: i32,
    buffer: *mut u8,
    capacity: i32,
    count: *mut i32,
    at: *mut [f32; 3],
) -> i32 {
    let read = with_world(|world| with_services(|services| services.item_drop_read(world, drop, take != 0)).flatten());
    let Some((item, many, place)) = read else { return -1 };
    if !buffer.is_null() && item.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(item.as_ptr(), buffer, item.len()) };
    }
    if !count.is_null() {
        unsafe { *count = many as i32 };
    }
    if !at.is_null() {
        unsafe { *at = place };
    }
    item.len() as i32
}
