//! Several Scene views, as Unity has: the first, docked from the start, and
//! any more opened from a window's menu (Add Scene View), each a panel of
//! its own beside the one it was opened from. Each looks through a camera of
//! its own, flown on its own, and has its own header: a shading menu (Lit,
//! Unlit, Wireframe, or Lit Wireframe, the lit scene with its meshes' edges
//! over it), the Gizmos button, which shows or hides lights, cameras and
//! what the selected one reaches (see `gizmos`), and full screen, which
//! gives it the dock alone until pressed again. The first one's shading and
//! Gizmos are settings, so they are kept; the tool strip's settings are
//! shared by all. The tools act in the Scene view the pointer is over: while
//! it is, that one is the editor's `scene`, so flying, the handles, a click
//! to select and a drop all go to it. One opened from the menu goes for good
//! when it closes, its camera with it. Each is built from SceneView.vui.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::FlyCamera;
use voxear_render::{
    Camera, DEBUG_VIEW_LIT_WIREFRAME, DEBUG_VIEW_NONE, DEBUG_VIEW_UNLIT, DEBUG_VIEW_WIREFRAME, Renderer,
};
use voxear_scene::{Name, Transform};
use voxear_ui::{Color, NodeId, Shape, Ui};

use super::{Editor, GIZMO_END, GIZMO_MIDDLE, GIZMO_REACH};
use crate::{gizmos, hierarchy::EditorOnly, scene_tools::ViewCamera};

/// What every Scene view is built from.
const MARKUP: &str = include_str!("../../ui/SceneView.vui");
/// The first Scene view's panel, docked from the start.
pub(super) const FIRST: &str = "scene";

/// A Scene view: its panel, its elements, the camera it looks through and
/// how that flies, and how it draws.
pub(super) struct SceneView {
    /// Its panel's id: `scene` for the first, `scene-2` and on for more.
    pub(super) panel: String,
    pub(super) viewport: NodeId,
    /// The layer the gizmos and handles are drawn on.
    pub(super) handles: NodeId,
    pub(super) tool_strip: NodeId,
    pub(super) tool: NodeId,
    pub(super) pivot: NodeId,
    pub(super) space: NodeId,
    pub(super) snap: NodeId,
    pub(super) stats: NodeId,
    /// The axis gizmo's box, and the element its axes are drawn on.
    pub(super) view_gizmo: NodeId,
    pub(super) gizmo: NodeId,
    /// The header's shading menu, Gizmos button and full screen button.
    pub(super) shading_menu: NodeId,
    pub(super) gizmos_button: NodeId,
    pub(super) maximize: NodeId,
    /// The bar naming a prefab open on its own, with Back and Close Prefab.
    pub(super) prefab_bar: NodeId,
    pub(super) prefab_name: NodeId,
    pub(super) prefab_back: NodeId,
    pub(super) prefab_close: NodeId,
    /// The camera it looks through, which the Hierarchy leaves out.
    pub(super) camera: Entity,
    pub(super) fly: FlyCamera,
    /// The shading menu's choice: Lit, Unlit, Wireframe or Lit Wireframe.
    pub(super) shading: usize,
    pub(super) show_gizmos: bool,
    /// The camera's rotation as the axis gizmo last showed it.
    pub(super) gizmo_shows: Option<Quat>,
    /// While the game plays, its camera in the level as edited, which is put
    /// where this one flew to when play stops.
    pub(super) edit_camera: Option<Entity>,
}

impl SceneView {
    /// Builds a Scene view in its panel's `content`, looking through `camera`.
    pub(super) fn build(ui: &mut Ui, content: NodeId, panel: &str, camera: Entity) -> Self {
        let mut load = |path: &str| Err(format!("{path} is not built in"));
        let root =
            ui.load_markup(content, "SceneView.vui", MARKUP, &mut load).expect("the Scene view's markup is built in");
        let find = |ui: &Ui, name: &str| ui.find(root, name).unwrap_or_else(|| panic!("SceneView.vui has no {name}"));
        let view = Self {
            panel: panel.to_string(),
            viewport: find(ui, "viewport"),
            handles: find(ui, "handles"),
            tool_strip: find(ui, "tool-strip"),
            tool: find(ui, "tool"),
            pivot: find(ui, "pivot"),
            space: find(ui, "space"),
            snap: find(ui, "snap"),
            stats: find(ui, "scene-stats"),
            view_gizmo: find(ui, "view-gizmo"),
            gizmo: find(ui, "gizmo"),
            shading_menu: find(ui, "shading"),
            gizmos_button: find(ui, "gizmos"),
            maximize: find(ui, "scene-maximize"),
            prefab_bar: find(ui, "prefab-bar"),
            prefab_name: find(ui, "prefab-name"),
            prefab_back: find(ui, "prefab-back"),
            prefab_close: find(ui, "prefab-close"),
            camera,
            fly: FlyCamera::default(),
            shading: 0,
            show_gizmos: true,
            gizmo_shows: None,
            edit_camera: None,
        };
        ui.set_visible(view.prefab_bar, false);
        view
    }

