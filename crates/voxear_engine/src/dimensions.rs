//! Dimensions: worlds of their own in the one game, beside the level's.
//!
//! Each has its own Block Worlds, sky, sun, post-processing and weather, at
//! coordinates of its own that may be the same as another's: nothing in one
//! is drawn, heard, collided with or streamed round in another (see
//! `voxear_scene::dimensions` for which dimension an Obj is in). The level's
//! own scenes are the first, [`DimensionId::LEVEL`]; a `.dimension` file
//! makes another, its Objs written as a scene writes them under a
//! `[dimension]` table of its settings:
//!
//! ```toml
//! format = 1
//! lighting = "Lighting/Dusk.lighting"
//!
//! [dimension]
//! kind = "caves"
//! sky = false
//!
//! [[obj]]
//! id = 1
//! name = "Ground"
//! [obj.block_world]
//! data = "Blocks/Deep.blockworld"
//! ```
//!
//! The game keeps what it has made in a [`Dimensions`] resource. The world's
//! sky, sun, post-processing and weather resources are always the *current*
//! dimension's, the one the game is seen from; each other dimension's are
//! kept aside ([`Surroundings`]) and put back when the camera comes into it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_render::{
    Camera, PostProcess, Sky, SunLook,
    weather::{Weather, WeatherPlaces},
};
use voxear_scene::{Destroying, EditorOnly, Parent, ScenePersistence, Switch};
pub use voxear_scene::{DimensionId, Dimensioned, InDimension, dimension_of, set_dimension};

use crate::scene_file::{LightingPost, PostTable, SceneFile, SceneLighting, SkyFromLook};

/// What a dimension is for, which sets good values for the rest of its
/// settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionKind {
    /// Open land under a sky of its own, with weather, on the level's clock.
    #[default]
    Overworld,
    /// Enclosed: no sky, no sun, no weather, one for each place that opens
    /// it, as each dungeon is.
    Dungeon,
    /// Enclosed, as a dungeon is, but only one.
    Caves,
}

/// Whose time of day a dimension keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionClock {
    /// The level's: its sun stands where the level's does.
    #[default]
    Level,
    /// Its own first Climate's.
    Own,
    /// None: its sun, if it has one, stays where it was put.
    None,
}

/// How many of a dimension there are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionInstances {
    /// One, wherever it is opened from.
    #[default]
    One,
    /// One for each place that opens it, each made from its own seed, as
    /// each dungeon is its own.
    EachSite,
}

/// How long a dimension stays made after nothing needs it, in seconds, where
/// its file does not say.
pub const KEEP_LOADED: f32 = 30.0;

/// A dimension's settings, its file's `[dimension]` table. What it leaves
/// out is as its kind says.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DimensionSettings {
    pub kind: DimensionKind,
    /// Whether it has a sky. Without one it is enclosed: no sun, no light
    /// from the sky, the dark of a cave beyond its own lights.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sky: Option<bool>,
    /// Whether its lighting's weather runs in it. Without, it is clear.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock: Option<DimensionClock>,
    /// How long it stays made after nothing needs it, in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_loaded: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances: Option<DimensionInstances>,
}

impl DimensionSettings {
    /// The settings of a dimension of `kind` with nothing more said.
    pub fn of(kind: DimensionKind) -> Self {
        Self { kind, ..Self::default() }
    }

    pub fn sky(&self) -> bool {
        self.sky.unwrap_or(self.kind == DimensionKind::Overworld)
    }

    pub fn weather(&self) -> bool {
        self.weather.unwrap_or(self.kind == DimensionKind::Overworld) && self.sky()
    }

    pub fn clock(&self) -> DimensionClock {
        self.clock.unwrap_or(match self.kind {
            DimensionKind::Overworld => DimensionClock::Level,
            DimensionKind::Dungeon | DimensionKind::Caves => DimensionClock::None,
        })
    }

    pub fn keep_loaded(&self) -> f32 {
        self.keep_loaded.unwrap_or(KEEP_LOADED).max(0.0)
    }

    pub fn instances(&self) -> DimensionInstances {
        self.instances.unwrap_or(match self.kind {
            DimensionKind::Dungeon => DimensionInstances::EachSite,
            DimensionKind::Overworld | DimensionKind::Caves => DimensionInstances::One,
        })
    }
}

