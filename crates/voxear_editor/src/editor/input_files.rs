//! The project's input actions written back after the Input Actions
//! window changed them, to the `.inputactions` file the game reads (the
//! first by path), or to a new `Input.inputactions` at the project's top
//! when it had none, and read into the game's input again. While a binding
//! listens for a key or button, the keys and mouse buttons go to the
//! game's input, wherever the focus is, and it reads them each frame.
//!
//! The window's pickers are the object picker: a binding's control from
//! every control, in folders by device, and an action's icon.

use voxear_assets::AssetKind;
use voxear_input::controls;
use voxear_ui::ICONS;

use super::Editor;
use crate::{
    input_edit,
    input_editor::{Browse, Picked},
    input_panel::IconsAsk,
    object_picker::{Choice, Chosen, PickFor},
};

/// Where the actions go when the project has no file of its own yet.
const NEW_FILE: &str = "Input.inputactions";

impl Editor {
    /// Gives the actions in the project's file that have no id one, and
    /// writes them, so a script's field or its Actions class can name them.
    fn give_action_ids(&mut self) {
        let has_file = self.game.content.db().entries().any(|entry| entry.kind == AssetKind::InputActions);
        let mut file = self.game.input.authored_file().clone();
        if has_file && file.fill_ids(|| voxear_assets::AssetGuid::new_random().to_string()) {
            match file.to_text() {
                Ok(text) => self.save_input_actions(&text),
                Err(error) => self.notify(&format!("Could not give the input actions ids: {error:#}")),
            }
        }
    }

    pub(super) fn update_input_actions(&mut self, renderer: &mut voxear_render::Renderer) {
        self.give_action_ids();
        // The Prompt shows a binding as players will see it, with the game's Prompt Icons.
        if self.shown(self.nodes.input_panel) {
            self.game.prompt_icons.follow(&self.game.input, &mut self.game.content, renderer);
        }
        let sprites = self.game.prompt_icons.sprites();
        if self.shown(self.nodes.input_panel)
            && let Some(text) = self.input_actions.update(&mut self.ui, &mut self.game.input, sprites)
        {
            self.save_input_actions(&text);
        }
        if self.input_actions.is_listening() && !self.is_playing() {
            self.game.input.update(1.0 / 60.0);
        }
        if self.shown(self.nodes.input_panel) {
            let shown = self.input_prompt_icons_path();
            match self.input_actions.prompt_icons(&mut self.ui, shown.as_deref()) {
                Some(IconsAsk::Browse(anchor)) => self.browse_input_prompt_icons(anchor),
                Some(IconsAsk::Drop(source)) => self.drop_input_prompt_icons(&source),
                None => {}
            }
        }
        match self.input_actions.take_browse() {
            Some(Browse::Control { anchor, now }) => {
                let choice = |control: controls::Control| Choice {
                    label: control.name,
                    detail: String::new(),
                    icon: input_edit::device_icon(&control.path),
                    pick: Chosen::Control(control.path),
                    preview: None,
                    folder: control.group.to_string(),
                };
                let choices = controls::all().into_iter().map(choice).collect();
                self.picker.open(&mut self.ui, (PickFor::InputControl, anchor), "Select Control", choices, &now);
            }
            Some(Browse::Icon { anchor, now }) => {
                let none = Choice {
                    label: "Default".into(),
                    detail: "by its type".into(),
                    icon: "close",
                    pick: Chosen::Icon(None),
                    preview: None,
                    folder: String::new(),
                };
                let icon = |&name: &&'static str| Choice {
                    label: name.to_string(),
                    detail: String::new(),
                    icon: name,
                    pick: Chosen::Icon(Some(name)),
                    preview: None,
                    folder: String::new(),
                };
                let mut choices = vec![none];
                choices.extend(ICONS.iter().filter(|name| !name.starts_with("logo-")).map(icon));
                self.picker.open(&mut self.ui, (PickFor::ActionIcon, anchor), "Select Icon", choices, &now);
            }
            None => {}
        }
    }

    /// Hands the Input Actions window what its picker picked.
    pub(super) fn pick_for_input(&mut self, picked: Chosen) {
        match picked {
            Chosen::Control(path) => self.input_actions.pick(Picked::Control(path)),
            Chosen::Icon(icon) => self.input_actions.pick(Picked::Icon(icon.map(str::to_string))),
            _ => {}
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

    pub(super) fn save_input_actions(&mut self, text: &str) {
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
