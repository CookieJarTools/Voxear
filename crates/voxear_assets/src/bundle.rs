//! Content bundles: built assets packed into a few files, found through a
//! catalog. A player reads its assets from bundles instead of the project
//! folder, through the same [`AssetReader`] calls.
//!
//! A bundle is `VXBUNDLE`, a format number, then each file's bytes,
//! compressed where that makes them smaller: with LZ4, quick to load, or
//! Zstd, smaller, as the build asks ([`Compression`]). The catalog says
//! which bundle holds each asset, where its files sit, and which roots
//! (levels, prefabs) need each bundle. It ships as `catalog.bin`, in
//! Voxear's own wrapping (see [`crate::packed`]), so no list of the game's
//! files can be read out of a build. A bundle is named after a hash of its
//! contents, so a rebuild that changes nothing rewrites nothing.

use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};

use crate::io::{Io, ReadAt};
use serde::{Deserialize, Serialize};

use crate::{AssetEntry, AssetGuid, AssetKind, Mount, reader::AssetReader};

pub const CATALOG_FILE: &str = "catalog.bin";
/// The catalog as builds before [`CATALOG_FILE`] wrote it, as text.
const TEXT_CATALOG_FILE: &str = "catalog.toml";
/// What [`CATALOG_FILE`] starts with.
const CATALOG_MAGIC: &[u8; 8] = b"VXCATLOG";
pub const CATALOG_FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"VXBUNDLE";
const BUNDLE_FORMAT: u32 = 1;
const HEADER_SIZE: u64 = 12;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub format: u32,
    #[serde(default, rename = "bundle")]
    pub bundles: Vec<BundleInfo>,
    #[serde(default, rename = "asset")]
    pub assets: Vec<CatalogAsset>,
    /// Assets a server build leaves out, still listed so references to
    /// them resolve, and the server knows to go without them.
    #[serde(default, rename = "stripped", skip_serializing_if = "Vec::is_empty")]
    pub stripped: Vec<StrippedAsset>,
}

/// An asset a server build left out: what it was, but none of its bytes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrippedAsset {
    pub guid: String,
    pub path: String,
    pub kind: AssetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BundleInfo {
    /// The file name: a hash of the contents, so unchanged bundles keep it.
    pub file: String,
    pub size: u64,
    /// The roots that need this bundle: its usage set.
    pub roots: Vec<String>,
    /// Bundles holding assets this one's assets refer to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogAsset {
    pub guid: String,
    pub path: String,
    pub kind: AssetKind,
    pub bundle: usize,
    /// The asset's own file, then any it reads beside it.
    pub files: Vec<PackedFile>,
    /// Stored ready for the GPU rather than as its source file: a model's
    /// meshes, LODs, textures and materials, or a picture with its mips, in
    /// Voxear's own format.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cooked: bool,
    /// The package it is from, by name, for one of a package's assets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PackedFile {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    /// Bytes in the bundle; smaller than `size` when compressed.
    pub stored: u64,
    /// How it was compressed, when it was: LZ4 unless the catalog says.
    #[serde(default, skip_serializing_if = "Codec::is_lz4")]
    pub codec: Codec,
}

/// How a build compresses what it packs into bundles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    /// Quick to unpack, for loading speed.
    #[default]
    Lz4,
    /// Smaller, for downloads, and slower to build.
    Zstd,
    /// Stored as they are, for quick development builds: any other build
    /// packs with LZ4 instead.
    None,
}

/// How one packed file was compressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    #[default]
    Lz4,
    Zstd,
}

impl Codec {
    fn is_lz4(&self) -> bool {
        *self == Codec::Lz4
    }
}

/// Zstd's level for bundles: near its smallest, still quick to unpack.
const ZSTD_LEVEL: i32 = 19;

