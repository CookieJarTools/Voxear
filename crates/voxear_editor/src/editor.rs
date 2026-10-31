//! The editor itself: the open project in edit mode, its panels, the Scene
//! view, which looks through the editor's own camera, and the Game view,
//! which looks through the level's. Play mode is in [`play`].

mod play;

use std::{
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec3},
};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, FlyCamera, Game, Project,
    script_types::{self, ScriptTypes},
};
use voxear_render::{Camera, MeshRenderer, Renderer, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_ui::{Color, NodeId, Shape, Ui};

use crate::{
    console::Console,
    hierarchy::EditorOnly,
    hierarchy::Hierarchy,
    hierarchy::ObjAction,
    history::{Edit, History, ObjState},
    inspector::{Field, Inspector, RowAction},
    levels_panel::{LevelAction, LevelsPanel},
    objs,
    project_view::ProjectView,
    props,
    scene_tools::{Handle, SceneTools, Tool, ViewCamera, pick},
};

const MARKUP: &str = include_str!("../ui/Editor.vui");
const SHEET: &str = include_str!("../ui/Editor.vss");
/// The layout a project opens with until it has one of its own: the
/// Hierarchy on the left, the Scene in the middle over the Project and
/// Console, and the Inspector on the right. The Project sits under the Scene,
/// as in Unity, because its folders and files need the width.
const LAYOUT: &str =
    "row(240, [hierarchy, levels], row(-300, column(66%, [scene, game, cookbook], [project, console]), [inspector]))";
/// The window's background, which the 3D view's surroundings are cleared to.
const BACKGROUND: u32 = 0x0c0e10;
/// How long a message stays in the status bar.
const NOTICE_SECONDS: f32 = 4.0;
/// How often the status bar's numbers change.
const STATUS_INTERVAL: f32 = 0.5;
/// The axis gizmo: its middle, how far its axes reach, and their ends' size.
const GIZMO_MIDDLE: f32 = 15.0;
const GIZMO_REACH: f32 = 11.0;
const GIZMO_END: f32 = 2.5;

/// Elements the editor reads and writes, found by name in Editor.vui.
struct Nodes {
    dock: NodeId,
    viewport: NodeId,
    game_viewport: NodeId,
    game_empty: NodeId,
    frame: NodeId,
    command: NodeId,
    gizmo: NodeId,
    handles: NodeId,
    tool: NodeId,
    tool_strip: NodeId,
    view_gizmo: NodeId,
    pivot: NodeId,
    space: NodeId,
    snap: NodeId,
    stats: NodeId,
    status_left: NodeId,
    status_time: NodeId,
    status_fps: NodeId,
    status_memory: NodeId,
    /// Play, Pause and Step.
    play_buttons: [NodeId; 3],
    build: NodeId,
}

/// What a shortcut asks of the editor, done in the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Save,
    Undo,
    Redo,
    Duplicate,
    Delete,
}

/// How far in front of the Scene camera a new Obj goes.
const NEW_OBJ_DISTANCE: f32 = 8.0;
/// How often the scripts are looked at for changes, to describe them again.
const SCRIPTS_CHECK: Duration = Duration::from_secs(2);

pub struct Editor {
    pub game: Game,
    pub ui: Ui,
    pub fly: FlyCamera,
    nodes: Nodes,
    hierarchy: Hierarchy,
    project: ProjectView,
    inspector: Inspector,
    console: Console,
    /// The project's own documentation, from its `Docs` folder.
    cookbook: voxear_cookbook::Cookbook,
    /// Every level and loose scene, to open, play or change to.
    levels: LevelsPanel,
    /// The Scene view's own camera, which the Hierarchy leaves out.
    camera: Entity,
    /// Play mode, and the scripts it keeps between runs.
    play: play::Play,
    /// The camera the renderer was last told to look through.
    view_camera: Option<Entity>,
    /// The dock area showing the 3D view, made see-through, and where the
    /// frame over its edge was last put.
    viewport_area: Option<NodeId>,
    framed: Option<[f32; 4]>,
    /// The camera's rotation as the gizmo last showed it.
    gizmo_shows: Option<Quat>,
    /// The pointer, in physical pixels, for whether it is over the viewport.
    pointer: Option<(f32, f32)>,
    scale: f32,
    /// Control, or Command on macOS, is held.
    control: bool,
    shift: bool,
    /// Shortcuts pressed since the last frame.
    commands: Vec<Command>,
    /// Values to keep after play: played Objs, and which of their values.
    kept: Vec<(Entity, Field)>,
    /// The Scene view's tools, the handle under the pointer, and a press on
    /// one or a release of the one held, since the last frame.
    tools: SceneTools,
    hovered_handle: Option<Handle>,
    pressed_handle: Option<[f32; 2]>,
    released_handle: bool,
    /// Edits to undo and redo, and whether the open level has edits not saved.
    history: History,
    unsaved: bool,
    /// A level whose opening was held back because edits would be lost; a
    /// second click on it opens it anyway.
    discard_armed: Option<String>,
    /// A message for the status bar, and when it was given.
    notice: Option<(String, Instant)>,
    status: (u32, f32),
    /// The scripts' description being made from a new build, and when the
    /// last was started and when the sources were last looked at.
    describing: Option<Receiver<Result<ScriptTypes>>>,
    described_from: Option<SystemTime>,
    scripts_checked: Option<Instant>,
}

