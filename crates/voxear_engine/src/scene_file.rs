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
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    Camera, Foliage, FoliageInteraction, Light, LightKind, LodGroup, Look, LookName, MAX_TRANSITIONS, MeshRenderer, PostProcess, PostVolume, Renderer,
    DryArea, Ocean, ParticleSystem, Plant, Pool, Seasons, Sky, SkyBody, SkyMode, Stream, SunLook, VolumeShape, WindZone, scatter,
};
use voxear_scene::{
    BehaviourSpec, Behaviours, Layer, Layers, Name, Parent, SceneComponents, ScenePersistence, Transform,
};

use crate::{
    lighting::Lighting,
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
    /// The lighting settings it is lit by: a `.lighting` asset, by GUID or
    /// project path, or built-in lighting such as `builtin:golden_hour`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<String>,
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
        Self { format: SCENE_FORMAT, lighting: None, sky: None, post: None, objs: Vec::new() }
    }
}

/// The `[post]` table the current post-processing came from, so saving a
/// scene writes back its preset and overrides rather than every setting.
pub struct PostTable(pub toml::Table);

/// The sky came from the scene's look or lighting rather than its own
/// `[sky]`, so a save leaves it out, and changing the look changes the sky.
pub struct SkyFromLook;

/// The lighting settings the scene names, as it names them, so a save
/// writes them back.
pub struct SceneLighting(pub String);

/// The post-processing of the scene's lighting settings, beneath the
/// scene's own `[post]`, for what reads assets from it (its colour
/// lookup table).
pub struct LightingPost(pub toml::Table);

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
    /// The atmosphere's haze, 1 as on a clear day.
    #[serde(default = "one_f32", skip_serializing_if = "is_one_f32")]
    pub haze: f32,
    /// The colour the haze scatters, white for grey moisture, warmer for dust.
    #[serde(default = "white", skip_serializing_if = "is_white")]
    pub haze_tint: [f32; 3],
    /// How much the haze glows round the sun, 0 to 0.95.
    #[serde(default = "default_haze_glow", skip_serializing_if = "is_default_haze_glow")]
    pub haze_glow: f32,
    /// How quickly haze builds up with distance.
    #[serde(default = "default_distance_haze", skip_serializing_if = "is_default_distance_haze")]
    pub distance_haze: f32,
    /// The sun's disc across the sky, in degrees.
    #[serde(default = "default_sun_size", skip_serializing_if = "is_default_sun_size")]
    pub sun_size: f32,
}

fn default_haze_glow() -> f32 {
    Sky::default().haze_glow
}

fn is_default_haze_glow(glow: &f32) -> bool {
    *glow == default_haze_glow()
}

fn default_distance_haze() -> f32 {
    Sky::default().distance_haze
}

fn is_default_distance_haze(distance: &f32) -> bool {
    *distance == default_distance_haze()
}

fn default_sun_size() -> f32 {
    Sky::default().sun_size
}

fn is_default_sun_size(size: &f32) -> bool {
    *size == default_sun_size()
}

