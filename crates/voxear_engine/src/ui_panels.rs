//! UI Panels: `.vui` documents shown on rectangles in the world, which
//! players read and click by pointing at them (see
//! [`voxear_render::UiPanel`]).
//!
//! Each panel's document is a Voxear UI of its own ([`PanelUi`], held by
//! [`GameUi`]), laid out at the panel's size in metres times its pixels per
//! metre, so its layout never changes with distance. It is drawn at the size
//! it showed on screen in the last frame, in steps of √2 so its text is not
//! drawn afresh as the camera moves, and the renderer mipmaps it.
//!
//! The pointer works the nearest clickable panel a ray from the camera meets
//! in front of anything solid: through the cursor, or through the middle of
//! the view while the cursor is locked, where a first-person game's
//! crosshair is ([`GameUi::point_panels`]). A click that lands on a panel
//! goes to it rather than to the game ([`GameUi::panel_input`]), and its
//! release goes to the panel that took the press. Keys go to a panel while
//! one of its text fields has focus.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{Renderer, UiPanel, UiPanelFrame};
use voxear_ui::{Ui, UiFrame};
use winit::event::{ElementState, WindowEvent};

use crate::{
    Content,
    game_ui::{GameUi, Surface},
};

/// The number scripts hold a UI Panel's document by, on its Obj while it
/// shows one, for `UiPanel.Document` in scripts to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelDocument(pub u32);

impl voxear_ecs::Component for PanelDocument {}

/// The largest a panel's texture is on a side, in pixels.
const MAX_PANEL_PIXELS: f32 = 2048.0;
/// The smallest a panel is drawn, as a step: an eighth of its layout's size.
const SMALLEST_STEP: i32 = -6;

/// A UI Panel as scenes write it. Left out, a setting has its default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiPanelData {
    /// The document, by GUID or project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    #[serde(default = "default_size", skip_serializing_if = "is_default_size")]
    pub size: [f32; 2],
    #[serde(default = "default_pixels", skip_serializing_if = "is_default_pixels")]
    pub pixels_per_metre: f32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub face_camera: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub clickable: bool,
}

fn default_size() -> [f32; 2] {
    UiPanel::default().size
}

fn is_default_size(size: &[f32; 2]) -> bool {
    *size == default_size()
}

fn default_pixels() -> f32 {
    UiPanel::default().pixels_per_metre
}

fn is_default_pixels(pixels: &f32) -> bool {
    *pixels == default_pixels()
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Default for UiPanelData {
    fn default() -> Self {
        toml::from_str("").expect("every setting has a default")
    }
}

impl UiPanelData {
    /// The settings that differ from `expected`, each written out even at
    /// its default, as a prefab instance's override merges them onto the
    /// prefab's.
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        let table = |data: &Self| {
            let mut table = toml::Table::new();
            let mut put = |key: &str, value: toml::Value| drop(table.insert(key.to_string(), value));
            if let Some(document) = &data.document {
                put("document", document.clone().into());
            }
            put("size", toml::Value::try_from(data.size).expect("two numbers"));
            put("pixels_per_metre", f64::from(data.pixels_per_metre).into());
            put("face_camera", data.face_camera.into());
            put("clickable", data.clickable.into());
            table
        };
        let mut changes = table(self);
        if let Some(expected) = expected.map(table) {
            changes.retain(|key, value| expected.get(key) != Some(value));
        }
        changes
    }
}

/// A UI Panel from a scene's data, its document found by GUID or path.
pub fn ui_panel(content: &Content, data: &UiPanelData) -> Result<UiPanel> {
    let document = match &data.document {
        Some(reference) => {
            let found = content.parse(reference)?;
            Some(found.guid().with_context(|| format!("`{reference}` is not a UI document"))?)
        }
        None => None,
    };
    Ok(UiPanel {
        document,
        size: data.size.map(|side| side.max(0.01)),
        pixels_per_metre: data.pixels_per_metre.max(1.0),
        face_camera: data.face_camera,
        clickable: data.clickable,
    })
}

