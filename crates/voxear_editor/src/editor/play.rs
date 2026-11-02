//! Play mode, as in Unity: Play copies the open level, edits included, into
//! a fresh engine whose behaviours run, and Stop throws that away and brings
//! back the level exactly as it was edited. Pause holds the game still, its
//! sound too, and Step moves a paused game on by one frame, in silence.
//!
//! The project's scripts are built and started the first time Play is
//! pressed, the build off the main thread. .NET starts only once per
//! process, so the scripts are kept from run to run, and reloaded when they
//! change: while editing as soon as the editor has built them again, while
//! playing into the running game (hot reload, unless the settings say to
//! wait for play to stop), and on Play when they changed since they were
//! last built.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::Result;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Levels, Project,
    script_types::{self, ScriptTypes},
    scripts,
};
use voxear_render::Renderer;
use voxear_scene::Name;
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
    /// A `[Command]` to run once the scripts are built and started, instead
    /// of playing: its type and method.
    command: Option<(String, String)>,
    /// The scripts are being built and started for a VoxDocs guide's checks,
    /// not to play.
    start_only: bool,
}

impl Play {
    pub fn new() -> Self {
        Self { mode: Mode::Editing, scripts: None, ready: false, error: None, command: None, start_only: false }
    }

    /// The scripts edit mode runs the Inspector's code with, once they have
    /// started, and while they are not playing.
    pub fn edit_scripts(&mut self) -> Option<&mut ScriptRuntime> {
        let started = matches!(self.mode, Mode::Editing) && self.error.is_none() && self.ready;
        self.scripts.as_mut().filter(|_| started)
    }

