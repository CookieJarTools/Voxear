//! Rivers and lakes on finished ground, for a terrain's Rivers layer.
//!
//! First, where water goes ([`flow`]): every hollow filled to where it would
//! spill, so all water drains off the terrain's edges or into the sea, then
//! each point's water sent to the neighbour it drains through, and how much
//! ground drains through each point counted.
//!
//! Where enough ground drains into one place, a river starts, and runs down
//! that way, wider and deeper as more drains into it; its bed is cut below
//! the filled ground there, so it always runs downhill ([`carve`]). Where a
//! hollow is deep and wide enough, a lake stands in it, as high as it fills
//! before it spills.
//!
//! The ground is worked on as erosion works on it (see `erosion::grid`), and
//! what carving changed is kept in the layer's raster. The rivers' courses
//! and the lakes are worked out again from the same ground when the editor
//! makes water for them: Streams along the rivers and Pools over the lakes.

use std::{cmp::Reverse, collections::BinaryHeap};

use serde::{Deserialize, Serialize};

use crate::erosion::Grid;

/// How a Rivers layer finds and cuts its rivers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RiverSettings {
    /// How much ground drains into one place before a river starts there,
    /// in hectares.
    pub source: f32,
    /// How wide a river is where a square kilometre drains into it, in
    /// metres; wider as more does.
    pub width: f32,
    /// How deep its bed is cut there, in metres.
    pub depth: f32,
}

impl Default for RiverSettings {
    fn default() -> Self {
        Self { source: 20.0, width: 8.0, depth: 1.5 }
    }
}

/// Where a cell's water goes when it leaves the ground: off its edge or into
/// the sea.
pub const AWAY: u32 = u32::MAX;

/// How much higher each cell's filled ground stands than the one it drains
/// to, at least, so water runs over a filled hollow rather than stands.
const FALL: f32 = 1e-4;

/// The shallowest a hollow holds a lake, in metres.
const LAKE_DEPTH: f32 = 0.5;

/// The smallest lake, in square metres.
const SMALLEST_LAKE: f32 = 2000.0;

/// Where water goes over a grid: the ground with its hollows filled, the
/// cell each drains to ([`AWAY`] where water leaves), how much ground drains
/// through each, in square metres, and the cells in the order the filling
/// reached them, each after the one it drains to.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub filled: Vec<f32>,
    pub down: Vec<u32>,
    pub area: Vec<f32>,
    pub order: Vec<u32>,
}

/// One of a cell's eight neighbours, or none past the grid's edge.
fn neighbour(cells: [u32; 2], i: u32, (dx, dz): (i32, i32)) -> Option<u32> {
    let (x, z) = ((i % cells[0]) as i32 + dx, (i / cells[0]) as i32 + dz);
    (x >= 0 && z >= 0 && x < cells[0] as i32 && z < cells[1] as i32).then(|| z as u32 * cells[0] + x as u32)
}

const AROUND: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// Where water goes over `grid`, the sea at `sea_level` if it has one. The
/// lowest ground is filled first, from the edges and the sea inwards, so
/// each cell drains to the neighbour it was reached from: the lowest way
/// out, over the rim of any hollow it is in.
pub fn flow(grid: &Grid, sea_level: Option<f32>) -> Flow {
    let cells = grid.cells;
    let count = (cells[0] * cells[1]) as usize;
    let mut filled = vec![f32::NAN; count];
    let mut down = vec![AWAY; count];
    let mut order = Vec::with_capacity(count);
    // Lowest first, and among the same, by place, so it is the same every time.
    let key = |height: f32, i: u32| {
        Reverse((height.to_bits() as i32 ^ ((height.to_bits() as i32 >> 31) as u32 >> 1) as i32, i))
    };
    let mut open = BinaryHeap::new();
    for i in 0..count as u32 {
        let (x, z) = (i % cells[0], i / cells[0]);
        let edge = x == 0 || z == 0 || x == cells[0] - 1 || z == cells[1] - 1;
        let sea = sea_level.is_some_and(|level| grid.heights[i as usize] < level);
        if edge || sea {
            filled[i as usize] = grid.heights[i as usize];
            open.push(key(filled[i as usize], i));
        }
    }
    while let Some(Reverse((_, i))) = open.pop() {
        order.push(i);
        for step in AROUND {
            let Some(n) = neighbour(cells, i, step) else { continue };
            if !filled[n as usize].is_nan() {
                continue;
            }
            filled[n as usize] = grid.heights[n as usize].max(filled[i as usize] + FALL);
            down[n as usize] = i;
            open.push(key(filled[n as usize], n));
        }
    }
    // Every cell's ground, and all that drains into it, passed on down: the furthest from the way out first.
    let cell_area = grid.spacing * grid.spacing;
    let mut area = vec![cell_area; count];
    for &i in order.iter().rev() {
        let to = down[i as usize];
        if to != AWAY {
            area[to as usize] += area[i as usize];
        }
    }
    Flow { filled, down, area, order }
}

