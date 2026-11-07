//! A gun's handling: how it is held and how its actions go, as a designer
//! sets them in the Gun Editor, saved in a `.gunhandling` file the Gun
//! picks. Its poses are offsets on how every gun is held. Its built-in
//! actions are offsets on the gun kind's templates, finding their beats by
//! key, so the templates can improve under them. And it can have actions
//! of its own, one of which may take a built-in's place: an editable copy
//! of a built-in is one.
//!
//! ```toml
//! kind = "bolt_action_rifle"
//! [poses]
//! hip = { at = [0.01, -0.02, 0.0], turn = [0.0, 0.0, -3.0] }
//!
//! [[builtin]]
//! action = "reload"
//! speed = 0.9
//! [[builtin.beat]]
//! beat = "press"
//! pace = 1.25
//! tilt = [-4.0, 0.0, 2.0]
//! ```

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat4, Quat, Vec3};
use voxear_ecs::World;

use super::actions::{ActionKind, ActionSpec, Beat, BeatKey, When};
use super::gun::{Gun, GunKind, Socket};
use super::hands::Grip;
use crate::content::Content;

/// A place and turn: metres, and degrees about X, then Y, then Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Place {
    pub at: [f32; 3],
    pub turn: [f32; 3],
}

impl Place {
    pub fn is_zero(&self) -> bool {
        *self == Place::default()
    }

    pub fn rotation(&self) -> Quat {
        let [x, y, z] = self.turn.map(f32::to_radians);
        Quat::from_euler(EulerRot::XYZ, x, y, z)
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation(), Vec3::from(self.at))
    }
}

/// Offsets on how the gun is held, all nothing by default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Poses {
    /// On the hold at the hip, from the eyes.
    pub hip: Place,
    /// On where a long gun's butt sits from the shooting shoulder, in the
    /// body's space.
    pub shoulder: [f32; 3],
    /// On the hold at the eye, in the sights' own space, and how far it
    /// cants, in degrees, its top to the right.
    pub aim: Place,
    pub cant: f32,
    /// On where actions bring the gun before the eyes.
    pub work: Place,
    /// The grips the hands hold the gun with at rest, instead of the usual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest_grip: Option<Grip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest_support: Option<Grip>,
    /// How quickly a hand goes back to its hold after an action, 1 as usual.
    pub return_pace: f32,
    /// On each pouch, left then right, in the body's space.
    pub pouches: [[f32; 3]; 2],
    /// On where the hands hold at rest, on the grip and under the
    /// fore-end, in the gun's space.
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub grip: Place,
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub support: Place,
}

impl Poses {
    /// `gun` with its hands' sockets moved by these poses.
    pub fn held(&self, gun: &Gun) -> Gun {
        let moved = |socket: Socket, by: Place| {
            let add = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| a[i] + b[i]);
            Socket { position: add(socket.position, by.at), rotation: add(socket.rotation, by.turn) }
        };
        Gun { grip: moved(gun.grip, self.grip), support: moved(gun.support, self.support), ..gun.clone() }
    }
}

impl Default for Poses {
    fn default() -> Self {
        Self {
            hip: Place::default(),
            shoulder: [0.0; 3],
            aim: Place::default(),
            cant: 0.0,
            work: Place::default(),
            rest_grip: None,
            rest_support: None,
            return_pace: 1.0,
            pouches: [[0.0; 3]; 2],
            grip: Place::default(),
            support: Place::default(),
        }
    }
}

/// Offsets on one of the gun kind's built-in actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionOffsets {
    pub action: ActionKind,
    /// How fast it plays, 1 as built.
    pub speed: f32,
    /// How far it lifts the gun before the eyes, instead of as built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lift: Option<f32>,
    /// On where it brings the gun before the eyes.
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub work: Place,
    #[serde(rename = "beat", skip_serializing_if = "Vec::is_empty")]
    pub beats: Vec<BeatOffset>,
}

impl Default for ActionOffsets {
    fn default() -> Self {
        Self { action: ActionKind::Reload, speed: 1.0, lift: None, work: Place::default(), beats: Vec::new() }
    }
}