/// A `.dimension` file: a scene's Objs, lighting, sky and post-processing,
/// and its settings.
#[derive(Debug, Default)]
pub struct DimensionFile {
    pub settings: DimensionSettings,
    pub scene: SceneFile,
}

impl DimensionFile {
    pub fn parse(text: &str, name: &str) -> Result<Self> {
        let mut table: toml::Table = toml::from_str(text).with_context(|| format!("reading {name}"))?;
        let settings = match table.remove("dimension") {
            Some(value) => value.try_into().with_context(|| format!("reading the [dimension] of {name}"))?,
            None => DimensionSettings::default(),
        };
        if !table.contains_key("format") {
            table.insert("format".into(), toml::Value::Integer(crate::scene_file::SCENE_FORMAT.into()));
        }
        let scene: SceneFile = toml::Value::Table(table).try_into().with_context(|| format!("reading {name}"))?;
        Ok(Self { settings, scene })
    }

    pub fn to_toml(&self) -> Result<String> {
        let mut table = toml::Table::try_from(&self.scene)?;
        let settings = toml::Table::try_from(&self.settings)?;
        // Its settings after its format and lighting, before its Objs, as the file is read.
        let objs = table.remove("obj");
        table.insert("dimension".into(), toml::Value::Table(settings));
        if let Some(objs) = objs {
            table.insert("obj".into(), objs);
        }
        Ok(toml::to_string(&table)?)
    }
}

/// A dimension's sky, sun, post-processing and weather: the world's
/// resources while it is the current dimension, kept here while it is not.
#[derive(Clone, Default)]
pub struct Surroundings {
    sky: Option<Sky>,
    sky_from_look: bool,
    sun: Option<SunLook>,
    lighting: Option<String>,
    lighting_post: Option<toml::Table>,
    post_table: Option<toml::Table>,
    post: Option<PostProcess>,
    weather: Option<Weather>,
}

impl Surroundings {
    /// The world's, as they are now, left in place.
    pub fn copy(world: &World) -> Self {
        Self {
            sky: world.resource::<Sky>().copied(),
            sky_from_look: world.resource::<SkyFromLook>().is_some(),
            sun: world.resource::<SunLook>().copied(),
            lighting: world.resource::<SceneLighting>().map(|lighting| lighting.0.clone()),
            lighting_post: world.resource::<LightingPost>().map(|post| post.0.clone()),
            post_table: world.resource::<PostTable>().map(|table| table.0.clone()),
            post: world.resource::<PostProcess>().copied(),
            weather: world.resource::<Weather>().copied(),
        }
    }

    /// Makes them the world's, in place of whatever it had.
    pub fn put(&self, world: &mut World) {
        fn set<T: 'static + Send + Sync>(world: &mut World, value: Option<T>) {
            match value {
                Some(value) => drop(world.insert_resource(value)),
                None => drop(world.remove_resource::<T>()),
            }
        }
        set(world, self.sky);
        set(world, self.sky_from_look.then_some(SkyFromLook));
        set(world, self.sun);
        set(world, self.lighting.clone().map(SceneLighting));
        set(world, self.lighting_post.clone().map(LightingPost));
        set(world, self.post_table.clone().map(PostTable));
        set(world, self.post);
        set(world, self.weather);
        // What storms add where the viewer stands is worked out again from the storms of the dimension come into.
        world.remove_resource::<WeatherPlaces>();
    }

    /// The surroundings of a dimension with no sky: dark all round, no sun,
    /// clear, its post-processing `post` but for its fog, which thickens with
    /// depth under the level's ground and would fill a deep dungeon.
    pub fn enclosed(post: Option<PostProcess>) -> Self {
        let mut post = post.unwrap_or_default();
        post.fog.enabled = false;
        let post = Some(post);
        let sky = Sky {
            zenith: voxear_core::math::Vec3::ZERO,
            horizon: voxear_core::math::Vec3::ZERO,
            ground: voxear_core::math::Vec3::ZERO,
            ambient: 0.0,
            stars: 0.0,
            clouds: voxear_render::Clouds::default(),
            ..Sky::default()
        };
        Self { sky: Some(sky), post, weather: Some(Weather::default()), ..Self::default() }
    }

    /// The same with clear weather.
    pub fn cleared(self) -> Self {
        Self { weather: Some(Weather::default()), ..self }
    }

    pub fn sky(&self) -> Option<&Sky> {
        self.sky.as_ref()
    }

    pub fn weather(&self) -> Option<Weather> {
        self.weather
    }

    pub fn post(&self) -> Option<PostProcess> {
        self.post
    }

    /// As a view through a Portal into the dimension draws it.
    fn look(&self) -> voxear_render::DimensionLook {
        voxear_render::DimensionLook {
            sky: self.sky.unwrap_or_default(),
            sun: self.sun,
            post: self.post,
            weather: self.weather.unwrap_or_default(),
        }
    }
}

