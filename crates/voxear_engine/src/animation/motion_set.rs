//! Motion Sets: a `.motionset` file holds what a character can do, shared by
//! every character that plays it. Its states are the ways it gets about,
//! as walking, crouching or swimming, each with the looping clips a Motion
//! Matcher blends and a clip to stand with. Its actions are clips played
//! once over them, as swinging a wand, jumping or dying, each over the
//! whole body or a part of it, moving the character or not, with marks at
//! the moments scripts want to know of.
//!
//! ```toml
//! [[state]]
//! name = "Walking"
//! role = "grounded"
//! idle = "…#clip:Idle_Loop"
//! [[state.clip]]
//! clip = "…#clip:Walk_Loop"
//! [[state.clip]]
//! clip = "…#clip:Walk_Loop"
//! backwards = true
//!
//! [[action]]
//! name = "Cast"
//! clip = "…#clip:Spell_Shoot"
//! body = "upper"
//! [[action.mark]]
//! name = "Release"
//! time = 0.4
//! ```
//!
//! Clips are referenced as clip assets are (see [`super::clips`]); an
//! action a state starts with, by its name in the same set.

use serde::{Deserialize, Serialize};

/// What a character can do: its states, and its actions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionSet {
    #[serde(rename = "state", skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<MotionState>,
    #[serde(rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<MotionAction>,
}

impl MotionSet {
    /// Every clip the set plays, by reference, each once.
    pub fn clips(&self) -> Vec<&str> {
        let states = self.states.iter().flat_map(|state| {
            std::iter::once(state.idle.as_str()).chain(state.clips.iter().map(|clip| clip.clip.as_str()))
        });
        let actions = self.actions.iter().map(|action| action.clip.as_str());
        let mut clips: Vec<&str> = states.chain(actions).filter(|clip| !clip.is_empty()).collect();
        clips.sort_unstable();
        clips.dedup();
        clips
    }

    /// The first state with `role`.
    pub fn state_for(&self, role: StateRole) -> Option<usize> {
        self.states.iter().position(|state| state.role == role)
    }

    /// The state named `name`.
    pub fn state_named(&self, name: &str) -> Option<usize> {
        self.states.iter().position(|state| state.name == name)
    }

    /// The action named `name`.
    pub fn action_named(&self, name: &str) -> Option<usize> {
        self.actions.iter().position(|action| action.name == name)
    }
}

/// A way of getting about: the loops a Motion Matcher blends while in it,
/// and the clip it stands with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionState {
    pub name: String,
    /// What it is for, by which code asks for it.
    pub role: StateRole,
    /// The clip it stands with; none stands with the first loop of its
    /// clips that stands still.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub idle: String,
    #[serde(rename = "clip", skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<StateClip>,
    /// How long, in seconds, changing into it takes to settle.
    pub blend: f32,
    /// The action played on the way in, as standing up into a crouch, by
    /// its name in the set; none for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub enter: String,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            name: "State".into(),
            role: StateRole::Grounded,
            idle: String::new(),
            clips: Vec::new(),
            blend: 0.25,
            enter: String::new(),
        }
    }
}

/// A loop of a state's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateClip {
    pub clip: String,
    /// Played from its end to its start, so a walk forward is also a walk
    /// back.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub backwards: bool,
}

/// What a state is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateRole {
    /// Walking and running about on the ground.
    #[default]
    Grounded,
    Crouched,
    /// In the air, jumping or falling.
    Airborne,
    Swimming,
    /// Only when code asks for it by its name.
    Custom,
}

impl StateRole {
    pub const ALL: [Self; 5] = [Self::Grounded, Self::Crouched, Self::Airborne, Self::Swimming, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grounded => "Grounded",
            Self::Crouched => "Crouched",
            Self::Airborne => "Airborne",
            Self::Swimming => "Swimming",
            Self::Custom => "Custom",
        }
    }
}

