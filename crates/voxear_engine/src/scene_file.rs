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

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Light, LightKind, MeshRenderer, PostProcess, Renderer, Sky};
use voxear_scene::{Behaviours, Name, Parent, ScenePersistence, Transform};

use crate::{
    content::{Content, MaterialData, MaterialSource, linear, srgb},
    engine::Engine,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjData {
    /// Unique within the file; `parent` refers to it.
    pub id: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub position: [f32; 3],
    /// Euler degrees, applied Z, then X, then Y, as in Unity.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: [f32; 3],
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: [f32; 3],
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
}

impl ObjData {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            parent: None,
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
            persistent: false,
            behaviours: Vec::new(),
            camera: None,
            light: None,
            renderer: None,
        }
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

/// Creates the scene's Objs and returns them with their ids; their
/// behaviours start together as one batch. Objs listed in `skip` (and their
/// children) are left out. Without a renderer (headless runs) meshes are
/// skipped.
pub fn spawn(
    engine: &mut Engine,
    content: &mut Content,
    mut renderer: Option<&mut Renderer>,
    scene: &SceneFile,
    skip: &HashSet<u32>,
) -> Result<Vec<(u32, Entity)>> {
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

    let skipped = with_descendants(scene, skip);
    let mut ids = HashMap::with_capacity(scene.objs.len());
    let mut spawned = Vec::with_capacity(scene.objs.len());
    for obj in scene.objs.iter().filter(|o| !skipped.contains(&o.id)) {
        let entity =
            spawn_obj(engine, content, renderer.as_deref_mut(), obj).with_context(|| format!("Obj {}", obj.name))?;
        if ids.insert(obj.id, entity).is_some() {
            bail!("two Objs have id {}", obj.id);
        }
        spawned.push((obj.id, entity));
    }
    for obj in scene.objs.iter().filter(|o| !skipped.contains(&o.id)) {
        let Some(parent) = obj.parent else { continue };
        let parent = *ids
            .get(&parent)
            .with_context(|| format!("{} has parent {parent}, which is not in the scene", obj.name))?;
        engine.world.insert(ids[&obj.id], Parent(parent));
    }
    Ok(spawned)
}

/// `roots` plus every Obj in the file below them.
fn with_descendants(scene: &SceneFile, roots: &HashSet<u32>) -> HashSet<u32> {
    let mut found = roots.clone();
    loop {
        let before = found.len();
        for obj in &scene.objs {
            if obj.parent.is_some_and(|p| found.contains(&p)) {
                found.insert(obj.id);
            }
        }
        if found.len() == before {
            return found;
        }
    }
}

fn spawn_obj(
    engine: &mut Engine,
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    obj: &ObjData,
) -> Result<Entity> {
    let [x, y, z] = obj.rotation.map(f32::to_radians);
    // Unity applies Z, then X, then Y: R = Ry · Rx · Rz.
    let rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
    let transform = Transform::new(Vec3::from(obj.position), rotation, Vec3::from(obj.scale));
    let entity = engine.spawn_obj(&obj.name, transform, &obj.behaviours)?;
    if let Some(camera) = obj.camera {
        engine
            .world
            .insert(entity, Camera { fov_y_degrees: camera.fov, exposure: camera.exposure, ..Camera::default() });
    }
    if let Some(light) = obj.light {
        engine.world.insert(entity, light_component(light));
    }
    if obj.persistent {
        engine.world.insert(entity, ScenePersistence);
    }
    if let (Some(data), Some(renderer)) = (&obj.renderer, renderer) {
        let mesh = content.mesh(renderer, &content.parse(&data.mesh)?)?;
        let source = match &data.material {
            Some(MaterialRef::Asset(reference)) => MaterialSource::Asset(content.parse(reference)?),
            Some(MaterialRef::Inline(material)) => MaterialSource::Inline(*material),
            None => MaterialSource::Inline(default_material()),
        };
        let material = content.material(renderer, &source)?;
        engine.world.insert(entity, MeshRenderer { mesh, material });
    }
    Ok(entity)
}

/// Describes Objs as a scene, the inverse of [`spawn`]. Parents outside
/// `entities` are left out, making those Objs roots.
pub fn capture(world: &World, content: &Content, entities: &[Entity]) -> SceneFile {
    let ids: HashMap<Entity, u32> = entities.iter().enumerate().map(|(i, e)| (*e, i as u32 + 1)).collect();
    let objs = entities
        .iter()
        .filter(|e| world.is_alive(**e))
        .map(|&entity| capture_obj(world, content, entity, &ids))
        .collect();
    let sky = world.resource::<Sky>().map(|sky| SkyData {
        zenith: round3(srgb(sky.zenith.to_array())),
        horizon: round3(srgb(sky.horizon.to_array())),
        ground: round3(srgb(sky.ground.to_array())),
        ambient: sky.ambient,
    });
    let post = world.resource::<PostTable>().map(|table| table.0.clone());
    SceneFile { format: SCENE_FORMAT, sky, post, objs }
}

fn capture_obj(world: &World, content: &Content, entity: Entity, ids: &HashMap<Entity, u32>) -> ObjData {
    let name = world.get::<Name>(entity).map_or("Obj", |n| n.0.as_str());
    let mut obj = ObjData::new(ids[&entity], name);
    if let Some(transform) = world.get::<Transform>(entity) {
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        obj.position = round3(transform.position.to_array());
        obj.rotation = round3([x, y, z].map(f32::to_degrees));
        obj.scale = round3(transform.scale.to_array());
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

fn light_component(light: LightData) -> Light {
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

fn default_material() -> MaterialData {
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

fn one() -> [f32; 3] {
    [1.0; 3]
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
