//! Steam Audio, Valve's spatial audio library (Apache 2.0; see
//! `steam-audio/README.md`), loaded when it is first wanted rather than
//! linked, so that Voxear builds and plays without it. Its C API is bound
//! here as `steam-audio/include/phonon.h` declares it, as far as Voxear
//! uses it: a context, an HRTF, and the binaural effect that renders a mono
//! voice for headphones from where it is, with the time and frequency cues
//! of a real head.
//!
//! Steam Audio's effects work on blocks of a fixed size, [`FRAME`] samples,
//! at the mixer's rate. Its world is right-handed, with -Z ahead, where
//! Voxear's has +Z ahead, so a direction's Z turns over on the way in.
//!
//! [`Acoustics`] is the rest: the level's walls as Steam Audio's scene, and
//! how much of each voice gets through them to the listener.

mod acoustics;

use std::{
    ffi::{c_char, c_void},
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use anyhow::{Context, Result, bail};
use libloading::Library;
use voxear_core::math::Vec3;

use crate::spatial::Listener;
pub use acoustics::{AcousticMaterial, AcousticMesh, Acoustics, Muffle, Occlusion, SourceAcoustics};
use acoustics::{
    SceneSettings, SharedInputs, SimulationInputs, SimulationOutputs, SimulationSettings, SourceSettings,
    StaticMeshSettings,
};

/// How many samples Steam Audio's effects take at once.
pub const FRAME: usize = 256;

/// The Steam Audio this binding follows: 4.8.1.
const VERSION: u32 = 4 << 16 | 8 << 8 | 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct IplVector3 {
    x: f32,
    y: f32,
    z: f32,
}

#[repr(C)]
struct ContextSettings {
    version: u32,
    log: Option<unsafe extern "C" fn(i32, *const c_char)>,
    allocate: *mut c_void,
    free: *mut c_void,
    simd_level: i32,
    flags: i32,
}

#[repr(C)]
struct AudioSettings {
    sampling_rate: i32,
    frame_size: i32,
}

#[repr(C)]
struct AudioBuffer {
    channels: i32,
    samples: i32,
    data: *mut *mut f32,
}

#[repr(C)]
struct HrtfSettings {
    kind: i32,
    sofa_file: *const c_char,
    sofa_data: *const u8,
    sofa_size: i32,
    volume: f32,
    normalization: i32,
}

#[repr(C)]
struct BinauralSettings {
    hrtf: *mut c_void,
}

#[repr(C)]
struct BinauralParams {
    direction: IplVector3,
    interpolation: i32,
    spatial_blend: f32,
    hrtf: *mut c_void,
    peak_delays: *mut f32,
}

/// Steam Audio's binaural interpolation: bilinear, smooth as a voice moves.
const BILINEAR: i32 = 1;

type Handle = *mut c_void;

/// The functions Voxear calls, looked up once.
struct Api {
    context_create: unsafe extern "C" fn(*mut ContextSettings, *mut Handle) -> i32,
    hrtf_create: unsafe extern "C" fn(Handle, *mut AudioSettings, *mut HrtfSettings, *mut Handle) -> i32,
    hrtf_release: unsafe extern "C" fn(*mut Handle),
    binaural_create: unsafe extern "C" fn(Handle, *mut AudioSettings, *mut BinauralSettings, *mut Handle) -> i32,
    binaural_release: unsafe extern "C" fn(*mut Handle),
    binaural_apply: unsafe extern "C" fn(Handle, *mut BinauralParams, *mut AudioBuffer, *mut AudioBuffer) -> i32,
    scene_create: unsafe extern "C" fn(Handle, *mut SceneSettings, *mut Handle) -> i32,
    scene_commit: unsafe extern "C" fn(Handle),
    scene_release: unsafe extern "C" fn(*mut Handle),
    static_mesh_create: unsafe extern "C" fn(Handle, *mut StaticMeshSettings, *mut Handle) -> i32,
    static_mesh_add: unsafe extern "C" fn(Handle, Handle),
    static_mesh_remove: unsafe extern "C" fn(Handle, Handle),
    static_mesh_release: unsafe extern "C" fn(*mut Handle),
    simulator_create: unsafe extern "C" fn(Handle, *mut SimulationSettings, *mut Handle) -> i32,
    simulator_set_scene: unsafe extern "C" fn(Handle, Handle),
    simulator_set_shared_inputs: unsafe extern "C" fn(Handle, i32, *mut SharedInputs),
    simulator_commit: unsafe extern "C" fn(Handle),
    simulator_run_direct: unsafe extern "C" fn(Handle),
    simulator_release: unsafe extern "C" fn(*mut Handle),
    source_create: unsafe extern "C" fn(Handle, *mut SourceSettings, *mut Handle) -> i32,
    source_add: unsafe extern "C" fn(Handle, Handle),
    source_remove: unsafe extern "C" fn(Handle, Handle),
    source_set_inputs: unsafe extern "C" fn(Handle, i32, *mut SimulationInputs),
    source_get_outputs: unsafe extern "C" fn(Handle, i32, *mut SimulationOutputs),
    source_release: unsafe extern "C" fn(*mut Handle),
}

/// Steam Audio, loaded: its library and a context. It loads once, the first
/// time anything asks with [`SteamAudio::get`], and stays for as long as
/// the program runs.
pub struct SteamAudio {
    api: Api,
    context: Handle,
    /// Where it was loaded from.
    pub path: PathBuf,
    _library: Library,
}

// Steam Audio's context, and the HRTFs made from it, may be shared between
// threads; each effect is used by one thread at a time.
unsafe impl Send for SteamAudio {}
unsafe impl Sync for SteamAudio {}

/// Steam Audio's licence and the notices of what it is built from, which
/// go with every copy of its library given out, as a player build's.
pub const NOTICES: [(&str, &str); 2] = [
    ("LICENSE.md", include_str!("../steam-audio/LICENSE.md")),
    ("THIRDPARTY.md", include_str!("../steam-audio/THIRDPARTY.md")),
];

/// The library's file name on this system.
pub fn file_name() -> &'static str {
    if cfg!(windows) { "phonon.dll" } else { "libphonon.so" }
}

