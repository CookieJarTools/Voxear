//! The project's packages, as the engine, the editor and `vox` use them:
//! resolved from its `packages.toml` against this engine, with anything
//! the machine does not have yet fetched into the shared cache. See
//! [`voxear_packages`] for what a package is.

use anyhow::Result;
pub use voxear_packages::*;

use crate::project::{Project, engine_version};

/// This engine's version, as packages' `engine` ranges are checked against.
/// A version that is not semantic counts as 0.1.0.
pub fn engine() -> semver::Version {
    semver::Version::parse(engine_version()).unwrap_or_else(|_| semver::Version::new(0, 1, 0))
}

/// Options for working with packages against this engine.
pub fn options(cache: &Cache, offline: bool, update: Update) -> Options<'_> {
    Options { engine: engine(), cache, offline, update }
}

/// The packages the project uses, each after what it needs, with
/// `packages.lock` kept to match. Fetches only what this machine does not
/// have, so it needs no network once the lock and the cache agree. A
/// project without a `packages.toml` uses none.
pub fn installed(project: &Project) -> Result<Vec<Package>> {
    if !project.root.join(PACKAGES_FILE).is_file() && !project.root.join(LOCK_FILE).is_file() {
        return Ok(Vec::new());
    }
    let cache = Cache::for_machine();
    sync(&project.root, &options(&cache, false, Update::Nothing))
}
