//! World Areas: parts of a world, an outline on an Obj or a patch painted on
//! the world's Area map, between two heights, each with tags. A tag is an
//! Area Tag data asset, and each carries a data asset of the game's own: a
//! town's laws, a region's music, a dungeon floor's monsters. Ask what is at a
//! place with [`at`], and the data of a tag there with [`value`], which falls
//! back on the world's defaults, for the biome there or for anywhere.
//!
//! Structures keep to them (see `voxear_terrain::blocks::area_layer`): a
//! structure's placement may stand within areas of some tags and never within
//! others, and an area with No Structures keeps every structure out.
//!
//! ```toml
//! [obj.world_area]
//! outline = [[-40.0, -40.0], [40.0, -40.0], [40.0, 40.0], [-40.0, 40.0]]
//! low = 20.0                 # the lowest it holds, in metres up; none for any
//! priority = 1
//! no_structures = true
//! tags = [{ tag = "…", data = "…" }]
//!
//! [obj.area_defaults]          # on the world's Obj
//! tags = [{ tag = "…", data = "…" }, { tag = "…", data = "…", biome = "…" }]
//! ```

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::WorldTransform;
use voxear_terrain::blocks::area_layer::AreaLayer;

use crate::ground_fit::WorldPlace;

/// Whether a World Area is its outline or what is painted with its number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AreaShape {
    #[default]
    Outline,
    Painted,
}

impl AreaShape {
    pub const ALL: [AreaShape; 2] = [AreaShape::Outline, AreaShape::Painted];

    pub fn name(self) -> &'static str {
        match self {
            AreaShape::Outline => "Outline",
            AreaShape::Painted => "Painted",
        }
    }
}

/// A tag of an area, and the data it carries there: both data assets, the
/// tag of the Area Tag Definition, the data of any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AreaEntry {
    pub tag: Option<AssetGuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<AssetGuid>,
    /// For the world's defaults, the biome it holds for; none for anywhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biome: Option<AssetGuid>,
}

/// A part of a world, with tags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldArea {
    pub shape: AreaShape,
    /// Its outline along x and z, in metres in its Obj's own space.
    pub outline: Vec<[f32; 2]>,
    /// Its number on the world's painted Area map, given it when it is first
    /// painted; 0 for none yet.
    #[serde(skip_serializing_if = "is_zero")]
    pub number: u32,
    /// The lowest and highest it holds, in metres up in the world; none for
    /// any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<f32>,
    /// Which comes first where two hold a place: the higher.
    pub priority: i32,
    /// Whether it keeps the generator's structures out.
    pub no_structures: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<AreaEntry>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl voxear_ecs::Component for WorldArea {}

impl Default for WorldArea {
    fn default() -> Self {
        Self {
            shape: AreaShape::Outline,
            outline: vec![[-32.0, -32.0], [32.0, -32.0], [32.0, 32.0], [-32.0, 32.0]],
            number: 0,
            low: None,
            high: None,
            priority: 0,
            no_structures: false,
            tags: Vec::new(),
        }
    }
}

/// A world's own data for each tag, where no area there carries it: for a
/// biome first, then for anywhere. On the world's Obj, or any in its
/// dimension.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AreaDefaults {
    pub tags: Vec<AreaEntry>,
}

impl voxear_ecs::Component for AreaDefaults {}

impl WorldArea {
    /// Its outline's corners in the world, along x and z.
    pub fn world_outline(&self, placed: &Mat4) -> Vec<[f32; 2]> {
        let corner = |[x, z]: [f32; 2]| {
            let at = placed.transform_point3(Vec3::new(x, 0.0, z));
            [at.x, at.z]
        };
        self.outline.iter().copied().map(corner).collect()
    }

    /// Whether it holds `at`, `painted` being the number painted there.
    pub fn holds(&self, placed: &Mat4, at: Vec3, painted: Option<u32>) -> bool {
        let within = match self.shape {
            AreaShape::Outline => {
                let outline = self.world_outline(placed);
                outline.len() >= 3 && inside(&outline, [at.x, at.z])
            }
            AreaShape::Painted => self.number != 0 && painted == Some(self.number),
        };
        within && self.low.is_none_or(|low| at.y >= low) && self.high.is_none_or(|high| at.y <= high)
    }

