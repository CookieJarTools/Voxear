//! Plants: where each of a Foliage's plants grows, which are gone, and
//! what a trunk collides as. Kept apart from how they are drawn, so a
//! server with no GPU has them too: physics stands trunks on them, scripts
//! cut them down, and saves keep which are gone.
//!
//! A plant's place in its Obj's list is its number. The list is shared
//! (`Arc`) and only the editor changes it; while a game runs, plants taken
//! away (cut down, dug up) are only marked gone, so every other plant
//! keeps its number, and a save names the gone ones by it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::Component;

/// Where one plant grows: its place in the Obj's space, its turn about its
/// up axis in radians, its size, and how it leans: the x and z of the way
/// its up points, nothing for upright, as a plant stands leaning with a
/// slope.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plant {
    pub position: [f32; 3],
    pub yaw: f32,
    pub scale: f32,
    #[serde(default)]
    pub lean: [f32; 2],
}

impl Plant {
    /// An upright plant.
    pub fn new(position: [f32; 3], yaw: f32, scale: f32) -> Self {
        Self { position, yaw, scale, lean: [0.0; 2] }
    }

    /// Which way it is turned: leaning as it says, then turned about its
    /// own up.
    pub fn rotation(&self) -> Quat {
        let [x, z] = self.lean;
        let flat = x * x + z * z;
        let up = Vec3::new(x, (1.0 - flat).max(0.0).sqrt(), z);
        let lean = if flat > 1e-8 { Quat::from_rotation_arc(Vec3::Y, up.normalize()) } else { Quat::IDENTITY };
        lean * Quat::from_rotation_y(self.yaw)
    }

    /// Where it stands in the world, in an Obj placed by `model`.
    pub fn matrix(&self, model: Mat4) -> Mat4 {
        model
            * Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rotation(), Vec3::from(self.position))
    }
}

/// A set of plants by number, a bit each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlantSet {
    words: Vec<u64>,
}

impl PlantSet {
    pub fn contains(&self, number: u32) -> bool {
        self.words.get((number / 64) as usize).is_some_and(|word| word >> (number % 64) & 1 != 0)
    }

    /// Adds `number`; whether it was not in already.
    pub fn insert(&mut self, number: u32) -> bool {
        let at = (number / 64) as usize;
        if at >= self.words.len() {
            self.words.resize(at + 1, 0);
        }
        let had = self.contains(number);
        self.words[at] |= 1 << (number % 64);
        !had
    }

    /// Takes `number` out; whether it was in.
    pub fn remove(&mut self, number: u32) -> bool {
        let had = self.contains(number);
        if let Some(word) = self.words.get_mut((number / 64) as usize) {
            *word &= !(1 << (number % 64));
        }
        had
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&word| word == 0)
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|word| word.count_ones() as usize).sum()
    }

    /// Every number in it, in order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(at, &word)| {
            (0..64).filter(move |bit| word >> bit & 1 != 0).map(move |bit| at as u32 * 64 + bit)
        })
    }
}

impl FromIterator<u32> for PlantSet {
    fn from_iter<T: IntoIterator<Item = u32>>(numbers: T) -> Self {
        let mut set = Self::default();
        for number in numbers {
            set.insert(number);
        }
        set
    }
}

/// What a plant's trunk collides as: an upright capsule `radius` metres
/// round and `height` metres tall from the ground, scaled with the plant.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trunk {
    pub radius: f32,
    pub height: f32,
}

impl Default for Trunk {
    fn default() -> Self {
        Self { radius: 0.25, height: 4.0 }
    }
}

/// An Obj's plants (a Foliage's): where each grows, which are gone, and
/// their trunks' shape when they collide.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plants {
    /// Each plant, in the Obj's space, numbered by its place. Shared, so
    /// what keeps a copy can tell the list is the same by its pointer.
    pub plants: Arc<Vec<Plant>>,
    /// The plants taken away while the game runs, by number.
    pub removed: Arc<PlantSet>,
    /// Their trunks, which physics stands up for each plant still there;
    /// none for plants walked through.
    pub trunk: Option<Trunk>,
    /// The prefab a plant becomes when cut down, by its reference; none
    /// leaves nothing.
    pub cut: Option<String>,
}