/// A dimension the game has made.
pub struct Made {
    pub id: DimensionId,
    /// The `.dimension` it was made from; none for one made by code.
    pub asset: Option<AssetGuid>,
    /// Which of its asset's dimensions it is: 0 where there is only one,
    /// else a number of what opened it, as a dungeon's site.
    pub key: u64,
    pub settings: DimensionSettings,
    /// The Objs its file placed, its roots.
    pub roots: Vec<Entity>,
    /// Every Obj its file placed, by its path in the file, for what in
    /// another file names one, as a Portal leading into it does.
    pub objs: Vec<(crate::prefab::ObjPath, Entity)>,
    /// Made because something came near that leads into it, and let go
    /// once nothing has for its `keep_loaded` seconds.
    pub on_demand: bool,
    /// How long nothing has needed it, in seconds.
    idle: f32,
    /// Its sky, sun, post-processing and weather, while it is not current.
    kept: Surroundings,
    /// Told apart from every dimension made before it with its number:
    /// what a script holds of it is its number and this (see
    /// [`Dimensions::handle`]).
    serial: u16,
    /// The dungeon it holds, for a dungeon's own dimension.
    pub dungeon: Option<crate::dungeons::DungeonGate>,
}

impl Made {
    /// What it is called in a save: its asset and key, and for a dungeon's,
    /// where the dungeon is.
    pub fn name(&self) -> DimensionName {
        DimensionName { asset: self.asset, key: self.key }
    }
}

/// Which dimension a save means: the `.dimension` it is made from, none for
/// one made by code (a dungeon's), and its key among that asset's. A dungeon's
/// key is its site's (`DungeonGate::key`), the same each time it is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DimensionName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetGuid>,
    #[serde(default)]
    pub key: u64,
}

/// Every dimension the game has made besides the level's, and which one the
/// game is seen from. Absent, there is only the level's.
#[derive(Default)]
pub struct Dimensions {
    made: Vec<Made>,
    current: DimensionId,
    /// The level's surroundings, while another dimension is current.
    level: Surroundings,
    /// The level they were made in: a change of level lets them all go.
    made_in: Option<String>,
    /// How many have been made, for each one's serial.
    serials: u16,
}

impl Dimensions {
    /// The dimension the game is seen from.
    pub fn current(&self) -> DimensionId {
        self.current
    }

    pub fn made(&self) -> impl Iterator<Item = &Made> {
        self.made.iter()
    }

    pub fn get(&self, id: DimensionId) -> Option<&Made> {
        self.made.iter().find(|made| made.id == id)
    }

    /// The dimension made from `asset` as `key`, if there is one.
    pub fn find(&self, asset: AssetGuid, key: u64) -> Option<DimensionId> {
        self.made.iter().find(|made| made.asset == Some(asset) && made.key == key).map(|made| made.id)
    }

