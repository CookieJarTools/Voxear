//! The content build: packs a project's assets into bundles by how they are
//! used, so a level loads exactly the bundles it needs.
//!
//! 1. **Roots** are what a game can ask for by name: every level and prefab,
//!    and every asset a behaviour field names, since `Ref<T>` loads those on
//!    their own. `[content]` in `voxear.toml` adds roots with `include` and
//!    takes them away with `exclude` (see [`ContentSettings`]).
//! 2. **Dependencies** come from the files: a level's scenes, the prefabs,
//!    meshes and materials that scenes and prefabs use, and a model's
//!    textures. Files a model reads that are not assets, such as its
//!    buffers, travel with it.
//! 3. An asset's **usage set** is the roots that reach it. Assets with the
//!    same usage set share a bundle, so loading a root opens only bundles
//!    it needs, and what two levels share sits in a bundle both open.
//!    Assets no root reaches are left out.
//! 4. Models are **cooked**: stored ready for the GPU, with their meshes,
//!    LODs, decoded textures and materials, so loading one is a read and an
//!    upload. Cooks are cached by a hash of their inputs, so an unchanged
//!    model is not cooked again.
//! 5. Bundles are named by a hash of their contents, so an unchanged bundle
//!    is not written again.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use voxear_assets::{
    AssetDb, AssetGuid, AssetKind, AssetRef,
    bundle::{BundleInfo, BundleWriter, CATALOG_FORMAT, Catalog, CatalogAsset, content_hash},
};

use crate::project::ContentSettings;

/// What a build produced, for `vox content build` to print.
#[derive(Debug)]
pub struct BuildReport {
    pub output: PathBuf,
    pub bundles: Vec<BundleReport>,
    /// Assets no root reaches, which the build left out.
    pub excluded: Vec<String>,
    /// Models cooked this time, and models whose cook came from the cache.
    pub cooked: usize,
    pub cached: usize,
}

#[derive(Debug)]
pub struct BundleReport {
    pub file: String,
    pub size: u64,
    pub roots: Vec<String>,
    pub assets: Vec<String>,
    /// False when an identical bundle was already there.
    pub written: bool,
}

/// Where a project's built content goes unless told otherwise.
pub fn default_output(root: &Path) -> PathBuf {
    root.join(".voxear").join("Content")
}

/// Where cooked models are kept between builds.
pub fn cache_dir(root: &Path) -> PathBuf {
    root.join(".voxear").join("ContentCache")
}

/// What one asset needs.
#[derive(Default)]
struct Needs {
    /// Assets that load with it.
    direct: BTreeSet<AssetGuid>,
    /// Assets its behaviour fields name, which load on their own.
    named: BTreeSet<AssetGuid>,
    /// Files it reads beside its own, such as a model's buffers and images.
    /// They are cooked into it rather than shipped.
    companions: Vec<String>,
}

pub fn build(root: &Path, output: &Path, settings: &ContentSettings) -> Result<BuildReport> {
    let (db, _) = AssetDb::scan(root)?;
    let mut needs = HashMap::new();
    for entry in db.entries() {
        let found = find_needs(&db, entry.guid, entry.kind, &entry.path)
            .with_context(|| format!("reading what {} uses", entry.path))?;
        needs.insert(entry.guid, found);
    }

    let usage = usage_sets(&db, &needs, settings);
    let mut excluded: Vec<String> =
        db.entries().filter(|e| !usage.contains_key(&e.guid)).map(|e| e.path.clone()).collect();
    excluded.sort();

    // One bundle per distinct usage set, each holding its assets in path order.
    let mut groups: BTreeMap<Vec<String>, Vec<AssetGuid>> = BTreeMap::new();
    for (guid, roots) in &usage {
        groups.entry(roots.iter().cloned().collect()).or_default().push(*guid);
    }
    let path_of = |guid: &AssetGuid| db.get(*guid).expect("from the database").path.clone();

    std::fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;
    let mut cooker = Cooker::open(root)?;
    let mut catalog = Catalog { format: CATALOG_FORMAT, ..Catalog::default() };
    let mut reports = Vec::with_capacity(groups.len());
    let mut bundle_of = HashMap::new();
    for (index, (roots, mut assets)) in groups.into_iter().enumerate() {
        assets.sort_by_key(path_of);
        let mut writer = BundleWriter::default();
        for &guid in &assets {
            let entry = db.get(guid).expect("from the database");
            let cooked = entry.kind == AssetKind::Model;
            let bytes = if cooked {
                cooker.cook(&entry.path, &needs[&guid].companions).with_context(|| format!("cooking {}", entry.path))?
            } else {
                read(root, &entry.path)?
            };
            catalog.assets.push(CatalogAsset {
                guid: guid.to_string(),
                path: entry.path.clone(),
                kind: entry.kind,
                bundle: index,
                files: vec![writer.add(&entry.path, &bytes)],
                cooked,
            });
            bundle_of.insert(guid, index);
        }

        let bytes = writer.finish();
        let file = format!("{}.bundle", content_hash(&[&bytes]));
        let target = output.join(&file);
        let written = !target.metadata().is_ok_and(|m| m.len() == bytes.len() as u64);
        if written {
            std::fs::write(&target, &bytes).with_context(|| format!("writing {}", target.display()))?;
        }
        catalog.bundles.push(BundleInfo {
            file: file.clone(),
            size: bytes.len() as u64,
            roots: roots.clone(),
            dependencies: Vec::new(),
        });
        reports.push(BundleReport {
            file,
            size: bytes.len() as u64,
            roots,
            assets: assets.iter().map(path_of).collect(),
            written,
        });
    }

    link_bundles(&mut catalog, &needs, &bundle_of);
    catalog.assets.sort_by(|a, b| a.path.cmp(&b.path));
    catalog.write(output)?;
    remove_stale(output, &catalog)?;
    cooker.prune()?;
    let (cooked, cached) = (cooker.cooked, cooker.cached);
    Ok(BuildReport { output: output.to_path_buf(), bundles: reports, excluded, cooked, cached })
}

