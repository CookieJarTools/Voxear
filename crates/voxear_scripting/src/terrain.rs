//! Terrain for C#: `Terrain` in Voxear.Core asks about the level's ground,
//! its height and the way it faces under a point, and where a ray meets it,
//! straight from the terrains' heights (see `voxear_terrain`), with or
//! without physics, and changes it: a crater, ground raised or flattened,
//! a surface painted, a hole cut, grass cleared, each the same on every
//! machine (see `voxear_terrain::runtime`), and kept for a save of the
//! scene or a player joining a game.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Plants, world_matrix};
use voxear_terrain::{
    Terrain,
    runtime::{Edit, TerrainEdits},
};

use crate::{
    interop::{utf8, with_world},
    physics::RaycastHitData,
    services::with_services,
};

/// How many terrains the level has.
pub(crate) unsafe extern "C" fn terrain_count() -> i32 {
    with_world(|world| Some(voxear_terrain::terrains(world).len() as i32)).unwrap_or(0)
}

/// The ground under `at`: its height, then the way it faces, and the
/// terrain's Obj; 0 where no terrain reaches.
pub(crate) unsafe extern "C" fn terrain_ground(at: *const [f32; 3], out: *mut [f32; 4], obj: *mut u64) -> i32 {
    if at.is_null() || out.is_null() {
        return 0;
    }
    let [x, _, z] = unsafe { *at };
    let Some(ground) = with_world(|world| voxear_terrain::ground_at(world, x, z)) else { return 0 };
    unsafe { *out = [ground.height, ground.normal.x, ground.normal.y, ground.normal.z] };
    if !obj.is_null() {
        unsafe { *obj = ground.terrain.to_bits() };
    }
    1
}

/// The first place a ray from `origin` along `direction` meets any terrain
/// within `distance` metres; 0 where it meets none.
pub(crate) unsafe extern "C" fn terrain_raycast(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    distance: f32,
    hit: *mut RaycastHitData,
) -> i32 {
    if origin.is_null() || direction.is_null() || hit.is_null() || distance.is_nan() || distance <= 0.0 {
        return 0;
    }
    let (origin, direction) = unsafe { (Vec3::from(*origin), Vec3::from(*direction)) };
    let reach = distance.min(1.0e7);
    let found = with_world(|world| voxear_terrain::raycast(world, origin, direction, reach));
    let Some((terrain, ground)) = found else { return 0 };
    unsafe {
        *hit = RaycastHitData {
            obj: terrain.to_bits(),
            point: ground.point.into(),
            normal: ground.normal.into(),
            distance: ground.distance,
            plant: 0,
        };
    }
    1
}

/// An edit to the ground as C# sends it: `TerrainEditData` in Voxear.Core,
/// made by a `TerrainEdit`. `kind` is 0 raise, 1 crater, 2 flatten, 3 paint,
/// 4 holes, 5 grass, 6 dig, 7 fill, 8 tunnel and 9 explode; `amount` the
/// height raised, the crater's depth, the height in the world flattened to,
/// the paint's strength or the grass's density; `cut` whether holes are cut
/// or filled; `to` where a tunnel goes to.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TerrainEditData {
    pub kind: i32,
    pub cut: i32,
    pub at: [f32; 3],
    pub radius: f32,
    pub amount: f32,
    pub to: [f32; 3],
}

const _: () = assert!(size_of::<TerrainEditData>() == 40, "TerrainEditData in Voxear.Core is 40 bytes");

/// The edit `data` makes of a terrain whose corner is at `corner`, in its
/// space, with `surface` the number of the surface it lays; none for an
/// edit of no kind, or a paint of a surface the terrain has not.
fn local_edit(data: &TerrainEditData, corner: Vec3, surface: Option<u8>) -> Option<Edit> {
    let at = [data.at[0] - corner.x, data.at[2] - corner.z];
    let (radius, amount) = (data.radius, data.amount);
    let local = |p: [f32; 3]| (Vec3::from(p) - corner).to_array();
    Some(match data.kind {
        0 => Edit::Raise { at, radius, height: amount },
        1 => Edit::Crater { at, radius, depth: amount, surface },
        2 => Edit::Flatten { at, radius, height: amount - corner.y },
        3 => Edit::Paint { at, radius, surface: surface?, strength: amount },
        4 => Edit::Holes { at, radius, cut: data.cut != 0 },
        5 => Edit::Grass { at, radius, kind: None, amount },
        6 => Edit::Dig { at: local(data.at), radius, surface },
        7 => Edit::Fill { at: local(data.at), radius },
        8 => Edit::Tunnel { from: local(data.at), to: local(data.to), radius, surface },
        9 => Edit::Explode { at: local(data.at), radius, surface },
        _ => return None,
    })
}

