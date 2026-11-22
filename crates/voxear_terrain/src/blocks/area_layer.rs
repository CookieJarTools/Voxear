//! World Areas as the generator sees them: outlines or painted patches of a
//! world, between two heights, each with its tags, that say where its
//! structures may stand. A structure placed "within" a tag stands only in an
//! area holding it, one placed "never within" a tag never in one, and an area
//! with No Structures keeps every structure out.
//!
//! They shape no ground, so they are kept beside the world's layers (see
//! `world_layers`) and asked only where a structure's site is tried.

use crate::generate::painted::Painted;

/// One World Area, in the world's blocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaLayer {
    /// What it is known by between one set of areas and the next, as the Obj
    /// it is on.
    pub id: u64,
    /// Its outline along x and z, a closed loop of corners; none for an area
    /// painted on the world's Area map.
    pub outline: Vec<[f32; 2]>,
    /// Its number on the world's painted Area map; 0 for none.
    pub painted: u32,
    /// The lowest and highest ground it holds, in blocks up; none for any.
    pub low: Option<i32>,
    pub high: Option<i32>,
    /// Its tags, each by its asset's number.
    pub tags: Vec<u128>,
    pub no_structures: bool,
    pub priority: i32,
}

impl AreaLayer {
    /// Everything it is, as bytes, the same on every machine.
    pub fn bytes(&self) -> Vec<u8> {
        let AreaLayer { id, outline, painted, low, high, tags, no_structures, priority } = self;
        let mut bytes = Vec::new();
        bytes.extend(id.to_le_bytes());
        bytes.extend((outline.len() as u32).to_le_bytes());
        outline.iter().flatten().for_each(|v| bytes.extend(v.to_le_bytes()));
        bytes.extend(painted.to_le_bytes());
        for side in [low, high] {
            bytes.extend(side.map_or([0xff; 5], |v| {
                let mut kept = [0; 5];
                kept[1..].copy_from_slice(&v.to_le_bytes());
                kept
            }));
        }
        bytes.extend((tags.len() as u32).to_le_bytes());
        tags.iter().for_each(|tag| bytes.extend(tag.to_le_bytes()));
        bytes.push(u8::from(*no_structures));
        bytes.extend(priority.to_le_bytes());
        bytes
    }

    /// The box its outline lies within, along x and z, in blocks: `[low,
    /// high]`, the high side reached; none for a painted area.
    pub fn bounds(&self) -> Option<[[i32; 2]; 2]> {
        let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
        for corner in &self.outline {
            for axis in 0..2 {
                low[axis] = low[axis].min(corner[axis]);
                high[axis] = high[axis].max(corner[axis]);
            }
        }
        (low[0] <= high[0] && low.iter().chain(&high).all(|v| v.is_finite()))
            .then(|| [low.map(|v| v.floor() as i32 - 1), high.map(|v| v.ceil() as i32 + 1)])
    }

    /// Whether the middle of column `[x, z]`, its ground at `ground` blocks
    /// up, lies in it; `painted` is the number painted there, if any.
    pub fn holds(&self, [x, z]: [i32; 2], ground: Option<i32>, painted: Option<u32>) -> bool {
        let point = [x as f32 + 0.5, z as f32 + 0.5];
        let within = match self.painted {
            0 => self.outline.len() >= 3 && inside(&self.outline, point),
            number => painted == Some(number),
        };
        within
            && ground.is_none_or(|ground| {
                self.low.is_none_or(|low| ground >= low) && self.high.is_none_or(|high| ground <= high)
            })
    }
}

/// A world's areas, and its painted Area map, read from where the world lies
/// in the generator's metres.
#[derive(Clone, Debug, Default)]
pub struct WorldAreas {
    pub areas: Vec<AreaLayer>,
    pub painted: Option<Painted>,
    /// Where the world's block 0 lies in the generator's metres, along x and
    /// z, and how wide a block is.
    pub origin: [f32; 2],
    pub block_size: f32,
}

impl PartialEq for WorldAreas {
    fn eq(&self, other: &Self) -> bool {
        self.areas == other.areas
            && self.painted.as_ref().map(|p| p.id) == other.painted.as_ref().map(|p| p.id)
            && self.origin == other.origin
            && self.block_size == other.block_size
    }
}

impl WorldAreas {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// The number painted on the Area map under column `[x, z]`.
    pub fn painted_at(&self, [x, z]: [i32; 2]) -> Option<u32> {
        let painted = self.painted.as_ref()?;
        let size = self.block_size.max(0.01);
        let at = [self.origin[0] + (x as f32 + 0.5) * size, self.origin[1] + (z as f32 + 0.5) * size];
        let number = painted.maps().areas.nearest(at[0], at[1])?;
        (number >= 1.0).then_some(number.round() as u32)
    }

