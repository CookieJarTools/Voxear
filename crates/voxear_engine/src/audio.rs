//! Sound in a running game: sound assets loaded through [`Content`], the
//! Audio Source component that plays one from its Obj, and who hears it.
//!
//! An Audio Source keeps only its settings, which scenes save and the
//! Inspector edits. What it is playing lives in [`GameAudio`], which once a
//! frame, after the game's update, starts what should play (sources that
//! play on start, and those a script asked to), pushes changed settings and
//! positions to their voices, and notices voices that ended.
//!
//! The listener is the first Obj with an Audio Listener, or else the
//! camera the game looks through.
//!
//! ```toml
//! [[obj]]
//! id = 4
//! name = "Waterfall"
//! [obj.audio]
//! sound = "Sounds/Waterfall.ogg"   # a GUID once saved
//! loop = true
//! max_distance = 30.0
//! ```

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, SoundImport};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_render::Camera;
use voxear_scene::{Destroying, EditorOnly, Name, Transform, WorldTransform};
use voxear_scripting::{AudioAsk, BusValue, PlaySound, SoundFacts, SourceValue, VoiceValue};

use voxear_audio::steam_audio::{Muffle, Occlusion, SourceAcoustics, SteamAudio};

use crate::acoustics::Walls;
pub use voxear_audio::{
    Audio, Bus, Distance, Listener, ReverbTimes, Sound, SoundInfo, VoiceId, VoiceParams, VoiceReport, probe,
};

use crate::content::Content;

/// A sound asset, by the id scripts and Audio Sources hold. It stays the
/// same while the game runs, even after the sound is freed and loaded again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

/// Sounds by GUID, loaded when first used and freed when nothing uses them.
#[derive(Default)]
pub(crate) struct SoundCache {
    ids: HashMap<AssetGuid, SoundId>,
    guids: Vec<AssetGuid>,
    /// By id; `None` until loaded, and once freed.
    loaded: Vec<Option<Arc<Sound>>>,
    /// Played or loaded since the last sweep.
    used: HashSet<SoundId>,
}

impl SoundCache {
    pub(crate) fn id(&mut self, guid: AssetGuid) -> SoundId {
        *self.ids.entry(guid).or_insert_with(|| {
            self.guids.push(guid);
            self.loaded.push(None);
            SoundId(self.guids.len() as u32 - 1)
        })
    }

    pub(crate) fn guid(&self, id: SoundId) -> Option<AssetGuid> {
        self.guids.get(id.0 as usize).copied()
    }

    pub(crate) fn get(&mut self, id: SoundId) -> Option<Arc<Sound>> {
        let sound = self.loaded.get(id.0 as usize)?.clone()?;
        self.used.insert(id);
        Some(sound)
    }

    pub(crate) fn put(&mut self, id: SoundId, sound: Arc<Sound>) {
        self.loaded[id.0 as usize] = Some(sound);
        self.used.insert(id);
    }

    /// Keeps a sound loaded elsewhere, as by a background load, unless it
    /// is loaded already.
    pub(crate) fn adopt(&mut self, guid: AssetGuid, sound: Sound) {
        let id = self.id(guid);
        if self.loaded[id.0 as usize].is_none() {
            self.put(id, Arc::new(sound));
        }
    }

    /// The sounds loaded now, by GUID.
    pub(crate) fn loaded_guids(&self) -> impl Iterator<Item = AssetGuid> + '_ {
        self.guids.iter().zip(&self.loaded).filter(|(_, sound)| sound.is_some()).map(|(guid, _)| *guid)
    }

    /// Frees every sound not in `keep` and not used since the last sweep.
    /// Returns how many went.
    pub(crate) fn release(&mut self, keep: &HashSet<SoundId>) -> usize {
        let mut freed = 0;
        for (index, slot) in self.loaded.iter_mut().enumerate() {
            let id = SoundId(index as u32);
            if slot.is_some() && !keep.contains(&id) && !self.used.contains(&id) {
                *slot = None;
                freed += 1;
            }
        }
        self.used.clear();
        freed
    }

    pub(crate) fn release_one(&mut self, id: SoundId) {
        if let Some(slot) = self.loaded.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    pub(crate) fn loaded(&self) -> usize {
        self.loaded.iter().flatten().count()
    }
}

/// Reads a sound file as its import settings say: whole, or kept to stream.
pub(crate) fn load_sound(bytes: Vec<u8>, path: &str, settings: SoundImport) -> Result<Sound> {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
    let mut sound = Sound::open(bytes.into(), extension.as_deref(), |info| settings.streams(info.seconds()))
        .with_context(|| format!("reading {path}"))?;
    sound.volume = settings.volume.max(0.0);
    Ok(sound)
}

