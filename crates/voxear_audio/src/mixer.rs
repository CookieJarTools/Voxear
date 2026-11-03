//! The mixer: every voice playing, summed into the output a block at a
//! time on the audio's own thread. It takes what to do from the game as
//! commands, and tells it which voices finished.
//!
//! Voices go through a bus, music or effects, and both through the master
//! volume. The editor's own sounds, such as the Inspector's play button, go
//! straight out, so the game's volumes and pause never touch them.
//!
//! A change of volume, pan or pause slides across one block rather than
//! jumping, and a stopped voice fades out over one, so nothing clicks. A
//! sound at another sample rate than the output, or played at another
//! pitch, is read between its samples.
//!
//! 3D voices on the effects bus are also sent to the reverb, which rings as
//! the listener's room does, once the game says how (see `reverb`).
//!
//! With headphones on, 3D voices go through Steam Audio's HRTF, heard from
//! where they are with the cues of a real head, above and behind included,
//! rather than panned. Its HRTF is made on a thread of its own when first
//! wanted, so voices pan as before until it is ready, or when Steam Audio
//! is not here.

use std::sync::{
    Arc,
    mpsc::{self, Receiver, SyncSender},
};

use voxear_core::math::Vec3;

use crate::{
    bands::Bands,
    ears::Ears,
    reverb::{Reverb, ReverbTimes},
    spatial::{self, Distance, Listener},
    steam_audio::{Hrtf, Muffle},
    stream::{Frame, StreamReader},
};

/// A voice: one sound playing, from its start until it ends or is stopped.
/// 0 is no voice.
pub type VoiceId = u64;

/// Voices playing at once. A voice past this takes the place of the oldest
/// effect that does not loop, or does not play.
pub const MAX_VOICES: usize = 256;

/// Where a voice's sound goes before the master volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Bus {
    Music,
    #[default]
    Effects,
    /// The editor's own sounds, outside the game's volumes and pause.
    Preview,
}

/// How a voice plays. Volume and pitch multiply: 1 is the sound as it is,
/// a pitch of 2 an octave up and twice as fast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceParams {
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
    pub bus: Bus,
    /// Where a 3D voice is and how it fades with distance; `None` for a 2D
    /// one, which plays as the sound is, the same wherever the listener is.
    pub spatial: Option<(Vec3, Distance)>,
    /// How fast a 3D voice moves, in metres a second, and how strongly the
    /// Doppler effect changes its pitch from that and the listener's: 1 as
    /// in the world, 0 not at all.
    pub velocity: Vec3,
    pub doppler: f32,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pitch: 1.0,
            looping: false,
            bus: Bus::Effects,
            spatial: None,
            velocity: Vec3::ZERO,
            doppler: 1.0,
        }
    }
}

impl VoiceParams {
    pub(crate) const MIN_PITCH: f32 = 0.05;
    pub(crate) const MAX_PITCH: f32 = 4.0;
}

pub(crate) enum Command {
    Play(Box<NewVoice>),
    Stop(VoiceId),
    Pause(VoiceId, bool),
    Volume(VoiceId, f32),
    Pitch(VoiceId, f32),
    Looping(VoiceId, bool),
    Position(VoiceId, Vec3),
    Distance(VoiceId, Distance),
    Velocity(VoiceId, Vec3),
    Doppler(VoiceId, f32),
    Muffle(VoiceId, Muffle),
    Reverb(ReverbTimes),
    Bus(Bus, f32),
    Master(f32),
    Listener(Listener),
    Headphones(bool),
    PauseGame(bool),
    StopGame,
}

/// A voice to start, made on the game's thread.
pub(crate) struct NewVoice {
    pub id: VoiceId,
    pub source: Source,
    /// The sound's own sample rate.
    pub rate: u32,
    /// The sound's own volume, from its import settings.
    pub gain: f32,
    pub params: VoiceParams,
}

