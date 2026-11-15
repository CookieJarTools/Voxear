//! Baked reflection probes: what a level's probes captured, saved beside the
//! level as its `.probes` file (see [`probes_file`]). As the level opens,
//! each probe the file names is given its bake, a [`ReflectionProbeBake`],
//! which the renderer shows in place of capturing it, so the game opens with
//! its reflections ready and the same every time. Builds ship the file with
//! its level.
//!
//! The file is kept small and quick to read, as it goes over the network to
//! players joining: a short header, then, compressed with LZ4, each probe's
//! scene by GUID and its Obj's path in that scene, how many texels across a
//! face is, and its six faces' texels as RGB9E5, three 9-bit numbers
//! sharing a 5-bit exponent, four bytes a texel. The texels are written a
//! byte of each at a time, the first bytes of every texel, then the
//! seconds, and so on, which LZ4 finds more alike than whole texels.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_render::{ProbePixels, ProbeUpdate, ReflectionProbe, ReflectionProbeBake, Renderer};
use voxear_scene::{Destroying, Switch, Switched};

use crate::{content::Content, levels::Levels, prefab::ObjPath};

const MAGIC: [u8; 4] = *b"VXRP";
const VERSION: u16 = 1;
/// The most texels across a face a file may hold, so a broken file cannot
/// ask for more memory than any probe takes.
const MOST_SIZE: u32 = 1024;
/// Half floats' 1, for the unused alpha of a loaded texel.
const HALF_ONE: u16 = 0x3c00;

/// Where a level's baked reflection probes are kept: beside it, named as it
/// is, as `Levels/Main.level`'s in `Levels/Main.probes`.
pub fn probes_file(level: &str) -> String {
    let stem = level.strip_suffix(".level").unwrap_or(level);
    format!("{stem}.probes")
}

/// One probe's bake: the Obj it is on, by its scene and its path in that
/// scene, and what it captured.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedProbe {
    pub scene: AssetGuid,
    pub obj: ObjPath,
    pub pixels: ProbePixels,
}

/// The probes a bake reads: those captured once that are switched on and
/// kept in an open scene, each with where its scene keeps it. A live probe
/// is captured as the game goes, so it has nothing to bake.
pub fn bakeable(levels: &Levels, world: &World) -> Vec<(Entity, AssetGuid, ObjPath)> {
    let switched = Switched::of(world);
    let probes = world.query_ref::<(Entity, &ReflectionProbe)>().without::<Destroying>();
    let mut found: Vec<(Entity, AssetGuid, ObjPath)> = probes
        .into_iter()
        .filter(|(_, probe)| probe.update == ProbeUpdate::Once)
        .filter(|(entity, _)| switched.is_on(world, *entity, Switch::ReflectionProbe))
        .filter_map(|(entity, _)| levels.place_of(entity).map(|(scene, path)| (entity, scene, path)))
        .collect();
    found.sort_by_key(|(entity, ..)| entity.to_bits());
    found
}

/// Reads back what each of `probes` captured, as [`bakeable`] lists them.
/// One the renderer has not captured yet is left out.
pub fn read_back(renderer: &Renderer, probes: &[(Entity, AssetGuid, ObjPath)]) -> Result<Vec<(Entity, BakedProbe)>> {
    let mut baked = Vec::with_capacity(probes.len());
    for (entity, scene, obj) in probes {
        if let Some(pixels) = renderer.reflection_probe_pixels(*entity)? {
            baked.push((*entity, BakedProbe { scene: *scene, obj: obj.clone(), pixels }));
        }
    }
    Ok(baked)
}

/// Puts the bakes of the open level's `.probes` file on the probes it
/// names, as the level opens. A probe that has a bake already keeps it, as
/// one the editor has just baked; a file that is missing gives none, and
/// one that cannot be read is passed over with a warning. Returns how many
/// probes were given their bake.
pub fn put_on_level(levels: &Levels, world: &mut World, content: &mut Content) -> usize {
    let Some(level) = levels.current() else { return 0 };
    let path = probes_file(level);
    let Ok(bytes) = content.read(&path) else { return 0 };
    match read(&bytes) {
        Ok(probes) => put_on(levels, world, probes),
        Err(error) => {
            tracing::warn!("could not read {path}: {error:#}");
            0
        }
    }
}

/// Puts each bake on the probe it names that has none yet. Returns how many
/// were put on.
pub fn put_on(levels: &Levels, world: &mut World, probes: Vec<BakedProbe>) -> usize {
    let mut put = 0;
    for probe in probes {
        let Some(entity) = levels.obj_at(probe.scene, &probe.obj) else { continue };
        if !world.has::<ReflectionProbe>(entity) || world.has::<ReflectionProbeBake>(entity) {
            continue;
        }
        world.insert(entity, ReflectionProbeBake(Arc::new(probe.pixels)));
        put += 1;
    }
    put
}

