//! A gun's handling: how it is held and how its actions go, as a designer
//! sets them in its Inspector, saved in a `.gunhandling` file the Gun
//! picks. Its poses are offsets on how every gun is held. Its built-in
//! actions are offsets on the gun kind's templates, finding their beats by
//! key, so the templates can improve under them. And it can have actions
//! of its own, one of which may take a built-in's place: an editable copy
//! of a built-in is one. For a gun from an animation pack, its clips hold
//! the gun, and any action can be played from clips, part by part.
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

use super::actions::{ActionKind, ActionSpec, Beat, BeatKey, ClipPart, When};
use super::base_clip::BaseClip;
use super::gun::{Gun, GunKind, Socket};
use crate::animation::grip::Grip;
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
    /// The clip it is based on, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<BaseClip>,
    /// The parts it plays from clips in place of its beats, if any.
    #[serde(rename = "part", skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ClipPart>,
}

impl Default for ActionOffsets {
    fn default() -> Self {
        let work = Place::default();
        let (beats, parts) = (Vec::new(), Vec::new());
        Self { action: ActionKind::Reload, speed: 1.0, lift: None, work, beats, clip: None, parts }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<BaseClip>,
    /// The parts it plays from clips in place of its beats, if any.
    #[serde(rename = "part", skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ClipPart>,
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
            clip: None,
            parts: Vec::new(),
        }
    }
}

impl CustomAction {
    pub fn spec(&self) -> ActionSpec {
        let mut spec = ActionSpec {
            when: self.when,
            lift: self.lift,
            lowers_aim: self.lowers_aim,
            work: self.work,
            beats: self.beats.clone(),
            clip: self.clip.clone(),
            ..ActionSpec::default()
        };
        if !self.parts.is_empty() {
            spec.play(&self.parts, 1.0);
        }
        spec
    }
}

/// The clips a gun from an animation pack is held by: the bone of the arms
/// clips' skeleton that carries the gun, and where the gun sits from it;
/// where the eyes the clips were made for are from their head, in metres,
/// as the holder's eyes are from its own: none for clips framed for a
/// camera at the head itself;
/// the gun's own clip it stands in between actions; the arms clips it is
/// held by as its holder stands, walks, runs and sprints, blended by how
/// fast it goes, and those that start a run and a sprint from standing or
/// walking; the arms clips a shot plays, from the hip and aimed, and
/// the gun's own clip for it; how long a clip takes to come in and go, and
/// how long at least a hand takes back to its hold as an action ends, in
/// seconds; where the camera looks from while they hold it; and how much
/// of each layer the holder puts on top of them: the gun's sway as the view
/// turns, its lag as the holder moves, its bob with the steps and its
/// breathing, 1 as for any gun and 0 none, and the camera's own bob with
/// the steps. Each clip is a reference, empty for none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PackClips {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub carrier: String,
    #[serde(skip_serializing_if = "Place::is_zero")]
    pub carrier_offset: Place,
    #[serde(skip_serializing_if = "is_origin")]
    pub eyes: [f32; 3],
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rest: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub idle: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub walk: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run_start: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sprint: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sprint_start: String,
    pub walk_speed: f32,
    pub run_speed: f32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fire: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub aimed_fire: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gun_fire: String,
    pub blend: f32,
    pub hand_return: f32,
    #[serde(skip_serializing_if = "PackCamera::is_filmed")]
    pub camera: PackCamera,
    pub look_sway: f32,
    pub move_sway: f32,
    pub bob: f32,
    pub breathing: f32,
    pub head_bob: f32,
}

/// Where the camera looks from while clips hold the gun.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackCamera {
    /// Where the clips were filmed from, as far from the holder's
    /// shoulders as it was from the clip's: the arms hold the gun as the
    /// clips have it, and it shows as they show it.
    #[default]
    Filmed,
    /// At the holder's eyes. The arms hold the gun as the clips have it
    /// as far as they reach it from there, and it shows lower.
    Eyes,
}

impl PackCamera {
    fn is_filmed(&self) -> bool {
        *self == PackCamera::Filmed
    }
}

impl Default for PackClips {
    fn default() -> Self {
        Self {
            carrier: String::new(),
            carrier_offset: Place::default(),
            eyes: [0.0; 3],
            rest: String::new(),
            idle: String::new(),
            walk: String::new(),
            run: String::new(),
            run_start: String::new(),
            sprint: String::new(),
            sprint_start: String::new(),
            walk_speed: 1.4,
            run_speed: 4.0,
            fire: String::new(),
            aimed_fire: String::new(),
            gun_fire: String::new(),
            blend: 0.2,
            hand_return: 0.35,
            camera: PackCamera::Filmed,
            // Packs film their clips from a camera held still, the gun all but still in it as the arms walk: what
            // they leave out is put back lightly, and the breathing their idle clips have of their own not at all.
            look_sway: 0.3,
            move_sway: 0.3,
            bob: 0.3,
            breathing: 0.0,
            head_bob: 0.0,
        }
    }
}

