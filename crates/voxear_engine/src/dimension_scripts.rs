//! Dimensions from C#: `Dimension`, `DimensionInstance` and `Dimensions` in
//! Voxear.Core, which dimension an Obj is in and moving it to another, a
//! dungeon's stage, and asking of one dimension's blocks and physics while
//! the game is seen from another.
//!
//! Scripts hold a dimension made as a number (see
//! `dimensions::Dimensions::handle`): 0 for the level's. They reach it
//! through `kind_value` and `kind_text` as `ComponentKind::Dimension`, the
//! number in the place of an Obj. What needs the whole game, making a
//! dimension from its file or a dungeon's and letting one go, is asked here
//! ([`DimensionAsks`]) and done by the game once scripts have run
//! (`Game::serve_dimension_asks`); a script hears how it went the next frame.

use serde_json::{Value, json};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, DimensionId, Parent, Transform};

use crate::{
    Game,
    dimensions::{self, DimensionClock, DimensionKind, Dimensions},
    dungeons::{DungeonGate, DungeonStage, DungeonStages},
};

// What `values` reads or writes, as `DimensionInstance` in Voxear.Core numbers them.
/// The dimension an Obj is in.
const OF_OBJ: i32 = 1;
/// The dimension the game is seen from.
const CURRENT: i32 = 2;
/// Moves an Obj into a dimension made, at a place there: written with the place and the dimension.
const MOVE: i32 = 3;
/// Whether a dimension stays made until it is let go, or goes once nothing needs it.
const KEEP_LOADED: i32 = 4;
/// Whether a dimension's ground is made round a place: written with the place and how far round, then read.
const READY: i32 = 5;
/// Asks of a dimension's blocks and physics from now on, or (written with nothing) as before again.
const ASK_IN: i32 = 6;
const ASK_END: i32 = 7;
/// Whether a dimension is made.
const IS_MADE: i32 = 9;
/// A dimension's sky: whether it has one, and its light from the sky.
const SKY: i32 = 10;
/// A dimension's clock: whose it keeps, its time of day and its day's length.
const CLOCK: i32 = 11;
/// A dimension's weather: its kind, and whether it has any.
const WEATHER: i32 = 12;
/// What its weather adds now: the first four amounts, then the last four.
const WEATHER_FIRST: i32 = 13;
const WEATHER_LAST: i32 = 14;

// What `texts` answers.
/// Makes a dimension: from its asset and key, or a dungeon's from its site. Gives it, or the number of the ask.
const MAKE: i32 = 1;
/// How an ask went.
const ASKED: i32 = 2;
/// What a dimension is: its asset, key, kind, settings and dungeon.
const INFO: i32 = 3;
/// Every dimension made, the level's first.
const MADE: i32 = 4;
/// Every dungeon's stage and cleared flag where either is set, by its site.
const STAGES: i32 = 5;
/// Puts every dungeon's stage back, as `STAGES` gave them.
const SET_STAGES: i32 = 6;
/// Sets one dungeon's stage and cleared flag, or grows it a stage where nobody is in its dimension.
const SET_STAGE: i32 = 7;
/// Lets a dimension go.
const LET_GO: i32 = 8;
/// The Objs at the top of the hierarchy put in a dimension other than the level's, that are not that
/// dimension's own (its file's, its dungeon's rock and Portal): each with its dimension and where it is.
const ELSEWHERE: i32 = 9;

/// Sets what answers `DimensionInstance` and `Dimensions` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::Dimension, values);
    voxear_scripting::set_kind_texts(voxear_scripting::ComponentKind::Dimension, texts);
}

/// A dimension a script asked for.
#[derive(Clone, Debug, PartialEq)]
enum Wanted {
    /// The dimension a `.dimension` makes as `key`, kept until it is let go where `keep`.
    Asset {
        asset: voxear_assets::AssetGuid,
        key: u64,
        keep: bool,
    },
    /// A dungeon's own dimension.
    Dungeon {
        gate: DungeonGate,
        keep: bool,
    },
    LetGo(DimensionId),
}

/// How an ask went.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Waiting,
    Made(u32),
    Failed(String),
}

/// What scripts asked of dimensions, for the game to do once they have run,
/// and how each ask went, the last few kept for scripts to hear.
#[derive(Default)]
pub struct DimensionAsks {
    next: u32,
    asks: Vec<(u32, Wanted, Answer)>,
}

