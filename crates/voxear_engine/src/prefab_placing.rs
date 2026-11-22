//! Placing prefabs on a world: where each goes when lined up along a road or
//! scattered over an area, worked out from a seed so "again with another
//! seed" gives another town and the same seed the same one. Each prefab's
//! **Placement** says how much ground it takes up, which way its front faces
//! and where its door is; prefabs without one take up a square of a set size.
//! What is placed keeps clear of what stands there already (Placements and
//! Ground Fits) and of each other.
//!
//! The Objs themselves are made by whoever asks: the editor, as one step to
//! undo, or a script.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::WorldTransform;

/// How much ground a prefab takes up, which way its front faces and where
/// its door is, all in its own space: on its root, so every instance has it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Placement {
    /// Its footprint's size across and along its front, in metres, and its
    /// middle from the Obj along x and z.
    pub size: [f32; 2],
    pub centre: [f32; 2],
    /// Which way its front faces, in degrees about up from +z.
    pub front: f32,
    /// Its door, in its own space.
    pub door: [f32; 3],
}

impl voxear_ecs::Component for Placement {}

impl Default for Placement {
    fn default() -> Self {
        Self { size: [6.0, 6.0], centre: [0.0, 0.0], front: 0.0, door: [0.0, 0.0, 3.0] }
    }
}

/// A rectangle of ground along x and z: its middle, half its size along its
/// own two ways, and how far it is turned about up, in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub centre: [f32; 2],
    pub half: [f32; 2],
    pub yaw: f32,
}

impl Footprint {
    fn axes(&self) -> [[f32; 2]; 2] {
        let (sin, cos) = self.yaw.sin_cos();
        [[cos, -sin], [sin, cos]]
    }

    fn corners(&self) -> [[f32; 2]; 4] {
        let [a, b] = self.axes();
        let [hx, hz] = self.half;
        let at = |sx: f32, sz: f32| {
            [self.centre[0] + a[0] * hx * sx + b[0] * hz * sz, self.centre[1] + a[1] * hx * sx + b[1] * hz * sz]
        };
        [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
    }

    /// Whether it and `other` overlap, by their sides as axes apart.
    pub fn meets(&self, other: &Footprint) -> bool {
        let (mine, theirs) = (self.corners(), other.corners());
        let axes = self.axes().into_iter().chain(other.axes());
        for axis in axes {
            let project = |corners: &[[f32; 2]; 4]| {
                let along = corners.map(|c| c[0] * axis[0] + c[1] * axis[1]);
                (along.iter().copied().fold(f32::MAX, f32::min), along.iter().copied().fold(f32::MIN, f32::max))
            };
            let ((a0, a1), (b0, b1)) = (project(&mine), project(&theirs));
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
        true
    }

    /// Whether its middle lies inside the loop `outline`.
    fn inside(&self, outline: &[[f32; 2]]) -> bool {
        let point = self.centre;
        let mut within = false;
        for (at, &a) in outline.iter().enumerate() {
            let b = outline[(at + 1) % outline.len()];
            if (a[1] > point[1]) != (b[1] > point[1]) {
                let cross = a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if point[0] < cross {
                    within = !within;
                }
            }
        }
        within
    }
}

/// A prefab to place, as often as its weight, with its Placement.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub weight: u32,
    pub placement: Placement,
}

/// Where a prefab goes: which of the candidates, where along x and z (up is
/// the ground's, found by whoever makes it), and how far it is turned about
/// up, in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub which: usize,
    pub position: [f32; 2],
    pub yaw: f32,
}

/// Which side of a road to line up along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    #[default]
    Both,
}

/// How to line prefabs up along a road: which side, the stretch from `from`
/// to `to` metres along it (`to` 0 for its end), how far back from its edge
/// each stands, give or take `jitter`, how far apart, and the seed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineUp {
    pub side: Side,
    pub from: f32,
    pub to: f32,
    pub setback: f32,
    pub jitter: f32,
    pub gap: f32,
    pub seed: u64,
}

impl Default for LineUp {
    fn default() -> Self {
        Self { side: Side::Both, from: 0.0, to: 0.0, setback: 1.0, jitter: 0.5, gap: 1.0, seed: 1 }
    }
}

/// A number from 0 to 1 for `seed` and the counts given, the same on every
/// machine.
fn unit(seed: u64, a: u64, b: u64) -> f32 {
    let bytes = [seed.to_le_bytes(), a.to_le_bytes(), b.to_le_bytes()].concat();
    (voxear_terrain::layers::stable_hash(&bytes) >> 40) as f32 / (1u64 << 24) as f32
}

/// One of `candidates` by weight, `roll` from 0 to 1, other than `not` where
/// another can be had.
fn pick(candidates: &[Candidate], roll: f32, not: Option<usize>) -> Option<usize> {
    let weight = |at: usize| if Some(at) == not && candidates.len() > 1 { 0 } else { candidates[at].weight };
    let total: u32 = (0..candidates.len()).map(weight).sum();
    if total == 0 {
        return None;
    }
    let mut left = (roll * total as f32) as u32 % total;
    for at in 0..candidates.len() {
        if left < weight(at) {
            return Some(at);
        }
        left -= weight(at);
    }
    None
}