/// The roots that reach each asset. Roots reach themselves and, through
/// direct needs, everything that loads with them.
fn usage_sets(
    db: &AssetDb,
    needs: &HashMap<AssetGuid, Needs>,
    settings: &ContentSettings,
) -> HashMap<AssetGuid, BTreeSet<String>> {
    let named: BTreeSet<AssetGuid> = needs.values().flat_map(|n| n.named.iter().copied()).collect();
    let matches = |patterns: &[String], path: &str| patterns.iter().any(|p| glob(p, path));
    let roots = db
        .entries()
        .filter(|e| {
            // A field naming an asset means something shipped uses it, so exclude cannot drop it.
            let blanket = matches!(e.kind, AssetKind::Level | AssetKind::Prefab) || matches(&settings.include, &e.path);
            named.contains(&e.guid) || (blanket && !matches(&settings.exclude, &e.path))
        })
        .map(|e| (e.guid, e.path.clone()));

    let mut usage: HashMap<AssetGuid, BTreeSet<String>> = HashMap::new();
    for (root, name) in roots {
        let mut stack = vec![root];
        while let Some(guid) = stack.pop() {
            if usage.entry(guid).or_default().insert(name.clone()) {
                stack.extend(needs.get(&guid).into_iter().flat_map(|n| n.direct.iter().copied()));
            }
        }
    }
    usage
}

/// Records which bundles each bundle's assets reach into.
fn link_bundles(catalog: &mut Catalog, needs: &HashMap<AssetGuid, Needs>, bundle_of: &HashMap<AssetGuid, usize>) {
    let mut links: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); catalog.bundles.len()];
    for (guid, &bundle) in bundle_of {
        let reached = needs[guid].direct.iter().filter_map(|d| bundle_of.get(d)).filter(|&&b| b != bundle);
        links[bundle].extend(reached);
    }
    for (bundle, linked) in catalog.bundles.iter_mut().zip(links) {
        bundle.dependencies = linked.into_iter().collect();
    }
}

