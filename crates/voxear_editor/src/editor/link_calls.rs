//! The editor's side of its link (see `crate::link`): calls from tools
//! taken each frame, run as the editor's own panels would run them, and
//! answered. A change is a step to undo, as the person's own edits are, and
//! says so in the Console. Calls that take frames, as a capture or Play
//! starting, are answered once done.
//!
//! The open scenes' files and `voxear.toml` are watched here too, so what a
//! tool writes to them shows in the editor.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::Receiver,
    time::{Duration, Instant, SystemTime},
};

use serde_json::{Value as Json, json};
use voxear_assets::AssetKind;
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::local_link::{Answer, ErrorKind, LinkError, LinkServer, Request};
use voxear_render::Renderer;
use voxear_scene::{EditorOnly, Name, Parent};

use super::Editor;
use crate::{
    fields::Pick,
    history::{Edit, ObjState},
    link::{self, Setting, bad, need, need_text, obj_ref, opt_bool, opt_text, opt_vector, refuse},
    objs::{self, Destination, NEW_OBJS},
    palette,
    project_files::{self, NewFile},
    project_view::ProjectEvent,
    props::{self, Part, Value},
    settings::{self, SettingKind, SettingValue},
};

/// How often the open scenes' files and `voxear.toml` are looked at.
const WATCH: Duration = Duration::from_millis(500);
/// Frames a view is given to show before it is captured.
const SETTLE: u32 = 3;
/// How long Play may take to start, building the scripts included.
const PLAY_PATIENCE: Duration = Duration::from_secs(600);
/// How long a capture may take to come back.
const CAPTURE_PATIENCE: Duration = Duration::from_secs(20);
/// The most lines `console.read` gives at once, unless asked for fewer.
const LINES: usize = 200;
/// The most of a text file `asset.get` gives back.
const TEXT_LIMIT: usize = 1 << 20;

/// What the editor keeps for its link.
#[derive(Default)]
pub(super) struct Link {
    server: Option<LinkServer>,
    /// Calls made in this program, as `vox editor call --headless` makes them.
    local: Vec<Request>,
    waiting: Vec<(Request, Waiting)>,
    connections: usize,
    headless: bool,
    quit: bool,
    watched: Option<Instant>,
    /// The watched files and when each last changed, as last looked at.
    times: HashMap<PathBuf, Option<SystemTime>>,
    /// Scenes changed on disk under edits not saved, said once each.
    told: Vec<PathBuf>,
    /// The scripts' last build failed with this, for `scripts.build`.
    pub(super) scripts_failed: Option<String>,
}

/// A call that answers once frames have gone by.
enum Waiting {
    Frames(u32),
    Play(Instant),
    /// A view to copy: its node, or the whole window, where to write it,
    /// frames left to settle, and when it was asked for, once it was.
    Capture {
        view: Option<voxear_ui::NodeId>,
        path: PathBuf,
        settle: u32,
        asked: Option<Instant>,
    },
    /// The scripts building: the Console's count of lines when it began,
    /// and whether the build has been seen under way.
    Scripts {
        from: u64,
        began: bool,
    },
}

/// A call done, or one that answers later.
enum Outcome {
    Done(Json),
    Later(Waiting),
}

type Called = Result<Outcome, LinkError>;

fn done(value: Json) -> Called {
    Ok(Outcome::Done(value))
}

impl Editor {
    /// Opens the editor's link, so `vox editor call` and `vox mcp` reach it;
    /// `headless` says it has no window, so a windowed editor may take over.
    pub fn serve_link(&mut self, headless: bool) -> anyhow::Result<()> {
        let folder = link::link_folder(&self.game.project.root);
        let server = LinkServer::listen(&folder, link::LINK_FILE, "editor", headless)?;
        tracing::info!(
            "[Tools] tools can reach this editor: vox editor call --project {}",
            self.game.project.root.display()
        );
        self.link.server = Some(server);
        self.link.headless = headless;
        Ok(())
    }

    /// Makes a call from this program, as a test or `vox editor call
    /// --headless` does; its answer arrives once frames have run it.
    pub fn call(&mut self, call: &str, args: Json) -> Receiver<Answer> {
        let (request, answer) = Request::local(call, args);
        self.link.local.push(request);
        answer
    }

    /// Whether a tool asked the editor to close.
    pub fn wants_to_quit(&self) -> bool {
        self.link.quit
    }

    /// The calls since the last frame, and those waiting on frames.
    pub(super) fn update_link(&mut self, renderer: &mut Renderer) {
        self.watch_files(renderer);
        let mut requests = std::mem::take(&mut self.link.local);
        if let Some(server) = &self.link.server {
            requests.extend(server.poll());
            let connections = server.connections();
            if connections != self.link.connections {
                if connections > self.link.connections {
                    self.notify("A tool connected to the editor");
                }
                self.link.connections = connections;
            }
        }
        for request in requests {
            let outcome = self.run_call(&request.call, &request.args, renderer);
            match outcome {
                Ok(Outcome::Done(value)) => request.answer(Ok(value)),
                Ok(Outcome::Later(waiting)) => self.link.waiting.push((request, waiting)),
                Err(error) => request.answer(Err(error)),
            }
        }
        for (request, waiting) in std::mem::take(&mut self.link.waiting) {
            match self.go_on_waiting(waiting, renderer) {
                Ok(Outcome::Done(value)) => request.answer(Ok(value)),
                Ok(Outcome::Later(waiting)) => self.link.waiting.push((request, waiting)),
                Err(error) => request.answer(Err(error)),
            }
        }
    }

