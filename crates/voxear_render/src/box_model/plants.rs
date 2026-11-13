//! Plants to start from: grass, flowers, bushes, ferns, mushrooms, trees,
//! and what lies about under them (stumps, fallen logs, sticks) and among
//! them (pebbles, rocks, boulders), each a box model of one bone and one
//! box sculpted from small cubes (see `voxels`). They are made here, from a
//! few shapes and a number that picks each cube's shade, so a new project
//! has something to grow on its Block Worlds, and every one of them opens
//! in the Model window to be reshaped like any model.
//!
//! The small ones are eight cubes to a block, the smallest sixteen, and the
//! trees and the boulder four, so a tree is no heavier to draw than a bush.

use voxear_core::math::Vec3;

use super::{BoxModel, BoxSkin, ModelBox, Sheet, Voxels};

/// A plant to start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StarterPlant {
    Tuft,
    Flower,
    Bush,
    Rock,
    Tree,
    Pine,
    TallGrass,
    Daisy,
    Bluebell,
    Fern,
    DryBush,
    Mushroom,
    Birch,
    DeadTree,
    Stump,
    Log,
    Stick,
    Pebbles,
    Boulder,
}

/// One cube's colour, red, green and blue.
type Color = [u8; 3];

const GRASS: [Color; 3] = [[74, 128, 46], [92, 148, 54], [116, 168, 62]];
const LEAVES: [Color; 3] = [[44, 96, 40], [58, 116, 46], [78, 136, 54]];
const NEEDLES: [Color; 3] = [[30, 74, 48], [38, 90, 56], [50, 106, 62]];
const BARK: [Color; 2] = [[92, 66, 42], [110, 80, 52]];
const STONE: [Color; 3] = [[112, 112, 116], [134, 134, 138], [154, 152, 150]];
const PETAL: [Color; 2] = [[206, 44, 40], [226, 66, 52]];
const POLLEN: Color = [240, 200, 60];
const BERRY: Color = [176, 40, 60];
const WHITE_PETAL: [Color; 2] = [[236, 236, 228], [250, 250, 244]];
const BLUE_PETAL: [Color; 2] = [[72, 92, 204], [96, 118, 224]];
const FROND: [Color; 3] = [[38, 100, 42], [52, 122, 50], [70, 144, 58]];
const DRY: [Color; 2] = [[150, 136, 74], [172, 158, 90]];
const TWIG: [Color; 2] = [[104, 82, 56], [124, 98, 66]];
const CAP: [Color; 2] = [[176, 52, 40], [196, 70, 48]];
const SPOT: Color = [240, 236, 224];
const STALK: Color = [226, 214, 190];
const BIRCH_BARK: [Color; 2] = [[226, 224, 214], [206, 204, 196]];
const BIRCH_MARK: Color = [58, 56, 52];
const BIRCH_LEAVES: [Color; 3] = [[84, 158, 40], [108, 182, 48], [136, 204, 60]];
const DEAD_WOOD: [Color; 2] = [[108, 96, 84], [128, 114, 98]];
const CUT_WOOD: [Color; 2] = [[178, 144, 98], [160, 126, 84]];
const MOSS: [Color; 2] = [[70, 116, 48], [88, 134, 54]];

/// A number from a cube's place and `salt`, well mixed: whole numbers
/// alone, so a plant is the same wherever it is made.
fn mixed([x, y, z]: [i32; 3], salt: u32) -> u32 {
    let mut h = salt.wrapping_mul(0x9E37_79B9) ^ (x as u32).wrapping_mul(0x8DA6_B343);
    h = h.rotate_left(13) ^ (y as u32).wrapping_mul(0xD816_3841);
    h = h.rotate_left(11) ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// From 0 up to 1.
fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / 16_777_216.0
}

/// One of `shades` for the cube at `cell`.
fn shade<const N: usize>(shades: [Color; N], cell: [i32; 3], salt: u32) -> Color {
    shades[(mixed(cell, salt) % N as u32) as usize]
}

/// How far inside a rounded shape `cell`'s middle is: over nothing inside
/// it. The shape is `radii` cubes each way about `middle`, its edge pushed
/// in and out by up to `rough` of its size, cube by cube.
fn inside(cell: [i32; 3], middle: [f32; 3], radii: [f32; 3], rough: f32, salt: u32) -> f32 {
    let off = |axis: usize| (cell[axis] as f32 + 0.5 - middle[axis]) / radii[axis];
    let far = (off(0) * off(0) + off(1) * off(1) + off(2) * off(2)).sqrt();
    1.0 + (unit(mixed(cell, salt)) - 0.5) * 2.0 * rough - far
}

