//! Baking the open level's reverb probes, from the palette's Bake Reverb
//! Probes, a level's Inspector or a tool: every probe of every Reverb
//! Probe's box is traced on the GPU, a few a frame, with many rays, as the
//! listener is with ray-traced audio on, and how long sound rings there is
//! written beside the level as its `.reverb` file (see
//! `voxear_engine::reverb_probes`), which the game rings by when it does
//! not trace rays itself.

use voxear_engine::reverb_probes::{self, BakedReverb, ProbeGrid};

use super::Editor;

/// Frames a bake waits for the renderer before it gives up, as with no
/// view drawing.
const PATIENCE_FRAMES: u32 = 1200;

/// A bake under way: the level's path, its boxes' grids, and the frames
/// gone by.
pub(super) struct ReverbBake {
    level: String,
    grids: Vec<ProbeGrid>,
    frames: u32,
}

/// What a bake made: how many probes, how many heard nothing back, and the
/// file it wrote with its size.
#[derive(Clone, Debug)]
pub(super) struct ReverbBaked {
    pub probes: usize,
    pub inside: usize,
    pub file: String,
    pub bytes: u64,
}

impl Editor {
    /// Starts baking the open level's reverb probes.
    pub(super) fn bake_reverb(&mut self) {
        if self.reverb_bake.is_some() {
            return self.notify("The reverb probes are baking already");
        }
        if self.is_playing() {
            return self.notify("Stop playing to bake the level's reverb probes");
        }
        let Some(level) = self.game.levels.current().map(str::to_string) else {
            return self.notify("Open a level to bake its reverb probes");
        };
        let grids: Vec<ProbeGrid> =
            reverb_probes::grids(&self.game.engine.world).into_iter().map(|(_, grid)| grid).collect();
        let spots: Vec<_> = grids.iter().flat_map(ProbeGrid::spots).collect();
        if spots.is_empty() {
            self.reverb_bake_end = Some(Err("the level has no reverb probes to bake".into()));
            return self.notify("The level has no reverb probes to bake: add a Reverb Probe from the + menu");
        }
        if spots.len() > reverb_probes::MOST_PROBES {
            let error = format!("{} probes, more than a level holds; space them further apart", spots.len());
            self.notify(&format!("Could not bake the reverb probes: {error}"));
            self.reverb_bake_end = Some(Err(error));
            return;
        }
        self.game.audio.tracer.bake(spots);
        self.reverb_bake_end = None;
        self.reverb_bake = Some(ReverbBake { level, grids, frames: 0 });
    }

    /// Bakes the reverb probes of the level at `path`, if it is the open one.
    pub(super) fn bake_level_reverb(&mut self, path: &str) {
        if self.game.levels.current() == Some(path) {
            return self.bake_reverb();
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        self.notify(&format!("Open {} to bake its reverb probes", name.trim_end_matches(".level")));
    }

    /// Whether a bake is under way.
    pub(super) fn baking_reverb(&self) -> bool {
        self.reverb_bake.is_some()
    }

    /// How the last bake ended, once it has.
    pub(super) fn reverb_bake_end(&self) -> Option<&Result<ReverbBaked, String>> {
        self.reverb_bake_end.as_ref()
    }

    /// What the status bar says of a bake under way.
    pub(super) fn reverb_bake_status(&self) -> Option<String> {
        self.reverb_bake.as_ref()?;
        let (done, of) = self.game.audio.tracer.baking()?;
        Some(format!("Baking reverb probes · {done} of {of}"))
    }

    /// Once every probe has been traced, writes them beside their level.
    pub(super) fn update_reverb_bake(&mut self) {
        let Some(bake) = &mut self.reverb_bake else { return };
        bake.frames += 1;
        let Some(times) = self.game.audio.tracer.take_bake() else {
            if bake.frames > PATIENCE_FRAMES {
                let level = self.reverb_bake.take().map(|bake| bake.level).unwrap_or_default();
                self.game.audio.tracer.bake(Vec::new());
                let _ = self.game.audio.tracer.take_bake();
                self.end_reverb_bake(&level, Err("the renderer traced none of its probes".into()));
            }
            return;
        };
        let bake = self.reverb_bake.take().expect("looked at above");
        let mut times = times.into_iter();
        let boxes: Vec<_> =
            bake.grids.iter().map(|grid| (*grid, times.by_ref().take(grid.count()).collect::<Vec<_>>())).collect();
        let baked = BakedReverb { boxes };
        let inside = baked.boxes.iter().flat_map(|(_, times)| times).filter(|t| t.is_none()).count();
        let bytes = baked.write();
        let file = reverb_probes::reverb_file(&bake.level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&file), &bytes) {
            return self.end_reverb_bake(&bake.level, Err(format!("could not write {file}: {error}")));
        }
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        // The game reads it again as it next plays.
        self.game.audio.forget_paths();
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&file));
        self.refresh_other_projects();
        let shown = self.inspector.shown_asset().filter(|item| item.path == bake.level).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        let baked = ReverbBaked { probes: baked.probes(), inside, file, bytes: bytes.len() as u64 };
        self.end_reverb_bake(&bake.level, Ok(baked));
    }

    fn end_reverb_bake(&mut self, level: &str, end: Result<ReverbBaked, String>) {
        let name = level.rsplit('/').next().unwrap_or(level).trim_end_matches(".level").to_string();
        match &end {
            Ok(baked) => {
                let inside = match baked.inside {
                    0 => String::new(),
                    n => format!(", {n} inside walls"),
                };
                tracing::info!(
                    "baked {name}'s reverb probes into {}: {} probes{inside}, {} bytes",
                    baked.file,
                    baked.probes,
                    baked.bytes
                );
                self.notify(&format!("Baked {name}'s reverb probes: {} probes{inside}", baked.probes));
            }
            Err(error) => {
                tracing::error!("could not bake {name}'s reverb probes: {error}");
                self.notify(&format!("Could not bake {name}'s reverb probes: {error}"));
            }
        }
        self.reverb_bake_end = Some(end);
    }
}
