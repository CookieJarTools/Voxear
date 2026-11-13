//! The content build: packs a project's assets into bundles by how they are
//! used, so a level loads exactly the bundles it needs.
//!
//! 1. **Roots** are what a game can ask for by name: every level, prefab,
//!    UI document and block, and every asset a behaviour field names, since
//!    `Ref<T>` loads those on their own. `[content]` in `voxear.toml` adds roots with `include` and
//!    takes them away with `exclude` (see [`ContentSettings`]). `[ContentRoot]`
//!    in scripts names what code loads by path ([`CodeRoot`]); like a field,
//!    it ships even when excluded.
//! 2. **Dependencies** come from the files: a level's scenes, the prefabs,
//!    meshes, materials and sounds that scenes and prefabs use, a model's
//!    textures, and the `.material` assets its materials are remapped to.
//!    Files a model reads that are not assets, such as its buffers, are
//!    cooked into it, and the sheets a UI document names ship beside it, so
//!    it reads them by path as it does from the project. So does a sound's
//!    `.meta`, for its import settings, and a remapped model's, for its
//!    remapping.
//! 3. An asset's **usage set** is the roots that reach it. Assets with the
//!    same usage set share a bundle, so loading a root opens only bundles
//!    it needs, and what two levels share sits in a bundle both open.
//!    Assets no root reaches are left out.
//! 4. Models are **cooked**: stored ready for the GPU, with their meshes,
//!    LODs, materials and textures (BC7, or BC5 for normal maps, with their
//!    mips), so loading one is a read and an upload. So are pictures used on
//!    their own, as their import settings say, BC7 or BC5 by how the shipped
//!    materials use them. Cooks are cached by a hash of their inputs, so an
//!    unchanged model or picture is not cooked again. Sounds ship in
//!    Voxear's own wrapping (see [`voxear_assets::packed`]): no asset ships
//!    as the file it was made as.
//! 5. Bundles are named by a hash of their contents, so an unchanged bundle
//!    is not written again.
//!
//! A **server** build ([`ContentSettings::server`]) leaves out what only
//! shows or sounds: pictures, materials, shaders, sounds and the paths sound
//! takes, light profiles, lighting and UI documents. Models ship without
//! their pictures, for their colliders' shapes and their skeletons. What is
//! left out stays listed in the catalog, so references to it still resolve.
//! An asset whose `.meta` says `[build] server = true` ships whole.
//!
//! The project's packages' assets are part of the build, under
//! `Packages/<name>/` (see [`crate::package_assets`]). A package's levels,
//! prefabs and UI documents are not roots of their own: they ship when
//! something the game ships uses them, a field names them, or `include` or
//! `[ContentRoot]` asks for them.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use voxear_assets::{
    AssetDb, AssetGuid, AssetKind, AssetRef, BuildInclusion, MaterialRemap, Mount, Mounts, TextureImport,
    bundle::{BundleInfo, BundleWriter, CATALOG_FORMAT, Catalog, CatalogAsset, Compression, StrippedAsset, content_hash},
};
use voxear_render::{CookedPicture, PictureUse};

use crate::{MaterialMap, project::ContentSettings};

/// What a build produced, for `vox content build` to print.
#[derive(Debug)]
pub struct BuildReport {
    pub output: PathBuf,
    pub bundles: Vec<BundleReport>,
    /// Assets no root reaches, which the build left out.
    pub excluded: Vec<String>,
    /// Models and pictures cooked this time, and those whose cook came from
    /// the cache.
    pub cooked: usize,
    pub cached: usize,
    /// Code roots that match no asset, which is most likely a typo.
    pub unmatched: Vec<CodeRoot>,
    /// What a server build left out, and how many bytes of files that was.
    pub stripped: Vec<String>,
    pub stripped_bytes: u64,
}

/// A `[ContentRoot]` in the game's scripts: assets that code loads by a path
/// the build cannot see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeRoot {
    /// A project path, a GUID, or a path pattern with `*` and `**`; or
    /// `type:` and a Definition's name, for every data asset that holds it,
    /// as code finds them all without naming one (a project's recipes).
    pub pattern: String,
    /// The type or member that declares it, for messages.
    pub source: String,
}

impl CodeRoot {
    /// What comes before a Definition's name in the pattern of a root that
    /// is every data asset of that Definition.
    pub const TYPE: &'static str = "type:";

    fn matches(&self, guid: AssetGuid, path: &str) -> bool {
        glob(&self.pattern, path) || self.pattern.parse::<AssetGuid>().is_ok_and(|g| g == guid)
    }

