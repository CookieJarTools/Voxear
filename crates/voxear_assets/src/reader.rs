use std::{fs::File, path::Path};

use anyhow::{Context, Result};

use crate::{
    io::{Io, ReadAt},
    mount::Mounts,
};

/// Reads asset files by project path, such as `Models/Ship.gltf`, wherever
/// the content lives.
pub trait AssetReader: Send {
    fn read(&mut self, path: &str) -> Result<Vec<u8>>;

    /// Reads several files at once. Readers overlap the reads, so asking for
    /// everything known to be needed in one call is fastest.
    fn read_many(&mut self, paths: &[&str]) -> Result<Vec<Vec<u8>>> {
        paths.iter().map(|path| self.read(path)).collect()
    }

    fn read_text(&mut self, path: &str) -> Result<String> {
        String::from_utf8(self.read(path)?).with_context(|| format!("{path} is not UTF-8 text"))
    }
}

/// Reads straight from a project folder, so pressing play never needs a
/// content build, and from the folders of the packages mounted beside it.
pub struct FolderReader {
    mounts: Mounts,
    io: Io,
}

impl FolderReader {
    pub fn new(root: &Path) -> Self {
        Self::mounted(Mounts::project(root))
    }

    /// Reads the project's files and its packages', each package's under
    /// `Packages/<name>/`.
    pub fn mounted(mounts: Mounts) -> Self {
        Self { mounts, io: Io::new() }
    }
}

impl AssetReader for FolderReader {
    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        Ok(self.read_many(&[path])?.pop().expect("one file asked for"))
    }

    fn read_many(&mut self, paths: &[&str]) -> Result<Vec<Vec<u8>>> {
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let full = self.mounts.file(path);
            let file = File::open(&full).with_context(|| format!("reading {}", full.display()))?;
            let len = file.metadata()?.len() as usize;
            files.push((file, len));
        }
        let mut buffers: Vec<Vec<u8>> = files.iter().map(|(_, len)| vec![0; *len]).collect();
        let mut reads: Vec<ReadAt> =
            files.iter().zip(&mut buffers).map(|((file, _), buffer)| ReadAt { file, offset: 0, buffer }).collect();
        self.io
            .read_all(&mut reads)
            .with_context(|| format!("reading {} from {}", paths.join(", "), self.mounts.root().display()))?;
        Ok(buffers)
    }
}