/// How many answered asks are kept for scripts to hear.
const ANSWERS_KEPT: usize = 64;

impl DimensionAsks {
    /// Asks for `wanted`, or gives the number of the ask for it still waiting.
    fn ask(&mut self, wanted: Wanted) -> u32 {
        let same = self.asks.iter().find(|(_, asked, answer)| *asked == wanted && *answer == Answer::Waiting);
        if let Some((number, ..)) = same {
            return *number;
        }
        self.next += 1;
        self.asks.push((self.next, wanted, Answer::Waiting));
        let answered = self.asks.iter().filter(|(.., answer)| *answer != Answer::Waiting).count();
        if answered > ANSWERS_KEPT
            && let Some(at) = self.asks.iter().position(|(.., answer)| *answer != Answer::Waiting)
        {
            self.asks.remove(at);
        }
        self.next
    }
}

/// A dimension a script names by its number, while it is made.
fn named(world: &World, entity: Entity) -> Option<DimensionId> {
    dimensions::from_handle(world, (entity.to_bits() & 0xffff_ffff) as u32)
}

fn handle(world: &World, id: DimensionId) -> f32 {
    f32::from_bits(dimensions::handle_of(world, id).unwrap_or(0))
}

/// What the dimension asked of was before scripts asked of another.
#[derive(Default)]
struct AskedIn(Vec<DimensionId>);

/// Makes `id` the dimension blocks, physics and every other place are asked
/// of, as `CurrentDimension` says.
fn ask_in(world: &mut World, id: DimensionId) {
    world.insert_resource(voxear_scene::CurrentDimension(id));
    if let Some(physics) = world.resource_mut::<crate::physics::PhysicsScene>() {
        physics.world_mut().ask_in(id.0);
    }
}

/// Where a script asked whether a dimension's ground is made.
#[derive(Default)]
struct ReadyAt(Vec3, f32);

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let number = |n: f32| Some([n, 0.0, 0.0, 0.0]);
    match (what, written) {
        (OF_OBJ, None) => {
            world.is_alive(entity).then_some(())?;
            number(handle(world, dimensions::dimension_of(world, entity)))
        }
        (CURRENT, None) => number(handle(world, dimensions::current(world))),
        (MOVE, Some([x, y, z, into])) => {
            let id = dimensions::from_handle(world, into.to_bits())?;
            move_to(world, entity, id, Vec3::new(x, y, z)).then_some([0.0; 4])
        }
        (IS_MADE, None) => number(f32::from(u8::from(named(world, entity).is_some()))),
        (KEEP_LOADED, None) => {
            let id = named(world, entity)?;
            let kept = id.is_level() || world.resource::<Dimensions>()?.get(id).is_some_and(|made| !made.on_demand);
            number(f32::from(u8::from(kept)))
        }
        (KEEP_LOADED, Some([keep, ..])) => {
            let id = named(world, entity)?;
            dimensions::set_on_demand(world, id, keep == 0.0);
            Some([0.0; 4])
        }
        (READY, Some([x, y, z, radius])) => {
            world.insert_resource(ReadyAt(Vec3::new(x, y, z), radius.max(0.0)));
            Some([0.0; 4])
        }
        (READY, None) => {
            let id = named(world, entity)?;
            let ReadyAt(at, radius) = world.resource::<ReadyAt>().map(|ready| ReadyAt(ready.0, ready.1))?;
            // A dimension with no world made round places has its ground whole already.
            let made = crate::blocks::made_round(world, (at, id), radius).unwrap_or(true);
            number(f32::from(u8::from(made)))
        }
        (ASK_IN, Some(_)) => {
            let id = named(world, entity)?;
            let was = dimensions::current(world);
            world.resource_mut::<AskedIn>().map(|asked| asked.0.push(was)).unwrap_or_else(|| {
                world.insert_resource(AskedIn(vec![was]));
            });
            ask_in(world, id);
            Some([0.0; 4])
        }
        (ASK_END, Some(_)) => {
            let was = world.resource_mut::<AskedIn>().and_then(|asked| asked.0.pop())?;
            ask_in(world, was);
            Some([0.0; 4])
        }
        (SKY, None) => {
            let id = named(world, entity)?;
            let dimensions = world.resource::<Dimensions>();
            let settings = dimensions.map_or_else(dimensions::DimensionSettings::default, |made| made.settings(id));
            let surroundings = match dimensions {
                Some(dimensions) => dimensions.surroundings(world, id),
                None => dimensions::Surroundings::copy(world),
            };
            let ambient = surroundings.sky().map_or(0.0, |sky| sky.ambient);
            Some([f32::from(u8::from(settings.sky())), ambient, 0.0, 0.0])
        }
        (CLOCK, None) => clock(world, named(world, entity)?),
        (WEATHER | WEATHER_FIRST | WEATHER_LAST, None) => {
            let id = named(world, entity)?;
            let dimensions = world.resource::<Dimensions>();
            let settings = dimensions.map_or_else(dimensions::DimensionSettings::default, |made| made.settings(id));
            let weather = match (id == dimensions::current(world), dimensions) {
                (true, _) | (false, None) => world.resource::<voxear_render::weather::Weather>().copied(),
                (false, Some(dimensions)) => dimensions.surroundings(world, id).weather(),
            }
            .unwrap_or_default();
            let amounts = weather.now().to_array();
            match what {
                WEATHER => Some([weather.kind().index() as f32, f32::from(u8::from(settings.weather())), 0.0, 0.0]),
                WEATHER_FIRST => Some([amounts[0], amounts[1], amounts[2], amounts[3]]),
                _ => Some([amounts[4], amounts[5], amounts[6], amounts[7]]),
            }
        }
        _ => None,
    }
}