impl Editor {
    /// Opens `project`'s start level in edit mode: its Objs load with their
    /// behaviours as data, and no script runs.
    pub fn open(project: Project, renderer: &mut Renderer, log: LogCapture) -> Result<Self> {
        let started = Instant::now();
        let root = project.root.clone();
        let mut engine = Engine::new();
        engine.set_edit_mode(true);
        let mut game = Game::new(project, engine)?;
        game.start(Some(renderer)).context("opening the start level")?;
        let camera = spawn_camera(&mut game);
        renderer.set_view_options(ViewOptions { camera: Some(camera), ..ViewOptions::default() });
        renderer.set_clear_color(Color::hex(BACKGROUND).linear_premultiplied());

        let mut ui = Ui::new();
        let ui_root = ui.root();
        let mut load = |path: &str| match path {
            "Editor.vss" => Ok(SHEET.to_string()),
            other => Err(format!("{other} is not built in")),
        };
        let document =
            ui.load_markup(ui_root, "Editor.vui", MARKUP, &mut load).expect("the editor's markup is built in");
        let find = |ui: &Ui, name: &str| ui.find(document, name).unwrap_or_else(|| panic!("Editor.vui has no {name}"));
        let nodes = Nodes {
            dock: find(&ui, "dock"),
            viewport: find(&ui, "viewport"),
            game_viewport: find(&ui, "game-viewport"),
            game_empty: find(&ui, "game-empty"),
            frame: find(&ui, "viewport-frame"),
            command: find(&ui, "command"),
            gizmo: find(&ui, "gizmo"),
            handles: find(&ui, "handles"),
            tool: find(&ui, "tool"),
            tool_strip: find(&ui, "tool-strip"),
            view_gizmo: find(&ui, "view-gizmo"),
            pivot: find(&ui, "pivot"),
            space: find(&ui, "space"),
            snap: find(&ui, "snap"),
            stats: find(&ui, "scene-stats"),
            status_left: find(&ui, "status-left"),
            status_time: find(&ui, "status-time"),
            status_fps: find(&ui, "status-fps"),
            status_memory: find(&ui, "status-memory"),
            play_buttons: [find(&ui, "play"), find(&ui, "pause"), find(&ui, "step")],
            build: find(&ui, "build"),
        };
        let rail_scene = find(&ui, "rail-scene");
        ui.set_checked(rail_scene, true);
        let avatar = find(&ui, "avatar");
        ui.set_text(avatar, &initial());
        let branch = git_branch(&root);
        let branch_name = find(&ui, "status-branch-name");
        ui.set_text(branch_name, branch.as_deref().unwrap_or_default());
        let branch_shown = find(&ui, "status-branch");
        ui.set_visible(branch_shown, branch.is_some());
        ui.set_dock_layout(nodes.dock, LAYOUT).expect("the default layout is valid");
        let (tree, filter, add) =
            (find(&ui, "hierarchy-tree"), find(&ui, "hierarchy-filter"), find(&ui, "hierarchy-add"));
        let hierarchy = Hierarchy::new(&mut ui, tree, filter, add);
        let project = ProjectView::new(&mut ui, document, game.project.name(), &root, game.content.db());
        let console_list = find(&ui, "console-list");
        let console = Console::new(&mut ui, console_list, log);
        let mut inspector = Inspector::new(find(&ui, "inspector"));
        inspector.set_types(script_types::cached(&game.project).unwrap_or_default());
        inspector.show(&mut ui, &game, None, false);
        let levels = LevelsPanel::new(&mut ui, document, &mut game.content);
        let cookbook_panel = find(&ui, "cookbook");
        let cookbook = voxear_cookbook::Cookbook::new(&mut ui, cookbook_panel, &root, game.project.name());
        ui.set_text(nodes.status_time, &format!("Opened in {:.2} s", started.elapsed().as_secs_f32()));

        tracing::info!("opened {} in edit mode", game.project.name());
        Ok(Self {
            game,
            ui,
            fly: FlyCamera::default(),
            nodes,
            hierarchy,
            project,
            inspector,
            console,
            cookbook,
            levels,
            camera,
            play: play::Play::new(),
            view_camera: Some(camera),
            viewport_area: None,
            framed: None,
            gizmo_shows: None,
            pointer: None,
            scale: 1.0,
            control: false,
            shift: false,
            commands: Vec::new(),
            kept: Vec::new(),
            tools: SceneTools::new(),
            hovered_handle: None,
            pressed_handle: None,
            released_handle: false,
            history: History::default(),
            unsaved: false,
            discard_armed: None,
            notice: None,
            status: (0, 0.0),
            describing: None,
            described_from: None,
            scripts_checked: None,
        })
    }

