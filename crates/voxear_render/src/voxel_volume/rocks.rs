//! Floating rocks: jagged islands of rock hanging in the air over an endless
//! volume's ground, scattered as far as it is seen, each drifting and
//! turning slowly on its own.
//!
//! The places a rock may hang are a grid `spacing` apart over the ground,
//! each with one or none as a hash of its cell says, so the same settings
//! hang the same rocks over the same ground every time. Most are small and a
//! few are huge. Each rock is a volume of a set size of its own, its voxels
//! made on a thread once it is among the `most` that matter most round the
//! camera, the largest and nearest first, and let go once it is not. It is
//! drawn turned and moved as its drift says at the time, as a felled tree's
//! piece is.
//!
//! A rock's shape is a few lumps, each the inside of a dozen planes leaning
//! every way, so its faces are flat and its edges sharp, one of them drawn
//! out downwards into a jagged tail, and crumbled over by noise. Only its
//! shell is kept, three voxels deep, which is all a ray ever meets.

use voxear_core::math::{Quat, Vec3};

use super::bricks::{BRICK, Builder, Packed};
use super::{VoxelVolume, kind_byte};

/// The most rocks drawn at once, of any volume.
pub const MOST_ROCKS: usize = 40;

/// Floating rocks over an endless volume's ground.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoxelRocks {
    /// How far apart the places a rock may hang are, in metres.
    pub spacing: f32,
    /// How many of those places have a rock, from 0 to 1.
    pub how_often: f32,
    /// How far across a rock is, in metres, from the smallest to the
    /// largest. Most are small; a few are near the largest.
    pub size: [f32; 2],
    /// How high over the ground under it a rock's middle hangs, in metres,
    /// from the lowest to the highest. A rock's tail never reaches the ground.
    pub height: [f32; 2],
    /// How far a rock drifts to and fro, in metres: the largest this far,
    /// smaller ones further.
    pub drift: f32,
    /// How fast the largest rocks turn, in degrees a second; smaller ones
    /// turn faster, some one way and some the other.
    pub turn: f32,
    /// How small a rock's voxels are: this many make a metre.
    pub voxels_per_metre: u32,
    /// How much of a rock is cracked with glowing seams, from 0 for none to
    /// 1 for many.
    pub embers: f32,
    /// The rock's colour, in sRGB from 0 to 255. Its faces are shades of it,
    /// paler where ash lies on top.
    pub colour: [u8; 3],
    /// The most rocks drawn at once, the largest and nearest first, up to 40.
    pub most: u32,
    /// Two volumes' rocks alike but for their seed hang in other places.
    pub seed: u32,
}

impl Default for VoxelRocks {
    fn default() -> Self {
        Self {
            spacing: 90.0,
            how_often: 0.45,
            size: [3.0, 60.0],
            height: [12.0, 110.0],
            drift: 3.0,
            turn: 0.6,
            voxels_per_metre: 8,
            embers: 0.3,
            colour: [52, 45, 42],
            most: 32,
            seed: 0,
        }
    }
}

/// A rock that hangs at a place of the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rock {
    /// Its place in the grid, which names it.
    pub cell: [i32; 2],
    /// Where its middle hangs over the ground, in metres from the volume's
    /// corner along X and Z.
    pub at: [f32; 2],
    /// How far across it is, and how high its middle hangs over the ground.
    pub size: f32,
    pub lift: f32,
    pub seed: u32,
}

