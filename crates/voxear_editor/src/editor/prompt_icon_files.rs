//! Prompt Icons in the editor (see `prompt_icons_inspector`): a control's
//! picture picked from the object picker or dropped from the Project panel,
//! written to its file and shown by the game's prompts at once; and the set
//! the project's input actions name, picked or dropped on the Input page.

use voxear_assets::AssetKind;
use voxear_engine::prompt_icons::{DRAWN, PromptIconsFile, Sprite};
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::{
    fields::{self, Pick},
    object_picker::{self, PickFor},
    prompt_icons_inspector::IconSlot,
};

/// How the Input page names Voxear's own set, and none at all.
const VOXEARS: &str = "Voxear's";
const DRAWN_LABEL: &str = "Drawn";

impl Editor {
    /// Opens the object picker under `anchor` for a control's picture in
    /// the set at `icons`.
    pub(super) fn browse_prompt_icon(&mut self, icons: &str, slot: IconSlot, anchor: NodeId) {
        let choices = object_picker::decorate(&self.game, fields::assets_of(&self.game, Some(AssetKind::Texture)));
        let target = PickFor::PromptIcon { icons: icons.to_string(), slot };
        self.picker.open(&mut self.ui, (target, anchor), "Select Picture", choices, "");
    }

    /// Puts a picture dragged from the Project panel on a control's row.
    pub(super) fn drop_prompt_icon(&mut self, icons: &str, slot: &IconSlot, source: &DragSource) {
        let dragged = self.dragged_assets(source);
        let db = self.game.content.db();
        let picture = dragged
            .into_iter()
            .find(|path| db.guid_of(path).and_then(|g| db.get(g)).is_some_and(|e| e.kind == AssetKind::Texture));
        if let Some(path) = picture {
            self.pick_prompt_icon(icons, slot, &Pick::Asset(Some(path)));
        }
    }

    /// Puts the picture picked on a control's row of the set at `icons`, or
    /// takes it out for none, keeping the part of a sheet it had; saves the
    /// set and has the game's prompts read it again.
    pub(super) fn pick_prompt_icon(&mut self, icons: &str, slot: &IconSlot, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let file = self.game.project.root.join(icons);
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let mut set: PromptIconsFile = toml::from_str(&text).unwrap_or_default();
        let sprites = set.sprites_mut(slot.device);
        match choice.as_deref().and_then(|path| self.game.content.db().guid_of(path)) {
            Some(guid) => {
                let rect = sprites.get(&slot.control).and_then(|sprite| sprite.rect);
                sprites.insert(slot.control.clone(), Sprite { texture: guid.to_string(), rect });
            }
            None => drop(sprites.remove(&slot.control)),
        }
        let written = set.to_text().and_then(|text| std::fs::write(&file, text).map_err(anyhow::Error::from));
        if let Err(error) = written {
            return self.notify(&format!("Could not save {icons}: {error:#}"));
        }
        self.files_seen();
        self.game.prompt_icons.forget();
        if let Some(item) = self.project.item(icons) {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
    }

    /// Opens the object picker under `anchor` for the set the input actions
    /// name.
    pub(super) fn browse_input_prompt_icons(&mut self, anchor: NodeId) {
        let mut choices =
            vec![(VOXEARS.to_string(), Pick::Asset(None)), (DRAWN_LABEL.to_string(), Pick::Asset(Some(DRAWN.into())))];
        choices.extend(fields::assets_of(&self.game, Some(AssetKind::PromptIcons)).into_iter().skip(1));
        let choices = object_picker::decorate(&self.game, choices);
        self.picker.open(&mut self.ui, (PickFor::InputPromptIcons, anchor), "Select Prompt Icons", choices, "");
    }

    /// Takes Prompt Icons dragged from the Project panel onto the Input
    /// page's field.
    pub(super) fn drop_input_prompt_icons(&mut self, source: &DragSource) {
        let dragged = self.dragged_assets(source);
        if let Some(path) = dragged.into_iter().find(|path| path.ends_with(".prompticons")) {
            self.pick_input_prompt_icons(&Pick::Asset(Some(path)));
        }
    }

    /// Names the set picked in the project's input actions, or none, and
    /// writes them.
    pub(super) fn pick_input_prompt_icons(&mut self, pick: &Pick) {
        let Pick::Asset(choice) = pick else { return };
        let named = match choice.as_deref() {
            None => None,
            Some(DRAWN) => Some(DRAWN.to_string()),
            Some(path) => self.game.content.db().guid_of(path).map(|guid| guid.to_string()),
        };
        let mut file = self.game.input.authored_file().clone();
        file.prompt_icons = named;
        match file.to_text() {
            Ok(text) => self.save_input_actions(&text),
            Err(error) => self.notify(&format!("Could not save the input actions: {error:#}")),
        }
    }

    /// What the Input page shows for the set the input actions name: its
    /// path, or Voxear's own, or none, drawn.
    pub(super) fn input_prompt_icons_path(&self) -> Option<String> {
        let Some(reference) = self.game.input.prompt_icons() else { return Some(VOXEARS.to_string()) };
        if reference == DRAWN {
            return Some(DRAWN_LABEL.to_string());
        }
        let db = self.game.content.db();
        let guid = reference.parse().ok().or_else(|| db.guid_of(reference))?;
        db.get(guid).map(|entry| entry.path.clone())
    }
}
