//! The listener's room heard by convolution: Steam Audio's own echoes of
//! it, worked out on the room's thread (see [`RoomReverb`](super::RoomReverb))
//! as an impulse response, which every sound sent to the reverb is played
//! through. Where the parametric reverb only knows how long the room rings,
//! this hears each wall's echo at its own time and from its own side: a
//! corridor flutters, a hall's far wall answers late, and a room open to
//! the sky on one side sounds open there.
//!
//! The echoes come as first-order Ambisonics, four channels round the
//! listener in the world, and are turned to stereo for speakers, or heard
//! binaurally through the HRTF with headphones on, as the listener faces
//! at the time.
//!
//! Steam Audio keeps a source's latest response for whoever reads it, and
//! what a reader is given stays as it is until that reader reads again, so
//! the room's thread can work out the next response while the audio's
//! thread plays the last. Only the audio's thread reads it, here, once a
//! block. The source and both effects are held by references of their own,
//! so the room's thread letting go of its simulator never takes away a
//! response that is playing.

use std::{collections::VecDeque, sync::Arc};

use super::{
    AudioBuffer, AudioSettings, FRAME, Handle, Hrtf, IplVector3,
    acoustics::{
        Acoustics, CONVOLUTION, CoordinateSpace, REFLECTIONS, ROOM_ORDER, ROOM_SECONDS, SimulationOutputs,
        listener_space, zeroed,
    },
};
use crate::spatial::Listener;

/// The channels of first-order Ambisonics.
const CHANNELS: usize = 4;
/// Steam Audio's stereo speakers, left and right.
const STEREO: i32 = 1;
/// How far each sample moves the volume when fading out: over about 20
/// milliseconds at 48 kHz, so stopping does not click.
const FADE: f32 = 1.0 / 1024.0;

#[repr(C)]
pub(super) struct ReflectionEffectSettings {
    kind: i32,
    ir_size: i32,
    channels: i32,
}

#[repr(C)]
struct SpeakerLayout {
    kind: i32,
    count: i32,
    speakers: *mut IplVector3,
}

#[repr(C)]
pub(super) struct AmbisonicsDecodeSettings {
    speakers: SpeakerLayout,
    hrtf: Handle,
    max_order: i32,
}

#[repr(C)]
pub(super) struct AmbisonicsDecodeParams {
    order: i32,
    hrtf: Handle,
    orientation: CoordinateSpace,
    binaural: i32,
}

/// The room's echoes, played on the audio's thread: what is sent to the
/// reverb goes in a block of [`FRAME`] samples at a time, and what comes
/// back plays one block behind, about 5 milliseconds at 48 kHz.
pub struct Convolution {
    hrtf: Arc<Hrtf>,
    source: Handle,
    reflection: Handle,
    decode: Handle,
    sample_rate: u32,
    ir_size: i32,
    /// The block being gathered, then its echoes in Ambisonics, then turned
    /// to the left and the right.
    input: Box<[f32; FRAME]>,
    filled: usize,
    ambisonics: Box<[[f32; FRAME]; CHANNELS]>,
    stereo: Box<[[f32; FRAME]; 2]>,
    /// What came back, still to be played. It starts a block of silence
    /// long, so that it never runs dry before the next block.
    output: VecDeque<[f32; 2]>,
    /// How loud it plays: 1, until it is told to fade out.
    level: f32,
    fading: bool,
}

// Its effects and its reading of the source are used by one thread at a time, the audio's.
unsafe impl Send for Convolution {}

impl Drop for Convolution {
    fn drop(&mut self) {
        let api = &self.hrtf.steam.api;
        unsafe {
            if !self.decode.is_null() {
                (api.decode_release)(&mut self.decode);
            }
            if !self.reflection.is_null() {
                (api.reflection_release)(&mut self.reflection);
            }
            if !self.source.is_null() {
                (api.source_release)(&mut self.source);
            }
        }
    }
}

