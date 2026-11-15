//! A level's baked lighting: the lightmaps and light probes a lighting bake
//! made (see `voxear_render::lightmapper`), saved beside the level as its
//! `.lightmaps` file (see [`lightmaps_file`]). As the level opens, the file
//! is read: its lightmaps and probes go on the world as a
//! [`BakedLighting`] resource, and each part it names is given its place in
//! them, a [`LightmapBake`] on its Obj. Builds ship the file with its level.
//!
//! The file is kept small and quick to read, as it goes over the network to
//! players joining: a short header, then, compressed with LZ4, each
//! lightmap's size and texels, as the GPU takes them (BC6H for the light,
//! BC7 for its directions, a byte a texel each), each part's scene by GUID,
//! its Obj's path in that scene, which part and where it lies in which
//! lightmap, and each probe's place and its nine harmonics as half floats.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::World;
use voxear_render::{BakedAtlas, BakedLighting, BakedProbes, LightBake, LightmapBake, LightmapPlace, ProbeLight};

use crate::reflection_bakes::{f32_to_half, half_to_f32};
use crate::{content::Content, levels::Levels, prefab::ObjPath};

const MAGIC: [u8; 4] = *b"VXLM";
const VERSION: u16 = 1;
/// The most a file may say it holds, so a broken one cannot ask for more
/// memory than any bake takes.
const MOST_SIZE: u32 = 16384;
const MOST_PAYLOAD: usize = 1 << 30;

/// Where a level's baked lighting is kept: beside it, named as it is, as
/// `Levels/Main.level`'s in `Levels/Main.lightmaps`.
pub fn lightmaps_file(level: &str) -> String {
    let stem = level.strip_suffix(".level").unwrap_or(level);
    format!("{stem}.lightmaps")
}

/// A part's place in the bake, by its Obj's scene and path in that scene.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedPart {
    pub scene: AssetGuid,
    pub obj: ObjPath,
    pub part: u32,
    pub place: LightmapPlace,
}

/// A level's bake as its file keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedLevel {
    pub atlases: Vec<BakedAtlas>,
    pub parts: Vec<BakedPart>,
    pub probes: BakedProbes,
}

impl BakedLevel {
    /// What a bake made, with each part's Obj named by where its scene keeps
    /// it; a part of an Obj no open scene keeps is left out.
    pub fn of(levels: &Levels, bake: &LightBake) -> Self {
        let parts = bake
            .places
            .iter()
            .filter_map(|&(entity, part, place)| {
                let (scene, obj) = levels.place_of(entity)?;
                Some(BakedPart { scene, obj, part, place })
            })
            .collect();
        Self { atlases: bake.atlases.clone(), parts, probes: bake.probes.clone() }
    }
}

/// Puts the bake of the open level's `.lightmaps` file on the world as the
/// level opens, in place of any bake before; a level without one is lit
/// without, and a file that cannot be read is passed over with a warning.
/// Returns how many parts were given a place.
pub fn put_on_level(levels: &Levels, world: &mut World, content: &mut Content) -> usize {
    world.remove_resource::<BakedLighting>();
    let Some(level) = levels.current() else { return 0 };
    let path = lightmaps_file(level);
    let Ok(bytes) = content.read(&path) else { return 0 };
    match read(&bytes) {
        Ok(baked) => put_on(levels, world, baked),
        Err(error) => {
            tracing::warn!("could not read {path}: {error:#}");
            0
        }
    }
}