/// A river: points along its water from where it starts to where it ends.
#[derive(Clone, Debug, PartialEq)]
pub struct River {
    pub points: Vec<RiverPoint>,
}

/// A point along a river: its water's middle, in metres from the terrain's
/// corner, and how wide its bed's floor is and how deep it runs there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiverPoint {
    pub at: [f32; 3],
    pub width: f32,
    pub depth: f32,
}

impl RiverPoint {
    /// How wide its water is, bank to bank: its floor, and its banks up to
    /// its water, a little into them so the water's edges meet the ground.
    pub fn water_width(&self) -> f32 {
        self.width + self.depth * 2.0 * (WATER - 0.3) / BANKS * 1.1
    }
}

/// How far under its water a river's bed is, as a share of its depth: the
/// water stands a little below its banks.
const WATER: f32 = 0.8;

/// How steeply a river's banks rise from its floor, in metres up a metre
/// out.
const BANKS: f32 = 1.0;

/// A lake: how high its water stands, and boxes of ground it covers, each
/// `[min_x, min_z, max_x, max_z]` in metres from the terrain's corner and
/// how deep its water is at most in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Lake {
    pub level: f32,
    pub boxes: Vec<([f32; 4], f32)>,
}

/// `grid`'s ground with rivers cut into it, and its rivers and lakes.
#[derive(Clone, Debug, PartialEq)]
pub struct Carved {
    pub heights: Vec<f32>,
    pub rivers: Vec<River>,
    pub lakes: Vec<Lake>,
}

impl RiverSettings {
    /// How wide and how deep a river is, in metres, where `area` square
    /// metres drain into it.
    pub fn size(&self, area: f32) -> (f32, f32) {
        let square_kilometres = area / 1e6;
        let width = (self.width.max(0.1) * square_kilometres.sqrt()).clamp(1.0, 250.0);
        let depth = (self.depth.max(0.0) * square_kilometres.sqrt().sqrt()).clamp(0.1, 20.0);
        (width, depth)
    }
}