/// What a scene writes for a UI Panel.
pub fn ui_panel_data(panel: &UiPanel) -> UiPanelData {
    UiPanelData {
        document: panel.document.map(|guid| guid.to_string()),
        size: panel.size,
        pixels_per_metre: panel.pixels_per_metre,
        face_camera: panel.face_camera,
        clickable: panel.clickable,
    }
}

/// How large to draw a panel laid out `layout` pixels in size, as a step:
/// `2^(step / 2)` times its layout. It grows as soon as it shows larger
/// than it is drawn (`shown`, in pixels, last frame), and shrinks only once
/// it shows two steps smaller, so it is not drawn afresh at every step as
/// the camera moves to and fro. Never larger than [`MAX_PANEL_PIXELS`].
pub fn drawing_step(current: i32, shown: Option<[f32; 2]>, layout: [f32; 2]) -> i32 {
    let longest = layout[0].max(layout[1]).max(1.0);
    let largest = ((MAX_PANEL_PIXELS / longest).log2() * 2.0).floor() as i32;
    let Some(shown) = shown.filter(|size| size[0] > 0.0 && size[1] > 0.0) else {
        return current.clamp(SMALLEST_STEP, largest.max(SMALLEST_STEP));
    };
    let wanted = (shown[0] / layout[0].max(1.0)).max(shown[1] / layout[1].max(1.0)).min(MAX_PANEL_PIXELS);
    let step = (wanted.max(1e-3).log2() * 2.0).ceil() as i32;
    let step = match step {
        step if step > current => step,
        step if step < current - 1 => step + 1,
        _ => current,
    };
    step.clamp(SMALLEST_STEP, largest.max(SMALLEST_STEP))
}

/// The size of a panel's texture laid out `layout` pixels in size and drawn
/// at `step`, in pixels.
fn drawn_size(layout: [f32; 2], step: i32) -> [u32; 2] {
    let scale = 2f32.powf(step as f32 / 2.0);
    layout.map(|side| (side * scale).ceil().clamp(1.0, MAX_PANEL_PIXELS) as u32)
}

/// A UI Panel's UI: its Obj, its numbers' first, the document it shows and
/// the number scripts hold that by, and how large it is drawn.
pub(crate) struct PanelUi {
    pub entity: Entity,
    pub first: u32,
    pub surface: Surface,
    /// The document it shows, by GUID, and by the number scripts hold it by.
    shown: Option<AssetGuid>,
    document: Option<u32>,
    /// A document that would not load, not tried again until it changes.
    failed: Option<AssetGuid>,
    layout: [f32; 2],
    step: i32,
    /// The size it was last drawn at, in pixels, which its pointer is in.
    pub size: [u32; 2],
}

impl GameUi {
    /// Keeps a UI for each UI Panel in `world`, its document loaded: makes
    /// one for each new panel, loads its document again when that changes,
    /// and lets go of those whose panels went. How large each is drawn next
    /// follows how large it showed last frame, `shown` in pixels.
    pub fn sync_panels(&mut self, world: &mut World, content: &mut Content, shown: &HashMap<Entity, [f32; 2]>) {
        let found = voxear_render::ui_panel_rects(world, Vec3::ZERO);
        self.panels.retain(|panel| found.iter().any(|(entity, ..)| *entity == panel.entity));
        for (entity, _, panel) in found {
            if !self.panels.iter().any(|kept| kept.entity == entity) {
                let (surface, first) = self.new_surface();
                let made = PanelUi {
                    entity,
                    first,
                    surface,
                    shown: None,
                    document: None,
                    failed: None,
                    layout: panel.layout_size(),
                    step: 0,
                    size: [0, 0],
                };
                self.panels.push(made);
            }
            let kept = self.panels.iter_mut().find(|kept| kept.entity == entity).expect("made above");
            kept.layout = panel.layout_size();
            kept.step = drawing_step(kept.step, shown.get(&entity).copied(), kept.layout);
            if kept.shown != panel.document && kept.failed != panel.document {
                kept.show(content, panel.document);
            }
            let document = kept.document.map(PanelDocument);
            if world.get::<PanelDocument>(entity).copied() != document {
                match document {
                    Some(document) => drop(world.insert(entity, document)),
                    None => drop(world.remove::<PanelDocument>(entity)),
                }
            }
        }
        if self.hovered.is_some_and(|hovered| !self.panels.iter().any(|panel| panel.entity == hovered)) {
            self.hovered = None;
        }
    }

