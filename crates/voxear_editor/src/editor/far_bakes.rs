//! Baking a streamed Block World's far view, from the palette's Bake Far
//! View or `vox editor call blocks.bake_far`: every tile of its coarser
//! levels over the whole world (a world without end, round where the Scene
//! view looks from), built on as streaming builds them, kept in the
//! project's `.voxear/FarView` on threads of their own (see
//! `voxear_engine::blocks::start_far_bake`). Streaming reads them as soon as
//! they are kept, the coarsest first, so the horizon is there at once. The
//! status bar shows how far it has got.

use std::sync::atomic::Ordering;

use serde_json::{Value as Json, json};
use voxear_ecs::Entity;

use super::Editor;

/// The far view's first level a bake starts from when none is given: the
/// rings from 2.5 km out with metre blocks at Medium, which are the slowest
/// to make.
pub(super) const FAR_BAKE_FROM: u32 = 3;

impl Editor {
    /// The streamed Block World a bake is for: `asked`, or the selected Obj
    /// where it is one, or the first in the open level.
    fn far_bake_world(&self, asked: Option<Entity>) -> Option<Entity> {
        let world = &self.game.engine.world;
        let streamed = |entity: Entity| {
            world.get::<voxear_engine::blocks::BlockWorldAsset>(entity).is_some_and(|asset| {
                asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some())
            })
        };
        if let Some(asked) = asked {
            return streamed(asked).then_some(asked);
        }
        self.selected().filter(|&entity| streamed(entity)).or_else(|| {
            let mut all: Vec<Entity> = world
                .query_ref::<(Entity, &voxear_engine::blocks::BlockWorldAsset)>()
                .into_iter()
                .map(|(entity, _)| entity)
                .filter(|&entity| streamed(entity))
                .collect();
            all.sort_by_key(|entity| entity.to_bits());
            all.first().copied()
        })
    }

    /// Starts baking a streamed Block World's far view: `world`, or the one
    /// [`Self::far_bake_world`] finds; from level `from` out, a world
    /// without end `reach` metres round. Says why where it cannot.
    pub(super) fn bake_far_view(&mut self, world: Option<Entity>, from: u32, reach: Option<f32>) -> Result<Entity, String> {
        if self.far_bake.is_some() {
            return Err("The far view is baking already".into());
        }
        let entity = self.far_bake_world(world).ok_or("Select a streamed Block World to bake its far view")?;
        // Most of the machine, a couple of threads left for the editor and the streaming it does.
        let threads = std::thread::available_parallelism().map_or(4, usize::from).saturating_sub(2).max(1);
        let game = &mut self.game;
        let bake =
            voxear_engine::blocks::start_far_bake(&mut game.engine.world, &mut game.content, entity, from, reach, threads)
                .map_err(|error| format!("Could not bake the far view: {error:#}"))?;
        self.far_bake = Some(bake);
        Ok(entity)
    }

    /// Bakes the selected or first streamed Block World's far view, from
    /// the palette.
    pub(super) fn bake_far_view_command(&mut self) {
        match self.bake_far_view(None, FAR_BAKE_FROM, None) {
            Ok(_) => self.notify("Baking the far view"),
            Err(error) => self.notify(&error),
        }
    }

    /// Stops a bake under way after the tiles it is making; what it kept
    /// stays, and a bake started again goes on from there.
    pub(super) fn stop_far_bake(&mut self) {
        match &self.far_bake {
            Some(bake) => {
                bake.baking.stop.store(true, Ordering::Relaxed);
                self.notify("Stopping the far view's bake");
            }
            None => self.notify("The far view is not baking"),
        }
    }

    /// How far a bake has got, as JSON: none baking, or its world, tiles
    /// done of all and the level under way.
    pub(super) fn far_bake_json(&self) -> Json {
        match &self.far_bake {
            Some(bake) => json!({
                "baking": true,
                "world": crate::link::obj_ref(&self.game, bake.world),
                "done": bake.baking.done.load(Ordering::Relaxed),
                "total": bake.baking.total.load(Ordering::Relaxed),
                "level": bake.baking.level.load(Ordering::Relaxed),
            }),
            None => json!({ "baking": false, "last": self.far_bake_end }),
        }
    }

    /// What the status bar says of a bake under way.
    pub(super) fn far_bake_status(&self) -> Option<String> {
        let bake = self.far_bake.as_ref()?;
        let (done, total) = (bake.baking.done.load(Ordering::Relaxed), bake.baking.total.load(Ordering::Relaxed));
        let level = bake.baking.level.load(Ordering::Relaxed);
        let share = if total == 0 { 0.0 } else { done as f32 / total as f32 * 100.0 };
        Some(format!("Baking far view · level {level} · {done} of {total} tiles · {share:.0}%"))
    }

    /// Notes the end of a bake once it is done.
    pub(super) fn finish_far_bake(&mut self) {
        let Some(bake) = &self.far_bake else { return };
        let Ok(baked) = bake.done.try_recv() else { return };
        let stopped = bake.baking.stop.load(Ordering::Relaxed);
        self.far_bake = None;
        match baked {
            Ok(made) => {
                let end = if stopped { "stopped" } else { "done" };
                tracing::info!("far view baked ({end}): {made} tiles made");
                self.far_bake_end = Some(json!({ "ended": end, "made": made }));
                self.notify(&format!("Far view baked ({end}): {made} tiles made"));
            }
            Err(error) => {
                tracing::error!("could not bake the far view: {error}");
                self.far_bake_end = Some(json!({ "ended": "failed", "error": error }));
                self.notify(&format!("Could not bake the far view: {error}"));
            }
        }
    }
}
