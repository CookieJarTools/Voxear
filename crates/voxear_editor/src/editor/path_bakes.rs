//! Baking the open level's sound paths, from the palette's Bake Sound
//! Paths or a level's Inspector: Steam Audio stands probes on the level's
//! floors and works out the paths between them, round the walls its still
//! colliders make, on a thread of its own, as it takes seconds to minutes.
//! The status bar shows how far it has got. Once done, the paths are
//! written beside the level as its `.paths` file (see
//! `voxear_engine::acoustics`), which the game reads as it plays.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

use voxear_engine::acoustics::{self, BakedPaths, bake_paths};

use super::Editor;

/// A bake under way: the level's path, how far it has got, as the bits of
/// a fraction, and what it makes.
pub(super) struct PathBake {
    level: String,
    progress: Arc<AtomicU32>,
    done: Receiver<Result<BakedPaths, String>>,
}

impl Editor {
    /// Starts baking the open level's sound paths.
    pub(super) fn bake_paths(&mut self) {
        if self.path_bake.is_some() {
            return self.notify("The paths are baking already");
        }
        if self.is_playing() {
            return self.notify("Stop playing to bake the level's sound paths");
        }
        let Some(level) = self.game.levels.current().map(str::to_string) else {
            return self.notify("Open a level to bake its sound paths");
        };
        let walls = acoustics::walls(&self.game.engine.world);
        let progress = Arc::new(AtomicU32::new(0));
        let (given, done) = mpsc::channel();
        let told = progress.clone();
        let started = thread::Builder::new().name("voxear-paths".into()).spawn(move || {
            let baked = bake_paths(&walls, &mut |fraction| told.store(fraction.to_bits(), Ordering::Relaxed));
            let _ = given.send(baked);
        });
        match started {
            Ok(_) => self.path_bake = Some(PathBake { level, progress, done }),
            Err(error) => self.notify(&format!("Could not start baking: {error}")),
        }
    }

    /// Bakes the paths of the level at `path`, if it is the open one.
    pub(super) fn bake_level_paths(&mut self, path: &str) {
        if self.game.levels.current() == Some(path) {
            return self.bake_paths();
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        self.notify(&format!("Open {} to bake its sound paths", name.trim_end_matches(".level")));
    }

    /// What the status bar says of a bake under way.
    pub(super) fn path_bake_status(&self) -> Option<String> {
        let bake = self.path_bake.as_ref()?;
        let fraction = f32::from_bits(bake.progress.load(Ordering::Relaxed));
        Some(format!("Baking sound paths · {:.0}%", fraction * 100.0))
    }

    /// Writes the paths beside their level once the bake is done.
    pub(super) fn finish_path_bake(&mut self) {
        let Some(bake) = &self.path_bake else { return };
        let Ok(baked) = bake.done.try_recv() else { return };
        let level = self.path_bake.take().map(|bake| bake.level).unwrap_or_default();
        let name = level.rsplit('/').next().unwrap_or(&level).trim_end_matches(".level").to_string();
        let baked = match baked {
            Ok(baked) => baked,
            Err(error) => {
                tracing::error!("could not bake {name}'s sound paths: {error}");
                return self.notify(&format!("Could not bake {name}'s sound paths: {error}"));
            }
        };
        let path = acoustics::paths_file(&level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&path), &baked.bytes) {
            tracing::error!("could not write {path}: {error}");
            return self.notify(&format!("Could not write {path}: {error}"));
        }
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        self.game.audio.forget_paths();
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
        self.refresh_other_projects();
        let shown = self.inspector.shown_asset().filter(|item| item.path == level).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        tracing::info!("baked {name}'s sound paths between {} probes into {path}", baked.probes);
        self.notify(&format!("Baked {name}'s sound paths: {} probes", baked.probes));
    }
}
