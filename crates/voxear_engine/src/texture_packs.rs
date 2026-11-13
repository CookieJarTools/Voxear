//! Texture packs: the pictures a game's blocks and materials are drawn
//! with, chosen by whoever plays it. A `.texturepack` is a short list of
//! which picture stands in where, naming what it replaces by its asset: a
//! block's sides, top and bottom, or a material's maps.
//!
//! ```toml
//! format = 1
//! description = "Hand-drawn blocks, sixteen pixels a side."
//! pixels = true
//!
//! [[block]]
//! block = "Blocks/Grass.block"
//! top = "grass_top.png"
//! sides = "grass_side.png"
//! bottom = "dirt.png"
//!
//! [[block]]
//! block = "Blocks/Stone.block"
//! all = { color_map = "stone.png", normal_map = "stone_n.png" }
//!
//! [[block]]
//! block = "Blocks/Glass.block"
//! all = "glass.png"   # five tiles side by side
//! connected = true
//!
//! [[material]]
//! material = "Materials/Planks.material"
//! color_map = "planks.png"
//! ```
//!
//! A block of a colour alone is drawn with the pack's picture as it is. A
//! block with a material, and a material the pack names, keeps everything
//! else its material says (its tint, how rough it is, whether it is cut
//! out) and only its pictures change. `pixels` draws the pack's colour
//! pictures sharp, each pixel a square of one colour however near. A block or
//! a material that is `connected` has pictures of five tiles, and a block's
//! faces drawn with them join those of its kind beside them (see
//! `voxear_terrain::blocks::mesh`). `[grain]`
//! gives every block of a colour alone that the pack has no picture for a
//! speckled picture of its colour, made as the pack is worn.
//!
//! A pack comes from one of three places: the engine's own, built in
//! (`builtin:pixel_grain`), which are grain alone since the engine knows no
//! game's blocks; the game's, a `.texturepack` asset whose pictures are
//! pictures of its project, named by path or GUID; or the player's, a folder
//! in the game's `Textures` folder beside its saves, holding a
//! `.texturepack` and the pictures it names by their files' names (`file:`
//! and the pack file's path). A player's pack reads nothing outside its own
//! folder, and is pictures and settings alone. [`list`] gives them all, for
//! a menu.
//!
//! The pack worn is kept by the game's [`Content`], so it goes with the
//! player from level to level. [`wear`] reads a pack and has every world of
//! blocks take its blocks up again; [`sync`], each tick, brings the
//! renderer's materials up to date with it. `blocks::resolved` gives a block
//! the pack dresses a face of the pack's in place of its material
//! ([`Pack::dress`]), and `blocks::look` has [`face_material`] make it.

use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_render::{MaterialDesc, MaterialHandle, Renderer, TextureId, TextureSource};
use voxear_terrain::blocks::file::{BlockKind, JOINS_BOTTOM, JOINS_SIDES, JOINS_TOP};

use crate::{
    content::{Content, MaterialData, MaterialMap},
    scene_file::default_material,
};

/// The `.texturepack` format this version writes and reads.
pub const TEXTURE_PACK_FORMAT: u32 = 1;

/// What a texture pack's file name ends with.
pub const TEXTURE_PACK_EXTENSION: &str = "texturepack";

/// The folder a player puts texture packs of their own in, beside the
/// game's saves: a folder a pack.
pub const TEXTURES_FOLDER: &str = "Textures";

/// What a pack read from a file outside the game is named by, before its
/// path.
pub const FILE_PREFIX: &str = "file:";

/// What a face of a pack's is named by where a block's material would be,
/// before the block's GUID and which of its faces.
pub const FACE_PREFIX: &str = "texturepack:";

/// The longest side a player's picture may have, in pixels.
const LARGEST_PICTURE: u32 = 4096;

/// The engine's own packs: what each is named by after `builtin:`, what a
/// menu calls it, and its file.
pub const BUILTIN: [(&str, &str, &str); 2] = [
    ("pixel_grain", "Pixel Grain", include_str!("texture_packs/pixel_grain.texturepack")),
    ("soft_grain", "Soft Grain", include_str!("texture_packs/soft_grain.texturepack")),
];

/// A `.texturepack` file as written.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TexturePackFile {
    #[serde(default = "texture_pack_format")]
    pub format: u32,
    /// A line for the menu a player picks it from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Whether its colour pictures are drawn sharp, each pixel a square of
    /// one colour however near, as pixel art is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pixels: bool,
    /// A speckled picture for every block of a colour alone that the pack
    /// has no picture for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain: Option<Grain>,
    #[serde(default, rename = "block", skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockPictures>,
    #[serde(default, rename = "material", skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<MaterialPictures>,
}

fn texture_pack_format() -> u32 {
    TEXTURE_PACK_FORMAT
}

/// A picture made for a block of a colour alone: its colour, each pixel a
/// little lighter or darker.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grain {
    /// How many pixels a block's side is.
    pub pixels: u32,
    /// How far its pixels stray from the block's colour, 0 to 1.
    pub strength: f32,
}

impl Default for Grain {
    fn default() -> Self {
        Self { pixels: 16, strength: 0.5 }
    }
}

impl Grain {
    /// The most pixels a block's side is made.
    pub const MOST: u32 = 128;

    fn within(self) -> Self {
        let strength = if self.strength.is_finite() { self.strength.clamp(0.0, 1.0) } else { 0.5 };
        Self { pixels: self.pixels.clamp(2, Self::MOST), strength }
    }

    /// The picture of a block of `color`, as sRGB bytes, `pixels` a side:
    /// the same for the same block every time, and another for another
    /// block of the same colour, by `seed`.
    pub fn picture(self, color: [f32; 3], seed: u32) -> Vec<u8> {
        let grain = self.within();
        let side = grain.pixels;
        let mut rgba = Vec::with_capacity((side * side * 4) as usize);
        for y in 0..side {
            for x in 0..side {
                // Each pixel its own shade, and each patch of four by four a shade of its own under it.
                let own = unit(hash(x, y, seed)) - 0.5;
                let patch = unit(hash(x / 4, y / 4, seed ^ 0x9E37_79B9)) - 0.5;
                let shade = 1.0 + (own * 0.36 + patch * 0.2) * grain.strength;
                rgba.extend(color.map(|part| (part * shade).clamp(0.0, 1.0)).map(|part| (part * 255.0).round() as u8));
                rgba.push(255);
            }
        }
        rgba
    }
}

fn hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x.wrapping_mul(0x85EB_CA6B) ^ y.wrapping_mul(0xC2B2_AE35) ^ seed.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / (1u32 << 24) as f32
}

