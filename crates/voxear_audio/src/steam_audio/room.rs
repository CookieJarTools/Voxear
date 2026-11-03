//! How the listener's room rings, worked out on a thread of its own:
//! Steam Audio traces thousands of rays from the listener round the walls,
//! which takes tens of milliseconds, too long for a frame. The game tells
//! it the walls when they change and where the listener is each frame,
//! and it works the room out again every tenth of a second at most, from
//! the latest of each.
//!
//! It works the room out as the [`ReverbMode`] it starts with asks: how
//! long the room rings, giving back [`ReverbTimes`] for the mixer's reverb
//! each time, or the room's echoes, giving back once a [`Convolution`] for
//! the mixer to play them through, which takes each new working out as it
//! comes. When the echoes will not start, it falls back to reverb times.

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use super::{Acoustics, Convolution, SteamAudio, Wall};
use crate::{
    reverb::{ReverbMode, ReverbTimes},
    spatial::Listener,
};

/// The least time between two workings out of the room.
const EVERY: Duration = Duration::from_millis(100);

/// What makes a room's simulator for its echoes, at a sample rate, and the
/// echoes to play.
type MakeEchoes = fn(u32) -> Result<(Acoustics, Box<Convolution>), String>;

enum Job {
    Walls(Arc<[Wall]>),
    Listener(Listener),
}

/// What the room's thread gives back: how long the room rings, each time
/// it is worked out, or, once, its echoes to play.
pub enum RoomHeard {
    Times(ReverbTimes),
    Echoes(Box<Convolution>),
}

/// The thread working out the room, until this is dropped.
pub struct RoomReverb {
    jobs: Sender<Job>,
    heard: Receiver<RoomHeard>,
    mode: ReverbMode,
    sample_rate: u32,
}

impl RoomReverb {
    /// Starts the thread working the room out as `mode` asks, its echoes
    /// for a mixer at `sample_rate`, loading Steam Audio if it has not been.
    pub fn start(mode: ReverbMode, sample_rate: u32) -> Result<Self, String> {
        SteamAudio::get()?;
        let (jobs, taken) = mpsc::channel();
        let (given, heard) = mpsc::channel();
        thread::Builder::new()
            .name("voxear-reverb".into())
            .spawn(move || work(mode, sample_rate, &taken, &given))
            .map_err(|error| format!("no thread for reverb: {error}"))?;
        Ok(Self { jobs, heard, mode, sample_rate })
    }

    /// The mode it was started with, whether or not it fell back from it.
    pub fn mode(&self) -> ReverbMode {
        self.mode
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Puts these walls in place of the ones before.
    pub fn set_walls(&self, walls: Arc<[Wall]>) {
        let _ = self.jobs.send(Job::Walls(walls));
    }

    /// Tells it where the listener is now, and gives what it has worked
    /// out since the last time: the room's latest reverb times, or its
    /// echoes, the once.
    pub fn hear_from(&self, listener: Listener) -> Option<RoomHeard> {
        let _ = self.jobs.send(Job::Listener(listener));
        let mut latest = None;
        for heard in self.heard.try_iter() {
            match heard {
                RoomHeard::Echoes(_) => return Some(heard),
                RoomHeard::Times(_) => latest = Some(heard),
            }
        }
        latest
    }
}

fn work(mode: ReverbMode, sample_rate: u32, jobs: &Receiver<Job>, heard: &Sender<RoomHeard>) {
    let Some((mut acoustics, mode)) = simulator(mode, sample_rate, heard, echoes) else { return };
    let mut listener = None;
    while let Ok(job) = jobs.recv() {
        let started = Instant::now();
        // Only the latest walls and listener matter.
        for job in std::iter::once(job).chain(jobs.try_iter()) {
            match job {
                Job::Walls(walls) => acoustics.set_geometry(&walls.iter().map(Wall::mesh).collect::<Vec<_>>()),
                Job::Listener(at) => listener = Some(at),
            }
        }
        let Some(at) = listener else { continue };
        match mode {
            ReverbMode::Convolution => acoustics.echo(&at),
            ReverbMode::Parametric => {
                if heard.send(RoomHeard::Times(acoustics.room(&at))).is_err() {
                    return;
                }
            }
        }
        thread::sleep(EVERY.saturating_sub(started.elapsed()));
    }
}

/// The simulator for the room, and the mode it works in: for `mode`'s
/// echoes, made by `make`, which go back to be played, unless they will
/// not start; otherwise for its reverb times.
fn simulator(
    mode: ReverbMode,
    sample_rate: u32,
    heard: &Sender<RoomHeard>,
    make: MakeEchoes,
) -> Option<(Acoustics, ReverbMode)> {
    if mode == ReverbMode::Convolution {
        match make(sample_rate) {
            Ok((acoustics, echoes)) => {
                heard.send(RoomHeard::Echoes(echoes)).ok()?;
                return Some((acoustics, ReverbMode::Convolution));
            }
            Err(error) => {
                tracing::warn!(target: "audio", "The room rings by its reverb times rather than its echoes: {error}");
            }
        }
    }
    match Acoustics::for_rooms() {
        Ok(acoustics) => Some((acoustics, ReverbMode::Parametric)),
        Err(error) => {
            tracing::warn!(target: "audio", "Sounds play without reverb: {error}");
            None
        }
    }
}

fn echoes(sample_rate: u32) -> Result<(Acoustics, Box<Convolution>), String> {
    let mut acoustics = Acoustics::for_echoes(sample_rate)?;
    let echoes = Convolution::new(&mut acoustics, sample_rate)?;
    Ok((acoustics, echoes))
}

#[cfg(test)]
pub(super) mod tests {
    use voxear_core::math::Vec3;