    /// The dimensions made on demand, as for a Portal into one.
    pub fn on_demand(&self) -> impl Iterator<Item = DimensionId> + '_ {
        self.made.iter().filter(|made| made.on_demand).map(|made| made.id)
    }

    /// The dimension made for `key`, whatever it was made from.
    pub fn find_by_key(&self, key: u64) -> Option<DimensionId> {
        self.made.iter().find(|made| made.key == key && key != 0).map(|made| made.id)
    }

    /// The Obj a dimension's file placed at `path`.
    pub fn obj_at(&self, id: DimensionId, path: &crate::prefab::ObjPath) -> Option<Entity> {
        self.get(id)?.objs.iter().find(|(at, _)| at == path).map(|(_, entity)| *entity)
    }

    /// Whether `id` is the level's or one made.
    pub fn exists(&self, id: DimensionId) -> bool {
        id.is_level() || self.get(id).is_some()
    }

    /// The settings of a dimension: the level's are an overworld's.
    pub fn settings(&self, id: DimensionId) -> DimensionSettings {
        self.get(id).map_or_else(|| DimensionSettings::of(DimensionKind::Overworld), |made| made.settings.clone())
    }

    /// The surroundings of a dimension as they are now: the world's for the
    /// current one, else those kept for it.
    pub fn surroundings(&self, world: &World, id: DimensionId) -> Surroundings {
        match (id == self.current, id.is_level()) {
            (true, _) => Surroundings::copy(world),
            (false, true) => self.level.clone(),
            (false, false) => self.get(id).map(|made| made.kept.clone()).unwrap_or_default(),
        }
    }

    fn next_id(&self) -> Option<DimensionId> {
        (1..DimensionId::MOST).map(DimensionId).find(|id| self.get(*id).is_none())
    }

    /// What a script holds of a dimension: its number, and above it its
    /// serial, so a dimension made later with the same number is not taken
    /// for it. 0 is the level's; none for one not made.
    pub fn handle(&self, id: DimensionId) -> Option<u32> {
        match id.is_level() {
            true => Some(0),
            false => self.get(id).map(|made| u32::from(id.0) | (u32::from(made.serial) << 16)),
        }
    }

    /// The dimension a script's handle names, while it is still made.
    pub fn from_handle(&self, handle: u32) -> Option<DimensionId> {
        let id = DimensionId((handle & 0xffff) as u16);
        (self.handle(id) == Some(handle)).then_some(id)
    }

    /// The dimension a save's name means, if it is made.
    pub fn named(&self, name: DimensionName) -> Option<DimensionId> {
        self.made.iter().find(|made| made.name() == name).map(|made| made.id)
    }
}

/// A script's handle of a dimension (see [`Dimensions::handle`]): 0, the
/// level's, in a world with no other.
pub fn handle_of(world: &World, id: DimensionId) -> Option<u32> {
    match world.resource::<Dimensions>() {
        Some(dimensions) => dimensions.handle(id),
        None => id.is_level().then_some(0),
    }
}

/// The dimension a script's handle names, while it is made.
pub fn from_handle(world: &World, handle: u32) -> Option<DimensionId> {
    match world.resource::<Dimensions>() {
        Some(dimensions) => dimensions.from_handle(handle),
        None => (handle == 0).then_some(DimensionId::LEVEL),
    }
}

/// The dimension a world's things are made round and its sound heard from:
/// the current one, or the level's in a world with no other.
pub fn current(world: &World) -> DimensionId {
    voxear_scene::CurrentDimension::of(world)
}

/// The dimension the editor's Scene view shows, where it is not the level's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewerDimension(pub DimensionId);

/// The dimension the game is seen from: the editor's Scene view's while it
/// has one, or else that of the game's first camera switched on, or the one
/// a Portal it looks through is in.
pub fn seen_from(world: &World) -> DimensionId {
    if world.resource::<crate::terrain::TerrainViewer>().is_some() {
        return world.resource::<ViewerDimension>().map_or(DimensionId::LEVEL, |viewer| viewer.0);
    }
    world
        .query_ref::<(Entity, &Camera)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, _)| voxear_scene::is_on(world, *entity, Switch::Camera))
        .min_by_key(|(entity, _)| entity.index())
        .map_or(DimensionId::LEVEL, |(entity, _)| voxear_render::ViewCarry::dimension(world, entity))
}

