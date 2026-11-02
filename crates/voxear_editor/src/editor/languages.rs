//! The game's languages in the editor: string tables edited in the
//! Inspector, written and read again as they change, and pseudo-localising
//! the game's text from the palette (Pseudo-localise Text), to see what is
//! not in the tables and what will not fit once it is translated, while
//! editing and playing.

use voxear_assets::AssetKind;
use voxear_engine::localization::{Localization, StringTable};

use super::Editor;

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
