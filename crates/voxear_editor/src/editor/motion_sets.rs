//! Motion Sets in the Inspector (see `motion_set_inspector`): saved as they
//! are typed into, shown again when something is added, taken out, moved
//! or picked, and their clip fields filled from the object picker or by
//! clips dropped from the Project panel. Several clips dropped on a state
//! fill it: its loops, the clip it stands with, and backward loops when it
//! has none.

use voxear_core::math::Vec3;
use voxear_engine::animation::{CLIP_PART, ClipRun, MotionDatabase, MotionSet, MotionState, StateClip};
use voxear_render::ModelAnimation;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    fields::Pick,
    motion_set_inspector::ClipSlot,
    object_picker::{self, PickFor},
    props,
};

/// What a new Motion Set's file starts with, above its states and actions.
const HEADER: &str = "# Voxear Motion Set: what a character can do, its states' looping clips and its actions.\n";

impl Editor {
    /// Writes a Motion Set, plays it as written from the next frame, and when
    /// `rebuild`, shows it again.
    pub(super) fn save_motion_set(&mut self, path: &str, set: &MotionSet, rebuild: bool) {
        let text = match toml::to_string(set) {
            Ok(text) => format!("{HEADER}{text}"),
            Err(error) => return self.notify(&format!("Could not save {path}: {error}")),
        };
        if let Err(error) = std::fs::write(self.game.project.root.join(path), text) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_motion_set(guid);
        }
        if let Some(item) = self.project.item(path).filter(|_| rebuild) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Opens the object picker under `anchor` for a clip of the set at `set`.
    pub(super) fn browse_clip(&mut self, set: &str, slot: ClipSlot, anchor: NodeId) {
        let mut choices = vec![("None".to_string(), Pick::Asset(None))];
        choices.extend(props::model_clips(&self.game));
        let choices = object_picker::decorate(&self.game, choices);
        let target = PickFor::MotionSetClip { set: set.to_string(), slot };
        self.picker.open(&mut self.ui, (target, anchor), "Select Clip", choices, "");
    }

    /// Puts a clip dragged from the Project panel in a clip field, or fills
    /// the state it is dropped on with several.
    pub(super) fn drop_clip(&mut self, set: &str, slot: ClipSlot, source: &DragSource) {
        let dragged = self.dragged_assets(source);
        let state = match slot {
            ClipSlot::Idle(state) | ClipSlot::Loop(state, _) | ClipSlot::NewLoop(state) => Some(state),
            ClipSlot::Action(_) => None,
        };
        match (dragged.as_slice(), state) {
            ([], _) => {}
            ([_, _, ..], Some(state)) => self.fill_state(set, state, &dragged),
            ([first, ..], _) => self.pick_motion_set_clip(set, &slot, &Pick::Asset(Some(first.clone()))),
        }
    }

    /// Fills state `state` of the set at `set` from clips dropped together
    /// (see [`fill`]), saves it, and says in the status bar what it did.
    fn fill_state(&mut self, set: &str, state: usize, dropped: &[String]) {
        let text = std::fs::read_to_string(self.game.project.root.join(set)).unwrap_or_default();
        let mut motion_set: MotionSet = toml::from_str(&text).unwrap_or_default();
        let (mut clips, mut left_out) = (Vec::new(), Vec::new());
        for choice in dropped {
            let clip = props::clip_reference(&mut self.game, choice).filter(|_| choice.contains(CLIP_PART));
            match clip.and_then(|clip| self.clip_motion(clip)) {
                Some(found) => clips.push(found),
                None => left_out.push(clip_name(choice)),
            }
        }
        let Some(target) = motion_set.states.get(state) else { return };
        let mut known: Vec<(String, ClipRun)> = clips.clone();
        for entry in &target.clips {
            if let Some(found) = self.clip_motion(entry.clip.clone()) {
                known.push(found);
            }
        }
        let run_of = |clip: &str| known.iter().find(|(known, _)| known == clip).map(|(_, run)| *run);
        let filled = fill(&mut motion_set.states[state], &clips, run_of);
        self.save_motion_set(set, &motion_set, true);
        left_out.extend(filled.left_out);
        let mut said = format!("{} loops added", filled.added);
        if filled.backwards > 0 {
            said += &format!(", {} backwards", filled.backwards);
        }
        if let Some(idle) = filled.idle {
            said += &format!(", stands with {idle}");
        }
        if !left_out.is_empty() {
            said += &format!(" · Left out: {}", left_out.join(", "));
        }
        self.notify(&said);
    }

    /// How a clip moves, as a Motion Matcher reads it on its own: its
    /// velocity seen from its root, and whether it loops.
    fn clip_motion(&mut self, clip: String) -> Option<(String, ClipRun)> {
        let source = self.game.content.clip(&self.game.content.parse(&clip).ok()?)?;
        let skeleton = source.animation.skeleton.clone();
        let animation = ModelAnimation { skeleton, skins: Vec::new(), clips: vec![source.clip().clone()] };
        let run = *MotionDatabase::build(&animation, &[source.settings])?.runs.first()?;
        Some((clip, run))
    }

