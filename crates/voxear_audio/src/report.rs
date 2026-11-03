//! What the mixer is playing, for the editor's Audio panel: each voice as
//! it was at the end of the mixer's last block. The mixer writes the list
//! only while something watches, and never waits for it: a block that
//! finds the list being read leaves it for the next.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use voxear_core::math::Vec3;

use crate::{Bus, VoiceId, steam_audio::Muffle};

/// One voice, as the mixer has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceReport {
    pub id: VoiceId,
    pub bus: Bus,
    /// Its own volume, the sound's from its import settings times what the
    /// game set.
    pub volume: f32,
    /// How loud it is heard, on the louder side: its volume, its bus and
    /// the master volume, and for a 3D voice its distance and pan.
    pub heard: f32,
    pub pitch: f32,
    /// Where a 3D voice is.
    pub at: Option<Vec3>,
    /// How much gets past the walls, once any have been in its way.
    pub muffle: Option<Muffle>,
    /// Heard through Steam Audio's HRTF.
    pub binaural: bool,
    pub looping: bool,
    pub paused: bool,
    /// Fading out, stopped or at its end.
    pub ending: bool,
}

#[derive(Default)]
pub(crate) struct VoiceBoard {
    pub watched: AtomicBool,
    pub voices: Mutex<Vec<VoiceReport>>,
}

impl VoiceBoard {
    pub fn watched(&self) -> bool {
        self.watched.load(Ordering::Relaxed)
    }

    pub fn watch(&self, on: bool) {
        self.watched.store(on, Ordering::Relaxed);
    }

    /// The list as the mixer last wrote it.
    pub fn read(&self) -> Vec<VoiceReport> {
        self.voices.lock().map(|voices| voices.clone()).unwrap_or_default()
    }

    /// Writes the list, unless it is being read.
    pub fn write(&self, voices: impl Iterator<Item = VoiceReport>) {
        if let Ok(mut list) = self.voices.try_lock() {
            list.clear();
            list.extend(voices);
        }
    }
}
