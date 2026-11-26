//! What grows where on an endless Voxel Volume: the World Areas its trees
//! and crops keep within or out of, by their tags, and its crops and ground
//! cover ([`VoxelCover`]), as the GPU reads them (`growth` in
//! `shaders/voxel_world.slang`). The CPU asks the same of a tree's foot
//! ([`Growth::tree_allowed`]), so its trees stand where the GPU draws them.
//!
//! The words: how many areas and cover kinds there are, where each's
//! records start, and each kind of tree's rules, two words a kind (the tags
//! it keeps within, and those it keeps out of, a bit each). Then each area:
//! the box its outline lies in and the lowest and highest ground it holds,
//! in voxels; its tags, a bit each; and where its corners start and how
//! many. Then each cover kind, then the corners, X and Z in voxels.

use voxear_terrain::{blocks::area_layer::AreaLayer, generate::structures::AreaRules};

use super::{VoxelArea, VoxelCover, VoxelCoverKind, VoxelMaterial, VoxelTrees, trees};

pub(crate) const HEADER: usize = 4 + trees::MOST_KINDS * 2;
pub(crate) const AREA_WORDS: usize = 9;
pub(crate) const COVER_WORDS: usize = 8;
/// The most kinds of cover a volume grows.
pub const MOST_COVER_KINDS: usize = 8;
/// How many colours a kind of cover has: its stem in two shades, its head
/// in two.
pub(crate) const COVER_COLORS: usize = 4;

/// A volume's areas and rules, in its voxels.
#[derive(Clone, Debug, Default)]
pub(crate) struct Growth {
    /// The areas that hold any tag a rule names, each with its tags as bits.
    areas: Vec<(AreaLayer, u32)>,
    /// Each kind of tree's rules, and each kind of cover's, as bits:
    /// within, never.
    trees: Vec<(u32, u32)>,
    cover_masks: Vec<(u32, u32)>,
}

/// The bit of each tag the rules name, at most 32.
fn tag_bits(rules: &[&AreaRules]) -> Vec<u128> {
    let mut tags: Vec<u128> = Vec::new();
    for tag in rules.iter().flat_map(|rule| rule.within.iter().chain(&rule.never)) {
        if !tags.contains(tag) && tags.len() < 32 {
            tags.push(*tag);
        }
    }
    tags
}

fn mask(tags: &[u128], of: &[u128]) -> u32 {
    of.iter().filter_map(|tag| tags.iter().position(|t| t == tag)).fold(0, |mask, bit| mask | 1 << bit)
}

/// The kinds of tree a volume grows, as its forest has them.
pub(crate) fn growing(trees: &[VoxelTrees]) -> impl Iterator<Item = &VoxelTrees> {
    trees.iter().filter(|kind| kind.per_hectare > 0.0).take(trees::MOST_KINDS)
}

/// The kinds of cover a volume grows.
pub(crate) fn covering(cover: &[VoxelCover]) -> impl Iterator<Item = &VoxelCover> {
    cover.iter().filter(|kind| kind.density > 0.0 && kind.height > 0.0).take(MOST_COVER_KINDS)
}

impl Growth {
    /// A volume's areas and rules, its voxels `voxel` metres across.
    pub fn new(areas: &[VoxelArea], trees: &[VoxelTrees], cover: &[VoxelCover], voxel: f32) -> Self {
        let rules: Vec<&AreaRules> =
            growing(trees).map(|kind| &kind.areas).chain(covering(cover).map(|kind| &kind.areas)).collect();
        let tags = tag_bits(&rules);
        let masks = |rules: &AreaRules| (mask(&tags, &rules.within), mask(&tags, &rules.never));
        let cover_masks = covering(cover).map(|kind| masks(&kind.areas)).collect();
        let trees = growing(trees).map(|kind| masks(&kind.areas)).collect();
        let v = voxel.max(1e-4);
        let areas = areas
            .iter()
            .filter(|area| area.outline.len() >= 3)
            .map(|area| (area, mask(&tags, &area.tags)))
            .filter(|(_, bits)| *bits != 0)
            .map(|(area, bits)| {
                let layer = AreaLayer {
                    outline: area.outline.iter().map(|&[x, z]| [x / v, z / v]).collect(),
                    low: area.low.map(|low| (low / v).floor() as i32),
                    high: area.high.map(|high| (high / v).floor() as i32),
                    ..AreaLayer::default()
                };
                (layer, bits)
            })
            .collect();
        Self { areas, trees, cover_masks }
    }

