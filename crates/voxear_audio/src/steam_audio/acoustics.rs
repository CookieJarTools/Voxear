//! The level as Steam Audio hears it: its walls, each made of a material
//! that lets more or less of a sound through, and how much of each voice
//! reaches the listener past them.
//!
//! Steam Audio traces rays from a voice to the listener. Where a wall is in
//! the way, the voice is occluded, and only what the wall's material lets
//! through carries on, less of the high notes than of the low, so a sound
//! in the next room is quieter and duller. What comes back is a [`Muffle`]
//! for the mixer: a volume in each of Steam Audio's three bands of pitch.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::Vec3;

use super::{
    Handle, IplVector3, PathProbes, SteamAudio,
    paths::{VISIBLE_RADIUS, VISIBLE_RANGE, VISIBLE_SHARE},
    steam,
};
use crate::{reverb::ReverbTimes, spatial::Listener, steam_audio::FRAME};

/// Steam Audio's simulations: what it works out of a voice's direct sound,
/// and of how it reflects round the room.
const DIRECT: i32 = 1;
pub(super) const REFLECTIONS: i32 = 1 << 1;
const PATHING: i32 = 1 << 2;
/// How the room is heard: by its echoes themselves, an impulse response to
/// convolve with, or by how long it rings, for a reverb to follow.
pub(super) const CONVOLUTION: i32 = 0;
const PARAMETRIC: i32 = 1;
/// The rays a room's reverb is traced with, how many times each may
/// bounce, and how long after a sound its reflections are followed for,
/// which is as long as its echoes are.
const ROOM_RAYS: i32 = 4096;
const ROOM_BOUNCES: i32 = 16;
pub(super) const ROOM_SECONDS: f32 = 2.0;
/// The order of the spherical harmonics a room's echoes come in: 1, four
/// channels, enough for which side each comes from.
pub(super) const ROOM_ORDER: i32 = 1;
/// The key of the source a room is heard from, which stands on the listener.
const ROOM: u64 = u64::MAX;
/// Of a voice's direct sound, whether walls stop it, and whether they let
/// some through.
const OCCLUSION: i32 = 1 << 3;
const TRANSMISSION: i32 = 1 << 4;
/// The most rays a volumetric voice may trace, and how many it does.
const MAX_SAMPLES: i32 = 32;
const SAMPLES: i32 = 16;
/// The order of the spherical harmonics a path's direction comes in: 1,
/// four numbers, enough for which way it comes from.
const PATH_ORDER: i32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct IplMaterial {
    absorption: [f32; 3],
    scattering: f32,
    transmission: [f32; 3],
}

#[repr(C)]
pub(super) struct SceneSettings {
    kind: i32,
    closest_hit: Handle,
    any_hit: Handle,
    batched_closest_hit: Handle,
    batched_any_hit: Handle,
    user_data: Handle,
    embree: Handle,
    radeon_rays: Handle,
}

#[repr(C)]
pub(super) struct StaticMeshSettings {
    vertex_count: i32,
    triangle_count: i32,
    material_count: i32,
    vertices: *mut IplVector3,
    triangles: *mut [i32; 3],
    material_indices: *mut i32,
    materials: *mut IplMaterial,
}

#[repr(C)]
pub(super) struct SimulationSettings {
    flags: i32,
    scene_type: i32,
    reflection_type: i32,
    max_occlusion_samples: i32,
    max_rays: i32,
    diffuse_samples: i32,
    max_duration: f32,
    max_order: i32,
    max_sources: i32,
    threads: i32,
    ray_batch_size: i32,
    vis_samples: i32,
    sampling_rate: i32,
    frame_size: i32,
    opencl: Handle,
    radeon_rays: Handle,
    true_audio_next: Handle,
}

