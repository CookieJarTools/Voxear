//! A box model's clips: its bones moved by keys. A key says how far a bone
//! is turned, moved and sized from how it stands at rest, at one moment of
//! the clip, and between two keys the bone goes from one to the other as
//! the earlier one's [`Ease`] says. Only bones that move have keys, and
//! only at the moments that matter, so a clip is a handful of numbers.
//!
//! A clip that loops goes on from its last key round to its first, so a
//! walk needs no key at its end to match the one at its start.
//!
//! What plays a model's clips wants every bone's pose at every frame:
//! [`BoxModel::baked`] makes that of a clip, and [`BoxModel::prepared`]
//! gives the model's clips so, as a model file's own clips are given.
//!
//! In the file each clip lists the bones it moves, and each of those its
//! keys' times with what they say, side by side:
//!
//! ```toml
//! [[clips]]
//! name = "Wave"
//! length = 1.0
//! looping = true
//!
//! [[clips.bones]]
//! bone = "RightArm"
//! times = [0.0, 0.5]
//! rotation = [[0.0, 0.0, 0.0], [0.0, 0.0, -120.0]]
//! ```

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};

use super::{BoxModel, degrees, turned};
use crate::skeleton::{BonePose, CLIP_RATE, Clip};

/// Keys stand on moments this far apart, in seconds: three frames of a
/// clip as it plays.
pub const KEY_STEP: f32 = 0.05;

/// The longest a clip lasts, in seconds.
pub const LONGEST_CLIP: f32 = 60.0;

/// The moment keys stand on that is nearest `time`.
pub fn on_step(time: f32) -> f32 {
    (time / KEY_STEP).round() / (1.0 / KEY_STEP).round()
}

/// How a bone goes from a key to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ease {
    /// Straight there, at one speed.
    #[default]
    Linear,
    /// In a curve through the keys either side too, never stopping at one.
    Smooth,
    /// Holding this key until the next, then there at once.
    Step,
}

impl Ease {
    pub const ALL: [Ease; 3] = [Ease::Linear, Ease::Smooth, Ease::Step];

    /// As a list offers it.
    pub fn label(self) -> &'static str {
        match self {
            Ease::Linear => "Linear",
            Ease::Smooth => "Smooth",
            Ease::Step => "Step",
        }
    }
}

/// How a bone stands at one moment of a clip, from how it stands at rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxKey {
    /// Seconds from the clip's start.
    pub time: f32,
    /// How far it is turned about its pivot, in degrees.
    pub rotation: Vec3,
    /// How far its pivot is moved, in pixels.
    pub offset: Vec3,
    /// How large it and all under it are: 1 as built.
    pub scale: Vec3,
    /// How it goes on to the next key.
    pub ease: Ease,
}

impl BoxKey {
    /// A key at `time` that leaves the bone as it stands at rest.
    pub fn at_rest(time: f32) -> Self {
        Self { time, rotation: Vec3::ZERO, offset: Vec3::ZERO, scale: Vec3::ONE, ease: Ease::Linear }
    }

    /// A key at `time` that turns the bone.
    pub fn turned(time: f32, rotation: Vec3) -> Self {
        Self { rotation, ..Self::at_rest(time) }
    }

    /// Turns the bone `by` degrees further about one of its own axes, X, Y
    /// or Z by its number, as the key leaves it: what a handle round the
    /// bone does.
    pub fn turn_about(&mut self, axis: usize, by: f32) {
        let about = [Vec3::X, Vec3::Y, Vec3::Z][axis.min(2)];
        self.rotation = degrees(turned(self.rotation) * Quat::from_axis_angle(about, by.to_radians()));
    }

    fn mix(&self, other: &Self, t: f32) -> Self {
        Self {
            rotation: self.rotation.lerp(other.rotation, t),
            offset: self.offset.lerp(other.offset, t),
            scale: self.scale.lerp(other.scale, t),
            ..*self
        }
    }
}

/// One bone's keys in a clip, the earliest first.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxTrack {
    /// The bone, by its name: a bone moved among the bones keeps its keys.
    pub bone: String,
    pub keys: Vec<BoxKey>,
}

