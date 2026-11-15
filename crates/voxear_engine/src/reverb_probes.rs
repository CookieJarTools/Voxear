//! Reverb probes: how long sound rings at points through a level, baked
//! from the editor so a game can hear its rooms ring without tracing rays
//! as it plays: on machines without the GPU time to spare, or with
//! ray-traced audio off. A Reverb Probe is a box on its Obj, filled with
//! probes `spacing` metres apart; each is traced on the GPU as the
//! listener is with ray-traced audio on (see [`crate::sound_tracing`]),
//! with many more rays, and keeps its reverb times.
//!
//! They are saved beside the level as its `.reverb` file (see
//! [`reverb_file`]), small to send to players joining: a short header,
//! then each box's lowest probe, spacing and probes along each side, then
//! each probe's three times in milliseconds, 0 for a probe inside
//! something, which heard nothing back. Builds ship it with its level.
//!
//! As the game plays, the listener's reverb comes from the probes round
//! it, blended by how near each is, those inside something left out.

use anyhow::{Result, bail, ensure};
use voxear_audio::ReverbTimes;
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_scripting::ComponentKind;

const MAGIC: [u8; 4] = *b"VXRV";
const VERSION: u16 = 1;
/// The most probes along a box's side, and in a level, so a broken file or
/// a box made far too big cannot ask for more than any level needs.
pub const MOST_ALONG: u32 = 64;
pub const MOST_PROBES: usize = 16_384;

/// A box of reverb probes on its Obj, round the Obj's place, `size` metres
/// along each axis of the world, its probes `spacing` metres apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbProbe {
    pub size: [f32; 3],
    pub spacing: f32,
}

impl Component for ReverbProbe {}

impl Default for ReverbProbe {
    fn default() -> Self {
        Self { size: [10.0, 4.0, 10.0], spacing: 2.0 }
    }
}

/// One box's probes: the lowest, how far apart, and how many along each
/// axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeGrid {
    pub lowest: Vec3,
    pub spacing: f32,
    pub along: [u32; 3],
}

impl ProbeGrid {
    /// The probes of a box `size` across round `middle`, `spacing` apart,
    /// none nearer its sides than half that, at least one along each axis.
    pub fn new(middle: Vec3, size: [f32; 3], spacing: f32) -> Self {
        let spacing = spacing.max(0.25);
        let along = size.map(|side| ((side.max(0.0) / spacing).floor() as u32).clamp(1, MOST_ALONG));
        let span = Vec3::new(along[0] as f32 - 1.0, along[1] as f32 - 1.0, along[2] as f32 - 1.0) * spacing;
        Self { lowest: middle - span * 0.5, spacing, along }
    }

    pub fn count(&self) -> usize {
        self.along.iter().map(|&n| n as usize).product()
    }

    /// Every probe's place, x fastest, then y, then z.
    pub fn spots(&self) -> Vec<Vec3> {
        let [x, y, z] = self.along;
        (0..z)
            .flat_map(|k| (0..y).flat_map(move |j| (0..x).map(move |i| [i, j, k])))
            .map(|[i, j, k]| self.lowest + Vec3::new(i as f32, j as f32, k as f32) * self.spacing)
            .collect()
    }

    /// Whether `at` is within the box its probes fill, and half a spacing
    /// round it.
    fn holds(&self, at: Vec3) -> bool {
        let from = (at - self.lowest) / self.spacing;
        (0..3).all(|axis| from[axis] >= -0.5 && from[axis] <= self.along[axis] as f32 - 0.5)
    }
}

/// A level's baked reverb probes: each box's grid, and each of its probes'
/// times, none inside something.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedReverb {
    pub boxes: Vec<(ProbeGrid, Vec<Option<ReverbTimes>>)>,
}

/// Where a level's baked reverb probes are kept: beside it, named as it is,
/// as `Levels/Main.level`'s in `Levels/Main.reverb`.
pub fn reverb_file(level: &str) -> String {
    let stem = level.strip_suffix(".level").unwrap_or(level);
    format!("{stem}.reverb")
}