#[repr(C)]
pub(super) struct SourceSettings {
    flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct CoordinateSpace {
    right: IplVector3,
    up: IplVector3,
    ahead: IplVector3,
    origin: IplVector3,
}

#[repr(C)]
struct DistanceAttenuationModel {
    kind: i32,
    min_distance: f32,
    callback: Handle,
    user_data: Handle,
    dirty: i32,
}

#[repr(C)]
struct AirAbsorptionModel {
    kind: i32,
    coefficients: [f32; 3],
    callback: Handle,
    user_data: Handle,
    dirty: i32,
}

#[repr(C)]
struct Directivity {
    dipole_weight: f32,
    dipole_power: f32,
    callback: Handle,
    user_data: Handle,
}

#[repr(C)]
pub(super) struct Sphere {
    pub(super) center: IplVector3,
    pub(super) radius: f32,
}

#[repr(C)]
pub(super) struct BakedDataIdentifier {
    pub(super) kind: i32,
    pub(super) variation: i32,
    pub(super) influence: Sphere,
}

#[repr(C)]
pub(super) struct SimulationInputs {
    flags: i32,
    direct_flags: i32,
    source: CoordinateSpace,
    distance_attenuation: DistanceAttenuationModel,
    air_absorption: AirAbsorptionModel,
    directivity: Directivity,
    occlusion_type: i32,
    occlusion_radius: f32,
    occlusion_samples: i32,
    reverb_scale: [f32; 3],
    hybrid_reverb_transition_time: f32,
    hybrid_reverb_overlap: f32,
    baked: i32,
    baked_data: BakedDataIdentifier,
    pathing_probes: Handle,
    vis_radius: f32,
    vis_threshold: f32,
    vis_range: f32,
    pathing_order: i32,
    validation: i32,
    alternate_paths: i32,
    transmission_rays: i32,
    deviation_model: Handle,
}

#[repr(C)]
pub(super) struct SharedInputs {
    listener: CoordinateSpace,
    rays: i32,
    bounces: i32,
    duration: f32,
    order: i32,
    irradiance_min_distance: f32,
    pathing_visualization: Handle,
    pathing_user_data: Handle,
}

#[repr(C)]
struct DirectEffectParams {
    flags: i32,
    transmission_type: i32,
    distance_attenuation: f32,
    air_absorption: [f32; 3],
    directivity: f32,
    occlusion: f32,
    transmission: [f32; 3],
}

#[repr(C)]
pub(super) struct ReflectionEffectParams {
    pub(super) kind: i32,
    pub(super) ir: Handle,
    reverb_times: [f32; 3],
    eq: [f32; 3],
    delay: i32,
    pub(super) channels: i32,
    pub(super) ir_size: i32,
    true_audio_next: Handle,
    true_audio_next_slot: i32,
}

#[repr(C)]
struct PathEffectParams {
    eq: [f32; 3],
    spherical_harmonics: *mut f32,
    order: i32,
    binaural: i32,
    hrtf: Handle,
    listener: CoordinateSpace,
    normalize_eq: i32,
}

#[repr(C)]
pub(super) struct SimulationOutputs {
    direct: DirectEffectParams,
    pub(super) reflections: ReflectionEffectParams,
    pathing: PathEffectParams,
}

/// A struct of Steam Audio's with every field zero, null or false, as its
/// fields Voxear leaves alone should be.
pub(super) fn zeroed<T>() -> T {
    // SAFETY: only used for the plain C structs above, whose fields are numbers, pointers and options of functions.
    unsafe { std::mem::zeroed() }
}

/// What a surface is made of, as Steam Audio has it, in its three bands of
/// pitch, low, middle and high: how much of a sound striking it is soaked
/// up, how much is scattered rather than reflected as a mirror would, and
/// how much passes through it. The presets are Steam Audio's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcousticMaterial {
    pub absorption: [f32; 3],
    pub scattering: f32,
    pub transmission: [f32; 3],
}