/// Whether a 3D source fades with distance and pans, or plays the same
/// wherever the listener is, as music and interface sounds do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundSpace {
    #[default]
    #[serde(rename = "3d")]
    ThreeD,
    #[serde(rename = "2d")]
    TwoD,
}

/// Whether walls between a 3D source and the listener stop its sound, as
/// Steam Audio works out from the level's colliders, and how it tells: by
/// one ray from the source's middle, or by rays across a sphere round it,
/// so a source half behind a corner is half heard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundOcclusion {
    #[default]
    Off,
    Raycast,
    Volumetric,
}

impl SoundOcclusion {
    pub const ALL: [Self; 3] = [Self::Off, Self::Raycast, Self::Volumetric];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Raycast => "Raycast",
            Self::Volumetric => "Volumetric",
        }
    }
}

/// The bus a source plays through, under the master volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioBus {
    #[default]
    Effects,
    Music,
}

impl AudioBus {
    pub fn bus(self) -> Bus {
        match self {
            Self::Effects => Bus::Effects,
            Self::Music => Bus::Music,
        }
    }
}

/// Plays a sound from its Obj, as Unity's Audio Source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSource {
    pub sound: Option<SoundId>,
    pub volume: f32,
    /// 1 plays the sound as it is; 2 an octave up and twice as fast.
    pub pitch: f32,
    pub looping: bool,
    /// Starts playing as its Obj comes into the running game.
    pub play_on_start: bool,
    pub space: SoundSpace,
    /// A 3D source is at full volume out to this many metres...
    pub min_distance: f32,
    /// ...and silent from this many.
    pub max_distance: f32,
    pub bus: AudioBus,
    /// How strongly a 3D source's pitch rises as it and the listener come
    /// together and falls as they part, as Unity's Doppler Level: 1 as in
    /// the world, 0 not at all.
    pub doppler: f32,
    /// Whether walls in a 3D source's way stop its sound; the radius of the
    /// sphere a volumetric one is heard from; and whether the walls let
    /// some of it through, as their acoustic materials do.
    pub occlusion: SoundOcclusion,
    pub occlusion_radius: f32,
    pub transmission: bool,
}
impl Component for AudioSource {}

impl Default for AudioSource {
    fn default() -> Self {
        Self::from(AudioSourceData::default())
    }
}

impl AudioSource {
    pub fn distance(&self) -> Distance {
        Distance { min: self.min_distance, max: self.max_distance }
    }

    /// How its voice plays, at `at` when it is 3D.
    fn params(&self, at: Vec3) -> VoiceParams {
        VoiceParams {
            volume: self.volume,
            pitch: self.pitch,
            looping: self.looping,
            bus: self.bus.bus(),
            spatial: (self.space == SoundSpace::ThreeD).then(|| (at, self.distance())),
            doppler: self.doppler,
            ..VoiceParams::default()
        }
    }

    /// How Steam Audio is to hear a 3D source past the walls; none for a 2D
    /// one, or with occlusion off.
    fn acoustics(&self) -> Option<SourceAcoustics> {
        let occlusion = match (self.space, self.occlusion) {
            (SoundSpace::TwoD, _) | (_, SoundOcclusion::Off) => return None,
            (_, SoundOcclusion::Raycast) => Occlusion::Raycast,
            (_, SoundOcclusion::Volumetric) => Occlusion::Volumetric,
        };
        Some(SourceAcoustics { occlusion, radius: self.occlusion_radius, transmission: self.transmission })
    }
}

/// Hears the game's 3D sound from its Obj, in place of the camera, as for a
/// third-person game that hears from the character rather than behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioListener;
impl Component for AudioListener {}

/// An Audio Source as scenes write it. Left out, a setting has its default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSourceData {
    /// The sound, by GUID or project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub volume: f32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub pitch: f32,
    #[serde(default, rename = "loop", skip_serializing_if = "is_false")]
    pub looping: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub play_on_start: bool,
    #[serde(default, rename = "mode", skip_serializing_if = "is_default")]
    pub space: SoundSpace,
    #[serde(default = "default_min", skip_serializing_if = "is_default_min")]
    pub min_distance: f32,
    #[serde(default = "default_max", skip_serializing_if = "is_default_max")]
    pub max_distance: f32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bus: AudioBus,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub doppler: f32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub occlusion: SoundOcclusion,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub occlusion_radius: f32,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub transmission: bool,
}

impl Default for AudioSourceData {
    fn default() -> Self {
        toml::from_str("").expect("every setting has a default")
    }
}

