//! Painting a world of blocks' land, height and biomes (see
//! `voxear_terrain::generate::painted`): a dab of a brush at a place in the
//! world, by the editor's brushes, `vox editor call paint.map` and scripts.
//! Its maps are begun where it has none, and the world makes again the
//! columns a dab reaches.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_terrain::generate::painted::{PaintKind, PaintedMap};

/// Which map a dab paints, or all three taken back to the generator alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintMap {
    Land,
    Height,
    /// A biome, by its place in the world's list.
    Biome(usize),
    Erase,
}

/// What a dab does: adds, takes away (Shift), or smooths (Ctrl).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaintMode {
    #[default]
    Add,
    TakeAway,
    Smooth,
}

/// One dab: where in the world, how wide in metres, how strong from 0 to 1,
/// and how much of its radius eases out, from 0 to 1.
#[derive(Clone, Copy, Debug)]
pub struct Dab {
    pub map: PaintMap,
    pub mode: PaintMode,
    pub at: Vec3,
    pub radius: f32,
    pub strength: f32,
    pub falloff: f32,
}

/// A dab given as JSON, as scripts give it: `{ "map": "land" | "height" |
/// "biome" | "erase", "at": [x, y, z], "radius", "strength", "falloff",
/// "mode": "add" | "take_away" | "smooth", "biome": its place in the list }`.
pub fn paint_json(world: &World, content: &mut crate::content::Content, blocks: Entity, json: &str) -> bool {
    let Ok(asked) = serde_json::from_str::<serde_json::Value>(json) else { return false };
    let number = |key: &str, default: f32| asked[key].as_f64().map_or(default, |v| v as f32);
    let at = asked["at"].as_array().map(|v| v.iter().filter_map(serde_json::Value::as_f64).collect::<Vec<f64>>());
    let Some([x, y, z]) = at.and_then(|at| <[f64; 3]>::try_from(at).ok()) else { return false };
    let map = match asked["map"].as_str() {
        Some("land") => PaintMap::Land,
        Some("height") => PaintMap::Height,
        Some("biome") => PaintMap::Biome(asked["biome"].as_u64().unwrap_or(0) as usize),
        Some("erase") => PaintMap::Erase,
        _ => return false,
    };
    let mode = match asked["mode"].as_str() {
        Some("take_away") => PaintMode::TakeAway,
        Some("smooth") => PaintMode::Smooth,
        _ => PaintMode::Add,
    };
    let at = Vec3::new(x as f32, y as f32, z as f32);
    let dab = Dab {
        map,
        mode,
        at,
        radius: number("radius", 40.0),
        strength: number("strength", 0.3),
        falloff: number("falloff", 0.5),
    };
    paint(world, content, blocks, &dab)
}

/// A dab on the world of blocks on `blocks`; whether it changed anything.
pub fn paint(world: &World, content: &mut crate::content::Content, blocks: Entity, dab: &Dab) -> bool {
    let Some(asset) = world.get::<crate::blocks::BlockWorldAsset>(blocks) else { return false };
    let Some(generate) = asset.file.generate.as_ref() else { return false };
    let Some(corner) = crate::ground_fit::world_corner(world, blocks) else { return false };
    let Some(painted) = content.painted(asset.guid, true) else { return false };
    // The generator's own metres: the world lies at its position in them.
    let at = [generate.position[0] + dab.at.x - corner.x, generate.position[1] + dab.at.z - corner.z];
    let (radius, falloff) = (dab.radius.max(0.5), dab.falloff.clamp(0.0, 1.0));
    let strength = dab.strength.clamp(0.0, 1.0);
    painted.change(|maps| {
        let mut reached: Vec<[[f32; 2]; 2]> = Vec::new();
        let mut field = |map: &mut PaintedMap| {
            let before = map.clone();
            let stroke = |was: Option<f32>, cover: f32| -> Option<f32> {
                let now = was.unwrap_or(0.0);
                let next = match dab.mode {
                    PaintMode::Add => now + strength * cover,
                    PaintMode::TakeAway => now - strength * cover,
                    PaintMode::Smooth => now,
                };
                let next = next.clamp(-1.0, 1.0);
                (next.abs() > 1e-3).then_some(next)
            };
            // Smoothing eases each pixel towards the four round it, as they were before the dab.
            let spacing = before.spacing;
            let smooth = |was: Option<f32>, cover: f32, px: [f32; 2]| -> Option<f32> {
                let round = [[spacing, 0.0], [-spacing, 0.0], [0.0, spacing], [0.0, -spacing]]
                    .map(|[dx, dz]| before.sample(px[0] + dx, px[1] + dz, 0.0).unwrap_or(0.0));
                let average = round.iter().sum::<f32>() / 4.0;
                let now = was.unwrap_or(0.0);
                let next = now + (average - now) * strength * cover;
                (next.abs() > 1e-3).then_some(next)
            };
            let area = match dab.mode {
                PaintMode::Smooth => map.paint_at(at, radius, falloff, &smooth),
                _ => map.paint(at, radius, falloff, &stroke),
            };
            reached.extend(area);
        };
        match dab.map {
            PaintMap::Land => field(&mut maps.land),
            PaintMap::Height => field(&mut maps.height),
            PaintMap::Biome(index) => {
                let set = |was: Option<f32>, cover: f32| match (dab.mode, cover > 0.5) {
                    (PaintMode::TakeAway, true) => None,
                    (_, true) => Some(index as f32),
                    _ => was,
                };
                reached.extend(maps.biome.paint(at, radius, falloff, &set));
            }
            PaintMap::Erase => {
                for kind in PaintKind::ALL {
                    let gone = |was: Option<f32>, cover: f32| if cover > 0.0 { None } else { was };
                    reached.extend(maps.map_mut(kind).paint(at, radius, falloff, &gone));
                }
            }
        }
        for area in &reached {
            maps.changed(*area);
        }
        !reached.is_empty()
    })
}