    /// Whether it is every data asset holding the Definition `held`.
    fn holds(&self, held: Option<&str>) -> bool {
        held.is_some_and(|held| self.pattern.strip_prefix(Self::TYPE) == Some(held))
    }
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
    /// Files it reads beside its own: a model's buffers and images, which
    /// are cooked into it, or a UI document's sheets and a level's baked
    /// paths, which ship with it.
    companions: Vec<String>,
    /// The pictures it names, each with how it uses it.
    pictures: Vec<(AssetGuid, PictureUse)>,
    /// For a data asset, the Definition it holds, by name.
    holds: Option<String>,
}

pub fn build(root: &Path, output: &Path, settings: &ContentSettings, code_roots: &[CodeRoot]) -> Result<BuildReport> {
    build_with(root, output, settings, code_roots, &mut |_, _, _| Ok(()))
}

/// [`build`], telling `progress` of each asset as it is read or cooked:
/// how many are done, of how many, and its path. An error from `progress`
/// stops the build there, as a cancelled build does.
pub fn build_with(
    root: &Path,
    output: &Path,
    settings: &ContentSettings,
    code_roots: &[CodeRoot],
    progress: &mut dyn FnMut(usize, usize, &str) -> Result<()>,
) -> Result<BuildReport> {
    let mounts = crate::package_assets::mounts(root)?;
    build_mounted(&mounts, output, settings, code_roots, progress)
}