impl AcousticMaterial {
    pub const GENERIC: Self = Self::new([0.10, 0.20, 0.30], [0.100, 0.050, 0.030]);
    pub const BRICK: Self = Self::new([0.03, 0.04, 0.07], [0.015, 0.015, 0.015]);
    pub const CONCRETE: Self = Self::new([0.05, 0.07, 0.08], [0.015, 0.002, 0.001]);
    pub const CERAMIC: Self = Self::new([0.01, 0.02, 0.02], [0.060, 0.044, 0.011]);
    pub const GRAVEL: Self = Self::new([0.60, 0.70, 0.80], [0.031, 0.012, 0.008]);
    pub const CARPET: Self = Self::new([0.24, 0.69, 0.73], [0.020, 0.005, 0.003]);
    pub const GLASS: Self = Self::new([0.06, 0.03, 0.02], [0.060, 0.044, 0.011]);
    pub const PLASTER: Self = Self::new([0.12, 0.06, 0.04], [0.056, 0.056, 0.004]);
    pub const WOOD: Self = Self::new([0.11, 0.07, 0.06], [0.070, 0.014, 0.005]);
    pub const METAL: Self = Self::new([0.20, 0.07, 0.06], [0.200, 0.025, 0.010]);
    pub const ROCK: Self = Self::new([0.13, 0.20, 0.24], [0.015, 0.002, 0.001]);

    const fn new(absorption: [f32; 3], transmission: [f32; 3]) -> Self {
        Self { absorption, scattering: 0.05, transmission }
    }

    fn steam(&self) -> IplMaterial {
        IplMaterial { absorption: self.absorption, scattering: self.scattering, transmission: self.transmission }
    }
}

/// Triangles that sound strikes, in the world, all of one material.
pub struct AcousticMesh<'a> {
    pub vertices: &'a [Vec3],
    pub triangles: &'a [[u32; 3]],
    pub material: AcousticMaterial,
}

/// An [`AcousticMesh`] that owns its triangles, to hand to another thread.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    pub material: AcousticMaterial,
}

impl Wall {
    pub fn mesh(&self) -> AcousticMesh<'_> {
        AcousticMesh { vertices: &self.vertices, triangles: &self.triangles, material: self.material }
    }
}

/// How Steam Audio tells whether a wall is in a voice's way: one ray from
/// its middle, or rays across a sphere round it, so a voice half behind a
/// corner is half heard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Occlusion {
    #[default]
    Raycast,
    Volumetric,
}

/// How a voice meets the walls: how it is occluded, the radius of its
/// sphere when volumetric, whether walls in its way let some of it
/// through, as their materials do, rather than none, and whether it is
/// heard round them too, along the level's baked paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceAcoustics {
    pub occlusion: Occlusion,
    pub radius: f32,
    pub transmission: bool,
    pub pathing: bool,
}

impl Default for SourceAcoustics {
    fn default() -> Self {
        Self { occlusion: Occlusion::Raycast, radius: 1.0, transmission: true, pathing: false }
    }
}

/// What reaches the listener of a voice past the walls: a volume in each of
/// three bands of pitch, low (below 800 Hz), middle and high (above 8 kHz).
/// 1 in each is a voice in the open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Muffle(pub [f32; 3]);

impl Muffle {
    pub const NONE: Self = Self([1.0; 3]);
}

/// What reaches the listener of a voice: how muffled it is, and, when some
/// of it comes round the walls along a baked path, which way it seems to
/// come from, as a direction in the world from the listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    pub muffle: Muffle,
    pub from: Option<Vec3>,
}

impl Heard {
    pub const OPEN: Self = Self { muffle: Muffle::NONE, from: None };
}