fn is_origin(at: &[f32; 3]) -> bool {
    *at == [0.0; 3]
}

impl PackClips {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether arms clips hold the gun: with none, it is held as every gun is.
    pub fn holds(&self) -> bool {
        [&self.idle, &self.walk, &self.run, &self.sprint].iter().any(|clip| !clip.is_empty())
    }

    /// Every clip it names.
    pub fn clips(&self) -> impl Iterator<Item = &str> {
        let all = [&self.rest, &self.idle, &self.walk, &self.run, &self.sprint, &self.fire, &self.aimed_fire];
        all.into_iter().chain([&self.gun_fire]).map(String::as_str).filter(|clip| !clip.is_empty())
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
    /// The clips a gun from an animation pack is held by.
    #[serde(rename = "clips", skip_serializing_if = "PackClips::is_default")]
    pub pack: PackClips,
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
            beats: if spec.played.is_empty() { spec.beats } else { Vec::new() },
            clip: spec.clip,
            parts: spec.played,
        }
    }

    /// The clips its actions are based on and played from, and those it is
    /// held by.
    pub fn clips(&self) -> impl Iterator<Item = &str> {
        let built_in = self.offsets.iter().filter_map(|offsets| offsets.clip.as_ref());
        let own = self.actions.iter().filter_map(|own| own.clip.as_ref());
        let based = built_in.chain(own).map(|base| base.clip.as_str());
        let parts = self.offsets.iter().flat_map(|offsets| &offsets.parts);
        let parts = parts.chain(self.actions.iter().flat_map(|own| &own.parts));
        let played = parts.flat_map(|part| [part.arms.as_str(), part.gun.as_str()]);
        based.chain(played).chain(self.pack.clips()).filter(|clip| !clip.is_empty())
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
    spec.clip.clone_from(&offsets.clip);
    if !offsets.parts.is_empty() {
        spec.play(&offsets.parts, speed);
        return;
    }
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
    /// The asset each reference resolved names, by its ID, so a handling
    /// named by its path and by its ID is known as one.
    ids: HashMap<String, String>,
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
        handlings.ids.clear();
        handlings.version = content.handlings_version();
    }
    for reference in wanted {
        if handlings.found.contains_key(&reference) {
            continue;
        }
        let guid = content.parse(&reference).ok().and_then(|parsed| parsed.guid());
        if let Some(guid) = guid {
            handlings.ids.insert(reference.clone(), guid.to_string());
        }
        handlings.found.insert(reference, guid.and_then(|guid| content.gun_handling(guid)));
    }
}

/// Whether two references name the same handling: alike, or the one a
/// gun picks by its path and the other by its asset's ID, as a script's
/// typed reference does.
pub fn same_handling(world: &World, a: &str, b: &str) -> bool {
    let id = |reference: &str| {
        let resolved = world.resource::<Handlings>().and_then(|handlings| handlings.ids.get(reference).cloned());
        resolved.unwrap_or_else(|| reference.to_string())
    };
    a == b || id(a) == id(b)
}

/// Gives guns that pick `reference` this handling, until one is saved: as
/// a tool may show edits before they are saved.
pub fn put_handling(world: &mut World, reference: &str, handling: Arc<GunHandling>) {
    if world.resource::<Handlings>().is_none() {
        world.insert_resource(Handlings::default());
    }
    if let Some(handlings) = world.resource_mut::<Handlings>() {
        handlings.found.insert(reference.to_string(), Some(handling));
    }
}

/// The clips the world's guns' handlings base their actions on, to be read
/// before they play.
pub fn clips_wanted(world: &World) -> Vec<String> {
    let guns = world.query_ref::<&Gun>().into_iter();
    guns.flat_map(|gun| handling_of(world, gun).clips().map(str::to_string).collect::<Vec<_>>()).collect()
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

    #[test]
    fn a_handling_named_by_its_path_and_by_its_id_is_one() {
        let mut world = World::new();
        let mut handlings = Handlings::default();
        handlings.ids.insert("Guns/Rifle.gunhandling".into(), "63d2f37b09594ceab7e51b9719a46daa".into());
        world.insert_resource(handlings);
        assert!(same_handling(&world, "Guns/Rifle.gunhandling", "63d2f37b09594ceab7e51b9719a46daa"));
        assert!(same_handling(&world, "Guns/Rifle.gunhandling", "Guns/Rifle.gunhandling"));
        assert!(!same_handling(&world, "Guns/Rifle.gunhandling", "0000000000000000000000000000beef"));
    }
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