/// How far `cell`'s middle is from the line from `from` to `to`, in cubes.
fn off_line(cell: [i32; 3], from: [f32; 3], to: [f32; 3]) -> f32 {
    let at = cell.map(|v| v as f32 + 0.5);
    let along = [0, 1, 2].map(|k| to[k] - from[k]);
    let long = along[0] * along[0] + along[1] * along[1] + along[2] * along[2];
    let reached = (0..3).map(|k| (at[k] - from[k]) * along[k]).sum::<f32>() / long.max(1e-6);
    let share = reached.clamp(0.0, 1.0);
    let off = [0, 1, 2].map(|k| at[k] - (from[k] + along[k] * share));
    (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt()
}

/// Whether `cell` is within `thick` cubes of any of `lines`.
fn on_lines(cell: [i32; 3], lines: &[([f32; 3], [f32; 3])], thick: f32) -> bool {
    lines.iter().any(|&(from, to)| off_line(cell, from, to) < thick)
}

/// The tree's limbs, from its trunk out and up into its crown.
const TREE_LIMBS: [([f32; 3], [f32; 3]); 3] = [
    ([10.0, 4.5, 9.0], [15.0, 10.5, 10.5]),
    ([8.0, 5.5, 8.5], [3.0, 11.0, 6.5]),
    ([9.0, 6.5, 10.0], [9.5, 11.5, 15.0]),
];

/// The birch's limbs, and its crown's tiers from the lowest: how high each
/// one's middle is, how wide and how tall it is from its middle, and how
/// far off the trunk its middle stands along x and z.
const BIRCH_LIMBS: [([f32; 3], [f32; 3]); 2] =
    [([10.0, 9.5, 9.0], [13.5, 13.5, 10.5]), ([8.0, 11.0, 8.5], [4.5, 14.5, 7.0])];
const BIRCH_TIERS: [(f32, f32, f32, [f32; 2]); 4] = [
    (17.0, 7.8, 2.7, [0.0, 0.0]),
    (22.0, 6.6, 2.7, [0.6, -0.4]),
    (26.5, 5.2, 2.6, [-0.5, 0.5]),
    (30.6, 3.4, 2.9, [0.2, 0.2]),
];

/// The dead tree's limbs, each forking once.
const DEAD_LIMBS: [([f32; 3], [f32; 3]); 7] = [
    ([7.0, 9.0, 7.0], [11.5, 14.0, 8.0]),
    ([11.5, 14.0, 8.0], [12.5, 18.0, 10.0]),
    ([7.0, 12.0, 7.0], [3.0, 16.0, 5.5]),
    ([3.0, 16.0, 5.5], [1.5, 20.0, 5.0]),
    ([7.3, 15.0, 6.8], [8.0, 19.0, 11.0]),
    ([7.4, 18.0, 6.6], [5.5, 23.0, 7.5]),
    ([7.5, 20.0, 6.5], [8.5, 25.0, 6.0]),
];

/// The dry bush's twigs, from its foot.
const DRY_TWIGS: [([f32; 3], [f32; 3]); 9] = [
    ([4.5, 0.0, 4.5], [1.5, 5.0, 2.5]),
    ([4.5, 0.0, 4.5], [7.5, 6.0, 3.5]),
    ([4.5, 0.0, 4.5], [2.5, 4.5, 7.5]),
    ([4.5, 0.0, 4.5], [6.5, 5.0, 7.5]),
    ([4.5, 0.0, 4.5], [4.5, 6.5, 4.5]),
    ([4.5, 0.0, 4.5], [8.5, 3.0, 5.5]),
    ([3.0, 2.5, 3.5], [0.5, 3.5, 4.5]),
    ([6.0, 3.0, 4.0], [8.0, 4.5, 2.0]),
    ([5.5, 2.5, 6.0], [4.5, 4.0, 8.5]),
];

/// The stick: a length, a kink, and a fork lying beside it.
const STICK: [([f32; 3], [f32; 3]); 3] = [
    ([0.5, 0.5, 3.5], [9.0, 0.5, 4.5]),
    ([9.0, 0.5, 4.5], [13.5, 1.4, 3.0]),
    ([6.0, 0.5, 4.2], [9.5, 0.5, 7.2]),
];

/// The pebbles: each one's middle and how far it reaches along x, y and z.
const PEBBLES: [([f32; 3], [f32; 3]); 5] = [
    ([2.5, 0.6, 3.0], [2.0, 1.4, 1.7]),
    ([7.5, 0.5, 2.5], [1.6, 1.2, 1.5]),
    ([9.0, 0.6, 8.5], [2.2, 1.6, 2.0]),
    ([4.0, 0.4, 8.5], [1.4, 1.0, 1.3]),
    ([6.0, 0.3, 5.5], [1.0, 0.8, 1.0]),
];

impl StarterPlant {
    pub const ALL: [StarterPlant; 19] = [
        StarterPlant::Tuft,
        StarterPlant::Flower,
        StarterPlant::Bush,
        StarterPlant::Rock,
        StarterPlant::Tree,
        StarterPlant::Pine,
        StarterPlant::TallGrass,
        StarterPlant::Daisy,
        StarterPlant::Bluebell,
        StarterPlant::Fern,
        StarterPlant::DryBush,
        StarterPlant::Mushroom,
        StarterPlant::Birch,
        StarterPlant::DeadTree,
        StarterPlant::Stump,
        StarterPlant::Log,
        StarterPlant::Stick,
        StarterPlant::Pebbles,
        StarterPlant::Boulder,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StarterPlant::Tuft => "Grass Tuft",
            StarterPlant::Flower => "Flower",
            StarterPlant::Bush => "Bush",
            StarterPlant::Rock => "Rock",
            StarterPlant::Tree => "Tree",
            StarterPlant::Pine => "Pine",
            StarterPlant::TallGrass => "Tall Grass",
            StarterPlant::Daisy => "Daisy",
            StarterPlant::Bluebell => "Bluebell",
            StarterPlant::Fern => "Fern",
            StarterPlant::DryBush => "Dry Bush",
            StarterPlant::Mushroom => "Mushroom",
            StarterPlant::Birch => "Birch",
            StarterPlant::DeadTree => "Dead Tree",
            StarterPlant::Stump => "Stump",
            StarterPlant::Log => "Log",
            StarterPlant::Stick => "Stick",
            StarterPlant::Pebbles => "Pebbles",
            StarterPlant::Boulder => "Boulder",
        }
    }

    /// The colour a wood of it is far off, where it is a tree: its leaves',
    /// sRGB from 0 to 1. None for plants too small to show in a far view.
    pub fn far_color(self) -> Option<[f32; 3]> {
        let color = |[r, g, b]: Color| [r, g, b].map(|part| f32::from(part) / 255.0);
        match self {
            StarterPlant::Tree => Some(color(LEAVES[1])),
            StarterPlant::Pine => Some(color(NEEDLES[1])),
            StarterPlant::Birch => Some(color(BIRCH_LEAVES[1])),
            _ => None,
        }
    }

    /// How many of its cubes make a metre, and how many it measures along
    /// X, Y and Z.
    fn grid(self) -> (f32, [u32; 3]) {
        match self {
            StarterPlant::Tuft => (8.0, [8, 5, 8]),
            StarterPlant::Flower => (8.0, [3, 7, 3]),
            StarterPlant::Bush => (8.0, [10, 7, 10]),
            StarterPlant::Rock => (8.0, [13, 6, 10]),
            StarterPlant::Tree => (4.0, [18, 26, 18]),
            StarterPlant::Pine => (4.0, [14, 32, 14]),
            StarterPlant::TallGrass => (8.0, [7, 9, 7]),
            StarterPlant::Daisy => (8.0, [3, 4, 3]),
            StarterPlant::Bluebell => (8.0, [4, 6, 3]),
            StarterPlant::Fern => (8.0, [11, 4, 11]),
            StarterPlant::DryBush => (8.0, [9, 7, 9]),
            StarterPlant::Mushroom => (8.0, [5, 5, 5]),
            StarterPlant::Birch => (4.0, [18, 34, 18]),
            StarterPlant::DeadTree => (4.0, [14, 26, 14]),
            StarterPlant::Stump => (8.0, [8, 4, 8]),
            StarterPlant::Log => (8.0, [20, 6, 7]),
            StarterPlant::Stick => (16.0, [14, 2, 8]),
            StarterPlant::Pebbles => (16.0, [12, 3, 12]),
            StarterPlant::Boulder => (4.0, [14, 11, 13]),
        }
    }

    /// The cube at `cell` of its box: its colour, or none for air.
    fn cube(self, cell: [i32; 3]) -> Option<Color> {
        let [x, y, z] = cell;
        match self {
            StarterPlant::Tuft => {
                // Blades: where each stands, and how tall.
                const BLADES: [(i32, i32, i32); 10] = [
                    (1, 1, 3),
                    (3, 5, 4),
                    (6, 2, 2),
                    (2, 6, 5),
                    (5, 3, 3),
                    (4, 0, 2),
                    (7, 6, 4),
                    (0, 4, 2),
                    (6, 5, 3),
                    (3, 2, 4),
                ];
                let blade = BLADES.iter().find(|(bx, bz, tall)| *bx == x && *bz == z && y < *tall)?;
                // Lighter at its tip.
                Some(if y + 1 == blade.2 { GRASS[2] } else { shade([GRASS[0], GRASS[1]], [x, 0, z], 1) })
            }
            StarterPlant::Flower => match (x, y, z) {
                (1, 0..=4, 1) => Some(GRASS[0]),
                (0, 2, 1) | (2, 1, 1) => Some(GRASS[1]),
                (1, 6, 1) => Some(POLLEN),
                (_, 5, _) => Some(shade(PETAL, cell, 2)),
                _ => None,
            },
            StarterPlant::Bush => {
                let deep = inside(cell, [5.0, 2.6, 5.0], [5.0, 4.2, 5.0], 0.16, 3);
                if deep <= 0.0 {
                    return None;
                }
                // A berry here and there on its outside.
                let berry = deep < 0.22 && y >= 2 && mixed(cell, 4) % 13 == 0;
                Some(if berry { BERRY } else { shade(LEAVES, cell, 5) })
            }
            StarterPlant::Rock => {
                // A boulder and a smaller stone against it, both sunk into the ground.
                let large = inside(cell, [5.0, 1.4, 5.0], [5.0, 4.4, 4.6], 0.1, 6);
                let small = inside(cell, [10.2, 0.6, 4.4], [2.8, 2.6, 3.0], 0.12, 7);
                (large > 0.0 || small > 0.0).then(|| shade(STONE, [x, y / 2, z], 8))
            }
            StarterPlant::Tree => {
                if (8..=9).contains(&x) && (8..=9).contains(&z) && y < 15 {
                    return Some(shade(BARK, cell, 9));
                }
                // Limbs out from the trunk, showing under the crown before they run into it.
                if on_lines(cell, &TREE_LIMBS, 0.75) {
                    return Some(shade(BARK, cell, 9));
                }
                let deep = inside(cell, [9.0, 17.0, 9.0], [8.6, 8.4, 8.6], 0.14, 10);
                // Darker underneath, lighter on top.
                let lit = usize::from(y >= 14) + usize::from(y >= 21 && mixed(cell, 11) % 2 == 0);
                (deep > 0.0).then(|| if mixed(cell, 12) % 5 == 0 { shade(LEAVES, cell, 13) } else { LEAVES[lit] })
            }
            StarterPlant::Pine => {
                if (6..=7).contains(&x) && (6..=7).contains(&z) && y < 28 {
                    return Some(shade(BARK, cell, 14));
                }
                if y < 6 {
                    return None;
                }
                // Skirts of needles, each wide at its foot and narrow under the next, narrowing to the top.
                let up = (y - 6) as f32 / 26.0;
                let skirt = 1.0 - ((y - 6) % 6) as f32 / 6.0;
                let radius = (1.0 - up) * 6.6 * (0.55 + 0.45 * skirt) + 0.6;
                let (dx, dz) = (x as f32 + 0.5 - 7.0, z as f32 + 0.5 - 7.0);
                let edge = radius + (unit(mixed(cell, 15)) - 0.5) * 1.2;
                (dx * dx + dz * dz < edge * edge).then(|| shade(NEEDLES, cell, 16))
            }
            StarterPlant::TallGrass => {
                // Blades: where each stands, how tall, and which way its top third leans.
                const BLADES: [(i32, i32, i32, [i32; 2]); 12] = [
                    (1, 1, 6, [0, 0]),
                    (3, 5, 9, [1, 0]),
                    (5, 2, 7, [0, -1]),
                    (2, 4, 8, [-1, 0]),
                    (4, 3, 9, [0, 1]),
                    (3, 1, 5, [0, 0]),
                    (5, 5, 7, [1, 0]),
                    (1, 3, 6, [-1, 0]),
                    (4, 4, 8, [0, 0]),
                    (2, 2, 7, [0, -1]),
                    (5, 3, 5, [0, 0]),
                    (3, 3, 9, [0, 0]),
                ];
                let blade = BLADES.iter().find(|(bx, bz, tall, lean)| {
                    let leans = y * 3 >= tall * 2;
                    y < *tall && x == bx + if leans { lean[0] } else { 0 } && z == bz + if leans { lean[1] } else { 0 }
                })?;
                // Lighter at its tip, and a few tips gone to seed.
                let tip = y + 1 == blade.2;
                Some(match (tip, blade.2 >= 9) {
                    (true, true) => DRY[1],
                    (true, false) => GRASS[2],
                    _ => shade([GRASS[0], GRASS[1]], [blade.0, 0, blade.1], 17),
                })
            }
            StarterPlant::Daisy => match (x, y, z) {
                (1, 0..=2, 1) => Some(GRASS[0]),
                (0, 1, 1) => Some(GRASS[1]),
                (1, 3, 1) => Some(POLLEN),
                (0, 3, 1) | (2, 3, 1) | (1, 3, 0) | (1, 3, 2) => Some(shade(WHITE_PETAL, cell, 18)),
                _ => None,
            },
            StarterPlant::Bluebell => match (x, y, z) {
                // A stem that bends over at its top, with bells hanging from it.
                (1, 0..=4, 1) | (2, 5, 1) | (0, 4, 1) => Some(GRASS[0]),
                (0, 0, 1) | (2, 1, 1) | (1, 0, 0) => Some(GRASS[1]),
                (2, 4, 1) | (3, 4, 1) | (0, 3, 1) | (2, 3, 1) | (1, 5, 1) => Some(shade(BLUE_PETAL, cell, 19)),
                _ => None,
            },
            StarterPlant::Fern => {
                // Eight fronds from its middle, each rising and drooping, with leaflets to either side.
                const WAYS: [[i32; 2]; 8] = [[1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0], [-1, -1], [0, -1], [1, -1]];
                const ARCH: [i32; 6] = [0, 1, 2, 2, 1, 0];
                if (x, z) == (5, 5) && y <= 1 {
                    return Some(FROND[0]);
                }
                for (frond, [dx, dz]) in WAYS.into_iter().enumerate() {
                    let long = if frond % 2 == 0 { 5 } else { 4 };
                    for out in 1..=long {
                        let (fx, fz, fy) = (5 + dx * out, 5 + dz * out, ARCH[out as usize]);
                        if (x, y, z) == (fx, fy, fz) {
                            return Some(FROND[1]);
                        }
                        let leaflet = (x, z) == (fx - dz, fz + dx) || (x, z) == (fx + dz, fz - dx);
                        if leaflet && y == fy && out >= 2 && out < long {
                            return Some(shade([FROND[1], FROND[2]], cell, 20));
                        }
                    }
                }
                None
            }
            StarterPlant::DryBush => {
                if !on_lines(cell, &DRY_TWIGS, 0.62) {
                    return None;
                }
                // A dry leaf here and there towards the twigs' ends.
                Some(if y >= 4 && mixed(cell, 21) % 4 == 0 { shade(DRY, cell, 22) } else { shade(TWIG, cell, 23) })
            }
            StarterPlant::Mushroom => {
                let (dx, dz) = ((x - 2).abs(), (z - 2).abs());
                match y {
                    0..=2 => (dx == 0 && dz == 0).then_some(STALK),
                    3 => (dx + dz <= 3).then(|| if mixed(cell, 24) % 5 == 0 { SPOT } else { shade(CAP, cell, 25) }),
                    _ => (dx <= 1 && dz <= 1).then(|| if dx + dz == 0 { SPOT } else { shade(CAP, cell, 25) }),
                }
            }
            StarterPlant::Birch => {
                if (8..=9).contains(&x) && (8..=9).contains(&z) && y < 28 {
                    // White bark with dark marks across it.
                    let marked = mixed([x, y, z], 26) % 7 == 0 || (y % 5 == 2 && mixed([x / 2, y, z], 27) % 2 == 0);
                    return Some(if marked { BIRCH_MARK } else { shade(BIRCH_BARK, cell, 28) });
                }
                if on_lines(cell, &BIRCH_LIMBS, 0.7) {
                    return Some(shade(BIRCH_BARK, cell, 28));
                }
                // Tiers of leaves, each a low dome over the one below: light on top, dark underneath.
                for (tier, &(up, wide, tall, [ox, oz])) in BIRCH_TIERS.iter().enumerate() {
                    if inside(cell, [9.0 + ox, up, 9.0 + oz], [wide, tall, wide], 0.16, 29 + tier as u32) <= 0.0 {
                        continue;
                    }
                    let within = (y as f32 + 0.5 - up) / tall;
                    return Some(if within > 0.25 {
                        shade([BIRCH_LEAVES[1], BIRCH_LEAVES[2]], cell, 35)
                    } else if within < -0.35 {
                        BIRCH_LEAVES[0]
                    } else {
                        shade(BIRCH_LEAVES, cell, 36)
                    });
                }
                None
            }
            StarterPlant::DeadTree => {
                // A bare trunk, thinner towards its top, and limbs that fork.
                let thick = 1.5 - 1.0 * (y as f32 / 20.0).min(1.0);
                let trunk = off_line(cell, [7.0, 0.0, 7.0], [7.5, 20.0, 6.5]) < thick;
                (trunk || on_lines(cell, &DEAD_LIMBS, 0.62)).then(|| shade(DEAD_WOOD, [x, y / 2, z], 37))
            }
            StarterPlant::Stump => {
                let (dx, dz) = (x as f32 + 0.5 - 4.0, z as f32 + 0.5 - 4.0);
                let out = (dx * dx + dz * dz).sqrt();
                // Roots spread from its foot along x and z.
                let root = y == 0 && out < 4.0 && (dx.abs() < 1.0 || dz.abs() < 1.0);
                if out >= 2.9 + (unit(mixed([x, 0, z], 38)) - 0.5) * 0.5 && !root {
                    return None;
                }
                Some(if y == 3 && out < 2.2 {
                    // Its cut top, in rings.
                    if out < 1.0 { CUT_WOOD[1] } else { CUT_WOOD[0] }
                } else if y >= 2 && mixed(cell, 39) % 6 == 0 {
                    shade(MOSS, cell, 40)
                } else {
                    shade(BARK, cell, 41)
                })
            }
            StarterPlant::Log => {
                // A trunk lying along x, sunk a little into the ground, with the stubs of two branches.
                let (dy, dz) = (y as f32 + 0.5 - 2.3, z as f32 + 0.5 - 3.5);
                let out = (dy * dy + dz * dz).sqrt();
                let stubs = [([6.0, 3.0, 3.5], [7.0, 5.6, 2.4]), ([13.0, 2.5, 3.5], [14.5, 3.2, 6.6])];
                if out >= 2.5 + (unit(mixed([x / 2, y, z], 42)) - 0.5) * 0.3 {
                    return on_lines(cell, &stubs, 0.62).then(|| shade(BARK, cell, 43));
                }
                Some(if (x == 0 || x == 19) && out < 2.0 {
                    // Its cut ends, in rings.
                    if out < 1.1 { CUT_WOOD[1] } else { CUT_WOOD[0] }
                } else if y >= 4 && mixed([x / 2, y, z / 2], 44) % 3 == 0 {
                    shade(MOSS, cell, 45)
                } else {
                    shade(BARK, [x / 2, y, z], 43)
                })
            }
            StarterPlant::Stick => on_lines(cell, &STICK, 0.6).then(|| shade(TWIG, [x / 2, y, z / 2], 46)),
            StarterPlant::Pebbles => PEBBLES.iter().enumerate().find_map(|(stone, &(middle, radii))| {
                // Each stone a shade of its own, a cube or two lighter or darker.
                (inside(cell, middle, radii, 0.1, 47 + stone as u32) > 0.0)
                    .then(|| if mixed(cell, 53) % 5 == 0 { shade(STONE, cell, 54) } else { STONE[stone % 3] })
            }),
            StarterPlant::Boulder => {
                // A great stone and a shoulder against it, both sunk into the ground, with moss on top.
                let great = inside(cell, [7.0, 3.0, 6.5], [6.6, 7.4, 6.0], 0.045, 55);
                let shoulder = inside(cell, [10.6, 1.6, 8.6], [3.4, 4.4, 3.6], 0.05, 56);
                if great <= 0.0 && shoulder <= 0.0 {
                    return None;
                }
                let mossy = y >= 8 && great < 0.3 && mixed([x / 2, y, z / 2], 57) % 3 == 0;
                Some(if mossy { shade(MOSS, cell, 58) } else { shade(STONE, [x / 2, y / 2, z / 2], 59) })
            }
        }
    }

    /// How much it bends in the wind and away from what walks through it:
    /// grass and flowers right over, a bush less, a tree only a little,
    /// and a rock not at all. A low, wide plant bends less for the same
    /// number than a tall, thin one (a plant's height is read from the
    /// ball round its mesh), so a tuft has the most.
    pub fn wind_bend(self) -> f32 {
        match self {
            StarterPlant::Tuft => 2.0,
            StarterPlant::Flower => 1.5,
            StarterPlant::Bush => 0.8,
            StarterPlant::Rock => 0.0,
            StarterPlant::Tree => 0.35,
            StarterPlant::Pine => 0.25,
            StarterPlant::TallGrass => 2.0,
            StarterPlant::Daisy | StarterPlant::Bluebell => 1.5,
            StarterPlant::Fern => 1.2,
            StarterPlant::DryBush => 0.5,
            StarterPlant::Mushroom => 0.4,
            StarterPlant::Birch => 0.3,
            StarterPlant::DeadTree => 0.1,
            StarterPlant::Stump
            | StarterPlant::Log
            | StarterPlant::Stick
            | StarterPlant::Pebbles
            | StarterPlant::Boulder => 0.0,
        }
    }

    /// Whether it stands still in the wind: stone, and wood lying dead.
    pub fn is_still(self) -> bool {
        self.wind_bend() == 0.0
    }

    /// How large it is, in metres along X, Y and Z, standing on the ground
    /// about its middle.
    pub fn size(self) -> [f32; 3] {
        let (cubes_per_metre, size) = self.grid();
        size.map(|side| side as f32 / cubes_per_metre)
    }

    /// The plant as a box model: one bone at its foot, one box of cubes
    /// standing on the ground about its middle.
    pub fn model(self) -> BoxModel {
        let (pixels_per_block, size) = self.grid();
        let wind_bend = self.wind_bend();
        let sheet = Sheet::filled(16, 16, [0; 4]);
        let mut model = BoxModel { pixels_per_block, sheet, wind_bend, ..BoxModel::default() };
        let bone = model.add_bone("Plant", None, Vec3::ZERO);
        let mut voxels = Voxels::filled(size, 0);
        for z in 0..size[2] as i32 {
            for y in 0..size[1] as i32 {
                for x in 0..size[0] as i32 {
                    if let Some([r, g, b]) = self.cube([x, y, z]) {
                        let color = model.color_index([r, g, b, 255]);
                        voxels.set(size, [x, y, z], color);
                    }
                }
            }
        }
        let from = Vec3::new(-(size[0] as f32) * 0.5, 0.0, -(size[2] as f32) * 0.5);
        let mut sculpted = ModelBox::new(from, size, [0, 0]);
        sculpted.skin = BoxSkin::Faces([None; 6]);
        sculpted.voxels = Some(voxels);
        model.bones[bone].boxes.push(sculpted);
        model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_plant_is_a_box_of_cubes_standing_on_the_ground_and_reads_back_as_written() {
        for plant in StarterPlant::ALL {
            let model = plant.model();
            assert_eq!((model.bones.len(), model.bones[0].boxes.len()), (1, 1), "{}", plant.name());
            let sculpted = &model.bones[0].boxes[0];
            let voxels = sculpted.voxels.as_ref().expect("sculpted from cubes");
            let size = sculpted.size;
            assert!(voxels.solid() > 8, "{} has {} cubes", plant.name(), voxels.solid());
            // It stands on the ground, about its middle.
            assert_eq!(sculpted.from.y, 0.0);
            let stands = (0..size[0] as i32).any(|x| (0..size[2] as i32).any(|z| voxels.get(size, [x, 0, z]) != 0));
            assert!(stands, "{} has no cube on the ground", plant.name());
            // More than one colour, each of its palette's used.
            assert!(model.palette.len() >= 2, "{}", plant.name());
            let used = |color: u8| voxels.cells.iter().any(|cell| *cell == color);
            assert!((1..=model.palette.len() as u8).all(used), "{}", plant.name());
            // The same again, and the same from its file.
            assert_eq!(plant.model(), model);
            assert_eq!(BoxModel::parse(&model.to_text()).unwrap(), model, "{}", plant.name());
            // As a mesh: something to draw, no taller than its box.
            let prepared = model.prepared("Plants/Test.boxmodel");
            assert_eq!(prepared.nodes.len(), 1);
            let tall = size[1] as f32 / model.pixels_per_block;
            assert!((0.12..=8.5).contains(&tall), "{} is {tall} m tall", plant.name());
        }
        // A tuft is thin blades, a tree taller than a bush, a rock wider than it is tall.
        let cubes = |plant: StarterPlant| plant.model().bones[0].boxes[0].voxels.as_ref().unwrap().solid();
        assert!(cubes(StarterPlant::Tuft) < 40 && cubes(StarterPlant::Tree) > 1000);
        let [wide, tall, _] = StarterPlant::Rock.grid().1;
        assert!(wide > tall);
        // Stones in three sizes; a log longer than it is thick; a birch taller than the tree, white in the trunk.
        let stones = [StarterPlant::Pebbles, StarterPlant::Rock, StarterPlant::Boulder].map(|stone| stone.size()[0]);
        assert!(stones[0] < stones[1] && stones[1] * 2.0 < stones[2], "{stones:?}");
        let [long, thick, _] = StarterPlant::Log.size();
        assert!(long > thick * 3.0);
        assert!(StarterPlant::Birch.size()[1] > StarterPlant::Tree.size()[1]);
        let white = StarterPlant::Birch.model().palette.iter().any(|color| color[..3].iter().all(|part| *part > 200));
        assert!(white, "the birch's bark");
        // The tree's limbs reach out from its trunk under its crown: wood where there was air.
        let limb = StarterPlant::Tree.cube([12, 7, 9]);
        assert!(BARK.contains(&limb.expect("a limb")), "{limb:?}");
        // Names are all different, and every plant that is a tree says what a wood of it looks like far off.
        let names: std::collections::HashSet<&str> = StarterPlant::ALL.iter().map(|plant| plant.name()).collect();
        assert_eq!(names.len(), StarterPlant::ALL.len());
        let far = StarterPlant::ALL.into_iter().filter(|plant| plant.far_color().is_some()).count();
        assert_eq!(far, 3);
    }

    #[test]
    fn starter_plants_bend_in_the_wind_but_for_the_rock_and_a_trees_crown_sways_more_than_its_trunk() {
        for plant in StarterPlant::ALL {
            let prepared = plant.model().prepared("Plants/Test.boxmodel");
            let (material, vertices) = (&prepared.materials[0].desc, &prepared.meshes[0].0.vertices);
            assert_eq!(material.wind_bend > 0.0, !plant.is_still(), "{}", plant.name());
            assert_eq!(material.wind_flutter, 0.0, "cubes do not flutter apart");
            if plant.is_still() {
                continue;
            }
            // Nothing flutters; faces that meet at a place sway alike there; and what is further out sways more.
            assert!(vertices.iter().all(|vertex| vertex.wind[0] == 0 && vertex.wind[1] == 0), "{}", plant.name());
            let mut swayed = std::collections::HashMap::new();
            for vertex in vertices {
                let place = vertex.position.map(f32::to_bits);
                assert_eq!(*swayed.entry(place).or_insert(vertex.wind[2]), vertex.wind[2], "{}", plant.name());
            }
            let out = |vertex: &crate::mesh::Vertex| vertex.position[0].hypot(vertex.position[2]);
            let furthest = vertices.iter().max_by(|a, b| out(a).total_cmp(&out(b))).unwrap();
            let nearest = vertices.iter().min_by(|a, b| out(a).total_cmp(&out(b))).unwrap();
            assert!(furthest.wind[2] == 255 && nearest.wind[2] < 70, "{}: {}", plant.name(), nearest.wind[2]);
        }
        // A build keeps the bend: cooked and read back, the tuft's material has it.
        let tuft = StarterPlant::Tuft.model().prepared("Plants/Tuft.boxmodel");
        let cooked = tuft.cook().unwrap().to_bytes();
        let back = crate::CookedModel::from_bytes("Plants/Tuft.boxmodel", &cooked).unwrap();
        assert_eq!(back.materials[0].desc.wind_bend, StarterPlant::Tuft.wind_bend());
        // Written with its bend, and read back with it; a model that does not bend writes nothing of it.
        assert!(StarterPlant::Tuft.model().to_text().contains("wind_bend = 2.0"));
        assert!(!StarterPlant::Rock.model().to_text().contains("wind_bend"));
        assert!(StarterPlant::Pine.wind_bend() < StarterPlant::Bush.wind_bend());
        let [wide, tall, deep] = StarterPlant::Rock.size();
        assert_eq!([wide, tall, deep], [1.625, 0.75, 1.25]);
    }
}
