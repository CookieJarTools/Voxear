//! The Navigation gizmo: where things can walk in each Block World near the
//! first Scene view's camera, a square on each place's floor coloured by
//! what it is (ground, a ford, water swum, a shut door), and the way each
//! Nav Agent and Block Mob is walking, drawn while editing and playing.

use voxear_core::{debug_draw, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::nav_agent::NavAgentState;
use voxear_nav::PlaceKind;
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{BlockWorld, chunk::CHUNK};

use super::Editor;

/// How far round the camera places are drawn, in metres.
const REACH: f32 = 40.0;
/// The most places drawn in a frame, the nearest first.
const MOST: usize = 8000;
/// How a place of each kind is coloured, sRGB with alpha.
const STAND: [f32; 4] = [0.25, 0.85, 0.55, 0.35];
const WADE: [f32; 4] = [0.35, 0.7, 1.0, 0.4];
const SWIM: [f32; 4] = [0.2, 0.35, 1.0, 0.45];
const DOOR: [f32; 4] = [1.0, 0.6, 0.2, 0.55];
/// How an agent's way is drawn.
const WAY: [f32; 4] = [1.0, 0.85, 0.25, 1.0];

impl Editor {
    /// Draws the Navigation gizmo where it is switched on.
    pub(super) fn draw_nav_view(&mut self) {
        if !self.memory.settings.nav_gizmos {
            return;
        }
        let Some(camera) = self.all_scene_views().next().map(|view| view.camera) else { return };
        let world = &mut self.game.engine.world;
        let Some(eye) = world.get::<Transform>(camera).map(|transform| transform.position) else { return };
        let here = voxear_scene::dimension_of(world, camera);
        let worlds: Vec<(Entity, Vec3, f32)> = world
            .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(entity, ..)| voxear_scene::dimension_of(world, *entity) == here)
            .map(|(entity, blocks, local, placed)| (entity, voxear_terrain::corner(local, placed), blocks.block_size))
            .collect();
        voxear_engine::navigation::update(world);
        let style = debug_draw::LineStyle { gizmo: true, ..debug_draw::LineStyle::default() };
        for (entity, corner, size) in worlds {
            let Some(nav) = voxear_engine::navigation::nav_of(world, entity) else { continue };
            let size = size.max(0.01);
            let (seen, reach) = ((eye - corner) / size, REACH / size);
            let side = CHUNK as f32;
            // Each place within reach, nearest first, up to the most drawn.
            let mut near: Vec<(f32, Vec3, PlaceKind)> = Vec::new();
            for (_, slot) in nav.slots() {
                let low = Vec3::new(slot.key[0] as f32, slot.key[1] as f32, slot.key[2] as f32) * side;
                let nearest = seen.clamp(low, low + Vec3::splat(side));
                if (nearest - seen).length() > reach {
                    continue;
                }
                for z in 0..CHUNK {
                    for x in 0..CHUNK {
                        for place in slot.chunk.column(x, z) {
                            let at = Vec3::new(
                                low.x + x as f32 + 0.5,
                                place.feet(slot.key[1]) as f32 / 16.0,
                                low.z + z as f32 + 0.5,
                            );
                            let far = (at - seen).length();
                            if far <= reach {
                                near.push((far, at, place.kind));
                            }
                        }
                    }
                }
            }
            near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            near.truncate(MOST);
            let half = 0.42 * size;
            let mut squares: [Vec<[Vec3; 3]>; 4] = Default::default();
            for (_, at, kind) in near {
                let middle = corner + at * size + Vec3::Y * 0.03 * size;
                let [a, b, c, d] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .map(|(x, z)| middle + Vec3::new(x * half, 0.0, z * half));
                // Wound to face up: debug draw shows a triangle's front only.
                squares[kind as usize].extend([[a, c, b], [a, d, c]]);
            }
            for (kind, triangles) in squares.into_iter().enumerate() {
                let color = [STAND, WADE, SWIM, DOOR][kind];
                debug_draw::solid(triangles, color, 0.0, style);
            }
        }
        // Each agent's way, corner to corner, a little over the floor.
        let lift = Vec3::Y * 0.1;
        let ways: Vec<Vec<Vec3>> = world
            .query_ref::<&NavAgentState>()
            .into_iter()
            .filter(|state| state.corners.len() > 1)
            .map(|state| state.corners.clone())
            .collect();
        for corners in ways {
            for pair in corners.windows(2) {
                debug_draw::line(pair[0] + lift, pair[1] + lift, WAY, 0.0, style);
            }
        }
    }
}
