//! Megascans from Fab, brought into a project in one step.
//!
//! Fab has no API an engine could sign in or download through, so an asset
//! is downloaded in the browser, signed in there, and its zip, or the folder
//! it was unpacked into, is dropped on the editor, or turns up in the folder
//! the editor watches for downloads (see `editor/importing`). What each file
//! is comes from its name, as Fab and older Megascans downloads write them,
//! with the `.json` some downloads carry for the asset's name and type:
//!
//! - pictures: `Mossy_Rock_tdcnfcdr_High_4K_BaseColor.jpg`, or
//!   `tdcnfcdr_4K_Albedo.jpg`, or in a glTF download, `T_tdcnfcdr_4K_ORM.png`,
//!   with `_LOD1` after a normal map baked for one LOD;
//! - models: `Mossy_Rock_tdcnfcdr_High.fbx`, or one for each LOD, as
//!   `tdcnfcdr_LOD2.fbx`, a plant's variations as `..._VarA_LOD0.fbx`, and in
//!   a glTF download, `tdcnfcdr_tier_0.gltf`.
//!
//! The asset becomes a folder named for it, as `Megascans/Mossy Rock`:
//!
//! - its pictures, in `Textures`, as a material reads them: its colour, with
//!   a plant's opacity in its alpha, its normals, and its roughness,
//!   metalness and occlusion packed into one picture, in green, blue and
//!   red, as a material's metallic map has them. A surface keeps its height
//!   map too.
//! - a `.material` reading them, clipped by alpha where the asset has
//!   opacity, and as foliage for a plant;
//! - its models, drawn with that material, and for one that comes as a model
//!   for each LOD, a prefab with a LOD Group showing them in turn: what to
//!   drag into a scene.
//!
//! The folder is written beside where it goes, under a name starting with a
//! dot, which nothing looks in, and moved into place once whole, so the
//! editor never sees half an asset, and a failed import leaves nothing.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufReader, BufWriter, Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use image::{DynamicImage, GrayImage, RgbImage, RgbaImage, codecs::png, imageops};
use serde_json::Value;
use voxear_assets::{AssetGuid, MaterialRemap, ModelImport, WindMasks, meta_path};
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_engine::MaterialData;
use voxear_render::{LodMeasure, PartPlace};

use crate::project_files;

/// Megascans' page on Fab, which Open Fab shows in the browser.
pub const FAB_URL: &str = "https://www.fab.com/sellers/Quixel%20Megascans";

/// The folder at the project's top that downloads go into, when they are
/// not dropped on a folder of the project's own.
pub const FOLDER: &str = "Megascans";

/// The alpha a material with opacity clips below.
const CUTOFF: f32 = 0.5;

/// The furthest a surface's height map is taken to reach from a
/// download's `.json`, in metres: past it, the size is not its depth.
const DEEPEST: f32 = 0.25;

/// Folders deeper than this in a download are not looked in.
const DEPTH: usize = 8;

/// The most files a download's folder is looked through for, so a large
/// folder dropped by mistake is soon known not to be one.
const MOST_FILES: usize = 20_000;

const PICTURES: [&str; 4] = ["png", "jpg", "jpeg", "exr"];
const MODELS: [&str; 3] = ["fbx", "glb", "gltf"];
/// Models of the kinds a download may offer that Voxear does not read.
const UNREAD_MODELS: [&str; 4] = ["obj", "usd", "usdz", "abc"];

/// What an import made, as the editor shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The asset's folder, as it is named in the folder it went into.
    pub folder: String,
    /// What to select once it is in, from the folder it went into: its
    /// first prefab, or else its first model, or else its material.
    pub select: String,
    /// Files of the download that were left out, and why.
    pub skipped: Vec<String>,
}

/// What a download is: a model, a plant, or a surface, which is pictures
/// alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanKind {
    Model,
    Plant,
    Surface,
}

/// Whether the zip or folder at `path` is a Megascans download: one with
/// pictures named as Megascans names them.
pub fn is_download(path: &Path) -> bool {
    (path.is_dir() || is_zip(&path.to_string_lossy())) && Download::open(path).is_ok()
}

/// Brings the Megascans download at `path`, a zip or a folder, into a
/// folder of its own named for the asset, inside `into`, which is made if
/// it is not there.
pub fn import(path: &Path, into: &Path) -> Result<Imported> {
    let mut download = Download::open(path)?;
    let name = download.name();
    let kind = download.kind();
    std::fs::create_dir_all(into).with_context(|| format!("making {}", into.display()))?;
    // Its own, so two imports of one asset at once never share it.
    let stage = Stage::new(into.join(format!(".{name} {}.importing", AssetGuid::new_random())))?;
    let mut skipped = download.unread.clone();

    let maps = download.write_maps(&stage.path, &name, false, kind, &mut skipped)?;
    let height = download.json.as_ref().and_then(json_height).filter(|_| kind == ScanKind::Surface);
    let material = write_material(&stage.path, &name, &maps, kind, height.unwrap_or(0.0))?;
    let billboard = match download.pictures.iter().any(|picture| picture.billboard) {
        true => {
            let base = format!("{name} Billboard");
            let maps = download.write_maps(&stage.path, &base, true, kind, &mut skipped)?;
            Some(write_material(&stage.path, &base, &maps, kind, 0.0)?)
        }
        false => None,
    };
    let select = download.write_models(&stage.path, &name, kind, &material, billboard.as_ref())?;
    let select = select.unwrap_or(material.file);
    let folder = stage.finish(into, &name)?;
    Ok(Imported { select: format!("{folder}/{select}"), folder, skipped })
}

