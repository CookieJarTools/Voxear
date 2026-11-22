//! The Scene view's Prefab Brush (see `voxear_engine::prefab_placing::PrefabBrush`):
//! with an Obj that has one selected, a ring on the ground under the
//! pointer; a stroke scatters its prefabs as Objs under the Obj, Shift takes
//! them away, and in Single a click places one, R turning it a step. Each
//! stroke or click is one step to undo.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::prefab_placing::{self, BrushMode, Candidate, Placed, PrefabBrush};
use voxear_render::Renderer;
use voxear_ui::{Color, Shape};

use super::Editor;
use crate::{
    history::Edit,
    objs,
    scene_tools::{self, ViewCamera},
};

const COLOR: u32 = 0x8fd18a;

/// A stroke under way: the brush's Obj, whether it takes away, where its last
/// dab was, how many dabs it made, and the depth of the history it started at.
struct Stroke {
    entity: Entity,
    erase: bool,
    last: [f32; 2],
    dabs: u64,
    depth: usize,
}

/// The ground under the pointer while the brush shows, and any stroke.
#[derive(Default)]
pub struct PrefabBrushes {
    under: Option<(Entity, Vec3)>,
    stroke: Option<Stroke>,
    strokes: u64,
}

impl PrefabBrushes {
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.under.is_some()
    }
}

impl Editor {
    /// Notes the ground under the pointer for the selected Obj's Prefab Brush.
    pub(super) fn hover_prefab_brush(
        &mut self,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Option<Entity>,
        point: Option<[f32; 2]>,
    ) {
        if self.prefab_brushes.is_dragging() {
            return;
        }
        let world = &self.game.engine.world;
        let brushing =
            entity.filter(|entity| world.get::<PrefabBrush>(*entity).is_some_and(|brush| !brush.prefabs.is_empty()));
        self.prefab_brushes.under = brushing.zip(point).and_then(|(entity, point)| {
            // What the brush already placed is left out of what it finds, so it paints on the ground.
            let skip = objs::family(world, entity);
            Some((entity, scene_tools::surface_past(world, renderer, camera, point, &skip)?))
        });
    }

    /// Starts a stroke, or in Single places one; whether it took the press.
    pub(super) fn begin_prefab_brush(&mut self, entity: Entity, erase: bool, renderer: &mut Renderer) -> bool {
        let Some((_, at)) = self.prefab_brushes.under.filter(|(e, _)| *e == entity) else { return false };
        self.prefab_brushes.strokes += 1;
        let depth = self.history.depth();
        self.prefab_brushes.stroke = Some(Stroke { entity, erase, last: [at.x, at.z], dabs: 0, depth });
        self.prefab_dab(at, renderer);
        true
    }

    /// Carries a Paint stroke on to the ground under `point`.
    pub(super) fn drag_prefab_brush(&mut self, camera: &ViewCamera, point: [f32; 2], renderer: &mut Renderer) {
        let Some(stroke) = &self.prefab_brushes.stroke else { return };
        let world = &self.game.engine.world;
        let Some(brush) = world.get::<PrefabBrush>(stroke.entity) else { return };
        if brush.mode == BrushMode::Single {
            return;
        }
        let skip = objs::family(world, stroke.entity);
        let Some(at) = scene_tools::surface_past(world, renderer, camera, point, &skip) else { return };
        self.prefab_brushes.under = Some((stroke.entity, at));
        // A dab each three quarters of a radius along the way.
        let gap = (brush.radius * 0.75).max(0.5);
        if ((at.x - stroke.last[0]).powi(2) + (at.z - stroke.last[1]).powi(2)).sqrt() >= gap {
            self.prefab_dab(at, renderer);
        }
    }

    /// Ends a stroke as one step to undo.
    pub(super) fn end_prefab_brush(&mut self) -> bool {
        let Some(stroke) = self.prefab_brushes.stroke.take() else { return false };
        self.history.group_since(stroke.depth);
        self.shown_again();
        true
    }

    /// Turns the Single brush's prefab a step, as R does over it.
    pub(super) fn turn_prefab_brush(&mut self) {
        let Some((entity, _)) = self.prefab_brushes.under else { return };
        if let Some(brush) = self.game.engine.world.get_mut::<PrefabBrush>(entity) {
            brush.turn = (brush.turn + brush.turn_step).rem_euclid(360.0);
        }
    }