/// Rivers and lakes on `grid`'s ground as `settings` says, the sea at
/// `sea_level` if it has one: its ground with their beds cut in, and each
/// river and lake.
pub fn carve(grid: &Grid, settings: &RiverSettings, sea_level: Option<f32>) -> Carved {
    let flow = flow(grid, sea_level);
    let cells = grid.cells;
    let count = (cells[0] * cells[1]) as usize;
    let spacing = grid.spacing;
    let lakes = lakes(grid, &flow, sea_level);
    let mut in_lake = vec![false; count];
    for (cells_of, _) in &lakes {
        cells_of.iter().for_each(|&i| in_lake[i as usize] = true);
    }
    let source = settings.source.max(0.01) * 10_000.0;
    let river =
        |i: usize| flow.area[i] >= source && !in_lake[i] && sea_level.is_none_or(|level| flow.filled[i] >= level);
    // Each river's bed cut along its course, under its water, falling as the water falls.
    let rivers = courses(grid, &flow, settings, &|i| river(i as usize));
    let mut heights = grid.heights.clone();
    let mut cut = |at: [f32; 2], bed: f32, width: f32, depth: f32| {
        let half = width * 0.5;
        // Its floor rounded, deepest in its middle; its banks rising from its edge until they meet the ground.
        let reach = half + depth * 2.0 / BANKS + spacing * 2.0;
        let (cx, cz) = (at[0] / spacing, at[1] / spacing);
        let span = reach / spacing;
        let (x0, x1) = (((cx - span).floor() as i32).max(0), ((cx + span).ceil() as i32).min(cells[0] as i32 - 1));
        let (z0, z1) = (((cz - span).floor() as i32).max(0), ((cz + span).ceil() as i32).min(cells[1] as i32 - 1));
        for z in z0..=z1 {
            for x in x0..=x1 {
                let across = ((x as f32 - cx).powi(2) + (z as f32 - cz).powi(2)).sqrt() * spacing;
                if across > reach {
                    continue;
                }
                let shape = match across <= half {
                    true => depth * 0.3 * (across / half.max(1e-3)) * (across / half.max(1e-3)),
                    false => depth * 0.3 + (across - half) * BANKS,
                };
                let i = (z as u32 * cells[0] + x as u32) as usize;
                // Nor under a lake's water or the sea's.
                let under_sea = sea_level.is_some_and(|level| grid.heights[i] < level);
                if !in_lake[i] && !under_sea {
                    heights[i] = heights[i].min(bed + shape);
                }
            }
        }
    };
    for river in &rivers {
        for pair in river.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = ((b.at[0] - a.at[0]).powi(2) + (b.at[2] - a.at[2]).powi(2)).sqrt();
            let steps = (length / (spacing * 0.5)).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let mix = |p: f32, q: f32| p + (q - p) * t;
                let depth = mix(a.depth, b.depth);
                let water = mix(a.at[1], b.at[1]);
                cut(
                    [mix(a.at[0], b.at[0]), mix(a.at[2], b.at[2])],
                    water - depth * WATER,
                    mix(a.width, b.width),
                    depth,
                );
            }
        }
    }
    let lakes = lakes
        .into_iter()
        .map(|(cells_of, level)| Lake { level, boxes: boxes(grid, &flow, &cells_of, level) })
        .collect();
    Carved { heights, rivers, lakes }
}

/// The hollows deep and wide enough to hold lakes: each one's cells, and how
/// high its water stands.
fn lakes(grid: &Grid, flow: &Flow, sea_level: Option<f32>) -> Vec<(Vec<u32>, f32)> {
    let cells = grid.cells;
    let count = (cells[0] * cells[1]) as usize;
    let deep = |i: usize| {
        flow.filled[i] - grid.heights[i] > LAKE_DEPTH * 0.2 && sea_level.is_none_or(|level| flow.filled[i] > level)
    };
    let mut seen = vec![false; count];
    let mut found = Vec::new();
    for start in 0..count {
        if seen[start] || !deep(start) {
            continue;
        }
        // The whole hollow, reached across neighbours as deep.
        let (mut hollow, mut stack) = (Vec::new(), vec![start as u32]);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            hollow.push(i);
            for step in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if let Some(n) = neighbour(cells, i, step)
                    && !seen[n as usize]
                    && deep(n as usize)
                {
                    seen[n as usize] = true;
                    stack.push(n);
                }
            }
        }
        let level = hollow.iter().map(|&i| flow.filled[i as usize]).fold(f32::MIN, f32::max);
        let deepest = hollow.iter().map(|&i| flow.filled[i as usize] - grid.heights[i as usize]).fold(0.0f32, f32::max);
        let area = hollow.len() as f32 * grid.spacing * grid.spacing;
        if deepest >= LAKE_DEPTH && area >= SMALLEST_LAKE {
            hollow.sort_unstable();
            found.push((hollow, level));
        }
    }
    found
}

