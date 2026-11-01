//! The editor itself: the open project in edit mode, its panels, the Scene
//! view, which looks through the editor's own camera, and the Game view,
//! which looks through the level's. Play mode is in [`play`].

mod color_popup;
mod debug_texts;
mod docs_links;
mod docs_pictures;
mod docs_steps;
mod drawer;
mod play;
mod prefab_mode;
mod previews;
mod scene_view;
mod tooltip;
mod windows;

pub use drawer::Drawer;

use std::{
    path::Path,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec3},
    profile_scope, profiling,
};
use voxear_ecs::Entity;
use voxear_engine::{
    AssetPreviews, Engine, FlyCamera, Game, Project,
    debug_texts::DebugTexts,
    script_types::{self, ScriptTypes},
};
use voxear_render::{Camera, MeshRenderer, Renderer, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_ui::{Color, DragSource, NodeId, Shape, Theme, Ui};

use crate::{
    asset_inspector::AssetAction,
    build_window::BuildWindow,
    console::Console,
    entities_panel::EntitiesPanel,
    fields::Pick,
    frame_debugger_panel::FrameDebuggerPanel,
    gizmos,
    hierarchy::EditorOnly,
    hierarchy::Hierarchy,
    hierarchy::ObjAction,
    history::{Edit, History, ObjState},
    inspector::{Field, Inspector, RowAction},
    levels_panel::{LevelAction, LevelsPanel},
    object_picker::{self, ObjectPicker},
    objs,
    palette::{Added, Keys, Palette, PaletteAction, PaletteCommand, PaletteKey, Picked, Sources},
    profiler::{self, Profiler},
    project_view::{ItemKind, ProjectEvent, ProjectItem, ProjectView},
    props::{self, Part, Prop},
    scene_tools::{Handle, SceneTools, Tool, ViewCamera, pick},
    settings::{self, Memory, SNAPS, SettingKind, SettingValue, Settings},
    settings_panel::SettingsPanel,
};

const MARKUP: &str = include_str!("../ui/Editor.vui");
const SHEET: &str = include_str!("../ui/Editor.vss");
/// The layout a project opens with until it has one of its own: the
/// Hierarchy on the left, the Scene in the middle over the Project and
/// Console, and the Inspector on the right. The Project sits under the Scene,
/// as in Unity, because its folders and files need the width.
const LAYOUT: &str =
    "row(240, [hierarchy], row(-300, column(66%, [scene, game, docs], [project, console]), [inspector]))";
/// How long a message stays in the status bar.
const NOTICE_SECONDS: f32 = 4.0;
/// How often the status bar's numbers change: each is the average over this
/// long, so they are steady enough to read.
const STATUS_INTERVAL: f32 = 1.0;
/// A frame rate within this share of the one shown is not worth changing the
/// number for, so it holds still while the rate wobbles.
const FPS_STEADY: f32 = 0.03;
/// How much each second's frame rate moves the one shown, which follows the
/// trend rather than each second's swing.
const FPS_FOLLOW: f32 = 0.35;
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
    /// The top bar's Window menu button.
    windows: NodeId,
    /// The rail's buttons, and the drawer they open with its title, close
    /// button and pages.
    rail_levels: NodeId,
    rail_settings: NodeId,
    drawer: NodeId,
    drawer_title: NodeId,
    drawer_close: NodeId,
    drawer_levels: NodeId,
    drawer_settings: NodeId,
    /// The Scene view's shading menu, Gizmos button and full screen button.
    shading: NodeId,
    gizmos: NodeId,
    scene_maximize: NodeId,
    /// The top bar's workspaces, and the Profiler's panel.
    workspaces: NodeId,
    profiler_panel: NodeId,
    /// The Entities panel's content, the ECS inspector.
    entities_panel: NodeId,
    frame_debugger_panel: NodeId,
    /// The Build window's panel.
    build_panel: NodeId,
    /// A script field's tooltip, shown under its label.
    tooltip: NodeId,
    /// Over the window, where debug draw's texts show.
    debug_texts: NodeId,
    /// The status bar's dot, which pulses while the scripts build.
    status_dot: NodeId,
    /// The bar over the Scene view while a prefab is open on its own.
    prefab_bar: NodeId,
    prefab_name: NodeId,
    prefab_back: NodeId,
}

/// What a shortcut asks of the editor, done in the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
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

