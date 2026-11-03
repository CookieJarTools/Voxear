//! A 3D voice heard as through headphones: Steam Audio's binaural effect
//! takes the voice a block of [`FRAME`] samples at a time, so the voice's
//! samples are gathered into blocks, and what comes back plays one block
//! behind, about 5 milliseconds at 48 kHz.

use std::{collections::VecDeque, sync::Arc};

use voxear_core::math::Vec3;

use crate::{
    spatial::Listener,
    steam_audio::{Binaural, FRAME, Hrtf},
};

pub(crate) struct Ears {
    effect: Binaural,
    input: Box<[f32; FRAME]>,
    filled: usize,
    /// What the effect gave back, still to be played. It starts a block of
    /// silence long, so that it never runs dry before the next block.
    output: VecDeque<[f32; 2]>,
    /// Frames of silence taken since the voice's sound ended.
    drained: usize,
}

impl Ears {
    pub fn new(hrtf: &Arc<Hrtf>) -> Option<Box<Self>> {
        let mut output = VecDeque::with_capacity(FRAME * 2);
        output.resize(FRAME, [0.0; 2]);
        Some(Box::new(Self {
            effect: Binaural::new(hrtf)?,
            input: Box::new([0.0; FRAME]),
            filled: 0,
            output,
            drained: 0,
        }))
    }

    /// Takes the voice's next sample, at its volume, from `at`, and gives
    /// the left and right to play now.
    pub fn hear(&mut self, sample: f32, listener: &Listener, at: Vec3) -> [f32; 2] {
        self.input[self.filled] = sample;
        self.filled += 1;
        if self.filled == FRAME {
            self.effect.apply(&mut self.input, listener, at, &mut self.output);
            self.filled = 0;
        }
        self.output.pop_front().unwrap_or_default()
    }

    /// Once the voice's sound is over: plays out what is still to come, a
    /// frame at a time, and then nothing.
    pub fn drain(&mut self, listener: &Listener, at: Vec3) -> Option<[f32; 2]> {
        if self.drained >= FRAME {
            return None;
        }
        self.drained += 1;
        Some(self.hear(0.0, listener, at))
    }
}
