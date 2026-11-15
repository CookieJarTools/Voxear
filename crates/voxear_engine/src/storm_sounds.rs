//! The weather's sounds while the game runs: rain, wind and a rough sea's
//! surf, each a loop as loud as the weather where the listener is, and
//! thunder after each stroke of lightning, later and lower the further off
//! it is taken to be. Under a roof the rain drums muffled overhead and the
//! wind drops.
//!
//! The sounds are made here, from filtered noise, once, the first time
//! they are wanted: nothing ships for them. How loud they all are is the
//! Storms component's **Sounds** (`StormRules::sounds`), none at 0.

use std::sync::{Arc, OnceLock};

use voxear_audio::steam_audio::Muffle;
use voxear_core::math::Vec3;
use voxear_ecs::World;
use voxear_render::sea::SeaLevel;

use crate::audio::{Audio, Bus, Sound, VoiceId, VoiceParams};
use crate::weather::Weather;

/// How fast the sounds are made, a second.
const RATE: u32 = 44_100;
/// How long each loop is, in seconds.
const RAIN_LOOP: f32 = 4.0;
const WIND_LOOP: f32 = 8.0;
const SURF_LOOP: f32 = 12.0;
const THUNDER: f32 = 6.0;
/// How fast a loop comes up to how loud it should be, and dies away, in
/// seconds.
const EASE: f32 = 1.2;
/// Quieter than this, a loop is stopped.
const SILENT: f32 = 0.002;
/// How rain and wind are heard from under a roof: their low, middle and
/// high pitches each so loud.
const ROOFED_RAIN: Muffle = Muffle([0.9, 0.45, 0.15]);
const ROOFED_WIND: Muffle = Muffle([0.7, 0.35, 0.15]);

/// The sounds, made once.
struct Made {
    rain: Arc<Sound>,
    wind: Arc<Sound>,
    surf: Arc<Sound>,
    near: Arc<Sound>,
    far: Arc<Sound>,
}

fn made() -> &'static Made {
    static MADE: OnceLock<Made> = OnceLock::new();
    MADE.get_or_init(|| Made {
        rain: Arc::new(stereo(RAIN_LOOP, true, Rain::default())),
        wind: Arc::new(stereo(WIND_LOOP, true, Wind::default())),
        surf: Arc::new(stereo(SURF_LOOP, true, Surf::default())),
        near: Arc::new(stereo(THUNDER, false, Thunder { near: true, ..Thunder::default() })),
        far: Arc::new(stereo(THUNDER, false, Thunder::default())),
    })
}

/// Random numbers from a seed, the same each time.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

/// A filter that lets through what is lower than `hz` and softens the rest.
#[derive(Clone, Copy, Default)]
struct Low(f32);

impl Low {
    fn run(&mut self, x: f32, hz: f32) -> f32 {
        let k = 1.0 - (-std::f32::consts::TAU * hz / RATE as f32).exp();
        self.0 += (x - self.0) * k;
        self.0
    }
}

/// What makes one of the sounds, a sample at a time, from noise, given the
/// time; each channel has one of its own.
trait Maker: Clone {
    fn sample(&mut self, noise: &mut Noise, t: f32) -> f32;
}

/// A sound of `seconds`, two channels each made by its own `maker` from
/// noise of its own: a loop has its end laid over its start, so it goes
/// round without a seam.
fn stereo(seconds: f32, looped: bool, maker: impl Maker) -> Sound {
    let frames = (seconds * RATE as f32) as usize;
    let fade = if looped { RATE as usize / 2 } else { 0 };
    let channel = |seed: u64| {
        let (mut noise, mut maker) = (Noise(seed), maker.clone());
        let mut made: Vec<f32> = (0..frames + fade).map(|n| maker.sample(&mut noise, n as f32 / RATE as f32)).collect();
        for n in 0..fade {
            let part = n as f32 / fade as f32;
            made[n] = made[n] * part + made[frames + n] * (1.0 - part);
        }
        made.truncate(frames);
        made
    };
    let (left, right) = (channel(0x5107_0001), channel(0x5107_0002));
    let samples = left.into_iter().zip(right).flat_map(|(l, r)| [l, r]).collect();
    Sound::from_samples(samples, 2, RATE)
}