    use super::*;
    use crate::steam_audio::AcousticMaterial;

    /// A closed box `size` metres across round the origin, its walls facing in.
    pub(in crate::steam_audio) fn room(size: f32, material: AcousticMaterial) -> Wall {
        let half = size * 0.5;
        let corner = |i: u32| Vec3::new(sign(i & 1), sign(i & 2), sign(i & 4)) * half;
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let triangles = faces.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect();
        Wall { vertices: (0..8).map(corner).collect(), triangles, material }
    }

    fn sign(bit: u32) -> f32 {
        if bit == 0 { -1.0 } else { 1.0 }
    }

    /// The first thing the room's thread gives back for these walls.
    fn heard(mode: ReverbMode, walls: Vec<Wall>) -> RoomHeard {
        let reverb = RoomReverb::start(mode, 48_000).expect("Steam Audio");
        reverb.set_walls(walls.into());
        let started = Instant::now();
        loop {
            assert!(started.elapsed() < Duration::from_secs(20), "the room is worked out");
            if let Some(heard) = reverb.hear_from(Listener::default()) {
                return heard;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn rings(walls: Vec<Wall>) -> ReverbTimes {
        match heard(ReverbMode::Parametric, walls) {
            RoomHeard::Times(times) => times,
            RoomHeard::Echoes(_) => panic!("echoes, asked for reverb times"),
        }
    }

    #[test]
    fn a_big_hard_room_rings_longer_than_a_small_soft_one_and_the_open_not_at_all() {
        if SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let hall = rings(vec![room(30.0, AcousticMaterial::CONCRETE)]);
        let closet = rings(vec![room(3.0, AcousticMaterial::CARPET)]);
        let open = rings(Vec::new());
        assert!(hall.0[1] > closet.0[1] * 2.0 && hall.0[1] > 0.5, "hall {hall:?}, closet {closet:?}");
        // Steam Audio's shortest is a tenth of a second, which the reverb takes for no room.
        assert!(open.0.iter().all(|&seconds| seconds <= 0.1 + 1e-3), "outdoors {open:?}");
    }

    #[test]
    fn the_echoes_play_on_the_audio_thread_while_the_rooms_thread_works_them_out_again() {
        if SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let reverb = RoomReverb::start(ReverbMode::Convolution, 48_000).expect("Steam Audio");
        reverb.set_walls(vec![room(20.0, AcousticMaterial::CONCRETE)].into());
        let mut listener = Listener::default();
        let started = Instant::now();
        let mut echoes = loop {
            assert!(started.elapsed() < Duration::from_secs(20), "the echoes are made");
            match reverb.hear_from(listener) {
                Some(RoomHeard::Echoes(echoes)) => break echoes,
                Some(RoomHeard::Times(_)) => panic!("reverb times, asked for echoes"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        };
        // Clicks a tenth of a second apart, while the listener walks and the room is worked out again under them.
        let mut loudest = 0.0f32;
        for at in 0usize.. {
            assert!(started.elapsed() < Duration::from_secs(20), "the room echoes: loudest {loudest}");
            if at % 4_800 == 0 {
                listener.position.x = (at / 4_800 % 10) as f32 * 0.5;
                assert!(reverb.hear_from(listener).is_none(), "the echoes come the once");
                thread::sleep(Duration::from_millis(5));
            }
            let [left, right] = echoes.apply(if at % 4_800 == 0 { 1.0 } else { 0.0 }, &listener, at % 9_600 < 4_800);
            loudest = loudest.max(left.abs()).max(right.abs());
            if loudest > 1e-4 && at > 48_000 {
                break;
            }
        }
    }

    #[test]
    fn echoes_that_will_not_start_fall_back_to_reverb_times() {
        if SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let (given, heard) = mpsc::channel();
        let (_, mode) =
            simulator(ReverbMode::Convolution, 48_000, &given, |_| Err("none here".into())).expect("a simulator");
        assert_eq!(mode, ReverbMode::Parametric);
        assert!(heard.try_recv().is_err(), "no echoes given back");
    }
}