/// Boxes covering a lake's cells, at `level`: its bounds, halved along their
/// longer side until hardly any holds ground below the water that drains
/// elsewhere, lower, which would show water standing over it; or until they
/// are a few cells across. Its shallow edges, under the same water, count
/// as its own.
fn boxes(grid: &Grid, flow: &Flow, lake: &[u32], level: f32) -> Vec<([f32; 4], f32)> {
    let cells = grid.cells;
    let mut own = vec![false; (cells[0] * cells[1]) as usize];
    lake.iter().for_each(|&i| own[i as usize] = true);
    let bounds = lake.iter().fold([u32::MAX, u32::MAX, 0, 0], |[x0, z0, x1, z1], &i| {
        let (x, z) = (i % cells[0], i / cells[0]);
        [x0.min(x), z0.min(z), x1.max(x), z1.max(z)]
    });
    let mut out = Vec::new();
    let mut stack = vec![bounds];
    while let Some([x0, z0, x1, z1]) = stack.pop() {
        let (mut mine, mut stray, mut deepest) = (0, 0, 0.0f32);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let i = (z * cells[0] + x) as usize;
                let below = level - grid.heights[i];
                match own[i] {
                    true => (mine, deepest) = (mine + 1, deepest.max(below)),
                    false => stray += usize::from(below > 0.05 && flow.filled[i] < level - 0.05),
                }
            }
        }
        if mine == 0 {
            continue;
        }
        let small = x1 - x0 < 4 && z1 - z0 < 4;
        let few = stray <= ((x1 - x0 + 1) * (z1 - z0 + 1) / 100).max(1) as usize;
        if few || small {
            let at = |v: u32| v as f32 * grid.spacing;
            out.push(([at(x0), at(z0), at(x1), at(z1)], deepest + 0.5));
            continue;
        }
        if x1 - x0 >= z1 - z0 {
            let middle = (x0 + x1) / 2;
            stack.extend([[x0, z0, middle, z1], [middle + 1, z0, x1, z1]]);
        } else {
            let middle = (z0 + z1) / 2;
            stack.extend([[x0, z0, x1, middle], [x0, middle + 1, x1, z1]]);
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The rivers' courses: from each cell a river starts at (none of its
/// river's water coming from another river cell) down to where it leaves
/// the ground, meets the sea or a lake, or joins another river, a point
/// every few cells, eased over the cells round it so it runs smoothly.
fn courses(grid: &Grid, flow: &Flow, settings: &RiverSettings, river: &dyn Fn(u32) -> bool) -> Vec<River> {
    let cells = grid.cells;
    let count = (cells[0] * cells[1]) as usize;
    let mut fed = vec![false; count];
    for i in 0..count as u32 {
        let to = flow.down[i as usize];
        if river(i) && to != AWAY && river(to) {
            fed[to as usize] = true;
        }
    }
    let mut traced = vec![false; count];
    let mut rivers = Vec::new();
    // Heads in the order the filling reached them, the highest last, so the same every time.
    for &head in flow.order.iter().rev() {
        if !river(head) || fed[head as usize] {
            continue;
        }
        let mut path = vec![head];
        let mut at = head;
        loop {
            traced[at as usize] = true;
            let to = flow.down[at as usize];
            if to == AWAY {
                break;
            }
            path.push(to);
            if !river(to) || traced[to as usize] {
                break;
            }
            at = to;
        }
        if path.len() < 8 {
            continue;
        }
        let place = |i: u32| [(i % cells[0]) as f32 * grid.spacing, (i / cells[0]) as f32 * grid.spacing];
        let mut points = Vec::new();
        let last = path.len() - 1;
        for (k, &i) in path.iter().enumerate() {
            if k % 4 != 0 && k != last {
                continue;
            }
            // Eased over the cells either side along its course.
            let (from, to) = (k.saturating_sub(3), (k + 3).min(last));
            let mut sum = [0.0f32; 2];
            for &j in &path[from..=to] {
                let [x, z] = place(j);
                sum = [sum[0] + x, sum[1] + z];
            }
            let n = (to - from + 1) as f32;
            let (width, depth) = settings.size(flow.area[i as usize]);
            let water = flow.filled[i as usize] - depth * (1.0 - WATER);
            points.push(RiverPoint { at: [sum[0] / n, water, sum[1] / n], width, depth });
        }
        // Never rising as it runs, though easing moved its points.
        for k in 1..points.len() {
            points[k].at[1] = points[k].at[1].min(points[k - 1].at[1]);
        }
        rivers.push(River { points });
    }
    rivers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{erosion::grid, heights::Heightmap};

    /// A valley running down along z between two hillsides, its floor
    /// falling a metre every 20, with a bowl dug in its upper half.
    fn valley() -> Grid {
        let heights = Heightmap::from_fn(1, 1, 2.0, |x, z| {
            let across = (x - 256.0).abs() * 0.1;
            let bowl = 8.0 * (1.0 - ((x - 256.0).powi(2) + (z - 150.0).powi(2)).sqrt() / 60.0).max(0.0);
            40.0 - z * 0.05 + across - bowl
        });
        grid(&heights)
    }

    #[test]
    fn all_water_drains_away_downhill_and_rivers_only_ever_run_down() {
        let ground = valley();
        let flow = flow(&ground, None);
        // Every cell's water is counted once where it leaves.
        let total: f32 = (0..flow.down.len()).filter(|&i| flow.down[i] == AWAY).map(|i| flow.area[i]).sum();
        let all = ground.heights.len() as f32 * ground.spacing * ground.spacing;
        assert!((total - all).abs() < all * 1e-4, "{total} of {all}");
        // Filled ground never rises from a cell to the one it drains to.
        for (i, &to) in flow.down.iter().enumerate() {
            if to != AWAY {
                assert!(flow.filled[to as usize] < flow.filled[i], "cell {i} drains up");
            }
        }
        let carved = carve(&ground, &RiverSettings { source: 2.0, ..RiverSettings::default() }, None);
        assert!(!carved.rivers.is_empty(), "a river down the valley");
        for river in &carved.rivers {
            for pair in river.points.windows(2) {
                assert!(pair[1].at[1] <= pair[0].at[1], "a river rises: {:?} to {:?}", pair[0].at, pair[1].at);
            }
        }
        // The main river runs down the valley's floor, out at its foot, wider at its end.
        let longest = carved.rivers.iter().max_by_key(|river| river.points.len()).unwrap();
        let (first, last) = (longest.points.first().unwrap(), longest.points.last().unwrap());
        assert!(last.at[2] > 500.0 && (last.at[0] - 256.0).abs() < 20.0, "out at the foot: {:?}", last.at);
        assert!(last.width > first.width, "wider as more drains into it");
        // Its bed under its water all along.
        let place = |v: f32| (v / ground.spacing).round() as u32;
        for point in &longest.points[1..longest.points.len() - 1] {
            let bed = carved.heights[(place(point.at[2]) * ground.cells[0] + place(point.at[0])) as usize];
            assert!(bed < point.at[1], "the bed under its water at {:?}: {bed}", point.at);
        }
        // Its bed cut into the ground, and the hillsides left as they were.
        let cell = |heights: &[f32], x: u32, z: u32| heights[(z * ground.cells[0] + x) as usize];
        let lowest = (110..146).map(|x| cell(&carved.heights, x, 230)).fold(f32::MAX, f32::min);
        assert!(lowest < cell(&ground.heights, 128, 230) - 0.5, "the bed cut down");
        assert_eq!(cell(&carved.heights, 30, 230), cell(&ground.heights, 30, 230), "the hillside as it was");
    }

    #[test]
    fn a_bowl_holds_a_lake_as_high_as_it_fills_before_it_spills() {
        let ground = valley();
        let carved = carve(&ground, &RiverSettings::default(), None);
        let [lake] = carved.lakes.as_slice() else { panic!("one lake: {:?}", carved.lakes.len()) };
        // It fills to where the bowl's rim spills down the valley, a few metres over its floor.
        let floor = ground.heights[(75 * ground.cells[0] + 128) as usize];
        assert!(lake.level > floor + 1.0 && lake.level < floor + 6.0, "{} over {floor}", lake.level);
        // Every box over ground of the lake's own, and none over ground below its water outside it.
        for (b, depth) in &lake.boxes {
            assert!(b[0] <= b[2] && b[1] <= b[3] && *depth > 0.5, "{b:?} {depth}");
            assert!((b[0] - 256.0).abs() < 60.0 && (b[1] - 150.0).abs() < 60.0, "{b:?} round the bowl");
        }
        // Nothing worked out differently the second time.
        assert_eq!(carve(&ground, &RiverSettings::default(), None), carved);
    }

    #[test]
    fn water_reaching_the_sea_goes_no_further_and_no_river_runs_under_it() {
        let ground = valley();
        let sea = 20.0;
        let carved = carve(&ground, &RiverSettings { source: 2.0, ..RiverSettings::default() }, Some(sea));
        for river in &carved.rivers {
            assert!(river.points.iter().all(|p| p.at[1] >= sea - 0.5), "a river under the sea");
        }
        let under = (0..ground.heights.len()).filter(|&i| ground.heights[i] < sea);
        assert!(under.clone().all(|i| carved.heights[i] == ground.heights[i]), "the sea floor left as it was");
    }
}
