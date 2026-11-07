//! A model's clips in its Inspector: each clip with how it moves and loops,
//! written to the model's `.meta` (see [`ClipSettings`]) on Apply, and
//! played with from then on.

use voxear_assets::ClipSettings;

use super::Editor;

impl Editor {
    /// Lists the clips of the model the Inspector shows, reading them first
    /// if they have not been.
    pub(super) fn fill_model_clips(&mut self) {
        let Some(view) = self.inspector.asset_view().filter(|view| view.wants_clips()) else { return };
        let path = view.item().path.clone();
        let Some(guid) = self.game.content.db().guid_of(&path) else { return };
        let names: Vec<String> = self
            .game
            .content
            .clip_file(guid)
            .map_or_else(Vec::new, |animation| animation.clips.iter().map(|clip| clip.name.clone()).collect());
        let settings = ClipSettings::read_meta(&self.game.content.db().mounts().meta_file(&path));
        let Some(view) = self.inspector.asset_view() else { return };
        view.fill_clips(&mut self.ui, &names, settings);
    }

    /// Writes a model's clip settings to its `.meta`, and plays its clips
    /// with them from the next frame.
    pub(super) fn apply_clip_settings(&mut self, path: &str, settings: &ClipSettings) {
        let written = self.game.content.db().mounts().writable_meta(path).and_then(|meta| settings.write_meta(&meta));
        if let Err(error) = written {
            return self.notify(&format!("Could not save the clip settings of {path}: {error:#}"));
        }
        self.wrote_meta(path);
        if let Some(guid) = self.game.content.db().guid_of(path) {
            self.game.content.forget_clips(guid);
        }
    }
}