/// [`build_with`], with the packages' assets mounted as `mounts` says rather
/// than as the project's `packages.toml` resolves.
pub fn build_mounted(
    mounts: &Mounts,
    output: &Path,
    settings: &ContentSettings,
    code_roots: &[CodeRoot],
    progress: &mut dyn FnMut(usize, usize, &str) -> Result<()>,
) -> Result<BuildReport> {
    let root = mounts.root();
    let (db, _) = AssetDb::scan_mounted(mounts.clone())?;
    let mut needs = HashMap::new();
    for entry in db.entries() {
        let found = find_needs(&db, entry.guid, entry.kind, &entry.path)
            .with_context(|| format!("reading what {} uses", entry.path))?;
        needs.insert(entry.guid, found);
    }

    let mut usage = usage_sets(&db, &needs, settings, code_roots);
    // A server goes without what only shows or sounds, unless an asset says it needs it.
    let mut stripped = Vec::new();
    if settings.server {
        usage.retain(|guid, _| {
            let entry = db.get(*guid).expect("from the database");
            let keep = server_reads(entry.kind) || included(db.mounts(), &entry.path).server;
            if !keep {
                stripped.push(entry.clone());
            }
            keep
        });
        stripped.sort_by(|a, b| a.path.cmp(&b.path));
    }
    let file_size = |path: &str| db.mounts().file(path).metadata().map_or(0, |m| m.len());
    let mut stripped_bytes: u64 = stripped.iter().map(|e| file_size(&e.path)).sum();
    // How what ships uses each picture, which says how the picture is cooked.
    let mut uses: HashMap<AssetGuid, PictureUse> = HashMap::new();
    let shipped = needs.iter().filter(|(owner, _)| usage.contains_key(*owner));
    for &(picture, how) in shipped.flat_map(|(_, found)| &found.pictures) {
        uses.entry(picture).and_modify(|was| *was = was.and(how)).or_insert(how);
    }
    // A root that is every data asset of a Definition matches none in a project that has none, which is no mistake.
    let matched = |root: &CodeRoot| {
        root.pattern.starts_with(CodeRoot::TYPE) || db.entries().any(|e| root.matches(e.guid, &e.path))
    };
    let unmatched = code_roots.iter().filter(|root| !matched(root)).cloned().collect();
    let mut excluded: Vec<String> =
        db.entries().filter(|e| !usage.contains_key(&e.guid)).map(|e| e.path.clone()).collect();
    excluded.sort();

    // Every asset as it will ship: models and pictures cooked, sounds wrapped,
    // everything else as written, and a UI document's sheets, a sound's import
    // settings, a level's baked paths and a terrain's heights beside it.
    let mut cooker = Cooker::open(root, db.mounts())?;
    let mut stored = HashMap::with_capacity(usage.len());
    let mut beside: HashMap<AssetGuid, Vec<(String, Vec<u8>)>> = HashMap::new();
    for (done, guid) in usage.keys().enumerate() {
        let entry = db.get(*guid).expect("from the database");
        progress(done, usage.len(), &entry.path)?;
        let bytes = if entry.kind == AssetKind::Model {
            let pictures = !settings.server || included(db.mounts(), &entry.path).server;
            // The pictures a model reads, which a server's cook goes without.
            if !pictures {
                let picture = |file: &&String| AssetKind::from_path(Path::new(file.as_str())) == Some(AssetKind::Texture);
                stripped_bytes += needs[guid].companions.iter().filter(picture).map(|file| file_size(file)).sum::<u64>();
            }
            cooker.cook(&entry.path, &needs[guid].companions, pictures).with_context(|| format!("cooking {}", entry.path))?
        } else if entry.kind == AssetKind::Texture {
            // A picture nothing shipped names, as one code loads by path, is taken for colours.
            let how = uses.get(guid).copied().unwrap_or(PictureUse::Colour);
            cooker.cook_picture(&entry.path, how).with_context(|| format!("cooking {}", entry.path))?
        } else if entry.kind == AssetKind::Sound {
            // A sound file compressed already would only grow.
            let compression = if compressed_sound(&entry.path) { Compression::None } else { settings.compression };
            let extension = entry.path.rsplit_once('.').map_or("", |(_, extension)| extension).to_ascii_lowercase();
            voxear_assets::packed::wrap_sound(&extension, &read(db.mounts(), &entry.path)?, compression)
        } else {
            read(db.mounts(), &entry.path)?
        };
        stored.insert(*guid, bytes);
        let remapped = || !MaterialRemap::read_meta(&db.mounts().meta_file(&entry.path)).0.is_empty();
        if entry.kind == AssetKind::Model && remapped() {
            let meta = format!("{}.{}", entry.path, voxear_assets::META_EXTENSION);
            beside.insert(*guid, vec![(meta.clone(), read(db.mounts(), &meta)?)]);
        }
        if matches!(entry.kind, AssetKind::Ui | AssetKind::Sound | AssetKind::Level | AssetKind::Terrain) {
            let why = match entry.kind {
                AssetKind::Ui => "names",
                AssetKind::Sound => "keeps its import settings in",
                AssetKind::Terrain => "keeps its heights in",
                _ => "keeps its paths in",
            };
            let files = needs[guid].companions.iter().map(|file| {
                let bytes = read(db.mounts(), file).with_context(|| format!("{} {why} {file}", entry.path))?;
                Ok((file.clone(), bytes))
            });
            beside.insert(*guid, files.collect::<Result<_>>()?);
        }
    }

    // One bundle per distinct usage set, then split and merged to size.
    let mut groups: BTreeMap<Vec<String>, Vec<AssetGuid>> = BTreeMap::new();
    for (guid, roots) in &usage {
        groups.entry(roots.iter().cloned().collect()).or_default().push(*guid);
    }
    let path_of = |guid: &AssetGuid| db.get(*guid).expect("from the database").path.clone();
    let limits = (settings.merge_below_kb * 1024, settings.split_above_mb * 1024 * 1024);
    let size_of = |g: &AssetGuid| {
        let sheets: usize = beside.get(g).map_or(0, |files| files.iter().map(|(_, bytes)| bytes.len()).sum());
        (stored[g].len() + sheets) as u64
    };
    let planned = plan_bundles(groups, size_of, path_of, limits);

    std::fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;
    let mut catalog = Catalog { format: CATALOG_FORMAT, ..Catalog::default() };
    let mut reports = Vec::with_capacity(planned.len());
    let mut bundle_of = HashMap::new();
    for (index, Planned { roots, assets, .. }) in planned.into_iter().enumerate() {
        let mut writer = BundleWriter::new(settings.compression);
        for &guid in &assets {
            let entry = db.get(guid).expect("from the database");
            // A cooked model compressed its own GPU data, as GDeflate the GPU unpacks, and a
            // wrapped sound was compressed as it was wrapped.
            let as_is = matches!(entry.kind, AssetKind::Model | AssetKind::Sound);
            let mut files = vec![if as_is {
                writer.add_as_is(&entry.path, &stored[&guid])
            } else {
                writer.add(&entry.path, &stored[&guid])
            }];
            for (sheet, bytes) in beside.get(&guid).into_iter().flatten() {
                files.push(writer.add(sheet, bytes));
            }
            catalog.assets.push(CatalogAsset {
                guid: guid.to_string(),
                path: entry.path.clone(),
                kind: entry.kind,
                bundle: index,
                files,
                cooked: matches!(entry.kind, AssetKind::Model | AssetKind::Texture),
                package: entry.mount.package().map(str::to_string),
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
    catalog.stripped = stripped
        .iter()
        .map(|e| StrippedAsset {
            guid: e.guid.to_string(),
            path: e.path.clone(),
            kind: e.kind,
            package: e.mount.package().map(str::to_string),
        })
        .collect();
    let shaders: Vec<(AssetGuid, String, String)> = usage
        .keys()
        .filter_map(|guid| db.get(*guid).filter(|entry| entry.kind == AssetKind::Shader))
        .map(|entry| Ok((entry.guid, entry.path.clone(), String::from_utf8(stored[&entry.guid].clone())?)))
        .collect::<Result<_>>()?;
    // The built-in shaders the materials shipped pick, which are compiled in with the project's.
    let picked: Vec<_> = usage
        .keys()
        .filter(|guid| db.get(**guid).is_some_and(|entry| entry.kind == AssetKind::Material))
        .map(|guid| {
            let text = std::str::from_utf8(&stored[guid]).ok();
            text.and_then(|text| toml::from_str::<crate::MaterialData>(text).ok()).and_then(|data| data.shader)
        })
        .collect();
    let used = crate::shaders::builtins_used(picked);
    // A server draws nothing, so its shaders are left out.
    if !settings.server {
        progress(usage.len(), usage.len(), "custom shaders")?;
        let scratch = root.join(".voxear").join("ShaderBuild");
        compile_shaders(shaders, &used, &scratch, output).context("compiling the project's custom shaders")?;
    }
    catalog.write(output)?;
    remove_stale(output, &catalog)?;
    cooker.prune()?;
    let (cooked, cached) = (cooker.cooked, cooker.cached);
    let stripped = stripped.into_iter().map(|e| e.path).collect();
    Ok(BuildReport {
        output: output.to_path_buf(),
        bundles: reports,
        excluded,
        cooked,
        cached,
        unmatched,
        stripped,
        stripped_bytes,
    })
}

/// Whether a dedicated server reads assets of this kind: what makes up its
/// levels and their Objs, its input and data. What only shows or sounds it
/// goes without.
pub fn server_reads(kind: AssetKind) -> bool {
    match kind {
        AssetKind::Level
        | AssetKind::Scene
        | AssetKind::Prefab
        | AssetKind::Model
        | AssetKind::InputActions
        | AssetKind::Strings
        | AssetKind::AssetTable
        | AssetKind::MotionSet
        | AssetKind::GunHandling
        | AssetKind::Data
        | AssetKind::Terrain
        | AssetKind::Particles
        | AssetKind::WorldGen
        | AssetKind::BlockWorld
        | AssetKind::Block
        | AssetKind::Biome
        | AssetKind::Structure
        | AssetKind::Plant
        | AssetKind::VoxModel => true,
        AssetKind::Texture
        | AssetKind::Material
        | AssetKind::Ui
        | AssetKind::Sound
        | AssetKind::Paths
        | AssetKind::LightProfile
        | AssetKind::Lighting
        | AssetKind::LookPack
        | AssetKind::TexturePack
        | AssetKind::Shader
        | AssetKind::PromptIcons => false,
    }
}

/// Which builds an asset's `.meta` asks to take it whole.
fn included(mounts: &Mounts, path: &str) -> BuildInclusion {
    BuildInclusion::read_meta(&mounts.meta_file(path))
}

/// A bundle to write: the roots that need it, and its assets in path order.
struct Planned {
    roots: Vec<String>,
    assets: Vec<AssetGuid>,
    size: u64,
}

/// Turns usage-set groups into bundles of a sensible size.
///
/// Groups over `split_above` bytes split into several bundles with the same
/// roots, so a patch or download touches less. A bundle under `merge_below`
/// bytes merges into one whose roots include all of its own: whenever any
/// of its roots loads, that bundle opens anyway, so the merge saves a file
/// and costs nothing. It picks the host with the fewest roots, then the
/// smallest, and never grows a bundle past the split size.
fn plan_bundles(
    groups: BTreeMap<Vec<String>, Vec<AssetGuid>>,
    size_of: impl Fn(&AssetGuid) -> u64,
    path_of: impl Fn(&AssetGuid) -> String,
    (merge_below, split_above): (u64, u64),
) -> Vec<Planned> {
    let split_above = if split_above == 0 { u64::MAX } else { split_above };
    let mut planned = Vec::new();
    for (roots, mut assets) in groups {
        assets.sort_by_key(&path_of);
        let mut current = Planned { roots: roots.clone(), assets: Vec::new(), size: 0 };
        for guid in assets {
            let size = size_of(&guid);
            if !current.assets.is_empty() && current.size + size > split_above {
                let full =
                    std::mem::replace(&mut current, Planned { roots: roots.clone(), assets: Vec::new(), size: 0 });
                planned.push(full);
            }
            current.size += size;
            current.assets.push(guid);
        }
        planned.push(current);
    }

    let covers = |host: &Planned, tiny: &Planned| tiny.roots.iter().all(|r| host.roots.contains(r));
    loop {
        let merge = (0..planned.len()).filter(|&t| planned[t].size < merge_below).find_map(|tiny| {
            let fits = |&host: &usize| {
                host != tiny
                    && covers(&planned[host], &planned[tiny])
                    && planned[host].size + planned[tiny].size <= split_above
            };
            let host = (0..planned.len()).filter(fits).min_by_key(|&h| (planned[h].roots.len(), planned[h].size))?;
            Some((tiny, host))
        });
        let Some((tiny, host)) = merge else { break };
        let moved = planned.remove(tiny);
        let host = &mut planned[if host > tiny { host - 1 } else { host }];
        host.size += moved.size;
        host.assets.extend(moved.assets);
        host.assets.sort_by_key(&path_of);
    }
    planned
}

/// The roots that reach each asset. Roots reach themselves and, through
/// direct needs, everything that loads with them.
fn usage_sets(
    db: &AssetDb,
    needs: &HashMap<AssetGuid, Needs>,
    settings: &ContentSettings,
    code_roots: &[CodeRoot],
) -> HashMap<AssetGuid, BTreeSet<String>> {
    let named: BTreeSet<AssetGuid> = needs.values().flat_map(|n| n.named.iter().copied()).collect();
    let matches = |patterns: &[String], path: &str| patterns.iter().any(|p| glob(p, path));
    let roots = db
        .entries()
        .filter(|e| {
            // A field or code naming an asset means something shipped uses it, so exclude cannot drop it.
            let held = needs.get(&e.guid).and_then(|found| found.holds.as_deref());
            let rooted = |root: &CodeRoot| root.matches(e.guid, &e.path) || root.holds(held);
            let needed = named.contains(&e.guid) || code_roots.iter().any(rooted);
            // Code loads levels, prefabs and UI documents by path, which the build cannot see, and the game
            // reads its input actions and every string and asset table when it starts, an asset table taking
            // each language's asset with it. The project's shaders all ship, as they are compiled in together. So
            // do its blocks: any world can hold any of them, and scripts place them by name. And its look packs,
            // which a player picks from a list of all there are. A package's levels,
            // prefabs, UI documents and blocks ship only when the game uses them, so a package of many ships what
            // the game takes from it; its tables still ship, since the game reads them all, and its
            // input actions never stand in for the project's.
            let blanket = match e.mount {
                Mount::Project => matches!(
                    e.kind,
                    AssetKind::Level
                        | AssetKind::Prefab
                        | AssetKind::Ui
                        | AssetKind::InputActions
                        | AssetKind::Strings
                        | AssetKind::AssetTable
                        | AssetKind::Shader
                        | AssetKind::Block
                        | AssetKind::LookPack
                        | AssetKind::TexturePack
                ),
                Mount::Package(_) => matches!(e.kind, AssetKind::Strings | AssetKind::AssetTable),
            } || matches(&settings.include, &e.path);
            needed || (blanket && !matches(&settings.exclude, &e.path))
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
/// Where a build keeps the project's custom shaders, compiled into the
/// engine's that draw materials, for the game to draw with as they are.
pub const SHADERS_FOLDER: &str = "Shaders";

/// The file in [`SHADERS_FOLDER`] naming what its modules were compiled
/// from, so an unchanged build does not compile them again.
const SHADERS_STAMP: &str = "compiled-from";

/// Compiles the shaders shipped into the engine's modules that draw
/// materials, into `output`'s [`SHADERS_FOLDER`]: a folder that is not there
/// for a game without any. With them, the built-in shaders in `used`.
/// Unchanged shaders and engine are not compiled again. Slang works in
/// `scratch`.
fn compile_shaders(
    shaders: Vec<(AssetGuid, String, String)>,
    used: &std::collections::BTreeSet<crate::shaders::BuiltinShader>,
    scratch: &Path,
    output: &Path,
) -> Result<()> {
    let folder = output.join(SHADERS_FOLDER);
    let registry = crate::shaders::ShaderRegistry::new(shaders, used);
    if !registry.any_used() {
        if folder.is_dir() {
            std::fs::remove_dir_all(&folder).with_context(|| format!("removing {}", folder.display()))?;
        }
        return Ok(());
    }
    if let Some(shader) = registry.shaders.iter().find(|shader| shader.used && shader.info.is_err())
        && let Err(error) = &shader.info
    {
        anyhow::bail!("{}: {error}", shader.path);
    }
    let code = voxear_render::CustomCode {
        materials: crate::shaders::dispatch_code(&registry),
        ui: crate::shaders::ui_dispatch_code(&registry),
        look: crate::shaders::look_dispatch_code(&registry),
    };
    let engine = voxear_render::engine_shader_dir().context("the engine's shader sources are not installed beside it")?;
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&engine)?.map(|entry| Ok(entry?.path())).collect::<Result<_>>()?;
    sources.sort();
    let mut parts =
        vec![code.materials.clone().into_bytes(), code.ui.clone().into_bytes(), code.look.clone().into_bytes()];
    for source in sources.iter().filter(|path| path.extension().is_some_and(|extension| extension == "slang")) {
        parts.push(std::fs::read(source)?);
    }
    let stamp = content_hash(&parts.iter().map(Vec::as_slice).collect::<Vec<_>>());
    if std::fs::read_to_string(folder.join(SHADERS_STAMP)).is_ok_and(|was| was == stamp) {
        return Ok(());
    }
    let modules = voxear_render::compile_custom_shaders(code, scratch)?;
    if folder.is_dir() {
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::create_dir_all(&folder)?;
    for (name, spirv) in modules {
        std::fs::write(folder.join(format!("{name}.spv")), spirv)?;
    }
    std::fs::write(folder.join(SHADERS_STAMP), stamp)?;
    Ok(())
}

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
        AssetKind::Level
        | AssetKind::Scene
        | AssetKind::Prefab
        | AssetKind::Material
        | AssetKind::AssetTable
        | AssetKind::Lighting
        | AssetKind::MotionSet
        | AssetKind::GunHandling
        | AssetKind::Data
        | AssetKind::Terrain
        // The pictures a particle effect's emitters draw with.
        | AssetKind::Particles
        // The blocks a world of blocks lists and the materials its own kinds' faces are drawn with; what a block names.
        | AssetKind::BlockWorld
        | AssetKind::Block
        // The blocks a biome's ground is made of; a structure's blocks, plant and biomes; a plant's wood and leaves.
        | AssetKind::Biome
        | AssetKind::Structure
        | AssetKind::Plant
        // The pictures a texture pack draws blocks and materials with, and those blocks and materials.
        | AssetKind::TexturePack
        // The pictures a set of prompt icons shows, and the set the input actions name.
        | AssetKind::PromptIcons
        | AssetKind::InputActions => {
            let text = std::fs::read_to_string(db.file(path))?;
            let table: toml::Table = toml::from_str(&text)?;
            // A terrain's layers made its heights, which go with it, so a game needs only what draws its ground:
            // its surfaces' materials and its grass's meshes and materials. One without end is made as the game
            // goes, from what its layers name too.
            let made_in_game = kind != AssetKind::Terrain || table.contains_key("endless");
            let drawn = |key: &str| made_in_game || matches!(key, "surfaces" | "grass");
            for (key, value) in table.iter().filter(|(key, _)| drawn(key)) {
                collect(db, key == "fields", key, value, &mut needs);
            }
            if kind == AssetKind::Data {
                needs.holds = table.get("type").and_then(toml::Value::as_str).map(str::to_string);
            }
            // A level's baked paths go with it, for its sound to follow round the walls.
            let paths = crate::acoustics::paths_file(path);
            if kind == AssetKind::Level && db.file(&paths).is_file() {
                needs.companions.push(paths);
            }
            // A world of blocks' changes from its ground go with it.
            let changes = voxear_terrain::blocks::edits::blocks_file(path);
            if kind == AssetKind::BlockWorld && db.file(&changes).is_file() {
                needs.companions.push(changes);
            }
            // A terrain's heights go with it.
            let heights = voxear_terrain::file::heights_file(path);
            if kind == AssetKind::Terrain && db.file(&heights).is_file() {
                needs.companions.push(heights);
            }
        }
        AssetKind::Model => {
            let bytes = read(db.mounts(), path)?;
            needs.companions = voxear_render::companion_files(path, &bytes)?;
            let remap = MaterialRemap::read_meta(&db.mounts().meta_file(path));
            needs.direct.extend(remap.0.values().filter(|material| db.get(**material).is_some()));
        }
        AssetKind::Ui => {
            // Sheets are named from the document's folder, as the game reads them.
            let markup = std::fs::read_to_string(db.file(path))?;
            let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
            let sheets = voxear_ui::markup::style_sources(&markup).map_err(|e| anyhow::anyhow!("{e}"))?;
            let beside = |sheet: String| if folder.is_empty() { sheet } else { format!("{folder}/{sheet}") };
            needs.companions = sheets.into_iter().map(beside).collect();
        }
        AssetKind::Sound => needs.companions = vec![format!("{path}.{}", voxear_assets::META_EXTENSION)],
        // A world generator and a model of blocks name nothing else, nor does a look pack.
        AssetKind::Texture
        | AssetKind::LookPack
        | AssetKind::Strings
        | AssetKind::Paths
        | AssetKind::LightProfile
        | AssetKind::Shader
        | AssetKind::WorldGen
        | AssetKind::VoxModel => {}
    }
    needs.direct.remove(&guid);
    needs.named.remove(&guid);
    Ok(needs)
}

/// Whether a sound file is compressed already: OGG Vorbis, FLAC and MP3,
/// but not WAV.
fn compressed_sound(path: &str) -> bool {
    AssetKind::from_path(Path::new(path)) == Some(AssetKind::Sound) && !path.to_ascii_lowercase().ends_with(".wav")
}

/// The assets a level, scene, prefab or material file loads with it: what it
/// names outside behaviour fields.
pub(crate) fn direct_needs(db: &AssetDb, table: &toml::Table) -> BTreeSet<AssetGuid> {
    let mut needs = Needs::default();
    for (key, value) in table {
        collect(db, key == "fields", key, value, &mut needs);
    }
    needs.direct
}

/// Every string in a file that names an asset. Those inside behaviour
/// `fields` are named rather than direct, since a `Ref<T>` loads its asset
/// only when asked. Strings that name nothing, such as Obj names, are
/// skipped. A picture is noted with how it is used, from the `key` it is
/// named under.
fn collect(db: &AssetDb, in_fields: bool, key: &str, value: &toml::Value, needs: &mut Needs) {
    match value {
        toml::Value::String(text) => {
            let Some(guid) = AssetRef::parse(text, db).ok().and_then(|r| r.guid()) else { return };
            let Some(entry) = db.get(guid) else { return };
            if entry.kind == AssetKind::Texture {
                needs.pictures.push((guid, picture_use(key)));
            }
            let target = if in_fields { &mut needs.named } else { &mut needs.direct };
            target.insert(guid);
        }
        toml::Value::Array(items) => items.iter().for_each(|v| collect(db, in_fields, key, v, needs)),
        toml::Value::Table(table) => {
            for (key, value) in table {
                collect(db, in_fields || key == "fields", key, value, needs);
            }
        }
        _ => {}
    }
}

/// How a file uses a picture it names under `key`: a material's normal map,
/// its other maps of values, and anything else, a colour map, a light's
/// cookie or a shader's property, for colours.
fn picture_use(key: &str) -> PictureUse {
    match MaterialMap::ALL.into_iter().find(|map| map.key() == key) {
        Some(MaterialMap::Normal) => PictureUse::Normal,
        Some(map) if !map.srgb() => PictureUse::Values,
        _ => PictureUse::Colour,
    }
}

/// Cooks models and pictures, reusing earlier cooks of the same inputs.
struct Cooker {
    /// The project's folder and its packages', which models are read from.
    mounts: Mounts,
    dir: PathBuf,
    used: Vec<PathBuf>,
    cooked: usize,
    cached: usize,
}

impl Cooker {
    fn open(root: &Path, mounts: &Mounts) -> Result<Self> {
        let dir = cache_dir(root);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self { mounts: mounts.clone(), dir, used: Vec::new(), cooked: 0, cached: 0 })
    }

    /// The cooked model, with its pictures or, for a server, without. Its
    /// cache key covers every file it reads with its `.meta`, for the
    /// model's import settings and its pictures', the format version and
    /// whether it has pictures, so changing any of them cooks again.
    fn cook(&mut self, path: &str, companions: &[String], pictures: bool) -> Result<Vec<u8>> {
        let meta = self.mounts.meta_file(path);
        let mut inputs = vec![voxear_render::COOKED_VERSION.to_le_bytes().to_vec(), read(&self.mounts, path)?];
        if !pictures {
            inputs.push(b"without pictures".to_vec());
        }
        inputs.push(std::fs::read(meta).unwrap_or_default());
        for file in companions {
            inputs.push(file.as_bytes().to_vec());
            // An FBX file's textures are looked for in more than one place, so one may be missing: it counts as
            // empty, and its coming changes the hash. A glTF file's buffer that is missing fails when it is read.
            inputs.push(read(&self.mounts, file).unwrap_or_default());
            inputs.push(std::fs::read(self.mounts.meta_file(file)).unwrap_or_default());
        }
        let parts: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
        let cached = self.dir.join(format!("{}.model", content_hash(&parts)));
        self.used.push(cached.clone());
        if let Ok(bytes) = std::fs::read(&cached) {
            self.cached += 1;
            return Ok(bytes);
        }
        let mounts = &self.mounts;
        let mut model = voxear_render::prepare_model(path, &mut |file| read(mounts, file))?;
        if pictures {
            model.compress_textures();
        } else {
            model.drop_pictures();
        }
        let bytes = model.cook()?.to_bytes();
        std::fs::write(&cached, &bytes).with_context(|| format!("writing {}", cached.display()))?;
        self.cooked += 1;
        Ok(bytes)
    }

    /// The picture cooked as its import settings say, for how the shipped
    /// content uses it. Its cache key covers its bytes, its `.meta`, its use
    /// and the format version.
    fn cook_picture(&mut self, path: &str, usage: PictureUse) -> Result<Vec<u8>> {
        let file = read(&self.mounts, path)?;
        let meta = std::fs::read(self.mounts.meta_file(path)).unwrap_or_default();
        let version = voxear_render::COOKED_PICTURE_VERSION.to_le_bytes();
        let cached = self.dir.join(format!("{}.picture", content_hash(&[&version, &[usage.key()], &file, &meta])));
        self.used.push(cached.clone());
        if let Ok(bytes) = std::fs::read(&cached) {
            self.cached += 1;
            return Ok(bytes);
        }
        let settings = TextureImport::from_meta(&String::from_utf8_lossy(&meta));
        let bytes = CookedPicture::cook(&file, &settings, usage)?.to_bytes();
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
pub(crate) fn glob(pattern: &str, path: &str) -> bool {
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

/// A file by its content path, from the project's folder or a package's.
fn read(mounts: &Mounts, path: &str) -> Result<Vec<u8>> {
    let file = mounts.file(path);
    std::fs::read(&file).with_context(|| format!("reading {}", file.display()))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use voxear_assets::AssetGuid;

    use super::{glob, plan_bundles};

    #[test]
    fn large_groups_split_and_tiny_bundles_merge_into_covering_ones() {
        let assets: Vec<(AssetGuid, &str, u64)> = [("a1", 60), ("a2", 60), ("ac", 30), ("b", 500), ("c", 5)]
            .into_iter()
            .map(|(name, size)| (AssetGuid::new_random(), name, size))
            .collect();
        let size: HashMap<AssetGuid, u64> = assets.iter().map(|(g, _, s)| (*g, *s)).collect();
        let name: HashMap<AssetGuid, &str> = assets.iter().map(|(g, n, _)| (*g, *n)).collect();
        let roots = |list: &[&str]| list.iter().map(|r| r.to_string()).collect::<Vec<_>>();
        let mut groups = BTreeMap::new();
        groups.insert(roots(&["A"]), vec![assets[0].0, assets[1].0]);
        groups.insert(roots(&["A", "C"]), vec![assets[2].0]);
        groups.insert(roots(&["B"]), vec![assets[3].0]);
        groups.insert(roots(&["C"]), vec![assets[4].0]);

        let planned = plan_bundles(groups, |g| size[g], |g| name[g].to_string(), (50, 100));
        let layout: Vec<(Vec<String>, Vec<&str>)> =
            planned.iter().map(|p| (p.roots.clone(), p.assets.iter().map(|g| name[g]).collect())).collect();
        assert_eq!(
            layout,
            [
                // 120 bytes for A split in two at 100.
                (roots(&["A"]), vec!["a1"]),
                (roots(&["A"]), vec!["a2"]),
                // C's 5 bytes join the bundle {A, C}, which C opens anyway.
                (roots(&["A", "C"]), vec!["ac", "c"]),
                // One asset past the split size still gets a bundle; assets never split.
                (roots(&["B"]), vec!["b"]),
            ]
        );
    }

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
