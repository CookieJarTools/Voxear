//! Working out which packages a project uses: the ones it asks for, the
//! ones those need, and so on, each at one version that suits everyone who
//! asks for it.
//!
//! A package asked for from Git or a folder is that package, whatever its
//! version; a range from a registry takes the newest release every range
//! allows. When the project and a package ask for the same package from
//! different places, the project's choice wins, as Cargo's patches do. There
//! is no search back through older releases when two ranges cannot both be
//! met: the error names who asked for what, which is usually enough to fix.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use semver::{Version, VersionReq};

use crate::{
    cache::{Cache, Fetch},
    lock::Lock,
    manifest::Manifest,
    registry::{Registry, Release},
    source::{GitRev, Request, short},
};

/// A package the project uses, ready on disk.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub manifest: Manifest,
    /// Its folder: in the machine's cache for one from Git or a registry,
    /// which nothing should change, or its own folder.
    pub root: PathBuf,
    pub origin: Origin,
    /// The packages it needs, by name.
    pub dependencies: Vec<String>,
    /// Whether the project asks for it itself, rather than only through
    /// other packages.
    pub direct: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Git { url: String, rev: GitRev, path: Option<String>, commit: String },
    Folder { asked: PathBuf, folder: PathBuf },
    Registry { registry: String, release: Release },
}

impl Package {
    pub fn version(&self) -> &Version {
        &self.manifest.version
    }

    /// Whether it comes from a folder of the developer's own, which they
    /// change, rather than a fixed copy in the cache.
    pub fn is_local(&self) -> bool {
        matches!(self.origin, Origin::Folder { .. })
    }
}

/// Which packages may move to newer commits and releases than the lock has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// None: the lock is kept wherever it still fits.
    Nothing,
    All,
    Only(Vec<String>),
}

impl Update {
    fn covers(&self, name: &str) -> bool {
        match self {
            Self::Nothing => false,
            Self::All => true,
            Self::Only(names) => names.iter().any(|known| known == name),
        }
    }
}

pub struct Options<'a> {
    /// The engine the packages must work with.
    pub engine: Version,
    pub cache: &'a Cache,
    /// Only what the cache has already, with no fetching.
    pub offline: bool,
    pub update: Update,
}

/// Where the project's registries are: the default one and any it names.
#[derive(Clone, Debug)]
pub struct Registries {
    pub default: Registry,
    pub named: BTreeMap<String, Registry>,
}

impl Registries {
    pub fn get(&self, name: Option<&str>) -> Result<&Registry> {
        match name {
            None => Ok(&self.default),
            Some(name) => self.named.get(name).with_context(|| {
                format!("there is no registry called {name}: add it under [registries] in packages.toml")
            }),
        }
    }
}

/// Who asked for a package, and how.
#[derive(Clone, Debug)]
struct Want {
    request: Request,
    /// The package asking, or `None` for the project.
    by: Option<String>,
    /// The folder a folder request is from.
    base: PathBuf,
}

impl Want {
    fn who(&self) -> String {
        match &self.by {
            Some(name) => name.clone(),
            None => "the project".to_string(),
        }
    }
}

/// Resolves a project's requests to the packages it uses, dependencies
/// first, so that each comes after everything it needs.
pub fn resolve(
    project: &Path,
    requests: &[(String, Request)],
    registries: &Registries,
    lock: &Lock,
    options: &Options,
) -> Result<Vec<Package>> {
    let mut resolver = Resolver { registries, lock, options, listings: BTreeMap::new() };
    let mut chosen: BTreeMap<String, Package> = BTreeMap::new();
    for _ in 0..64 {
        let wants = gather(project, requests, &chosen);
        let mut next = BTreeMap::new();
        for (name, list) in &wants {
            let package = resolver.pick(name, list, chosen.get(name))?;
            next.insert(name.clone(), package);
        }
        let settled = next.len() == chosen.len()
            && next.iter().all(|(name, package)| chosen.get(name).is_some_and(|known| known.origin == package.origin));
        chosen = next;
        if settled {
            return order(chosen);
        }
    }
    bail!("the packages' needs keep changing each other and do not settle: look for packages that ask for each other")
}

/// Everything asked for: by the project, and by each package the project
/// reaches through what has been picked so far.
fn gather(
    project: &Path,
    requests: &[(String, Request)],
    chosen: &BTreeMap<String, Package>,
) -> BTreeMap<String, Vec<Want>> {
    let mut wants: BTreeMap<String, Vec<Want>> = BTreeMap::new();
    let mut reached = BTreeSet::new();
    let mut queue = Vec::new();
    for (name, request) in requests {
        let want = Want { request: request.clone(), by: None, base: project.to_path_buf() };
        wants.entry(name.clone()).or_default().push(want);
        queue.push(name.clone());
    }
    while let Some(name) = queue.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let Some(package) = chosen.get(&name) else { continue };
        for (needed, request) in &package.manifest.dependencies {
            let want = Want { request: request.clone(), by: Some(name.clone()), base: package.root.clone() };
            wants.entry(needed.clone()).or_default().push(want);
            queue.push(needed.clone());
        }
    }
    wants
}