    /// The renderer's debug view its shading asks for.
    pub(super) fn debug_view(&self) -> u32 {
        match self.shading {
            1 => DEBUG_VIEW_UNLIT,
            2 => DEBUG_VIEW_WIREFRAME,
            3 => DEBUG_VIEW_LIT_WIREFRAME,
            _ => DEBUG_VIEW_NONE,
        }
    }

    /// Its place among the Scene views: 1 for the first, then as numbered.
    fn number(&self) -> u32 {
        self.panel.strip_prefix("scene-").and_then(|n| n.parse().ok()).unwrap_or(1)
    }
}

impl Editor {
    /// Opens another Scene view beside the panel `beside`, looking from where
    /// the Scene view in use looks, and shaded as it is. Returns its panel.
    pub fn add_scene_view(&mut self, beside: &str) -> String {
        let taken = |n: u32| self.all_scene_views().any(|view| view.number() == n);
        let number = (2..).find(|&n| !taken(n)).expect("there is always a free number");
        let panel = format!("scene-{number}");
        let content = self.ui.add_dock_panel_beside(self.nodes.dock, &panel, "Scene", beside);
        self.ui.set_dock_panel_icon(self.nodes.dock, &panel, "cube");
        let world = &mut self.game.engine.world;
        let place = world.get::<Transform>(self.scene.camera).copied().unwrap_or(Transform::IDENTITY);
        let lens = world.get::<Camera>(self.scene.camera).copied().unwrap_or_default();
        let camera = world.spawn((Name("Scene Camera".into()), place, lens, EditorOnly));
        let mut view = SceneView::build(&mut self.ui, content, &panel, camera);
        view.fly.set_speed(self.scene.fly.speed());
        (view.shading, view.show_gizmos) = (self.scene.shading, self.scene.show_gizmos);
        if let Some(open) = self.open_prefab_path() {
            let name = open.rsplit('/').next().unwrap_or(open).to_string();
            self.ui.set_text(view.prefab_name, &name);
            self.ui.set_visible(view.prefab_bar, true);
        }
        self.scene_views.push(view);
        self.apply_settings();
        panel
    }

    /// Takes a Scene view opened from the menu away for good, as closing it
    /// does, with its camera. False for a panel that is not one; the first
    /// Scene view only closes, to open again.
    pub(super) fn remove_scene_view(&mut self, panel: &str) -> bool {
        if panel == FIRST || !self.all_scene_views().any(|view| view.panel == panel) {
            return false;
        }
        // The one in use goes to the first, so there is always one in use.
        if self.scene.panel == panel {
            let first = self.scene_views.iter().position(|view| view.panel == FIRST).expect("the first is kept");
            self.use_scene_view(first);
        }
        let index = self.scene_views.iter().position(|view| view.panel == panel).expect("it is one of the others");
        let view = self.scene_views.remove(index);
        self.game.engine.world.despawn(view.camera);
        if let Some(edit_camera) = view.edit_camera {
            self.despawn_in_edit_world(edit_camera);
        }
        self.ui.remove_dock_panel(self.nodes.dock, panel);
        true
    }

    /// How many Scene views there are, the first included.
    pub fn scene_view_count(&self) -> usize {
        1 + self.scene_views.len()
    }

    /// The panel of the Scene view in use, which the tools act in.
    pub fn scene_view_in_use(&self) -> &str {
        &self.scene.panel
    }

    /// The camera each Scene view looks through, by its panel, the first first.
    pub fn scene_view_cameras(&self) -> Vec<(String, Entity)> {
        let mut views: Vec<&SceneView> = self.all_scene_views().collect();
        views.sort_by_key(|view| view.number());
        views.into_iter().map(|view| (view.panel.clone(), view.camera)).collect()
    }

    /// The fly camera of the Scene view in use.
    pub fn fly(&self) -> &FlyCamera {
        &self.scene.fly
    }

    /// The fly camera of the Scene view in use, to fly it.
    pub fn fly_mut(&mut self) -> &mut FlyCamera {
        &mut self.scene.fly
    }

