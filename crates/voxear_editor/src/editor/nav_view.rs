//! The Navigation gizmo: where things can walk in each Block World near the
//! first Scene view's camera, a square on each place's floor coloured by
//! what it is (ground, a ford, water swum, a shut door), and the way each
//! Nav Agent and Block Mob is walking, drawn while editing and playing; and
//! each Nav Obstacle's box, brighter while it is cut out of the nav, and
//! each Portal's way in.

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
/// How a Nav Link is drawn.
const LINK: [f32; 4] = [0.85, 0.45, 1.0, 1.0];
/// How a Nav Obstacle is drawn: carving, and gone round as it moves.
const CARVING: [f32; 4] = [1.0, 0.3, 0.25, 1.0];
const MOVING: [f32; 4] = [1.0, 0.3, 0.25, 0.45];

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
        // The level's baked nav mesh: each polygon near the camera, a little over its floor.
        if let Some((mesh, offset)) = voxear_engine::navigation::level_nav(world) {
            let seen = eye + offset;
            let mut triangles = Vec::new();
            for poly in &mesh.polys {
                let nearest =
                    Vec3::new(seen.x.clamp(poly.min[0], poly.max[0]), seen.y, seen.z.clamp(poly.min[1], poly.max[1]));
                if (nearest - seen).length() > REACH {
                    continue;
                }
                let inset = 0.04;
                let corner = |x: f32, z: f32| Vec3::new(x, poly.height_at(x, z) + 0.05, z) - offset;
                let (x0, z0, x1, z1) =
                    (poly.min[0] + inset, poly.min[1] + inset, poly.max[0] - inset, poly.max[1] - inset);
                let [a, b, c, d] = [corner(x0, z0), corner(x1, z0), corner(x1, z1), corner(x0, z1)];
                triangles.extend([[a, c, b], [a, d, c]]);
                if triangles.len() > MOST * 2 {
                    break;
                }
            }
            debug_draw::solid(triangles, STAND, 0.0, style);
        }
        // Each Nav Link, from its Obj to its end, an arrow, both ways where it goes both ways.
        let links: Vec<(Vec3, Vec3, bool)> = world
            .query_ref::<(Entity, &voxear_engine::navigation::NavLink)>()
            .into_iter()
            .filter_map(|(entity, link)| {
                let (from, to) = voxear_engine::navigation::link_ends(world, entity, link)?;
                Some((from, to, link.both_ways))
            })
            .collect();
        for (from, to, both) in links {
            debug_draw::arrow(from + Vec3::Y * 0.2, to + Vec3::Y * 0.2, LINK, 0.0, style);
            if both {
                debug_draw::arrow(to + Vec3::Y * 0.2, from + Vec3::Y * 0.2, LINK, 0.0, style);
            }
        }
        // Each Portal as agents take it: from where they stand in front of it to just through its opening.
        for way in voxear_engine::portals::nav_portals(world) {
            debug_draw::arrow(way.approach + Vec3::Y * 0.2, way.through + Vec3::Y * 0.2, LINK, 0.0, style);
        }
        // Each Nav Obstacle's box, turned and scaled with its Obj.
        let obstacles: Vec<(Vec3, Vec3, voxear_core::math::Quat, bool)> = world
            .query_ref::<(Entity, &voxear_engine::nav_obstacle::NavObstacle, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, obstacle, local, placed)| {
                let (scale, rotation, middle) = match placed {
                    Some(placed) => placed.0.to_scale_rotation_translation(),
                    None => (local.scale, local.rotation, local.position),
                };
                let half = Vec3::from_array(obstacle.size).abs() * scale.abs() * 0.5;
                (middle, half, rotation, voxear_engine::nav_obstacle::is_carving(world, entity))
            })
            .collect();
        for (middle, half, rotation, carving) in obstacles {
            debug_draw::wire_box(middle, half, rotation, if carving { CARVING } else { MOVING }, 0.0, style);
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
