//! The asset database, and where asset bytes come from.
//!
//! Every asset file in a project gets a stable GUID, kept in a `.meta` file
//! beside it (`Pyramid.gltf` → `Pyramid.gltf.meta`), as in Unity. Scenes and
//! other assets refer to each other by GUID, so renaming or moving a file
//! (with its `.meta`) never breaks a reference.
//!
//! The `.meta` file keeps the asset's import settings too, such as a
//! model's units and scale (see [`ModelImport`]).
//!
//! Assets are read through an [`AssetReader`]: from the project folder while
//! developing, or from built content bundles (see [`bundle`]).

pub mod bundle;
mod db;
mod guid;
mod import;
pub mod io;
mod reader;
mod reference;

pub use db::{AssetDb, AssetEntry, AssetKind, META_EXTENSION, ScanReport};
pub use guid::AssetGuid;
pub use import::{ModelImport, Normals, Tangents, TextureImport, Units, meta_path};
pub use reader::{AssetReader, FolderReader};
pub use reference::AssetRef;
