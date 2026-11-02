//! Sounds: a file's samples decoded into memory, or its bytes kept to be
//! decoded as it plays, which long music wants.
//!
//! Every sound plays as mono or stereo. A file with more channels is mixed
//! down to stereo as it decodes: its centre goes to both sides, and its
//! surrounds to their side, quieter.

use std::{io::Cursor, sync::Arc};

use anyhow::{Context, Result, bail};
use symphonia::core::{
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    errors::Error as DecodeError,
    formats::{FormatOptions, FormatReader, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
};

/// What a sound file holds: its channels, how many samples a second each
/// has, and how long it is in frames (one sample of every channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: u64,
}

impl SoundInfo {
    /// How long the sound plays at its own speed.
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The channels it plays with: the file's, or two for a file with more.
    pub fn playing_channels(&self) -> usize {
        usize::from(self.channels.clamp(1, 2))
    }
}

/// A sound, ready to play any number of times at once.
pub struct Sound {
    info: SoundInfo,
    data: SoundData,
    /// Multiplies every voice of it, as its import settings say.
    pub volume: f32,
}

enum SoundData {
    /// Interleaved samples, mono or stereo.
    Decoded(Arc<[f32]>),
    /// The file, decoded as it plays; the extension helps tell its format.
    Encoded { bytes: Arc<[u8]>, extension: Option<String> },
}

impl Sound {
    /// Decodes a whole file into memory, so it starts at once and costs no
    /// work while it plays. `extension` (`"wav"`, `"ogg"`, ...) helps tell
    /// the format; the file's own header decides.
    pub fn decode(bytes: &[u8], extension: Option<&str>) -> Result<Self> {
        Self::open(Arc::from(bytes), extension, |_| false)
    }

    /// Keeps the file's bytes, decoding them a little ahead of where it
    /// plays: a tenth of the memory for long music, at the cost of some
    /// work on the audio's own thread while it plays.
    pub fn streamed(bytes: Arc<[u8]>, extension: Option<&str>) -> Result<Self> {
        Self::open(bytes, extension, |_| true)
    }

    /// Reads a file, decoding it whole unless `streams` says, from what the
    /// file holds, to keep it and decode it as it plays.
    pub fn open(bytes: Arc<[u8]>, extension: Option<&str>, streams: impl FnOnce(&SoundInfo) -> bool) -> Result<Self> {
        let (mut decoder, mut info) = Decoder::open(bytes.clone(), extension)?;
        if streams(&info) {
            let extension = extension.map(str::to_string);
            return Ok(Self { info, data: SoundData::Encoded { bytes, extension }, volume: 1.0 });
        }
        let mut samples = Vec::with_capacity(info.frames as usize * info.playing_channels());
        while decoder.next(&mut samples)? {}
        // A file whose header did not say how long it is, as some MP3s do.
        info.frames = (samples.len() / info.playing_channels()) as u64;
        Ok(Self { info, data: SoundData::Decoded(samples.into()), volume: 1.0 })
    }

    /// A sound made from samples, interleaved when there are two channels:
    /// generated tones, and tests.
    pub fn from_samples(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Self {
        let channels = channels.clamp(1, 2);
        let frames = (samples.len() / usize::from(channels)) as u64;
        Self {
            info: SoundInfo { channels, sample_rate, frames },
            data: SoundData::Decoded(samples.into()),
            volume: 1.0,
        }
    }

    pub fn info(&self) -> SoundInfo {
        self.info
    }

    pub fn is_streamed(&self) -> bool {
        matches!(self.data, SoundData::Encoded { .. })
    }

    /// The decoded samples, for a sound loaded whole.
    pub fn samples(&self) -> Option<&Arc<[f32]>> {
        match &self.data {
            SoundData::Decoded(samples) => Some(samples),
            SoundData::Encoded { .. } => None,
        }
    }

    /// A decoder from the start of a streamed sound's file.
    pub(crate) fn open_stream(&self) -> Option<Result<Decoder>> {
        let SoundData::Encoded { bytes, extension } = &self.data else { return None };
        Some(Decoder::open(bytes.clone(), extension.as_deref()).map(|(decoder, _)| decoder))
    }

    /// Memory its samples or bytes take.
    pub fn bytes(&self) -> usize {
        match &self.data {
            SoundData::Decoded(samples) => samples.len() * size_of::<f32>(),
            SoundData::Encoded { bytes, .. } => bytes.len(),
        }
    }
}

/// What a sound file holds, read from its header without decoding it, for
/// the Inspector. A file whose header does not say how long it is, as some
/// MP3s do, is decoded to count.
pub fn probe(bytes: &[u8], extension: Option<&str>) -> Result<SoundInfo> {
    let (mut decoder, info) = Decoder::open(Arc::from(bytes), extension)?;
    if info.frames > 0 {
        return Ok(info);
    }
    let (mut samples, mut frames) = (Vec::new(), 0);
    while decoder.next(&mut samples)? {
        frames += (samples.len() / info.playing_channels()) as u64;
        samples.clear();
    }
    Ok(SoundInfo { frames, ..info })
}

/// Reads a sound file packet by packet into mono or stereo samples.
pub(crate) struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    channels: usize,
    /// The packet just decoded, interleaved as the file has it.
    packet: Vec<f32>,
}