/// The `.probes` file of `probes`.
pub fn write(probes: &[BakedProbe]) -> Vec<u8> {
    let mut payload = Vec::new();
    for probe in probes {
        payload.extend_from_slice(&probe.scene.to_u128().to_le_bytes());
        let ids = probe.obj.ids();
        payload.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in ids {
            payload.extend_from_slice(&id.to_le_bytes());
        }
        payload.extend_from_slice(&probe.pixels.size.to_le_bytes());
        let packed: Vec<[u8; 4]> = probe
            .pixels
            .texels
            .iter()
            .map(|&[r, g, b, _]| pack_rgb9e5([r, g, b].map(half_to_f32)).to_le_bytes())
            .collect();
        for byte in 0..4 {
            payload.extend(packed.iter().map(|texel| texel[byte]));
        }
    }
    let mut file = Vec::with_capacity(payload.len() / 2 + 16);
    file.extend_from_slice(&MAGIC);
    file.extend_from_slice(&VERSION.to_le_bytes());
    file.extend_from_slice(&(probes.len() as u32).to_le_bytes());
    file.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    file.extend_from_slice(&lz4_flex::block::compress(&payload));
    file
}

/// The probes a `.probes` file holds.
pub fn read(file: &[u8]) -> Result<Vec<BakedProbe>> {
    ensure!(file.len() >= 14 && file[..4] == MAGIC, "not a reflection probes file");
    let version = u16::from_le_bytes([file[4], file[5]]);
    ensure!(version == VERSION, "reflection probes file version {version}, where this engine reads {VERSION}");
    let count = u32::from_le_bytes(file[6..10].try_into().expect("four bytes")) as usize;
    let size = u32::from_le_bytes(file[10..14].try_into().expect("four bytes")) as usize;
    let most = count.saturating_mul(64 + 6 * (MOST_SIZE * MOST_SIZE) as usize * 4);
    ensure!(size <= most, "a reflection probes file that says it holds more than its probes could");
    let payload = lz4_flex::block::decompress(&file[14..], size).context("uncompressing reflection probes")?;
    ensure!(payload.len() == size, "a reflection probes file cut short");
    let mut reader = Reader { bytes: &payload, at: 0 };
    let mut probes = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let scene = AssetGuid::from_u128(u128::from_le_bytes(reader.take(16)?.try_into().expect("sixteen bytes")));
        let depth = reader.u32()? as usize;
        ensure!(depth <= 64, "an Obj path {depth} deep");
        let ids: Vec<String> = (0..depth).map(|_| reader.u32().map(|id| id.to_string())).collect::<Result<_>>()?;
        let obj = ObjPath::parse(&ids.join("/"))?;
        let size = reader.u32()?;
        if size == 0 || size > MOST_SIZE {
            bail!("a reflection probe {size} texels across");
        }
        let texels = (size * size * 6) as usize;
        let planes = reader.take(texels * 4)?;
        let texels = (0..texels)
            .map(|i| {
                let packed =
                    u32::from_le_bytes([planes[i], planes[texels + i], planes[2 * texels + i], planes[3 * texels + i]]);
                let [r, g, b] = unpack_rgb9e5(packed).map(f32_to_half);
                [r, g, b, HALF_ONE]
            })
            .collect();
        probes.push(BakedProbe { scene, obj, pixels: ProbePixels { size, texels } });
    }
    Ok(probes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len());
        let end = end.context("a reflection probes file cut short")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
}

const MANTISSA_BITS: i32 = 9;
const EXPONENT_BIAS: i32 = 15;
const MOST_EXPONENT: i32 = 31;
/// The brightest a channel of RGB9E5 can hold.
const MOST_RGB9E5: f32 = (511.0 / 512.0) * 65536.0;

/// Red, green and blue as RGB9E5: each a 9-bit number, sharing a 5-bit
/// exponent, as the shared-exponent texture format packs them. Negative
/// and not-a-number channels are black.
pub fn pack_rgb9e5(rgb: [f32; 3]) -> u32 {
    let [r, g, b] = rgb.map(|c| if c > 0.0 { c.min(MOST_RGB9E5) } else { 0.0 });
    let brightest = r.max(g).max(b);
    if brightest <= 0.0 {
        return 0;
    }
    let mut exponent = (brightest.log2().floor() as i32).max(-EXPONENT_BIAS - 1) + 1 + EXPONENT_BIAS;
    let step = |exponent: i32| 2f32.powi(exponent - EXPONENT_BIAS - MANTISSA_BITS);
    if (brightest / step(exponent) + 0.5).floor() as i32 >= 1 << MANTISSA_BITS {
        exponent += 1;
    }
    let exponent = exponent.clamp(0, MOST_EXPONENT);
    let scale = step(exponent);
    let [r, g, b] = [r, g, b].map(|c| ((c / scale + 0.5).floor() as u32).min(511));
    r | (g << 9) | (b << 18) | ((exponent as u32) << 27)
}

/// RGB9E5's red, green and blue.
pub fn unpack_rgb9e5(packed: u32) -> [f32; 3] {
    let exponent = (packed >> 27) as i32;
    let scale = 2f32.powi(exponent - EXPONENT_BIAS - MANTISSA_BITS);
    [packed & 511, (packed >> 9) & 511, (packed >> 18) & 511].map(|c| c as f32 * scale)
}