/// The footprint `placement` takes up placed at `position`, turned `yaw`.
pub fn footprint_of(placement: &Placement, position: [f32; 2], yaw: f32) -> Footprint {
    let (sin, cos) = yaw.sin_cos();
    let [cx, cz] = placement.centre;
    let centre = [position[0] + cx * cos + cz * sin, position[1] - cx * sin + cz * cos];
    Footprint { centre, half: placement.size.map(|v| v.max(0.1) * 0.5), yaw }
}

/// Prefabs of `candidates` lined up along a road through `points` (each its
/// place along x and z and its width there in metres), facing it, as `how`
/// says: none where another stands already in `taken`, no two alike side by
/// side where there are others to choose.
pub fn line_up(points: &[([f32; 2], f32)], candidates: &[Candidate], how: &LineUp, taken: &[Footprint]) -> Vec<Placed> {
    let mut placed: Vec<Placed> = Vec::new();
    let mut prints: Vec<Footprint> = taken.to_vec();
    let lengths: Vec<f32> = points.windows(2).map(|pair| distance(pair[0].0, pair[1].0)).collect();
    let total: f32 = lengths.iter().sum();
    let end = if how.to > 0.0 { how.to.min(total) } else { total };
    let sides: &[f32] = match how.side {
        Side::Left => &[1.0],
        Side::Right => &[-1.0],
        Side::Both => &[1.0, -1.0],
    };
    for (side_number, &side) in sides.iter().enumerate() {
        let (mut along, mut count, mut last) = (how.from.max(0.0), 0u64, None);
        while along < end {
            count += 1;
            let roll = unit(how.seed, side_number as u64, count);
            let Some(which) = pick(candidates, roll, last) else { break };
            let placement = &candidates[which].placement;
            let [across, deep] = placement.size.map(|v| v.max(0.5));
            let middle = along + across * 0.5;
            if middle > end {
                break;
            }
            let Some((at, tangent, width)) = point_along(points, &lengths, middle) else { break };
            let normal = [-tangent[1], tangent[0]];
            let jitter = (unit(how.seed ^ 0x9e37, side_number as u64, count) * 2.0 - 1.0) * how.jitter;
            let out = width * 0.5 + how.setback.max(0.0) + jitter.max(-how.setback.max(0.0)) + deep * 0.5;
            let position = [at[0] + normal[0] * side * out, at[1] + normal[1] * side * out];
            // Its front to the road: facing back across the normal.
            let facing = [-normal[0] * side, -normal[1] * side];
            let yaw = facing[0].atan2(facing[1]) - placement.front.to_radians();
            let print = footprint_of(placement, position, yaw);
            if prints.iter().any(|other| other.meets(&print)) {
                // Taken: tried again a metre on.
                along += 1.0;
                continue;
            }
            prints.push(print);
            placed.push(Placed { which, position, yaw });
            last = Some(which);
            along += across + how.gap.max(0.0);
        }
    }
    placed
}

