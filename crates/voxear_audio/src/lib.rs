//! Voxear's sound: sound files decoded or streamed, a mixer on its own
//! thread with music and effects buses under a master volume, and voices
//! in 2D or placed in 3D around a listener.
//!
//! [`Audio`] is the game's side: it starts voices and changes them, and
//! the mixer applies each change on its next block, a few milliseconds
//! later. The output device opens on the first sound, so a game or test
//! that plays none never touches it.
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use voxear_audio::{Audio, Sound, VoiceParams};
//! let mut audio = Audio::new();
//! let sound = Arc::new(Sound::decode(&std::fs::read("Boom.wav")?, Some("wav"))?);
//! let voice = audio.play(&sound, VoiceParams::default());
//! audio.set_volume(voice, 0.5);
//! # anyhow::Ok(())
//! ```
//!
//! With [`Audio::set_headphones`], 3D voices are heard through Steam
//! Audio's HRTF (see [`steam_audio`]); spatial acoustics, occlusion,
//! reflections and pathing, come from it too. See the Audio section of
//! `docs/ARCHITECTURE.md`.

mod bands;
mod ears;
mod mixer;
mod output;
mod report;
mod reverb;
mod sound;
mod spatial;
pub mod steam_audio;
mod stream;

use std::{
    collections::HashMap,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

pub use mixer::{Bus, MAX_VOICES, Mixer, VoiceId, VoiceParams};
pub use report::VoiceReport;
pub use reverb::ReverbTimes;
pub use sound::{Sound, SoundInfo, probe};
pub use spatial::{Distance, Listener, SPEED_OF_SOUND, doppler, gains, pan};
use voxear_core::math::Vec3;

use crate::{
    mixer::{Command, NewVoice, Source},
    output::Output,
    report::VoiceBoard,
    stream::Streamer,
};

/// Finished voices the mixer can tell of before the game takes them.
const FINISHED: usize = 4096;

/// Where the mix goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The default device, opened on the first sound.
    Device,
    /// A quiet thread at a device's pace, as without one.
    Quiet,
    /// Nowhere: whoever made it renders the mixer by hand, as tests do.
    Offline,
}

/// What the game knows of a voice it started.
#[derive(Clone, Copy, Debug)]
struct Known {
    bus: Bus,
    paused: bool,
    volume: f32,
    pitch: f32,
}

/// The game's side of the sound: what it plays, and the volumes.
pub struct Audio {
    commands: Sender<Command>,
    finished: Receiver<VoiceId>,
    mode: Mode,
    /// The mixer, until the output starts and takes it.
    mixer: Option<Mixer>,
    output: Option<Output>,
    streamer: Option<Streamer>,
    next: VoiceId,
    voices: HashMap<VoiceId, Known>,
    master: f32,
    music: f32,
    effects: f32,
    game_paused: bool,
    headphones: bool,
    board: Arc<VoiceBoard>,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    /// Sound through the default output device, opened on the first sound;
    /// in silence when there is none, or when `VOXEAR_AUDIO` is `off`.
    pub fn new() -> Self {
        let off = std::env::var("VOXEAR_AUDIO").is_ok_and(|value| value.eq_ignore_ascii_case("off"));
        Self::with_mode(if off { Mode::Quiet } else { Mode::Device }, 48_000)
    }

    /// Sound that never reaches a device: voices play out and end in time,
    /// silently, as for a headless game.
    pub fn quiet() -> Self {
        Self::with_mode(Mode::Quiet, 48_000)
    }

    /// Sound mixed only when asked: the mixer comes back to render by hand,
    /// at `sample_rate`, as tests do.
    pub fn offline(sample_rate: u32) -> (Self, Mixer) {
        let mut audio = Self::with_mode(Mode::Offline, sample_rate);
        let mixer = audio.mixer.take().expect("made with one");
        (audio, mixer)
    }

    fn with_mode(mode: Mode, sample_rate: u32) -> Self {
        let (commands, received) = mpsc::channel();
        let (finish, finished) = mpsc::sync_channel(FINISHED);
        let board = Arc::new(VoiceBoard::default());
        Self {
            commands,
            finished,
            mode,
            mixer: Some(Mixer::new(sample_rate, received, finish, board.clone())),
            output: None,
            streamer: None,
            next: 1,
            voices: HashMap::new(),
            master: 1.0,
            music: 1.0,
            effects: 1.0,
            game_paused: false,
            headphones: false,
            board,
        }
    }

