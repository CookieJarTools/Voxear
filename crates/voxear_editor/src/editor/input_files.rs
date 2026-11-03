//! The project's input actions written back after the Input Actions
//! window changed them, to the `.inputactions` file the game reads (the
//! first by path), or to a new `Input.inputactions` at the project's top
//! when it had none, and read into the game's input again. While a binding
//! listens for a key or button, the keys and mouse buttons go to the
//! game's input, wherever the focus is, and it reads them each frame.

use voxear_assets::AssetKind;

use super::Editor;

/// Where the actions go when the project has no file of its own yet.
const NEW_FILE: &str = "Input.inputactions";

impl Editor {
    pub(super) fn update_input_actions(&mut self) {
        if self.shown(self.nodes.input_panel)
            && let Some(text) = self.input_actions.update(&mut self.ui, &mut self.game.input)
        {
            self.save_input_actions(&text);
        }
        if self.input_actions.is_listening() && !self.is_playing() {
            self.game.input.update(1.0 / 60.0);
        }
    }

    /// Whether a binding that listens takes this event.
    pub(super) fn input_listening_takes(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        if !self.input_actions.is_listening() {
            return false;
        }
        let taken = matches!(event, WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. });
        if taken {
            self.game.input.handle(event);
        }
        taken
    }

    fn save_input_actions(&mut self, text: &str) {
        let db = self.game.content.db();
        let mut files: Vec<String> = db
            .entries()
            .filter(|entry| entry.kind == AssetKind::InputActions)
            .map(|entry| entry.path.clone())
            .collect();
        files.sort();
        let path = files.into_iter().next().unwrap_or_else(|| NEW_FILE.to_string());
        let written = std::fs::write(self.game.project.root.join(&path), text);
        if let Err(error) = written.map_err(anyhow::Error::from).and_then(|()| self.game.content.rescan()) {
            return self.notify(&format!("Could not save {path}: {error:#}"));
        }
        self.game.load_input_actions();
    }
}