/// Rain: a hiss of fine drops, and the patter of larger ones each a short
/// tick of its own.
#[derive(Clone, Copy, Default)]
struct Rain {
    low: Low,
    lower: Low,
    drop: f32,
    pitch: f32,
}

impl Maker for Rain {
    fn sample(&mut self, noise: &mut Noise, t: f32) -> f32 {
        let white = noise.next();
        // Hiss between a few hundred and a few thousand a second.
        let hiss = self.low.run(white, 5000.0) - self.lower.run(white, 400.0);
        // A drop now and then, ringing a moment.
        if noise.next() > 0.986 {
            self.drop = 0.3 + 0.5 * noise.next().abs();
            self.pitch = 1800.0 + 2500.0 * noise.next().abs();
        }
        let tick = self.drop * (std::f32::consts::TAU * self.pitch * t).sin();
        self.drop *= 0.992;
        (hiss * 0.5 + tick * 0.25).clamp(-1.0, 1.0)
    }
}

/// Wind: a low roar that gusts, rising and falling a few times round the
/// loop, with a faint whistle on the gusts.
#[derive(Clone, Copy, Default)]
struct Wind {
    roar: Low,
    body: Low,
    whistle: Low,
}

impl Maker for Wind {
    fn sample(&mut self, noise: &mut Noise, t: f32) -> f32 {
        let white = noise.next();
        let tau = std::f32::consts::TAU;
        // Gusts a whole number of times round the loop, so it has no seam.
        let gust = 0.55 + 0.3 * (tau * t * 2.0 / WIND_LOOP).sin() + 0.15 * (tau * t * 5.0 / WIND_LOOP + 1.3).sin();
        let low = self.roar.run(self.body.run(white, 900.0), 260.0);
        let high = self.whistle.run(white, 1400.0) - low;
        ((low * 3.0 + high * 0.25 * gust) * gust).clamp(-1.0, 1.0)
    }
}

/// Surf: the sea's roar, swelling as each wave comes in and breaks, twice
/// round the loop.
#[derive(Clone, Copy, Default)]
struct Surf {
    roar: Low,
    wash: Low,
}

impl Maker for Surf {
    fn sample(&mut self, noise: &mut Noise, t: f32) -> f32 {
        let white = noise.next();
        let beat = (t / (SURF_LOOP / 2.0)).fract();
        // Rising as it comes in, breaking, then washing back.
        let swell = if beat < 0.3 { beat / 0.3 } else { (-(beat - 0.3) * 4.0).exp() };
        let low = self.roar.run(white, 600.0);
        let hiss = self.wash.run(white, 3000.0) - low;
        ((low * 2.0 * (0.35 + 0.65 * swell) + hiss * 0.5 * swell) * 0.8).clamp(-1.0, 1.0)
    }
}

/// Thunder: near, a crack and then a long rumble with a few more peals in
/// it; far, the rumble alone, lower.
#[derive(Clone, Copy, Default)]
struct Thunder {
    near: bool,
    rumble: Low,
    deep: Low,
    crack: Low,
}