/// Deletes bundles from earlier builds that the new catalog no longer lists.
fn remove_stale(output: &Path, catalog: &Catalog) -> Result<()> {
    for entry in std::fs::read_dir(output)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let listed = catalog.bundles.iter().any(|b| b.file == name);
        if name.ends_with(".bundle") && !listed {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

fn find_needs(db: &AssetDb, guid: AssetGuid, kind: AssetKind, path: &str) -> Result<Needs> {
    let mut needs = Needs::default();
    match kind {
        AssetKind::Level | AssetKind::Scene | AssetKind::Prefab | AssetKind::Material => {
            let text = std::fs::read_to_string(db.root().join(path))?;
            let table: toml::Table = toml::from_str(&text)?;
            for (key, value) in &table {
                collect(db, key == "fields", value, &mut needs);
            }
        }
        AssetKind::Model => {
            let bytes = read(db.root(), path)?;
            needs.companions = voxear_render::companion_files(path, &bytes)?;
        }
        AssetKind::Texture => {}
    }
    needs.direct.remove(&guid);
    needs.named.remove(&guid);
    Ok(needs)
}

/// The assets a level, scene, prefab or material file loads with it: what it
/// names outside behaviour fields.
pub(crate) fn direct_needs(db: &AssetDb, table: &toml::Table) -> BTreeSet<AssetGuid> {
    let mut needs = Needs::default();
    for (key, value) in table {
        collect(db, key == "fields", value, &mut needs);
    }
    needs.direct
}

/// Every string in a file that names an asset. Those inside behaviour
/// `fields` are named rather than direct, since a `Ref<T>` loads its asset
/// only when asked. Strings that name nothing, such as Obj names, are
/// skipped.
fn collect(db: &AssetDb, in_fields: bool, value: &toml::Value, needs: &mut Needs) {
    match value {
        toml::Value::String(text) => {
            let Some(guid) = AssetRef::parse(text, db).ok().and_then(|r| r.guid()) else { return };
            if db.get(guid).is_none() {
                return;
            }
            let target = if in_fields { &mut needs.named } else { &mut needs.direct };
            target.insert(guid);
        }
        toml::Value::Array(items) => items.iter().for_each(|v| collect(db, in_fields, v, needs)),
        toml::Value::Table(table) => {
            for (key, value) in table {
                collect(db, in_fields || key == "fields", value, needs);
            }
        }
        _ => {}
    }
}

/// Cooks models, reusing earlier cooks of the same inputs.
struct Cooker {
    root: PathBuf,
    dir: PathBuf,
    used: Vec<PathBuf>,
    cooked: usize,
    cached: usize,
}

impl Cooker {
    fn open(root: &Path) -> Result<Self> {
        let dir = cache_dir(root);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self { root: root.to_path_buf(), dir, used: Vec::new(), cooked: 0, cached: 0 })
    }

    /// The cooked model. Its cache key covers every file it reads and the
    /// format version, so changing any of them cooks again.
    fn cook(&mut self, path: &str, companions: &[String]) -> Result<Vec<u8>> {
        let mut inputs = vec![voxear_render::COOKED_VERSION.to_le_bytes().to_vec(), read(&self.root, path)?];
        for file in companions {
            inputs.push(file.as_bytes().to_vec());
            inputs.push(read(&self.root, file)?);
        }
        let parts: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
        let cached = self.dir.join(format!("{}.model", content_hash(&parts)));
        self.used.push(cached.clone());
        if let Ok(bytes) = std::fs::read(&cached) {
            self.cached += 1;
            return Ok(bytes);
        }
        let root = &self.root;
        let model = voxear_render::prepare_model(path, &mut |file| read(root, file))?;
        let bytes = model.to_bytes();
        std::fs::write(&cached, &bytes).with_context(|| format!("writing {}", cached.display()))?;
        self.cooked += 1;
        Ok(bytes)
    }

    /// Deletes cooks this build did not use, such as those of changed models.
    fn prune(&self) -> Result<()> {
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if !self.used.contains(&path) {
                std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            }
        }
        Ok(())
    }
}

/// Whether a project path matches a pattern: `*` within a folder, `**` across
/// any number of folders (including none), `?` one character.
fn glob(pattern: &str, path: &str) -> bool {
    fn segments(pattern: &[&str], path: &[&str]) -> bool {
        match (pattern.first(), path.first()) {
            (None, None) => true,
            (Some(&"**"), _) => segments(&pattern[1..], path) || (!path.is_empty() && segments(pattern, &path[1..])),
            (Some(p), Some(s)) => segment(p.as_bytes(), s.as_bytes()) && segments(&pattern[1..], &path[1..]),
            _ => false,
        }
    }
    fn segment(pattern: &[u8], text: &[u8]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some(b'*'), _) => segment(&pattern[1..], text) || (!text.is_empty() && segment(pattern, &text[1..])),
            (Some(b'?'), Some(_)) => segment(&pattern[1..], &text[1..]),
            (Some(p), Some(t)) => p == t && segment(&pattern[1..], &text[1..]),
            _ => false,
        }
    }
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments(&pattern, &path)
}

fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    let file = root.join(path);
    std::fs::read(&file).with_context(|| format!("reading {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::glob;

    #[test]
    fn patterns_match_like_gitignore_globs() {
        assert!(glob("Prefabs/*.prefab", "Prefabs/Wolf.prefab"));
        assert!(!glob("Prefabs/*.prefab", "Prefabs/Debug/Wolf.prefab"), "* stays within a folder");
        assert!(glob("Prefabs/**", "Prefabs/Debug/Deep/Wolf.prefab"));
        assert!(glob("**/Debug/*", "Prefabs/Debug/Wolf.prefab"));
        assert!(glob("**/Wolf.prefab", "Wolf.prefab"), "** can be no folders at all");
        assert!(glob("Levels/Level?.level", "Levels/Level2.level"));
        assert!(!glob("Levels/Level?.level", "Levels/Level10.level"));
    }
}