/// Makes a dimension of `settings` with nothing in it yet, for code to fill:
/// Objs put in it with [`set_dimension`] on their roots. Its surroundings
/// are `surroundings`, or with none, the level's as they are now (an
/// enclosed one's dark). Returns its number; none once there are as many
/// as there can be.
pub fn make_empty(
    world: &mut World,
    settings: DimensionSettings,
    surroundings: Option<Surroundings>,
    level: Option<&str>,
) -> Option<DimensionId> {
    let mut dimensions = world.remove_resource::<Dimensions>().unwrap_or_default();
    if dimensions.made.is_empty() {
        dimensions.made_in = level.map(str::to_string);
    }
    let made = dimensions.next_id().map(|id| {
        let kept = surroundings.unwrap_or_else(|| {
            let level_now = dimensions.surroundings(world, DimensionId::LEVEL);
            match settings.sky() {
                true => level_now,
                false => Surroundings::enclosed(level_now.post),
            }
        });
        let kept = if settings.weather() { kept } else { kept.cleared() };
        let roots = Vec::new();
        dimensions.serials = dimensions.serials.wrapping_add(1);
        let serial = dimensions.serials;
        let (asset, key, objs, on_demand, idle) = (None, 0, Vec::new(), false, 0.0);
        let made = Made { id, asset, key, settings, roots, objs, on_demand, idle, kept, serial, dungeon: None };
        dimensions.made.push(made);
        id
    });
    world.insert_resource(dimensions);
    made
}

/// Notes the Objs a dimension's file placed, and which asset and key it was
/// made from, once they are spawned.
pub fn record_made(
    world: &mut World,
    id: DimensionId,
    (asset, key): (Option<AssetGuid>, u64),
    objs: Vec<(crate::prefab::ObjPath, Entity)>,
) {
    let roots: Vec<Entity> =
        objs.iter().map(|(_, entity)| *entity).filter(|&entity| !world.has::<Parent>(entity)).collect();
    let dimensions = world.resource_mut::<Dimensions>();
    if let Some(made) = dimensions.and_then(|dimensions| dimensions.made.iter_mut().find(|made| made.id == id)) {
        (made.asset, made.key, made.roots, made.objs) = (asset, key, roots, objs);
    }
}

/// Notes which of its asset's dimensions a dimension is, for one made by
/// code (see [`Dimensions::find_by_key`]).
pub fn note_key(world: &mut World, id: DimensionId, key: u64) {
    let dimensions = world.resource_mut::<Dimensions>();
    if let Some(made) = dimensions.and_then(|dimensions| dimensions.made.iter_mut().find(|made| made.id == id)) {
        made.key = key;
    }
}

/// Marks a dimension as made on demand: let go once nothing has needed it
/// for its `keep_loaded` seconds (see [`idle_ones`]).
pub fn made_on_demand(world: &mut World, id: DimensionId) {
    set_on_demand(world, id, true);
}

/// Whether a dimension is let go once nothing has needed it for its
/// `keep_loaded` seconds, or kept until it is let go by name, as a script
/// keeps one it made.
pub fn set_on_demand(world: &mut World, id: DimensionId, on_demand: bool) {
    let dimensions = world.resource_mut::<Dimensions>();
    if let Some(made) = dimensions.and_then(|dimensions| dimensions.made.iter_mut().find(|made| made.id == id)) {
        (made.on_demand, made.idle) = (on_demand, 0.0);
    }
}

/// Notes which dungeon a dimension holds.
pub fn note_dungeon(world: &mut World, id: DimensionId, gate: crate::dungeons::DungeonGate) {
    let dimensions = world.resource_mut::<Dimensions>();
    if let Some(made) = dimensions.and_then(|dimensions| dimensions.made.iter_mut().find(|made| made.id == id)) {
        made.dungeon = Some(gate);
    }
}

/// Moves on how long each dimension made on demand has gone unneeded by
/// `delta` seconds, `needed` those something needs now, and returns those
/// unneeded for longer than they are kept, to let go.
pub fn idle_ones(world: &mut World, delta: f32, needed: &[DimensionId]) -> Vec<DimensionId> {
    let Some(dimensions) = world.resource_mut::<Dimensions>() else { return Vec::new() };
    let current = dimensions.current;
    let mut idle = Vec::new();
    for made in dimensions.made.iter_mut().filter(|made| made.on_demand) {
        match made.id == current || needed.contains(&made.id) {
            true => made.idle = 0.0,
            false => made.idle += delta.max(0.0),
        }
        if made.idle > made.settings.keep_loaded() {
            idle.push(made.id);
        }
    }
    idle
}

/// Puts a root Obj, and everything under it, in `dimension`: where it is
/// stays the same numbers, in that dimension's coordinates. A child Obj is
/// moved by its root. False where there is no such dimension.
pub fn move_obj(world: &mut World, obj: Entity, dimension: DimensionId) -> bool {
    let known = world.resource::<Dimensions>().is_some_and(|dimensions| dimensions.exists(dimension));
    if !(known || dimension.is_level()) || !world.is_alive(obj) {
        return false;
    }
    let root = root_of(world, obj);
    set_dimension(world, root, dimension);
    true
}

