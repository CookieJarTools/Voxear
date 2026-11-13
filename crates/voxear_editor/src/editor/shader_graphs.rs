//! The Shader Graph panel in the editor (see `shader_graph_panel`): a
//! `.shadergraph` opened from the Project or from Open in its Inspector,
//! the search that adds a node, and the keys that are the graph's while its
//! panel is the one last pressed in: undo and redo, Delete, Ctrl+D, Ctrl+C
//! and Ctrl+V, F and Space. What the panel changes is saved once nothing is
//! held, and compiled into the renderer's shaders, so materials using it
//! draw with it as it now is.

use voxear_render::Renderer;
use voxear_shader_graph::Graph;
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    graph_canvas::CanvasKey,
    object_picker::{Choice, Chosen, PickFor},
    shader_graph_panel::{ShaderGraphEvent, ShaderGraphPanel, folder_icon},
};

/// What a shader graph's file starts with, before the graph.
pub const SHADER_GRAPH_HEADING: &str =
    "# Voxear shader graph: a shader made of nodes, which materials pick as they pick a shader.\n";

impl Editor {
    /// Opens the shader graph at `path` in the Shader Graph panel, as a
    /// double click on it in the Project does. False where its file cannot
    /// be read as one.
    pub fn open_shader_graph(&mut self, path: &str, renderer: &mut Renderer) -> bool {
        let Some(guid) = self.game.content.db().guid_of(path) else {
            self.notify(&format!("{path} is not in the project"));
            return false;
        };
        if self.shader_graph.path() != Some(path) {
            if let Some((open, graph)) = self.shader_graph.take_unsaved() {
                self.save_shader_graph(&open, &graph, renderer);
            }
            let file = self.game.content.db().file(path);
            let read = std::fs::read_to_string(file).map_err(anyhow::Error::from);
            match read.and_then(|text| Graph::from_toml(&text)) {
                Ok(graph) => self.shader_graph.open(&mut self.ui, path, graph),
                Err(error) => {
                    self.notify(&format!("Could not open {path}: {error:#}"));
                    return false;
                }
            }
            self.shader_graph_guid = Some(guid);
        }
        self.show_panel("shader-graph");
        self.shader_graph.set_active(true);
        true
    }

    /// The Shader Graph panel, and the graph open in it.
    pub fn shader_graph(&self) -> &ShaderGraphPanel {
        &self.shader_graph
    }

    /// A key for the graph in the Shader Graph panel, as pressed with its
    /// panel the one last pressed in.
    pub fn shader_graph_key(&mut self, key: CanvasKey) {
        self.shader_graph.key(key);
    }

    /// Whether the keys are the open shader graph's: its panel shows, and
    /// was the last one pressed in.
    pub(super) fn shader_graph_has_keys(&self) -> bool {
        self.shader_graph.is_active() && self.shown(self.nodes.shader_graph_panel)
    }

    /// Undoes or redoes a step of the open shader graph.
    pub(super) fn shader_graph_undo(&mut self, undo: bool) {
        self.ui.blur();
        let done = if undo { self.shader_graph.undo() } else { self.shader_graph.redo() };
        if !done {
            self.notify(if undo { "Nothing to undo" } else { "Nothing to redo" });
        }
    }

    /// The Shader Graph panel's frame.
    pub(super) fn update_shader_graph(&mut self, renderer: &mut Renderer) {
        self.follow_shader_graph_file();
        self.follow_shader_graph_presses();
        let shown = self.shown(self.nodes.shader_graph_panel);
        for event in self.shader_graph.update(&mut self.ui, shown, self.alt) {
            match event {
                ShaderGraphEvent::Save(path, graph) => self.save_shader_graph(&path, &graph, renderer),
                ShaderGraphEvent::Search(anchor) => self.search_shader_nodes(anchor),
            }
        }
    }

    /// Writes a shader graph's file, and compiles the project's shaders
    /// again with it: on their own thread, the materials drawing with the
    /// shaders as they were meanwhile.
    pub(super) fn save_shader_graph(&mut self, path: &str, graph: &Graph, renderer: &mut Renderer) {
        let text = format!("{SHADER_GRAPH_HEADING}{}", graph.to_toml());
        let file = self.game.content.db().file(path);
        if let Err(error) = std::fs::write(&file, text) {
            return self.notify(&format!("Could not save {path}: {error}"));
        }
        self.files_seen();
        self.game.apply_custom_shaders(renderer);
    }

    /// A shader graph's file was written outside the editor: the open graph reads it again.
    pub(super) fn shader_graph_file_changed(&mut self, path: &str) {
        if self.shader_graph.path() != Some(path) {
            return;
        }
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        if let Some(graph) = text.ok().and_then(|text| Graph::from_toml(&text).ok()) {
            self.shader_graph.reread(graph);
        }
    }

    /// Keeps the open graph with its file: renamed or moved, it is followed; gone, it closes.
    fn follow_shader_graph_file(&mut self) {
        let (Some(path), Some(guid)) = (self.shader_graph.path(), self.shader_graph_guid) else { return };
        let db = self.game.content.db();
        if db.guid_of(path) == Some(guid) {
            return;
        }
        match db.get(guid).map(|entry| entry.path.clone()) {
            Some(moved) => self.shader_graph.moved_to(&mut self.ui, &moved),
            None => {
                self.shader_graph.close();
                self.shader_graph_guid = None;
            }
        }
    }

    /// The keys are the graph's once its panel is pressed in, and no longer once anything else is.
    fn follow_shader_graph_presses(&mut self) {
        let (ui, panel) = (&self.ui, self.nodes.shader_graph_panel);
        let in_panel =
            ui.is_pressed(panel) || ui.clicked(panel) || ui.context_clicked(panel) || ui.is_middle_pressed(panel);
        let anywhere = ui.holds_press() || ui.clicked(ui.root()) || ui.context_clicked(ui.root());
        if in_panel {
            self.shader_graph.set_active(true);
        } else if anywhere {
            self.shader_graph.set_active(false);
        }
    }

    /// Opens the search for a node to add to the open shader graph, under `anchor`.
    fn search_shader_nodes(&mut self, anchor: NodeId) {
        let choices = self
            .shader_graph
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
        self.shader_graph.add_offered(place);
    }
}