/// Offsets on the beats of a built-in action that have one key: every one
/// of them, each time it repeats.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BeatOffset {
    pub beat: BeatSlot,
    /// How fast its reach and work go, 1 as built, and how much later it
    /// starts, in seconds.
    pub pace: f32,
    pub delay: f32,
    /// Where its hand holds, from where it would, in the gun's space, and
    /// how much more the hand is turned, in degrees.
    pub at: [f32; 3],
    pub turn: [f32; 3],
    /// Added to how the gun is tilted for it.
    pub tilt: [f32; 3],
    /// Its grip, instead of as built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grip: Option<Grip>,
}

impl Default for BeatOffset {
    fn default() -> Self {
        Self {
            beat: BeatSlot::Known(BeatKey::Wait),
            pace: 1.0,
            delay: 0.0,
            at: [0.0; 3],
            turn: [0.0; 3],
            tilt: [0.0; 3],
            grip: None,
        }
    }
}

/// The beat an offset is for: by its key, or by a key this engine no
/// longer knows, kept as written so that saving loses nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BeatSlot {
    Known(BeatKey),
    Retired(String),
}

/// An action of the gun's own: the name its scripts and the editor know
/// it by, the built-in action it takes the place of, if any, and the action
/// itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomAction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces: Option<ActionKind>,
    #[serde(skip_serializing_if = "When::is_always")]
    pub when: When,
    pub lift: f32,
    pub lowers_aim: bool,
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub work: Place,
    #[serde(rename = "beat")]
    pub beats: Vec<Beat>,
}

impl Default for CustomAction {
    fn default() -> Self {
        let spec = ActionSpec::default();
        Self {
            name: String::new(),
            replaces: None,
            when: spec.when,
            lift: spec.lift,
            lowers_aim: spec.lowers_aim,
            work: Place::default(),
            beats: Vec::new(),
        }
    }
}

impl CustomAction {
    pub fn spec(&self) -> ActionSpec {
        ActionSpec {
            when: self.when,
            lift: self.lift,
            lowers_aim: self.lowers_aim,
            work: self.work,
            beats: self.beats.clone(),
        }
    }
}

/// How a gun is held and how its actions go.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GunHandling {
    /// The kind of gun it was made for, whose built-in actions it offsets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GunKind>,
    pub poses: Poses,
    #[serde(rename = "builtin", skip_serializing_if = "Vec::is_empty")]
    pub offsets: Vec<ActionOffsets>,
    #[serde(rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<CustomAction>,
}

impl GunHandling {
    /// What plays for a built-in action on a gun of `kind`: an action of
    /// the gun's own that takes its place, or else the kind's template with
    /// this handling's offsets on it.
    pub fn spec(&self, kind: GunKind, action: ActionKind) -> ActionSpec {
        if let Some(own) = self.actions.iter().find(|own| own.replaces == Some(action)) {
            return own.spec();
        }
        let mut spec = super::templates::template(kind, action);
        if let Some(offsets) = self.offsets(action) {
            offset(&mut spec, offsets);
        }
        spec
    }

    /// The offsets on a built-in action, if it has any.
    pub fn offsets(&self, action: ActionKind) -> Option<&ActionOffsets> {
        self.offsets.iter().find(|offsets| offsets.action == action)
    }

    /// An action of the gun's own, by its name.
    pub fn action(&self, name: &str) -> Option<&CustomAction> {
        self.actions.iter().find(|own| own.name == name)
    }

    /// A built-in action on a gun of `kind`, its offsets baked into its
    /// beats, as an action of the gun's own that takes its place: still
    /// playing beats only in the states they did, and repeating as they
    /// did.
    pub fn editable_copy(&self, kind: GunKind, action: ActionKind) -> CustomAction {
        let mut spec = super::templates::template(kind, action);
        if let Some(offsets) = self.offsets(action) {
            offset(&mut spec, offsets);
        }
        CustomAction {
            name: action.title().to_string(),
            replaces: Some(action),
            when: spec.when,
            lift: spec.lift,
            lowers_aim: spec.lowers_aim,
            work: spec.work,
            beats: spec.beats,
        }
    }

    /// Offsets on beats a built-in action of `kind` no longer has, or by
    /// keys this engine does not know: kept, but doing nothing.
    pub fn stale(&self, kind: GunKind) -> Vec<(ActionKind, BeatSlot)> {
        let mut stale = Vec::new();
        for offsets in &self.offsets {
            let template = super::templates::template(kind, offsets.action);
            for beat in &offsets.beats {
                let found = matches!(beat.beat, BeatSlot::Known(key) if template.beats.iter().any(|b| b.key == key));
                if !found {
                    stale.push((offsets.action, beat.beat.clone()));
                }
            }
        }
        stale
    }
}