pub(crate) enum Source {
    /// Interleaved samples, one or two channels.
    Samples {
        samples: Arc<[f32]>,
        channels: usize,
    },
    Stream(StreamReader),
}

struct Voice {
    id: VoiceId,
    source: Source,
    rate: u32,
    gain: f32,
    params: VoiceParams,
    /// Where it is in its sound, in frames, between them when resampled.
    position: f64,
    /// The left and right volumes it ended the last block with, or none
    /// before its first.
    gains: Option<[f32; 2]>,
    paused: bool,
    stopping: bool,
    /// Steam Audio's binaural effect, for a 3D voice with headphones on.
    ears: Option<Box<Ears>>,
    /// How much of a 3D voice gets past the walls, once any have been in
    /// its way.
    bands: Option<Box<Bands>>,
    /// Its sound is over, and its binaural effect plays out what it holds.
    done: bool,
}

/// What became of a voice this block.
enum Played {
    Going,
    Ended,
}

pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Voice>,
    master: f32,
    music: f32,
    effects: f32,
    listener: Listener,
    game_paused: bool,
    headphones: bool,
    /// The HRTF at the output's rate, once made, and while it is being.
    hrtf: Option<Arc<Hrtf>>,
    making: Option<Receiver<Result<Arc<Hrtf>, String>>>,
    commands: Receiver<Command>,
    finished: SyncSender<VoiceId>,
    /// The block being mixed, in stereo, and what of it goes to the reverb,
    /// in mono, once there is a room to ring.
    mix: Vec<f32>,
    sent: Vec<f32>,
    reverb: Option<Box<Reverb>>,
}

impl Mixer {
    pub(crate) fn new(sample_rate: u32, commands: Receiver<Command>, finished: SyncSender<VoiceId>) -> Self {
        Self {
            sample_rate,
            voices: Vec::with_capacity(64),
            master: 1.0,
            music: 1.0,
            effects: 1.0,
            listener: Listener::default(),
            game_paused: false,
            headphones: false,
            hrtf: None,
            making: None,
            commands,
            finished,
            mix: Vec::new(),
            sent: Vec::new(),
            reverb: None,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Sets the rate the output plays at, before the first block.
    pub(crate) fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.max(1);
        if self.hrtf.as_ref().is_some_and(|hrtf| hrtf.sample_rate != self.sample_rate) {
            self.hrtf = None;
            self.give_ears();
            self.make_hrtf();
        }
    }

    /// Whether 3D voices are heard through Steam Audio's HRTF now: wanted,
    /// and ready.
    pub fn hears_binaurally(&self) -> bool {
        self.headphones && self.hrtf.is_some()
    }

    /// How many voices are playing or fading out.
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Mixes the next block into `out`, which it fills: interleaved, with
    /// `channels` to a frame. Two channels get the stereo mix; one gets
    /// both sides together, and more get it in their first two.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        self.take_commands();
        self.take_hrtf();
        self.mix.clear();
        self.mix.resize(frames * 2, 0.0);
        self.sent.clear();
        self.sent.resize(frames, 0.0);
        let mut index = 0;
        while index < self.voices.len() {
            match self.play(index, frames) {
                Played::Going => index += 1,
                Played::Ended => {
                    let ended = self.voices.swap_remove(index);
                    let _ = self.finished.try_send(ended.id);
                }
            }
        }
        if let Some(reverb) = &mut self.reverb {
            for (mixed, &sent) in self.mix.as_chunks_mut::<2>().0.iter_mut().zip(&self.sent) {
                let [left, right] = reverb.apply(sent);
                mixed[0] += left;
                mixed[1] += right;
            }
        }
        for (frame, mixed) in out.chunks_exact_mut(channels).zip(self.mix.as_chunks::<2>().0) {
            match frame {
                [one] => *one = (mixed[0] + mixed[1]) * 0.5,
                [left, right, rest @ ..] => {
                    *left = mixed[0];
                    *right = mixed[1];
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
    }

    fn take_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Play(voice) => self.start(*voice),
                Command::Stop(id) => self.with(id, |voice| voice.stopping = true),
                Command::Pause(id, paused) => self.with(id, |voice| voice.paused = paused),
                Command::Volume(id, volume) => self.with(id, |voice| voice.params.volume = volume),
                Command::Pitch(id, pitch) => self.with(id, |voice| voice.params.pitch = pitch),
                Command::Looping(id, looping) => self.with(id, |voice| voice.params.looping = looping),
                Command::Position(id, at) => self.with(id, |voice| {
                    if let Some((position, _)) = &mut voice.params.spatial {
                        *position = at;
                    }
                }),
                Command::Distance(id, distance) => self.with(id, |voice| {
                    if let Some((_, fade)) = &mut voice.params.spatial {
                        *fade = distance;
                    }
                }),
                Command::Velocity(id, velocity) => self.with(id, |voice| voice.params.velocity = velocity),
                Command::Doppler(id, level) => self.with(id, |voice| voice.params.doppler = level.max(0.0)),
                Command::Muffle(id, muffle) => {
                    let rate = self.sample_rate;
                    // The first muffle slides in from the open, as any after slides from the last.
                    self.with(id, |voice| {
                        voice.bands.get_or_insert_with(|| Bands::new(rate, Muffle::NONE)).set(muffle)
                    });
                }
                Command::Reverb(times) => match &mut self.reverb {
                    Some(reverb) => reverb.set(times),
                    None if times != ReverbTimes::NONE => self.reverb = Some(Reverb::new(self.sample_rate, times)),
                    None => {}
                },
                Command::Bus(Bus::Music, volume) => self.music = volume,
                Command::Bus(Bus::Effects, volume) => self.effects = volume,
                Command::Bus(Bus::Preview, _) => {}
                Command::Master(volume) => self.master = volume,
                Command::Listener(listener) => self.listener = listener,
                Command::Headphones(on) => self.set_headphones(on),
                Command::PauseGame(paused) => self.game_paused = paused,
                Command::StopGame => {
                    for voice in self.voices.iter_mut().filter(|voice| voice.params.bus != Bus::Preview) {
                        voice.stopping = true;
                    }
                }
            }
        }
    }