/// Where the library is looked for, in order: the path in
/// `VOXEAR_STEAM_AUDIO`, beside the running program, and the copy
/// `steam-audio/fetch.sh` fetched for this source tree.
pub fn library_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("VOXEAR_STEAM_AUDIO") {
        paths.push(PathBuf::from(path));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        paths.push(dir.join(file_name()));
    }
    let platform = if cfg!(windows) { "windows-x64" } else { "linux-x64" };
    paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("steam-audio/lib").join(platform).join(file_name()));
    paths
}

impl SteamAudio {
    /// Steam Audio, loading it from the first of [`library_paths`] there
    /// the first time. Why it is not here, when it is not: the same answer
    /// each time, as it is tried only once.
    pub fn get() -> Result<Arc<Self>, String> {
        static LOADED: OnceLock<Result<Arc<SteamAudio>, String>> = OnceLock::new();
        LOADED.get_or_init(|| Self::load().map(Arc::new).map_err(|error| format!("{error:#}"))).clone()
    }

    fn load() -> Result<Self> {
        let Some(path) = library_paths().into_iter().find(|path| path.is_file()) else {
            bail!("Steam Audio's library is not here; fetch it with crates/voxear_audio/steam-audio/fetch.sh");
        };
        let library = unsafe { Library::new(&path) }.with_context(|| format!("loading {}", path.display()))?;
        let api = unsafe { Api::from(&library) }.context("Steam Audio's library lacks a function Voxear calls")?;
        let mut settings = ContextSettings {
            version: VERSION,
            log: None,
            allocate: std::ptr::null_mut(),
            free: std::ptr::null_mut(),
            simd_level: 3,
            flags: 0,
        };
        let mut context: Handle = std::ptr::null_mut();
        if unsafe { (api.context_create)(&mut settings, &mut context) } != 0 {
            bail!("Steam Audio would not start");
        }
        tracing::info!(target: "audio", "Steam Audio 4.8.1 loaded from {}", path.display());
        Ok(Self { api, context, path, _library: library })
    }
}