impl Catalog {
    /// The catalog in `dir`: its `catalog.bin`, or the `catalog.toml` an
    /// older build wrote.
    pub fn read(dir: &Path) -> Result<Self> {
        let (path, old) = (dir.join(CATALOG_FILE), dir.join(TEXT_CATALOG_FILE));
        let text = match std::fs::read(&path) {
            Ok(bytes) => {
                let shown = path.display().to_string();
                String::from_utf8(crate::packed::unwrap(CATALOG_MAGIC, &bytes, &shown)?)
                    .with_context(|| format!("reading {shown}"))?
            }
            Err(_) if old.is_file() => {
                std::fs::read_to_string(&old).with_context(|| format!("reading {}", old.display()))?
            }
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}; run `vox content build` first", path.display()));
            }
        };
        let catalog: Catalog = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        ensure!(catalog.format == CATALOG_FORMAT, "{} uses catalog format {}", path.display(), catalog.format);
        Ok(catalog)
    }

    /// Writes the catalog into `dir` as `catalog.bin`, taking away a
    /// `catalog.toml` an older build left there.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(CATALOG_FILE);
        let bytes = crate::packed::wrap(CATALOG_MAGIC, toml::to_string(self)?.as_bytes(), Compression::Zstd);
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        let old = dir.join(TEXT_CATALOG_FILE);
        if old.is_file() {
            std::fs::remove_file(&old).with_context(|| format!("removing {}", old.display()))?;
        }
        Ok(())
    }

    /// The assets as database entries, so references resolve by path or GUID
    /// as they do in a project: those left out of a server build too.
    pub fn entries(&self) -> Result<Vec<AssetEntry>> {
        let shipped = self.assets.iter().map(|a| (&a.guid, &a.path, a.kind, &a.package));
        let stripped = self.stripped.iter().map(|a| (&a.guid, &a.path, a.kind, &a.package));
        shipped
            .chain(stripped)
            .map(|(guid, path, kind, package)| {
                let mount = package.clone().map_or(Mount::Project, Mount::Package);
                Ok(AssetEntry { guid: guid.parse::<AssetGuid>()?, path: path.clone(), kind, mount })
            })
            .collect()
    }

    /// The assets a server build left out.
    pub fn stripped_guids(&self) -> Result<Vec<AssetGuid>> {
        self.stripped.iter().map(|a| a.guid.parse::<AssetGuid>()).collect()
    }
}

/// Packs files into one bundle in memory.
pub struct BundleWriter {
    bytes: Vec<u8>,
    compression: Compression,
}

impl Default for BundleWriter {
    fn default() -> Self {
        Self::new(Compression::default())
    }
}

