//! Packages: code, tools and content carried from one project to another,
//! as Unity's packages are.
//!
//! A package is a folder with a `package.toml` (see [`Manifest`]), laid out
//! as [`contents`] describes. A project asks for packages in its
//! `packages.toml` (see [`PackagesFile`]): from a Git repository, a folder,
//! or a registry by version. Resolving them works out every package the
//! project uses, fetches what the machine does not have into a cache shared
//! by every project on it, and writes `packages.lock`, which keeps the exact
//! commit of each so that a team all gets the same code.
//!
//! Changes go through a [`Plan`], which says what would be added, removed
//! or moved before anything is written: a package from the internet runs
//! code, so the editor and `vox` show what a new one holds and ask first.

pub mod authoring;
pub mod cache;
pub mod contents;
pub mod lock;
pub mod manifest;
pub mod project;
pub mod registry;
pub mod resolve;
pub mod source;

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
pub use cache::{Cache, Fetch};
pub use contents::Contents;
pub use lock::{LOCK_FILE, Lock};
pub use manifest::{MANIFEST_FILE, Manifest, assembly_name, title};
pub use project::{PACKAGES_FILE, PackagesFile};
pub use registry::{DEFAULT_REGISTRY, Listing, Registry, Release};
pub use resolve::{Options, Origin, Package, Registries, Update};
use semver::{Version, VersionReq};
pub use source::{GitRev, Request};

/// A package to add to a project.
#[derive(Clone, Debug)]
pub enum Add {
    Git {
        url: String,
        rev: GitRev,
        path: Option<String>,
    },
    Folder(PathBuf),
    /// By name from a registry: the newest release that works with the
    /// engine, when no range is given.
    Registry {
        name: String,
        version: Option<VersionReq>,
        registry: Option<String>,
    },
}

/// What a change to a project's packages would do, worked out but not yet
/// written.
pub struct Plan {
    project: PathBuf,
    file: PackagesFile,
    /// Every package the project would use, each after what it needs.
    pub packages: Vec<Package>,
    pub changes: Vec<Change>,
    before: Lock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Added {
        name: String,
        version: Version,
    },
    Removed {
        name: String,
        version: String,
    },
    /// A new version, or the same version from another commit or place.
    Changed {
        name: String,
        from: String,
        to: Version,
    },
}

impl Plan {
    /// The packages the project does not have yet, which a person should
    /// see, with what they hold, before they are added.
    pub fn added(&self) -> impl Iterator<Item = &Package> {
        self.packages.iter().filter(|package| {
            self.changes.iter().any(|change| matches!(change, Change::Added { name, .. } if *name == package.name))
        })
    }

    /// The packages that would come from somewhere the project has not
    /// taken them from before: the ones added, and any moved to another
    /// repository, folder or registry. Their code is new to the project.
    pub fn arriving(&self) -> impl Iterator<Item = &Package> {
        self.packages.iter().filter(|package| match self.before.get(&package.name) {
            None => true,
            Some(before) => {
                let now = lock::Locked::of(package);
                (&before.git, &before.folder, &before.registry) != (&now.git, &now.folder, &now.registry)
            }
        })
    }

    /// Writes `packages.toml` and `packages.lock`, and returns the
    /// packages the project now uses.
    pub fn apply(self) -> Result<Vec<Package>> {
        self.file.save()?;
        Lock::of(&self.packages).write(&self.project)?;
        Ok(self.packages)
    }
}

/// The packages a project uses as its `packages.toml` asks, with the lock
/// written to match: what a build of its scripts and content starts with.
/// Fetches only what the machine does not have, unless `options` says to
/// update.
pub fn sync(project: &Path, options: &Options) -> Result<Vec<Package>> {
    plan(project, options)?.apply()
}

/// What [`sync`] would do, without writing anything.
pub fn plan(project: &Path, options: &Options) -> Result<Plan> {
    let file = PackagesFile::open(project)?;
    finish(project, file, options)
}