/// The level's walls in Steam Audio, and the voices it works out for: what
/// gets past the walls to the listener, or, made with
/// [`for_rooms`](Self::for_rooms), how long the listener's room rings, or,
/// made [`for_echoes`](Self::for_echoes), the room's echoes themselves.
pub struct Acoustics {
    pub(super) steam: Arc<SteamAudio>,
    pub(super) scene: Handle,
    simulator: Handle,
    mesh: Handle,
    sources: HashMap<u64, Handle>,
    /// The simulation it runs.
    flags: i32,
    /// The level's baked paths, when it has them.
    paths: Option<Arc<PathProbes>>,
}

// Steam Audio's scene and simulator are used from one thread at a time.
unsafe impl Send for Acoustics {}

impl Drop for Acoustics {
    fn drop(&mut self) {
        let api = &self.steam.api;
        unsafe {
            for (_, mut source) in self.sources.drain() {
                (api.source_remove)(source, self.simulator);
                (api.source_release)(&mut source);
            }
            if let Some(paths) = self.paths.take() {
                (api.simulator_remove_probe_batch)(self.simulator, paths.handle());
            }
            if !self.mesh.is_null() {
                (api.static_mesh_remove)(self.mesh, self.scene);
                (api.static_mesh_release)(&mut self.mesh);
            }
            (api.simulator_release)(&mut self.simulator);
            (api.scene_release)(&mut self.scene);
        }
    }
}

impl Acoustics {
    /// An empty level, with no walls or paths yet, loading Steam Audio if
    /// it has not been.
    pub fn new() -> Result<Self, String> {
        Self::make(SimulationSettings {
            flags: DIRECT | PATHING,
            max_occlusion_samples: MAX_SAMPLES,
            max_order: PATH_ORDER,
            max_sources: crate::MAX_VOICES as i32,
            threads: 1,
            vis_samples: 4,
            sampling_rate: 48_000,
            frame_size: FRAME as i32,
            ..zeroed()
        })
    }

    /// An empty level for working out how long rooms ring, which takes far
    /// longer than what gets past the walls: tens of milliseconds, for a
    /// thread of its own.
    pub fn for_rooms() -> Result<Self, String> {
        Self::make(room_settings(PARAMETRIC, 48_000))
    }

    /// An empty level for working out a room's echoes, as an impulse
    /// response a [`Convolution`](super::Convolution) at `sample_rate`
    /// plays the room through: as long to work out as how long it rings,
    /// and as much again to turn into the response.
    pub fn for_echoes(sample_rate: u32) -> Result<Self, String> {
        Self::make(room_settings(CONVOLUTION, sample_rate))
    }

    fn make(mut settings: SimulationSettings) -> Result<Self, String> {
        let steam = SteamAudio::get()?;
        let api = &steam.api;
        let mut scene_settings: SceneSettings = zeroed();
        let mut scene: Handle = std::ptr::null_mut();
        if unsafe { (api.scene_create)(steam.context, &mut scene_settings, &mut scene) } != 0 {
            return Err("Steam Audio made no scene".into());
        }
        let flags = settings.flags;
        let mut simulator: Handle = std::ptr::null_mut();
        if unsafe { (api.simulator_create)(steam.context, &mut settings, &mut simulator) } != 0 {
            unsafe { (api.scene_release)(&mut scene) };
            return Err("Steam Audio made no simulator".into());
        }
        unsafe {
            (api.simulator_set_scene)(simulator, scene);
            (api.simulator_commit)(simulator);
        }
        let mesh = std::ptr::null_mut();
        Ok(Self { steam, scene, simulator, mesh, sources: HashMap::new(), flags, paths: None })
    }

