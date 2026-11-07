//! Motion Sets in the Inspector (see `motion_set_inspector`): saved as they
//! are typed into, shown again when something is added, taken out, moved
//! or picked, and their clip fields filled from the object picker or by
//! clips dropped from the Project panel.

use voxear_engine::animation::{MotionSet, StateClip};
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

    /// Puts a clip dragged from the Project panel in a clip field.
    pub(super) fn drop_clip(&mut self, set: &str, slot: ClipSlot, source: &DragSource) {
        let Some(path) = self.dragged_asset(source) else { return };
        self.pick_motion_set_clip(set, &slot, &Pick::Asset(Some(path)));
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
