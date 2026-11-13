//! The Generator panel in the editor (see `generator_panel`): a World
//! Generator's graph opened from the Project or from its Inspector, the
//! node picked on it shown in the Inspector, the search that adds a node,
//! and the keys that are the graph's while its panel is the one last
//! pressed in: undo and redo, Delete, Ctrl+D, Ctrl+C and Ctrl+V, F and
//! Space. What the panel changes is saved through `save_world_gen`, which
//! makes the level's ground again from it.

use voxear_render::Renderer;
use voxear_terrain::generate::{NodeId as GraphNode, NodeKind, WorldGen};
use voxear_ui::NodeId;

use super::{Command, Editor};
use crate::{
    generator_panel::{EditKey, GeneratorEvent, GeneratorPanel},
    world_gen_inspector::{NodeEdit, RoomChoice},
    graph_canvas::CanvasKey,
    object_picker::{Choice, Chosen, PickFor},
};

impl Editor {
    /// Opens the graph of the World Generator at `path` in the Generator
    /// panel, as a double click on it in the Project does, and shows the
    /// generator in the Inspector. False where its file cannot be read as
    /// one.
    pub fn open_generator(&mut self, path: &str) -> bool {
        let Some(guid) = self.game.content.db().guid_of(path) else {
            self.notify(&format!("{path} is not in the project"));
            return false;
        };
        if self.generator.path() != Some(path) {
            // The graph it takes the place of is written first, if a change to it still waits.
            if let Some((open, world)) = self.generator.take_unsaved() {
                self.save_world_gen(&open, &world);
            }
            let file = self.game.content.db().file(path);
            let read = std::fs::read_to_string(file).map_err(anyhow::Error::from);
            match read.and_then(|text| WorldGen::parse(&text)) {
                Ok(world) => self.generator.open(&mut self.ui, path, world),
                Err(error) => {
                    self.notify(&format!("Could not open {path}: {error:#}"));
                    return false;
                }
            }
            self.generator_guid = Some(guid);
        }
        self.show_panel("generator");
        self.generator.set_active(true);
        self.inspect_generator(None);
        true
    }

    /// Opens the asset at `path`, as a double click on it in the Project
    /// does. False for a path the project does not have.
    pub fn open_path(&mut self, path: &str, renderer: &mut Renderer) -> bool {
        let Some(item) = self.project.item(path) else { return false };
        self.open_item(&item, renderer);
        true
    }

    /// Picks the open graph's first node titled `title`, as a press on it
    /// does, and shows it in the Inspector. False where it has none.
    pub fn pick_generator_node(&mut self, title: &str) -> bool {
        let graph = self.generator.world().map(|world| world.graph());
        let Some(node) = graph.and_then(|graph| graph.nodes.iter().find(|node| node.kind.title() == title)) else {
            return false;
        };
        let node = node.id;
        self.generator.pick(&[node]);
        self.generator.set_active(true);
        self.inspect_generator(Some(node));
        true
    }

    /// The Generator panel, and the graph open in it.
    pub fn generator(&self) -> &GeneratorPanel {
        &self.generator
    }

    /// A key for the graph in the Generator panel, as pressed with its
    /// panel the one last pressed in.
    pub fn generator_key(&mut self, key: CanvasKey) {
        self.generator.key(key);
    }

    /// Whether the keys are the open graph's: its panel shows, and was the
    /// last one pressed in.
    pub(super) fn generator_has_keys(&self) -> bool {
        self.generator.is_active() && self.shown(self.nodes.generator_panel)
    }

    /// What a shortcut asks of the open graph, while the keys are its own.
    /// False for one that is not the graph's to answer.
    pub(super) fn graph_command(&mut self, command: Command) -> bool {
        // The VoxShade shader's, when its panel was the last pressed in.
        if self.voxshade_has_keys() {
            match command {
                Command::Undo | Command::Redo => self.voxshade_undo(command == Command::Undo),
                Command::Delete => self.voxshade_key(CanvasKey::Delete),
                Command::Duplicate => self.voxshade_key(CanvasKey::Duplicate),
                Command::Frame => self.voxshade_key(CanvasKey::Frame),
                _ => return false,
            }
            return true;
        }
        if !self.generator_has_keys() {
            return false;
        }
        match command {
            Command::Undo | Command::Redo => {
                // A field being typed in would keep its text over the undone value.
                self.ui.blur();
                let done = if command == Command::Undo { self.generator.undo() } else { self.generator.redo() };
                if !done {
                    self.notify(if command == Command::Undo { "Nothing to undo" } else { "Nothing to redo" });
                }
            }
            Command::Delete => self.generator.key(CanvasKey::Delete),
            Command::Duplicate => self.generator.key(CanvasKey::Duplicate),
            Command::Frame => self.generator.key(CanvasKey::Frame),
            _ => return false,
        }
        true
    }