    /// Loads the document each panel shows again, as after its file or a
    /// sheet it names changed.
    pub fn reload_panels(&mut self, content: &mut Content) {
        for panel in &mut self.panels {
            let document = panel.shown.take();
            panel.failed = None;
            panel.show(content, document);
        }
    }

    /// Lays each panel's UI out at the size it is drawn, its clock moved on
    /// by `delta` seconds: each panel's Obj, the size in pixels, and what it
    /// draws.
    pub fn lay_out_panels(&mut self, delta: f32) -> Vec<(Entity, [u32; 2], UiFrame)> {
        let mut frames = Vec::with_capacity(self.panels.len());
        for panel in &mut self.panels {
            let size = drawn_size(panel.layout, panel.step);
            let scale = size[0] as f32 / panel.layout[0].max(1.0);
            panel.size = size;
            frames.push((panel.entity, size, panel.surface.frame(delta, (size[0], size[1]), scale)));
        }
        frames
    }

    /// Lays each panel's UI out (see [`lay_out_panels`](Self::lay_out_panels))
    /// and hands the renderer them all.
    pub fn draw_panels(&mut self, renderer: &mut Renderer, delta: f32) -> Result<()> {
        let frames = self.lay_out_panels(delta);
        let frames: Vec<UiPanelFrame<'_>> =
            frames.iter().map(|(entity, size, frame)| UiPanelFrame { entity: *entity, frame, size: *size }).collect();
        renderer.set_ui_panels(&frames)
    }

    /// The number scripts hold a panel's document by, once it shows one.
    pub fn panel_document(&self, entity: Entity) -> Option<u32> {
        self.panels.iter().find(|panel| panel.entity == entity)?.document
    }

    /// A panel's UI, as laid out last.
    pub fn panel_ui(&mut self, entity: Entity) -> Option<&mut Ui> {
        self.panels.iter_mut().find(|panel| panel.entity == entity).map(|panel| &mut panel.surface.ui)
    }

    /// The panel the pointer is over, if any.
    pub fn hovered_panel(&self) -> Option<Entity> {
        self.hovered
    }

    /// Points at the panels with `ray`, from the camera along a direction a
    /// metre long, or none when the pointer is off the view. The nearest
    /// clickable panel it meets takes the pointer where it meets it, unless
    /// `solid` finds something nearer in the way (how far along the ray, in
    /// metres); every other panel's pointer leaves.
    pub fn point_panels(&mut self, world: &World, ray: Option<(Vec3, Vec3)>, solid: impl Fn(Vec3, Vec3) -> Option<f32>) {
        let hit = ray.and_then(|(from, direction)| {
            let rects = voxear_render::ui_panel_rects(world, from);
            let clickable = rects.into_iter().filter(|(_, _, panel)| panel.clickable);
            let hits = clickable.filter_map(|(entity, rect, _)| Some((entity, rect.hit(from, direction)?)));
            let (entity, (along, at)) = hits.min_by(|a, b| a.1.0.total_cmp(&b.1.0))?;
            let blocked = solid(from, direction).is_some_and(|distance| distance < along - 1e-3);
            (!blocked).then_some((entity, at))
        });
        let hovered = hit.map(|(entity, _)| entity);
        for panel in &mut self.panels {
            if self.hovered == Some(panel.entity) && hovered != Some(panel.entity) {
                panel.surface.ui.pointer_leave();
            }
        }
        if let Some((entity, [u, v])) = hit
            && let Some(panel) = self.panels.iter_mut().find(|panel| panel.entity == entity && panel.size[0] > 0)
        {
            panel.surface.ui.pointer_move(u * panel.size[0] as f32, v * panel.size[1] as f32);
        }
        self.hovered = hovered;
    }

