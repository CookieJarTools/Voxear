//! The Scene view's Heat shading: the lit scene with the ground round its
//! camera tinted by how hot or cold it is there, blue for freezing through
//! green for mild to red for scorching (see `voxear_engine::heat`). The
//! ground is looked at on a grid, a few hundred places a frame, and the
//! tint laid over it as squares the Scene views draw with their gizmos.

use std::collections::HashMap;

use voxear_core::{debug_draw, math::Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{BlockWorld, is_ground};

use super::Editor;

/// The shading menu's Heat, by its place in the menu.
pub(super) const HEAT_SHADING: usize = 4;
/// How far apart the tinted places are, in metres.
const SPACING: f32 = 1.5;
/// How many places along each side of the grid.
const SIDE: i32 = 40;
/// How many places are looked at again each frame.
const PER_FRAME: usize = 240;
/// How far under the camera the ground is looked for, in metres.
const DEPTH: f32 = 80.0;

/// What the Heat shading has looked at: each grid place's ground and
/// temperature, and which place it looks at next.
#[derive(Default)]
pub(crate) struct HeatView {
    places: HashMap<[i32; 2], Option<(f32, f32)>>,
    next: usize,
}

impl Editor {
    /// Tints the ground round the camera of the first Scene view shaded by
    /// Heat, and forgets what it looked at while none is.
    pub(super) fn draw_heat_view(&mut self) {
        let Some(camera) = self.all_scene_views().find(|view| view.shading == HEAT_SHADING).map(|view| view.camera)
        else {
            self.heat_view = HeatView::default();
            return;
        };
        let world = &mut self.game.engine.world;
        let Some(eye) = world.get::<Transform>(camera).map(|transform| transform.position) else { return };
        let middle = [(eye.x / SPACING).round() as i32, (eye.z / SPACING).round() as i32];
        let half = SIDE / 2;
        let wanted: Vec<[i32; 2]> =
            (-half..half).flat_map(|z| (-half..half).map(move |x| [middle[0] + x, middle[1] + z])).collect();
        let view = &mut self.heat_view;
        view.places.retain(|place, _| (place[0] - middle[0]).abs() <= half && (place[1] - middle[1]).abs() <= half);
        // Places not yet looked at first, then the rest in turn, so a change shows within a few frames.
        let mut looked = 0;
        for place in &wanted {
            if looked < PER_FRAME && !view.places.contains_key(place) {
                view.places.insert(*place, look(world, *place, eye.y));
                looked += 1;
            }
        }
        while looked < PER_FRAME {
            let place = wanted[view.next % wanted.len()];
            view.next = (view.next + 1) % wanted.len();
            view.places.insert(place, look(world, place, eye.y));
            looked += 1;
        }
        let style = debug_draw::LineStyle { gizmo: true, ..debug_draw::LineStyle::default() };
        let corner = SPACING * 0.48;
        for (place, seen) in &view.places {
            let Some((height, celsius)) = *seen else { continue };
            let at = Vec3::new(place[0] as f32 * SPACING, height + 0.03, place[1] as f32 * SPACING);
            let [a, b, c, d] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, z)| at + Vec3::new(x * corner, 0.0, z * corner));
            debug_draw::solid([[a, b, c], [a, c, d]], tint(celsius), 0.0, style);
        }
    }
}

/// The ground at a grid place under a camera `eye` metres high, and how hot
/// it is just over it: none where there is no ground.
fn look(world: &mut World, place: [i32; 2], eye: f32) -> Option<(f32, f32)> {
    let (x, z) = (place[0] as f32 * SPACING, place[1] as f32 * SPACING);
    let height = ground(world, x, z, eye)?;
    Some((height, voxear_engine::heat::at(world, Vec3::new(x, height + 0.5, z))))
}

/// The highest ground under `eye` at `(x, z)`: a world of blocks' top block
/// there, or a terrain's surface.
fn ground(world: &World, x: f32, z: f32, eye: f32) -> Option<f32> {
    let worlds: Vec<(Entity, Vec3)> = world
        .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _, local, placed)| (entity, voxear_terrain::corner(local, placed)))
        .collect();
    let mut highest: Option<f32> = None;
    for (entity, corner) in worlds {
        let Some(blocks) = world.get::<BlockWorld>(entity) else { continue };
        let size = blocks.block_size.max(0.01);
        let local = Vec3::new(x, eye, z) - corner;
        let (top, _) = blocks.block_at(local.to_array());
        let steps = (DEPTH / size) as i32;
        let found = (0..steps)
            .map(|down| [top[0], top[1] - down, top[2]])
            .find(|at| is_ground(&blocks.faces, blocks.blocks.get(*at)));
        if let Some(at) = found {
            let height = corner.y + (at[1] + 1) as f32 * size;
            highest = Some(highest.map_or(height, |was| was.max(height)));
        }
    }
    let terrain = voxear_terrain::ground_at(world, x, z).map(|ground| ground.height).filter(|&height| height <= eye);
    match (highest, terrain) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// The colour a temperature is tinted: blue at -20 °C and colder, cyan at
/// freezing, green at 15, yellow at 30, red at 45 and white past 200, a
/// little see-through.
fn tint(celsius: f32) -> [f32; 4] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (-20.0, [0.15, 0.25, 1.0]),
        (0.0, [0.2, 0.85, 1.0]),
        (15.0, [0.25, 0.85, 0.3]),
        (30.0, [1.0, 0.85, 0.15]),
        (45.0, [1.0, 0.2, 0.1]),
        (200.0, [1.0, 0.95, 0.9]),
    ];
    let color = match STOPS.iter().position(|(at, _)| celsius < *at) {
        Some(0) => STOPS[0].1,
        None => STOPS[STOPS.len() - 1].1,
        Some(next) => {
            let ((from, low), (to, high)) = (STOPS[next - 1], STOPS[next]);
            let share = (celsius - from) / (to - from);
            [0, 1, 2].map(|at| low[at] + (high[at] - low[at]) * share)
        }
    };
    [color[0], color[1], color[2], 0.45]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_is_blue_mild_green_and_hot_red() {
        assert_eq!(tint(-40.0), [0.15, 0.25, 1.0, 0.45]);
        assert_eq!(tint(15.0), [0.25, 0.85, 0.3, 0.45]);
        assert_eq!(tint(45.0)[0], 1.0);
        assert!(tint(22.5)[1] > tint(40.0)[1], "greener when mild");
        assert_eq!(tint(1000.0), [1.0, 0.95, 0.9, 0.45]);
    }
}
