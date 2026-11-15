//! Baking the open level's reflection probes, from the palette's Bake
//! Reflection Probes, a level's Inspector or a tool: every probe captured
//! once is captured again, the renderer taking one a frame, then read back
//! from the GPU, written beside the level as its `.probes` file (see
//! `voxear_engine::reflection_bakes`) and put on the probes, which show it
//! from then on, as the game does when the level opens.

use std::sync::Arc;

use voxear_assets::AssetGuid;
use voxear_ecs::Entity;
use voxear_engine::{ObjPath, reflection_bakes};
use voxear_render::{ReflectionProbe, ReflectionProbeBake, Renderer};

use super::Editor;

/// Frames a probe may wait before the renderer captures it, as one that
/// has only just appeared waits for the light to settle, and a few more:
/// what the status bar counts towards.
const SETTLE_FRAMES: u32 = 16;
/// Frames past those a bake waits for a capture that has not come, as for
/// a probe switched off meanwhile.
const PATIENCE_FRAMES: u32 = 600;

/// A bake under way: the level's path, its probes, and the frames gone by.
pub(super) struct ProbeBake {
    level: String,
    probes: Vec<(Entity, AssetGuid, ObjPath)>,
    frames: u32,
}

/// What a bake made: how many probes, and the file it wrote with its size.
#[derive(Clone, Debug)]
pub(super) struct ProbesBaked {
    pub probes: usize,
    pub file: String,
    pub bytes: u64,
}

impl ProbeBake {
    /// Frames for every probe to be captured again: one a frame, after the
    /// longest one may wait.
    fn frames_needed(&self) -> u32 {
        SETTLE_FRAMES + self.probes.len() as u32 + 2
    }
}

impl Editor {
    /// Starts baking the open level's reflection probes.
    pub(super) fn bake_probes(&mut self) {
        if self.probe_bake.is_some() {
            return self.notify("The reflection probes are baking already");
        }
        if self.is_playing() {
            return self.notify("Stop playing to bake the level's reflection probes");
        }
        let Some(level) = self.game.levels.current().map(str::to_string) else {
            return self.notify("Open a level to bake its reflection probes");
        };
        let world = &mut self.game.engine.world;
        let probes = reflection_bakes::bakeable(&self.game.levels, world);
        if probes.is_empty() {
            self.probe_bake_end = Some(Err("the level has no reflection probes to bake".into()));
            return self.notify("The level has no reflection probes to bake: add one from the + menu");
        }
        // Captured afresh, not loaded from the bake before.
        for (entity, ..) in &probes {
            world.remove::<ReflectionProbeBake>(*entity);
            if let Some(probe) = world.get_mut::<ReflectionProbe>(*entity) {
                probe.capture();
            }
        }
        self.probe_bake_end = None;
        self.probe_bake = Some(ProbeBake { level, probes, frames: 0 });
    }

    /// Bakes the reflection probes of the level at `path`, if it is the open
    /// one.
    pub(super) fn bake_level_probes(&mut self, path: &str) {
        if self.game.levels.current() == Some(path) {
            return self.bake_probes();
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        self.notify(&format!("Open {} to bake its reflection probes", name.trim_end_matches(".level")));
    }

    /// Whether a bake is under way.
    pub(super) fn baking_probes(&self) -> bool {
        self.probe_bake.is_some()
    }

    /// How the last bake ended, once it has.
    pub(super) fn probe_bake_end(&self) -> Option<&Result<ProbesBaked, String>> {
        self.probe_bake_end.as_ref()
    }

    /// What the status bar says of a bake under way.
    pub(super) fn probe_bake_status(&self) -> Option<String> {
        let bake = self.probe_bake.as_ref()?;
        let fraction = bake.frames as f32 / bake.frames_needed() as f32;
        Some(format!("Baking reflection probes · {:.0}%", fraction.min(1.0) * 100.0))
    }

    /// Once every probe has been captured again, reads them back, writes
    /// them beside their level and puts them on the probes.
    pub(super) fn update_probe_bake(&mut self, renderer: &Renderer) {
        let Some(bake) = &mut self.probe_bake else { return };
        bake.frames += 1;
        let world = &self.game.engine.world;
        bake.probes.retain(|(entity, ..)| world.has::<ReflectionProbe>(*entity));
        // Each probe's capture asked for when the bake began has been taken.
        let taken = |entity: Entity| {
            let asked = world.get::<ReflectionProbe>(entity).map(|probe| probe.captures);
            asked.is_some() && renderer.reflection_probe_captured(entity) == asked
        };
        let waiting = !bake.probes.iter().all(|(entity, ..)| taken(*entity));
        if waiting && bake.frames < bake.frames_needed() + PATIENCE_FRAMES {
            return;
        }
        let read = reflection_bakes::read_back(renderer, &bake.probes);
        let bake = self.probe_bake.take().expect("looked at above");
        let name = bake.level.rsplit('/').next().unwrap_or(&bake.level).trim_end_matches(".level").to_string();
        let read = match read {
            Ok(read) if !read.is_empty() => read,
            Ok(_) => return self.end_probe_bake(&name, Err("no probe was captured".into())),
            Err(error) => return self.end_probe_bake(&name, Err(format!("{error:#}"))),
        };
        if read.len() < bake.probes.len() {
            tracing::warn!(
                "{} of {name}'s reflection probes were not captured, so are not baked",
                bake.probes.len() - read.len()
            );
        }
        let probes: Vec<reflection_bakes::BakedProbe> = read.iter().map(|(_, probe)| probe.clone()).collect();
        let bytes = reflection_bakes::write(&probes);
        let file = reflection_bakes::probes_file(&bake.level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&file), &bytes) {
            return self.end_probe_bake(&name, Err(format!("could not write {file}: {error}")));
        }
        let world = &mut self.game.engine.world;
        for (entity, probe) in read {
            world.insert(entity, ReflectionProbeBake(Arc::new(probe.pixels)));
        }
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&file));
        self.refresh_other_projects();
        let shown = self.inspector.shown_asset().filter(|item| item.path == bake.level).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        let baked = ProbesBaked { probes: probes.len(), file, bytes: bytes.len() as u64 };
        self.end_probe_bake(&name, Ok(baked));
    }

    fn end_probe_bake(&mut self, name: &str, end: Result<ProbesBaked, String>) {
        match &end {
            Ok(baked) => {
                let size = kilobytes(baked.bytes);
                let probes = if baked.probes == 1 { "1 probe".to_string() } else { format!("{} probes", baked.probes) };
                tracing::info!("baked {name}'s reflection probes into {}: {probes}, {size}", baked.file);
                self.notify(&format!("Baked {name}'s reflection probes: {probes}, {size}"));
            }
            Err(error) => {
                tracing::error!("could not bake {name}'s reflection probes: {error}");
                self.notify(&format!("Could not bake {name}'s reflection probes: {error}"));
            }
        }
        self.probe_bake_end = Some(end);
    }
}

fn kilobytes(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

impl Editor {
    /// Captures the selected Reflection Probes again when the Inspector's
    /// Capture button is clicked.
    pub(super) fn update_probe_captures(&mut self) {
        if !self.inspector.capture_clicked(&self.ui) {
            return;
        }
        for entity in self.selected_objs() {
            if let Some(probe) = self.game.engine.world.get_mut::<ReflectionProbe>(entity) {
                probe.capture();
            }
        }
    }
}