fn is_white(color: &[f32; 3]) -> bool {
    *color == [1.0; 3]
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
    /// A whole model drawn by the Obj, each part where the model puts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelData>,
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
    /// A Wind Zone, blowing along the Obj's +Z over the whole level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_zone: Option<WindZone>,
    /// A Particle System, sending out particles from the Obj.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particles: Option<ParticleSystem>,
    /// Smoke filling the space round the Obj.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoke: Option<voxear_render::Smoke>,
    /// A trail of mist left behind the Obj as it moves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mist: Option<voxear_render::Mist>,
    /// Shared with a networked game's players.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<crate::net::NetObject>,
    /// Plays one of the Obj's model's clips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animator: Option<crate::animation::Animator>,
    /// Animates the Obj's model by motion matching over its clips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_matcher: Option<crate::animation::MotionMatcher>,
    /// Keeps the Obj on a bone of its parent's model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bone_attachment: Option<crate::animation::BoneAttachment>,
    /// An Ocean, the sea filling the level at the Obj's height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocean: Option<Ocean>,
    /// A Pool, water (or lava) over a box of ground at the Obj's height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<Pool>,
    /// A Dry Area, keeping water out of its shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_area: Option<DryAreaData>,
    /// A Path, points joined by a curve through them, for a Stream to run along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<voxear_scene::Path>,
    /// A Stream, water (or lava) running along the Obj's Path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<Stream>,
    /// A Buoyancy, floating the Obj's Rigidbody on water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buoyancy: Option<crate::water::Buoyancy>,
    /// Seasons, turning the level's plants through the year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasons: Option<Seasons>,
    /// Bends foliage out of the Obj's way as it passes through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foliage_interaction: Option<FoliageInteraction>,
    /// One plant, grown in many places without an Obj each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foliage: Option<FoliageData>,
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
            model: None,
            audio: None,
            audio_listener: false,
            layer: None,
            collider: None,
            wind_zone: None,
            particles: None,
            smoke: None,
            mist: None,
            net: None,
            animator: None,
            motion_matcher: None,
            bone_attachment: None,
            ocean: None,
            pool: None,
            dry_area: None,
            path: None,
            stream: None,
            buoyancy: None,
            seasons: None,
            foliage_interaction: None,
            foliage: None,
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
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
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
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default = "default_range")]
        range: f32,
        /// Whether it casts shadows (see `Light::shadows`).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        shadows: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
    },
    Spot {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default = "default_range")]
        range: f32,
        /// Whether it casts shadows (see `Light::shadows`).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        shadows: bool,
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
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default = "default_range")]
        range: f32,
        /// Whether it casts shadows (see `Light::shadows`).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        shadows: bool,
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
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default = "default_range")]
        range: f32,
        /// Whether it casts shadows (see `Light::shadows`).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        shadows: bool,
        #[serde(default = "half")]
        radius: f32,
    },
    /// A glowing capsule along the Obj's +X.
    Tube {
        #[serde(default = "white")]
        color: [f32; 3],
        #[serde(default = "default_lamp")]
        intensity: f32,
        /// The colour of a light this hot, in kelvin, which `color` then filters.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default = "default_range")]
        range: f32,
        /// Whether it casts shadows (see `Light::shadows`).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        shadows: bool,
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
    /// The light's colour temperature, in kelvin, whichever kind it is.
    /// Whether a local light casts shadows; the sun always does.
    pub fn shadows(&self) -> bool {
        match *self {
            Self::Directional { .. } => false,
            Self::Point { shadows, .. }
            | Self::Spot { shadows, .. }
            | Self::Rectangle { shadows, .. }
            | Self::Disc { shadows, .. }
            | Self::Tube { shadows, .. } => shadows,
        }
    }

    pub fn temperature(&self) -> Option<f32> {
        match *self {
            Self::Directional { temperature, .. }
            | Self::Point { temperature, .. }
            | Self::Spot { temperature, .. }
            | Self::Rectangle { temperature, .. }
            | Self::Disc { temperature, .. }
            | Self::Tube { temperature, .. } => temperature,
        }
    }

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

/// A Foliage as a scene writes it: one plant, its mesh and material as a
/// Mesh Renderer names them, grown in many places in the Obj's space.
///
/// ```toml
/// [obj.foliage]
/// mesh = "Models/Grass.glb#mesh0.0"
/// material = "Materials/Grass.material"
/// draw_distance = 60.0
/// plants = [[1.0, 0.0, 2.0, 45.0, 1.1]]   # x, y, z, turn in degrees, size
/// [obj.foliage.scatter]
/// size = [40.0, 40.0]   # metres across, centred on the Obj
/// density = 2.0         # plants to a square metre
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoliageData {
    pub mesh: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<MaterialRef>,
    /// How far from the camera plants are drawn, in metres.
    #[serde(default = "default_draw_distance")]
    pub draw_distance: f32,
    /// Over how many metres short of the draw distance they fade out.
    #[serde(default = "default_fade_distance")]
    pub fade_distance: f32,
    /// How far from the camera they cast shadows, in metres; 0 for none.
    #[serde(default = "default_shadow_distance")]
    pub shadow_distance: f32,
    /// The colours plants are tinted between, in sRGB, in patches
    /// `color_spread` metres across. White leaves them as made.
    #[serde(default = "white", skip_serializing_if = "is_white")]
    pub healthy_color: [f32; 3],
    #[serde(default = "white", skip_serializing_if = "is_white")]
    pub dry_color: [f32; 3],
    #[serde(default = "default_color_spread", skip_serializing_if = "is_default_color_spread")]
    pub color_spread: f32,
    /// Whether far plants are drawn as impostors: `auto`, `off`, or `from`
    /// `impostor_distance` metres.
    #[serde(default, skip_serializing_if = "is_auto_impostors")]
    pub impostors: voxear_render::Impostors,
    #[serde(default = "default_impostor_distance", skip_serializing_if = "is_default_impostor_distance")]
    pub impostor_distance: f32,
    /// Plants placed one by one: x, y and z in the Obj's space, a turn in
    /// degrees, and a size.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plants: Vec<[f32; 5]>,
    /// Plants scattered over an area, besides those placed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scatter: Option<ScatterData>,
}

fn default_color_spread() -> f32 {
    8.0
}

fn is_default_color_spread(spread: &f32) -> bool {
    *spread == default_color_spread()
}

fn is_auto_impostors(impostors: &voxear_render::Impostors) -> bool {
    *impostors == voxear_render::Impostors::Auto
}

fn default_impostor_distance() -> f32 {
    60.0
}

fn is_default_impostor_distance(distance: &f32) -> bool {
    *distance == default_impostor_distance()
}

/// Plants scattered evenly over an area round the Obj (see
/// `voxear_render::scatter`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScatterData {
    /// Metres across, along X and Z, centred on the Obj.
    pub size: [f32; 2],
    /// Plants to a square metre.
    pub density: f32,
    /// Another seed scatters them another way.
    #[serde(default)]
    pub seed: u32,
    /// The smallest and largest size.
    #[serde(default = "plain_scale")]
    pub scale: [f32; 2],
}

fn default_draw_distance() -> f32 {
    60.0
}

fn default_fade_distance() -> f32 {
    10.0
}

fn default_shadow_distance() -> f32 {
    25.0
}

fn plain_scale() -> [f32; 2] {
    [1.0, 1.0]
}

impl FoliageData {
    /// Every plant: those placed, then those scattered.
    pub fn plants(&self) -> Vec<Plant> {
        let placed = self.plants.iter().map(|&[x, y, z, turn, scale]| Plant {
            position: [x, y, z],
            yaw: turn.to_radians(),
            scale,
        });
        let scattered = self.scatter.map(|s| scatter(s.size, s.density, s.seed, s.scale)).unwrap_or_default();
        placed.chain(scattered).collect()
    }
}

/// A whole model drawn by one Obj, as a scene writes it, with the parts
/// this Obj changes:
///
/// ```toml
/// [obj.model]
/// model = "fdfa941753cd46a487317019a970cbc2"  # Models/Sponza/Sponza.gltf
///
/// [[obj.model.part]]
/// node = 12
/// material = "Materials/Brass.material"
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelData {
    /// The model, by GUID or project path.
    pub model: String,
    /// As a Mesh Renderer's, shifting which LOD each part draws.
    #[serde(default, skip_serializing_if = "is_no_bias")]
    pub lod_bias: f32,
    /// The parts this Obj changes from the model, each by its node.
    #[serde(default, rename = "part", skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartData>,
}