    /// Runs one call, as the editor's own panels would.
    fn run_call(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        let Some(info) = link::call_info(call) else {
            return Err(refuse(
                ErrorKind::NoSuchCall,
                format!("no call is named `{call}`; `vox editor call --describe` lists them"),
            ));
        };
        if !args.is_object() && !args.is_null() {
            return Err(bad("a call's arguments are a JSON object"));
        }
        let empty = json!({});
        let args = if args.is_null() { &empty } else { args };
        if info.changes && !self.memory.settings.tools_may_change {
            return Err(refuse(
                ErrorKind::NotAllowed,
                "the editor's settings let tools read the project, not change it",
            ));
        }
        if info.changes && call != "undo" && call != "redo" && call != "batch" {
            tracing::info!("[Tools] {call} {}", short(args));
        }
        match call {
            "editor.state" => done(self.state_json()),
            "objs.list" => self.objs_list(args),
            "obj.get" => self.obj_get(args),
            "components" => done(self.components_json()),
            "kinds" => done(kinds_json(&self.game.project.modules())),
            "obj.create" => self.obj_create(args, renderer),
            "obj.set" => self.obj_set(args),
            "obj.duplicate" => self.obj_duplicate(args, renderer),
            "obj.delete" => self.obj_delete(args),
            "obj.move" => self.obj_move(args),
            "component.add" => self.component_add(args, renderer),
            "component.remove" => self.component_change(args, |game, entity, part, _| {
                props::remove(&mut game.engine.world, entity, part);
                Ok(())
            }),
            "component.enable" => self.component_enable(args),
            "field.set" => self.field_set(args, renderer),
            "field.reset" => self.field_change(args, |game, _, entity, prop| {
                props::reset_field(&mut game.engine.world, entity, prop);
            }),
            "field.choices" => self.field_choices(args),
            "list.add" => self.field_change(args, |game, types, entity, prop| {
                props::add_item(&mut game.engine.world, types, entity, prop);
            }),
            "list.remove" => self.field_change(args, |game, types, entity, prop| {
                props::remove_item(&mut game.engine.world, types, entity, prop);
            }),
            "undo" | "redo" => self.undo_or_redo(call == "undo", renderer),
            "batch" => self.batch(args, renderer),
            "select" => {
                let objs = self.objs_arg(need(args, "objs")?)?;
                match objs.first() {
                    None => self.select(None),
                    Some(_) => self.select_many(objs),
                }
                done(json!({}))
            }
            "frame" => {
                let objs = match args.get("objs") {
                    Some(objs) => self.objs_arg(objs)?,
                    None => self.selected_objs(),
                };
                self.frame(&objs, renderer);
                done(json!({}))
            }
            "commands" => {
                let sources = self.command_sources();
                let commands: Vec<Json> = palette::command_list(&sources)
                    .into_iter()
                    .map(|(id, title, keys, _)| json!({ "id": id, "title": title, "keys": keys }))
                    .collect();
                done(json!({ "commands": commands }))
            }
            "command.run" => {
                let id = need_text(args, "id")?;
                let action = palette::command_list(&self.command_sources()).into_iter().find(|(key, ..)| key == id);
                let Some((.., action)) = action else {
                    return Err(bad(format!("no command has the id `{id}`; `commands` lists them")));
                };
                self.run_palette(action, renderer);
                done(json!({}))
            }
            "save" => {
                self.refuse_while_playing("save")?;
                let saved = self.save();
                done(json!({ "saved": saved }))
            }
            "level.open" => self.level_open(args, renderer),
            "scene.open_alongside" => {
                self.refuse_while_playing("open a scene")?;
                let (_, path, _) = link::resolve_project_asset(&self.game, need(args, "scene")?)?;
                self.open_scene_alongside(&path, renderer);
                done(json!({}))
            }
            "scene.reload" => {
                self.refuse_while_playing("read the scenes again")?;
                if self.unsaved && opt_bool(args, "discard") != Some(true) {
                    return Err(refuse(
                        ErrorKind::UnsavedConflict,
                        "the scenes have edits not saved; save, or give `discard`",
                    ));
                }
                let reloaded = self.reload_scenes(None, renderer).map_err(failed)?;
                done(json!({ "reloaded": reloaded }))
            }
            "files.look" => {
                self.imports.look_again();
                self.scripts_checked = None;
                self.link.watched = None;
                self.watch_files(renderer);
                done(json!({}))
            }
            "assets.list" => done(self.assets_list(args)),
            "asset.get" => self.asset_get(args),
            "asset.create" => self.asset_create(args, renderer),
            "asset.rename" | "asset.move" | "asset.duplicate" | "asset.delete" => self.asset_file(call, args, renderer),
            "play" => {
                if self.is_playing() {
                    return done(json!({ "playing": true }));
                }
                self.play(renderer);
                Ok(Outcome::Later(Waiting::Play(Instant::now())))
            }
            "pause" => {
                self.refuse_unless_playing("pause")?;
                let paused = opt_bool(args, "paused").unwrap_or(!self.is_paused());
                self.set_paused(paused);
                done(json!({ "paused": self.is_paused() }))
            }
            "step" => {
                self.refuse_unless_playing("step")?;
                self.step();
                Ok(Outcome::Later(Waiting::Frames(1)))
            }
            "stop" => {
                if self.is_playing() {
                    self.stop(renderer);
                }
                done(json!({}))
            }
            "game.console" => self.game_console(need_text(args, "line")?),
            "console.read" => self.console_read(args),
            "capture" => self.capture(args),
            "scripts.build" => {
                self.rebuild_scripts = true;
                self.link.scripts_failed = None;
                let from = self.console.log().lock().map_or(0, |log| log.total);
                Ok(Outcome::Later(Waiting::Scripts { from, began: false }))
            }
            "settings.get" => done(self.settings_json()),
            "settings.set" => self.settings_set(args),
            "project.get" => {
                let text = std::fs::read_to_string(self.game.project.root.join(voxear_engine::PROJECT_FILE))
                    .map_err(|error| refuse(ErrorKind::Failed, format!("reading voxear.toml: {error}")))?;
                done(json!({ "project": link::toml_json(&text) }))
            }
            "docs.search" => done(self.docs_search(args)),
            "docs.read" => self.docs_read(need_text(args, "reference")?),
            "editor.wait" => {
                let frames = args.get("frames").and_then(Json::as_u64).unwrap_or(1).clamp(1, 600) as u32;
                Ok(Outcome::Later(Waiting::Frames(frames)))
            }
            "editor.quit" => {
                if opt_bool(args, "save") == Some(true) && !self.is_playing() {
                    self.save();
                }
                if self.unsaved && opt_bool(args, "discard") != Some(true) {
                    return Err(refuse(
                        ErrorKind::UnsavedConflict,
                        "the scenes have edits not saved; give `save` or `discard`",
                    ));
                }
                self.link.quit = true;
                done(json!({}))
            }
            other => Err(refuse(ErrorKind::NoSuchCall, format!("`{other}` is described but not answered yet"))),
        }
    }

    /// Goes on with a call that waits on frames.
    fn go_on_waiting(&mut self, waiting: Waiting, renderer: &mut Renderer) -> Called {
        match waiting {
            Waiting::Frames(0) => done(json!({})),
            Waiting::Frames(left) => Ok(Outcome::Later(Waiting::Frames(left - 1))),
            Waiting::Play(began) => {
                if let Some(error) = self.play_error() {
                    return Err(refuse(ErrorKind::Failed, format!("the game did not start: {error}")));
                }
                if self.is_playing() {
                    return done(json!({ "playing": true }));
                }
                if began.elapsed() > PLAY_PATIENCE {
                    return Err(refuse(ErrorKind::Failed, "the game took too long to start"));
                }
                Ok(Outcome::Later(Waiting::Play(began)))
            }
            Waiting::Capture { view, path, settle, asked } => {
                self.go_on_capturing_for_tool(view, path, settle, asked, renderer)
            }
            Waiting::Scripts { from, began } => {
                let building = self.describing.is_some() || self.rebuild_scripts;
                if building || !began {
                    return Ok(Outcome::Later(Waiting::Scripts { from, began: began || building }));
                }
                let mut output: Vec<String> = Vec::new();
                if let Ok(log) = self.console.log().lock() {
                    let first = log.total - log.lines.len() as u64;
                    let skip = from.saturating_sub(first) as usize;
                    output.extend(log.lines.iter().skip(skip).map(|line| line.message.clone()));
                }
                output.extend(self.link.scripts_failed.clone());
                let problems = link::compiler_problems(&output.join("\n"));
                let failed = self.link.scripts_failed.take();
                let mut answer = json!({ "ok": failed.is_none(), "problems": problems });
                if let Some(failed) = failed.filter(|_| problems.is_empty()) {
                    answer["message"] = json!(failed);
                }
                done(answer)
            }
        }
    }