impl AudioSourceData {
    /// The settings that differ from `expected`, each written out even at
    /// its default, as a prefab instance's override merges them onto the
    /// prefab's.
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        let table = |data: &Self| {
            let mut table = toml::Table::new();
            let mut put = |key: &str, value: toml::Value| drop(table.insert(key.to_string(), value));
            if let Some(sound) = &data.sound {
                put("sound", sound.clone().into());
            }
            put("volume", f64::from(data.volume).into());
            put("pitch", f64::from(data.pitch).into());
            put("loop", data.looping.into());
            put("play_on_start", data.play_on_start.into());
            put("mode", toml::Value::try_from(data.space).expect("a name"));
            put("min_distance", f64::from(data.min_distance).into());
            put("max_distance", f64::from(data.max_distance).into());
            put("bus", toml::Value::try_from(data.bus).expect("a name"));
            put("doppler", f64::from(data.doppler).into());
            put("occlusion", toml::Value::try_from(data.occlusion).expect("a name"));
            put("occlusion_radius", f64::from(data.occlusion_radius).into());
            put("transmission", data.transmission.into());
            table
        };
        let mut changes = table(self);
        if let Some(expected) = expected.map(table) {
            changes.retain(|key, value| expected.get(key) != Some(value));
        }
        changes
    }
}

impl From<AudioSourceData> for AudioSource {
    /// The settings alone: the sound is loaded by [`audio_source`].
    fn from(data: AudioSourceData) -> Self {
        Self {
            sound: None,
            volume: data.volume.max(0.0),
            pitch: data.pitch.clamp(0.05, 4.0),
            looping: data.looping,
            play_on_start: data.play_on_start,
            space: data.space,
            min_distance: data.min_distance.max(0.0),
            max_distance: data.max_distance.max(data.min_distance),
            bus: data.bus,
            doppler: data.doppler.clamp(0.0, 5.0),
            occlusion: data.occlusion,
            occlusion_radius: data.occlusion_radius.max(0.01),
            transmission: data.transmission,
        }
    }
}

/// An Audio Source from a scene's data, with its sound loaded.
pub fn audio_source(content: &mut Content, data: &AudioSourceData) -> Result<AudioSource> {
    let sound = match &data.sound {
        Some(reference) => Some(content.sound(&content.parse(reference)?)?),
        None => None,
    };
    Ok(AudioSource { sound, ..AudioSource::from(data.clone()) })
}

/// What a scene writes for an Audio Source.
pub fn audio_source_data(content: &Content, source: &AudioSource) -> AudioSourceData {
    AudioSourceData {
        sound: source.sound.and_then(|id| content.sound_guid(id)).map(|guid| guid.to_string()),
        volume: source.volume,
        pitch: source.pitch,
        looping: source.looping,
        play_on_start: source.play_on_start,
        space: source.space,
        min_distance: source.min_distance,
        max_distance: source.max_distance,
        bus: source.bus,
        doppler: source.doppler,
        occlusion: source.occlusion,
        occlusion_radius: source.occlusion_radius,
        transmission: source.transmission,
    }
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn default_min() -> f32 {
    1.0
}

fn is_default_min(value: &f32) -> bool {
    *value == default_min()
}

fn default_max() -> f32 {
    50.0
}

fn is_default_max(value: &f32) -> bool {
    *value == default_max()
}

/// A sound played once from a script, by `Audio.Play`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OneShot {
    pub volume: f32,
    pub pitch: f32,
    pub bus: AudioBus,
    /// Where it plays in 3D, and how it fades; `None` plays it in 2D.
    pub at: Option<(Vec3, Distance)>,
}

impl Default for OneShot {
    fn default() -> Self {
        Self { volume: 1.0, pitch: 1.0, bus: AudioBus::Effects, at: None }
    }
}

/// What a source is doing, as scripts asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Playing,
    Paused,
    Stopped,
}

/// An Audio Source in the running game: its voice, if one plays, and what
/// it should be doing.
#[derive(Clone, Copy, Debug)]
struct Playback {
    voice: VoiceId,
    state: State,
    /// Play was asked for: start again from the beginning.
    restart: bool,
    /// The voice is held, as Pause asked.
    held: bool,
    /// The settings the voice last had.
    applied: AudioSource,
    /// Where its Obj was last frame, to tell how fast it moves.
    was: Option<Vec3>,
    /// The voice last muffled by the walls, and how much.
    muffled: (VoiceId, Muffle),
}

/// What a script asks of an Audio Source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRequest {
    /// From the start, or on from where it was paused.
    Play,
    Pause,
    Stop,
}

