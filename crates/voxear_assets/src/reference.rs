use std::fmt;

use anyhow::{Context, Result};

use crate::{db::AssetDb, guid::AssetGuid};

/// A reference from one asset to another, as written in scene and level files:
///
/// - `builtin:cube`: an engine primitive.
/// - `9f2c…e1`: an asset by GUID; `9f2c…e1#mesh0.0`: a part of one (a mesh or
///   material inside a model).
/// - `Models/Pyramid.gltf#mesh0.0`: a project path, for hand-written files.
///   It is resolved to a GUID on load, and saved back as one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssetRef {
    Builtin(String),
    Asset { guid: AssetGuid, part: Option<String> },
}

impl AssetRef {
    pub fn builtin(name: &str) -> Self {
        Self::Builtin(name.to_string())
    }

    pub fn asset(guid: AssetGuid, part: Option<&str>) -> Self {
        Self::Asset { guid, part: part.map(str::to_string) }
    }

    pub fn guid(&self) -> Option<AssetGuid> {
        match self {
            Self::Asset { guid, .. } => Some(*guid),
            Self::Builtin(_) => None,
        }
    }

    pub fn part(&self) -> Option<&str> {
        match self {
            Self::Asset { part, .. } => part.as_deref(),
            Self::Builtin(_) => None,
        }
    }

    /// Parses any of the forms above, resolving paths through the database.
    pub fn parse(text: &str, db: &AssetDb) -> Result<Self> {
        if let Some(name) = text.strip_prefix("builtin:") {
            return Ok(Self::builtin(name));
        }
        let (target, part) = match text.split_once('#') {
            Some((target, part)) => (target, Some(part)),
            None => (text, None),
        };
        let guid = match target.parse::<AssetGuid>() {
            Ok(guid) => guid,
            Err(_) => db.guid_of(target).with_context(|| format!("no asset at `{target}`"))?,
        };
        Ok(Self::asset(guid, part))
    }
}

impl fmt::Display for AssetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(name) => write!(f, "builtin:{name}"),
            Self::Asset { guid, part: None } => write!(f, "{guid}"),
            Self::Asset { guid, part: Some(part) } => write!(f, "{guid}#{part}"),
        }
    }
}
