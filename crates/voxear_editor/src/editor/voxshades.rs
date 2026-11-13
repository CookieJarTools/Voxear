//! The VoxShade panel in the editor (see `voxshade_panel`): a
//! `.voxshade` opened from the Project or from Open in its Inspector,
//! the search that adds a node, and the keys that are the graph's while its
//! panel is the one last pressed in: undo and redo, Delete, Ctrl+D, Ctrl+C
//! and Ctrl+V, F and Space. What the panel changes is saved once nothing is
//! held, and compiled into the renderer's shaders, so materials using it
//! draw with it as it now is.

use voxear_render::Renderer;
use voxear_ui::NodeId;
use voxear_voxshade::Graph;

use super::Editor;
use crate::{
    graph_canvas::CanvasKey,
    object_picker::{Choice, Chosen, PickFor},
    voxshade_panel::{VoxShadeEvent, VoxShadePanel, folder_icon},
};

/// What a VoxShade shader's file starts with, before the graph.
pub const VOXSHADE_HEADING: &str = "# VoxShade: a shader made of nodes, which materials pick as they pick a shader.\n";

impl Editor {
    /// Opens the VoxShade shader at `path` in the VoxShade panel, as a
    /// double click on it in the Project does. False where its file cannot
    /// be read as one.
    pub fn open_voxshade(&mut self, path: &str, renderer: &mut Renderer) -> bool {
        let Some(guid) = self.game.content.db().guid_of(path) else {
            self.notify(&format!("{path} is not in the project"));
            return false;
        };
        if self.voxshade.path() != Some(path) {
            if let Some((open, graph)) = self.voxshade.take_unsaved() {
                self.save_voxshade(&open, &graph, renderer);
            }
            let file = self.game.content.db().file(path);
            let read = std::fs::read_to_string(file).map_err(anyhow::Error::from);
            match read.and_then(|text| Graph::from_toml(&text)) {
                Ok(graph) => self.voxshade.open(&mut self.ui, path, graph),
                Err(error) => {
                    self.notify(&format!("Could not open {path}: {error:#}"));
                    return false;
                }
            }
            self.voxshade_guid = Some(guid);
        }
        self.show_panel("voxshade");
        self.voxshade.set_active(true);
        true
    }

    /// The VoxShade panel, and the graph open in it.
    pub fn voxshade(&self) -> &VoxShadePanel {
        &self.voxshade
    }

    /// A key for the graph in the VoxShade panel, as pressed with its
    /// panel the one last pressed in.
    pub fn voxshade_key(&mut self, key: CanvasKey) {
        self.voxshade.key(key);
    }

    /// Whether the keys are the open VoxShade shader's: its panel shows, and
    /// was the last one pressed in.
    pub(super) fn voxshade_has_keys(&self) -> bool {
        self.voxshade.is_active() && self.shown(self.nodes.voxshade_panel)
    }

    /// Undoes or redoes a step of the open VoxShade shader.
    pub(super) fn voxshade_undo(&mut self, undo: bool) {
        self.ui.blur();
        let done = if undo { self.voxshade.undo() } else { self.voxshade.redo() };
        if !done {
            self.notify(if undo { "Nothing to undo" } else { "Nothing to redo" });
        }
    }

    /// The VoxShade panel's frame.
    pub(super) fn update_voxshade(&mut self, renderer: &mut Renderer) {
        self.follow_voxshade_file();
        self.follow_voxshade_presses();
        let shown = self.shown(self.nodes.voxshade_panel);
        for event in self.voxshade.update(&mut self.ui, shown, self.alt) {
            match event {
                VoxShadeEvent::Save(path, graph) => self.save_voxshade(&path, &graph, renderer),
                VoxShadeEvent::Search(anchor) => self.search_shader_nodes(anchor),
            }
        }
    }

    /// Writes a VoxShade shader's file, and compiles the project's shaders
    /// again with it: on their own thread, the materials drawing with the
    /// shaders as they were meanwhile.
    pub(super) fn save_voxshade(&mut self, path: &str, graph: &Graph, renderer: &mut Renderer) {
        let text = format!("{VOXSHADE_HEADING}{}", graph.to_toml());
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(&file, text) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        self.game.apply_custom_shaders(renderer);
    }

    /// A VoxShade shader's file was written outside the editor: the open graph reads it again.
    pub(super) fn voxshade_file_changed(&mut self, path: &str) {
        if self.voxshade.path() != Some(path) {
            return;
        }
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        if let Some(graph) = text.ok().and_then(|text| Graph::from_toml(&text).ok()) {
            self.voxshade.reread(graph);
        }
    }

    /// Keeps the open graph with its file: renamed or moved, it is followed; gone, it closes.
    fn follow_voxshade_file(&mut self) {
        let (Some(path), Some(guid)) = (self.voxshade.path(), self.voxshade_guid) else { return };
        let db = self.game.content.db();
        if db.guid_of(path) == Some(guid) {
            return;
        }
        match db.get(guid).map(|entry| entry.path.clone()) {
            Some(moved) => self.voxshade.moved_to(&mut self.ui, &moved),
            None => {
                self.voxshade.close();
                self.voxshade_guid = None;
            }
        }
    }

    /// The keys are the graph's once its panel is pressed in, and no longer once anything else is.
    fn follow_voxshade_presses(&mut self) {
        let (ui, panel) = (&self.ui, self.nodes.voxshade_panel);
        let in_panel =
            ui.is_pressed(panel) || ui.clicked(panel) || ui.context_clicked(panel) || ui.is_middle_pressed(panel);
        let anywhere = ui.holds_press() || ui.clicked(ui.root()) || ui.context_clicked(ui.root());
        if in_panel {
            self.voxshade.set_active(true);
        } else if anywhere {
            self.voxshade.set_active(false);
        }
    }

    /// Opens the search for a node to add to the open VoxShade shader, under `anchor`.
    fn search_shader_nodes(&mut self, anchor: NodeId) {
        let choices = self
            .voxshade
            .offers()
            .iter()
            .enumerate()
            .map(|(place, offer)| Choice {
                label: offer.label.clone(),
                detail: String::new(),
                icon: folder_icon(offer.folder),
                pick: Chosen::GraphNode(place),
                preview: None,
                folder: offer.folder.to_string(),
            })
            .collect();
        self.picker.open(&mut self.ui, (PickFor::ShaderNode, anchor), "Add Node", choices, "");
    }

    /// Adds the node the search picked, by its place among those it offered.
    pub(super) fn pick_shader_node(&mut self, place: usize) {
        self.voxshade.add_offered(place);
    }
}
