//! A Voxel Volume's own ground: rolling ground with grass blades a voxel
//! across, and flowers here and there, all of the volume's voxels, as the
//! small-voxels research's meadow had them (`docs/research/small-voxels.md`).

use voxear_core::math::Vec3;

use super::bricks::Builder;

/// A volume's ground and what grows on it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct VoxelGround {
    /// How high the ground stands over the volume's floor, in metres.
    pub height: f32,
    /// How high its hills rise over that, in metres.
    pub hills: f32,
    /// How far apart its hills are, in metres.
    pub hill_spacing: f32,
    /// How much of the ground grows grass, from 0 to 1, and how tall the
    /// tallest blades are, in metres.
    pub grass: f32,
    pub grass_height: f32,
    /// How many flowers grow, from 0 to 1.
    pub flowers: f32,
    /// Which ground of all those these numbers make.
    pub seed: u32,
}

impl Default for VoxelGround {
    fn default() -> Self {
        Self { height: 2.0, hills: 1.5, hill_spacing: 12.0, grass: 0.6, grass_height: 0.6, flowers: 0.2, seed: 1 }
    }
}

/// The ground's colours, as sRGB, in the volume's palette from its start.
pub const GROUND_COLORS: [[u8; 4]; 13] = [
    [74, 150, 48, 255], // 0 the ground's top
    [102, 74, 50, 255], // 1 dirt
    [72, 160, 44, 255], // 2-5 blades
    [92, 176, 52, 255],
    [58, 132, 40, 255],
    [110, 180, 64, 255],
    [140, 200, 84, 255],  // 6 blade tips
    [196, 24, 24, 255],   // 7 red
    [104, 56, 196, 255],  // 8 purple
    [224, 226, 230, 255], // 9 white
    [200, 120, 176, 255], // 10 pink
    [52, 132, 40, 255],   // 11 stems
    [230, 190, 40, 255],  // 12 a flower's middle
];

pub(super) fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - x.floor(), z - z.floor());
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let at = |a, b| unit(hash(ix + a, 0, iz + b, seed));
    let low = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let high = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    low + (high - low) * sz
}

fn fbm(x: f32, z: f32, seed: u32) -> f32 {
    let (mut sum, mut scale, mut weight) = (0.0, 1.0, 0.5);
    for octave in 0..4 {
        sum += value_noise(x * scale, z * scale, seed.wrapping_add(octave)) * weight;
        scale *= 2.0;
        weight *= 0.5;
    }
    sum / 0.9375
}

impl VoxelGround {
    /// The ground's height at `x`, `z` metres from the volume's corner, in
    /// metres over its floor.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let spacing = self.hill_spacing.max(0.5);
        self.height + self.hills * fbm(x / spacing, z / spacing, self.seed)
    }

    /// Sets the ground's voxels into `into`, for a volume `size` voxels a
    /// side at `per_metre` to a metre; the ground's colours are the palette's
    /// first.
    pub fn build(&self, into: &mut Builder, size: [u32; 3], per_metre: f32) {
        let voxel = 1.0 / per_metre;
        let [wide, tall, deep] = size.map(|a| a as i32);
        let height = |x: i32, z: i32| {
            (self.height_at((x as f32 + 0.5) * voxel, (z as f32 + 0.5) * voxel) / voxel).floor() as i32
        };
        let tallest = (self.grass_height.max(0.0) / voxel).max(1.0);
        let seed = self.seed;
        for z in 0..deep {
            for x in 0..wide {
                let h = height(x, z).min(tall - 1);
                if h < 0 {
                    continue;
                }
                // Down to the lowest of its neighbours, so no ray finds a gap under the ground.
                let low = [height(x - 1, z), height(x + 1, z), height(x, z - 1), height(x, z + 1)]
                    .into_iter()
                    .min()
                    .unwrap_or(h)
                    .min(h)
                    - 1;
                for y in low.max(0)..=h {
                    into.set([x, y, z], if y == h { 1 } else { 2 });
                }
                let r = hash(x, 0, z, seed.wrapping_add(21));
                if unit(r) < self.grass {
                    let blade = 1 + (unit(hash(x, 1, z, seed.wrapping_add(21))) * tallest) as i32;
                    for y in h + 1..=(h + blade).min(tall - 1) {
                        let tip = y == h + blade && blade as f32 * voxel > 0.2;
                        into.set([x, y, z], if tip { 7 } else { 3 + (r & 3) as u8 });
                    }
                }
            }
        }
        self.flowers_into(into, size, voxel, &height);
    }

    fn flowers_into(&self, into: &mut Builder, size: [u32; 3], voxel: f32, height: &dyn Fn(i32, i32) -> i32) {
        let cell = 0.5;
        let across = [size[0] as f32 * voxel / cell, size[2] as f32 * voxel / cell].map(|a| a.ceil() as i32);
        let seed = self.seed.wrapping_add(13);
        for cz in 0..across[1] {
            for cx in 0..across[0] {
                let patch = fbm(cx as f32 * cell / 6.0, cz as f32 * cell / 6.0, seed);
                let chance = self.flowers * (0.3 + 1.4 * (patch - 0.4).max(0.0));
                if unit(hash(cx, 3, cz, seed)) > chance {
                    continue;
                }
                let at = Vec3::new(
                    (cx as f32 + unit(hash(cx, 1, cz, seed))) * cell,
                    0.0,
                    (cz as f32 + unit(hash(cx, 2, cz, seed))) * cell,
                );
                let (x, z) = ((at.x / voxel) as i32, (at.z / voxel) as i32);
                let base = height(x, z);
                let stem = ((0.3 + unit(hash(cx, 5, cz, seed)) * 0.4) / voxel).max(2.0) as i32;
                for y in base + 1..=base + stem {
                    into.set([x, y, z], 12);
                }
                let top = base + stem;
                match (unit(hash(cx, 4, cz, seed)) * 4.0) as u32 {
                    // A wide red head, ragged at its rim.
                    0 => {
                        for dz in -2..=2i32 {
                            for dx in -2..=2i32 {
                                let r2 = dx * dx + dz * dz;
                                if r2 <= 8 && (r2 < 5 || unit(hash(x + dx, top, z + dz, seed)) < 0.6) {
                                    into.set([x + dx, top + 1, z + dz], 8);
                                    into.set([x + dx, top + 2, z + dz], 8);
                                }
                            }
                        }
                        into.set([x, top + 3, z], 13);
                    }
                    // A purple spike.
                    1 => {
                        for y in top - 4..=top + 2 {
                            for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                if unit(hash(x + dx, y, z + dz, seed)) < 0.85 {
                                    into.set([x + dx, y, z + dz], 9);
                                }
                            }
                        }
                    }
                    // A white cube.
                    2 => {
                        for y in top + 1..=top + 3 {
                            for dz in -1..=1 {
                                for dx in -1..=1 {
                                    into.set([x + dx, y, z + dz], 10);
                                }
                            }
                        }
                    }
                    _ => {
                        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                            into.set([x + dx, top + 1, z + dz], 11);
                        }
                        into.set([x, top + 2, z], 13);
                    }
                }
            }
        }
    }
}
