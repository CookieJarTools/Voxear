//! Behaviours' gizmos (`DrawGizmos` in Voxear.Core), drawn each frame for
//! the Scene views that show gizmos: while editing, from each behaviour's
//! scene values, for the types that draw any; while playing, by the running
//! behaviours. Each is told whether its Obj is selected, and whether the
//! pointer is over it in the Scene view in use. What they draw goes into
//! debug draw marked as gizmos, which the Game view leaves out.
//!
//! The handles they show (`Handles` in Voxear.Core) are told what the
//! pointer does to them. While editing, the fields a drag changes come
//! back to be written into the scene, as the Inspector would.
//!
//! Each behaviour's gizmo lines are kept with its Obj until the next
//! frame's, so a click on one selects the Obj, as a click on a light's sun
//! does. A behaviour type's `[GizmoIcon]` shows where each Obj with it is,
//! and a click on the icon selects it too. The Gizmos menu hides a type's
//! gizmos and icon together.

use voxear_core::{debug_draw, math::Vec3};
use voxear_ecs::Entity;
use voxear_scene::{Behaviours, Transform, WorldTransform};
use voxear_scripting::{GizmoAnswer, GizmoTarget};
use voxear_ui::{Color, Shape};

use super::Editor;
use crate::{
    fields,
    scene_tools::{Markers, ViewCamera, to_segment},
};

/// How near a click must be to a gizmo's line to select its Obj, in logical pixels.
const REACH: f32 = 5.0;
/// A behaviour's icon on screen, and the disc behind it, which a click
/// anywhere on selects its Obj, in logical pixels.
const ICON: f32 = 13.0;
const ICON_DISC: f32 = 10.0;
const ICON_COLOR: u32 = 0xe6e8eb;
/// The selected Obj's icon is drawn in the accent.
const ICON_SELECTED: u32 = 0x7988ff;

