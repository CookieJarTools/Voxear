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
    project_settings,
    project_view::ProjectEvent,
    props::{self, Part, Value},
    setting_info,
    settings::{self, Settings},
};

/// How often an editor waiting for the link asks for it again.
const LINK_RETRY: Duration = Duration::from_secs(2);
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
    /// The link this editor wants while another editor of the project
    /// still has it, and when it last asked: it takes the link once that
    /// one has closed.
    wanted: Option<Instant>,
    /// A script command may have given a world of blocks a block it did not
    /// hold, whose look is taken up in the next frame.
    pub(super) refresh_blocks: bool,
}

/// A call that answers once frames have gone by.
pub(super) enum Waiting {
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
    /// The open level's reflection probes baking.
    ProbeBake,
    /// The open level's reverb probes baking.
    ReverbBake,
    /// The open level's lighting baking.
    LightingBake,
    /// The open level's nav mesh baking.
    NavBake,
}

/// A call done, or one that answers later.
pub(super) enum Outcome {
    Done(Json),
    Later(Waiting),
}

pub(super) type Called = Result<Outcome, LinkError>;

pub(super) fn done(value: Json) -> Called {
    Ok(Outcome::Done(value))
}

impl Editor {
    /// Opens the editor's link, so `vox editor call` and `vox mcp` reach it;
    /// `headless` says it has no window, so a windowed editor may take over.
    /// While another editor of the project has the link, this one asks
    /// again every few seconds and takes it once that one has closed.
    pub fn serve_link(&mut self, headless: bool) -> anyhow::Result<()> {
        self.link.headless = headless;
        let folder = link::link_folder(&self.game.project.root);
        match LinkServer::listen(&folder, link::LINK_FILE, "editor", headless) {
            Ok(server) => {
                tracing::info!(
                    "[Tools] tools can reach this editor: vox editor call --project {}",
                    self.game.project.root.display()
                );
                self.link.server = Some(server);
                self.link.wanted = None;
                Ok(())
            }
            Err(error) => {
                self.link.wanted = Some(Instant::now());
                Err(error.context("tools reach this editor once the other one closes"))
            }
        }
    }

    /// Whether tools can reach this editor now.
    pub fn has_link(&self) -> bool {
        self.link.server.is_some()
    }