    /// The box of blocks its painted Area map covers.
    pub fn painted_bounds(&self) -> Option<[[i32; 2]; 2]> {
        let [low, high] = self.painted.as_ref()?.maps().areas.bounds()?;
        let size = self.block_size.max(0.01);
        let block = |v: f32, axis: usize| (v - self.origin[axis]) / size;
        Some([
            [block(low[0], 0).floor() as i32 - 1, block(low[1], 1).floor() as i32 - 1],
            [block(high[0], 0).ceil() as i32 + 1, block(high[1], 1).ceil() as i32 + 1],
        ])
    }

    /// The areas holding column `column`, its ground at `ground`, highest
    /// priority first.
    pub fn at(&self, column: [i32; 2], ground: Option<i32>) -> Vec<&AreaLayer> {
        let painted = self.areas.iter().any(|area| area.painted != 0).then(|| self.painted_at(column)).flatten();
        let mut found: Vec<&AreaLayer> = self.areas.iter().filter(|area| area.holds(column, ground, painted)).collect();
        found.sort_by_key(|area| (std::cmp::Reverse(area.priority), area.id));
        found
    }

    /// Whether a structure kept `within` areas of those tags (any area where
    /// none) and `never` within areas of these may stand at column `column`,
    /// its ground at `ground`.
    pub fn allows(&self, column: [i32; 2], ground: Option<i32>, within: &[u128], never: &[u128]) -> bool {
        let here = self.at(column, ground);
        if here.iter().any(|area| area.no_structures) {
            return false;
        }
        let holds = |tags: &[u128]| here.iter().any(|area| area.tags.iter().any(|tag| tags.contains(tag)));
        (within.is_empty() || holds(within)) && !holds(never)
    }
}

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
    use super::*;
    use crate::generate::painted::PaintedMaps;

    fn square(id: u64, low: f32, high: f32) -> AreaLayer {
        AreaLayer { id, outline: vec![[low, low], [high, low], [high, high], [low, high]], ..AreaLayer::default() }
    }

    #[test]
    fn a_structure_stands_within_its_tags_never_within_others_and_nowhere_with_no_structures() {
        let (town, forest) = (7u128, 9u128);
        let areas = WorldAreas {
            areas: vec![
                AreaLayer { tags: vec![town], priority: 1, ..square(1, 0.0, 100.0) },
                AreaLayer { tags: vec![forest], ..square(2, 50.0, 200.0) },
                AreaLayer { no_structures: true, ..square(3, 300.0, 400.0) },
            ],
            ..WorldAreas::default()
        };
        assert!(areas.allows([10, 10], None, &[town], &[]), "within the town");
        assert!(!areas.allows([150, 150], None, &[town], &[]), "out of the town");
        assert!(!areas.allows([60, 60], None, &[], &[forest]), "never in the forest");
        assert!(areas.allows([10, 10], None, &[], &[forest]));
        assert!(!areas.allows([350, 350], None, &[], &[]), "No Structures keeps all out");
        assert!(areas.allows([900, 900], None, &[], &[]), "anywhere with no rules");
        let at: Vec<u64> = areas.at([60, 60], None).iter().map(|area| area.id).collect();
        assert_eq!(at, [1, 2], "highest priority first");
    }

    #[test]
    fn an_areas_height_range_holds_only_ground_between_its_heights() {
        let area = AreaLayer { low: Some(-20), high: Some(-5), ..square(1, 0.0, 10.0) };
        assert!(area.holds([4, 4], Some(-10), None));
        assert!(!area.holds([4, 4], Some(12), None), "a town over the cave");
        assert!(area.holds([4, 4], None, None), "with no height asked");
    }

    #[test]
    fn a_painted_area_holds_where_its_number_is_painted() {
        let painted = Painted::new(3, PaintedMaps::default());
        painted.change(|maps| maps.areas.paint([40.0, 40.0], 12.0, 0.0, &|_, cover| (cover > 0.5).then_some(2.0)));
        let areas = WorldAreas {
            areas: vec![AreaLayer { id: 1, painted: 2, no_structures: true, ..AreaLayer::default() }],
            painted: Some(painted),
            origin: [0.0, 0.0],
            block_size: 1.0,
        };
        assert_eq!(areas.painted_at([40, 40]), Some(2));
        assert!(!areas.allows([40, 40], None, &[], &[]));
        assert!(areas.allows([90, 90], None, &[], &[]));
    }
}
