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
//! components = ["Game.Spin"]     # C# IComponent structs, which systems work on
//! [obj.fields."Game.Spinner"]
//! degrees_per_second = 45.0
//! [obj.fields."Game.Spin"]
//! speed = 90.0
//! [obj.renderer]
//! mesh = "builtin:cube"          # or "<model guid>#mesh0.0", or a project path
//! material = { color = [0.5, 0.3, 0.9] }
//! ```
//!
//! An Obj's `[obj.audio]` is an Audio Source, and `audio_listener = true`
//! hears the game from it (see [`crate::audio`]). `[obj.lod_group]` is a
//! LOD Group, which shows one of its levels' Objs at a time (see
//! [`crate::lod_groups`]). `layer = 3` puts an Obj on a layer by its
//! number, which stays the same when the project renames it; a file
//! written by hand may name it instead, as `layer = "Player"` (see
//! [`LayerRef`]). `[obj.collider]`, `[obj.rigidbody]` and
//! `[obj.character]` are its physics (see [`crate::physics`]), with
//! `[obj.joint]` holding it to another (see [`crate::joints`]), and
//! `[obj.vehicle]` and `[obj.wheel]` a vehicle and its wheels,
//! `[obj.collider2d]` and `[obj.rigidbody2d]` its 2D physics (see
//! [`crate::physics2d`]).
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
use voxear_render::{
    Camera, Light, LightKind, LodGroup, Look, LookName, MAX_TRANSITIONS, MeshRenderer, PostProcess, PostVolume, Renderer,
    Sky, SkyBody, SkyMode, SunLook, VolumeShape,
};
use voxear_scene::{
    BehaviourSpec, Behaviours, Layer, Layers, Name, Parent, SceneComponents, ScenePersistence, Transform,
};

use crate::{
    audio::{AudioListener, AudioSource, AudioSourceData, audio_source_data},
    content::{Content, MaterialData, MaterialSource, linear, srgb},
    engine::{EditBehaviours, Engine},
    joints::JointData,
    lod_groups::LodGroupData,
    physics::{CharacterController, Collider, Joint, Rigidbody, Vehicle, Wheel},
    physics2d::{Collider2D, Rigidbody2D},
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

/// The sky came from the scene's look rather than its own `[sky]`, so a
/// save leaves it out, and changing the look changes the sky.
pub struct SkyFromLook;

/// Sky colours are sRGB, like every colour in project files.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyData {
    /// `"gradient"`, the colours below, or `"atmosphere"`, the sky of a
    /// planet like Earth lit by the sun.
    #[serde(default, skip_serializing_if = "is_gradient")]
    pub mode: SkyModeData,
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub ground: [f32; 3],
    #[serde(default = "one_f32")]
    pub ambient: f32,
    /// How bright the stars are, 1 as the night sky shows them; they show
    /// only when the sky is dark.
    #[serde(default = "one_f32", skip_serializing_if = "is_one_f32")]
    pub stars: f32,
}

/// A [`SkyMode`] as scene files write it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkyModeData {
    #[default]
    Gradient,
    Atmosphere,
}

/// A [`SkyBody`] as scene files write it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkyBodyData {
    #[default]
    Sun,
    Moon,
}

fn is_sun(body: &SkyBodyData) -> bool {
    *body == SkyBodyData::Sun
}

fn is_gradient(mode: &SkyModeData) -> bool {
    *mode == SkyModeData::Gradient
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
    /// C# components (`IComponent` structs), by full type name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    /// Values for the behaviours' and components' serialized fields, by type
    /// and then field name. Field names may be written in snake_case.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<LightData>,
    /// A post volume, whose post-processing blends in as the camera comes
    /// into it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_volume: Option<PostVolumeData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<RendererData>,
    /// An Audio Source, which plays a sound from the Obj.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioSourceData>,
    /// Hears the game's sound from here rather than from the camera.
    #[serde(default, skip_serializing_if = "is_false")]
    pub audio_listener: bool,
    /// A LOD Group, which shows one of its levels' Objs at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lod_group: Option<LodGroupData>,
    /// Its layer, when not Default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider: Option<Collider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rigidbody: Option<Rigidbody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterController>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<JointData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider2d: Option<Collider2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rigidbody2d: Option<Rigidbody2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<Vehicle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wheel: Option<Wheel>,
    /// Changes to Objs inside a prefab instance, by path (`"2"`, `"3/2"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, toml::Table>,
    /// Components a prefab locks, so its instances show designers only some
    /// fields: by component (a behaviour's type, or `renderer`, `light`,
    /// `camera` or `audio`), each with the fields still shown. Only a prefab's own file
    /// sets them; an instance cannot change them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locks: BTreeMap<String, Vec<String>>,
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
            components: Vec::new(),
            fields: BTreeMap::new(),
            camera: None,
            light: None,
            post_volume: None,
            renderer: None,
            audio: None,
            audio_listener: false,
            layer: None,
            collider: None,
            rigidbody: None,
            character: None,
            joint: None,
            collider2d: None,
            rigidbody2d: None,
            vehicle: None,
            wheel: None,
            lod_group: None,
            overrides: BTreeMap::new(),
            locks: BTreeMap::new(),
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
    /// How near the camera sees, in metres.
    #[serde(default = "default_near", skip_serializing_if = "is_default_near")]
    pub near: f32,
}