/// What an Obj changes of one part of the model it draws: where the part
/// sits in the model, as the node's own position, rotation in degrees and
/// scale, and the material it draws with, each only when changed; or that
/// an Obj of its own draws it now, so the model leaves it out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartData {
    /// Which of the model's nodes, in its order.
    pub node: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
    /// A material asset, by GUID or project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub apart: bool,
}

impl ModelData {
    /// What this Obj changes of the model's node `node`, if anything.
    pub fn part(&self, node: u32) -> Option<&PartData> {
        self.parts.iter().find(|part| part.node == node)
    }

    /// What this Obj changes of node `node`, made empty if it changes
    /// nothing of it yet. Empty changes are left out when written.
    pub fn part_mut(&mut self, node: u32) -> &mut PartData {
        let at = match self.parts.iter().position(|part| part.node == node) {
            Some(at) => at,
            None => {
                self.parts.push(PartData { node, ..PartData::default() });
                self.parts.sort_by_key(|part| part.node);
                self.parts.iter().position(|part| part.node == node).expect("just put in")
            }
        };
        &mut self.parts[at]
    }

    /// Leaves out the parts this Obj no longer changes.
    pub fn tidy_parts(&mut self) {
        self.parts.retain(|part| part.changes());
    }
}

impl PartData {
    pub fn changes(&self) -> bool {
        let PartData { node: _, position, rotation, scale, material, apart } = self;
        position.is_some() || rotation.is_some() || scale.is_some() || material.is_some() || *apart
    }

    /// Where the part sits in the model, as changed, over where the model
    /// puts it.
    pub fn transform(&self, (position, rotation, scale): (Vec3, Quat, Vec3)) -> (Vec3, Quat, Vec3) {
        let euler =
            |[x, y, z]: [f32; 3]| Quat::from_euler(EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians());
        (
            self.position.map_or(position, Vec3::from),
            self.rotation.map_or(rotation, euler),
            self.scale.map_or(scale, Vec3::from),
        )
    }
}

fn is_no_bias(bias: &f32) -> bool {
    *bias == 0.0
}

/// How the Obj's model was written, kept beside its Model Renderer so a save
/// writes it back, and kept alone where nothing draws, as headless.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelSource(pub ModelData);
impl voxear_ecs::Component for ModelSource {}

/// How the Obj's Foliage was written, so a save writes it back as it was:
/// its scatter rather than every plant it made.
#[derive(Clone, Debug, PartialEq)]
pub struct FoliageSource(pub FoliageData);
impl voxear_ecs::Component for FoliageSource {}

/// A Dry Area as a scene writes it: its shape and size, and for the Mesh
/// shape the mesh, written as a Mesh Renderer's is, or none for the Obj's own
/// Mesh Renderer's.
/// ```toml
/// [obj.dry_area]
/// shape = "mesh"
/// mesh = "Models/Boat.glb#mesh1.0"
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DryAreaData {
    pub shape: voxear_render::DryShape,
    pub size: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
    /// How deep the water in it stands, in metres; none when left out.
    #[serde(skip_serializing_if = "no_bilge")]
    pub bilge: f32,
}

fn no_bilge(depth: &f32) -> bool {
    *depth == 0.0
}

impl Default for DryAreaData {
    fn default() -> Self {
        let area = DryArea::default();
        Self { shape: area.shape, size: area.size, mesh: None, bilge: area.bilge }
    }
}