    /// Puts these walls in place of the ones before.
    pub fn set_geometry(&mut self, meshes: &[AcousticMesh]) {
        let api = &self.steam.api;
        if !self.mesh.is_null() {
            unsafe {
                (api.static_mesh_remove)(self.mesh, self.scene);
                (api.static_mesh_release)(&mut self.mesh);
            }
        }
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut material_indices = Vec::new();
        let mut materials = Vec::new();
        for mesh in meshes.iter().filter(|mesh| !mesh.triangles.is_empty()) {
            let first = vertices.len() as i32;
            vertices.extend(mesh.vertices.iter().map(|&v| steam(v)));
            // Turned over into Steam Audio's hand, a triangle winds the other way round.
            triangles
                .extend(mesh.triangles.iter().map(|t| [first + t[0] as i32, first + t[2] as i32, first + t[1] as i32]));
            material_indices.extend(std::iter::repeat_n(materials.len() as i32, mesh.triangles.len()));
            materials.push(mesh.material.steam());
        }
        if !triangles.is_empty() {
            let mut settings = StaticMeshSettings {
                vertex_count: vertices.len() as i32,
                triangle_count: triangles.len() as i32,
                material_count: materials.len() as i32,
                vertices: vertices.as_mut_ptr(),
                triangles: triangles.as_mut_ptr(),
                material_indices: material_indices.as_mut_ptr(),
                materials: materials.as_mut_ptr(),
            };
            let made = unsafe { (api.static_mesh_create)(self.scene, &mut settings, &mut self.mesh) };
            if made == 0 {
                unsafe { (api.static_mesh_add)(self.mesh, self.scene) };
            } else {
                self.mesh = std::ptr::null_mut();
                tracing::warn!(target: "audio", "Steam Audio took none of the level's walls");
            }
        }
        unsafe {
            (api.scene_commit)(self.scene);
            (api.simulator_commit)(self.simulator);
        }
    }

    /// Puts the level's baked paths in place of the ones before, or takes
    /// them away. Only for one made with [`new`](Self::new).
    pub fn set_paths(&mut self, paths: Option<Arc<PathProbes>>) {
        let api = &self.steam.api;
        unsafe {
            if let Some(old) = self.paths.take() {
                (api.simulator_remove_probe_batch)(self.simulator, old.handle());
            }
            if let Some(new) = &paths {
                (api.simulator_add_probe_batch)(self.simulator, new.handle());
            }
            (api.simulator_commit)(self.simulator);
        }
        self.paths = paths;
    }

    /// How much of each voice, by its key, from where it is, reaches the
    /// listener past the walls, and round them along the level's paths for
    /// voices that follow them, in the same order. Voices not asked of
    /// since the last time are let go of.
    pub fn hear(&mut self, listener: &Listener, voices: &[(u64, Vec3, SourceAcoustics)]) -> Vec<Heard> {
        self.keep_sources(voices);
        let api = &self.steam.api;
        let probes = self.paths.as_ref().map(|paths| paths.handle());
        let follows = |settings: &SourceAcoustics| if settings.pathing && probes.is_some() { PATHING } else { 0 };
        for (key, at, settings) in voices {
            let mut inputs = inputs(*at, settings);
            if let Some(probes) = probes.filter(|_| settings.pathing) {
                along_paths(&mut inputs, probes);
            }
            unsafe { (api.source_set_inputs)(self.sources[key], DIRECT | follows(settings), &mut inputs) };
        }
        let pathing = voices.iter().fold(0, |flags, (.., settings)| flags | follows(settings));
        let mut shared = SharedInputs { listener: listener_space(listener), ..zeroed() };
        unsafe {
            (api.simulator_set_shared_inputs)(self.simulator, DIRECT | pathing, &mut shared);
            (api.simulator_run_direct)(self.simulator);
            if pathing != 0 {
                (api.simulator_run_pathing)(self.simulator);
            }
        }
        voices
            .iter()
            .map(|(key, at, settings)| {
                let mut outputs: SimulationOutputs = zeroed();
                let mut harmonics = [0.0f32; 16];
                outputs.pathing.spherical_harmonics = harmonics.as_mut_ptr();
                let flags = DIRECT | follows(settings);
                unsafe { (api.source_get_outputs)(self.sources[key], flags, &mut outputs) };
                let through = muffle(&outputs.direct, settings.transmission);
                match flags & PATHING {
                    0 => Heard { muffle: through, from: None },
                    _ => round(listener, *at, &outputs, through),
                }
            })
            .collect()
    }