    fn set_headphones(&mut self, on: bool) {
        self.headphones = on;
        self.give_ears();
        self.make_hrtf();
    }

    /// Starts making the HRTF for the output's rate, away from this thread,
    /// when headphones are on and there is none.
    fn make_hrtf(&mut self) {
        if !self.headphones || self.hrtf.is_some() || self.making.is_some() {
            return;
        }
        let (made, making) = mpsc::sync_channel(1);
        let rate = self.sample_rate;
        let started = std::thread::Builder::new().name("voxear-hrtf".into()).spawn(move || {
            let _ = made.send(Hrtf::new(rate));
        });
        if started.is_ok() {
            self.making = Some(making);
        }
    }

    /// Takes the HRTF once it is made, if it is still for the output's rate.
    fn take_hrtf(&mut self) {
        let Some(made) = self.making.as_ref().and_then(|making| making.try_recv().ok()) else { return };
        self.making = None;
        match made {
            Ok(hrtf) if hrtf.sample_rate == self.sample_rate => {
                self.hrtf = Some(hrtf);
                self.give_ears();
            }
            Ok(_) => self.make_hrtf(),
            Err(error) => {
                tracing::warn!(target: "audio", "3D sound pans without headphones' HRTF: {error}");
                self.headphones = false;
            }
        }
    }

    /// Gives every 3D voice its binaural effect while headphones are on and
    /// the HRTF is ready, and takes it away otherwise.
    fn give_ears(&mut self) {
        let hrtf = self.hrtf.clone().filter(|_| self.headphones);
        for voice in &mut self.voices {
            voice.ears = match (&hrtf, voice.params.spatial) {
                (Some(hrtf), Some(_)) if voice.ears.is_some() => continue,
                (Some(hrtf), Some(_)) => Ears::new(hrtf),
                _ => None,
            };
        }
    }