/// A dimension's clock: whose time it keeps (0 the level's, 1 its own, 2
/// none), its time of day in hours and its day's length in minutes, NaN
/// where it keeps none.
fn clock(world: &World, id: DimensionId) -> Option<[f32; 4]> {
    let kind = match id.is_level() {
        true => DimensionClock::Level,
        false => world.resource::<Dimensions>()?.settings(id).clock(),
    };
    let dimensioned = voxear_scene::Dimensioned::of(world);
    let mut climates: Vec<(Entity, crate::heat::Climate)> = world
        .query_ref::<(Entity, &crate::heat::Climate)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, climate)| (entity, climate.clone()))
        .collect();
    climates.sort_by_key(|(entity, _)| entity.to_bits());
    let of = |dimension: DimensionId| climates.iter().find(|(entity, _)| dimensioned.of_obj(*entity) == dimension);
    // The level's clock is its first Climate's in its own dimension, or failing that the first anywhere, as the
    // clock is turned (`heat::turn_clock`).
    let climate = match kind {
        DimensionClock::Level => of(DimensionId::LEVEL).or(climates.first()),
        DimensionClock::Own => of(id),
        DimensionClock::None => None,
    };
    let code = match kind {
        DimensionClock::Level => 0.0,
        DimensionClock::Own => 1.0,
        DimensionClock::None => 2.0,
    };
    Some(match climate {
        Some((_, climate)) => [code, climate.time_of_day, climate.day_length, 0.0],
        None => [code, f32::NAN, f32::NAN, 0.0],
    })
}

/// Puts an Obj, and everything under it, in dimension `id` at `at` in that
/// dimension's coordinates, keeping its speed. False where it is gone.
pub fn move_to(world: &mut World, obj: Entity, id: DimensionId, at: Vec3) -> bool {
    if !world.is_alive(obj) || world.has::<Destroying>(obj) {
        return false;
    }
    let exists = world.resource::<Dimensions>().map_or(id.is_level(), |dimensions| dimensions.exists(id));
    if !exists {
        return false;
    }
    voxear_scene::put_in_dimension(world, obj, id);
    let local = match world.get::<Parent>(obj).map(|parent| parent.0) {
        Some(parent) => voxear_scene::world_matrix(world, parent).inverse().transform_point3(at),
        None => at,
    };
    if let Some(transform) = world.get_mut::<Transform>(obj) {
        transform.position = local;
    }
    voxear_scene::propagate_transforms(world);
    if let Some(mut physics) = world.remove_resource::<crate::physics::PhysicsScene>() {
        physics.teleported(world, obj, Quat::IDENTITY);
        world.insert_resource(physics);
    }
    true
}