    /// The output device's name, once a sound has opened it. It changes
    /// when sound moves to another device, as when headphones are unplugged.
    pub fn device(&self) -> Option<String> {
        self.output.as_ref().and_then(Output::device)
    }

    /// Starts `sound` playing, returning its voice. A streamed sound whose
    /// file will not decode gives 0, no voice, with a warning.
    pub fn play(&mut self, sound: &Arc<Sound>, params: VoiceParams) -> VoiceId {
        self.start_output();
        let source = match sound.samples() {
            Some(samples) => Source::Samples { samples: samples.clone(), channels: sound.info().playing_channels() },
            None => {
                let streamer = self.streamer.get_or_insert_with(Streamer::start);
                match streamer.open(sound, params.looping) {
                    Some(reader) => Source::Stream(reader),
                    None => return 0,
                }
            }
        };
        let id = self.next;
        self.next += 1;
        let params = VoiceParams {
            volume: params.volume.max(0.0),
            pitch: params.pitch.clamp(VoiceParams::MIN_PITCH, VoiceParams::MAX_PITCH),
            ..params
        };
        let voice = NewVoice { id, source, rate: sound.info().sample_rate, gain: sound.volume, params };
        self.send(Command::Play(Box::new(voice)));
        self.voices.insert(id, Known { bus: params.bus, paused: false, volume: params.volume, pitch: params.pitch });
        id
    }

    /// Stops a voice, fading it out over a few milliseconds.
    pub fn stop(&mut self, voice: VoiceId) {
        if self.voices.remove(&voice).is_some() {
            self.send(Command::Stop(voice));
        }
    }

    /// Holds a voice where it is, or lets it go on.
    pub fn set_paused(&mut self, voice: VoiceId, paused: bool) {
        if let Some(known) = self.voices.get_mut(&voice) {
            known.paused = paused;
            self.send(Command::Pause(voice, paused));
        }
    }

    pub fn set_volume(&mut self, voice: VoiceId, volume: f32) {
        let volume = volume.max(0.0);
        if let Some(known) = self.voices.get_mut(&voice) {
            known.volume = volume;
        }
        self.change(voice, Command::Volume(voice, volume));
    }

    pub fn set_pitch(&mut self, voice: VoiceId, pitch: f32) {
        let pitch = pitch.clamp(VoiceParams::MIN_PITCH, VoiceParams::MAX_PITCH);
        if let Some(known) = self.voices.get_mut(&voice) {
            known.pitch = pitch;
        }
        self.change(voice, Command::Pitch(voice, pitch));
    }

    /// A voice's volume and pitch, as last set, while it is there.
    pub fn volume(&self, voice: VoiceId) -> Option<f32> {
        self.voices.get(&voice).map(|known| known.volume)
    }

    pub fn pitch(&self, voice: VoiceId) -> Option<f32> {
        self.voices.get(&voice).map(|known| known.pitch)
    }

    /// Whether a voice starts again at its end. A streamed one keeps what
    /// it started with.
    pub fn set_looping(&mut self, voice: VoiceId, looping: bool) {
        self.change(voice, Command::Looping(voice, looping));
    }

    /// Moves a 3D voice. A 2D one stays as it is.
    pub fn set_position(&mut self, voice: VoiceId, at: Vec3) {
        self.change(voice, Command::Position(voice, at));
    }

    pub fn set_distance(&mut self, voice: VoiceId, distance: Distance) {
        self.change(voice, Command::Distance(voice, distance));
    }

    /// How fast a 3D voice moves, in metres a second, for the Doppler effect.
    pub fn set_velocity(&mut self, voice: VoiceId, velocity: Vec3) {
        self.change(voice, Command::Velocity(voice, velocity));
    }

    /// How much of a 3D voice gets past the walls to the listener, in each
    /// band of pitch, as [`steam_audio::Acoustics`] works out.
    pub fn set_muffle(&mut self, voice: VoiceId, muffle: steam_audio::Muffle) {
        self.change(voice, Command::Muffle(voice, muffle));
    }

    /// How long the listener's room rings, for the reverb the 3D voices on
    /// the effects bus are sent to, as [`steam_audio::RoomReverb`] works it
    /// out; [`ReverbTimes::NONE`] for none.
    pub fn set_reverb(&mut self, times: ReverbTimes) {
        self.send(Command::Reverb(times));
    }

