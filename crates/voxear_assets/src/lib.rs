//! The asset database.
//!
//! Every asset file in a project gets a stable GUID, kept in a `.meta` file
//! beside it (`Pyramid.gltf` → `Pyramid.gltf.meta`), as in Unity. Scenes and
//! other assets refer to each other by GUID, so renaming or moving a file
//! (with its `.meta`) never breaks a reference.

mod db;
mod guid;
mod reference;

pub use db::{AssetDb, AssetEntry, AssetKind, META_EXTENSION, ScanReport};
pub use guid::AssetGuid;
pub use reference::AssetRef;