/// A dungeon's site as scripts give it: its world's Obj, which structure,
/// its site and seed.
fn gate_of(site: &Value) -> Option<DungeonGate> {
    let world = Entity::from_bits(site.get("world")?.as_u64()?);
    let structure = site.get("structure")?.as_u64()? as usize;
    let at = site.get("at")?.as_array()?;
    let at = [at.first()?.as_f64()? as f32, at.get(1)?.as_f64()? as f32];
    let seed = site.get("seed")?.as_u64()? as u32;
    Some(DungeonGate { world, structure, site: at, seed })
}

fn site_json(gate: &DungeonGate) -> Value {
    json!({ "world": gate.world.to_bits(), "structure": gate.structure, "at": gate.site, "seed": gate.seed })
}

/// Whether anybody is in a dimension: a camera, or a character.
fn anybody_in(world: &World, id: DimensionId) -> bool {
    let cameras =
        world.query_ref::<(Entity, &voxear_render::Camera)>().without::<Destroying>().into_iter().map(|(e, _)| e);
    let characters = world
        .query_ref::<(Entity, &crate::physics::CharacterController)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(e, _)| e);
    let mut found = cameras.chain(characters);
    found.any(|entity| dimensions::dimension_of(world, entity) == id)
}

/// Grows a dungeon a stage: a floor more under its last. Not while anybody
/// is in its dimension; where it is made with nobody in it, it is let go,
/// its changes kept, and made again grown when it is next needed. False
/// where it does not grow.
pub fn grow(world: &mut World, gate: DungeonGate) -> bool {
    let site = gate.site();
    let guid = world.get::<crate::blocks::BlockWorldAsset>(gate.world).map(|asset| asset.guid);
    let made = guid.and_then(|guid| world.resource::<Dimensions>()?.find_by_key(gate.key(guid)));
    if made.is_some_and(|id| anybody_in(world, id)) {
        return false;
    }
    let now = DungeonStages::of(world, site);
    let most = voxear_terrain::blocks::dungeon::MOST_FLOORS;
    let floors = world.get::<crate::blocks::BlockWorldAsset>(gate.world).and_then(|asset| {
        let built = asset.file.generate.as_ref()?.built.clone()?;
        match &built.list.get(gate.structure)?.blocks {
            voxear_terrain::blocks::structure::StructureBlocks::Dungeon(dungeon) => {
                Some(crate::dungeons::floors_at_stage(dungeon, gate.seed, now.stage))
            }
            _ => None,
        }
    });
    if floors.is_none_or(|floors| floors >= most) {
        return false;
    }
    DungeonStages::set(world, site, DungeonStage { stage: now.stage + 1, ..now });
    if let Some(id) = made {
        let mut asks = world.remove_resource::<DimensionAsks>().unwrap_or_default();
        asks.ask(Wanted::LetGo(id));
        world.insert_resource(asks);
    }
    true
}

fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    match what {
        MAKE => {
            let asked: Value = serde_json::from_str(text).ok()?;
            let keep = asked.get("keep").and_then(Value::as_bool).unwrap_or(true);
            let wanted = match asked.get("dungeon") {
                Some(site) => Wanted::Dungeon { gate: gate_of(site)?, keep },
                None => {
                    let asset: voxear_assets::AssetGuid = asked.get("asset")?.as_str()?.parse().ok()?;
                    let key = asked.get("key").and_then(Value::as_u64).unwrap_or(0);
                    // Made already, it is given at once.
                    let made = world.resource::<Dimensions>().and_then(|dimensions| dimensions.find(asset, key));
                    if let Some(id) = made {
                        if keep {
                            dimensions::set_on_demand(world, id, false);
                        }
                        return Some(json!({ "dimension": handle(world, id).to_bits() }).to_string());
                    }
                    Wanted::Asset { asset, key, keep }
                }
            };
            let mut asks = world.remove_resource::<DimensionAsks>().unwrap_or_default();
            let number = asks.ask(wanted);
            world.insert_resource(asks);
            Some(json!({ "ask": number }).to_string())
        }
        ASKED => {
            let number: u32 = text.trim().parse().ok()?;
            let asks = world.resource::<DimensionAsks>()?;
            let (_, _, answer) = asks.asks.iter().find(|(asked, ..)| *asked == number)?;
            Some(
                match answer {
                    Answer::Waiting => json!({ "waiting": true }),
                    Answer::Made(made) => json!({ "dimension": made }),
                    Answer::Failed(error) => json!({ "error": error }),
                }
                .to_string(),
            )
        }
        INFO => {
            let id = named(world, entity)?;
            let dimensions = world.resource::<Dimensions>();
            let made = dimensions.and_then(|dimensions| dimensions.get(id));
            let settings = made.map_or_else(
                || dimensions::DimensionSettings::of(DimensionKind::Overworld),
                |made| made.settings.clone(),
            );
            let kind = match settings.kind {
                DimensionKind::Overworld => "Overworld",
                DimensionKind::Dungeon => "Dungeon",
                DimensionKind::Caves => "Caves",
            };
            Some(
                json!({
                    "dimension": handle(world, id).to_bits(),
                    "level": id.is_level(),
                    "asset": made.and_then(|made| made.asset).map(|asset| asset.to_string()),
                    "key": made.map_or(0, |made| made.key),
                    "kind": kind,
                    "keep_loaded": settings.keep_loaded(),
                    "kept": made.is_none_or(|made| !made.on_demand),
                    "dungeon": made.and_then(|made| made.dungeon).map(|gate| site_json(&gate)),
                })
                .to_string(),
            )
        }
        MADE => {
            let mut made = vec![0u32];
            if let Some(dimensions) = world.resource::<Dimensions>() {
                made.extend(dimensions.made().filter_map(|made| dimensions.handle(made.id)));
            }
            Some(json!(made).to_string())
        }
        STAGES => {
            let stages = world.resource::<DungeonStages>();
            let mut listed: Vec<Value> = stages
                .into_iter()
                .flat_map(|stages| stages.0.iter())
                .map(|(site, stage)| {
                    json!({
                        "site": { "world": site.world.to_bits(), "structure": site.structure,
                                  "at": site.site.map(f32::from_bits) },
                        "stage": stage.stage, "cleared": stage.cleared,
                    })
                })
                .collect();
            listed.sort_by_key(|value| value.to_string());
            Some(Value::Array(listed).to_string())
        }
        SET_STAGES => {
            let listed: Vec<Value> = serde_json::from_str(text).ok()?;
            world.remove_resource::<DungeonStages>();
            for entry in listed {
                let Some(gate) = stage_site(entry.get("site")?) else { continue };
                let stage = entry.get("stage").and_then(Value::as_u64).unwrap_or(0) as u32;
                let cleared = entry.get("cleared").and_then(Value::as_bool).unwrap_or(false);
                DungeonStages::set(world, gate, DungeonStage { stage, cleared });
            }
            Some(String::new())
        }
        SET_STAGE => {
            let asked: Value = serde_json::from_str(text).ok()?;
            let gate = gate_of(asked.get("site")?)?;
            if asked.get("grow").and_then(Value::as_bool) == Some(true) {
                return grow(world, gate).then(String::new);
            }
            let now = DungeonStages::of(world, gate.site());
            let stage = asked.get("stage").and_then(Value::as_u64).map_or(now.stage, |stage| stage as u32);
            let cleared = asked.get("cleared").and_then(Value::as_bool).unwrap_or(now.cleared);
            DungeonStages::set(world, gate.site(), DungeonStage { stage, cleared });
            Some(String::new())
        }
        ELSEWHERE => Some(Value::Array(elsewhere(world)).to_string()),
        LET_GO => {
            let id = named(world, entity).filter(|id| !id.is_level())?;
            let mut asks = world.remove_resource::<DimensionAsks>().unwrap_or_default();
            asks.ask(Wanted::LetGo(id));
            world.insert_resource(asks);
            Some(String::new())
        }
        _ => None,
    }
}

