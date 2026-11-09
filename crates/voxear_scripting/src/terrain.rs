//! Terrain for C#: `Terrain` in Voxear.Core asks about the level's ground,
//! its height and the way it faces under a point, and where a ray meets it,
//! straight from the terrains' heights (see `voxear_terrain`), with or
//! without physics.

use voxear_core::math::Vec3;

use crate::{interop::with_world, physics::RaycastHitData};

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
