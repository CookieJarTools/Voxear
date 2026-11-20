//! The Ground Fit gizmo: each selected Obj's footprint drawn at the height it
//! levels to, and the rings its margin and slope out reach to round it.

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
    }
}