/// The pictures a pack draws one block with: `all` for every face, and
/// `sides`, `top` and `bottom` in its place for those.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPictures {
    /// The `.block` it draws, by path or GUID.
    pub block: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<FacePictures>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sides: Option<FacePictures>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<FacePictures>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<FacePictures>,
    /// Whether its pictures here are five tiles each, its faces joining
    /// those of its kind beside them (see `MaterialData::connected`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub connected: bool,
    /// How far up the order of blocks that meet it lies under this pack, in
    /// place of its own (see `BlockDef::overlap`): its picture creeps over
    /// the edge of a block beside it with a lower one. None for its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap: Option<u8>,
}

/// What a face is drawn with: a picture of its colour, or a picture for
/// each map a material has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FacePictures {
    Color(String),
    Maps(MapPictures),
}

impl FacePictures {
    fn maps(&self) -> MapPictures {
        match self {
            Self::Color(color) => MapPictures { color_map: Some(color.clone()), ..MapPictures::default() },
            Self::Maps(maps) => maps.clone(),
        }
    }
}

/// A picture for each of a material's maps, named as a material's file
/// names them; none for a map left as it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapPictures {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_map: Option<String>,
}

impl MapPictures {
    /// Its pictures, in [`MaterialMap::ALL`]'s order.
    fn each(&self) -> [&Option<String>; 5] {
        [&self.color_map, &self.metallic_map, &self.normal_map, &self.emissive_map, &self.height_map]
    }
}

/// The pictures a pack draws one material with, wherever it is drawn.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialPictures {
    /// The `.material` it draws, by path or GUID.
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_map: Option<String>,
    /// Whether its pictures here are five tiles each, for blocks' faces
    /// that join (see `MaterialData::connected`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub connected: bool,
}

impl MaterialPictures {
    fn maps(&self) -> MapPictures {
        MapPictures {
            color_map: self.color_map.clone(),
            metallic_map: self.metallic_map.clone(),
            normal_map: self.normal_map.clone(),
            emissive_map: self.emissive_map.clone(),
            height_map: self.height_map.clone(),
        }
    }
}

impl TexturePackFile {
    /// Reads a pack's file. What it names is looked for when it is worn
    /// ([`load`]).
    pub fn parse(text: &str) -> Result<Self> {
        let mut file: Self = toml::from_str(text)?;
        ensure!(file.format <= TEXTURE_PACK_FORMAT, "written by a newer Voxear (format {})", file.format);
        file.grain = file.grain.map(Grain::within);
        Ok(file)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a texture pack's file writes as TOML")
    }

    /// One of the engine's own packs, by its name.
    pub fn builtin(name: &str) -> Result<Self> {
        let Some((.., text)) = BUILTIN.iter().find(|(key, ..)| *key == name) else {
            bail!("no built-in texture pack named `{name}`");
        };
        Self::parse(text).with_context(|| format!("reading the built-in texture pack {name}"))
    }
}

/// A picture of a pack's: one of the game's own, or a file in a player's
/// pack.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Asset(AssetGuid),
    File(PathBuf),
}

/// A picture for each of a material's maps, in [`MaterialMap::ALL`]'s order.
pub type Maps = [Option<Picture>; 5];

/// What a pack draws a face or a material with: its pictures, and whether
/// they are the five tiles of a face that joins those beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub maps: Maps,
    pub connected: bool,
}

impl Face {
    /// Whether it brings a colour picture, which says whether a face
    /// drawn with it joins; with none, the material's own says.
    fn colors(&self) -> bool {
        self.maps[0].is_some()
    }
}

/// Which of a block's faces a pack's face is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    All,
    Sides,
    Top,
    Bottom,
    /// A picture made of the block's colour (see [`Grain`]).
    Grain,
}

impl Part {
    const NAMED: [(Self, &'static str); 5] = [
        (Self::All, "all"),
        (Self::Sides, "sides"),
        (Self::Top, "top"),
        (Self::Bottom, "bottom"),
        (Self::Grain, "grain"),
    ];

    fn key(self) -> &'static str {
        Self::NAMED.iter().find(|(part, _)| *part == self).map_or("all", |(_, key)| key)
    }

    fn parse(key: &str) -> Option<Self> {
        Self::NAMED.iter().find(|(_, named)| *named == key).map(|(part, _)| *part)
    }
}

/// What names the face `part` of the block at `guid`, where a block's
/// material would be.
fn face_reference(guid: AssetGuid, part: Part) -> String {
    format!("{FACE_PREFIX}{guid}:{}", part.key())
}

/// A pack as it is worn: what names it, and each block and material it
/// draws, found in the game, with the pictures it draws them with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pack {
    pub reference: String,
    pub description: String,
    pub pixels: bool,
    pub grain: Option<Grain>,
    blocks: HashMap<AssetGuid, HashMap<Part, Face>>,
    materials: HashMap<AssetGuid, Face>,
    /// The blocks it gives another place in the order of blocks that meet.
    overlaps: HashMap<AssetGuid, u8>,
}

impl Pack {
    /// Whether it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.materials.is_empty() && self.overlaps.is_empty() && self.grain.is_none()
    }

    /// The blocks it draws with pictures of its own.
    pub fn blocks(&self) -> impl Iterator<Item = AssetGuid> + '_ {
        self.blocks.keys().copied()
    }

    /// The materials it draws with pictures of its own.
    pub fn materials(&self) -> impl Iterator<Item = AssetGuid> + '_ {
        self.materials.keys().copied()
    }

    /// The part of the pack `face` of the block at `guid` is drawn with:
    /// its own, or the one for every face.
    fn part(&self, guid: AssetGuid, face: Part) -> Option<Part> {
        let parts = self.blocks.get(&guid)?;
        [face, Part::All].into_iter().find(|part| parts.contains_key(part))
    }

    /// Gives `kind`, a world's entry for the block at `guid` with the
    /// block's own look, the pack's faces in place of its materials. A
    /// liquid is drawn in its colour alone, so it stays as it is.
    pub fn dress(&self, guid: AssetGuid, kind: &mut BlockKind) {
        if kind.water {
            return;
        }
        if let Some(&overlap) = self.overlaps.get(&guid) {
            kind.overlap = overlap;
        }
        let Some(parts) = self.blocks.get(&guid) else {
            let plain = kind.material.is_none() && kind.top.is_none() && kind.bottom.is_none();
            if plain && self.grain.is_some() {
                kind.material = Some(face_reference(guid, Part::Grain));
            }
            return;
        };
        let faces = [
            (Part::Sides, &mut kind.material, JOINS_SIDES),
            (Part::Top, &mut kind.top, JOINS_TOP),
            (Part::Bottom, &mut kind.bottom, JOINS_BOTTOM),
        ];
        // Before any is dressed: whether each face joins by its own material, its sides' where it has none.
        let joins = kind.connected;
        let own =
            faces.each_ref().map(|(_, material, bit)| joins & if material.is_some() { *bit } else { JOINS_SIDES } != 0);
        let mut connected = 0;
        for ((face, material, bit), own) in faces.into_iter().zip(own) {
            let part = self.part(guid, face);
            if let Some(part) = part {
                *material = Some(face_reference(guid, part));
            }
            // A face joins as the pack's colour picture for it says; with none, as its own material does.
            let dressed = part.and_then(|part| parts.get(&part)).filter(|face| face.colors());
            if dressed.map_or(own, |face| face.connected) {
                connected |= bit;
            }
        }
        kind.connected = connected;
    }
}

