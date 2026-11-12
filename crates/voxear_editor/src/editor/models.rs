//! The Model window in the editor (see `model_panel`): a box model opened
//! from the Project, the bone or box picked in it shown in the Inspector,
//! and the keys that are the model's while its window is the one last
//! pressed in: undo and redo, Delete, Ctrl+D and F. What the window changes
//! is written to the model's file, and whatever draws the model in the
//! level shows it as it now is.

use voxear_render::{BoxModel, Renderer};

use super::{Command, Editor};
use crate::{
    model_panel::{Mode, ModelEvent, ModelPanel, Tool},
    model_view::Picked,
};

impl Editor {
    /// Opens the box model at `path` in the Model window, as a double
    /// click on it in the Project does, and shows the model in the
    /// Inspector. False where its file cannot be read as one.
    pub fn open_model(&mut self, path: &str) -> bool {
        let Some(guid) = self.game.content.db().guid_of(path) else {
            self.notify(&format!("{path} is not in the project"));
            return false;
        };
        if self.model.path() != Some(path) {
            // The model it takes the place of is written first, if a change to it still waits.
            if let Some((open, model)) = self.model.take_unsaved() {
                self.write_box_model(&open, &model);
            }
            let file = self.game.content.db().file(path);
            let read = std::fs::read_to_string(file).map_err(anyhow::Error::from);
            match read.and_then(|text| BoxModel::parse(&text)) {
                Ok(model) => self.model.open(&mut self.ui, path, model),
                Err(error) => {
                    self.notify(&format!("Could not open {path}: {error:#}"));
                    return false;
                }
            }
            self.model_guid = Some(guid);
        }
        self.show_panel("model");
        self.model.set_active(true);
        self.inspect_model(None);
        true
    }

    /// Picks the open model's bone named `name`, as a click on its first
    /// box in the view does, or on the bone in the tree where it has no
    /// box. False where the model has no such bone.
    pub fn pick_model_bone(&mut self, name: &str) -> bool {
        let Some(bone) = self.model.model().and_then(|model| model.find_bone(name)) else { return false };
        let boxes = self.model.model().map_or(0, |model| model.bones[bone].boxes.len());
        self.model.pick(Some(if boxes > 0 { Picked::Box(bone, 0) } else { Picked::Bone(bone) }));
        self.model.set_active(true);
        true
    }

    /// Has the Model window shape its model, or paint it, as its bar's
    /// Shape and Paint do.
    pub fn set_model_mode(&mut self, mode: Mode) {
        self.model.set_mode(&mut self.ui, mode);
    }

    /// Picks what a stroke of paint does in the Model window.
    pub fn set_model_tool(&mut self, tool: Tool) {
        self.model.set_tool(&mut self.ui, tool);
    }

    /// How fast the Model window shows its model moving, in its Motion mode.
    pub fn set_model_pace(&mut self, pace: crate::model_panel::Pace) {
        self.model.set_pace(&mut self.ui, pace);
    }

    /// Whether a stroke on the model in the Model window paints the other
    /// side of the body too.
    pub fn set_model_symmetry(&mut self, on: bool) {
        self.model.set_symmetry(&mut self.ui, on);
    }

    /// The Model window, and the model open in it.
    pub fn model_window(&self) -> &ModelPanel {
        &self.model
    }

    /// The Model window, to do to its model what its bar and keys do.
    pub fn model_window_mut(&mut self) -> &mut ModelPanel {
        &mut self.model
    }

    /// Whether the keys are the open model's: its window shows, and was the
    /// last one pressed in.
    fn model_has_keys(&self) -> bool {
        self.model.is_active() && self.model.path().is_some() && self.shown(self.nodes.model_panel)
    }

    /// What a shortcut asks of the open model, while the keys are its own.
    /// False for one that is not the model's to answer.
    pub(super) fn model_command(&mut self, command: Command) -> bool {
        if !self.model_has_keys() {
            return false;
        }
        match command {
            Command::Undo | Command::Redo => {
                // A field being typed in would keep its text over the undone value.
                self.ui.blur();
                let done = if command == Command::Undo { self.model.undo() } else { self.model.redo() };
                if !done {
                    self.notify(if command == Command::Undo { "Nothing to undo" } else { "Nothing to redo" });
                }
            }
            Command::Delete => self.model.delete_picked(),
            Command::Duplicate => self.model.duplicate_picked(),
            Command::Frame => self.model.frame(),
            _ => return false,
        }
        true
    }

    /// The Model window's frame, and the Inspector's for the bone or box
    /// it shows.
    pub(super) fn update_model(&mut self, renderer: &mut Renderer) {
        self.follow_model_file(renderer);
        self.follow_model_presses();
        self.edit_box_part();
        let shown = self.shown(self.nodes.model_panel);
        let events = self.model.update(&mut self.ui, shown, renderer, self.scale);
        for event in events {
            match event {
                ModelEvent::Save(path, model) => self.save_box_model(&path, &model, renderer),
                ModelEvent::Inspect(part) => self.inspect_model(part),
            }
        }
        self.show_model_in_inspector();
    }

