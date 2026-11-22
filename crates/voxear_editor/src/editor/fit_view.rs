//! The Ground Fit gizmo: each selected Obj's footprint drawn at the height it
//! levels to, and the rings its margin and slope out reach to round it. And,
//! where the Gizmos menu's Areas and Routes is on, each World Area's outline
//! tinted by its first tag, and each Route coloured by its kind.

use voxear_core::{debug_draw, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::ground_fit::{FitBox, GroundFit, WorldPlace};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::BlockWorld;

use super::Editor;

/// How the footprint, its margin and its slope out are drawn, sRGB with alpha.
const FOOTPRINT: [f32; 4] = [1.0, 0.75, 0.2, 1.0];
const MARGIN: [f32; 4] = [1.0, 0.75, 0.2, 0.55];
const SLOPE_OUT: [f32; 4] = [1.0, 0.75, 0.2, 0.25];

impl Editor {
    /// Draws the Ground Fit of every Obj selected that has one.
    pub(super) fn draw_fit_view(&mut self) {
        // World layers left out of the ground where the Scene view's Gizmos ▸ Bare Ground says.
        let hidden = voxear_engine::ground_fit::HiddenLayers { fits: self.memory.settings.bare_ground };
        if self.game.engine.world.resource::<voxear_engine::ground_fit::HiddenLayers>().copied() != Some(hidden) {
            self.game.engine.world.insert_resource(hidden);
        }
        let world = &self.game.engine.world;
        let chosen: Vec<Entity> = self.selection.into_iter().chain(self.others.iter().copied()).collect();
        let style = debug_draw::LineStyle { gizmo: true, ..debug_draw::LineStyle::default() };
        for entity in chosen {
            let (Some(fit), Some(placed)) = (world.get::<GroundFit>(entity), world.get::<WorldTransform>(entity))
            else {
                continue;
            };
            // The world of blocks it shapes: the first in its dimension.
            let here = voxear_scene::dimension_of(world, entity);
            let place = world
                .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
                .without::<Destroying>()
                .into_iter()
                .find(|(blocks, ..)| voxear_scene::dimension_of(world, *blocks) == here)
                .map(|(_, blocks, local, at)| WorldPlace {
                    corner: voxear_terrain::corner(local, at),
                    block_size: blocks.block_size,
                })
                .unwrap_or(WorldPlace { corner: Vec3::ZERO, block_size: 1.0 });
            let size = place.block_size.max(0.01);
            let height = place.corner.y + fit.level_of(&placed.0, place) as f32 * size + 0.05;
            let loop_at = |corners: &[[f32; 2]], color: [f32; 4]| {
                for (at, corner) in corners.iter().enumerate() {
                    let next = corners[(at + 1) % corners.len()];
                    let point =
                        |[x, z]: [f32; 2]| Vec3::new(place.corner.x + x * size, height, place.corner.z + z * size);
                    debug_draw::line(point(*corner), point(next), color, 0.0, style);
                }
            };
            for outline in fit.outlines(&placed.0, place) {
                loop_at(&outline, FOOTPRINT);
            }
            // A box's rings, grown in the Obj's own space by the margin and the slope out.
            let grown = |by: f32| -> Vec<Vec<[f32; 2]>> {
                let boxes =
                    fit.boxes.iter().map(|b| FitBox { size: [b.size[0] + 2.0 * by, b.size[1] + 2.0 * by], ..*b });
                let wider = GroundFit { boxes: boxes.collect(), outline: Vec::new(), ..fit.clone() };
                wider.outlines(&placed.0, place)
            };
            let margin = fit.margin * size;
            if margin > 0.0 {
                grown(margin).iter().for_each(|ring| loop_at(ring, MARGIN));
            }
            if fit.slope_out > 0 {
                grown(margin + fit.slope_out as f32 * size).iter().for_each(|ring| loop_at(ring, SLOPE_OUT));
            }
        }
        if self.memory.settings.area_gizmos {
            self.draw_areas_and_routes();
        }
    }

    /// Each World Area's outline at its Obj's height, and each Route.
    fn draw_areas_and_routes(&mut self) {
        use voxear_engine::{
            world_areas::WorldArea,
            world_routes::{Route, RouteKind},
        };
        let style = debug_draw::LineStyle { gizmo: true, ..debug_draw::LineStyle::default() };
        let world = &self.game.engine.world;
        let here = voxear_engine::dimensions::current(world);
        let shown = |obj: Entity| voxear_scene::is_active(world, obj) && voxear_scene::dimension_of(world, obj) == here;
        let areas: Vec<(Vec<[f32; 2]>, f32, [f32; 4])> = world
            .query_ref::<(Entity, &WorldArea, &WorldTransform)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(obj, area, _)| shown(*obj) && area.shape == voxear_engine::world_areas::AreaShape::Outline)
            .map(|(_, area, placed)| {
                let tag = area.tags.iter().find_map(|entry| entry.tag);
                let color = tag.map_or(UNTAGGED, |tag| tint(tag.to_u128()));
                let color = if area.no_structures { [color[0], color[1], color[2], 1.0] } else { color };
                (area.world_outline(&placed.0), placed.0.w_axis.y + 0.1, color)
            })
            .collect();
        for (outline, height, color) in areas {
            for (at, &[x, z]) in outline.iter().enumerate() {
                let [nx, nz] = outline[(at + 1) % outline.len()];
                debug_draw::line(Vec3::new(x, height, z), Vec3::new(nx, height, nz), color, 0.0, style);
            }
        }
        let routes: Vec<(Entity, Vec<Vec3>, RouteKind)> = world
            .query_ref::<(Entity, &Route, &WorldTransform)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(obj, ..)| shown(*obj))
            .map(|(obj, route, placed)| (obj, route.world_points(&placed.0), route.kind))
            .collect();
        for (obj, points, kind) in routes {
            let aground = kind == RouteKind::Sea
                && voxear_engine::world_routes::aground(&mut self.game.engine.world, &mut self.game.content, obj);
            let color = match kind {
                _ if aground => AGROUND,
                RouteKind::Road => [0.95, 0.7, 0.3, 0.9],
                RouteKind::Sea => [0.35, 0.7, 1.0, 0.9],
                RouteKind::Air => [0.9, 0.95, 1.0, 0.9],
                RouteKind::Custom => [0.75, 0.5, 1.0, 0.9],
            };
            for pair in points.windows(2) {
                debug_draw::line(pair[0] + Vec3::Y * 0.2, pair[1] + Vec3::Y * 0.2, color, 0.0, style);
            }
        }
    }
}

/// An area with no tag, and a sea route aground, sRGB with alpha.
const UNTAGGED: [f32; 4] = [0.8, 0.8, 0.8, 0.6];
const AGROUND: [f32; 4] = [1.0, 0.2, 0.15, 1.0];

/// A tag's tint: a hue of its own, the same every time.
fn tint(tag: u128) -> [f32; 4] {
    let hue = (voxear_terrain::layers::stable_hash(&tag.to_le_bytes()) % 360) as f32 / 60.0;
    let x = 1.0 - (hue % 2.0 - 1.0).abs();
    let [r, g, b] = match hue as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    [0.35 + r * 0.65, 0.35 + g * 0.65, 0.35 + b * 0.65, 0.75]
}
