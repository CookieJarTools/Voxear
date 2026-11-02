//! Where the mix goes: the system's default output device, through cpal,
//! on a thread that owns the device's stream. When that device goes away,
//! or another becomes the default (headphones unplugged or plugged in), the
//! thread opens the default device again and the mix carries on there.
//! Without a device, or with `VOXEAR_AUDIO=off`, the mix runs on a quiet
//! thread at the same pace and goes nowhere, so voices still play out and
//! end as they would; a device that turns up later takes the mix over.

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use cpal::{
    BufferSize, ErrorKind, FromSample, SampleFormat, SizedSample, StreamConfig, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::mixer::Mixer;

/// The rate the quiet thread mixes at.
const QUIET_RATE: u32 = 48_000;
/// The frames each block asks for, about ten milliseconds at 48 kHz: short
/// enough that a sound starts with its frame, long enough not to break up.
const BLOCK_FRAMES: u32 = 480;
/// How often the thread looks for a new default device.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// After a device would not open, the longest wait before trying it again,
/// in checks. The wait doubles from one check up to this.
const MOST_CHECKS_BETWEEN_TRIES: u32 = 8;

/// The mix running, until this is dropped.
pub(crate) struct Output {
    events: Sender<Event>,
    thread: Option<JoinHandle<()>>,
    /// The device's name, or `None` while the mix goes nowhere.
    device: Arc<Mutex<Option<String>>>,
}

impl Output {
    pub(crate) fn device(&self) -> Option<String> {
        self.device.lock().ok()?.clone()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the output thread hears.
enum Event {
    Stop,
    /// The stream of that opening failed, so the device opens again.
    Reopen(u64),
    /// The stream of that opening moved to the new default device itself.
    Rerouted(u64),
}

/// Tells the output thread what became of a stream it opened.
#[derive(Clone)]
struct Report {
    events: Sender<Event>,
    opening: u64,
}

impl Report {
    fn reopen(&self) {
        let _ = self.events.send(Event::Reopen(self.opening));
    }

    fn rerouted(&self) {
        let _ = self.events.send(Event::Rerouted(self.opening));
    }
}

/// Output devices: the system's, or made-up ones in tests.
trait Devices {
    type Stream;

    /// What tells the default device from others, when there is one.
    fn default_id(&self) -> Option<String>;

    fn default_name(&self) -> Option<String>;

    /// Opens the default device with a stream playing `mixer`, returning it
    /// with the device's id and name. `report` hears when it fails.
    fn open(&self, mixer: &Arc<Mutex<Mixer>>, report: Report) -> Result<(Self::Stream, String, String)>;
}

/// Starts the mix: through the default device unless `quiet`, or when it
/// will not open.
pub(crate) fn start(mixer: Mixer, quiet: bool) -> Output {
    let output = start_with(mixer, move || (!quiet).then(|| System(cpal::default_host())), CHECK_EVERY);
    if let Some(name) = output.device() {
        tracing::info!(target: "audio", "sound plays through {name}");
    }
    output
}

/// Starts the mix on a thread with the devices `devices` makes there, or
/// quietly without any, looking for a new default device every `check`.
/// Returns once the first device has opened, or would not.
fn start_with<D: Devices + 'static>(
    mixer: Mixer,
    devices: impl FnOnce() -> Option<D> + Send + 'static,
    check: Duration,
) -> Output {
    let mixer = Arc::new(Mutex::new(mixer));
    let device = Arc::new(Mutex::new(None));
    let (events, heard) = mpsc::channel();
    let (opened, told) = mpsc::channel();
    let (named, sender) = (device.clone(), events.clone());
    let thread = std::thread::Builder::new()
        .name("Audio Output".into())
        .spawn(move || {
            let Some(devices) = devices() else {
                let _ = opened.send(());
                run_quietly(&mixer, &heard, || false);
                return;
            };
            let output = Thread { devices, mixer, events: heard, sender, device: named, check };
            output.run(opened);
        })
        .expect("the audio output thread starts");
    let _ = told.recv();
    Output { events, thread: Some(thread), device }
}

/// The output thread's side.
struct Thread<D> {
    devices: D,
    mixer: Arc<Mutex<Mixer>>,
    events: Receiver<Event>,
    /// For the reports each stream makes.
    sender: Sender<Event>,
    device: Arc<Mutex<Option<String>>>,
    check: Duration,
}

impl<D: Devices> Thread<D> {
    /// Plays through the default device, opening it again whenever it
    /// fails or another becomes the default, and mixes quietly while none
    /// will open, until told to stop. Tells `opened` after the first try.
    fn run(&self, opened: Sender<()>) {
        let mut first = Some(opened);
        let mut opening = 0;
        // Tries in a row that failed, so a device that will not open is
        // tried less and less often, and only the first failure is logged.
        let mut failed = 0;
        loop {
            opening += 1;
            let report = Report { events: self.sender.clone(), opening };
            match self.devices.open(&self.mixer, report) {
                Ok((stream, id, name)) => {
                    if first.is_none() {
                        tracing::info!(target: "audio", "sound plays through {name}");
                    }
                    self.set_device(Some(name));
                    failed = 0;
                    tell(first.take());
                    let stopped = self.play(opening, id);
                    drop(stream);
                    if stopped {
                        return;
                    }
                }
                Err(error) => {
                    if failed == 0 {
                        tracing::warn!(target: "audio", "no sound: {error:#}; voices play on in silence");
                    }
                    failed += 1;
                    self.set_device(None);
                    tell(first.take());
                    if self.wait_quietly(failed) {
                        return;
                    }
                }
            }
        }
    }

    /// Waits while a stream plays. Returns true when told to stop, or false
    /// when the stream failed or another device became the default.
    fn play(&self, opening: u64, mut id: String) -> bool {
        loop {
            match self.events.recv_timeout(self.check) {
                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => return true,
                Ok(Event::Reopen(at)) if at == opening => {
                    tracing::info!(target: "audio", "the sound output went; opening the default one again");
                    return false;
                }
                Ok(Event::Rerouted(at)) if at == opening => {
                    id = self.devices.default_id().unwrap_or(id);
                    if let Some(name) = self.devices.default_name() {
                        tracing::info!(target: "audio", "sound plays through {name}");
                        self.set_device(Some(name));
                    }
                }
                // From a stream closed since.
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if self.devices.default_id().as_ref() != Some(&id) {
                        return false;
                    }
                }
            }
        }
    }

    /// Mixes quietly after the default device would not open, until told to
    /// stop (returning true), or until it is time to try again: when another
    /// device is the default, or after a wait that grows with each failure.
    fn wait_quietly(&self, failed: u32) -> bool {
        let id = self.devices.default_id();
        let checks = 2u32.saturating_pow(failed - 1).min(MOST_CHECKS_BETWEEN_TRIES);
        let retry_at = Instant::now() + self.check * checks;
        let mut next_check = Instant::now() + self.check;
        run_quietly(&self.mixer, &self.events, || {
            let now = Instant::now();
            if now < next_check {
                return false;
            }
            next_check = now + self.check;
            now >= retry_at || self.devices.default_id() != id
        })
    }

    fn set_device(&self, name: Option<String>) {
        if let Ok(mut device) = self.device.lock() {
            *device = name;
        }
    }
}

/// Tells whoever started the thread that the first device was tried.
fn tell(opened: Option<Sender<()>>) {
    if let Some(opened) = opened {
        let _ = opened.send(());
    }
}

/// The system's output devices, through cpal.
struct System(cpal::Host);

impl Devices for System {
    type Stream = cpal::Stream;

    fn default_id(&self) -> Option<String> {
        let device = self.0.default_output_device()?;
        Some(device.id().map_or_else(|_| String::new(), |id| id.to_string()))
    }

    fn default_name(&self) -> Option<String> {
        self.0.default_output_device().map(|device| name(&device))
    }

    fn open(&self, mixer: &Arc<Mutex<Mixer>>, report: Report) -> Result<(cpal::Stream, String, String)> {
        let device = self.0.default_output_device().context("there is no output device")?;
        let id = device.id().map_or_else(|_| String::new(), |id| id.to_string());
        let supported = device.default_output_config().context("the output device has no format to play")?;
        let mut config: StreamConfig = supported.config();
        if let Ok(mut mixer) = mixer.lock() {
            mixer.set_sample_rate(config.sample_rate);
        }
        if let SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            config.buffer_size = BufferSize::Fixed(BLOCK_FRAMES.clamp(*min, *max));
        }
        let format = supported.sample_format();
        let stream = match build(&device, config, format, mixer, report.clone()) {
            Ok(stream) => stream,
            // Some devices take only their own block size.
            Err(_) => {
                build(&device, StreamConfig { buffer_size: BufferSize::Default, ..config }, format, mixer, report)?
            }
        };
        stream.play().context("the output device would not start")?;
        Ok((stream, id, name(&device)))
    }
}

