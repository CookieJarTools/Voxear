use std::{fmt, str::FromStr};

use anyhow::{Context, Result};

/// A 128-bit asset identifier, written as 32 lowercase hex digits like
/// Unity's GUIDs.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetGuid(u128);

impl AssetGuid {
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4().as_u128())
    }
}

impl fmt::Display for AssetGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl fmt::Debug for AssetGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssetGuid({self})")
    }
}

impl FromStr for AssetGuid {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let valid = text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit());
        anyhow::ensure!(valid, "`{text}` is not a GUID (32 hex digits)");
        u128::from_str_radix(text, 16).map(Self).context("parsing a GUID")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let guid = AssetGuid::new_random();
        assert_eq!(guid.to_string().parse::<AssetGuid>().unwrap(), guid);
        assert_ne!(guid, AssetGuid::new_random());
        assert!("not-a-guid".parse::<AssetGuid>().is_err());
    }
}