/// The game's sound: the mixer, and what each Audio Source plays.
pub struct GameAudio {
    pub audio: Audio,
    sources: HashMap<Entity, Playback>,
    /// The game ran last frame, rather than being edited.
    running: bool,
    /// Where the listener was last frame, to tell how fast it moves.
    listener_was: Option<Vec3>,
    /// The editor's preview of a sound asset, while it plays, and the
    /// sound's project path.
    preview: VoiceId,
    preview_path: String,
    /// Whether 3D voices start out heard as through headphones each time
    /// the game starts, from the project's `[audio]` settings.
    pub headphones: bool,
    /// Whether 3D voices on the effects bus ring as the listener's room
    /// does each time the game starts, from the project's `[audio]`
    /// settings, and whether they do now.
    pub reverb: bool,
    reverb_on: bool,
    /// How long the listener's room rings, as Steam Audio last said.
    room: ReverbTimes,
    /// The level's walls in Steam Audio, while the game runs.
    walls: Walls,
    /// The sound each one-shot voice plays, for the Audio panel.
    shots: HashMap<VoiceId, SoundId>,
}

/// A voice playing, as the editor's Audio panel lists it: what the mixer
/// says of it, and where it comes from, named.
#[derive(Clone, Debug, PartialEq)]
pub struct HeardVoice {
    pub report: VoiceReport,
    /// The Audio Source's Obj, for a source's voice.
    pub source: Option<Entity>,
    /// The Obj's name, or what else plays it: a one-shot, or the editor's
    /// preview of a sound.
    pub name: String,
    /// The sound's project path, when it is known.
    pub sound: String,
}

impl Default for GameAudio {
    fn default() -> Self {
        Self::new(Audio::new())
    }
}

impl GameAudio {
    pub fn new(audio: Audio) -> Self {
        Self {
            audio,
            sources: HashMap::new(),
            running: false,
            listener_was: None,
            preview: 0,
            preview_path: String::new(),
            headphones: false,
            reverb: false,
            reverb_on: false,
            room: ReverbTimes::NONE,
            walls: Walls::default(),
            shots: HashMap::new(),
        }
    }

    /// As the game starts, before its scripts do: 3D voices heard as the
    /// project's `[audio]` settings start them, whatever the last play left.
    pub fn start_game(&mut self) {
        self.audio.set_headphones(self.headphones);
        self.set_reverb(self.reverb);
    }

    /// Whether 3D voices on the effects bus ring as the listener's room
    /// does, as Steam Audio works out from the level's walls.
    pub fn set_reverb(&mut self, on: bool) {
        if self.reverb_on && !on {
            self.audio.set_reverb(ReverbTimes::NONE);
            self.room = ReverbTimes::NONE;
        }
        self.reverb_on = on;
    }

    pub fn reverb_on(&self) -> bool {
        self.reverb_on
    }

    /// Where the listener was last frame, while the game runs.
    pub fn listener_at(&self) -> Option<Vec3> {
        self.listener_was
    }

    /// How long the listener's room rings, in seconds in each band of
    /// pitch, as Steam Audio last worked it out; none with reverb off.
    pub fn room(&self) -> ReverbTimes {
        self.room
    }