fn root_of(world: &World, obj: Entity) -> Entity {
    let mut at = obj;
    for _ in 0..64 {
        match world.get::<Parent>(at) {
            Some(parent) => at = parent.0,
            None => break,
        }
    }
    at
}

/// The Objs to destroy when a dimension goes: the root of every Obj in it,
/// but for persistent ones, which are moved to the level's.
fn letting_go(world: &mut World, id: DimensionId) -> Vec<Entity> {
    let roots: Vec<(Entity, bool)> = world
        .query_ref::<(Entity, &InDimension, Option<&ScenePersistence>)>()
        .into_iter()
        .filter(|(_, own, _)| own.0 == id)
        .map(|(entity, _, persistent)| (entity, persistent.is_some()))
        .collect();
    let mut doomed = Vec::new();
    for (root, persistent) in roots {
        match persistent {
            true => set_dimension(world, root, DimensionId::LEVEL),
            false => doomed.push(root),
        }
    }
    doomed
}

/// Lets a dimension go: returns its Objs' roots, to destroy as the engine
/// destroys Objs (`Engine::destroy_objs`); persistent ones are moved to the
/// level's. If it was current, the level's is current again. False where it
/// was not made.
pub fn let_go(world: &mut World, id: DimensionId) -> Option<Vec<Entity>> {
    if id.is_level() {
        return None;
    }
    let mut dimensions = world.remove_resource::<Dimensions>()?;
    let Some(at) = dimensions.made.iter().position(|made| made.id == id) else {
        world.insert_resource(dimensions);
        return None;
    };
    if dimensions.current == id {
        dimensions.level.put(world);
        dimensions.current = DimensionId::LEVEL;
        world.insert_resource(voxear_scene::CurrentDimension(DimensionId::LEVEL));
    }
    dimensions.made.remove(at);
    let doomed = letting_go(world, id);
    if let Some(looks) = world.resource_mut::<voxear_render::DimensionLooks>() {
        looks.0.remove(&id);
    }
    if !dimensions.made.is_empty() {
        world.insert_resource(dimensions);
    }
    Some(doomed)
}

/// A dimension file's sky, sun, post-processing and weather: the level's,
/// with its own lighting, `[sky]` and `[post]` laid over them as a scene's
/// are; dark all round where it has no sky, and clear where it has no
/// weather.
pub fn file_surroundings(
    engine: &mut crate::Engine,
    content: &mut crate::Content,
    file: &DimensionFile,
) -> Result<Surroundings> {
    let world = &engine.world;
    let level = match world.resource::<Dimensions>() {
        Some(dimensions) => dimensions.surroundings(world, DimensionId::LEVEL),
        None => Surroundings::copy(world),
    };
    let scene = &file.scene;
    let own = match scene.lighting.is_some() || scene.sky.is_some() || scene.post.is_some() {
        true => {
            let before = Surroundings::copy(&engine.world);
            level.put(&mut engine.world);
            let lighting = match &scene.lighting {
                Some(reference) => Some((reference.as_str(), content.lighting(&content.parse(reference)?)?)),
                None => None,
            };
            let lighting = lighting.as_ref().map(|(reference, lit)| (*reference, lit));
            let environment = crate::scene_file::Environment { lighting, sky: scene.sky, post: scene.post.as_ref() };
            let applied = crate::scene_file::apply_environment(engine, environment);
            let own = Surroundings::copy(&engine.world);
            before.put(&mut engine.world);
            applied?;
            own
        }
        false => level,
    };
    let settings = &file.settings;
    Ok(match (settings.sky(), settings.weather()) {
        (false, _) => Surroundings::enclosed(own.post()),
        (true, false) => own.cleared(),
        (true, true) => own,
    })
}

/// While a dimension other than the level's is current, puts the level's
/// sky, sun, post-processing and weather in the world for something that
/// sets them, as a scene's lighting does, and returns the current one's to
/// put back with [`level_back`]. None while the level's is current.
pub fn level_aside(world: &mut World) -> Option<Surroundings> {
    let dimensions = world.resource::<Dimensions>()?;
    if dimensions.current.is_level() {
        return None;
    }
    let level = dimensions.level.clone();
    let own = Surroundings::copy(world);
    level.put(world);
    Some(own)
}