    /// Every Scene view, the one in use first.
    pub(super) fn all_scene_views(&self) -> impl Iterator<Item = &SceneView> {
        std::iter::once(&self.scene).chain(&self.scene_views)
    }

    pub(super) fn all_scene_views_mut(&mut self) -> impl Iterator<Item = &mut SceneView> {
        std::iter::once(&mut self.scene).chain(&mut self.scene_views)
    }

    /// The Scene views that show, by their number, with the cameras they
    /// look through.
    pub(super) fn shown_scene_views(&self) -> Vec<(NodeId, Entity)> {
        let mut views: Vec<&SceneView> = self.all_scene_views().filter(|view| self.shown(view.viewport)).collect();
        views.sort_by_key(|view| view.number());
        views.into_iter().map(|view| (view.viewport, view.camera)).collect()
    }

    /// The Scene view drawing into `viewport`, if one is.
    pub(super) fn scene_view_at(&self, viewport: NodeId) -> Option<&SceneView> {
        self.all_scene_views().find(|view| view.viewport == viewport)
    }

    /// Puts the Scene view under the pointer in use, unless the one in use is
    /// being flown or a handle in it dragged.
    pub(super) fn follow_pointer(&mut self) {
        if self.scene.fly.is_looking() || self.tools.is_dragging() || self.pressed_handle.is_some() {
            return;
        }
        let hovered = self.scene_views.iter().position(|view| self.ui.is_hovered(view.viewport));
        if let Some(index) = hovered {
            self.use_scene_view(index);
        }
    }

    /// Swaps one of the other Scene views in as the one in use.
    fn use_scene_view(&mut self, index: usize) {
        std::mem::swap(&mut self.scene, &mut self.scene_views[index]);
        self.hovered_handle = None;
        self.tools.hover(None);
    }

    /// Every Scene view's header: its shading menu and Gizmos button change
    /// how it draws, and full screen gives it the dock. The first one's
    /// shading and Gizmos are settings, so they are kept. Returns whether a
    /// setting changed.
    pub(super) fn update_scene_headers(&mut self) -> bool {
        let (ui, settings) = (&mut self.ui, &mut self.memory.settings);
        let mut changed = false;
        let mut maximize = None;
        for view in std::iter::once(&mut self.scene).chain(&mut self.scene_views) {
            let first = view.panel == FIRST;
            if ui.changed(view.shading_menu) {
                view.shading = ui.selected_option(view.shading_menu);
                if first {
                    settings.scene_shading = view.shading;
                    changed = true;
                }
            }
            if ui.clicked(view.gizmos_button) {
                view.show_gizmos = !view.show_gizmos;
                if first {
                    settings.show_gizmos = view.show_gizmos;
                    changed = true;
                }
            }
            if ui.clicked(view.maximize) {
                maximize = Some(view.panel.clone());
            }
        }
        if let Some(panel) = maximize {
            self.toggle_maximized(&panel);
        }
        let maximized = self.maximized.is_some();
        for view in std::iter::once(&self.scene).chain(&self.scene_views) {
            self.ui.set_checked(view.maximize, maximized);
        }
        changed
    }

    /// Puts the settings into every Scene view: the first one's shading and
    /// Gizmos, the tool strip's labels and tool, which overlays show, and the
    /// speed the one in use flies at. Only the first shows the stats, which
    /// are of the first view drawn.
    pub(super) fn apply_scene_settings(&mut self) {
        let (ui, settings, tools) = (&mut self.ui, &self.memory.settings, &self.tools);
        let (tool, speed) = (ui.segment(self.scene.tool), self.scene.fly.speed());
        for view in std::iter::once(&mut self.scene).chain(&mut self.scene_views) {
            let first = view.panel == FIRST;
            if first {
                (view.shading, view.show_gizmos) = (settings.scene_shading, settings.show_gizmos);
            }
            ui.set_text(view.pivot, if tools.centred { "Center" } else { "Pivot" });
            ui.set_text(view.space, if tools.local { "Local" } else { "Global" });
            if let Some(&label) = ui.children(view.snap).last() {
                ui.set_text(label, &format!("{} m", tools.snap));
            }
            if ui.segment(view.tool) != tool {
                ui.set_segment(view.tool, tool);
            }
            ui.set_visible(view.stats, first && settings.show_stats);
            ui.set_visible(view.view_gizmo, settings.show_axis_gizmo);
            ui.set_checked(view.gizmos_button, view.show_gizmos);
            if ui.selected_option(view.shading_menu) != view.shading {
                ui.set_selected_option(view.shading_menu, view.shading);
            }
            view.fly.set_speed(speed);
        }
    }