impl DryAreaData {
    /// `area` as a scene writes it, its mesh named as `content` knows it.
    pub fn of(content: &Content, area: &DryArea) -> Self {
        let mesh = area.mesh.and_then(|mesh| content.mesh_ref(mesh)).map(|reference| reference.to_string());
        Self { shape: area.shape, size: area.size, mesh, bilge: area.bilge }
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
    let lighting = match &scene.lighting {
        Some(reference) => Some((reference.as_str(), content.lighting(&content.parse(reference)?)?)),
        None => None,
    };
    let lighting = lighting.as_ref().map(|(reference, lit)| (*reference, lit));
    apply_environment(engine, Environment { lighting, sky: scene.sky, post: scene.post.as_ref() })?;
    let objs = without(prefab::expand(content, &scene.objs)?, skip);
    let template = Template::resolve(content, renderer, &objs)?;
    let entities = engine.spawn_template(&template, None, None)?;
    Ok(template.objs.iter().map(|o| o.path.clone()).zip(entities).collect())
}

/// Sets the level's sky and post-processing from a scene that has them:
/// the lighting settings it names, which are its lighting whole, or
/// failing those, its own `[sky]` and `[post]`. What a scene leaves out
/// stays as it was.
pub fn apply_environment(engine: &mut Engine, environment: Environment<'_>) -> Result<()> {
    let Environment { lighting, sky, post } = environment;
    let (sky, post) = if lighting.is_some() { (None, None) } else { (sky, post) };
    // A look, picked in [post], brings its sun, and its sky where the scene has none of its own.
    let look = match post.and_then(|table| table.get("look")).and_then(|value| value.as_str()) {
        Some(name) => Some(Look::of(LookName::parse(name)?)),
        None => None,
    };
    let base_sky = look.map(|look| look.sky).or_else(|| lighting.map(|(_, lit)| lit.sky));
    let sun = look.map(|look| look.sun).or_else(|| lighting.map(|(_, lit)| lit.sun));
    // A scene without lighting or [post] leaves the look as it was; one with either and no look drops it, sky and all.
    match (sun, lighting.is_some() || post.is_some()) {
        (Some(sun), _) => drop(engine.world.insert_resource(sun)),
        (None, true) => {
            engine.world.remove_resource::<SunLook>();
            if engine.world.remove_resource::<SkyFromLook>().is_some() && sky.is_none() {
                engine.world.insert_resource(Sky::default());
            }
        }
        (None, false) => {}
    }
    if let (Some(base), None) = (base_sky, sky) {
        engine.world.insert_resource(base);
        engine.world.insert_resource(SkyFromLook);
    }
    if let Some(sky) = sky {
        engine.world.remove_resource::<SkyFromLook>();
        engine.world.insert_resource(sky_from_data(&sky));
    }
    match lighting {
        Some((reference, lit)) => {
            engine.world.insert_resource(SceneLighting(reference.to_string()));
            engine.world.insert_resource(LightingPost(lit.post.clone()));
        }
        None if post.is_some() => {
            engine.world.remove_resource::<SceneLighting>();
            engine.world.remove_resource::<LightingPost>();
        }
        None => {}
    }
    let mut effective = lighting.map(|(_, lit)| lit.post.clone()).unwrap_or_default();
    if let Some(table) = post {
        crate::lighting::merge(&mut effective, table);
        engine.world.insert_resource(PostTable(table.clone()));
    } else if lighting.is_some() {
        engine.world.remove_resource::<PostTable>();
    }
    if lighting.is_some() || post.is_some() {
        engine.world.insert_resource(PostProcess::from_toml(&effective)?);
    }
    Ok(())
}

/// What a scene sets its environment from: its lighting settings, as the
/// scene names them and worked out, and its own `[sky]` and `[post]`.
#[derive(Clone, Copy, Default)]
pub struct Environment<'a> {
    pub lighting: Option<(&'a str, &'a Lighting)>,
    pub sky: Option<SkyData>,
    pub post: Option<&'a toml::Table>,
}

/// The sky as a scene file writes it, in sRGB.
pub fn sky_data(sky: &Sky) -> SkyData {
    SkyData {
        mode: match sky.mode {
            SkyMode::Gradient => SkyModeData::Gradient,
            SkyMode::Atmosphere => SkyModeData::Atmosphere,
        },
        zenith: round3(srgb(sky.zenith.to_array())),
        horizon: round3(srgb(sky.horizon.to_array())),
        ground: round3(srgb(sky.ground.to_array())),
        ambient: sky.ambient,
        stars: sky.stars,
        haze: sky.haze,
        haze_tint: round3(srgb(sky.haze_tint.to_array())),
        haze_glow: sky.haze_glow,
        distance_haze: sky.distance_haze,
        sun_size: sky.sun_size,
    }
}

