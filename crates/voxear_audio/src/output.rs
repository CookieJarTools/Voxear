//! Where the mix goes: the system's default output device, through cpal,
//! on a thread that owns the device's stream. Without a device, or with
//! `VOXEAR_AUDIO=off`, the mix runs on a quiet thread at the same pace and
//! goes nowhere, so voices still play out and end as they would.

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::JoinHandle,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use cpal::{
    BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::mixer::Mixer;

/// The rate the quiet thread mixes at.
const QUIET_RATE: u32 = 48_000;
/// The frames each block asks for, about ten milliseconds at 48 kHz: short
/// enough that a sound starts with its frame, long enough not to break up.
const BLOCK_FRAMES: u32 = 480;

/// The mix running, until this is dropped.
pub(crate) struct Output {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
    /// The device's name, or `None` when the mix goes nowhere.
    pub device: Option<String>,
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the mix: through the default device unless `quiet`, or when it
/// will not open.
pub(crate) fn start(mixer: Mixer, quiet: bool) -> Output {
    let mixer = Arc::new(Mutex::new(mixer));
    let (stop, stopped) = mpsc::channel();
    let (opened, told) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("Audio Output".into())
        .spawn(move || {
            let stream = if quiet { Err(anyhow::anyhow!("sound is off (VOXEAR_AUDIO=off)")) } else { open(&mixer) };
            match stream {
                Ok((stream, name)) => {
                    let _ = opened.send(Some(name));
                    // The stream plays until this thread is told to stop.
                    let _ = stopped.recv();
                    drop(stream);
                }
                Err(error) => {
                    if !quiet {
                        tracing::warn!(target: "audio", "no sound: {error:#}; voices play on in silence");
                    }
                    let _ = opened.send(None);
                    run_quietly(&mixer, &stopped);
                }
            }
        })
        .expect("the audio output thread starts");
    let device = told.recv().ok().flatten();
    if let Some(name) = &device {
        tracing::info!(target: "audio", "sound plays through {name}");
    }
    Output { stop, thread: Some(thread), device }
}

/// Opens the default output device and starts its stream, returning it and
/// the device's name.
fn open(mixer: &Arc<Mutex<Mixer>>) -> Result<(cpal::Stream, String)> {
    let host = cpal::default_host();
    let device = host.default_output_device().context("there is no output device")?;
    let name = device.description().map_or_else(|_| "the default output".to_string(), |d| d.to_string());
    let supported = device.default_output_config().context("the output device has no format to play")?;
    let mut config: StreamConfig = supported.config();
    if let Ok(mut mixer) = mixer.lock() {
        mixer.set_sample_rate(config.sample_rate);
    }
    if let SupportedBufferSize::Range { min, max } = supported.buffer_size() {
        config.buffer_size = BufferSize::Fixed(BLOCK_FRAMES.clamp(*min, *max));
    }
    let format = supported.sample_format();
    let stream = match build(&device, config, format, mixer) {
        Ok(stream) => stream,
        // Some devices take only their own block size.
        Err(_) => build(&device, StreamConfig { buffer_size: BufferSize::Default, ..config }, format, mixer)?,
    };
    stream.play().context("the output device would not start")?;
    Ok((stream, name))
}

fn build(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    mixer: &Arc<Mutex<Mixer>>,
) -> Result<cpal::Stream> {
    match format {
        SampleFormat::F32 => stream::<f32>(device, config, mixer),
        SampleFormat::I16 => stream::<i16>(device, config, mixer),
        SampleFormat::U16 => stream::<u16>(device, config, mixer),
        SampleFormat::I32 => stream::<i32>(device, config, mixer),
        SampleFormat::F64 => stream::<f64>(device, config, mixer),
        other => bail!("the output device wants {other} samples, which Voxear does not write"),
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: StreamConfig,
    mixer: &Arc<Mutex<Mixer>>,
) -> Result<cpal::Stream> {
    let channels = usize::from(config.channels);
    let mixer = mixer.clone();
    let mut mixed: Vec<f32> = Vec::new();
    let stream = device.build_output_stream(
        config,
        move |out: &mut [T], _| {
            mixed.resize(out.len(), 0.0);
            // Only this thread locks the mixer while the stream plays.
            match mixer.try_lock() {
                Ok(mut mixer) => mixer.render(&mut mixed, channels),
                Err(_) => mixed.fill(0.0),
            }
            for (sample, &value) in out.iter_mut().zip(&mixed) {
                *sample = T::from_sample(value.clamp(-1.0, 1.0));
            }
        },
        |error| tracing::warn!(target: "audio", "the sound output: {error}"),
        None,
    )?;
    Ok(stream)
}

/// Mixes into nothing at the pace a device would, until told to stop.
fn run_quietly(mixer: &Arc<Mutex<Mixer>>, stopped: &Receiver<()>) {
    if let Ok(mut mixer) = mixer.lock() {
        mixer.set_sample_rate(QUIET_RATE);
    }
    let mut block = vec![0.0; BLOCK_FRAMES as usize * 2];
    let pace = Duration::from_secs_f64(f64::from(BLOCK_FRAMES) / f64::from(QUIET_RATE));
    loop {
        if let Ok(mut mixer) = mixer.lock() {
            mixer.render(&mut block, 2);
        }
        match stopped.recv_timeout(pace) {
            Err(RecvTimeoutError::Timeout) => {}
            _ => return,
        }
    }
}