impl Component for Plants {}

impl Plants {
    pub fn new(plants: Vec<Plant>) -> Self {
        Self { plants: Arc::new(plants), ..Self::default() }
    }

    /// Whether plant `number` is there: in the list and not taken away.
    pub fn has(&self, number: u32) -> bool {
        (number as usize) < self.plants.len() && !self.removed.contains(number)
    }

    /// Takes plant `number` away; whether it was there.
    pub fn remove(&mut self, number: u32) -> bool {
        self.has(number) && Arc::make_mut(&mut self.removed).insert(number)
    }

    /// Each plant still there, with its number.
    pub fn standing(&self) -> impl Iterator<Item = (u32, &Plant)> {
        let removed = &self.removed;
        self.plants.iter().enumerate().map(|(n, plant)| (n as u32, plant)).filter(move |(n, _)| !removed.contains(*n))
    }
}

/// Plants scattered over an area `size` metres across, centred on the
/// Obj, `density` to a square metre, evenly but not in rows: one in each
/// cell of a grid, somewhere in it, each turned its own way and sized
/// between `scale`'s two. The same `seed` scatters them the same way.
pub fn scatter(size: [f32; 2], density: f32, seed: u32, scale: [f32; 2]) -> Vec<Plant> {
    let step = 1.0 / density.max(1e-4).sqrt();
    let (across, deep) = ((size[0] / step).ceil() as u32, (size[1] / step).ceil() as u32);
    let mut plants = Vec::with_capacity((across * deep) as usize);
    let mut next = random(seed);
    for row in 0..deep {
        for column in 0..across {
            let x = (column as f32 + next()) * step - size[0] * 0.5;
            let z = (row as f32 + next()) * step - size[1] * 0.5;
            if x.abs() > size[0] * 0.5 || z.abs() > size[1] * 0.5 {
                continue;
            }
            let yaw = next() * std::f32::consts::TAU;
            let scale = scale[0] + (scale[1] - scale[0]) * next();
            plants.push(Plant::new([x, 0.0, z], yaw, scale));
        }
    }
    plants
}

/// Numbers from 0 to 1 from `seed`, the same everywhere (SplitMix64).
pub fn random(seed: u32) -> impl FnMut() -> f32 {
    let mut state = u64::from(seed) ^ 0x9e37_79b9_7f4a_7c15;
    move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plant_taken_away_is_gone_and_the_rest_keep_their_numbers() {
        let mut plants = Plants::new((0..100).map(|i| Plant::new([i as f32, 0.0, 0.0], 0.0, 1.0)).collect());
        let before = plants.plants.clone();
        assert!(plants.remove(70));
        assert!(!plants.remove(70), "gone already");
        assert!(!plants.remove(500), "no such plant");
        assert!(!plants.has(70) && plants.has(71));
        assert!(Arc::ptr_eq(&before, &plants.plants), "the list is kept");
        assert_eq!(plants.standing().count(), 99);
        assert_eq!(plants.standing().nth(70).map(|(n, _)| n), Some(71));
        assert_eq!(plants.removed.iter().collect::<Vec<_>>(), [70]);
        let set: PlantSet = [3, 64, 130].into_iter().collect();
        assert_eq!(set.iter().collect::<Vec<_>>(), [3, 64, 130]);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn a_leaning_plant_stands_along_the_way_it_leans() {
        let mut plant = Plant::new([1.0, 2.0, 3.0], 0.0, 2.0);
        assert_eq!(plant.matrix(Mat4::IDENTITY).transform_vector3(Vec3::Y), Vec3::new(0.0, 2.0, 0.0));
        plant.lean = [0.6, 0.0];
        let up = plant.matrix(Mat4::IDENTITY).transform_vector3(Vec3::Y).normalize();
        assert!((up - Vec3::new(0.6, 0.8, 0.0)).length() < 1e-5, "{up}");
        assert_eq!(plant.matrix(Mat4::from_translation(Vec3::X)).w_axis.truncate(), Vec3::new(2.0, 2.0, 3.0));
    }
}