    /// Asks for the link again, while another editor had it.
    fn ask_for_link(&mut self) {
        if self.link.wanted.is_none_or(|asked| asked.elapsed() < LINK_RETRY) {
            return;
        }
        self.link.wanted = Some(Instant::now());
        let folder = link::link_folder(&self.game.project.root);
        // An editor still closing is waited for without asking it, which could keep this one's frame waiting.
        if voxear_engine::local_link::holder_running(&folder, link::LINK_FILE) == Some(true) {
            return;
        }
        if let Ok(server) = LinkServer::listen(&folder, link::LINK_FILE, "editor", self.link.headless) {
            tracing::info!("[Tools] the other editor closed: tools reach this one now");
            self.link.server = Some(server);
            self.link.wanted = None;
        }
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
        self.ask_for_link();
        if std::mem::take(&mut self.link.refresh_blocks) {
            voxear_engine::blocks::refresh(&mut self.game.engine.world, &mut self.game.content, Some(renderer));
        }
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
            "obj.set" => self.obj_set(args, renderer),
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
            "view.get" => done(self.view_json()),
            "view.set" => self.view_set(args),
            "blocks.get" => self.blocks_get(args),
            "blocks.set" | "blocks.fill" => self.blocks_set(call, args, renderer),
            "graph.comments" => self.graph_comments(args),
            "graph.comment.add" | "graph.comment.set" | "graph.comment.remove" => {
                self.refuse_while_playing("change a graph")?;
                self.graph_comment(call, args, renderer)
            }
            "block_world.write_land" => {
                self.refuse_while_playing("write a world's land into its generator")?;
                self.block_world_write_land(args, renderer)
            }
            "dungeon.starter" => {
                self.refuse_while_playing("make the starter dungeon")?;
                self.dungeon_starter(args, renderer)
            }
            "dungeon.layouts" => self.dungeon_layouts(args),
            "navigation.status" => self.navigation_status(),
            "navigation.path" => self.navigation_path(args),
            "navigation.nearest" => {
                let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
                let reach = args.get("reach").and_then(Json::as_f64).unwrap_or(4.0) as f32;
                let place = voxear_engine::nav_agent::nearest_place(&mut self.game.engine.world, at, reach);
                done(json!({ "place": place.map(|place| place.to_array()) }))
            }
            "navigation.agents" => self.navigation_agents(),
            "nav_agent.set_destination" => {
                let entity = self.obj_arg(args)?;
                let to = opt_vector(args, "to")?.map(Vec3::from_array);
                let agent = self.game.engine.world.get_mut::<voxear_engine::nav_agent::NavAgent>(entity);
                let agent = agent.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Nav Agent"))?;
                (agent.destination, agent.stopped) = (to, false);
                done(json!({}))
            }
            "reflection_probe.capture" => {
                let entity = self.obj_arg(args)?;
                let probe = self.game.engine.world.get_mut::<voxear_render::ReflectionProbe>(entity);
                let probe = probe.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Reflection Probe"))?;
                probe.capture();
                done(json!({}))
            }
            "navigation.bake" => {
                self.refuse_while_playing("bake the navigation")?;
                if self.game.levels.current().is_none() {
                    return Err(refuse(ErrorKind::Failed, "no level is open"));
                }
                self.game.engine.world.remove_resource::<voxear_engine::navigation::LevelNav>();
                if !self.is_baking_nav() {
                    self.bake_nav();
                }
                Ok(Outcome::Later(Waiting::NavBake))
            }
            "lighting.bake" => {
                self.refuse_while_playing("bake the lighting")?;
                if self.game.levels.current().is_none() {
                    return Err(refuse(ErrorKind::Failed, "no level is open"));
                }
                if !self.baking_lighting() {
                    self.bake_lighting(renderer);
                }
                match self.lighting_bake_end() {
                    Some(Err(error)) if !self.baking_lighting() => Err(refuse(ErrorKind::Failed, error.clone())),
                    _ => Ok(Outcome::Later(Waiting::LightingBake)),
                }
            }
            "lighting.bake_cancel" => {
                let baking = self.baking_lighting();
                if baking {
                    self.cancel_lighting_bake(renderer);
                }
                done(json!({ "cancelled": baking }))
            }
            "lighting.bake_status" => {
                let last = match self.lighting_bake_end() {
                    Some(Ok(baked)) => json!({
                        "file": baked.file, "bytes": baked.bytes, "parts": baked.parts, "lightmaps": baked.lightmaps,
                        "probes": baked.probes, "texels": baked.texels, "seconds": baked.seconds,
                        "tracing_seconds": baked.tracing_seconds,
                    }),
                    Some(Err(error)) => json!({ "error": error }),
                    None => Json::Null,
                };
                let world = &self.game.engine.world;
                let loaded = world.resource::<voxear_render::BakedLighting>();
                done(json!({
                    "baking": self.baking_lighting(),
                    "progress": self.lighting_bake_fraction(renderer),
                    "last": last,
                    "lightmaps": loaded.map_or(0, |baked| baked.atlases.len()),
                    "probes": loaded.map_or(0, |baked| baked.probes.positions.len()),
                }))
            }
            "lighting.probe_light" => {
                let at = opt_vector(args, "position")?.ok_or_else(|| bad("`position` is three numbers"))?;
                let at = voxear_core::math::Vec3::from(at);
                let normal = opt_vector(args, "normal")?.map_or(voxear_core::math::Vec3::Y, voxear_core::math::Vec3::from);
                let baked = self.game.engine.world.resource::<voxear_render::BakedLighting>();
                let light = baked.and_then(|baked| baked.probes.light_at(at));
                done(match light {
                    Some(light) => json!({
                        "found": true,
                        "light": light.irradiance(normal).to_array(),
                        "average": light.average().to_array(),
                        "harmonics": light.0,
                    }),
                    None => json!({ "found": false }),
                })
            }
            "reflection_probes.bake" => {
                self.refuse_while_playing("bake the reflection probes")?;
                if self.game.levels.current().is_none() {
                    return Err(refuse(ErrorKind::Failed, "no level is open"));
                }
                if !self.baking_probes() {
                    self.bake_probes();
                }
                match self.probe_bake_end() {
                    Some(Err(error)) if !self.baking_probes() => Err(refuse(ErrorKind::Failed, error.clone())),
                    _ => Ok(Outcome::Later(Waiting::ProbeBake)),
                }
            }
            "reverb_probes.bake" => {
                self.refuse_while_playing("bake the reverb probes")?;
                if self.game.levels.current().is_none() {
                    return Err(refuse(ErrorKind::Failed, "no level is open"));
                }
                if !self.baking_reverb() {
                    self.bake_reverb();
                }
                match self.reverb_bake_end() {
                    Some(Err(error)) if !self.baking_reverb() => Err(refuse(ErrorKind::Failed, error.clone())),
                    _ => Ok(Outcome::Later(Waiting::ReverbBake)),
                }
            }
            "audio.rays" => self.audio_rays(args),
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
            "box_model.sheet_save" | "box_model.sheet_load" => self.box_model_sheet(call, args, renderer),
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
            "profiler.frames" => {
                let after = args.get("after").and_then(Json::as_u64);
                let limit = args.get("limit").and_then(Json::as_u64);
                let limit = limit.map_or(voxear_engine::profile_frames::READ, |n| n as usize);
                done(self.profiler.read_frames(after, limit))
            }
            "capture" => self.capture(args),
            "scripts.build" => {
                self.rebuild_scripts = true;
                self.link.scripts_failed = None;
                let from = self.console.log().lock().map_or(0, |log| log.total);
                Ok(Outcome::Later(Waiting::Scripts { from, began: false }))
            }
            "lighting.get" => self.lighting_get(renderer),
            "lighting.set" => self.lighting_set(args, renderer),
            "shader_pack.list" => self.shader_pack_list(),
            "shader_pack.wear" => self.shader_pack_wear(args),
            "shader_pack.options" => self.shader_pack_options(args),
            "settings.get" => done(self.settings_json(args)),
            "settings.set" => self.settings_set(args),
            "settings.show" => {
                let key = need_text(args, "key")?;
                if !self.show_setting(key) {
                    return Err(bad(format!(
                        "no setting has the key `{key}`; `settings.get` and `project_settings.get` list them"
                    )));
                }
                done(json!({}))
            }
            "project_settings.get" => done(self.project_settings_json(args)),
            "project.get" => {
                let text = std::fs::read_to_string(self.game.project.root.join(voxear_engine::PROJECT_FILE))
                    .map_err(|error| refuse(ErrorKind::Failed, format!("reading voxear.toml: {error}")))?;
                done(json!({ "project": link::toml_json(&text) }))
            }
            "world_quality.get" => done(self.world_quality_json()),
            "world_quality.set" => self.world_quality_set(args),
            "project.set" => self.project_set(args),
            "docs.search" => done(self.docs_search(args)),
            "docs.read" => self.docs_read(need_text(args, "reference")?),
            "layout.use" => {
                let name = need_text(args, "name")?;
                match self.use_layout(name) {
                    true => done(json!({ "layout": self.layout_name() })),
                    false => Err(bad(format!("no layout is called `{name}`; the Layout menu lists them"))),
                }
            }
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
            Waiting::ReverbBake => match (self.baking_reverb(), self.reverb_bake_end()) {
                (false, Some(Ok(baked))) => done(json!({
                    "probes": baked.probes, "inside": baked.inside, "file": baked.file, "bytes": baked.bytes
                })),
                (false, Some(Err(error))) => Err(refuse(ErrorKind::Failed, error.clone())),
                (false, None) => Err(refuse(ErrorKind::Failed, "the bake stopped")),
                (true, _) => Ok(Outcome::Later(Waiting::ReverbBake)),
            },
            Waiting::NavBake if self.is_baking_nav() => Ok(Outcome::Later(Waiting::NavBake)),
            Waiting::NavBake => {
                let level = self.game.levels.current().unwrap_or_default().to_string();
                match self.game.engine.world.resource::<voxear_engine::navigation::LevelNav>() {
                    Some(nav) => done(json!({
                        "path": voxear_engine::navigation::nav_file(&level),
                        "polygons": nav.0.counts().0,
                    })),
                    None => Err(refuse(ErrorKind::Failed, "the level has no still colliders to bake navigation from")),
                }
            }
            Waiting::LightingBake => match (self.baking_lighting(), self.lighting_bake_end()) {
                (false, Some(Ok(baked))) => done(json!({
                    "file": baked.file, "bytes": baked.bytes, "parts": baked.parts, "lightmaps": baked.lightmaps,
                    "probes": baked.probes, "texels": baked.texels, "seconds": baked.seconds,
                        "tracing_seconds": baked.tracing_seconds,
                })),
                (false, Some(Err(error))) => Err(refuse(ErrorKind::Failed, error.clone())),
                (false, None) => Err(refuse(ErrorKind::Failed, "the bake stopped")),
                (true, _) => Ok(Outcome::Later(Waiting::LightingBake)),
            },
            Waiting::ProbeBake => match (self.baking_probes(), self.probe_bake_end()) {
                (false, Some(Ok(baked))) => {
                    done(json!({ "probes": baked.probes, "file": baked.file, "bytes": baked.bytes }))
                }
                (false, Some(Err(error))) => Err(refuse(ErrorKind::Failed, error.clone())),
                (false, None) => Err(refuse(ErrorKind::Failed, "the bake stopped")),
                (true, _) => Ok(Outcome::Later(Waiting::ProbeBake)),
            },
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

    /// Ray-traced audio in the running game: what `audio.rays` sets, and
    /// what it does now.
    fn audio_rays(&mut self, args: &Json) -> Result<Outcome, LinkError> {
        use voxear_engine::audio::ReverbTimes;
        use voxear_engine::sound_tracing::RayQualityName;
        let tracer = &mut self.game.audio.tracer;
        let mut settings = tracer.settings();
        let mut changed = false;
        if let Some(on) = opt_bool(args, "on") {
            settings.on = on;
            changed = true;
        }
        if let Some(quality) = opt_text(args, "quality") {
            let named: RayQualityName = serde_json::from_value(json!(quality))
                .map_err(|_| bad(format!("`{quality}` is not a quality: low, medium or high")))?;
            settings.quality = named.into();
            changed = true;
        }
        if let Some(budget) = args.get("budget").filter(|b| !b.is_null()) {
            let budget = budget.as_f64().ok_or_else(|| bad("`budget` is a number of milliseconds"))?;
            settings.budget = (budget as f32).max(0.0);
            changed = true;
        }
        if changed {
            self.refuse_unless_playing("trace sound for")?;
            self.game.audio.tracer.script = Some(settings);
        }
        let tracer = &self.game.audio.tracer;
        let settings = tracer.settings();
        let room = match self.game.audio.reverb_on() {
            true => self.game.audio.room(),
            false => ReverbTimes::NONE,
        };
        done(json!({
            "on": settings.on,
            "quality": voxear_engine::sound_tracing::RayQualityName::from(settings.quality),
            "budget": settings.budget,
            "tracing": tracer.tracing(),
            "rays": tracer.rays(),
            "cost_ms": tracer.cost(),
            "room": room.0,
        }))
    }

    pub(super) fn refuse_while_playing(&self, what: &str) -> Result<(), LinkError> {
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
    pub(super) fn record_change(&mut self, entity: Entity, before: ObjState) -> bool {
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
            "dotnet_started": voxear_scripting::dotnet_started(),
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

    fn obj_get(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let types = self.inspector.types().clone();
        let parts = props::parts(&self.game.engine.world, entity);
        let named: Vec<Vec<(String, props::Prop)>> =
            parts.iter().map(|part| link::named_props(&mut self.game, &types, entity, part)).collect();
        let (game, types) = (&self.game, &types);
        let world = &game.engine.world;
        let state = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let components: Vec<Json> = parts
            .iter()
            .zip(&named)
            .map(|(part, named)| {
                let fields: Vec<Json> = named
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
        // A kind that stands on the ground, made where nothing says, goes on the ground in the middle of the view.
        let standing = opt_text(args, "kind").and_then(|kind| {
            NEW_OBJS
                .iter()
                .position(|(label, name, ..)| label.eq_ignore_ascii_case(kind) || name.eq_ignore_ascii_case(kind))
        });
        let position = match opt_vector(args, "position")? {
            Some(at) => Vec3::from_array(at),
            None if parent.is_some() => Vec3::ZERO,
            None if standing.is_some_and(objs::stands_on_ground) => self.new_obj_on_ground(renderer),
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
        if opt_bool(args, "on_ground") == Some(true)
            && let Some(at) = self.grounded(renderer, made)
            && let Some(transform) = self.game.engine.world.get_mut::<voxear_scene::Transform>(made)
        {
            transform.position = at;
        }
        self.record(Edit::Exists { root: made, taken: None });
        self.shown_again();
        done(json!({ "obj": obj_ref(&self.game, made) }))
    }

    /// Where, from its parent, the Obj `entity` stands on the ground under
    /// it (see `Editor::on_ground`), passing through itself and what is
    /// under it.
    fn grounded(&self, renderer: &Renderer, entity: Entity) -> Option<Vec3> {
        let world = &self.game.engine.world;
        let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        let ground = self.on_ground(renderer, at, &objs::family(world, entity));
        let parent = world.get::<voxear_scene::Parent>(entity).map(|parent| parent.0);
        let parent =
            parent.map_or(voxear_core::math::Mat4::IDENTITY, |parent| voxear_scene::world_matrix(world, parent));
        let local = world.get::<voxear_scene::Transform>(entity)?.position;
        Some(local + parent.inverse().transform_vector3(ground - at))
    }

    fn obj_set(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = self.obj_arg(args)?;
        // Stood on the ground where it would be, once moved.
        let moved = opt_vector(args, "position")?;
        let grounded = match opt_bool(args, "on_ground") == Some(true) {
            true => {
                let world = &mut self.game.engine.world;
                let was = world.get::<voxear_scene::Transform>(entity).map(|transform| transform.position);
                if let (Some(at), Some(transform)) = (moved, world.get_mut::<voxear_scene::Transform>(entity)) {
                    transform.position = Vec3::from_array(at);
                }
                let grounded = self.grounded(renderer, entity);
                if let (Some(was), Some(transform)) =
                    (was, self.game.engine.world.get_mut::<voxear_scene::Transform>(entity))
                {
                    transform.position = was;
                }
                grounded
            }
            false => None,
        };
        let world = &mut self.game.engine.world;
        let before = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let mut after = before.clone();
        if let Some(name) = opt_text(args, "name") {
            after.name = name.to_string();
        }
        if let Some(at) = moved {
            after.transform.position = Vec3::from_array(at);
        }
        if let Some(at) = grounded {
            after.transform.position = at;
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
        let prop = link::resolve_prop(&mut self.game, &types, entity, &part, need_text(args, "field")?)?;
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
        let prop = link::resolve_prop(&mut self.game, &types, entity, &part, field)?;
        let now = props::get(&self.game, &types, entity, &prop)
            .ok_or_else(|| refuse(ErrorKind::NoSuchField, format!("`{field}` has no value to set")))?;
        let given = args.get("value").unwrap_or(&Json::Null);
        // A reference is first looked for in the field's menu, by the value `field.choices` gives or its label: so
        // one set by a key, as an input action, is set as it is listed.
        let offered = match now {
            // None as the menu has it, which for a field set by a key is not an Obj's or a file's none.
            Value::Obj(_) | Value::Asset(_) if given.is_null() => props::choices(&self.game, &types, entity, &prop)
                .into_iter()
                .map(|(_, pick)| pick)
                .find(|pick| link::pick_json(&self.game, pick).is_none()),
            Value::Obj(_) | Value::Asset(_) => link::offered_pick(&self.game, &types, entity, &prop, given),
            _ => None,
        };
        let setting = match offered {
            Some(pick) => Setting::Pick(pick),
            None => link::read_value(&self.game, &now, given)?,
        };
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

    fn field_choices(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let part = link::part_on(&self.game, entity, need_text(args, "component")?)?;
        let types = self.inspector.types().clone();
        let prop = link::resolve_prop(&mut self.game, &types, entity, &part, need_text(args, "field")?)?;
        let types = &types;
        if let Some(Value::Choice(_, options)) = props::get(&self.game, types, entity, &prop) {
            let choices: Vec<Json> = options.iter().map(|option| json!({ "label": option, "value": option })).collect();
            return done(json!({ "choices": choices }));
        }
        let choices: Vec<Json> = props::choices(&self.game, types, entity, &prop)
            .into_iter()
            .map(|(label, pick)| json!({ "label": label, "value": link::pick_json(&self.game, &pick) }))
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
                "batch" | "play" | "step" | "capture" | "scripts.build" | "editor.wait" | "editor.quit"
                | "reflection_probes.bake" | "reverb_probes.bake" | "lighting.bake" | "navigation.bake" => {
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

    /// A box model's sheet written out as a picture, or painted from one.
    fn box_model_sheet(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        let (_, path, _) = link::resolve_project_asset(&self.game, need(args, "asset")?)?;
        if !voxear_render::is_box_model(&path) {
            return Err(bad(format!("{path} is not a box model")));
        }
        let text = std::fs::read_to_string(self.game.project.root.join(&path)).map_err(|error| failed(error.into()))?;
        let mut model = voxear_render::BoxModel::parse(&text).map_err(failed)?;
        if call == "box_model.sheet_save" {
            let saved = self.save_box_sheet(&path, &model.sheet, renderer).map_err(failed)?;
            return done(json!({ "picture": link::asset_ref(&self.game, &saved) }));
        }
        let (_, picture, _) = link::resolve_project_asset(&self.game, need(args, "picture")?)?;
        model.sheet = self.read_sheet_picture(&picture).map_err(failed)?;
        let size = [model.sheet.width, model.sheet.height];
        self.save_box_model(&path, &model, renderer);
        // The Model window, open on it, shows it as saved.
        self.model_file_changed(&path);
        done(json!({ "asset": link::asset_ref(&self.game, &path), "sheet": size }))
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

    fn view_json(&self) -> Json {
        let Some(camera) = self.game.engine.world.get::<voxear_scene::Transform>(self.scene.camera) else {
            return json!({});
        };
        let (y, x, z) = camera.rotation.to_euler(EulerRot::YXZ);
        json!({
            "position": camera.position.to_array(),
            "rotation": [x.to_degrees(), y.to_degrees(), z.to_degrees()],
            "forward": (camera.rotation * Vec3::Z).to_array(),
        })
    }

    fn view_set(&mut self, args: &Json) -> Called {
        let (position, rotation, look_at) =
            (opt_vector(args, "position")?, opt_vector(args, "rotation")?, opt_vector(args, "look_at")?);
        if rotation.is_some() && look_at.is_some() {
            return Err(bad("give `rotation` or `look_at`, not both"));
        }
        self.glide = None;
        let camera = self.scene.camera;
        let Some(transform) = self.game.engine.world.get_mut::<voxear_scene::Transform>(camera) else {
            return Err(refuse(ErrorKind::Failed, "the Scene view has no camera"));
        };
        if let Some(at) = position {
            transform.position = Vec3::from_array(at);
        }
        if let Some([x, y, z]) = rotation {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians());
        }
        // The camera looks along its +Z: turned about Y to face the point, then tipped up or down to it.
        let towards = look_at.map(|at| (Vec3::from_array(at) - transform.position).normalize_or_zero());
        if let Some(towards) = towards.filter(|towards| *towards != Vec3::ZERO) {
            let (yaw, pitch) = (towards.x.atan2(towards.z), -towards.y.clamp(-1.0, 1.0).asin());
            transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        }
        done(self.view_json())
    }

    /// The Block Worlds a blocks call works in, each with its corner and
    /// its blocks' size: `obj`'s, or every one.
    fn block_worlds(&self, args: &Json) -> Result<Vec<(Entity, Vec3, f32)>, LinkError> {
        use voxear_terrain::blocks::BlockWorld;
        let world = &self.game.engine.world;
        // From the hierarchy as it is now: while editing, world transforms are not kept up.
        let corner = |entity| voxear_scene::world_matrix(world, entity).w_axis.truncate();
        match args.get("obj").filter(|obj| !obj.is_null()) {
            Some(obj) => {
                let entity = link::resolve_obj(&self.game, obj)?;
                let blocks = world
                    .get::<BlockWorld>(entity)
                    .ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Block World"))?;
                Ok(vec![(entity, corner(entity), blocks.block_size)])
            }
            None => Ok(world
                .query_ref::<(Entity, &BlockWorld)>()
                .without::<EditorOnly>()
                .into_iter()
                .map(|(entity, blocks)| (entity, corner(entity), blocks.block_size))
                .collect()),
        }
    }

    fn dungeon_starter(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = link::resolve_obj(&self.game, need(args, "obj")?)?;
        let generated = self.game.engine.world.get::<voxear_engine::blocks::BlockWorldAsset>(entity);
        if !generated.is_some_and(|asset| asset.file.generate.is_some()) {
            return Err(refuse(ErrorKind::Failed, "dungeons are built by a generator: set the world's Ground to Generated"));
        }
        let before = crate::history::ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let (changed, structure) = crate::starter_dungeon::add(&mut self.game, Some(renderer), entity)
            .map_err(|error| refuse(ErrorKind::Failed, format!("the starter dungeon was not made: {error:#}")))?;
        if changed {
            self.record_change(entity, before);
        }
        done(json!({ "structure": link::asset_ref(&self.game, &structure), "changed": changed }))
    }

    fn dungeon_layouts(&mut self, args: &Json) -> Called {
        let near = match opt_vector(args, "near")? {
            Some(near) => near,
            None => self.game.engine.world.get::<voxear_scene::Transform>(self.scene.camera).map_or([0.0; 3], |camera| camera.position.to_array()),
        };
        let reach = args.get("reach").and_then(Json::as_f64).unwrap_or(500.0) as f32;
        let obj = match args.get("obj").filter(|obj| !obj.is_null()) {
            Some(obj) => link::resolve_obj(&self.game, obj)?.to_bits(),
            None => 0,
        };
        let world = &self.game.engine.world;
        let text = voxear_engine::dungeons::dungeons_json(world, &mut self.game.content, obj, [near[0], near[1], near[2], reach])
            .map_err(|error| refuse(ErrorKind::Failed, format!("{error:#}")))?;
        let answer: Json = serde_json::from_str(&text).map_err(|error| refuse(ErrorKind::Failed, error.to_string()))?;
        done(answer)
    }

    fn navigation_status(&mut self) -> Called {
        use voxear_engine::{block_mob::BlockMob, nav_agent::NavAgent, navigation::NavWorlds};
        let world = &self.game.engine.world;
        let worlds: Vec<Json> = world.resource::<NavWorlds>().map_or_else(Vec::new, |navs| {
            navs.worlds()
                .map(|(entity, builder)| {
                    let nav = builder.nav();
                    let (chunks, places) = nav.counts();
                    json!({
                        "world": link::obj_ref(&self.game, entity),
                        "chunks": chunks,
                        "places": places,
                        "kilobytes": (nav.bytes() as f64 / 1024.0 * 10.0).round() / 10.0,
                        "version": nav.version(),
                        "made": builder.made,
                        "build_ms": (builder.spent.as_secs_f64() * 1000.0 * 10.0).round() / 10.0,
                        "settled": builder.is_settled(),
                    })
                })
                .collect()
        });
        let agents = world.query_ref::<&NavAgent>().into_iter().count();
        let mobs = world.query_ref::<&BlockMob>().into_iter().count();
        done(json!({ "worlds": worlds, "agents": agents, "mobs": mobs }))
    }

    fn navigation_path(&mut self, args: &Json) -> Called {
        let from = Vec3::from_array(opt_vector(args, "from")?.ok_or_else(|| bad("`from` is missing"))?);
        let to = Vec3::from_array(opt_vector(args, "to")?.ok_or_else(|| bad("`to` is missing"))?);
        let height = args.get("height").and_then(Json::as_f64).unwrap_or(1.8) as f32;
        let began = std::time::Instant::now();
        let (status, corners, cost) = voxear_engine::nav_agent::find_way(&mut self.game.engine.world, from, to, height);
        let ms = began.elapsed().as_secs_f64() * 1000.0;
        let length: f32 = corners.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        done(json!({
            "status": format!("{status:?}"),
            "corners": corners.iter().map(|corner| corner.to_array()).collect::<Vec<_>>(),
            "cost": cost,
            "length": length,
            "ms": (ms * 1000.0).round() / 1000.0,
        }))
    }

    fn navigation_agents(&mut self) -> Called {
        use voxear_engine::{
            block_mob::{BlockMob, BlockMobState},
            nav_agent::{NavAgent, NavAgentState},
        };
        let world = &self.game.engine.world;
        let mut agents = Vec::new();
        let walkers: Vec<Entity> = world
            .query_ref::<(Entity, &voxear_scene::Transform)>()
            .into_iter()
            .filter(|(entity, _)| world.has::<NavAgent>(*entity) || world.has::<BlockMob>(*entity))
            .map(|(entity, _)| entity)
            .collect();
        for entity in walkers {
            let at = world.get::<voxear_scene::Transform>(entity).map_or(Vec3::ZERO, |transform| transform.position);
            let state = world.get::<NavAgentState>(entity);
            let destination = match world.get::<NavAgent>(entity) {
                Some(agent) => agent.destination,
                None => world.get::<BlockMobState>(entity).and_then(|state| state.destination),
            };
            let arrived = match (world.get::<NavAgent>(entity), state) {
                (Some(agent), Some(state)) => state.is_arrived(agent),
                (None, _) => world.get::<BlockMob>(entity).is_some_and(|mob| mob.arrived),
                (Some(agent), None) => agent.destination.is_none(),
            };
            agents.push(json!({
                "obj": link::obj_ref(&self.game, entity),
                "at": at.to_array(),
                "destination": destination.map(|to| to.to_array()),
                "arrived": arrived,
                "status": state.map_or("None".to_string(), |state| format!("{:?}", state.status)),
                "remaining": state.map_or(0.0, |state| state.remaining(at)),
                "velocity": state.map_or([0.0; 3], |state| state.velocity.to_array()),
                "corners": state.map_or_else(Vec::new, |state| state.corners.iter().map(|c| c.to_array()).collect()),
            }));
        }
        done(json!({ "agents": agents }))
    }

    fn blocks_get(&self, args: &Json) -> Called {
        use voxear_terrain::blocks::{BlockWorld, chunk::AIR, light};
        let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
        let given = args.get("obj").is_some_and(|obj| !obj.is_null());
        for (entity, corner, size) in self.block_worlds(args)? {
            let Some(blocks) = self.game.engine.world.get::<BlockWorld>(entity) else { continue };
            let cell = ((at - corner) / size).floor().as_ivec3().to_array();
            let block = blocks.blocks.get(cell);
            if block == AIR && !given {
                continue;
            }
            let named = |block| {
                let guid = voxear_engine::blocks::block_guid(&self.game.engine.world, entity, block);
                let path = guid.and_then(|guid| guid.parse().ok()).and_then(|guid| self.game.content.db().get(guid));
                path.map(|entry| link::asset_ref(&self.game, &entry.path))
            };
            // And what it is drawn as, where that is another block: grass under a block shows as dirt.
            let shown = blocks.shown(cell);
            let shows = if shown == block { Json::Null } else { json!(named(shown)) };
            // Its light, a world of cubes': its sky light and its block light, each from 0 to 15.
            let light = (!blocks.smooth).then(|| blocks.light.get(cell));
            let (sky, glow) = (light.map(light::sky_of), light.map(light::glow_of));
            let answer = json!({
                "obj": obj_ref(&self.game, entity), "cell": cell, "block": named(block), "shows": shows,
                "sky_light": sky, "block_light": glow,
            });
            return done(answer);
        }
        done(json!({ "obj": null, "cell": null, "block": null, "shows": null, "sky_light": null, "block_light": null }))
    }

    fn blocks_set(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        use voxear_terrain::blocks::{BlockWorld, chunk::AIR};
        /// The most blocks one call changes, as a script's Blocks.Fill.
        const MOST: i64 = 4_000_000;
        let block = match args.get("block") {
            None => return Err(bad("`block` is missing: a .block, or null for air")),
            Some(Json::Null) => None,
            Some(block) => Some(link::resolve_asset(&self.game, block)?),
        };
        let (from, to) = match call {
            "blocks.fill" => (opt_vector(args, "from")?, opt_vector(args, "to")?),
            _ => (opt_vector(args, "at")?, opt_vector(args, "at")?),
        };
        let (Some(from), Some(to)) = (from, to) else { return Err(bad("the points are missing")) };
        let (from, to) = (Vec3::from_array(from), Vec3::from_array(to));
        let mut changed = 0;
        for (entity, corner, size) in self.block_worlds(args)? {
            let cell = |p: Vec3| ((p - corner) / size).floor().as_ivec3();
            let (low, high) = (cell(from.min(to)), cell(from.max(to)));
            let count = (high - low + 1).as_i64vec3().element_product().max(0);
            if count > MOST {
                return Err(bad(format!("{count} blocks are too many at once; the most is {MOST}")));
            }
            let number = match &block {
                None => AIR,
                Some(path) => {
                    let game = &mut self.game;
                    voxear_engine::blocks::block_number(&mut game.engine.world, &mut game.content, entity, path, true)
                        .ok_or_else(|| refuse(ErrorKind::WrongValue, format!("{path} is not a block this world can hold")))?
                }
            };
            let cells: Vec<([i32; 3], u16)> = (low.z..=high.z)
                .flat_map(|z| (low.y..=high.y).flat_map(move |y| (low.x..=high.x).map(move |x| ([x, y, z], number))))
                .collect();
            let world = &mut self.game.engine.world;
            let Some(before) = ObjState::of(world, entity) else { continue };
            let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { continue };
            let count = blocks.set_blocks(cells);
            if count > 0 {
                world.insert(entity, blocks);
                changed += count;
                self.record_change(entity, before);
            }
        }
        // A block a world did not hold is drawn as its file says from this frame.
        voxear_engine::blocks::refresh(&mut self.game.engine.world, &mut self.game.content, Some(renderer));
        done(json!({ "changed": changed }))
    }

    /// What making worlds of blocks spends now, whether this machine keeps
    /// its own, and the project's.
    fn world_quality_json(&self) -> Json {
        let world = &self.game.engine.world;
        let mut now = serde_json::to_value(voxear_terrain::WorldBudget::of(world)).unwrap_or_default();
        now["machine"] = json!(voxear_terrain::MachineBudget::of(world).kept);
        now["project"] = serde_json::to_value(self.game.project.file.world).unwrap_or_default();
        now
    }

    fn world_quality_set(&mut self, args: &Json) -> Called {
        use voxear_terrain::{WorldBudget, WorldQuality};
        let quality = match opt_text(args, "quality") {
            Some(name) => Some(
                WorldQuality::ALL
                    .into_iter()
                    .find(|quality| quality.label().eq_ignore_ascii_case(name))
                    .ok_or_else(|| bad(format!("no world quality is named `{name}`")))?,
            ),
            None => None,
        };
        let number = |name: &str| -> Result<Option<f32>, LinkError> {
            match args.get(name) {
                None | Some(Json::Null) => Ok(None),
                Some(value) => match value.as_f64() {
                    Some(n) if n.is_finite() && n >= 0.0 => Ok(Some(n as f32)),
                    _ => Err(bad(format!("`{name}` is a number, nothing or more"))),
                },
            }
        };
        let numbers = [
            "threads",
            "columns",
            "upload",
            "view_distance",
            "far_distance",
            "near_detail",
            "far_detail",
            "plant_detail",
            "far_block_pixels",
        ]
        .map(number);
        let mut given = [None; 9];
        for (at, read) in numbers.into_iter().enumerate() {
            given[at] = read?;
        }
        // Whose it is first: this machine's own kept or forgotten, then the numbers go there.
        if let Some(keep) = opt_bool(args, "machine") {
            self.keep_world_budget(keep);
        }
        let mut budget = WorldBudget::of(&self.game.engine.world);
        if let Some(preset) = quality {
            budget = preset.budget().unwrap_or(WorldBudget { quality: WorldQuality::Custom, ..budget });
        }
        if given.iter().any(Option::is_some) {
            let [threads, columns, upload, view, far, near_detail, far_detail, plant_detail, far_block_pixels] = given;
            let whole = |n: Option<f32>, was: u32| n.map_or(was, |n| n.round() as u32);
            budget = WorldBudget {
                quality: WorldQuality::Custom,
                threads: whole(threads, budget.threads),
                columns: whole(columns, budget.columns),
                upload: whole(upload, budget.upload),
                view_distance: view.unwrap_or(budget.view_distance),
                far_distance: far.unwrap_or(budget.far_distance),
                near_detail: near_detail.unwrap_or(budget.near_detail),
                far_detail: far_detail.unwrap_or(budget.far_detail),
                plant_detail: plant_detail.unwrap_or(budget.plant_detail),
                far_block_pixels: far_block_pixels.unwrap_or(budget.far_block_pixels),
            };
        }
        if quality.is_some() || given.iter().any(Option::is_some) {
            self.set_world_budget(&budget.checked());
        }
        done(self.world_quality_json())
    }

    /// Preferences' settings, as `settings.get` lists them: every one, or
    /// a category's, or those a search finds.
    fn settings_json(&self, args: &Json) -> Json {
        let (settings, defaults) = (&self.memory.settings, Settings::default());
        let value = |key: &str| (settings.value(key).unwrap_or(Json::Null), defaults.value(key).unwrap_or(Json::Null));
        listed_settings(settings::PREFERENCES, settings::CATEGORIES, args, &value)
    }

    /// Project Settings' settings, as `project_settings.get` lists them.
    fn project_settings_json(&self, args: &Json) -> Json {
        let values = project_settings::values(&self.game);
        let value = |key: &str| values.get(key).cloned().unwrap_or((Json::Null, Json::Null));
        listed_settings(project_settings::settings(), project_settings::CATEGORIES, args, &value)
    }

    fn settings_set(&mut self, args: &Json) -> Called {
        let key = need_text(args, "key")?;
        let info = settings::info(key)
            .ok_or_else(|| bad(format!("no setting has the key `{key}`; `settings.get` lists them")))?;
        // Null sets it back to its default, or for a folder, to none.
        let value = args.get("value").unwrap_or(&Json::Null);
        if value.is_null() && !info.settable() {
            return Err(refuse(ErrorKind::WrongValue, format!("{} is not changed by a value", info.label)));
        }
        let checked = if value.is_null() {
            Json::Null
        } else {
            setting_info::check(info, value).map_err(|why| refuse(ErrorKind::WrongValue, why))?
        };
        if info.kind == setting_info::Kind::Folder
            && let Some(folder) = checked.as_str()
            && !std::path::Path::new(folder).is_dir()
        {
            return Err(refuse(ErrorKind::WrongValue, format!("no folder at {folder}")));
        }
        // Tools may let themselves read only, but not give themselves leave to change, by its default either.
        let wanted =
            if checked.is_null() { Settings::default().value(key).unwrap_or(Json::Null) } else { checked.clone() };
        if key == "tools_may_change" && wanted == Json::Bool(true) && !self.memory.settings.tools_may_change {
            return Err(refuse(ErrorKind::NotAllowed, "only the person at the editor lets tools change the project"));
        }
        self.set_preference(info.key, &checked);
        tracing::info!("[Tools] settings.set {key}");
        done(json!({ "value": self.memory.settings.value(key).unwrap_or(Json::Null) }))
    }

    /// Changes one of Project Settings' settings by its key, as the window
    /// does, and puts it into effect.
    fn project_set(&mut self, args: &Json) -> Called {
        let key = need_text(args, "key")?;
        if project_settings::find(key).is_none() {
            return Err(bad(format!("no project setting has the key `{key}`; `project_settings.get` lists them")));
        }
        let value = args.get("value").unwrap_or(&Json::Null);
        let now = self.set_project_setting(key, value).map_err(|why| refuse(ErrorKind::WrongValue, why))?;
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
                let file = &self.game.project.file;
                file.portals.put(&mut self.game.engine.world);
                file.navigation.put(&mut self.game.engine.world);
                self.game.engine.world.insert_resource(file.foliage.bounded());
                file.quality.put(&mut self.game.engine.world);
                file.physics.put(&mut self.game.engine.world);
                voxear_engine::modules::put(&mut self.game.engine.world, self.game.project.modules());
                // The project's world quality, unless this machine keeps its own.
                let machine = voxear_terrain::MachineBudget::of(&self.game.engine.world);
                if !machine.kept {
                    voxear_terrain::MachineBudget::open(&mut self.game.engine.world, file.world, machine.file);
                }
                let layers = self.game.project.layers();
                self.settings_layers.show(&mut self.ui, &layers);
                // Languages and their fonts may have changed too.
                self.game.load_strings();
                self.show_language_fonts();
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

/// Settings as `settings.get` and `project_settings.get` list them: the
/// categories, and every setting with where it shows, what it is called and
/// does, what it holds and may be, its value and its default; only a
/// `category`'s, or those `search` finds, where asked.
fn listed_settings(
    settings: &'static [setting_info::Setting],
    categories: &[setting_info::Category],
    args: &Json,
    value: &dyn Fn(&str) -> (Json, Json),
) -> Json {
    let category = opt_text(args, "category");
    let found: Vec<&setting_info::Setting> = match opt_text(args, "search").filter(|query| !query.trim().is_empty()) {
        Some(query) => setting_info::search(settings, query),
        None => settings.iter().collect(),
    };
    let listed: Vec<Json> = found
        .into_iter()
        .filter(|setting| category.is_none_or(|category| setting.category.eq_ignore_ascii_case(category)))
        .map(|setting| {
            let (now, default) = value(setting.key);
            setting_info::listed(setting, &now, &default)
        })
        .collect();
    let categories: Vec<Json> =
        categories.iter().map(|c| json!({ "name": c.name, "about": c.about, "manual": c.manual })).collect();
    json!({ "categories": categories, "settings": listed })
}