    /// Gives every voice asked of a source in the simulator, and lets go of
    /// the sources of voices no longer asked of.
    fn keep_sources(&mut self, voices: &[(u64, Vec3, SourceAcoustics)]) {
        let api = &self.steam.api;
        let mut changed = false;
        let mut stale: Vec<u64> = self.sources.keys().copied().collect();
        stale.retain(|key| !voices.iter().any(|(asked, ..)| asked == key));
        for key in stale {
            let Some(mut source) = self.sources.remove(&key) else { continue };
            unsafe {
                (api.source_remove)(source, self.simulator);
                (api.source_release)(&mut source);
            }
            changed = true;
        }
        for (key, ..) in voices {
            if self.sources.contains_key(key) {
                continue;
            }
            let mut settings = SourceSettings { flags: self.flags };
            let mut source: Handle = std::ptr::null_mut();
            if unsafe { (api.source_create)(self.simulator, &mut settings, &mut source) } != 0 {
                continue;
            }
            unsafe { (api.source_add)(source, self.simulator) };
            self.sources.insert(*key, source);
            changed = true;
        }
        if changed {
            unsafe { (api.simulator_commit)(self.simulator) };
        }
    }
}

impl Acoustics {
    /// How long sound lingers where the listener is, in each band, from
    /// reflections round the walls about it. Only for one made
    /// [`for_rooms`](Self::for_rooms).
    pub fn room(&mut self, listener: &Listener) -> ReverbTimes {
        self.echo(listener);
        let api = &self.steam.api;
        let mut outputs: SimulationOutputs = zeroed();
        unsafe { (api.source_get_outputs)(self.sources[&ROOM], REFLECTIONS, &mut outputs) };
        ReverbTimes(
            outputs.reflections.reverb_times.map(|seconds| if seconds.is_finite() { seconds.max(0.0) } else { 0.0 }),
        )
    }

    /// Works out again the reflections round the walls about the listener,
    /// from a sound where it stands. Made [`for_echoes`](Self::for_echoes),
    /// the room's echoes are left for the [`Convolution`](super::Convolution)
    /// playing them to take, on the audio's thread.
    pub fn echo(&mut self, listener: &Listener) {
        let at = listener.position;
        self.keep_sources(&[(ROOM, at, SourceAcoustics::default())]);
        let api = &self.steam.api;
        let mut inputs = SimulationInputs {
            flags: REFLECTIONS,
            source: space(at, Vec3::X, Vec3::Y, Vec3::Z),
            reverb_scale: [1.0; 3],
            ..zeroed()
        };
        let mut shared = SharedInputs {
            listener: listener_space(listener),
            rays: ROOM_RAYS,
            bounces: ROOM_BOUNCES,
            duration: ROOM_SECONDS,
            order: ROOM_ORDER,
            irradiance_min_distance: 1.0,
            ..zeroed()
        };
        unsafe {
            (api.source_set_inputs)(self.sources[&ROOM], REFLECTIONS, &mut inputs);
            (api.simulator_set_shared_inputs)(self.simulator, REFLECTIONS, &mut shared);
            (api.simulator_run_reflections)(self.simulator);
        }
    }

    /// The source a room is heard from, given to the listener's place, with
    /// a reference of its own for whoever takes it, which they release.
    pub(super) fn room_source(&mut self, listener: &Listener) -> Handle {
        self.keep_sources(&[(ROOM, listener.position, SourceAcoustics::default())]);
        unsafe { (self.steam.api.source_retain)(self.sources[&ROOM]) }
    }
}

