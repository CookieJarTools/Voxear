//! The editor itself: the open project in edit mode, its panels, the Scene
//! view, which looks through the editor's own camera, and the Game view,
//! which looks through the level's. Play mode is in [`play`].

mod color_popup;
mod component_clipboard;
mod blocks;
mod data_assets;
mod debug_texts;
mod desktop_windows;
mod docs_capture;
mod docs_links;
mod docs_pictures;
mod docs_steps;
mod drawer;
mod framing;
mod gun_handlings;
mod world_gens;
mod river_water;
mod hierarchies;
mod importing;
mod input_files;
mod inspectors;
mod languages;
mod lighting;
mod level_edits;
mod material_drops;
mod material_shaders;
mod model_clips;
mod motion_sets;
mod prompt_icon_files;
mod model_materials;
mod model_parts;
mod modules;
mod multiplayer;
mod path_bakes;
mod placing;
mod play;
mod prefab_mode;
mod prefab_overrides;
mod previews;
mod project_layers;
mod projects;
mod scene_gizmos;
mod scene_views;
mod scenes;
mod script_actions;
mod selection;
mod script_windows;
mod side_bar;
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
    AssetPreviews, Engine, Game, Project,
    debug_texts::DebugTexts,
    script_types::{self, ScriptTypes},
};
use voxear_render::{Camera, MeshRenderer, OutputId, Renderer, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_ui::{DragSource, NodeId, Theme, Ui};

use crate::{
    asset_inspector::AssetAction,
    audio_panel::AudioPanel,
    build_window::BuildWindow,
    console::Console,
    entities_panel::EntitiesPanel,
    field_handles::FieldHandles,
    path_handles::PathHandles,
    fields::{self, Pick},
    frame_debugger_panel::FrameDebuggerPanel,
    gizmos,
    hierarchy::EditorOnly,
    hierarchy::Hierarchy,
    hierarchy::ObjAction,
    history::{Edit, History, ObjState},
    inspector::{Field, Inspector, RowAction},
    levels_panel::{LevelAction, LevelsPanel},
    input_panel::InputPanel,
    localization_panel::{LocalizationPanel, TablesAction},
    memory_panel::MemoryPanel,
    object_picker::{self, Chosen, ObjectPicker, PickFor},
    objs,
    package_manager::{PackageManager, PackagesEvent},
    palette::{Added, Keys, Palette, PaletteAction, PaletteCommand, PaletteKey, Picked, Sources},
    profiler::{self, Profiler},
    project_view::{ItemKind, ProjectEvent, ProjectItem, ProjectView},
    props::{self, Part, Prop},
    scene_tools::{Handle, SceneTools, Tool, ViewCamera, pick},
    script_handles::ScriptHandles,
    settings::{self, Memory, SNAPS, SettingKind, SettingValue, Settings},
    lighting_panel::LightingPanel,
    project_settings::ProjectSettingsPanel,
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
    game_viewport: NodeId,
    game_empty: NodeId,
    /// The first Inspector's padlock.
    inspector_lock: NodeId,
    command: NodeId,
    status_left: NodeId,
    status_time: NodeId,
    status_fps: NodeId,
    status_memory: NodeId,
    /// Play, Pause and Step.
    play_buttons: [NodeId; 3],
    /// How Play plays, offline or networked, and its label.
    multiplayer: NodeId,
    multiplayer_label: NodeId,
    build: NodeId,
    /// The top bar's Window menu button.
    windows: NodeId,
    /// The rail's buttons, and the drawer they open with its title, close
    /// button and pages.
    rail_levels: NodeId,
    rail_lighting: NodeId,
    rail_project: NodeId,
    rail_settings: NodeId,
    /// Where the game's own side bar buttons and items go.
    rail_scripts: NodeId,
    drawer: NodeId,
    drawer_title: NodeId,
    drawer_close: NodeId,
    drawer_levels: NodeId,
    drawer_lighting: NodeId,
    drawer_project: NodeId,
    drawer_settings: NodeId,
    /// The top bar's Layout menu button and the name on it, the box that
    /// asks for a new layout's name with its field and Save button, and the
    /// Profiler's panel.
    layouts: NodeId,
    layout_name: NodeId,
    layout_prompt: NodeId,
    layout_prompt_name: NodeId,
    layout_prompt_save: NodeId,
    profiler_panel: NodeId,
    /// The Entities panel's content, the ECS inspector.
    entities_panel: NodeId,
    frame_debugger_panel: NodeId,
    memory_panel: NodeId,
    audio_panel: NodeId,
    localization_panel: NodeId,
    /// The Input Actions window's panel.
    input_panel: NodeId,
    /// The Build window's panel.
    build_panel: NodeId,
    /// The Package Manager's panel.
    packages_panel: NodeId,
    /// A script field's tooltip, shown under its label.
    tooltip: NodeId,
    /// Over the window, where debug draw's texts show.
    debug_texts: NodeId,
    /// The status bar's dot, which pulses while the scripts build.
    status_dot: NodeId,
}

/// What a shortcut asks of the editor, done in the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Save,
    Undo,
    Redo,
    Duplicate,
    Delete,
    /// Show the selection in the Scene view, as Unity's F does.
    Frame,
    /// Move the selection to where the Scene view looks from.
    AlignWithView,
    /// Show the project's layers in the Inspector.
    Layers,
    /// Show the game's text pseudo-localised, or as written.
    PseudoText,
    /// Bake the open level's sound paths.
    BakePaths,
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
    nodes: Nodes,
    /// The Scene view in use, which the tools act in, and the others; see
    /// [`scene_views`].
    scene: scene_views::SceneView,
    scene_views: Vec<scene_views::SceneView>,
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
    project_settings: ProjectSettingsPanel,
    /// The Lighting page, in the drawer.
    lighting_panel: LightingPanel,
    /// The theme the editor wears, which the default theme's own colours are
    /// until the settings ask for another.
    theme: Theme,
    /// The fly camera's speed as last seen, to tell the wheel's changes from
    /// the setting's.
    fly_speed: f32,
    /// Every level and loose scene, to open, play or change to.
    levels: LevelsPanel,
    /// Play mode, and the scripts it keeps between runs.
    play: play::Play,
    /// The camera the renderer was last told to look through, and the debug
    /// view it was told to show.
    view_camera: Option<Entity>,
    view_debug: u32,
    /// The dock areas showing 3D views, made see-through, each with a ring
    /// around its edge that rounds the view's corners to the panel's.
    viewport_areas: Vec<NodeId>,
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
    /// The handles that edit the selected Obj's fields in the Scene view,
    /// and the Obj as it was when a drag of one began, for undo.
    field_handles: FieldHandles,
    path_handles: PathHandles,
    /// The selected Terrain's brush and layer handles (see `terrain_tools`).
    terrain_tools: crate::terrain_tools::TerrainTools,
    block_tools: crate::block_tools::BlockTools,
    /// The selected Foliage's brush (see `plants_tools`).
    plants_tools: crate::plants_tools::PlantsTools,
    field_drag_start: Option<ObjState>,
    /// The handles behaviours' gizmos show for tools of their own.
    script_handles: ScriptHandles,
    /// The Obj under the pointer in the Scene view in use, for gizmos.
    hovered_obj: Option<Entity>,
    /// Behaviours' gizmo lines, each with its Obj, for a click to select.
    gizmo_lines: scene_gizmos::GizmoLines,
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
    /// The open level's sound paths, while they bake.
    path_bake: Option<path_bakes::PathBake>,
    /// The panel whose window menu is open, and what each of its items does.
    window_menu: Option<(String, Vec<Option<windows::WindowAction>>)>,
    /// What the rail's drawer shows, if it is open.
    drawer: Option<Drawer>,
    /// The prefab open on its own to edit, if one is.
    prefab: Option<prefab_mode::OpenPrefab>,
    /// The layouts to pick from, the one showing, and whether Save Layout
    /// As's box has yet to take the focus.
    layouts: Vec<windows::Layout>,
    layout: usize,
    naming_layout: bool,
    /// When the layout was last looked at, to keep it in the memory.
    layout_checked: Instant,
    profiler: Profiler,
    entities: EntitiesPanel,
    frame_debugger: FrameDebuggerPanel,
    memory_profiler: MemoryPanel,
    audio: AudioPanel,
    localization: LocalizationPanel,
    input_actions: InputPanel,
    build_window: BuildWindow,
    packages: PackageManager,
    /// A reference field's search, and what its keys picked, done next frame.
    picker: ObjectPicker,
    /// The tooltip the pointer rests on, and for how long, in seconds.
    hovered_tooltip: Option<(String, f32)>,
    /// Debug draw's texts, over the view showing.
    debug_texts: DebugTexts,
    /// The colour picker, opened from a colour's swatch.
    color_popup: color_popup::ColorPopup,
    picker_picks: Vec<(PickFor, Chosen)>,
    /// The Obj whose prefab overrides the Inspector last marked, as it was.
    overrides_seen: Option<(Entity, ObjState)>,
    /// What is being placed live under the pointer over a Scene view.
    live_place: Option<placing::LivePlace>,
    /// The Scene view's camera on its way to frame the selection.
    glide: Option<framing::Glide>,
    /// The component Copy Component copied.
    copied: Option<component_clipboard::Copied>,
    /// The Game view has the focus while playing, so the game's input hears
    /// the keyboard and the mouse.
    game_focused: bool,
    /// The games networked Play started besides the editor's own.
    test_players: multiplayer::TestPlayers,
    /// Esc, or the focus going elsewhere, let go of a mouse the game holds,
    /// until a press on the Game view gives it back.
    cursor_freed: bool,
    /// A material shown on an Obj while it is dragged over it from the Project.
    material_preview: Option<material_drops::MaterialPreview>,
    /// The Obj selected, which every Inspector not locked shows: the active
    /// one when several are (see `selection`).
    selection: Option<Entity>,
    /// A part of the model the selected Obj draws whole, by its node, when
    /// that is what the Hierarchy has selected, for the Inspectors to show.
    selected_part: Option<(Entity, u32)>,
    /// The other Objs selected with it, a box being dragged over the Scene
    /// view, and where the others were as the handles began a drag.
    others: Vec<Entity>,
    marquee: Option<selection::Marquee>,
    followers: Option<selection::Followers>,
    /// The Inspectors opened from a window's menu, after the first.
    extra_inspectors: Vec<inspectors::ExtraInspector>,
    /// The Hierarchies and Project panels opened from a window's menu, past the first.
    extra_hierarchies: Vec<hierarchies::ExtraHierarchy>,
    extra_projects: Vec<projects::ExtraProject>,
    /// The game's own windows written in C#, opened from a window's menu.
    script_windows: script_windows::ScriptWindows,
    side_bar: side_bar::SideBar,
    /// The desktop windows open for panels floated out of the editor's
    /// window; see `desktop_windows`.
    desktop: Vec<desktop_windows::Opened>,
    /// Previews of assets, of a Mesh Renderer's own mesh and material, and
    /// the Inspector's large one; see `previews`.
    asset_previews: AssetPreviews,
    /// Files dropped on the window, and when the project's files last
    /// changed, to bring in what changed outside the editor; see `importing`.
    imports: importing::Imports,
    /// Prefabs, models and materials VoxDocs pages show live.
    live_assets: Vec<docs_pictures::LiveAsset>,
    /// The fields of the live materials on the VoxDocs page read, in the
    /// places the page leaves for them.
    live_fields: Vec<(NodeId, crate::asset_inspector::AssetView)>,
    /// The GIFs VoxDocs pages show, playing.
    animations: Vec<docs_pictures::Animation>,
    /// A picture or recording being taken for the VoxDocs page written.
    docs_capture: Option<docs_capture::Taking>,
    /// When a VoxDocs guide's checks written in C# last ran.
    code_checked: Option<Instant>,
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
    /// Where the pointer is over the Game view while the game plays, in its
    /// window's pixels, which works the UI Panels in the world.
    game_pointer: Option<[f32; 2]>,
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
    /// The scripts build and are described again once no build is under
    /// way, whether or not a script changed, as after the packages change.
    rebuild_scripts: bool,
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
            game_viewport: find(&ui, "game-viewport"),
            game_empty: find(&ui, "game-empty"),
            inspector_lock: find(&ui, "inspector-lock"),
            command: find(&ui, "command"),
            status_left: find(&ui, "status-left"),
            status_time: find(&ui, "status-time"),
            status_fps: find(&ui, "status-fps"),
            status_memory: find(&ui, "status-memory"),
            play_buttons: [find(&ui, "play"), find(&ui, "pause"), find(&ui, "step")],
            multiplayer: find(&ui, "multiplayer"),
            multiplayer_label: find(&ui, "multiplayer-label"),
            build: find(&ui, "build"),
            windows: find(&ui, "windows"),
            rail_levels: find(&ui, "rail-levels"),
            rail_lighting: find(&ui, "rail-lighting"),
            rail_project: find(&ui, "rail-project"),
            rail_settings: find(&ui, "rail-settings"),
            rail_scripts: find(&ui, "rail-scripts"),
            drawer: find(&ui, "drawer"),
            drawer_title: find(&ui, "drawer-title"),
            drawer_close: find(&ui, "drawer-close"),
            drawer_levels: find(&ui, "drawer-levels"),
            drawer_lighting: find(&ui, "drawer-lighting"),
            drawer_project: find(&ui, "drawer-project"),
            drawer_settings: find(&ui, "drawer-settings"),
            layouts: find(&ui, "layouts"),
            layout_name: find(&ui, "layout-name"),
            layout_prompt: find(&ui, "layout-prompt"),
            layout_prompt_name: find(&ui, "layout-prompt-name"),
            layout_prompt_save: find(&ui, "layout-prompt-save"),
            profiler_panel: find(&ui, "profiler-panel"),
            entities_panel: find(&ui, "entities-panel"),
            frame_debugger_panel: find(&ui, "frame-debugger-panel"),
            memory_panel: find(&ui, "memory-panel"),
            audio_panel: find(&ui, "audio-panel"),
            localization_panel: find(&ui, "localization-panel"),
            input_panel: find(&ui, "input-panel"),
            build_panel: find(&ui, "build-panel"),
            packages_panel: find(&ui, "packages-panel"),
            tooltip: find(&ui, "tooltip"),
            debug_texts: find(&ui, "debug-texts"),
            status_dot: find(&ui, "status-dot"),
        };
        let branch = git_branch(&root);
        let branch_name = find(&ui, "status-branch-name");
        ui.set_text(branch_name, branch.as_deref().unwrap_or_default());
        let branch_shown = find(&ui, "status-branch");
        ui.set_visible(branch_shown, branch.is_some());
        ui.set_dock_layout_closing_rest(nodes.dock, LAYOUT).expect("the default layout is valid");
        ui.set_visible(nodes.drawer, false);
        ui.set_visible(nodes.layout_prompt, false);
        ui.set_visible(nodes.tooltip, false);
        let hierarchy_panel = find(&ui, "hierarchy-panel");
        let hierarchy = hierarchies::build(&mut ui, hierarchy_panel);
        let namespace = script_namespace(&game.project);
        let project_panel = find(&ui, "project-panel");
        let project_markup = projects::build(&mut ui, project_panel);
        let mut project =
            ProjectView::new(&mut ui, project_markup, game.project.name(), &root, game.content.db(), &namespace);
        if let Some(size) = memory.tile_size {
            project.set_tile_size(&mut ui, size);
        }
        let console_panel = find(&ui, "console-panel");
        let console = Console::new(&mut ui, console_panel, log, &root, &memory.settings);
        let mut inspector = Inspector::new(find(&ui, "inspector"));
        inspector.set_types(script_types::cached(&game.project).unwrap_or_default());
        inspector.set_lock_button(&mut ui, nodes.inspector_lock);
        inspector.show(&mut ui, &game, None, false);
        let levels = LevelsPanel::new(&mut ui, document, &mut game.content);
        let docs_panel = find(&ui, "docs");
        let mut docs = voxear_docs::VoxDocs::new(&mut ui, docs_panel, &root, game.project.name());
        docs.set_manual(&mut ui, &voxear_engine::manual_folder());
        docs.set_packages(&mut ui, package_docs(game.content.db().mounts()));
        if let Some(code) = code_reference(&game.project) {
            docs.set_code(&mut ui, code);
        }
        let (panel, list, empty) = (find(&ui, "palette"), find(&ui, "palette-list"), find(&ui, "palette-empty"));
        let palette = Palette::new(&mut ui, nodes.command, panel, list, empty);
        let (tabs, view, filter) = (find(&ui, "settings-tabs"), find(&ui, "settings"), find(&ui, "settings-filter"));
        let settings_panel = SettingsPanel::new(&mut ui, tabs, view, filter, &memory.settings);
        let (tabs, view) = (find(&ui, "project-tabs"), find(&ui, "project-settings"));
        let (modules, origin) = (game.project.modules(), &game.project.file.floating_origin);
        let project_settings = ProjectSettingsPanel::new(&mut ui, tabs, view, &modules, origin);
        let (top, tabs, view) = (find(&ui, "lighting-scene"), find(&ui, "lighting-tabs"), find(&ui, "lighting"));
        let lighting_panel = LightingPanel::new(&mut ui, top, tabs, view);
        let profiler = Profiler::new(&mut ui, nodes.profiler_panel);
        let entities = EntitiesPanel::new(&mut ui, nodes.entities_panel);
        let frame_debugger = FrameDebuggerPanel::new(&mut ui, nodes.frame_debugger_panel);
        let memory_profiler = MemoryPanel::new(&mut ui, nodes.memory_panel);
        let audio = AudioPanel::new(&mut ui, nodes.audio_panel);
        let localization = LocalizationPanel::new(&mut ui, nodes.localization_panel);
        let input_actions = InputPanel::new(&mut ui, nodes.input_panel);
        let debug_texts = DebugTexts::new(&mut ui, nodes.debug_texts);
        let build_window = BuildWindow::new(&mut ui, nodes.build_panel);
        let packages = PackageManager::new(&mut ui, nodes.packages_panel);
        let parts = ["picker", "picker-back", "picker-title", "picker-field", "picker-list", "picker-empty"]
            .map(|name| find(&ui, name));
        let picker = ObjectPicker::new(&mut ui, parts[0], (parts[1], parts[2]), parts[3], parts[4], parts[5]);
        let (color_panel, color_title) = (find(&ui, "color-popup"), find(&ui, "color-title"));
        let color_popup = color_popup::ColorPopup::new(&mut ui, color_panel, color_title);
        let scene_panel = find(&ui, "scene-panel");
        let mut scene = scene_views::SceneView::build(&mut ui, scene_panel, scene_views::FIRST, camera);
        scene.fly.set_speed(memory.settings.fly_speed);
        (scene.shading, scene.show_gizmos) = (memory.settings.scene_shading, memory.settings.show_gizmos);
        ui.set_text(nodes.status_time, &format!("Opened in {:.2} s", started.elapsed().as_secs_f32()));

        tracing::info!("opened {} in edit mode", game.project.name());
        let mut editor = Self {
            game,
            ui,
            scene,
            scene_views: Vec::new(),
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
            project_settings,
            lighting_panel,
            theme: Theme::DARK,
            levels,
            play: play::Play::new(),
            view_camera: Some(camera),
            view_debug: 0,
            viewport_areas: Vec::new(),
            pointer: None,
            scale: 1.0,
            control: false,
            shift: false,
            commands: Vec::new(),
            kept: Vec::new(),
            tools: SceneTools::new(),
            field_handles: FieldHandles::default(),
            path_handles: PathHandles::default(),
            terrain_tools: Default::default(),
            block_tools: Default::default(),
            plants_tools: Default::default(),
            field_drag_start: None,
            script_handles: ScriptHandles::default(),
            hovered_obj: None,
            gizmo_lines: Default::default(),
            hovered_handle: None,
            pressed_handle: None,
            released_handle: false,
            history: History::default(),
            unsaved: false,
            discard_armed: None,
            notice: None,
            path_bake: None,
            window_menu: None,
            drawer: None,
            prefab: None,
            layouts: windows::built_in_layouts(),
            layout: 0,
            naming_layout: false,
            profiler,
            entities,
            frame_debugger,
            memory_profiler,
            audio,
            localization,
            input_actions,
            build_window,
            packages,
            picker,
            color_popup,
            hovered_tooltip: None,
            debug_texts,
            picker_picks: Vec::new(),
            overrides_seen: None,
            live_place: None,
            glide: None,
            copied: None,
            game_focused: false,
            test_players: multiplayer::TestPlayers::default(),
            cursor_freed: false,
            material_preview: None,
            selection: None,
            selected_part: None,
            others: Vec::new(),
            marquee: None,
            followers: None,
            extra_inspectors: Vec::new(),
            extra_hierarchies: Vec::new(),
            extra_projects: Vec::new(),
            script_windows: script_windows::ScriptWindows::default(),
            side_bar: side_bar::SideBar::default(),
            desktop: Vec::new(),
            asset_previews: AssetPreviews::default(),
            imports: importing::Imports::new(&root),
            live_assets: Vec::new(),
            live_fields: Vec::new(),
            animations: Vec::new(),
            docs_capture: None,
            code_checked: None,
            field_previews: previews::FieldPreviews::default(),
            inspector_preview: None,
            redraw_field_previews: false,
            maximized: None,
            status: (0, 0.0),
            frames: 0,
            game_pointer: None,
            source_opener: None,
            average_fps: 0.0,
            shown_fps: 0.0,
            describing: None,
            described_from: None,
            scripts_checked: None,
            rebuild_scripts: false,
            layout_checked: Instant::now(),
        };
        editor.restore_layout();
        Ok(editor)
    }

    /// Selects an Obj, as a click in the Hierarchy does, or nothing.
    pub fn select(&mut self, entity: Option<Entity>) {
        self.selected_part = None;
        self.reselect(entity);
    }

    /// Selects `entity` as [`select`](Self::select) does, but keeps a model's
    /// part picked while it is that part's Obj, as an undo does.
    fn reselect(&mut self, entity: Option<Entity>) {
        self.others.clear();
        // An Obj made this frame is in the Hierarchies once they have looked again.
        self.refresh_hierarchies();
        self.select_in_hierarchies(entity);
        self.show_selection(entity);
    }

    /// The Obj selected, as the Inspector shows it.
    pub fn selected(&self) -> Option<Entity> {
        self.selection
    }

    /// The model's part the first Inspector shows, by its Obj and node, when
    /// it shows one.
    pub fn inspected_part(&self) -> Option<(Entity, u32)> {
        self.inspector.shown_part()
    }

    /// The Scene view's camera over its rectangle, when the Scene view is
    /// showing: where points in the world fall on screen, and back.
    pub fn scene_camera(&self) -> Option<ViewCamera> {
        let rect = self.shown(self.scene.viewport).then(|| self.ui.rect(self.scene.viewport))?;
        ViewCamera::new(&self.game.engine.world, self.scene.camera, rect)
    }

    /// The first Obj with a mesh, for showing something selected.
    pub fn first_mesh(&self) -> Option<Entity> {
        let world = &self.game.engine.world;
        let mut meshes: Vec<Entity> =
            world.query_ref::<(Entity, &MeshRenderer)>().without::<EditorOnly>().into_iter().map(|(e, _)| e).collect();
        meshes.sort_by_key(|e| e.index());
        meshes.first().copied()
    }

    /// What Add Component offers the Obj the Inspector shows, by name.
    pub fn component_offers(&self) -> Vec<String> {
        let choices = self.inspector.component_choices(self.is_playing(), &self.game.project.modules());
        choices.into_iter().map(|choice| choice.label).collect()
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
            self.update_multiplayer();
            self.follow_pointer();
            if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.scene.camera) {
                self.scene.fly.update_transform(transform, delta);
            }
            // Terrains without end are made round where the Scene view looks from while editing; in play, round the
            // game's camera.
            let viewer = self.game.engine.world.get::<Transform>(self.scene.camera).map(|camera| camera.position);
            match viewer.filter(|_| !self.is_playing()) {
                Some(at) => self.game.engine.world.insert_resource(voxear_engine::terrain::TerrainViewer(at)),
                None => drop(self.game.engine.world.remove_resource::<voxear_engine::terrain::TerrainViewer>()),
            }
            if let Some(delta) = self.game_delta(delta) {
                self.game.tick(delta, Some(renderer));
            }
        }
        let views = {
            profile_scope!("Editor");
            self.update_panels(renderer, delta)
        };
        // Terrains changed by edits, undo or redo, kept as their assets' to save and to play.
        if !self.is_playing() {
            voxear_engine::terrain::keep(&self.game.engine.world, &mut self.game.content);
            voxear_engine::blocks::keep(&self.game.engine.world, &mut self.game.content);
        }
        let frame = {
            profile_scope!("UI");
            // The editor's own UI draws backgrounds with the same shaders as the game's.
            self.ui.set_shaders(self.game.ui.shaders());
            self.ui.frame(size.0, size.1, scale)
        };
        let rendered = {
            profile_scope!("Render");
            // Each view into the window it is in: the main one, or a desktop window a panel floated out in.
            let first = views.first().and_then(|&(viewport, _)| Some((viewport, self.output_of(viewport, size)?)));
            let (output, rect) = match first {
                Some((viewport, (output, output_size))) => (output, self.viewport_rect(Some(viewport), output_size)),
                None => (OutputId::MAIN, self.viewport_rect(None, size)),
            };
            renderer.set_view_output(output);
            renderer.set_view_rect(Some(rect));
            // Each view after the first, as the Game view beside the Scene view, through its own camera.
            let more: Vec<(OutputId, [u32; 4], ViewOptions)> = views
                .iter()
                .skip(1)
                .filter_map(|&(viewport, camera)| {
                    let (output, output_size) = self.output_of(viewport, size)?;
                    Some((output, self.viewport_rect(Some(viewport), output_size), self.view_options(viewport, camera)))
                })
                .collect();
            renderer.set_more_views_in(&more)?;
            {
                profile_scope!("UI Upload");
                renderer.set_ui(&frame)?;
                self.draw_desktop_windows(renderer)?;
                self.draw_panels(renderer, &views, size, delta)?;
                self.draw_game_ui(renderer, &views, size, delta)?;
            }
            // In edit mode the game does not tick, so the level's lookup table is loaded here.
            self.game.prepare_post(renderer);
            renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)
        };
        self.profiler.record(profiling::end_frame(), delta * 1000.0, renderer);
        rendered
    }

    /// The UI Panels in the world, in every view, editing or playing. While
    /// the game plays, the pointer over the Game view works them, through
    /// its camera.
    fn draw_panels(
        &mut self,
        renderer: &mut Renderer,
        views: &[(NodeId, Entity)],
        size: (u32, u32),
        delta: f32,
    ) -> Result<()> {
        let game = self.nodes.game_viewport;
        let camera = views.iter().find(|(viewport, _)| *viewport == game).map(|(_, camera)| *camera);
        let area = self.output_of(game, size).map(|(_, output_size)| self.viewport_rect(Some(game), output_size));
        let view = area.map_or([0.0, 0.0, 1.0, 1.0], |area| area.map(|side| side as f32));
        let pointer = self.game_pointer.filter(|_| self.is_playing() && camera.is_some());
        self.game.show_panels(renderer, (view, camera), pointer, delta)
    }

    /// The game's own UI, over the Game view while it plays, in whichever
    /// window the Game view is in, and none in the others.
    fn draw_game_ui(
        &mut self,
        renderer: &mut Renderer,
        views: &[(NodeId, Entity)],
        size: (u32, u32),
        delta: f32,
    ) -> Result<()> {
        let game = self.nodes.game_viewport;
        let in_game = views.iter().any(|(viewport, _)| *viewport == game) && self.is_playing();
        let shown = self.output_of(game, size).filter(|_| in_game);
        let empty = voxear_ui::UiFrame::default();
        for output in std::iter::once(OutputId::MAIN).chain(self.desktop_outputs()) {
            if shown.is_none_or(|(there, _)| there != output) {
                renderer.set_output_game_ui(output, &empty, [0, 0])?;
            }
        }
        let Some((output, output_size)) = shown else { return Ok(()) };
        let area = self.viewport_rect(Some(game), output_size);
        // Scripts' Screen.Width and Height are the Game view's.
        (self.game.display.width, self.game.display.height) = (area[2], area[3]);
        let frame = match self.game.ui.is_empty() {
            true => empty,
            false => self.game.ui.frame(delta, (area[2], area[3]), self.scale),
        };
        renderer.set_output_game_ui(output, &frame, [area[0], area[1]])
    }

    /// The panels' frame, each a profiling sample of its own. Returns the
    /// views the 3D renders into, each with its camera, the first first.
    fn update_panels(&mut self, renderer: &mut Renderer, delta: f32) -> Vec<(NodeId, Entity)> {
        let playing = self.is_playing();
        // Before the Inspector, so what is picked shows in this frame.
        self.update_picker(renderer);
        self.update_color_popup(renderer);
        {
            profile_scope!("Hierarchy");
            self.update_hierarchies(renderer, delta);
        }
        {
            profile_scope!("Inspector");
            self.update_inspectors(renderer, playing);
            self.update_data_assets();
        }
        self.update_placing(renderer);
        self.update_material_drops(renderer);
        for command in std::mem::take(&mut self.commands) {
            self.run(command, renderer);
        }
        self.update_windows();
        self.update_layouts();
        self.keep_layout();
        {
            profile_scope!("Script Windows");
            self.update_script_windows();
        }
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
            let memory_shown = self.shown(self.nodes.memory_panel);
            self.memory_profiler.update(&mut self.ui, memory_shown, &self.game, renderer);
            let audio_shown = self.shown(self.nodes.audio_panel);
            if let Some(picked) = self.audio.update(&mut self.ui, audio_shown, &self.game) {
                self.select(Some(picked));
            }
            let tables_shown = self.shown(self.nodes.localization_panel);
            if let Some(TablesAction::Rename(table, old, new)) =
                self.localization.update(&mut self.ui, tables_shown, &self.game)
            {
                self.rename_string_key(&table, &old, &new, renderer);
            }
        }
        self.update_input_actions(renderer);
        {
            profile_scope!("Package Manager");
            self.update_packages(renderer);
        }
        self.update_drawer();
        self.update_modules();
        self.update_lighting(renderer);
        self.update_side_bar();
        self.update_prefab_mode(renderer);
        self.update_overrides();
        self.update_glide(delta);
        self.update_settings();
        let theme = self.memory.settings.theme();
        if theme != self.theme {
            self.ui.set_theme(&theme);
            renderer.set_clear_color(theme.base.linear_premultiplied());
            self.docs.set_dark(&mut self.ui, theme.is_dark());
            self.packages.set_dark(theme.is_dark());
            for inspector in self.inspectors_mut() {
                inspector.set_dark(theme.is_dark());
            }
            self.theme = theme;
        }
        {
            profile_scope!("Scene Tools");
            self.update_scene_tools(renderer);
        }
        self.update_script_types();
        {
            profile_scope!("Project");
            self.update_projects(renderer, delta);
            self.update_imports(renderer);
            if let Some(action) = self.inspector.update_asset(&mut self.ui) {
                self.asset_action(action, renderer);
            }
            self.fill_model_materials(renderer);
            self.fill_model_clips();
            let listening = self.game.audio.previewing().map(str::to_string);
            if let Some(view) = self.inspector.asset_view() {
                view.show_listening(&mut self.ui, listening.as_deref());
                view.show_shader_state(&mut self.ui, renderer.custom_shaders());
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
            self.update_docs_capture(renderer);
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
            let (db, project) = (self.game.content.db(), &self.project);
            let finished = self.build_window.update(&mut self.ui, &root, db, shown, &|source| project.dragged(source));
            if let Some(finished) = finished {
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
        let views = self.active_views();
        let view = views.first().copied();
        self.ui.set_visible(self.nodes.game_empty, self.game_camera().is_none());
        self.mark_viewport_areas(&views);
        self.update_axis_gizmos();
        self.finish_path_bake();
        self.update_status(renderer, delta);
        self.update_tooltip(delta);
        self.update_debug_draw(view, delta);
        self.draw_scene_gizmos();

        if let Some((viewport, camera)) = view {
            // Set each frame, a copy: the Gizmos button changes it as much as the camera does.
            let options = self.view_options(viewport, camera);
            renderer.set_view_options(options);
            (self.view_camera, self.view_debug) = (options.camera, options.debug_view);
        }
        views
    }

    /// The Profiler panel, and the frames it has taken.
    pub fn profiler(&self) -> &Profiler {
        &self.profiler
    }

    /// The Entities panel, the ECS inspector.
    pub fn memory_panel(&self) -> &MemoryPanel {
        &self.memory_profiler
    }

    /// The Audio panel, the voices the game plays.
    pub fn audio_panel(&self) -> &AudioPanel {
        &self.audio
    }

    /// The Localization Tables window.
    pub fn localization_panel(&self) -> &LocalizationPanel {
        &self.localization
    }

    pub fn entities_panel(&self) -> &EntitiesPanel {
        &self.entities
    }

    /// The Build window, and the build it runs.
    pub fn build_window(&mut self) -> &mut BuildWindow {
        &mut self.build_window
    }

    /// The Package Manager window.
    pub fn package_manager(&mut self) -> &mut PackageManager {
        &mut self.packages
    }

    /// The VoxDocs panel, with the manual's, the project's and its packages'
    /// pages.
    pub fn docs(&self) -> &voxear_docs::VoxDocs {
        &self.docs
    }

    /// The Package Manager's frame. Once it changes the project's packages,
    /// their assets are mounted again, VoxDocs takes their pages, and their
    /// scripts build with the project's, as a script changing makes them;
    /// then the Project panel and the rest look at the files again, as after
    /// the Project panel changes them.
    fn update_packages(&mut self, renderer: &mut Renderer) {
        let shown = self.shown(self.nodes.packages_panel);
        let root = self.game.project.root.clone();
        for event in self.packages.update(&mut self.ui, shown, &root) {
            match event {
                PackagesEvent::Changed(done) => {
                    if let Err(error) = self.game.content.remount_packages() {
                        tracing::error!("the packages' assets could not be mounted again: {error:#}");
                    }
                    let pages = package_docs(self.game.content.db().mounts());
                    self.docs.set_packages(&mut self.ui, pages);
                    self.rebuild_scripts = true;
                    self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
                    self.notify(&done);
                }
                PackagesEvent::Imported(path) => {
                    self.project_event(ProjectEvent::Changed { select: Some(path.clone()), rename: false }, renderer);
                    self.notify(&format!("Imported {path}"));
                }
            }
        }
    }

    /// What the Project panel asked for: an item shown in the Inspector, a
    /// file opened, or the files looked at again after it changed them.
    fn project_event(&mut self, event: ProjectEvent, renderer: &mut Renderer) {
        match event {
            ProjectEvent::Selected(item) => self.inspect_asset(&item),
            ProjectEvent::Open(item) => self.open_item(&item, renderer),
            ProjectEvent::OpenAdditively(item) => self.open_scene_alongside(&item.path, renderer),
            ProjectEvent::Changed { select, rename } => {
                if let Err(error) = self.game.content.rescan() {
                    tracing::warn!("could not look at the project's files again: {error:#}");
                }
                // An .inputactions file or a string table may have come, gone or changed.
                self.game.content.forget_kind(voxear_assets::AssetKind::InputActions);
                self.game.load_input_actions();
                self.game.content.forget_kind(voxear_assets::AssetKind::Strings);
                self.game.load_strings();
                // A shader may have come or gone, which numbers the rest afresh.
                self.game.apply_custom_shaders(renderer);
                self.files_seen();
                // Files moved or went, so previews are made again for the paths they have now.
                self.asset_previews.clear(renderer);
                self.project.refresh(&mut self.ui, self.game.content.db(), select.as_deref());
                self.refresh_other_projects();
                self.levels.refresh(&mut self.ui, &mut self.game.content);
                if let Some(item) = select.and_then(|path| self.project.item(&path)) {
                    self.inspect_asset(&item);
                }
                if rename {
                    self.project.start_rename(&mut self.ui);
                }
            }
            ProjectEvent::Notice(text) => self.notify(&text),
            ProjectEvent::Dropped { source, folder } => {
                if let Some(entity) = self.dragged_obj(&source) {
                    self.make_prefab(entity, &folder, renderer);
                } else if let Some(path) = self.dragged_project_entry(&source) {
                    self.move_in_project(&path, &folder, renderer);
                }
            }
            ProjectEvent::Move { path, folder } => self.move_in_project(&path, &folder, renderer),
            ProjectEvent::Import { folder } => self.choose_imports(folder),
            ProjectEvent::NewShader { folder, anchor } => self.browse_shader_starts(folder, anchor),
            ProjectEvent::NewDataAsset { folder, anchor } => self.browse_definitions(folder, anchor),
            ProjectEvent::WantsParts(path) => {
                let parts = objs::model_parts(&mut self.game, renderer, &path);
                if parts.is_empty() {
                    self.notify(&format!("{path} could not be read for its parts"));
                }
                self.project.set_parts(&mut self.ui, &path, parts);
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
        let at = match crate::project_files::place(self.game.content.db().mounts(), folder) {
            Ok(at) => at,
            Err(error) => return self.notify(&format!("Could not make a prefab of {name}: {error:#}")),
        };
        let local = crate::project_files::free_path(&at.root, &at.path, stem.trim(), ".prefab");
        let path = at.back(&local);
        match objs::make_prefab(&mut self.game, Some(renderer), entity, &at.root.join(&local), &path) {
            Ok((instance, taken)) => {
                let made = Edit::Exists { root: instance, taken: None };
                let replaced = Edit::Exists { root: entity, taken: Some(taken) };
                self.history.record_step(Edit::Group(vec![made, replaced]));
                self.unsaved = true;
                self.select(Some(instance));
                self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
                self.refresh_other_projects();
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
    /// Shows the asset at `path` in the Inspector, as selecting it in the
    /// Project panel does. False for a path the project does not have.
    pub fn inspect_path(&mut self, path: &str) -> bool {
        let Some(item) = self.project.item(path) else { return false };
        self.inspect_asset(&item);
        true
    }

    fn inspect_asset(&mut self, item: &ProjectItem) {
        self.select(None);
        // A data asset shows the fields of the Definition it holds, to edit as an Obj's behaviour's.
        if item.kind == ItemKind::Asset(AssetKind::Data) && self.inspect_data(item) {
            return;
        }
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
                let file = self.game.content.db().file(&item.path);
                self.open_source(&file, 1);
            }
            ItemKind::Asset(AssetKind::InputActions) => self.show_panel("input"),
            _ => self.inspect_asset(item),
        }
    }

    /// What the Inspector's asset view asked for.
    fn asset_action(&mut self, action: AssetAction, renderer: &mut Renderer) {
        match action {
            AssetAction::Open(item) => self.open_item(&item, renderer),
            AssetAction::Lighting(action) => self.lighting_action(action, renderer),
            AssetAction::SceneLighting(scene, action) => self.scene_lighting_action(&scene, action, renderer),
            AssetAction::SaveMaterial(path, data) => {
                if let Err(error) = self.game.content.save_material(renderer, &path, &data) {
                    self.notify(&format!("Could not save {path}: {error:#}"));
                }
                if let Some(view) = self.inspector.asset_view() {
                    view.show_swatches(&mut self.ui);
                }
                self.asset_changed(renderer, &path);
            }
            AssetAction::EditLevel(path, edit) => self.edit_level(&path, &edit, renderer),
            AssetAction::SaveStrings { path, table, rebuild } => self.save_strings(&path, &table, rebuild),
            AssetAction::Notice(text) => self.notify(&text),
            AssetAction::Import(path, settings) => self.apply_import_settings(&path, &settings, renderer),
            AssetAction::SaveClips(path, settings) => self.apply_clip_settings(&path, &settings),
            AssetAction::SaveMotionSet { path, set, rebuild } => self.save_motion_set(&path, &set, rebuild),
            AssetAction::BrowseClip { set, slot, anchor } => self.browse_clip(&set, slot, anchor),
            AssetAction::DropClip { set, slot, source } => self.drop_clip(&set, slot, &source),
            AssetAction::SaveGunHandling { path, handling, rebuild } => {
                self.save_gun_handling(&path, &handling, rebuild);
            }
            AssetAction::SaveWorldGen { path, world, rebuild } => self.save_world_gen(&path, &world, rebuild),
            AssetAction::SaveBlock { path, block, rebuild } => self.save_block(&path, &block, rebuild),
            AssetAction::BrowseBlockRef { block, field, anchor } => self.browse_block_ref(&block, field, anchor),
            AssetAction::DropBlockRef { block, field, source } => self.drop_block_ref(&block, field, &source),
            AssetAction::MakeBlockItem(block) => self.make_block_item(&block),
            AssetAction::BrowsePromptIcon { icons, slot, anchor } => self.browse_prompt_icon(&icons, slot, anchor),
            AssetAction::DropPromptIcon { icons, slot, source } => self.drop_prompt_icon(&icons, &slot, &source),
            AssetAction::BrowseGunClip { handling, field, anchor } => self.browse_gun_clip(&handling, field, anchor),
            AssetAction::DropGunClip { handling, field, source } => self.drop_gun_clip(&handling, field, &source),
            AssetAction::BrowseScene { level, at, anchor } => self.browse_level_scene(&level, at, anchor),
            AssetAction::DropScene { level, at, source } => self.drop_level_scene(&level, at, &source, renderer),
            AssetAction::BrowseStandsOn { level, anchor } => self.browse_stands_on(&level, anchor),
            AssetAction::DropStandsOn { level, source } => self.drop_stands_on(&level, &source, renderer),
            AssetAction::Ping(path) => self.ping_asset(&path),
            AssetAction::Listen(path) => {
                if let Err(error) = self.game.audio.preview(&mut self.game.content, &path) {
                    self.notify(&format!("Could not play {path}: {error:#}"));
                }
            }
            AssetAction::StopListening => self.game.audio.stop_preview(),
            AssetAction::BrowseMap { material, map, anchor } => self.browse_material_map(&material, map, anchor),
            AssetAction::BrowseShader { material, anchor } => self.browse_material_shader(&material, anchor),
            AssetAction::DropShader { material, source } => self.drop_material_shader(&material, &source, renderer),
            AssetAction::BrowseShaderPicture { material, property, anchor } => {
                self.browse_shader_picture(&material, &property, anchor);
            }
            AssetAction::DropShaderPicture { material, property, source } => {
                self.drop_shader_picture(&material, &property, &source, renderer);
            }
            AssetAction::DropMap { material, map, source } => {
                self.drop_material_map(&material, map, &source, renderer);
            }
            AssetAction::BrowseRemap { model, part, anchor } => self.browse_remap(&model, &part, anchor),
            AssetAction::DropRemap { model, part, source } => self.drop_remap(&model, &part, &source, renderer),
            AssetAction::ExtractMaterials(model) => self.extract_materials(&model, renderer),
            AssetAction::BakePaths(level) => self.bake_level_paths(&level),
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
            // The project's own pages, the manual's and the packages': the Code Reference's would crowd out
            // everything else.
            docs: docs
                .library()
                .pages()
                .filter(|(_, page)| !page.generated || page.is_shelved())
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
        // The prefabs scripts add to Create Object, as the built-in New … entries are.
        let objects = self.inspector.types().create_objects.iter().enumerate().map(|(entry, made)| Added {
            key: format!("new:{}", made.menu),
            title: format!("New {}", made.place().1),
            keys: String::new(),
            icon: "plus",
            action: PaletteAction::CreateFrom(entry),
        });
        added.chain(scripts).chain(objects).collect()
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
        changed |= self.update_scene_headers();
        // The wheel changes the speed while flying; the setting follows it once flying stops.
        let fly = &mut self.scene.fly;
        if fly.speed() != self.fly_speed && !fly.is_looking() {
            changed |= self.memory.settings.set("fly_speed", SettingValue::Number(fly.speed()));
        } else if self.memory.settings.fly_speed != self.fly_speed && !fly.is_looking() {
            fly.set_speed(self.memory.settings.fly_speed);
        }
        self.fly_speed = self.scene.fly.speed();
        if changed {
            self.memory.save();
        }
        self.apply_settings();
    }

    /// Puts the settings into the tool strip, its handles and the overlays.
    fn apply_settings(&mut self) {
        let settings = &self.memory.settings;
        (self.tools.snap, self.tools.centred, self.tools.local) = (settings.snap, settings.centred, settings.local);
        let prefab_parts = settings.prefab_parts;
        self.set_prefab_parts_shown(prefab_parts);
        self.apply_scene_settings();
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
            PaletteAction::Tool(index) => self.ui.set_segment(self.scene.tool, index),
            PaletteAction::Create(kind) => {
                self.obj_action(ObjAction::Create { kind, parent: None, scene: None }, renderer)
            }
            PaletteAction::CreateFrom(entry) => {
                self.obj_action(ObjAction::CreateFrom { entry, parent: None, scene: None }, renderer)
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
        self.show_selection(None);
        tracing::info!("opened {}", self.game.levels.current().unwrap_or("a scene"));
    }

    /// The Scene view's tools: the tool strip's settings, a click to
    /// select, the handles under the pointer, and dragging them.
    fn update_scene_tools(&mut self, renderer: &mut Renderer) {
        self.tools.tool = Tool::from_index(self.ui.segment(self.scene.tool));
        let selected = self.selected();
        self.draw_other_scene_views(renderer);
        let Some(camera) = self.scene_camera() else {
            self.hovered_handle = None;
            self.ui.set_shapes(self.scene.handles, Vec::new());
            return;
        };
        let point = self.pointer.map(|(x, y)| [x / self.scale, y / self.scale]);
        let world = &self.game.engine.world;
        // The fields' handles are edited, not played: only while editing.
        let editing = (!self.is_playing()).then_some(selected).flatten();
        if let Some(point) = self.pressed_handle.take() {
            let tool = selected.and_then(|e| Some((e, self.tools.handle_at(world, renderer, &camera, e, point)?)));
            let types = self.inspector.types();
            if let Some((entity, handle)) = tool {
                self.tools.begin(world, renderer, &camera, entity, handle, point);
                self.begin_following();
            } else if let Some(entity) =
                editing.filter(|&entity| self.field_handles.begin(world, types, &camera, entity, point))
            {
                self.field_drag_start = ObjState::of(world, entity);
            } else if let Some(owner) = self.script_handles.begin(&camera, point) {
                // The gizmos' own handles may be on any Obj, and are dragged while playing too.
                self.field_drag_start = ObjState::of(world, owner);
            } else if let Some(entity) = editing.filter(|_| self.terrain_tools.is_hovered()) {
                // A stroke may add a sculpt layer as it starts, so the step starts before.
                let before = ObjState::of(world, entity);
                if self.terrain_tools.begin(&mut self.game, &camera, entity, point, (self.shift, self.control)) {
                    self.field_drag_start = before;
                }
            } else if let Some(entity) = selected.filter(|_| self.block_tools.is_hovered()) {
                // Blocks are built while playing too, as a game would build them; play keeps none of it.
                let before = ObjState::of(world, entity);
                if self.block_tools.begin(&mut self.game, renderer, &camera, entity, point, self.shift) {
                    self.field_drag_start = before;
                }
            } else if let Some(entity) = editing.filter(|_| self.plants_tools.is_hovered()) {
                let before = ObjState::of(world, entity);
                if self.plants_tools.begin(&mut self.game, renderer, entity, self.shift) {
                    self.field_drag_start = before;
                }
            } else if let Some(entity) = editing {
                // A Path's dots add or take away points as they are taken, so the step starts before.
                let before = ObjState::of(world, entity);
                if self.path_handles.begin(&mut self.game.engine.world, &camera, entity, point, self.shift) {
                    self.field_drag_start = before;
                }
            }
        }
        if self.tools.is_dragging()
            && let Some(point) = point
        {
            self.tools.drag_to(&mut self.game.engine.world, &camera, point, self.control);
            self.follow_active();
        }
        if self.field_handles.is_dragging()
            && let Some(point) = point
        {
            let snap = (self.control, self.tools.snap);
            let types = self.inspector.types().clone();
            self.field_handles.drag_to(&mut self.game.engine.world, &types, &camera, point, snap);
        }
        if self.path_handles.is_dragging()
            && let Some(point) = point
        {
            let snap = (self.control, self.tools.snap);
            self.path_handles.drag_to(&mut self.game.engine.world, &camera, point, snap);
        }
        if self.script_handles.is_dragging()
            && let Some(point) = point
        {
            self.script_handles.drag_to(&camera, point, (self.control, self.tools.snap));
        }
        if self.terrain_tools.is_dragging()
            && let Some(point) = point
        {
            self.terrain_tools.drag_to(&mut self.game, &camera, point, self.shift);
        }
        if self.block_tools.is_dragging()
            && let Some(point) = point
        {
            self.block_tools.drag_to(&mut self.game, &camera, point, self.shift);
        }
        if self.plants_tools.is_dragging()
            && let Some(point) = point
        {
            self.plants_tools.drag_to(&mut self.game, renderer, &camera, point);
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
            let over_field = over.filter(|_| self.hovered_handle.is_none());
            self.field_handles.hover(world, self.inspector.types(), &camera, editing, over_field);
            self.path_handles.hover(world, &camera, editing, over_field.filter(|_| !self.field_handles.is_hovered()));
            let free = !self.field_handles.is_hovered() && !self.path_handles.is_hovered();
            self.script_handles.hover(&camera, over_field.filter(|_| free));
            let free = free && !self.script_handles.is_hovered();
            self.terrain_tools.hover(&self.game, &camera, editing, over_field.filter(|_| free));
            let free = free && !self.terrain_tools.is_hovered();
            self.block_tools.hover(&self.game, &camera, selected, over_field.filter(|_| free));
            let free = free && !self.block_tools.is_hovered();
            self.plants_tools.hover(&self.game, renderer, &camera, editing, over_field.filter(|_| free));
            let world = &self.game.engine.world;
            let markers = self.markers(self.scene.show_gizmos);
            self.hovered_obj = over.and_then(|point| pick(world, renderer, &camera, point, markers));
            // Letting go of a box is not also a click.
            if !self.update_marquee(renderer, &camera, point) {
                self.click_to_select(renderer, &camera, point);
            }
        }
        // Gizmos go under the handles, which are drawn last.
        let world = &self.game.engine.world;
        let playing = |entity| self.game.audio.is_source_playing(world, entity);
        let markers = self.markers(self.scene.show_gizmos);
        let mut shapes = gizmos::shapes(world, &camera, self.selected(), markers, &playing);
        if self.scene.show_gizmos {
            shapes.extend(scene_gizmos::icon_shapes(&self.gizmo_icons(), &camera, self.selected()));
        }
        shapes.extend(self.field_handles.shapes(world, self.inspector.types(), &camera, editing));
        shapes.extend(self.path_handles.shapes(world, &camera, editing));
        shapes.extend(self.terrain_tools.shapes(&self.game, &camera, editing));
        shapes.extend(self.block_tools.shapes(&self.game, &camera, selected, self.shift));
        shapes.extend(self.plants_tools.shapes(&self.game, renderer, &camera, editing));
        shapes.extend(self.script_handles.shapes(&camera));
        shapes.extend(self.selection_shapes(renderer, &camera));
        shapes.extend(self.tools.shapes(world, renderer, &camera, self.selected()));
        self.ui.set_shapes(self.scene.handles, shapes);
    }

    /// A click on the 3D view itself, not on what floats over it, selects
    /// the Obj under it, or nothing.
    fn click_to_select(&mut self, renderer: &Renderer, camera: &ViewCamera, point: Option<[f32; 2]>) {
        let (viewport, overlays) =
            (self.scene.viewport, [self.scene.tool_strip, self.scene.stats, self.scene.view_gizmo]);
        if !self.ui.clicked(viewport) || overlays.iter().any(|&o| self.ui.clicked(o)) {
            return;
        }
        let Some(point) = point else { return };
        // A behaviour's gizmo is drawn over its Obj, so a click on its icon or one of its lines is meant for it.
        let gizmo = self.scene.show_gizmos.then(|| self.gizmo_under(camera, point)).flatten();
        let markers = self.markers(self.scene.show_gizmos);
        let picked = gizmo.or_else(|| pick(&self.game.engine.world, renderer, camera, point, markers));
        // Shift adds what was clicked to the selection, and Ctrl adds or takes it out.
        match (picked, self.control || self.shift) {
            (Some(entity), true) => self.add_to_selection(entity, self.control),
            (None, true) => {}
            (picked, false) => self.select(picked),
        }
    }

    /// Turns the model an Obj draws whole into an Obj for each part, as one
    /// step to undo.
    fn unpack_model(&mut self, entity: Entity, renderer: &mut Renderer) {
        let Some(before) = ObjState::of(&self.game.engine.world, entity) else { return };
        match crate::objs::unpack_model(&mut self.game, renderer, entity) {
            Ok(parts) => {
                let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
                let mut edits = vec![Edit::Changed { entity, before, after }];
                edits.extend(parts.into_iter().map(|part| Edit::Exists { root: part, taken: None }));
                self.history.record_step(Edit::Group(edits));
                self.unsaved = true;
                self.refresh_hierarchies();
            }
            Err(error) => tracing::error!("could not unpack the model: {error:#}"),
        }
    }

    /// Lets go of a handle; the drag is one step to undo.
    fn end_drag(&mut self) {
        let held = self.field_handles.end().or_else(|| self.script_handles.end()).or_else(|| self.path_handles.end());
        let held = held.or_else(|| self.terrain_tools.end()).or_else(|| self.block_tools.end());
        let held = held.or_else(|| self.plants_tools.end());
        if let Some(entity) = held {
            return self.end_field_drag(entity);
        }
        let Some((entity, start)) = self.tools.end() else { return };
        // The other selected Objs that followed it, as one step with it.
        let mut edits = self.end_following();
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        let before = ObjState { transform: start, ..after.clone() };
        if before != after {
            edits.insert(0, Edit::Changed { entity, before, after });
        }
        if !edits.is_empty() && !self.is_playing() {
            let edit = if edits.len() == 1 { edits.remove(0) } else { Edit::Group(edits) };
            self.history.record_step(edit);
            self.unsaved = true;
        }
    }

    /// Lets go of a field's handle, or a gizmo's: the fields it changed are
    /// one step to undo, from the Obj as it was when the drag began. While
    /// playing, only the played world changed.
    fn end_field_drag(&mut self, entity: Entity) {
        let (Some(before), Some(after)) = (self.field_drag_start.take(), ObjState::of(&self.game.engine.world, entity))
        else {
            return;
        };
        if before != after && !self.is_playing() {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Makes, duplicates, deletes or reparents Objs, as the Hierarchy asks.
    /// Each is a step to undo, except while playing, when only the played
    /// world changes.
    fn obj_action(&mut self, action: ObjAction, renderer: &mut Renderer) {
        match action {
            ObjAction::OpenPrefab(entity) => return self.open_prefab_of(entity, renderer),
            ObjAction::Unpack(entity) => return self.unpack_prefab(entity),
            ObjAction::UnpackModel(entity) => return self.unpack_model(entity, renderer),
            ObjAction::Frame(entity) => {
                self.select(Some(entity));
                return self.frame(&[entity], renderer);
            }
            ObjAction::AddToLevel(scene) => return self.add_scene_to_level(scene, renderer),
            ObjAction::CloseScene(scene) => return self.close_scene(scene),
            _ => {}
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
            ObjAction::CreateFrom { entry, parent, scene } => {
                let Some(made) = self.inspector.types().create_objects.get(entry) else { return };
                let path = made.prefab_path(game.content.db());
                match objs::create_from_prefab(game, Some(renderer), &path, parent, scene, position) {
                    Ok(made) => (Edit::Exists { root: made, taken: None }, Some(made)),
                    Err(error) => return self.notify(&format!("Could not make {path}: {error:#}")),
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
            ObjAction::OpenPrefab(_)
            | ObjAction::Unpack(_)
            | ObjAction::UnpackModel(_)
            | ObjAction::Frame(_)
            | ObjAction::AddToLevel(_)
            | ObjAction::CloseScene(_) => return,
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
        let Some(view) = self.game.engine.world.get::<Transform>(self.scene.camera) else { return Vec3::ZERO };
        let at = view.position + view.rotation * Vec3::Z * NEW_OBJ_DISTANCE;
        (at / self.tools.snap).round() * self.tools.snap
    }

    /// Does what a menu in the Inspector asks: keeps a value after play or
    /// stops keeping it, resets a value or the Transform, or adds or removes
    /// a component. Each change is a step to undo, except while playing.
    fn row_action(&mut self, action: RowAction, renderer: &mut Renderer) {
        let Some(entity) = self.inspector.shown() else { return };
        self.row_action_on(entity, action, renderer);
    }

    /// Does what a menu in an Inspector asks, for the Obj it shows.
    fn row_action_on(&mut self, entity: Entity, action: RowAction, renderer: &mut Renderer) {
        if let RowAction::EditColor(prop, swatch) = action {
            return self.edit_color(entity, prop, swatch);
        }
        if let RowAction::Browse(prop, anchor) = action {
            return self.browse(entity, prop, anchor);
        }
        if let RowAction::Ping(prop) | RowAction::SelectTarget(prop) = &action {
            return self.reference_target(entity, prop, matches!(action, RowAction::SelectTarget(_)));
        }
        if let RowAction::ViewLod(level) = action {
            return self.view_lod(entity, level, renderer);
        }
        if let RowAction::BrowseComponents(anchor) = action {
            let choices = self.inspector.component_choices(self.is_playing(), &self.game.project.modules());
            let target = (PickFor::AddComponent(entity), anchor);
            return self.picker.open(&mut self.ui, target, "Add Component", choices, "");
        }
        match &action {
            RowAction::ApplyOverrides(paths) => return self.apply_overrides(&[(entity, paths.clone())], renderer),
            RowAction::ApplyAll => return self.apply_instance(entity, renderer),
            RowAction::RevertAll => return self.revert_instance(entity, renderer),
            RowAction::CopyComponent(part) => return self.copy_component(entity, part),
            RowAction::UnpackModel => return self.unpack_model(entity, renderer),
            _ => {}
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
            RowAction::Browse(..) | RowAction::EditColor(..) | RowAction::Ping(_) | RowAction::SelectTarget(_) => {}
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
            RowAction::ViewLod(_) | RowAction::BrowseComponents(_) | RowAction::UnpackModel => {}
            RowAction::ForceLod(lod) => {
                props::set_forced_lod(&mut self.game.engine.world, entity, lod);
            }
            RowAction::ResetLodTransitions => {
                props::set_lod_transitions(&mut self.game.engine.world, entity, None);
            }
            RowAction::PutOnBone(prop, bone) => {
                if let Some(at) = props::bone_place(&self.game.engine.world, entity, bone) {
                    let types = self.inspector.types().clone();
                    let placed = props::Value::Vector(at.to_vec());
                    props::set(&mut self.game, Some(renderer), &types, entity, &prop, placed);
                }
            }
            // The Obj dragged, or every Obj selected when it is one of them.
            RowAction::DropOnLodLevel(level, source) => {
                if let Some(obj) = self.dragged_obj(&source) {
                    let selected = self.selected_objs();
                    let objs = if selected.contains(&obj) { selected } else { vec![obj] };
                    if !props::put_in_lod_level(&mut self.game, entity, level, &objs) {
                        self.notify("A LOD Group's levels take Objs inside its Obj");
                    }
                }
            }
            RowAction::InsertLodLevel(at) => {
                props::insert_lod_level(&mut self.game, entity, at);
            }
            RowAction::ErodeTerrain => {
                if !crate::terrain_props::erode(&mut self.game, renderer, entity) {
                    self.notify("Could not work the layer out: see the Console");
                }
            }
            RowAction::AddRiverWater => self.add_river_water(entity, renderer),
            RowAction::AddTerrainLayer(kind) => {
                let assets = self.game.content.db().entries().count();
                crate::terrain_props::add_layer(&mut self.game, entity, kind);
                // An Erosion or Rivers layer is worked out over the ground under it as it is added.
                if crate::terrain_props::worked_stale(&self.game.engine.world, entity).is_some_and(|(_, stale)| stale) {
                    crate::terrain_props::erode(&mut self.game, renderer, entity);
                }
                // A Generate layer may have made a world generator, to show in the Project panel.
                if self.game.content.db().entries().count() != assets {
                    self.files_seen();
                    self.project.refresh(&mut self.ui, self.game.content.db(), None);
                }
            }
            RowAction::DeleteLodLevel(at) => {
                props::delete_lod_level(&mut self.game.engine.world, entity, at);
            }
            RowAction::RevertOverrides(paths) => self.revert_overrides(entity, &paths, renderer),
            RowAction::PasteComponent => self.paste_component(entity),
            RowAction::ApplyOverrides(_)
            | RowAction::ApplyAll
            | RowAction::RevertAll
            | RowAction::CopyComponent(_) => {}
        }
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        // A data asset's change goes into its file instead (see `data_assets`).
        if before != after && !self.is_playing() && !self.holds_data(entity) {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Points a reference at an Obj dragged from the Hierarchy or an asset
    /// from the Project, if it is one the field's menu offers; otherwise the
    /// status bar says why not.
    fn drop_reference(&mut self, entity: Entity, prop: &Prop, source: &DragSource, renderer: &mut Renderer) {
        let pick = match (self.dragged_obj(source), self.dragged_asset(source)) {
            (Some(obj), _) => Pick::Obj(Some(obj)),
            (None, Some(path)) => Pick::Asset(Some(path)),
            (None, None) => return,
        };
        let types = self.inspector.types();
        // A clip field takes a clip, or a model file with clips, as the Animator's clip does.
        if let (Pick::Asset(Some(_)), true) = (&pick, props::takes_clips(types, prop)) {
            let types = types.clone();
            if !props::pick(&mut self.game, Some(renderer), &types, entity, prop, &pick) {
                self.notify("Only a clip, or a model file with clips, goes there");
            }
            return;
        }
        // A model's part, from the Project panel's opened-out model, fits the field of its kind.
        let part = match (&pick, prop) {
            (Pick::Asset(Some(reference)), Prop::Mesh) => reference.contains("#mesh"),
            (Pick::Asset(Some(reference)), Prop::Material) => reference.contains("#material"),
            _ => false,
        };
        if part || props::choices(&self.game, types, entity, prop).iter().any(|(_, offered)| *offered == pick) {
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
        // A line about an Obj or an asset shows it, as Unity's log context does, leaving the selection alone.
        if let Some(entity) = result.ping.filter(|&e| self.game.engine.world.is_alive(e)) {
            self.ping_obj(entity);
        }
        if let Some(asset) = result.ping_asset {
            let db = self.game.content.db();
            let path = asset.parse().ok().and_then(|guid| db.get(guid)).map_or(asset.clone(), |e| e.path.clone());
            self.ping_asset(&path);
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
    /// only once a script changes again. The packages changing builds them
    /// too, since their scripts build with the project's.
    fn update_script_types(&mut self) {
        if let Some(describing) = &self.describing {
            match describing.try_recv() {
                Ok(Ok(types)) => {
                    for inspector in self.inspectors_mut() {
                        inspector.set_types(types.clone());
                    }
                    self.retry_script_windows();
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
        let forced = std::mem::take(&mut self.rebuild_scripts);
        if !forced && self.scripts_checked.is_some_and(|at| at.elapsed() < SCRIPTS_CHECK) {
            return;
        }
        self.scripts_checked = Some(Instant::now());
        let project = &self.game.project;
        let changed = self.described_from.is_none_or(|from| script_types::changed_since(project, from));
        if !forced && (!changed || !script_types::is_stale(project)) {
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
        for inspector in self.inspectors_mut() {
            inspector.set_types(types.clone());
        }
        self.retry_script_windows();
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
        if self.ui.clicked(self.scene.pivot) {
            changed |= settings.set("handle_position", SettingValue::Choice(usize::from(!settings.centred)));
        }
        if self.ui.clicked(self.scene.space) {
            changed |= settings.set("handle_space", SettingValue::Choice(usize::from(!settings.local)));
        }
        if self.ui.clicked(self.scene.snap)
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
            Command::Duplicate if self.duplicate_many(renderer) => {}
            Command::Duplicate => {
                if let Some(entity) = selected {
                    self.obj_action(ObjAction::Duplicate(entity), renderer);
                }
            }
            Command::Frame => self.frame(&self.selected_objs(), renderer),
            Command::Layers => self.inspect_layers(),
            Command::PseudoText => self.toggle_pseudo_text(),
            Command::BakePaths => self.bake_paths(),
            Command::Delete if self.delete_many() => {}
            Command::Delete => {
                if let Some(entity) = selected {
                    self.obj_action(ObjAction::Delete(entity), renderer);
                }
            }
            _ if self.is_playing() => {
                self.notify("Stop playing to save or undo; edits while playing are not kept");
            }
            Command::Save => self.save(),
            Command::AlignWithView => self.align_with_view(),
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
                        self.reselect(selected);
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

    /// Whether the pointer is over the Scene view in use, and not over a
    /// window floating on it: there the right mouse button flies its camera.
    pub fn over_viewport(&self) -> bool {
        let Some((x, y)) = self.pointer else { return false };
        if !self.shown(self.scene.viewport) || !self.ui.is_hovered(self.scene.viewport) {
            return false;
        }
        let [vx, vy, vw, vh] = self.viewport_rect(Some(self.scene.viewport), (u32::MAX, u32::MAX));
        (vx as f32..(vx + vw) as f32).contains(&x) && (vy as f32..(vy + vh) as f32).contains(&y)
    }

    /// The 3D views drawn this frame, each with its camera: each Scene view
    /// that shows, through its own camera, in the order they were opened,
    /// and the Game view, through the level's first camera, when it shows and
    /// the level has one. All draw at once, one floating over another. The
    /// order holds while the Scene view in use changes, so each view keeps
    /// its own history in the renderer.
    fn active_views(&self) -> Vec<(NodeId, Entity)> {
        let game = self.game_camera().filter(|_| self.shown(self.nodes.game_viewport));
        let game = game.map(|camera| (self.nodes.game_viewport, camera));
        let mut views: Vec<_> = self.shown_scene_views().into_iter().chain(game).collect();
        // Those in floating windows draw last, over the docked ones.
        views.sort_by_key(|&(viewport, _)| self.floats(viewport));
        views
    }

    /// Whether an element is in a floating window.
    fn floats(&self, node: NodeId) -> bool {
        std::iter::successors(self.ui.parent(node), |&n| self.ui.parent(n)).any(|n| self.ui.has_class(n, "dock-float"))
    }

    /// How a view draws: through its camera, and a Scene view shaded as its
    /// menu says, with behaviours' gizmos while its Gizmos button is on; the
    /// Game view always lit, and without gizmos.
    fn view_options(&self, viewport: NodeId, camera: Entity) -> ViewOptions {
        let scene_view = self.scene_view_at(viewport);
        let debug_view = scene_view.map_or(0, |view| view.debug_view());
        // Behaviours' gizmos show in a Scene view with its Gizmos button on, and never in the Game view.
        let gizmos = scene_view.is_some_and(|view| view.show_gizmos);
        ViewOptions { camera: Some(camera), debug_view, gizmos, ..ViewOptions::default() }
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
        if self.game_ui_takes(event) || self.panels_take(event) {
            return true;
        }
        if self.input_listening_takes(event) || self.game_input_takes(event) {
            return true;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => self.pointer = Some((position.x as f32, position.y as f32)),
            WindowEvent::Focused(focused) => {
                self.build_window.set_window_focused(*focused);
                // Files may have changed while the editor was away.
                if *focused {
                    self.imports.look_again();
                }
            }
            // A file dropped on the window goes into the project, or the VoxDocs page being written.
            WindowEvent::DroppedFile(path) => self.file_dropped(path),
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
            let over_handle = self.hovered_handle.is_some()
                || self.field_handles.is_hovered()
                || self.path_handles.is_hovered()
                || self.script_handles.is_hovered()
                || self.terrain_tools.is_hovered()
                || self.block_tools.is_hovered()
                || self.plants_tools.is_hovered();
            if pressed && over_handle && self.over_viewport() {
                self.pressed_handle = self.pointer.map(|(x, y)| [x / self.scale, y / self.scale]);
                return true;
            }
            // A press and release inside one frame still ends the drag the press begins.
            let dragging = self.tools.is_dragging()
                || self.field_handles.is_dragging()
                || self.path_handles.is_dragging()
                || self.script_handles.is_dragging()
                || self.terrain_tools.is_dragging()
                || self.block_tools.is_dragging()
                || self.plants_tools.is_dragging();
            if !pressed && (dragging || self.pressed_handle.is_some()) {
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
            let sideways = matches!(named, NamedKey::ArrowLeft | NamedKey::ArrowRight);
            if sideways && self.picker.side_key(&mut self.ui, *named == NamedKey::ArrowRight) {
                return true;
            }
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
            self.ui.set_segment(self.scene.tool, index);
            return true;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && !self.control
            && !self.ui.wants_keyboard()
            && let winit::keyboard::Key::Character(text) = &key.logical_key
            && matches!(text.as_str(), "[" | "]" | "{" | "}")
            && (crate::terrain_props::ui(&self.game.engine.world).tool.brushes()
                || crate::plants_props::ui(&self.game.engine.world).tool == crate::plants_props::FoliageTool::Paint)
        {
            // [ and ] size the terrain brush, and with Shift its strength, as painting tools do.
            let brush = &mut crate::terrain_props::ui_mut(&mut self.game.engine.world).brush;
            let up = matches!(text.as_str(), "]" | "}");
            match self.shift || matches!(text.as_str(), "{" | "}") {
                true => brush.strength = (brush.strength + if up { 0.1 } else { -0.1 }).clamp(0.0, 1.0),
                false => brush.radius = (brush.radius * if up { 1.25 } else { 0.8 }).clamp(0.1, 1000.0),
            }
            return true;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && !self.control
            && !self.ui.wants_keyboard()
            && !self.fly().is_looking()
            && let winit::keyboard::Key::Character(text) = &key.logical_key
            && text.to_lowercase() == "f"
        {
            self.commands.push(Command::Frame);
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
                ("f", true) => Some(Command::AlignWithView),
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
        let used = voxear_engine::ui_input::handle(&mut self.ui, event);
        // The Scene view under the pointer is the one a right click flies, even before the next frame.
        if let WindowEvent::CursorMoved { .. } = event {
            self.follow_pointer();
        }
        used
    }

    /// Lights an asset for a moment in every Project panel not locked,
    /// opening the folder it is in, as clicking a field naming it does.
    /// `reference` is its path, or its GUID or any reference to it or to a
    /// part of it, such as a model's mesh.
    pub fn ping_asset(&mut self, reference: &str) {
        let path = props::path_of(&self.game, reference).unwrap_or_else(|| reference.to_string());
        let panels = std::iter::once(&mut self.project).chain(self.extra_projects.iter_mut().map(|extra| extra.project_mut()));
        for project in panels.filter(|project| !project.is_locked()) {
            project.ping(&mut self.ui, &path);
        }
    }

    /// Pings what a reference of `entity` points at, as Unity's object fields
    /// do: an Obj in the Hierarchy, or an asset in the Project panel. With
    /// `select`, an Obj is selected instead, as a double click does.
    fn reference_target(&mut self, entity: Entity, prop: &Prop, select: bool) {
        let world = &self.game.engine.world;
        let target = match prop {
            Prop::Field { behaviour, path } => fields::target(world, self.inspector.types(), entity, behaviour, path),
            Prop::LodObj(..) => props::obj_target(world, entity, prop).map(fields::Target::Obj),
            // A mesh, material or other asset of the project's, by its file.
            _ => props::asset_path(&self.game, entity, prop).map(fields::Target::Asset),
        };
        match target {
            Some(fields::Target::Obj(obj)) if select => self.select(Some(obj)),
            Some(fields::Target::Obj(obj)) => self.ping_obj(obj),
            Some(fields::Target::Asset(path)) => self.ping_asset(&path),
            None => {}
        }
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
        self.picker.open(&mut self.ui, (PickFor::Field(entity, prop), anchor), &format!("Select {title}"), choices, &now);
    }

    /// The object picker's frame, and what it or its keys picked: for the
    /// Obj it was opened for, while it is there, or a level's scene.
    fn update_picker(&mut self, renderer: &mut Renderer) {
        let clicked = self.picker.update(&mut self.ui);
        for (target, picked) in std::mem::take(&mut self.picker_picks).into_iter().chain(clicked) {
            let alive = |entity: Entity| self.game.engine.world.is_alive(entity);
            match (target, picked) {
                // By whichever Inspector it was opened from.
                (PickFor::Field(entity, prop), Chosen::Value(pick)) if alive(entity) => {
                    self.row_action_on(entity, RowAction::Pick(prop, pick), renderer);
                }
                (PickFor::LevelScene { level, at }, Chosen::Value(pick)) => {
                    self.pick_level_scene(&level, at, &pick, renderer);
                }
                (PickFor::StandsOn { level }, Chosen::Value(pick)) => self.pick_stands_on(&level, &pick, renderer),
                (PickFor::MotionSetClip { set, slot }, Chosen::Value(pick)) => {
                    self.pick_motion_set_clip(&set, &slot, &pick);
                }
                (PickFor::GunHandlingClip { handling, field }, Chosen::Value(pick)) => {
                    self.pick_gun_handling_clip(&handling, field, &pick);
                }
                (PickFor::BlockRef { block, field }, Chosen::Value(pick)) => self.pick_block_ref(&block, field, &pick),
                (PickFor::PromptIcon { icons, slot }, Chosen::Value(pick)) => self.pick_prompt_icon(&icons, &slot, &pick),
                (PickFor::InputPromptIcons, Chosen::Value(pick)) => self.pick_input_prompt_icons(&pick),
                (PickFor::MaterialMap { material, map }, Chosen::Value(pick)) => {
                    self.pick_material_map(&material, map, &pick, renderer);
                }
                (PickFor::Remap { model, part }, Chosen::Value(pick)) => self.pick_remap(&model, &part, &pick, renderer),
                (PickFor::MaterialShader { material }, Chosen::Value(pick)) => {
                    self.pick_material_shader(&material, &pick, renderer);
                }
                (PickFor::ShaderPicture { material, property }, Chosen::Value(pick)) => {
                    self.pick_shader_picture(&material, &property, &pick, renderer);
                }
                (PickFor::NewShader { folder }, Chosen::Shader(start)) => self.make_shader(&folder, start, renderer),
                (PickFor::NewDataAsset { folder }, Chosen::Definition(definition)) => {
                    self.make_data_asset(&folder, &definition, renderer);
                }
                (PickFor::PartMaterial { owner, node }, Chosen::Value(pick)) if alive(owner) => {
                    self.pick_part_material(owner, node, &pick, renderer);
                }
                (PickFor::AddComponent(entity), Chosen::Component(part)) if alive(entity) => {
                    self.row_action_on(entity, RowAction::Add(part), renderer);
                }
                (PickFor::InputControl | PickFor::ActionIcon, picked) => self.pick_for_input(picked),
                _ => {}
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
        // The pointer is where the Game view is only in the window it is in.
        let pointing = matches!(event, WindowEvent::CursorMoved { .. } | WindowEvent::MouseInput { .. });
        if pointing && self.ui.window_of(self.nodes.game_viewport) != self.ui.pointer_window() {
            return false;
        }
        // A press the editor's UI holds, as a tab being dragged, lets go there even over the game's UI.
        // Taken by the game's instead, a tab let go over the playing Game view stayed dragged for good.
        if matches!(event, WindowEvent::MouseInput { state: winit::event::ElementState::Released, .. })
            && self.ui.holds_press()
        {
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

    /// Hands the UI Panels in the world what happens over the Game view while
    /// the game plays: where the pointer is, which they are pointed at from,
    /// and the clicks, wheel and keys that are theirs. Returns whether one
    /// took the event.
    fn panels_take(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        if !self.is_playing() || !self.shown(self.nodes.game_viewport) {
            self.game_pointer = None;
            return false;
        }
        let pointing = matches!(event, WindowEvent::CursorMoved { .. } | WindowEvent::MouseInput { .. });
        if pointing && self.ui.window_of(self.nodes.game_viewport) != self.ui.pointer_window() {
            return false;
        }
        if matches!(event, WindowEvent::MouseInput { state: winit::event::ElementState::Released, .. })
            && self.ui.holds_press()
        {
            return false;
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let [x, y, w, h] = self.ui.rect(self.nodes.game_viewport).map(|v| v * self.scale);
                let (px, py) = (position.x as f32, position.y as f32);
                self.game_pointer = (px >= x && px < x + w && py >= y && py < y + h).then_some([px, py]);
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.game_pointer = None;
                false
            }
            _ => self.game.ui.panel_input(event),
        }
    }

    /// Hands the game's input what the player does while it plays, as
    /// Unity's Game view does: the pointer over the view, in its own pixels,
    /// and while the view has the focus, which a press on it gives and a
    /// press elsewhere takes, the mouse's buttons and wheel and the keys.
    /// Keys go no further, so the editor's shortcuts do not fire, but for
    /// those with Ctrl, and keys a text field of the editor's has.
    fn game_input_takes(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::{ElementState, WindowEvent};
        if !self.is_playing() || !self.shown(self.nodes.game_viewport) {
            self.game_focused = false;
            return false;
        }
        let in_window = self.ui.window_of(self.nodes.game_viewport) == self.ui.pointer_window();
        let [x, y, w, h] = self.ui.rect(self.nodes.game_viewport).map(|v| v * self.scale);
        let over = |px: f32, py: f32| in_window && px >= x && px < x + w && py >= y && py < y + h;
        let pointer = self.pointer.filter(|&(px, py)| over(px, py));
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x as f32, position.y as f32);
                if over(px, py) {
                    self.game.input.devices_mut().set_pointer(px - x, py - y);
                }
                false
            }
            // While the game holds the mouse, its presses are the game's wherever the hidden pointer was left: on the
            // Play button, a shot would stop the game.
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } if self.game_holds_mouse() => {
                self.game.input.handle(event);
                true
            }
            WindowEvent::MouseInput { state, .. } => {
                if *state == ElementState::Pressed {
                    self.game_focused = pointer.is_some();
                    self.cursor_freed = false;
                }
                if self.game_focused {
                    self.game.input.handle(event);
                }
                false
            }
            WindowEvent::MouseWheel { .. } if self.game_focused && pointer.is_some() => {
                self.game.input.handle(event);
                true
            }
            WindowEvent::KeyboardInput { event: key, .. } if self.game_focused && !self.ui.wants_keyboard() => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                if key.physical_key == PhysicalKey::Code(KeyCode::Escape) && key.state == ElementState::Pressed {
                    self.cursor_freed = true;
                }
                self.game.input.handle(event);
                !self.control
            }
            WindowEvent::Focused(false) => {
                self.cursor_freed = true;
                self.game.input.release_all();
                false
            }
            _ => false,
        }
    }

    /// Whether the mouse shows, and how it is held, as the game's scripts
    /// asked, while the Game view has the focus; `None` leaves it the
    /// editor's, free.
    pub fn game_cursor(&self) -> Option<(bool, voxear_scripting::CursorLock)> {
        let display = &self.game.display;
        (self.game_focused && !self.cursor_freed && self.is_playing())
            .then_some((display.cursor_visible, display.cursor_lock))
    }

    /// Whether the game has the mouse locked to itself, hidden, as a shooter
    /// has it.
    fn game_holds_mouse(&self) -> bool {
        self.game_cursor().is_some_and(|(_, lock)| lock == voxear_scripting::CursorLock::Locked)
    }

    /// The middle of the Game view, in physical pixels of the window the
    /// pointer is in, where a mouse the game locks is put; none when the
    /// view is not showing there.
    pub fn game_view_middle(&self) -> Option<(f32, f32)> {
        let view = self.nodes.game_viewport;
        if !self.shown(view) || self.ui.window_of(view) != self.ui.pointer_window() {
            return None;
        }
        let [x, y, w, h] = self.ui.rect(view).map(|v| v * self.scale);
        Some((x + w / 2.0, y + h / 2.0))
    }

    /// The mouse's moves from the device, which the game hears while its
    /// view has the focus.
    pub fn game_mouse_moved(&mut self, dx: f64, dy: f64) {
        if self.game_focused && self.is_playing() {
            self.game.input.mouse_moved(dx, dy);
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

    /// Makes the dock area around each 3D view see-through, wherever its
    /// panel has been docked, with a ring around its edge that rounds the
    /// view's corners to the panel's (`.viewport-area`). The ring is the
    /// area's own, so a floating window over it covers it. Each view is
    /// see-through too, so what the UI drew under a floating one, such as
    /// the Scene view's gizmos, does not show through it.
    fn mark_viewport_areas(&mut self, views: &[(NodeId, Entity)]) {
        let viewports: Vec<NodeId> = self.all_scene_views().map(|view| view.viewport).collect();
        for viewport in viewports.into_iter().chain([self.nodes.game_viewport]) {
            self.ui.set_see_through(viewport, views.iter().any(|&(shown, _)| shown == viewport));
        }
        let areas: Vec<NodeId> = views.iter().filter_map(|&(viewport, _)| self.area_of(viewport)).collect();
        if areas == self.viewport_areas {
            return;
        }
        for &old in &self.viewport_areas {
            self.ui.remove_class(old, "viewport-area");
        }
        for &new in &areas {
            self.ui.add_class(new, "viewport-area");
        }
        self.viewport_areas = areas;
    }

    /// The dock area a panel's element is in.
    fn area_of(&self, node: NodeId) -> Option<NodeId> {
        std::iter::successors(self.ui.parent(node), |&n| self.ui.parent(n)).find(|&n| self.ui.has_class(n, "dock-area"))
    }

    fn update_status(&mut self, renderer: &Renderer, delta: f32) {
        let notice = self.notice.as_ref().filter(|(_, at)| at.elapsed().as_secs_f32() < NOTICE_SECONDS);
        let status = match notice {
            Some((text, _)) => text.clone(),
            None => match self.path_bake_status() {
                Some(baking) => baking,
                None if self.unsaved && !self.is_playing() => "Unsaved changes · Ctrl+S".to_string(),
                None => self.play.status().to_string(),
            },
        };
        self.ui.set_text(self.nodes.status_left, &status);
        let busy = self.play.is_building() || self.describing.is_some() || self.path_bake.is_some();
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
        let first = self.all_scene_views().find(|view| view.panel == scene_views::FIRST).map(|view| view.stats);
        if let Some(stats) = first {
            self.ui.set_text(stats, &chip);
        }
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

/// The pages of the packages whose assets the content has mounted, for
/// VoxDocs to show beside the manual: each package's `Docs` folder, under
/// its title.
pub(crate) fn package_docs(mounts: &voxear_assets::Mounts) -> Vec<voxear_docs::PackageDocs> {
    let docs = voxear_engine::packages::contents::DOCS_FOLDER;
    let packages = mounts.packages().iter();
    packages
        .map(|package| voxear_docs::PackageDocs {
            name: package.name.clone(),
            title: package.title.clone(),
            folder: package.root.join(docs),
        })
        .collect()
}

/// The Code Reference for VoxDocs, as the scripts were last described, if
/// they have been.
fn code_reference(project: &Project) -> Option<voxear_docs::CodeReference> {
    let path = script_types::code_path(project).filter(|path| path.is_file())?;
    voxear_docs::CodeReference::read(&path).inspect_err(|error| tracing::warn!("VoxDocs: {error:#}")).ok()
}