/// The Objs a save of the scene puts back in another dimension than the
/// level's: those at the top of the hierarchy put in one, but for what the
/// dimension is made with.
fn elsewhere(world: &World) -> Vec<Value> {
    let Some(dimensions) = world.resource::<Dimensions>() else { return Vec::new() };
    let own = |entity: Entity, id: DimensionId| {
        dimensions
            .get(id)
            .is_some_and(|made| made.roots.contains(&entity) || made.objs.iter().any(|(_, e)| *e == entity))
            || world.has::<crate::dungeons::DungeonInside>(entity)
            || world.has::<crate::dungeons::DungeonWayOut>(entity)
            || world.has::<voxear_scene::EditorOnly>(entity)
    };
    let mut found: Vec<(Entity, DimensionId)> = world
        .query_ref::<(Entity, &voxear_scene::InDimension)>()
        .without::<Destroying>()
        .without::<Parent>()
        .into_iter()
        .map(|(entity, inside)| (entity, inside.0))
        .filter(|&(entity, id)| !id.is_level() && !own(entity, id))
        .collect();
    found.sort_by_key(|(entity, _)| entity.to_bits());
    found
        .into_iter()
        .map(|(entity, id)| {
            let at = voxear_scene::world_matrix(world, entity).w_axis.truncate();
            json!({ "obj": entity.to_bits(), "dimension": dimensions.handle(id), "at": at.to_array() })
        })
        .collect()
}

/// A dungeon's site as `STAGES` gives it.
fn stage_site(site: &Value) -> Option<crate::dungeons::DungeonSite> {
    let world = Entity::from_bits(site.get("world")?.as_u64()?);
    let structure = site.get("structure")?.as_u64()? as usize;
    let at = site.get("at")?.as_array()?;
    let at = [at.first()?.as_f64()? as f32, at.get(1)?.as_f64()? as f32];
    Some(crate::dungeons::DungeonSite { world, structure, site: at.map(f32::to_bits) })
}

impl Game {
    /// Does what scripts asked of dimensions this frame: makes those asked
    /// for, a dungeon's or one from its file, and lets go those asked to go.
    pub fn serve_dimension_asks(&mut self, mut renderer: Option<&mut voxear_render::Renderer>) {
        let Some(mut asks) = self.engine.world.remove_resource::<DimensionAsks>() else { return };
        let waiting: Vec<(u32, Wanted)> = asks
            .asks
            .iter()
            .filter(|(.., answer)| *answer == Answer::Waiting)
            .map(|(number, wanted, _)| (*number, wanted.clone()))
            .collect();
        for (number, wanted) in waiting {
            let answer = match wanted {
                Wanted::Asset { asset, key, keep } => match self.make_dimension(asset, key, renderer.as_deref_mut()) {
                    Ok(id) => {
                        dimensions::set_on_demand(&mut self.engine.world, id, !keep);
                        Answer::Made(dimensions::handle_of(&self.engine.world, id).unwrap_or(0))
                    }
                    Err(error) => Answer::Failed(format!("{error:#}")),
                },
                Wanted::Dungeon { gate, keep } => match self.make_dungeon_dimension(gate, renderer.as_deref_mut()) {
                    Ok((id, _)) => {
                        if keep {
                            dimensions::set_on_demand(&mut self.engine.world, id, false);
                        }
                        Answer::Made(dimensions::handle_of(&self.engine.world, id).unwrap_or(0))
                    }
                    Err(error) => Answer::Failed(format!("{error:#}")),
                },
                Wanted::LetGo(id) => {
                    self.let_go_dimension(id);
                    Answer::Made(0)
                }
            };
            if let Some(asked) = asks.asks.iter_mut().find(|(asked, ..)| *asked == number) {
                asked.2 = answer;
            }
        }
        // Asks made while these were done wait for the next frame.
        if let Some(more) = self.engine.world.remove_resource::<DimensionAsks>() {
            asks.next = asks.next.max(more.next);
            asks.asks.extend(more.asks);
        }
        self.engine.world.insert_resource(asks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ask_for_a_dimension_still_waiting_is_the_same_ask() {
        let mut asks = DimensionAsks::default();
        let asset = voxear_assets::AssetGuid::from_u128(7);
        let first = asks.ask(Wanted::Asset { asset, key: 0, keep: true });
        assert_eq!(asks.ask(Wanted::Asset { asset, key: 0, keep: true }), first, "asked twice, as a script's call is");
        assert_ne!(asks.ask(Wanted::Asset { asset, key: 1, keep: true }), first);
        asks.asks[0].2 = Answer::Made(3);
        assert_ne!(asks.ask(Wanted::Asset { asset, key: 0, keep: true }), first, "once answered, a new ask");
    }
}
