//! The project's layers (see `layers_view`): named under Project Settings ▸
//! Tags and Layers, which Add Layer… in an Obj's Layer menu and Edit Layers
//! in the palette open, and which collide under Physics. Each change is
//! written to `voxear.toml` and put in the world, and while playing in the
//! edited level's world too, so physics takes it up at once.

use voxear_scene::Layers;

use super::Editor;

impl Editor {
    /// Shows the project's layers, in Project Settings.
    pub(super) fn inspect_layers(&mut self) {
        self.show_project_settings("Tags and Layers");
    }

    /// Opens the layers when the Inspector's Layer menu asks.
    pub(super) fn update_project_layers(&mut self) {
        if self.inspector.take_wants_layers() {
            self.inspect_layers();
        }
    }

    /// Writes the project's layers into `voxear.toml` and puts them in the
    /// worlds, showing them as they now are.
    pub(super) fn set_layers(&mut self, layers: &Layers) {
        if let Err(error) = self.game.project.set_layers(layers) {
            self.notify(&format!("Could not save the layers: {error:#}"));
        }
        self.files_seen();
        let layers = self.game.project.layers();
        self.game.engine.world.insert_resource(layers.clone());
        if let Some(world) = self.play.edit_world() {
            world.insert_resource(layers.clone());
        }
        self.settings_layers.show(&mut self.ui, &layers);
    }
}