    /// The tags, as bits, of the areas holding voxel column `[x, z]`, its
    /// ground `h` voxels up: as the GPU finds them (`areaTagsAt`).
    pub fn tags_at(&self, column: [i32; 2], h: i32) -> u32 {
        self.areas.iter().filter(|(area, _)| area.holds(column, Some(h), None)).fold(0, |tags, (_, bits)| tags | bits)
    }

    /// Whether kind `kind` of tree grows with its foot at voxel column
    /// `foot`, its ground `h` voxels up, by the areas there.
    pub fn tree_allowed(&self, kind: usize, foot: [i32; 2], h: i32) -> bool {
        let Some(&(within, never)) = self.trees.get(kind) else { return true };
        if within == 0 && never == 0 {
            return true;
        }
        allowed(within, never, self.tags_at(foot, h))
    }

    /// The words the GPU reads, the cover kinds' colours starting at
    /// palette place `first_color`: `cover` is the cover it was made with.
    pub fn words(&self, cover: &[VoxelCover], voxel: f32, first_color: u32) -> Vec<u32> {
        let covers: Vec<&VoxelCover> = covering(cover).collect();
        let areas_at = HEADER;
        let covers_at = areas_at + self.areas.len() * AREA_WORDS;
        let mut corners_at = covers_at + covers.len() * COVER_WORDS;
        let mut words = vec![0u32; corners_at];
        words[..4].copy_from_slice(&[self.areas.len() as u32, covers.len() as u32, areas_at as u32, covers_at as u32]);
        for (k, &(within, never)) in self.trees.iter().enumerate() {
            words[4 + k * 2] = within;
            words[5 + k * 2] = never;
        }
        let mut corners = Vec::new();
        for (a, (area, bits)) in self.areas.iter().enumerate() {
            let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
            for corner in &area.outline {
                for axis in 0..2 {
                    low[axis] = low[axis].min(corner[axis]);
                    high[axis] = high[axis].max(corner[axis]);
                }
            }
            let at = areas_at + a * AREA_WORDS;
            words[at..at + AREA_WORDS].copy_from_slice(&[
                low[0].to_bits(),
                low[1].to_bits(),
                high[0].to_bits(),
                high[1].to_bits(),
                area.low.unwrap_or(i32::MIN) as u32,
                area.high.unwrap_or(i32::MAX) as u32,
                *bits,
                corners_at as u32,
                area.outline.len() as u32,
            ]);
            corners.extend(area.outline.iter().flat_map(|corner| corner.map(f32::to_bits)));
            corners_at += area.outline.len() * 2;
        }
        let v = voxel.max(1e-4);
        for (k, kind) in covers.iter().enumerate() {
            let at = covers_at + k * COVER_WORDS;
            let ground = kind.on.iter().fold(0u32, |bits, &material| bits | 1 << ground_kind(material));
            let soil = kind.soil.map_or(0, |soil| ground_kind(soil) + 1);
            let rows = if kind.rows > 0.0 { (kind.rows / v).round().max(1.0) as u32 } else { 0 };
            words[at..at + COVER_WORDS].copy_from_slice(&[
                self.cover_masks[k].0,
                self.cover_masks[k].1,
                if ground == 0 { 1 } else { ground },
                kind.kind as u32 | soil << 8,
                kind.density.clamp(0.0, 1.0).to_bits(),
                (kind.height / v).to_bits(),
                rows,
                first_color + (k * COVER_COLORS) as u32,
            ]);
        }
        words.extend(corners);
        words
    }
}

fn allowed(within: u32, never: u32, here: u32) -> bool {
    (within == 0 || here & within != 0) && here & never == 0
}

/// The ground kind a material is as the tops pass numbers them
/// (`GROUND_GRASS` and on in `shaders/voxel_world.slang`).
pub(crate) fn ground_kind(material: VoxelMaterial) -> u32 {
    match material {
        VoxelMaterial::Grass => 0,
        VoxelMaterial::Rock => 1,
        VoxelMaterial::Sand => 2,
        VoxelMaterial::Snow => 3,
        VoxelMaterial::Gravel => 4,
        VoxelMaterial::Dirt => 5,
    }
}

/// The colours of each kind of cover, as sRGB: its stem in two shades, its
/// head in two.
pub(crate) fn cover_colors(cover: &[VoxelCover]) -> Vec<[u8; 4]> {
    covering(cover)
        .flat_map(|kind| {
            let [a, b, c, d] = match kind.kind {
                VoxelCoverKind::MeadowGrass => [[96, 146, 56], [120, 158, 62], [178, 162, 108], [150, 136, 92]],
                VoxelCoverKind::Wheat => [[188, 170, 92], [164, 152, 78], [226, 186, 98], [204, 160, 78]],
                VoxelCoverKind::Barley => [[178, 172, 104], [156, 158, 90], [232, 214, 150], [214, 196, 128]],
                VoxelCoverKind::Flax => [[90, 134, 62], [76, 118, 56], [108, 132, 214], [136, 156, 226]],
                VoxelCoverKind::Cabbage => [[78, 128, 80], [64, 112, 72], [156, 196, 128], [134, 180, 116]],
            };
            [a, b, c, d].map(|[r, g, b]| [r, g, b, 255])
        })
        .collect()
}

