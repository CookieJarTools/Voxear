//! The asset database, and where asset bytes come from.
//!
//! Every asset file in a project gets a stable GUID, kept in a `.meta` file
//! beside it (`Pyramid.gltf` → `Pyramid.gltf.meta`), as in Unity. Scenes and
//! other assets refer to each other by GUID, so renaming or moving a file
//! (with its `.meta`) never breaks a reference.
//!
//! Assets are read through an [`AssetReader`]: from the project folder while
//! developing, or from built content bundles (see [`bundle`]).

pub mod bundle;
mod db;
mod guid;
pub mod io;
mod reader;
mod reference;

pub use db::{AssetDb, AssetEntry, AssetKind, META_EXTENSION, ScanReport};
pub use guid::AssetGuid;
pub use reader::{AssetReader, FolderReader};
pub use reference::AssetRef;