/// Puts a bake on the world: its lightmaps and probes as a resource, and
/// each part's place on its Obj, in place of what those Objs had. Returns
/// how many parts were given a place.
pub fn put_on(levels: &Levels, world: &mut World, baked: BakedLevel) -> usize {
    let mut places: BTreeMap<u64, (voxear_ecs::Entity, Vec<Option<LightmapPlace>>)> = BTreeMap::new();
    for part in &baked.parts {
        let Some(entity) = levels.obj_at(part.scene, &part.obj) else { continue };
        let (_, list) = places.entry(entity.to_bits()).or_insert_with(|| (entity, Vec::new()));
        let index = part.part as usize;
        if list.len() <= index {
            list.resize(index + 1, None);
        }
        list[index] = Some(part.place);
    }
    let stale: Vec<_> = world.query_ref::<(voxear_ecs::Entity, &LightmapBake)>().into_iter().map(|(e, _)| e).collect();
    for entity in stale {
        world.remove::<LightmapBake>(entity);
    }
    let put = places.len();
    for (_, (entity, list)) in places {
        world.insert(entity, LightmapBake(list.into()));
    }
    world.insert_resource(BakedLighting::new(baked.atlases, baked.probes));
    put
}

/// The `.lightmaps` file of a bake.
pub fn write(baked: &BakedLevel) -> Vec<u8> {
    let mut p = Vec::new();
    let u32s = |p: &mut Vec<u8>, v: u32| p.extend_from_slice(&v.to_le_bytes());
    let f32s = |p: &mut Vec<u8>, v: f32| p.extend_from_slice(&v.to_le_bytes());
    u32s(&mut p, baked.atlases.len() as u32);
    for atlas in &baked.atlases {
        u32s(&mut p, atlas.width);
        u32s(&mut p, atlas.height);
        p.push(u8::from(atlas.directions.is_some()));
        p.extend_from_slice(&atlas.light);
        if let Some(directions) = &atlas.directions {
            p.extend_from_slice(directions);
        }
    }
    u32s(&mut p, baked.parts.len() as u32);
    for part in &baked.parts {
        p.extend_from_slice(&part.scene.to_u128().to_le_bytes());
        let ids = part.obj.ids();
        u32s(&mut p, ids.len() as u32);
        for &id in ids {
            u32s(&mut p, id);
        }
        u32s(&mut p, part.part);
        u32s(&mut p, part.place.atlas);
        for v in part.place.scale_offset {
            f32s(&mut p, v);
        }
    }
    u32s(&mut p, baked.probes.positions.len() as u32);
    for (position, light) in baked.probes.positions.iter().zip(&baked.probes.lights) {
        for v in position.to_array() {
            f32s(&mut p, v);
        }
        for c in light.0.iter().flatten() {
            p.extend_from_slice(&f32_to_half(*c).to_le_bytes());
        }
    }
    let mut file = Vec::with_capacity(p.len() + 10);
    file.extend_from_slice(&MAGIC);
    file.extend_from_slice(&VERSION.to_le_bytes());
    file.extend_from_slice(&(p.len() as u32).to_le_bytes());
    file.extend_from_slice(&lz4_flex::block::compress(&p));
    file
}

/// The bake a `.lightmaps` file holds.
pub fn read(file: &[u8]) -> Result<BakedLevel> {
    ensure!(file.len() >= 10 && file[..4] == MAGIC, "not a lightmaps file");
    let version = u16::from_le_bytes([file[4], file[5]]);
    ensure!(version == VERSION, "lightmaps file version {version}, where this engine reads {VERSION}");
    let size = u32::from_le_bytes(file[6..10].try_into().expect("four bytes")) as usize;
    ensure!(size <= MOST_PAYLOAD, "a lightmaps file that says it holds more than any bake");
    let payload = lz4_flex::block::decompress(&file[10..], size).context("uncompressing lightmaps")?;
    ensure!(payload.len() == size, "a lightmaps file cut short");
    let mut r = Reader { bytes: &payload, at: 0 };
    let mut baked = BakedLevel::default();
    for _ in 0..r.u32()? {
        let (width, height) = (r.u32()?, r.u32()?);
        ensure!(
            width % 4 == 0 && height % 4 == 0 && width <= MOST_SIZE && height <= MOST_SIZE,
            "a lightmap {width}×{height}"
        );
        let directional = r.take(1)?[0] != 0;
        let texels = (width * height) as usize;
        let light = r.take(texels)?.to_vec();
        let directions = if directional { Some(r.take(texels)?.to_vec()) } else { None };
        baked.atlases.push(BakedAtlas { width, height, light, directions });
    }
    for _ in 0..r.u32()? {
        let scene = AssetGuid::from_u128(u128::from_le_bytes(r.take(16)?.try_into().expect("sixteen bytes")));
        let depth = r.u32()? as usize;
        ensure!(depth <= 64, "an Obj path {depth} deep");
        let ids: Vec<String> = (0..depth).map(|_| r.u32().map(|id| id.to_string())).collect::<Result<_>>()?;
        let obj = ObjPath::parse(&ids.join("/"))?;
        let part = r.u32()?;
        let atlas = r.u32()?;
        ensure!((atlas as usize) < baked.atlases.len(), "a part in lightmap {atlas}, which is not there");
        let scale_offset = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        baked.parts.push(BakedPart { scene, obj, part, place: LightmapPlace { atlas, scale_offset } });
    }
    for _ in 0..r.u32()? {
        let position = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let halves = r.take(27 * 2)?;
        let value = |i: usize| half_to_f32(u16::from_le_bytes([halves[i * 2], halves[i * 2 + 1]]));
        let light = ProbeLight(std::array::from_fn(|c| [value(c * 3), value(c * 3 + 1), value(c * 3 + 2)]));
        baked.probes.positions.push(position);
        baked.probes.lights.push(light);
    }
    Ok(baked)
}