    /// Once a frame, after the game's update: hears which voices ended, and
    /// unless the game is only being edited, brings every Audio Source's
    /// voice in line with it and moves the listener. `dt` is the frame's
    /// time, by which how far each moved since the last tells its speed.
    pub fn update(&mut self, world: &World, content: &mut Content, running: bool, dt: f32) {
        self.audio.update();
        if !running {
            if std::mem::take(&mut self.running) {
                self.stop_game();
            }
            self.listener_was = None;
            return;
        }
        self.running = true;
        let hearing = listener(world).map(|mut listener| {
            listener.velocity = velocity(self.listener_was, listener.position, dt);
            self.listener_was = Some(listener.position);
            self.audio.set_listener(listener);
            listener
        });
        let found: Vec<(Entity, AudioSource, Vec3)> = world
            .query_ref::<(Entity, &AudioSource, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, source, local, placed)| (entity, *source, position(local, placed)))
            .collect();
        let alive: HashSet<Entity> = found.iter().map(|(entity, ..)| *entity).collect();
        for (_, playback) in self.sources.iter().filter(|(entity, _)| !alive.contains(entity)) {
            self.audio.stop(playback.voice);
        }
        self.sources.retain(|entity, _| alive.contains(entity));
        for (entity, source, at) in found.iter().copied() {
            let playback = *self.sources.entry(entity).or_insert_with(|| Playback::new(&source, source.play_on_start));
            let mut playback = self.bring_in_line(content, playback, &source, at);
            if source.space == SoundSpace::ThreeD && playback.voice != 0 {
                self.audio.set_velocity(playback.voice, velocity(playback.was, at, dt));
            }
            playback.was = Some(at);
            self.sources.insert(entity, playback);
        }
        if let Some(listener) = hearing {
            self.muffle_sources(world, &listener, &found);
        }
        if let Some(times) = hearing.filter(|_| self.reverb_on).and_then(|at| self.walls.room(world, &at)) {
            self.audio.set_reverb(times);
            self.room = times;
        }
    }

    /// Muffles each playing 3D source with occlusion on by the walls between
    /// it and the listener, and lets one whose occlusion went off be heard
    /// in the open again.
    fn muffle_sources(&mut self, world: &World, listener: &Listener, found: &[(Entity, AudioSource, Vec3)]) {
        let mut muffles: Vec<(Entity, Muffle)> = Vec::new();
        let mut voices: Vec<(u64, Vec3, SourceAcoustics)> = Vec::new();
        for (entity, source, at) in found {
            let Some(playback) = self.sources.get(entity).filter(|playback| playback.voice != 0) else { continue };
            match source.acoustics() {
                Some(acoustics) => voices.push((entity.to_bits(), *at, acoustics)),
                None if playback.muffled.0 == playback.voice => muffles.push((*entity, Muffle::NONE)),
                None => {}
            }
        }
        if let Some(heard) = (!voices.is_empty()).then(|| self.walls.hear(world, listener, &voices)).flatten() {
            let keyed = voices.iter().zip(heard).map(|((key, ..), muffle)| (Entity::from_bits(*key), muffle));
            muffles.extend(keyed);
        }
        for (entity, muffle) in muffles {
            let Some(playback) = self.sources.get_mut(&entity) else { continue };
            if playback.muffled != (playback.voice, muffle) {
                self.audio.set_muffle(playback.voice, muffle);
                playback.muffled = (playback.voice, muffle);
            }
        }
    }

    fn bring_in_line(
        &mut self,
        content: &mut Content,
        mut playback: Playback,
        source: &AudioSource,
        at: Vec3,
    ) -> Playback {
        let audio = &mut self.audio;
        match playback.state {
            State::Playing if playback.voice != 0 && !audio.exists(playback.voice) && !playback.restart => {
                // It played to its end.
                playback.voice = 0;
                playback.state = State::Stopped;
            }
            State::Playing if playback.restart || playback.voice == 0 => {
                audio.stop(playback.voice);
                playback.voice = source
                    .sound
                    .and_then(|id| sound(content, id))
                    .map_or(0, |sound| audio.play(&sound, source.params(at)));
                playback.restart = false;
                playback.held = false;
                playback.applied = *source;
                if playback.voice == 0 {
                    playback.state = State::Stopped;
                }
            }
            State::Playing => {
                if playback.held {
                    audio.set_paused(playback.voice, false);
                    playback.held = false;
                }
                apply(audio, playback.voice, &playback.applied, source);
                playback.applied = *source;
                if source.space == SoundSpace::ThreeD {
                    audio.set_position(playback.voice, at);
                }
            }
            State::Paused if !playback.held && playback.voice != 0 => {
                audio.set_paused(playback.voice, true);
                playback.held = true;
            }
            State::Paused => {}
            State::Stopped => {
                audio.stop(playback.voice);
                playback.voice = 0;
            }
        }
        playback
    }

    /// Asks an Audio Source to play, pause or stop. It happens at the end of
    /// the frame, when its voice is brought in line.
    pub fn request(&mut self, world: &World, entity: Entity, request: SourceRequest) -> bool {
        let Some(source) = world.get::<AudioSource>(entity) else { return false };
        let playback = self.sources.entry(entity).or_insert_with(|| Playback::new(source, false));
        match request {
            SourceRequest::Play if playback.state == State::Paused => playback.state = State::Playing,
            SourceRequest::Play => {
                playback.state = State::Playing;
                playback.restart = true;
            }
            SourceRequest::Pause if playback.state == State::Playing => playback.state = State::Paused,
            SourceRequest::Pause => {}
            SourceRequest::Stop => playback.state = State::Stopped,
        }
        true
    }

    /// Whether an Audio Source is playing, or about to: one that plays on
    /// start counts from the moment its Obj is in the game.
    pub fn is_source_playing(&self, world: &World, entity: Entity) -> bool {
        match self.sources.get(&entity) {
            Some(playback) => playback.state == State::Playing,
            None => world.get::<AudioSource>(entity).is_some_and(|source| source.play_on_start),
        }
    }

    /// Plays a sound once, in 2D or at a place, returning its voice.
    pub fn play(&mut self, content: &mut Content, sound_id: SoundId, shot: OneShot) -> Result<VoiceId> {
        let loaded = content.loaded_sound(sound_id)?;
        let params = VoiceParams {
            volume: shot.volume,
            pitch: shot.pitch,
            looping: false,
            bus: shot.bus.bus(),
            spatial: shot.at,
            ..VoiceParams::default()
        };
        let voice = self.audio.play(&loaded, params);
        self.shots.retain(|voice, _| self.audio.exists(*voice));
        self.shots.insert(voice, sound_id);
        Ok(voice)
    }

    /// Every voice the mixer is playing, named, while the Audio panel
    /// watches (see `Audio::watch_voices`): sources' first, by name.
    pub fn heard_voices(&self, world: &World, content: &Content) -> Vec<HeardVoice> {
        let path = |id: Option<SoundId>| {
            let guid = id.and_then(|id| content.sound_guid(id));
            guid.and_then(|guid| content.db().get(guid)).map_or_else(String::new, |entry| entry.path.clone())
        };
        let mut heard: Vec<HeardVoice> = self
            .audio
            .voice_reports()
            .into_iter()
            .map(|report| {
                let found = self.sources.iter().find(|(_, playback)| playback.voice == report.id);
                let (source, name, sound) = match found {
                    Some((&entity, playback)) => {
                        let name = world.get::<Name>(entity).map_or_else(|| "Obj".to_string(), |name| name.0.clone());
                        (Some(entity), name, path(playback.applied.sound))
                    }
                    None if report.id == self.preview => (None, "Preview".to_string(), self.preview_path.clone()),
                    None => (None, "One-shot".to_string(), path(self.shots.get(&report.id).copied())),
                };
                HeardVoice { report, source, name, sound }
            })
            .collect();
        heard.sort_by_key(|voice| (voice.source.is_none(), voice.name.clone(), voice.report.id));
        heard
    }

    /// Stops every game voice and forgets every source, as when play stops,
    /// and puts the volumes scripts set back to full for the next run.
    pub fn stop_game(&mut self) {
        self.audio.stop_game();
        self.sources.clear();
        self.set_reverb(false);
        self.walls = Walls::default();
        self.audio.set_master_volume(1.0);
        self.audio.set_bus_volume(Bus::Effects, 1.0);
        self.audio.set_bus_volume(Bus::Music, 1.0);
    }

    /// Answers what a script reads, or sets when `set` says, of the sound:
    /// a voice it played, an Audio Source in `world`, or a volume. `None`
    /// where there is no such voice or source.
    pub fn answer(
        &mut self,
        content: &mut Content,
        world: Option<&mut World>,
        ask: AudioAsk,
        set: Option<f32>,
    ) -> Option<f32> {
        match ask {
            AudioAsk::Voice(voice, what) => self.voice_value(voice, what, set),
            AudioAsk::Bus(bus) => Some(self.bus_value(bus, set)),
            AudioAsk::Source(entity, what) => self.source_value(world?, content, entity, what, set),
        }
    }

    fn voice_value(&mut self, voice: VoiceId, what: VoiceValue, set: Option<f32>) -> Option<f32> {
        let audio = &mut self.audio;
        if !audio.exists(voice) {
            // A voice that ended, or never started, plays no more.
            return matches!(what, VoiceValue::Playing | VoiceValue::Paused | VoiceValue::Stop).then_some(0.0);
        }
        match (what, set) {
            (VoiceValue::Playing, _) => return Some(flag(audio.is_playing(voice))),
            (VoiceValue::Paused, Some(paused)) => audio.set_paused(voice, paused != 0.0),
            (VoiceValue::Paused, None) => {}
            (VoiceValue::Stop, _) => audio.stop(voice),
            (VoiceValue::Volume, Some(volume)) => audio.set_volume(voice, volume),
            (VoiceValue::Volume, None) => return audio.volume(voice),
            (VoiceValue::Pitch, Some(pitch)) => audio.set_pitch(voice, pitch),
            (VoiceValue::Pitch, None) => return audio.pitch(voice),
        }
        Some(match what {
            VoiceValue::Paused => flag(audio.exists(voice) && !audio.is_playing(voice)),
            VoiceValue::Volume => audio.volume(voice).unwrap_or(0.0),
            VoiceValue::Pitch => audio.pitch(voice).unwrap_or(1.0),
            _ => 1.0,
        })
    }

    fn bus_value(&mut self, bus: BusValue, set: Option<f32>) -> f32 {
        match (bus, set) {
            (BusValue::Reverb, Some(on)) => self.set_reverb(on != 0.0),
            (BusValue::Reverb, None) => {}
            _ => return self.volume_value(bus, set),
        }
        flag(self.reverb_on && SteamAudio::get().is_ok())
    }

    fn volume_value(&mut self, bus: BusValue, set: Option<f32>) -> f32 {
        let audio = &mut self.audio;
        match (bus, set) {
            (BusValue::Master, Some(volume)) => audio.set_master_volume(volume),
            (BusValue::Effects, Some(volume)) => audio.set_bus_volume(Bus::Effects, volume),
            (BusValue::Music, Some(volume)) => audio.set_bus_volume(Bus::Music, volume),
            (BusValue::Headphones, Some(on)) => audio.set_headphones(on != 0.0),
            (BusValue::SteamAudio | BusValue::Reverb, Some(_)) | (_, None) => {}
        }
        match bus {
            BusValue::Master => audio.master_volume(),
            BusValue::Effects => audio.bus_volume(Bus::Effects),
            BusValue::Music => audio.bus_volume(Bus::Music),
            BusValue::Headphones => flag(audio.headphones() && SteamAudio::get().is_ok()),
            BusValue::SteamAudio => flag(SteamAudio::get().is_ok()),
            BusValue::Reverb => flag(self.reverb_on),
        }
    }

    fn source_value(
        &mut self,
        world: &mut World,
        content: &Content,
        entity: Entity,
        what: SourceValue,
        set: Option<f32>,
    ) -> Option<f32> {
        match what {
            SourceValue::Exists => return Some(flag(world.has::<AudioSource>(entity))),
            SourceValue::Add => {
                if !world.has::<AudioSource>(entity) && world.is_alive(entity) {
                    world.insert(entity, AudioSource::default());
                }
                return Some(flag(world.has::<AudioSource>(entity)));
            }
            SourceValue::Playing => {
                world.get::<AudioSource>(entity)?;
                return Some(flag(self.is_source_playing(world, entity)));
            }
            SourceValue::Play | SourceValue::Pause | SourceValue::Stop => {
                let request = match what {
                    SourceValue::Play => SourceRequest::Play,
                    SourceValue::Pause => SourceRequest::Pause,
                    _ => SourceRequest::Stop,
                };
                let done =
                    if set.is_some() { self.request(world, entity, request) } else { world.has::<AudioSource>(entity) };
                return done.then_some(1.0);
            }
            _ => {}
        }
        let source = world.get_mut::<AudioSource>(entity)?;
        if let Some(value) = set {
            match what {
                SourceValue::Sound => {
                    let id = (value >= 0.0).then_some(SoundId(value as u32));
                    source.sound = id.filter(|&id| content.sound_guid(id).is_some());
                }
                SourceValue::Volume => source.volume = value.max(0.0),
                SourceValue::Pitch => source.pitch = value.clamp(0.05, 4.0),
                SourceValue::Loop => source.looping = value != 0.0,
                SourceValue::Spatial => {
                    source.space = if value != 0.0 { SoundSpace::ThreeD } else { SoundSpace::TwoD };
                }
                SourceValue::MinDistance => source.min_distance = value.clamp(0.0, source.max_distance),
                SourceValue::MaxDistance => source.max_distance = value.max(source.min_distance),
                SourceValue::Bus => source.bus = if value != 0.0 { AudioBus::Music } else { AudioBus::Effects },
                SourceValue::PlayOnStart => source.play_on_start = value != 0.0,
                SourceValue::Doppler => source.doppler = value.clamp(0.0, 5.0),
                SourceValue::Occlusion => {
                    source.occlusion = SoundOcclusion::ALL.get(value as usize).copied().unwrap_or_default();
                }
                SourceValue::OcclusionRadius => source.occlusion_radius = value.max(0.01),
                SourceValue::Transmission => source.transmission = value != 0.0,
                _ => {}
            }
        }
        Some(match what {
            SourceValue::Sound => source.sound.map_or(-1.0, |id| id.0 as f32),
            SourceValue::Volume => source.volume,
            SourceValue::Pitch => source.pitch,
            SourceValue::Loop => flag(source.looping),
            SourceValue::Spatial => flag(source.space == SoundSpace::ThreeD),
            SourceValue::MinDistance => source.min_distance,
            SourceValue::MaxDistance => source.max_distance,
            SourceValue::Bus => flag(source.bus == AudioBus::Music),
            SourceValue::PlayOnStart => flag(source.play_on_start),
            SourceValue::Doppler => source.doppler,
            SourceValue::Occlusion => {
                SoundOcclusion::ALL.iter().position(|o| *o == source.occlusion).unwrap_or(0) as f32
            }
            SourceValue::OcclusionRadius => source.occlusion_radius,
            SourceValue::Transmission => flag(source.transmission),
            _ => 0.0,
        })
    }

    /// Plays a sound once for a script's `Audio.Play`, returning its voice,
    /// or 0 when it would not play.
    pub fn play_for_script(&mut self, content: &mut Content, sound: u32, play: &PlaySound) -> VoiceId {
        let shot = OneShot {
            volume: play.volume,
            pitch: play.pitch,
            bus: if play.bus == 1 { AudioBus::Music } else { AudioBus::Effects },
            at: (play.spatial != 0)
                .then(|| (Vec3::from(play.position), Distance { min: play.min_distance, max: play.max_distance })),
        };
        match self.play(content, SoundId(sound), shot) {
            Ok(voice) => voice,
            Err(error) => {
                tracing::error!(target: "script", "{error:#}");
                0
            }
        }
    }

    /// Holds the game's sound, as the editor's Pause does, or lets it go on.
    pub fn pause_game(&mut self, paused: bool) {
        self.audio.pause_game(paused);
    }

    /// Plays a sound asset in the editor, outside the game's volumes, in
    /// place of any it was playing. Returns its voice.
    pub fn preview(&mut self, content: &mut Content, path: &str) -> Result<VoiceId> {
        self.stop_preview();
        let reference = content.parse(path)?;
        let id = content.sound(&reference)?;
        let loaded = content.loaded_sound(id)?;
        self.preview = self.audio.play(&loaded, VoiceParams { bus: Bus::Preview, ..VoiceParams::default() });
        self.preview_path = path.to_string();
        Ok(self.preview)
    }

    pub fn stop_preview(&mut self) {
        self.audio.stop(self.preview);
        self.preview = 0;
    }

    /// The project path of the sound the editor is playing, while it plays.
    pub fn previewing(&self) -> Option<&str> {
        (self.preview != 0 && self.audio.is_playing(self.preview)).then_some(self.preview_path.as_str())
    }
}

