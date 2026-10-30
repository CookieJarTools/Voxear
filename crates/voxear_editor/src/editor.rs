//! The editor itself: the open project in edit mode, its panels, and the
//! Scene view, which looks through the editor's own camera.

use std::path::Path;

use anyhow::{Context, Result};
use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec3},
};
use voxear_ecs::Entity;
use voxear_engine::{Engine, FlyCamera, Game, Project};
use voxear_render::{Camera, MeshRenderer, Renderer, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_ui::{NodeId, Ui};

use crate::{console::Console, hierarchy::EditorOnly, hierarchy::Hierarchy, inspector::Inspector, project_view};

const MARKUP: &str = include_str!("../ui/Editor.vui");
const SHEET: &str = include_str!("../ui/Editor.vss");
/// The layout a project opens with until it has one of its own, as in the concept.
const LAYOUT: &str =
    "row(250, column(380, [hierarchy], [project]), row(740, column(470, [scene, game], [console]), [inspector]))";
/// How often the status bar's numbers change.
const STATUS_INTERVAL: f32 = 0.5;

/// Elements the editor reads and writes, found by name in Editor.vui.
struct Nodes {
    viewport: NodeId,
    command: NodeId,
    status_left: NodeId,
    status_right: NodeId,
    buttons: [(NodeId, &'static str); 4],
}

pub struct Editor {
    pub game: Game,
    pub ui: Ui,
    pub fly: FlyCamera,
    nodes: Nodes,
    hierarchy: Hierarchy,
    inspector: Inspector,
    console: Console,
    /// The Scene view's own camera, which the Hierarchy leaves out.
    camera: Entity,
    /// The dock area showing the viewport, made see-through.
    viewport_area: Option<NodeId>,
    /// The pointer, in physical pixels, for whether it is over the viewport.
    pointer: Option<(f32, f32)>,
    scale: f32,
    /// Control, or Command on macOS, is held.
    control: bool,
    branch: Option<String>,
    status: (u32, f32),
}

impl Editor {
    /// Opens `project`'s start level in edit mode: its Objs load with their
    /// behaviours as data, and no script runs.
    pub fn open(project: Project, renderer: &mut Renderer, log: LogCapture) -> Result<Self> {
        let root = project.root.clone();
        let mut engine = Engine::new();
        engine.set_edit_mode(true);
        let mut game = Game::new(project, engine)?;
        game.start(Some(renderer)).context("opening the start level")?;
        let camera = spawn_camera(&mut game);
        renderer.set_view_options(ViewOptions { camera: Some(camera), ..ViewOptions::default() });

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
            viewport: find(&ui, "viewport"),
            command: find(&ui, "command"),
            status_left: find(&ui, "status-left"),
            status_right: find(&ui, "status-right"),
            buttons: [
                (find(&ui, "play"), "Play"),
                (find(&ui, "pause"), "Pause"),
                (find(&ui, "step"), "Step"),
                (find(&ui, "build"), "Build"),
            ],
        };
        let world_tab = find(&ui, "workspace-world");
        ui.set_checked(world_tab, true);
        let dock = find(&ui, "dock");
        ui.set_dock_layout(dock, LAYOUT).expect("the default layout is valid");
        let (tree, filter) = (find(&ui, "hierarchy-tree"), find(&ui, "hierarchy-filter"));
        let hierarchy = Hierarchy::new(&mut ui, tree, filter);
        let project_tree = find(&ui, "project-tree");
        project_view::show(&mut ui, project_tree, game.content.db());
        let console_list = find(&ui, "console-list");
        let console = Console::new(&mut ui, console_list, log);
        let mut inspector = Inspector::new(find(&ui, "inspector"));
        inspector.show(&mut ui, &game.engine.world, None);

        tracing::info!("opened {} in edit mode", game.project.name());
        Ok(Self {
            game,
            ui,
            fly: FlyCamera::default(),
            nodes,
            hierarchy,
            inspector,
            console,
            camera,
            viewport_area: None,
            pointer: None,
            scale: 1.0,
            control: false,
            branch: git_branch(&root),
            status: (0, 0.0),
        })
    }

    /// Selects an Obj, as a click in the Hierarchy does.
    pub fn select(&mut self, entity: Option<Entity>) {
        if let Some(entity) = entity {
            self.hierarchy.select(&mut self.ui, &self.game.engine.world, entity);
        }
        self.inspector.show(&mut self.ui, &self.game.engine.world, entity);
    }

    /// The first Obj with a mesh, for showing something selected.
    pub fn first_mesh(&self) -> Option<Entity> {
        let world = &self.game.engine.world;
        let mut meshes: Vec<Entity> =
            world.query_ref::<(Entity, &MeshRenderer)>().without::<EditorOnly>().into_iter().map(|(e, _)| e).collect();
        meshes.sort_by_key(|e| e.index());
        meshes.first().copied()
    }

    /// One frame: input already given to the UI, then panels, the Scene
    /// view's rectangle, and the render.
    pub fn update(&mut self, renderer: &mut Renderer, size: (u32, u32), scale: f32, delta: f32) -> Result<()> {
        self.scale = scale;
        self.ui.advance(delta);
        if let Some(transform) = self.game.engine.world.get_mut::<Transform>(self.camera) {
            self.fly.update_transform(transform, delta);
        }
        self.game.tick(delta, Some(renderer));

        self.hierarchy.update(&mut self.ui, &self.game.engine.world);
        if let Some(selected) = self.hierarchy.selection_changed(&self.ui) {
            self.inspector.show(&mut self.ui, &self.game.engine.world, selected);
        }
        self.inspector.update(&mut self.ui, &mut self.game.engine.world);
        self.console.update(&mut self.ui);
        for (button, name) in self.nodes.buttons {
            if self.ui.clicked(button) {
                tracing::info!("{name} comes with play mode, the next step of the editor");
            }
        }
        self.mark_viewport_area();
        self.update_status(renderer, delta);

        let frame = self.ui.frame(size.0, size.1, scale);
        renderer.set_view_rect(Some(self.viewport_rect(size)));
        renderer.set_ui(&frame)?;
        renderer.render(&self.game.engine.world, self.game.engine.time().elapsed as f32)
    }

    /// Whether a physical point is over the Scene view, where the right
    /// mouse button flies the camera.
    pub fn over_viewport(&self) -> bool {
        let Some((x, y)) = self.pointer else { return false };
        let [vx, vy, vw, vh] = self.viewport_rect((u32::MAX, u32::MAX));
        (vx as f32..(vx + vw) as f32).contains(&x)
            && (vy as f32..(vy + vh) as f32).contains(&y)
            && self.viewport_shown()
    }

    /// Gives the UI a window event; returns whether it used it. Ctrl+K goes
    /// to the command bar.
    pub fn handle(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::{ElementState, WindowEvent};
        match event {
            WindowEvent::CursorMoved { position, .. } => self.pointer = Some((position.x as f32, position.y as f32)),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.control = modifiers.state().control_key() || modifiers.state().super_key();
            }
            _ => {}
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event
            && key.state == ElementState::Pressed
            && key.logical_key == winit::keyboard::Key::Character("k".into())
            && self.control
        {
            self.ui.focus(self.nodes.command);
            self.ui.key_down(voxear_ui::Key::Char('a'));
            return true;
        }
        voxear_engine::ui_input::handle(&mut self.ui, event)
    }

    /// The viewport's rectangle in physical pixels, within `size`.
    fn viewport_rect(&self, size: (u32, u32)) -> [u32; 4] {
        if !self.viewport_shown() {
            return [0, 0, 1, 1];
        }
        let [x, y, w, h] = self.ui.rect(self.nodes.viewport);
        let s = self.scale;
        let (x, y) = ((x * s).round().max(0.0) as u32, (y * s).round().max(0.0) as u32);
        let (w, h) = ((w * s).round().max(1.0) as u32, (h * s).round().max(1.0) as u32);
        [x.min(size.0.saturating_sub(1)), y.min(size.1.saturating_sub(1)), w, h]
    }

    /// Whether the Scene panel is the tab showing in its area.
    fn viewport_shown(&self) -> bool {
        let mut at = Some(self.nodes.viewport);
        while let Some(node) = at {
            if !self.ui.is_visible(node) {
                return false;
            }
            at = self.ui.parent(node);
        }
        true
    }

    /// Makes the dock area around the viewport see-through, wherever the
    /// Scene panel has been docked.
    fn mark_viewport_area(&mut self) {
        let mut at = self.ui.parent(self.nodes.viewport);
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
    }

    fn update_status(&mut self, renderer: &Renderer, delta: f32) {
        self.status.0 += 1;
        self.status.1 += delta;
        if self.status.1 < STATUS_INTERVAL && self.status.0 > 1 {
            return;
        }
        let fps = self.status.0 as f32 / self.status.1.max(1e-3);
        self.status = (0, 0.0);
        let level = self.game.levels.current().unwrap_or("no level").to_string();
        let objs = self.game.engine.world.query_ref::<&Name>().without::<EditorOnly>().into_iter().count();
        self.ui.set_text(self.nodes.status_left, &format!("Edit mode  ·  {level}  ·  {objs} Objs"));
        let gpu_ms: f32 = renderer.timings().iter().map(|(_, ms)| ms).sum();
        let mut right = format!("Vulkan  ·  {}  ·  {fps:.0} fps  ·  GPU {gpu_ms:.2} ms", renderer.gpu().name);
        if let Some(branch) = &self.branch {
            right.push_str(&format!("  ·  {branch}"));
        }
        self.ui.set_text(self.nodes.status_right, &right);
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
