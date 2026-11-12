//! Plants to start from: a tuft of grass, a flower, a bush, a rock, a tree
//! and a pine, each a box model of one bone and one box sculpted from small
//! cubes (see `voxels`). They are made here, from a few shapes and a number
//! that picks each cube's shade, so a new project has something to grow on
//! its Block Worlds, and every one of them opens in the Model window to be
//! reshaped like any model.
//!
//! The small ones are eight cubes to a block and the trees four, so a tree
//! is no heavier to draw than a bush.

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

impl StarterPlant {
    pub const ALL: [StarterPlant; 6] = [
        StarterPlant::Tuft,
        StarterPlant::Flower,
        StarterPlant::Bush,
        StarterPlant::Rock,
        StarterPlant::Tree,
        StarterPlant::Pine,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StarterPlant::Tuft => "Grass Tuft",
            StarterPlant::Flower => "Flower",
            StarterPlant::Bush => "Bush",
            StarterPlant::Rock => "Rock",
            StarterPlant::Tree => "Tree",
            StarterPlant::Pine => "Pine",
        }
    }

    /// The colour a wood of it is far off, where it is a tree: its leaves',
    /// sRGB from 0 to 1. None for plants too small to show in a far view.
    pub fn far_color(self) -> Option<[f32; 3]> {
        let color = |[r, g, b]: Color| [r, g, b].map(|part| f32::from(part) / 255.0);
        match self {
            StarterPlant::Tree => Some(color(LEAVES[1])),
            StarterPlant::Pine => Some(color(NEEDLES[1])),
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
        }
    }

    /// The plant as a box model: one bone at its foot, one box of cubes
    /// standing on the ground about its middle.
    pub fn model(self) -> BoxModel {
        let (pixels_per_block, size) = self.grid();
        let mut model = BoxModel { pixels_per_block, sheet: Sheet::filled(16, 16, [0; 4]), ..BoxModel::default() };
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
            assert!(voxels.solid() > 15, "{} has {} cubes", plant.name(), voxels.solid());
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
            assert!((0.5..=8.5).contains(&tall), "{} is {tall} m tall", plant.name());
        }
        // A tuft is thin blades, a tree taller than a bush, a rock wider than it is tall.
        let cubes = |plant: StarterPlant| plant.model().bones[0].boxes[0].voxels.as_ref().unwrap().solid();
        assert!(cubes(StarterPlant::Tuft) < 40 && cubes(StarterPlant::Tree) > 1000);
        let [wide, tall, _] = StarterPlant::Rock.grid().1;
        assert!(wide > tall);
    }
}
