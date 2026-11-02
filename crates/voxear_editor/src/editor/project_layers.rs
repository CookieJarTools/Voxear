//! The project's layers in the Inspector (see `layers_view`): shown from
//! Add Layer… in an Obj's Layer menu or Edit Layers in the palette, and
//! each change written to `voxear.toml` and put in the world, and while
//! playing in the edited level's world too, so physics takes it up at once.

use super::Editor;
use crate::layers_view::LayersEdit;

impl Editor {
    /// Shows the project's layers in the Inspector, in place of the Obj
    /// selected.
    pub(super) fn inspect_layers(&mut self) {
        self.select(None);
        let layers = self.game.project.layers();
        self.inspector.show_layers(&mut self.ui, &layers);
    }

    /// Takes the Inspector's change to the project's layers this frame.
    pub(super) fn update_project_layers(&mut self) {
        if self.inspector.take_wants_layers() {
            return self.inspect_layers();
        }
        let Some(edit) = self.inspector.update_layers(&mut self.ui) else { return };
        let mut layers = self.game.project.layers();
        match edit {
            LayersEdit::Name(layer, name) => {
                if let Some(other) = layers.find(&name).filter(|&other| other != layer) {
                    self.notify(&format!("Layer {other} is already named {name}"));
                    if let Some(view) = self.inspector.layers_view() {
                        view.reset_name(&mut self.ui, layer);
                    }
                    return;
                }
                layers.set_name(layer, &name);
            }
            LayersEdit::Collide(a, b, on) => layers.set_collide(a, b, on),
        }
        if let Err(error) = self.game.project.set_layers(&layers) {
            self.notify(&format!("Could not save the layers: {error:#}"));
        }
        let layers = self.game.project.layers();
        self.game.engine.world.insert_resource(layers.clone());
        if let Some(world) = self.play.edit_world() {
            world.insert_resource(layers.clone());
        }
        if let Some(view) = self.inspector.layers_view() {
            view.show(&mut self.ui, &layers);
        }
    }
}