/// Changes the ground of every terrain an edit reaches, kept with its
/// [`TerrainEdits`] for a save; a crater takes away the plants standing in
/// it too. `surface` names the material of the surface a crater lays on
/// its walls or a paint lays, as a project path or GUID. How many terrains
/// it changed.
pub(crate) unsafe extern "C" fn terrain_edit(
    edit: *const TerrainEditData,
    surface: *const u8,
    surface_len: i32,
) -> i32 {
    if edit.is_null() {
        return 0;
    }
    let data = unsafe { *edit };
    let material = unsafe { utf8(surface, surface_len) }.into_owned();
    let changed = with_world(|world| {
        let mut changed = 0;
        // Edits are kept as changes to a terrain as built, which one without end, made as it is seen, never is.
        for (entity, terrain, corner) in voxear_terrain::terrains(world)
            .into_iter()
            .filter(|(_, terrain, _)| terrain.origin.is_none())
            .map(|(entity, terrain, corner)| (entity, terrain.clone(), corner))
            .collect::<Vec<_>>()
        {
            let surface = (!material.is_empty())
                .then(|| with_services(|services| services.terrain_surface(world, entity, &material)).flatten())
                .flatten();
            let Some(edit) = local_edit(&data, corner, surface) else { continue };
            // A terrain keeps its edits from the first that reaches it.
            let mut terrain = terrain;
            let mut edits = world.remove::<TerrainEdits>(entity).unwrap_or_else(|| TerrainEdits::new(&terrain));
            let reached = edits.apply(&mut terrain, &edit).is_some();
            if reached || edits.has_edits() {
                world.insert(entity, edits);
            }
            if reached && let Some(kept) = world.get_mut::<Terrain>(entity) {
                kept.heights = terrain.heights;
                kept.weights = terrain.weights;
                kept.grass = terrain.grass;
                kept.dug = terrain.dug;
                changed += 1;
            }
        }
        if data.kind == 1 && changed > 0 {
            clear_plants(world, Vec3::from(data.at), data.radius);
        }
        // What stood on ground digging took away comes down.
        if (6..=9).contains(&data.kind) && changed > 0 {
            let reach = if data.kind == 8 { Vec3::from(data.at).distance(Vec3::from(data.to)) } else { 0.0 };
            let middle =
                (Vec3::from(data.at) + if data.kind == 8 { Vec3::from(data.to) } else { Vec3::from(data.at) }) * 0.5;
            uproot(world, middle, reach * 0.5 + data.radius * 1.5);
        }
        Some(changed)
    });
    changed.unwrap_or(0)
}

/// Takes away the plants standing within `radius` metres of `at` along the
/// ground.
fn clear_plants(world: &mut World, at: Vec3, radius: f32) {
    let mut gone: Vec<(Entity, u32)> = Vec::new();
    for (entity, plants) in world.query_ref::<(Entity, &Plants)>().without::<Destroying>() {
        let model = world_matrix(world, entity);
        for (number, plant) in plants.standing() {
            let stands = plant.matrix(model).w_axis.truncate();
            if (stands.x - at.x).hypot(stands.z - at.z) <= radius {
                gone.push((entity, number));
            }
        }
    }
    for (entity, number) in gone {
        if let Some(plants) = world.get_mut::<Plants>(entity) {
            plants.remove(number);
        }
    }
}