/// A map a picture holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Map {
    Color,
    Normal,
    /// Normals with the fine bumps baked in, as older downloads carry
    /// beside the plain ones.
    NormalBump,
    Roughness,
    Gloss,
    Metalness,
    Occlusion,
    /// Occlusion, roughness and metalness in red, green and blue, as glTF
    /// packs them.
    Packed,
    Height,
    Opacity,
    Emissive,
}

impl Map {
    /// The map a picture's last word names, as `BaseColor`, or in a glTF
    /// download's pictures, which start `T_`, as a letter, `B`.
    fn named(word: &str, short: bool) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "basecolor" | "albedo" | "diffuse" | "color" | "colour" => Self::Color,
            "normal" | "normalgl" => Self::Normal,
            "normalbump" => Self::NormalBump,
            "roughness" => Self::Roughness,
            "gloss" | "glossiness" => Self::Gloss,
            "metalness" | "metallic" | "metal" => Self::Metalness,
            "ao" | "occlusion" | "ambientocclusion" => Self::Occlusion,
            "orm" | "arm" => Self::Packed,
            "displacement" | "height" => Self::Height,
            "opacity" | "alpha" => Self::Opacity,
            "emissive" | "emission" => Self::Emissive,
            "b" if short => Self::Color,
            "n" if short => Self::Normal,
            "h" if short => Self::Height,
            _ => return None,
        })
    }
}

/// A picture of a download, and what its name says of it.
#[derive(Clone, Debug, PartialEq)]
struct Picture {
    file: String,
    map: Map,
    /// Its size, in thousands of pixels, as `4K` says.
    size: u32,
    /// The LOD it was baked for, for normals baked for each.
    lod: Option<u32>,
    /// Whether it is a plant's billboard's, for its last LOD.
    billboard: bool,
    /// The words of its name before its size.
    words: Vec<String>,
}

impl Picture {
    fn read(file: &str) -> Option<Self> {
        let (folders, name) = file.rsplit_once('/').unwrap_or(("", file));
        let (stem, extension) = name.rsplit_once('.')?;
        if !PICTURES.contains(&extension.to_ascii_lowercase().as_str()) {
            return None;
        }
        let mut words = words(stem);
        let lod = words.last().and_then(|word| lod_number(word));
        if lod.is_some() {
            words.pop();
        }
        let short = words.first().is_some_and(|word| word == "T");
        let last = words.pop()?;
        let map = Map::named(&last, short)?;
        let size = size_word(&words.pop()?)?;
        if short {
            words.remove(0);
        }
        let billboard = words.iter().map(String::as_str).chain(folders.split('/')).any(is_billboard);
        Some(Self { file: file.to_string(), map, size, lod, billboard, words })
    }

    /// Whether Voxear reads it: PNG and JPEG it does, EXR it does not.
    fn readable(&self) -> bool {
        !self.file.to_ascii_lowercase().ends_with(".exr")
    }

    fn extension(&self) -> String {
        self.file.rsplit_once('.').map_or("png", |(_, extension)| extension).to_ascii_lowercase()
    }
}

/// A model of a download, and what its name says of it.
#[derive(Clone, Debug, PartialEq)]
struct ModelFile {
    file: String,
    lod: Option<u32>,
    /// Which of a plant's variations it is, as `A` for `VarA`.
    variation: Option<String>,
    billboard: bool,
    /// Its quality, the best 0 (see [`tier`]).
    tier: Option<u32>,
    words: Vec<String>,
}

impl ModelFile {
    fn read(file: &str) -> Option<Self> {
        let (folders, name) = file.rsplit_once('/').unwrap_or(("", file));
        let (stem, extension) = name.rsplit_once('.')?;
        if !MODELS.contains(&extension.to_ascii_lowercase().as_str()) {
            return None;
        }
        let words = words(stem);
        let lod = words.iter().rev().find_map(|word| lod_number(word));
        let around = || words.iter().map(String::as_str).chain(folders.split('/'));
        let variation = around().find_map(variation_word);
        let billboard = around().any(is_billboard);
        Some(Self { file: file.to_string(), lod, variation, billboard, tier: tier(&words), words })
    }

    fn gltf(&self) -> bool {
        !self.file.to_ascii_lowercase().ends_with(".fbx")
    }
}

/// Anything that a zip can be read from: a file, or bytes in memory.
trait Archive: Read + Seek + Send {}
impl<T: Read + Seek + Send> Archive for T {}

/// Where a download's files are read from.
enum Source {
    Folder(PathBuf),
    /// A zip, with where each file is in it, by its path.
    Zip(zip::ZipArchive<Box<dyn Archive>>, HashMap<String, usize>),
}

impl Source {
    fn open(path: &Path) -> Result<Self> {
        if path.is_dir() {
            return Ok(Self::Folder(path.to_path_buf()));
        }
        ensure!(is_zip(&path.to_string_lossy()), "{} is neither a zip nor a folder", path.display());
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::zip(Box::new(BufReader::new(file)))
    }