/// A simulator for one room's reflections, the listener's, heard as
/// `reflection_type` has them.
fn room_settings(reflection_type: i32, sample_rate: u32) -> SimulationSettings {
    SimulationSettings {
        flags: REFLECTIONS,
        reflection_type,
        max_rays: ROOM_RAYS,
        diffuse_samples: 32,
        max_duration: ROOM_SECONDS,
        max_order: ROOM_ORDER,
        max_sources: 1,
        threads: 2,
        ray_batch_size: 1,
        sampling_rate: sample_rate as i32,
        frame_size: FRAME as i32,
        ..zeroed()
    }
}

/// What Steam Audio is to work out of a voice at `at`.
fn inputs(at: Vec3, settings: &SourceAcoustics) -> SimulationInputs {
    let transmission = if settings.transmission { TRANSMISSION } else { 0 };
    let volumetric = settings.occlusion == Occlusion::Volumetric;
    SimulationInputs {
        flags: DIRECT,
        direct_flags: OCCLUSION | transmission,
        source: space(at, Vec3::X, Vec3::Y, Vec3::Z),
        occlusion_type: i32::from(volumetric),
        occlusion_radius: settings.radius.max(0.01),
        occlusion_samples: if volumetric { SAMPLES } else { 1 },
        transmission_rays: 1,
        ..zeroed()
    }
}

/// Asks Steam Audio to find a voice's path to the listener among these
/// probes, from those that see it and the listener now: the walls are
/// those the paths were baked round, so a path is not tested again.
fn along_paths(inputs: &mut SimulationInputs, probes: Handle) {
    inputs.flags |= PATHING;
    inputs.pathing_probes = probes;
    inputs.vis_radius = VISIBLE_RADIUS;
    inputs.vis_threshold = VISIBLE_SHARE;
    inputs.vis_range = VISIBLE_RANGE;
    inputs.pathing_order = PATH_ORDER;
}

/// A voice heard through the walls and round them: of what the walls stop,
/// the share its path brings round, added to what gets through, heard
/// from the path's way in as much as that outweighs the sound straight
/// through.
fn round(listener: &Listener, at: Vec3, outputs: &SimulationOutputs, through: Muffle) -> Heard {
    let blocked = 1.0 - outputs.direct.occlusion.clamp(0.0, 1.0);
    let path = outputs.pathing.eq.map(|gain| if gain.is_finite() { gain.clamp(0.0, 1.0) * blocked } else { 0.0 });
    let muffle = Muffle(std::array::from_fn(|band| (through.0[band] + path[band]).min(1.0)));
    let mean = |bands: [f32; 3]| bands.iter().sum::<f32>() / 3.0;
    let (round, straight) = (mean(path), mean(through.0));
    let Some(way) = path_direction(&outputs.pathing).filter(|_| round > 1e-4) else {
        return Heard { muffle, from: None };
    };
    let ahead = (at - listener.position).try_normalize().unwrap_or(way);
    Heard { muffle, from: (way * round + ahead * straight).try_normalize() }
}

/// Which way a path comes in to the listener, from the first order of its
/// spherical harmonics, in Voxear's world.
fn path_direction(path: &PathEffectParams) -> Option<Vec3> {
    if path.spherical_harmonics.is_null() {
        return None;
    }
    // SAFETY: Steam Audio fills at least the four of order 1 asked for.
    let harmonics = unsafe { std::slice::from_raw_parts(path.spherical_harmonics, 4) };
    // In order, after the first: Y, Z and X of Ambisonics' axes, X ahead, Y left and Z up. In Steam Audio's they
    // are -X, Y and -Z, so the way in is (-Y, Z, -X) there, and turned over into Voxear's, (-Y, Z, X).
    let [_, y, z, x] = [harmonics[0], harmonics[1], harmonics[2], harmonics[3]];
    let way = Vec3::new(-y, z, x);
    way.is_finite().then(|| way.try_normalize()).flatten()
}

/// The listener's place and the ways it faces, as Steam Audio has them.
pub(super) fn listener_space(listener: &Listener) -> CoordinateSpace {
    let turn = listener.rotation;
    space(listener.position, turn * Vec3::X, turn * Vec3::Y, turn * Vec3::Z)
}