/// The Reverb Probes in `world`, each Obj's grid, to bake.
pub fn grids(world: &World) -> Vec<(Entity, ProbeGrid)> {
    let probes =
        world.query_ref::<(Entity, &ReverbProbe, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
    let mut found: Vec<_> = probes
        .into_iter()
        .map(|(entity, probe, local, placed)| {
            let middle = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            (entity, ProbeGrid::new(middle, probe.size, probe.spacing))
        })
        .collect();
    found.sort_by_key(|(entity, _)| entity.to_bits());
    found
}

impl BakedReverb {
    pub fn probes(&self) -> usize {
        self.boxes.iter().map(|(_, times)| times.len()).sum()
    }

    /// How long sound rings at `at`: blended from the probes round it of
    /// the smallest box that holds it, by how near each is, those inside
    /// something left out. None outside every box, or where every probe
    /// round it is inside something.
    pub fn times_at(&self, at: Vec3) -> Option<ReverbTimes> {
        let holding = self.boxes.iter().filter(|(grid, _)| grid.holds(at));
        let (grid, times) = holding.min_by_key(|(grid, _)| grid.count())?;
        let from = (at - grid.lowest) / grid.spacing;
        let mut sum = [0.0f32; 3];
        let mut weights = 0.0f32;
        for corner in 0..8u32 {
            let mut index = 0usize;
            let mut weight = 1.0f32;
            let mut stride = 1usize;
            for axis in 0..3 {
                let most = grid.along[axis] as f32 - 1.0;
                let place = from[axis].clamp(0.0, most);
                let low = place.floor().min((most - 1.0).max(0.0));
                let part = place - low;
                let up = corner >> axis & 1 == 1;
                let cell = if up { (low + 1.0).min(most) } else { low };
                weight *= if up { part } else { 1.0 - part };
                index += cell as usize * stride;
                stride *= grid.along[axis] as usize;
            }
            if weight <= 0.0 {
                continue;
            }
            if let Some(Some(probe)) = times.get(index) {
                for band in 0..3 {
                    sum[band] += probe.0[band] * weight;
                }
                weights += weight;
            }
        }
        (weights > 1e-4).then(|| ReverbTimes(sum.map(|s| s / weights)))
    }

    /// The file's bytes, as [`read`](Self::read) takes them.
    pub fn write(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.boxes.len() * 22 + self.probes() * 6);
        bytes.extend(MAGIC);
        bytes.extend(VERSION.to_le_bytes());
        bytes.extend((self.boxes.len() as u16).to_le_bytes());
        for (grid, _) in &self.boxes {
            for v in [grid.lowest.x, grid.lowest.y, grid.lowest.z, grid.spacing] {
                bytes.extend(v.to_le_bytes());
            }
            for n in grid.along {
                bytes.extend((n as u16).to_le_bytes());
            }
        }
        for (_, times) in &self.boxes {
            for probe in times {
                let ms = probe.map_or([0; 3], |t| t.0.map(|s| (s * 1000.0).round().clamp(1.0, 65_535.0) as u16));
                for n in ms {
                    bytes.extend(n.to_le_bytes());
                }
            }
        }
        bytes
    }

    pub fn read(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 8 && bytes[..4] == MAGIC, "not a reverb probes file");
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != VERSION {
            bail!("reverb probes of version {version}, which this Voxear does not read; bake them again");
        }
        let count = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let mut at = 8;
        let mut take = |n: usize| -> Result<&[u8]> {
            ensure!(at + n <= bytes.len(), "the reverb probes file ends early");
            at += n;
            Ok(&bytes[at - n..at])
        };
        let mut grids = Vec::with_capacity(count);
        for _ in 0..count {
            let numbers = take(16)?;
            let f = |i: usize| f32::from_le_bytes(numbers[i * 4..i * 4 + 4].try_into().expect("four bytes"));
            let (lowest, spacing) = (Vec3::new(f(0), f(1), f(2)), f(3));
            let sides = take(6)?;
            let along = [0, 1, 2].map(|i| u32::from(u16::from_le_bytes([sides[i * 2], sides[i * 2 + 1]])));
            ensure!(along.iter().all(|&n| (1..=MOST_ALONG).contains(&n)), "a box of reverb probes out of shape");
            ensure!(spacing.is_finite() && spacing > 0.0 && lowest.is_finite(), "a box of reverb probes out of place");
            grids.push(ProbeGrid { lowest, spacing, along });
        }
        let total: usize = grids.iter().map(ProbeGrid::count).sum();
        ensure!(total <= MOST_PROBES, "more reverb probes than a level holds");
        let mut boxes = Vec::with_capacity(count);
        for grid in grids {
            let mut times = Vec::with_capacity(grid.count());
            for _ in 0..grid.count() {
                let n = take(6)?;
                let ms = [0, 1, 2].map(|i| u16::from_le_bytes([n[i * 2], n[i * 2 + 1]]));
                times.push((ms != [0; 3]).then(|| ReverbTimes(ms.map(|ms| f32::from(ms) / 1000.0))));
            }
            boxes.push((grid, times));
        }
        Ok(Self { boxes })
    }
}

