//! How long the listener's room rings, worked out on a thread of its own:
//! Steam Audio traces thousands of rays from the listener round the walls,
//! which takes tens of milliseconds, too long for a frame. The game tells
//! it the walls when they change and where the listener is each frame,
//! and it works the room out again every tenth of a second at most, from
//! the latest of each, giving back [`ReverbTimes`] for the mixer's reverb.

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use super::{Acoustics, Wall};
use crate::{reverb::ReverbTimes, spatial::Listener};

/// The least time between two workings out of the room.
const EVERY: Duration = Duration::from_millis(100);

enum Job {
    Walls(Arc<[Wall]>),
    Listener(Listener),
}

/// The thread working out the room, until this is dropped.
pub struct RoomReverb {
    jobs: Sender<Job>,
    times: Receiver<ReverbTimes>,
}

impl RoomReverb {
    /// Starts the thread, loading Steam Audio if it has not been.
    pub fn start() -> Result<Self, String> {
        let acoustics = Acoustics::for_rooms()?;
        let (jobs, taken) = mpsc::channel();
        let (given, times) = mpsc::channel();
        thread::Builder::new()
            .name("voxear-reverb".into())
            .spawn(move || work(acoustics, &taken, &given))
            .map_err(|error| format!("no thread for reverb: {error}"))?;
        Ok(Self { jobs, times })
    }

    /// Puts these walls in place of the ones before.
    pub fn set_walls(&self, walls: Arc<[Wall]>) {
        let _ = self.jobs.send(Job::Walls(walls));
    }

    /// Tells it where the listener is now, and gives the room's latest
    /// reverb times, when new ones have come since the last time.
    pub fn hear_from(&self, listener: Listener) -> Option<ReverbTimes> {
        let _ = self.jobs.send(Job::Listener(listener));
        self.times.try_iter().last()
    }
}

fn work(mut acoustics: Acoustics, jobs: &Receiver<Job>, times: &Sender<ReverbTimes>) {
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
        if times.send(acoustics.room(&at)).is_err() {
            return;
        }
        thread::sleep(EVERY.saturating_sub(started.elapsed()));
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Vec3;

    use super::*;
    use crate::steam_audio::AcousticMaterial;

    /// A closed box `size` metres across round the origin, its walls facing in.
    fn room(size: f32, material: AcousticMaterial) -> Wall {
        let half = size * 0.5;
        let corner = |i: u32| Vec3::new(sign(i & 1), sign(i & 2), sign(i & 4)) * half;
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let triangles = faces.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect();
        Wall { vertices: (0..8).map(corner).collect(), triangles, material }
    }

    fn sign(bit: u32) -> f32 {
        if bit == 0 { -1.0 } else { 1.0 }
    }

    fn rings(walls: Vec<Wall>) -> ReverbTimes {
        let reverb = RoomReverb::start().expect("Steam Audio");
        reverb.set_walls(walls.into());
        let started = Instant::now();
        loop {
            assert!(started.elapsed() < Duration::from_secs(20), "the room is worked out");
            if let Some(times) = reverb.hear_from(Listener::default()) {
                return times;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_big_hard_room_rings_longer_than_a_small_soft_one_and_the_open_not_at_all() {
        if crate::steam_audio::SteamAudio::get().is_err() {
            return eprintln!("skipping: Steam Audio's library is not fetched");
        }
        let hall = rings(vec![room(30.0, AcousticMaterial::CONCRETE)]);
        let closet = rings(vec![room(3.0, AcousticMaterial::CARPET)]);
        let open = rings(Vec::new());
        assert!(hall.0[1] > closet.0[1] * 2.0 && hall.0[1] > 0.5, "hall {hall:?}, closet {closet:?}");
        // Steam Audio's shortest is a tenth of a second, which the reverb takes for no room.
        assert!(open.0.iter().all(|&seconds| seconds <= 0.1 + 1e-3), "outdoors {open:?}");
    }
}