    /// It as the generator sees it, for a world lying at `place`.
    pub fn layer(&self, placed: &Mat4, place: WorldPlace) -> AreaLayer {
        let size = place.block_size.max(0.01);
        let outline = match self.shape {
            AreaShape::Outline => self
                .world_outline(placed)
                .into_iter()
                .map(|[x, z]| [(x - place.corner.x) / size, (z - place.corner.z) / size])
                .collect(),
            AreaShape::Painted => Vec::new(),
        };
        let block = |metres: f32| ((metres - place.corner.y) / size).floor() as i32;
        AreaLayer {
            id: 0,
            outline,
            painted: if self.shape == AreaShape::Painted { self.number } else { 0 },
            low: self.low.map(block),
            high: self.high.map(block),
            tags: self.tags.iter().filter_map(|entry| entry.tag).map(AssetGuid::to_u128).collect(),
            no_structures: self.no_structures,
            priority: self.priority,
        }
    }

    /// The data its tag `tag` carries, if it has the tag.
    pub fn value(&self, tag: AssetGuid) -> Option<Option<AssetGuid>> {
        self.tags.iter().find(|entry| entry.tag == Some(tag)).map(|entry| entry.data)
    }
}

/// The World Areas whose structures rules the generator of the world of
/// blocks on `blocks`, lying at `place`, keeps to: those of the Objs in its
/// dimension that say anything of structures.
pub fn layers(world: &World, blocks: Entity, place: WorldPlace) -> Vec<AreaLayer> {
    let here = voxear_scene::dimension_of(world, blocks);
    let areas = world.query_ref::<(Entity, &WorldArea, &WorldTransform)>();
    let areas = areas.without::<voxear_scene::Destroying>().into_iter();
    let shown = |obj: Entity| voxear_scene::is_active(world, obj) && voxear_scene::dimension_of(world, obj) == here;
    let ruling = |area: &WorldArea| area.no_structures || area.tags.iter().any(|entry| entry.tag.is_some());
    areas
        .filter(|(obj, area, _)| shown(*obj) && ruling(area))
        .filter(|(_, area, _)| area.shape == AreaShape::Outline || area.number != 0)
        .map(|(obj, area, placed)| AreaLayer { id: obj.to_bits().max(1), ..area.layer(&placed.0, place) })
        .collect()
}

/// The world of blocks without end in `obj`'s dimension (the level's where
/// none), as areas are painted on.
fn endless_world(world: &World, dimension: voxear_scene::DimensionId) -> Option<Entity> {
    world
        .query_ref::<(Entity, &crate::blocks::BlockWorldAsset)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
        .map(|(entity, _)| entity)
        .find(|entity| voxear_scene::dimension_of(world, *entity) == dimension)
}

/// The number painted on the Area map of the world of blocks on `blocks`
/// under `at`.
pub fn painted_at(world: &World, content: &mut crate::content::Content, blocks: Entity, at: Vec3) -> Option<u32> {
    let asset = world.get::<crate::blocks::BlockWorldAsset>(blocks)?;
    let position = asset.file.generate.as_ref()?.position;
    let guid = asset.guid;
    let corner = crate::ground_fit::world_corner(world, blocks)?;
    let painted = content.painted(guid, false)?;
    let number = painted.maps().areas.nearest(position[0] + at.x - corner.x, position[1] + at.z - corner.z)?;
    (number >= 1.0).then_some(number.round() as u32)
}

/// The World Areas holding `at` in the dimension the game is seen from,
/// highest priority first.
pub fn at(world: &World, content: &mut crate::content::Content, at: Vec3) -> Vec<Entity> {
    let here = crate::dimensions::current(world);
    let painted = endless_world(world, here).and_then(|blocks| painted_at(world, content, blocks, at));
    let areas = world.query_ref::<(Entity, &WorldArea, &WorldTransform)>();
    let mut found: Vec<(i32, u64, Entity)> = areas
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(obj, ..)| voxear_scene::is_active(world, *obj) && voxear_scene::dimension_of(world, *obj) == here)
        .filter(|(_, area, placed)| area.holds(&placed.0, at, painted))
        .map(|(obj, area, _)| (-area.priority, obj.to_bits(), obj))
        .collect();
    found.sort();
    found.into_iter().map(|(.., obj)| obj).collect()
}