impl Convolution {
    /// The echoes of the room `acoustics` works out, made
    /// [`for_echoes`](Acoustics::for_echoes) at `sample_rate`, to play at
    /// that rate. It makes an HRTF of its own, which takes a moment, so
    /// it is made off the audio's thread and handed to it.
    pub fn new(acoustics: &mut Acoustics, sample_rate: u32) -> Result<Box<Self>, String> {
        let hrtf = Hrtf::new(sample_rate)?;
        let ir_size = (ROOM_SECONDS * sample_rate as f32).ceil() as i32;
        let mut output = VecDeque::with_capacity(FRAME * 2);
        output.resize(FRAME, [0.0; 2]);
        let mut made = Box::new(Self {
            hrtf,
            source: std::ptr::null_mut(),
            reflection: std::ptr::null_mut(),
            decode: std::ptr::null_mut(),
            sample_rate,
            ir_size,
            input: Box::new([0.0; FRAME]),
            filled: 0,
            ambisonics: Box::new([[0.0; FRAME]; CHANNELS]),
            stereo: Box::new([[0.0; FRAME]; 2]),
            output,
            level: 1.0,
            fading: false,
        });
        let (api, context) = (&made.hrtf.steam.api, made.hrtf.steam.context);
        let mut audio = AudioSettings { sampling_rate: sample_rate as i32, frame_size: FRAME as i32 };
        let mut settings = ReflectionEffectSettings { kind: CONVOLUTION, ir_size, channels: CHANNELS as i32 };
        let mut reflection: Handle = std::ptr::null_mut();
        if unsafe { (api.reflection_create)(context, &mut audio, &mut settings, &mut reflection) } != 0 {
            return Err("Steam Audio made no convolution reverb".into());
        }
        made.reflection = reflection;
        let speakers = SpeakerLayout { kind: STEREO, count: 0, speakers: std::ptr::null_mut() };
        let mut settings = AmbisonicsDecodeSettings { speakers, hrtf: made.hrtf.handle, max_order: ROOM_ORDER };
        let mut decode: Handle = std::ptr::null_mut();
        if unsafe { (api.decode_create)(context, &mut audio, &mut settings, &mut decode) } != 0 {
            return Err("Steam Audio made no Ambisonics decoder".into());
        }
        made.decode = decode;
        made.source = acoustics.room_source(&Listener::default());
        Ok(made)
    }

    /// The rate it plays at, which must be the mixer's.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Takes the next sample sent to the reverb, and gives the left and
    /// right of the room's echoes to play now, as heard by `listener`,
    /// binaurally or for speakers.
    pub fn apply(&mut self, sent: f32, listener: &Listener, binaural: bool) -> [f32; 2] {
        self.input[self.filled] = if self.fading { 0.0 } else { sent };
        self.filled += 1;
        if self.filled == FRAME {
            self.block(listener, binaural);
            self.filled = 0;
        }
        if self.fading {
            self.level = (self.level - FADE).max(0.0);
        }
        let [left, right] = self.output.pop_front().unwrap_or_default();
        [left * self.level, right * self.level]
    }

    /// Fades it out, taking nothing more in, as when the reverb goes off.
    pub fn fade_out(&mut self) {
        self.fading = true;
    }

    /// Whether it has faded out altogether, and can go.
    pub fn faded(&self) -> bool {
        self.fading && self.level == 0.0
    }

