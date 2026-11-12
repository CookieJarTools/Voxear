//! Particle effects in the Project panel: New Particle Effect offers the
//! starters, and makes a copy of the one picked as a `.particles` asset of
//! the project's own. And in the Scene view: an effect that plays once
//! plays over again while its Obj is selected, so it can be watched.

use voxear_render::{Renderer, Starter};
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    object_picker::{Choice, Chosen, PickFor},
    project_view::ProjectEvent,
};

/// How long after a selected effect that plays once is over before it plays again, in seconds.
const REPLAY_PAUSE: f32 = 0.6;

impl Editor {
    /// While editing, plays the selected Obj's Particle System over again once it is over, when it is one that
    /// ends: an explosion or a muzzle flash is seen again and again while it is being worked on.
    pub(super) fn replay_selected_effect(&mut self) {
        if self.is_playing() {
            return;
        }
        let Some(entity) = self.selected() else { return };
        let now = self.game.engine.time().elapsed as f32;
        let world = &mut self.game.engine.world;
        let Some(system) = world.get::<voxear_render::ParticleSystem>(entity) else { return };
        let library = world.resource::<voxear_render::ParticleEffects>();
        let Some(ends) = voxear_render::particles_end_after(library, system) else { return };
        let started = world.get::<voxear_render::ParticlePlayback>(entity).map_or(0.0, |playback| playback.started);
        if now - started > ends + REPLAY_PAUSE {
            world.insert(entity, voxear_render::ParticlePlayback { started: now, stopped: None });
        }
    }

    /// Opens the object picker under `anchor` for the starter a new
    /// particle effect in `folder` is a copy of.
    pub(super) fn browse_effect_starters(&mut self, folder: String, anchor: NodeId) {
        let choices = Starter::ALL
            .into_iter()
            .map(|starter| Choice {
                label: starter.name().to_string(),
                detail: emitters(starter),
                icon: "particles",
                pick: Chosen::Effect(starter),
                preview: None,
                folder: String::new(),
            })
            .collect();
        self.picker.open(&mut self.ui, (PickFor::NewEffect { folder }, anchor), "New Particle Effect", choices, "");
    }

    /// Makes a particle effect in `folder` as a copy of `starter`, and
    /// selects it, ready to rename.
    pub(super) fn make_effect(&mut self, folder: &str, starter: Starter, renderer: &mut Renderer) {
        let made = crate::project_files::place(self.game.content.db().mounts(), folder)
            .and_then(|at| crate::project_files::create_effect(&at.root, &at.path, starter).map(|made| at.back(&made)));
        match made {
            Ok(made) => self.project_event(ProjectEvent::Changed { select: Some(made), rename: true }, renderer),
            Err(error) => self.notify(&format!("{error:#}")),
        }
    }
}

/// A starter's emitters by name, as the picker says beside it.
fn emitters(starter: Starter) -> String {
    let names: Vec<&str> = starter.effect().emitters.iter().map(|emitter| emitter.name.as_str()).collect();
    names.join(", ")
}
