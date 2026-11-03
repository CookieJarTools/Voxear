//! Sound for scripts: `Sound`, `Audio` and `AudioSource` in `Voxear.Core`.
//! The engine does the work through [`ScriptServices`]: loading sounds,
//! playing them, and its Audio Sources, which live in the world.
//!
//! A voice, an Audio Source or a bus is read, and written when `set` is
//! not 0, through one call each, with what to read as a number below. A
//! sound goes through those as its id, and -1 for none.

use crate::{
    interop::{utf8, with_world},
    services::with_services,
};
use voxear_ecs::Entity;

/// What a loaded sound holds, for `Sound`'s properties.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoundFacts {
    pub seconds: f32,
    pub sample_rate: i32,
    pub channels: i32,
    pub streamed: i32,
}

/// How `Audio.Play` plays a sound once.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySound {
    pub volume: f32,
    pub pitch: f32,
    /// 0 for effects, 1 for music.
    pub bus: i32,
    /// Not 0 to play at `position`, fading out to `max_distance`.
    pub spatial: i32,
    pub position: [f32; 3],
    pub min_distance: f32,
    pub max_distance: f32,
}

/// What a script reads or sets of the sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioAsk {
    Voice(u64, VoiceValue),
    Source(Entity, SourceValue),
    Bus(BusValue),
}

/// What of a voice: whether it plays, is paused, its volume and pitch; or
/// stop it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceValue {
    Playing,
    Paused,
    Stop,
    Volume,
    Pitch,
}

impl VoiceValue {
    fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Playing,
            1 => Self::Paused,
            2 => Self::Stop,
            3 => Self::Volume,
            4 => Self::Pitch,
            _ => return None,
        })
    }
}

/// What of an Obj's Audio Source. `Exists` is 1 when it has one; `Add`
/// gives it one if not; `Play`, `Pause` and `Stop` ask it to, when set.
/// The rest are its settings, `Spatial` and `Loop` as 1 or 0, `Bus` 0 for
/// effects and 1 for music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceValue {
    Exists,
    Add,
    Play,
    Pause,
    Stop,
    Playing,
    Sound,
    Volume,
    Pitch,
    Loop,
    Spatial,
    MinDistance,
    MaxDistance,
    Bus,
    PlayOnStart,
    Doppler,
    Occlusion,
    OcclusionRadius,
    Transmission,
    Pathing,
}

impl SourceValue {
    fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Exists,
            1 => Self::Add,
            2 => Self::Play,
            3 => Self::Pause,
            4 => Self::Stop,
            5 => Self::Playing,
            6 => Self::Sound,
            7 => Self::Volume,
            8 => Self::Pitch,
            9 => Self::Loop,
            10 => Self::Spatial,
            11 => Self::MinDistance,
            12 => Self::MaxDistance,
            13 => Self::Bus,
            14 => Self::PlayOnStart,
            15 => Self::Doppler,
            16 => Self::Occlusion,
            17 => Self::OcclusionRadius,
            18 => Self::Transmission,
            19 => Self::Pathing,
            _ => return None,
        })
    }
}

/// Which volume: the master, over both buses, or a bus's; or, as 1 or 0,
/// whether 3D voices are heard as through headphones, whether they can be,
/// with Steam Audio here, and whether they ring as the listener's room does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusValue {
    Master,
    Effects,
    Music,
    Headphones,
    SteamAudio,
    Reverb,
    /// How long the changes after it take, in seconds.
    Transition,
    /// A bus's filter cutoffs, in Hz.
    MusicLowPass,
    EffectsLowPass,
    MusicHighPass,
    EffectsHighPass,
    /// The music's ducking under the effects: by how many decibels, 0 for
    /// none; above what level; and how fast it goes down and comes back.
    DuckAmount,
    DuckThreshold,
    DuckAttack,
    DuckRelease,
}

impl BusValue {
    fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Master,
            1 => Self::Effects,
            2 => Self::Music,
            3 => Self::Headphones,
            4 => Self::SteamAudio,
            5 => Self::Reverb,
            6 => Self::Transition,
            7 => Self::MusicLowPass,
            8 => Self::EffectsLowPass,
            9 => Self::MusicHighPass,
            10 => Self::EffectsHighPass,
            11 => Self::DuckAmount,
            12 => Self::DuckThreshold,
            13 => Self::DuckAttack,
            14 => Self::DuckRelease,
            _ => return None,
        })
    }
}

pub(crate) unsafe extern "C" fn sound_load(reference: *const u8, len: i32) -> i32 {
    let reference = unsafe { utf8(reference, len) };
    match with_services(|services| services.load_sound(&reference)) {
        Some(Ok(sound)) => sound as i32,
        Some(Err(error)) => {
            tracing::error!(target: "script", "could not load the sound {reference}: {error:#}");
            -1
        }
        None => -1,
    }
}

pub(crate) unsafe extern "C" fn sound_info(sound: i32, out: *mut SoundFacts) -> i32 {
    if sound < 0 || out.is_null() {
        return 0;
    }
    match with_services(|services| services.sound_info(sound as u32)).flatten() {
        Some(facts) => {
            unsafe { out.write_unaligned(facts) };
            1
        }
        None => 0,
    }
}

pub(crate) unsafe extern "C" fn sound_play(sound: i32, play: *const PlaySound) -> u64 {
    if sound < 0 || play.is_null() {
        return 0;
    }
    let play = unsafe { play.read_unaligned() };
    with_services(|services| services.play_sound(sound as u32, &play)).unwrap_or(0)
}

pub(crate) unsafe extern "C" fn voice_value(voice: u64, what: i32, set: i32, value: f32) -> f32 {
    let Some(what) = VoiceValue::from_code(what) else { return f32::NAN };
    ask(AudioAsk::Voice(voice, what), set, value)
}

pub(crate) unsafe extern "C" fn source_value(entity: u64, what: i32, set: i32, value: f32) -> f32 {
    let Some(what) = SourceValue::from_code(what) else { return f32::NAN };
    ask(AudioAsk::Source(Entity::from_bits(entity), what), set, value)
}

pub(crate) unsafe extern "C" fn audio_bus(bus: i32, set: i32, value: f32) -> f32 {
    let Some(bus) = BusValue::from_code(bus) else { return f32::NAN };
    ask(AudioAsk::Bus(bus), set, value)
}

/// Asks the engine, with the world lent when scripts are running in one.
/// NaN when there is no such voice or Audio Source.
fn ask(ask: AudioAsk, set: i32, value: f32) -> f32 {
    let set = (set != 0).then_some(value);
    let answer = match ask {
        AudioAsk::Source(..) => with_world(|world| with_services(|services| services.audio(Some(world), ask, set))),
        _ => with_services(|services| services.audio(None, ask, set)),
    };
    answer.flatten().unwrap_or(f32::NAN)
}
