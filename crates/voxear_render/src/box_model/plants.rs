//! Plants to start from: a tuft of grass, a flower, a bush, a rock, a tree
//! and a pine, each a box model of one bone and one box sculpted from small
//! cubes (see `voxels`). They are made here, from a few shapes and a number
//! that picks each cube's shade, so a new project has something to grow on
//! its Block Worlds, and every one of them opens in the Model window to be
//! reshaped like any model.
//!
//! And plants of the sea's bed, which grow under water: seagrass and kelp,
//! four corals (branching, brain, table and fan), a tube sponge and a clam.
//! The weeds sway with the water; coral, sponge and clam are stone-still.
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
    Seagrass,
    Kelp,
    BranchCoral,
    BrainCoral,
    TableCoral,
    FanCoral,
    TubeSponge,
    Clam,
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
const SEAGRASS: [Color; 3] = [[38, 110, 78], [50, 132, 86], [78, 156, 96]];
const KELP: [Color; 3] = [[70, 84, 30], [92, 104, 36], [124, 132, 48]];
const BRANCH_CORAL: [Color; 3] = [[204, 70, 96], [226, 96, 118], [244, 150, 160]];
const BRAIN_CORAL: [Color; 3] = [[176, 134, 60], [204, 164, 80], [128, 96, 48]];
const TABLE_CORAL: [Color; 3] = [[58, 102, 170], [80, 128, 196], [136, 176, 224]];
const FAN_CORAL: [Color; 3] = [[150, 64, 160], [182, 86, 184], [226, 130, 70]];
const SPONGE: [Color; 3] = [[214, 150, 40], [236, 178, 56], [120, 70, 24]];
const SHELL: [Color; 3] = [[226, 216, 200], [198, 186, 172], [236, 160, 170]];

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
    pub const ALL: [StarterPlant; 14] = [
        StarterPlant::Tuft,
        StarterPlant::Flower,
        StarterPlant::Bush,
        StarterPlant::Rock,
        StarterPlant::Tree,
        StarterPlant::Pine,
        StarterPlant::Seagrass,
        StarterPlant::Kelp,
        StarterPlant::BranchCoral,
        StarterPlant::BrainCoral,
        StarterPlant::TableCoral,
        StarterPlant::FanCoral,
        StarterPlant::TubeSponge,
        StarterPlant::Clam,
    ];

    /// Whether it grows on the bed under water, not on dry ground.
    pub fn under_water(self) -> bool {
        !matches!(
            self,
            StarterPlant::Tuft
                | StarterPlant::Flower
                | StarterPlant::Bush
                | StarterPlant::Rock
                | StarterPlant::Tree
                | StarterPlant::Pine
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            StarterPlant::Tuft => "Grass Tuft",
            StarterPlant::Flower => "Flower",
            StarterPlant::Bush => "Bush",
            StarterPlant::Rock => "Rock",
            StarterPlant::Tree => "Tree",
            StarterPlant::Pine => "Pine",
            StarterPlant::Seagrass => "Seagrass",
            StarterPlant::Kelp => "Kelp",
            StarterPlant::BranchCoral => "Branch Coral",
            StarterPlant::BrainCoral => "Brain Coral",
            StarterPlant::TableCoral => "Table Coral",
            StarterPlant::FanCoral => "Fan Coral",
            StarterPlant::TubeSponge => "Tube Sponge",
            StarterPlant::Clam => "Clam",
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
            StarterPlant::Seagrass => (8.0, [8, 9, 8]),
            StarterPlant::Kelp => (4.0, [7, 32, 7]),
            StarterPlant::BranchCoral => (8.0, [11, 11, 11]),
            StarterPlant::BrainCoral => (8.0, [10, 6, 10]),
            StarterPlant::TableCoral => (8.0, [14, 7, 14]),
            StarterPlant::FanCoral => (8.0, [13, 12, 3]),
            StarterPlant::TubeSponge => (8.0, [9, 10, 9]),
            StarterPlant::Clam => (8.0, [7, 4, 6]),
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
            StarterPlant::Seagrass => {
                // Long thin blades, taller than a tuft's, each leaning a cube as it rises.
                const BLADES: [(i32, i32, i32, i32); 9] = [
                    (1, 1, 6, 1),
                    (3, 5, 9, 0),
                    (6, 2, 5, -1),
                    (2, 6, 7, 1),
                    (5, 4, 8, 0),
                    (4, 0, 4, -1),
                    (6, 6, 6, 0),
                    (0, 3, 5, 1),
                    (3, 2, 7, -1),
                ];
                let blade = BLADES.iter().find(|(bx, bz, tall, lean)| {
                    let leaned = (bx + lean * (y / 4)).clamp(0, 7);
                    leaned == x && *bz == z && y < *tall
                })?;
                Some(if y + 1 == blade.2 { SEAGRASS[2] } else { shade([SEAGRASS[0], SEAGRASS[1]], [blade.0, 0, z], 17) })
            }
            StarterPlant::Kelp => {
                // A stalk that winds a cube either way as it climbs, with a blade out to one side and then the other.
                let wind = [0, 1, 1, 0, 0, -1, -1, 0][(y as usize / 2) % 8];
                let (sx, sz) = (3 + wind, 3);
                if x == sx && z == sz {
                    return Some(shade([KELP[0], KELP[1]], [0, y, 0], 18));
                }
                let frond = y % 5;
                if y < 3 || frond > 2 {
                    return None;
                }
                let side = if (y / 5) % 2 == 0 { 1 } else { -1 };
                let across = (y / 5) % 4 >= 2;
                let (out, off) = if across { (z - sz, x - sx) } else { (x - sx, z - sz) };
                let reach = 3 - frond;
                (off == 0 && out * side >= 1 && out * side <= reach).then(|| if out * side == reach { KELP[2] } else { KELP[1] })
            }
            StarterPlant::BranchCoral => {
                // A stem, and arms that go out from it and then up, each to its own height.
                const ARMS: [(i32, i32, i32, i32); 5] = [(1, 0, 2, 9), (-1, 0, 4, 10), (0, 1, 3, 8), (0, -1, 5, 11), (1, 1, 6, 9)];
                let (dx, dz) = (x - 5, z - 5);
                if dx == 0 && dz == 0 && y < 8 {
                    return Some(shade(BRANCH_CORAL, cell, 19));
                }
                let arm = ARMS.iter().find(|(ax, az, from, tall)| {
                    let out = (y - from).clamp(0, 3);
                    y >= *from && y < *tall && dx == ax * out && dz == az * out
                        || y == *from && (1..=3).any(|step| dx == ax * step && dz == az * step)
                })?;
                Some(if y + 1 == arm.3 { BRANCH_CORAL[2] } else { shade([BRANCH_CORAL[0], BRANCH_CORAL[1]], cell, 20) })
            }
            StarterPlant::BrainCoral => {
                let deep = inside(cell, [5.0, 0.6, 5.0], [5.0, 5.2, 5.0], 0.04, 21);
                if deep <= 0.0 {
                    return None;
                }
                // Grooves that wind over it.
                let groove = deep < 0.3 && (x + (z / 2) % 2 * 2 + y) % 4 == 0;
                Some(if groove { BRAIN_CORAL[2] } else { shade([BRAIN_CORAL[0], BRAIN_CORAL[1]], cell, 22) })
            }
            StarterPlant::TableCoral => {
                let (dx, dz) = (x as f32 + 0.5 - 7.0, z as f32 + 0.5 - 7.0);
                let out = (dx * dx + dz * dz).sqrt();
                // A stalk, a wide flat plate on it, a cube thicker at its middle.
                let stalk = y < 5 && out < 1.6;
                let plate = y == 5 && out < 6.8 + (unit(mixed([x, 0, z], 23)) - 0.5) || y == 6 && out < 6.0 && mixed([x, 0, z], 24) % 3 != 0 || y == 4 && out < 3.0;
                (stalk || plate).then(|| if y == 6 { TABLE_CORAL[2] } else { shade([TABLE_CORAL[0], TABLE_CORAL[1]], cell, 25) })
            }
            StarterPlant::FanCoral => {
                // A flat fan one cube thick: ribs spreading from its foot, joined by cross pieces.
                if z != 1 {
                    return None;
                }
                let (dx, up) = (x as f32 + 0.5 - 6.5, y as f32 + 0.5);
                let far = (dx * dx + up * up).sqrt();
                if y < 2 {
                    return (x == 6).then_some(FAN_CORAL[0]);
                }
                if far > 11.6 || dx.abs() > up * 1.1 {
                    return None;
                }
                let rib = ((dx / up.max(0.5) * 5.0 + 20.0).round() as i32) % 2 == 0;
                let cross = (far.round() as i32) % 3 == 0;
                (rib || cross).then(|| if far > 10.4 { FAN_CORAL[2] } else { shade([FAN_CORAL[0], FAN_CORAL[1]], cell, 26) })
            }
            StarterPlant::TubeSponge => {
                // Hollow tubes of different heights standing together.
                const TUBES: [(i32, i32, i32); 4] = [(2, 2, 10), (6, 3, 7), (3, 6, 6), (6, 6, 4)];
                TUBES.iter().find_map(|&(tx, tz, tall)| {
                    let (dx, dz) = ((x - tx).abs(), (z - tz).abs());
                    let wall = dx.max(dz) == 1 || y == 0 && dx.max(dz) == 0;
                    (y < tall && dx <= 1 && dz <= 1 && wall).then(|| {
                        if y + 1 == tall { SPONGE[2] } else { shade([SPONGE[0], SPONGE[1]], cell, 27) }
                    })
                })
            }
            StarterPlant::Clam => {
                // Two ribbed shells, the upper a little open, pink between them.
                let lower = inside(cell, [3.5, 0.4, 3.0], [3.5, 1.9, 3.0], 0.0, 28) > 0.0 && y < 2;
                let upper = inside(cell, [3.5, 1.6, 2.6], [3.4, 2.4, 2.8], 0.0, 29) > 0.0 && y >= 2 && z < 5;
                if lower && y == 1 && (1..=5).contains(&x) && (1..=4).contains(&z) {
                    return Some(SHELL[2]);
                }
                (lower || upper).then(|| SHELL[usize::from(x % 2 == 0)])
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
            StarterPlant::Seagrass => 2.0,
            StarterPlant::Kelp => 0.9,
            StarterPlant::FanCoral => 0.25,
            StarterPlant::BranchCoral
            | StarterPlant::BrainCoral
            | StarterPlant::TableCoral
            | StarterPlant::TubeSponge
            | StarterPlant::Clam => 0.0,
        }
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
        // Six grow on dry ground and eight on the sea's bed, the kelp the tallest of those.
        let sea: Vec<StarterPlant> = StarterPlant::ALL.into_iter().filter(|plant| plant.under_water()).collect();
        assert_eq!((sea.len(), sea[0]), (8, StarterPlant::Seagrass));
        assert!(sea.iter().all(|plant| plant.size()[1] <= StarterPlant::Kelp.size()[1]));
        assert_eq!(StarterPlant::Kelp.size()[1], 8.0);
        // A tuft is thin blades, a tree taller than a bush, a rock wider than it is tall.
        let cubes = |plant: StarterPlant| plant.model().bones[0].boxes[0].voxels.as_ref().unwrap().solid();
        assert!(cubes(StarterPlant::Tuft) < 40 && cubes(StarterPlant::Tree) > 1000);
        let [wide, tall, _] = StarterPlant::Rock.grid().1;
        assert!(wide > tall);
    }

    #[test]
    fn starter_plants_bend_in_the_wind_but_for_the_rock_and_a_trees_crown_sways_more_than_its_trunk() {
        for plant in StarterPlant::ALL {
            let prepared = plant.model().prepared("Plants/Test.boxmodel");
            let (material, vertices) = (&prepared.materials[0].desc, &prepared.meshes[0].0.vertices);
            let still = [StarterPlant::Rock, StarterPlant::BranchCoral, StarterPlant::BrainCoral, StarterPlant::TableCoral];
            let still = still.contains(&plant) || matches!(plant, StarterPlant::TubeSponge | StarterPlant::Clam);
            assert_eq!(material.wind_bend > 0.0, !still, "{}", plant.name());
            assert_eq!(material.wind_flutter, 0.0, "cubes do not flutter apart");
            if still {
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
            assert!(furthest.wind[2] == 255 && nearest.wind[2] < 40, "{}", plant.name());
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