    /// Plays the gathered block through the room's latest echoes.
    fn block(&mut self, listener: &Listener, binaural: bool) {
        let api = &self.hrtf.steam.api;
        let mut outputs: SimulationOutputs = zeroed();
        unsafe { (api.source_get_outputs)(self.source, REFLECTIONS, &mut outputs) };
        let mut params = outputs.reflections;
        if params.ir.is_null() {
            self.output.extend(std::iter::repeat_n([0.0; 2], FRAME));
            return;
        }
        params.kind = CONVOLUTION;
        params.channels = CHANNELS as i32;
        params.ir_size = self.ir_size;
        let mut input_channels = [self.input.as_mut_ptr()];
        let mut ambisonics_channels = self.ambisonics.each_mut().map(|channel| channel.as_mut_ptr());
        let mut stereo_channels = self.stereo.each_mut().map(|channel| channel.as_mut_ptr());
        let samples = FRAME as i32;
        let mut input = AudioBuffer { channels: 1, samples, data: input_channels.as_mut_ptr() };
        let mut ambisonics = AudioBuffer { channels: CHANNELS as i32, samples, data: ambisonics_channels.as_mut_ptr() };
        let mut stereo = AudioBuffer { channels: 2, samples, data: stereo_channels.as_mut_ptr() };
        let mut decode = AmbisonicsDecodeParams {
            order: ROOM_ORDER,
            hrtf: self.hrtf.handle,
            orientation: listener_space(listener),
            binaural: i32::from(binaural),
        };
        unsafe {
            (api.reflection_apply)(self.reflection, &mut params, &mut input, &mut ambisonics, std::ptr::null_mut());
            (api.decode_apply)(self.decode, &mut decode, &mut ambisonics, &mut stereo);
        }
        let [left, right] = &*self.stereo;
        self.output.extend(left.iter().zip(right).map(|(&l, &r)| [l, r]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam_audio::{AcousticMaterial, Wall, room::tests::room};

    const RATE: u32 = 48_000;

    /// The echoes of a room of these walls, worked out once, from where a
    /// listener at the origin stands.
    fn echoes(walls: &[Wall]) -> Box<Convolution> {
        let mut acoustics = Acoustics::for_echoes(RATE).expect("a simulator");
        acoustics.set_geometry(&walls.iter().map(Wall::mesh).collect::<Vec<_>>());
        let convolution = Convolution::new(&mut acoustics, RATE).expect("a convolution reverb");
        acoustics.echo(&Listener::default());
        convolution
    }

    /// What a click sent to the room gives back over `seconds`, in stereo.
    fn after_a_click(convolution: &mut Convolution, seconds: f32, binaural: bool) -> Vec<[f32; 2]> {
        let listener = Listener::default();
        (0..(seconds * RATE as f32) as usize)
            .map(|at| convolution.apply(if at == 0 { 1.0 } else { 0.0 }, &listener, binaural))
            .collect()
    }

    fn energy(samples: &[[f32; 2]]) -> f32 {
        samples.iter().map(|[left, right]| left * left + right * right).sum()
    }

    #[test]
    fn a_closed_room_echoes_a_click_long_after_it_and_the_open_does_not() {
        if crate::steam_audio::SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let hall = after_a_click(&mut echoes(&[room(20.0, AcousticMaterial::CONCRETE)]), 1.0, false);
        let open = after_a_click(&mut echoes(&[]), 1.0, false);
        let late = RATE as usize / 4;
        assert!(energy(&hall[late..]) > 1e-6, "a concrete hall still echoes a quarter of a second on");
        assert!(energy(&open) < energy(&hall) * 1e-3, "the open {} against the hall {}", energy(&open), energy(&hall));
    }

    #[test]
    fn the_echoes_are_not_the_dry_sound() {
        if crate::steam_audio::SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let mut convolution = echoes(&[room(8.0, AcousticMaterial::BRICK)]);
        let heard = after_a_click(&mut convolution, 0.5, false);
        // A click dry is one sample; through the room it spreads over many, each wall's echo at its own time.
        let loudest = heard.iter().fold(0.0f32, |most, [left, right]| most.max(left.abs()).max(right.abs()));
        let ringing = heard.iter().filter(|[left, right]| left.abs().max(right.abs()) > loudest * 0.01).count();
        assert!(loudest > 0.0 && ringing > 1_000, "{ringing} samples of echoes, the loudest {loudest}");
        assert!(heard.iter().any(|[left, right]| (left - right).abs() > loudest * 0.01), "the sides differ");

        let binaural = after_a_click(&mut convolution, 0.5, true);
        assert!(energy(&binaural) > 0.0, "and through the HRTF");
    }

    #[test]
    fn fading_out_ends_in_silence() {
        if crate::steam_audio::SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let mut convolution = echoes(&[room(20.0, AcousticMaterial::CONCRETE)]);
        after_a_click(&mut convolution, 0.1, false);
        convolution.fade_out();
        let after = after_a_click(&mut convolution, 0.1, false);
        assert!(convolution.faded());
        assert_eq!(energy(&after[2_048..]), 0.0, "silent once faded");
    }
}