/// The pack worn and what the renderer holds for it, kept by the game's
/// [`Content`].
#[derive(Default)]
pub(crate) struct Worn {
    pack: Option<Arc<Pack>>,
    /// Whether the renderer's materials are yet to be brought up to date
    /// with the pack ([`sync`]).
    stale: bool,
    /// The pictures uploaded for it that are no asset's: a player's files,
    /// and those made of blocks' colours, by what each is and whether it is
    /// read as colours.
    pictures: HashMap<(Picture, bool), TextureId>,
    /// The materials made for its blocks' faces, by what names each.
    faces: HashMap<String, MaterialHandle>,
    /// The loaded materials uploaded with pictures of a pack's.
    dressed: HashSet<AssetGuid>,
}

impl Worn {
    pub(crate) fn pack(&self) -> Option<&Arc<Pack>> {
        self.pack.as_ref()
    }
}

/// Where a pack's pictures are looked for.
enum Home<'a> {
    /// None: an engine's own pack is grain alone.
    Builtin,
    /// The game's assets.
    Game,
    /// A player's pack's folder.
    Folder(&'a Path),
}

/// The pack `reference` names: `builtin:` and a name, `file:` and the path
/// of a pack's file outside the game, or a `.texturepack` asset. A block,
/// material or picture it names that is not there is left out, with a
/// warning: a pack made for another version of a game still dresses what it
/// can.
pub fn load(content: &mut Content, reference: &str) -> Result<Pack> {
    let lasting = lasting(content, reference);
    if let Some(name) = reference.strip_prefix("builtin:") {
        return Ok(found(content, &lasting, &TexturePackFile::builtin(name)?, Home::Builtin));
    }
    if let Some(path) = reference.strip_prefix(FILE_PREFIX) {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let file = TexturePackFile::parse(&text).with_context(|| format!("reading {path}"))?;
        let folder = Path::new(path).parent().with_context(|| format!("{path} is in no folder"))?;
        return Ok(found(content, &lasting, &file, Home::Folder(folder)));
    }
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a texture pack"))?;
    // A server build leaves texture packs out: nothing is drawn.
    if content.is_stripped(guid) {
        return Ok(Pack { reference: lasting, ..Pack::default() });
    }
    let (text, path) = content.asset_text(guid, AssetKind::TexturePack)?;
    let file = TexturePackFile::parse(&text).with_context(|| format!("reading {path}"))?;
    Ok(found(content, &lasting, &file, Home::Game))
}

/// `file` with everything it names found: its blocks and materials among
/// the game's, its pictures in `home`.
fn found(content: &Content, reference: &str, file: &TexturePackFile, home: Home) -> Pack {
    let asset = |named: &str, kind: AssetKind| -> Option<AssetGuid> {
        let guid = content.parse(named).ok().and_then(|parsed| parsed.guid());
        let guid = guid.filter(|guid| content.db().get(*guid).is_some_and(|entry| entry.kind == kind));
        if guid.is_none() {
            tracing::warn!("the texture pack {reference} names {named}, which is no {kind:?} of the game's");
        }
        guid
    };
    let picture = |named: &str| -> Option<Picture> {
        let picture = match &home {
            Home::Builtin => None,
            Home::Game => asset(named, AssetKind::Texture).map(Picture::Asset),
            Home::Folder(folder) => inside(folder, named).filter(|path| path.is_file()).map(Picture::File),
        };
        if picture.is_none() && !matches!(home, Home::Game) {
            tracing::warn!("the texture pack {reference} names the picture {named}, which is not in it");
        }
        picture
    };
    let maps = |named: &MapPictures| -> Option<Maps> {
        let maps = named.each().map(|named| named.as_deref().and_then(&picture));
        maps.iter().any(Option::is_some).then_some(maps)
    };
    let mut pack = Pack {
        reference: reference.to_string(),
        description: file.description.clone(),
        pixels: file.pixels,
        grain: file.grain,
        ..Pack::default()
    };
    for entry in &file.blocks {
        let Some(guid) = asset(&entry.block, AssetKind::Block) else { continue };
        if let Some(overlap) = entry.overlap {
            pack.overlaps.insert(guid, overlap);
        }
        let faces = [
            (Part::All, &entry.all),
            (Part::Sides, &entry.sides),
            (Part::Top, &entry.top),
            (Part::Bottom, &entry.bottom),
        ];
        let face = |named: &FacePictures| Some(Face { maps: maps(&named.maps())?, connected: entry.connected });
        let parts: HashMap<Part, Face> =
            faces.into_iter().filter_map(|(part, named)| Some((part, face(named.as_ref()?)?))).collect();
        if !parts.is_empty() {
            pack.blocks.entry(guid).or_default().extend(parts);
        }
    }
    for entry in &file.materials {
        let Some(guid) = asset(&entry.material, AssetKind::Material) else { continue };
        if let Some(maps) = maps(&entry.maps()) {
            pack.materials.insert(guid, Face { maps, connected: entry.connected });
        }
    }
    pack
}

/// The file `named` in `folder`, where the name stays inside it: a name
/// that climbs out, or starts from the drive's root, names nothing.
fn inside(folder: &Path, named: &str) -> Option<PathBuf> {
    let path = Path::new(named);
    let stays = path.components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    (stays && !named.is_empty()).then(|| folder.join(path))
}

/// Wears the pack `reference` names, or with an empty reference none: every
/// world of blocks takes its blocks up again, and the renderer's materials
/// follow at the next [`sync`]. A pack that cannot be read leaves the one
/// worn as it is.
pub fn wear(content: &mut Content, reference: &str) -> Result<()> {
    let pack = match reference.is_empty() {
        true => None,
        false => {
            let pack = load(content, reference).with_context(|| format!("loading the texture pack {reference}"))?;
            Some(Arc::new(pack))
        }
    };
    let worn = content.textures_mut();
    if worn.pack == pack {
        return Ok(());
    }
    (worn.pack, worn.stale) = (pack, true);
    content.blocks_changed();
    Ok(())
}