fn space(origin: Vec3, right: Vec3, up: Vec3, ahead: Vec3) -> CoordinateSpace {
    CoordinateSpace { right: steam(right), up: steam(up), ahead: steam(ahead), origin: steam(origin) }
}

/// The volume in each band of what gets past the walls: what is not
/// occluded, and of what is, what the walls let through.
fn muffle(direct: &DirectEffectParams, transmission: bool) -> Muffle {
    let open = direct.occlusion.clamp(0.0, 1.0);
    if !transmission {
        return Muffle([open; 3]);
    }
    Muffle(direct.transmission.map(|through| open + (1.0 - open) * through.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn muffles(heard: Vec<Heard>) -> Vec<Muffle> {
        heard.into_iter().map(|heard| heard.muffle).collect()
    }

    /// A wall across the X axis at `x`, 10 metres square.
    fn wall(x: f32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let vertices =
            vec![Vec3::new(x, -5.0, -5.0), Vec3::new(x, 5.0, -5.0), Vec3::new(x, 5.0, 5.0), Vec3::new(x, -5.0, 5.0)];
        (vertices, vec![[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn a_wall_between_muffles_a_voice_by_its_material() {
        let Ok(mut acoustics) = Acoustics::new() else {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        };
        let listener = Listener::default();
        let behind = Vec3::new(6.0, 0.0, 0.0);
        let open = Vec3::new(-6.0, 0.0, 0.0);
        let settings = SourceAcoustics::default();
        let voices = [(1, behind, settings), (2, open, settings)];
        assert_eq!(muffles(acoustics.hear(&listener, &voices)), [Muffle::NONE, Muffle::NONE], "no walls yet");

        let (vertices, triangles) = wall(3.0);
        acoustics.set_geometry(&[AcousticMesh {
            vertices: &vertices,
            triangles: &triangles,
            material: AcousticMaterial::CONCRETE,
        }]);
        let heard = muffles(acoustics.hear(&listener, &voices));
        assert_eq!(heard[1], Muffle::NONE, "the other side is open");
        let [low, middle, high] = heard[0].0;
        assert!(low < 0.1 && high < middle && middle < low, "through concrete, the high notes least: {:?}", heard[0]);

        let shut = SourceAcoustics { transmission: false, ..settings };
        assert_eq!(muffles(acoustics.hear(&listener, &[(1, behind, shut)])), [Muffle([0.0; 3])], "nothing through");

        acoustics.set_geometry(&[]);
        assert_eq!(muffles(acoustics.hear(&listener, &[(1, behind, settings)])), [Muffle::NONE], "the wall gone");
    }

    #[test]
    fn a_volumetric_voice_half_behind_a_wall_is_half_heard() {
        let Ok(mut acoustics) = Acoustics::new() else {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        };
        // A wall ending at y = 0, a voice at its end, a metre round it.
        let vertices = vec![
            Vec3::new(3.0, -10.0, -5.0),
            Vec3::new(3.0, 0.0, -5.0),
            Vec3::new(3.0, 0.0, 5.0),
            Vec3::new(3.0, -10.0, 5.0),
        ];
        let triangles = [[0, 1, 2], [0, 2, 3]];
        let material = AcousticMaterial { transmission: [0.0; 3], ..AcousticMaterial::BRICK };
        acoustics.set_geometry(&[AcousticMesh { vertices: &vertices, triangles: &triangles, material }]);
        let settings =
            SourceAcoustics { occlusion: Occlusion::Volumetric, radius: 1.0, transmission: false, pathing: false };
        let heard = acoustics.hear(&Listener::default(), &[(7, Vec3::new(6.0, 0.0, 0.0), settings)]);
        let [low, ..] = heard[0].muffle.0;
        assert!(low > 0.1 && low < 0.9, "partly heard: {low}");
    }
}