    fn zip(reader: Box<dyn Archive>) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(reader).context("reading the zip")?;
        let mut names = HashMap::new();
        for index in 0..archive.len() {
            let entry = archive.by_index_raw(index)?;
            // Never a path out of the folder it would be unpacked into.
            let Some(path) = entry.enclosed_name().filter(|_| entry.is_file()) else { continue };
            let path: Vec<String> =
                path.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
            if path.first().is_some_and(|top| top == "__MACOSX") {
                continue;
            }
            names.insert(path.join("/"), index);
        }
        Ok(Self::Zip(archive, names))
    }

    /// Every file in it, by its path, with its size in bytes, in order.
    fn files(&mut self) -> Vec<(String, u64)> {
        let mut files = Vec::new();
        match self {
            Self::Folder(root) => walk(root, "", 0, &mut files),
            Self::Zip(archive, names) => {
                for (name, &index) in names.iter() {
                    if let Ok(entry) = archive.by_index_raw(index) {
                        files.push((name.clone(), entry.size()));
                    }
                }
            }
        }
        files.sort();
        files
    }

    fn reader(&mut self, file: &str) -> Result<Box<dyn Read + '_>> {
        match self {
            Self::Folder(root) => {
                ensure!(!file.split('/').any(|part| part == ".."), "{file} is outside the download");
                Ok(Box::new(File::open(root.join(file)).with_context(|| format!("opening {file}"))?))
            }
            Self::Zip(archive, names) => {
                let index = *names.get(file).with_context(|| format!("{file} is not in the zip"))?;
                Ok(Box::new(archive.by_index(index).with_context(|| format!("reading {file}"))?))
            }
        }
    }

    fn read(&mut self, file: &str) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.reader(file)?.read_to_end(&mut bytes).with_context(|| format!("reading {file}"))?;
        Ok(bytes)
    }

    fn copy(&mut self, file: &str, to: &Path) -> Result<()> {
        let mut out = BufWriter::new(File::create(to).with_context(|| format!("writing {}", to.display()))?);
        std::io::copy(&mut self.reader(file)?, &mut out).with_context(|| format!("copying {file}"))?;
        Ok(())
    }

    /// A zip inside it, opened.
    fn inner(&mut self, file: &str) -> Result<Self> {
        match self {
            Self::Folder(root) => Self::open(&root.join(file)),
            Self::Zip(..) => Self::zip(Box::new(Cursor::new(self.read(file)?))),
        }
    }
}

/// A Megascans download, opened.
struct Download {
    source: Source,
    /// The zip's or the folder's own name, without `.zip`.
    title: String,
    pictures: Vec<Picture>,
    models: Vec<ModelFile>,
    /// The `.json` older downloads describe their asset in.
    json: Option<Value>,
    /// Its models Voxear does not read, and why.
    unread: Vec<String>,
    /// The files written as they are, by their paths in the download, with
    /// where they went, so a glTF names the same ones.
    written: HashMap<String, String>,
}

impl Download {
    fn open(path: &Path) -> Result<Self> {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let title = if is_zip(&name) { name[..name.len() - 4].to_string() } else { name };
        let mut download = Self::read(Source::open(path)?, title)?;
        if download.pictures.is_empty() && download.models.is_empty() {
            // A zip for each quality, as some downloads hold: the best of them.
            let files = download.source.files();
            let zips = files.iter().filter(|(file, _)| is_zip(file));
            let best = zips.min_by_key(|(file, size)| (tier(&words(stem(file))).unwrap_or(u32::MAX), Reverse(*size)));
            if let Some((file, _)) = best {
                let inner = download.source.inner(file)?;
                download = Self::read(inner, download.title)?;
            }
        }
        let maps =
            download.pictures.iter().any(|picture| matches!(picture.map, Map::Color | Map::Normal | Map::Packed));
        ensure!(maps, "{} holds no pictures named as Megascans names them", path.display());
        Ok(download)
    }

    fn read(mut source: Source, title: String) -> Result<Self> {
        let files = source.files();
        let pictures: Vec<Picture> = files.iter().filter_map(|(file, _)| Picture::read(file)).collect();
        let models = files.iter().filter_map(|(file, _)| ModelFile::read(file)).collect();
        let extension = |file: &str| file.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
        let unread =
            files.iter().filter(|(file, _)| extension(file).is_some_and(|e| UNREAD_MODELS.contains(&e.as_str())));
        let unread = unread.map(|(file, _)| format!("{file}: Voxear reads FBX and glTF models")).collect();
        // Read only for a download, so another folder is soon known not to be one.
        let small_json =
            |(file, size): &&(String, u64)| file.to_ascii_lowercase().ends_with(".json") && *size < 1 << 22;
        let json = files.iter().filter(small_json).filter(|_| !pictures.is_empty()).find_map(|(file, _)| {
            let json: Value = serde_json::from_slice(&source.read(file).ok()?).ok()?;
            json.get("id").is_some().then_some(json)
        });
        Ok(Self { source, title, pictures, models, json, unread, written: HashMap::new() })
    }

    /// What it is: by its `.json` where it says, and otherwise a plant for
    /// one with variations or opacity, a model for another with models, and
    /// a surface for pictures alone.
    fn kind(&self) -> ScanKind {
        if self.models.is_empty() {
            return ScanKind::Surface;
        }
        let plant = self.models.iter().any(|model| model.variation.is_some())
            || self.pictures.iter().any(|picture| picture.map == Map::Opacity);
        match self.json.as_ref().and_then(json_kind) {
            Some(ScanKind::Surface) | None if !plant => ScanKind::Model,
            Some(ScanKind::Surface) | None => ScanKind::Plant,
            Some(kind) => kind,
        }
    }