    /// Selects an Obj, as a click in the Hierarchy does, or nothing.
    pub fn select(&mut self, entity: Option<Entity>) {
        // An Obj made this frame is in the Hierarchy once it has looked again.
        self.hierarchy.update(&mut self.ui, &self.game.engine.world);
        match entity {
            Some(entity) => self.hierarchy.select(&mut self.ui, &self.game.engine.world, entity),
            None => self.hierarchy.clear(&mut self.ui),
        }
        let playing = self.is_playing();
        self.inspector.show(&mut self.ui, &self.game, entity, playing);
    }

    /// The Obj selected, as the Inspector shows it.
    pub fn selected(&self) -> Option<Entity> {
        self.inspector.shown()
    }

    /// The Scene view's camera over its rectangle, when the Scene view is
    /// showing: where points in the world fall on screen, and back.
    pub fn scene_camera(&self) -> Option<ViewCamera> {
        let rect = self.shown(self.nodes.viewport).then(|| self.ui.rect(self.nodes.viewport))?;
        ViewCamera::new(&self.game.engine.world, self.camera, rect)
    }

    /// The first Obj with a mesh, for showing something selected.
    pub fn first_mesh(&self) -> Option<Entity> {
        let world = &self.game.engine.world;
        let mut meshes: Vec<Entity> =
            world.query_ref::<(Entity, &MeshRenderer)>().without::<EditorOnly>().into_iter().map(|(e, _)| e).collect();
        meshes.sort_by_key(|e| e.index());
        meshes.first().copied()
    }

    /// One frame: input already given to the UI, then play mode, the game
    /// (held still while paused), panels, the 3D view's rectangle and
    /// camera, and the render.
    pub fn update(&mut self, renderer: &mut Renderer, size: (u32, u32), scale: f32, delta: f32) -> Result<()> {
        self.scale = scale;
        self.ui.advance(delta);
        self.update_play(renderer);
        if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.camera) {
            self.fly.update_transform(transform, delta);
        }
        if let Some(delta) = self.game_delta(delta) {
            self.game.tick(delta, Some(renderer));
        }

        self.hierarchy.update(&mut self.ui, &self.game.engine.world);
        let playing = self.is_playing();
        if let Some(selected) = self.hierarchy.selection_changed(&self.ui) {
            self.inspector.show(&mut self.ui, &self.game, selected, playing);
        }
        let edit = self.inspector.update(&mut self.ui, &mut self.game, Some(renderer), playing);
        // Edits while playing go with the played world, so there is nothing to undo or save.
        if let Some(edit) = edit.filter(|_| !self.is_playing()) {
            self.history.record(edit);
            self.unsaved = true;
        }
        if let Some(action) = self.hierarchy.action(&mut self.ui) {
            self.obj_action(action, renderer);
        }
        let shown = self.selected();
        let can_keep = shown.is_some_and(|entity| self.edited_counterpart(entity).is_some());
        let kept: Vec<Field> = self.kept.iter().filter(|(e, _)| Some(*e) == shown).map(|(_, f)| f.clone()).collect();
        if let Some(action) = self.inspector.menus(&mut self.ui, &self.game, playing, can_keep, &kept) {
            self.row_action(action, renderer);
        }
        for command in std::mem::take(&mut self.commands) {
            self.run(command, renderer);
        }
        self.update_scene_tools(renderer);
        self.update_script_types();
        self.project.update(&mut self.ui);
        self.console.update(&mut self.ui);
        self.cookbook.update(&mut self.ui);
        let action = self.levels.update(&mut self.ui, self.game.levels.current(), self.game.loads.level_in_progress());
        if let Some(action) = action {
            self.level_action(action, renderer);
        }
        if let Some(path) = self.cookbook.take_file_request() {
            // A link to a file in the project shows it in the Project panel.
            self.ui.show_dock_panel(self.nodes.dock, "project");
            self.project.reveal(&mut self.ui, &path);
        }
        if self.ui.clicked(self.nodes.build) {
            tracing::info!("Build comes with build profiles, later in M3");
        }
        let view = self.active_view();
        let game_camera = self.game_camera();
        self.ui.set_visible(self.nodes.game_empty, game_camera.is_none());
        self.mark_viewport_area(view.map(|(viewport, _)| viewport));
        self.update_gizmo();
        self.update_status(renderer, delta);