    /// Puts a clip picked in a clip field of the set at `set`, or takes it
    /// out for none, and saves the set.
    pub(super) fn pick_motion_set_clip(&mut self, set: &str, slot: &ClipSlot, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let clip = match choice {
            None => String::new(),
            Some(choice) => match props::clip_reference(&mut self.game, choice) {
                Some(clip) => clip,
                None => return self.notify("Only a clip, or a model file with clips, goes there"),
            },
        };
        let text = std::fs::read_to_string(self.game.project.root.join(set)).unwrap_or_default();
        let mut motion_set: MotionSet = toml::from_str(&text).unwrap_or_default();
        match *slot {
            ClipSlot::Idle(state) => {
                let Some(state) = motion_set.states.get_mut(state) else { return };
                state.idle = clip;
            }
            ClipSlot::Loop(state, at) => {
                let Some(clips) = motion_set.states.get_mut(state).map(|state| &mut state.clips) else { return };
                match clip.is_empty() {
                    true if at < clips.len() => drop(clips.remove(at)),
                    _ => {
                        let Some(entry) = clips.get_mut(at) else { return };
                        entry.clip = clip;
                    }
                }
            }
            ClipSlot::NewLoop(state) => {
                let Some(state) = motion_set.states.get_mut(state).filter(|_| !clip.is_empty()) else { return };
                state.clips.push(StateClip { clip, backwards: false });
            }
            ClipSlot::Action(action) => {
                let Some(action) = motion_set.actions.get_mut(action) else { return };
                action.clip = clip;
            }
        }
        self.save_motion_set(set, &motion_set, true);
    }
}

/// A clip's name, from its reference, or a file's.
fn clip_name(reference: &str) -> String {
    match reference.split_once(&format!("#{CLIP_PART}")) {
        Some((_, name)) => name.to_string(),
        None => reference.rsplit('/').next().unwrap_or(reference).to_string(),
    }
}

/// What filling a state did: the loops it added, how many of those play
/// backwards, the clip it now stands with, and the clips left out.
#[derive(Debug, Default, PartialEq)]
struct Filled {
    added: usize,
    backwards: usize,
    idle: Option<String>,
    left_out: Vec<String>,
}

/// Fills `state` from `dropped` clips, each with how it moves: loops that
/// move go in its clips, once each; the first that stands still is what it
/// stands with when it has nothing yet; and when none of its loops goes
/// back, the forward ones dropped play backwards too. Clips that do not
/// loop are left out. `run_of` says how any of its clips moves.
fn fill(state: &mut MotionState, dropped: &[(String, ClipRun)], run_of: impl Fn(&str) -> Option<ClipRun>) -> Filled {
    let mut filled = Filled::default();
    let mut forward = Vec::new();
    for (clip, run) in dropped {
        let velocity = run.velocity;
        if !run.looping {
            filled.left_out.push(clip_name(clip));
        } else if run.stands() {
            if state.idle.is_empty() {
                filled.idle = Some(clip_name(clip));
                state.idle = clip.clone();
            }
        } else if !state.clips.iter().any(|entry| entry.clip == *clip && !entry.backwards) {
            if velocity.z > velocity.length() * 0.7 {
                forward.push(clip.clone());
            }
            state.clips.push(StateClip { clip: clip.clone(), backwards: false });
            filled.added += 1;
        }
    }
    let goes_back = state.clips.iter().any(|entry| {
        let velocity = run_of(&entry.clip).map_or(Vec3::ZERO, |run| run.velocity);
        let velocity = if entry.backwards { -velocity } else { velocity };
        velocity.z < -velocity.length() * 0.7
    });
    if !goes_back {
        filled.backwards = forward.len();
        filled.added += forward.len();
        state.clips.extend(forward.into_iter().map(|clip| StateClip { clip, backwards: true }));
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(velocity: Vec3, looping: bool) -> ClipRun {
        ClipRun { clip: 0, start: 0, frames: 61, looping, velocity, steady: true }
    }

    #[test]
    fn clips_dropped_together_fill_a_state() {
        let clips = [
            ("Hero.fbx#clip:Walk".to_string(), run(Vec3::Z * 1.4, true)),
            ("Hero.fbx#clip:Jog".to_string(), run(Vec3::Z * 4.0, true)),
            ("Hero.fbx#clip:Idle".to_string(), run(Vec3::ZERO, true)),
            ("Hero.fbx#clip:Jump".to_string(), run(Vec3::Z, false)),
            ("Hero.fbx#clip:Step_Left".to_string(), run(Vec3::NEG_X * 1.2, true)),
        ];
        let run_of = |clip: &str| clips.iter().find(|(c, _)| c == clip).map(|(_, run)| *run);
        let mut state = MotionState::default();
        let filled = fill(&mut state, &clips, run_of);
        assert_eq!(state.idle, "Hero.fbx#clip:Idle");
        let entries: Vec<(&str, bool)> = state.clips.iter().map(|e| (e.clip.as_str(), e.backwards)).collect();
        assert_eq!(
            entries,
            [
                ("Hero.fbx#clip:Walk", false),
                ("Hero.fbx#clip:Jog", false),
                ("Hero.fbx#clip:Step_Left", false),
                ("Hero.fbx#clip:Walk", true),
                ("Hero.fbx#clip:Jog", true),
            ],
            "with nothing going back, the forward loops play backwards too; the step aside does not"
        );
        assert_eq!(filled, Filled { added: 5, backwards: 2, idle: Some("Idle".into()), left_out: vec!["Jump".into()] });

        // Again, with a clip going back: nothing more, and nothing twice.
        let back = [("Hero.fbx#clip:Walk_Back".to_string(), run(Vec3::NEG_Z * 1.4, true))];
        let mut walking = MotionState {
            clips: vec![StateClip { clip: "Hero.fbx#clip:Walk".into(), backwards: false }],
            ..MotionState::default()
        };
        let all: Vec<(String, ClipRun)> = clips.iter().cloned().chain(back.iter().cloned()).collect();
        let run_of = |clip: &str| all.iter().find(|(c, _)| c == clip).map(|(_, run)| *run);
        let filled = fill(&mut walking, &all, run_of);
        assert_eq!(filled.backwards, 0, "a clip goes back already");
        assert_eq!(walking.clips.len(), 4, "Walk is there once, then Jog, Step_Left and Walk_Back");
    }
}