    /// The asset's name, as `Mossy Rock`: its `.json`'s, or the words before
    /// its ID in Fab's names, `Mossy_Rock_tdcnfcdr_High`, or else what is
    /// left of the zip's or folder's own name without its ID and sizes.
    fn name(&self) -> String {
        let json = self.json.as_ref();
        if let Some(name) = json.and_then(|json| json.get("name")?.as_str()).and_then(safe_name) {
            return name;
        }
        let color = self.pictures.iter().find(|picture| picture.map == Map::Color).map(|picture| &picture.words);
        let words = color.or(self.models.first().map(|model| &model.words)).cloned().unwrap_or_default();
        let words = name_words(&words);
        let id = json.and_then(|json| json.get("id")?.as_str()).map(str::to_string).or(words.last().cloned());
        if let [name @ .., _id] = words.as_slice()
            && let Some(name) = safe_name(&name.join(" "))
        {
            return name;
        }
        let left = self.title.split(['_', ' ', '-']).filter(|word| {
            let lower = word.to_ascii_lowercase();
            let kind = ["ms", "3d", "3dplant", "plant", "surface", "decal", "atlas"].contains(&lower.as_str());
            !kind
                && !is_quality(word)
                && size_word(word).is_none()
                && id.as_deref().is_none_or(|id| !id.eq_ignore_ascii_case(word))
        });
        let left = left.collect::<Vec<_>>().join(" ");
        safe_name(&left).or_else(|| id.as_deref().and_then(safe_name)).unwrap_or_else(|| "Megascans Asset".to_string())
    }

    /// The picture of a map for the asset, or for its billboard: the one
    /// for no LOD or the first, at the largest size. An EXR, which Voxear
    /// does not read, only when there is no other, and then it is skipped.
    fn pick(&self, map: Map, billboard: bool, skipped: &mut Vec<String>) -> Option<Picture> {
        let mut found: Vec<&Picture> =
            self.pictures.iter().filter(|picture| picture.map == map && picture.billboard == billboard).collect();
        found.sort_by_key(|picture| (!picture.readable(), picture.lod.unwrap_or(0), Reverse(picture.size)));
        let best = found.first()?;
        if !best.readable() {
            skipped.push(format!("{}: EXR pictures are not read", best.file));
            return None;
        }
        Some((*best).clone())
    }

    /// Writes the pictures of the asset, or of its billboard, into
    /// `Textures`, each named `base` and its map, as its material reads
    /// them.
    fn write_maps(
        &mut self,
        stage: &Path,
        base: &str,
        billboard: bool,
        kind: ScanKind,
        skipped: &mut Vec<String>,
    ) -> Result<Maps> {
        let mut pick = |map| self.pick(map, billboard, skipped);
        let (color, opacity) = (pick(Map::Color), pick(Map::Opacity));
        let normal = pick(Map::Normal).or_else(|| pick(Map::NormalBump));
        let (packed, metalness, occlusion) = (pick(Map::Packed), pick(Map::Metalness), pick(Map::Occlusion));
        let roughness = pick(Map::Roughness)
            .map(|picture| (picture, false))
            .or_else(|| pick(Map::Gloss).map(|picture| (picture, true)));
        // A model's shape is its own, so only a surface keeps its height.
        let height = if kind == ScanKind::Surface { pick(Map::Height) } else { None };
        let emissive = pick(Map::Emissive);
        let textures = stage.join("Textures");
        let mut maps = Maps::default();

        maps.color = match (&color, &opacity) {
            (Some(color), Some(opacity)) => {
                let rgba = with_alpha(self.decode(color)?, self.decode(opacity)?);
                maps.alpha = true;
                Some(write_png(&textures.join(format!("{base} Colour.png")), &DynamicImage::ImageRgba8(rgba))?)
            }
            (Some(color), None) => Some(self.copy_picture(&textures, color, &format!("{base} Colour"))?),
            (None, _) => None,
        };
        maps.metallic = match (&packed, &roughness, &metalness) {
            (Some(packed), ..) => {
                (maps.rough, maps.metal) = (true, true);
                Some(self.copy_picture(&textures, packed, &format!("{base} Metallic"))?)
            }
            (None, None, None) => None,
            (None, roughness, metalness) => {
                let rough = roughness.as_ref().map(|(picture, _)| self.decode(picture)).transpose()?;
                let metal = metalness.as_ref().map(|picture| self.decode(picture)).transpose()?;
                let occluded = occlusion.as_ref().map(|picture| self.decode(picture)).transpose()?;
                let gloss = roughness.as_ref().is_some_and(|(_, gloss)| *gloss);
                let luma = |image: Option<DynamicImage>| image.map(DynamicImage::into_luma8);
                (maps.rough, maps.metal) = (rough.is_some(), metal.is_some());
                let packed = pack(luma(rough), gloss, luma(metal), luma(occluded)).context("nothing to pack")?;
                Some(write_png(&textures.join(format!("{base} Metallic.png")), &DynamicImage::ImageRgb8(packed))?)
            }
        };
        maps.normal =
            normal.map(|normal| self.copy_picture(&textures, &normal, &format!("{base} Normal"))).transpose()?;
        maps.height =
            height.map(|height| self.copy_picture(&textures, &height, &format!("{base} Height"))).transpose()?;
        maps.emissive = emissive
            .map(|emissive| self.copy_picture(&textures, &emissive, &format!("{base} Emission")))
            .transpose()?;
        Ok(maps)
    }

    fn decode(&mut self, picture: &Picture) -> Result<DynamicImage> {
        image::load_from_memory(&self.source.read(&picture.file)?).with_context(|| format!("reading {}", picture.file))
    }

