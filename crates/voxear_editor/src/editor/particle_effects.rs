//! Particle effects in the Project panel: New Particle Effect offers the
//! starters, and makes a copy of the one picked as a `.particles` asset of
//! the project's own. And in the Scene view: the playback strip of the
//! Particle Systems being worked on, the selected Obj's or all of an effect
//! open on its own (see `prefab_mode`).

use voxear_ecs::{Entity, World};
use voxear_engine::particles::ParticlePreview;
use voxear_render::{ParticlePlayback, ParticleSystem, Renderer, Starter};
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    object_picker::{Choice, Chosen, PickFor},
    project_view::ProjectEvent,
};

/// How long after an effect that plays once is over before it plays again, in seconds.
const REPLAY_PAUSE: f32 = 0.6;
/// How long the scrubber spans for effects that never end, in seconds.
const LOOP_SPAN: f32 = 5.0;
/// The speeds the playback's speed button goes round.
const SPEEDS: [f32; 5] = [1.0, 0.5, 0.25, 0.1, 2.0];

impl Editor {
    /// The Particle Systems the Scene view's playback plays: every one in a
    /// particle effect open on its own, or else the selected Obj's. None
    /// while the game plays.
    fn previewed_particles(&self) -> Vec<Entity> {
        if self.is_playing() {
            return Vec::new();
        }
        let world = &self.game.engine.world;
        let has = |entity: Entity| world.has::<ParticleSystem>(entity);
        if self.open_prefab_path().is_some_and(|path| path.ends_with(".particles")) {
            return world.query_ref::<(Entity, &ParticleSystem)>().into_iter().map(|(entity, _)| entity).collect();
        }
        self.selected().into_iter().filter(|&entity| has(entity)).collect()
    }

    /// The playback strip over each Scene view, while there are Particle
    /// Systems to play: play and pause, Restart, the speed, and a scrubber
    /// over how long they last. An effect that plays once plays over again
    /// when it is over, so an explosion can be watched while it is worked on.
    pub(super) fn update_particle_preview(&mut self) {
        let targets = self.previewed_particles();
        let views: Vec<_> = self.all_scene_views().map(|view| view.particle_strip).collect();
        for strip in views {
            if self.ui.is_visible(strip) != !targets.is_empty() {
                self.ui.set_visible(strip, !targets.is_empty());
            }
        }
        let world = &mut self.game.engine.world;
        let mut preview = world.resource::<ParticlePreview>().copied().unwrap_or_default();
        if targets.is_empty() {
            // Nothing being watched: particles everywhere play on as they would.
            if preview != ParticlePreview::default() {
                world.insert_resource(ParticlePreview::default());
            }
            return;
        }
        let now = voxear_engine::particles::now(world) as f32;
        // How long they last between them, when they all end; and how far in the earliest started is.
        let library = world.resource::<voxear_render::ParticleEffects>();
        let ends: Vec<Option<f32>> = targets
            .iter()
            .filter_map(|&entity| world.get::<ParticleSystem>(entity))
            .map(|system| voxear_render::particles_end_after(library, system))
            .collect();
        let ends = ends.iter().copied().collect::<Option<Vec<f32>>>().map(|ends| ends.into_iter().fold(0.0, f32::max));
        let span = ends.unwrap_or(LOOP_SPAN).max(0.1);
        let started = |world: &World, entity: Entity| world.get::<ParticlePlayback>(entity).map_or(0.0, |p| p.started);
        let earliest = targets.iter().map(|&entity| started(world, entity)).fold(f32::INFINITY, f32::min);
        let mut restart = None;
        // What was pressed on any view's strip.
        let strips: Vec<_> = self
            .all_scene_views()
            .map(|view| (view.particle_play, view.particle_restart, view.particle_speed, view.particle_scrub))
            .collect();
        for (play, again, speed, scrub) in strips {
            if self.ui.clicked(play) {
                preview.paused = !preview.paused;
            }
            if self.ui.clicked(again) {
                restart = Some(now);
            }
            if self.ui.clicked(speed) {
                let at = SPEEDS.iter().position(|s| *s == preview.speed).unwrap_or(0);
                preview.speed = SPEEDS[(at + 1) % SPEEDS.len()];
            }
            // Dragged to a time: held there, as far in as it says.
            let shown = self.ui.slider_value(scrub);
            if self.ui.changed(scrub) && (shown - self.particle_scrubbed).abs() > 1e-4 {
                preview.paused = true;
                restart = Some(now - shown * span);
            }
        }
        // Over, and not held: played again after a moment.
        if restart.is_none() && !preview.paused && ends.is_some_and(|ends| now - earliest > ends + REPLAY_PAUSE) {
            restart = Some(now);
        }
        let world = &mut self.game.engine.world;
        if let Some(from) = restart {
            for &entity in &targets {
                world.insert(entity, ParticlePlayback { started: from, stopped: None });
            }
        }
        if world.resource::<ParticlePreview>() != Some(&preview) {
            world.insert_resource(preview);
        }
        // What the strips show: pause while playing, the speed, and how far in they are.
        let earliest = restart.unwrap_or(earliest);
        let elapsed = if ends.is_some() { (now - earliest).clamp(0.0, span) } else { (now - earliest).max(0.0) % span };
        let fraction = elapsed / span;
        self.particle_scrubbed = fraction;
        let (icon, speed) = (if preview.paused { "play" } else { "pause" }, speed_label(preview.speed));
        let time = format!("{elapsed:.2} s");
        let strips: Vec<_> = self
            .all_scene_views()
            .map(|view| (view.particle_play_icon, view.particle_speed, view.particle_scrub, view.particle_time))
            .collect();
        for (play_icon, speed_button, scrub, label) in strips {
            self.ui.set_icon(play_icon, icon);
            if self.ui.text(speed_button) != speed {
                self.ui.set_text(speed_button, &speed);
            }
            if self.ui.text(label) != time {
                self.ui.set_text(label, &time);
            }
            self.ui.set_slider_value(scrub, fraction);
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

/// A playback speed as its button says it.
fn speed_label(speed: f32) -> String {
    format!("{speed}×")
}

/// A starter's emitters by name, as the picker says beside it.
fn emitters(starter: Starter) -> String {
    let names: Vec<&str> = starter.effect().emitters.iter().map(|emitter| emitter.name.as_str()).collect();
    names.join(", ")
}