/// Keeps what the world has now as the level's and puts back the current
/// dimension's, after [`level_aside`].
pub fn level_back(world: &mut World, own: Option<Surroundings>) {
    let Some(own) = own else { return };
    let level = Surroundings::copy(world);
    if let Some(dimensions) = world.resource_mut::<Dimensions>() {
        dimensions.level = level;
    }
    own.put(world);
}

/// Gives a dimension made new settings and surroundings, as when its file's
/// are changed while it is open in the editor. False where it was not made.
pub fn change(world: &mut World, id: DimensionId, settings: DimensionSettings, surroundings: Surroundings) -> bool {
    let Some(mut dimensions) = world.remove_resource::<Dimensions>() else { return false };
    let current = dimensions.current == id;
    let found = dimensions.made.iter_mut().find(|made| made.id == id).map(|made| {
        made.settings = settings;
        if !current {
            made.kept = surroundings.clone();
        }
    });
    if found.is_some() && current {
        surroundings.put(world);
    }
    world.insert_resource(dimensions);
    found.is_some()
}

/// What a dimension is called where it is listed: its file's name, "Level"
/// for the level's own, or what it is with its key for one made by code,
/// as a dungeon's.
pub fn name_of(world: &World, content: &crate::Content, id: DimensionId) -> String {
    if id.is_level() {
        return "Level".into();
    }
    let Some(made) = world.resource::<Dimensions>().and_then(|dimensions| dimensions.get(id)) else {
        return format!("Dimension {}", id.0);
    };
    let file = made.asset.and_then(|asset| content.db().get(asset)).map(|entry| {
        let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        name.strip_suffix(".dimension").unwrap_or(name).to_string()
    });
    match (file, made.settings.kind) {
        (Some(name), _) if made.key == 0 => name,
        (Some(name), _) => format!("{name} {:04x}", made.key & 0xffff),
        (None, DimensionKind::Dungeon) => format!("Dungeon {:04x}", made.key & 0xffff),
        (None, kind) => format!("{kind:?} {}", id.0),
    }
}