impl Decoder {
    pub(crate) fn open(bytes: Arc<[u8]>, extension: Option<&str>) -> Result<(Self, SoundInfo)> {
        let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(extension) = extension {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe()
            .probe(&hint, source, FormatOptions::default(), MetadataOptions::default())
            .context("not a sound file Voxear reads (WAV, OGG Vorbis, FLAC or MP3)")?;
        let track = format.default_track(TrackType::Audio).context("the file has no sound in it")?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).context("the file's sound has no format")?;
        let channels = params.channels.as_ref().map_or(0, |c| c.count());
        let sample_rate = params.sample_rate.unwrap_or(0);
        if channels == 0 || sample_rate == 0 {
            bail!("the file does not say how many channels it has, or its sample rate");
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .context("the file's sound is in a format Voxear cannot decode")?;
        let info = SoundInfo {
            channels: u16::try_from(channels).unwrap_or(u16::MAX),
            sample_rate,
            frames: track.num_frames.unwrap_or(0),
        };
        let track = track.id;
        Ok((Self { format, decoder, track, channels, packet: Vec::new() }, info))
    }

    /// Adds the next packet's frames to `out`, as mono or stereo. False at
    /// the end of the file. A damaged packet is skipped, as players do.
    pub(crate) fn next(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(false),
                Err(DecodeError::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false);
                }
                Err(error) => return Err(error).context("reading the sound file"),
            };
            if packet.track_id != self.track {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => decoded.copy_to_vec_interleaved(&mut self.packet),
                Err(DecodeError::DecodeError(_) | DecodeError::IoError(_)) => continue,
                Err(error) => return Err(error).context("decoding the sound file"),
            }
            mix_down(&self.packet, self.channels, out);
            return Ok(true);
        }
    }
}

/// Adds interleaved samples of `channels` to `out` as mono or stereo: two
/// channels or fewer as they are, more mixed down to stereo in the usual
/// order (front left and right, centre, low frequencies, then surrounds).
fn mix_down(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 2 {
        out.extend_from_slice(samples);
        return;
    }
    const SIDE: f32 = std::f32::consts::FRAC_1_SQRT_2;
    for frame in samples.chunks_exact(channels) {
        let (mut left, mut right) = (frame[0], frame[1]);
        for (channel, &sample) in frame.iter().enumerate().skip(2) {
            match channel {
                2 => {
                    left += sample * SIDE;
                    right += sample * SIDE;
                }
                // The low-frequency channel is for speakers that have one.
                3 => {}
                _ if channel % 2 == 0 => left += sample * SIDE,
                _ => right += sample * SIDE,
            }
        }
        out.extend_from_slice(&[left, right]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surround_mixes_down_to_stereo() {
        // Front left, front right, centre, low frequencies, back left, back right.
        let frame = [1.0, 0.0, 1.0, 5.0, 0.0, 1.0];
        let mut out = Vec::new();
        mix_down(&frame, 6, &mut out);
        let side = std::f32::consts::FRAC_1_SQRT_2;
        assert!((out[0] - (1.0 + side)).abs() < 1e-6, "{out:?}");
        assert!((out[1] - 2.0 * side).abs() < 1e-6, "the low frequencies stay out: {out:?}");
    }
}
