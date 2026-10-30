//! Content bundles: built assets packed into a few files, found through a
//! catalog. A player reads its assets from bundles instead of the project
//! folder, through the same [`AssetReader`] calls.
//!
//! A bundle is `VXBUNDLE`, a format number, then each file's bytes,
//! compressed with LZ4 where that makes them smaller. `catalog.toml` says
//! which bundle holds each asset, where its files sit, and which roots
//! (levels, prefabs) need each bundle. A bundle is named after a hash of its
//! contents, so a rebuild that changes nothing rewrites nothing.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{AssetEntry, AssetGuid, AssetKind, reader::AssetReader};

pub const CATALOG_FILE: &str = "catalog.toml";
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
    /// meshes, LODs, textures and materials in Voxear's own format.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cooked: bool,
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
}

impl Catalog {
    pub fn read(dir: &Path) -> Result<Self> {
        let path = dir.join(CATALOG_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}; run `vox content build` first", path.display()))?;
        let catalog: Catalog = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        ensure!(catalog.format == CATALOG_FORMAT, "{} uses catalog format {}", path.display(), catalog.format);
        Ok(catalog)
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(CATALOG_FILE);
        let text = format!("# Voxear content catalog, written by `vox content build`\n{}", toml::to_string(self)?);
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// The assets as database entries, so references resolve by path or GUID
    /// as they do in a project.
    pub fn entries(&self) -> Result<Vec<AssetEntry>> {
        self.assets
            .iter()
            .map(|a| Ok(AssetEntry { guid: a.guid.parse::<AssetGuid>()?, path: a.path.clone(), kind: a.kind }))
            .collect()
    }
}

/// Packs files into one bundle in memory.
pub struct BundleWriter {
    bytes: Vec<u8>,
}

impl Default for BundleWriter {
    fn default() -> Self {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&BUNDLE_FORMAT.to_le_bytes());
        Self { bytes }
    }
}

impl BundleWriter {
    pub fn add(&mut self, path: &str, data: &[u8]) -> PackedFile {
        let compressed = lz4_flex::block::compress(data);
        let stored = if compressed.len() < data.len() { compressed.as_slice() } else { data };
        let offset = self.bytes.len() as u64;
        self.bytes.extend_from_slice(stored);
        PackedFile { path: path.to_string(), offset, size: data.len() as u64, stored: stored.len() as u64 }
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Reads assets out of built bundles, opening each bundle the first time one
/// of its files is needed.
pub struct BundleReader {
    dir: PathBuf,
    bundles: Vec<BundleInfo>,
    files: HashMap<String, (usize, PackedFile)>,
    open: HashMap<usize, File>,
    opened: Vec<usize>,
}

impl BundleReader {
    pub fn new(dir: &Path, catalog: &Catalog) -> Self {
        let files = catalog
            .assets
            .iter()
            .flat_map(|asset| asset.files.iter().map(|file| (file.path.clone(), (asset.bundle, file.clone()))))
            .collect();
        Self {
            dir: dir.to_path_buf(),
            bundles: catalog.bundles.clone(),
            files,
            open: HashMap::new(),
            opened: Vec::new(),
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

    fn bundle(&mut self, index: usize) -> Result<&mut File> {
        if !self.open.contains_key(&index) {
            let info = self.bundles.get(index).with_context(|| format!("the catalog has no bundle {index}"))?;
            let path = self.dir.join(&info.file);
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
        Ok(self.open.get_mut(&index).expect("opened above"))
    }
}

impl AssetReader for BundleReader {
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let (bundle, packed) =
            self.files.get(path).cloned().with_context(|| format!("{path} is not in the built content"))?;
        let file = self.bundle(bundle)?;
        file.seek(SeekFrom::Start(packed.offset))?;
        let mut stored = vec![0u8; packed.stored as usize];
        file.read_exact(&mut stored)?;
        if packed.stored == packed.size {
            return Ok(stored);
        }
        lz4_flex::block::decompress(&stored, packed.size as usize).with_context(|| format!("decompressing {path}"))
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
