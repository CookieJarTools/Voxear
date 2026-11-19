//! Clips baked into bones' matrices, for crowds far off: an Obj with a
//! [`PlayedClip`] is posed on the GPU from its clip's baked frames, with no
//! pose worked out on the CPU and no matrices sent for it each frame. A
//! clip is baked the first time an Obj plays it on a model, `BAKE_RATE`
//! frames a second, each frame every skin's joints' matrices as skinning
//! wants them (where the bone is, from where it was when the mesh was
//! bound), and kept for every Obj after.

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use anyhow::Result;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Component;
use voxear_rhi::{Buffer, Gpu, MemoryLocation, vk};

use crate::{Clip, ModelAnimation};

/// Frames a second clips are baked at.
pub const BAKE_RATE: f32 = 30.0;
/// How many matrices the baked clips' buffer holds at first, and at most:
/// 64 MB, some two hundred seconds of a 66-bone character.
const FIRST_BAKED: usize = 64 * 1024;
const MOST_BAKED: usize = 1024 * 1024;

/// Plays a clip on the Obj's model on the GPU, from its baked frames, in
/// place of a [`crate::Pose`]: `time` seconds in, going round when
/// `looping`. What sets it moves `time` on itself.
#[derive(Clone, Debug)]
pub struct PlayedClip {
    pub clip: Arc<Clip>,
    pub time: f32,
    pub looping: bool,
}

impl Component for PlayedClip {}

/// A clip baked for a model: how many frames, where each of its skins'
/// frames start among the baked matrices, and how far its bones reach from
/// each skin's mesh's middle at most, in the model's space.
#[derive(Clone, Debug)]
pub(crate) struct Baked {
    animation: Weak<ModelAnimation>,
    clip: Weak<Clip>,
    pub frames: u32,
    pub skins: Vec<u32>,
    pub reach: f32,
}

impl Baked {
    /// The frame to show `time` seconds in.
    pub fn frame(&self, clip: &Clip, time: f32, looping: bool) -> u32 {
        let duration = clip.duration();
        let time = match looping && duration > 0.0 {
            true => time.rem_euclid(duration),
            false => time.clamp(0.0, duration),
        };
        ((time * BAKE_RATE).round() as u32).min(self.frames.saturating_sub(1))
    }
}

/// Every clip baked so far, its matrices kept here as they are on the GPU.
#[derive(Default)]
pub(crate) struct BakedClips {
    rows: Vec<[f32; 4]>,
    baked: HashMap<(usize, usize), Baked>,
    buffer: Option<Buffer>,
    capacity: usize,
    /// The rows baked since the buffer was last written.
    sent: usize,
    full: bool,
}

impl BakedClips {
    /// `clip` baked for `animation`, baked now if it was not; `None` when
    /// there is no room left for it.
    pub fn get(&mut self, animation: &Arc<ModelAnimation>, clip: &Arc<Clip>) -> Option<&Baked> {
        let key = (Arc::as_ptr(animation) as usize, Arc::as_ptr(clip) as usize);
        // The same addresses may be another model's or clip's once these are gone.
        let alive = self.baked.get(&key).is_some_and(|baked| {
            baked.animation.upgrade().is_some_and(|a| Arc::ptr_eq(&a, animation))
                && baked.clip.upgrade().is_some_and(|c| Arc::ptr_eq(&c, clip))
        });
        if !alive {
            self.baked.remove(&key);
            let baked = self.bake(animation, clip)?;
            self.baked.insert(key, baked);
        }
        self.baked.get(&key)
    }

    fn bake(&mut self, animation: &Arc<ModelAnimation>, clip: &Arc<Clip>) -> Option<Baked> {
        let frames = (clip.duration() * BAKE_RATE).round() as u32 + 1;
        let joints: usize = animation.skins.iter().map(|skin| skin.joints.len()).sum();
        if self.full || self.rows.len() / 4 + frames as usize * joints > MOST_BAKED {
            self.full = true;
            return None;
        }
        let skeleton = &animation.skeleton;
        let mut pose = skeleton.rest();
        let mut skins = Vec::with_capacity(animation.skins.len());
        let mut reach = 0.0f32;
        let mut bones: Vec<Vec<Mat4>> = Vec::with_capacity(frames as usize);
        for frame in 0..frames {
            clip.sample(frame as f32 / BAKE_RATE, false, &mut pose);
            bones.push(skeleton.model_matrices(&pose));
        }
        for skin in &animation.skins {
            skins.push((self.rows.len() / 4) as u32);
            for posed in &bones {
                for (&joint, bind) in skin.joints.iter().zip(&skin.inverse_binds) {
                    let bone = posed.get(joint).copied().unwrap_or(Mat4::IDENTITY);
                    let m = (bone * *bind).transpose();
                    self.rows.extend([m.x_axis.to_array(), m.y_axis.to_array(), m.z_axis.to_array(), [0.0; 4]]);
                    reach = reach.max(bone.w_axis.truncate().distance(Vec3::ZERO));
                }
            }
        }
        let (animation, clip) = (Arc::downgrade(animation), Arc::downgrade(clip));
        Some(Baked { animation, clip, frames, skins, reach })
    }

    /// Sends what was baked since last to the GPU, the buffer made again
    /// larger, everything sent, when it does not fit.
    pub fn upload(&mut self, gpu: &Gpu) -> Result<()> {
        let needed = (self.rows.len() / 4).max(1);
        if self.buffer.is_none() || self.capacity < needed {
            let capacity = needed.next_power_of_two().max(FIRST_BAKED);
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let buffer = gpu.create_buffer("baked clips", capacity as u64 * 64, usage, MemoryLocation::CpuToGpu)?;
            if let Some(old) = self.buffer.replace(buffer) {
                // Frames in flight read the old one.
                gpu.wait_idle();
                gpu.destroy_buffer(old);
            }
            (self.capacity, self.sent) = (capacity, 0);
        }
        if self.sent < self.rows.len() {
            let buffer = self.buffer.as_ref().expect("made above");
            // Rows past what frames in flight read: none of them reads these yet.
            buffer.write(self.sent as u64 * 16, &self.rows[self.sent..]);
            self.sent = self.rows.len();
        }
        Ok(())
    }

    /// Where the baked matrices are.
    pub fn address(&self) -> u64 {
        self.buffer.as_ref().map_or(0, |buffer| buffer.address)
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(buffer) = self.buffer.take() {
            gpu.destroy_buffer(buffer);
        }
    }
}
