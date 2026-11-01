//! Assets imported again: a model whose import settings were applied in the
//! Inspector is read again with them and put behind the meshes and
//! materials it had, so every Obj drawing it, and every undo step, shows the
//! change at once. Parts of it placed in the open scenes move to where the
//! model puts them now, as after a change of size, unless moved since.

use voxear_assets::ModelImport;
use voxear_render::Renderer;

use super::Editor;

impl Editor {
    /// Writes a model's import settings to its `.meta`, and imports it again.
    pub(super) fn apply_import_settings(&mut self, path: &str, settings: &ModelImport, renderer: &mut Renderer) {
        if let Err(error) = settings.write(&self.game.project.root.join(path)) {
            self.notify(&format!("Could not save the import settings of {path}: {error:#}"));
            return;
        }
        self.reimport_model(path, renderer);
    }

    /// Imports a model again with the settings in its `.meta`, and draws
    /// again the previews that show it.
    pub(super) fn reimport_model(&mut self, path: &str, renderer: &mut Renderer) {
        let name = path.rsplit('/').next().unwrap_or(path);
        match self.game.reimport_model(renderer, path) {
            Ok(moved) => {
                // The parts' places are part of the scene, to save.
                self.unsaved |= moved > 0;
                self.notify(&format!("Imported {name} again"));
            }
            Err(error) => self.notify(&format!("Could not import {name} again: {error:#}")),
        }
        // Its parts' previews and those of prefabs drawing it, as well as its own.
        self.asset_previews.clear(renderer);
        self.asset_changed(renderer, path);
    }
}