/// A `[GizmoIcon]` to show: the Obj, where it is, and the icon.
pub(super) type GizmoIcon = (Entity, Vec3, &'static str);

/// The icons as shapes in a Scene view's own coordinates (from its top left).
pub(super) fn icon_shapes(icons: &[GizmoIcon], camera: &ViewCamera, selected: Option<Entity>) -> Vec<Shape> {
    let mut shapes = Vec::new();
    for &(entity, at, name) in icons {
        let Some(at) = camera.project(at) else { continue };
        let center = [at[0] - camera.rect[0], at[1] - camera.rect[1]];
        let color = Color::hex(if Some(entity) == selected { ICON_SELECTED } else { ICON_COLOR });
        let fill = Color::rgba(12, 14, 16, 210);
        shapes.push(Shape::Circle { center, radius: ICON_DISC, fill, border: Some((1.0, color.with_alpha(120))) });
        shapes.push(Shape::Icon { name, center, size: ICON, color });
    }
    shapes
}

/// The gizmo lines drawn last, each with the Obj whose behaviour drew it.
#[derive(Default)]
pub(super) struct GizmoLines(Vec<(Entity, Vec3, Vec3)>);

impl GizmoLines {
    /// Lines drawn since `start` in debug draw, given out in order to the
    /// Obj that drew each run of them.
    fn since(start: usize, drawn: &[(Entity, usize)]) -> Self {
        let owners = drawn.iter().flat_map(|&(entity, count)| std::iter::repeat_n(entity, count));
        let lines = debug_draw::lines_from(start).into_iter().filter(|line| line.gizmo);
        Self(owners.zip(lines).map(|(entity, line)| (entity, line.from, line.to)).collect())
    }

    /// The Obj whose gizmo line is nearest a point in the Scene view, if one
    /// is near enough.
    pub(super) fn under(&self, camera: &ViewCamera, point: [f32; 2]) -> Option<Entity> {
        let near = |&(entity, from, to): &(Entity, Vec3, Vec3)| {
            Some((entity, to_segment(point, camera.project(from)?, camera.project(to)?)))
        };
        let distances = self.0.iter().filter_map(near).filter(|&(_, d)| d <= REACH);
        distances.min_by(|a, b| a.1.total_cmp(&b.1)).map(|(entity, _)| entity)
    }
}

impl Editor {
    /// The `[GizmoIcon]`s to show: one for each Obj with a behaviour whose
    /// type has one, its first behaviour's, but for the types the Gizmos
    /// menu hides. While editing, from its scene values; while playing, from
    /// what runs.
    pub(super) fn gizmo_icons(&self) -> Vec<GizmoIcon> {
        let (types, hidden) = (self.inspector.types(), &self.memory.settings.hidden_gizmos);
        let icon_of = |type_name: &str| {
            let icon =
                types.behaviour(type_name).filter(|_| !hidden.iter().any(|h| h == type_name))?.icon.as_deref()?;
            voxear_ui::ICONS.iter().copied().find(|&known| known == icon)
        };
        let world = &self.game.engine.world;
        let at = |entity: Entity| {
            let placed = world.get::<WorldTransform>(entity).map(|w| w.0.w_axis.truncate());
            placed.or_else(|| world.get::<Transform>(entity).map(|t| t.position))
        };
        let icon = |entity: Entity, mut names: Vec<&str>| {
            let name = names.drain(..).find_map(icon_of)?;
            Some((entity, at(entity)?, name))
        };
        if self.is_playing() {
            let running = world.query_ref::<(Entity, &Behaviours)>().into_iter();
            return running
                .filter_map(|(entity, b)| icon(entity, b.0.iter().map(|b| b.spec.type_name.as_str()).collect()))
                .collect();
        }
        let kept = world.query_ref::<(Entity, &voxear_engine::EditBehaviours)>().into_iter();
        kept.filter_map(|(entity, k)| icon(entity, k.0.iter().map(|s| s.type_name.as_str()).collect())).collect()
    }

    /// The Obj whose behaviour's icon or gizmo line is under a point in the
    /// Scene view in use, if any.
    pub(super) fn gizmo_under(&self, camera: &ViewCamera, point: [f32; 2]) -> Option<Entity> {
        let near = |&(entity, at, _): &GizmoIcon| {
            let at = camera.project(at)?;
            Some((entity, (at[0] - point[0]).hypot(at[1] - point[1])))
        };
        let icons = self.gizmo_icons();
        let icon = icons.iter().filter_map(near).filter(|&(_, d)| d <= ICON_DISC);
        let icon = icon.min_by(|a, b| a.1.total_cmp(&b.1)).map(|(entity, _)| entity);
        icon.or_else(|| self.gizmo_lines.under(camera, point))
    }

    /// The lights' and cameras' markers that show in a Scene view, as its
    /// Gizmos button and their menu say.
    pub(super) fn markers(&self, view_shows: bool) -> Markers {
        let settings = &self.memory.settings;
        let shown = Markers { lights: settings.light_gizmos, cameras: settings.camera_gizmos };
        if view_shows { shown } else { Markers::NONE }
    }

    /// Draws this frame's gizmos, after debug draw counted down the last
    /// frame's, so they show in the frame they are drawn for.
    pub(super) fn draw_scene_gizmos(&mut self) {
        let start = debug_draw::line_count();
        let answer = self.ask_scene_gizmos();
        if let Some(Err(error)) = &answer {
            tracing::warn!("could not draw the behaviours' gizmos: {error:#}");
        }
        let GizmoAnswer { handles, changed, drawn } = answer.and_then(Result::ok).unwrap_or_default();
        self.gizmo_lines = GizmoLines::since(start, &drawn);
        self.script_handles.show(handles);
        for (entity, behaviour, values) in changed {
            for (name, value) in values {
                fields::set_raw(&mut self.game.engine.world, entity, &behaviour, &name, value);
            }
        }
    }

    /// The gizmos drawn, and what they gave back, or `None` with nothing to draw.
    fn ask_scene_gizmos(&mut self) -> Option<anyhow::Result<GizmoAnswer>> {
        if !self.all_scene_views().any(|view| view.show_gizmos) {
            return None;
        }
        let (selected, hovered) = (self.selected(), self.hovered_obj);
        let input = self.script_handles.input();
        let hidden = &self.memory.settings.hidden_gizmos;
        if self.is_playing() {
            return Some(self.game.engine.draw_live_gizmos((selected, hovered), hidden, &input));
        }
        let types = self.inspector.types();
        let world = &self.game.engine.world;
        let drawing: Vec<(Entity, voxear_scene::BehaviourSpec)> = world
            .query_ref::<(Entity, &voxear_engine::EditBehaviours)>()
            .into_iter()
            .flat_map(|(entity, kept)| kept.0.iter().map(move |spec| (entity, spec.clone())))
            .filter(|(_, spec)| types.behaviour(&spec.type_name).is_some_and(|t| t.gizmos))
            .filter(|(_, spec)| !hidden.contains(&spec.type_name))
            .collect();
        if drawing.is_empty() {
            return None;
        }
        let targets: Vec<GizmoTarget> = drawing
            .iter()
            .map(|(entity, spec)| GizmoTarget {
                entity: *entity,
                spec,
                selected: Some(*entity) == selected,
                hovered: Some(*entity) == hovered,
            })
            .collect();
        // The first time, this starts the scripts, as drawers do; until they have, there is nothing to draw.
        self.with_edit_scripts(|scripts, game| scripts.draw_gizmos(&mut game.engine.world, &targets, &input))
    }
}
