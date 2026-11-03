//! Paths round the walls, baked ahead of time: Steam Audio stands probes a
//! little above every floor of the level, a few metres apart, and works out
//! which of them see one another and the shortest way between every two.
//! While the game runs, a voice behind a wall is heard along the path from
//! the probe nearest it to the probe nearest the listener, so a sound in
//! the next room comes through the doorway, from the doorway's side, and
//! duller the more sharply its path bends.
//!
//! Baking takes seconds to minutes, so the editor does it on a thread of
//! its own. What it makes, [`BakedPaths`], is Steam Audio's own serialized
//! probe batch, which the level keeps beside it as a `.paths` file and the
//! game loads with [`PathProbes::load`].

use std::{ffi::c_void, sync::Arc};

use voxear_core::math::Vec3;

use super::{
    Acoustics, Handle, IplVector3, SteamAudio, Wall,
    acoustics::{BakedDataIdentifier, Sphere},
    steam,
};

/// Steam Audio's way of standing probes on the floors, and its layer of
/// paths between every two probes.
const UNIFORM_FLOOR: i32 = 1;
const PATHING: i32 = 1;
const DYNAMIC: i32 = 3;
/// How far apart probes stand at first, and how high above the floor. A
/// level that would have more than [`MOST_PROBES`] has them further apart.
const SPACING: f32 = 2.0;
const HEIGHT: f32 = 1.5;
const MOST_PROBES: i32 = 4096;
/// Whether two probes see one another: the rays traced between points
/// round each, in a sphere this big, and the share of them that must get
/// through.
const SAMPLES: i32 = 4;
pub(super) const VISIBLE_RADIUS: f32 = 0.5;
pub(super) const VISIBLE_SHARE: f32 = 0.1;
/// Probes further apart than this are never taken to see one another, and
/// a path longer than this is no path.
pub(super) const VISIBLE_RANGE: f32 = 50.0;
const PATH_RANGE: f32 = 100.0;

#[repr(C)]
pub(super) struct ProbeGenerationParams {
    kind: i32,
    spacing: f32,
    height: f32,
    /// Turns a box a metre across round the origin into the one probes
    /// stand in. Steam Audio's header says the box is from (0, 0, 0) to
    /// (1, 1, 1), but it is the one Unity's cube is, from -0.5 to 0.5.
    transform: [[f32; 4]; 4],
}

#[repr(C)]
pub(super) struct PathBakeParams {
    scene: Handle,
    probe_batch: Handle,
    identifier: BakedDataIdentifier,
    samples: i32,
    radius: f32,
    threshold: f32,
    visible_range: f32,
    path_range: f32,
    threads: i32,
}

#[repr(C)]
pub(super) struct SerializedSettings {
    data: *mut u8,
    size: usize,
}

pub(super) type Progress = unsafe extern "C" fn(f32, *mut c_void);

/// The layer of a probe batch that holds paths.
pub(super) fn identifier() -> BakedDataIdentifier {
    BakedDataIdentifier {
        kind: PATHING,
        variation: DYNAMIC,
        influence: Sphere { center: IplVector3::default(), radius: 0.0 },
    }
}

/// A level's paths as baked: Steam Audio's probe batch, as bytes for the
/// level's `.paths` file, and how many probes it has.
#[derive(Clone, Debug)]
pub struct BakedPaths {
    pub bytes: Vec<u8>,
    pub probes: usize,
}