/// Wears the pack `reference` names as its file is now, after it or a
/// picture of its changed: what the renderer holds for it is made again,
/// though it names the same things.
pub fn wear_again(content: &mut Content, reference: &str) -> Result<()> {
    let pack = load(content, reference).with_context(|| format!("loading the texture pack {reference}"))?;
    let worn = content.textures_mut();
    (worn.pack, worn.stale) = (Some(Arc::new(pack)), true);
    content.blocks_changed();
    Ok(())
}

/// What names a pack for as long as it is there: an asset by its GUID,
/// which a new name or place for its file leaves as it is, and any other as
/// it is named.
fn lasting(content: &Content, reference: &str) -> String {
    if reference.starts_with("builtin:") || reference.starts_with(FILE_PREFIX) {
        return reference.to_string();
    }
    let guid = content.parse(reference).ok().and_then(|asset| asset.guid());
    guid.map_or_else(|| reference.to_string(), |guid| guid.to_string())
}

/// What names the pack worn; none for none.
pub fn worn(content: &Content) -> Option<&str> {
    content.textures().pack.as_ref().map(|pack| pack.reference.as_str())
}

/// Brings the renderer's materials up to date with the pack worn, once it
/// has changed: lets go of the last pack's pictures and its blocks' faces,
/// and uploads again each loaded material either pack draws. Before the
/// worlds of blocks' looks are made again (`blocks::refresh`), which make
/// their faces anew.
pub fn sync(content: &mut Content, renderer: &mut Renderer) {
    if !content.textures().stale {
        return;
    }
    let worn = content.textures_mut();
    worn.stale = false;
    let (pictures, faces) = (std::mem::take(&mut worn.pictures), std::mem::take(&mut worn.faces));
    let mut again = std::mem::take(&mut worn.dressed);
    again.extend(worn.pack.iter().flat_map(|pack| pack.materials()));
    for guid in again {
        if let Err(error) = content.upload_material_again(renderer, guid) {
            tracing::warn!("a material under the texture pack: {error:#}");
        }
    }
    faces.into_values().for_each(|material| renderer.free_material(material));
    pictures.into_values().for_each(|texture| renderer.free_texture(texture));
}

/// Lays the pictures the pack worn draws the material at `guid` with over
/// `look`, the material's own; `look` as it is where it draws it with none.
pub(crate) fn dress_material(
    content: &mut Content,
    renderer: &mut Renderer,
    guid: AssetGuid,
    look: &mut MaterialDesc,
) -> Result<()> {
    let Some(pack) = content.textures().pack.clone() else { return Ok(()) };
    let Some(face) = pack.materials.get(&guid) else { return Ok(()) };
    lay(content, renderer, &pack, face, look)?;
    content.textures_mut().dressed.insert(guid);
    Ok(())
}

/// Whether the material at `guid` is drawn with the five tiles of a face
/// that joins those beside it: as the pack worn says where it brings its
/// colour picture, and else as its file does.
pub(crate) fn material_connected(content: &mut Content, guid: AssetGuid) -> bool {
    let dressed = content.textures().pack.as_ref().and_then(|pack| pack.materials.get(&guid));
    match dressed.filter(|face| face.colors()) {
        Some(face) => face.connected,
        None => content.material_data(guid).is_ok_and(|data| data.connected),
    }
}

/// The material of a face of the pack's, which `face` names after
/// [`FACE_PREFIX`]: the block's own material for that face, or a matt white
/// one for a block of a colour alone, with the pack's pictures. Made once,
/// and kept until another pack is worn.
pub fn face_material(content: &mut Content, renderer: &mut Renderer, face: &str) -> Result<MaterialHandle> {
    sync(content, renderer);
    if let Some(&material) = content.textures().faces.get(face) {
        return Ok(material);
    }
    let pack = content.textures().pack.clone().context("no texture pack is worn")?;
    let (guid, part) = face.split_once(':').with_context(|| format!("`{face}` names no face"))?;
    let part = Part::parse(part).with_context(|| format!("`{face}` names no face"))?;
    let (guid, block) = content.block(guid).with_context(|| format!("`{face}` names no block"))?;
    let material = if part == Part::Grain {
        let grain = pack.grain.context("the texture pack has no grain")?;
        let picture = (Picture::Asset(guid), true);
        let texture = match content.textures().pictures.get(&picture) {
            Some(&texture) => texture,
            None => {
                let (side, name) = (grain.within().pixels, format!("grain of {guid}"));
                let rgba = grain.picture(block.color, seed_of(guid));
                let texture = renderer.add_texture(&TextureSource {
                    name: &name,
                    width: side,
                    height: side,
                    rgba: &rgba,
                    srgb: true,
                })?;
                content.textures_mut().pictures.insert(picture, texture);
                texture
            }
        };
        let mut look = content.look(renderer, &plain_face())?;
        (look.base_color_texture, look.pixels, look.connected) = (Some(texture), pack.pixels, false);
        renderer.add_material(&look)?
    } else {
        let dressed = pack.blocks.get(&guid).and_then(|parts| parts.get(&part));
        let dressed = dressed.with_context(|| format!("the texture pack has no `{face}`"))?;
        // The block's own material for the face: its tint, its roughness and whether it is cut out stay.
        let own = match part {
            Part::Top => block.top.as_ref().or(block.material.as_ref()),
            Part::Bottom => block.bottom.as_ref().or(block.material.as_ref()),
            _ => block.material.as_ref(),
        };
        let own = own.and_then(|reference| content.parse(reference).ok()?.guid());
        let data = own.and_then(|guid| content.material_data(guid).ok()).unwrap_or_else(plain_face);
        let mut look = content.look(renderer, &data)?;
        lay(content, renderer, &pack, dressed, &mut look)?;
        renderer.add_material(&look)?
    };
    content.textures_mut().faces.insert(face.to_string(), material);
    Ok(material)
}

/// A number of the block's own, for the picture made of its colour.
fn seed_of(guid: AssetGuid) -> u32 {
    guid.to_string().bytes().fold(0, |seed, byte| seed.wrapping_mul(31).wrapping_add(u32::from(byte)))
}

/// What a face with a picture and no material of its own is drawn with:
/// white, so the picture shows as it is, and matt, as a block of a colour
/// alone is.
fn plain_face() -> MaterialData {
    MaterialData { color: [1.0; 3], roughness: 1.0, ..default_material() }
}