/// Opens a file at a line, as clicking a stack frame in the Console does.
pub type SourceOpener = Box<dyn FnMut(&Path, u32)>;

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
    docs: voxear_docs::VoxDocs,
    /// The command palette, what its keys asked to run since the last frame,
    /// and commands added to it by code.
    palette: Palette,
    palette_runs: Vec<Picked>,
    added: Vec<PaletteCommand>,
    /// The settings and what the palette ran recently, kept per project, and
    /// the panel that shows the settings.
    memory: Memory,
    settings_panel: SettingsPanel,
    /// The theme the editor wears, which the default theme's own colours are
    /// until the settings ask for another.
    theme: Theme,
    /// The fly camera's speed as last seen, to tell the wheel's changes from
    /// the setting's.
    fly_speed: f32,
    /// Every level and loose scene, to open, play or change to.
    levels: LevelsPanel,
    /// The Scene view's own camera, which the Hierarchy leaves out.
    camera: Entity,
    /// Play mode, and the scripts it keeps between runs.
    play: play::Play,
    /// The camera the renderer was last told to look through, and the debug
    /// view it was told to show.
    view_camera: Option<Entity>,
    view_debug: u32,
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
    alt: bool,
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
    /// The panel whose tab's menu is open.
    tab_menu: Option<String>,
    /// What the rail's drawer shows, if it is open.
    drawer: Option<Drawer>,
    /// The prefab open on its own to edit, if one is.
    prefab: Option<prefab_mode::OpenPrefab>,
    /// The workspace showing, and the layout each was last left in.
    workspace: usize,
    workspace_layouts: [Option<String>; 2],
    /// When the layout was last looked at, to keep it in the memory.
    layout_checked: Instant,
    profiler: Profiler,
    entities: EntitiesPanel,
    frame_debugger: FrameDebuggerPanel,
    build_window: BuildWindow,
    /// A reference field's search, and what its keys picked, done next frame.
    picker: ObjectPicker,
    /// The tooltip the pointer rests on, and for how long, in seconds.
    hovered_tooltip: Option<(String, f32)>,
    /// Debug draw's texts, over the view showing.
    debug_texts: DebugTexts,
    /// The colour picker, opened from a colour's swatch.
    color_popup: color_popup::ColorPopup,
    picker_picks: Vec<(Entity, Prop, Pick)>,
    /// Previews of assets, of a Mesh Renderer's own mesh and material, and
    /// the Inspector's large one; see `previews`.
    asset_previews: AssetPreviews,
    /// Prefabs, models and materials VoxDocs pages show live.
    live_assets: Vec<docs_pictures::LiveAsset>,
    /// The fields of the live materials on the VoxDocs page read, in the
    /// places the page leaves for them.
    live_fields: Vec<(NodeId, crate::asset_inspector::AssetView)>,
    field_previews: previews::FieldPreviews,
    inspector_preview: Option<previews::InspectorPreview>,
    /// A material changed, so field previews are drawn again.
    redraw_field_previews: bool,
    /// While one panel fills the dock: the layout from before, and the
    /// panels that were closed then.
    maximized: Option<(String, Vec<String>)>,
    /// Frames run, which each log line notes, and what opens a stack frame's
    /// file, when not the code editor the settings name.
    frames: u64,
    source_opener: Option<SourceOpener>,
    /// Frames and seconds counted towards the status bar's next numbers, and
    /// the frame rate it shows.
    status: (u32, f32),
    average_fps: f32,
    shown_fps: f32,
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
        let memory = Memory::open(&project.root);
        Self::open_with(project, renderer, log, memory)
    }

    /// [`open`](Self::open), remembering nothing: the settings start as
    /// their defaults and nothing is written, as tests and screenshots want.
    pub fn open_without_memory(project: Project, renderer: &mut Renderer, log: LogCapture) -> Result<Self> {
        Self::open_with(project, renderer, log, Memory::forgetful())
    }

    fn open_with(project: Project, renderer: &mut Renderer, log: LogCapture, memory: Memory) -> Result<Self> {
        let started = Instant::now();
        let root = project.root.clone();
        let mut engine = Engine::new();
        engine.set_edit_mode(true);
        let mut game = Game::new(project, engine)?;
        game.start(Some(renderer)).context("opening the start level")?;
        let camera = spawn_camera(&mut game);
        renderer.set_view_options(ViewOptions { camera: Some(camera), ..ViewOptions::default() });
        // The window's ground, which the theme's base colours.
        renderer.set_clear_color(Theme::DARK.base.linear_premultiplied());

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
            windows: find(&ui, "windows"),
            rail_levels: find(&ui, "rail-levels"),
            rail_settings: find(&ui, "rail-settings"),
            drawer: find(&ui, "drawer"),
            drawer_title: find(&ui, "drawer-title"),
            drawer_close: find(&ui, "drawer-close"),
            drawer_levels: find(&ui, "drawer-levels"),
            drawer_settings: find(&ui, "drawer-settings"),
            shading: find(&ui, "shading"),
            gizmos: find(&ui, "gizmos"),
            scene_maximize: find(&ui, "scene-maximize"),
            workspaces: find(&ui, "workspaces"),
            profiler_panel: find(&ui, "profiler-panel"),
            entities_panel: find(&ui, "entities-panel"),
            frame_debugger_panel: find(&ui, "frame-debugger-panel"),
            build_panel: find(&ui, "build-panel"),
            tooltip: find(&ui, "tooltip"),
            debug_texts: find(&ui, "debug-texts"),
            status_dot: find(&ui, "status-dot"),
            prefab_bar: find(&ui, "prefab-bar"),
            prefab_name: find(&ui, "prefab-name"),
            prefab_back: find(&ui, "prefab-back"),
        };
        let branch = git_branch(&root);
        let branch_name = find(&ui, "status-branch-name");
        ui.set_text(branch_name, branch.as_deref().unwrap_or_default());
        let branch_shown = find(&ui, "status-branch");
        ui.set_visible(branch_shown, branch.is_some());
        ui.set_dock_layout_closing_rest(nodes.dock, LAYOUT).expect("the default layout is valid");
        ui.set_visible(nodes.drawer, false);
        ui.set_visible(nodes.prefab_bar, false);
        ui.set_visible(nodes.tooltip, false);
        let (tree, filter, add) =
            (find(&ui, "hierarchy-tree"), find(&ui, "hierarchy-filter"), find(&ui, "hierarchy-add"));
        let hierarchy = Hierarchy::new(&mut ui, tree, filter, add);
        let namespace = script_namespace(&game.project);
        let project = ProjectView::new(&mut ui, document, game.project.name(), &root, game.content.db(), &namespace);
        let console_panel = find(&ui, "console-panel");
        let console = Console::new(&mut ui, console_panel, log, &root, &memory.settings);
        let mut inspector = Inspector::new(find(&ui, "inspector"));
        inspector.set_types(script_types::cached(&game.project).unwrap_or_default());
        inspector.show(&mut ui, &game, None, false);
        let levels = LevelsPanel::new(&mut ui, document, &mut game.content);
        let docs_panel = find(&ui, "docs");
        let mut docs = voxear_docs::VoxDocs::new(&mut ui, docs_panel, &root, game.project.name());
        if let Some(code) = code_reference(&game.project) {
            docs.set_code(&mut ui, code);
        }
        let (panel, list, empty) = (find(&ui, "palette"), find(&ui, "palette-list"), find(&ui, "palette-empty"));
        let palette = Palette::new(&mut ui, nodes.command, panel, list, empty);
        let (tabs, view, filter) = (find(&ui, "settings-tabs"), find(&ui, "settings"), find(&ui, "settings-filter"));
        let settings_panel = SettingsPanel::new(&mut ui, tabs, view, filter, &memory.settings);
        let profiler = Profiler::new(&mut ui, nodes.profiler_panel);
        let entities = EntitiesPanel::new(&mut ui, nodes.entities_panel);
        let frame_debugger = FrameDebuggerPanel::new(&mut ui, nodes.frame_debugger_panel);
        let debug_texts = DebugTexts::new(&mut ui, nodes.debug_texts);
        let build_window = BuildWindow::new(&mut ui, nodes.build_panel);
        let parts =
            ["picker", "picker-title", "picker-field", "picker-list", "picker-empty"].map(|name| find(&ui, name));
        let picker = ObjectPicker::new(&mut ui, parts[0], parts[1], parts[2], parts[3], parts[4]);
        let (color_panel, color_title) = (find(&ui, "color-popup"), find(&ui, "color-title"));
        let color_popup = color_popup::ColorPopup::new(&mut ui, color_panel, color_title);
        let mut fly = FlyCamera::default();
        fly.set_speed(memory.settings.fly_speed);
        ui.set_text(nodes.status_time, &format!("Opened in {:.2} s", started.elapsed().as_secs_f32()));

        tracing::info!("opened {} in edit mode", game.project.name());
        let mut editor = Self {
            game,
            ui,
            fly,
            alt: false,
            nodes,
            hierarchy,
            project,
            inspector,
            console,
            docs,
            palette,
            palette_runs: Vec::new(),
            added: Vec::new(),
            fly_speed: memory.settings.fly_speed.clamp(0.25, 250.0),
            memory,
            settings_panel,
            theme: Theme::DARK,
            levels,
            camera,
            play: play::Play::new(),
            view_camera: Some(camera),
            view_debug: 0,
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
            tab_menu: None,
            drawer: None,
            prefab: None,
            workspace: 0,
            workspace_layouts: [None, None],
            profiler,
            entities,
            frame_debugger,
            build_window,
            picker,
            color_popup,
            hovered_tooltip: None,
            debug_texts,
            picker_picks: Vec::new(),
            asset_previews: AssetPreviews::default(),
            live_assets: Vec::new(),
            live_fields: Vec::new(),
            field_previews: previews::FieldPreviews::default(),
            inspector_preview: None,
            redraw_field_previews: false,
            maximized: None,
            status: (0, 0.0),
            frames: 0,
            source_opener: None,
            average_fps: 0.0,
            shown_fps: 0.0,
            describing: None,
            described_from: None,
            scripts_checked: None,
            layout_checked: Instant::now(),
        };
        editor.restore_layout();
        Ok(editor)
    }

    /// Selects an Obj, as a click in the Hierarchy does, or nothing.
    pub fn select(&mut self, entity: Option<Entity>) {
        // An Obj made this frame is in the Hierarchy once it has looked again.
        self.hierarchy.update(&mut self.ui, &self.game.engine.world, &self.game.levels, self.inspector.types());
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
        self.frames += 1;
        voxear_core::logging::set_frame(self.frames);
        self.ui.advance(delta);
        {
            profile_scope!("Game");
            self.update_play(renderer);
            if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.camera) {
                self.fly.update_transform(transform, delta);
            }
            if let Some(delta) = self.game_delta(delta) {
                self.game.tick(delta, Some(renderer));
            }
        }
        let view = {
            profile_scope!("Editor");
            self.update_panels(renderer, delta)
        };
        let frame = {
            profile_scope!("UI Layout");
            self.ui.frame(size.0, size.1, scale)
        };
        let rendered = {
            profile_scope!("Render");
            renderer.set_view_rect(Some(self.viewport_rect(view.map(|(viewport, _)| viewport), size)));
            {
                profile_scope!("UI Upload");
                renderer.set_ui(&frame)?;
                // The game's own UI, over the Game view while it plays.
                let in_game =
                    view.is_some_and(|(viewport, _)| viewport == self.nodes.game_viewport) && self.is_playing();
                let area =
                    if in_game { self.viewport_rect(Some(self.nodes.game_viewport), size) } else { [0, 0, 0, 0] };
                match in_game {
                    true => self.game.ui.draw(renderer, delta, area, scale)?,
                    false => renderer.set_game_ui(&voxear_ui::UiFrame::default(), [0, 0])?,
                }
            }
            renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)
        };
        self.profiler.record(profiling::end_frame(), delta * 1000.0, renderer);
        rendered
    }

    /// The panels' frame, each a profiling sample of its own. Returns the
    /// view the 3D renders into, with its camera.
    fn update_panels(&mut self, renderer: &mut Renderer, delta: f32) -> Option<(NodeId, Entity)> {
        let playing = self.is_playing();
        // Before the Inspector, so what is picked shows in this frame.
        self.update_picker(renderer);
        self.update_color_popup(renderer);
        {
            profile_scope!("Hierarchy");
            self.hierarchy.update(&mut self.ui, &self.game.engine.world, &self.game.levels, self.inspector.types());
            if let Some(selected) = self.hierarchy.selection_changed(&self.ui) {
                self.inspector.show(&mut self.ui, &self.game, selected, playing);
            }
        }
        {
            profile_scope!("Inspector");
            let edit = self.inspector.update(&mut self.ui, &mut self.game, Some(renderer), playing);
            // Edits while playing go with the played world, so there is nothing to undo or save.
            if let Some(edit) = edit.filter(|_| !self.is_playing()) {
                self.history.record(edit);
                self.unsaved = true;
            }
            self.update_docs_links();
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
        self.update_windows();
        self.update_workspaces();
        self.keep_layout();
        {
            profile_scope!("Profiler");
            let profiler_shown = self.shown(self.nodes.profiler_panel);
            self.profiler.update(&mut self.ui, profiler_shown, &self.game);
            let entities_shown = self.shown(self.nodes.entities_panel);
            let frame = self.profiler.shown_sample().map(|sample| &sample.cpu);
            let types = self.inspector.types();
            if let Some(picked) = self.entities.update(&mut self.ui, entities_shown, &mut self.game, types, frame) {
                self.select(Some(picked));
            }
            let frames_shown = self.shown(self.nodes.frame_debugger_panel);
            self.frame_debugger.update(&mut self.ui, frames_shown, renderer);
        }
        self.update_drawer();
        self.update_prefab_mode(renderer);
        self.update_settings();
        let theme = self.memory.settings.theme();
        if theme != self.theme {
            self.ui.set_theme(&theme);
            renderer.set_clear_color(theme.base.linear_premultiplied());
            self.theme = theme;
        }
        {
            profile_scope!("Scene Tools");
            self.update_scene_tools(renderer);
        }
        self.update_script_types();
        {
            profile_scope!("Project");
            for event in self.project.update(&mut self.ui) {
                self.project_event(event, renderer);
            }
            if let Some(action) = self.inspector.update_asset(&self.ui) {
                self.asset_action(action, renderer);
            }
        }
        {
            profile_scope!("Console");
            self.update_console();
        }
        {
            profile_scope!("VoxDocs");
            self.docs.update(&mut self.ui);
            self.update_docs_pictures(renderer);
            self.update_docs_steps(renderer);
        }
        self.update_palette(renderer);
        let action = self.levels.update(&mut self.ui, self.game.levels.current(), self.game.loads.level_in_progress());
        if let Some(action) = action {
            self.level_action(action, renderer);
        }
        if let Some(path) = self.docs.take_file_request() {
            // A link to a file in the project shows it in the Project panel.
            self.show_panel("project");
            self.project.reveal(&mut self.ui, &path);
        }
        if self.ui.clicked(self.nodes.build) {
            self.show_panel("build");
        }
        {
            profile_scope!("Build");
            let shown = self.shown(self.nodes.build_panel);
            let root = self.game.project.root.clone();
            if let Some(path) = self.build_window.take_file_request() {
                // A log line naming a file in the project shows it in the Project panel.
                self.show_panel("project");
                self.project.reveal(&mut self.ui, &path);
            }
            if let Some(finished) = self.build_window.update(&mut self.ui, &root, shown) {
                match finished.succeeded {
                    true => tracing::info!("{}", finished.text),
                    false => tracing::warn!("{}", finished.text),
                }
                self.notify(&finished.text);
            }
        }
        {
            profile_scope!("Previews");
            self.update_previews(renderer);
        }
        let view = self.active_view();
        let game_camera = self.game_camera();
        self.ui.set_visible(self.nodes.game_empty, game_camera.is_none());
        self.mark_viewport_area(view.map(|(viewport, _)| viewport));
        self.update_gizmo();
        self.update_status(renderer, delta);
        self.update_tooltip(delta);
        self.update_debug_draw(view, delta);

        let camera = view.map(|(_, camera)| camera);
        // The Scene view shades as its menu says; the Game view always lit.
        let in_scene = view.is_some_and(|(viewport, _)| viewport == self.nodes.viewport);
        let debug_view = if in_scene { self.scene_debug_view() } else { 0 };
        if camera.is_some() && (camera != self.view_camera || debug_view != self.view_debug) {
            renderer.set_view_options(ViewOptions { camera, debug_view, ..ViewOptions::default() });
            (self.view_camera, self.view_debug) = (camera, debug_view);
        }
        view
    }

    /// The Profiler panel, and the frames it has taken.
    pub fn profiler(&self) -> &Profiler {
        &self.profiler
    }

    /// The Entities panel, the ECS inspector.
    pub fn entities_panel(&self) -> &EntitiesPanel {
        &self.entities
    }

    /// The Build window, and the build it runs.
    pub fn build_window(&mut self) -> &mut BuildWindow {
        &mut self.build_window
    }

    /// What the Project panel asked for: an item shown in the Inspector, a
    /// file opened, or the files looked at again after it changed them.
    fn project_event(&mut self, event: ProjectEvent, renderer: &mut Renderer) {
        match event {
            ProjectEvent::Selected(item) => self.inspect_asset(&item),
            ProjectEvent::Open(item) => self.open_item(&item, renderer),
            ProjectEvent::Changed { select, rename } => {
                if let Err(error) = self.game.content.rescan() {
                    tracing::warn!("could not look at the project's files again: {error:#}");
                }
                // Files moved or went, so previews are made again for the paths they have now.
                self.asset_previews.clear(renderer);
                self.project.refresh(&mut self.ui, self.game.content.db(), select.as_deref());
                if let Some(item) = select.and_then(|path| self.project.item(&path)) {
                    self.inspect_asset(&item);
                }
                if rename {
                    self.project.start_rename(&mut self.ui);
                }
            }
            ProjectEvent::Notice(text) => self.notify(&text),
            ProjectEvent::Dropped { source, folder } => {
                if let Some(entity) = self.hierarchy.dragged(&source) {
                    self.make_prefab(entity, &folder, renderer);
                }
            }
        }
    }

    /// Makes a prefab of an Obj dragged onto the Project panel, in `folder`,
    /// and puts an instance of it in the Obj's place, as one step to undo.
    fn make_prefab(&mut self, entity: Entity, folder: &str, renderer: &mut Renderer) {
        if self.is_playing() {
            return self.notify("Prefabs are made from the edited level, not while playing");
        }
        let name = self.game.engine.world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_else(|| "Obj".into());
        let stem: String = name.chars().filter(|c| !r#"/\:*?"<>|"#.contains(*c)).collect();
        let root = self.game.project.root.clone();
        let path = crate::project_files::free_path(&root, folder, stem.trim(), ".prefab");
        match objs::make_prefab(&mut self.game, Some(renderer), entity, &root.join(&path), &path) {
            Ok((instance, taken)) => {
                let made = Edit::Exists { root: instance, taken: None };
                let replaced = Edit::Exists { root: entity, taken: Some(taken) };
                self.history.record_step(Edit::Group(vec![made, replaced]));
                self.unsaved = true;
                self.select(Some(instance));
                self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
                self.notify(&format!("Made the prefab {path}"));
            }
            Err(error) => self.notify(&format!("Could not make a prefab of {name}: {error:#}")),
        }
    }

    /// The Project panel's item the Inspector shows, if it shows one.
    pub fn inspected_asset(&self) -> Option<&ProjectItem> {
        self.inspector.shown_asset()
    }

    /// Shows an item of the Project panel in the Inspector, in place of the
    /// Obj selected, as selecting one in Unity's Project window does.
    fn inspect_asset(&mut self, item: &ProjectItem) {
        self.select(None);
        self.inspector.show_asset(&mut self.ui, &self.game, item);
    }

    /// Opens a file: a level or scene to edit, a prefab on its own, a script
    /// in the code editor, and anything else in the Inspector.
    fn open_item(&mut self, item: &ProjectItem, renderer: &mut Renderer) {
        match item.kind {
            ItemKind::Asset(AssetKind::Level) => self.level_action(LevelAction::Open(item.path.clone()), renderer),
            ItemKind::Asset(AssetKind::Scene) => self.level_action(LevelAction::OpenScene(item.path.clone()), renderer),
            ItemKind::Asset(AssetKind::Prefab) => self.open_prefab(&item.path.clone(), renderer),
            ItemKind::Script => {
                let file = self.game.project.root.join(&item.path);
                self.open_source(&file, 1);
            }
            _ => self.inspect_asset(item),
        }
    }

    /// What the Inspector's asset view asked for.
    fn asset_action(&mut self, action: AssetAction, renderer: &mut Renderer) {
        match action {
            AssetAction::Open(item) => self.open_item(&item, renderer),
            AssetAction::SaveMaterial(path, data) => {
                if let Err(error) = self.game.content.save_material(renderer, &path, &data) {
                    self.notify(&format!("Could not save {path}: {error:#}"));
                }
                if let Some(view) = self.inspector.asset_view() {
                    view.show_swatches(&mut self.ui);
                }
                self.asset_changed(renderer, &path);
            }
        }
    }

    /// What a click in the Levels panel asked for. Playing, a level is
    /// changed to as the game would change to it; stopped, it is opened to
    /// edit, and Play opens it and plays.
    /// Opens and narrows the command palette, and runs what it picks.
    fn update_palette(&mut self, renderer: &mut Renderer) {
        let (docs, game, memory) = (&self.docs, &self.game, &self.memory);
        let added = self.added_commands();
        let playing = self.is_playing();
        let sources = || Sources {
            game,
            playing,
            settings: &memory.settings,
            recent: &memory.recent,
            // The project's own pages: the Code Reference's would crowd out everything else.
            docs: docs
                .library()
                .pages()
                .filter(|(_, page)| !page.generated)
                .map(|(_, page)| (page.title().to_string(), page.reference().to_string()))
                .collect(),
            added,
        };
        let clicked = self.palette.update(&mut self.ui, sources);
        for picked in std::mem::take(&mut self.palette_runs).into_iter().chain(clicked) {
            self.memory.ran(&picked.key);
            self.run_palette(picked.action, renderer);
        }
    }

    /// Commands added by code and the scripts' `[Command]` methods, as the
    /// palette lists them.
    fn added_commands(&self) -> Vec<Added> {
        let added = self.added.iter().map(|command| Added {
            key: format!("added:{}", command.id),
            title: command.title.clone(),
            keys: command.keys.clone(),
            icon: command.icon,
            action: PaletteAction::Added(command.id.clone()),
        });
        let scripts = self.inspector.types().commands.iter().map(|command| Added {
            key: format!("script:{}.{}", command.type_name, command.method),
            title: command.title.clone(),
            keys: command.keys.clone(),
            icon: "code",
            action: PaletteAction::Script { type_name: command.type_name.clone(), method: command.method.clone() },
        });
        added.chain(scripts).collect()
    }

    /// Adds a command to the palette, as code built on the editor does.
    /// Its keys, if any, run it from anywhere in the editor.
    pub fn add_command(&mut self, command: PaletteCommand) {
        self.added.retain(|c| c.id != command.id);
        self.added.push(command);
    }

    /// The editor's settings, as the Settings panel shows them.
    pub fn settings(&self) -> &Settings {
        &self.memory.settings
    }

    /// Runs a setting as the palette does: a toggle flips, a choice moves on,
    /// and a number opens the Settings panel at it, to type.
    fn run_setting(&mut self, key: &'static str) {
        let Some(info) = settings::info(key) else { return };
        let next = match (self.memory.settings.get(key), info.kind) {
            (Some(SettingValue::Toggle(on)), _) => Some(SettingValue::Toggle(!on)),
            (Some(SettingValue::Choice(i)), SettingKind::Choice(options)) => {
                Some(SettingValue::Choice((i + 1) % options.len()))
            }
            _ => None,
        };
        match next {
            Some(value) => {
                self.memory.settings.set(key, value);
                self.memory.save();
                self.apply_settings();
                let now = self.memory.settings.describe(info);
                self.notify(&format!("{}: {now}", info.title));
            }
            None => {
                self.show_panel("settings");
                self.settings_panel.reveal(key);
            }
        }
    }

    /// Puts the settings into effect: the tool strip and its handles, the fly
    /// camera's speed, and the overlays over the Scene view. Changes from the
    /// tool strip, the wheel and the Settings panel are kept.
    fn update_settings(&mut self) {
        let mut changed = self.settings_panel.update(&mut self.ui, &mut self.memory.settings);
        changed |= self.update_tool_strip();
        changed |= self.update_scene_view();
        // The wheel changes the speed while flying; the setting follows it once flying stops.
        if self.fly.speed() != self.fly_speed && !self.fly.is_looking() {
            changed |= self.memory.settings.set("fly_speed", SettingValue::Number(self.fly.speed()));
        } else if self.memory.settings.fly_speed != self.fly_speed && !self.fly.is_looking() {
            self.fly.set_speed(self.memory.settings.fly_speed);
        }
        self.fly_speed = self.fly.speed();
        if changed {
            self.memory.save();
        }
        self.apply_settings();
    }

    /// Puts the settings into the tool strip, its handles and the overlays.
    fn apply_settings(&mut self) {
        let settings = &self.memory.settings;
        (self.tools.snap, self.tools.centred, self.tools.local) = (settings.snap, settings.centred, settings.local);
        let (show_stats, show_gizmo) = (settings.show_stats, settings.show_axis_gizmo);
        self.ui.set_text(self.nodes.pivot, if self.tools.centred { "Center" } else { "Pivot" });
        self.ui.set_text(self.nodes.space, if self.tools.local { "Local" } else { "Global" });
        if let Some(&label) = self.ui.children(self.nodes.snap).last() {
            self.ui.set_text(label, &format!("{} m", self.tools.snap));
        }
        self.ui.set_visible(self.nodes.stats, show_stats);
        self.ui.set_visible(self.nodes.view_gizmo, show_gizmo);
        self.ui.set_checked(self.nodes.gizmos, settings.show_gizmos);
        if self.ui.selected_option(self.nodes.shading) != settings.scene_shading {
            self.ui.set_selected_option(self.nodes.shading, settings.scene_shading);
        }
    }

    /// Does what a result of the command palette says.
    fn run_palette(&mut self, action: PaletteAction, renderer: &mut Renderer) {
        match action {
            PaletteAction::Setting(key) => self.run_setting(key),
            PaletteAction::Added(id) => {
                let run = self.added.iter().find(|c| c.id == id).map(|c| c.run.clone());
                if let Some(run) = run {
                    run(self);
                }
            }
            PaletteAction::Script { type_name, method } => self.run_script_command(&type_name, &method),
            PaletteAction::Command(command) => self.run(command, renderer),
            PaletteAction::Play if self.is_playing() => self.stop(renderer),
            PaletteAction::Play => self.play(renderer),
            PaletteAction::Pause => {
                let paused = !self.is_paused();
                self.set_paused(paused);
            }
            PaletteAction::Step => self.step(),
            PaletteAction::Tool(index) => self.ui.set_segment(self.nodes.tool, index),
            PaletteAction::Create(kind) => {
                self.obj_action(ObjAction::Create { kind, parent: None, scene: None }, renderer)
            }
            PaletteAction::ShowPanel(id) => self.show_panel(id),
            PaletteAction::Select(entity) => {
                self.show_panel("hierarchy");
                self.select(Some(entity));
            }
            PaletteAction::OpenLevel(path) => self.level_action(LevelAction::Open(path), renderer),
            PaletteAction::Reveal(path) => {
                self.show_panel("project");
                self.project.reveal(&mut self.ui, &path);
            }
            PaletteAction::OpenDoc(reference) => {
                self.show_panel("docs");
                self.docs.open_reference(&mut self.ui, &reference);
            }
        }
    }

    fn level_action(&mut self, action: LevelAction, renderer: &mut Renderer) {
        let playing = self.is_playing();
        let target = match &action {
            LevelAction::Open(path) | LevelAction::OpenScene(path) | LevelAction::Play(path) => path.clone(),
        };
        if !playing && !self.may_discard(&target) {
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
            LevelAction::OpenScene(scene) => self.open_scene(&scene, renderer),
            LevelAction::Play(level) => {
                if playing {
                    self.stop(renderer);
                }
                self.open_level(&level, renderer);
                self.play(renderer);
            }
        }
    }

    /// Whether something may open in place of what is open now, losing its
    /// edits not saved: the first time, the status bar warns and it is held
    /// back; asked again for the same `target`, it goes ahead.
    fn may_discard(&mut self, target: &str) -> bool {
        if self.unsaved && self.discard_armed.as_deref() != Some(target) {
            self.notify("Unsaved changes: Ctrl+S keeps them, or click again to open without them");
            self.discard_armed = Some(target.to_string());
            return false;
        }
        true
    }

    /// Opens a scene to edit on its own, in place of the level.
    fn open_scene(&mut self, scene: &str, renderer: &mut Renderer) {
        let game = &mut self.game;
        match game.levels.open_scene(&mut game.engine, &mut game.content, Some(&mut *renderer), scene) {
            Ok(_) => self.opened(renderer),
            Err(error) => tracing::error!("could not open {scene}: {error:#}"),
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
        self.leave_prefab_mode();
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
        // Gizmos go under the handles, which are drawn last.
        let world = &self.game.engine.world;
        let mut shapes =
            if self.memory.settings.show_gizmos { gizmos::shapes(world, &camera, self.selected()) } else { Vec::new() };
        shapes.extend(self.tools.shapes(world, renderer, &camera, self.selected()));
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
        if let ObjAction::OpenPrefab(entity) = action {
            return self.open_prefab_of(entity, renderer);
        }
        let position = self.new_obj_position();
        let game = &mut self.game;
        let (edit, selected) = match action {
            ObjAction::Create { kind, parent, scene } => {
                match objs::create(game, Some(renderer), kind, parent, scene, position) {
                    Ok(made) => (Edit::Exists { root: made, taken: None }, Some(made)),
                    Err(error) => return tracing::error!("could not make the Obj: {error:#}"),
                }
            }
            ObjAction::Duplicate(entity) => match objs::duplicate(game, Some(renderer), entity) {
                Ok(copy) => (Edit::Exists { root: copy, taken: None }, Some(copy)),
                Err(error) => return tracing::error!("could not duplicate the Obj: {error:#}"),
            },
            ObjAction::Delete(entity) => {
                let taken = objs::take(game, entity);
                (Edit::Exists { root: entity, taken: Some(taken) }, None)
            }
            ObjAction::Move(entity, to) => match objs::move_obj(game, entity, to) {
                Some((before, after)) => (Edit::Moved { entity, before, after }, Some(entity)),
                None => return,
            },
            ObjAction::OpenPrefab(_) => return,
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
        if let RowAction::EditColor(prop, swatch) = action {
            return self.edit_color(entity, prop, swatch);
        }
        if let RowAction::Browse(prop, anchor) = action {
            return self.browse(entity, prop, anchor);
        }
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
                if let Err(error) = props::add(&mut self.game, Some(renderer), entity, &part, self.inspector.types()) {
                    tracing::error!("could not add {}: {error:#}", part.title());
                }
            }
            RowAction::Remove(part) => props::remove(&mut self.game.engine.world, entity, &part),
            RowAction::Browse(..) | RowAction::EditColor(..) => {}
            RowAction::Pick(prop, pick) => {
                props::pick(&mut self.game, Some(renderer), self.inspector.types(), entity, &prop, &pick);
            }
            RowAction::Dropped(prop, source) => self.drop_reference(entity, &prop, &source, renderer),
            RowAction::Move(Part::Behaviour(name), by) => {
                props::move_behaviour(&mut self.game.engine.world, entity, &name, by);
            }
            RowAction::Move(..) => {}
            RowAction::Lock(part, locked) => props::set_locked(&mut self.game.engine.world, entity, &part, locked),
            RowAction::ShowToDesigners(prop, shown) => {
                props::show_to_designers(&mut self.game.engine.world, entity, &prop, shown);
            }
        }
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        if before != after && !self.is_playing() {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Points a reference at an Obj dragged from the Hierarchy or an asset
    /// from the Project, if it is one the field's menu offers; otherwise the
    /// status bar says why not.
    fn drop_reference(&mut self, entity: Entity, prop: &Prop, source: &DragSource, renderer: &mut Renderer) {
        let pick = match (self.hierarchy.dragged(source), self.project.dragged(source)) {
            (Some(obj), _) => Pick::Obj(Some(obj)),
            (None, Some(path)) => Pick::Asset(Some(path)),
            (None, None) => return,
        };
        let types = self.inspector.types();
        if props::choices(&self.game, types, entity, prop).iter().any(|(_, offered)| *offered == pick) {
            props::pick(&mut self.game, Some(renderer), types, entity, prop, &pick);
        } else {
            let why = props::why_not(&self.game, types, entity, prop, &pick);
            self.notify(&why);
        }
    }

    /// The Console's frame: what it changed of the settings, Error Pause,
    /// a frame to open, and the Obj a line is about.
    fn update_console(&mut self) {
        let result = self.console.update(&mut self.ui, &mut self.memory.settings);
        if result.settings_changed {
            self.memory.save();
            self.apply_settings();
        }
        if result.errors > 0 && self.memory.settings.error_pause && self.is_playing() && !self.is_paused() {
            self.set_paused(true);
            self.notify("Paused on an error, as Error Pause asks");
        }
        if let Some((file, line)) = result.open {
            self.open_source(&file, line);
        }
        if let Some(entity) = result.ping.filter(|&e| self.game.engine.world.is_alive(e)) {
            self.select(Some(entity));
        }
    }

    /// Opens a file at a line in the code editor the settings name, as a
    /// stack frame clicked in the Console asks.
    pub fn open_source(&mut self, file: &Path, line: u32) {
        if let Some(opener) = &mut self.source_opener {
            return opener(file, line);
        }
        let path = file.display().to_string();
        let (program, args): (&str, Vec<String>) = match self.memory.settings.code_editor {
            0 => ("code", vec!["--goto".into(), format!("{path}:{line}")]),
            1 => ("rider", vec!["--line".into(), line.to_string(), path]),
            2 => ("zed", vec![format!("{path}:{line}")]),
            _ if cfg!(target_os = "macos") => ("open", vec![path]),
            _ if cfg!(windows) => ("explorer", vec![path]),
            _ => ("xdg-open", vec![path]),
        };
        match std::process::Command::new(program).args(&args).spawn() {
            Ok(mut child) => {
                // Waited on elsewhere, so it does not linger once it exits.
                std::thread::spawn(move || child.wait());
            }
            Err(error) => {
                let editor = settings::CODE_EDITORS[self.memory.settings.code_editor.min(3)];
                self.notify(&format!(
                    "Could not open it with {editor} ({error}); pick another in Settings, Code editor"
                ));
            }
        }
    }

    /// Opens stack frames' files some other way than the code editor the
    /// settings name, as tests and tools built on the editor do.
    pub fn set_source_opener(&mut self, opener: SourceOpener) {
        self.source_opener = Some(opener);
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
                    self.reload_scripts();
                    if let Some(code) = code_reference(&self.game.project) {
                        self.docs.set_code(&mut self.ui, code);
                    }
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

    /// Shows script fields as `types` describes them, as a new build's
    /// description does: for code that describes scripts its own way, and
    /// for tests, which need no .NET this way.
    pub fn set_script_types(&mut self, types: ScriptTypes) {
        self.inspector.set_types(types);
    }

    /// Whether the scripts are described as they are now, with no build of
    /// them under way.
    pub fn scripts_described(&self) -> bool {
        self.describing.is_none() && !script_types::is_stale(&self.game.project)
    }

    /// Pivot or Center, Global or Local, and the snap distance, as the
    /// tool strip's buttons toggle them: they are settings, so they are kept.
    fn update_tool_strip(&mut self) -> bool {
        let settings = &mut self.memory.settings;
        let mut changed = false;
        if self.ui.clicked(self.nodes.pivot) {
            changed |= settings.set("handle_position", SettingValue::Choice(usize::from(!settings.centred)));
        }
        if self.ui.clicked(self.nodes.space) {
            changed |= settings.set("handle_space", SettingValue::Choice(usize::from(!settings.local)));
        }
        if self.ui.clicked(self.nodes.snap)
            && let Some(SettingValue::Choice(at)) = settings.get("snap")
        {
            changed |= settings.set("snap", SettingValue::Choice((at + 1) % SNAPS.len()));
        }
        changed
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
                self.prefab_saved();
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
        if self.game_ui_takes(event) {
            return true;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => self.pointer = Some((position.x as f32, position.y as f32)),
            WindowEvent::Focused(focused) => self.build_window.set_window_focused(*focused),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.control = modifiers.state().control_key() || modifiers.state().super_key();
                self.shift = modifiers.state().shift_key();
                self.alt = modifiers.state().alt_key();
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
            && self.color_picker_open()
            && key.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
        {
            self.close_color();
            return true;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && (self.palette.is_open() || self.picker.is_open())
            && let winit::keyboard::Key::Named(named) = &key.logical_key
        {
            use winit::keyboard::NamedKey;
            let palette_key = match named {
                NamedKey::ArrowUp => Some(PaletteKey::Up),
                NamedKey::ArrowDown => Some(PaletteKey::Down),
                NamedKey::Enter => Some(PaletteKey::Enter),
                NamedKey::Escape => Some(PaletteKey::Escape),
                _ => None,
            };
            if let Some(palette_key) = palette_key {
                if self.picker.is_open() {
                    self.picker_key(palette_key);
                } else {
                    self.palette_key(palette_key);
                }
                return true;
            }
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && !key.repeat
            && !self.ui.wants_keyboard()
            && let Some(picked) = self.command_for_keys(&key.logical_key)
        {
            self.palette_runs.push(picked);
            return true;
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
            // In a text field, undo, redo and duplicate are the field's; Ctrl+S
            // saves the VoxDocs page being written, or else the scene.
            let typing = self.ui.wants_keyboard();
            let command = match (text.to_lowercase().as_str(), self.shift) {
                ("s", _) if self.docs.has_keyboard(&self.ui) => {
                    self.docs.save(&mut self.ui);
                    return true;
                }
                ("z" | "y" | "d", _) if typing => None,
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

    /// Opens the object picker for a reference of `entity`, under its field.
    fn browse(&mut self, entity: Entity, prop: Prop, anchor: NodeId) {
        let types = self.inspector.types();
        let choices = object_picker::decorate(&self.game, props::choices(&self.game, types, entity, &prop));
        let title = props::title(types, &prop);
        let now = match props::get(&self.game, types, entity, &prop) {
            Some(props::Value::Asset(now) | props::Value::Obj(now)) => now,
            _ => String::new(),
        };
        self.picker.open(&mut self.ui, (entity, prop, anchor), &title, choices, &now);
    }

    /// The object picker's frame, and what it or its keys picked, for the
    /// Obj it was opened for if that is still the one selected.
    fn update_picker(&mut self, renderer: &mut Renderer) {
        let clicked = self.picker.update(&mut self.ui);
        for (entity, prop, pick) in std::mem::take(&mut self.picker_picks).into_iter().chain(clicked) {
            if self.selected() == Some(entity) {
                self.row_action(RowAction::Pick(prop, pick), renderer);
            }
        }
    }

    /// A key for the open object picker: Up and Down move through it, Enter
    /// picks what is highlighted on the next frame, and Escape closes it.
    pub fn picker_key(&mut self, key: PaletteKey) {
        if let Some(picked) = self.picker.key(&mut self.ui, key) {
            self.picker_picks.push(picked);
        }
    }

    /// Whether the object picker is open.
    pub fn picker_open(&self) -> bool {
        self.picker.is_open()
    }

    /// A key for the open command palette: Up and Down move through it, Enter
    /// runs what is highlighted on the next frame, and Escape closes it.
    pub fn palette_key(&mut self, key: PaletteKey) {
        if let Some(picked) = self.palette.key(&mut self.ui, key) {
            self.palette_runs.push(picked);
        }
    }

    /// The command added by code, or the scripts' `[Command]`, whose keys
    /// these are, to run as the palette would.
    fn command_for_keys(&self, key: &winit::keyboard::Key) -> Option<Picked> {
        use winit::keyboard::Key;
        let pressed = match key {
            Key::Character(text) => text.to_lowercase(),
            Key::Named(named) => format!("{named:?}").to_lowercase(),
            _ => return None,
        };
        let matches = |keys: &str| {
            Keys::parse(keys).is_some_and(|k| {
                k.key == pressed && k.control == self.control && k.shift == self.shift && k.alt == self.alt
            })
        };
        self.added_commands().into_iter().find(|c| matches(&c.keys)).map(|c| Picked { action: c.action, key: c.key })
    }

    /// A viewport's rectangle in physical pixels, within `size`; a pixel
    /// in the corner when no view shows.
    /// Hands the game's UI what happens over the Game view while it plays,
    /// in the view's own pixels: the pointer, and keys while one of its text
    /// fields has them. Returns whether it took the event.
    fn game_ui_takes(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        if !self.is_playing() || !self.shown(self.nodes.game_viewport) || self.game.ui.is_empty() {
            return false;
        }
        let [x, y, w, h] = self.ui.rect(self.nodes.game_viewport).map(|v| v * self.scale);
        let ui = self.game.ui.ui();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x as f32, position.y as f32);
                match px >= x && px < x + w && py >= y && py < y + h {
                    true => ui.pointer_move(px - x, py - y),
                    false => ui.pointer_leave(),
                }
                false
            }
            WindowEvent::MouseInput { button: winit::event::MouseButton::Left, state, .. } => {
                ui.pointer_button(*state == winit::event::ElementState::Pressed)
            }
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) if ui.wants_keyboard() => {
                voxear_engine::ui_input::handle(ui, event)
            }
            _ => false,
        }
    }

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
        let busy = self.play.is_building() || self.describing.is_some();
        self.ui.set_class(self.nodes.status_dot, "busy", busy);
        self.status.0 += 1;
        self.status.1 += delta;
        if self.status.1 < STATUS_INTERVAL && self.status.0 > 1 {
            return;
        }
        let second = self.status.0 as f32 / self.status.1.max(1e-3);
        self.status = (0, 0.0);
        let fps =
            if self.average_fps > 0.0 { self.average_fps + (second - self.average_fps) * FPS_FOLLOW } else { second };
        self.average_fps = fps;
        if (fps - self.shown_fps).abs() > self.shown_fps * FPS_STEADY {
            self.shown_fps = fps;
            self.ui.set_text(self.nodes.status_fps, &format!("{fps:.0} fps"));
        }
        let memory = profiler::resident_megabytes().map(|mb| format!("{mb:.0} MB")).unwrap_or_default();
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

/// The namespace the project's scripts declare, from their project's
/// `RootNamespace`, or else made from the project's name.
fn script_namespace(project: &Project) -> String {
    let declared = project.scripts_project().and_then(|csproj| std::fs::read_to_string(csproj).ok()).and_then(|text| {
        let start = text.find("<RootNamespace>")? + "<RootNamespace>".len();
        let end = start + text[start..].find("</RootNamespace>")?;
        Some(text[start..end].trim().to_string())
    });
    declared.unwrap_or_else(|| voxear_engine::new_project::namespace(project.name()))
}

/// The Code Reference for VoxDocs, as the scripts were last described, if
/// they have been.
fn code_reference(project: &Project) -> Option<voxear_docs::CodeReference> {
    let path = script_types::code_path(project).filter(|path| path.is_file())?;
    voxear_docs::CodeReference::read(&path).inspect_err(|error| tracing::warn!("VoxDocs: {error:#}")).ok()
}
