//! The project's packages as content: each package's assets mounted beside
//! the project's under `Packages/<name>/` (see [`voxear_assets::mount`]),
//! so that scenes and behaviour fields name them as they do the project's
//! own, the editor shows them, and content builds ship the ones the game
//! uses.

use std::path::Path;

use anyhow::Result;
use voxear_assets::{Access, AssetDb, Mounts, PackageMount, ScanReport};

use crate::packages::{Cache, LOCK_FILE, PACKAGES_FILE, Package, Update, options, sync, title};

/// How a package's assets are mounted: read-only for one in the machine's
/// cache, which every project on it shares.
pub fn mount_of(package: &Package) -> PackageMount {
    PackageMount {
        name: package.name.clone(),
        title: title(&package.name),
        root: package.root.clone(),
        access: if package.is_local() { Access::Editable } else { Access::ReadOnly },
    }
}

/// The packages the project at `root` uses, resolved as
/// [`installed`](crate::packages::installed) resolves them, as mounts. A
/// project without a `packages.toml` has none.
pub fn mounts(root: &Path) -> Result<Mounts> {
    if !root.join(PACKAGES_FILE).is_file() && !root.join(LOCK_FILE).is_file() {
        return Ok(Mounts::project(root));
    }
    let cache = Cache::for_machine();
    let packages = sync(root, &options(&cache, false, Update::Nothing))?;
    Ok(Mounts::new(root, packages.iter().map(mount_of).collect()))
}

/// [`mounts`], or the project alone when its packages cannot be had, as
/// with no network and an empty cache: what the editor opens with, so that
/// a package that cannot be fetched leaves only its own assets missing.
pub fn mounts_or_project(root: &Path) -> Mounts {
    mounts(root).unwrap_or_else(|error| {
        tracing::error!("the project's packages could not be read, so their assets are missing: {error:#}");
        Mounts::project(root)
    })
}

/// Every asset of the project at `root` and of its packages, writing `.meta`
/// files for new ones of its own.
pub fn scan(root: &Path) -> Result<(AssetDb, ScanReport)> {
    AssetDb::scan_mounted(mounts(root)?)
}