impl BoxTrack {
    /// How the bone stands `time` seconds into a clip `length` long:
    /// between the keys either side, and for a clip that loops, between
    /// its last key and its first where `time` is past one or before the
    /// other.
    pub fn at(&self, time: f32, length: f32, looping: bool) -> BoxKey {
        let count = self.keys.len() as isize;
        if count == 0 {
            return BoxKey::at_rest(time);
        }
        // The key so many places along, going round a loop or stopping at the ends.
        let key = |place: isize| match looping && length > 0.0 {
            true => {
                let found = self.keys[place.rem_euclid(count) as usize];
                BoxKey { time: found.time + length * place.div_euclid(count) as f32, ..found }
            }
            false => self.keys[place.clamp(0, count - 1) as usize],
        };
        let after = self.keys.partition_point(|key| key.time <= time) as isize;
        let (from, to) = (key(after - 1), key(after));
        let span = to.time - from.time;
        let t = if span > 1e-6 { ((time - from.time) / span).clamp(0.0, 1.0) } else { 0.0 };
        let found = match from.ease {
            Ease::Step => from,
            Ease::Linear => from.mix(&to, t),
            Ease::Smooth => {
                let (before, beyond) = (key(after - 2), key(after + 1));
                let curve =
                    |pick: fn(&BoxKey) -> Vec3| through(pick(&before), pick(&from), pick(&to), pick(&beyond), t);
                BoxKey { rotation: curve(|key| key.rotation), offset: curve(|key| key.offset), ..from.mix(&to, t) }
            }
        };
        BoxKey { time, ..found }
    }
}

/// The point `t` of the way from `b` to `c` on the curve through all four.
fn through(a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: f32) -> Vec3 {
    let (t2, t3) = (t * t, t * t * t);
    ((b * 2.0) + (c - a) * t + (a * 2.0 - b * 5.0 + c * 4.0 - d) * t2 + (b * 3.0 - a - c * 3.0 + d) * t3) * 0.5
}

/// A clip: the bones it moves, each with its keys.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxClip {
    pub name: String,
    /// How long it lasts, in seconds.
    pub length: f32,
    /// Whether it goes round from its end to its start.
    pub looping: bool,
    pub tracks: Vec<BoxTrack>,
}

impl BoxClip {
    /// A clip a second long that loops, with no keys.
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), length: 1.0, looping: true, tracks: Vec::new() }
    }

    /// `time` as a moment of the clip: round its loop, or held at its ends.
    /// The end of a loop is its own moment, which shows as its start does
    /// unless a key stands there.
    pub fn within(&self, time: f32) -> f32 {
        let length = self.length.max(0.0);
        match self.looping && length > 0.0 && !(0.0..=length).contains(&time) {
            true => time.rem_euclid(length),
            false => time.clamp(0.0, length),
        }
    }

    pub fn track(&self, bone: &str) -> Option<&BoxTrack> {
        self.tracks.iter().find(|track| track.bone == bone)
    }

    /// The bone's key that stands on the moment nearest `time`.
    pub fn key(&self, bone: &str, time: f32) -> Option<&BoxKey> {
        self.track(bone)?.keys.iter().find(|key| same_moment(key.time, time))
    }

    /// How the bone stands at `time`: as its keys either side have it, or
    /// at rest with none.
    pub fn value(&self, bone: &str, time: f32) -> BoxKey {
        let time = self.within(time);
        self.track(bone).map_or(BoxKey::at_rest(time), |track| track.at(time, self.length, self.looping))
    }

    /// Gives the bone a key, in place of one it has at that moment.
    pub fn set_key(&mut self, bone: &str, key: BoxKey) {
        let key = BoxKey { time: on_step(key.time.clamp(0.0, self.length.max(0.0))), ..key };
        let track = match self.tracks.iter().position(|track| track.bone == bone) {
            Some(at) => &mut self.tracks[at],
            None => {
                self.tracks.push(BoxTrack { bone: bone.to_string(), keys: Vec::new() });
                self.tracks.last_mut().expect("just added")
            }
        };
        match track.keys.iter().position(|other| same_moment(other.time, key.time)) {
            Some(at) => track.keys[at] = key,
            None => {
                let at = track.keys.partition_point(|other| other.time < key.time);
                track.keys.insert(at, key);
            }
        }
    }

    /// Takes the bone's key at `time` away, and returns it. A bone left
    /// with no keys is no longer moved by the clip.
    pub fn remove_key(&mut self, bone: &str, time: f32) -> Option<BoxKey> {
        let track = self.tracks.iter_mut().find(|track| track.bone == bone)?;
        let at = track.keys.iter().position(|key| same_moment(key.time, time))?;
        let removed = track.keys.remove(at);
        self.tracks.retain(|track| !track.keys.is_empty());
        Some(removed)
    }

    /// Moves the bone's key at `from` to `to`, over any key it has there.
    /// False where it has no key at `from`.
    pub fn move_key(&mut self, bone: &str, from: f32, to: f32) -> bool {
        let Some(key) = self.remove_key(bone, from) else { return false };
        self.set_key(bone, BoxKey { time: to, ..key });
        true
    }

    /// Makes the clip `length` seconds long. Keys past its new end are
    /// taken away.
    pub fn set_length(&mut self, length: f32) {
        self.length = on_step(length.clamp(KEY_STEP, LONGEST_CLIP));
        for track in &mut self.tracks {
            track.keys.retain(|key| key.time <= self.length + KEY_STEP * 0.5);
        }
        self.tracks.retain(|track| !track.keys.is_empty());
    }
}