/// Puts a built-in action's offsets on its spec.
fn offset(spec: &mut ActionSpec, offsets: &ActionOffsets) {
    let speed = offsets.speed.max(0.05);
    spec.lift = offsets.lift.unwrap_or(spec.lift);
    spec.work = offsets.work;
    for beat in &mut spec.beats {
        (beat.reach, beat.work) = (beat.reach / speed, beat.work / speed);
        let Some(by) = offsets.beats.iter().find(|by| by.beat == BeatSlot::Known(beat.key)) else { continue };
        let pace = by.pace.max(0.05);
        (beat.reach, beat.work) = ((beat.reach / pace + by.delay).max(0.0), beat.work / pace);
        let add = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| a[i] + b[i]);
        (beat.at, beat.turn, beat.tilt) = (add(beat.at, by.at), add(beat.turn, by.turn), add(beat.tilt, by.tilt));
        beat.grip = by.grip.unwrap_or(beat.grip);
    }
}

/// The handlings the world's guns pick, as the game resolved them; none
/// for one that could not be read.
#[derive(Default)]
pub struct Handlings {
    version: u64,
    found: HashMap<String, Option<Arc<GunHandling>>>,
}

/// Resolves the handlings the world's guns pick that it has not yet, and
/// every one again after one was saved.
pub fn give_handlings(world: &mut World, content: &mut Content) {
    let wanted: Vec<String> = world
        .query_ref::<&Gun>()
        .into_iter()
        .map(|gun| gun.handling.clone())
        .filter(|reference| !reference.is_empty())
        .collect();
    if wanted.is_empty() && world.resource::<Handlings>().is_none() {
        return;
    }
    if world.resource::<Handlings>().is_none() {
        world.insert_resource(Handlings::default());
    }
    let Some(handlings) = world.resource_mut::<Handlings>() else { return };
    if handlings.version != content.handlings_version() {
        handlings.found.clear();
        handlings.version = content.handlings_version();
    }
    for reference in wanted {
        handlings.found.entry(reference.clone()).or_insert_with(|| {
            let guid = content.parse(&reference).ok().and_then(|parsed| parsed.guid());
            guid.and_then(|guid| content.gun_handling(guid))
        });
    }
}

/// Gives guns that pick `reference` this handling, until one is saved: as
/// the Gun Editor shows its edits before they are saved.
pub fn put_handling(world: &mut World, reference: &str, handling: Arc<GunHandling>) {
    if world.resource::<Handlings>().is_none() {
        world.insert_resource(Handlings::default());
    }
    if let Some(handlings) = world.resource_mut::<Handlings>() {
        handlings.found.insert(reference.to_string(), Some(handling));
    }
}

