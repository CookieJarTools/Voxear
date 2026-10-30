//! The `.scene` file format: plain TOML, so scenes diff and merge cleanly in
//! version control. Builds will use a binary form made by the content build.
//!
//! ```toml
//! format = 1
//!
//! [[obj]]
//! id = 1
//! name = "Main Camera"
//! position = [0.0, 2.2, -7.0]
//! rotation = [12.0, 0.0, 0.0]   # Euler degrees, applied Z, X, then Y, as in Unity
//! [obj.camera]
//! fov = 55.0
//!
//! [[obj]]
//! id = 2
//! name = "Crate"
//! parent = 1
//! behaviours = ["Game.Spinner"]
//! [obj.renderer]
//! mesh = "builtin:cube"          # or "<model guid>#mesh0.0", or a project path
//! material = { color = [0.5, 0.3, 0.9] }
//! ```
//!
//! An optional `[sky]` sets the sky colours and `[post]` the
//! post-processing (see [`PostProcess`]). Like Unity's global volume, the
//! last scene loaded with either one sets it for the whole level.
//!
//! An Obj with `prefab = "..."` is a prefab instance; see [`crate::prefab`].

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, AssetRef};
use voxear_core::math::{EulerRot, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Light, LightKind, MeshRenderer, PostProcess, Renderer, Sky};
use voxear_scene::{Behaviours, Name, Parent, ScenePersistence, Transform};

use crate::{
    content::{Content, MaterialData, MaterialSource, linear, srgb},
    engine::Engine,
    prefab::{self, FlatObj, FromPrefab, ObjPath, PrefabInstance, Template},
};

pub const SCENE_FORMAT: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneFile {
    pub format: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sky: Option<SkyData>,
    /// A preset and the settings that differ from it, kept as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<toml::Table>,
    #[serde(default, rename = "obj", skip_serializing_if = "Vec::is_empty")]
    pub objs: Vec<ObjData>,
}

impl Default for SceneFile {
    fn default() -> Self {
        Self { format: SCENE_FORMAT, sky: None, post: None, objs: Vec::new() }
    }
}

/// The `[post]` table the current post-processing came from, so saving a
/// scene writes back its preset and overrides rather than every setting.
pub struct PostTable(pub toml::Table);

/// Sky colours are sRGB, like every colour in project files.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyData {
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub ground: [f32; 3],
    #[serde(default = "one_f32")]
    pub ambient: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjData {
    /// Unique within the file; `parent` refers to it.
    pub id: u32,
    /// Prefab instances keep the prefab root's name when this is empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Makes this Obj an instance of a prefab, by GUID or project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    /// Left out, the transform is the identity, or a prefab instance keeps
    /// its prefab root's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    /// Euler degrees, applied Z, then X, then Y, as in Unity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
    /// Survives level changes, like Unity's `DontDestroyOnLoad`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub persistent: bool,
    /// Full C# type names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviours: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<LightData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<RendererData>,
    /// Changes to Objs inside a prefab instance, by path (`"2"`, `"3/2"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, toml::Table>,
}