impl Api {
    unsafe fn from(library: &Library) -> Result<Self> {
        macro_rules! get {
            ($name:literal) => {
                *unsafe { library.get(concat!($name, "\0").as_bytes()) }.context($name)?
            };
        }
        Ok(Self {
            context_create: get!("iplContextCreate"),
            hrtf_create: get!("iplHRTFCreate"),
            hrtf_release: get!("iplHRTFRelease"),
            binaural_create: get!("iplBinauralEffectCreate"),
            binaural_release: get!("iplBinauralEffectRelease"),
            binaural_apply: get!("iplBinauralEffectApply"),
            scene_create: get!("iplSceneCreate"),
            scene_commit: get!("iplSceneCommit"),
            scene_release: get!("iplSceneRelease"),
            static_mesh_create: get!("iplStaticMeshCreate"),
            static_mesh_add: get!("iplStaticMeshAdd"),
            static_mesh_remove: get!("iplStaticMeshRemove"),
            static_mesh_release: get!("iplStaticMeshRelease"),
            simulator_create: get!("iplSimulatorCreate"),
            simulator_set_scene: get!("iplSimulatorSetScene"),
            simulator_set_shared_inputs: get!("iplSimulatorSetSharedInputs"),
            simulator_commit: get!("iplSimulatorCommit"),
            simulator_run_direct: get!("iplSimulatorRunDirect"),
            simulator_release: get!("iplSimulatorRelease"),
            source_create: get!("iplSourceCreate"),
            source_add: get!("iplSourceAdd"),
            source_remove: get!("iplSourceRemove"),
            source_set_inputs: get!("iplSourceSetInputs"),
            source_get_outputs: get!("iplSourceGetOutputs"),
            source_release: get!("iplSourceRelease"),
        })
    }
}

/// How a head hears, at one sample rate: Steam Audio's own HRTF, which suits
/// most people. Making one takes a moment, so the mixer makes it away from
/// its own thread.
pub struct Hrtf {
    steam: Arc<SteamAudio>,
    handle: Handle,
    pub sample_rate: u32,
}

unsafe impl Send for Hrtf {}
unsafe impl Sync for Hrtf {}

impl Drop for Hrtf {
    fn drop(&mut self) {
        unsafe { (self.steam.api.hrtf_release)(&mut self.handle) };
    }
}

impl Hrtf {
    /// The HRTF for a mixer at `sample_rate`, loading Steam Audio if it has
    /// not been.
    pub fn new(sample_rate: u32) -> Result<Arc<Self>, String> {
        let steam = SteamAudio::get()?;
        let mut audio = AudioSettings { sampling_rate: sample_rate as i32, frame_size: FRAME as i32 };
        let mut settings = HrtfSettings {
            kind: 0,
            sofa_file: std::ptr::null(),
            sofa_data: std::ptr::null(),
            sofa_size: 0,
            volume: 1.0,
            normalization: 0,
        };
        let mut handle: Handle = std::ptr::null_mut();
        if unsafe { (steam.api.hrtf_create)(steam.context, &mut audio, &mut settings, &mut handle) } != 0 {
            return Err("Steam Audio made no HRTF".into());
        }
        Ok(Arc::new(Self { steam, handle, sample_rate }))
    }
}

/// One voice rendered for headphones: Steam Audio's binaural effect, and
/// the blocks it works on.
pub struct Binaural {
    hrtf: Arc<Hrtf>,
    effect: Handle,
    left: Vec<f32>,
    right: Vec<f32>,
}

unsafe impl Send for Binaural {}

impl Drop for Binaural {
    fn drop(&mut self) {
        unsafe { (self.hrtf.steam.api.binaural_release)(&mut self.effect) };
    }
}

