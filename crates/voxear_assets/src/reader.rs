use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Reads asset files by project path, such as `Models/Ship.gltf`, wherever
/// the content lives.
pub trait AssetReader: Send {
    fn read(&mut self, path: &str) -> Result<Vec<u8>>;

    fn read_text(&mut self, path: &str) -> Result<String> {
        String::from_utf8(self.read(path)?).with_context(|| format!("{path} is not UTF-8 text"))
    }
}

/// Reads straight from a project folder, so pressing play never needs a
/// content build.
pub struct FolderReader {
    root: PathBuf,
}

impl FolderReader {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
}

impl AssetReader for FolderReader {
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let file = self.root.join(path);
        std::fs::read(&file).with_context(|| format!("reading {}", file.display()))
    }
}