fn same_moment(a: f32, b: f32) -> bool {
    (a - b).abs() < KEY_STEP * 0.5
}

impl BoxModel {
    /// Adds a clip with no keys, named as `wanted` or near it where a clip
    /// has that name, and returns its place among the clips.
    pub fn add_clip(&mut self, wanted: &str) -> usize {
        let taken = |name: &str| self.clips.iter().any(|clip| clip.name == name);
        let name = match taken(wanted) {
            false => wanted.to_string(),
            true => (2..).map(|count| format!("{wanted}{count}")).find(|name| !taken(name)).expect("one is free"),
        };
        self.clips.push(BoxClip::new(&name));
        self.clips.len() - 1
    }

    /// Names a clip, as `wanted` or near it where another has that name.
    pub fn rename_clip(&mut self, clip: usize, wanted: &str) {
        let taken = |name: &str| self.clips.iter().enumerate().any(|(at, other)| at != clip && other.name == name);
        let wanted = if wanted.trim().is_empty() { "Clip" } else { wanted.trim() };
        let name = match taken(wanted) {
            false => wanted.to_string(),
            true => (2..).map(|count| format!("{wanted}{count}")).find(|name| !taken(name)).expect("one is free"),
        };
        if let Some(clip) = self.clips.get_mut(clip) {
            clip.name = name;
        }
    }

    /// Names a bone, as `wanted` or near it where another has that name,
    /// and its keys in every clip go with it.
    pub fn rename_bone(&mut self, bone: usize, wanted: &str) {
        let Some(was) = self.bones.get(bone).map(|bone| bone.name.clone()) else { return };
        self.bones[bone].name.clear();
        let name = self.unique_name(wanted);
        for track in self.clips.iter_mut().flat_map(|clip| &mut clip.tracks).filter(|track| track.bone == was) {
            track.bone = name.clone();
        }
        self.bones[bone].name = name;
    }

    /// Every bone's pose `time` seconds into a clip, from its parent, in
    /// metres: where it stands at rest, turned, moved and sized as its
    /// keys say.
    pub fn clip_pose(&self, clip: &BoxClip, time: f32) -> Vec<BonePose> {
        let size = self.pixel_size();
        let bones = self.bones.iter().map(|bone| {
            let key = clip.value(&bone.name, time);
            BonePose {
                translation: (bone.pivot + key.offset) * size,
                rotation: (turned(bone.rotation) * turned(key.rotation)).normalize(),
                scale: key.scale,
            }
        });
        bones.collect()
    }

    /// Where each bone is in the model's space `time` seconds into a clip,
    /// in metres, as [`mesh_posed`](Self::mesh_posed) takes them.
    pub fn clip_matrices(&self, clip: &BoxClip, time: f32) -> Vec<Mat4> {
        self.skeleton().model_matrices(&self.clip_pose(clip, time))
    }