    /// The gizmos and the selection's handles in the Scene views not in use,
    /// which only show them; the one in use draws its own as it takes the
    /// pointer.
    pub(super) fn draw_other_scene_views(&mut self, renderer: &Renderer) {
        let (ui, world, tools) = (&mut self.ui, &self.game.engine.world, &self.tools);
        let selected = self.selection;
        for view in &self.scene_views {
            let shown = std::iter::successors(Some(view.viewport), |&n| ui.parent(n)).all(|n| ui.is_visible(n));
            let camera = shown.then(|| ViewCamera::new(world, view.camera, ui.rect(view.viewport))).flatten();
            let Some(camera) = camera else {
                ui.set_shapes(view.handles, Vec::new());
                continue;
            };
            let mut shapes = if view.show_gizmos { gizmos::shapes(world, &camera, selected) } else { Vec::new() };
            shapes.extend(tools.shapes(world, renderer, &camera, selected));
            ui.set_shapes(view.handles, shapes);
        }
    }

    /// Turns each Scene view's axis gizmo with its camera: each world axis
    /// drawn where the camera sees it, the ones nearer the viewer on top,
    /// and a faint dot where each points away.
    pub(super) fn update_axis_gizmos(&mut self) {
        let (ui, world) = (&mut self.ui, &self.game.engine.world);
        for view in std::iter::once(&mut self.scene).chain(&mut self.scene_views) {
            let Some(rotation) = world.get::<Transform>(view.camera).map(|t| t.rotation) else { continue };
            if view.gizmo_shows != Some(rotation) {
                view.gizmo_shows = Some(rotation);
                ui.set_shapes(view.gizmo, axis_shapes(rotation));
            }
        }
    }

    /// Gives each Scene view a camera in the world about to play, where its
    /// own is, and keeps the one in the level as edited for when play stops.
    pub(super) fn scene_cameras_into(&mut self, played: &mut World) {
        let world = &self.game.engine.world;
        for view in std::iter::once(&mut self.scene).chain(&mut self.scene_views) {
            let place = world.get::<Transform>(view.camera).copied().unwrap_or(Transform::IDENTITY);
            let lens = world.get::<Camera>(view.camera).copied().unwrap_or_default();
            let camera = played.spawn((Name("Scene Camera".into()), place, lens, EditorOnly));
            view.edit_camera = Some(std::mem::replace(&mut view.camera, camera));
        }
    }

    /// Where each Scene view's camera is in the world playing, and its lens,
    /// in the order `scene_cameras_back` takes them.
    pub(super) fn scene_cameras_flown(&self) -> Vec<(Transform, Camera)> {
        let world = &self.game.engine.world;
        let flown = |view: &SceneView| {
            let place = world.get::<Transform>(view.camera).copied().unwrap_or(Transform::IDENTITY);
            (place, world.get::<Camera>(view.camera).copied().unwrap_or_default())
        };
        self.all_scene_views().map(flown).collect()
    }

    /// After play stops: each Scene view looks through its camera in the
    /// level as edited again, put where it flew to while playing. One opened
    /// while playing gets a camera there.
    pub(super) fn scene_cameras_back(&mut self, flown: Vec<(Transform, Camera)>) {
        let world = &mut self.game.engine.world;
        for (view, (place, lens)) in std::iter::once(&mut self.scene).chain(&mut self.scene_views).zip(flown) {
            view.camera = match view.edit_camera.take() {
                Some(camera) => {
                    if let Some(transform) = world.get_mut::<Transform>(camera) {
                        *transform = place;
                    }
                    camera
                }
                None => world.spawn((Name("Scene Camera".into()), place, lens, EditorOnly)),
            };
        }
    }

    /// Shows the bar naming a prefab open on its own over every Scene view,
    /// or hides it.
    pub(super) fn show_prefab_bars(&mut self, prefab: Option<&str>) {
        let name = prefab.map(|path| path.rsplit('/').next().unwrap_or(path));
        for view in std::iter::once(&self.scene).chain(&self.scene_views) {
            if let Some(name) = name {
                self.ui.set_text(view.prefab_name, name);
            }
            self.ui.set_visible(view.prefab_bar, name.is_some());
        }
    }

    /// Whether Back or Close Prefab was clicked on any Scene view's bar.
    pub(super) fn prefab_bar_clicked(&self) -> bool {
        self.all_scene_views().any(|view| self.ui.clicked(view.prefab_back) || self.ui.clicked(view.prefab_close))
    }
}

/// The axis gizmo's shapes for a camera turned by `rotation`.
fn axis_shapes(rotation: Quat) -> Vec<Shape> {
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
    shapes
}
