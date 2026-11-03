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

use super::{Handle, IplVector3, SteamAudio, steam};
use crate::{spatial::Listener, steam_audio::FRAME};

/// Steam Audio's simulation: what it works out of a voice's direct sound.
const DIRECT: i32 = 1;
/// Of a voice's direct sound, whether walls stop it, and whether they let
/// some through.
const OCCLUSION: i32 = 1 << 3;
const TRANSMISSION: i32 = 1 << 4;
/// The most rays a volumetric voice may trace, and how many it does.
const MAX_SAMPLES: i32 = 32;
const SAMPLES: i32 = 16;

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
struct Sphere {
    center: IplVector3,
    radius: f32,
}

#[repr(C)]
struct BakedDataIdentifier {
    kind: i32,
    variation: i32,
    influence: Sphere,
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
struct ReflectionEffectParams {
    kind: i32,
    ir: Handle,
    reverb_times: [f32; 3],
    eq: [f32; 3],
    delay: i32,
    channels: i32,
    ir_size: i32,
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
    reflections: ReflectionEffectParams,
    pathing: PathEffectParams,
}

/// A struct of Steam Audio's with every field zero, null or false, as its
/// fields Voxear leaves alone should be.
fn zeroed<T>() -> T {
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
/// sphere when volumetric, and whether walls in its way let some of it
/// through, as their materials do, rather than none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceAcoustics {
    pub occlusion: Occlusion,
    pub radius: f32,
    pub transmission: bool,
}

impl Default for SourceAcoustics {
    fn default() -> Self {
        Self { occlusion: Occlusion::Raycast, radius: 1.0, transmission: true }
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

/// The level's walls in Steam Audio, and the voices it works out for.
pub struct Acoustics {
    steam: Arc<SteamAudio>,
    scene: Handle,
    simulator: Handle,
    mesh: Handle,
    sources: HashMap<u64, Handle>,
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
    /// An empty level, with no walls yet, loading Steam Audio if it has not
    /// been.
    pub fn new() -> Result<Self, String> {
        let steam = SteamAudio::get()?;
        let api = &steam.api;
        let mut scene_settings: SceneSettings = zeroed();
        let mut scene: Handle = std::ptr::null_mut();
        if unsafe { (api.scene_create)(steam.context, &mut scene_settings, &mut scene) } != 0 {
            return Err("Steam Audio made no scene".into());
        }
        let mut settings = SimulationSettings {
            flags: DIRECT,
            max_occlusion_samples: MAX_SAMPLES,
            max_sources: crate::MAX_VOICES as i32,
            threads: 1,
            sampling_rate: 48_000,
            frame_size: FRAME as i32,
            ..zeroed()
        };
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
        Ok(Self { steam, scene, simulator, mesh, sources: HashMap::new() })
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

    /// How much of each voice, by its key, from where it is, reaches the
    /// listener past the walls, in the same order. Voices not asked of
    /// since the last time are let go of.
    pub fn hear(&mut self, listener: &Listener, voices: &[(u64, Vec3, SourceAcoustics)]) -> Vec<Muffle> {
        self.keep_sources(voices);
        let api = &self.steam.api;
        for (key, at, settings) in voices {
            let mut inputs = inputs(*at, settings);
            unsafe { (api.source_set_inputs)(self.sources[key], DIRECT, &mut inputs) };
        }
        let mut shared = SharedInputs { listener: listener_space(listener), ..zeroed() };
        unsafe {
            (api.simulator_set_shared_inputs)(self.simulator, DIRECT, &mut shared);
            (api.simulator_run_direct)(self.simulator);
        }
        voices
            .iter()
            .map(|(key, _, settings)| {
                let mut outputs: SimulationOutputs = zeroed();
                unsafe { (api.source_get_outputs)(self.sources[key], DIRECT, &mut outputs) };
                muffle(&outputs.direct, settings.transmission)
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
            let mut settings = SourceSettings { flags: DIRECT };
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

/// The listener's place and the ways it faces, as Steam Audio has them.
fn listener_space(listener: &Listener) -> CoordinateSpace {
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
        assert_eq!(acoustics.hear(&listener, &voices), [Muffle::NONE, Muffle::NONE], "no walls yet");

        let (vertices, triangles) = wall(3.0);
        acoustics.set_geometry(&[AcousticMesh {
            vertices: &vertices,
            triangles: &triangles,
            material: AcousticMaterial::CONCRETE,
        }]);
        let heard = acoustics.hear(&listener, &voices);
        assert_eq!(heard[1], Muffle::NONE, "the other side is open");
        let [low, middle, high] = heard[0].0;
        assert!(low < 0.1 && high < middle && middle < low, "through concrete, the high notes least: {:?}", heard[0]);

        let shut = SourceAcoustics { transmission: false, ..settings };
        assert_eq!(acoustics.hear(&listener, &[(1, behind, shut)]), [Muffle([0.0; 3])], "nothing through");

        acoustics.set_geometry(&[]);
        assert_eq!(acoustics.hear(&listener, &[(1, behind, settings)]), [Muffle::NONE], "the wall gone");
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
        let settings = SourceAcoustics { occlusion: Occlusion::Volumetric, radius: 1.0, transmission: false };
        let heard = acoustics.hear(&Listener::default(), &[(7, Vec3::new(6.0, 0.0, 0.0), settings)]);
        let [low, ..] = heard[0].0;
        assert!(low > 0.1 && low < 0.9, "partly heard: {low}");
    }
}