    /// Copies a picture into `folder` as it is, named `stem`, keeping its
    /// own extension.
    fn copy_picture(&mut self, folder: &Path, picture: &Picture, stem: &str) -> Result<AssetGuid> {
        let name = format!("{stem}.{}", picture.extension());
        let to = folder.join(&name);
        self.source.copy(&picture.file, &to)?;
        self.written.insert(picture.file.clone(), format!("Textures/{name}"));
        write_meta(&to)
    }

    /// The models to bring in: glTF's over FBX's in a download with both,
    /// at the best quality it has.
    fn chosen_models(&self) -> Vec<ModelFile> {
        let gltf = self.models.iter().any(ModelFile::gltf);
        let models: Vec<&ModelFile> = self.models.iter().filter(|model| model.gltf() == gltf).collect();
        let best = models.iter().filter_map(|model| model.tier).min();
        models.into_iter().filter(|model| model.tier.is_none() || model.tier == best).cloned().collect()
    }

    /// Writes the models, drawn with `material`, or a billboard with
    /// `billboard`'s, and for each set of them that has a model for each
    /// LOD, a prefab with a LOD Group showing them in turn. Returns what to
    /// select: the first prefab, or else the first model.
    fn write_models(
        &mut self,
        stage: &Path,
        name: &str,
        kind: ScanKind,
        material: &Written,
        billboard: Option<&Written>,
    ) -> Result<Option<String>> {
        let (boards, models): (Vec<ModelFile>, Vec<ModelFile>) =
            self.chosen_models().into_iter().partition(|model| model.billboard);
        let mut sets: BTreeMap<Option<String>, Vec<ModelFile>> = BTreeMap::new();
        for model in models {
            sets.entry(model.variation.clone()).or_default().push(model);
        }
        // A billboard of no variation's is every variation's last LOD.
        for board in boards {
            match &board.variation {
                Some(_) if sets.contains_key(&board.variation) || sets.is_empty() => {
                    sets.entry(board.variation.clone()).or_default().push(board)
                }
                _ if sets.is_empty() => sets.entry(None).or_default().push(board),
                _ => sets.values_mut().for_each(|set| set.push(board.clone())),
            }
        }
        let mut select = None;
        for (variation, mut models) in sets {
            models.sort_by_key(|model| (model.billboard, model.lod.unwrap_or(0)));
            let base = variation.map_or_else(|| name.to_string(), |variation| format!("{name} {variation}"));
            let grouped = models.len() > 1;
            let mut levels = Vec::new();
            for (level, model) in models.iter().enumerate() {
                let stem = match (model.billboard, grouped) {
                    (true, _) => format!("{base} Billboard"),
                    (false, true) => format!("{base}_LOD{level}"),
                    (false, false) => base.clone(),
                };
                let draws = if model.billboard { billboard.unwrap_or(material) } else { material };
                levels.push(self.write_model(stage, model, &stem, draws, kind, grouped)?);
            }
            let picked = match grouped {
                true => write_prefab(stage, &base, &levels)?.file,
                false => levels.remove(0).written.file,
            };
            select.get_or_insert(picked);
        }
        Ok(select)
    }

    /// Writes a model as `stem`, drawn with `material`, its import settings
    /// in its `.meta`: without LODs of the engine's own in a LOD Group,
    /// whose levels are its LODs, and with wind masks made for a plant.
    fn write_model(
        &mut self,
        stage: &Path,
        model: &ModelFile,
        stem: &str,
        material: &Written,
        kind: ScanKind,
        grouped: bool,
    ) -> Result<Level> {
        let extension = model.file.rsplit_once('.').map_or("fbx", |(_, extension)| extension).to_ascii_lowercase();
        let file = format!("{stem}.{extension}");
        let to = stage.join(&file);
        match extension.as_str() {
            "gltf" => self.write_gltf(stage, model, stem, &to)?,
            _ => self.source.copy(&model.file, &to)?,
        }
        let guid = write_meta(&to)?;
        let wind_masks = if kind == ScanKind::Plant { WindMasks::Generate } else { WindMasks::None };
        ModelImport { lods: !grouped, wind_masks, ..ModelImport::default() }.write(&to)?;

        // Read for its parts only, without making LODs of its own to read them.
        let mut read = |path: &str| -> Result<Vec<u8>> {
            ensure!(!path.split(['/', '\\']).any(|part| part == "..") && !Path::new(path).is_absolute(), "outside");
            match path.ends_with(".meta") {
                true => Ok(b"[model]\nlods = false\n".to_vec()),
                false => Ok(std::fs::read(stage.join(path))?),
            }
        };
        let prepared =
            voxear_render::prepare_model(&file, &mut read).with_context(|| format!("reading {}", model.file))?;
        let parts: Vec<Part> = prepared.part_places().map(Part::from).collect();
        let remap = parts.iter().filter_map(|part| Some((part.material.clone()?, material.guid))).collect();
        MaterialRemap(remap).write(&to)?;
        Ok(Level { written: Written { file, guid }, obj: stem.to_string(), parts, material: material.clone() })
    }