struct Resolver<'a> {
    registries: &'a Registries,
    lock: &'a Lock,
    options: &'a Options<'a>,
    listings: BTreeMap<(String, String), crate::registry::Listing>,
}

impl Resolver<'_> {
    fn pick(&mut self, name: &str, wants: &[Want], before: Option<&Package>) -> Result<Package> {
        let exact = wants
            .iter()
            .find(|want| want.by.is_none() && !is_range(&want.request))
            .or_else(|| wants.iter().find(|want| !is_range(&want.request)));
        let package = match exact {
            Some(want) => {
                for other in wants.iter().filter(|other| other.by.is_some() && !is_range(&other.request)) {
                    if want.by.is_some() && !same_place(want, other) {
                        bail!(
                            "{} asks for {name} from {}, but {} asks for it from {}: ask for it from one of them in \
                             packages.toml",
                            want.who(),
                            want.request,
                            other.who(),
                            other.request
                        );
                    }
                }
                self.exact(name, want, before)?
            }
            None => self.registry_release(name, wants)?,
        };
        if package.manifest.name != name {
            bail!(
                "{name} was asked for, but the package at {} is called {}",
                describe(&package.origin),
                package.manifest.name
            );
        }
        if !package.manifest.works_with(&self.options.engine) {
            bail!(
                "{name} {} works with Voxear {}, not this engine, {}",
                package.manifest.version,
                package.manifest.engine,
                self.options.engine
            );
        }
        for want in wants {
            if let Request::Registry { version, .. } = &want.request
                && !version.matches(&package.manifest.version)
            {
                bail!(
                    "{} needs {name} {version}, but the {name} in use, from {}, is {}",
                    want.who(),
                    describe(&package.origin),
                    package.manifest.version
                );
            }
        }
        let direct = wants.iter().any(|want| want.by.is_none());
        let dependencies = package.manifest.dependencies.iter().map(|(needed, _)| needed.clone()).collect();
        Ok(Package { direct, dependencies, ..package })
    }

    fn fetch(&self, name: &str) -> Fetch {
        if self.options.offline {
            Fetch::Offline
        } else if self.options.update.covers(name) {
            Fetch::Always
        } else {
            Fetch::IfMissing
        }
    }

    fn exact(&mut self, name: &str, want: &Want, before: Option<&Package>) -> Result<Package> {
        match &want.request {
            Request::Folder { path } => {
                let folder = want.base.join(path);
                let folder = folder.canonicalize().with_context(|| format!("{name}'s folder, {}", folder.display()))?;
                let manifest = Manifest::read(&folder)?;
                let origin = Origin::Folder { asked: path.clone(), folder: folder.clone() };
                Ok(package(name, manifest, folder, origin))
            }
            Request::Git { url, rev, path } => {
                if let Some(before) = before
                    && let Origin::Git { url: known, rev: known_rev, path: known_path, .. } = &before.origin
                    && (known, known_rev, known_path) == (url, rev, path)
                {
                    return Ok(before.clone());
                }
                let cache = self.options.cache;
                let locked = self.lock.get(name).and_then(|locked| locked.git_commit(url, rev, path));
                let commit = match locked {
                    Some(commit) if !self.options.update.covers(name) => commit.to_string(),
                    _ => cache.find(url, rev, self.fetch(name))?,
                };
                let checkout = cache.checkout(url, &commit, self.fetch(name))?;
                let root = within(&checkout, path.as_deref())?;
                let manifest = Manifest::read(&root).with_context(|| format!("{url} at {}", short(&commit)))?;
                let origin = Origin::Git { url: url.clone(), rev: rev.clone(), path: path.clone(), commit };
                Ok(package(name, manifest, root, origin))
            }
            Request::Registry { .. } => unreachable!("ranges are picked from a registry"),
        }
    }

    fn registry_release(&mut self, name: &str, wants: &[Want]) -> Result<Package> {
        let mut registry_name: Option<&Option<String>> = None;
        let mut ranges: Vec<&VersionReq> = Vec::new();
        for want in wants {
            let Request::Registry { version, registry } = &want.request else { continue };
            if let Some(known) = registry_name
                && known != registry
            {
                bail!("{name} is asked for from two different registries; ask for it from one in packages.toml");
            }
            registry_name = Some(registry);
            ranges.push(version);
        }
        let registry = self.registries.get(registry_name.and_then(|name| name.as_deref()))?.clone();
        let locked = self.lock.get(name).filter(|locked| locked.registry.as_deref() == Some(registry.url.as_str()));
        let release = match locked.and_then(|locked| locked_release(locked, &ranges)) {
            Some(release) if !self.options.update.covers(name) => release,
            _ => self.best(&registry, name, &ranges, wants)?,
        };
        let cache = self.options.cache;
        let checkout = cache.checkout(&release.git, &release.commit, self.fetch(name))?;
        let root = within(&checkout, release.path.as_deref())?;
        let manifest =
            Manifest::read(&root).with_context(|| format!("{name} {} from {}", release.version, registry.url))?;
        let origin = Origin::Registry { registry: registry.url.clone(), release };
        Ok(package(name, manifest, root, origin))
    }

    fn best(&mut self, registry: &Registry, name: &str, ranges: &[&VersionReq], wants: &[Want]) -> Result<Release> {
        let key = (registry.url.clone(), name.to_string());
        if !self.listings.contains_key(&key) {
            let fetch = if self.options.offline { Fetch::Offline } else { Fetch::IfMissing };
            let listing = registry
                .listing(self.options.cache, name, fetch)?
                .with_context(|| format!("{} does not list a package called {name}", registry.url))?;
            self.listings.insert(key.clone(), listing);
        }
        let listing = &self.listings[&key];
        if let Some(release) = listing.best(ranges, &self.options.engine) {
            return Ok(release.clone());
        }
        let asked: Vec<String> = wants
            .iter()
            .filter_map(|want| match &want.request {
                Request::Registry { version, .. } => Some(format!("{} needs {version}", want.who())),
                _ => None,
            })
            .collect();
        let known: Vec<String> = listing
            .releases
            .iter()
            .filter(|release| !release.yanked)
            .map(|release| release.version.to_string())
            .collect();
        bail!(
            "no release of {name} suits everyone and this engine: {}. It has {}",
            asked.join(", "),
            if known.is_empty() { "no releases".to_string() } else { known.join(", ") }
        )
    }
}