/// The rocks within `reach` metres of `camera` (in metres from the volume's
/// corner, along X and Z) that matter most, the largest and nearest first,
/// at most `most`; those in `kept` count as a little nearer, so a rock does
/// not come and go as the camera wanders to and fro.
pub(crate) fn round(rocks: &VoxelRocks, camera: [f32; 2], reach: f32, most: usize, kept: &[[i32; 2]]) -> Vec<Rock> {
    let spacing = rocks.spacing.max(8.0);
    let cells = (reach / spacing).ceil() as i32 + 1;
    let middle = [(camera[0] / spacing).floor() as i32, (camera[1] / spacing).floor() as i32];
    let mut found: Vec<(f32, Rock)> = Vec::new();
    for dz in -cells..=cells {
        for dx in -cells..=cells {
            let cell = [middle[0] + dx, middle[1] + dz];
            let Some(rock) = rock_in(rocks, cell) else { continue };
            let away = ((rock.at[0] - camera[0]).powi(2) + (rock.at[1] - camera[1]).powi(2)).sqrt();
            if away > reach {
                continue;
            }
            let held = if kept.contains(&cell) { 1.3 } else { 1.0 };
            // How large it looks, more or less: what is near and large first.
            let seen = rock.size / (away - rock.size * 0.5).max(4.0) * held;
            found.push((seen, rock));
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(most.min(MOST_ROCKS)).map(|(_, rock)| rock).collect()
}

/// The rock in `cell`, if it has one.
pub(crate) fn rock_in(rocks: &VoxelRocks, cell: [i32; 2]) -> Option<Rock> {
    let seed = hash3(cell[0] as u32, cell[1] as u32, rocks.seed ^ 0x5eed_0f0c);
    let unit = |n: u32| (hash3(seed, n, 77) >> 8) as f32 / (1u32 << 24) as f32;
    if unit(0) >= rocks.how_often.clamp(0.0, 1.0) {
        return None;
    }
    let spacing = rocks.spacing.max(8.0);
    let at = [(cell[0] as f32 + 0.15 + 0.7 * unit(1)) * spacing, (cell[1] as f32 + 0.15 + 0.7 * unit(2)) * spacing];
    let [small, large] = [rocks.size[0].max(0.5), rocks.size[1].max(rocks.size[0].max(0.5))];
    // Most small, a few large: the cube of an even spread.
    let size = small + (large - small) * unit(3).powi(3);
    let [low, high] = [rocks.height[0], rocks.height[1].max(rocks.height[0])];
    // The tail hangs a rock's size under its middle; it never touches the ground.
    let lift = (low + (high - low) * unit(4)).max(size * 1.1 + 3.0);
    Some(Rock { cell, at, size, lift, seed })
}

/// What a rock is made into: its own volume of a set size, its voxels and
/// its palette.
pub(crate) fn build(rocks: &VoxelRocks, rock: &Rock) -> (VoxelVolume, Packed, Vec<[u8; 4]>) {
    let per = rocks.voxels_per_metre.clamp(2, 32);
    let voxel = 1.0 / per as f32;
    let shape = Shape::new(rock);
    let size = shape.extent;
    let volume = VoxelVolume { size, voxels_per_metre: per, roughness: 0.9, ground: None, ..VoxelVolume::default() };
    let cells = volume.voxel_size();
    let palette = palette(rocks.colour);
    let mut builder = Builder::default();
    // Bricks far from the surface are passed over at a look at their middles: the shape changes by at most 1.6
    // metres a metre, so a brick whose middle is further than that times its half diagonal holds no shell.
    let half = BRICK as f32 * voxel * 0.5;
    let reach = half * 3f32.sqrt() * 1.6;
    let shell = 3.0 * voxel;
    let bricks = cells.map(|n| (n as i32 + BRICK - 1) / BRICK);
    let ember_band = 0.012 + 0.035 * rocks.embers.clamp(0.0, 1.0);
    for bz in 0..bricks[2] {
        for by in 0..bricks[1] {
            for bx in 0..bricks[0] {
                let middle = Vec3::new(bx as f32, by as f32, bz as f32) * (BRICK as f32 * voxel) + Vec3::splat(half);
                let d = shape.distance(middle);
                if d > reach || d < -shell - reach {
                    continue;
                }
                for z in 0..BRICK {
                    for y in 0..BRICK {
                        for x in 0..BRICK {
                            let cell = [bx * BRICK + x, by * BRICK + y, bz * BRICK + z];
                            if (0..3).any(|a| cell[a] >= cells[a] as i32) {
                                continue;
                            }
                            let p = (Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + 0.5) * voxel;
                            let d = shape.distance(p);
                            if d > 0.0 || d < -shell {
                                continue;
                            }
                            builder.set(cell, shape.material(p, voxel, rocks.embers > 0.0, ember_band));
                        }
                    }
                }
            }
        }
    }
    let packed = builder.pack(cells, &palette);
    (volume, packed, palette)
}

/// Where a rock's volume stands at `time` seconds, and how it is turned:
/// its corner in metres from the endless volume's corner, its middle having
/// hung over ground `ground` metres over the volume's floor.
pub(crate) fn pose(rocks: &VoxelRocks, rock: &Rock, extent: Vec3, ground: f32, time: f32) -> (Vec3, Quat) {
    let unit = |n: u32| (hash3(rock.seed, n, 191) >> 8) as f32 / (1u32 << 24) as f32;
    // Small rocks drift further and turn faster than great ones.
    let small = (20.0 / rock.size.max(1.0)).clamp(0.5, 3.0);
    let t = time as f64;
    let wave = |rate: f32, phase: f32| ((t * rate as f64 + phase as f64 * std::f64::consts::TAU).sin()) as f32;
    let drift = rocks.drift.max(0.0) * small;
    let middle = Vec3::new(
        rock.at[0] + drift * wave(0.031 + 0.02 * unit(1), unit(2)),
        ground + rock.lift + drift * 0.35 * wave(0.05 + 0.03 * unit(3), unit(4)),
        rock.at[1] + drift * wave(0.027 + 0.02 * unit(5), unit(6)),
    );
    let way = if unit(7) < 0.5 { -1.0 } else { 1.0 };
    let spin = (rocks.turn * small * (0.5 + unit(8))).to_radians() * way;
    let yaw = (unit(9) * std::f64::consts::TAU as f32) + ((t * spin as f64) % std::f64::consts::TAU) as f32;
    // A slow lean back and forth, as a thing that hangs on nothing.
    let lean = |rate: f32, phase: f32| (4.0 * small.min(1.5)).to_radians() * wave(rate, phase);
    let turn = Quat::from_rotation_y(yaw)
        * Quat::from_rotation_x(lean(0.043, unit(10)) + (unit(11) - 0.5) * 0.3)
        * Quat::from_rotation_z(lean(0.037, unit(12)) + (unit(13) - 0.5) * 0.3);
    // The volume turns about its corner, so the corner is set where it keeps the rock's middle at its middle.
    let corner = middle - turn * Shape::middle_of(extent);
    (corner, turn)
}

/// The palette's places: four shades of rock, ash, and glowing seams.
const SHADES: u8 = 4;
const ASH: u8 = SHADES + 1;
const EMBER: u8 = SHADES + 2;

fn palette(colour: [u8; 3]) -> Vec<[u8; 4]> {
    let shade = |k: f32| {
        let [r, g, b] = colour.map(|c| (f32::from(c) * k).round().clamp(0.0, 255.0) as u8);
        [r, g, b, 255]
    };
    let mut palette: Vec<[u8; 4]> = [0.72, 0.86, 1.0, 1.16].into_iter().map(shade).collect();
    let ash = colour.map(|c| (f32::from(c) * 1.5 + 24.0).min(255.0) as u8);
    palette.push([ash[0], ash[1], ash[2], 255]);
    palette.push([255, 104, 32, kind_byte(255, 11)]);
    palette
}

/// A lump: the inside of planes leaning every way round its middle.
struct Lump {
    middle: Vec3,
    /// How much it is drawn out downwards: 1 for not.
    stretch: f32,
    planes: Vec<(Vec3, f32)>,
}

impl Lump {
    fn distance(&self, p: Vec3) -> f32 {
        let mut q = p - self.middle;
        q.y /= self.stretch;
        self.planes.iter().fold(f32::MIN, |d, (normal, out)| d.max(q.dot(*normal) - out))
    }
}

/// A rock's shape, in metres from its volume's corner.
struct Shape {
    lumps: Vec<Lump>,
    /// How large its volume is, in metres.
    extent: Vec3,
    size: f32,
    seed: u32,
}

impl Shape {
    fn new(rock: &Rock) -> Self {
        let s = rock.size.max(0.5);
        // Room for its body and, under it, its tails, nearly as deep as it is wide.
        let extent = Vec3::new(s * 1.2, s * 1.55, s * 1.2);
        let middle = Self::middle_of(extent);
        let mut n = 0u32;
        let mut unit = || {
            n += 1;
            (hash3(rock.seed, n, 1013) >> 8) as f32 / (1u32 << 24) as f32
        };
        let lump = |at: Vec3, radius: f32, stretch: f32, flat_top: bool, unit: &mut dyn FnMut() -> f32| {
            let mut planes = Vec::new();
            for _ in 0..18 {
                // Even over the sphere, then each plane its own way out: some cut deep, so faces are uneven.
                let y = unit() * 2.0 - 1.0;
                let a = unit() * std::f32::consts::TAU;
                let r = (1.0 - y * y).max(0.0).sqrt();
                let normal = Vec3::new(r * a.cos(), y, r * a.sin());
                planes.push((normal, radius * (0.66 + 0.5 * unit())));
            }
            if flat_top {
                // A broken table on top, tilted.
                let tilt = Vec3::new(unit() - 0.5, 2.0, unit() - 0.5).normalize();
                planes.push((tilt, radius * (0.25 + 0.25 * unit())));
            }
            Lump { middle: at, stretch, planes }
        };
        // Some wide islands, some tall shards.
        let squash = 0.55 + 0.5 * unit();
        let mut lumps = vec![lump(middle, s * 0.5, squash, true, &mut unit)];
        // Shoulders off to the sides, at their own heights.
        for _ in 0..2 {
            let side = Vec3::new(unit() - 0.5, unit() * 0.3 - 0.2, unit() - 0.5) * s * 0.6;
            lumps.push(lump(middle + side, s * (0.16 + 0.14 * unit()), 0.7 + 0.5 * unit(), true, &mut unit));
        }
        // Tails: drawn out downwards to ragged points, the first the longest.
        for (k, length) in [0.36f32, 0.26, 0.18].into_iter().enumerate() {
            let spread = if k == 0 { 0.15 } else { 0.5 };
            let under = Vec3::new((unit() - 0.5) * s * spread, -s * length, (unit() - 0.5) * s * spread);
            lumps.push(lump(middle + under, s * (0.2 - 0.04 * k as f32), 2.6 + unit(), false, &mut unit));
        }
        Self { lumps, extent, size: s, seed: rock.seed }
    }

    /// Where a rock's middle is in its volume, `extent` large.
    fn middle_of(extent: Vec3) -> Vec3 {
        Vec3::new(extent.x * 0.5, extent.y * 0.7, extent.z * 0.5)
    }

    fn distance(&self, p: Vec3) -> f32 {
        let body = self.lumps.iter().fold(f32::MAX, |d, lump| d.min(lump.distance(p)));
        // Crumbled: broad dents and knobs, then fine pits.
        let broad = value_noise(p / (self.size * 0.12), self.seed) * self.size * 0.02;
        let fine = value_noise(p / 0.5, self.seed ^ 0x9e37) * 0.1;
        body + broad + fine
    }

    /// What the shell is made of at `p`: ash where the open air is over it,
    /// glowing seams where a crack runs through, and shades of rock.
    fn material(&self, p: Vec3, voxel: f32, embers: bool, band: f32) -> u8 {
        if embers {
            let crack = value_noise(p / (self.size * 0.22 + 1.0), self.seed ^ 0x51a7);
            let crack2 = value_noise(p / (self.size * 0.07 + 0.5), self.seed ^ 0x3c1);
            if crack.abs() < band && crack2 > -0.1 {
                return EMBER;
            }
        }
        if self.distance(p + Vec3::Y * voxel * 2.0) > 0.0 && self.distance(p + Vec3::Y * voxel * 5.0) > 0.0 {
            let up = self.distance(p + Vec3::Y * voxel) - self.distance(p - Vec3::Y * voxel);
            if up > voxel * 1.2 {
                return ASH;
            }
        }
        let n = value_noise(p / 1.7, self.seed ^ 0x77) * 0.5 + value_noise(p / 0.35, self.seed ^ 0x99) * 0.5;
        // Bands down its height too, as layered rock has.
        let band_at = (p.y / (self.size * 0.09 + 0.4) + n * 0.8).floor();
        let shade = (hash3(band_at as i32 as u32, self.seed, 5) % 3) as f32 + n * 1.5;
        1 + (shade.round() as i32).clamp(0, SHADES as i32 - 1) as u8
    }
}

/// A whole-number hash of three numbers.
fn hash3(a: u32, b: u32, c: u32) -> u32 {
    let mut v = [a.wrapping_mul(1664525).wrapping_add(1013904223), b.wrapping_mul(22695477).wrapping_add(1), c];
    v[0] = v[0].wrapping_add(v[1].wrapping_mul(v[2]));
    v[1] = v[1].wrapping_add(v[2].wrapping_mul(v[0]));
    v[2] = v[2].wrapping_add(v[0].wrapping_mul(v[1]));
    v[0] ^= v[0] >> 16;
    v[1] ^= v[1] >> 16;
    v[2] ^= v[2] >> 16;
    v[0].wrapping_add(v[1].wrapping_mul(v[2]))
}

/// Value noise through space, from -1 to 1, smooth between whole numbers.
fn value_noise(p: Vec3, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let w = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let at = |x: i32, y: i32, z: i32| {
        let h = hash3((cell.x as i32 + x) as u32, (cell.y as i32 + y) as u32, (cell.z as i32 + z) as u32 ^ seed);
        (h >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(at(0, 0, 0), at(1, 0, 0), w.x);
    let x10 = lerp(at(0, 1, 0), at(1, 1, 0), w.x);
    let x01 = lerp(at(0, 0, 1), at(1, 0, 1), w.x);
    let x11 = lerp(at(0, 1, 1), at(1, 1, 1), w.x);
    lerp(lerp(x00, x10, w.y), lerp(x01, x11, w.y), w.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_settings_hang_the_same_rocks_and_another_seed_others() {
        let rocks = VoxelRocks::default();
        let here = round(&rocks, [0.0, 0.0], 600.0, 32, &[]);
        assert_eq!(here, round(&rocks, [0.0, 0.0], 600.0, 32, &[]));
        assert!(here.len() >= 16, "a few dozen places hold rocks: {}", here.len());
        let other = round(&VoxelRocks { seed: 9, ..rocks }, [0.0, 0.0], 600.0, 32, &[]);
        assert_ne!(
            here.iter().map(|rock| rock.cell).collect::<Vec<_>>(),
            other.iter().map(|rock| rock.cell).collect::<Vec<_>>()
        );
        // Over every place, most are small; the nearest and largest are drawn first.
        let sizes: Vec<f32> = (-20..20)
            .flat_map(|x| (-20..20).map(move |z| [x, z]))
            .filter_map(|cell| rock_in(&rocks, cell))
            .map(|rock| rock.size)
            .collect();
        let small = sizes.iter().filter(|size| **size < 20.0).count();
        assert!(small * 2 > sizes.len(), "most rocks are small: {small} of {}", sizes.len());
        assert!(sizes.iter().any(|size| *size > 45.0), "a few are huge");
        assert!(here.iter().all(|rock| rock.lift > rock.size * 1.1), "every tail clears the ground");
    }

    #[test]
    fn a_rock_is_a_shell_of_rock_with_ash_on_top_and_glowing_seams() {
        let rocks = VoxelRocks { embers: 1.0, ..VoxelRocks::default() };
        let rock = Rock { cell: [3, 4], at: [0.0, 0.0], size: 12.0, lift: 30.0, seed: 1234 };
        let (volume, packed, palette) = build(&rocks, &rock);
        assert_eq!(palette.len(), EMBER as usize);
        let cells = volume.voxel_size();
        let mut counts = [0usize; 8];
        // A line down through the middle meets rock, and inside it the shell leaves it hollow.
        let middle = Shape::middle_of(volume.size) * rocks.voxels_per_metre as f32;
        let mut column = Vec::new();
        for y in 0..cells[1] as i32 {
            column.push(packed.get([middle.x as i32, y, middle.z as i32]));
        }
        let solid = column.iter().filter(|m| **m != 0).count();
        assert!(solid > 0 && solid < column.len() / 2, "a hollow shell, met on the way: {solid} of {}", column.len());
        for z in 0..cells[2] as i32 {
            for x in 0..cells[0] as i32 {
                for y in 0..cells[1] as i32 {
                    counts[packed.get([x, y, z]) as usize] += 1;
                }
            }
        }
        assert!(counts[ASH as usize] > 0, "ash lies on top: {counts:?}");
        assert!(counts[EMBER as usize] > 0, "seams glow: {counts:?}");
        assert!((1..=SHADES as usize).all(|shade| counts[shade] > 0), "shades of rock: {counts:?}");
    }

    #[test]
    fn a_rock_drifts_and_turns_about_its_own_middle() {
        let rocks = VoxelRocks::default();
        let rock = Rock { cell: [0, 0], at: [100.0, 50.0], size: 10.0, lift: 40.0, seed: 7 };
        let extent = Shape::new(&rock).extent;
        let (corner, turn) = pose(&rocks, &rock, extent, 5.0, 0.0);
        let middle = corner + turn * Shape::middle_of(extent);
        assert!((middle - Vec3::new(100.0, 45.0, 50.0)).length() < rocks.drift * 3.0 + 1.0, "{middle}");
        let (later, turned) = pose(&rocks, &rock, extent, 5.0, 30.0);
        assert!(turn.angle_between(turned) > 0.05, "it turns");
        assert!((later + turned * Shape::middle_of(extent) - middle).length() > 0.05, "it drifts");
    }
}