/// A post volume as a scene writes it:
///
/// ```toml
/// [obj.post_volume]
/// shape = "box"            # or "sphere", or "global" for everywhere
/// blend_distance = 2.0     # metres outside it that it fades in over
/// weight = 1.0
/// priority = 0
/// [obj.post_volume.post]   # what it changes, as [post] writes it
/// bloom = { intensity = 0.2 }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostVolumeData {
    #[serde(default)]
    pub shape: VolumeShapeData,
    #[serde(default = "one_f32")]
    pub blend_distance: f32,
    #[serde(default = "one_f32")]
    pub weight: f32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub priority: i32,
    #[serde(default)]
    pub post: toml::Table,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeShapeData {
    Global,
    #[default]
    Box,
    Sphere,
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

impl PostVolumeData {
    pub fn component(&self) -> Result<PostVolume> {
        let shape = match self.shape {
            VolumeShapeData::Global => VolumeShape::Global,
            VolumeShapeData::Box => VolumeShape::Box,
            VolumeShapeData::Sphere => VolumeShape::Sphere,
        };
        let volume = PostVolume {
            shape,
            blend_distance: self.blend_distance,
            weight: self.weight,
            priority: self.priority,
            overrides: self.post.clone(),
        };
        volume.check()?;
        Ok(volume)
    }

    pub fn of(volume: &PostVolume) -> Self {
        let shape = match volume.shape {
            VolumeShape::Global => VolumeShapeData::Global,
            VolumeShape::Box => VolumeShapeData::Box,
            VolumeShape::Sphere => VolumeShapeData::Sphere,
        };
        Self {
            shape,
            blend_distance: volume.blend_distance,
            weight: volume.weight,
            priority: volume.priority,
            post: volume.overrides.clone(),
        }
    }
}

/// A light as a scene writes it. A `cookie` is a picture the light shines
/// through, and a `profile` an `.ies` file, each by GUID or project path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum LightData {
    Directional {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_sun")]
        intensity: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cookie: Option<String>,
        /// How many metres the cookie covers before it repeats.
        #[serde(default = "default_cookie_size")]
        cookie_size: f32,
        /// How wide the sun looks, in degrees, which softens its shadows.
        #[serde(default = "default_softness")]
        softness: f32,
        /// sRGB: the colour its shadows take, as a share of its light.
        #[serde(default, skip_serializing_if = "is_black3")]
        shadow_tint: [f32; 3],
        /// What it shows as in the sky: `"sun"`, or `"moon"` for a night sky.
        #[serde(default, skip_serializing_if = "is_sun")]
        body: SkyBodyData,
    },
    Point {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cookie: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
    },
    /// A panel shining from its face along the Obj's +Z, `width` along +X.
    Rectangle {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default = "one")]
        width: f32,
        #[serde(default = "one")]
        height: f32,
    },
    Disc {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default = "half")]
        radius: f32,
    },
    /// A glowing capsule along the Obj's +X.
    Tube {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        #[serde(default = "default_range")]
        range: f32,
        #[serde(default = "one")]
        length: f32,
        #[serde(default = "default_tube_radius")]
        radius: f32,
    },
}

fn one() -> f32 {
    1.0
}

fn half() -> f32 {
    0.5
}

fn default_tube_radius() -> f32 {
    0.05
}

fn default_cookie_size() -> f32 {
    10.0
}

fn default_softness() -> f32 {
    0.5
}

fn is_black3(colour: &[f32; 3]) -> bool {
    colour.iter().all(|&c| c <= 0.0)
}

impl LightData {
    /// The light's cookie and IES profile, as the scene names them.
    pub fn assets(&self) -> (Option<&str>, Option<&str>) {
        match self {
            Self::Directional { cookie, .. } => (cookie.as_deref(), None),
            Self::Point { profile, .. } => (None, profile.as_deref()),
            Self::Spot { cookie, profile, .. } => (cookie.as_deref(), profile.as_deref()),
            Self::Rectangle { .. } | Self::Disc { .. } | Self::Tube { .. } => (None, None),
        }
    }
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
    /// Shifts which LOD of the mesh is drawn: 1 switches to simpler ones
    /// sooner, -1 later (see `MeshRenderer::lod_bias`).
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub lod_bias: f32,
    /// A LOD drawn at every distance. Left out, or -1, the distance picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_lod: Option<i32>,
    /// Where each LOD ends, as the share of the screen's height each shows
    /// down to (see `MeshRenderer::lod_transitions`). Left out, or empty, the
    /// engine switches them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lod_transitions: Vec<f32>,
}

impl RendererData {
    /// The LOD it forces, if any.
    pub fn forced_lod(&self) -> Option<u8> {
        self.force_lod.filter(|lod| *lod >= 0).map(|lod| lod.min(255) as u8)
    }

    /// Where its LODs end, as the renderer keeps them: those not written end
    /// at nothing, so the LOD before shows all the way down.
    pub fn transitions(&self) -> Option<[f32; MAX_TRANSITIONS]> {
        if self.lod_transitions.is_empty() {
            return None;
        }
        let mut ends = [0.0; MAX_TRANSITIONS];
        for (end, &written) in ends.iter_mut().zip(&self.lod_transitions) {
            *end = written.clamp(0.0, 1.0);
        }
        Some(ends)
    }
}

/// Where a renderer's LODs end, as a file writes them: rounded, with the
/// ends at nothing after the last that says something left out.
fn written_transitions(ends: Option<[f32; MAX_TRANSITIONS]>) -> Vec<f32> {
    let mut written: Vec<f32> = ends.map_or_else(Vec::new, |ends| ends.map(|end| (end * 1e4).round() / 1e4).to_vec());
    while written.last() == Some(&0.0) && written.len() > 1 {
        written.pop();
    }
    written
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaterialRef {
    Asset(String),
    Inline(MaterialData),
}

pub fn read(path: &Path) -> Result<SceneFile> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text, &path.display().to_string())
}

