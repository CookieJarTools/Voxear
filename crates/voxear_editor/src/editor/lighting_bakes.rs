//! Baking the open level's lighting, from the palette's Bake Lighting, the
//! Lighting page or a tool: the renderer traces the light falling on every
//! Obj with a Lightmap and on every Light Probe Group's probes a little
//! each frame (see `voxear_render::lightmapper`), then the editor writes
//! what it made beside the level as its `.lightmaps` file (see
//! `voxear_engine::light_bakes`) and puts it on the level, which is lit by it
//! from then on, as the game is when the level opens. A bake can be given
//! up part way.

use voxear_engine::light_bakes::{self, BakedLevel};
use voxear_render::{BakeStage, Renderer};

use super::Editor;

/// A bake under way: the level's path.
pub(super) struct LightingBake {
    level: String,
}

/// What a bake made: how many parts and probes, its lightmaps, and the file
/// it wrote with its size, and how long it took.
#[derive(Clone, Debug)]
pub(super) struct LightingBaked {
    pub parts: usize,
    pub lightmaps: usize,
    pub probes: usize,
    pub texels: u64,
    pub file: String,
    pub bytes: u64,
    pub seconds: f32,
}

impl Editor {
    /// Starts baking the open level's lighting.
    pub(super) fn bake_lighting(&mut self, renderer: &mut Renderer) {
        if self.lighting_bake.is_some() {
            return self.notify("The lighting is baking already");
        }
        if self.is_playing() {
            return self.notify("Stop playing to bake the level's lighting");
        }
        let Some(level) = self.game.levels.current().map(str::to_string) else {
            return self.notify("Open a level to bake its lighting");
        };
        self.lighting_bake_end = None;
        match renderer.start_light_bake(&self.game.engine.world) {
            Ok((0, _, 0)) => {
                renderer.cancel_light_bake();
                self.lighting_bake_end = Some(Err("the level has nothing to bake".into()));
                self.notify(
                    "The level has nothing to bake: give its still Objs a Lightmap, or add a Light Probe Group",
                );
            }
            Ok((parts, texels, probes)) => {
                tracing::info!("baking {level}'s lighting: {parts} parts, {texels} texels, {probes} probes");
                self.lighting_bake = Some(LightingBake { level });
            }
            Err(error) => {
                self.lighting_bake_end = Some(Err(format!("{error:#}")));
                self.notify(&format!("Could not bake the lighting: {error:#}"));
            }
        }
    }

    /// Gives up the bake under way.
    pub(super) fn cancel_lighting_bake(&mut self, renderer: &mut Renderer) {
        if self.lighting_bake.take().is_none() {
            return self.notify("No lighting bake is under way");
        }
        renderer.cancel_light_bake();
        self.lighting_bake_end = Some(Err("the bake was given up".into()));
        self.notify("Gave up baking the lighting");
    }

    /// Whether a bake is under way.
    pub(super) fn baking_lighting(&self) -> bool {
        self.lighting_bake.is_some()
    }

    /// How the last bake ended, once it has.
    pub(super) fn lighting_bake_end(&self) -> Option<&Result<LightingBaked, String>> {
        self.lighting_bake_end.as_ref()
    }

    /// What the status bar says of a bake under way.
    pub(super) fn lighting_bake_status(&self, renderer: &Renderer) -> Option<String> {
        self.lighting_bake.as_ref()?;
        let progress = renderer.light_bake_progress();
        let fraction = progress.map_or(1.0, |progress| progress.fraction);
        let doing = match progress.map(|progress| progress.stage) {
            Some(BakeStage::Tracing) | None => "Baking lighting",
            Some(BakeStage::Finishing) => "Baking lighting · denoising",
        };
        Some(format!("{doing} · {:.0}%", fraction.min(1.0) * 100.0))
    }

    /// How far the bake under way has gone, from 0 to 1.
    pub(super) fn lighting_bake_fraction(&self, renderer: &Renderer) -> Option<f32> {
        self.lighting_bake.as_ref()?;
        Some(renderer.light_bake_progress().map_or(1.0, |progress| progress.fraction))
    }

    /// Once the bake is done, writes it beside its level and puts it on the
    /// level. Scripts read how it goes from the world.
    pub(super) fn update_lighting_bake(&mut self, renderer: &mut Renderer) {
        let progress = self.lighting_bake_fraction(renderer);
        let state = voxear_render::LightBakeState { baking: progress.is_some(), progress: progress.unwrap_or(0.0) };
        let world = &mut self.game.engine.world;
        if world.resource::<voxear_render::LightBakeState>().copied() != Some(state) {
            world.insert_resource(state);
        }
        let Some(bake) = &self.lighting_bake else { return };
        let level = bake.level.clone();
        let Some(made) = renderer.take_light_bake() else { return };
        self.lighting_bake = None;
        let name = level.rsplit('/').next().unwrap_or(&level).trim_end_matches(".level").to_string();
        let made = match made {
            Ok(made) => made,
            Err(error) => return self.end_lighting_bake(&name, Err(format!("{error:#}"))),
        };
        if self.game.levels.current() != Some(level.as_str()) {
            return self.end_lighting_bake(&name, Err("another level was opened meanwhile".into()));
        }
        let baked = BakedLevel::of(&self.game.levels, &made);
        let bytes = light_bakes::write(&baked);
        let file = light_bakes::lightmaps_file(&level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&file), &bytes) {
            return self.end_lighting_bake(&name, Err(format!("could not write {file}: {error}")));
        }
        let (parts, lightmaps, probes) = (baked.parts.len(), baked.atlases.len(), baked.probes.positions.len());
        // As the file holds it, so the level is lit now just as it will be when it opens.
        let kept = light_bakes::read(&bytes).unwrap_or(baked);
        light_bakes::put_on(&self.game.levels, &mut self.game.engine.world, kept);
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&file));
        self.refresh_other_projects();
        let baked = LightingBaked {
            parts,
            lightmaps,
            probes,
            texels: made.texels,
            file,
            bytes: bytes.len() as u64,
            seconds: made.seconds,
        };
        self.end_lighting_bake(&name, Ok(baked));
    }

    fn end_lighting_bake(&mut self, name: &str, end: Result<LightingBaked, String>) {
        match &end {
            Ok(baked) => {
                let size = match baked.bytes {
                    0..1_048_576 => format!("{:.0} KB", baked.bytes as f64 / 1024.0),
                    _ => format!("{:.1} MB", baked.bytes as f64 / 1_048_576.0),
                };
                let lightmaps = if baked.lightmaps == 1 {
                    "1 lightmap".to_string()
                } else {
                    format!("{} lightmaps", baked.lightmaps)
                };
                tracing::info!(
                    "baked {name}'s lighting into {} in {:.1} s: {} parts, {lightmaps}, {} texels, {} probes, {size}",
                    baked.file,
                    baked.seconds,
                    baked.parts,
                    baked.texels,
                    baked.probes
                );
                self.notify(&format!(
                    "Baked {name}'s lighting: {lightmaps}, {} probes, {size}, {:.1} s",
                    baked.probes, baked.seconds
                ));
            }
            Err(error) => {
                tracing::error!("could not bake {name}'s lighting: {error}");
                self.notify(&format!("Could not bake {name}'s lighting: {error}"));
            }
        }
        self.lighting_bake_end = Some(end);
    }
}