/// Copies an edited world's bake into a world made from it to play, as the
/// play copy's Objs are those of the same places.
pub fn copy_to(world: &World, into: &mut World, matching: impl Fn(voxear_ecs::Entity) -> Option<voxear_ecs::Entity>) {
    if let Some(baked) = world.resource::<BakedLighting>() {
        into.insert_resource(baked.clone());
    }
    let bakes: Vec<_> =
        world.query_ref::<(voxear_ecs::Entity, &LightmapBake)>().into_iter().map(|(e, b)| (e, b.clone())).collect();
    for (entity, bake) in bakes {
        if let Some(played) = matching(entity) {
            into.insert(played, bake);
        }
    }
}

/// How much of a bake's lightmaps and probes take, in bytes on the GPU.
pub fn gpu_bytes(baked: &BakedLighting) -> usize {
    let lightmaps: usize =
        baked.atlases.iter().map(|a| a.light.len() + a.directions.as_ref().map_or(0, Vec::len)).sum();
    lightmaps + baked.probes.positions.len() * size_of::<ProbeLight>()
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len());
        let end = end.context("a lightmaps file cut short")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_levels_lightmaps_are_kept_beside_it() {
        assert_eq!(lightmaps_file("Levels/Main.level"), "Levels/Main.lightmaps");
    }

    #[test]
    fn a_lightmaps_file_reads_back_what_was_written() {
        let scene = AssetGuid::new_random();
        let light = ProbeLight(std::array::from_fn(|i| [i as f32 * 0.25, 1.0, -0.5]));
        let baked = BakedLevel {
            atlases: vec![
                BakedAtlas { width: 8, height: 4, light: (0..32).collect(), directions: Some(vec![7; 32]) },
                BakedAtlas { width: 4, height: 4, light: vec![3; 16], directions: None },
            ],
            parts: vec![BakedPart {
                scene,
                obj: ObjPath::parse("3/2").unwrap(),
                part: 1,
                place: LightmapPlace { atlas: 1, scale_offset: [0.5, 0.25, 0.125, 0.0] },
            }],
            probes: BakedProbes { positions: vec![Vec3::new(1.0, 2.0, 3.0)], lights: vec![light] },
        };
        let file = write(&baked);
        let back = read(&file).unwrap();
        assert_eq!(back.atlases, baked.atlases);
        assert_eq!(back.parts, baked.parts);
        assert_eq!(back.probes.positions, baked.probes.positions);
        assert_eq!(back.probes.lights[0], light, "harmonics this plain are exact as half floats");
        assert!(read(&file[..file.len() - 2]).is_err(), "a file cut short is refused");
        assert!(read(b"VXRP\x01\x00\x00\x00\x00\x00").is_err());
    }
}