    /// A clip as what plays it wants it: every bone's pose at every frame.
    /// One that loops ends as it starts.
    pub fn baked(&self, clip: &BoxClip) -> Clip {
        let frames = (clip.length.clamp(0.0, LONGEST_CLIP) * CLIP_RATE).round() as usize + 1;
        let mut poses = Vec::with_capacity(frames * self.bones.len());
        for frame in 0..frames {
            // The last frame of a loop is its first again.
            let last = frame + 1 == frames && frames > 1;
            let time = if last && clip.looping { 0.0 } else { frame as f32 / CLIP_RATE };
            poses.extend(self.clip_pose(clip, time.min(clip.length)));
        }
        Clip { name: clip.name.clone(), frames, poses }
    }
}

/// A clip as the file has it.
#[derive(Serialize, Deserialize)]
pub(super) struct FileClip {
    name: String,
    length: f64,
    #[serde(default, skip_serializing_if = "super::is_false")]
    looping: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bones: Vec<FileTrack>,
}

/// A bone's keys as the file has them: their times, and beside those what
/// each says, left out where no key says anything of it.
#[derive(Serialize, Deserialize)]
struct FileTrack {
    bone: String,
    times: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rotation: Vec<[f64; 3]>,
    #[serde(default, rename = "move", skip_serializing_if = "Vec::is_empty")]
    offset: Vec<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    scale: Vec<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ease: Vec<Ease>,
}

/// A number as the file has it: to a thousandth, so 0.4 is written as 0.4.
fn written(value: f32) -> f64 {
    // Nothing less than nothing: -0 is written as 0.
    (f64::from(value) * 1000.0).round() / 1000.0 + 0.0
}

fn written_all(value: Vec3) -> [f64; 3] {
    value.to_array().map(written)
}

/// The model's clips as its file has them, without keys for bones it no
/// longer has.
pub(super) fn write(model: &BoxModel) -> Vec<FileClip> {
    let clips = model.clips.iter().map(|clip| {
        let kept = clip.tracks.iter().filter(|track| model.find_bone(&track.bone).is_some() && !track.keys.is_empty());
        let bones = kept.map(|track| {
            let said = |all: Vec<[f64; 3]>, nothing: [f64; 3]| {
                if all.iter().all(|value| *value == nothing) { Vec::new() } else { all }
            };
            let ease: Vec<Ease> = track.keys.iter().map(|key| key.ease).collect();
            FileTrack {
                bone: track.bone.clone(),
                times: track.keys.iter().map(|key| written(key.time)).collect(),
                rotation: said(track.keys.iter().map(|key| written_all(key.rotation)).collect(), [0.0; 3]),
                offset: said(track.keys.iter().map(|key| written_all(key.offset)).collect(), [0.0; 3]),
                scale: said(track.keys.iter().map(|key| written_all(key.scale)).collect(), [1.0; 3]),
                ease: if ease.iter().all(|ease| *ease == Ease::Linear) { Vec::new() } else { ease },
            }
        });
        let length = written(clip.length);
        FileClip { name: clip.name.clone(), length, looping: clip.looping, bones: bones.collect() }
    });
    clips.collect()
}