/// A half float's value.
pub fn half_to_f32(half: u16) -> f32 {
    let negative = half & 0x8000 != 0;
    let exponent = u32::from((half >> 10) & 0x1f);
    let mantissa = u32::from(half & 0x3ff);
    let value = match exponent {
        0 => mantissa as f32 * 2f32.powi(-24),
        31 if mantissa == 0 => f32::INFINITY,
        31 => f32::NAN,
        _ => f32::from_bits(((exponent + 127 - 15) << 23) | (mantissa << 13)),
    };
    if negative { -value } else { value }
}

/// The half float nearest `value`, ties to even.
pub fn f32_to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        return sign | 0x7c00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 31 {
        return sign | 0x7c00;
    }
    // Rounds `full`'s bits below `shift` away, to the nearest, ties to even.
    let round = |full: u32, shift: u32| {
        let kept = full >> shift;
        let rest = full & ((1 << shift) - 1);
        let half_way = 1 << (shift - 1);
        if rest > half_way || (rest == half_way && kept & 1 == 1) { kept + 1 } else { kept }
    };
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        return sign | round(mantissa | 0x80_0000, (14 - half_exponent) as u32) as u16;
    }
    // A mantissa that rounds up past its top carries into the exponent, as it should.
    sign | round(((half_exponent as u32) << 23) | mantissa, 13) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_levels_probes_are_kept_beside_it() {
        assert_eq!(probes_file("Levels/Main.level"), "Levels/Main.probes");
        assert_eq!(probes_file("Start.level"), "Start.probes");
    }

    #[test]
    fn half_floats_go_to_floats_and_back() {
        for value in [0.0, 1.0, -2.5, 0.000_06, 65504.0, 0.333_25, 1.0e-7] {
            let half = f32_to_half(value);
            assert_eq!(f32_to_half(half_to_f32(half)), half, "{value}");
            assert!((half_to_f32(half) - value).abs() <= value.abs() / 1000.0 + 1e-7, "{value}");
        }
        assert_eq!(f32_to_half(1.0), HALF_ONE);
        assert_eq!(half_to_f32(f32_to_half(100_000.0)), f32::INFINITY);
    }

    #[test]
    fn rgb9e5_keeps_each_channel_within_a_step_of_the_brightest() {
        for rgb in [[0.0, 0.0, 0.0], [1.0, 0.5, 0.25], [12.0, 0.01, 3.0], [0.002, 0.001, 0.0], [60000.0, 1.0, 0.0]] {
            let back = unpack_rgb9e5(pack_rgb9e5(rgb));
            let brightest = rgb.into_iter().fold(0.0f32, f32::max);
            for (was, now) in rgb.into_iter().zip(back) {
                assert!((was - now).abs() <= brightest / 256.0 + 1e-9, "{rgb:?} came back as {back:?}");
            }
        }
        assert_eq!(unpack_rgb9e5(pack_rgb9e5([-1.0, f32::NAN, 2.0]))[..2], [0.0, 0.0], "black, not garbage");
    }

    #[test]
    fn a_probes_file_reads_back_what_was_written_within_rgb9e5s_steps() {
        let size = 8;
        let texels: Vec<[u16; 4]> = (0..size * size * 6)
            .map(|i| {
                let t = i as f32 / (size * size * 6) as f32;
                [f32_to_half(t * 4.0), f32_to_half(0.5 - t * 0.4), f32_to_half(t * t), 0]
            })
            .collect();
        let scene = AssetGuid::new_random();
        let hall = BakedProbe { scene, obj: ObjPath::parse("3/2").unwrap(), pixels: ProbePixels { size, texels } };
        let flat = ProbePixels { size: 4, texels: vec![[f32_to_half(0.25); 4]; 4 * 4 * 6] };
        let cellar = BakedProbe { scene, obj: ObjPath::new(7), pixels: flat };
        let file = write(&[hall.clone(), cellar.clone()]);
        assert!(file.len() < (size * size * 6 * 4 + 4 * 4 * 6 * 4) as usize, "smaller than the texels: {}", file.len());
        let back = read(&file).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!((back[0].scene, &back[0].obj, back[0].pixels.size), (scene, &hall.obj, size));
        assert_eq!((back[1].obj.ids(), back[1].pixels.size), ([7].as_slice(), 4));
        for (was, now) in hall.pixels.texels.iter().zip(&back[0].pixels.texels) {
            let was = [was[0], was[1], was[2]].map(half_to_f32);
            let now = [now[0], now[1], now[2]].map(half_to_f32);
            let brightest = was.into_iter().fold(0.0f32, f32::max);
            for (a, b) in was.into_iter().zip(now) {
                assert!((a - b).abs() <= brightest / 256.0 + 1e-3 * b.abs() + 1e-7, "{was:?} came back as {now:?}");
            }
        }
        assert!(back[1].pixels.texels.iter().all(|t| half_to_f32(t[0]) == 0.25 && t[3] == HALF_ONE));
        assert!(read(&file[..file.len() - 3]).is_err(), "a file cut short is refused");
        assert!(read(b"not a probe file").is_err());
    }
}