/// Puts each picture of `face` in `look` in place of the map it has. Its
/// colour picture brings how it is drawn with it: sharp or smooth, and as
/// one face or the five tiles of a face that joins.
fn lay(
    content: &mut Content,
    renderer: &mut Renderer,
    pack: &Pack,
    face: &Face,
    look: &mut MaterialDesc,
) -> Result<()> {
    for (picture, map) in face.maps.iter().zip(MaterialMap::ALL) {
        let Some(picture) = picture else { continue };
        let texture = Some(uploaded(content, renderer, picture, map.srgb())?);
        match map {
            MaterialMap::Color => {
                (look.base_color_texture, look.pixels, look.connected) = (texture, pack.pixels, face.connected);
            }
            MaterialMap::Metallic => look.metallic_roughness_texture = texture,
            MaterialMap::Normal => look.normal_texture = texture,
            MaterialMap::Emissive => look.emissive_texture = texture,
            MaterialMap::Height => look.height_texture = texture,
        }
    }
    Ok(())
}

/// A picture of the pack's as a texture, uploaded on first use.
fn uploaded(content: &mut Content, renderer: &mut Renderer, picture: &Picture, srgb: bool) -> Result<TextureId> {
    let path = match picture {
        Picture::Asset(guid) => return content.picture(renderer, *guid, srgb),
        Picture::File(path) => path,
    };
    let key = (picture.clone(), srgb);
    if let Some(&texture) = content.textures().pictures.get(&key) {
        return Ok(texture);
    }
    let name = path.display().to_string();
    let bytes = std::fs::read(path).with_context(|| format!("reading {name}"))?;
    let read = image::load_from_memory(&bytes).with_context(|| format!("reading {name}"))?.to_rgba8();
    let (width, height) = read.dimensions();
    ensure!(
        width.max(height) <= LARGEST_PICTURE,
        "{name} is {width} × {height} pixels; a pack's pictures are {LARGEST_PICTURE} a side at most"
    );
    let texture = renderer.add_texture(&TextureSource { name: &name, width, height, rgba: read.as_raw(), srgb })?;
    content.textures_mut().pictures.insert(key, texture);
    Ok(texture)
}

/// The file beside the saves that keeps the texture pack the player chose.
const TEXTURES_FILE: &str = "textures.txt";

/// What names the texture pack the player last chose, kept beside the saves
/// so a game starts in it again; none when they chose none.
pub fn remembered() -> Option<String> {
    let reference = std::fs::read_to_string(voxear_scripting::saves_folder()?.join(TEXTURES_FILE)).ok()?;
    Some(reference.trim().to_string()).filter(|reference| !reference.is_empty())
}

/// Keeps the texture pack the player chose, for [`remembered`]; an empty
/// reference for none.
pub fn remember(reference: &str) {
    let Some(folder) = voxear_scripting::saves_folder() else { return };
    let kept = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(folder.join(TEXTURES_FILE), reference));
    if let Err(error) = kept {
        tracing::warn!("could not keep the texture pack chosen: {error}");
    }
}

/// Where a pack is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackSource {
    /// The engine's own.
    Builtin,
    /// A `.texturepack` asset of the game's.
    Game,
    /// A folder in the player's `Textures` folder.
    Player,
}

/// A pack as a menu lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackInfo {
    /// What [`wear`] takes to wear it.
    pub reference: String,
    pub name: String,
    pub description: String,
    pub source: PackSource,
}

/// Where a player's own packs are kept for a game whose saves are in
/// `saves`: `Textures`, beside them.
pub fn player_folder(saves: &Path) -> Option<PathBuf> {
    Some(saves.parent()?.join(TEXTURES_FOLDER))
}

/// Every pack there is to wear: the engine's, then the game's by name,
/// then those in the player's `folder` by name. A player's pack that cannot
/// be read is left out, with a warning.
pub fn list(content: &mut Content, folder: Option<&Path>) -> Vec<PackInfo> {
    let mut packs: Vec<PackInfo> = BUILTIN
        .iter()
        .map(|(key, title, text)| PackInfo {
            reference: format!("builtin:{key}"),
            name: title.to_string(),
            description: TexturePackFile::parse(text).map(|file| file.description).unwrap_or_default(),
            source: PackSource::Builtin,
        })
        .collect();
    let named = |path: &Path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let assets: Vec<_> = content
        .db()
        .entries()
        .filter(|entry| entry.kind == AssetKind::TexturePack)
        .map(|entry| (entry.guid, named(Path::new(&entry.path))))
        .collect();
    let mut game = Vec::new();
    for (guid, name) in assets {
        if content.is_stripped(guid) {
            continue;
        }
        let read = content.asset_text(guid, AssetKind::TexturePack).ok();
        let file = read.and_then(|(text, _)| toml::from_str::<TexturePackFile>(&text).ok());
        let description = file.map(|file| file.description).unwrap_or_default();
        game.push(PackInfo { reference: guid.to_string(), name, description, source: PackSource::Game });
    }
    game.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packs.extend(game);
    packs.extend(folder.map(player_packs).unwrap_or_default());
    packs
}

/// Every pack there is to wear, which one is worn and where a player keeps
/// their own, as JSON for scripts: `packs`, `worn` and `folder`.
pub fn json(content: &mut Content, folder: Option<&Path>) -> String {
    serde_json::json!({
        "packs": list(content, folder),
        "worn": worn(content),
        "folder": folder.map(|folder| folder.display().to_string()),
    })
    .to_string()
}

/// The pack file in `folder`, a pack of a player's: the first by name of
/// those it holds.
fn pack_file(folder: &Path) -> Option<PathBuf> {
    let files = std::fs::read_dir(folder).ok()?.flatten().map(|file| file.path());
    let mut packs: Vec<PathBuf> =
        files.filter(|path| path.extension().and_then(|e| e.to_str()) == Some(TEXTURE_PACK_EXTENSION)).collect();
    packs.sort();
    packs.into_iter().next()
}

/// The packs in a player's `folder`, by name: each a folder of its own, a
/// pack's file in it.
fn player_packs(folder: &Path) -> Vec<PackInfo> {
    let Ok(folders) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut packs = Vec::new();
    for path in folders.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()) {
        let Some(file) = pack_file(&path) else { continue };
        let read = std::fs::read_to_string(&file).map_err(anyhow::Error::from);
        match read.and_then(|text| TexturePackFile::parse(&text)) {
            Ok(pack) => packs.push(PackInfo {
                reference: format!("{FILE_PREFIX}{}", file.display()),
                name: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
                description: pack.description,
                source: PackSource::Player,
            }),
            Err(error) => tracing::warn!("the texture pack {} is left out: {error:#}", file.display()),
        }
    }
    packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    packs
}

