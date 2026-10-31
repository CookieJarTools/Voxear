//! The editor itself: the open project in edit mode, its panels, and the
//! Scene view, which looks through the editor's own camera.

use std::{path::Path, time::Instant};

use anyhow::{Context, Result};
use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec3},
};
use voxear_ecs::Entity;
use voxear_engine::{Engine, FlyCamera, Game, Project};
use voxear_render::{Camera, MeshRenderer, Renderer, ViewOptions};
use voxear_scene::{Name, Transform};
use voxear_ui::{Color, NodeId, Shape, Ui};

use crate::{
    console::Console, hierarchy::EditorOnly, hierarchy::Hierarchy, inspector::Inspector, project_view::ProjectView,
};

const MARKUP: &str = include_str!("../ui/Editor.vui");
const SHEET: &str = include_str!("../ui/Editor.vss");
/// The layout a project opens with until it has one of its own: the
/// Hierarchy on the left, the Scene in the middle over the Project and
/// Console, and the Inspector on the right. The Project sits under the Scene,
/// as in Unity, because its folders and files need the width.
const LAYOUT: &str = "row(240, [hierarchy], row(-300, column(66%, [scene, game], [project, console]), [inspector]))";
/// The window's background, which the 3D view's surroundings are cleared to.
const BACKGROUND: u32 = 0x0c0e10;
/// How often the status bar's numbers change.
const STATUS_INTERVAL: f32 = 0.5;
/// The axis gizmo: its middle, how far its axes reach, and their ends' size.
const GIZMO_MIDDLE: f32 = 15.0;
const GIZMO_REACH: f32 = 11.0;
const GIZMO_END: f32 = 2.5;

/// Elements the editor reads and writes, found by name in Editor.vui.
struct Nodes {
    viewport: NodeId,
    frame: NodeId,
    command: NodeId,
    gizmo: NodeId,
    stats: NodeId,
    status_time: NodeId,
    status_fps: NodeId,
    status_memory: NodeId,
    buttons: [(NodeId, &'static str); 4],
}

pub struct Editor {
    pub game: Game,
    pub ui: Ui,
    pub fly: FlyCamera,
    nodes: Nodes,
    hierarchy: Hierarchy,
    project: ProjectView,
    inspector: Inspector,
    console: Console,
    /// The Scene view's own camera, which the Hierarchy leaves out.
    camera: Entity,
    /// The dock area showing the viewport, made see-through, and where the
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
    status: (u32, f32),
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
            viewport: find(&ui, "viewport"),
            frame: find(&ui, "viewport-frame"),
            command: find(&ui, "command"),
            gizmo: find(&ui, "gizmo"),
            stats: find(&ui, "scene-stats"),
            status_time: find(&ui, "status-time"),
            status_fps: find(&ui, "status-fps"),
            status_memory: find(&ui, "status-memory"),
            buttons: [
                (find(&ui, "play"), "Play"),
                (find(&ui, "pause"), "Pause"),
                (find(&ui, "step"), "Step"),
                (find(&ui, "build"), "Build"),
            ],
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
        let dock = find(&ui, "dock");
        ui.set_dock_layout(dock, LAYOUT).expect("the default layout is valid");
        let (tree, filter) = (find(&ui, "hierarchy-tree"), find(&ui, "hierarchy-filter"));
        let hierarchy = Hierarchy::new(&mut ui, tree, filter);
        let project = ProjectView::new(&mut ui, document, game.project.name(), &root, game.content.db());
        let console_list = find(&ui, "console-list");
        let console = Console::new(&mut ui, console_list, log);
        let mut inspector = Inspector::new(find(&ui, "inspector"));
        inspector.show(&mut ui, &game.engine.world, None);
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
            camera,
            viewport_area: None,
            framed: None,
            gizmo_shows: None,
            pointer: None,
            scale: 1.0,
            control: false,
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
        self.project.update(&mut self.ui);
        self.console.update(&mut self.ui);
        for (button, name) in self.nodes.buttons {
            if self.ui.clicked(button) {
                tracing::info!("{name} comes with play mode, the next step of the editor");
            }
        }
        self.mark_viewport_area();
        self.update_gizmo();
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
    /// Scene panel has been docked, and lays the frame over its edge, which
    /// rounds the 3D view's corners to the panel's.
    fn mark_viewport_area(&mut self) {
        let mut at = self.ui.parent(self.nodes.viewport);
        let mut area = None;
        while let Some(node) = at.filter(|_| self.viewport_shown()) {
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
