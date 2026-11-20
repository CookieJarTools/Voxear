//! Roads as world layers (see `world_layers`): a line of points along the
//! ground, as wide as each point says, laid with a palette or a pattern of
//! blocks, an edge and a kerb each side; laid on the ground as it is, or
//! graded between its points so it climbs no more than a set slope, cut and
//! filled with a slope out each side; across gaps and water as a bridge on
//! piers, through high ground as a tunnel, and with water in it as a canal.
//!
//! Everything is worked out from where a column is and the road's own points,
//! so chunks made apart meet and the same road comes back every time.

use serde::{Deserialize, Serialize};

use super::chunk::{AIR, BlockId};

/// The blocks a road or an area is laid with: a mix, each block as often as
/// its weight, the same every time for a place; or a pattern of entries of
/// the mix by their place from 1, rows along and columns across, repeated,
/// 0 for the mix.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub mix: Vec<(BlockId, u32)>,
    pub pattern: Vec<Vec<u8>>,
}

impl Palette {
    pub fn is_empty(&self) -> bool {
        self.mix.iter().all(|(block, weight)| *block == AIR || *weight == 0)
    }

    /// The block at `[x, z]`, `along` and `across` its road or area in whole
    /// blocks, `seed` its layer's own: air where it has none.
    pub fn pick(&self, seed: u64, [x, z]: [i32; 2], along: i32, across: i32) -> BlockId {
        if !self.pattern.is_empty() {
            let row = &self.pattern[along.rem_euclid(self.pattern.len() as i32) as usize];
            if !row.is_empty() {
                let entry = row[across.rem_euclid(row.len() as i32) as usize] as usize;
                if let Some(&(block, _)) = entry.checked_sub(1).and_then(|at| self.mix.get(at)) {
                    return block;
                }
            }
        }
        let total: u64 = self.mix.iter().map(|(_, weight)| u64::from(*weight)).sum();
        if total == 0 {
            return AIR;
        }
        let mixed = crate::layers::stable_hash(
            &[seed.to_le_bytes(), i64::from(x).to_le_bytes(), i64::from(z).to_le_bytes()].concat(),
        );
        let mut roll = mixed % total;
        for &(block, weight) in &self.mix {
            if roll < u64::from(weight) {
                return block;
            }
            roll -= u64::from(weight);
        }
        AIR
    }

    pub(super) fn bytes(&self, bytes: &mut Vec<u8>) {
        bytes.extend((self.mix.len() as u32).to_le_bytes());
        for (block, weight) in &self.mix {
            bytes.extend(block.to_le_bytes());
            bytes.extend(weight.to_le_bytes());
        }
        for row in &self.pattern {
            bytes.extend((row.len() as u32).to_le_bytes());
            bytes.extend(row);
        }
    }
}

/// How a road climbs where it is not graded: whole blocks, or a slab of its
/// top block on each step up, where the world has slabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Steps {
    #[default]
    Blocks,
    Slabs,
}

/// A road over water and gaps: its deck's block, how many blocks it keeps
/// over water at least, and a pier down to the bed every so many blocks along.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bridge {
    pub deck: BlockId,
    pub over_water: u32,
    pub pier_every: u32,
}

/// A road through ground higher than `cover` blocks over it: cut `height`
/// blocks high, with a roof of `roof` where the ground does not hold one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tunnel {
    pub cover: u32,
    pub height: u32,
    pub roof: BlockId,
}

/// A road, a canal or a path of a world, in its own blocks (see the module's
/// own words).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoadLayer {
    /// What it is known by between one set of layers and the next.
    pub id: u64,
    /// Its points along x, up and z, the first air over the ground where
    /// each was dropped; and how wide it is at each, in blocks.
    pub points: Vec<[f32; 3]>,
    pub widths: Vec<f32>,
    pub palette: Palette,
    /// The block along each edge, and how many blocks its kerb stands over
    /// the road.
    pub edge: BlockId,
    pub kerb: u32,
    /// Graded, the most it climbs a block along, in blocks, its top going
    /// straight between its points' heights; none laid on the ground as it
    /// is.
    pub grade: Option<f32>,
    /// Over how many blocks each side the ground eases back from a graded
    /// road's height.
    pub slope_out: u32,
    pub steps: Steps,
    pub bridge: Option<Bridge>,
    pub tunnel: Option<Tunnel>,
    /// Water in it, as a canal: its block and how many blocks deep.
    pub water: Option<(BlockId, u32)>,
    pub clear_plants: bool,
    pub priority: i32,
}