    /// The key the open graph takes for itself, when this is one: Ctrl+C,
    /// Ctrl+V and Space.
    pub(super) fn graph_key(&self, key: &winit::keyboard::Key) -> Option<CanvasKey> {
        use winit::keyboard::{Key, NamedKey};
        if !self.generator_has_keys() && !self.voxshade_has_keys() {
            return None;
        }
        match key {
            Key::Character(text) if self.control && text.eq_ignore_ascii_case("c") => Some(CanvasKey::Copy),
            Key::Character(text) if self.control && text.eq_ignore_ascii_case("v") => Some(CanvasKey::Paste),
            Key::Named(NamedKey::Space) if !self.control => Some(CanvasKey::Search),
            _ => None,
        }
    }

    /// The Generator panel's frame, and the Inspector's for the node or
    /// the generator it shows.
    pub(super) fn update_generator(&mut self, renderer: &mut Renderer) {
        self.save_edited_world_gen();
        if self.generator.rooms_to_read() {
            self.cave_rooms_changed(None);
        }
        // A Block World's overhangs and caves moved into the open generator are nodes of the graph shown.
        for path in crate::block_props::take_written_generators(&mut self.game.engine.world) {
            self.generator_file_changed(&path);
        }
        self.follow_generator_file(renderer);
        self.follow_generator_presses();
        self.edit_graph_node();
        let shown = self.shown(self.nodes.generator_panel);
        let events = self.generator.update(&mut self.ui, shown, renderer, self.alt, self.scale);
        for event in events {
            match event {
                GeneratorEvent::Save(path, world) => self.save_world_gen(&path, &world),
                GeneratorEvent::Inspect(node) => self.inspect_generator(node),
                GeneratorEvent::Search(anchor) => self.search_graph_nodes(anchor),
            }
        }
        self.show_generator_in_inspector();
    }

    /// Keeps the open graph with its file as the project's files change: a
    /// file renamed or moved is followed, and one that went closes it.
    fn follow_generator_file(&mut self, renderer: &mut Renderer) {
        let (Some(path), Some(guid)) = (self.generator.path(), self.generator_guid) else { return };
        let db = self.game.content.db();
        if db.guid_of(path) == Some(guid) {
            return;
        }
        match db.get(guid).map(|entry| entry.path.clone()) {
            Some(moved) => self.generator.moved_to(&mut self.ui, &moved),
            None => {
                self.generator.close(renderer);
                self.generator_guid = None;
            }
        }
    }

    /// The keys are the graph's once its panel is pressed in, or the
    /// Inspector while it shows the graph's node or generator, and no longer
    /// once anything else is.
    fn follow_generator_presses(&mut self) {
        let (ui, panel, inspector) = (&self.ui, self.nodes.generator_panel, self.inspector.root());
        let in_panel =
            ui.is_pressed(panel) || ui.clicked(panel) || ui.context_clicked(panel) || ui.is_middle_pressed(panel);
        let in_inspector = (ui.is_pressed(inspector) || ui.clicked(inspector)) && self.inspector_shows_generator();
        let anywhere = ui.holds_press() || ui.clicked(ui.root()) || ui.context_clicked(ui.root());
        if in_panel || in_inspector {
            self.generator.set_active(true);
        } else if anywhere {
            self.generator.set_active(false);
        }
    }

    /// Whether the first Inspector shows a node of the open graph, or the
    /// open generator itself.
    fn inspector_shows_generator(&self) -> bool {
        let Some(open) = self.generator.path() else { return false };
        self.inspector.shown_graph_node().is_some_and(|(path, _)| path == open)
            || self.inspector.shown_asset().is_some_and(|item| item.path == open)
    }

