//! What a model brings for animation: its skeleton, each bone's place and
//! parent; its skins, which bones move a mesh's vertices and how they stood
//! when it was bound to them; and its clips, every bone's pose sampled at an
//! even rate. All in Voxear's space: mirrored on X and sized as the model's
//! meshes are (see `gltf_import`).

use voxear_core::math::{Mat4, Quat, Vec3};

/// The rate clips are sampled at, in frames a second.
pub const CLIP_RATE: f32 = 60.0;

/// Which bones move a vertex, up to four, and how much each, out of 65535,
/// the four adding up to it. Kept beside the mesh's vertices, one each.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinVertex {
    /// Joints of the mesh's skin (see [`Skin::joints`]).
    pub joints: [u16; 4],
    pub weights: [u16; 4],
}

impl SkinVertex {
    /// Up to four joints and weights, the heaviest kept and the weights made
    /// to add up to one, as 65535.
    pub fn new(joints: [u16; 4], weights: [f32; 4]) -> Self {
        let total: f32 = weights.iter().map(|w| w.max(0.0)).sum();
        if total <= 0.0 {
            return Self { joints, weights: [u16::MAX, 0, 0, 0] };
        }
        let mut scaled = weights.map(|w| (w.max(0.0) / total * 65535.0).round() as u16);
        // Rounding may leave the sum a little off; the heaviest takes it.
        let sum: i32 = scaled.iter().map(|&w| i32::from(w)).sum();
        let heaviest = (0..4).max_by_key(|&i| scaled[i]).unwrap_or(0);
        scaled[heaviest] = (i32::from(scaled[heaviest]) + 65535 - sum).clamp(0, 65535) as u16;
        Self { joints, weights: scaled }
    }
}

/// A bone's place relative to its parent's, or to the model's for a bone
/// with none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BonePose {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for BonePose {
    fn default() -> Self {
        Self { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }
}

impl BonePose {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Between this pose and `other`, `t` of the way.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Its parent, always before it among the skeleton's bones.
    pub parent: Option<usize>,
    /// Where it stands when nothing animates it.
    pub rest: BonePose,
}

/// A model's bones, every parent before its children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

impl Skeleton {
    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// Each bone's matrix in the model's space for a pose, one per bone.
    pub fn model_matrices(&self, pose: &[BonePose]) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (bone, local) in self.bones.iter().zip(pose) {
            let local = local.matrix();
            let matrix = bone.parent.map_or(local, |parent| out[parent] * local);
            out.push(matrix);
        }
        out
    }

    /// Every bone at rest.
    pub fn rest(&self) -> Vec<BonePose> {
        self.bones.iter().map(|bone| bone.rest).collect()
    }
}

/// The bones that move a skinned mesh, and how each stood, in the model's
/// space, when the mesh was bound to it: a vertex goes where
/// `bone × inverse_bind × vertex` puts it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skin {
    /// The skeleton's bone for each of the skin's joints.
    pub joints: Vec<usize>,
    pub inverse_binds: Vec<Mat4>,
    /// For each joint, the box round the vertices it moves most, in its own
    /// space as it was bound, least and most: what hands holding a skinned
    /// gun stop at. A joint that moves none has its least above its most.
    pub bounds: Vec<(Vec3, Vec3)>,
    /// For each joint, those vertices cut into slices along their box's
    /// longest side, about 4 cm each, and the box round each slice: a
    /// closer fit than the one box for a long bone, as a gun's whose stock,
    /// action and barrel all move as one. None for a joint that moves none.
    pub slices: Vec<Vec<(Vec3, Vec3)>>,
}

/// An animation: every bone's pose at each of its frames, `CLIP_RATE` a
/// second, frame by frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub name: String,
    pub frames: usize,
    /// `frames` × the skeleton's bones.
    pub poses: Vec<BonePose>,
}

impl Clip {
    /// How long it lasts, in seconds, from its first frame to its last.
    pub fn duration(&self) -> f32 {
        self.frames.saturating_sub(1) as f32 / CLIP_RATE
    }

    /// Every bone's pose at frame `frame`.
    pub fn frame(&self, frame: usize) -> &[BonePose] {
        let bones = self.poses.len() / self.frames.max(1);
        let frame = frame.min(self.frames.saturating_sub(1));
        &self.poses[frame * bones..(frame + 1) * bones]
    }

    /// Every bone's pose `time` seconds in, between the frames either side,
    /// into `out`; going round from the end to the start when `looping`.
    pub fn sample(&self, time: f32, looping: bool, out: &mut [BonePose]) {
        if self.frames == 0 {
            return;
        }
        let duration = self.duration();
        let time = match looping && duration > 0.0 {
            true => time.rem_euclid(duration),
            false => time.clamp(0.0, duration),
        };
        let at = time * CLIP_RATE;
        let first = (at.floor() as usize).min(self.frames - 1);
        let next = (first + 1).min(self.frames - 1);
        let t = at - first as f32;
        for ((slot, a), b) in out.iter_mut().zip(self.frame(first)).zip(self.frame(next)) {
            *slot = a.lerp(b, t);
        }
    }
}

/// All a model brings for animation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelAnimation {
    pub skeleton: Skeleton,
    pub skins: Vec<Skin>,
    pub clips: Vec<Clip>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm() -> Skeleton {
        let at = |x: f32| BonePose { translation: Vec3::new(x, 0.0, 0.0), ..BonePose::default() };
        Skeleton {
            bones: vec![
                Bone { name: "shoulder".into(), parent: None, rest: at(0.0) },
                Bone { name: "elbow".into(), parent: Some(0), rest: at(1.0) },
            ],
        }
    }

    #[test]
    fn a_child_bone_follows_its_parent() {
        let skeleton = arm();
        let mut pose = skeleton.rest();
        pose[0].rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let matrices = skeleton.model_matrices(&pose);
        let elbow = matrices[1].w_axis.truncate();
        assert!((elbow - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5, "{elbow}");
        assert_eq!(skeleton.find("elbow"), Some(1));
    }

    #[test]
    fn a_clip_is_sampled_between_its_frames_and_round_when_looping() {
        let skeleton = arm();
        let mut poses = skeleton.rest();
        let mut last = skeleton.rest();
        last[1].translation.x = 3.0;
        poses.extend(last);
        let clip = Clip { name: "reach".into(), frames: 2, poses };
        let mut out = skeleton.rest();
        clip.sample(0.5 / CLIP_RATE, false, &mut out);
        assert!((out[1].translation.x - 2.0).abs() < 1e-5);
        clip.sample(1.5 / CLIP_RATE, true, &mut out);
        assert!((out[1].translation.x - 2.0).abs() < 1e-5, "round to the middle again");
        clip.sample(9.0, false, &mut out);
        assert_eq!(out[1].translation.x, 3.0, "held at its end");
    }

    #[test]
    fn skin_weights_add_up_whatever_they_were() {
        let skin = SkinVertex::new([0, 1, 2, 3], [0.5, 0.25, 0.25, 0.0]);
        assert_eq!(skin.weights.iter().map(|&w| u32::from(w)).sum::<u32>(), 65535);
        let odd = SkinVertex::new([0, 1, 2, 3], [0.3, 0.3, 0.3, 0.0]);
        assert_eq!(odd.weights.iter().map(|&w| u32::from(w)).sum::<u32>(), 65535);
        assert_eq!(SkinVertex::new([4, 0, 0, 0], [0.0; 4]).weights, [65535, 0, 0, 0], "none given: all on the first");
    }
}