/// What adding a package would do. A Git package or a folder is fetched or
/// read to learn its name; one from a registry is asked for from its newest
/// release that works with the engine.
pub fn plan_add(project: &Path, add: Add, options: &Options) -> Result<Plan> {
    let mut file = PackagesFile::open(project)?;
    let (name, request) = request_for(project, &file, add, options)?;
    file.set(&name, &request)?;
    finish(project, file, options)
}

/// What removing a package the project asks for would do. Packages that
/// only it needed go with it.
pub fn plan_remove(project: &Path, name: &str, options: &Options) -> Result<Plan> {
    let mut file = PackagesFile::open(project)?;
    if !file.remove(name) {
        anyhow::bail!("the project does not ask for {name} in {PACKAGES_FILE}");
    }
    finish(project, file, options)
}

fn finish(project: &Path, file: PackagesFile, options: &Options) -> Result<Plan> {
    let requests = file.requests()?;
    let registries = file.registries()?;
    let lock = Lock::read(project)?;
    let packages = resolve::resolve(project, &requests, &registries, &lock, options)?;
    let changes = changes(&lock, &packages);
    Ok(Plan { project: project.to_path_buf(), file, packages, changes, before: lock })
}

fn changes(lock: &Lock, packages: &[Package]) -> Vec<Change> {
    let mut changes = Vec::new();
    for package in packages {
        let now = lock::Locked::of(package);
        match lock.get(&package.name) {
            None => changes.push(Change::Added { name: package.name.clone(), version: package.version().clone() }),
            Some(before) if *before != now => changes.push(Change::Changed {
                name: package.name.clone(),
                from: before.version.clone(),
                to: package.version().clone(),
            }),
            Some(_) => {}
        }
    }
    for before in &lock.packages {
        if !packages.iter().any(|package| package.name == before.name) {
            changes.push(Change::Removed { name: before.name.clone(), version: before.version.clone() });
        }
    }
    changes
}

fn request_for(project: &Path, file: &PackagesFile, add: Add, options: &Options) -> Result<(String, Request)> {
    let fetch = if options.offline { Fetch::Offline } else { Fetch::IfMissing };
    match add {
        Add::Git { url, rev, path } => {
            let commit = options.cache.find(&url, &rev, fetch)?;
            let checkout = options.cache.checkout(&url, &commit, fetch)?;
            let root = match &path {
                Some(path) => checkout.join(path),
                None => checkout,
            };
            let manifest = Manifest::read(&root).with_context(|| format!("{url} is not a Voxear package"))?;
            Ok((manifest.name, Request::Git { url, rev, path }))
        }
        Add::Folder(folder) => {
            let folder = if folder.is_absolute() { folder } else { std::env::current_dir()?.join(folder) };
            let manifest = Manifest::read(&folder)?;
            let path = relative(project, &folder).unwrap_or(folder);
            Ok((manifest.name, Request::Folder { path }))
        }
        Add::Registry { name, version, registry } => {
            let version = match version {
                Some(version) => version,
                None => {
                    let registries = file.registries()?;
                    let source = registries.get(registry.as_deref())?;
                    let listing = source
                        .listing(options.cache, &name, fetch)?
                        .with_context(|| format!("{} does not list a package called {name}", source.url))?;
                    let latest = listing
                        .latest(&options.engine)
                        .with_context(|| format!("no release of {name} works with Voxear {}", options.engine))?;
                    VersionReq::parse(&format!("^{}", latest.version))?
                }
            };
            Ok((name, Request::Registry { version, registry }))
        }
    }
}

/// `to` as a path from `from`, as `../tools/orbit`, when both are on the
/// same drive.
fn relative(from: &Path, to: &Path) -> Option<PathBuf> {
    let from = from.canonicalize().ok()?;
    let to = to.canonicalize().ok()?;
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    if from.first() != to.first() {
        return None;
    }
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut path = PathBuf::new();
    for _ in shared..from.len() {
        path.push("..");
    }
    for part in &to[shared..] {
        path.push(part);
    }
    Some(path)
}