/// What a road makes of a column it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoadColumn {
    /// The first air over the road's top, or over the ground where it eases
    /// out.
    pub top: i32,
    /// Whether the column is on the road itself, its edge, or eases out.
    pub on: OnRoad,
    pub block: BlockId,
    /// How far along and across the road the column is, in whole blocks.
    pub along: i32,
    pub across: i32,
    /// Whether a slab of its block is laid over it, as the road steps up a
    /// block just ahead.
    pub half: bool,
    /// The first air over the road's own top where it passes nearest.
    pub level: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnRoad {
    Road,
    Edge,
    Out,
}

/// The nearest point of a line of points to `at`: how far, how far along it,
/// which side (positive to the left going along), and the share along the
/// segment, with the segment.
struct Nearest {
    /// Whether it lies past either end, beyond where the road stops.
    beyond: bool,
    distance: f32,
    along: f32,
    side: f32,
    segment: usize,
    share: f32,
}

impl RoadLayer {
    /// How far past its middle it reaches at most, in blocks.
    pub fn reach(&self) -> f32 {
        let widest = self.widths.iter().copied().fold(1.0f32, f32::max);
        widest * 0.5 + self.slope_out as f32 + 1.0
    }

    /// The box everything it changes lies within, along x and z, in blocks.
    pub fn bounds(&self) -> Option<[[i32; 2]; 2]> {
        if self.points.len() < 2 {
            return None;
        }
        let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
        for point in &self.points {
            for (axis, &v) in [point[0], point[2]].iter().enumerate() {
                low[axis] = low[axis].min(v);
                high[axis] = high[axis].max(v);
            }
        }
        if !low.iter().chain(&high).all(|v| v.is_finite()) {
            return None;
        }
        let reach = self.reach() + 1.0;
        Some([low.map(|v| (v - reach).floor() as i32), high.map(|v| (v + reach).ceil() as i32)])
    }

    fn width_at(&self, segment: usize, share: f32) -> f32 {
        let width = |at: usize| self.widths.get(at).or(self.widths.last()).copied().unwrap_or(3.0).max(1.0);
        width(segment) + (width(segment + 1) - width(segment)) * share
    }