    fn refuse_while_playing(&self, what: &str) -> Result<(), LinkError> {
        match self.is_playing() {
            true => Err(refuse(ErrorKind::RefusedWhilePlaying, format!("stop playing to {what}"))),
            false => Ok(()),
        }
    }

    fn refuse_unless_playing(&self, what: &str) -> Result<(), LinkError> {
        match self.is_playing() {
            true => Ok(()),
            false => Err(refuse(ErrorKind::Failed, format!("the game is not playing, so there is nothing to {what}"))),
        }
    }

    /// Notes a change as a step to undo, unless playing, when it changed the
    /// played world only.
    fn record(&mut self, edit: Edit) -> bool {
        if self.is_playing() {
            return false;
        }
        self.history.record_step(edit);
        self.unsaved = true;
        true
    }

    /// Records the change from `before` to the Obj as it is now, and shows
    /// it in the Inspectors and Hierarchies.
    fn record_change(&mut self, entity: Entity, before: ObjState) -> bool {
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return false };
        let changed = before != after;
        if changed && !self.holds_data(entity) {
            self.record(Edit::Changed { entity, before, after });
        }
        self.shown_again();
        changed
    }

    /// The Hierarchies and Inspectors show what a call changed.
    fn shown_again(&mut self) {
        self.refresh_hierarchies();
        let selected = self.selection.filter(|&e| self.game.engine.world.is_alive(e));
        self.show_selection(selected);
    }

    fn obj_arg(&self, args: &Json) -> Result<Entity, LinkError> {
        link::resolve_obj(&self.game, need(args, "obj")?)
    }

    fn objs_arg(&self, objs: &Json) -> Result<Vec<Entity>, LinkError> {
        let list = objs.as_array().ok_or_else(|| bad("`objs` is a list of Objs"))?;
        list.iter().map(|obj| link::resolve_obj(&self.game, obj)).collect()
    }

    fn state_json(&self) -> Json {
        let levels = &self.game.levels;
        let scenes: Vec<Json> = levels
            .open_scenes()
            .into_iter()
            .map(|(guid, path)| json!({ "guid": guid.to_string(), "path": path }))
            .collect();
        let selection: Vec<Json> = self.selected_objs().into_iter().map(|e| obj_ref(&self.game, e)).collect();
        json!({
            "project": self.game.project.name(),
            "root": std::fs::canonicalize(&self.game.project.root).unwrap_or_else(|_| self.game.project.root.clone()).display().to_string(),
            "level": levels.current(),
            "scenes": scenes,
            "active_scene": levels.active_scene(),
            "playing": self.is_playing(),
            "paused": self.is_paused(),
            "building_scripts": self.describing.is_some() || self.play.is_building(),
            "unsaved": self.unsaved,
            "selection": selection,
            "undo": self.history.depth(),
            "redo": self.history.redo_depth(),
            "tools_may_change": self.memory.settings.tools_may_change,
            "headless": self.link.headless,
        })
    }

    fn objs_list(&self, args: &Json) -> Called {
        let world = &self.game.engine.world;
        let name = opt_text(args, "name").map(str::to_lowercase);
        let component = opt_text(args, "component");
        let under = match args.get("under").filter(|u| !u.is_null()) {
            Some(under) => Some(link::resolve_obj(&self.game, under)?),
            None => None,
        };
        let mut found = Vec::new();
        for (scene, _, entities) in self.game.levels.scene_order() {
            for entity in entities {
                if !world.is_alive(entity) || world.has::<EditorOnly>(entity) {
                    continue;
                }
                let own = world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_default();
                if name.as_ref().is_some_and(|wanted| !own.to_lowercase().contains(wanted)) {
                    continue;
                }
                let parts = props::parts(world, entity);
                if component.is_some_and(|wanted| !parts.iter().any(|p| link::component_id(p) == wanted)) {
                    continue;
                }
                let ancestors = ancestors(world, entity);
                if under.is_some_and(|under| !ancestors.contains(&under)) {
                    continue;
                }
                let parent = world.get::<Parent>(entity).map(|p| obj_ref(&self.game, p.0));
                let components: Vec<String> = parts.iter().map(link::component_id).collect();
                found.push(json!({
                    "obj": obj_ref(&self.game, entity),
                    "name": own,
                    "parent": parent,
                    "depth": ancestors.len(),
                    "scene": scene.to_string(),
                    "components": components,
                }));
            }
        }
        done(json!({ "objs": found }))
    }

    fn obj_get(&self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let (game, types) = (&self.game, self.inspector.types());
        let world = &game.engine.world;
        let state = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let components: Vec<Json> = props::parts(world, entity)
            .iter()
            .map(|part| {
                let fields: Vec<Json> = link::named_props(game, types, entity, part)
                    .iter()
                    .map(|(name, prop)| link::field_json(game, types, entity, name, prop))
                    .collect();
                let enabled = match part {
                    Part::Behaviour(name) => props::behaviour_enabled(game, entity, name),
                    other => other.switch().map(|switch| !state.off.contains(switch)),
                };
                json!({ "component": link::component_id(part), "title": part.title(), "enabled": enabled, "fields": fields })
            })
            .collect();
        let transform = state.transform;
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        let parent = world.get::<Parent>(entity).map(|p| obj_ref(game, p.0));
        done(json!({
            "obj": obj_ref(game, entity),
            "name": state.name,
            "parent": parent,
            "active": state.active,
            "persistent": state.persistent,
            "position": transform.position.to_array(),
            "rotation": [x.to_degrees(), y.to_degrees(), z.to_degrees()],
            "scale": transform.scale.to_array(),
            "components": components,
        }))
    }

    fn components_json(&self) -> Json {
        let modules = self.game.project.modules();
        let mut components: Vec<Json> = link::every_part()
            .into_iter()
            .map(|(part, folder)| {
                let module = part.module().map(|module| format!("{module:?}"));
                let offered = part.module().is_none_or(|module| modules.has(module));
                json!({
                    "component": link::component_id(&part),
                    "title": part.title(),
                    "folder": folder,
                    "module": module,
                    "offered": offered,
                    "script": false,
                })
            })
            .collect();
        let types = self.inspector.types();
        for script in types.behaviours.iter().chain(&types.components) {
            let fields: Vec<Json> = script
                .fields
                .iter()
                .map(|field| json!({ "field": field.name, "type": format!("{:?}", field.kind).to_lowercase() }))
                .collect();
            components.push(json!({
                "component": script.name,
                "title": Part::Behaviour(script.name.clone()).title(),
                "script": true,
                "offered": true,
                "fields": fields,
            }));
        }
        json!({ "components": components })
    }

    fn obj_create(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let parent = match args.get("parent").filter(|p| !p.is_null()) {
            Some(parent) => Some(link::resolve_obj(&self.game, parent)?),
            None => None,
        };
        let scene = match args.get("scene").filter(|s| !s.is_null()) {
            Some(scene) => Some(link::resolve_project_asset(&self.game, scene)?.0),
            None => None,
        };
        let position = match opt_vector(args, "position")? {
            Some(at) => Vec3::from_array(at),
            None if parent.is_some() => Vec3::ZERO,
            None => self.new_obj_position(),
        };
        let modules = self.game.project.modules();
        let made = match (opt_text(args, "kind"), args.get("prefab"), args.get("model")) {
            (Some(kind), None, None) => {
                let index = NEW_OBJS.iter().position(|(label, name, _, module)| {
                    (label.eq_ignore_ascii_case(kind) || name.eq_ignore_ascii_case(kind))
                        && module.is_none_or(|module| modules.has(module))
                });
                let Some(index) = index else {
                    return Err(bad(format!("`{kind}` is no New Object kind this project has; `kinds` lists them")));
                };
                objs::create(&mut self.game, Some(renderer), index, parent, scene, position)
            }
            (None, Some(prefab), None) => {
                let (_, path, kind) = link::resolve_project_asset(&self.game, prefab)?;
                if kind != AssetKind::Prefab {
                    return Err(bad(format!("{path} is not a prefab")));
                }
                objs::create_from_prefab(&mut self.game, Some(renderer), &path, parent, scene, position)
            }
            (None, None, Some(model)) => {
                let (_, path, kind) = link::resolve_project_asset(&self.game, model)?;
                if kind != AssetKind::Model {
                    return Err(bad(format!("{path} is not a model")));
                }
                objs::place_model(&mut self.game, renderer, &path, parent, scene, position)
            }
            _ => return Err(bad("give one of `kind`, `prefab` or `model`")),
        };
        let made = made.map_err(failed)?;
        if parent.is_some()
            && let (Some(at), Some(transform)) =
                (opt_vector(args, "position")?, self.game.engine.world.get_mut::<voxear_scene::Transform>(made))
        {
            transform.position = Vec3::from_array(at);
        }
        if let Some(name) = opt_text(args, "name") {
            self.game.engine.world.insert(made, Name(name.to_string()));
        }
        self.record(Edit::Exists { root: made, taken: None });
        self.shown_again();
        done(json!({ "obj": obj_ref(&self.game, made) }))
    }

    fn obj_set(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let world = &mut self.game.engine.world;
        let before = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let mut after = before.clone();
        if let Some(name) = opt_text(args, "name") {
            after.name = name.to_string();
        }
        if let Some(at) = opt_vector(args, "position")? {
            after.transform.position = Vec3::from_array(at);
        }
        if let Some([x, y, z]) = opt_vector(args, "rotation")? {
            after.transform.rotation = Quat::from_euler(EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians());
        }
        if let Some(scale) = opt_vector(args, "scale")? {
            after.transform.scale = Vec3::from_array(scale);
        }
        if let Some(active) = opt_bool(args, "active") {
            after.active = active;
        }
        if let Some(persistent) = opt_bool(args, "persistent") {
            after.persistent = persistent;
        }
        after.apply(world, entity);
        let changed = self.record_change(entity, before);
        done(json!({ "changed": changed }))
    }

    fn obj_duplicate(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = self.obj_arg(args)?;
        let copy = objs::duplicate(&mut self.game, Some(renderer), entity).map_err(failed)?;
        self.record(Edit::Exists { root: copy, taken: None });
        self.shown_again();
        done(json!({ "obj": obj_ref(&self.game, copy) }))
    }

    fn obj_delete(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let going = objs::family(&self.game.engine.world, entity);
        if self.selected_objs().iter().any(|selected| going.contains(selected)) {
            self.select(None);
        }
        let taken = objs::take(&mut self.game, entity);
        self.record(Edit::Exists { root: entity, taken: Some(taken) });
        self.shown_again();
        done(json!({}))
    }

    fn obj_move(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let to = match (args.get("into"), args.get("before"), args.get("after"), args.get("scene")) {
            (Some(into), None, None, None) => Destination::Into(link::resolve_obj(&self.game, into)?),
            (None, Some(before), None, None) => Destination::Before(link::resolve_obj(&self.game, before)?),
            (None, None, Some(after), None) => Destination::After(link::resolve_obj(&self.game, after)?),
            (None, None, None, Some(scene)) => Destination::SceneEnd(link::resolve_project_asset(&self.game, scene)?.0),
            _ => return Err(bad("give one of `into`, `before`, `after` or `scene`")),
        };
        let moved = objs::move_obj(&mut self.game, entity, to);
        if let Some((before, after)) = moved.clone() {
            self.record(Edit::Moved { entity, before, after });
        }
        self.shown_again();
        done(json!({ "changed": moved.is_some() }))
    }

    fn component_add(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = self.obj_arg(args)?;
        let types = self.inspector.types().clone();
        let part = link::resolve_part(&types, need_text(args, "component")?)?;
        if props::parts(&self.game.engine.world, entity).contains(&part) {
            return done(json!({ "changed": false }));
        }
        let modules = self.game.project.modules();
        if let Some(module) = part.module().filter(|module| !modules.has(*module)) {
            return Err(refuse(
                ErrorKind::NotAllowed,
                format!("{} needs the {module:?} module, which the project has off", part.title()),
            ));
        }
        let before = ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        props::add(&mut self.game, Some(renderer), entity, &part, &types).map_err(failed)?;
        let changed = self.record_change(entity, before);
        done(json!({ "changed": changed }))
    }

    /// A change to one component of an Obj, as a step to undo.
    fn component_change(
        &mut self,
        args: &Json,
        change: impl FnOnce(
            &mut voxear_engine::Game,
            Entity,
            &Part,
            &voxear_engine::script_types::ScriptTypes,
        ) -> Result<(), LinkError>,
    ) -> Called {
        let entity = self.obj_arg(args)?;
        let part = link::part_on(&self.game, entity, need_text(args, "component")?)?;
        let types = self.inspector.types().clone();
        let before = ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        change(&mut self.game, entity, &part, &types)?;
        let changed = self.record_change(entity, before);
        done(json!({ "changed": changed }))
    }

    fn component_enable(&mut self, args: &Json) -> Called {
        let enabled = opt_bool(args, "enabled").ok_or_else(|| bad("`enabled` is true or false"))?;
        self.component_change(args, |game, entity, part, _| match part {
            Part::Behaviour(name) => {
                props::set_behaviour_enabled(game, entity, name, enabled);
                Ok(())
            }
            other => {
                let switch = other.switch().ok_or_else(|| bad(format!("{} cannot be switched off", other.title())))?;
                let world = &mut game.engine.world;
                let mut state =
                    ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
                state.off.set(switch, !enabled);
                state.apply(world, entity);
                Ok(())
            }
        })
    }

    /// A change to one field, as a step to undo.
    fn field_change(
        &mut self,
        args: &Json,
        change: impl FnOnce(&mut voxear_engine::Game, &voxear_engine::script_types::ScriptTypes, Entity, &props::Prop),
    ) -> Called {
        let entity = self.obj_arg(args)?;
        let part = link::part_on(&self.game, entity, need_text(args, "component")?)?;
        let types = self.inspector.types().clone();
        let prop = link::resolve_prop(&self.game, &types, entity, &part, need_text(args, "field")?)?;
        let before = ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        change(&mut self.game, &types, entity, &prop);
        let changed = self.record_change(entity, before);
        done(json!({ "changed": changed }))
    }

    fn field_set(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = self.obj_arg(args)?;
        let part = link::part_on(&self.game, entity, need_text(args, "component")?)?;
        let field = need_text(args, "field")?;
        let types = self.inspector.types().clone();
        let prop = link::resolve_prop(&self.game, &types, entity, &part, field)?;
        let now = props::get(&self.game, &types, entity, &prop)
            .ok_or_else(|| refuse(ErrorKind::NoSuchField, format!("`{field}` has no value to set")))?;
        let setting =
            link::read_value(&self.game, &now, need(args, "value").or_else(|_| Ok::<_, LinkError>(&Json::Null))?)?;
        let before = ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        match setting {
            Setting::Value(value) => {
                props::set(&mut self.game, Some(renderer), &types, entity, &prop, value);
            }
            Setting::Pick(pick) => self.pick_for_tool(entity, &prop, &pick, &types, renderer)?,
        }
        let changed = self.record_change(entity, before);
        let shown = link::field_json(&self.game, &types, entity, field, &prop);
        done(json!({ "changed": changed, "value": shown["value"] }))
    }

    /// Points a reference field at what a tool gave, if its menu offers
    /// it, as dropping it on the field would.
    fn pick_for_tool(
        &mut self,
        entity: Entity,
        prop: &props::Prop,
        pick: &Pick,
        types: &voxear_engine::script_types::ScriptTypes,
        renderer: &mut Renderer,
    ) -> Result<(), LinkError> {
        let part_of_model = match (pick, prop) {
            (Pick::Asset(Some(reference)), props::Prop::Mesh) => reference.contains("#mesh"),
            (Pick::Asset(Some(reference)), props::Prop::Material) => reference.contains("#material"),
            _ => false,
        };
        let clip = matches!(pick, Pick::Asset(Some(_))) && props::takes_clips(types, prop);
        let none = matches!(pick, Pick::Obj(None) | Pick::Asset(None));
        let offered = props::choices(&self.game, types, entity, prop).iter().any(|(_, offered)| offered == pick);
        if !(part_of_model || clip || none || offered) {
            return Err(refuse(ErrorKind::WrongValue, props::why_not(&self.game, types, entity, prop, pick)));
        }
        if !props::pick(&mut self.game, Some(renderer), types, entity, prop, pick) && !none {
            return Err(refuse(ErrorKind::WrongValue, props::why_not(&self.game, types, entity, prop, pick)));
        }
        Ok(())
    }

    fn field_choices(&self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let part = link::part_on(&self.game, entity, need_text(args, "component")?)?;
        let types = self.inspector.types();
        let prop = link::resolve_prop(&self.game, types, entity, &part, need_text(args, "field")?)?;
        if let Some(Value::Choice(_, options)) = props::get(&self.game, types, entity, &prop) {
            let choices: Vec<Json> = options.iter().map(|option| json!({ "label": option, "value": option })).collect();
            return done(json!({ "choices": choices }));
        }
        let choices: Vec<Json> = props::choices(&self.game, types, entity, &prop)
            .into_iter()
            .map(|(label, pick)| {
                let value = match pick {
                    Pick::Obj(Some(target)) => obj_ref(&self.game, target),
                    Pick::Asset(Some(path)) => link::asset_ref(&self.game, &path),
                    Pick::Key(Some(key)) => json!({ "key": key }),
                    Pick::Obj(None) | Pick::Asset(None) | Pick::Key(None) => Json::Null,
                };
                json!({ "label": label, "value": value })
            })
            .collect();
        done(json!({ "choices": choices }))
    }

    fn undo_or_redo(&mut self, undo: bool, renderer: &mut Renderer) -> Called {
        self.refuse_while_playing(if undo { "undo" } else { "redo" })?;
        let (undo_before, redo_before) = (self.history.depth(), self.history.redo_depth());
        if undo {
            self.undo(renderer);
        } else {
            self.redo(renderer);
        }
        let moved = (self.history.depth(), self.history.redo_depth()) != (undo_before, redo_before);
        tracing::info!("[Tools] {}", if undo { "undo" } else { "redo" });
        self.shown_again();
        let key = if undo { "undone" } else { "redone" };
        done(json!({ key: moved, "undo": self.history.depth(), "redo": self.history.redo_depth() }))
    }

    fn batch(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let calls = need(args, "calls")?.as_array().ok_or_else(|| bad("`calls` is a list"))?.clone();
        let depth = self.history.depth();
        let mut results = Vec::new();
        let mut failed = Json::Null;
        for (index, step) in calls.iter().enumerate() {
            let call = step.get("call").and_then(Json::as_str).unwrap_or_default();
            let step_args = step.get("args").cloned().unwrap_or_else(|| json!({}));
            let answer = match call {
                "batch" | "play" | "step" | "capture" | "scripts.build" | "editor.wait" | "editor.quit" => {
                    Err(bad(format!("`{call}` waits on frames, so it is not made in a batch")))
                }
                _ => self.run_call(call, &step_args, renderer),
            };
            match answer {
                Ok(Outcome::Done(result)) => results.push(result),
                Ok(Outcome::Later(_)) => unreachable!("calls that wait are refused above"),
                Err(error) => {
                    failed = json!({ "index": index, "call": call, "error": error.to_json() });
                    break;
                }
            }
        }
        self.history.group_since(depth);
        done(json!({ "results": results, "failed": failed }))
    }

    fn level_open(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        self.refuse_while_playing("open a level")?;
        let (_, path, kind) = link::resolve_project_asset(&self.game, need(args, "level")?)?;
        if self.unsaved && opt_bool(args, "discard") != Some(true) {
            return Err(refuse(ErrorKind::UnsavedConflict, "the scenes have edits not saved; save, or give `discard`"));
        }
        match kind {
            AssetKind::Level => self.open_level(&path, renderer),
            AssetKind::Scene | AssetKind::Prefab => self.open_scene(&path, renderer),
            _ => return Err(bad(format!("{path} is not a level or a scene"))),
        }
        self.link.times.clear();
        done(json!({ "level": self.game.levels.current() }))
    }

    fn assets_list(&self, args: &Json) -> Json {
        let kind = opt_text(args, "kind");
        let folder = opt_text(args, "folder").map(|folder| folder.trim_end_matches('/').to_string());
        let mut assets: Vec<(&str, Json)> = self
            .game
            .content
            .db()
            .entries()
            .filter(|entry| kind.is_none_or(|kind| link::asset_kind_id(entry.kind).0 == kind))
            .filter(|entry| folder.as_ref().is_none_or(|folder| entry.path.starts_with(&format!("{folder}/"))))
            .map(|entry| {
                let (kind, _) = link::asset_kind_id(entry.kind);
                (entry.path.as_str(), json!({ "guid": entry.guid.to_string(), "path": entry.path, "kind": kind }))
            })
            .collect();
        assets.sort_by(|a, b| a.0.cmp(b.0));
        json!({ "assets": assets.into_iter().map(|(_, asset)| asset).collect::<Vec<_>>() })
    }

    fn asset_get(&self, args: &Json) -> Called {
        let (guid, path, kind) = link::resolve_project_asset(&self.game, need(args, "asset")?)?;
        let (kind_id, text) = link::asset_kind_id(kind);
        let file = self.game.content.db().mounts().file(&path);
        let mut answer =
            json!({ "guid": guid.to_string(), "path": path, "kind": kind_id, "file": file.display().to_string() });
        if text && let Ok(content) = std::fs::read_to_string(&file) {
            match link::toml_json(&content) {
                Some(parsed) => answer["content"] = parsed,
                None if content.len() <= TEXT_LIMIT => answer["text"] = json!(content),
                None => {
                    let end = (0..=TEXT_LIMIT).rev().find(|&at| content.is_char_boundary(at)).unwrap_or(0);
                    answer["text"] = json!(&content[..end]);
                }
            }
        }
        done(answer)
    }

    fn asset_create(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let kind = need_text(args, "kind")?;
        let modules = self.game.project.modules();
        let new = new_files()
            .into_iter()
            .filter(|new| new.module().is_none_or(|module| modules.has(module)))
            .find(|new| new_file_name(*new).eq_ignore_ascii_case(kind))
            .ok_or_else(|| bad(format!("`{kind}` is no kind of file the Project panel makes; `kinds` lists them")))?;
        let folder = opt_text(args, "folder").unwrap_or_default().trim_matches('/').to_string();
        let root = self.game.project.root.clone();
        if !folder.is_empty() {
            std::fs::create_dir_all(root.join(&folder))
                .map_err(|error| refuse(ErrorKind::Failed, format!("making {folder}: {error}")))?;
        }
        let namespace = super::script_namespace(&self.game.project);
        let made = project_files::create(&root, &folder, new, &namespace).map_err(failed)?;
        self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        done(json!({ "asset": link::asset_ref(&self.game, &made) }))
    }

    fn asset_file(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        let (_, path, _) = link::resolve_project_asset(&self.game, need(args, "asset")?)?;
        let mounts = self.game.content.db().mounts().clone();
        let place = project_files::item_place(&mounts, &path).map_err(failed)?;
        let root = self.game.project.root.clone();
        let answer = match call {
            "asset.rename" => {
                let name = need_text(args, "name")?;
                let renamed = project_files::rename(&place.root, &place.path, name).map_err(failed)?;
                Some(place.back(&renamed))
            }
            "asset.move" => {
                let folder = need_text(args, "folder")?.trim_matches('/');
                self.move_in_project(&path, folder, renderer);
                let moved = if folder.is_empty() {
                    path.rsplit('/').next().unwrap_or(&path).to_string()
                } else {
                    format!("{folder}/{}", path.rsplit('/').next().unwrap_or(&path))
                };
                Some(moved)
            }
            "asset.duplicate" => Some(place.back(&project_files::duplicate(&place.root, &place.path).map_err(failed)?)),
            _ => {
                let trashed = project_files::trash(&place.root, &place.path, &root).map_err(failed)?;
                self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
                return done(json!({ "trashed": trashed.display().to_string() }));
            }
        };
        self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        let asset = answer.map(|path| link::asset_ref(&self.game, &path));
        done(json!({ "asset": asset }))
    }

    fn game_console(&mut self, line: &str) -> Called {
        self.refuse_unless_playing("run a console line in")?;
        let log = self.console.log().clone();
        let from = log.lock().map_or(0, |log| log.total);
        let ran = self.game.engine.run_console(line).map_err(failed)?;
        let lines: Vec<String> = match log.lock() {
            Ok(log) => {
                let first = log.total - log.lines.len() as u64;
                let skip = from.saturating_sub(first) as usize;
                log.lines.iter().skip(skip).map(|line| line.message.clone()).collect()
            }
            Err(_) => Vec::new(),
        };
        done(json!({ "ran": ran, "lines": lines }))
    }

    fn console_read(&self, args: &Json) -> Called {
        let level = match opt_text(args, "level") {
            Some("error") => tracing::Level::ERROR,
            Some("warning") => tracing::Level::WARN,
            Some("info") | None => tracing::Level::TRACE,
            Some(other) => return Err(bad(format!("`{other}` is not info, warning or error"))),
        };
        let limit = args.get("limit").and_then(Json::as_u64).map_or(LINES, |n| n as usize).max(1);
        let after = args.get("after").and_then(Json::as_u64);
        let log = self
            .console
            .log()
            .lock()
            .map_err(|_| refuse(ErrorKind::Failed, "the Console's lines could not be read"))?;
        let first = log.total - log.lines.len() as u64;
        let mut lines: Vec<Json> = Vec::new();
        for (index, line) in log.lines.iter().enumerate() {
            let number = first + index as u64;
            if after.is_some_and(|after| number < after) || line.level > level {
                continue;
            }
            let mut shown = json!({
                "number": number,
                "level": line.level.as_str().to_lowercase(),
                "message": line.message,
                "from": line.target,
                "seconds": line.seconds,
                "frame": line.frame,
            });
            if let (Some(file), Some(at)) = (&line.file, line.line) {
                (shown["file"], shown["line"]) = (json!(file), json!(at));
            }
            if let Some(stack) = &line.stack {
                shown["stack"] = json!(stack);
            }
            if let Some(entity) = line.context.map(Entity::from_bits).filter(|e| self.game.engine.world.is_alive(*e)) {
                shown["obj"] = obj_ref(&self.game, entity);
            }
            if let Some(asset) = &line.asset {
                shown["asset"] = json!(asset);
            }
            lines.push(shown);
        }
        let skip = lines.len().saturating_sub(limit);
        done(json!({ "lines": lines.split_off(skip), "next": log.total }))
    }

    fn capture(&mut self, args: &Json) -> Called {
        let view = match need_text(args, "view")? {
            "scene" => {
                if !self.shown(self.scene.viewport) {
                    let panel = self.scene.panel.clone();
                    self.show_panel(&panel);
                }
                Some(self.scene.viewport)
            }
            "game" => {
                if self.game_camera().is_none() {
                    return Err(refuse(ErrorKind::Failed, "the level has no camera for the Game view to look through"));
                }
                if !self.shown(self.nodes.game_viewport) {
                    self.show_panel("game");
                }
                Some(self.nodes.game_viewport)
            }
            "editor" => None,
            other => return Err(bad(format!("`{other}` is not scene, game or editor"))),
        };
        let path = match opt_text(args, "path") {
            Some(path) => PathBuf::from(path),
            None => {
                let folder = link::link_folder(&self.game.project.root).join("Captures");
                let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis();
                folder.join(format!("{}-{stamp}.png", need_text(args, "view")?))
            }
        };
        Ok(Outcome::Later(Waiting::Capture { view, path, settle: SETTLE, asked: None }))
    }

    fn go_on_capturing_for_tool(
        &mut self,
        view: Option<voxear_ui::NodeId>,
        path: PathBuf,
        settle: u32,
        asked: Option<Instant>,
        renderer: &mut Renderer,
    ) -> Called {
        let waiting = |settle, asked| Ok(Outcome::Later(Waiting::Capture { view, path: path.clone(), settle, asked }));
        // A VoxDocs page's capture takes every copy the renderer makes; the tool's waits its turn.
        if self.docs_capture.is_some() {
            return waiting(settle, asked);
        }
        if settle > 0 {
            return waiting(settle - 1, asked);
        }
        let Some(began) = asked else {
            let area = view.map(|view| self.capture_area(view));
            renderer.request_capture(voxear_render::Capture { area, before_ui: view.is_some() });
            return waiting(0, Some(Instant::now()));
        };
        let Some(image) = renderer.take_capture() else {
            if began.elapsed() > CAPTURE_PATIENCE {
                return Err(refuse(ErrorKind::Failed, "the view could not be copied on this system"));
            }
            return waiting(0, asked);
        };
        if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
            std::fs::create_dir_all(folder)
                .map_err(|error| refuse(ErrorKind::Failed, format!("making {}: {error}", folder.display())))?;
        }
        let picture = image::RgbaImage::from_raw(image.width, image.height, image.rgba)
            .ok_or_else(|| refuse(ErrorKind::Failed, "the capture's size"))?;
        picture
            .save(&path)
            .map_err(|error| refuse(ErrorKind::Failed, format!("writing {}: {error}", path.display())))?;
        done(json!({ "path": path.display().to_string(), "width": image.width, "height": image.height }))
    }

    fn settings_json(&self) -> Json {
        let settings = &self.memory.settings;
        let listed: Vec<Json> = settings::SETTINGS
            .iter()
            .map(|info| {
                let (kind, value, options) = match (settings.get(info.key), info.kind) {
                    (Some(SettingValue::Toggle(on)), _) => ("toggle", json!(on), None),
                    (Some(SettingValue::Choice(at)), SettingKind::Choice(options)) => {
                        ("choice", json!(options.get(at)), Some(options))
                    }
                    (Some(SettingValue::Number(number)), _) => ("number", json!(number), None),
                    (Some(SettingValue::Color(color)), _) => ("color", json!(format!("#{color:06x}")), None),
                    _ => ("none", Json::Null, None),
                };
                let mut shown =
                    json!({ "key": info.key, "title": info.title, "group": info.group, "type": kind, "value": value });
                if let Some(options) = options {
                    shown["options"] = json!(options);
                }
                shown
            })
            .collect();
        json!({ "settings": listed })
    }

    fn settings_set(&mut self, args: &Json) -> Called {
        let key = need_text(args, "key")?;
        let info = settings::info(key)
            .ok_or_else(|| bad(format!("no setting has the key `{key}`; `settings.get` lists them")))?;
        let value = need(args, "value")?;
        let wrong = || refuse(ErrorKind::WrongValue, format!("{value} is not a value for {}", info.title));
        let setting = match info.kind {
            SettingKind::Toggle => SettingValue::Toggle(value.as_bool().ok_or_else(wrong)?),
            SettingKind::Choice(options) => {
                let word = value.as_str().ok_or_else(wrong)?;
                SettingValue::Choice(options.iter().position(|o| o.eq_ignore_ascii_case(word)).ok_or_else(wrong)?)
            }
            SettingKind::Number { .. } => SettingValue::Number(value.as_f64().ok_or_else(wrong)? as f32),
            SettingKind::Color => {
                let hex = value.as_str().and_then(|text| text.strip_prefix('#')).ok_or_else(wrong)?;
                SettingValue::Color(u32::from_str_radix(hex, 16).map_err(|_| wrong())?)
            }
        };
        // Tools may let themselves read only, but not give themselves leave to change.
        if key == "tools_may_change" && setting == SettingValue::Toggle(true) && !self.memory.settings.tools_may_change
        {
            return Err(refuse(ErrorKind::NotAllowed, "only the person at the editor lets tools change the project"));
        }
        if self.memory.settings.set(key, setting) {
            self.memory.save();
            self.apply_settings();
        }
        tracing::info!("[Tools] settings.set {key}");
        let now = self.memory.settings.describe(info);
        done(json!({ "value": now }))
    }

    fn docs_search(&self, args: &Json) -> Json {
        let words: Vec<String> =
            opt_text(args, "query").unwrap_or_default().split_whitespace().map(str::to_lowercase).collect();
        let limit = args.get("limit").and_then(Json::as_u64).unwrap_or(20) as usize;
        let mut found: Vec<(usize, Json)> = Vec::new();
        for (_, page) in self.docs.library().pages() {
            if page.is_folder && page.body.trim().is_empty() {
                continue;
            }
            let title = page.title().to_lowercase();
            let body = page.body.to_lowercase();
            if !words.iter().all(|word| title.contains(word) || body.contains(word)) {
                continue;
            }
            let in_title = words.iter().filter(|word| title.contains(word.as_str())).count();
            found.push((in_title, json!({ "reference": page.reference(), "title": page.title() })));
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        json!({ "pages": found.into_iter().take(limit).map(|(_, page)| page).collect::<Vec<_>>() })
    }

    fn docs_read(&self, reference: &str) -> Called {
        let library = self.docs.library();
        let id = library
            .find(reference)
            .ok_or_else(|| bad(format!("no page is `{reference}`; `docs.search` finds them")))?;
        let page = library.page(id);
        done(json!({ "title": page.title(), "text": page.body }))
    }

    /// Looks at the open scenes' files and `voxear.toml` now and then, and
    /// takes up what a tool changed in them: a scene with no edits waiting
    /// is read again, and one with edits is left, said once.
    fn watch_files(&mut self, renderer: &mut Renderer) {
        if self.link.watched.is_some_and(|at| at.elapsed() < WATCH) {
            return;
        }
        self.link.watched = Some(Instant::now());
        let root = self.game.project.root.clone();
        let mut files: Vec<(PathBuf, Option<voxear_assets::AssetGuid>)> =
            vec![(root.join(voxear_engine::PROJECT_FILE), None)];
        if let Some(level) = self.game.levels.current().filter(|level| level.ends_with(".level")) {
            files.push((root.join(level), None));
        }
        for (guid, path) in self.game.levels.open_scenes() {
            files.push((root.join(path), Some(guid)));
        }
        let mut changed_scenes = Vec::new();
        let (mut level_changed, mut project_changed) = (false, false);
        for (file, scene) in &files {
            let now = std::fs::metadata(file).and_then(|m| m.modified()).ok();
            let Some(was) = self.link.times.insert(file.clone(), now) else { continue };
            if was == now {
                continue;
            }
            match scene {
                Some(guid) => changed_scenes.push((*guid, file.clone())),
                None if file.ends_with(voxear_engine::PROJECT_FILE) => project_changed = true,
                None => level_changed = true,
            }
        }
        if project_changed {
            self.project_file_changed();
        }
        if self.is_playing() && (level_changed || !changed_scenes.is_empty()) {
            // Looked at again once play stops.
            for (_, file) in &changed_scenes {
                self.link.times.insert(file.clone(), None);
            }
            return;
        }
        if level_changed && let Err(error) = self.open_level_again(renderer) {
            tracing::error!("could not open the level again after its file changed: {error:#}");
        }
        // A scene whose file holds what it has now was saved by the editor itself.
        let game = &mut self.game;
        changed_scenes
            .retain(|(guid, _)| !game.levels.scene_matches_disk(&game.engine, &mut game.content, &root, *guid));
        if changed_scenes.is_empty() {
            return;
        }
        if self.unsaved {
            for (_, file) in changed_scenes {
                if !self.link.told.contains(&file) {
                    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    tracing::warn!(
                        "[Tools] {name} changed on disk under edits not saved; saving keeps yours, or a tool can read it again with scene.reload"
                    );
                    self.notify(&format!("{name} changed on disk; save keeps your edits"));
                    self.link.told.push(file);
                }
            }
            return;
        }
        let guids: Vec<voxear_assets::AssetGuid> = changed_scenes.iter().map(|(guid, _)| *guid).collect();
        match self.reload_scenes(Some(&guids), renderer) {
            Ok(reloaded) => {
                tracing::info!("[Tools] read {} again, changed on disk", reloaded.join(", "));
                self.notify(&format!("Read {} again, changed on disk", reloaded.join(", ")));
            }
            Err(error) => tracing::error!("could not read the changed scenes again: {error:#}"),
        }
    }

    /// Reads open scenes again from their files: `only` those, or all.
    /// The selection stays on the same Objs; undo starts afresh, as opening
    /// a level does.
    fn reload_scenes(
        &mut self,
        only: Option<&[voxear_assets::AssetGuid]>,
        renderer: &mut Renderer,
    ) -> anyhow::Result<Vec<String>> {
        let open: Vec<(voxear_assets::AssetGuid, String)> =
            self.game.levels.open_scenes().into_iter().map(|(guid, path)| (guid, path.to_string())).collect();
        let scenes: Vec<voxear_assets::AssetGuid> =
            open.iter().map(|(guid, _)| *guid).filter(|guid| only.is_none_or(|only| only.contains(guid))).collect();
        let selected: Vec<_> = self.selected_objs().into_iter().filter_map(|e| self.game.levels.place_of(e)).collect();
        let game = &mut self.game;
        game.levels.reload_scenes(&mut game.engine, &mut game.content, Some(renderer), &scenes)?;
        self.history.clear();
        self.unsaved = false;
        self.link.told.clear();
        let reselected: Vec<Entity> =
            selected.iter().filter_map(|(scene, path)| self.game.levels.entity_at(*scene, path)).collect();
        match reselected.first() {
            Some(_) => self.select_many(reselected),
            None => self.select(None),
        }
        // Its files as they are now are what the editor has.
        let root = self.game.project.root.clone();
        for (_, path) in open.iter().filter(|(guid, _)| scenes.contains(guid)) {
            let file = root.join(path);
            let time = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
            self.link.times.insert(file, time);
        }
        self.shown_again();
        Ok(open.into_iter().filter(|(guid, _)| scenes.contains(guid)).map(|(_, path)| path).collect())
    }

    /// `voxear.toml` changed on disk: the editor reads it again, so the
    /// project's modules, layers and settings are as the file has them.
    fn project_file_changed(&mut self) {
        match voxear_engine::Project::open(&self.game.project.root) {
            Ok(project) => {
                self.game.project = project;
                let modules = self.game.project.modules();
                let file = &self.game.project.file;
                self.project_settings.show(&mut self.ui, &modules);
                self.project_settings.show_origin(&mut self.ui, &file.floating_origin);
                self.project_settings.show_quality(&mut self.ui, &file.quality);
                self.project_settings.show_budget(&mut self.ui, &file.world);
                tracing::info!("[Tools] read voxear.toml again, changed on disk");
            }
            Err(error) => tracing::error!("voxear.toml changed on disk, but could not be read: {error:#}"),
        }
    }
}