    /// Takes what the Inspector's rows did to the node it shows into the
    /// open graph.
    fn edit_graph_node(&mut self) {
        let Some(world) = self.generator.world() else { return };
        let Some(node) = self.inspector.shown_graph_node().filter(|(path, _)| Some(*path) == self.generator.path())
        else {
            return;
        };
        let node = node.1;
        let edit = self.inspector.graph_node_view().and_then(|view| view.update(&mut self.ui, world));
        if let Some(mut edit) = edit {
            // A room just picked has its shapes and exits read from its prefab before the graph takes it.
            if let NodeEdit::Rooms(rooms) = &mut edit {
                for room in rooms.iter_mut().filter(|room| room.shapes.is_empty()) {
                    voxear_engine::caves::read_room(&mut self.game.content, room);
                }
            }
            self.generator.edit_node(node, edit);
        }
    }

    /// The project's prefabs, for a Cave Network's rooms to pick among: each by its path, kept by its GUID.
    fn room_choices(&self) -> Vec<RoomChoice> {
        let entries = self.game.content.db().entries().filter(|entry| entry.kind == voxear_assets::AssetKind::Prefab);
        let mut choices: Vec<RoomChoice> =
            entries.map(|entry| RoomChoice { path: entry.path.clone(), reference: entry.guid.to_string() }).collect();
        choices.sort_by(|a, b| a.path.cmp(&b.path));
        choices
    }

    /// A prefab changed, at `path`, or with none every prefab may have: each world generator that lists a
    /// changed one as a cave room reads the room's shapes and exits from it again, and is saved where they
    /// changed. The graph open in the Generator panel takes the change as a step of its own; every other
    /// starts from its file.
    pub(super) fn cave_rooms_changed(&mut self, path: Option<&str>) {
        let changed = path.map(|path| self.game.content.db().guid_of(path));
        if changed == Some(None) {
            return;
        }
        let changed = changed.flatten();
        let lists = |content: &voxear_engine::Content, graph: &voxear_terrain::generate::Graph| match changed {
            Some(guid) => voxear_engine::caves::lists_room(content, graph, guid),
            None => graph.nodes.iter().any(|node| node.kind.rooms().is_some_and(|rooms| !rooms.is_empty())),
        };
        let open = self.generator.path().map(str::to_string);
        if let Some(world) = self.generator.world().filter(|world| lists(&self.game.content, world.graph())).cloned()
            && let Some(read) = voxear_engine::caves::read_rooms(&mut self.game.content, world.graph())
        {
            self.generator.change(world.with_graph(read), None);
        }
        let entries = self.game.content.db().entries().filter(|entry| entry.kind == voxear_assets::AssetKind::WorldGen);
        let files: Vec<String> =
            entries.map(|entry| entry.path.clone()).filter(|path| Some(path) != open.as_ref()).collect();
        for file in files {
            let Ok(text) = std::fs::read_to_string(self.game.project.root.join(&file)) else { continue };
            // Before the file is read whole: few generators have a network at all.
            if !text.contains("cave_network") {
                continue;
            }
            let Ok(world) = WorldGen::parse(&text) else { continue };
            if !lists(&self.game.content, world.graph()) {
                continue;
            }
            if let Some(read) = voxear_engine::caves::read_rooms(&mut self.game.content, world.graph()) {
                self.save_world_gen(&file, &world.with_graph(read));
            }
        }
    }

    /// Shows a node of the open graph in the Inspector, in place of the
    /// Obj selected; or with no node, the generator itself, as selecting it
    /// in the Project does.
    fn inspect_generator(&mut self, node: Option<GraphNode>) {
        let Some((path, world)) = self.generator.path().map(str::to_string).zip(self.generator.world().cloned()) else {
            return;
        };
        if self.inspector.is_locked() {
            return;
        }
        match node {
            Some(node) => {
                if self.inspector.shown_graph_node() == Some((path.as_str(), node)) {
                    return;
                }
                self.select(None);
                let prefabs = self.room_choices();
                self.inspector.show_graph_node(&mut self.ui, &path, &world, node, &prefabs);
            }
            None => {
                if self.inspector.shown_asset().is_some_and(|item| item.path == path) {
                    return;
                }
                self.inspect_path(&path);
                self.show_generator_in_inspector();
            }
        }
    }