impl Binaural {
    pub fn new(hrtf: &Arc<Hrtf>) -> Option<Self> {
        let steam = &hrtf.steam;
        let mut audio = AudioSettings { sampling_rate: hrtf.sample_rate as i32, frame_size: FRAME as i32 };
        let mut settings = BinauralSettings { hrtf: hrtf.handle };
        let mut effect: Handle = std::ptr::null_mut();
        let made = unsafe { (steam.api.binaural_create)(steam.context, &mut audio, &mut settings, &mut effect) };
        (made == 0).then(|| Self { hrtf: hrtf.clone(), effect, left: vec![0.0; FRAME], right: vec![0.0; FRAME] })
    }

    /// Renders a block of [`FRAME`] mono samples heard from `at` by the
    /// listener into stereo, adding it to `out`, two to a frame.
    pub fn apply(&mut self, mono: &mut [f32; FRAME], listener: &Listener, at: Vec3, out: &mut impl Extend<[f32; 2]>) {
        let mut params = BinauralParams {
            direction: direction(listener, at),
            interpolation: BILINEAR,
            spatial_blend: 1.0,
            hrtf: self.hrtf.handle,
            peak_delays: std::ptr::null_mut(),
        };
        let mut input_channels = [mono.as_mut_ptr()];
        let mut output_channels = [self.left.as_mut_ptr(), self.right.as_mut_ptr()];
        let mut input = AudioBuffer { channels: 1, samples: FRAME as i32, data: input_channels.as_mut_ptr() };
        let mut output = AudioBuffer { channels: 2, samples: FRAME as i32, data: output_channels.as_mut_ptr() };
        unsafe { (self.hrtf.steam.api.binaural_apply)(self.effect, &mut params, &mut input, &mut output) };
        out.extend(self.left.iter().zip(&self.right).map(|(&l, &r)| [l, r]));
    }
}

/// Which way a voice at `at` is from the listener, in the listener's own
/// space as Steam Audio has it: right-handed, with -Z ahead. One on top of
/// the listener is ahead.
fn direction(listener: &Listener, at: Vec3) -> IplVector3 {
    let local = listener.rotation.inverse() * (at - listener.position);
    steam(local.try_normalize().unwrap_or(Vec3::Z))
}

/// A point or direction in Voxear's world as Steam Audio's has it.
fn steam(v: Vec3) -> IplVector3 {
    IplVector3 { x: v.x, y: v.y, z: -v.z }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_turn_over_into_steam_audios_hand() {
        let listener = Listener::default();
        assert_eq!(direction(&listener, Vec3::new(0.0, 0.0, 5.0)), IplVector3 { x: 0.0, y: 0.0, z: -1.0 });
        assert_eq!(direction(&listener, Vec3::new(3.0, 0.0, 0.0)), IplVector3 { x: 1.0, y: 0.0, z: 0.0 });
        assert_eq!(direction(&listener, listener.position), IplVector3 { x: 0.0, y: 0.0, z: -1.0 }, "on top: ahead");
    }

    #[test]
    fn a_voice_to_the_right_is_louder_in_the_right_ear() {
        let Ok(hrtf) = Hrtf::new(48_000) else {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        };
        let mut binaural = Binaural::new(&hrtf).expect("a binaural effect");
        let listener = Listener::default();
        let mut out = Vec::new();
        // A few blocks of noise, for the effect to settle.
        let mut seed = 1u32;
        for _ in 0..8 {
            let mut block = [0.0f32; FRAME];
            for sample in &mut block {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *sample = (seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5;
            }
            binaural.apply(&mut block, &listener, Vec3::new(5.0, 0.0, 0.0), &mut out);
        }
        let energy = |side: usize| out[FRAME * 4..].iter().map(|s| s[side] * s[side]).sum::<f32>();
        assert!(energy(1) > energy(0) * 2.0, "right {} left {}", energy(1), energy(0));
    }
}