impl Maker for Thunder {
    fn sample(&mut self, noise: &mut Noise, t: f32) -> f32 {
        let white = noise.next();
        let peals = [(0.0, 1.0), (0.7, 0.6), (1.6, 0.45), (2.9, 0.3)];
        let loud: f32 = peals.iter().map(|&(at, how)| if t >= at { how * (-(t - at) * 1.3).exp() } else { 0.0 }).sum();
        let low = self.deep.run(self.rumble.run(white, if self.near { 220.0 } else { 120.0 }), 90.0);
        let crack = self.crack.run(white, 4000.0) - low;
        let sharp = if self.near && t < 0.12 { crack * (1.0 - t / 0.12) * 0.9 } else { 0.0 };
        let fade = 1.0 - smooth(THUNDER - 1.0, THUNDER, t);
        ((low * 6.0 * loud + sharp) * fade).clamp(-1.0, 1.0)
    }
}

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A number from 0 to 1 for a whole number, the same every time.
fn chance(of: u64) -> f32 {
    let mut x = of.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((x ^ (x >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

/// One of the looping sounds: its voice while one plays, and how loud it is.
#[derive(Clone, Copy, Debug, Default)]
struct Loop {
    voice: VoiceId,
    volume: f32,
    muffled: bool,
}

/// How loud each sound should be where the listener is, and whether they
/// are under a roof.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heard {
    pub rain: f32,
    pub wind: f32,
    pub surf: f32,
    pub roofed: bool,
}

/// How loud the weather's sounds should be at `at`, in the world.
pub fn heard_at(world: &World, at: Vec3) -> Heard {
    let volume = crate::storms::rules(world).sounds.clamp(0.0, 1.0);
    let now = world.resource::<Weather>().map(Weather::now).unwrap_or_default();
    let roofed = crate::weather_maps::covered(world, at);
    // The sea is heard as rough as it is, from near its top.
    let sea_top = world
        .query_ref::<(&SeaLevel, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .map(|(sea, local, placed)| voxear_terrain::corner(local, placed).y + sea.0)
        .next();
    let rough = world.resource::<crate::storms::SeaNow>().map_or(0.0, |sea| sea.0.rough_at(at.x, at.z));
    let near_sea = sea_top.map_or(0.0, |top| 1.0 - smooth(4.0, 60.0, at.y - top));
    Heard {
        rain: volume * now.rain.max(0.0).powf(0.7) * 0.55 * if roofed { 0.8 } else { 1.0 },
        wind: volume * (now.wind.max(0.0).powf(1.3) * 0.45 + now.dust * 0.2).min(1.0) * if roofed { 0.6 } else { 1.0 },
        surf: volume * rough * near_sea * 0.5,
        roofed,
    }
}

/// The weather's voices, kept by the game's sound.
#[derive(Default)]
pub struct StormSounds {
    rain: Loop,
    wind: Loop,
    surf: Loop,
    /// The lightning last frame, and thunder to come: when, on this clock,
    /// how loud and whether near.
    flash: f32,
    thunder: Vec<(f64, f32, bool)>,
    clock: f64,
    /// How many peals of thunder it has played.
    pub(crate) pealed: u32,
}

impl StormSounds {
    /// Each frame the game runs: every loop brought towards how loud the
    /// weather where the listener is makes it, and thunder after each
    /// stroke of lightning.
    pub fn update(&mut self, audio: &mut Audio, world: &World, dt: f32) {
        self.clock += f64::from(dt.max(0.0));
        let Some(listener) = crate::audio::listener(world) else {
            self.quiet(audio);
            return;
        };
        let heard = heard_at(world, listener.position);
        if heard == Heard::default() && self.is_silent() && self.thunder.is_empty() && self.flash <= 0.0 {
            return;
        }
        let ease = (dt / EASE).min(1.0);
        let made = made();
        let muffles = [(ROOFED_RAIN, heard.rain), (ROOFED_WIND, heard.wind), (Muffle::NONE, heard.surf)];
        for ((sound, voice), (roofed, wanted)) in
            [(&made.rain, &mut self.rain), (&made.wind, &mut self.wind), (&made.surf, &mut self.surf)]
                .into_iter()
                .zip(muffles)
        {
            voice.volume += (wanted - voice.volume) * ease;
            if voice.volume <= SILENT && wanted <= SILENT {
                if voice.voice != 0 {
                    audio.stop(voice.voice);
                }
                *voice = Loop::default();
                continue;
            }
            if voice.voice == 0 || !audio.exists(voice.voice) {
                voice.voice = audio.play(
                    sound,
                    VoiceParams { volume: voice.volume, looping: true, bus: Bus::Effects, ..VoiceParams::default() },
                );
                voice.muffled = false;
            }
            audio.set_volume(voice.voice, voice.volume);
            if voice.muffled != heard.roofed {
                audio.set_muffle(voice.voice, if heard.roofed { roofed } else { Muffle::NONE });
                voice.muffled = heard.roofed;
            }
        }

        // Thunder: each stroke heard after as long as it takes to come, the further the later and the lower.
        let weather = world.resource::<Weather>();
        let flash = weather.map_or(0.0, Weather::flash);
        let lightning = weather.map_or(0.0, |weather| weather.now().lightning);
        let volume = crate::storms::rules(world).sounds.clamp(0.0, 1.0);
        if flash > 0.0 && self.flash <= 0.0 && volume > 0.0 {
            let key = (self.clock * 10.0) as u64;
            let delay = 0.3 + 5.0 * chance(key);
            let near = delay < 1.6;
            let loud = volume * (0.35 + 0.65 * lightning) * (1.0 - delay / 7.0) * if heard.roofed { 0.7 } else { 1.0 };
            self.thunder.push((self.clock + f64::from(delay), loud, near));
        }
        self.flash = flash;
        let clock = self.clock;
        for (_, loud, near) in self.thunder.iter().filter(|(at, ..)| *at <= clock) {
            let sound = if *near { &made.near } else { &made.far };
            let pitch = 0.85 + 0.25 * chance(clock.to_bits());
            audio.play(sound, VoiceParams { volume: *loud, pitch, bus: Bus::Effects, ..VoiceParams::default() });
            self.pealed += 1;
        }
        self.thunder.retain(|(at, ..)| *at > clock);
    }

    fn is_silent(&self) -> bool {
        [self.rain, self.wind, self.surf].iter().all(|voice| voice.voice == 0)
    }

    /// Stops every loop.
    pub fn quiet(&mut self, audio: &mut Audio) {
        for voice in [self.rain, self.wind, self.surf] {
            if voice.voice != 0 {
                audio.stop(voice.voice);
            }
        }
        *self = Self::default();
    }

    /// The voices of the rain, wind and surf loops, those playing.
    pub fn voices(&self) -> Vec<VoiceId> {
        [self.rain, self.wind, self.surf].iter().map(|voice| voice.voice).filter(|&voice| voice != 0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loudness(sound: &Sound) -> f32 {
        let samples = sound.samples().expect("made from samples");
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn the_sounds_are_made_as_loud_as_heard_and_their_loops_go_round_without_a_seam() {
        let made = made();
        for (name, sound, seconds) in
            [("rain", &made.rain, RAIN_LOOP), ("wind", &made.wind, WIND_LOOP), ("surf", &made.surf, SURF_LOOP)]
        {
            let info = sound.info();
            assert_eq!((info.channels, info.sample_rate), (2, RATE));
            assert_eq!(info.frames, (seconds * RATE as f32) as u64, "{name}");
            let rms = loudness(sound);
            assert!(rms > 0.02 && rms < 0.6, "{name}: {rms}");
            // Its end runs into its start: no jump bigger than any step within it.
            let samples = sound.samples().unwrap();
            let step = |a: f32, b: f32| (a - b).abs();
            let biggest =
                samples.chunks(2).collect::<Vec<_>>().windows(2).map(|w| step(w[0][0], w[1][0])).fold(0.0, f32::max);
            let seam = step(samples[samples.len() - 2], samples[0]);
            assert!(seam <= biggest, "{name}: a seam of {seam} against steps of {biggest}");
        }
        assert!(loudness(&made.near) > loudness(&made.far) * 0.8, "thunder near is no quieter");
        assert!(made.near.samples().unwrap().iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn a_storm_is_heard_as_loud_as_it_blows_thunder_follows_lightning_and_a_roof_muffles_the_rain() {
        use voxear_render::weather::WeatherKind;
        let mut world = crate::weather_maps::tests::roofed_yard();
        let out = Vec3::new(15.2, 2.0, 5.2);
        assert_eq!(heard_at(&world, out), Heard::default(), "nothing in clear weather");
        world.insert_resource(Weather::of(WeatherKind::Storm));
        crate::weather_maps::tests::settle(&mut world);
        let storm = heard_at(&world, out);
        assert!(storm.rain > 0.4 && storm.wind > 0.3 && !storm.roofed, "{storm:?}");
        let under = heard_at(&world, Vec3::new(5.2, 2.0, 5.2));
        assert!(under.roofed && under.rain < storm.rain && under.wind < storm.wind, "{under:?}");
        // No sea, no surf.
        assert_eq!(storm.surf, 0.0);

        let mut audio = Audio::quiet();
        let mut sounds = StormSounds::default();
        for _ in 0..(90 * 10) {
            world.resource_mut::<Weather>().unwrap().advance(0.1);
            sounds.update(&mut audio, &world, 0.1);
        }
        assert_eq!(sounds.voices().len(), 2, "rain and wind");
        assert!(sounds.pealed > 3, "thunder after the lightning: {} peals", sounds.pealed);
        // Turned off on the Storms component, they die away.
        world.spawn(crate::storms::StormRules { sounds: 0.0, ..Default::default() });
        for _ in 0..100 {
            sounds.update(&mut audio, &world, 0.1);
        }
        assert!(sounds.voices().is_empty());
    }
}