/// Prefabs of `candidates` scattered over the loop `outline`, about
/// `density` of them each 100 square metres, turned any way, none where
/// another stands already in `taken`, from `seed`.
pub fn scatter(outline: &[[f32; 2]], candidates: &[Candidate], density: f32, seed: u64, taken: &[Footprint]) -> Vec<Placed> {
    if outline.len() < 3 || density <= 0.0 {
        return Vec::new();
    }
    let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
    for corner in outline {
        for axis in 0..2 {
            low[axis] = low[axis].min(corner[axis]);
            high[axis] = high[axis].max(corner[axis]);
        }
    }
    // A grid a try a cell, each cell as large as one prefab is given, its try somewhere inside it.
    let cell = (100.0 / density).sqrt().max(0.5);
    let mut prints: Vec<Footprint> = taken.to_vec();
    let mut placed = Vec::new();
    let cells = |axis: usize| ((high[axis] - low[axis]) / cell).ceil().max(1.0) as i64;
    for z in 0..cells(1) {
        for x in 0..cells(0) {
            let key = (x as u64) << 32 | z as u64;
            let Some(which) = pick(candidates, unit(seed, key, 0), None) else { return placed };
            let position = [
                low[0] + (x as f32 + unit(seed, key, 1)) * cell,
                low[1] + (z as f32 + unit(seed, key, 2)) * cell,
            ];
            let yaw = unit(seed, key, 3) * std::f32::consts::TAU;
            let print = footprint_of(&candidates[which].placement, position, yaw);
            if !print.inside(outline) || prints.iter().any(|other| other.meets(&print)) {
                continue;
            }
            prints.push(print);
            placed.push(Placed { which, position, yaw });
        }
    }
    placed
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

/// The place `along` metres along a line of points, which way it runs
/// there, and how wide it is.
fn point_along(points: &[([f32; 2], f32)], lengths: &[f32], along: f32) -> Option<([f32; 2], [f32; 2], f32)> {
    let mut walked = 0.0;
    for (at, &length) in lengths.iter().enumerate() {
        if along <= walked + length && length > 0.0 {
            let share = (along - walked) / length;
            let ((a, wa), (b, wb)) = (points[at], points[at + 1]);
            let tangent = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
            return Some(([a[0] + (b[0] - a[0]) * share, a[1] + (b[1] - a[1]) * share], tangent, wa + (wb - wa) * share));
        }
        walked += length;
    }
    None
}

/// The ground every Obj with a Placement or a Ground Fit in `world` takes up,
/// along x and z in metres, but for `skip` and what is under it.
pub fn taken(world: &World, skip: &[Entity]) -> Vec<Footprint> {
    let yaw_of = |placed: &Mat4| {
        let forward = placed.transform_vector3(Vec3::Z);
        forward.x.atan2(forward.z)
    };
    let mut prints = Vec::new();
    for (entity, placement, placed) in world.query_ref::<(Entity, &Placement, &WorldTransform)>() {
        if !skip.contains(&entity) {
            let at = placed.0.w_axis;
            prints.push(footprint_of(placement, [at.x, at.z], yaw_of(&placed.0)));
        }
    }
    for (entity, fit, placed) in world.query_ref::<(Entity, &crate::ground_fit::GroundFit, &WorldTransform)>() {
        if skip.contains(&entity) || world.has::<Placement>(entity) {
            continue;
        }
        let at = placed.0.w_axis;
        for fit_box in &fit.boxes {
            let placement = Placement { size: fit_box.size, centre: fit_box.centre, ..Placement::default() };
            prints.push(footprint_of(&placement, [at.x, at.z], yaw_of(&placed.0)));
        }
    }
    prints
}

#[cfg(test)]
mod tests {
    use super::*;

    fn houses() -> Vec<Candidate> {
        let house = |size: [f32; 2]| Candidate { weight: 1, placement: Placement { size, ..Placement::default() } };
        vec![house([6.0, 6.0]), house([8.0, 6.0]), house([5.0, 7.0])]
    }

    #[test]
    fn houses_line_up_along_both_sides_facing_the_road_none_alike_beside_none_overlapping() {
        let road = [([0.0, 0.0], 6.0), ([100.0, 0.0], 6.0)];
        let placed = line_up(&road, &houses(), &LineUp::default(), &[]);
        assert!(placed.len() >= 20, "{} placed", placed.len());
        let left: Vec<&Placed> = placed.iter().filter(|p| p.position[1] > 0.0).collect();
        assert!(!left.is_empty() && left.len() < placed.len(), "both sides");
        assert!(left.windows(2).all(|pair| pair[0].which != pair[1].which), "none alike side by side");
        // Facing the road: a house on the left (+z) faces -z.
        let facing = |p: &Placed| [p.yaw.sin(), p.yaw.cos()];
        assert!(left.iter().all(|p| facing(p)[1] < -0.99));
        // Back from the edge, no nearer than the road's half width and its setback less the jitter.
        assert!(left.iter().all(|p| p.position[1] >= 3.0 + 0.5 + 3.0 - 0.01));
        let prints: Vec<Footprint> =
            placed.iter().map(|p| footprint_of(&houses()[p.which].placement, p.position, p.yaw)).collect();
        for (at, print) in prints.iter().enumerate() {
            assert!(prints[at + 1..].iter().all(|other| !print.meets(other)), "overlap at {at}");
        }
        // The same seed the same street; another seed another.
        assert_eq!(placed, line_up(&road, &houses(), &LineUp::default(), &[]));
        let other = line_up(&road, &houses(), &LineUp { seed: 7, ..LineUp::default() }, &[]);
        assert_ne!(placed, other);
        // A house already there is kept clear of.
        let there = Footprint { centre: [50.0, 7.0], half: [10.0, 4.0], yaw: 0.0 };
        let around = line_up(&road, &houses(), &LineUp { side: Side::Left, ..LineUp::default() }, &[there]);
        assert!(around.iter().all(|p| !footprint_of(&houses()[p.which].placement, p.position, p.yaw).meets(&there)));
    }

    #[test]
    fn scattering_fills_an_outline_by_density_and_keeps_inside_it() {
        let square = [[0.0, 0.0], [40.0, 0.0], [40.0, 40.0], [0.0, 40.0]];
        let small = vec![Candidate { weight: 1, placement: Placement { size: [1.0, 1.0], ..Placement::default() } }];
        let some = scatter(&square, &small, 2.0, 3, &[]);
        assert!((20..=40).contains(&some.len()), "{} scattered over 1600 m² at 2 a hundred", some.len());
        assert!(some.iter().all(|p| (0.0..=40.0).contains(&p.position[0]) && (0.0..=40.0).contains(&p.position[1])));
        let more = scatter(&square, &small, 8.0, 3, &[]);
        assert!(more.len() > some.len() * 2);
        let triangle = [[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]];
        assert!(scatter(&triangle, &small, 8.0, 3, &[]).iter().all(|p| p.position[0] + p.position[1] <= 41.0));
    }
}
