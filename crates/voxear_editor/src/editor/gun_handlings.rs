//! Gun Handlings in the Inspector (see `gun_handling_inspector`): saved as
//! they are typed into, so a game playing in the editor holds and acts as
//! they say at once; shown again when the kind or a clip is picked; their
//! clip fields filled from the object picker or by clips dropped from the
//! Project panel.

use voxear_engine::guns::handling::GunHandling;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    fields::Pick,
    gun_handling_inspector::{HandlingAction, set_clip},
    object_picker::{self, PickFor},
    project_files::NEW_GUN_HANDLING,
    props,
};

impl Editor {
    /// Writes a Gun Handling, reads it again for the guns that pick it,
    /// and when `rebuild`, shows it again.
    pub(super) fn save_gun_handling(&mut self, path: &str, handling: &GunHandling, rebuild: bool) {
        let text = match toml::to_string(handling) {
            Ok(text) => format!("{NEW_GUN_HANDLING}{text}"),
            Err(error) => return self.notify(&format!("Could not save {path}: {error}")),
        };
        if let Err(error) = std::fs::write(self.game.project.root.join(path), text) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_gun_handling(guid);
        }
        if let Some(item) = self.project.item(path).filter(|_| rebuild) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Opens the object picker under `anchor` for the clip `action` of the
    /// handling at `handling` is based on.
    pub(super) fn browse_gun_clip(&mut self, handling: &str, action: HandlingAction, anchor: NodeId) {
        let mut choices = vec![("None".to_string(), Pick::Asset(None))];
        choices.extend(props::model_clips(&self.game));
        let choices = object_picker::decorate(&self.game, choices);
        let target = PickFor::GunHandlingClip { handling: handling.to_string(), action };
        self.picker.open(&mut self.ui, (target, anchor), "Select Clip", choices, "");
    }

    /// Bases `action` on the first clip dragged onto its field.
    pub(super) fn drop_gun_clip(&mut self, handling: &str, action: HandlingAction, source: &DragSource) {
        if let Some(first) = self.dragged_assets(source).into_iter().next() {
            self.pick_gun_handling_clip(handling, action, &Pick::Asset(Some(first)));
        }
    }

    /// Bases `action` of the handling at `handling` on the clip picked, or
    /// on none, and saves it.
    pub(super) fn pick_gun_handling_clip(&mut self, handling: &str, action: HandlingAction, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let clip = match choice {
            None => String::new(),
            Some(choice) => match props::clip_reference(&mut self.game, choice) {
                Some(clip) => clip,
                None => return self.notify("Only a clip, or a model file with clips, goes there"),
            },
        };
        let text = std::fs::read_to_string(self.game.project.root.join(handling)).unwrap_or_default();
        let mut based: GunHandling = toml::from_str(&text).unwrap_or_default();
        set_clip(&mut based, action, clip);
        self.save_gun_handling(handling, &based, true);
    }
}