    fn nearest(&self, point: [f32; 2]) -> Option<Nearest> {
        let mut found: Option<Nearest> = None;
        let mut walked = 0.0;
        for (segment, pair) in self.points.windows(2).enumerate() {
            let (a, b) = ([pair[0][0], pair[0][2]], [pair[1][0], pair[1][2]]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let length = (ab[0] * ab[0] + ab[1] * ab[1]).sqrt();
            let ap = [point[0] - a[0], point[1] - a[1]];
            let raw = if length > 0.0 { (ap[0] * ab[0] + ap[1] * ab[1]) / (length * length) } else { 0.0 };
            let last = segment + 2 == self.points.len();
            let beyond = segment == 0 && raw < 0.0 || last && raw > 1.0;
            let share = raw.clamp(0.0, 1.0);
            let (dx, dz) = (ap[0] - ab[0] * share, ap[1] - ab[1] * share);
            let distance = (dx * dx + dz * dz).sqrt();
            if found.as_ref().is_none_or(|found| distance < found.distance) {
                let side = (ab[0] * ap[1] - ab[1] * ap[0]).signum();
                found = Some(Nearest { beyond, distance, along: walked + share * length, side, segment, share });
            }
            walked += length;
        }
        found
    }

    /// The first air over its top at a share along a segment: its points'
    /// heights joined straight, steepened no more than its grade allows.
    fn graded_top(&self, segment: usize, share: f32) -> i32 {
        let (a, b) = (self.points[segment], self.points[segment + 1]);
        let mut top = a[1] + (b[1] - a[1]) * share;
        if let Some(grade) = self.grade.filter(|grade| *grade > 0.0) {
            // Climbing faster than the grade between two points, it climbs at the grade from the lower one.
            let run = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
            let most = grade * run;
            if (b[1] - a[1]).abs() > most {
                let low = a[1].min(b[1]);
                let from_low = if b[1] < a[1] { 1.0 - share } else { share };
                top = low + most * from_low;
            }
        }
        top.round() as i32
    }

    /// What it makes of column `[x, z]` whose ground's top is `ground`: none
    /// where it does not reach.
    pub fn shape(&self, [x, z]: [i32; 2], ground: i32) -> Option<RoadColumn> {
        let near = self.nearest([x as f32 + 0.5, z as f32 + 0.5])?;
        let half = self.width_at(near.segment, near.share) * 0.5;
        let along = near.along.floor() as i32;
        let across = (near.side * near.distance + half).floor() as i32;
        let road_top = match self.grade {
            Some(_) => self.graded_top(near.segment, near.share),
            None => ground,
        };
        // Past an end it stops square: only the ground eases out there.
        if near.distance <= half && !near.beyond {
            let edge = near.distance > half - 1.0 && self.edge != AIR;
            let block = match edge {
                true => self.edge,
                false => self.palette.pick(self.id, [x, z], along, across),
            };
            let on = if edge { OnRoad::Edge } else { OnRoad::Road };
            let ahead =
                |by: f32| self.at_along(near.along + by).map(|(segment, share)| self.graded_top(segment, share));
            let steps_up = [1.0, -1.0].into_iter().any(|by| ahead(by).is_some_and(|next| next > road_top));
            let half = self.steps == Steps::Slabs && self.grade.is_some() && steps_up;
            return Some(RoadColumn { top: road_top, on, block, along, across, half, level: road_top });
        }
        // Past its edge a graded road's ground eases back a block a step.
        let steps = ((near.distance - half).ceil() as i32).max(0);
        if self.grade.is_none() || steps > self.slope_out as i32 {
            return (self.clear_plants && steps <= self.slope_out as i32 + 1).then_some(RoadColumn {
                top: ground,
                on: OnRoad::Out,
                block: AIR,
                along,
                across,
                half: false,
                level: road_top,
            });
        }
        let top = ground.clamp(road_top - steps, road_top + steps);
        Some(RoadColumn { top, on: OnRoad::Out, block: AIR, along, across, half: false, level: road_top })
    }

    /// How wide it is, in whole blocks, where it passes nearest column `at`.
    pub fn width_near(&self, [x, z]: [i32; 2]) -> i32 {
        self.nearest([x as f32 + 0.5, z as f32 + 0.5])
            .map_or(0, |near| self.width_at(near.segment, near.share).round() as i32)
    }

    /// The segment and the share along it `along` blocks along the road.
    fn at_along(&self, along: f32) -> Option<(usize, f32)> {
        let mut walked = 0.0;
        for (segment, pair) in self.points.windows(2).enumerate() {
            let length = ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][2] - pair[0][2]).powi(2)).sqrt();
            if along <= walked + length && length > 0.0 {
                return Some((segment, ((along - walked) / length).clamp(0.0, 1.0)));
            }
            walked += length;
        }
        None
    }

    /// Everything it is, as bytes, the same on every machine.
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(self.id.to_le_bytes());
        for point in &self.points {
            point.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        }
        self.widths.iter().for_each(|v| bytes.extend(v.to_le_bytes()));
        self.palette.bytes(&mut bytes);
        bytes.extend(self.edge.to_le_bytes());
        bytes.extend(self.kerb.to_le_bytes());
        bytes.extend(self.grade.unwrap_or(-1.0).to_le_bytes());
        bytes.extend(self.slope_out.to_le_bytes());
        bytes.push(self.steps as u8);
        if let Some(bridge) = self.bridge {
            bytes.extend([1, 0]);
            bytes.extend(bridge.deck.to_le_bytes());
            bytes.extend(bridge.over_water.to_le_bytes());
            bytes.extend(bridge.pier_every.to_le_bytes());
        }
        if let Some(tunnel) = self.tunnel {
            bytes.extend([2, 0]);
            bytes.extend(tunnel.cover.to_le_bytes());
            bytes.extend(tunnel.height.to_le_bytes());
            bytes.extend(tunnel.roof.to_le_bytes());
        }
        if let Some((water, depth)) = self.water {
            bytes.extend([3, 0]);
            bytes.extend(water.to_le_bytes());
            bytes.extend(depth.to_le_bytes());
        }
        bytes.push(u8::from(self.clear_plants));
        bytes.extend(self.priority.to_le_bytes());
        bytes
    }

    /// The point of its middle line nearest `at` along x and z, in blocks,
    /// and how far along it that is.
    pub fn nearest_point(&self, at: [f32; 2]) -> Option<([f32; 3], f32)> {
        let near = self.nearest(at)?;
        let (a, b) = (self.points[near.segment], self.points[near.segment + 1]);
        let lerp = |axis: usize| a[axis] + (b[axis] - a[axis]) * near.share;
        Some(([lerp(0), lerp(1), lerp(2)], near.along))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road() -> RoadLayer {
        RoadLayer {
            id: 5,
            points: vec![[0.0, 10.0, 0.5], [40.0, 10.0, 0.5]],
            widths: vec![5.0, 5.0],
            palette: Palette { mix: vec![(3, 1), (4, 1)], pattern: Vec::new() },
            edge: 7,
            kerb: 1,
            grade: Some(0.25),
            slope_out: 3,
            steps: Steps::Blocks,
            bridge: None,
            tunnel: None,
            water: None,
            clear_plants: true,
            priority: 0,
        }
    }

    #[test]
    fn a_road_is_laid_with_its_mix_and_edges_and_eases_out_each_side() {
        let road = road();
        let middle = road.shape([10, 0], 6).unwrap();
        assert_eq!((middle.top, middle.on), (10, OnRoad::Road));
        assert!(matches!(middle.block, 3 | 4));
        assert_eq!(road.shape([10, 2], 6).unwrap().on, OnRoad::Edge);
        assert_eq!(road.shape([10, 2], 6).unwrap().block, 7);
        // Out from the edge a block down a step until it meets the ground.
        assert_eq!(road.shape([10, 3], 6).unwrap().top, 9);
        assert_eq!(road.shape([10, 4], 6).unwrap().top, 8);
        assert_eq!(road.shape([10, 9], 6), None);
        // Past its end it stops: the ground eases out from its height there, with no kerb across it.
        let past = road.shape([41, 0], 6).unwrap();
        assert_eq!((past.on, past.top), (OnRoad::Out, 10));
        // The same block every time.
        assert_eq!(road.shape([10, 0], 6), road.shape([10, 0], 6));
    }

    #[test]
    fn a_graded_road_climbs_no_faster_than_its_grade_and_a_pattern_repeats() {
        let steep = RoadLayer { points: vec![[0.0, 0.0, 0.5], [20.0, 20.0, 0.5]], ..road() };
        let tops: Vec<i32> = (0..20).map(|x| steep.shape([x, 0], 0).unwrap().top).collect();
        assert!(tops.windows(4).all(|run| run[3] - run[0] <= 1), "{tops:?}");
        assert!(tops[19] <= 5);
        let patterned = RoadLayer {
            palette: Palette { mix: vec![(3, 1), (4, 1)], pattern: vec![vec![1, 2]] },
            edge: AIR,
            ..road()
        };
        let row: Vec<BlockId> = (-2..=2).map(|z| patterned.shape([10, z], 10).unwrap().block).collect();
        assert!(row.windows(2).all(|pair| pair[0] != pair[1]), "{row:?}");
        let followed = RoadLayer { grade: None, ..road() };
        assert_eq!(followed.shape([10, 0], 6).unwrap().top, 6);
    }
}