fn package(name: &str, manifest: Manifest, root: PathBuf, origin: Origin) -> Package {
    Package { name: name.to_string(), manifest, root, origin, dependencies: Vec::new(), direct: false }
}

fn locked_release(locked: &crate::lock::Locked, ranges: &[&VersionReq]) -> Option<Release> {
    let version = locked.version()?;
    if !ranges.iter().all(|range| range.matches(&version)) {
        return None;
    }
    Some(Release {
        version,
        git: locked.git.clone()?,
        tag: locked.tag.clone(),
        commit: locked.commit.clone()?,
        path: locked.path.clone(),
        engine: VersionReq::STAR,
        yanked: false,
    })
}

fn within(checkout: &Path, path: Option<&str>) -> Result<PathBuf> {
    let Some(path) = path else { return Ok(checkout.to_path_buf()) };
    let inner = Path::new(path);
    if inner.is_absolute() || inner.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
        bail!("`{path}` should be a folder inside the repository");
    }
    Ok(checkout.join(inner))
}

fn is_range(request: &Request) -> bool {
    matches!(request, Request::Registry { .. })
}

fn same_place(a: &Want, b: &Want) -> bool {
    match (&a.request, &b.request) {
        (Request::Folder { path: one }, Request::Folder { path: two }) => {
            let one = a.base.join(one).canonicalize().ok();
            one.is_some() && one == b.base.join(two).canonicalize().ok()
        }
        (one, two) => one == two,
    }
}

pub(crate) fn describe(origin: &Origin) -> String {
    match origin {
        Origin::Git { url, commit, .. } => format!("{url} at {}", short(commit)),
        Origin::Folder { folder, .. } => folder.display().to_string(),
        Origin::Registry { registry, .. } => registry.clone(),
    }
}

/// The packages in an order where each comes after what it needs.
fn order(mut chosen: BTreeMap<String, Package>) -> Result<Vec<Package>> {
    let mut ordered = Vec::with_capacity(chosen.len());
    let mut placed = BTreeSet::new();
    while !chosen.is_empty() {
        let ready: Vec<String> = chosen
            .iter()
            .filter(|(_, package)| package.dependencies.iter().all(|needed| placed.contains(needed)))
            .map(|(name, _)| name.clone())
            .collect();
        if ready.is_empty() {
            let left: Vec<&str> = chosen.keys().map(String::as_str).collect();
            bail!("these packages need each other in a circle, which their scripts cannot do: {}", left.join(", "));
        }
        for name in ready {
            placed.insert(name.clone());
            ordered.push(chosen.remove(&name).expect("ready packages are chosen"));
        }
    }
    Ok(ordered)
}
