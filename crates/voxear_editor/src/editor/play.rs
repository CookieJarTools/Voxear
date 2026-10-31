//! Play mode, as in Unity: Play copies the open level, edits included, into
//! a fresh engine whose behaviours run, and Stop throws that away and brings
//! back the level exactly as it was edited. Pause holds the game still, and
//! Step moves a paused game on by one frame.
//!
//! The project's scripts are built and started the first time Play is
//! pressed, the build off the main thread. .NET starts only once per
//! process, so the scripts are kept from run to run: until hot reload comes,
//! a change to them needs the editor restarted.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::Result;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Levels, Project, scripts};
use voxear_render::{Camera, Renderer};
use voxear_scene::{Name, Transform};
use voxear_scripting::ScriptRuntime;

use super::Editor;
use crate::{
    hierarchy::EditorOnly,
    history::{Edit, ObjState},
    inspector::Field,
    props::{self, Value},
};

/// A value kept from play, as the played Obj had it when play stopped: its
/// whole state, for its name or Transform, or one component's value.
enum Kept {
    State(ObjState),
    Value(Value),
}

/// How far Step moves a paused game.
const STEP: f32 = 1.0 / 60.0;

pub(super) enum Mode {
    Editing,
    /// The scripts are building for the first run, which starts once they are.
    Building(Receiver<Result<()>>),
    Playing(Box<Run>),
}

/// What is put aside while the game runs, and how it is running.
pub(super) struct Run {
    edit: Engine,
    levels: Levels,
    /// The edit world's Scene camera.
    camera: Entity,
    pub paused: bool,
    /// Step was pressed: move the paused game on one frame.
    step: bool,
}

pub(super) struct Play {
    pub mode: Mode,
    /// .NET with the scripts loaded, kept between runs.
    scripts: Option<ScriptRuntime>,
    /// The scripts are built and started, or the project has none.
    ready: bool,
    /// Why the last Play failed, for the status bar.
    pub error: Option<String>,
}

impl Play {
    pub fn new() -> Self {
        Self { mode: Mode::Editing, scripts: None, ready: false, error: None }
    }

    /// What the status bar says the editor is doing.
    pub fn status(&self) -> &str {
        match (&self.mode, &self.error) {
            (Mode::Building(_), _) => "Building scripts…",
            (Mode::Playing(run), _) if run.paused => "Paused",
            (Mode::Playing(_), _) => "Playing",
            (Mode::Editing, Some(error)) => error,
            (Mode::Editing, None) => "Ready",
        }
    }
}

impl Editor {
    pub fn is_playing(&self) -> bool {
        matches!(self.play.mode, Mode::Playing(_))
    }

    pub fn is_paused(&self) -> bool {
        matches!(&self.play.mode, Mode::Playing(run) if run.paused)
    }

    /// Why the last Play failed, if it did.
    pub fn play_error(&self) -> Option<&str> {
        self.play.error.as_deref()
    }