impl BundleWriter {
    /// A bundle whose files are compressed as `compression` says.
    pub fn new(compression: Compression) -> Self {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&BUNDLE_FORMAT.to_le_bytes());
        Self { bytes, compression }
    }

    /// Adds a file, compressed when that makes it smaller.
    pub fn add(&mut self, path: &str, data: &[u8]) -> PackedFile {
        let (compressed, codec) = match self.compression {
            Compression::Lz4 => (Some(lz4_flex::block::compress(data)), Codec::Lz4),
            // Compressing in memory cannot fail but for running out of it, when storing it as it is will do.
            Compression::Zstd => (zstd::bulk::compress(data, ZSTD_LEVEL).ok(), Codec::Zstd),
            Compression::None => (None, Codec::Lz4),
        };
        let smaller = compressed.filter(|bytes| bytes.len() < data.len());
        let stored = smaller.as_deref().unwrap_or(data);
        let offset = self.bytes.len() as u64;
        self.bytes.extend_from_slice(stored);
        let (size, stored) = (data.len() as u64, stored.len() as u64);
        PackedFile { path: path.to_string(), offset, size, stored, codec }
    }

    /// Stores a file without compressing it, for data that compressed itself.
    pub fn add_as_is(&mut self, path: &str, data: &[u8]) -> PackedFile {
        let offset = self.bytes.len() as u64;
        self.bytes.extend_from_slice(data);
        let size = data.len() as u64;
        PackedFile { path: path.to_string(), offset, size, stored: size, codec: Codec::Lz4 }
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Reads assets out of built bundles, opening each bundle the first time one
/// of its files is needed. Catalogs can be layered: a later one, such as a
/// mod or a patch, wins for every file it also has.
pub struct BundleReader {
    bundles: Vec<BundleInfo>,
    /// The folder each bundle is in, since layers come from different builds.
    dirs: Vec<PathBuf>,
    files: HashMap<String, (usize, PackedFile)>,
    open: HashMap<usize, File>,
    opened: Vec<usize>,
    io: Io,
}

impl BundleReader {
    pub fn new(dir: &Path, catalog: &Catalog) -> Self {
        let mut reader = Self {
            bundles: Vec::new(),
            dirs: Vec::new(),
            files: HashMap::new(),
            open: HashMap::new(),
            opened: Vec::new(),
            io: Io::new(),
        };
        reader.mount(dir, catalog);
        reader
    }

    /// Layers another catalog on top: its files replace any with the same
    /// project path.
    pub fn mount(&mut self, dir: &Path, catalog: &Catalog) {
        let first = self.bundles.len();
        self.bundles.extend(catalog.bundles.iter().cloned());
        self.dirs.extend(catalog.bundles.iter().map(|_| dir.to_path_buf()));
        for asset in &catalog.assets {
            for file in &asset.files {
                self.files.insert(file.path.clone(), (first + asset.bundle, file.clone()));
            }
        }
    }

    /// Closes every open bundle; each opens again when next read from.
    pub fn close_all(&mut self) {
        self.open.clear();
    }

    /// The bundles opened so far, in the order they were first needed.
    pub fn opened(&self) -> impl Iterator<Item = &BundleInfo> {
        self.opened.iter().map(|&b| &self.bundles[b])
    }

    fn open_bundle(&mut self, index: usize) -> Result<()> {
        if !self.open.contains_key(&index) {
            let info = self.bundles.get(index).with_context(|| format!("the catalog has no bundle {index}"))?;
            let path = self.dirs[index].join(&info.file);
            let mut file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
            let mut header = [0u8; HEADER_SIZE as usize];
            file.read_exact(&mut header)?;
            if &header[..8] != MAGIC || header[8..] != BUNDLE_FORMAT.to_le_bytes() {
                bail!("{} is not a Voxear bundle of format {BUNDLE_FORMAT}", path.display());
            }
            tracing::info!("opened bundle {} for {}", info.file, info.roots.join(", "));
            self.open.insert(index, file);
            self.opened.push(index);
        }
        Ok(())
    }
}

impl AssetReader for BundleReader {
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        Ok(self.read_many(&[path])?.pop().expect("one file asked for"))
    }

    /// Reads every file's stored bytes in one batch, then decompresses them.
    fn read_many(&mut self, paths: &[&str]) -> Result<Vec<Vec<u8>>> {
        let mut found = Vec::with_capacity(paths.len());
        for path in paths {
            let (bundle, packed) =
                self.files.get(*path).cloned().with_context(|| format!("{path} is not in the built content"))?;
            self.open_bundle(bundle)?;
            found.push((bundle, packed));
        }
        let mut stored: Vec<Vec<u8>> = found.iter().map(|(_, packed)| vec![0; packed.stored as usize]).collect();
        let mut reads: Vec<ReadAt> = found
            .iter()
            .zip(&mut stored)
            .map(|((bundle, packed), buffer)| ReadAt { file: &self.open[bundle], offset: packed.offset, buffer })
            .collect();
        self.io.read_all(&mut reads).with_context(|| format!("reading {} from bundles", paths.join(", ")))?;
        let unpack = |((bytes, (_, packed)), path): ((Vec<u8>, &(usize, PackedFile)), &&str)| {
            if packed.stored == packed.size {
                return Ok(bytes);
            }
            let size = packed.size as usize;
            match packed.codec {
                Codec::Lz4 => {
                    lz4_flex::block::decompress(&bytes, size).with_context(|| format!("decompressing {path}"))
                }
                Codec::Zstd => zstd::bulk::decompress(&bytes, size).with_context(|| format!("decompressing {path}")),
            }
        };
        stored.into_iter().zip(&found).zip(paths).map(unpack).collect()
    }
}

