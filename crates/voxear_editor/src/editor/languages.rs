//! The game's languages in the editor: string tables edited in the
//! Inspector, written and read again as they change, and pseudo-localising
//! the game's text from the palette (Pseudo-localise Text), to see what is
//! not in the tables and what will not fit once it is translated, while
//! editing and playing. A key renamed in the Localization Tables window is
//! renamed where it is used too, and the level opened again to show it.

use voxear_assets::AssetKind;
use voxear_engine::localization::{Localization, StringTable};
use voxear_render::Renderer;

use super::Editor;
use crate::localization_tables;

impl Editor {
    /// Writes a string table the Inspector changed and reads the tables
    /// again. With `rebuild`, its entries came, went or moved, so the
    /// Inspector shows it again, an entry moved by its grip easing into
    /// place in the list built again.
    pub(super) fn save_strings(&mut self, path: &str, table: &StringTable, rebuild: bool) {
        if let Err(error) = std::fs::write(self.game.project.root.join(path), table.write()) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        self.game.content.forget_kind(AssetKind::Strings);
        self.game.load_strings();
        let Some(item) = self.project.item(path).filter(|_| rebuild) else { return };
        let items = |editor: &mut Self| editor.inspector.asset_view().and_then(|view| view.strings_items());
        let slides = items(self).map(|old| self.ui.slides_of(old)).unwrap_or_default();
        self.inspector.show_asset(&mut self.ui, &self.game, &item);
        if let Some(new) = items(self) {
            self.ui.start_slides(new, &slides);
        }
    }

    /// Renames a string table's key, in its table and everywhere it is
    /// used: not while playing, nor with the level's changes unsaved, as
    /// the files they would be saved over change.
    pub(super) fn rename_string_key(&mut self, table: &str, old: &str, new: &str, renderer: &mut Renderer) {
        if self.is_playing() {
            return self.notify("Stop playing before renaming a key");
        }
        if self.unsaved {
            return self.notify("Save the level before renaming a key");
        }
        let changed = match localization_tables::rename(&self.game.project.root, table, old, new) {
            Ok(changed) => changed,
            Err(error) => return self.notify(&format!("{error:#}")),
        };
        self.files_seen();
        for path in &changed {
            self.game.content.forget(path);
        }
        self.game.content.forget_kind(AssetKind::Strings);
        self.game.load_strings();
        let level_files = [".scene", ".prefab", ".level"];
        if changed.iter().any(|path| level_files.iter().any(|ending| path.ends_with(ending)))
            && let Err(error) = self.open_level_again(renderer)
        {
            tracing::error!("{error:#}");
        }
        let files = if changed.len() == 1 { "1 file".to_string() } else { format!("{} files", changed.len()) };
        self.notify(&format!("Renamed {old} to {new} in {files}"));
    }

    /// Turns pseudo-localisation on or off, in the edited level's world and,
    /// while playing, the game's.
    pub(super) fn toggle_pseudo_text(&mut self) {
        let on = !self.game.engine.world.resource::<Localization>().is_some_and(Localization::pseudo);
        if let Some(localization) = self.game.engine.world.resource_mut::<Localization>() {
            localization.set_pseudo(on);
        }
        if let Some(localization) = self.play.edit_world().and_then(|world| world.resource_mut::<Localization>()) {
            localization.set_pseudo(on);
        }
        self.notify(if on { "Text is pseudo-localised" } else { "Text is shown as written" });
    }
}