/// The handling a gun picks, or how every gun is handled when it picks
/// none or its file could not be read.
pub fn handling_of(world: &World, gun: &Gun) -> Arc<GunHandling> {
    static USUAL: LazyLock<Arc<GunHandling>> = LazyLock::new(|| Arc::new(GunHandling::default()));
    if gun.handling.is_empty() {
        return USUAL.clone();
    }
    let found = world.resource::<Handlings>().and_then(|handlings| handlings.found.get(&gun.handling).cloned());
    found.flatten().unwrap_or_else(|| USUAL.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guns::actions::expand;
    use crate::guns::mechanism::GunState;

    fn empty_rifle() -> (GunKind, GunState) {
        let kind = GunKind::BoltActionRifle;
        let mut state = GunState::loaded(kind);
        while state.ready(kind) {
            state.shoot(kind);
            state.cycle();
        }
        (kind, state)
    }

    #[test]
    fn the_hands_hold_where_the_poses_move_the_sockets_and_nowhere_else_moves() {
        let gun = Gun::default();
        let poses = Poses {
            grip: Place { at: [0.0, 0.01, -0.02], turn: [0.0, 0.0, 5.0] },
            support: Place { at: [0.0, 0.0, 0.05], turn: [0.0; 3] },
            ..Poses::default()
        };
        let held = poses.held(&gun);
        let moved = |a: [f32; 3], b: [f32; 3], by: Vec3| (Vec3::from(b) - Vec3::from(a)).abs_diff_eq(by, 1e-6);
        assert!(moved(gun.grip.position, held.grip.position, Vec3::new(0.0, 0.01, -0.02)));
        assert!(moved(gun.grip.rotation, held.grip.rotation, Vec3::new(0.0, 0.0, 5.0)));
        assert!(moved(gun.support.position, held.support.position, Vec3::new(0.0, 0.0, 0.05)));
        assert_eq!(Gun { grip: gun.grip, support: gun.support, ..held }, gun, "nothing else changes");
        assert_eq!(Poses::default().held(&gun), gun);
    }

    #[test]
    fn no_offsets_play_the_template_and_offsets_change_only_their_beats() {
        let (kind, state) = empty_rifle();
        let usual = GunHandling::default();
        for action in ActionKind::ALL {
            assert_eq!(usual.spec(kind, action), super::super::templates::template(kind, action));
        }
        let text = r#"
            [[builtin]]
            action = "reload"
            speed = 0.5
            [[builtin.beat]]
            beat = "press"
            pace = 2.0
            delay = 0.1
            tilt = [-4.0, 0.0, 2.0]
            at = [0.0, 0.01, 0.0]
            grip = "open"
        "#;
        let handling: GunHandling = toml::from_str(text).unwrap();
        let (built, offset) = (
            expand(&usual.spec(kind, ActionKind::Reload), kind, &state, 20),
            expand(&handling.spec(kind, ActionKind::Reload), kind, &state, 20),
        );
        assert_eq!(built.len(), offset.len());
        for (built, offset) in built.iter().zip(&offset) {
            // Half the speed doubles every beat's time; the press then plays twice as fast, and starts later.
            let pace = if built.key == BeatKey::Press { 2.0 } else { 1.0 };
            let delay = if built.key == BeatKey::Press { 0.1 } else { 0.0 };
            assert!((offset.work - built.work * 2.0 / pace).abs() < 1e-5, "{:?}", built.key);
            assert!((offset.reach - (built.reach * 2.0 / pace + delay)).abs() < 1e-5, "{:?}", built.key);
            if built.key == BeatKey::Press {
                assert_eq!((offset.tilt[0], offset.at[1], offset.grip), (built.tilt[0] - 4.0, 0.01, Grip::OPEN));
            } else {
                assert_eq!((offset.tilt, offset.at, offset.grip), (built.tilt, built.at, built.grip));
            }
        }
        // Every press of every clip has the offset, and the other actions none.
        assert_eq!(handling.spec(kind, ActionKind::Cycle), usual.spec(kind, ActionKind::Cycle));
    }

    #[test]
    fn an_offset_by_a_key_the_engine_does_not_know_is_kept_and_listed() {
        let text = "[[builtin]]\naction = \"reload\"\n[[builtin.beat]]\nbeat = \"twirl\"\npace = 2.0\n";
        let handling: GunHandling = toml::from_str(text).unwrap();
        let (kind, _) = empty_rifle();
        assert_eq!(handling.stale(kind), [(ActionKind::Reload, BeatSlot::Retired("twirl".into()))]);
        let saved = toml::to_string(&handling).unwrap();
        assert!(saved.contains("beat = \"twirl\""), "{saved}");
        assert_eq!(
            handling.spec(kind, ActionKind::Reload),
            super::super::templates::template(kind, ActionKind::Reload)
        );
    }

    #[test]
    fn an_editable_copy_plays_as_the_offset_built_in_did_and_takes_its_place() {
        let (kind, state) = empty_rifle();
        let mut handling = GunHandling::default();
        handling.offsets.push(ActionOffsets {
            speed: 0.8,
            beats: vec![BeatOffset {
                beat: BeatSlot::Known(BeatKey::Pull),
                tilt: [5.0, 0.0, 0.0],
                ..BeatOffset::default()
            }],
            ..ActionOffsets::default()
        });
        let offset = expand(&handling.spec(kind, ActionKind::Reload), kind, &state, 20);
        let copy = handling.editable_copy(kind, ActionKind::Reload);
        handling.actions.push(copy);
        handling.offsets.clear();
        assert_eq!(expand(&handling.spec(kind, ActionKind::Reload), kind, &state, 20), offset);
        let read: GunHandling = toml::from_str(&toml::to_string(&handling).unwrap()).unwrap();
        assert_eq!(read, handling);
    }
}