/// Each frame, before anything reads the sky or streams ground: follows the
/// camera into whichever dimension it is in, the world's sky, sun,
/// post-processing and weather becoming that dimension's; and once the
/// level has changed, lets every dimension go. Returns the Objs to destroy.
pub fn update(world: &mut World, level: Option<&str>) -> Vec<Entity> {
    let Some(mut dimensions) = world.remove_resource::<Dimensions>() else { return Vec::new() };
    if dimensions.made_in.as_deref() != level {
        // A new level: its own surroundings are the world's already, and what was made in the old one goes.
        let ids: Vec<DimensionId> = dimensions.made.iter().map(|made| made.id).collect();
        let mut doomed = Vec::new();
        for id in ids {
            doomed.extend(letting_go(world, id));
        }
        world.remove_resource::<voxear_scene::CurrentDimension>();
        world.remove_resource::<voxear_render::DimensionLooks>();
        return doomed;
    }
    let now = seen_from(world);
    if now != dimensions.current && dimensions.exists(now) {
        let leaving = Surroundings::copy(world);
        match dimensions.current.is_level() {
            true => dimensions.level = leaving,
            false => {
                if let Some(made) = dimensions.made.iter_mut().find(|made| made.id == dimensions.current) {
                    made.kept = leaving;
                }
            }
        }
        let coming = match now.is_level() {
            true => std::mem::take(&mut dimensions.level),
            false => dimensions.get(now).map(|made| made.kept.clone()).unwrap_or_default(),
        };
        coming.put(world);
        dimensions.current = now;
    }
    // Every other dimension's look, for a view through a Portal into one.
    let mut looks = voxear_render::DimensionLooks::default();
    if !dimensions.current.is_level() {
        looks.0.insert(DimensionId::LEVEL, dimensions.level.look());
    }
    for made in dimensions.made.iter().filter(|made| made.id != dimensions.current) {
        looks.0.insert(made.id, made.kept.look());
    }
    world.insert_resource(looks);
    world.insert_resource(voxear_scene::CurrentDimension(dimensions.current));
    world.insert_resource(dimensions);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dimension_file_is_a_scene_with_its_settings_and_kinds_fill_in_the_rest() {
        let text = "format = 1\nlighting = \"builtin:storybook\"\n\n[dimension]\nkind = \"dungeon\"\nkeep_loaded = 5.0\n\n[[obj]]\nid = 1\nname = \"Ground\"\n";
        let file = DimensionFile::parse(text, "Deep.dimension").unwrap();
        assert_eq!(file.settings.kind, DimensionKind::Dungeon);
        assert_eq!(file.scene.lighting.as_deref(), Some("builtin:storybook"));
        assert_eq!(file.scene.objs.len(), 1);
        let settings = &file.settings;
        assert!(!settings.sky() && !settings.weather());
        assert_eq!((settings.clock(), settings.instances()), (DimensionClock::None, DimensionInstances::EachSite));
        assert_eq!(settings.keep_loaded(), 5.0);
        let again = DimensionFile::parse(&file.to_toml().unwrap(), "again").unwrap();
        assert_eq!(again.settings, file.settings);
        assert_eq!(again.scene.objs.len(), 1);
        // A file with no [dimension] is an overworld's; one without its format still reads.
        let plain = DimensionFile::parse("[[obj]]\nid = 1\nname = \"A\"\n", "plain").unwrap();
        assert!(plain.settings.sky() && plain.settings.weather());
        assert!(DimensionFile::parse("[dimension]\nkind = \"nowhere\"\n", "bad").is_err());
    }

    #[test]
    fn the_world_takes_the_sky_of_the_dimension_its_camera_is_in() {
        let mut world = World::new();
        let level_sky = Sky { haze: 2.0, ..Sky::default() };
        world.insert_resource(level_sky);
        world.insert_resource(Weather::of(voxear_render::weather::WeatherKind::Rain));
        let camera = world.spawn((voxear_scene::Transform::default(), Camera::default()));
        let caves = make_empty(&mut world, DimensionSettings::of(DimensionKind::Caves), None, None).unwrap();
        assert_ne!(caves, DimensionId::LEVEL);
        assert!(update(&mut world, None).is_empty());
        assert_eq!(current(&world), DimensionId::LEVEL, "the camera is in the level's");
        assert!(move_obj(&mut world, camera, caves));
        update(&mut world, None);
        assert_eq!(current(&world), caves);
        assert_eq!(world.resource::<Sky>().map(|sky| sky.ambient), Some(0.0), "an enclosed dimension is dark");
        assert_eq!(world.resource::<Weather>().map(|weather| weather.now().rain), Some(0.0), "and clear");
        assert!(move_obj(&mut world, camera, DimensionId::LEVEL));
        update(&mut world, None);
        assert_eq!(world.resource::<Sky>().map(|sky| sky.haze), Some(2.0), "the level's sky comes back");
        assert!(world.resource::<Weather>().is_some_and(|weather| weather.now().rain > 0.0), "and its rain");
        // Letting it go takes its Objs and the camera back to the level's.
        let rock = world.spawn((voxear_scene::Transform::default(), InDimension(caves)));
        assert!(move_obj(&mut world, camera, caves));
        update(&mut world, None);
        let mut doomed = let_go(&mut world, caves).unwrap();
        doomed.sort_by_key(|entity| entity.to_bits());
        assert_eq!(doomed, vec![camera, rock]);
        assert_eq!(world.resource::<Sky>().map(|sky| sky.haze), Some(2.0));
        assert!(world.resource::<Dimensions>().is_none(), "with none made, there is only the level's");
    }

    #[test]
    fn a_change_of_level_lets_every_dimension_go() {
        let mut world = World::new();
        let made = make_empty(&mut world, DimensionSettings::default(), None, Some("Levels/A.level")).unwrap();
        let obj = world.spawn((voxear_scene::Transform::default(), InDimension(made)));
        let kept = world.spawn((voxear_scene::Transform::default(), InDimension(made), ScenePersistence));
        assert!(update(&mut world, Some("Levels/A.level")).is_empty());
        assert_eq!(update(&mut world, Some("Levels/B.level")), vec![obj]);
        assert!(world.resource::<Dimensions>().is_none());
        assert_eq!(dimension_of(&world, kept), DimensionId::LEVEL, "a persistent Obj stays, in the level's");
    }
}