/// Takes away the plants within `radius` metres of `at` along the ground
/// whose foot the ground fell away from, by more than a few centimetres.
fn uproot(world: &mut World, at: Vec3, radius: f32) {
    let mut gone: Vec<(Entity, u32)> = Vec::new();
    for (entity, plants) in world.query_ref::<(Entity, &Plants)>().without::<Destroying>() {
        let model = world_matrix(world, entity);
        for (number, plant) in plants.standing() {
            let foot = plant.matrix(model).w_axis.truncate();
            if (foot.x - at.x).hypot(foot.z - at.z) > radius {
                continue;
            }
            let fell =
                voxear_terrain::ground_at(world, foot.x, foot.z).is_none_or(|ground| foot.y > ground.height + 0.1);
            if fell {
                gone.push((entity, number));
            }
        }
    }
    for (entity, number) in gone {
        if let Some(plants) = world.get_mut::<Plants>(entity) {
            plants.remove(number);
        }
    }
}

/// What every edited terrain's edits are: the terrains' count, then each
/// one's Obj's key (its length in two bytes, then it) and its edits (their
/// length in four bytes, then them), in the order of their keys.
fn capture(world: &World) -> Vec<u8> {
    let mut kept: Vec<(String, Vec<u8>)> = world
        .query_ref::<(Entity, &Terrain, &TerrainEdits)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, terrain, edits)| {
            let key = with_services(|services| services.obj_key(world, entity)).flatten()?;
            Some((key, edits.save(terrain)))
        })
        .collect();
    kept.sort();
    let mut out = (kept.len() as u32).to_le_bytes().to_vec();
    for (key, edits) in kept {
        out.extend_from_slice(&(key.len() as u16).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(edits.len() as u32).to_le_bytes());
        out.extend_from_slice(&edits);
    }
    out
}

/// Puts each terrain `bytes` (from [`capture`]) names back as it was built,
/// then makes its edits; how many it found, or an error for bytes that are
/// not edits.
fn restore(world: &mut World, bytes: &[u8]) -> anyhow::Result<i32> {
    use anyhow::Context;
    let take = |at: usize, len: usize| bytes.get(at..at + len).context("terrain edits end early");
    let count = u32::from_le_bytes(take(0, 4)?.try_into()?);
    let mut at = 4;
    let mut found = 0;
    for _ in 0..count {
        let key_len = usize::from(u16::from_le_bytes(take(at, 2)?.try_into()?));
        let key = std::str::from_utf8(take(at + 2, key_len)?)?;
        at += 2 + key_len;
        let len = u32::from_le_bytes(take(at, 4)?.try_into()?) as usize;
        let edits = take(at + 4, len)?;
        at += 4 + len;
        let Some(entity) = with_services(|services| services.obj_by_key(world, key)).flatten() else { continue };
        let Some(mut terrain) = world.get::<Terrain>(entity).cloned() else { continue };
        let mut kept = world.remove::<TerrainEdits>(entity).unwrap_or_else(|| TerrainEdits::new(&terrain));
        let loaded = kept.load(&mut terrain, edits);
        world.insert(entity, kept);
        loaded.with_context(|| format!("the edits of the terrain {key}"))?;
        if let Some(now) = world.get_mut::<Terrain>(entity) {
            *now = terrain;
        }
        found += 1;
    }
    Ok(found)
}

/// `Terrain.CaptureEdits` (`what` 0): every edited terrain's edits into
/// `out`, their length returned, written when it fits. `Terrain.ApplyEdits`
/// (`what` 1): puts the edits in `data` back, returning how many terrains
/// it found, or -1 with an error logged for bytes that are not edits.
pub(crate) unsafe extern "C" fn terrain_edits(
    what: i32,
    data: *const u8,
    len: i32,
    out: *mut u8,
    capacity: i32,
) -> i32 {
    match what {
        0 => {
            let bytes = with_world(|world| Some(capture(world))).unwrap_or_else(|| 0u32.to_le_bytes().to_vec());
            if !out.is_null() && bytes.len() <= capacity.max(0) as usize {
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
            }
            bytes.len() as i32
        }
        1 if !data.is_null() && len >= 0 => {
            let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) };
            match with_world(|world| Some(restore(world, bytes))) {
                Some(Ok(found)) => found,
                Some(Err(error)) => {
                    tracing::error!("Terrain.ApplyEdits: {error:#}");
                    -1
                }
                None => 0,
            }
        }
        _ => -1,
    }
}