/// Sets what answers `ReverbProbe` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::ReverbProbe, values);
}

// What `values` reads or writes, as `ReverbProbe` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const SIZE: i32 = 3;
const SPACING: i32 = 4;

/// Reads value `what` of `entity`'s Reverb Probe, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return Some([f32::from(u8::from(world.has::<ReverbProbe>(entity))), 0.0, 0.0, 0.0]),
        (ADD, Some(_)) => {
            if !world.has::<ReverbProbe>(entity) {
                world.insert(entity, ReverbProbe::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<ReverbProbe>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let probe = world.get_mut::<ReverbProbe>(entity)?;
    match written {
        None => match what {
            SIZE => Some([probe.size[0], probe.size[1], probe.size[2], 0.0]),
            SPACING => Some([probe.spacing, 0.0, 0.0, 0.0]),
            _ => None,
        },
        Some([x, y, z, w]) => {
            match what {
                SIZE => probe.size = [x, y, z].map(|side| side.max(0.0)),
                SPACING => probe.spacing = x.max(0.25),
                _ => return None,
            }
            Some([x, y, z, w])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baked() -> BakedReverb {
        let grid = ProbeGrid::new(Vec3::ZERO, [4.0, 2.0, 2.0], 2.0);
        // Two probes along x: a hall's long ring on the left, a closet's short one on the right.
        assert_eq!(grid.along, [2, 1, 1]);
        let times = vec![Some(ReverbTimes([2.0, 1.6, 1.0])), Some(ReverbTimes([0.4, 0.3, 0.2]))];
        BakedReverb { boxes: vec![(grid, times)] }
    }

    #[test]
    fn a_levels_reverb_probes_are_kept_beside_it() {
        assert_eq!(reverb_file("Levels/Main.level"), "Levels/Main.reverb");
    }

    #[test]
    fn baked_probes_load_as_written_and_blend_between_them() {
        let baked = baked();
        let bytes = baked.write();
        assert_eq!(bytes.len(), 8 + 22 + 2 * 6, "small to send");
        let read = BakedReverb::read(&bytes).expect("reads back");
        assert_eq!(read, baked);
        let left = read.times_at(Vec3::new(-1.0, 0.0, 0.0)).unwrap();
        assert!((left.0[1] - 1.6).abs() < 1e-3, "on the left probe: {left:?}");
        let middle = read.times_at(Vec3::ZERO).unwrap();
        assert!((middle.0[1] - 0.95).abs() < 1e-3, "halfway between: {middle:?}");
        assert!(read.times_at(Vec3::new(10.0, 0.0, 0.0)).is_none(), "outside the box");
        assert!(BakedReverb::read(b"VXRV\x09\x00\x00\x00").is_err(), "another version");
        assert!(BakedReverb::read(&bytes[..bytes.len() - 1]).is_err(), "cut short");
    }

    #[test]
    fn probes_inside_something_are_left_out_of_the_blend() {
        let mut baked = baked();
        baked.boxes[0].1[1] = None;
        let read = BakedReverb::read(&baked.write()).unwrap();
        assert_eq!(read.boxes[0].1[1], None);
        let middle = read.times_at(Vec3::ZERO).unwrap();
        assert!((middle.0[1] - 1.6).abs() < 1e-3, "only the probe that heard: {middle:?}");
    }

    #[test]
    fn a_box_fills_with_probes_spaced_apart_round_its_middle() {
        let grid = ProbeGrid::new(Vec3::new(10.0, 0.0, 0.0), [9.0, 1.0, 4.0], 3.0);
        assert_eq!(grid.along, [3, 1, 1]);
        let spots = grid.spots();
        assert_eq!(spots, vec![Vec3::new(7.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(13.0, 0.0, 0.0)]);
    }

    #[test]
    fn a_script_adds_a_reverb_probe_and_sizes_it() {
        let mut world = World::new();
        let hall = world.spawn(());
        values(&mut world, hall, ADD, Some([0.0; 4]));
        values(&mut world, hall, SIZE, Some([20.0, -1.0, 8.0, 0.0]));
        values(&mut world, hall, SPACING, Some([0.0; 4]));
        assert_eq!(*world.get::<ReverbProbe>(hall).unwrap(), ReverbProbe { size: [20.0, 0.0, 8.0], spacing: 0.25 });
        assert!(values(&mut world, hall, REMOVE, Some([0.0; 4])).is_some());
        assert_eq!(values(&mut world, hall, HAS, None), Some([0.0; 4]));
    }
}