    /// What the status bar says the editor is doing.
    /// Whether the scripts are building for play to start.
    pub fn is_building(&self) -> bool {
        matches!(self.mode, Mode::Building(_))
    }

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
        if self.prefab.is_some() {
            return self.notify("A prefab open on its own does not play: go Back to its level first");
        }
        self.play.error = None;
        if self.memory.settings.save_before_play && self.unsaved {
            self.save();
        }
        if self.play.ready && !self.scripts_out_of_date() {
            self.start_run(renderer);
            return;
        }
        self.build_scripts();
    }

    /// Whether the scripts started are behind their sources: a script
    /// changed since they were last built, or they were built since they
    /// were loaded.
    fn scripts_out_of_date(&self) -> bool {
        let loaded_behind = self.play.scripts.as_ref().is_some_and(ScriptRuntime::is_out_of_date);
        loaded_behind || script_types::is_stale(&self.game.project)
    }

    /// Brings the started scripts up to their latest build, once the editor
    /// has built them again: while editing at once, and while playing into
    /// the running game, which keeps going (see `Engine::reload_scripts`),
    /// unless the settings say to wait for play to stop. Nothing happens
    /// before the scripts first start.
    pub(super) fn reload_scripts(&mut self) {
        match self.play.mode {
            Mode::Editing => {
                if let Err(error) = self.reload_edit_scripts() {
                    tracing::error!("{error:#}");
                    self.notify("The scripts could not reload; the Console says why");
                }
            }
            Mode::Playing(_) => self.reload_played_scripts(),
            // Once built, they reload before the game starts.
            Mode::Building(_) => {}
        }
    }

    /// While editing nothing runs, so there is no behaviour to carry over.
    fn reload_edit_scripts(&mut self) -> Result<()> {
        let Some(scripts) = self.play.scripts.as_mut().filter(|scripts| scripts.is_out_of_date()) else {
            return Ok(());
        };
        scripts.reload_game_assembly(&mut self.game.engine.world, &[])?;
        Ok(())
    }

    fn reload_played_scripts(&mut self) {
        if !self.game.engine.scripts().is_some_and(ScriptRuntime::is_out_of_date) {
            return;
        }
        if !self.memory.settings.reload_while_playing {
            return self.notify("The scripts changed; they reload when play stops");
        }
        match self.game.engine.reload_scripts() {
            Ok(()) => self.notify("Reloaded the scripts into the running game"),
            Err(error) => {
                tracing::error!("{error:#}");
                self.notify("The scripts could not reload; the game runs on with the old ones");
            }
        }
    }

    /// Builds the scripts off the main thread; once built they start, and
    /// the game plays or the waiting command runs.
    fn build_scripts(&mut self) {
        let root = self.game.project.root.clone();
        let (done, building) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(Project::open(&root).and_then(|project| scripts::build(&project)));
        });
        self.play.mode = Mode::Building(building);
    }

    /// Runs a scripts' `[Command]` method, as the palette asks. While
    /// editing it runs against the edited level, and what it changes,
    /// renames, moves or destroys is one step to undo; while playing it runs
    /// against the game. The first time, the scripts build and start, and it
    /// runs once they have.
    pub(super) fn run_script_command(&mut self, type_name: &str, method: &str) {
        let ran = match &mut self.play.mode {
            Mode::Playing(_) => self.game.engine.run_command(type_name, method),
            Mode::Building(_) => return self.notify("Wait for the scripts to finish building"),
            Mode::Editing if !self.play.ready || self.play.scripts.is_none() => {
                self.play.command = Some((type_name.to_string(), method.to_string()));
                return self.build_scripts();
            }
            Mode::Editing => self.run_command_on_edits(type_name, method),
        };
        if let Err(error) = ran {
            tracing::error!("{error:#}");
            self.notify(&format!("{error}"));
        }
    }

    /// Runs a guide's check written in C# (`[DocsCheck]`): against the game
    /// while playing, or the edited level once the scripts have started,
    /// which the first check asks for, without playing. `None` until they
    /// have.
    /// Runs `run` with the scripts started for the edited level, without
    /// playing, as the Inspector's `[Button]`s and answers need them. `None`
    /// while playing, while they build and start, which this sets going the
    /// first time, or when there are none or they would not start.
    pub(super) fn with_edit_scripts<R>(&mut self, run: impl FnOnce(&mut ScriptRuntime, &mut Game) -> R) -> Option<R> {
        if !matches!(self.play.mode, Mode::Editing) || self.play.error.is_some() {
            return None;
        }
        if !self.play.ready {
            self.play.start_only = true;
            self.build_scripts();
            return None;
        }
        let scripts = self.play.scripts.as_mut()?;
        Some(run(scripts, &mut self.game))
    }

    pub(super) fn run_script_check(&mut self, type_name: &str, method: &str) -> Option<Result<Result<(), String>>> {
        match &self.play.mode {
            Mode::Playing(_) => Some(self.game.engine.run_check(type_name, method)),
            Mode::Building(_) => None,
            // Not again and again for a project with no scripts, or scripts that would not build or start.
            Mode::Editing if self.play.ready && self.play.scripts.is_none() => {
                Some(Err(anyhow::anyhow!("this project has no scripts")))
            }
            Mode::Editing if self.play.error.is_some() => {
                Some(Err(anyhow::anyhow!("the scripts did not build or start; the Console says why")))
            }
            Mode::Editing if !self.play.ready => {
                self.play.start_only = true;
                self.build_scripts();
                None
            }
            Mode::Editing => {
                let scripts = self.play.scripts.as_mut()?;
                Some(scripts.run_check(&mut self.game.engine.world, type_name, method))
            }
        }
    }

    /// Runs a command against the edited level, keeping what it changed as
    /// one step to undo: Objs it changed, and those it destroyed, which undo
    /// brings back.
    fn run_command_on_edits(&mut self, type_name: &str, method: &str) -> Result<()> {
        let world = &self.game.engine.world;
        let objs: Vec<Entity> =
            world.query_ref::<(Entity, &Name)>().without::<EditorOnly>().into_iter().map(|(e, _)| e).collect();
        let before: Vec<(Entity, ObjState)> = objs.iter().filter_map(|&e| Some((e, ObjState::of(world, e)?))).collect();
        let scripts = self.play.scripts.as_mut().expect("checked by the caller");
        let ran = scripts.run_command(&mut self.game.engine.world, type_name, method);
        let doomed = scripts.take_obj_destroys();
        let world = &self.game.engine.world;
        let mut edits: Vec<Edit> = before
            .into_iter()
            .filter_map(|(entity, before)| {
                let after = ObjState::of(world, entity)?;
                (after != before).then_some(Edit::Changed { entity, before, after })
            })
            .collect();
        let alive: Vec<Entity> = doomed.into_iter().filter(|&e| self.game.engine.world.is_alive(e)).collect();
        for entity in alive {
            let taken = crate::objs::take(&mut self.game, entity);
            edits.push(Edit::Exists { root: entity, taken: Some(taken) });
        }
        if !edits.is_empty() {
            self.history.record_step(Edit::Group(edits));
            self.unsaved = true;
            let selected = self.selected();
            self.select(selected.filter(|&e| self.game.engine.world.is_alive(e)));
        }
        ran
    }

    /// Stops playing, as the Play button does while the game runs, and
    /// brings back the level as it was edited.
    pub fn stop(&mut self, renderer: &mut Renderer) {
        let Mode::Playing(run) = std::mem::replace(&mut self.play.mode, Mode::Editing) else { return };
        // What the game drew goes with it, and its UI and sound.
        voxear_core::debug_draw::clear();
        self.game.ui = voxear_engine::game_ui::GameUi::default();
        self.game.audio.stop_game();
        let Run { edit, levels, .. } = *run;
        let selected = self.selected().and_then(|entity| self.game.levels.counterpart(entity, &levels));
        let locked = self.locked_shown(|entity| self.game.levels.counterpart(entity, &levels));
        // The values to keep, read before the played world goes.
        let kept: Vec<(Entity, Field, Kept)> = std::mem::take(&mut self.kept)
            .into_iter()
            .filter_map(|(played, field)| {
                let value = match &field {
                    Field::Prop(prop) => Kept::Value(props::get(&self.game, &ScriptTypes::default(), played, prop)?),
                    _ => Kept::State(ObjState::of(&self.game.engine.world, played)?),
                };
                Some((self.game.levels.counterpart(played, &levels)?, field, value))
            })
            .collect();
        let flown = self.scene_cameras_flown();
        let mut played = std::mem::replace(&mut self.game.engine, edit);
        self.game.levels = levels;
        self.play.scripts = played.shutdown();
        drop(played);
        self.scene_cameras_back(flown);
        self.game.release_unused(Some(renderer));
        self.keep_from_play(kept, renderer);
        self.after_swap(selected, "scene");
        self.show_locked(locked);
        // Changes left waiting for play to stop.
        self.reload_scripts();
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
                    props::set(&mut self.game, Some(renderer), &ScriptTypes::default(), entity, prop, value);
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
            self.game.audio.pause_game(paused);
        }
    }

    /// Moves the running game on one frame and leaves it paused.
    pub fn step(&mut self) {
        if let Mode::Playing(run) = &mut self.play.mode {
            run.paused = true;
            run.step = true;
            self.game.audio.pause_game(true);
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
        // Started before, and built again since.
        if self.play.scripts.is_some() {
            self.play.mode = Mode::Editing;
            if let Err(error) = self.reload_edit_scripts() {
                return self.fail("The scripts could not reload", error);
            }
            return match self.play.command.take() {
                Some((type_name, method)) => self.run_script_command(&type_name, &method),
                None if std::mem::take(&mut self.play.start_only) => {}
                None => self.start_run(renderer),
            };
        }
        let started = Project::open(&self.game.project.root).and_then(|project| scripts::start(&project));
        match started {
            Ok(scripts) => {
                self.play.scripts = scripts;
                self.play.ready = true;
                self.play.mode = Mode::Editing;
                match self.play.command.take() {
                    Some((type_name, method)) => self.run_script_command(&type_name, &method),
                    None if std::mem::take(&mut self.play.start_only) => {}
                    None => self.start_run(renderer),
                }
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
        self.scene_cameras_into(&mut engine.world);
        let selected = self.selected().and_then(|entity| self.game.levels.counterpart(entity, &levels));
        let locked = self.locked_shown(|entity| self.game.levels.counterpart(entity, &levels));
        let edit = std::mem::replace(&mut self.game.engine, engine);
        let levels = std::mem::replace(&mut self.game.levels, levels);
        self.kept.clear();
        let paused = self.memory.settings.start_paused;
        if self.memory.settings.clear_console_on_play {
            self.console.clear(&mut self.ui);
        }
        self.game.audio.pause_game(paused);
        let run = Run { edit, levels, paused, step: false };
        self.play.mode = Mode::Playing(Box::new(run));
        self.after_swap(selected, "game");
        self.show_locked(locked);
        tracing::info!("playing {}", self.game.levels.current().unwrap_or("the level"));
    }

    /// Shows the world just swapped in, keeping the selection where it has a
    /// counterpart, and brings `panel` (the Game or Scene view) to the front.
    fn after_swap(&mut self, selected: Option<Entity>, panel: &str) {
        for view in self.all_scene_views_mut() {
            view.gizmo_shows = None;
        }
        self.view_camera = None;
        self.refresh_hierarchies();
        self.select(selected);
        self.show_panel(panel);
    }

    /// Takes an Obj out of the level as edited, which is kept aside while
    /// the game plays, or out of the world when it is not playing.
    pub(super) fn despawn_in_edit_world(&mut self, entity: Entity) {
        match &mut self.play.mode {
            Mode::Playing(run) => run.edit.world.despawn(entity),
            _ => self.game.engine.world.despawn(entity),
        };
    }

    fn fail(&mut self, what: &str, error: anyhow::Error) {
        tracing::error!("{what}: {error:#}");
        self.play.mode = Mode::Editing;
        self.play.error = Some(what.to_string());
    }
}