/// A 128-bit hash of some bytes, as 32 hex digits, for naming bundles by
/// their contents. Two FNV-1a passes with different bases: stable across
/// Rust versions and platforms, which `DefaultHasher` does not promise.
pub fn content_hash(parts: &[&[u8]]) -> String {
    let fnv = |basis: u64| {
        parts
            .iter()
            .flat_map(|p| p.iter())
            .fold(basis, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
    };
    format!("{:016x}{:016x}", fnv(0xcbf29ce484222325), fnv(0x6c62272e07bb0142))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_come_back_however_they_were_compressed() {
        let level = "a level that says the same thing again and again ".repeat(200);
        let mut stored = Vec::new();
        for compression in [Compression::Lz4, Compression::Zstd, Compression::None] {
            let dir = tempfile::tempdir().unwrap();
            let mut writer = BundleWriter::new(compression);
            let packed = writer.add("Levels/Main.level", level.as_bytes());
            let tiny = writer.add("tiny.txt", b"x");
            let bytes = writer.finish();
            std::fs::write(dir.path().join("one.bundle"), &bytes).unwrap();
            let bundle =
                BundleInfo { file: "one.bundle".into(), size: bytes.len() as u64, roots: vec![], dependencies: vec![] };
            let asset = CatalogAsset {
                guid: "0".into(),
                path: "Levels/Main.level".into(),
                kind: AssetKind::Level,
                bundle: 0,
                files: vec![packed.clone(), tiny.clone()],
                cooked: false,
                package: None,
            };
            // Through the catalog's own file, as a player reads it.
            Catalog { format: CATALOG_FORMAT, bundles: vec![bundle], assets: vec![asset], ..Catalog::default() }.write(dir.path()).unwrap();
            let mut reader = BundleReader::new(dir.path(), &Catalog::read(dir.path()).unwrap());
            assert_eq!(reader.read("Levels/Main.level").unwrap(), level.as_bytes(), "{compression:?}");
            assert_eq!(reader.read("tiny.txt").unwrap(), b"x");
            assert_eq!(tiny.stored, 1, "nothing is compressed bigger");
            stored.push(packed.stored);
        }
        let [lz4, zstd, none] = stored[..] else { unreachable!() };
        assert!(zstd < lz4 && lz4 < none, "Zstd packs smallest: {zstd} {lz4} {none}");
    }

    #[test]
    fn the_catalog_ships_with_no_path_in_it_that_can_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let asset = |path: &str| CatalogAsset {
            guid: AssetGuid::new_random().to_string(),
            path: path.into(),
            kind: AssetKind::Texture,
            bundle: 0,
            files: vec![PackedFile { path: path.into(), offset: 12, size: 5, stored: 5, codec: Codec::Lz4 }],
            cooked: true,
            package: None,
        };
        let paths = ["Textures/Gate_Albedo.png", "Sounds/Rain on the Roof.wav", "Levels/Coast.level"];
        let bundle = BundleInfo { file: "one.bundle".into(), size: 17, roots: vec!["Levels/Coast.level".into()], dependencies: vec![] };
        let catalog = Catalog {
            format: CATALOG_FORMAT,
            bundles: vec![bundle],
            assets: paths.iter().map(|path| asset(path)).collect(),
            ..Catalog::default()
        };
        std::fs::write(dir.path().join(TEXT_CATALOG_FILE), "left by an older build").unwrap();
        catalog.write(dir.path()).unwrap();

        let bytes = std::fs::read(dir.path().join(CATALOG_FILE)).unwrap();
        for path in paths {
            let name = path.rsplit('/').next().unwrap().split('.').next().unwrap();
            assert!(!bytes.windows(name.len()).any(|w| w == name.as_bytes()), "{name} shows in the catalog");
        }
        assert!(!bytes.windows(4).any(|w| w == b"guid"), "nor do its keys");
        assert!(!dir.path().join(TEXT_CATALOG_FILE).exists(), "an older build's text catalog goes");
        let read = Catalog::read(dir.path()).unwrap();
        assert_eq!(read.assets, catalog.assets);
        assert_eq!(read.bundles, catalog.bundles);
    }

    #[test]
    fn an_older_builds_text_catalog_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let text = "format = 1\n\n[[bundle]]\nfile = \"one.bundle\"\nsize = 12\nroots = [\"Levels/Main.level\"]\n";
        std::fs::write(dir.path().join(TEXT_CATALOG_FILE), text).unwrap();
        let catalog = Catalog::read(dir.path()).unwrap();
        assert_eq!(catalog.bundles[0].file, "one.bundle");
        assert!(Catalog::read(tempfile::tempdir().unwrap().path()).is_err(), "no catalog at all");
    }
}