    fn with(&mut self, id: VoiceId, change: impl FnOnce(&mut Voice)) {
        if let Some(voice) = self.voices.iter_mut().find(|voice| voice.id == id) {
            change(voice);
        }
    }

    fn start(&mut self, new: NewVoice) {
        if self.voices.len() >= MAX_VOICES {
            let oldest = self.voices.iter().position(|v| !v.params.looping && v.params.bus == Bus::Effects);
            let Some(oldest) = oldest else {
                let _ = self.finished.try_send(new.id);
                return;
            };
            let taken = self.voices.remove(oldest);
            let _ = self.finished.try_send(taken.id);
        }
        let hrtf = self.hrtf.as_ref().filter(|_| self.headphones && new.params.spatial.is_some());
        self.voices.push(Voice {
            ears: hrtf.and_then(Ears::new),
            bands: None,
            done: false,
            id: new.id,
            source: new.source,
            rate: new.rate,
            gain: new.gain,
            params: new.params,
            position: 0.0,
            gains: None,
            paused: false,
            stopping: false,
        });
    }

    /// The left and right volumes a voice should have now: silent while
    /// stopping or paused, or while the game is paused for a game voice.
    fn target(&self, voice: &Voice) -> [f32; 2] {
        if voice.stopping || self.held(voice) {
            return [0.0; 2];
        }
        let bus = match voice.params.bus {
            Bus::Music => self.music * self.master,
            Bus::Effects => self.effects * self.master,
            Bus::Preview => 1.0,
        };
        let volume = voice.params.volume * voice.gain * bus;
        match &voice.params.spatial {
            // Heard binaurally, the HRTF places it, so its volume is only its distance's.
            Some((at, distance)) if voice.ears.is_some() => {
                [distance.falloff(self.listener.position.distance(*at)) * volume; 2]
            }
            Some((at, distance)) => spatial::gains(&self.listener, *at, distance).map(|side| side * volume),
            None => [volume; 2],
        }
    }

    /// Paused, or a game voice while the game is.
    fn held(&self, voice: &Voice) -> bool {
        voice.paused || (self.game_paused && voice.params.bus != Bus::Preview)
    }

    /// Mixes one voice's block in, sliding from its last volumes to the
    /// ones it should have now.
    fn play(&mut self, index: usize, frames: usize) -> Played {
        let target = self.target(&self.voices[index]);
        let held = self.held(&self.voices[index]);
        let (output_rate, listener) = (f64::from(self.sample_rate), self.listener);
        let voice = &mut self.voices[index];
        let from = voice.gains.unwrap_or(target);
        let silent = from == [0.0; 2] && target == [0.0; 2];
        if voice.stopping && (silent || voice.gains.is_none()) {
            voice.done = true;
        }
        if voice.done {
            return drain(voice, &mut self.mix.as_chunks_mut::<2>().0[..frames], &listener);
        }
        // Faded out while held: it keeps its place without reading its sound.
        // A voice only too far away to hear plays on.
        if silent && held {
            voice.gains = Some(target);
            return Played::Going;
        }
        let shift = match voice.params.spatial {
            Some((at, _)) => spatial::doppler(&listener, at, voice.params.velocity, voice.params.doppler),
            None => 1.0,
        };
        let pitch = (voice.params.pitch * shift).clamp(VoiceParams::MIN_PITCH, VoiceParams::MAX_PITCH);
        let step = f64::from(voice.rate) / output_rate * f64::from(pitch);
        let per_frame = [(target[0] - from[0]) / frames.max(1) as f32, (target[1] - from[1]) / frames.max(1) as f32];
        let mut gains = from;
        let mut ended = false;
        let mut played = 0;
        let mix = &mut self.mix.as_chunks_mut::<2>().0[..frames];
        for (out, sent) in mix.iter_mut().zip(&mut self.sent) {
            let Some(sound) = voice.read() else {
                ended = true;
                break;
            };
            gains = [gains[0] + per_frame[0], gains[1] + per_frame[1]];
            let [left, right] = voice.heard(sound, gains, &listener, sent);
            out[0] += left;
            out[1] += right;
            played += 1;
            if !voice.advance(step) {
                ended = true;
                break;
            }
        }
        voice.gains = Some(target);
        if !ended && !voice.stopping {
            return Played::Going;
        }
        voice.done = true;
        drain(voice, &mut mix[played..], &listener)
    }
}