    /// Writes a `.gltf` to `to`, with its buffers beside it and the
    /// pictures it reads in `Textures`, each named where it is now.
    fn write_gltf(&mut self, stage: &Path, model: &ModelFile, stem: &str, to: &Path) -> Result<()> {
        let mut gltf: Value = serde_json::from_slice(&self.source.read(&model.file)?)
            .with_context(|| format!("reading {}", model.file))?;
        let folder = model.file.rsplit_once('/').map_or("", |(folder, _)| folder);
        let from = |uri: &str| -> Result<String> {
            let mut path: Vec<&str> = folder.split('/').filter(|part| !part.is_empty()).collect();
            let decoded = percent_decode(uri);
            for part in decoded.split('/') {
                match part {
                    ".." => ensure!(path.pop().is_some(), "{uri} is outside the download"),
                    "." | "" => {}
                    part => path.push(part),
                }
            }
            Ok(path.join("/"))
        };
        let count = gltf["buffers"].as_array().map_or(0, Vec::len);
        for (index, buffer) in gltf["buffers"].as_array_mut().into_iter().flatten().enumerate() {
            let Some(uri) = buffer["uri"].as_str().filter(|uri| !uri.starts_with("data:")) else { continue };
            let name = if count == 1 { format!("{stem}.bin") } else { format!("{stem} {index}.bin") };
            self.source.copy(&from(uri)?, &stage.join(&name))?;
            buffer["uri"] = Value::String(percent_encode(&name));
        }
        for image in gltf["images"].as_array_mut().into_iter().flatten() {
            let Some(uri) = image["uri"].as_str().filter(|uri| !uri.starts_with("data:")) else { continue };
            let file = from(uri)?;
            let written = match self.written.get(&file) {
                Some(written) => written.clone(),
                None => {
                    let name = file.rsplit('/').next().unwrap_or(&file).to_string();
                    let to = stage.join("Textures").join(&name);
                    if !to.exists() {
                        self.source.copy(&file, &to)?;
                        write_meta(&to)?;
                    }
                    self.written.insert(file, format!("Textures/{name}"));
                    format!("Textures/{name}")
                }
            };
            image["uri"] = Value::String(percent_encode(&written));
        }
        std::fs::write(to, serde_json::to_vec_pretty(&gltf)?).with_context(|| format!("writing {}", to.display()))
    }
}

/// The pictures a material reads, by their GUIDs, and what they hold.
#[derive(Clone, Debug, Default)]
struct Maps {
    color: Option<AssetGuid>,
    metallic: Option<AssetGuid>,
    /// Whether the metallic map holds the roughness, in green, and where it
    /// is metal, in blue; without them, the material's own values are used.
    rough: bool,
    metal: bool,
    normal: Option<AssetGuid>,
    height: Option<AssetGuid>,
    emissive: Option<AssetGuid>,
    /// Whether the colour's alpha is the asset's opacity.
    alpha: bool,
}

/// A file written into the asset's folder, from its folder, and its GUID.
#[derive(Clone, Debug)]
struct Written {
    file: String,
    guid: AssetGuid,
}

/// A model written as one level of a LOD Group, the Obj it shows as, and
/// the material its parts draw with.
struct Level {
    written: Written,
    obj: String,
    parts: Vec<Part>,
    material: Written,
}

/// A part of a model and where the model puts it.
struct Part {
    name: String,
    mesh: String,
    material: Option<String>,
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
}

impl From<PartPlace<'_>> for Part {
    fn from(place: PartPlace) -> Self {
        Self {
            name: place.name.to_string(),
            mesh: place.mesh.to_string(),
            material: place.material.map(str::to_string),
            position: place.position,
            rotation: place.rotation,
            scale: place.scale,
        }
    }
}

/// The folder an import is written into before it is moved into place,
/// taken away if the import fails.
struct Stage {
    path: PathBuf,
    done: bool,
}

impl Stage {
    fn new(path: PathBuf) -> Result<Self> {
        if path.exists() {
            std::fs::remove_dir_all(&path).with_context(|| format!("clearing {}", path.display()))?;
        }
        std::fs::create_dir_all(path.join("Textures")).with_context(|| format!("making {}", path.display()))?;
        Ok(Self { path, done: false })
    }