    /// Keeps what the Inspector shows of the open graph as the graph now
    /// is: a node's rows built again once they are other rows (a wire into
    /// an input, a value exposed), the generator shown once its node is
    /// gone, and the generator's own rows following what the graph changed.
    fn show_generator_in_inspector(&mut self) {
        let (Some(open), Some(world)) = (self.generator.path(), self.generator.world()) else {
            // With no graph open there is no node of one to show.
            if self.inspector.shown_graph_node().is_some() {
                let playing = self.is_playing();
                self.inspector.show(&mut self.ui, &self.game, None, playing);
            }
            return;
        };
        if let Some((same, node)) = self.inspector.shown_graph_node().map(|(path, node)| (path == open, node)) {
            let stale = !same || self.inspector.graph_node_view().is_some_and(|view| view.stale(world));
            let prefabs = if stale { self.room_choices() } else { Vec::new() };
            if stale && !(same && self.inspector.show_graph_node(&mut self.ui, open, world, node, &prefabs)) {
                // The node went: the generator shows in its place.
                let open = open.to_string();
                self.inspect_path(&open);
                return self.show_generator_in_inspector();
            }
        }
        if !self.inspector.shown_asset().is_some_and(|item| item.path == open) {
            return;
        }
        if let Some(fields) = self.inspector.asset_view().and_then(|view| view.world_fields())
            && fields.world != *world
        {
            fields.follow(&mut self.ui, world);
        }
    }

    /// The generator's own rows changed in its Inspector: into the open
    /// graph when it is this generator's, as a step to undo there, or else
    /// to its file, once nothing is held: a label dragged saves, and makes
    /// the level's ground again, when it is let go. `reshaped` when a
    /// preset was picked: a step of its own, with the new graph in view.
    pub(super) fn world_gen_edited(&mut self, path: &str, world: &WorldGen, reshaped: bool) {
        if self.generator.path() == Some(path) {
            match reshaped {
                true => self.generator.replace(world.clone()),
                false => self.generator.change(world.clone(), Some(EditKey::Own)),
            }
        } else {
            self.world_gen_to_save = Some((path.to_string(), world.clone()));
            self.save_edited_world_gen();
        }
    }

    /// Writes the generator its Inspector last changed, unless a press is
    /// still held on it.
    fn save_edited_world_gen(&mut self) {
        if self.ui.holds_press() {
            return;
        }
        if let Some((path, world)) = self.world_gen_to_save.take() {
            self.save_world_gen(&path, &world);
        }
    }

    /// A World Generator's file was written outside the editor: the open
    /// graph reads it again.
    pub(super) fn generator_file_changed(&mut self, path: &str) {
        if self.generator.path() != Some(path) {
            return;
        }
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        if let Some(world) = text.ok().and_then(|text| WorldGen::parse(&text).ok()) {
            self.generator.reread(world);
        }
    }

    /// Opens the search for a node to add to the open graph, under `anchor`:
    /// every kind of node, in its folder.
    fn search_graph_nodes(&mut self, anchor: NodeId) {
        let choices = NodeKind::all()
            .iter()
            .enumerate()
            .map(|(place, kind)| Choice {
                label: kind.title().to_string(),
                detail: String::new(),
                icon: folder_icon(kind.folder()),
                pick: Chosen::GraphNode(place),
                preview: None,
                folder: kind.folder().to_string(),
            })
            .collect();
        self.picker.open(&mut self.ui, (PickFor::GraphNode, anchor), "Add Node", choices, "");
    }

    /// Adds the kind of node the search picked, by its place among the
    /// kinds there are.
    pub(super) fn pick_graph_node(&mut self, place: usize) {
        if let Some(kind) = NodeKind::all().into_iter().nth(place) {
            self.generator.add_node(kind);
        }
    }
}

/// The icon the search shows beside the nodes of a folder.
fn folder_icon(folder: &str) -> &'static str {
    match folder {
        "Sources" => "mountain",
        "Shape" => "transform",
        "Masks" => "layers",
        "Erosion" => "drop",
        "Climate" => "globe",
        "Biomes" => "leaf",
        "Voxels" => "blocks",
        _ => "sprout",
    }
}