    /// Hands a window event to the panels: a click or the wheel to the panel
    /// the pointer is over, a press's release to the panel that took it, and
    /// keys to a panel whose text field has focus. Returns whether a panel
    /// used it, so the game can leave it alone. The pointer's moves are not
    /// theirs: [`point_panels`](Self::point_panels) places it.
    pub fn panel_input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                let taken = self.hovered.and_then(|entity| self.panel(entity)).is_some_and(|ui| {
                    crate::ui_input::handle(ui, event)
                });
                self.pressed = if taken { self.hovered } else { None };
                // Pressing anywhere else takes the keyboard back from a panel's text field.
                for panel in self.panels.iter_mut().filter(|panel| !taken || Some(panel.entity) != self.hovered) {
                    panel.surface.ui.blur();
                }
                taken
            }
            WindowEvent::MouseInput { state: ElementState::Released, .. } => {
                let Some(entity) = self.pressed.take() else { return false };
                if let Some(ui) = self.panel(entity) {
                    crate::ui_input::handle(ui, event);
                }
                true
            }
            WindowEvent::MouseWheel { .. } => {
                self.hovered.and_then(|entity| self.panel(entity)).is_some_and(|ui| crate::ui_input::handle(ui, event))
            }
            WindowEvent::ModifiersChanged(_) => {
                for panel in &mut self.panels {
                    crate::ui_input::handle(&mut panel.surface.ui, event);
                }
                false
            }
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) => {
                let typing = self.panels.iter_mut().find(|panel| panel.surface.ui.wants_keyboard());
                typing.is_some_and(|panel| crate::ui_input::handle(&mut panel.surface.ui, event))
            }
            _ => false,
        }
    }

    fn panel(&mut self, entity: Entity) -> Option<&mut Ui> {
        self.panel_ui(entity)
    }
}

impl crate::Game {
    /// Points at the UI Panels from the camera and hands the renderer their
    /// UIs, each drawn at the size it showed last frame. `view` is where the
    /// game is drawn (left, top, width and height, in pixels) through
    /// `camera`, or the first in the world, and `pointer` where the pointer
    /// is, or none where it points at no panel, as while editing. While the
    /// cursor is locked, the middle of the view points, where a
    /// first-person game's crosshair is.
    pub fn show_panels(
        &mut self,
        renderer: &mut Renderer,
        (view, camera): ([f32; 4], Option<Entity>),
        pointer: Option<[f32; 2]>,
        delta: f32,
    ) -> Result<()> {
        let world = &self.engine.world;
        let locked = self.display.cursor_lock == voxear_scripting::CursorLock::Locked;
        let middle = [view[0] + view[2] * 0.5, view[1] + view[3] * 0.5];
        let pointer = pointer.map(|at| if locked { middle } else { at });
        let ray = pointer.and_then(|at| crate::debug_texts::camera_ray(world, camera, view, at));
        let physics = world.resource::<voxear_physics::scene::PhysicsScene>();
        let solid = |from: Vec3, along: Vec3| physics?.raycast(from, along, 1000.0, u32::MAX, false).map(|hit| hit.distance);
        self.ui.point_panels(world, ray, solid);
        let shown = renderer.ui_panel_sizes().clone();
        self.ui.sync_panels(&mut self.engine.world, &mut self.content, &shown);
        self.ui.draw_panels(renderer, delta)
    }
}

impl PanelUi {
    /// Shows `document` in place of what it showed, or nothing.
    fn show(&mut self, content: &mut Content, document: Option<AssetGuid>) {
        if let Some(number) = self.document.take() {
            self.surface.close(number);
        }
        self.shown = document;
        self.failed = None;
        let Some(guid) = document else { return };
        let path = content.db().get(guid).map(|entry| entry.path.clone());
        let loaded = path.with_context(|| format!("no asset has the GUID {guid}")).and_then(|path| {
            self.surface.load(content, &path).with_context(|| format!("showing {path} on a UI Panel"))
        });
        match loaded {
            Ok(number) => self.document = Some(number),
            Err(error) => {
                tracing::warn!("{error:#}");
                self.shown = None;
                self.failed = Some(guid);
            }
        }
    }
}