/// The clips a file has, for a model with these bones.
pub(super) fn read(clips: Vec<FileClip>, model: &BoxModel) -> Result<Vec<BoxClip>> {
    let mut read: Vec<BoxClip> = Vec::with_capacity(clips.len());
    for clip in clips {
        ensure!(!clip.name.is_empty(), "a clip with no name");
        ensure!(read.iter().all(|other| other.name != clip.name), "two clips named {}", clip.name);
        ensure!(
            clip.length > 0.0 && clip.length <= f64::from(LONGEST_CLIP),
            "{} is {} seconds long: a clip is more than none and {LONGEST_CLIP} at most",
            clip.name,
            clip.length
        );
        let length = clip.length as f32;
        let mut made = BoxClip { name: clip.name, length, looping: clip.looping, tracks: Vec::new() };
        for track in clip.bones {
            let (name, count) = (made.name.clone(), track.times.len());
            ensure!(model.find_bone(&track.bone).is_some(), "{name} moves {}, which is not a bone", track.bone);
            ensure!(made.track(&track.bone).is_none(), "{name} has {}'s keys twice", track.bone);
            let fits = |said: usize| said == 0 || said == count;
            ensure!(
                fits(track.rotation.len())
                    && fits(track.offset.len())
                    && fits(track.scale.len())
                    && fits(track.ease.len()),
                "{name} says something of {}'s keys other than once for each of the {count}",
                track.bone
            );
            for (at, &time) in track.times.iter().enumerate() {
                let time = time as f32;
                ensure!((0.0..=made.length).contains(&time), "{name} has a key at {time} seconds, outside it");
                let said = |all: &Vec<[f64; 3]>, nothing: Vec3| {
                    all.get(at).map_or(nothing, |value| Vec3::from(value.map(|part| part as f32)))
                };
                let key = BoxKey {
                    time,
                    rotation: said(&track.rotation, Vec3::ZERO),
                    offset: said(&track.offset, Vec3::ZERO),
                    scale: said(&track.scale, Vec3::ONE),
                    ease: track.ease.get(at).copied().unwrap_or_default(),
                };
                made.set_key(&track.bone, key);
            }
        }
        read.push(made);
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::box_model::BoxBone;

    /// A body with an arm hanging from its shoulder.
    fn armed() -> BoxModel {
        let mut model = BoxModel::default();
        let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
        model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [0, 0]);
        let arm = model.add_bone("Arm", Some(body), Vec3::new(6.0, 10.0, 0.0));
        model.add_box(arm, Vec3::new(-2.0, -10.0, -2.0), [4, 12, 4], [0, 16]);
        model
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn a_bone_goes_from_a_key_to_the_next_as_the_earlier_one_says() {
        let mut clip = BoxClip { looping: false, ..BoxClip::new("Wave") };
        assert!(near(clip.value("Arm", 0.3).rotation, Vec3::ZERO), "no keys: at rest");
        assert_eq!(clip.value("Arm", 0.3).scale, Vec3::ONE);
        clip.set_key("Arm", BoxKey::turned(0.2, Vec3::new(0.0, 0.0, 40.0)));
        assert!(near(clip.value("Arm", 0.9).rotation, Vec3::new(0.0, 0.0, 40.0)), "one key holds throughout");
        clip.set_key("Arm", BoxKey::turned(0.6, Vec3::new(0.0, 0.0, 80.0)));
        assert!(near(clip.value("Arm", 0.0).rotation, Vec3::new(0.0, 0.0, 40.0)), "before the first key, the first");
        assert!(near(clip.value("Arm", 0.4).rotation, Vec3::new(0.0, 0.0, 60.0)), "halfway between two");
        assert!(near(clip.value("Arm", 1.0).rotation, Vec3::new(0.0, 0.0, 80.0)), "past the last key, the last");

        // Turned by a handle, a key turns about the bone's own axis as it then stands.
        let mut key = BoxKey::turned(0.0, Vec3::new(0.0, 90.0, 0.0));
        key.turn_about(0, 30.0);
        assert!(turned(key.rotation).angle_between(turned(Vec3::new(30.0, 90.0, 0.0))) < 1e-3, "{:?}", key.rotation);
        key.turn_about(2, 45.0);
        let wanted = turned(Vec3::new(30.0, 90.0, 0.0)) * Quat::from_rotation_z(45f32.to_radians());
        assert!(turned(key.rotation).angle_between(wanted) < 1e-3, "{:?}", key.rotation);

        // A key that steps holds until the next.
        clip.set_key("Arm", BoxKey { ease: Ease::Step, ..BoxKey::turned(0.2, Vec3::new(0.0, 0.0, 40.0)) });
        assert!(near(clip.value("Arm", 0.55).rotation, Vec3::new(0.0, 0.0, 40.0)));
        assert!(near(clip.value("Arm", 0.6).rotation, Vec3::new(0.0, 0.0, 80.0)));

        // A smooth one passes through its keys, and between them bends toward where it is going next.
        let mut clip = BoxClip { looping: false, ..BoxClip::new("Swing") };
        for (time, turn) in [(0.0, 0.0), (0.2, 10.0), (0.4, 40.0), (0.6, 50.0)] {
            clip.set_key("Arm", BoxKey { ease: Ease::Smooth, ..BoxKey::turned(time, Vec3::X * turn) });
        }
        assert!(near(clip.value("Arm", 0.2).rotation, Vec3::X * 10.0));
        assert!(near(clip.value("Arm", 0.4).rotation, Vec3::X * 40.0));
        let middle = clip.value("Arm", 0.3).rotation.x;
        assert!((middle - 25.0).abs() < 0.01, "even either side, the curve's middle is the line's: {middle}");
        let early = clip.value("Arm", 0.25).rotation.x;
        assert!(early < 17.5, "slower leaving the key it came to slowly: {early}");
    }

    #[test]
    fn a_clip_that_loops_goes_from_its_last_key_round_to_its_first() {
        let mut clip = BoxClip::new("Walk");
        clip.set_key("Arm", BoxKey::turned(0.0, Vec3::X * 30.0));
        clip.set_key("Arm", BoxKey::turned(0.5, Vec3::X * -30.0));
        assert!(near(clip.value("Arm", 0.25).rotation, Vec3::ZERO));
        assert!(near(clip.value("Arm", 0.75).rotation, Vec3::ZERO), "halfway back to the first key");
        assert!(near(clip.value("Arm", 1.25).rotation, Vec3::ZERO), "and round again");
        assert!(near(clip.value("Arm", 1.0).rotation, Vec3::X * 30.0));
        // Held at its ends, the same keys stay at the last.
        clip.looping = false;
        assert!(near(clip.value("Arm", 0.75).rotation, Vec3::X * -30.0));
    }

    #[test]
    fn keys_stand_on_steps_in_order_and_one_takes_anothers_place() {
        let mut clip = BoxClip::new("Wave");
        clip.set_key("Arm", BoxKey::turned(0.52, Vec3::X));
        clip.set_key("Arm", BoxKey::turned(0.11, Vec3::Y));
        clip.set_key("Arm", BoxKey::turned(3.0, Vec3::Z));
        let times: Vec<f32> = clip.track("Arm").unwrap().keys.iter().map(|key| key.time).collect();
        assert_eq!(times, [0.1, 0.5, 1.0], "on steps, in order, and none past the end");
        clip.set_key("Arm", BoxKey::turned(0.49, Vec3::NEG_X));
        assert_eq!(clip.track("Arm").unwrap().keys.len(), 3, "a key at a moment that has one takes its place");
        assert_eq!(clip.key("Arm", 0.5).unwrap().rotation, Vec3::NEG_X);

        assert!(clip.move_key("Arm", 0.5, 0.1), "moved onto another");
        assert_eq!(clip.track("Arm").unwrap().keys.len(), 2);
        assert_eq!(clip.key("Arm", 0.1).unwrap().rotation, Vec3::NEG_X);
        assert!(!clip.move_key("Arm", 0.5, 0.7), "no key there now");

        clip.set_length(0.5);
        assert_eq!(clip.track("Arm").unwrap().keys.len(), 1, "the key past the new end is gone");
        assert!(clip.remove_key("Arm", 0.1).is_some());
        assert!(clip.tracks.is_empty(), "a bone with no keys is not moved");
    }

    #[test]
    fn a_clip_poses_the_bones_it_keys_and_leaves_the_rest_as_they_stand() {
        let mut model = armed();
        model.bones[1].rotation = Vec3::new(0.0, 0.0, 10.0);
        let wave = model.add_clip("Wave");
        model.clips[wave].set_key("Arm", BoxKey::turned(0.0, Vec3::ZERO));
        let raised = BoxKey { offset: Vec3::new(0.0, 2.0, 0.0), ..BoxKey::turned(0.5, Vec3::new(0.0, 0.0, 80.0)) };
        model.clips[wave].set_key("Arm", raised);

        let rest = model.skeleton().rest();
        let pose = model.clip_pose(&model.clips[wave], 0.5);
        assert_eq!(pose[0], rest[0], "the body has no keys");
        assert!(near(pose[1].translation, rest[1].translation + Vec3::Y * 2.0 / 16.0), "moved in pixels");
        assert!(pose[1].rotation.angle_between(turned(Vec3::new(0.0, 0.0, 90.0))) < 1e-3, "turned on from rest");
        let start = model.clip_pose(&model.clips[wave], 0.0)[1];
        assert!(near(start.translation, rest[1].translation) && start.rotation.angle_between(rest[1].rotation) < 1e-4);

        // Every frame of it, for what plays it; a loop's last is its first again.
        let baked = model.baked(&model.clips[wave]);
        assert_eq!((baked.name.as_str(), baked.frames, baked.poses.len()), ("Wave", 61, 61 * 2));
        assert!(baked.frame(30)[1].rotation.angle_between(pose[1].rotation) < 1e-3);
        assert_eq!(baked.frame(60), baked.frame(0));
        let prepared = model.prepared("Models/Armed.boxmodel");
        assert_eq!(prepared.animation.unwrap().clips, vec![baked]);
    }

    #[test]
    fn clips_are_kept_in_the_file_small_and_follow_a_bone_renamed() {
        let mut model = armed();
        let wave = model.add_clip("Wave");
        model.clips[wave].length = 0.8;
        model.clips[wave].set_key("Arm", BoxKey::turned(0.0, Vec3::ZERO));
        model.clips[wave].set_key("Arm", BoxKey { ease: Ease::Smooth, ..BoxKey::turned(0.4, Vec3::Z * -120.0) });
        let nod = model.add_clip("Wave");
        assert_eq!(model.clips[nod].name, "Wave2", "no two clips of a name");
        model.clips[nod].looping = false;
        model.clips[nod].set_key("Body", BoxKey { scale: Vec3::new(1.0, 0.9, 1.0), ..BoxKey::at_rest(0.5) });

        let text = model.to_text();
        assert!(text.contains("times = [0.0, 0.4]"), "{text}");
        assert!(text.contains("rotation = [[0.0, 0.0, 0.0], [0.0, 0.0, -120.0]]"), "{text}");
        assert!(text.contains("ease = [\"linear\", \"smooth\"]"), "{text}");
        assert!(!text.contains("move ="), "nothing said of what no key changes: {text}");
        assert_eq!(BoxModel::parse(&text).unwrap(), model);

        model.rename_bone(1, "RightArm");
        assert_eq!(model.clips[wave].tracks[0].bone, "RightArm");
        model.rename_clip(nod, "Wave");
        assert_eq!(model.clips[nod].name, "Wave2", "nor by renaming");
        model.rename_clip(nod, "Nod");
        assert_eq!(model.clips[nod].name, "Nod");

        // A bone taken out takes its keys out of the file with it.
        model.bones.push(BoxBone::new("Hat", Some(0), Vec3::ZERO));
        model.clips[wave].set_key("Hat", BoxKey::turned(0.2, Vec3::Y));
        model.remove_bone(2);
        let read = BoxModel::parse(&model.to_text()).unwrap();
        assert!(read.clips[wave].track("Hat").is_none());
        assert_eq!(read.clips[wave].track("RightArm"), model.clips[wave].track("RightArm"));
    }

    #[test]
    fn a_file_whose_clips_do_not_fit_its_bones_says_what_is_wrong() {
        let model = armed();
        let with = |clips: &str| BoxModel::parse(&(model.to_text() + clips)).map(|_| ()).map_err(|e| format!("{e:#}"));
        let wave = "\n[[clips]]\nname = \"Wave\"\nlength = 1.0\n";
        assert_eq!(with(wave), Ok(()));
        let error = with(&format!("{wave}[[clips.bones]]\nbone = \"Wing\"\ntimes = [0.0]\n")).unwrap_err();
        assert!(error.contains("Wing, which is not a bone"), "{error}");
        let error =
            with(&format!("{wave}[[clips.bones]]\nbone = \"Arm\"\ntimes = [0.0, 0.5]\nrotation = [[0, 0, 9]]\n"));
        assert!(error.unwrap_err().contains("once for each of the 2"));
        let error = with(&format!("{wave}[[clips.bones]]\nbone = \"Arm\"\ntimes = [1.5]\n")).unwrap_err();
        assert!(error.contains("outside it"), "{error}");
        assert!(with(&format!("{wave}{wave}")).unwrap_err().contains("two clips named Wave"));
        assert!(with("\n[[clips]]\nname = \"Long\"\nlength = 0.0\n").unwrap_err().contains("seconds long"));
    }
}