impl ObjData {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            prefab: None,
            parent: None,
            position: None,
            rotation: None,
            scale: None,
            persistent: false,
            behaviours: Vec::new(),
            camera: None,
            light: None,
            renderer: None,
            overrides: BTreeMap::new(),
        }
    }

    pub fn position(&self) -> [f32; 3] {
        self.position.unwrap_or([0.0; 3])
    }

    pub fn rotation(&self) -> [f32; 3] {
        self.rotation.unwrap_or([0.0; 3])
    }

    pub fn scale(&self) -> [f32; 3] {
        self.scale.unwrap_or([1.0; 3])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraData {
    #[serde(default = "default_fov")]
    pub fov: f32,
    #[serde(default = "one_f32", skip_serializing_if = "is_one_f32")]
    pub exposure: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum LightData {
    Directional {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_sun")]
        intensity: f32,
    },
    Point {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
    },
    Spot {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default = "default_inner")]
        inner_angle: f32,
        #[serde(default = "default_outer")]
        outer_angle: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererData {
    /// `builtin:cube|sphere|plane`, `<model guid>#mesh<m>.<p>`, or a project path.
    pub mesh: String,
    /// A `.material` or model material reference, or an inline material.
    /// The default grey material when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<MaterialRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaterialRef {
    Asset(String),
    Inline(MaterialData),
}

pub fn read(path: &Path) -> Result<SceneFile> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let scene: SceneFile = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    ensure!(scene.format == SCENE_FORMAT, "{} uses scene format {}, not {SCENE_FORMAT}", path.display(), scene.format);
    Ok(scene)
}

pub fn write(path: &Path, scene: &SceneFile) -> Result<()> {
    let text = format!("# Voxear scene\n{}", toml::to_string(scene)?);
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// Creates the scene's Objs, expanding prefab instances, and returns each
/// with its path in the scene. Their behaviours start together as one batch.
/// Objs in `skip`, and everything under them, are left out. Without a
/// renderer (headless runs) meshes are skipped.
pub fn spawn(
    engine: &mut Engine,
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    scene: &SceneFile,
    skip: &HashSet<ObjPath>,
) -> Result<Vec<(ObjPath, Entity)>> {
    if let Some(sky) = scene.sky {
        engine.world.insert_resource(Sky {
            zenith: Vec3::from(linear(sky.zenith)),
            horizon: Vec3::from(linear(sky.horizon)),
            ground: Vec3::from(linear(sky.ground)),
            ambient: sky.ambient,
        });
    }
    if let Some(table) = &scene.post {
        engine.world.insert_resource(PostProcess::from_toml(table)?);
        engine.world.insert_resource(PostTable(table.clone()));
    }

    let objs = without(prefab::expand(content, &scene.objs)?, skip);
    let template = Template::resolve(content, renderer, &objs)?;
    let entities = engine.spawn_template(&template, None, None)?;
    Ok(template.objs.iter().map(|o| o.path.clone()).zip(entities).collect())
}

/// Leaves out the Objs in `skip` and everything parented under them.
fn without(objs: Vec<FlatObj>, skip: &HashSet<ObjPath>) -> Vec<FlatObj> {
    if skip.is_empty() {
        return objs;
    }
    let mut skipped = skip.clone();
    loop {
        let before = skipped.len();
        for obj in &objs {
            if obj.parent.as_ref().is_some_and(|p| skipped.contains(p)) {
                skipped.insert(obj.path.clone());
            }
        }
        if skipped.len() == before {
            return objs.into_iter().filter(|o| !skipped.contains(&o.path)).collect();
        }
    }
}

/// Describes Objs as a scene, the inverse of [`spawn`]. Parents outside
/// `entities` are left out, making those Objs roots. Prefab instances are
/// saved as instances, with what changed inside them as overrides.
///
/// Not saved yet: Objs destroyed inside an instance, and cameras, lights or
/// renderers removed from one. Both come back from the prefab on reload.
pub fn capture(world: &World, content: &mut Content, entities: &[Entity]) -> SceneFile {
    let alive: Vec<Entity> = entities.iter().copied().filter(|e| world.is_alive(*e)).collect();
    let captured: HashSet<Entity> = alive.iter().copied().collect();
    // Objs inside a captured instance are saved as its overrides, not on their own.
    let instance_of = |entity: Entity| {
        world.get::<FromPrefab>(entity).filter(|f| !f.path.is_root() && captured.contains(&f.instance))
    };
    let top: Vec<Entity> = alive.iter().copied().filter(|e| instance_of(*e).is_none()).collect();
    let ids: HashMap<Entity, u32> = top.iter().enumerate().map(|(i, e)| (*e, i as u32 + 1)).collect();

    let mut objs = Vec::with_capacity(top.len());
    for &entity in &top {
        let mut obj = capture_obj(world, content, entity, &ids);
        if let Some(parent) = world.get::<Parent>(entity).and_then(|p| instance_of(p.0)) {
            tracing::warn!(
                "{} is parented inside a prefab instance, which scenes cannot save yet; it moves to the instance's root",
                obj.name
            );
            obj.parent = ids.get(&parent.instance).copied();
        }
        if let Some(PrefabInstance(guid)) = world.get::<PrefabInstance>(entity).copied() {
            let members: HashMap<ObjPath, Entity> = alive
                .iter()
                .filter_map(|&e| instance_of(e).filter(|f| f.instance == entity).map(|f| (f.path.clone(), e)))
                .collect();
            match capture_instance(world, content, guid, obj.clone(), &members, &ids) {
                Ok(instance) => obj = instance,
                Err(error) => tracing::warn!("{} is saved without its prefab link: {error:#}", obj.name),
            }
        }
        objs.push(obj);
    }

    let sky = world.resource::<Sky>().map(|sky| SkyData {
        zenith: round3(srgb(sky.zenith.to_array())),
        horizon: round3(srgb(sky.horizon.to_array())),
        ground: round3(srgb(sky.ground.to_array())),
        ambient: sky.ambient,
    });
    let post = world.resource::<PostTable>().map(|table| table.0.clone());
    SceneFile { format: SCENE_FORMAT, sky, post, objs }
}

/// An instance of `guid` whose root was captured as `root`, with everything
/// that differs from the prefab written as instance fields and overrides.
fn capture_instance(
    world: &World,
    content: &mut Content,
    guid: AssetGuid,
    root: ObjData,
    members: &HashMap<ObjPath, Entity>,
    ids: &HashMap<Entity, u32>,
) -> Result<ObjData> {
    let expected = prefab::expand_prefab(content, guid)?;
    let mut instance = ObjData::new(root.id, "");
    instance.prefab = Some(AssetRef::asset(guid, None).to_string());
    instance.parent = root.parent;
    let changed = |actual: [f32; 3], expected: [f32; 3]| Some(actual).filter(|a| !nearly_equal(*a, expected));

    let prefab_root = &expected.iter().find(|o| o.path.is_root()).expect("a prefab always has its root").data;
    instance.position = changed(root.position(), prefab_root.position());
    instance.rotation = changed(root.rotation(), prefab_root.rotation());
    instance.scale = changed(root.scale(), prefab_root.scale());
    if root.name != prefab_root.name {
        instance.name = root.name.clone();
    }
    instance.persistent = root.persistent && !prefab_root.persistent;
    match root.behaviours.strip_prefix(prefab_root.behaviours.as_slice()) {
        Some(added) => instance.behaviours = added.to_vec(),
        None => tracing::warn!("{}: behaviours removed from a prefab root are not saved yet", root.name),
    }
    if root.camera.is_some() && root.camera != prefab_root.camera {
        instance.camera = root.camera;
    }
    if root.light.is_some() && root.light != prefab_root.light {
        instance.light = root.light;
    }
    if root.renderer.is_some() && !same_renderer(content, &root.renderer, &prefab_root.renderer) {
        instance.renderer = root.renderer.clone();
    }

    for inner in expected.iter().filter(|o| !o.path.is_root()) {
        let Some(&entity) = members.get(&inner.path) else { continue };
        let actual = capture_obj(world, content, entity, ids);
        let changes = differences(content, &inner.data, &actual)?;
        if !changes.is_empty() {
            instance.overrides.insert(inner.path.to_string(), changes);
        }
    }
    Ok(instance)
}

/// The fields of `actual` that differ from `expected`, as an override.
fn differences(content: &Content, expected: &ObjData, actual: &ObjData) -> Result<toml::Table> {
    let mut changes = toml::Table::new();
    let mut put = |key: &str, value: toml::Value| {
        changes.insert(key.to_string(), value);
    };
    if actual.name != expected.name {
        put("name", actual.name.clone().into());
    }
    for (key, actual, expected) in [
        ("position", actual.position(), expected.position()),
        ("rotation", actual.rotation(), expected.rotation()),
        ("scale", actual.scale(), expected.scale()),
    ] {
        if !nearly_equal(actual, expected) {
            put(key, toml::Value::try_from(actual)?);
        }
    }
    if actual.persistent != expected.persistent {
        put("persistent", actual.persistent.into());
    }
    if actual.behaviours != expected.behaviours {
        put("behaviours", toml::Value::try_from(&actual.behaviours)?);
    }
    if let Some(camera) = actual.camera.filter(|c| Some(*c) != expected.camera) {
        put("camera", toml::Value::try_from(camera)?);
    }
    if let Some(light) = actual.light.filter(|l| Some(*l) != expected.light) {
        put("light", toml::Value::try_from(light)?);
    }
    if actual.renderer.is_some() && !same_renderer(content, &actual.renderer, &expected.renderer) {
        put("renderer", toml::Value::try_from(&actual.renderer)?);
    }
    Ok(changes)
}

/// Compares renderers by what they load, so `Models/Ship.gltf` and its GUID
/// count as the same mesh.
fn same_renderer(content: &Content, a: &Option<RendererData>, b: &Option<RendererData>) -> bool {
    let parse = |text: &str| content.parse(text).ok();
    let material = |m: &Option<MaterialRef>| match m {
        None => Some(MaterialSource::Inline(default_material())),
        Some(MaterialRef::Inline(data)) => Some(MaterialSource::Inline(*data)),
        Some(MaterialRef::Asset(text)) => parse(text).map(MaterialSource::Asset),
    };
    match (a, b) {
        (Some(a), Some(b)) => parse(&a.mesh) == parse(&b.mesh) && material(&a.material) == material(&b.material),
        (None, None) => true,
        _ => false,
    }
}

/// Saved transforms are rounded and Euler angles go through a quaternion, so
/// exact comparison would see changes that are not there.
fn nearly_equal(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-3)
}

fn capture_obj(world: &World, content: &Content, entity: Entity, ids: &HashMap<Entity, u32>) -> ObjData {
    let name = world.get::<Name>(entity).map_or("Obj", |n| n.0.as_str());
    // Objs inside a prefab instance have no id of their own in the file.
    let mut obj = ObjData::new(ids.get(&entity).copied().unwrap_or_default(), name);
    if let Some(transform) = world.get::<Transform>(entity) {
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        obj.position = Some(round3(transform.position.to_array())).filter(|p| !is_zero(p));
        obj.rotation = Some(round3([x, y, z].map(f32::to_degrees))).filter(|r| !is_zero(r));
        obj.scale = Some(round3(transform.scale.to_array())).filter(|s| !is_one(s));
    }
    obj.parent = world.get::<Parent>(entity).and_then(|p| ids.get(&p.0).copied());
    obj.persistent = world.has::<ScenePersistence>(entity);
    obj.behaviours =
        world.get::<Behaviours>(entity).map(|b| b.type_names().map(str::to_string).collect()).unwrap_or_default();
    obj.camera = world.get::<Camera>(entity).map(|c| CameraData { fov: c.fov_y_degrees, exposure: c.exposure });
    obj.light = world.get::<Light>(entity).map(light_data);
    obj.renderer = world.get::<MeshRenderer>(entity).and_then(|r| {
        let mesh = content.mesh_ref(r.mesh)?.to_string();
        let material = match content.material_source(r.material) {
            Some(MaterialSource::Asset(reference)) => Some(MaterialRef::Asset(reference.to_string())),
            Some(MaterialSource::Inline(data)) if *data == default_material() => None,
            Some(MaterialSource::Inline(data)) => Some(MaterialRef::Inline(*data)),
            None => None,
        };
        Some(RendererData { mesh, material })
    });
    obj
}

pub(crate) fn light_component(light: LightData) -> Light {
    match light {
        LightData::Directional { color, intensity } => Light::directional(Vec3::from(linear(color)), intensity),
        LightData::Point { color, intensity, range } => Light::point(Vec3::from(linear(color)), intensity, range),
        LightData::Spot { color, intensity, range, inner_angle, outer_angle } => {
            Light::spot(Vec3::from(linear(color)), intensity, range, inner_angle, outer_angle)
        }
    }
}

fn light_data(light: &Light) -> LightData {
    let color = round3(srgb(light.color.to_array()));
    let intensity = light.intensity;
    match light.kind {
        LightKind::Directional => LightData::Directional { color, intensity },
        LightKind::Point => LightData::Point { color, intensity, range: light.range },
        LightKind::Spot => LightData::Spot {
            color,
            intensity,
            range: light.range,
            inner_angle: light.spot_inner_degrees,
            outer_angle: light.spot_outer_degrees,
        },
    }
}

pub(crate) fn default_material() -> MaterialData {
    toml::from_str("").expect("every material field has a default")
}

/// Keeps saved files readable: `12.0`, not `12.000000953674316`.
fn round3(values: [f32; 3]) -> [f32; 3] {
    values.map(|v| {
        let rounded = (v * 10_000.0).round() / 10_000.0;
        if rounded == -0.0 { 0.0 } else { rounded }
    })
}

fn is_zero(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}

fn is_one(v: &[f32; 3]) -> bool {
    *v == [1.0; 3]
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_one_f32(v: &f32) -> bool {
    *v == 1.0
}

fn one_f32() -> f32 {
    1.0
}

fn white() -> [f32; 3] {
    [1.0; 3]
}

fn default_fov() -> f32 {
    60.0
}

fn default_sun() -> f32 {
    3.0
}

fn default_lamp() -> f32 {
    10.0
}

fn default_range() -> f32 {
    10.0
}

fn default_inner() -> f32 {
    30.0
}

fn default_outer() -> f32 {
    45.0
}