/// The sky a scene file describes, in linear colour.
pub fn sky_from_data(sky: &SkyData) -> Sky {
    Sky {
        mode: match sky.mode {
            SkyModeData::Gradient => SkyMode::Gradient,
            SkyModeData::Atmosphere => SkyMode::Atmosphere,
        },
        zenith: Vec3::from(linear(sky.zenith)),
        horizon: Vec3::from(linear(sky.horizon)),
        ground: Vec3::from(linear(sky.ground)),
        ambient: sky.ambient,
        stars: sky.stars,
        haze: sky.haze,
        haze_tint: Vec3::from(linear(sky.haze_tint)),
        haze_glow: sky.haze_glow,
        distance_haze: sky.distance_haze,
        sun_size: sky.sun_size,
    }
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

    let sky = world.resource::<Sky>().filter(|_| world.resource::<SkyFromLook>().is_none()).map(sky_data);
    let post = world.resource::<PostTable>().map(|table| table.0.clone());
    let lighting = world.resource::<SceneLighting>().map(|lighting| lighting.0.clone());
    (SceneFile { format: SCENE_FORMAT, lighting, sky, post, objs }, ids)
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
    if root.wind_zone.is_some() && root.wind_zone != prefab_root.wind_zone {
        instance.wind_zone = root.wind_zone;
    }
    if root.particles.is_some() && root.particles != prefab_root.particles {
        instance.particles = root.particles;
    }
    if root.smoke.is_some() && root.smoke != prefab_root.smoke {
        instance.smoke = root.smoke;
    }
    if root.mist.is_some() && root.mist != prefab_root.mist {
        instance.mist = root.mist;
    }
    if root.net.is_some() && root.net != prefab_root.net {
        instance.net = root.net;
    }
    if root.animator.is_some() && root.animator != prefab_root.animator {
        instance.animator = root.animator.clone();
    }
    if root.motion_matcher.is_some() && root.motion_matcher != prefab_root.motion_matcher {
        instance.motion_matcher = root.motion_matcher.clone();
    }
    if root.bone_attachment.is_some() && root.bone_attachment != prefab_root.bone_attachment {
        instance.bone_attachment = root.bone_attachment.clone();
    }
    if root.ocean.is_some() && root.ocean != prefab_root.ocean {
        instance.ocean = root.ocean;
    }
    if root.pool.is_some() && root.pool != prefab_root.pool {
        instance.pool = root.pool;
    }
    if root.dry_area.is_some() && root.dry_area != prefab_root.dry_area {
        instance.dry_area.clone_from(&root.dry_area);
    }
    if root.path.is_some() && root.path != prefab_root.path {
        instance.path.clone_from(&root.path);
    }
    if root.stream.is_some() && root.stream != prefab_root.stream {
        instance.stream = root.stream;
    }
    if root.buoyancy.is_some() && root.buoyancy != prefab_root.buoyancy {
        instance.buoyancy = root.buoyancy;
    }
    if root.model.is_some() && root.model != prefab_root.model {
        instance.model.clone_from(&root.model);
    }
    if root.seasons.is_some() && root.seasons != prefab_root.seasons {
        instance.seasons = root.seasons;
    }
    if root.foliage_interaction.is_some() && root.foliage_interaction != prefab_root.foliage_interaction {
        instance.foliage_interaction = root.foliage_interaction;
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
    if let Some(model) = actual.model.as_ref().filter(|m| Some(*m) != expected.model.as_ref()) {
        put("model", toml::Value::try_from(model)?);
    }
    if let Some(wind) = actual.wind_zone.filter(|w| Some(*w) != expected.wind_zone) {
        put("wind_zone", whole(&wind, &WindZone::default())?);
    }
    if let Some(particles) = actual.particles.filter(|p| Some(*p) != expected.particles) {
        put("particles", whole(&particles, &ParticleSystem::default())?);
    }
    if let Some(smoke) = actual.smoke.filter(|s| Some(*s) != expected.smoke) {
        put("smoke", whole(&smoke, &voxear_render::Smoke::default())?);
    }
    if let Some(mist) = actual.mist.filter(|m| Some(*m) != expected.mist) {
        put("mist", whole(&mist, &voxear_render::Mist::default())?);
    }
    if let Some(net) = actual.net.filter(|n| Some(*n) != expected.net) {
        put("net", whole(&net, &crate::net::NetObject::default())?);
    }
    if let Some(animator) = actual.animator.as_ref().filter(|a| Some(*a) != expected.animator.as_ref()) {
        put("animator", whole(animator, &crate::animation::Animator::default())?);
    }
    let matcher = actual.motion_matcher.as_ref().filter(|m| Some(*m) != expected.motion_matcher.as_ref());
    if let Some(matcher) = matcher {
        put("motion_matcher", whole(matcher, &crate::animation::MotionMatcher::default())?);
    }
    let attachment = actual.bone_attachment.as_ref().filter(|a| Some(*a) != expected.bone_attachment.as_ref());
    if let Some(attachment) = attachment {
        put("bone_attachment", whole(attachment, &crate::animation::BoneAttachment::default())?);
    }
    if let Some(ocean) = actual.ocean.filter(|o| Some(*o) != expected.ocean) {
        put("ocean", whole(&ocean, &Ocean::default())?);
    }
    if let Some(pool) = actual.pool.filter(|p| Some(*p) != expected.pool) {
        put("pool", whole(&pool, &Pool::default())?);
    }
    if let Some(dry) = actual.dry_area.as_ref().filter(|d| Some(*d) != expected.dry_area.as_ref()) {
        put("dry_area", whole(dry, &DryAreaData::default())?);
    }
    if let Some(path) = actual.path.as_ref().filter(|p| Some(*p) != expected.path.as_ref()) {
        put("path", whole(path, &voxear_scene::Path::default())?);
    }
    if let Some(stream) = actual.stream.filter(|s| Some(*s) != expected.stream) {
        put("stream", whole(&stream, &Stream::default())?);
    }
    if let Some(buoyancy) = actual.buoyancy.filter(|b| Some(*b) != expected.buoyancy) {
        put("buoyancy", whole(&buoyancy, &crate::water::Buoyancy::default())?);
    }
    if let Some(seasons) = actual.seasons.filter(|s| Some(*s) != expected.seasons) {
        put("seasons", whole(&seasons, &Seasons::default())?);
    }
    if let Some(push) = actual.foliage_interaction.filter(|p| Some(*p) != expected.foliage_interaction) {
        put("foliage_interaction", whole(&push, &FoliageInteraction::default())?);
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
    obj.wind_zone = world.get::<WindZone>(entity).copied();
    obj.particles = world.get::<ParticleSystem>(entity).copied();
    obj.smoke = world.get::<voxear_render::Smoke>(entity).copied();
    obj.mist = world.get::<voxear_render::Mist>(entity).copied();
    obj.net = world.get::<crate::net::NetObject>(entity).copied();
    obj.animator = world.get::<crate::animation::Animator>(entity).cloned();
    obj.motion_matcher = world.get::<crate::animation::MotionMatcher>(entity).cloned();
    obj.bone_attachment = world.get::<crate::animation::BoneAttachment>(entity).cloned();
    obj.ocean = world.get::<Ocean>(entity).copied();
    obj.pool = world.get::<Pool>(entity).copied();
    obj.dry_area = world.get::<DryArea>(entity).map(|area| DryAreaData::of(content, area));
    obj.path = world.get::<voxear_scene::Path>(entity).cloned();
    obj.stream = world.get::<Stream>(entity).copied();
    obj.buoyancy = world.get::<crate::water::Buoyancy>(entity).copied();
    obj.seasons = world.get::<Seasons>(entity).copied();
    obj.foliage_interaction = world.get::<FoliageInteraction>(entity).copied();
    obj.model = world.get::<ModelSource>(entity).map(|source| {
        let lod_bias = world.get::<voxear_render::ModelRenderer>(entity).map_or(source.0.lod_bias, |drawn| drawn.lod_bias);
        ModelData { lod_bias, ..source.0.clone() }
    });
    obj.foliage = world.get::<FoliageSource>(entity).map(|source| {
        let mut data = source.0.clone();
        if let Some(foliage) = world.get::<Foliage>(entity) {
            (data.draw_distance, data.fade_distance, data.shadow_distance) =
                (foliage.draw_distance, foliage.fade_distance, foliage.shadow_distance);
            (data.healthy_color, data.dry_color, data.color_spread) =
                (foliage.healthy_color, foliage.dry_color, foliage.color_spread);
            (data.impostors, data.impostor_distance) = (foliage.impostors, foliage.impostor_distance);
        }
        data
    });
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
pub(crate) fn light_component(data: &LightData) -> Light {
    let light = match *data {
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
        LightData::Rectangle { color, intensity, range, width, height, .. } => {
            Light::rectangle(Vec3::from(linear(color)), intensity, range, width, height)
        }
        LightData::Disc { color, intensity, range, radius, .. } => {
            Light::disc(Vec3::from(linear(color)), intensity, range, radius)
        }
        LightData::Tube { color, intensity, range, length, radius, .. } => {
            Light::tube(Vec3::from(linear(color)), intensity, range, length, radius)
        }
    };
    Light { temperature: data.temperature(), shadows: data.shadows(), ..light }
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
    let temperature = light.temperature;
    let cookie = light.cookie.and_then(|texture| content.picture_guid(texture)).map(|guid| guid.to_string());
    let profile = light.profile.and_then(|texture| content.light_profile_guid(texture)).map(|guid| guid.to_string());
    let shadows = light.shadows;
    match light.kind {
        LightKind::Directional => LightData::Directional {
            color,
            intensity,
            temperature,
            cookie,
            cookie_size: light.cookie_size,
            softness: light.softness,
            shadow_tint: round3(srgb(light.shadow_tint.to_array())),
            body: match light.body {
                SkyBody::Sun => SkyBodyData::Sun,
                SkyBody::Moon => SkyBodyData::Moon,
            },
        },
        LightKind::Point => LightData::Point { color, intensity, temperature, range: light.range, shadows, profile },
        LightKind::Spot => LightData::Spot {
            color,
            intensity,
            temperature,
            range: light.range,
            shadows,
            inner_angle: light.spot_inner_degrees,
            outer_angle: light.spot_outer_degrees,
            cookie,
            profile,
        },
        LightKind::Rectangle => {
            let (width, height) = (light.width, light.height);
            LightData::Rectangle { color, intensity, temperature, range: light.range, shadows, width, height }
        }
        LightKind::Disc => {
            LightData::Disc { color, intensity, temperature, range: light.range, shadows, radius: light.radius }
        }
        LightKind::Tube => {
            let (length, radius) = (light.length, light.radius);
            LightData::Tube { color, intensity, temperature, range: light.range, shadows, length, radius }
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

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
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
    fn a_scene_that_names_lighting_is_lit_by_it_alone() {
        let file: crate::lighting::LightingFile =
            toml::from_str("look = \"golden_hour\"\n[sky]\nhaze = 2.5\n[post.bloom]\nintensity = 0.3").unwrap();
        let lit = file.resolve().unwrap();
        let mut engine = Engine::new();
        let own: toml::Table = toml::from_str("[bloom]\nscatter = 0.5").unwrap();
        let environment = Environment { lighting: Some(("Lighting/Dusk.lighting", &lit)), sky: None, post: Some(&own) };
        apply_environment(&mut engine, environment).unwrap();
        assert_eq!(engine.world.resource::<Sky>().map(|sky| sky.haze), Some(2.5), "the lighting's sky");
        assert!(engine.world.resource::<SkyFromLook>().is_some(), "which a save leaves out");
        assert_eq!(engine.world.resource::<SunLook>().copied(), Some(lit.sun));
        let post = engine.world.resource::<PostProcess>().copied().unwrap();
        let golden = Look::of(LookName::GoldenHour).post;
        assert_eq!((post.bloom.intensity, post.bloom.scatter), (0.3, golden.bloom.scatter), "the lighting's alone");
        assert_eq!(engine.world.resource::<SceneLighting>().map(|l| l.0.as_str()), Some("Lighting/Dusk.lighting"));
        assert!(engine.world.resource::<PostTable>().is_none(), "the scene's own [post] waits until it names none");
    }

    #[test]
    fn a_look_brings_its_sun_and_its_sky_where_the_scene_has_none() {
        let post = |text: &str| toml::from_str::<toml::Table>(text).unwrap();
        let apply = |engine: &mut Engine, sky: Option<SkyData>, text: Option<&str>| {
            let table = text.map(post);
            apply_environment(engine, Environment { lighting: None, sky, post: table.as_ref() })
        };
        let mut engine = Engine::new();
        apply(&mut engine, None, Some("look = \"storybook\"")).unwrap();
        let storybook = Look::of(LookName::Storybook);
        assert_eq!(engine.world.resource::<SunLook>().copied(), Some(storybook.sun));
        assert_eq!(engine.world.resource::<Sky>().map(|sky| sky.zenith), Some(storybook.sky.zenith));
        assert!(engine.world.resource::<SkyFromLook>().is_some(), "so a save leaves the sky out");

        apply(&mut engine, None, None).unwrap();
        assert!(engine.world.resource::<SunLook>().is_some(), "a scene without [post] keeps the look");

        let own: SkyData = toml::from_str("zenith = [0.1, 0.2, 0.3]\nhorizon = [0.5, 0.5, 0.5]\nground = [0.3, 0.3, 0.3]").unwrap();
        apply(&mut engine, Some(own), Some("look = \"soft_overcast\"")).unwrap();
        assert!(engine.world.resource::<SkyFromLook>().is_none(), "the scene's own sky wins and is saved");
        assert_eq!(engine.world.resource::<SunLook>().copied(), Some(Look::of(LookName::SoftOvercast).sun));

        apply(&mut engine, None, Some("look = \"storybook\"")).unwrap();
        apply(&mut engine, None, Some("[bloom]\nintensity = 0.1")).unwrap();
        assert!(engine.world.resource::<SunLook>().is_none(), "[post] without a look drops it");
        assert_eq!(engine.world.resource::<Sky>().map(|sky| sky.zenith), Some(Sky::default().zenith));
        assert!(apply(&mut engine, None, Some("look = \"gloomy\"")).is_err());
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
    fn seasons_read_and_write_back_with_each_seasons_look_and_leaves_on_materials() {
        let text = "id = 1\nname = \"Seasons\"\n[seasons]\ntime_of_year = 2.5\n[seasons.autumn]\ncolor_amount = 0.5";
        let obj: ObjData = toml::from_str(text).unwrap();
        let seasons = obj.seasons.expect("the seasons are read");
        assert_eq!((seasons.time_of_year, seasons.autumn.color_amount), (2.5, 0.5));
        assert_eq!(seasons.autumn.snow, Seasons::default().autumn.snow, "the rest of autumn as it starts");
        assert_eq!(toml::from_str::<ObjData>(&toml::to_string(&obj).unwrap()).unwrap().seasons, Some(seasons));
        let pine: MaterialData = toml::from_str("foliage = true\nleaves = \"stay\"").unwrap();
        assert_eq!(pine.leaves, voxear_render::Leaves::Stay);
        assert!(!toml::to_string(&MaterialData::default()).unwrap().contains("leaves"), "leaves that turn are not written");
    }

    #[test]
    fn an_ocean_reads_and_writes_back() {
        let obj: ObjData = toml::from_str("id = 1\nname = \"Sea\"\n[ocean]\nweather = \"stormy\"\nfoam = 0.8").unwrap();
        let ocean = obj.ocean.expect("the sea is read");
        assert_eq!((ocean.weather, ocean.foam), (voxear_render::Weather::Stormy, 0.8));
        assert_eq!(ocean.choppiness, Ocean::default().choppiness, "the rest as they start");
        assert_eq!(toml::from_str::<ObjData>(&toml::to_string(&obj).unwrap()).unwrap().ocean, Some(ocean));
        let obj: ObjData = toml::from_str("id = 2\n[pool]\nsize = [4.0, 8.0]\nliquid = \"lava\"\n[dry_area]\nshape = \"capsule\"").unwrap();
        assert_eq!(obj.pool.map(|p| (p.size, p.liquid)), Some(([4.0, 8.0], voxear_render::Liquid::Lava)));
        assert_eq!(obj.dry_area.as_ref().map(|d| d.shape), Some(voxear_render::DryShape::Capsule));
        let back: ObjData = toml::from_str(&toml::to_string(&obj).unwrap()).unwrap();
        assert_eq!((back.pool, back.dry_area), (obj.pool, obj.dry_area));
        let river = "id = 3\n[stream]\nspeed = 2.5\n[path]\npoints = [{ position = [0.0, 0.0, 0.0] }, { position = [0.0, -1.0, 8.0], width = 6.0 }]";
        let obj: ObjData = toml::from_str(river).unwrap();
        let path = obj.path.clone().expect("its path");
        assert_eq!((path.points.len(), path.points[0].width, path.points[1].width), (2, 4.0, 6.0), "a width left out as it starts");
        assert_eq!(obj.stream.map(|s| s.speed), Some(2.5));
        let back: ObjData = toml::from_str(&toml::to_string(&obj).unwrap()).unwrap();
        assert_eq!((back.path, back.stream), (obj.path, obj.stream));
        let raft: ObjData = toml::from_str("id = 4\n[buoyancy]\nsubmerged = 0.3").unwrap();
        assert_eq!(raft.buoyancy.map(|b| (b.submerged, b.drag)), Some((0.3, crate::water::Buoyancy::default().drag)));
    }

    #[test]
    fn a_mesh_dry_area_writes_its_mesh_as_a_renderer_does() {
        let obj: ObjData = toml::from_str("id = 3\n[dry_area]\nshape = \"mesh\"\nmesh = \"builtin:sphere\"").unwrap();
        let dry = obj.dry_area.clone().unwrap();
        assert_eq!((dry.shape, dry.mesh.as_deref()), (voxear_render::DryShape::Mesh, Some("builtin:sphere")));
        let back: ObjData = toml::from_str(&toml::to_string(&obj).unwrap()).unwrap();
        assert_eq!(back.dry_area, obj.dry_area);
        // Its own Mesh Renderer's mesh, written as none.
        let own: ObjData = toml::from_str("id = 4\n[dry_area]\nshape = \"mesh\"").unwrap();
        assert_eq!(own.dry_area.and_then(|d| d.mesh), None);
    }

    #[test]
    fn a_particle_system_reads_and_writes_back() {
        let text = "id = 1\nname = \"Sparks\"\n[particles]\nrate = 40.0\nblend = \"additive\"\nshape = \"sphere\"";
        let obj: ObjData = toml::from_str(text).unwrap();
        let system = obj.particles.expect("the system is read");
        use voxear_render::{EmitShape, ParticleBlend};
        assert_eq!((system.rate, system.blend, system.shape), (40.0, ParticleBlend::Additive, EmitShape::Sphere));
        assert_eq!(system.lifetime, ParticleSystem::default().lifetime, "the rest as a new one has");
        assert_eq!(toml::from_str::<ObjData>(&toml::to_string(&obj).unwrap()).unwrap().particles, Some(system));
    }

    #[test]
    fn a_wind_zone_reads_and_writes_back_and_foliage_moves_in_it_unless_told() {
        let obj: ObjData = toml::from_str("id = 1\nname = \"Wind\"\n[wind_zone]\nstrength = 2.0\ngusts = 0.0").unwrap();
        let wind = obj.wind_zone.expect("the zone is read");
        assert_eq!((wind.strength, wind.gusts, wind.turbulence), (2.0, 0.0, WindZone::default().turbulence));
        assert_eq!(toml::from_str::<ObjData>(&toml::to_string(&obj).unwrap()).unwrap().wind_zone, Some(wind));
        let runner: ObjData = toml::from_str("id = 2\nname = \"Runner\"\n[foliage_interaction]\nradius = 1.2").unwrap();
        let push = runner.foliage_interaction.expect("the interaction is read");
        assert_eq!((push.radius, push.strength), (1.2, 1.0));
        assert_eq!(toml::from_str::<ObjData>(&toml::to_string(&runner).unwrap()).unwrap().foliage_interaction, Some(push));

        let leaf: MaterialData = toml::from_str("foliage = true").unwrap();
        assert_eq!((leaf.wind_bend(), leaf.wind_flutter()), (1.0, 1.0), "foliage moves without setup");
        let still: MaterialData = toml::from_str("foliage = true\nwind_bend = 0.0\nwind_flutter = 0.0").unwrap();
        assert_eq!((still.wind_bend(), still.wind_flutter()), (0.0, 0.0));
        let trunk: MaterialData = toml::from_str("wind_bend = 1.0").unwrap();
        assert_eq!((trunk.wind_bend(), trunk.wind_flutter()), (1.0, 0.0), "a trunk bends with its leaves");
        assert!(!toml::to_string(&MaterialData::default()).unwrap().contains("wind"), "nothing written unset");
    }

    #[test]
    fn the_skys_haze_reads_and_writes_back_and_is_left_out_unset() {
        let colours = "zenith = [0.3, 0.5, 0.9]\nhorizon = [0.7, 0.8, 0.9]\nground = [0.4, 0.4, 0.4]";
        let plain: SkyData = toml::from_str(colours).unwrap();
        let written = toml::to_string(&plain).unwrap();
        assert!(!written.contains("haze") && !written.contains("sun_size"), "{written}");
        let hazy = "haze = 2.0\nhaze_tint = [1.0, 0.9, 0.8]\nhaze_glow = 0.9\ndistance_haze = 20.0\nsun_size = 1.0";
        let dusty: SkyData = toml::from_str(&format!("{colours}\n{hazy}")).unwrap();
        assert_eq!(toml::from_str::<SkyData>(&toml::to_string(&dusty).unwrap()).unwrap(), dusty);
        assert_eq!((dusty.haze, dusty.haze_glow, dusty.distance_haze, dusty.sun_size), (2.0, 0.9, 20.0, 1.0));
    }

    #[test]
    fn a_lights_temperature_reads_and_writes_back_and_is_left_out_unset() {
        let candle: LightData = toml::from_str("type = \"point\"\ntemperature = 1900.0").unwrap();
        assert_eq!(light_component(&candle).temperature, Some(1900.0));
        assert_eq!(toml::from_str::<LightData>(&toml::to_string(&candle).unwrap()).unwrap(), candle);
        let plain: LightData = toml::from_str("type = \"spot\"").unwrap();
        assert_eq!(light_component(&plain).temperature, None);
        assert!(!toml::to_string(&plain).unwrap().contains("temperature"));
        let lit = Light { temperature: Some(1900.0), ..default_light(LightKind::Point) };
        let warm = lit.light_color();
        assert!(warm.x > warm.z * 3.0, "a candle lights orange: {warm}");
    }

    #[test]
    fn a_models_part_changes_read_and_write_back_and_go_when_they_change_nothing() {
        let text = "model = \"Models/Pair.gltf\"\n\n[[part]]\nnode = 1\nposition = [-3.0, 0.0, 0.0]\n\
                    material = \"Materials/Brass.material\"\n\n[[part]]\nnode = 0\napart = true\n";
        let mut data: ModelData = toml::from_str(text).unwrap();
        assert_eq!(data.part(1).and_then(|part| part.position), Some([-3.0, 0.0, 0.0]));
        assert!(data.part(0).is_some_and(|part| part.apart));
        assert_eq!(toml::from_str::<ModelData>(&toml::to_string(&data).unwrap()).unwrap(), data);
        let placed = (Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY, Vec3::ONE);
        assert_eq!(data.part(1).unwrap().transform(placed).0, Vec3::new(-3.0, 0.0, 0.0), "moved where it says");
        assert_eq!(data.part(1).unwrap().transform(placed).2, Vec3::ONE, "and the rest as the model has it");
        *data.part_mut(1) = PartData { node: 1, ..PartData::default() };
        data.part_mut(2);
        data.tidy_parts();
        assert_eq!(data.parts.len(), 1, "a part changed in nothing is left out: {data:?}");
    }

    #[test]
    fn a_lights_shadows_are_on_unless_turned_off_and_left_out_when_on() {
        for kind in ["point", "spot", "rectangle", "disc", "tube"] {
            let plain: LightData = toml::from_str(&format!("type = \"{kind}\"")).unwrap();
            assert!(light_component(&plain).shadows, "{kind} casts shadows unless told not to");
            assert!(!toml::to_string(&plain).unwrap().contains("shadows"));
            let none: LightData = toml::from_str(&format!("type = \"{kind}\"\nshadows = false")).unwrap();
            assert!(!light_component(&none).shadows, "{kind}");
            assert_eq!(toml::from_str::<LightData>(&toml::to_string(&none).unwrap()).unwrap(), none);
        }
        assert!(default_light(LightKind::Point).shadows, "a new light casts them too");
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