/// Makes a pack to start from in a player's `folder`, in a folder of its
/// own called `name`: every block of the game listed by its path, each
/// block of a colour alone with a small picture of its colour to paint
/// over, and each block with a material named and left as it is. Gives the
/// pack's file, which [`wear`] takes after [`FILE_PREFIX`].
pub fn write_starter(content: &mut Content, folder: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    let plain = !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\', ':']);
    ensure!(plain, "`{name}` is no name for a pack's folder");
    let home = folder.join(name);
    ensure!(!home.exists(), "there is a pack called {name} already");
    std::fs::create_dir_all(&home).with_context(|| format!("making {}", home.display()))?;
    let mut text = format!(
        "# A texture pack for this game: which picture each block is drawn with.\n\
         # Paint over the pictures here, or name others in this folder. A block's `all` is every face;\n\
         # `sides`, `top` and `bottom` in its place draw those. Take the # off a block to give it pictures.\n\
         format = {TEXTURE_PACK_FORMAT}\n\
         description = \"A pack of my own.\"\n\
         # Pixel art stays sharp however near.\n\
         pixels = true\n"
    );
    let grain = Grain::default();
    let mut used = HashSet::new();
    for reference in crate::blocks::every_block(content) {
        let Some((guid, block)) = content.block(&reference) else { continue };
        let Some(path) = content.db().get(guid).map(|entry| entry.path.clone()) else { continue };
        if !block.liquid.is_none() {
            continue;
        }
        let called = content.block_name(guid).unwrap_or_default();
        let mut file = format!("{called}.png");
        let mut again = 2;
        while !used.insert(file.to_lowercase()) {
            file = format!("{called} {again}.png");
            again += 1;
        }
        if block.material.is_some() || block.top.is_some() || block.bottom.is_some() {
            text += &format!("\n# [[block]]\n# block = {}\n# all = {}\n", quoted(&path), quoted(&file));
            continue;
        }
        let side = grain.pixels;
        let rgba = grain.picture(block.color, seed_of(guid));
        let picture = image::RgbaImage::from_raw(side, side, rgba).context("a picture of a block's colour")?;
        picture.save(home.join(&file)).with_context(|| format!("writing {file}"))?;
        text += &format!("\n[[block]]\nblock = {}\nall = {}\n", quoted(&path), quoted(&file));
    }
    let file = home.join(format!("{name}.{TEXTURE_PACK_EXTENSION}"));
    std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
    Ok(file)
}