    /// Moves it into `into` as a folder named `name`, or numbered after it
    /// when one of that name is there, and returns the name it has.
    fn finish(mut self, into: &Path, name: &str) -> Result<String> {
        let folder = project_files::unique(into, name, "");
        let to = into.join(&folder);
        std::fs::rename(&self.path, &to).with_context(|| format!("moving the asset to {}", to.display()))?;
        self.done = true;
        Ok(folder)
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.done {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Writes `base.material`, reading `maps`: white, so its colour map shows
/// as it is; its roughness and metalness from the metallic map where it
/// holds them; clipped by alpha where the colour holds opacity; foliage for
/// a plant; and moved out by `displacement` metres by its height map.
fn write_material(stage: &Path, base: &str, maps: &Maps, kind: ScanKind, displacement: f32) -> Result<Written> {
    let defaults = MaterialData::default();
    let data = MaterialData {
        color: if maps.color.is_some() { [1.0; 3] } else { defaults.color },
        metallic: if maps.metal { 1.0 } else { 0.0 },
        roughness: if maps.rough { 1.0 } else { defaults.roughness },
        emissive: if maps.emissive.is_some() { [1.0; 3] } else { [0.0; 3] },
        color_map: maps.color,
        metallic_map: maps.metallic,
        normal_map: maps.normal,
        emissive_map: maps.emissive,
        height_map: maps.height,
        displacement: if maps.height.is_some() { displacement } else { 0.0 },
        foliage: kind == ScanKind::Plant,
        cutout: maps.alpha.then_some(CUTOFF),
        ..defaults
    };
    let file = format!("{base}.material");
    let to = stage.join(&file);
    std::fs::write(&to, toml::to_string(&data)?).with_context(|| format!("writing {file}"))?;
    Ok(Written { file, guid: write_meta(&to)? })
}

/// Writes `base.prefab`: an Obj with a LOD Group whose levels are an Obj
/// for each of `levels`, drawing its model's parts where the model puts
/// them, each level shown down to half the screen the last was.
fn write_prefab(stage: &Path, base: &str, levels: &[Level]) -> Result<Written> {
    let string = |text: &str| toml::Value::String(text.to_string());
    let mut objs = String::new();
    let mut shown = Vec::new();
    let mut next = 2;
    for level in levels {
        let id = next;
        next += 1;
        shown.push(id);
        objs += &format!("\n[[obj]]\nid = {id}\nname = {}\nparent = 1\n", string(&level.obj));
        let drawn = |part: &Part| {
            let mesh = format!("{}#{}", level.written.guid, part.mesh);
            let (model, material) = (&level.written.file, &level.material);
            format!(
                "[obj.renderer]\nmesh = {}  # {model}\nmaterial = \"{}\"  # {}\n",
                string(&mesh),
                material.guid,
                material.file
            )
        };
        match level.parts.as_slice() {
            [part] => objs += &(transform(part) + &drawn(part)),
            parts => {
                for part in parts {
                    let at = next;
                    next += 1;
                    objs += &format!("\n[[obj]]\nid = {at}\nname = {}\nparent = {id}\n", string(&part.name));
                    objs += &(transform(part) + &drawn(part));
                }
            }
        }
    }
    let levels: Vec<String> = (0..)
        .zip(&shown)
        .map(|(index, id)| {
            format!("{{ objs = [{id}], until = {} }}", voxear_render::default_until(LodMeasure::ScreenSize, index))
        })
        .collect();
    let text = format!(
        "# {base}, from Megascans on Fab: one of its LODs at a time, by how much of the screen it covers.\nformat = 1\n\n\
         [[obj]]\nid = 1\nname = {}\n[obj.lod_group]\nlevels = [{}]\n{objs}",
        string(base),
        levels.join(", "),
    );
    let file = format!("{base}.prefab");
    let to = stage.join(&file);
    std::fs::write(&to, text).with_context(|| format!("writing {file}"))?;
    Ok(Written { file, guid: write_meta(&to)? })
}

/// A part's place, as an Obj's `position`, `rotation` and `scale`, leaving
/// out what is as an Obj starts.
fn transform(part: &Part) -> String {
    let mut text = String::new();
    let (y, x, z) = part.rotation.to_euler(EulerRot::YXZ);
    let degrees = [x, y, z].map(f32::to_degrees);
    if part.position.abs().max_element() > 1e-6 {
        text += &format!("position = {}\n", vector(part.position.to_array()));
    }
    if degrees.iter().any(|degrees| degrees.abs() > 1e-3) {
        text += &format!("rotation = {}\n", vector(degrees));
    }
    if (part.scale - Vec3::ONE).abs().max_element() > 1e-6 {
        text += &format!("scale = {}\n", vector(part.scale.to_array()));
    }
    text
}

fn vector(values: [f32; 3]) -> String {
    let number = |value: f32| {
        let text = format!("{}", if value.abs() < 1e-6 { 0.0 } else { value });
        if text.contains('.') { text } else { text + ".0" }
    };
    format!("[{}, {}, {}]", number(values[0]), number(values[1]), number(values[2]))
}

/// Gives the asset at `file` a `.meta` with a new GUID, so what is written
/// after it can name it before the project is looked at again.
fn write_meta(file: &Path) -> Result<AssetGuid> {
    let guid = AssetGuid::new_random();
    let meta = meta_path(file);
    std::fs::write(&meta, format!("guid = \"{guid}\"\n")).with_context(|| format!("writing {}", meta.display()))?;
    Ok(guid)
}

/// Writes a picture as a PNG, quickly rather than small, as the largest are
/// 8K, and gives it a `.meta`.
fn write_png(to: &Path, image: &DynamicImage) -> Result<AssetGuid> {
    let file = BufWriter::new(File::create(to).with_context(|| format!("writing {}", to.display()))?);
    let encoder = png::PngEncoder::new_with_quality(file, png::CompressionType::Fast, png::FilterType::Adaptive);
    image.write_with_encoder(encoder).with_context(|| format!("writing {}", to.display()))?;
    write_meta(to)
}

/// The colour with the opacity as its alpha.
fn with_alpha(color: DynamicImage, opacity: DynamicImage) -> RgbaImage {
    let mut rgba = color.into_rgba8();
    let alpha = fit(opacity.into_luma8(), rgba.width(), rgba.height());
    for (pixel, alpha) in rgba.pixels_mut().zip(alpha.pixels()) {
        pixel.0[3] = alpha.0[0];
    }
    rgba
}

/// Roughness, or gloss turned to it, metalness and occlusion as one
/// picture, in green, blue and red, as a material's metallic map reads them,
/// at the size of the roughness, or else the metalness: rough, not metal and
/// not occluded where one is missing.
fn pack(
    roughness: Option<GrayImage>,
    gloss: bool,
    metalness: Option<GrayImage>,
    occlusion: Option<GrayImage>,
) -> Option<RgbImage> {
    let (width, height) = roughness.as_ref().or(metalness.as_ref())?.dimensions();
    let [roughness, metalness, occlusion] =
        [roughness, metalness, occlusion].map(|map| map.map(|map| fit(map, width, height)));
    Some(RgbImage::from_fn(width, height, |x, y| {
        let read = |map: &Option<GrayImage>, none: u8| map.as_ref().map_or(none, |map| map.get_pixel(x, y).0[0]);
        let rough = read(&roughness, 255);
        let rough = if gloss && roughness.is_some() { 255 - rough } else { rough };
        image::Rgb([read(&occlusion, 255), rough, read(&metalness, 0)])
    }))
}

fn fit(map: GrayImage, width: u32, height: u32) -> GrayImage {
    match map.dimensions() == (width, height) {
        true => map,
        false => imageops::resize(&map, width, height, imageops::FilterType::Triangle),
    }
}

/// What a download's `.json` says it is: a `3D plant`, a `3D asset`, or a
/// surface, decal or atlas.
fn json_kind(json: &Value) -> Option<ScanKind> {
    let named = [json.pointer("/semanticTags/asset_type"), json.get("type"), json.pointer("/categories/0")];
    let named = named.into_iter().flatten().find_map(Value::as_str)?.to_ascii_lowercase();
    Some(match () {
        _ if named.contains("plant") => ScanKind::Plant,
        _ if named.contains("3d") => ScanKind::Model,
        _ => ScanKind::Surface,
    })
}

/// How far a surface's height map reaches, in metres, as a download's
/// `.json` gives it among its `meta`, as `"0.05 m"`.
fn json_height(json: &Value) -> Option<f32> {
    let meta = json.get("meta")?.as_array()?;
    let height = meta.iter().find(|entry| entry.get("key").and_then(Value::as_str) == Some("height"))?;
    let value = height.get("value")?;
    let text = value.as_str().map(str::to_string).or_else(|| value.as_f64().map(|value| value.to_string()))?;
    metres(&text).filter(|&metres| metres <= DEEPEST)
}

/// A length as `0.05 m`, `5 cm` or `50mm`, in metres.
fn metres(text: &str) -> Option<f32> {
    let text = text.trim().to_ascii_lowercase();
    let end = text.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(text.len());
    let value: f32 = text[..end].parse().ok()?;
    let unit = match text[end..].trim() {
        "" | "m" => 1.0,
        "cm" => 0.01,
        "mm" => 0.001,
        _ => return None,
    };
    Some(value * unit).filter(|metres| metres.is_finite() && *metres > 0.0)
}

/// The quality a name's words say, the best 0, as Fab names them: Raw,
/// High, Mid and Low, or for glTF, `tier_0` to `tier_3`.
fn tier(words: &[String]) -> Option<u32> {
    words.iter().enumerate().find_map(|(at, word)| match word.to_ascii_lowercase().as_str() {
        "raw" => Some(0),
        "high" => Some(1),
        "mid" | "medium" => Some(2),
        "low" => Some(3),
        "tier" => words.get(at + 1)?.parse().ok(),
        _ => None,
    })
}

fn is_quality(word: &str) -> bool {
    tier(&[word.to_string()]).is_some() || word.eq_ignore_ascii_case("tier")
}

/// The words of a file's name that name the asset and its ID: without its
/// quality, `tier_0`'s number with it, and which LOD or variation it is.
fn name_words(words: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut after_tier = false;
    for word in words {
        let tier_number = after_tier && word.parse::<u32>().is_ok();
        after_tier = word.eq_ignore_ascii_case("tier");
        if !tier_number && !is_quality(word) && !is_detail(word) {
            kept.push(word.clone());
        }
    }
    kept
}

/// Whether a word says which LOD or variation a file is.
fn is_detail(word: &str) -> bool {
    lod_number(word).is_some() || variation_word(word).is_some()
}

fn is_billboard(word: &str) -> bool {
    word.eq_ignore_ascii_case("billboard")
}

/// The number in `LOD2`.
fn lod_number(word: &str) -> Option<u32> {
    let rest = word.get(..3).filter(|start| start.eq_ignore_ascii_case("lod")).and(word.get(3..))?;
    rest.parse().ok()
}

/// The size in `4K`, in thousands of pixels.
fn size_word(word: &str) -> Option<u32> {
    let number = word.strip_suffix(['k', 'K'])?;
    number.parse().ok().filter(|size| (1..=32).contains(size))
}

/// The variation `VarA` and `Var1` are, as `A` and `1`.
fn variation_word(word: &str) -> Option<String> {
    let rest = word.get(..3).filter(|start| start.eq_ignore_ascii_case("var")).and(word.get(3..))?;
    let short = (1..=3).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_alphanumeric());
    short.then(|| rest.to_string())
}

fn words(stem: &str) -> Vec<String> {
    stem.split('_').filter(|word| !word.is_empty()).map(str::to_string).collect()
}

fn stem(file: &str) -> &str {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn is_zip(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with(".zip")
}

/// A name made safe for a file's, or none when nothing of it is left.
fn safe_name(name: &str) -> Option<String> {
    let forbidden = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control();
    let safe: String = name.chars().map(|c| if forbidden(c) { '_' } else { c }).collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = safe.trim_start_matches('.').to_string();
    (!safe.is_empty()).then_some(safe)
}

/// A file name as a URI names it: its spaces and percent signs escaped.
fn percent_encode(name: &str) -> String {
    name.replace('%', "%25").replace(' ', "%20")
}

fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes.get(at + 1..at + 3).and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                at += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Every file under `folder`, by its path from the download's top, with its
/// size, leaving out what starts with a dot.
fn walk(folder: &Path, at: &str, depth: usize, files: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "__MACOSX" {
            continue;
        }
        if files.len() >= MOST_FILES {
            return;
        }
        let path = if at.is_empty() { name } else { format!("{at}/{name}") };
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() && depth < DEPTH {
            walk(&entry.path(), &path, depth + 1, files);
        } else if metadata.is_file() {
            files.push((path, metadata.len()));
        }
    }
}

#[cfg(test)]
mod tests;