/// Plays out what a finished voice's binaural effect still holds, into the
/// rest of the block, and ends it once that is out, or at once without one.
fn drain(voice: &mut Voice, mix: &mut [[f32; 2]], listener: &Listener) -> Played {
    let (Some(ears), Some((at, _))) = (voice.ears.as_mut(), voice.params.spatial) else { return Played::Ended };
    for out in mix {
        let Some([left, right]) = ears.drain(listener, at) else { return Played::Ended };
        out[0] += left;
        out[1] += right;
    }
    Played::Going
}

impl Voice {
    /// One frame of the voice's sound as the listener hears it, at its
    /// left and right volumes: as it is for a 2D voice, and in mono from
    /// where it is for a 3D one, panned or through its binaural effect. A
    /// 3D voice on the effects bus adds itself to what goes to the reverb.
    fn heard(&mut self, [left, right]: [f32; 2], gains: [f32; 2], listener: &Listener, sent: &mut f32) -> [f32; 2] {
        let Some((at, _)) = self.params.spatial else { return [left * gains[0], right * gains[1]] };
        let mut mono = (left + right) * 0.5;
        if let Some(bands) = &mut self.bands {
            mono = bands.apply(mono);
        }
        if self.params.bus == Bus::Effects {
            *sent += mono * (gains[0] + gains[1]) * 0.5;
        }
        match &mut self.ears {
            Some(ears) => ears.hear(mono * gains[0], listener, at),
            None => [mono * gains[0], mono * gains[1]],
        }
    }

    /// The sound at the voice's place, between its frames. `None` at its
    /// end; silence while a stream waits for its next block.
    fn read(&mut self) -> Option<[f32; 2]> {
        let index = self.position.floor();
        let between = (self.position - index) as f32;
        let index = index as u64;
        let (first, second) = match &mut self.source {
            Source::Samples { samples, channels } => {
                let frames = (samples.len() / *channels) as u64;
                if index >= frames {
                    return None;
                }
                let next = match index + 1 {
                    next if next < frames => next,
                    _ if self.params.looping => 0,
                    _ => index,
                };
                (frame(samples, *channels, index), frame(samples, *channels, next))
            }
            Source::Stream(reader) => match reader.frames(index) {
                (Frame::Here(first), second) => (first, second),
                (Frame::Waiting, _) => return Some([0.0; 2]),
                (Frame::End, _) => return None,
            },
        };
        Some([first[0] + (second[0] - first[0]) * between, first[1] + (second[1] - first[1]) * between])
    }

    /// Moves on by `step` frames. False when a sound that does not loop has
    /// played to its end; a stream knows its own end, and loops itself.
    fn advance(&mut self, step: f64) -> bool {
        self.position += step;
        let Source::Samples { samples, channels } = &self.source else { return true };
        let frames = (samples.len() / *channels) as f64;
        if self.position < frames {
            return true;
        }
        if self.params.looping && frames > 0.0 {
            self.position %= frames;
            return true;
        }
        false
    }
}

fn frame(samples: &[f32], channels: usize, index: u64) -> [f32; 2] {
    let at = index as usize * channels;
    match channels {
        1 => [samples[at]; 2],
        _ => [samples[at], samples[at + 1]],
    }
}