/// An Obj's parents, nearest first.
fn ancestors(world: &voxear_ecs::World, entity: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut at = world.get::<Parent>(entity).map(|p| p.0);
    while let Some(parent) = at.filter(|parent| !found.contains(parent)) {
        found.push(parent);
        at = world.get::<Parent>(parent).map(|p| p.0);
    }
    found
}

fn failed(error: anyhow::Error) -> LinkError {
    refuse(ErrorKind::Failed, format!("{error:#}"))
}

/// A call's arguments short enough for a Console line.
fn short(args: &Json) -> String {
    let text = args.to_string();
    match text.char_indices().nth(160) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

/// The New Object kinds and the files the Project panel makes, as the
/// project's modules offer them.
fn kinds_json(modules: &voxear_engine::modules::Modules) -> Json {
    let objs: Vec<&str> = NEW_OBJS
        .iter()
        .filter(|(.., module)| module.is_none_or(|module| modules.has(module)))
        .map(|(label, ..)| *label)
        .collect();
    let assets: Vec<&str> = new_files()
        .into_iter()
        .filter(|new| new.module().is_none_or(|module| modules.has(module)))
        .map(new_file_name)
        .collect();
    json!({ "objs": objs, "assets": assets })
}

/// Every file the Project panel's Create menu makes.
fn new_files() -> Vec<NewFile> {
    // Matched whole: a new kind of file does not build until it is listed here.
    fn listed(new: NewFile) -> NewFile {
        match new {
            NewFile::Folder
            | NewFile::Scene
            | NewFile::Level
            | NewFile::Material
            | NewFile::Lighting
            | NewFile::LookPack
            | NewFile::TexturePack
            | NewFile::Shader
            | NewFile::UnlitShader
            | NewFile::UiShader
            | NewFile::LookShader
            | NewFile::Script
            | NewFile::MotionSet
            | NewFile::GunHandling
            | NewFile::Terrain
            | NewFile::WorldGen
            | NewFile::Block
            | NewFile::GlassBlock
            | NewFile::Biome
            | NewFile::PromptIcons
            | NewFile::BoxModel => new,
        }
    }
    [
        NewFile::Folder,
        NewFile::Scene,
        NewFile::Level,
        NewFile::Material,
        NewFile::Lighting,
        NewFile::LookPack,
        NewFile::TexturePack,
        NewFile::Shader,
        NewFile::UnlitShader,
        NewFile::UiShader,
        NewFile::LookShader,
        NewFile::Script,
        NewFile::MotionSet,
        NewFile::GunHandling,
        NewFile::Terrain,
        NewFile::WorldGen,
        NewFile::Block,
        NewFile::GlassBlock,
        NewFile::Biome,
        NewFile::PromptIcons,
        NewFile::BoxModel,
    ]
    .into_iter()
    .map(listed)
    .collect()
}

/// A file kind as `asset.create` takes it: its menu item without "New ".
fn new_file_name(new: NewFile) -> &'static str {
    let label = new.label();
    label.strip_prefix("New ").unwrap_or(label)
}