/// A clip played once over a state's, on the whole body or a part of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionAction {
    pub name: String,
    pub clip: String,
    /// The part of the body it plays on.
    pub body: BodyPart,
    /// With `body` a bone: the bone it plays on, and every bone under it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bone: String,
    /// How long, in seconds, it takes to fade in, and out at its end.
    pub blend_in: f32,
    pub blend_out: f32,
    /// Playing round until stopped.
    pub looping: bool,
    /// Holding its last frame at its end, as dying does, until another
    /// action or a stop.
    pub hold_end: bool,
    /// How the character moves while it plays.
    pub movement: ActionMovement,
    /// With a lunge: how far, in metres, ahead it goes, from how many
    /// seconds in, over how many.
    pub lunge_distance: f32,
    pub lunge_start: f32,
    pub lunge_length: f32,
    /// For how many seconds from its start the body may still turn.
    pub turn_until: f32,
    /// From how many seconds in wanting to move ends it, when it may be cut
    /// short so.
    pub cancel_from: f32,
    pub moving_cancels: bool,
    #[serde(rename = "mark", skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<ActionMark>,
}

impl Default for MotionAction {
    fn default() -> Self {
        Self {
            name: "Action".into(),
            clip: String::new(),
            body: BodyPart::Full,
            bone: String::new(),
            blend_in: 0.1,
            blend_out: 0.2,
            looping: false,
            hold_end: false,
            movement: ActionMovement::Keep,
            lunge_distance: 1.5,
            lunge_start: 0.0,
            lunge_length: 0.25,
            turn_until: 0.0,
            cancel_from: 0.0,
            moving_cancels: false,
            marks: Vec::new(),
        }
    }
}

/// Which part of the body an action plays on, found by the bones' names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyPart {
    #[default]
    Full,
    /// The spine and everything above it.
    Upper,
    Arms,
    LeftArm,
    RightArm,
    /// A bone chosen, and every bone under it.
    Bone,
}

impl BodyPart {
    pub const ALL: [Self; 6] = [Self::Full, Self::Upper, Self::Arms, Self::LeftArm, Self::RightArm, Self::Bone];

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full Body",
            Self::Upper => "Upper Body",
            Self::Arms => "Arms",
            Self::LeftArm => "Left Arm",
            Self::RightArm => "Right Arm",
            Self::Bone => "From Bone",
        }
    }
}

/// How the character moves while an action plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionMovement {
    /// Its state's clips carry on as the game moves it, as while swinging a
    /// wand.
    #[default]
    Keep,
    /// It stands, as while sitting.
    Freeze,
    /// The clip's own travel moves it, as a roll does.
    Root,
    /// It lunges ahead.
    Lunge,
}

impl ActionMovement {
    pub const ALL: [Self; 4] = [Self::Keep, Self::Freeze, Self::Root, Self::Lunge];

    pub fn label(self) -> &'static str {
        match self {
            Self::Keep => "Keep Moving",
            Self::Freeze => "Freeze",
            Self::Root => "Root Motion",
            Self::Lunge => "Lunge",
        }
    }
}

/// A moment of an action scripts are told of.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionMark {
    pub name: String,
    /// Seconds from the action's start.
    pub time: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_motion_set_reads_and_writes_as_its_file_shows_it() {
        let text = r#"
[[state]]
name = "Walking"
role = "grounded"
idle = "Models/A.glb#clip:Idle"
[[state.clip]]
clip = "Models/A.glb#clip:Walk"
[[state.clip]]
clip = "Models/A.glb#clip:Walk"
backwards = true

[[action]]
name = "Cast"
clip = "Models/A.glb#clip:Shoot"
body = "upper"
[[action.mark]]
name = "Release"
time = 0.4
"#;
        let set: MotionSet = toml::from_str(text).unwrap();
        assert_eq!(set.states[0].clips[1].backwards, true);
        assert_eq!(set.states[0].blend, 0.25, "unsaid, the default");
        assert_eq!(set.actions[0].body, BodyPart::Upper);
        assert_eq!(set.actions[0].marks[0].time, 0.4);
        assert_eq!(set.clips(), ["Models/A.glb#clip:Idle", "Models/A.glb#clip:Shoot", "Models/A.glb#clip:Walk"]);
        assert_eq!(set.state_for(StateRole::Grounded), Some(0));
        let back: MotionSet = toml::from_str(&toml::to_string(&set).unwrap()).unwrap();
        assert_eq!(back, set);
    }
}