    /// Starts playing, as the Play button does. The first time, the scripts
    /// build first, and the game starts on a later frame once they have.
    pub fn play(&mut self, renderer: &mut Renderer) {
        if !matches!(self.play.mode, Mode::Editing) {
            return;
        }
        self.play.error = None;
        if self.play.ready {
            self.start_run(renderer);
            return;
        }
        let root = self.game.project.root.clone();
        let (done, building) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(Project::open(&root).and_then(|project| scripts::build(&project)));
        });
        self.play.mode = Mode::Building(building);
    }

    /// Stops playing, as the Play button does while the game runs, and
    /// brings back the level as it was edited.
    pub fn stop(&mut self, renderer: &mut Renderer) {
        let Mode::Playing(run) = std::mem::replace(&mut self.play.mode, Mode::Editing) else { return };
        let Run { edit, levels, camera, .. } = *run;
        let selected = self.inspector.shown().and_then(|entity| self.game.levels.counterpart(entity, &levels));
        // The values to keep, read before the played world goes.
        let kept: Vec<(Entity, Field, Kept)> = std::mem::take(&mut self.kept)
            .into_iter()
            .filter_map(|(played, field)| {
                let value = match &field {
                    Field::Prop(prop) => Kept::Value(props::get(&self.game, &[], played, prop)?),
                    _ => Kept::State(ObjState::of(&self.game.engine.world, played)?),
                };
                Some((self.game.levels.counterpart(played, &levels)?, field, value))
            })
            .collect();
        let view = self.game.engine.world.get::<Transform>(self.camera).copied();
        let mut played = std::mem::replace(&mut self.game.engine, edit);
        self.game.levels = levels;
        self.play.scripts = played.shutdown();
        drop(played);
        if let (Some(view), Some(transform)) = (view, self.game.engine.world.get_mut::<Transform>(camera)) {
            *transform = view;
        }
        self.camera = camera;
        self.game.release_unused(Some(renderer));
        self.keep_from_play(kept, renderer);
        self.after_swap(selected, "scene");
        tracing::info!("stopped playing; the level is back as it was edited");
    }

    /// The edited Obj a played one was copied from, while playing: what a
    /// value kept after play goes back into.
    pub(super) fn edited_counterpart(&self, played: Entity) -> Option<Entity> {
        let Mode::Playing(run) = &self.play.mode else { return None };
        self.game.levels.counterpart(played, &run.levels)
    }

    /// Brings values kept from play into the edited level, as one step to
    /// undo.
    fn keep_from_play(&mut self, kept: Vec<(Entity, Field, Kept)>, renderer: &mut Renderer) {
        let mut edits = Vec::new();
        for (entity, field, played) in kept {
            let Some(before) = ObjState::of(&self.game.engine.world, entity) else { continue };
            match (&field, played) {
                (Field::Prop(prop), Kept::Value(value)) => {
                    props::set(&mut self.game, Some(renderer), &[], entity, prop, value);
                }
                (_, Kept::State(played)) => {
                    let mut after = before.clone();
                    field.copy(&played, &mut after);
                    after.apply(&mut self.game.engine.world, entity);
                }
                (_, Kept::Value(_)) => {}
            }
            let Some(after) = ObjState::of(&self.game.engine.world, entity) else { continue };
            if before != after {
                edits.push(Edit::Changed { entity, before, after });
            }
        }
        if edits.is_empty() {
            return;
        }
        let count = edits.len();
        self.history.record_step(Edit::Group(edits));
        self.unsaved = true;
        self.notify(&format!("Kept {count} {} from play", if count == 1 { "value" } else { "values" }));
    }

    /// Pauses or resumes the running game.
    pub fn set_paused(&mut self, paused: bool) {
        if let Mode::Playing(run) = &mut self.play.mode {
            run.paused = paused;
        }
    }

    /// Moves the running game on one frame and leaves it paused.
    pub fn step(&mut self) {
        if let Mode::Playing(run) = &mut self.play.mode {
            run.paused = true;
            run.step = true;
        }
    }

    /// The top bar's Play, Pause and Step, and a finished script build.
    pub(super) fn update_play(&mut self, renderer: &mut Renderer) {
        let [play, pause, step] = self.nodes.play_buttons;
        if self.ui.clicked(play) {
            if matches!(self.play.mode, Mode::Editing) {
                self.play(renderer);
            } else if self.is_playing() {
                self.stop(renderer);
            } else {
                // Pressed again while the scripts build: no run after all.
                self.play.mode = Mode::Editing;
            }
        }
        if self.ui.clicked(pause) {
            let paused = !self.is_paused();
            self.set_paused(paused);
        }
        if self.ui.clicked(step) {
            self.step();
        }
        if let Mode::Building(building) = &self.play.mode {
            match building.try_recv() {
                Ok(Ok(())) => self.scripts_built(renderer),
                Ok(Err(error)) => self.fail("The scripts failed to build", error),
                Err(TryRecvError::Disconnected) => {
                    self.fail("The scripts failed to build", anyhow::anyhow!("the build stopped"))
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let (playing, building, paused) =
            (self.is_playing(), matches!(self.play.mode, Mode::Building(_)), self.is_paused());
        self.ui.set_checked(play, playing || building);
        self.ui.set_checked(pause, paused);
    }

    /// How far the game moves this frame: not at all while paused, unless
    /// stepping.
    pub(super) fn game_delta(&mut self, delta: f32) -> Option<f32> {
        match &mut self.play.mode {
            Mode::Playing(run) if run.paused => std::mem::take(&mut run.step).then_some(STEP),
            _ => Some(delta),
        }
    }

    fn scripts_built(&mut self, renderer: &mut Renderer) {
        let started = Project::open(&self.game.project.root).and_then(|project| scripts::start(&project));
        match started {
            Ok(scripts) => {
                self.play.scripts = scripts;
                self.play.ready = true;
                self.play.mode = Mode::Editing;
                self.start_run(renderer);
            }
            Err(error) => self.fail("The scripts could not start", error),
        }
    }

    /// Copies the level into a new engine with the scripts and swaps it in.
    fn start_run(&mut self, renderer: &mut Renderer) {
        let mut engine = Engine::new();
        if let Some(scripts) = self.play.scripts.take() {
            engine.set_scripts(scripts);
        }
        let copied = self.game.levels.play_copy(&self.game.engine, &mut engine, &mut self.game.content, Some(renderer));
        let levels = match copied {
            Ok(levels) => levels,
            Err(error) => {
                self.play.scripts = engine.shutdown();
                return self.fail("The level could not start", error);
            }
        };
        let view = self.game.engine.world.get::<Transform>(self.camera).copied().unwrap_or(Transform::IDENTITY);
        let lens = self.game.engine.world.get::<Camera>(self.camera).copied().unwrap_or_default();
        let camera = engine.world.spawn((Name("Scene Camera".into()), view, lens, EditorOnly));
        let selected = self.inspector.shown().and_then(|entity| self.game.levels.counterpart(entity, &levels));
        let edit = std::mem::replace(&mut self.game.engine, engine);
        let levels = std::mem::replace(&mut self.game.levels, levels);
        self.kept.clear();
        let run = Run { edit, levels, camera: self.camera, paused: false, step: false };
        self.play.mode = Mode::Playing(Box::new(run));
        self.camera = camera;
        self.after_swap(selected, "game");
        tracing::info!("playing {}", self.game.levels.current().unwrap_or("the level"));
    }

    /// Shows the world just swapped in, keeping the selection where it has a
    /// counterpart, and brings `panel` (the Game or Scene view) to the front.
    fn after_swap(&mut self, selected: Option<Entity>, panel: &str) {
        self.gizmo_shows = None;
        self.view_camera = None;
        self.hierarchy.update(&mut self.ui, &self.game.engine.world);
        self.select(selected);
        self.ui.show_dock_panel(self.nodes.dock, panel);
    }

    fn fail(&mut self, what: &str, error: anyhow::Error) {
        tracing::error!("{what}: {error:#}");
        self.play.mode = Mode::Editing;
        self.play.error = Some(what.to_string());
    }
}