    /// How strongly the Doppler effect changes a 3D voice's pitch: 1 as in
    /// the world, 0 not at all.
    pub fn set_doppler(&mut self, voice: VoiceId, level: f32) {
        self.change(voice, Command::Doppler(voice, level));
    }

    /// Whether a voice is playing: started, not paused, and not yet at its
    /// end or stopped. A voice that ended counts as playing until the next
    /// [`update`](Self::update) hears of it.
    pub fn is_playing(&self, voice: VoiceId) -> bool {
        self.voices.get(&voice).is_some_and(|known| !known.paused)
    }

    /// Whether a voice is still there, playing or paused.
    pub fn exists(&self, voice: VoiceId) -> bool {
        self.voices.contains_key(&voice)
    }

    /// Hears which voices finished, so they stop counting as playing. The
    /// game calls this once a frame.
    pub fn update(&mut self) {
        while let Ok(voice) = self.finished.try_recv() {
            self.voices.remove(&voice);
        }
    }

    /// The volume of the music or effects bus, from 0. The editor's own
    /// sounds have none.
    pub fn bus_volume(&self, bus: Bus) -> f32 {
        match bus {
            Bus::Music => self.music,
            Bus::Effects => self.effects,
            Bus::Preview => 1.0,
        }
    }

    pub fn set_bus_volume(&mut self, bus: Bus, volume: f32) {
        let volume = volume.max(0.0);
        match bus {
            Bus::Music => self.music = volume,
            Bus::Effects => self.effects = volume,
            Bus::Preview => return,
        }
        self.send(Command::Bus(bus, volume));
    }

    /// The volume over both buses.
    pub fn master_volume(&self) -> f32 {
        self.master
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master = volume.max(0.0);
        self.send(Command::Master(self.master));
    }

    /// Who hears the 3D voices. Sent only while sound plays, so a game that
    /// plays none sends nothing.
    pub fn set_listener(&mut self, listener: Listener) {
        if self.output.is_some() || self.mode == Mode::Offline {
            self.send(Command::Listener(listener));
        }
    }

    /// Whether 3D voices are heard as through headphones: from where they
    /// are around the listener's head, above and behind included, through
    /// Steam Audio's HRTF, rather than panned between two speakers. Off by
    /// default, as it suits headphones and not speakers. The HRTF is made
    /// on the first sound after, so voices pan for a moment until it is
    /// ready, and on without Steam Audio, which logs why.
    pub fn set_headphones(&mut self, on: bool) {
        if self.headphones != on {
            self.headphones = on;
            self.send(Command::Headphones(on));
        }
    }

    pub fn headphones(&self) -> bool {
        self.headphones
    }

    /// Has the mixer say what it plays, for [`voice_reports`](Self::voice_reports),
    /// while `on`; it does nothing for it otherwise.
    pub fn watch_voices(&self, on: bool) {
        self.board.watch(on);
    }

    /// Every voice the mixer is playing, as it was at the end of its last
    /// block, while watched; empty otherwise.
    pub fn voice_reports(&self) -> Vec<VoiceReport> {
        if self.board.watched() { self.board.read() } else { Vec::new() }
    }

    /// Holds every game voice where it is, as the editor's Pause does, or
    /// lets them go on. The editor's own sounds play on.
    pub fn pause_game(&mut self, paused: bool) {
        if self.game_paused != paused {
            self.game_paused = paused;
            self.send(Command::PauseGame(paused));
        }
    }

    pub fn is_game_paused(&self) -> bool {
        self.game_paused
    }

    /// Stops every game voice, as when play stops, keeping the volumes.
    /// The editor's own sounds play on.
    pub fn stop_game(&mut self) {
        self.voices.retain(|_, known| known.bus == Bus::Preview);
        self.send(Command::StopGame);
        self.pause_game(false);
    }

    /// How many voices the game knows are playing or paused.
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    fn change(&mut self, voice: VoiceId, command: Command) {
        if self.voices.contains_key(&voice) {
            self.send(command);
        }
    }

    fn send(&mut self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Opens the output, with the mixer, on the first sound.
    fn start_output(&mut self) {
        if self.output.is_some() || self.mode == Mode::Offline {
            return;
        }
        let Some(mixer) = self.mixer.take() else { return };
        self.output = Some(output::start(mixer, self.mode == Mode::Quiet));
    }
}