impl Playback {
    fn new(source: &AudioSource, playing: bool) -> Self {
        let state = if playing { State::Playing } else { State::Stopped };
        Self { voice: 0, state, restart: false, held: false, applied: *source, was: None, muffled: (0, Muffle::NONE) }
    }
}

fn flag(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

/// What a sound holds, as scripts read it.
pub(crate) fn sound_facts(sound: &Sound) -> SoundFacts {
    let info = sound.info();
    SoundFacts {
        seconds: info.seconds() as f32,
        sample_rate: info.sample_rate as i32,
        channels: i32::from(info.channels),
        streamed: i32::from(sound.is_streamed()),
    }
}

/// A loaded sound, logging why when it will not load.
fn sound(content: &mut Content, id: SoundId) -> Option<Arc<Sound>> {
    content.loaded_sound(id).map_err(|error| tracing::error!(target: "audio", "{error:#}")).ok()
}

/// Pushes what changed in a source's settings to its voice. A change of
/// sound or of 2D and 3D takes effect when it next plays.
fn apply(audio: &mut Audio, voice: VoiceId, was: &AudioSource, now: &AudioSource) {
    if was == now {
        return;
    }
    if was.volume != now.volume {
        audio.set_volume(voice, now.volume);
    }
    if was.pitch != now.pitch {
        audio.set_pitch(voice, now.pitch);
    }
    if was.looping != now.looping {
        audio.set_looping(voice, now.looping);
    }
    if was.distance() != now.distance() {
        audio.set_distance(voice, now.distance());
    }
    if was.doppler != now.doppler {
        audio.set_doppler(voice, now.doppler);
    }
}

/// How fast something moved, in metres a second, from where it was last
/// frame to where it is: still on its first frame.
fn velocity(was: Option<Vec3>, now: Vec3, dt: f32) -> Vec3 {
    match was {
        Some(was) if dt > 0.0 => (now - was) / dt,
        _ => Vec3::ZERO,
    }
}

fn position(local: &Transform, placed: Option<&WorldTransform>) -> Vec3 {
    placed.map_or(local.position, |placed| placed.0.w_axis.truncate())
}

/// Who hears the game: the first Obj with an Audio Listener, or the first
/// camera, as the game looks through. Never one of the editor's own.
pub fn listener(world: &World) -> Option<Listener> {
    let first = |found: Vec<(Entity, Listener)>| found.into_iter().min_by_key(|(entity, _)| entity.index());
    let placed = |local: &Transform, placed: Option<&WorldTransform>| match placed {
        Some(placed) => {
            let (_, rotation, position) = placed.0.to_scale_rotation_translation();
            Listener { position, rotation, ..Listener::default() }
        }
        None => Listener { position: local.position, rotation: local.rotation, ..Listener::default() },
    };
    let listeners = world
        .query_ref::<(Entity, &AudioListener, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _, local, world)| (entity, placed(local, world)))
        .collect();
    if let Some((_, listener)) = first(listeners) {
        return Some(listener);
    }
    let cameras = world
        .query_ref::<(Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _, local, world)| (entity, placed(local, world)))
        .collect();
    first(cameras).map(|(_, listener)| listener)
}