/// `text` as TOML writes a string.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use voxear_terrain::blocks::block::BlockDef;

    use super::*;

    /// A project with three blocks (grass and stone of a colour alone, planks with a material), a material with a
    /// colour map, and a few pictures.
    fn project() -> (tempfile::TempDir, Content) {
        let dir = tempfile::tempdir().unwrap();
        let picture = |path: &str, color: [u8; 4]| {
            let file = dir.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            image::RgbaImage::from_pixel(4, 4, image::Rgba(color)).save(file).unwrap();
        };
        let write = |path: &str, text: String| {
            let file = dir.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        picture("Pictures/planks.png", [150, 110, 60, 255]);
        picture("Packs/Old/grass_top.png", [40, 160, 40, 255]);
        picture("Packs/Old/stone.png", [90, 90, 96, 255]);
        picture("Packs/Old/planks.png", [60, 40, 20, 255]);
        write("Blocks/Grass.block", BlockDef::colored([0.2, 0.6, 0.1]).to_text());
        write("Blocks/Stone.block", BlockDef::colored([0.5, 0.5, 0.52]).to_text());
        let planks = BlockDef { material: Some("Materials/Planks.material".into()), ..BlockDef::default() };
        write("Blocks/Planks.block", planks.to_text());
        let water =
            BlockDef { liquid: voxear_terrain::blocks::block::Liquid::Water, ..BlockDef::colored([0.04, 0.46, 0.33]) };
        write("Blocks/Water.block", water.to_text());
        // Scanned once for the picture's GUID, which a material names its map by.
        let content = Content::open(dir.path()).unwrap();
        let map = content.parse("Pictures/planks.png").unwrap().guid().unwrap();
        write(
            "Materials/Planks.material",
            format!("color = [1.0, 0.9, 0.8]\nroughness = 0.7\ncolor_map = \"{map}\"\n"),
        );
        let pack = "description = \"Older.\"\npixels = true\n\n\
                    [[block]]\nblock = \"Blocks/Grass.block\"\ntop = \"Packs/Old/grass_top.png\"\n\n\
                    [[block]]\nblock = \"Blocks/Stone.block\"\nall = { color_map = \"Packs/Old/stone.png\" }\n\n\
                    [[block]]\nblock = \"Blocks/Gone.block\"\nall = \"Packs/Old/stone.png\"\n\n\
                    [[material]]\nmaterial = \"Materials/Planks.material\"\ncolor_map = \"Packs/Old/planks.png\"\n";
        write("Packs/Old/Old.texturepack", pack.to_string());
        drop(content);
        let content = Content::open(dir.path()).unwrap();
        (dir, content)
    }

    fn guid(content: &Content, path: &str) -> AssetGuid {
        content.parse(path).unwrap().guid().unwrap()
    }

    #[test]
    fn a_pack_reads_as_written_and_one_that_writes_what_there_is_not_is_not_read() {
        let text = "format = 1\ndescription = \"Older.\"\npixels = true\n\n[grain]\npixels = 8\nstrength = 0.25\n\n\
                    [[block]]\nblock = \"Blocks/Grass.block\"\nsides = \"side.png\"\ntop = \"top.png\"\n\n\
                    [[block]]\nblock = \"Blocks/Stone.block\"\n\n[block.all]\ncolor_map = \"stone.png\"\n\
                    normal_map = \"stone_n.png\"\n\n\
                    [[material]]\nmaterial = \"Materials/Planks.material\"\ncolor_map = \"planks.png\"\n";
        let file = TexturePackFile::parse(text).unwrap();
        assert_eq!((file.blocks.len(), file.materials.len(), file.pixels), (2, 1, true));
        assert_eq!(file.blocks[0].top, Some(FacePictures::Color("top.png".into())));
        assert_eq!(file.blocks[1].all.as_ref().unwrap().maps().normal_map.as_deref(), Some("stone_n.png"));
        assert_eq!(file.grain, Some(Grain { pixels: 8, strength: 0.25 }));
        assert_eq!(TexturePackFile::parse(&file.to_text()).unwrap(), file);
        assert!(TexturePackFile::parse("[[block]]\nblock = \"a\"\nfront = \"a.png\"").is_err(), "no such face");
        assert!(TexturePackFile::parse("[[material]]\ncolor_map = \"a.png\"").is_err(), "which material");
        assert!(TexturePackFile::parse("script = \"run.cs\"").is_err(), "pictures and settings alone");
        assert!(TexturePackFile::parse("format = 99").is_err(), "a newer Voxear's");
        // A block or a material whose pictures are a joining face's five tiles says so.
        let joined = "[[block]]\nblock = \"Blocks/Glass.block\"\nall = \"glass.png\"\nconnected = true\n\n\
                      [[material]]\nmaterial = \"Materials/Tiles.material\"\ncolor_map = \"tiles.png\"\nconnected = true\n";
        let joined = TexturePackFile::parse(joined).unwrap();
        assert!(joined.blocks[0].connected && joined.materials[0].connected && !file.blocks[0].connected);
        assert_eq!(TexturePackFile::parse(&joined.to_text()).unwrap(), joined);
        assert!(!file.to_text().contains("connected"), "what does not join does not say so");
        // And a block's place in the order of blocks that meet, pictures or none.
        let creeping = TexturePackFile::parse("[[block]]\nblock = \"Blocks/Grass.block\"\noverlap = 2\n").unwrap();
        assert_eq!((creeping.blocks[0].overlap, file.blocks[0].overlap), (Some(2), None));
        assert_eq!(TexturePackFile::parse(&creeping.to_text()).unwrap(), creeping);
        let wild = TexturePackFile::parse("[grain]\npixels = 9000\nstrength = 7.0").unwrap().grain.unwrap();
        assert_eq!((wild.pixels, wild.strength), (Grain::MOST, 1.0));
    }

    #[test]
    fn every_built_in_pack_reads_and_has_a_line_for_the_menu() {
        for (key, title, _) in BUILTIN {
            let file = TexturePackFile::builtin(key).unwrap_or_else(|error| panic!("{key}: {error:#}"));
            assert!(!title.is_empty() && file.description.len() > 20, "{key} says what it is");
            assert!(file.grain.is_some() && file.blocks.is_empty(), "{key} knows no game's blocks");
        }
        assert!(TexturePackFile::builtin("gloomy").is_err());
    }

    #[test]
    fn a_grain_is_the_blocks_colour_speckled_the_same_every_time() {
        let grain = Grain { pixels: 16, strength: 0.5 };
        let picture = grain.picture([0.2, 0.6, 0.1], 7);
        assert_eq!(picture.len(), 16 * 16 * 4);
        assert_eq!(picture, grain.picture([0.2, 0.6, 0.1], 7));
        assert_ne!(picture, grain.picture([0.2, 0.6, 0.1], 8), "another block's is another");
        let greens: Vec<u8> = picture.chunks_exact(4).map(|pixel| pixel[1]).collect();
        let mean = greens.iter().map(|&g| f32::from(g)).sum::<f32>() / greens.len() as f32;
        assert!((mean - 153.0).abs() < 6.0, "about the block's colour: {mean}");
        let (least, most) = (greens.iter().min().unwrap(), greens.iter().max().unwrap());
        assert!(most - least > 20 && most - least < 90, "speckled, not loud: {least} to {most}");
        assert!(picture.chunks_exact(4).all(|pixel| pixel[3] == 255));
        let flat = Grain { pixels: 4, strength: 0.0 }.picture([1.0, 0.0, 0.5], 1);
        assert!(flat.chunks_exact(4).all(|pixel| pixel == [255, 0, 128, 255]));
    }

    #[test]
    fn a_games_pack_finds_its_blocks_materials_and_pictures_and_leaves_out_what_is_not_there() {
        let (_dir, mut content) = project();
        let pack = load(&mut content, "Packs/Old/Old.texturepack").unwrap();
        let (grass, stone) = (guid(&content, "Blocks/Grass.block"), guid(&content, "Blocks/Stone.block"));
        assert_eq!(pack.reference, guid(&content, "Packs/Old/Old.texturepack").to_string(), "named by its GUID");
        assert!(pack.pixels && pack.description == "Older.");
        let mut blocks: Vec<AssetGuid> = pack.blocks().collect();
        blocks.sort_by_key(|guid| guid.to_string());
        let mut wanted = vec![grass, stone];
        wanted.sort_by_key(|guid| guid.to_string());
        assert_eq!(blocks, wanted, "the block that is gone is left out");
        assert_eq!(pack.materials().collect::<Vec<_>>(), [guid(&content, "Materials/Planks.material")]);
        let top = Picture::Asset(guid(&content, "Packs/Old/grass_top.png"));
        assert_eq!(pack.blocks[&grass][&Part::Top].maps[0], Some(top));
        assert!(!pack.blocks[&grass][&Part::Top].connected);
        assert!(load(&mut content, "Blocks/Grass.block").is_err(), "a block is no pack");
        assert!(load(&mut content, "builtin:gloomy").is_err());
    }

    #[test]
    fn a_pack_dresses_the_faces_it_has_pictures_for_and_grain_the_blocks_of_a_colour_alone() {
        let (_dir, mut content) = project();
        let pack = load(&mut content, "Packs/Old/Old.texturepack").unwrap();
        let entry = |content: &mut Content, path: &str| {
            let (guid, block) = content.block(path).unwrap();
            let mut kind = BlockKind::of(path);
            block.fill(&mut kind);
            (guid, kind)
        };
        // Grass has a top of the pack's: its sides and bottom stay its colour.
        let (grass, mut kind) = entry(&mut content, "Blocks/Grass.block");
        pack.dress(grass, &mut kind);
        assert_eq!(kind.top, Some(format!("{FACE_PREFIX}{grass}:top")));
        assert!(kind.material.is_none() && kind.bottom.is_none());
        // Stone has one picture for every face.
        let (stone, mut kind) = entry(&mut content, "Blocks/Stone.block");
        pack.dress(stone, &mut kind);
        let all = Some(format!("{FACE_PREFIX}{stone}:all"));
        assert_eq!((&kind.material, &kind.top, &kind.bottom), (&all, &all, &all));
        // Planks are not named, and have a material: left as they are, grain or no grain.
        let grainy = Pack { grain: Some(Grain::default()), ..pack.clone() };
        let (planks, mut kind) = entry(&mut content, "Blocks/Planks.block");
        grainy.dress(planks, &mut kind);
        assert_eq!(kind.material.as_deref(), Some("Materials/Planks.material"));
        // A face joins as the pack's picture for it says: stone's every face, with a pack that says so, and
        // planks, which join by their own material, no longer once a pack draws them with one picture.
        let mut joining = pack.clone();
        joining.blocks.get_mut(&stone).unwrap().get_mut(&Part::All).unwrap().connected = true;
        let (_, mut kind) = entry(&mut content, "Blocks/Stone.block");
        joining.dress(stone, &mut kind);
        assert_eq!(kind.connected, JOINS_SIDES | JOINS_TOP | JOINS_BOTTOM);
        let top = pack.blocks[&grass][&Part::Top].clone();
        joining.blocks.insert(planks, HashMap::from([(Part::Top, top)]));
        let (_, mut kind) = entry(&mut content, "Blocks/Planks.block");
        kind.connected = JOINS_SIDES;
        joining.dress(planks, &mut kind);
        // Its bottom has no material of its own, so it is drawn as its sides are.
        assert_eq!(kind.connected, JOINS_SIDES | JOINS_BOTTOM, "its top the pack's plain picture");
        // A pack gives a block another place in the order of blocks that meet, pictures or none.
        let mut ordered = Pack { grain: Some(Grain::default()), ..Pack::default() };
        ordered.overlaps.insert(grass, 3);
        let (_, mut kind) = entry(&mut content, "Blocks/Grass.block");
        ordered.dress(grass, &mut kind);
        assert!(kind.overlap == 3 && kind.material.is_some(), "and the grain as well: {kind:?}");
        // Grain: a block of a colour alone the pack has no picture for; water stays water.
        let (grass, mut kind) = entry(&mut content, "Blocks/Grass.block");
        Pack { grain: Some(Grain::default()), ..Pack::default() }.dress(grass, &mut kind);
        assert_eq!(kind.material, Some(format!("{FACE_PREFIX}{grass}:grain")));
        let (water, mut kind) = entry(&mut content, "Blocks/Water.block");
        grainy.dress(water, &mut kind);
        assert!(kind.water && kind.material.is_none());
    }

    #[test]
    fn wearing_a_pack_has_worlds_take_their_blocks_up_again_and_the_same_pack_again_does_not() {
        let (_dir, mut content) = project();
        let before = content.blocks_version();
        wear(&mut content, "Packs/Old/Old.texturepack").unwrap();
        let old = guid(&content, "Packs/Old/Old.texturepack").to_string();
        assert_eq!(worn(&content), Some(old.as_str()));
        let under = content.blocks_version();
        assert!(under > before);
        wear(&mut content, &old).unwrap();
        assert_eq!(content.blocks_version(), under, "the same pack, as it was");
        assert!(wear(&mut content, "Packs/None.texturepack").is_err());
        assert_eq!(worn(&content), Some(old.as_str()), "one that cannot be read leaves the one worn");
        wear(&mut content, "").unwrap();
        assert!(worn(&content).is_none() && content.blocks_version() > under);
    }

    #[test]
    fn a_players_folder_lists_its_packs_by_name_and_a_pack_reads_only_its_own_folder() {
        let (_dir, mut content) = project();
        let data = tempfile::tempdir().unwrap();
        let textures = player_folder(&data.path().join("Saves")).unwrap();
        assert_eq!(textures, data.path().join(TEXTURES_FOLDER));
        assert!(player_packs(&textures).is_empty(), "no folder yet");
        let pack = |name: &str, text: &str| {
            std::fs::create_dir_all(textures.join(name)).unwrap();
            std::fs::write(textures.join(name).join("pack.texturepack"), text).unwrap();
        };
        let mine = "description = \"Mine.\"\n[[block]]\nblock = \"Blocks/Grass.block\"\nall = \"grass.png\"\n\
                    [[block]]\nblock = \"Blocks/Stone.block\"\nall = \"../secret.png\"\n\
                    [[block]]\nblock = \"Blocks/Planks.block\"\nall = \"missing.png\"\n";
        pack("Mine", mine);
        pack("bright", "pixels = true");
        pack("Broken", "[[block]]\nfront = 1");
        std::fs::create_dir_all(textures.join("Empty")).unwrap();
        std::fs::write(textures.join("notes.txt"), "not a pack").unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255]))
            .save(textures.join("Mine/grass.png"))
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])).save(textures.join("secret.png")).unwrap();

        let packs = list(&mut content, Some(&textures));
        let names: Vec<(&str, PackSource)> = packs.iter().map(|pack| (pack.name.as_str(), pack.source)).collect();
        let wanted = [
            ("Pixel Grain", PackSource::Builtin),
            ("Soft Grain", PackSource::Builtin),
            ("Old", PackSource::Game),
            ("bright", PackSource::Player),
            ("Mine", PackSource::Player),
        ];
        assert_eq!(names, wanted);
        let mine = &packs[4];
        assert_eq!(mine.description, "Mine.");
        let file = mine.reference.strip_prefix(FILE_PREFIX).expect("named by its file");
        assert_eq!(Path::new(file), textures.join("Mine/pack.texturepack"));
        // Its picture beside it is found; one outside its folder and one that is not there are not.
        let pack = load(&mut content, &mine.reference).unwrap();
        let grass = guid(&content, "Blocks/Grass.block");
        assert_eq!(pack.blocks().collect::<Vec<_>>(), [grass]);
        assert_eq!(pack.blocks[&grass][&Part::All].maps[0], Some(Picture::File(textures.join("Mine/grass.png"))));
        assert!(inside(&textures, "/etc/passwd").is_none() && inside(&textures, "a/../../b.png").is_none());
        assert_eq!(inside(&textures, "a/b.png"), Some(textures.join("a/b.png")));
    }

    #[test]
    fn a_starter_pack_lists_every_block_with_a_picture_of_each_colour_to_paint_over() {
        let (_dir, mut content) = project();
        let data = tempfile::tempdir().unwrap();
        let file = write_starter(&mut content, data.path(), "First").unwrap();
        assert_eq!(file, data.path().join("First/First.texturepack"));
        let text = std::fs::read_to_string(&file).unwrap();
        let read = TexturePackFile::parse(&text).unwrap();
        let blocks: Vec<&str> = read.blocks.iter().map(|entry| entry.block.as_str()).collect();
        assert_eq!(blocks, ["Blocks/Grass.block", "Blocks/Stone.block"], "{text}");
        assert!(text.contains("# block = \"Blocks/Planks.block\""), "one with a material is named: {text}");
        assert!(!text.contains("Water"), "a liquid has no pictures: {text}");
        let grass = image::open(data.path().join("First/Grass.png")).unwrap().to_rgba8();
        assert_eq!(grass.dimensions(), (16, 16));
        // It is a pack as it stands: listed, and its pictures found.
        let listed = player_packs(data.path());
        assert_eq!(listed.len(), 1);
        let pack = load(&mut content, &listed[0].reference).unwrap();
        assert_eq!(pack.blocks().count(), 2);
        assert!(pack.pixels);
        assert!(write_starter(&mut content, data.path(), "First").is_err(), "one of that name is there");
        assert!(write_starter(&mut content, data.path(), "../Out").is_err());
    }
}