/// Bakes the paths round these walls, telling `progress` how far it has
/// got, from 0 to 1, now and then. The walls need a floor for probes to
/// stand on.
pub fn bake_paths(walls: &[Wall], progress: &mut dyn FnMut(f32)) -> Result<BakedPaths, String> {
    let mut acoustics = Acoustics::new()?;
    let Some((low, high)) = bounds(walls) else { return Err("The level has no walls to bake paths round".into()) };
    acoustics.set_geometry(&walls.iter().map(Wall::mesh).collect::<Vec<_>>());
    let steam = acoustics.steam.clone();
    let api = &steam.api;
    let batch = ProbeBatch::new(&steam)?;
    let mut probes = stand_probes(&steam, acoustics.scene, low, high)?;
    unsafe {
        (api.probe_batch_add_array)(batch.handle, probes);
        (api.probe_array_release)(&mut probes);
        (api.probe_batch_commit)(batch.handle);
    }
    let count = unsafe { (api.probe_batch_count)(batch.handle) };
    if count == 0 {
        return Err("No probes stood in the level: it needs a floor".into());
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
    let mut params = PathBakeParams {
        scene: acoustics.scene,
        probe_batch: batch.handle,
        identifier: identifier(),
        samples: SAMPLES,
        radius: VISIBLE_RADIUS,
        threshold: VISIBLE_SHARE,
        visible_range: VISIBLE_RANGE,
        path_range: PATH_RANGE,
        threads: threads as i32,
    };
    let mut told: &mut dyn FnMut(f32) = progress;
    let told = (&mut told as *mut &mut dyn FnMut(f32)).cast::<c_void>();
    unsafe { (api.path_baker_bake)(steam.context, &mut params, Some(tell), told) };
    Ok(BakedPaths { bytes: batch.save()?, probes: count as usize })
}

/// Hands Steam Audio's progress on to the closure `bake_paths` was given.
unsafe extern "C" fn tell(progress: f32, user: *mut c_void) {
    let progress_fn = unsafe { &mut *user.cast::<&mut dyn FnMut(f32)>() };
    progress_fn(progress.clamp(0.0, 1.0));
}

/// Probes on every floor between `low` and `high`, as far apart as keeps
/// them to [`MOST_PROBES`].
fn stand_probes(steam: &SteamAudio, scene: Handle, low: Vec3, high: Vec3) -> Result<Handle, String> {
    let api = &steam.api;
    let mut spacing = SPACING;
    loop {
        let mut probes: Handle = std::ptr::null_mut();
        if unsafe { (api.probe_array_create)(steam.context, &mut probes) } != 0 {
            return Err("Steam Audio made no probes".into());
        }
        let mut params =
            ProbeGenerationParams { kind: UNIFORM_FLOOR, spacing, height: HEIGHT, transform: box_transform(low, high) };
        unsafe { (api.probe_array_generate)(probes, scene, &mut params) };
        if unsafe { (api.probe_array_count)(probes) } <= MOST_PROBES || spacing > 64.0 {
            return Ok(probes);
        }
        unsafe { (api.probe_array_release)(&mut probes) };
        spacing *= 1.5;
    }
}

/// The box round every wall, with room above the highest floor for a
/// probe to stand.
fn bounds(walls: &[Wall]) -> Option<(Vec3, Vec3)> {
    let mut points = walls.iter().flat_map(|wall| &wall.vertices).copied();
    let first = points.next()?;
    let (low, high) = points.fold((first, first), |(low, high), p| (low.min(p), high.max(p)));
    Some((low - Vec3::Y, high + Vec3::Y * (HEIGHT + 1.0)))
}

/// The matrix, rows first, that turns Steam Audio's unit box into the one
/// from `low` to `high` in Voxear's world, its Z turned over.
fn box_transform(low: Vec3, high: Vec3) -> [[f32; 4]; 4] {
    let size = high - low;
    let middle = steam((low + high) * 0.5);
    [[size.x, 0.0, 0.0, middle.x], [0.0, size.y, 0.0, middle.y], [0.0, 0.0, size.z, middle.z], [0.0, 0.0, 0.0, 1.0]]
}

/// Steam Audio's batch of probes, with the paths between them.
struct ProbeBatch {
    steam: Arc<SteamAudio>,
    handle: Handle,
}

impl Drop for ProbeBatch {
    fn drop(&mut self) {
        unsafe { (self.steam.api.probe_batch_release)(&mut self.handle) };
    }
}

impl ProbeBatch {
    fn new(steam: &Arc<SteamAudio>) -> Result<Self, String> {
        let mut handle: Handle = std::ptr::null_mut();
        if unsafe { (steam.api.probe_batch_create)(steam.context, &mut handle) } != 0 {
            return Err("Steam Audio made no probe batch".into());
        }
        Ok(Self { steam: steam.clone(), handle })
    }

    fn save(&self) -> Result<Vec<u8>, String> {
        let api = &self.steam.api;
        let mut settings = SerializedSettings { data: std::ptr::null_mut(), size: 0 };
        let mut serialized: Handle = std::ptr::null_mut();
        if unsafe { (api.serialized_create)(self.steam.context, &mut settings, &mut serialized) } != 0 {
            return Err("Steam Audio could not write the paths out".into());
        }
        let bytes = unsafe {
            (api.probe_batch_save)(self.handle, serialized);
            let (data, size) = ((api.serialized_data)(serialized), (api.serialized_size)(serialized));
            let bytes = if data.is_null() { Vec::new() } else { std::slice::from_raw_parts(data, size).to_vec() };
            (api.serialized_release)(&mut serialized);
            bytes
        };
        Ok(bytes)
    }
}

/// A level's baked paths, loaded for the game to hear voices along.
pub struct PathProbes {
    batch: ProbeBatch,
}

// Steam Audio's probe batch is only read once loaded.
unsafe impl Send for PathProbes {}
unsafe impl Sync for PathProbes {}

impl PathProbes {
    /// Loads paths from a `.paths` file's bytes, loading Steam Audio if it
    /// has not been.
    pub fn load(bytes: &[u8]) -> Result<Arc<Self>, String> {
        let steam = SteamAudio::get()?;
        let api = &steam.api;
        let mut data = bytes.to_vec();
        let mut settings = SerializedSettings { data: data.as_mut_ptr(), size: data.len() };
        let mut serialized: Handle = std::ptr::null_mut();
        if unsafe { (api.serialized_create)(steam.context, &mut settings, &mut serialized) } != 0 {
            return Err("Steam Audio could not read the paths".into());
        }
        let mut handle: Handle = std::ptr::null_mut();
        let loaded = unsafe { (api.probe_batch_load)(steam.context, serialized, &mut handle) };
        unsafe { (api.serialized_release)(&mut serialized) };
        if loaded != 0 || handle.is_null() {
            return Err("The paths are not Steam Audio's, or are from another version".into());
        }
        unsafe { (api.probe_batch_commit)(handle) };
        Ok(Arc::new(Self { batch: ProbeBatch { steam, handle } }))
    }

    /// How many probes the paths run between.
    pub fn probes(&self) -> usize {
        unsafe { (self.batch.steam.api.probe_batch_count)(self.batch.handle) }.max(0) as usize
    }

    pub(super) fn handle(&self) -> Handle {
        self.batch.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        spatial::Listener,
        steam_audio::{AcousticMaterial, Heard, Muffle, Occlusion, SourceAcoustics},
    };

    /// A box from `low` to `high`, its faces outwards.
    fn block(low: Vec3, high: Vec3, material: AcousticMaterial) -> Wall {
        let corner = |i: u32| {
            Vec3::new(
                if i & 1 == 0 { low.x } else { high.x },
                if i & 2 == 0 { low.y } else { high.y },
                if i & 4 == 0 { low.z } else { high.z },
            )
        };
        let faces = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let triangles = faces.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect();
        Wall { vertices: (0..8).map(corner).collect(), triangles, material }
    }

    /// A floor 20 metres square, and a wall across it at x = 3, 4 metres
    /// high, with a doorway 2 metres wide at z from 4 to 6.
    fn rooms() -> Vec<Wall> {
        let concrete = AcousticMaterial { transmission: [0.0; 3], ..AcousticMaterial::CONCRETE };
        vec![
            block(Vec3::new(-10.0, -0.5, -10.0), Vec3::new(10.0, 0.0, 10.0), concrete),
            block(Vec3::new(2.8, 0.0, -10.0), Vec3::new(3.2, 4.0, 4.0), concrete),
            block(Vec3::new(2.8, 0.0, 6.0), Vec3::new(3.2, 4.0, 10.0), concrete),
        ]
    }

    #[test]
    fn a_sound_behind_a_wall_comes_round_through_the_doorway() {
        if SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let walls = rooms();
        let mut told = Vec::new();
        let baked = bake_paths(&walls, &mut |progress| told.push(progress)).expect("baked");
        assert!(baked.probes > 20 && !baked.bytes.is_empty(), "{} probes", baked.probes);
        assert!(told.last().is_some_and(|&last| last > 0.99), "progress to the end: {:?}", told.last());

        let paths = PathProbes::load(&baked.bytes).expect("loaded again");
        assert_eq!(paths.probes(), baked.probes);
        let mut acoustics = Acoustics::new().expect("Steam Audio");
        acoustics.set_geometry(&walls.iter().map(Wall::mesh).collect::<Vec<_>>());
        acoustics.set_paths(Some(paths));

        let listener = Listener { position: Vec3::new(0.0, 1.5, 0.0), ..Listener::default() };
        let behind = Vec3::new(6.0, 1.5, 0.0);
        let shut = SourceAcoustics { occlusion: Occlusion::Raycast, radius: 1.0, transmission: false, pathing: false };
        let round = SourceAcoustics { pathing: true, ..shut };
        let heard = acoustics.hear(&listener, &[(1, behind, shut), (2, behind, round)]);
        assert_eq!(heard[0], Heard { muffle: Muffle([0.0; 3]), from: None }, "only straight: nothing");
        let [low, middle, high] = heard[1].muffle.0;
        assert!(low > 0.05 && high <= middle && middle <= low, "round the corner, duller: {:?}", heard[1]);
        let from = heard[1].from.expect("from the doorway");
        assert!(from.z > 0.5 && from.x > 0.2, "from the doorway's side: {from}");

        let turned = Listener { position: behind, ..listener };
        let from = acoustics.hear(&turned, &[(2, listener.position, round)])[0].from.expect("from the doorway");
        assert!(from.z > 0.5 && from.x < -0.2, "from the doorway's side, the other way: {from}");

        let open = Vec3::new(-4.0, 1.5, 0.0);
        let heard = acoustics.hear(&listener, &[(2, open, round)]);
        assert_eq!(heard[0].muffle, Muffle::NONE, "in the open, as it is");
    }
}