/// A scene from its text; `name` says which file it was, for errors.
pub fn parse(text: &str, name: &str) -> Result<SceneFile> {
    let scene: SceneFile = toml::from_str(text).with_context(|| format!("reading {name}"))?;
    ensure!(scene.format == SCENE_FORMAT, "{name} uses scene format {}, not {SCENE_FORMAT}", scene.format);
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
    apply_environment(engine, scene.sky, scene.post.as_ref())?;
    let objs = without(prefab::expand(content, &scene.objs)?, skip);
    let template = Template::resolve(content, renderer, &objs)?;
    let entities = engine.spawn_template(&template, None, None)?;
    Ok(template.objs.iter().map(|o| o.path.clone()).zip(entities).collect())
}

/// Sets the level's sky and post-processing from a scene that has them.
/// What a scene leaves out stays as it was.
pub fn apply_environment(engine: &mut Engine, sky: Option<SkyData>, post: Option<&toml::Table>) -> Result<()> {
    // A look, picked in [post], brings its sun, and its sky where the scene has none of its own.
    let look = match post.and_then(|table| table.get("look")).and_then(|value| value.as_str()) {
        Some(name) => Some(Look::of(LookName::parse(name)?)),
        None => None,
    };
    // A scene without [post] leaves the look as it was; one with [post] and no look drops it, sky and all.
    match (look, post) {
        (Some(look), _) => drop(engine.world.insert_resource(look.sun)),
        (None, Some(_)) => {
            engine.world.remove_resource::<SunLook>();
            if engine.world.remove_resource::<SkyFromLook>().is_some() && sky.is_none() {
                engine.world.insert_resource(Sky::default());
            }
        }
        (None, None) => {}
    }
    if let (Some(look), None) = (look, sky) {
        engine.world.insert_resource(look.sky);
        engine.world.insert_resource(SkyFromLook);
    }
    if sky.is_some() {
        engine.world.remove_resource::<SkyFromLook>();
    }
    if let Some(sky) = sky {
        engine.world.insert_resource(Sky {
            mode: match sky.mode {
                SkyModeData::Gradient => SkyMode::Gradient,
                SkyModeData::Atmosphere => SkyMode::Atmosphere,
            },
            zenith: Vec3::from(linear(sky.zenith)),
            horizon: Vec3::from(linear(sky.horizon)),
            ground: Vec3::from(linear(sky.ground)),
            ambient: sky.ambient,
            stars: sky.stars,
        });
    }
    if let Some(table) = post {
        engine.world.insert_resource(PostProcess::from_toml(table)?);
        engine.world.insert_resource(PostTable(table.clone()));
    }
    Ok(())
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
    let objs: Vec<(ObjPath, Entity)> = entities.iter().map(|&e| (ObjPath::default(), e)).collect();
    capture_scene(world, content, &objs)
}

/// [`capture`], keeping each Obj's id from where it was loaded (a single-id
/// [`ObjPath`]) so the scene reads back with the same paths, as play mode
/// needs for persistent Objs to be recognised. Objs without one, such as
/// those created at runtime, get ids after the highest.
pub fn capture_scene(world: &World, content: &mut Content, objs: &[(ObjPath, Entity)]) -> SceneFile {
    capture_scene_with_ids(world, content, objs).0
}