    /// One dab of the stroke at `at`: prefabs scattered within the brush's
    /// radius, or the one Single places, or those under it taken away.
    fn prefab_dab(&mut self, at: Vec3, renderer: &mut Renderer) {
        let Some(stroke) = &mut self.prefab_brushes.stroke else { return };
        stroke.last = [at.x, at.z];
        stroke.dabs += 1;
        let (entity, erase) = (stroke.entity, stroke.erase);
        let seed = self.prefab_brushes.strokes << 20 | stroke.dabs;
        let Some(brush) = self.game.engine.world.get::<PrefabBrush>(entity).cloned() else { return };
        let paths: Vec<String> = brush
            .prefabs
            .iter()
            .filter_map(|entry| Some(self.game.content.db().get(entry.prefab?)?.path.clone()))
            .collect();
        if erase {
            return self.erase_under(entity, at, brush.radius);
        }
        let candidates: Vec<Candidate> = brush
            .prefabs
            .iter()
            .filter(|entry| entry.prefab.is_some())
            .zip(&paths)
            .map(|(entry, path)| Candidate {
                weight: entry.weight,
                placement: self.placement_of(path, renderer).unwrap_or_default(),
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let spots = match brush.mode {
            BrushMode::Single => {
                let spot = Placed { which: 0, position: [at.x, at.z], yaw: brush.turn.to_radians() };
                vec![match brush.face_road {
                    true => self.facing_road(spot, &candidates[0].placement, at, brush.setback),
                    false => spot,
                }]
            }
            BrushMode::Paint => {
                let circle: Vec<[f32; 2]> = (0..24)
                    .map(|i| {
                        let angle = i as f32 / 24.0 * std::f32::consts::TAU;
                        [at.x + angle.cos() * brush.radius, at.z + angle.sin() * brush.radius]
                    })
                    .collect();
                let taken = prefab_placing::taken(&self.game.engine.world, &[]);
                let mut spots = prefab_placing::scatter(&circle, &candidates, brush.density, seed, &taken);
                if !brush.random_turn {
                    spots.iter_mut().for_each(|spot| spot.yaw = brush.turn.to_radians());
                }
                spots
            }
        };
        for (number, spot) in spots.into_iter().enumerate() {
            let mut ground = |x: f32, z: f32| {
                let (world, content) = (&mut self.game.engine.world, &mut self.game.content);
                voxear_engine::ground_fit::ground_under(world, content, Vec3::new(x, at.y, z), None)
            };
            let [x, z] = spot.position;
            // The ground's slope there, from a metre either way.
            let (Some(east), Some(west), Some(north), Some(south)) =
                (ground(x + 1.0, z), ground(x - 1.0, z), ground(x, z + 1.0), ground(x, z - 1.0))
            else {
                continue;
            };
            let normal = Vec3::new(west - east, 2.0, south - north).normalize_or(Vec3::Y);
            if normal.y.acos().to_degrees() > brush.steepest {
                continue;
            }
            let Some(path) = paths.get(spot.which) else { continue };
            let choice = super::place_calls::Choice { path: path.clone(), weight: 1 };
            let Ok(made) = self.place_on_ground(&choice, (spot.position, spot.yaw), Some(entity), renderer) else {
                continue;
            };
            let scale = prefab_placing::scale_for(brush.scale, seed.wrapping_mul(31).wrapping_add(number as u64));
            if let Some(transform) = self.game.engine.world.get_mut::<voxear_scene::Transform>(made) {
                transform.scale *= scale;
                if brush.lean {
                    transform.rotation = Quat::from_rotation_arc(Vec3::Y, normal) * transform.rotation;
                }
            }
        }
    }

    /// Takes away what the brush on `entity` placed within `radius` of `at`.
    fn erase_under(&mut self, entity: Entity, at: Vec3, radius: f32) {
        let world = &self.game.engine.world;
        let going: Vec<Entity> = world
            .query_ref::<(Entity, &voxear_scene::Parent, &voxear_scene::WorldTransform)>()
            .into_iter()
            .filter(|(_, parent, _)| parent.0 == entity)
            .filter(|(_, _, placed)| {
                let p = placed.0.w_axis;
                Vec3::new(p.x - at.x, 0.0, p.z - at.z).length() <= radius
            })
            .map(|(child, ..)| child)
            .collect();
        for child in going {
            let taken = objs::take(&mut self.game, child);
            self.record(Edit::Exists { root: child, taken: Some(taken) });
        }
    }

    /// The brush's ring on the ground, in the view's own coordinates.
    pub(super) fn prefab_brush_shapes(&self, renderer: &Renderer, camera: &ViewCamera) -> Vec<Shape> {
        let Some((entity, at)) = self.prefab_brushes.under else { return Vec::new() };
        let world = &self.game.engine.world;
        let Some(brush) = world.get::<PrefabBrush>(entity) else { return Vec::new() };
        let radius = if brush.mode == BrushMode::Single { 1.0 } else { brush.radius };
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let skip = objs::family(world, entity);
        let ground = |x: f32, z: f32| {
            let found = scene_tools::ground_below(world, renderer, (x, z, at.y + radius + 30.0), &skip);
            found.map_or(Vec3::new(x, at.y, z), |(ground, _)| ground)
        };
        let ring: Vec<Vec3> = (0..=32)
            .map(|i| {
                let angle = i as f32 / 32.0 * std::f32::consts::TAU;
                ground(at.x + angle.cos() * radius, at.z + angle.sin() * radius)
            })
            .collect();
        let mut shapes: Vec<Shape> = ring
            .windows(2)
            .filter_map(|pair| Some((camera.project(pair[0])?, camera.project(pair[1])?)))
            .map(|(from, to)| Shape::Line { from: local(from), to: local(to), width: 1.5, color: Color::hex(COLOR) })
            .collect();
        // In Single, which way its front will face.
        if brush.mode == BrushMode::Single {
            let (sin, cos) = brush.turn.to_radians().sin_cos();
            let ahead = at + Vec3::new(sin, 0.0, cos) * 2.0;
            if let (Some(from), Some(to)) = (camera.project(at), camera.project(ahead)) {
                shapes.push(Shape::Line { from: local(from), to: local(to), width: 2.0, color: Color::hex(COLOR) });
            }
        }
        shapes
    }
}
