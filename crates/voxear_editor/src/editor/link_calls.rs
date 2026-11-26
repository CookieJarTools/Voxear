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
/// How long a game's call waits for the scripts to build and start.
const GAME_CALL_PATIENCE: Duration = Duration::from_secs(600);
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
    /// A game's call, waiting for the scripts to start: its name, its
    /// arguments, and when it was asked.
    GameCall {
        name: String,
        args: Json,
        asked: Instant,
    },
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
        // A game's own call by its name, as `game.calls` lists it.
        if link::call_info(call).is_none() && self.inspector.types().calls.iter().any(|known| known.name == call) {
            return self.game_call(call, args, None);
        }
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
            "inspector.mode" => self.inspector_mode(args),
            "inspector.debug" => self.inspector_debug(args, renderer),
            "components" => done(self.components_json()),
            "kinds" => done(kinds_json(&self.game.project.modules())),
            "obj.create" => self.obj_create(args, renderer),
            "obj.set" => self.obj_set(args, renderer),
            "obj.duplicate" => self.obj_duplicate(args, renderer),
            "obj.delete" => self.obj_delete(args),
            "obj.move" => self.obj_move(args),
            "obj.parent" => self.obj_parent(args),
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
            "voxels.carve" | "voxels.fill" => self.voxels_edit(call, args),
            "voxels.raycast" => self.voxels_raycast(args),
            "voxels.water_at" => self.voxels_water_at(args),
            "voxels.print" => self.voxels_print(args),
            "voxel_prints.make" => self.voxel_prints_make(args),
            "voxel_prints.info" => self.voxel_prints_info(args),
            "blocks.restore" => self.blocks_restore(args),
            "blocks.bake_far" => {
                let world = match args.get("obj").filter(|obj| !obj.is_null()) {
                    Some(obj) => Some(link::resolve_obj(&self.game, obj)?),
                    None => None,
                };
                let from = args.get("from").and_then(Json::as_u64).map_or(super::far_bakes::FAR_BAKE_FROM, |v| v as u32);
                let reach = args.get("reach").and_then(Json::as_f64).map(|v| v as f32);
                let entity = self.bake_far_view(world, from, reach).map_err(bad)?;
                done(json!({ "world": link::obj_ref(&self.game, entity), "from": from }))
            }
            "blocks.bake_far_status" => done(self.far_bake_json()),
            "blocks.bake_far_stop" => {
                if let Some(bake) = &self.far_bake {
                    bake.baking.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                done(self.far_bake_json())
            }
            "paint.map" => self.paint_map(args),
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
            "towns.near" => self.towns_near(args),
            "dungeon.open" => self.dungeon_open(args, renderer),
            "dimension.list" => done(self.dimension_list()),
            "dimension.show" => {
                let id = self.dimension_arg(args)?;
                self.show_dimension(id);
                done(json!({ "shown": self.scene_dimension().0 }))
            }
            "dimension.create" => {
                self.refuse_while_playing("make a dimension's file")?;
                self.dimension_create(args, renderer)
            }
            "dimension.make" => {
                let (guid, ..) = link::resolve_project_asset(&self.game, need(args, "dimension")?)?;
                let id = self.open_dimension(guid, renderer).map_err(|error| refuse(ErrorKind::Failed, error))?;
                done(json!({ "dimension": id.0 }))
            }
            "dimension.let_go" => {
                let id = self.dimension_arg(args)?;
                let discard = opt_bool(args, "discard").unwrap_or(false);
                self.let_go_of_dimension(id, discard).map_err(|error| refuse(ErrorKind::UnsavedConflict, error))?;
                done(json!({}))
            }
            "obj.move_to_dimension" => {
                let entity = self.obj_arg(args)?;
                let id = self.dimension_arg(args)?;
                let at = opt_vector(args, "at")?.map(Vec3::from_array);
                self.move_to_dimension(entity, id, at).map_err(|error| refuse(ErrorKind::NotAllowed, error))?;
                done(json!({ "dimension": id.0 }))
            }
            "navigation.status" => self.navigation_status(),
            "ground_fit.land" => self.ground_fit_land(args),
            "place.prefab" | "place.line_up" | "place.scatter" | "place.brush" => self.place_call(call, args, renderer),
            "road.set_points" | "road.set_mix" | "area_fill.set_outline" | "world_area.set_outline" => {
                self.road_change(call, args)
            }
            "areas.at" | "areas.value" | "route.set_points" | "route.lay_sea" | "routes.find" | "routes.nearest" => {
                self.world_route_call(call, args)
            }
            "roads.nearest" => {
                let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
                let reach = args.get("reach").and_then(Json::as_f64).unwrap_or(50.0) as f32;
                let world = &self.game.engine.world;
                let blocks = world
                    .query_ref::<(Entity, &voxear_engine::blocks::BlockWorldAsset)>()
                    .into_iter()
                    .find(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
                    .map(|(entity, _)| entity);
                let found = blocks.and_then(|blocks| voxear_engine::roads::nearest(world, blocks, at, reach, None));
                done(match found {
                    Some((road, point, along)) => {
                        json!({ "road": link::obj_ref(&self.game, road), "point": point.to_array(), "along": along })
                    }
                    None => Json::Null,
                })
            }
            "blocks.columns" => {
                let world = &self.game.engine.world;
                let moves = world.resource::<voxear_engine::ground_streaming::GroundMoves>().cloned().unwrap_or_default();
                let worlds: Vec<Json> = moves
                    .worlds
                    .iter()
                    .map(|moved| {
                        let mut near: Vec<[i32; 2]> = moved.near.iter().copied().collect();
                        near.sort_unstable();
                        json!({ "world": link::obj_ref(&self.game, moved.world), "near": near, "came": moved.came, "went": moved.went })
                    })
                    .collect();
                done(json!({ "worlds": worlds, "kept": voxear_engine::ground_streaming::kept_count(world) }))
            }
            "world_layers.status" => {
                let world = &self.game.engine.world;
                let worlds: Vec<Json> = world
                    .query_ref::<(Entity, &voxear_engine::blocks::BlockWorldAsset)>()
                    .into_iter()
                    .filter_map(|(entity, _)| Some((entity, voxear_engine::ground_fit::kept(world, entity)?)))
                    .map(|(entity, layers)| {
                        json!({ "world": link::obj_ref(&self.game, entity), "fits": layers.fits().len(), "columns": layers.columns() })
                    })
                    .collect();
                let hidden = world.resource::<voxear_engine::ground_fit::HiddenLayers>().copied().unwrap_or_default();
                done(json!({ "worlds": worlds, "hidden": { "fits": hidden.fits } }))
            }
            "scene_view.projection" => {
                let camera = self.scene.camera;
                if let Some(name) = args.get("projection").and_then(Json::as_str) {
                    let projection = voxear_render::Projection::named(name)
                        .ok_or_else(|| bad(format!("`{name}` is not a projection: `orthographic` or `perspective`")))?;
                    self.turn_scene_projection(camera, Some(projection));
                }
                let lens = self.game.engine.world.get::<voxear_render::Camera>(camera).copied().unwrap_or_default();
                done(json!({ "projection": lens.projection.name(), "size": lens.size }))
            }
            "scene_view.culling" => {
                // As the Scene view's Gizmos ▸ Game Camera's Culling, which takes hold next frame.
                if let Some(on) = opt_bool(args, "on") {
                    self.memory.settings.game_culling = on;
                }
                let camera = self.culling_camera().map(|camera| link::obj_ref(&self.game, camera));
                let stats = renderer.stats();
                done(json!({
                    "on": self.memory.settings.game_culling,
                    "camera": camera,
                    "first_view": {
                        "frustum_culled": stats.frustum_culled,
                        "occlusion_culled": stats.occlusion_culled,
                        "meshlets_occlusion_culled": stats.meshlets_occlusion_culled,
                    },
                }))
            }
            "world_layers.hide" => {
                // As the Scene view's Gizmos ▸ Bare Ground, which takes hold next frame.
                if let Some(fits) = opt_bool(args, "fits") {
                    self.memory.settings.bare_ground = fits;
                }
                done(json!({ "hidden": { "fits": self.memory.settings.bare_ground } }))
            }
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
                let into = if args.get("dimension").is_some() { Some(self.dimension_arg(args)?) } else { None };
                let agent = self.game.engine.world.get_mut::<voxear_engine::nav_agent::NavAgent>(entity);
                let agent = agent.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Nav Agent"))?;
                (agent.destination, agent.destination_dimension, agent.stopped) = (to, into, false);
                voxear_engine::nav_agent::direct(&mut self.game.engine.world, entity);
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
            "game.calls" => {
                let calls: Vec<Json> = self.inspector.types().calls.iter().map(|call| call.describe()).collect();
                done(json!({ "calls": calls }))
            }
            "game.call" => {
                let name = need_text(args, "name")?.to_string();
                let given = args.get("args").cloned().unwrap_or_else(|| json!({}));
                self.game_call(&name, &given, None)
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
            "mesh_pool.get" => done(serde_json::to_value(renderer.mesh_pool()).unwrap_or_default()),
            "crowds.get" => {
                let world = &self.game.engine.world;
                let settings = world.resource::<voxear_engine::crowds::CrowdSettings>().cloned().unwrap_or_default();
                let tiers = world.resource::<voxear_engine::crowds::CrowdTiers>().map_or([0; 3], |tiers| tiers.counts);
                let records = voxear_engine::crowd_records::count(world);
                done(json!({
                    "enabled": settings.enabled,
                    "near": settings.near,
                    "middle": settings.middle,
                    "records_beyond": settings.records_beyond,
                    "counts": { "near": tiers[0], "middle": tiers[1], "far": tiers[2], "records": records },
                    "posed": renderer.stats().posed,
                }))
            }
            "scripts.build" => {
                self.rebuild_scripts = true;
                self.link.scripts_failed = None;
                let from = self.console.log().lock().map_or(0, |log| log.total);
                Ok(Outcome::Later(Waiting::Scripts { from, began: false }))
            }
            "lighting.get" => self.lighting_get(renderer),
            "lighting.set" => self.lighting_set(args, renderer),
            "lighting.scene" => self.lighting_scene(args, renderer),
            "lighting.copy" => self.lighting_copy(renderer),
            "shader_pack.list" => self.shader_pack_list(),
            "shader_pack.wear" => self.shader_pack_wear(args),
            "shader_pack.options" => self.shader_pack_options(args),
            "look_pack.list" => self.look_pack_list(),
            "look_pack.wear" => self.look_pack_wear(args),
            "look_pack.get" => self.look_pack_get(args),
            "look_pack.set" => self.look_pack_set(args),
            "texture_pack.list" => self.texture_pack_list(),
            "texture_pack.wear" => self.texture_pack_wear(args),
            "audio.mix" => self.audio_mix(args),
            "display" => self.display(args),
            "settings.get" => done(self.settings_json(args)),
            "settings.set" => self.settings_set(args),
            "settings.find" => done(self.settings_find(args)),
            "settings.show" => {
                let key = need_text(args, "key")?;
                if !self.show_found(key) {
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
            Waiting::GameCall { name, args, asked } => self.game_call(&name, &args, Some(asked)),
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
                let world = &self.game.engine.world;
                let types = self.game.project.file.navigation.agent_types();
                let baked: Vec<Json> = voxear_engine::navigation::baked_types(world)
                    .into_iter()
                    .filter_map(|slot| {
                        let (mesh, _) = voxear_engine::navigation::level_nav_for(world, slot)?;
                        let name = types.get(usize::from(slot)).map(|kind| kind.name.clone()).unwrap_or_default();
                        Some(json!({ "agent_type": slot, "name": name, "polygons": mesh.counts().0 }))
                    })
                    .collect();
                match world.resource::<voxear_engine::navigation::LevelNav>() {
                    Some(nav) => done(json!({
                        "path": voxear_engine::navigation::nav_file(&level),
                        "polygons": nav.0.counts().0,
                        "agent_types": baked,
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

    pub(super) fn refuse_unless_playing(&self, what: &str) -> Result<(), LinkError> {
        match self.is_playing() {
            true => Ok(()),
            false => Err(refuse(ErrorKind::Failed, format!("the game is not playing, so there is nothing to {what}"))),
        }
    }

    /// Notes a change as a step to undo, unless playing, when it changed the
    /// played world only.
    pub(super) fn record(&mut self, edit: Edit) -> bool {
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
    pub(super) fn shown_again(&mut self) {
        self.refresh_hierarchies();
        let selected = self.selection.filter(|&e| self.game.engine.world.is_alive(e));
        self.show_selection(selected);
    }

    /// Every setting the editor has that a search for `query` finds, by
    /// what it does (see `setting_search`), best first.
    fn settings_find(&self, args: &Json) -> Json {
        let query = opt_text(args, "query").unwrap_or_default();
        let most = args.get("most").and_then(Json::as_u64).unwrap_or(20) as usize;
        let everything = crate::setting_search::everything(&self.game, self.inspector.types());
        let found = crate::setting_search::search(&everything, query);
        let listed = found.iter().take(most).map(|hit| {
            json!({
                "key": hit.entry.place.key(),
                "label": hit.entry.label,
                "where": hit.entry.group,
                "line": hit.line,
                "matched": hit.words,
                "score": hit.score,
                "hidden": hit.entry.hidden,
            })
        });
        json!({ "found": listed.collect::<Vec<_>>() })
    }

    /// An Inspector's mode, set where `mode` is given (see `inspector.mode`).
    fn inspector_mode(&mut self, args: &Json) -> Called {
        let number = args.get("inspector").and_then(Json::as_u64).unwrap_or(1);
        let count = self.inspector_count();
        let index = usize::try_from(number).ok().filter(|&n| (1..=count).contains(&n)).ok_or_else(|| {
            bad(format!("there are {count} Inspectors; `inspector` is one of 1 to {count}"))
        })?;
        if let Some(mode) = opt_text(args, "mode") {
            let debug = match mode.to_ascii_lowercase().as_str() {
                "debug" => true,
                "normal" => false,
                _ => return Err(bad(format!("`{mode}` is not a mode: Normal or Debug"))),
            };
            self.set_inspector_debug(index - 1, debug);
        }
        let debug = self.inspector_is_debug(index - 1);
        done(json!({ "inspector": index, "mode": if debug { "Debug" } else { "Normal" } }))
    }

    /// What Debug mode shows of an Obj or an asset (see `inspector.debug`).
    fn inspector_debug(&mut self, args: &Json, renderer: &Renderer) -> Called {
        let rows = match args.get("asset") {
            Some(asset) => crate::debug_inspector::asset_rows(&self.game, &link::resolve_asset(&self.game, asset)?),
            None => {
                let entity = match args.get("obj") {
                    Some(obj) => link::resolve_obj(&self.game, obj)?,
                    None => self.selected().ok_or_else(|| bad("nothing is selected; give `obj` or `asset`"))?,
                };
                let playing = self.is_playing();
                self.debug_rows(entity, renderer, playing)
            }
        };
        done(json!({ "rows": rows.iter().map(crate::debug_inspector::DebugRow::json).collect::<Vec<_>>() }))
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
            None => self.scene_for_new_objs(),
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

    /// Sets a Road's points or blocks, or an Area Fill's outline or blocks, as
    /// `call` says, as one step to undo.
    fn road_change(&mut self, call: &str, args: &Json) -> Called {
        use voxear_engine::roads::{AreaFill, MixBlock, Road, RoadPoint};
        let entity = self.obj_arg(args)?;
        let world = &self.game.engine.world;
        let before = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let numbers = |value: &Json| -> Option<Vec<f32>> {
            value.as_array()?.iter().map(|v| v.as_f64().map(|v| v as f32)).collect()
        };
        let answer = match call {
            "road.set_points" => {
                let road = world.get::<Road>(entity).cloned();
                let mut road = road.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Road"))?;
                let given = need(args, "points")?.as_array().ok_or_else(|| bad("`points` is not a list"))?;
                let points: Option<Vec<RoadPoint>> = given
                    .iter()
                    .map(|point| {
                        let v = numbers(point)?;
                        let width = v.get(3).copied().unwrap_or(5.0).clamp(1.0, 64.0);
                        (v.len() >= 3).then(|| RoadPoint { at: [v[0], v[1], v[2]], width })
                    })
                    .collect();
                let points = points.filter(|points| points.len() >= 2);
                let mut points = points.ok_or_else(|| bad("two points at least, each `[x, y, z, width]`"))?;
                if opt_bool(args, "land") == Some(true) {
                    let placed = voxear_scene::world_matrix(world, entity);
                    let game = &mut self.game;
                    for point in &mut points {
                        let at = placed.transform_point3(Vec3::from(point.at));
                        let (world, content) = (&mut game.engine.world, &mut game.content);
                        let ground = voxear_engine::ground_fit::ground_under(world, content, at, Some(entity));
                        let landed = Vec3::new(at.x, ground.unwrap_or(at.y), at.z);
                        point.at = placed.inverse().transform_point3(landed).to_array();
                    }
                }
                road.points = points;
                let given: Vec<[f32; 4]> = road.points.iter().map(|p| [p.at[0], p.at[1], p.at[2], p.width]).collect();
                let answer = json!({ "points": given });
                self.game.engine.world.insert(entity, road);
                answer
            }
            "road.set_mix" => {
                let given = need(args, "mix")?.as_array().ok_or_else(|| bad("`mix` is not a list"))?;
                let mut mix = Vec::new();
                for entry in given {
                    let block = entry.get("block").ok_or_else(|| bad("each of `mix` needs a `block`"))?;
                    let (guid, _, _) = link::resolve_project_asset(&self.game, block)?;
                    let weight = entry.get("weight").and_then(Json::as_u64).unwrap_or(1).min(1000) as u32;
                    mix.push(MixBlock { block: Some(guid), weight });
                }
                let pattern: Vec<String> = match args.get("pattern") {
                    Some(rows) => {
                        let rows = rows.as_array().ok_or_else(|| bad("`pattern` is not a list"))?;
                        rows.iter().filter_map(|row| row.as_str().map(str::to_string)).collect()
                    }
                    None => Vec::new(),
                };
                let world = &mut self.game.engine.world;
                if let Some(road) = world.get_mut::<Road>(entity) {
                    (road.mix, road.pattern) = (mix, pattern.clone());
                } else if let Some(area) = world.get_mut::<AreaFill>(entity) {
                    (area.mix, area.pattern) = (mix, pattern.clone());
                } else {
                    return Err(refuse(ErrorKind::NoSuchComponent, "the Obj has neither a Road nor an Area Fill"));
                }
                json!({ "mix": given, "pattern": pattern })
            }
            _ => {
                let given = need(args, "outline")?.as_array().ok_or_else(|| bad("`outline` is not a list"))?;
                let outline: Option<Vec<[f32; 2]>> =
                    given.iter().map(|corner| numbers(corner).filter(|v| v.len() == 2).map(|v| [v[0], v[1]])).collect();
                let outline = outline.filter(|outline| outline.len() >= 3);
                let outline = outline.ok_or_else(|| bad("three corners at least, each `[x, z]`"))?;
                let world = &mut self.game.engine.world;
                if call == "world_area.set_outline" {
                    let area = world.get_mut::<voxear_engine::world_areas::WorldArea>(entity);
                    let area = area.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no World Area"))?;
                    area.outline = outline.clone();
                    area.shape = voxear_engine::world_areas::AreaShape::Outline;
                } else {
                    let area = world.get_mut::<AreaFill>(entity);
                    let area = area.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Area Fill"))?;
                    area.outline = outline.clone();
                }
                json!({ "outline": outline })
            }
        };
        self.record_change(entity, before);
        done(answer)
    }

    /// Asks about the World Areas and Routes, or sets or lays a Route's
    /// points as one step to undo.
    fn world_route_call(&mut self, call: &str, args: &Json) -> Called {
        use voxear_engine::{
            world_areas::WorldArea,
            world_routes::{self, KindOf, Route, RouteKind},
        };
        let at = |key: &str| -> Result<Vec3, LinkError> {
            Ok(Vec3::from_array(opt_vector(args, key)?.ok_or_else(|| bad(format!("`{key}` is missing")))?))
        };
        let kind = match args.get("kind") {
            None | Some(Json::Null) => KindOf::of(RouteKind::Road),
            Some(Json::String(named)) if named == "road" => KindOf::of(RouteKind::Road),
            Some(Json::String(named)) if named == "sea" => KindOf::of(RouteKind::Sea),
            Some(Json::String(named)) if named == "air" => KindOf::of(RouteKind::Air),
            Some(asset) => {
                let (guid, _, _) = link::resolve_project_asset(&self.game, asset)?;
                KindOf { kind: RouteKind::Custom, custom: Some(guid) }
            }
        };
        let path_of = |game: &voxear_engine::Game, guid: Option<voxear_assets::AssetGuid>| {
            guid.and_then(|guid| game.content.db().get(guid).map(|entry| entry.path.clone()))
        };
        match call {
            "areas.at" => {
                let point = at("at")?;
                let found = voxear_engine::world_areas::at(&self.game.engine.world, &mut self.game.content, point);
                let listed: Vec<Json> = found
                    .into_iter()
                    .filter_map(|obj| {
                        let area = self.game.engine.world.get::<WorldArea>(obj)?;
                        let tags: Vec<Json> = area
                            .tags
                            .iter()
                            .map(|entry| json!({ "tag": path_of(&self.game, entry.tag), "data": path_of(&self.game, entry.data) }))
                            .collect();
                        Some(json!({ "obj": link::obj_ref(&self.game, obj), "priority": area.priority, "tags": tags }))
                    })
                    .collect();
                done(Json::Array(listed))
            }
            "areas.value" => {
                let point = at("at")?;
                let (tag, _, _) = link::resolve_project_asset(&self.game, need(args, "tag")?)?;
                let found =
                    voxear_engine::world_areas::value(&self.game.engine.world, &mut self.game.content, point, tag);
                done(found.map_or(Json::Null, |data| json!({ "data": path_of(&self.game, data) })))
            }
            "routes.find" => {
                let way = world_routes::find(&mut self.game.engine.world, at("from")?, at("to")?, kind);
                done(way.map_or(Json::Null, |way| {
                    let stops: Vec<Json> = way.stops.iter().map(|&obj| link::obj_ref(&self.game, obj)).collect();
                    let points: Vec<[f32; 3]> = way.points.iter().map(|point| point.to_array()).collect();
                    json!({ "points": points, "length": way.length, "stops": stops })
                }))
            }
            "routes.nearest" => {
                let found = world_routes::nearest(&mut self.game.engine.world, at("at")?, kind);
                done(found.map_or(Json::Null, |(route, point, along)| {
                    json!({ "route": link::obj_ref(&self.game, route), "point": point.to_array(), "along": along })
                }))
            }
            _ => {
                let entity = self.obj_arg(args)?;
                let world = &self.game.engine.world;
                let before =
                    ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
                let route = world.get::<Route>(entity).cloned();
                let mut route = route.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no Route"))?;
                let placed = voxear_scene::world_matrix(world, entity);
                let points: Vec<[f32; 3]> = if call == "route.lay_sea" {
                    let ends = route.world_points(&placed);
                    let (Some(&from), Some(&to)) = (ends.first(), ends.last()) else {
                        return Err(bad("the Route has no points"));
                    };
                    let laid =
                        world_routes::lay_sea(world, &mut self.game.content, from, to, route.draught, route.shore);
                    let laid = laid.ok_or_else(|| refuse(ErrorKind::Failed, "the sea does not join its ends"))?;
                    laid.iter().map(|&at| placed.inverse().transform_point3(at).to_array()).collect()
                } else {
                    let given = need(args, "points")?.as_array().ok_or_else(|| bad("`points` is not a list"))?;
                    let read: Option<Vec<[f32; 3]>> = given
                        .iter()
                        .map(|point| {
                            let v: Vec<f32> =
                                point.as_array()?.iter().filter_map(|v| v.as_f64().map(|v| v as f32)).collect();
                            (v.len() == 3).then(|| [v[0], v[1], v[2]])
                        })
                        .collect();
                    let mut read = read
                        .filter(|points| points.len() >= 2)
                        .ok_or_else(|| bad("two points at least, each `[x, y, z]`"))?;
                    if opt_bool(args, "land") == Some(true) {
                        for point in &mut read {
                            let at = placed.transform_point3(Vec3::from(*point));
                            let (world, content) = (&mut self.game.engine.world, &mut self.game.content);
                            let ground = voxear_engine::ground_fit::ground_under(world, content, at, Some(entity));
                            *point = placed
                                .inverse()
                                .transform_point3(Vec3::new(at.x, ground.unwrap_or(at.y), at.z))
                                .to_array();
                        }
                    }
                    read
                };
                route.points = points.clone();
                self.game.engine.world.insert(entity, route);
                let aground = world_routes::aground(&mut self.game.engine.world, &mut self.game.content, entity);
                self.record_change(entity, before);
                done(json!({ "points": points, "aground": aground }))
            }
        }
    }

    fn ground_fit_land(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let game = &mut self.game;
        let placed = game.engine.world.get::<voxear_scene::WorldTransform>(entity).map(|placed| placed.0);
        let here = placed.map_or(Vec3::ZERO, |placed| placed.w_axis.truncate());
        let at = opt_vector(args, "at")?.map_or(here, |at| Vec3::new(at[0], here.y, at[2]));
        if !game.engine.world.has::<voxear_engine::ground_fit::GroundFit>(entity) {
            return Err(refuse(ErrorKind::NoSuchComponent, "the Obj has no Ground Fit"));
        }
        let landed = voxear_engine::ground_fit::landing(&mut game.engine.world, &mut game.content, entity, at)
            .ok_or_else(|| refuse(ErrorKind::Failed, "no world of blocks without end is in the Obj's dimension"))?;
        let world = &mut self.game.engine.world;
        let before = ObjState::of(world, entity).ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let mut after = before.clone();
        // Its own place under its parent's.
        let parent = world.get::<voxear_scene::Parent>(entity).map(|parent| parent.0);
        let above = parent.and_then(|parent| world.get::<voxear_scene::WorldTransform>(parent)).map(|placed| placed.0);
        after.transform.position = above.map_or(landed, |above| above.inverse().transform_point3(landed));
        after.apply(world, entity);
        self.record_change(entity, before);
        done(json!({ "position": landed.to_array() }))
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
        let moved =
            objs::move_obj_checked(&mut self.game, entity, to).map_err(|error| refuse(ErrorKind::WrongValue, error))?;
        if let Some((before, after)) = moved.clone() {
            self.record(Edit::Moved { entity, before, after });
        }
        self.shown_again();
        done(json!({ "changed": moved.is_some() }))
    }

    fn obj_parent(&mut self, args: &Json) -> Called {
        let entity = self.obj_arg(args)?;
        let parent = args.get("parent").filter(|parent| !parent.is_null());
        let parent = parent.map(|parent| link::resolve_obj(&self.game, parent)).transpose()?;
        let keep_world = opt_bool(args, "keep_world").unwrap_or(true);
        let (changed, moved) = objs::set_parent(&mut self.game, entity, parent, keep_world)
            .map_err(|error| refuse(ErrorKind::WrongValue, error))?;
        if let Some((before, after)) = moved {
            self.record(Edit::Moved { entity, before, after });
        }
        self.shown_again();
        done(json!({ "changed": changed }))
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

    /// Makes one of the game's own calls (a `[Call]` method), its Objs as
    /// the link names them given to it by number; later, once the scripts
    /// have started, when they have not, for at most `GAME_CALL_PATIENCE`
    /// from when it was first asked, `asked` when it was waiting.
    fn game_call(&mut self, name: &str, args: &Json, waited: Option<Instant>) -> Called {
        let asked = waited.unwrap_or_else(Instant::now);
        let Some(call) = self.inspector.types().calls.iter().find(|call| call.name == name).cloned() else {
            return Err(refuse(
                ErrorKind::NoSuchCall,
                format!("the game has no call `{name}`; `game.calls` lists them"),
            ));
        };
        if !args.is_object() && !args.is_null() {
            return Err(bad("a call's arguments are a JSON object"));
        }
        if call.changes && !self.memory.settings.tools_may_change {
            return Err(refuse(
                ErrorKind::NotAllowed,
                "the editor's settings let tools read the project, not change it",
            ));
        }
        let playing = self.is_playing();
        match call.when.as_str() {
            "editing" if playing => {
                return Err(refuse(ErrorKind::RefusedWhilePlaying, format!("`{name}` is called only while editing")));
            }
            "playing" if !playing => {
                return Err(refuse(ErrorKind::Failed, format!("`{name}` is called only while the game plays")));
            }
            _ => {}
        }
        let mut given = if args.is_object() { args.clone() } else { json!({}) };
        for arg in call.args.iter().filter(|arg| arg.kind == "obj") {
            if let Some(reference) = given.get(&arg.name).filter(|value| !value.is_null()).cloned() {
                given[&arg.name] = json!(link::resolve_obj(&self.game, &reference)?.to_bits());
            }
        }
        if call.changes && waited.is_none() {
            tracing::info!("[Tools] {name} {}", short(&given));
        }
        match self.run_game_call(&call.type_name, &call.method, &given.to_string()) {
            Some(Ok(answer)) => {
                let result: Json = serde_json::from_str(&answer).unwrap_or(Json::String(answer));
                done(json!({ "result": result }))
            }
            Some(Err(error)) => Err(refuse(ErrorKind::Failed, format!("{error:#}"))),
            None if asked.elapsed() > GAME_CALL_PATIENCE => {
                Err(refuse(ErrorKind::Failed, "the scripts took too long to build and start"))
            }
            None => Ok(Outcome::Later(Waiting::GameCall { name: name.to_string(), args: args.clone(), asked })),
        }
    }

    fn towns_near(&mut self, args: &Json) -> Called {
        let near = match opt_vector(args, "near")? {
            Some(near) => near,
            None => self
                .game
                .engine
                .world
                .get::<voxear_scene::Transform>(self.scene.camera)
                .map_or([0.0; 3], |camera| camera.position.to_array()),
        };
        let reach = args.get("reach").and_then(Json::as_f64).unwrap_or(500.0) as f32;
        let obj = match args.get("obj").filter(|obj| !obj.is_null()) {
            Some(obj) => link::resolve_obj(&self.game, obj)?.to_bits(),
            None => 0,
        };
        let world = &self.game.engine.world;
        let text =
            voxear_engine::towns::towns_json(world, &mut self.game.content, obj, [near[0], near[1], near[2], reach])
                .map_err(|error| refuse(ErrorKind::Failed, format!("{error:#}")))?;
        let mut answer: Json =
            serde_json::from_str(&text).map_err(|error| refuse(ErrorKind::Failed, error.to_string()))?;
        // Each town's world as the link names Objs.
        for town in answer["towns"].as_array_mut().into_iter().flatten() {
            let world = town["world"].as_u64().map(Entity::from_bits);
            town["world"] = world.map_or(Json::Null, |world| link::obj_ref(&self.game, world));
        }
        done(answer)
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
        let mut answer: Json =
            serde_json::from_str(&text).map_err(|error| refuse(ErrorKind::Failed, error.to_string()))?;
        // A dungeon's dimension by the number `dimension.list` gives it, where scripts have their handle for it.
        let world = &self.game.engine.world;
        for dungeon in answer["dungeons"].as_array_mut().into_iter().flatten() {
            let handle = dungeon["dimension"].as_u64().and_then(|handle| u32::try_from(handle).ok());
            let id = handle.and_then(|handle| voxear_engine::dimensions::from_handle(world, handle));
            dungeon["dimension"] = id.map_or(Json::Null, |id| json!(id.0));
        }
        done(answer)
    }

    /// The made dimension a call names: by its number, or by its
    /// `.dimension` file.
    fn dimension_arg(&self, args: &Json) -> Result<voxear_engine::dimensions::DimensionId, LinkError> {
        use voxear_engine::dimensions::{DimensionId, Dimensions};
        let given = need(args, "dimension")?;
        let made = self.game.engine.world.resource::<Dimensions>();
        if let Some(number) = given.as_u64() {
            let id = DimensionId(u16::try_from(number).map_err(|_| bad(format!("{number} is no dimension's number")))?);
            if id.is_level() || made.is_some_and(|made| made.exists(id)) {
                return Ok(id);
            }
            return Err(refuse(ErrorKind::Gone, format!("no dimension {number} is made: `dimension.list` lists them")));
        }
        let (guid, path, kind) = link::resolve_project_asset(&self.game, given)?;
        if kind != AssetKind::Dimension {
            return Err(bad(format!("{path} is not a .dimension file")));
        }
        let open = self.game.levels.dimension_of_scene(guid);
        let found = open.or_else(|| made.and_then(|made| made.find(guid, 0)));
        found.ok_or_else(|| refuse(ErrorKind::Gone, format!("{path} is not made: `dimension.make` makes it")))
    }

    fn dimension_list(&self) -> Json {
        use voxear_engine::dimensions::{self, DimensionId, Dimensions};
        let world = &self.game.engine.world;
        let content = &self.game.content;
        let dimensioned = voxear_scene::Dimensioned::of(world);
        let roots = |id: DimensionId| -> Vec<Json> {
            let mut found: Vec<Entity> = world
                .query_ref::<(Entity, &Name)>()
                .without::<Parent>()
                .without::<EditorOnly>()
                .without::<voxear_scene::Destroying>()
                .into_iter()
                .map(|(entity, _)| entity)
                .filter(|&entity| dimensioned.of_obj(entity) == id)
                .collect();
            found.sort_by_key(|entity| entity.index());
            let named = |entity: Entity| world.get::<Name>(entity).map_or(String::new(), |name| name.0.clone());
            let listed = |entity: Entity| json!({ "obj": obj_ref(&self.game, entity), "name": named(entity) });
            found.into_iter().map(listed).collect()
        };
        let listed = |id: DimensionId, file: Option<voxear_assets::AssetGuid>, key: u64| {
            let settings = world.resource::<Dimensions>().map_or_else(
                || dimensions::DimensionSettings::of(dimensions::DimensionKind::Overworld),
                |made| made.settings(id),
            );
            let file = file.and_then(|guid| content.db().get(guid));
            let file = file.map(|entry| link::asset_ref(&self.game, &entry.path));
            let word = |value: &dyn std::fmt::Debug| format!("{value:?}").to_lowercase();
            let one = settings.instances() == dimensions::DimensionInstances::One;
            json!({
                "dimension": id.0, "name": dimensions::name_of(world, content, id), "file": file,
                "key": key.to_string(), "kind": word(&settings.kind), "sky": settings.sky(),
                "weather": settings.weather(), "clock": word(&settings.clock()), "keep_loaded": settings.keep_loaded(),
                "instances": if one { "one" } else { "each_site" },
                "saved": id.is_level() || self.game.levels.dimension_scene(id).is_some(),
                "objs": roots(id),
            })
        };
        let mut list = vec![listed(DimensionId::LEVEL, None, 0)];
        if let Some(made) = world.resource::<Dimensions>() {
            let mut all: Vec<&dimensions::Made> = made.made().collect();
            all.sort_by_key(|made| made.id.0);
            list.extend(all.into_iter().map(|made| listed(made.id, made.asset, made.key)));
        }
        json!({ "shown": self.scene_dimension().0, "dimensions": list })
    }

    fn dimension_create(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        use voxear_engine::dimensions::DimensionKind;
        let kind = match need_text(args, "kind")? {
            "overworld" => DimensionKind::Overworld,
            "dungeon" => DimensionKind::Dungeon,
            "caves" => DimensionKind::Caves,
            other => return Err(bad(format!("`{other}` is not a kind: overworld, dungeon or caves"))),
        };
        let folder = opt_text(args, "folder").unwrap_or("Dimensions").trim_matches('/').to_string();
        let root = self.game.project.root.clone();
        std::fs::create_dir_all(root.join(&folder)).map_err(|error| refuse(ErrorKind::Failed, error.to_string()))?;
        let path = project_files::create(&root, &folder, NewFile::Dimension(kind), "")
            .map_err(|error| refuse(ErrorKind::Failed, format!("{error:#}")))?;
        self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        done(json!({ "asset": link::asset_ref(&self.game, &path) }))
    }

    fn dungeon_open(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let given = args.get("obj").filter(|obj| !obj.is_null());
        let given = given.map(|obj| link::resolve_obj(&self.game, obj)).transpose()?;
        let world = &self.game.engine.world;
        let standing = given.and_then(|entity| world.get::<voxear_engine::dungeons::DungeonGate>(entity).copied());
        // Its site as `dungeon.layouts` gives it, or where that is, [x, z].
        let site = args.get("site").map(|site| site.get("at").unwrap_or(site));
        let gate = match (standing, site.and_then(link::numbers)) {
            (Some(gate), _) => gate,
            (None, Some(site)) if site.len() >= 2 => {
                let within = given.map_or(0, |entity| entity.to_bits());
                voxear_engine::dungeons::gate_at(world, &mut self.game.content, within, [site[0], site[site.len() - 1]])
                    .ok_or_else(|| refuse(ErrorKind::Gone, "no dungeon in a dimension of its own has its site there"))?
            }
            _ => return Err(bad("give a dungeon's `site`, [x, z] as `dungeon.layouts` gives it, or its gate as `obj`")),
        };
        let (id, back) = self.open_dungeon(gate, renderer).map_err(|error| refuse(ErrorKind::Failed, error))?;
        done(json!({ "dimension": id.0, "way_out": obj_ref(&self.game, back) }))
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
                        "model_cells": voxear_engine::walkable_models::stamped_cells(world, entity),
                    })
                })
                .collect()
        });
        let agents = world.query_ref::<&NavAgent>().into_iter().count();
        let mobs = world.query_ref::<&BlockMob>().into_iter().count();
        let (obstacles, carving) = voxear_engine::nav_obstacle::counts(world);
        let walkable = world.query_ref::<&voxear_engine::walkable_models::Walkable>().into_iter().count();
        done(json!({
            "worlds": worlds,
            "agents": agents,
            "mobs": mobs,
            "obstacles": obstacles,
            "carving": carving,
            "walkable": walkable,
        }))
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

    fn voxels_raycast(&mut self, args: &Json) -> Called {
        let from = Vec3::from_array(opt_vector(args, "from")?.ok_or_else(|| bad("`from` is missing"))?);
        let direction = Vec3::from_array(opt_vector(args, "direction")?.ok_or_else(|| bad("`direction` is missing"))?);
        let reach = args.get("reach").and_then(Json::as_f64).map_or(1000.0, |reach| reach as f32);
        let only = match args.get("obj") {
            Some(obj) if !obj.is_null() => Some(link::resolve_obj(&self.game, obj)?),
            _ => None,
        };
        let world = &self.game.engine.world;
        match voxear_engine::voxel_volumes::raycast(world, from, direction, reach, only) {
            Some((entity, hit)) => done(json!({
                "obj": obj_ref(&self.game, entity),
                "point": hit.point.to_array(),
                "normal": hit.normal.to_array(),
                "distance": hit.distance,
            })),
            None => done(json!({ "obj": null, "point": null, "normal": null, "distance": null })),
        }
    }

    fn voxels_water_at(&mut self, args: &Json) -> Called {
        let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
        let only = match args.get("obj") {
            Some(obj) if !obj.is_null() => Some(link::resolve_obj(&self.game, obj)?),
            _ => None,
        };
        match voxear_engine::voxel_volumes::water_at(&self.game.engine.world, at, only) {
            Some((entity, water)) => done(json!({
                "obj": obj_ref(&self.game, entity),
                "height": water.height,
                "current": water.current.to_array(),
            })),
            None => done(json!({ "obj": null, "height": null, "current": null })),
        }
    }

    fn voxels_edit(&mut self, call: &str, args: &Json) -> Called {
        use voxear_render::{VoxelMaterial, VoxelVolume};
        let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
        let radius = need(args, "radius")?.as_f64().ok_or_else(|| bad("`radius` is a number of metres"))? as f32;
        let fill = match call {
            "voxels.fill" => {
                let name = need(args, "material")?.as_str().unwrap_or_default();
                let material = VoxelMaterial::named(name).ok_or_else(|| {
                    refuse(
                        ErrorKind::WrongValue,
                        format!("{name} is not a ground: dirt, grass, rock, sand, snow or gravel"),
                    )
                })?;
                Some(material)
            }
            _ => None,
        };
        let only = match args.get("obj") {
            Some(obj) => Some(link::resolve_obj(&self.game, obj)?),
            None => None,
        };
        let world = &mut self.game.engine.world;
        let volumes: Vec<Entity> = world
            .query_ref::<(Entity, &VoxelVolume)>()
            .into_iter()
            .filter(|(entity, volume)| volume.endless && only.is_none_or(|only| only == *entity))
            .map(|(entity, _)| entity)
            .collect();
        let mut reached = 0;
        for entity in volumes {
            let Some(before) = ObjState::of(&self.game.engine.world, entity) else { continue };
            let world = &mut self.game.engine.world;
            if voxear_engine::voxel_volumes::edit(world, at, radius, fill, Some(entity)) > 0 {
                reached += 1;
                self.record_change(entity, before);
            }
        }
        done(json!({ "reached": reached }))
    }

    fn voxels_print(&mut self, args: &Json) -> Called {
        use voxear_render::VoxelVolume;
        let (guid, _, kind) = link::resolve_project_asset(&self.game, need(args, "print")?)?;
        if kind != AssetKind::VoxelPrint {
            return Err(refuse(ErrorKind::WrongValue, "`print` is not a .voxprint"));
        }
        let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
        let turn = args.get("turn").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let rotation = opt_vector(args, "rotation")?.unwrap_or([0.0, turn, 0.0]);
        let only = match args.get("obj") {
            Some(obj) if !obj.is_null() => Some(link::resolve_obj(&self.game, obj)?),
            _ => None,
        };
        let volumes: Vec<Entity> = self
            .game
            .engine
            .world
            .query_ref::<(Entity, &VoxelVolume)>()
            .into_iter()
            .filter(|(entity, volume)| volume.endless && only.is_none_or(|only| only == *entity))
            .map(|(entity, _)| entity)
            .collect();
        let reference = guid.to_string();
        let mut reached = 0;
        for entity in volumes {
            let Some(before) = ObjState::of(&self.game.engine.world, entity) else { continue };
            let (content, world) = (&mut self.game.content, &mut self.game.engine.world);
            let printed = voxear_engine::voxel_volumes::print(content, world, &reference, at, rotation, Some(entity));
            match printed {
                None => return Err(refuse(ErrorKind::WrongValue, "the Voxel Print could not be read")),
                Some(0) => {}
                Some(_) => {
                    reached += 1;
                    self.record_change(entity, before);
                }
            }
        }
        done(json!({ "reached": reached }))
    }

    fn voxel_prints_make(&mut self, args: &Json) -> Called {
        use voxear_render::{VoxelPrintFill, VoxelPrintPicture, VoxelPrintSettings};
        let made_again = match args.get("print") {
            Some(print) if !print.is_null() => Some(link::resolve_project_asset(&self.game, print)?),
            _ => None,
        };
        let mut settings = match &made_again {
            Some((guid, _, kind)) => {
                if *kind != AssetKind::VoxelPrint {
                    return Err(refuse(ErrorKind::WrongValue, "`print` is not a .voxprint"));
                }
                voxear_engine::voxel_prints::settings_of(&mut self.game.content, &guid.to_string())
                    .ok_or_else(|| refuse(ErrorKind::WrongValue, "the Voxel Print could not be read"))?
            }
            None => VoxelPrintSettings::default(),
        };
        if let Some(model) = args.get("model").filter(|model| !model.is_null()) {
            let (guid, _, kind) = link::resolve_project_asset(&self.game, model)?;
            if kind != AssetKind::Model {
                return Err(refuse(ErrorKind::WrongValue, "`model` is not a model"));
            }
            settings.model = guid.to_string();
        }
        if settings.model.is_empty() {
            return Err(bad("give `model`, or `print` to make again"));
        }
        if let Some(per) = args.get("voxels_per_metre").and_then(Json::as_u64) {
            settings.voxels_per_metre = (per as u32).clamp(1, voxear_render::MOST_VOXELS_A_METRE);
        }
        if let Some(scale) = args.get("scale").and_then(Json::as_f64) {
            settings.scale = scale as f32;
        }
        if let Some(colours) = args.get("colours").and_then(Json::as_u64) {
            settings.colours = (colours as u32).clamp(1, voxear_render::MOST_PRINT_COLOURS);
        }
        if let Some(fill) = args.get("fill").and_then(Json::as_str) {
            settings.fill = VoxelPrintFill::ALL
                .into_iter()
                .find(|each| each.name().eq_ignore_ascii_case(fill))
                .ok_or_else(|| refuse(ErrorKind::WrongValue, format!("{fill} is not solid, sealed or shell")))?;
        }
        if let Some(pictures) = args.get("pictures").and_then(Json::as_array) {
            settings.pictures.clear();
            for given in pictures {
                let part = need(given, "part")?.as_str().ok_or_else(|| bad("a picture's `part` is text"))?;
                let (guid, _, kind) = link::resolve_project_asset(&self.game, need(given, "picture")?)?;
                if kind != AssetKind::Texture {
                    return Err(refuse(ErrorKind::WrongValue, format!("the picture for {part} is not a picture")));
                }
                settings.pictures.push(VoxelPrintPicture { part: part.to_string(), picture: guid.to_string() });
            }
        }
        let path = match (&made_again, args.get("path").and_then(Json::as_str)) {
            (_, Some(path)) => path.to_string(),
            (Some((_, path, _)), None) => path.clone(),
            (None, None) => {
                let model = self.game.content.parse(&settings.model).ok().and_then(|reference| reference.guid());
                let model = model.and_then(|guid| self.game.content.db().get(guid)).map(|entry| entry.path.clone());
                voxear_engine::voxel_prints::path_for(&model.unwrap_or_default())
            }
        };
        // On a thread of its own, as the Project panel's menu makes it: the answer says it is begun, and
        // `voxel_prints.info` finds it once it is made.
        if args.get("background").and_then(Json::as_bool) == Some(true) {
            if self.print_jobs.iter().any(|job| job.path == path) {
                return Err(refuse(ErrorKind::WrongValue, format!("{path} is being made already")));
            }
            let job = voxear_engine::voxel_prints::PrintJob::new(&self.game.content, &settings)
                .map_err(|error| refuse(ErrorKind::WrongValue, format!("{error:#}")))?;
            self.start_print(&path, job);
            return done(json!({ "making": { "path": path } }));
        }
        let started = std::time::Instant::now();
        let (print, guid) = voxear_engine::voxel_prints::make_file(&mut self.game.content, &path, &settings)
            .map_err(|error| refuse(ErrorKind::WrongValue, format!("{error:#}")))?;
        tracing::info!("made the Voxel Print {path} in {:.1} s", started.elapsed().as_secs_f32());
        let bytes = print.to_bytes().map(|bytes| bytes.len()).unwrap_or_default();
        done(json!({
            "print": { "guid": guid.map(|guid| guid.to_string()), "path": path },
            "size": print.size,
            "metres": print.metres().to_array(),
            "solid": print.solid,
            "colours": print.palette.len(),
            "bytes": bytes,
        }))
    }

    fn voxel_prints_info(&mut self, args: &Json) -> Called {
        let (guid, _, kind) = link::resolve_project_asset(&self.game, need(args, "print")?)?;
        if kind != AssetKind::VoxelPrint {
            return Err(refuse(ErrorKind::WrongValue, "`print` is not a .voxprint"));
        }
        let (_, print) = self
            .game
            .content
            .voxel_print(&guid.to_string())
            .ok_or_else(|| refuse(ErrorKind::WrongValue, "the Voxel Print could not be read"))?;
        let bytes = print.to_bytes().map(|bytes| bytes.len()).unwrap_or_default();
        done(json!({
            "size": print.size,
            "metres": print.metres().to_array(),
            "voxels_per_metre": print.voxels_per_metre,
            "solid": print.solid,
            "colours": print.palette.len(),
            "bytes": bytes,
            "settings": serde_json::to_value(&print.settings).unwrap_or(Json::Null),
        }))
    }

    fn paint_map(&mut self, args: &Json) -> Called {
        use voxear_engine::map_painting::{Dab, PaintMap, PaintMode};
        self.refuse_while_playing("paint a world's maps")?;
        let at = Vec3::from_array(opt_vector(args, "at")?.ok_or_else(|| bad("`at` is missing"))?);
        let number = |key: &str, default: f32| args.get(key).and_then(Json::as_f64).map_or(default, |v| v as f32);
        let mode = match (opt_bool(args, "smooth"), opt_bool(args, "take_away")) {
            (Some(true), _) => PaintMode::Smooth,
            (_, Some(true)) => PaintMode::TakeAway,
            _ => PaintMode::Add,
        };
        let mut changed = false;
        for (entity, ..) in self.block_worlds(args)? {
            let world = &self.game.engine.world;
            let Some(asset) = world.get::<voxear_engine::blocks::BlockWorldAsset>(entity).cloned() else { continue };
            let map = match need(args, "map")?.as_str() {
                Some("land") => PaintMap::Land,
                Some("height") => PaintMap::Height,
                Some("erase") => PaintMap::Erase,
                Some("area") => {
                    let area = link::resolve_obj(&self.game, need(args, "area")?)?;
                    let number = voxear_engine::world_areas::number_for(&mut self.game.engine.world, area);
                    PaintMap::Area(
                        number.ok_or_else(|| refuse(ErrorKind::NoSuchComponent, "the Obj has no World Area"))?,
                    )
                }
                Some("biome") => {
                    let path = link::resolve_asset(&self.game, need(args, "biome")?)?;
                    let guid = self.game.content.parse(&path).ok().and_then(|r| r.guid());
                    let listed = asset.file.generate.as_ref().map(|g| g.biomes.clone()).unwrap_or_default();
                    let index = listed
                        .iter()
                        .position(|reference| self.game.content.parse(reference).ok().and_then(|r| r.guid()) == guid)
                        .ok_or_else(|| {
                            refuse(ErrorKind::WrongValue, format!("{path} is not a biome the world lists"))
                        })?;
                    PaintMap::Biome(index)
                }
                _ => return Err(bad("`map` is land, height, biome, area or erase")),
            };
            let dab = Dab {
                map,
                mode,
                at,
                radius: number("radius", 40.0),
                strength: number("strength", 0.3),
                falloff: number("falloff", 0.5),
            };
            let before = self.game.content.painted(asset.guid, true).map(|painted| painted.maps());
            if voxear_engine::map_painting::paint(&self.game.engine.world, &mut self.game.content, entity, &dab)
                && let (Some(before), Some(painted)) = (before, self.game.content.painted(asset.guid, false))
            {
                self.record(Edit::Painted { world: asset.guid, before, after: painted.maps() });
                changed = true;
            }
        }
        done(json!({ "changed": changed }))
    }

    fn blocks_restore(&mut self, args: &Json) -> Called {
        use voxear_terrain::blocks::BlockWorld;
        const MOST: i64 = 4_000_000;
        let (Some(from), Some(to)) = (opt_vector(args, "from")?, opt_vector(args, "to")?) else {
            return Err(bad("the points are missing"));
        };
        let (from, to) = (Vec3::from_array(from), Vec3::from_array(to));
        let mut changed = 0;
        for (entity, corner, size) in self.block_worlds(args)? {
            let cell = |p: Vec3| ((p - corner) / size).floor().as_ivec3();
            let (low, high) = (cell(from.min(to)), cell(from.max(to)));
            let count = (high - low + 1).as_i64vec3().element_product().max(0);
            if count > MOST {
                return Err(bad(format!("{count} blocks are too many at once; the most is {MOST}")));
            }
            let cells: Vec<[i32; 3]> = (low.z..=high.z)
                .flat_map(|z| (low.y..=high.y).flat_map(move |y| (low.x..=high.x).map(move |x| [x, y, z])))
                .collect();
            let world = &mut self.game.engine.world;
            let changes = voxear_engine::blocks::restored(world, entity, &cells);
            let Some(before) = ObjState::of(world, entity) else { continue };
            let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { continue };
            let count = blocks.set_blocks(changes);
            if count > 0 {
                world.insert(entity, blocks);
                changed += count;
                self.record_change(entity, before);
            }
        }
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
                file.crowds.put(&mut self.game.engine.world);
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
            | NewFile::Dimension(_)
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
        NewFile::Dimension(voxear_engine::dimensions::DimensionKind::Overworld),
        NewFile::Dimension(voxear_engine::dimensions::DimensionKind::Dungeon),
        NewFile::Dimension(voxear_engine::dimensions::DimensionKind::Caves),
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