        let camera = view.map(|(_, camera)| camera);
        if camera.is_some() && camera != self.view_camera {
            renderer.set_view_options(ViewOptions { camera, ..ViewOptions::default() });
            self.view_camera = camera;
        }
        let frame = self.ui.frame(size.0, size.1, scale);
        renderer.set_view_rect(Some(self.viewport_rect(view.map(|(viewport, _)| viewport), size)));
        renderer.set_ui(&frame)?;
        renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)
    }

    /// What a click in the Levels panel asked for. Playing, a level is
    /// changed to as the game would change to it; stopped, it is opened to
    /// edit, and Play opens it and plays.
    fn level_action(&mut self, action: LevelAction, renderer: &mut Renderer) {
        let playing = self.is_playing();
        let target = match &action {
            LevelAction::Open(path) | LevelAction::OpenScene(path) | LevelAction::Play(path) => path.clone(),
        };
        if !playing && self.unsaved && self.discard_armed.as_deref() != Some(target.as_str()) {
            self.notify("Unsaved changes: Ctrl+S keeps them, or click again to open without them");
            self.discard_armed = Some(target);
            return;
        }
        match action {
            LevelAction::Open(level) if playing => {
                if let Err(error) = self.game.switch_level(&level, Some(renderer)) {
                    tracing::error!("could not change to {level}: {error:#}");
                }
            }
            LevelAction::Open(level) => self.open_level(&level, renderer),
            LevelAction::OpenScene(scene) if playing => {
                tracing::info!("{scene} is in no level; stop playing to open it on its own");
            }
            LevelAction::OpenScene(scene) => {
                let game = &mut self.game;
                let opened = game.levels.open_scene(&mut game.engine, &mut game.content, Some(&mut *renderer), &scene);
                match opened {
                    Ok(_) => self.opened(renderer),
                    Err(error) => tracing::error!("could not open {scene}: {error:#}"),
                }
            }
            LevelAction::Play(level) => {
                if playing {
                    self.stop(renderer);
                }
                self.open_level(&level, renderer);
                self.play(renderer);
            }
        }
    }

    /// Opens a level to edit in place of the one open now.
    fn open_level(&mut self, level: &str, renderer: &mut Renderer) {
        match self.game.load_level(level, Some(&mut *renderer)) {
            Ok(()) => self.opened(renderer),
            Err(error) => tracing::error!("could not open {level}: {error:#}"),
        }
    }

    /// After another level or scene opened: nothing is selected, and what
    /// it no longer uses is let go.
    fn opened(&mut self, renderer: &mut Renderer) {
        self.history.clear();
        self.unsaved = false;
        self.discard_armed = None;
        self.game.release_unused(Some(renderer));
        self.inspector.show(&mut self.ui, &self.game, None, false);
        tracing::info!("opened {}", self.game.levels.current().unwrap_or("a scene"));
    }

    /// The Scene view's tools: the tool strip's settings, a click to
    /// select, the handles under the pointer, and dragging them.
    fn update_scene_tools(&mut self, renderer: &Renderer) {
        self.tools.tool = Tool::from_index(self.ui.segment(self.nodes.tool));
        self.update_tool_strip();
        let selected = self.inspector.shown();
        let Some(camera) = self.scene_camera() else {
            self.hovered_handle = None;
            self.ui.set_shapes(self.nodes.handles, Vec::new());
            return;
        };
        let point = self.pointer.map(|(x, y)| [x / self.scale, y / self.scale]);
        let world = &self.game.engine.world;
        if let (Some(point), Some(entity)) = (self.pressed_handle.take(), selected)
            && let Some(handle) = self.tools.handle_at(world, renderer, &camera, entity, point)
        {
            self.tools.begin(world, renderer, &camera, entity, handle, point);
        }
        if self.tools.is_dragging()
            && let Some(point) = point
        {
            self.tools.drag_to(&mut self.game.engine.world, &camera, point, self.control);
        }
        if std::mem::take(&mut self.released_handle) {
            self.end_drag();
        }
        if !self.tools.is_dragging() {
            let over = point.filter(|_| self.over_viewport());
            let world = &self.game.engine.world;
            self.hovered_handle =
                over.zip(selected).and_then(|(p, e)| self.tools.handle_at(world, renderer, &camera, e, p));
            self.tools.hover(self.hovered_handle);
            self.click_to_select(renderer, &camera, point);
        }
        let shapes = self.tools.shapes(&self.game.engine.world, renderer, &camera, self.selected());
        self.ui.set_shapes(self.nodes.handles, shapes);
    }

    /// A click on the 3D view itself, not on what floats over it, selects
    /// the Obj under it, or nothing.
    fn click_to_select(&mut self, renderer: &Renderer, camera: &ViewCamera, point: Option<[f32; 2]>) {
        let (viewport, overlays) =
            (self.nodes.viewport, [self.nodes.tool_strip, self.nodes.stats, self.nodes.view_gizmo]);
        if !self.ui.clicked(viewport) || overlays.iter().any(|&o| self.ui.clicked(o)) {
            return;
        }
        let Some(point) = point else { return };
        let picked = pick(&self.game.engine.world, renderer, camera, point);
        self.select(picked);
    }

    /// Lets go of a handle; the drag is one step to undo.
    fn end_drag(&mut self) {
        let Some((entity, start)) = self.tools.end() else { return };
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        let before = ObjState { transform: start, ..after.clone() };
        if before != after && !self.is_playing() {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Makes, duplicates, deletes or reparents Objs, as the Hierarchy asks.
    /// Each is a step to undo, except while playing, when only the played
    /// world changes.
    fn obj_action(&mut self, action: ObjAction, renderer: &mut Renderer) {
        let position = self.new_obj_position();
        let game = &mut self.game;
        let (edit, selected) = match action {
            ObjAction::Create { kind, parent } => match objs::create(game, Some(renderer), kind, parent, position) {
                Ok(made) => (Edit::Exists { root: made, taken: None }, Some(made)),
                Err(error) => return tracing::error!("could not make the Obj: {error:#}"),
            },
            ObjAction::Duplicate(entity) => match objs::duplicate(game, Some(renderer), entity) {
                Ok(copy) => (Edit::Exists { root: copy, taken: None }, Some(copy)),
                Err(error) => return tracing::error!("could not duplicate the Obj: {error:#}"),
            },
            ObjAction::Delete(entity) => {
                let taken = objs::take(game, entity);
                (Edit::Exists { root: entity, taken: Some(taken) }, None)
            }
            ObjAction::Reparent(entity, parent) => match objs::reparent(game, entity, parent) {
                Some((before, after)) => (Edit::Moved { entity, before, after }, Some(entity)),
                None => return,
            },
        };
        if !self.is_playing() {
            self.history.record_step(edit);
            self.unsaved = true;
        }
        self.select(selected);
    }

    /// Where a new Obj goes: a little in front of the Scene camera, on the
    /// snap grid.
    fn new_obj_position(&self) -> Vec3 {
        let Some(view) = self.game.engine.world.get::<Transform>(self.camera) else { return Vec3::ZERO };
        let at = view.position + view.rotation * Vec3::Z * NEW_OBJ_DISTANCE;
        (at / self.tools.snap).round() * self.tools.snap
    }

    /// Does what a menu in the Inspector asks: keeps a value after play or
    /// stops keeping it, resets a value or the Transform, or adds or removes
    /// a component. Each change is a step to undo, except while playing.
    fn row_action(&mut self, action: RowAction, renderer: &mut Renderer) {
        let Some(entity) = self.selected() else { return };
        if let RowAction::Keep(field) = action {
            let label = field.label();
            let kept = (entity, field);
            match self.kept.iter().position(|k| *k == kept) {
                Some(at) => {
                    self.kept.remove(at);
                    self.notify(&format!("{label} will not be kept after play"));
                }
                None => {
                    self.kept.push(kept);
                    self.notify(&format!("{label} will be kept when play stops"));
                }
            }
            return;
        }
        let Some(before) = ObjState::of(&self.game.engine.world, entity) else { return };
        match action {
            RowAction::Keep(_) => {}
            RowAction::Reset(Field::Prop(prop)) => {
                props::reset_field(&mut self.game.engine.world, entity, &prop);
            }
            RowAction::Reset(field) => {
                let mut after = before.clone();
                field.reset(&mut after);
                after.apply(&mut self.game.engine.world, entity);
            }
            RowAction::ResetTransform => {
                let mut after = before.clone();
                after.transform = Transform::IDENTITY;
                after.apply(&mut self.game.engine.world, entity);
            }
            RowAction::Add(part) => {
                if let Err(error) = props::add(&mut self.game, Some(renderer), entity, &part) {
                    tracing::error!("could not add {}: {error:#}", part.title());
                }
            }
            RowAction::Remove(part) => props::remove(&mut self.game.engine.world, entity, &part),
            RowAction::Pick(prop, pick) => {
                props::pick(&mut self.game.engine.world, self.inspector.types(), entity, &prop, &pick);
            }
        }
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        if before != after && !self.is_playing() {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Keeps the scripts' description as their last build: when a script
    /// changes, they build off the main thread and are described again, and
    /// the Inspector shows the new fields. A build that fails is tried again
    /// only once a script changes again.
    fn update_script_types(&mut self) {
        if let Some(describing) = &self.describing {
            match describing.try_recv() {
                Ok(Ok(types)) => {
                    self.inspector.set_types(types);
                    self.describing = None;
                }
                Ok(Err(error)) => {
                    tracing::warn!("the scripts could not be described: {error:#}");
                    self.notify("The scripts failed to build; their fields show as they last built");
                    self.describing = None;
                }
                Err(TryRecvError::Disconnected) => self.describing = None,
                Err(TryRecvError::Empty) => {}
            }
            return;
        }
        if self.scripts_checked.is_some_and(|at| at.elapsed() < SCRIPTS_CHECK) {
            return;
        }
        self.scripts_checked = Some(Instant::now());
        let project = &self.game.project;
        let changed = self.described_from.is_none_or(|from| script_types::changed_since(project, from));
        if !changed || !script_types::is_stale(project) {
            return;
        }
        self.described_from = Some(SystemTime::now());
        let root = project.root.clone();
        let (done, describing) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(Project::open(&root).and_then(|project| script_types::describe(&project)));
        });
        self.describing = Some(describing);
    }

    /// Whether the scripts are described as they are now, with no build of
    /// them under way.
    pub fn scripts_described(&self) -> bool {
        self.describing.is_none() && !script_types::is_stale(&self.game.project)
    }

    /// Pivot or Center, Global or Local, and the snap distance, as the
    /// tool strip's buttons toggle them.
    fn update_tool_strip(&mut self) {
        if self.ui.clicked(self.nodes.pivot) {
            self.tools.centred = !self.tools.centred;
            self.ui.set_text(self.nodes.pivot, if self.tools.centred { "Center" } else { "Pivot" });
        }
        if self.ui.clicked(self.nodes.space) {
            self.tools.local = !self.tools.local;
            self.ui.set_text(self.nodes.space, if self.tools.local { "Local" } else { "Global" });
        }
        if self.ui.clicked(self.nodes.snap) {
            let steps = [0.1, 0.25, 0.5, 1.0];
            let next =
                steps.iter().position(|s| (*s - self.tools.snap).abs() < 1e-4).map_or(0, |i| (i + 1) % steps.len());
            self.tools.snap = steps[next];
            if let Some(&label) = self.ui.children(self.nodes.snap).last() {
                self.ui.set_text(label, &format!("{} m", self.tools.snap));
            }
        }
    }

    /// Saves, undoes or redoes, as a shortcut asked.
    fn run(&mut self, command: Command, renderer: &mut Renderer) {
        let selected = self.selected();
        match command {
            Command::Duplicate => {
                if let Some(entity) = selected {
                    self.obj_action(ObjAction::Duplicate(entity), renderer);
                }
            }
            Command::Delete => {
                if let Some(entity) = selected {
                    self.obj_action(ObjAction::Delete(entity), renderer);
                }
            }
            _ if self.is_playing() => {
                self.notify("Stop playing to save or undo; edits while playing are not kept");
            }
            Command::Save => self.save(),
            Command::Undo | Command::Redo => {
                // A field being typed in would keep its text over the undone value.
                self.ui.blur();
                let game = &mut self.game;
                let step = match command {
                    Command::Undo => self.history.undo(game, Some(renderer)),
                    _ => self.history.redo(game, Some(renderer)),
                };
                match step {
                    Some(selected) => {
                        self.select(selected);
                        self.unsaved = true;
                    }
                    None => self.notify(if command == Command::Undo { "Nothing to undo" } else { "Nothing to redo" }),
                }
            }
        }
    }

    /// Writes the open level's scenes that changed back to their files.
    pub fn save(&mut self) {
        let game = &mut self.game;
        let saved = game.levels.save(&game.engine, &mut game.content, &game.project.root);
        match saved {
            Ok(written) if written.is_empty() => self.notify("Nothing to save: the scenes are as they are on disk"),
            Ok(written) => {
                tracing::info!("saved {}", written.join(", "));
                let names: Vec<&str> = written.iter().map(|path| path.rsplit('/').next().unwrap_or(path)).collect();
                self.notify(&format!("Saved {}", names.join(", ")));
            }
            Err(error) => {
                tracing::error!("could not save: {error:#}");
                self.notify("Could not save; the Console says why");
                return;
            }
        }
        self.unsaved = false;
        self.discard_armed = None;
    }

    /// Takes back the last edit, as Ctrl+Z does.
    pub fn undo(&mut self, renderer: &mut Renderer) {
        self.run(Command::Undo, renderer);
    }

    /// Makes the last undone edit again, as Ctrl+Shift+Z does.
    pub fn redo(&mut self, renderer: &mut Renderer) {
        self.run(Command::Redo, renderer);
    }

    /// Whether the open level has edits not saved.
    pub fn has_unsaved_changes(&self) -> bool {
        self.unsaved
    }

    /// Shows a message in the status bar for a few seconds.
    fn notify(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now()));
    }

    /// Whether a physical point is over the Scene view, where the right
    /// mouse button flies the camera.
    pub fn over_viewport(&self) -> bool {
        let Some((x, y)) = self.pointer else { return false };
        let Some((viewport, _)) = self.active_view().filter(|(viewport, _)| *viewport == self.nodes.viewport) else {
            return false;
        };
        let [vx, vy, vw, vh] = self.viewport_rect(Some(viewport), (u32::MAX, u32::MAX));
        (vx as f32..(vx + vw) as f32).contains(&x) && (vy as f32..(vy + vh) as f32).contains(&y)
    }

    /// Where the 3D view goes this frame, and through which camera: the Game
    /// view when it is showing and the level has a camera (first, while
    /// playing), otherwise the Scene view, through the editor's camera.
    fn active_view(&self) -> Option<(NodeId, Entity)> {
        let game = self.game_camera().filter(|_| self.shown(self.nodes.game_viewport));
        let scene = self.shown(self.nodes.viewport).then_some(self.camera);
        match (game, scene) {
            (Some(game), Some(_)) if self.is_playing() => Some((self.nodes.game_viewport, game)),
            (_, Some(scene)) => Some((self.nodes.viewport, scene)),
            (Some(game), None) => Some((self.nodes.game_viewport, game)),
            (None, None) => None,
        }
    }

    /// The camera the game looks through: the level's first.
    fn game_camera(&self) -> Option<Entity> {
        let world = &self.game.engine.world;
        let cameras = world.query_ref::<(Entity, &Camera)>().without::<EditorOnly>().into_iter();
        cameras.map(|(entity, _)| entity).min_by_key(|entity| entity.index())
    }

    /// Gives the UI a window event; returns whether it used it. Ctrl+K goes
    /// to the command bar.
    pub fn handle(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::{ElementState, WindowEvent};
        match event {
            WindowEvent::CursorMoved { position, .. } => self.pointer = Some((position.x as f32, position.y as f32)),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.control = modifiers.state().control_key() || modifiers.state().super_key();
                self.shift = modifiers.state().shift_key();
            }
            _ => {}
        }
        if let WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } = event {
            let pressed = *state == ElementState::Pressed;
            // A handle takes the press before the UI sees it, so it is not also a click.
            if pressed && self.hovered_handle.is_some() && self.over_viewport() {
                self.pressed_handle = self.pointer.map(|(x, y)| [x / self.scale, y / self.scale]);
                return true;
            }
            // A press and release inside one frame still ends the drag the press begins.
            if !pressed && (self.tools.is_dragging() || self.pressed_handle.is_some()) {
                self.released_handle = true;
                return true;
            }
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && !self.ui.wants_keyboard()
            && key.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Delete)
        {
            self.commands.push(Command::Delete);
            return true;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && !self.control
            && !self.ui.wants_keyboard()
            && let winit::keyboard::Key::Character(text) = &key.logical_key
            && let Some(index) = ["q", "w", "e", "r"].iter().position(|k| *k == text.to_lowercase())
        {
            // Q, W, E and R pick the tool, as in Unity.
            self.ui.set_segment(self.nodes.tool, index);
            return true;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && self.control
            && let winit::keyboard::Key::Character(text) = &key.logical_key
        {
            let command = match (text.to_lowercase().as_str(), self.shift) {
                ("s", _) => Some(Command::Save),
                ("d", _) => Some(Command::Duplicate),
                ("z", false) => Some(Command::Undo),
                ("z", true) | ("y", _) => Some(Command::Redo),
                ("k", _) => {
                    self.ui.focus(self.nodes.command);
                    self.ui.key_down(voxear_ui::Key::Char('a'));
                    return true;
                }
                _ => None,
            };
            if let Some(command) = command {
                self.commands.push(command);
                return true;
            }
        }
        voxear_engine::ui_input::handle(&mut self.ui, event)
    }

    /// A viewport's rectangle in physical pixels, within `size`; a pixel
    /// in the corner when no view shows.
    fn viewport_rect(&self, viewport: Option<NodeId>, size: (u32, u32)) -> [u32; 4] {
        let Some(viewport) = viewport else { return [0, 0, 1, 1] };
        let [x, y, w, h] = self.ui.rect(viewport);
        let s = self.scale;
        let (x, y) = ((x * s).round().max(0.0) as u32, (y * s).round().max(0.0) as u32);
        let (w, h) = ((w * s).round().max(1.0) as u32, (h * s).round().max(1.0) as u32);
        [x.min(size.0.saturating_sub(1)), y.min(size.1.saturating_sub(1)), w, h]
    }

    /// Whether a view's panel is the tab showing in its area.
    fn shown(&self, viewport: NodeId) -> bool {
        let mut at = Some(viewport);
        while let Some(node) = at {
            if !self.ui.is_visible(node) {
                return false;
            }
            at = self.ui.parent(node);
        }
        true
    }

    /// Makes the dock area around the 3D view see-through, wherever its
    /// panel has been docked, and lays the frame over its edge, which rounds
    /// the view's corners to the panel's.
    fn mark_viewport_area(&mut self, viewport: Option<NodeId>) {
        let mut at = viewport.and_then(|viewport| self.ui.parent(viewport));
        let mut area = None;
        while let Some(node) = at {
            if self.ui.has_class(node, "dock-area") {
                area = Some(node);
                break;
            }
            at = self.ui.parent(node);
        }
        if area != self.viewport_area {
            if let Some(old) = self.viewport_area {
                self.ui.remove_class(old, "viewport-area");
            }
            if let Some(new) = area {
                self.ui.add_class(new, "viewport-area");
            }
            self.viewport_area = area;
        }
        let rect = area.map(|area| self.ui.rect(area));
        if rect != self.framed {
            let style = match rect {
                Some([x, y, w, h]) => format!("display: flex; left: {x}px; top: {y}px; width: {w}px; height: {h}px"),
                None => "display: none".to_string(),
            };
            self.ui.set_inline_style(self.nodes.frame, &style);
            self.framed = rect;
        }
    }

    /// Turns the axis gizmo with the Scene camera: each world axis drawn
    /// where the camera sees it, the ones nearer the viewer on top, and a
    /// faint dot where each points away.
    fn update_gizmo(&mut self) {
        let Some(rotation) = self.game.engine.world.get::<Transform>(self.camera).map(|t| t.rotation) else {
            return;
        };
        if self.gizmo_shows == Some(rotation) {
            return;
        }
        self.gizmo_shows = Some(rotation);
        let (right, up, forward) = (rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z);
        let colors = [0xeb5757, 0x4cb782, 0x5e8bf0].map(Color::hex);
        let mut axes: Vec<(usize, [f32; 2], f32)> = [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .enumerate()
            .map(|(i, axis)| (i, [axis.dot(right), -axis.dot(up)], axis.dot(forward)))
            .collect();
        // Furthest first, so the nearest draw last.
        axes.sort_by(|a, b| b.2.total_cmp(&a.2));
        let middle = [GIZMO_MIDDLE, GIZMO_MIDDLE];
        let mut shapes = Vec::new();
        for &(i, [x, y], _) in &axes {
            let back = [middle[0] - x * GIZMO_REACH, middle[1] - y * GIZMO_REACH];
            let faded = colors[i].with_alpha(0x66);
            shapes.push(Shape::Circle { center: back, radius: GIZMO_END - 0.5, fill: faded, border: None });
        }
        for &(i, [x, y], _) in &axes {
            let end = [middle[0] + x * GIZMO_REACH, middle[1] + y * GIZMO_REACH];
            shapes.push(Shape::Line { from: middle, to: end, width: 2.0, color: colors[i] });
            shapes.push(Shape::Circle { center: end, radius: GIZMO_END, fill: colors[i], border: None });
        }
        self.ui.set_shapes(self.nodes.gizmo, shapes);
    }

    fn update_status(&mut self, renderer: &Renderer, delta: f32) {
        let notice = self.notice.as_ref().filter(|(_, at)| at.elapsed().as_secs_f32() < NOTICE_SECONDS);
        let status = match notice {
            Some((text, _)) => text.clone(),
            None if self.unsaved && !self.is_playing() => "Unsaved changes · Ctrl+S".to_string(),
            None => self.play.status().to_string(),
        };
        self.ui.set_text(self.nodes.status_left, &status);
        self.status.0 += 1;
        self.status.1 += delta;
        if self.status.1 < STATUS_INTERVAL && self.status.0 > 1 {
            return;
        }
        let fps = self.status.0 as f32 / self.status.1.max(1e-3);
        self.status = (0, 0.0);
        self.ui.set_text(self.nodes.status_fps, &format!("{fps:.0} fps"));
        let memory = resident_megabytes().map(|mb| format!("{mb:.0} MB")).unwrap_or_default();
        self.ui.set_text(self.nodes.status_memory, &memory);
        let stats = renderer.stats();
        let gpu_ms: f32 = renderer.timings().iter().map(|(_, ms)| ms).sum();
        let drawn = stats.drawn_early + stats.drawn_late;
        let chip = format!("{} objs   {drawn} draws   {gpu_ms:.2} ms GPU", stats.instances);
        self.ui.set_text(self.nodes.stats, &chip);
    }
}

/// The Scene view's camera: where the level's own camera is, or a little
/// back from the origin.
fn spawn_camera(game: &mut Game) -> Entity {
    let world = &mut game.engine.world;
    let start = world.query_ref::<(&Camera, &Transform)>().into_iter().next().map(|(c, t)| (*c, *t));
    let (camera, transform) = start.unwrap_or_else(|| {
        let transform = Transform::new(Vec3::new(0.0, 2.0, -8.0), Quat::from_rotation_x(0.15), Vec3::ONE);
        (Camera::default(), transform)
    });
    world.spawn((Name("Scene Camera".into()), transform, camera, EditorOnly))
}

/// The checked-out git branch of the repository the project is in.
fn git_branch(root: &Path) -> Option<String> {
    let mut dir = Some(root);
    while let Some(at) = dir {
        if let Ok(head) = std::fs::read_to_string(at.join(".git").join("HEAD")) {
            return Some(head.trim().strip_prefix("ref: refs/heads/").unwrap_or("detached").to_string());
        }
        dir = at.parent();
    }
    None
}

/// The first letter of the user's name, for the avatar.
fn initial() -> String {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    user.chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect())
}

/// How much memory the editor holds, from Linux's /proc.
fn resident_megabytes() -> Option<f32> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: f32 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096.0 / 1_048_576.0)
}
