//! The asset database, and where asset bytes come from.
//!
//! Every asset file in a project gets a stable GUID, kept in a `.meta` file
//! beside it (`Pyramid.gltf` → `Pyramid.gltf.meta`), as in Unity. Scenes and
//! other assets refer to each other by GUID, so renaming or moving a file
//! (with its `.meta`) never breaks a reference.
//!
//! The `.meta` file keeps the asset's import settings too, such as a
//! model's units and scale (see [`ModelImport`]), or whether a sound
//! streams (see [`SoundImport`]).
//!
//! Assets are read through an [`AssetReader`]: from the project folder while
//! developing, or from built content bundles (see [`bundle`]).
//!
//! Packages' assets are mounted beside the project's, under
//! `Packages/<name>/` (see [`mount`]).

pub mod bundle;
mod db;
mod guid;
mod import;
pub mod io;
pub mod mount;
mod reader;
mod reference;

pub use db::{AssetDb, AssetEntry, AssetKind, META_EXTENSION, ScanReport};
pub use guid::AssetGuid;
pub use import::{
    BuildInclusion, MaterialRemap, ModelImport, Normals, SoundImport, SoundLoad, Tangents, TextureImport, Units, WindMasks, meta_path,
};
pub use mount::{Access, Mount, Mounts, PACKAGES_FOLDER, PackageMount};
pub use reader::{AssetReader, FolderReader};
pub use reference::AssetRef;