    /// Keeps the open model with its file as the project's files change: a
    /// file renamed or moved is followed, and one that went closes it.
    fn follow_model_file(&mut self, renderer: &mut Renderer) {
        let (Some(path), Some(guid)) = (self.model.path(), self.model_guid) else { return };
        let db = self.game.content.db();
        if db.guid_of(path) == Some(guid) {
            return;
        }
        match db.get(guid).map(|entry| entry.path.clone()) {
            Some(moved) => {
                self.model.moved_to(&mut self.ui, &moved);
                if let Some(view) = self.inspector.box_part_view() {
                    view.moved_to(&moved);
                }
            }
            None => {
                self.model.close(renderer);
                self.model_guid = None;
            }
        }
    }

    /// The keys are the model's once its window is pressed in, or the
    /// Inspector while it shows the model or a part of it, and no longer
    /// once anything else is.
    fn follow_model_presses(&mut self) {
        let (ui, panel, inspector) = (&self.ui, self.nodes.model_panel, self.inspector.root());
        let in_panel =
            ui.is_pressed(panel) || ui.clicked(panel) || ui.context_clicked(panel) || ui.is_middle_pressed(panel);
        let in_inspector = (ui.is_pressed(inspector) || ui.clicked(inspector)) && self.inspector_shows_model();
        let anywhere = ui.holds_press() || ui.clicked(ui.root()) || ui.context_clicked(ui.root());
        if in_panel || in_inspector {
            self.model.set_active(true);
        } else if anywhere {
            self.model.set_active(false);
        }
    }

    /// Whether the first Inspector shows a part of the open model, or the
    /// open model itself.
    fn inspector_shows_model(&self) -> bool {
        let Some(open) = self.model.path() else { return false };
        self.inspector.shown_box_part().is_some_and(|(path, _)| path == open)
            || self.inspector.shown_asset().is_some_and(|item| item.path == open)
    }

    /// Takes what the Inspector's rows did to the part it shows into the
    /// open model.
    fn edit_box_part(&mut self) {
        let Some(model) = self.model.model() else { return };
        let Some((_, part)) = self.inspector.shown_box_part().filter(|(path, _)| Some(*path) == self.model.path())
        else {
            return;
        };
        let edit = self.inspector.box_part_view().and_then(|view| view.update(&mut self.ui, model));
        if let Some(edit) = edit {
            self.model.edit(part, edit);
        }
    }

    /// Shows a bone or box of the open model in the Inspector, or with
    /// none the model itself.
    fn inspect_model(&mut self, part: Option<Picked>) {
        let Some((path, model)) = self.model.path().map(str::to_string).zip(self.model.model().cloned()) else {
            return;
        };
        if self.inspector.is_locked() {
            return;
        }
        match part {
            Some(part) => {
                if self.inspector.shown_box_part() == Some((path.as_str(), part)) {
                    return;
                }
                self.select(None);
                self.inspector.show_box_part(&mut self.ui, &path, &model, part);
            }
            None => {
                if !self.inspector.shown_asset().is_some_and(|item| item.path == path) {
                    self.inspect_path(&path);
                }
            }
        }
    }

    /// Keeps what the Inspector shows of the open model as the model now
    /// is: a part's rows built again once they are another kind of part's,
    /// the model shown once its part is gone, and nothing of a model that
    /// is no longer open.
    fn show_model_in_inspector(&mut self) {
        let Some((shown, part)) = self.inspector.shown_box_part().map(|(path, part)| (path.to_string(), part)) else {
            return;
        };
        let open = self.model.path().map(str::to_string).zip(self.model.model().cloned());
        let Some((open, model)) = open.filter(|(path, _)| *path == shown) else {
            let playing = self.is_playing();
            return self.inspector.show(&mut self.ui, &self.game, None, playing);
        };
        let fits = self.inspector.box_part_view().is_some_and(|view| view.fits(&model));
        if !fits && !self.inspector.show_box_part(&mut self.ui, &open, &model, part) {
            self.inspect_path(&open);
        }
    }

    /// Writes a box model to its file, taking the write as seen.
    fn write_box_model(&mut self, path: &str, model: &BoxModel) -> bool {
        if let Err(error) = std::fs::write(self.game.project.root.join(path), model.to_text()) {
            self.notify(&format!("Could not save {path}: {error}"));
            return false;
        }
        self.files_seen();
        true
    }

    /// Saves a box model, and has whatever draws it in the level, and its
    /// previews, show it as it now is.
    pub(super) fn save_box_model(&mut self, path: &str, model: &BoxModel, renderer: &mut Renderer) {
        if !self.write_box_model(path, model) {
            return;
        }
        let aside = self.play.edit_world();
        if let Err(error) = self.game.reimport_model(renderer, path, aside) {
            self.notify(&format!("Could not draw {path} as saved: {error:#}"));
        }
        self.asset_previews.clear(renderer);
        self.asset_changed(renderer, path);
    }

    /// A box model's file changed outside the editor: where it is the one
    /// open, it is read again.
    pub(super) fn model_file_changed(&mut self, path: &str) {
        if self.model.path() != Some(path) {
            return;
        }
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        if let Some(model) = text.ok().and_then(|text| BoxModel::parse(&text).ok()) {
            self.model.reread(model);
        }
    }
}