/// Whether what keeps to `rules` may be at `at`, in metres from a volume's
/// corner and floor, by its areas `areas`: within one of the tags `rules`
/// keeps within (anywhere where it names none), and in none it keeps out of.
pub fn areas_allow(areas: &[VoxelArea], rules: &AreaRules, at: voxear_core::math::Vec3) -> bool {
    if rules.within.is_empty() && rules.never.is_empty() {
        return true;
    }
    let holding = areas.iter().filter(|area| {
        let layer = area;
        let height = area.low.is_none_or(|low| at.y >= low) && area.high.is_none_or(|high| at.y <= high);
        height && layer.outline.len() >= 3 && inside(&layer.outline, [at.x, at.z])
    });
    let tags: Vec<u128> = holding.flat_map(|area| area.tags.iter().copied()).collect();
    let holds = |of: &[u128]| of.iter().any(|tag| tags.contains(tag));
    (rules.within.is_empty() || holds(&rules.within)) && !holds(&rules.never)
}

/// Whether `point` lies inside `outline`, as `AreaLayer::holds` finds it.
fn inside(outline: &[[f32; 2]], point: [f32; 2]) -> bool {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_core::math::Vec3;
    use voxear_terrain::blocks::plant::PlantDef;

    use super::*;

    fn square(tag: u128, low: f32, high: f32) -> VoxelArea {
        VoxelArea {
            outline: vec![[low, low], [high, low], [high, high], [low, high]],
            tags: vec![tag],
            ..VoxelArea::default()
        }
    }

    fn trees(within: Vec<u128>, never: Vec<u128>) -> VoxelTrees {
        let areas = AreaRules { within, never };
        VoxelTrees { plant: Arc::new(PlantDef::default()), per_hectare: 40.0, wood: [0; 3], leaves: [0; 3], areas }
    }

    #[test]
    fn a_kind_keeps_within_its_tags_areas_and_out_of_its_never_tags_and_the_words_say_so() {
        let (wood, town, other) = (7u128, 9u128, 11u128);
        // Areas in metres, 16 voxels a metre: the wood from 0 to 10 m, the town from 6 to 8 m inside it.
        let mut town_area = square(town, 6.0, 8.0);
        town_area.low = Some(1.0);
        let areas = [square(wood, 0.0, 10.0), town_area, square(other, 20.0, 30.0)];
        let kinds = [trees(vec![wood], vec![town]), trees(Vec::new(), Vec::new())];
        let wheat = [VoxelCover::of(VoxelCoverKind::Wheat)];
        let growth = Growth::new(&areas, &kinds, &wheat, 1.0 / 16.0);
        assert_eq!(growth.areas.len(), 2, "an area whose tags no rule names is left out");
        assert!(growth.tree_allowed(0, [16, 16], 0), "in the wood");
        assert!(!growth.tree_allowed(0, [200, 16], 0), "out of the wood");
        assert!(!growth.tree_allowed(0, [112, 112], 32), "in the town");
        assert!(growth.tree_allowed(0, [112, 112], 0), "under the town's lowest ground");
        assert!(growth.tree_allowed(1, [200, 16], 0), "a kind with no rules grows anywhere");
        let words = growth.words(&wheat, 1.0 / 16.0, 30);
        assert_eq!(&words[..4], &[2, 1, HEADER as u32, (HEADER + 2 * AREA_WORDS) as u32]);
        assert_eq!(&words[4..6], &[1, 2], "the wood's bit within, the town's never");
        let cover = &words[HEADER + 2 * AREA_WORDS..HEADER + 2 * AREA_WORDS + COVER_WORDS];
        assert_eq!(
            (cover[2], cover[3], cover[6], cover[7]),
            (1, 1 | 6 << 8, 4, 30),
            "on grass, dirt for soil, rows 4 voxels apart"
        );
        // A point in metres, as mobs ask.
        let rules = AreaRules { within: vec![wood], never: vec![town] };
        assert!(areas_allow(&areas, &rules, Vec3::new(3.0, 0.0, 3.0)));
        assert!(!areas_allow(&areas, &rules, Vec3::new(7.0, 2.0, 7.0)));
        assert!(!areas_allow(&areas, &rules, Vec3::new(15.0, 0.0, 3.0)));
    }
}