fn name(device: &cpal::Device) -> String {
    device.description().map_or_else(|_| "the default output".to_string(), |d| d.to_string())
}

fn build(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    mixer: &Arc<Mutex<Mixer>>,
    report: Report,
) -> Result<cpal::Stream> {
    match format {
        SampleFormat::F32 => stream::<f32>(device, config, mixer, report),
        SampleFormat::I16 => stream::<i16>(device, config, mixer, report),
        SampleFormat::U16 => stream::<u16>(device, config, mixer, report),
        SampleFormat::I32 => stream::<i32>(device, config, mixer, report),
        SampleFormat::F64 => stream::<f64>(device, config, mixer, report),
        other => bail!("the output device wants {other} samples, which Voxear does not write"),
    }
}

fn stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: StreamConfig,
    mixer: &Arc<Mutex<Mixer>>,
    report: Report,
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
        move |error| match error.kind() {
            ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated | ErrorKind::HostUnavailable => {
                report.reopen();
            }
            ErrorKind::DeviceChanged => report.rerouted(),
            _ => tracing::warn!(target: "audio", "the sound output: {error}"),
        },
        None,
    )?;
    Ok(stream)
}

/// Mixes into nothing at the pace a device would, until told to stop
/// (returning true), or until `retry` says to try a device again.
fn run_quietly(mixer: &Arc<Mutex<Mixer>>, events: &Receiver<Event>, mut retry: impl FnMut() -> bool) -> bool {
    if let Ok(mut mixer) = mixer.lock() {
        mixer.set_sample_rate(QUIET_RATE);
    }
    let mut block = vec![0.0; BLOCK_FRAMES as usize * 2];
    let pace = Duration::from_secs_f64(f64::from(BLOCK_FRAMES) / f64::from(QUIET_RATE));
    loop {
        if let Ok(mut mixer) = mixer.lock() {
            mixer.render(&mut block, 2);
        }
        match events.recv_timeout(pace) {
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => return true,
            // From a stream closed since.
            Ok(Event::Reopen(_) | Event::Rerouted(_)) | Err(RecvTimeoutError::Timeout) => {}
        }
        if retry() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Audio, Sound, VoiceParams};

    use super::*;

    /// Made-up devices, each named by its id, with the default one chosen
    /// by the test, which plays each stream by hand.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Plugged>>);

    #[derive(Default)]
    struct Plugged {
        default: Option<&'static str>,
        /// Each stream opened, on what, with the mixer it plays and how it
        /// reports, the latest last.
        opened: Vec<(String, Arc<Mutex<Mixer>>, Report)>,
        closed: usize,
    }

    /// A stream on a made-up device, until dropped.
    struct FakeStream(Fake);

    impl Drop for FakeStream {
        fn drop(&mut self) {
            self.0.0.lock().unwrap().closed += 1;
        }
    }

    impl Devices for Fake {
        type Stream = FakeStream;

        fn default_id(&self) -> Option<String> {
            self.0.lock().unwrap().default.map(String::from)
        }

        fn default_name(&self) -> Option<String> {
            self.default_id()
        }

        fn open(&self, mixer: &Arc<Mutex<Mixer>>, report: Report) -> Result<(FakeStream, String, String)> {
            let mut plugged = self.0.lock().unwrap();
            let device = plugged.default.context("there is no output device")?.to_string();
            mixer.lock().unwrap().set_sample_rate(48_000);
            plugged.opened.push((device.clone(), mixer.clone(), report));
            Ok((FakeStream(self.clone()), device.clone(), device))
        }
    }

    impl Fake {
        fn set_default(&self, device: Option<&'static str>) {
            self.0.lock().unwrap().default = device;
        }

        /// Plays a block through the latest stream, returning how loud it is.
        fn play_block(&self) -> f32 {
            let mixer = self.0.lock().unwrap().opened.last().expect("a stream opened").1.clone();
            let mut block = vec![0.0; BLOCK_FRAMES as usize * 2];
            mixer.lock().unwrap().render(&mut block, 2);
            block.iter().fold(0.0, |most, sample| sample.abs().max(most))
        }

        /// The latest stream fails, as when its device is unplugged.
        fn fail(&self) {
            self.0.lock().unwrap().opened.last().expect("a stream opened").2.reopen();
        }

        fn opened(&self) -> Vec<String> {
            self.0.lock().unwrap().opened.iter().map(|(device, ..)| device.clone()).collect()
        }

        fn closed(&self) -> usize {
            self.0.lock().unwrap().closed
        }
    }

    /// The output on `fake`'s devices, playing a sound that loops.
    fn playing(fake: &Fake) -> (Audio, Output) {
        let (mut audio, mixer) = Audio::offline(48_000);
        let sound = Arc::new(Sound::from_samples(vec![0.5; 4800], 1, 48_000));
        audio.play(&sound, VoiceParams { looping: true, ..VoiceParams::default() });
        let devices = fake.clone();
        (audio, start_with(mixer, move || Some(devices), Duration::from_millis(5)))
    }

    fn until(what: &str, done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(5), "waited too long for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn sound_carries_on_through_the_new_default_when_its_device_goes() {
        let fake = Fake::default();
        fake.set_default(Some("Headphones"));
        let (_audio, output) = playing(&fake);
        assert_eq!(output.device().as_deref(), Some("Headphones"));
        assert!(fake.play_block() > 0.1, "the headphones play the sound");

        // Unplugged: their stream fails, and the speakers are the default now.
        fake.set_default(Some("Speakers"));
        fake.fail();
        until("the speakers", || output.device().as_deref() == Some("Speakers"));
        assert_eq!(fake.opened(), ["Headphones", "Speakers"]);
        assert_eq!(fake.closed(), 1, "the headphones' stream is closed");
        assert!(fake.play_block() > 0.1, "the speakers play the sound on");
    }

    #[test]
    fn sound_moves_to_a_device_that_becomes_the_default() {
        let fake = Fake::default();
        fake.set_default(Some("Speakers"));
        let (_audio, output) = playing(&fake);

        // Headphones plugged in: nothing fails, but they are the default now.
        fake.set_default(Some("Headphones"));
        until("the headphones", || output.device().as_deref() == Some("Headphones"));
        assert_eq!(fake.opened(), ["Speakers", "Headphones"]);
        assert!(fake.play_block() > 0.1, "the headphones play the sound");
    }

    #[test]
    fn sound_goes_quiet_without_a_device_and_comes_back_with_one() {
        let fake = Fake::default();
        let (_audio, output) = playing(&fake);
        assert_eq!(output.device(), None, "nothing to play through");

        fake.set_default(Some("Speakers"));
        until("the speakers", || output.device().as_deref() == Some("Speakers"));
        assert!(fake.play_block() > 0.1, "the speakers play the sound, which went on in silence");

        // Unplugged with nothing else to play through, then plugged in again.
        fake.set_default(None);
        fake.fail();
        until("silence", || output.device().is_none());
        fake.set_default(Some("Speakers"));
        until("the speakers again", || output.device().as_deref() == Some("Speakers"));
        assert_eq!(fake.opened(), ["Speakers", "Speakers"]);
    }
}