/// The data of the tag `tag` at `at`: of the first area holding `at` with the
/// tag; where none has it, the world's default for the biome there, then for
/// anywhere. `Some(None)` where the tag is there with no data.
pub fn value(
    world: &World,
    content: &mut crate::content::Content,
    point: Vec3,
    tag: AssetGuid,
) -> Option<Option<AssetGuid>> {
    for obj in at(world, content, point) {
        if let Some(found) = world.get::<WorldArea>(obj).and_then(|area| area.value(tag)) {
            return Some(found);
        }
    }
    let here = crate::dimensions::current(world);
    let defaults: Vec<AreaEntry> = world
        .query_ref::<(Entity, &AreaDefaults)>()
        .into_iter()
        .filter(|(obj, _)| voxear_scene::dimension_of(world, *obj) == here)
        .flat_map(|(_, defaults)| defaults.tags.clone())
        .filter(|entry| entry.tag == Some(tag))
        .collect();
    if defaults.is_empty() {
        return None;
    }
    let biome =
        defaults.iter().any(|entry| entry.biome.is_some()).then(|| crate::blocks::biome_at(world, content, point));
    let for_biome = defaults.iter().find(|entry| entry.biome.is_some() && entry.biome == biome.flatten());
    let anywhere = defaults.iter().find(|entry| entry.biome.is_none());
    for_biome.or(anywhere).map(|entry| entry.data)
}

/// The number the World Area on `obj` is painted with: given it, one past the
/// highest any area has, the first time, as it becomes a painted area.
pub fn number_for(world: &mut World, obj: Entity) -> Option<u32> {
    let next = world.query_ref::<&WorldArea>().into_iter().map(|area| area.number).max().unwrap_or(0) + 1;
    let area = world.get_mut::<WorldArea>(obj)?;
    if area.number == 0 {
        area.number = next;
    }
    area.shape = AreaShape::Painted;
    Some(area.number)
}

/// Answers a script's question about the world's areas, as JSON: `{ "ask":
/// "areas_at", "at": [x, y, z] }` gives the Objs, highest priority first;
/// `{ "ask": "area_value", "at", "tag": its GUID }` the data's GUID, or
/// nothing.
pub fn answer(
    world: &World,
    content: &mut crate::content::Content,
    asked: &serde_json::Value,
) -> anyhow::Result<String> {
    let at = asked["at"].as_array().map(|v| v.iter().filter_map(serde_json::Value::as_f64).collect::<Vec<f64>>());
    let Some([x, y, z]) = at.and_then(|at| <[f64; 3]>::try_from(at).ok()) else { anyhow::bail!("`at` is not a place") };
    let point = Vec3::new(x as f32, y as f32, z as f32);
    match asked["ask"].as_str() {
        Some("areas_at") => {
            let found: Vec<u64> = at_point(world, content, point);
            Ok(serde_json::json!(found).to_string())
        }
        Some("area_value") => {
            let tag = asked["tag"].as_str().and_then(|guid| guid.parse::<AssetGuid>().ok());
            let Some(tag) = tag else { anyhow::bail!("`tag` is not an asset") };
            Ok(value(world, content, point, tag).map_or_else(String::new, |data| {
                serde_json::json!({ "data": data.map(|d| d.to_string()) }).to_string()
            }))
        }
        other => anyhow::bail!("no such question about the world's areas: {other:?}"),
    }
}

fn at_point(world: &World, content: &mut crate::content::Content, point: Vec3) -> Vec<u64> {
    at(world, content, point).into_iter().map(Entity::to_bits).collect()
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

    #[test]
    fn a_world_area_holds_its_outline_between_its_heights_and_turns_with_its_obj() {
        let area = WorldArea { low: Some(0.0), high: Some(30.0), ..WorldArea::default() };
        let placed = Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0));
        assert!(area.holds(&placed, Vec3::new(120.0, 10.0, 0.0), None));
        assert!(!area.holds(&placed, Vec3::new(120.0, 40.0, 0.0), None), "over it");
        assert!(!area.holds(&placed, Vec3::new(20.0, 10.0, 0.0), None), "off it");
        let painted = WorldArea { shape: AreaShape::Painted, number: 3, ..WorldArea::default() };
        assert!(painted.holds(&placed, Vec3::ZERO, Some(3)));
        assert!(!painted.holds(&placed, Vec3::ZERO, Some(2)));
        let layer = area.layer(&placed, WorldPlace { corner: Vec3::new(0.0, -10.0, 0.0), block_size: 2.0 });
        assert_eq!(layer.outline[0], [34.0, -16.0]);
        assert_eq!((layer.low, layer.high), (Some(5), Some(20)));
    }
}