/// [`capture_scene`], and the id each Obj that is not inside a prefab
/// instance was given, for keeping track of Objs that had none.
pub fn capture_scene_with_ids(
    world: &World,
    content: &mut Content,
    objs: &[(ObjPath, Entity)],
) -> (SceneFile, HashMap<Entity, u32>) {
    let alive: Vec<Entity> = objs.iter().map(|(_, e)| *e).filter(|e| world.is_alive(*e)).collect();
    let captured: HashSet<Entity> = alive.iter().copied().collect();
    // Objs inside a captured instance are saved as its overrides, not on their own.
    let instance_of = |entity: Entity| {
        world.get::<FromPrefab>(entity).filter(|f| !f.path.is_root() && captured.contains(&f.instance))
    };
    let top: Vec<Entity> = alive.iter().copied().filter(|e| instance_of(*e).is_none()).collect();
    let loaded: HashMap<Entity, u32> =
        objs.iter().filter(|(path, _)| path.ids().len() == 1).map(|(path, e)| (*e, path.ids()[0])).collect();
    let mut next = loaded.values().max().copied().unwrap_or_default();
    let mut ids = HashMap::with_capacity(top.len());
    for &entity in &top {
        let id = loaded.get(&entity).copied().unwrap_or_else(|| {
            next += 1;
            next
        });
        ids.insert(entity, id);
    }

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

    let sky = world.resource::<Sky>().filter(|_| world.resource::<SkyFromLook>().is_none()).map(|sky| SkyData {
        mode: match sky.mode {
            SkyMode::Gradient => SkyModeData::Gradient,
            SkyMode::Atmosphere => SkyModeData::Atmosphere,
        },
        zenith: round3(srgb(sky.zenith.to_array())),
        horizon: round3(srgb(sky.horizon.to_array())),
        ground: round3(srgb(sky.ground.to_array())),
        ambient: sky.ambient,
        stars: sky.stars,
    });
    let post = world.resource::<PostTable>().map(|table| table.0.clone());
    (SceneFile { format: SCENE_FORMAT, sky, post, objs }, ids)
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
    let mut expected = prefab::expand_prefab(content, guid)?;
    // Inside the prefab, references read from its root; in this file, from the instance.
    for obj in &mut expected {
        prefab::map_obj_refs(&mut obj.data, &|target| ObjPath::new(root.id).join(target))?;
        obj.data.layer = LayerRef::as_saved(&obj.data.layer, world);
    }
    let mut instance = ObjData::new(root.id, "");
    // By path, as scenes are written by hand; the GUID only when the asset is not in the database.
    let prefab_path = content.db().get(guid).map(|entry| entry.path.clone());
    instance.prefab = Some(prefab_path.unwrap_or_else(|| AssetRef::asset(guid, None).to_string()));
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
    match root.components.strip_prefix(prefab_root.components.as_slice()) {
        Some(added) => instance.components = added.to_vec(),
        None => tracing::warn!("{}: components removed from a prefab root are not saved yet", root.name),
    }
    instance.fields = changed_fields(&prefab_root.fields, &root.fields);
    if root.camera.is_some() && root.camera != prefab_root.camera {
        instance.camera = root.camera;
    }
    if root.post_volume.is_some() && root.post_volume != prefab_root.post_volume {
        instance.post_volume = root.post_volume.clone();
    }
    if root.light.is_some() && root.light != prefab_root.light {
        instance.light = root.light;
    }
    if root.renderer.is_some() && !same_renderer(content, &root.renderer, &prefab_root.renderer) {
        instance.renderer = root.renderer.clone();
    }
    if root.audio.is_some() && root.audio != prefab_root.audio {
        instance.audio = root.audio.clone();
    }
    instance.audio_listener = root.audio_listener && !prefab_root.audio_listener;
    if root.lod_group.is_some() && root.lod_group != prefab_root.lod_group {
        instance.lod_group = root.lod_group.clone();
    }
    if root.layer != prefab_root.layer {
        instance.layer = Some(root.layer.clone().unwrap_or(LayerRef::Number(0)));
    }
    if root.collider.is_some() && root.collider != prefab_root.collider {
        instance.collider = root.collider;
    }
    if root.rigidbody.is_some() && root.rigidbody != prefab_root.rigidbody {
        instance.rigidbody = root.rigidbody;
    }
    if root.character.is_some() && root.character != prefab_root.character {
        instance.character = root.character;
    }
    if root.joint.is_some() && root.joint != prefab_root.joint {
        instance.joint = root.joint.clone();
    }
    if root.collider2d.is_some() && root.collider2d != prefab_root.collider2d {
        instance.collider2d = root.collider2d;
    }
    if root.rigidbody2d.is_some() && root.rigidbody2d != prefab_root.rigidbody2d {
        instance.rigidbody2d = root.rigidbody2d;
    }
    if root.vehicle.is_some() && root.vehicle != prefab_root.vehicle {
        instance.vehicle = root.vehicle;
    }
    if root.wheel.is_some() && root.wheel != prefab_root.wheel {
        instance.wheel = root.wheel;
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

/// A component's every value as a table, those at their defaults too, as
/// an override writes it so that none of the prefab's stays.
fn whole<T: Serialize>(value: &T, defaults: &T) -> Result<toml::Value> {
    let mut table = toml::Table::try_from(value)?;
    for (key, default) in toml::Table::try_from(defaults)? {
        table.entry(key).or_insert(default);
    }
    Ok(toml::Value::Table(table))
}

/// The fields of `actual` that differ from `expected`, as an override.
pub(crate) fn differences(content: &Content, expected: &ObjData, actual: &ObjData) -> Result<toml::Table> {
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
    if actual.components != expected.components {
        put("components", toml::Value::try_from(&actual.components)?);
    }
    let fields = changed_fields(&expected.fields, &actual.fields);
    if !fields.is_empty() {
        put("fields", toml::Value::try_from(&fields)?);
    }
    if let Some(camera) = actual.camera.filter(|c| Some(*c) != expected.camera) {
        put("camera", toml::Value::try_from(camera)?);
    }
    if let Some(volume) = actual.post_volume.as_ref().filter(|v| Some(*v) != expected.post_volume.as_ref()) {
        put("post_volume", toml::Value::try_from(volume)?);
    }
    if let Some(light) = actual.light.as_ref().filter(|l| Some(*l) != expected.light.as_ref()) {
        put("light", toml::Value::try_from(light)?);
    }
    if actual.renderer.is_some() && !same_renderer(content, &actual.renderer, &expected.renderer) {
        put("renderer", renderer_changes(content, &actual.renderer, &expected.renderer)?);
    }
    if let Some(audio) = actual.audio.as_ref().filter(|a| Some(*a) != expected.audio.as_ref()) {
        put("audio", toml::Value::Table(audio.changes_from(expected.audio.as_ref())));
    }
    if actual.audio_listener != expected.audio_listener {
        put("audio_listener", actual.audio_listener.into());
    }
    if actual.layer != expected.layer {
        put("layer", toml::Value::try_from(actual.layer.clone().unwrap_or(LayerRef::Number(0)))?);
    }
    // Each written whole, as an override merges into the prefab's and a value left out would keep the prefab's.
    if let Some(collider) = actual.collider.filter(|c| Some(*c) != expected.collider) {
        put("collider", whole(&collider, &crate::physics::Collider::default())?);
    }
    if let Some(body) = actual.rigidbody.filter(|b| Some(*b) != expected.rigidbody) {
        put("rigidbody", whole(&body, &crate::physics::Rigidbody::default())?);
    }
    if let Some(character) = actual.character.filter(|c| Some(*c) != expected.character) {
        put("character", whole(&character, &crate::physics::CharacterController::default())?);
    }
    if let Some(joint) = actual.joint.as_ref().filter(|j| Some(*j) != expected.joint.as_ref()) {
        put("joint", joint.whole()?);
    }
    if let Some(collider) = actual.collider2d.filter(|c| Some(*c) != expected.collider2d) {
        put("collider2d", whole(&collider, &Collider2D::default())?);
    }
    if let Some(body) = actual.rigidbody2d.filter(|b| Some(*b) != expected.rigidbody2d) {
        put("rigidbody2d", whole(&body, &Rigidbody2D::default())?);
    }
    if let Some(vehicle) = actual.vehicle.filter(|v| Some(*v) != expected.vehicle) {
        put("vehicle", whole(&vehicle, &Vehicle::default())?);
    }
    if let Some(wheel) = actual.wheel.filter(|w| Some(*w) != expected.wheel) {
        put("wheel", whole(&wheel, &Wheel::default())?);
    }
    if let Some(lod) = actual.lod_group.as_ref().filter(|l| Some(*l) != expected.lod_group.as_ref()) {
        put("lod_group", lod.whole()?);
    }
    Ok(changes)
}

/// What differs between a renderer and the prefab's, as an override merges
/// it: only the mesh if it changed, and only the material values that did.
fn renderer_changes(
    content: &Content,
    actual: &Option<RendererData>,
    expected: &Option<RendererData>,
) -> Result<toml::Value> {
    let actual_value = toml::Value::try_from(actual)?;
    let (Some(actual), Some(expected)) = (actual, expected) else { return Ok(actual_value) };
    let (toml::Value::Table(mut changes), toml::Value::Table(before)) =
        (actual_value, toml::Value::try_from(expected)?)
    else {
        return Ok(toml::Value::try_from(actual)?);
    };
    let parse = |text: &str| content.parse(text).ok();
    if parse(&actual.mesh) == parse(&expected.mesh) {
        changes.remove("mesh");
    }
    // LOD settings back at their defaults are written out, or the prefab's would stay.
    if actual.lod_bias != expected.lod_bias {
        changes.insert("lod_bias".into(), toml::Value::Float(f64::from(actual.lod_bias)));
    } else {
        changes.remove("lod_bias");
    }
    if actual.forced_lod() != expected.forced_lod() {
        changes.insert("force_lod".into(), i64::from(actual.force_lod.unwrap_or(-1)).into());
    } else {
        changes.remove("force_lod");
    }
    // Empty, as an override writes the engine switching them again.
    if actual.transitions() != expected.transitions() {
        changes.insert("lod_transitions".into(), toml::Value::try_from(&actual.lod_transitions)?);
    } else {
        changes.remove("lod_transitions");
    }
    if let (Some(toml::Value::Table(material)), Some(toml::Value::Table(was))) =
        (changes.get_mut("material"), before.get("material"))
    {
        material.retain(|key, value| was.get(key) != Some(value));
    }
    if changes.get("material").and_then(toml::Value::as_table).is_some_and(toml::Table::is_empty) {
        changes.remove("material");
    }
    Ok(toml::Value::Table(changes))
}

/// A copy of behaviour fields with each `{ entity = n }` written back as a
/// path in the file being saved. An Obj outside the saved set becomes an
/// empty table, which loads as `Obj.None`, like a missing reference in Unity.
fn unbind_refs(world: &World, ids: &HashMap<Entity, u32>, fields: &toml::Table) -> toml::Table {
    let mut unbound = fields.clone();
    unbound.iter_mut().for_each(|(_, v)| unbind_value(world, ids, v));
    unbound
}

fn unbind_value(world: &World, ids: &HashMap<Entity, u32>, value: &mut toml::Value) {
    match value {
        toml::Value::Table(table) => match entity_ref(table) {
            Some(entity) => {
                *table = match file_path(world, ids, entity) {
                    Some(path) => toml::Table::from_iter([("obj".to_string(), prefab::ref_value(&path))]),
                    None => {
                        tracing::warn!(
                            "a behaviour field points at an Obj that is not being saved; it is saved as none"
                        );
                        toml::Table::new()
                    }
                };
            }
            None => table.iter_mut().for_each(|(_, v)| unbind_value(world, ids, v)),
        },
        toml::Value::Array(items) => items.iter_mut().for_each(|v| unbind_value(world, ids, v)),
        _ => {}
    }
}

/// The entity in a bound reference, `{ entity = n }`.
fn entity_ref(table: &toml::Table) -> Option<Entity> {
    match (table.len(), table.get("entity")) {
        (1, Some(toml::Value::Integer(bits))) => Some(Entity::from_bits(*bits as u64)),
        _ => None,
    }
}

/// Where an entity is in the file being saved: its own id, or its path
/// inside a saved prefab instance.
fn file_path(world: &World, ids: &HashMap<Entity, u32>, entity: Entity) -> Option<ObjPath> {
    if let Some(&id) = ids.get(&entity) {
        return Some(ObjPath::new(id));
    }
    let link = world.get::<FromPrefab>(entity)?;
    ids.get(&link.instance).map(|&id| ObjPath::new(id).join(&link.path))
}

/// The field values in `actual` that `expected` does not already have. Fields
/// merge one by one, so only these need writing.
fn changed_fields(
    expected: &BTreeMap<String, toml::Table>,
    actual: &BTreeMap<String, toml::Table>,
) -> BTreeMap<String, toml::Table> {
    let mut changed = BTreeMap::new();
    for (behaviour, values) in actual {
        let before = expected.get(behaviour);
        let values: toml::Table = values
            .iter()
            .filter(|(k, v)| before.and_then(|b| b.get(*k)) != Some(*v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !values.is_empty() {
            changed.insert(behaviour.clone(), values);
        }
    }
    changed
}

/// Compares renderers by what they load, so `Models/Ship.gltf` and its GUID
/// count as the same mesh.
fn same_renderer(content: &Content, a: &Option<RendererData>, b: &Option<RendererData>) -> bool {
    let parse = |text: &str| content.parse(text).ok();
    let material = |m: &Option<MaterialRef>| match m {
        None => Some(MaterialSource::Inline(default_material())),
        Some(MaterialRef::Inline(data)) => Some(MaterialSource::Inline(data.clone())),
        Some(MaterialRef::Asset(text)) => parse(text).map(MaterialSource::Asset),
    };
    match (a, b) {
        (Some(a), Some(b)) => {
            parse(&a.mesh) == parse(&b.mesh)
                && material(&a.material) == material(&b.material)
                && (a.lod_bias, a.forced_lod(), a.transitions()) == (b.lod_bias, b.forced_lod(), b.transitions())
        }
        (None, None) => true,
        _ => false,
    }
}

/// Saved transforms are rounded and Euler angles go through a quaternion, so
/// exact comparison would see changes that are not there.
fn nearly_equal(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-3)
}

pub(crate) fn capture_obj(world: &World, content: &Content, entity: Entity, ids: &HashMap<Entity, u32>) -> ObjData {
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
    obj.locks = world.get::<crate::prefab::Locks>(entity).map(|locks| locks.0.clone()).unwrap_or_default();
    // Running behaviours and the components made from the scene, or in edit
    // mode the same as data.
    let mut specs: Vec<&BehaviourSpec> = match (world.get::<Behaviours>(entity), world.get::<EditBehaviours>(entity)) {
        (Some(behaviours), _) => behaviours.0.iter().map(|b| &b.spec).collect(),
        (None, Some(kept)) => kept.0.iter().collect(),
        (None, None) => Vec::new(),
    };
    specs.extend(world.get::<SceneComponents>(entity).into_iter().flat_map(|made| &made.0));
    let names = |component: bool| specs.iter().filter(move |s| s.component == component).map(|s| s.type_name.clone());
    obj.behaviours = names(false).collect();
    obj.components = names(true).collect();
    // Saved as loaded: values scripts change at runtime are not written back.
    obj.fields = specs
        .iter()
        .filter(|spec| !spec.fields.is_empty())
        .map(|spec| (spec.type_name.clone(), unbind_refs(world, ids, &spec.fields)))
        .collect();
    obj.camera =
        world.get::<Camera>(entity).map(|c| CameraData { fov: c.fov_y_degrees, exposure: c.exposure, near: c.near });
    obj.light = world.get::<Light>(entity).map(|light| light_data(content, light));
    obj.post_volume = world.get::<PostVolume>(entity).map(PostVolumeData::of);
    obj.renderer = world.get::<MeshRenderer>(entity).and_then(|r| {
        let mesh = content.mesh_ref(r.mesh)?.to_string();
        let material = match content.material_source(r.material) {
            Some(MaterialSource::Asset(reference)) => Some(MaterialRef::Asset(reference.to_string())),
            Some(MaterialSource::Inline(data)) if *data == default_material() => None,
            Some(MaterialSource::Inline(data)) => Some(MaterialRef::Inline(data.clone())),
            None => None,
        };
        let (lod_bias, force_lod, lod_transitions) =
            (r.lod_bias, r.forced_lod.map(i32::from), written_transitions(r.lod_transitions));
        Some(RendererData { mesh, material, lod_bias, force_lod, lod_transitions })
    });
    obj.audio = world.get::<AudioSource>(entity).map(|source| audio_source_data(content, source));
    obj.audio_listener = world.has::<AudioListener>(entity);
    obj.layer = world.get::<Layer>(entity).filter(|layer| layer.0 != 0).map(|layer| LayerRef::Number(layer.0));
    obj.collider = world.get::<Collider>(entity).copied();
    obj.rigidbody = world.get::<Rigidbody>(entity).copied();
    obj.character = world.get::<CharacterController>(entity).copied();
    obj.joint = world.get::<Joint>(entity).map(|joint| JointData::of(joint, &|e| file_path(world, ids, e)));
    obj.collider2d = world.get::<Collider2D>(entity).copied();
    obj.rigidbody2d = world.get::<Rigidbody2D>(entity).copied();
    // What drives a Vehicle and how it goes are the game's, not the scene's.
    obj.vehicle = world.get::<Vehicle>(entity).map(|vehicle| Vehicle {
        throttle: 0.0,
        steering: 0.0,
        brake: 0.0,
        handbrake: 0.0,
        speed: 0.0,
        rpm: 0.0,
        gear: 0,
        ..*vehicle
    });
    obj.wheel = world.get::<Wheel>(entity).copied();
    obj.lod_group = world.get::<LodGroup>(entity).map(|lod| LodGroupData::of(lod, &|e| file_path(world, ids, e)));
    obj
}

/// A light of `kind` as a scene makes one when it says nothing more about it.
pub fn default_light(kind: LightKind) -> Light {
    let name = match kind {
        LightKind::Directional => "directional",
        LightKind::Point => "point",
        LightKind::Spot => "spot",
        LightKind::Rectangle => "rectangle",
        LightKind::Disc => "disc",
        LightKind::Tube => "tube",
    };
    light_component(&toml::from_str(&format!("type = \"{name}\"")).expect("every light field has a default"))
}

/// A light as a scene describes it, without its cookie and profile, which
/// need their assets loaded (see [`light_with_assets`]).
pub(crate) fn light_component(light: &LightData) -> Light {
    match *light {
        LightData::Directional { color, intensity, cookie_size, softness, shadow_tint, body, .. } => {
            let shadow_tint = Vec3::from(linear(shadow_tint));
            let body = match body {
                SkyBodyData::Sun => SkyBody::Sun,
                SkyBodyData::Moon => SkyBody::Moon,
            };
            Light {
                cookie_size,
                softness,
                shadow_tint,
                body,
                ..Light::directional(Vec3::from(linear(color)), intensity)
            }
        }
        LightData::Point { color, intensity, range, .. } => Light::point(Vec3::from(linear(color)), intensity, range),
        LightData::Spot { color, intensity, range, inner_angle, outer_angle, .. } => {
            Light::spot(Vec3::from(linear(color)), intensity, range, inner_angle, outer_angle)
        }
        LightData::Rectangle { color, intensity, range, width, height } => {
            Light::rectangle(Vec3::from(linear(color)), intensity, range, width, height)
        }
        LightData::Disc { color, intensity, range, radius } => {
            Light::disc(Vec3::from(linear(color)), intensity, range, radius)
        }
        LightData::Tube { color, intensity, range, length, radius } => {
            Light::tube(Vec3::from(linear(color)), intensity, range, length, radius)
        }
    }
}

/// A light with its cookie and IES profile loaded, as a scene names them.
pub(crate) fn light_with_assets(content: &mut Content, renderer: &mut Renderer, data: &LightData) -> Result<Light> {
    let mut light = light_component(data);
    let (cookie, profile) = data.assets();
    if let Some(cookie) = cookie {
        light.cookie = Some(content.cookie(renderer, &content.parse(cookie)?)?);
    }
    if let Some(profile) = profile {
        light.profile = Some(content.light_profile(renderer, &content.parse(profile)?)?);
    }
    Ok(light)
}

fn light_data(content: &Content, light: &Light) -> LightData {
    let color = round3(srgb(light.color.to_array()));
    let intensity = light.intensity;
    let cookie = light.cookie.and_then(|texture| content.picture_guid(texture)).map(|guid| guid.to_string());
    let profile = light.profile.and_then(|texture| content.light_profile_guid(texture)).map(|guid| guid.to_string());
    match light.kind {
        LightKind::Directional => LightData::Directional {
            color,
            intensity,
            cookie,
            cookie_size: light.cookie_size,
            softness: light.softness,
            shadow_tint: round3(srgb(light.shadow_tint.to_array())),
            body: match light.body {
                SkyBody::Sun => SkyBodyData::Sun,
                SkyBody::Moon => SkyBodyData::Moon,
            },
        },
        LightKind::Point => LightData::Point { color, intensity, range: light.range, profile },
        LightKind::Spot => LightData::Spot {
            color,
            intensity,
            range: light.range,
            inner_angle: light.spot_inner_degrees,
            outer_angle: light.spot_outer_degrees,
            cookie,
            profile,
        },
        LightKind::Rectangle => {
            LightData::Rectangle { color, intensity, range: light.range, width: light.width, height: light.height }
        }
        LightKind::Disc => LightData::Disc { color, intensity, range: light.range, radius: light.radius },
        LightKind::Tube => {
            LightData::Tube { color, intensity, range: light.range, length: light.length, radius: light.radius }
        }
    }
}

/// An Obj's layer as a file has it: by number, as a save writes it, so
/// that renaming a layer keeps its Objs on it, or by the name the project
/// gives it, as one may write by hand: `layer = 7` or `layer = "Player"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LayerRef {
    Number(u8),
    Name(String),
}

impl LayerRef {
    /// The layer it names in the world's project; one it does not have is
    /// Default, with a warning.
    pub fn layer(&self, world: &World) -> Layer {
        let found = self.find(world);
        if let (None, Self::Name(name)) = (found, self) {
            tracing::warn!("there is no layer {name}; the Obj is on Default");
        }
        found.unwrap_or_default()
    }

    fn find(&self, world: &World) -> Option<Layer> {
        match self {
            Self::Number(number) => Some(Layer(*number % voxear_scene::LAYER_COUNT as u8)),
            Self::Name(name) => world.resource::<Layers>().and_then(|layers| layers.find(name)).map(Layer),
        }
    }

    /// An Obj's layer as a save writes it, by number and left out for
    /// Default, so that a prefab's file naming its layer and an instance
    /// on the same one compare alike.
    pub fn as_saved(layer: &Option<Self>, world: &World) -> Option<Self> {
        let layer = layer.as_ref().and_then(|layer| layer.find(world))?;
        (layer.0 != 0).then_some(Self::Number(layer.0))
    }
}

/// The material an Obj gets when its scene names none.
pub fn default_material() -> MaterialData {
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

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
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

fn default_near() -> f32 {
    Camera::default().near
}

fn is_default_near(near: &f32) -> bool {
    *near == default_near()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sky_is_a_gradient_unless_it_says_atmosphere() {
        let colours = "zenith = [0.5, 0.7, 0.9]\nhorizon = [0.8, 0.8, 0.9]\nground = [0.5, 0.5, 0.4]";
        let gradient: SkyData = toml::from_str(colours).unwrap();
        assert_eq!(gradient.mode, SkyModeData::Gradient);
        assert!(!toml::to_string(&gradient).unwrap().contains("mode"), "a gradient is not written out");
        let air: SkyData = toml::from_str(&format!("mode = \"atmosphere\"\n{colours}")).unwrap();
        assert_eq!(air.mode, SkyModeData::Atmosphere);
        assert_eq!(toml::from_str::<SkyData>(&toml::to_string(&air).unwrap()).unwrap(), air);
    }

    #[test]
    fn a_look_brings_its_sun_and_its_sky_where_the_scene_has_none() {
        let post = |text: &str| toml::from_str::<toml::Table>(text).unwrap();
        let mut engine = Engine::new();
        apply_environment(&mut engine, None, Some(&post("look = \"storybook\""))).unwrap();
        let storybook = Look::of(LookName::Storybook);
        assert_eq!(engine.world.resource::<SunLook>().copied(), Some(storybook.sun));
        assert_eq!(engine.world.resource::<Sky>().map(|sky| sky.zenith), Some(storybook.sky.zenith));
        assert!(engine.world.resource::<SkyFromLook>().is_some(), "so a save leaves the sky out");

        apply_environment(&mut engine, None, None).unwrap();
        assert!(engine.world.resource::<SunLook>().is_some(), "a scene without [post] keeps the look");

        let own: SkyData = toml::from_str("zenith = [0.1, 0.2, 0.3]\nhorizon = [0.5, 0.5, 0.5]\nground = [0.3, 0.3, 0.3]").unwrap();
        apply_environment(&mut engine, Some(own), Some(&post("look = \"soft_overcast\""))).unwrap();
        assert!(engine.world.resource::<SkyFromLook>().is_none(), "the scene's own sky wins and is saved");
        assert_eq!(engine.world.resource::<SunLook>().copied(), Some(Look::of(LookName::SoftOvercast).sun));

        apply_environment(&mut engine, None, Some(&post("look = \"storybook\""))).unwrap();
        apply_environment(&mut engine, None, Some(&post("[bloom]\nintensity = 0.1"))).unwrap();
        assert!(engine.world.resource::<SunLook>().is_none(), "[post] without a look drops it");
        assert_eq!(engine.world.resource::<Sky>().map(|sky| sky.zenith), Some(Sky::default().zenith));
        assert!(apply_environment(&mut engine, None, Some(&post("look = \"gloomy\""))).is_err());
    }

    #[test]
    fn a_moon_and_the_skys_stars_read_and_write_back() {
        let moon: LightData = toml::from_str("type = \"directional\"\nintensity = 0.15\nbody = \"moon\"").unwrap();
        assert_eq!(light_component(&moon).body, SkyBody::Moon);
        assert!(toml::to_string(&moon).unwrap().contains("body = \"moon\""));
        let sun: LightData = toml::from_str("type = \"directional\"").unwrap();
        assert_eq!(light_component(&sun).body, SkyBody::Sun);
        assert!(!toml::to_string(&sun).unwrap().contains("body"), "the sun, as most are, is not written out");

        let colours = "zenith = [0.0, 0.0, 0.1]\nhorizon = [0.0, 0.0, 0.1]\nground = [0.0, 0.0, 0.0]";
        let plain: SkyData = toml::from_str(colours).unwrap();
        assert_eq!(plain.stars, 1.0);
        assert!(!toml::to_string(&plain).unwrap().contains("stars"));
        let dim: SkyData = toml::from_str(&format!("{colours}\nstars = 0.5")).unwrap();
        assert_eq!(toml::from_str::<SkyData>(&toml::to_string(&dim).unwrap()).unwrap().stars, 0.5);
    }

    #[test]
    fn area_and_tube_lights_read_and_write_back() {
        for text in [
            "type = \"rectangle\"\nwidth = 2.0\nheight = 0.5",
            "type = \"disc\"\nradius = 0.3",
            "type = \"tube\"\nlength = 1.2\nradius = 0.02\nrange = 4.0",
        ] {
            let read: LightData = toml::from_str(text).unwrap();
            let light = light_component(&read);
            assert!(light.kind.has_shape(), "{text}");
            assert_eq!(toml::from_str::<LightData>(&toml::to_string(&read).unwrap()).unwrap(), read, "{text}");
        }
        let panel = default_light(LightKind::Rectangle);
        assert_eq!((panel.width, panel.height), (1.0, 1.0));
        assert_eq!(default_light(LightKind::Tube).radius, 0.05);
    }

    #[test]
    fn a_renderers_lod_settings_are_written_only_when_set() {
        let plain: RendererData = toml::from_str("mesh = \"builtin:sphere\"").unwrap();
        assert_eq!((plain.lod_bias, plain.forced_lod()), (0.0, None));
        assert_eq!(toml::to_string(&plain).unwrap().trim(), "mesh = \"builtin:sphere\"");

        let tuned: RendererData = toml::from_str("mesh = \"builtin:sphere\"\nlod_bias = 1.5\nforce_lod = 2").unwrap();
        assert_eq!((tuned.lod_bias, tuned.forced_lod()), (1.5, Some(2)));
        let back: RendererData = toml::from_str(&toml::to_string(&tuned).unwrap()).unwrap();
        assert_eq!(back, tuned);

        let unforced: RendererData = toml::from_str("mesh = \"builtin:sphere\"\nforce_lod = -1").unwrap();
        assert_eq!(unforced.forced_lod(), None, "-1, as an override writes it, picks by distance again");

        let ends: RendererData = toml::from_str("mesh = \"builtin:sphere\"\nlod_transitions = [0.5, 0.25]").unwrap();
        assert_eq!(ends.transitions(), Some([0.5, 0.25, 0.0, 0.0, 0.0]), "LODs not written end at nothing");
        assert_eq!(written_transitions(ends.transitions()), [0.5, 0.25], "and are left out again");
        assert_eq!(written_transitions(None), Vec::<f32>::new());
    }
}