/// Sets what answers `UiPanel` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::UiPanel, values);
}

// What `values` reads or writes, as `UiPanel` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const SIZE: i32 = 3;
const PIXELS_PER_METRE: i32 = 4;
const FACE_CAMERA: i32 = 5;
const CLICKABLE: i32 = 6;
const DOCUMENT: i32 = 7;

/// Reads value `what` of `entity`'s UI Panel, or with `written`, writes it:
/// `None` when it has none, or is gone. Its document reads as the number
/// scripts hold it by, bit for bit, and whether it shows one.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    let flag = |on: bool| number(f32::from(u8::from(on)));
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<UiPanel>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<UiPanel>(entity) {
                world.insert(entity, UiPanel::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<PanelDocument>(entity);
            return world.remove::<UiPanel>(entity).map(|_| [0.0; 4]);
        }
        (DOCUMENT, None) => {
            world.get::<UiPanel>(entity)?;
            let document = world.get::<PanelDocument>(entity).map(|document| document.0);
            return Some([f32::from_bits(document.unwrap_or(0)), f32::from(u8::from(document.is_some())), 0.0, 0.0]);
        }
        _ => {}
    }
    let panel = world.get_mut::<UiPanel>(entity)?;
    match (what, written) {
        (SIZE, None) => Some([panel.size[0], panel.size[1], 0.0, 0.0]),
        (PIXELS_PER_METRE, None) => Some(number(panel.pixels_per_metre)),
        (FACE_CAMERA, None) => Some(flag(panel.face_camera)),
        (CLICKABLE, None) => Some(flag(panel.clickable)),
        (SIZE, Some([x, y, ..])) => {
            panel.size = [x.max(0.01), y.max(0.01)];
            Some([0.0; 4])
        }
        (PIXELS_PER_METRE, Some([x, ..])) => {
            panel.pixels_per_metre = x.clamp(1.0, 10_000.0);
            Some([0.0; 4])
        }
        (FACE_CAMERA, Some([x, ..])) => {
            panel.face_camera = x != 0.0;
            Some([0.0; 4])
        }
        (CLICKABLE, Some([x, ..])) => {
            panel.clickable = x != 0.0;
            Some([0.0; 4])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_grows_at_once_and_shrinks_only_two_steps_down() {
        let layout = [800.0, 450.0];
        assert_eq!(drawing_step(0, None, layout), 0, "unseen, it stays as it is");
        assert_eq!(drawing_step(0, Some([1500.0, 840.0]), layout), 2, "shown nearly twice as big: twice");
        assert_eq!(drawing_step(2, Some([1000.0, 560.0]), layout), 2, "a step smaller: kept");
        assert_eq!(drawing_step(2, Some([500.0, 280.0]), layout), 0, "three steps smaller: one above that");
        assert_eq!(drawing_step(0, Some([f32::MAX; 2]), layout), 2, "at the eye: no larger than it may be");
        assert_eq!(drawn_size(layout, 2), [1600, 900]);
        assert_eq!(drawn_size(layout, -6), [100, 57]);
        assert_eq!(drawing_step(0, Some([1.0, 1.0]), layout), SMALLEST_STEP);
    }

    #[test]
    fn a_panel_writes_only_what_differs_from_its_defaults() {
        let data = UiPanelData { size: [2.0, 1.0], clickable: false, ..UiPanelData::default() };
        let text = toml::to_string(&data).unwrap();
        assert_eq!(text, "size = [2.0, 1.0]\nclickable = false\n");
        assert_eq!(toml::from_str::<UiPanelData>(&text).unwrap(), data);
        let changes = data.changes_from(Some(&UiPanelData::default()));
        assert_eq!(changes.keys().collect::<Vec<_>>(), ["clickable", "size"]);
    }
}
