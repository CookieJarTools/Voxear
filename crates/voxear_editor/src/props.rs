//! The components the Inspector shows below the Transform, and the values
//! in each that it edits. A component is a [`Part`]; each value in it is
//! named by a [`Prop`], read from the world as a [`Value`] and written
//! back. Script fields, and the values inside them, are props too (see
//! `fields`).

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::first_person::FirstPersonView;
use voxear_engine::guns::mechanism::{GunPart, PartMotion, PartRole};
use voxear_engine::guns::{Gun, GunHolder, GunKind};

/// A Gun Part's roles as the Inspector names them, in its menu's order.
const PART_ROLES: [(PartRole, &str); 9] = [
    (PartRole::Bolt, "Bolt"),
    (PartRole::Slide, "Slide"),
    (PartRole::Toggle, "Toggle"),
    (PartRole::Pump, "Pump"),
    (PartRole::Magazine, "Magazine"),
    (PartRole::Clip, "Clip"),
    (PartRole::Cover, "Cover"),
    (PartRole::Barrel, "Barrel"),
    (PartRole::Case, "Case"),
];
use voxear_engine::modules::Module;
use voxear_engine::{
    EditBehaviours, Game, MaterialSource,
    animation::{BoneAttachment, CLIP_PART},
    audio::{AudioBus, AudioListener, AudioSource, SoundOcclusion, SoundSpace},
    physics::{
        Acoustic, CharacterController, Collider, ColliderShape, Joint, JointType, Rigidbody, Vehicle, VehicleKind,
        Wheel,
    },
    physics2d::{Collider2D, ColliderShape2D, Rigidbody2D},
    linear,
    prefab::Locks,
    scene_file::{FoliageData, FoliageSource, ModelData, ModelSource, ScatterData, default_light, default_material},
    script_types::ScriptTypes,
    srgb,
};
use voxear_render::{
    Camera, Foliage, FoliageInteraction, Impostors, Light, LightKind, LodGroup, LodLevel, LodMeasure, MAX_TRANSITIONS, MeshRenderer, PostVolume, Renderer,
    ModelRenderer, Ocean, Season, Seasons, SkyBody, VolumeShape, Weather, WindShape, WindSource, WindZone, DryArea, DryShape, Liquid, Pool, Stream,
    EmitShape, ParticleBlend, ParticleSystem,
};
use voxear_scene::{BehaviourSpec, Behaviours, Name, Parent, SceneComponents};

use crate::fields::{self, Pick, Step};

/// The meshes every project has, as the Mesh menu lists them first.
const BUILTIN_MESHES: [(&str, &str); 3] =
    [("Cube", "builtin:cube"), ("Sphere", "builtin:sphere"), ("Plane", "builtin:plane")];
/// A mesh Dry Area's Mesh menu's first choice: its Obj's own Mesh Renderer's.
const OWN_MESH: &str = "Mesh Renderer";
/// The materials every project has, as the Material menu lists them after
/// the default.
pub(crate) const BUILTIN_MATERIALS: [(&str, &str); 1] = [("Glass", "builtin:glass")];
/// How a light's colour is set: as it is, or as a filter over a colour
/// temperature, as Unity's Light Appearance.
const LIGHT_APPEARANCES: [&str; 2] = ["Colour", "Filter and Temperature"];
/// The temperature a light starts at when it turns to one: daylight's.
const DAYLIGHT_KELVIN: f32 = 6500.0;
const LIGHT_KINDS: [&str; 6] = ["Directional", "Point", "Spot", "Rectangle", "Disc", "Tube"];
/// The light kinds in `LIGHT_KINDS`'s order.
const LIGHT_KIND_ORDER: [LightKind; 6] =
    [LightKind::Directional, LightKind::Point, LightKind::Spot, LightKind::Rectangle, LightKind::Disc, LightKind::Tube];
const SOUND_SPACES: [&str; 2] = ["3D", "2D"];
const SKY_BODIES: [&str; 2] = ["Sun", "Moon"];
/// The sky bodies in `SKY_BODIES`'s order.
const SKY_BODY_ORDER: [SkyBody; 2] = [SkyBody::Sun, SkyBody::Moon];
const VOLUME_SHAPES: [&str; 3] = ["Global", "Box", "Sphere"];
/// The volume shapes in `VOLUME_SHAPES`'s order.
const VOLUME_SHAPE_ORDER: [VolumeShape; 3] = [VolumeShape::Global, VolumeShape::Box, VolumeShape::Sphere];
const AUDIO_BUSES: [&str; 2] = ["Effects", "Music"];
const LOD_MEASURES: [&str; 2] = ["Screen size", "Distance"];

/// A component the Inspector shows as a section of its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Renderer,
    Light,
    Camera,
    Audio,
    Listener,
    LodGroup,
    Collider,
    Rigidbody,
    Character,
    Joint,
    Collider2D,
    Rigidbody2D,
    Vehicle,
    Wheel,
    PostVolume,
    WindZone,
    ParticleSystem,
    Smoke,
    Mist,
    NetObject,
    Animator,
    MotionMatcher,
    BoneAttachment,
    FirstPersonView,
    Gun,
    GunHolder,
    GunPart,
    Ocean,
    Pool,
    DryArea,
    /// A Path, and a Stream running along it.
    Path,
    Stream,
    /// Floats its Obj's Rigidbody on water.
    Buoyancy,
    Seasons,
    FoliageInteraction,
    Foliage,
    /// A Model Renderer: a whole model drawn by the Obj.
    Model,
    /// A script, by its full type name.
    Behaviour(String),
}

impl Part {
    /// The component as a prefab's locks name it: a behaviour by its type, or
    /// `renderer`, `light`, `camera`, `audio`, `audio_listener` or `lod_group`.
    pub fn lock_key(&self) -> String {
        match self {
            Part::Renderer => "renderer".into(),
            Part::Light => "light".into(),
            Part::Camera => "camera".into(),
            Part::Audio => "audio".into(),
            Part::Listener => "audio_listener".into(),
            Part::LodGroup => "lod_group".into(),
            Part::Collider => "collider".into(),
            Part::Rigidbody => "rigidbody".into(),
            Part::Character => "character".into(),
            Part::Joint => "joint".into(),
            Part::Collider2D => "collider2d".into(),
            Part::Rigidbody2D => "rigidbody2d".into(),
            Part::Vehicle => "vehicle".into(),
            Part::Wheel => "wheel".into(),
            Part::PostVolume => "post_volume".into(),
            Part::WindZone => "wind_zone".into(),
            Part::ParticleSystem => "particles".into(),
            Part::Smoke => "smoke".into(),
            Part::Mist => "mist".into(),
            Part::NetObject => "net".into(),
            Part::Animator => "animator".into(),
            Part::MotionMatcher => "motion_matcher".into(),
            Part::BoneAttachment => "bone_attachment".into(),
            Part::FirstPersonView => "first_person".into(),
            Part::Gun => "gun".into(),
            Part::GunHolder => "gun_holder".into(),
            Part::GunPart => "gun_part".into(),
            Part::Ocean => "ocean".into(),
            Part::Pool => "pool".into(),
            Part::DryArea => "dry_area".into(),
            Part::Path => "path".into(),
            Part::Stream => "stream".into(),
            Part::Buoyancy => "buoyancy".into(),
            Part::Seasons => "seasons".into(),
            Part::FoliageInteraction => "foliage_interaction".into(),
            Part::Foliage => "foliage".into(),
            Part::Model => "model".into(),
            Part::Behaviour(name) => name.clone(),
        }
    }

    /// The optional module it belongs to, if any: a project without that
    /// module is not offered it.
    pub fn module(&self) -> Option<Module> {
        match self {
            Part::NetObject => Some(Module::Networking),
            Part::Gun | Part::GunHolder | Part::GunPart => Some(Module::Guns),
            Part::Collider2D | Part::Rigidbody2D => Some(Module::Physics2D),
            Part::Seasons | Part::FoliageInteraction | Part::Foliage => Some(Module::Vegetation),
            Part::Ocean | Part::Pool | Part::DryArea | Part::Stream | Part::Buoyancy => Some(Module::Water),
            _ => None,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Part::Renderer => "Mesh Renderer",
            Part::Light => "Light",
            Part::Camera => "Camera",
            Part::Audio => "Audio Source",
            Part::Listener => "Audio Listener",
            Part::LodGroup => "LOD Group",
            Part::Collider => "Collider",
            Part::Rigidbody => "Rigidbody",
            Part::Character => "Character Controller",
            Part::Joint => "Joint",
            Part::Collider2D => "Collider 2D",
            Part::Rigidbody2D => "Rigidbody 2D",
            Part::Vehicle => "Vehicle",
            Part::Wheel => "Wheel",
            Part::PostVolume => "Post Volume",
            Part::WindZone => "Wind Zone",
            Part::ParticleSystem => "Particle System",
            Part::Smoke => "Smoke",
            Part::Mist => "Mist",
            Part::NetObject => "Net Object",
            Part::Animator => "Animator",
            Part::MotionMatcher => "Motion Matcher",
            Part::BoneAttachment => "Bone Attachment",
            Part::FirstPersonView => "First-Person View",
            Part::Gun => "Gun",
            Part::GunHolder => "Gun Holder",
            Part::GunPart => "Gun Part",
            Part::Ocean => "Ocean",
            Part::Pool => "Pool",
            Part::DryArea => "Dry Area",
            Part::Path => "Path",
            Part::Stream => "Stream",
            Part::Buoyancy => "Buoyancy",
            Part::Seasons => "Seasons",
            Part::FoliageInteraction => "Foliage Interaction",
            Part::Foliage => "Foliage",
            Part::Model => "Model Renderer",
            Part::Behaviour(name) => name.rsplit('.').next().unwrap_or(name),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Part::Renderer => "cube",
            Part::Light => "light",
            Part::Camera => "camera",
            Part::Audio | Part::Listener => "audio",
            Part::LodGroup => "lod",
            Part::Collider => "box",
            Part::Rigidbody => "move",
            Part::Character => "obj",
            Part::Joint => "pivot",
            Part::Collider2D => "box",
            Part::Rigidbody2D => "move",
            Part::Vehicle => "move",
            Part::Wheel => "rotate",
            Part::PostVolume => "panels",
            Part::WindZone => "move",
            Part::ParticleSystem => "particles",
            Part::Smoke => "particles",
            Part::Mist => "particles",
            Part::NetObject => "globe",
            Part::Animator => "play",
            Part::MotionMatcher => "play",
            Part::BoneAttachment => "play",
            Part::FirstPersonView => "camera",
            Part::Gun | Part::GunHolder | Part::GunPart => "crosshair",
            Part::Ocean => "globe",
            Part::Pool => "globe",
            Part::DryArea => "box",
            Part::Path => "move",
            Part::Stream => "globe",
            Part::Buoyancy => "move",
            Part::Seasons => "globe",
            Part::FoliageInteraction => "move",
            Part::Foliage => "cube",
            Part::Model => "box",
            Part::Behaviour(_) => "code",
        }
    }
}

/// One value in a component.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    Mesh,
    Material,
    /// How soon a Mesh Renderer switches to simpler LODs.
    LodBias,
    LightKind,
    /// Whether a light's colour is its own, or a filter over a temperature.
    LightAppearance,
    LightColor,
    LightTemperature,
    Intensity,
    Range,
    SpotInner,
    SpotOuter,
    LightWidth,
    LightHeight,
    LightRadius,
    LightLength,
    LightCookie,
    LightCookieSize,
    LightProfile,
    /// Whether a local light casts shadows.
    LightShadows,
    LightSoftness,
    LightShadowTint,
    LightBody,
    VolumeShape,
    VolumeBlend,
    VolumeWeight,
    VolumePriority,
    WindStrength,
    WindGusts,
    WindGustSize,
    WindGustSpeed,
    WindTurbulence,
    /// A Wind Zone's shape, and a Sphere's width or a Box's size, and how far
    /// out it fades.
    WindShape,
    WindSize,
    WindWidth,
    WindFalloff,
    /// A Particle System's round, its stream and burst; each particle's life,
    /// speed and size; its colours, as it starts and as it dies, how it
    /// blends and is lit; where it starts from; what pulls and turns it.
    ParticleDuration,
    ParticleLooping,
    ParticleRate,
    ParticleBurst,
    ParticleLifetime,
    ParticleSpeed,
    ParticleSize,
    ParticleSizeEnd,
    ParticleColor,
    ParticleOpacity,
    ParticleColorEnd,
    ParticleOpacityEnd,
    ParticleIntensity,
    ParticleBlend,
    ParticleLit,
    ParticleSoftness,
    ParticleShape,
    ParticleRadius,
    ParticleAngle,
    ParticleBoxSize,
    ParticleGravity,
    ParticleDrag,
    ParticleSpin,
    ParticleSeed,
    ParticleSizeCurve,
    ParticleOpacityCurve,
    ParticleSpeedCurve,
    ParticleColorGradient,
    /// A Smoke's size and cells, how thick it is and its colour, how long it
    /// spreads, lasts and fades, and which swirls.
    SmokeSize,
    SmokeCell,
    SmokeDensity,
    SmokeColor,
    SmokeSpread,
    SmokeLasts,
    SmokeFade,
    SmokeSeed,
    SmokePlayOnStart,
    /// A Mist's trail: whether it is left now, how wide and thick it is, its
    /// colour, how long it lingers, and which swirls.
    MistEmitting,
    MistWidth,
    MistDensity,
    MistColor,
    MistLinger,
    MistSeed,
    /// A Net Object's priority and range.
    NetPriority,
    NetRange,
    /// An Animator's clip, how fast and whether it loops, how long a change
    /// of clip blends, and whether its root moves the Obj.
    AnimatorClip,
    AnimatorSpeed,
    AnimatorLooping,
    AnimatorBlend,
    AnimatorRootMotion,
    /// A Motion Matcher's settings: the clip it stands with and below what
    /// speed, how often it searches, how its clips fade and pace, and how
    /// its body turns.
    MatcherMotionSet,
    MatcherIdleSpeed,
    MatcherSearchEvery,
    MatcherBlend,
    MatcherTurnRate,
    MatcherTurnToMovement,
    MatcherLegTurn,
    MatcherMatchSpeed,
    MatcherRootMotion,
    MatcherFootLocking,
    MatcherDebug,
    /// A Bone Attachment's bone, where it sits from it, whether it turns
    /// with it, and whether it hides it.
    AttachBone,
    AttachOffset,
    AttachTurn,
    AttachHide,
    /// A First-Person View's mouse look and how quick it is, how far it
    /// tilts, how much bob reaches the camera, where the eyes are, and
    /// whether it turns the Obj, leans the body and hides the head.
    ViewMouseLook,
    ViewSensitivity,
    ViewPitchLimits,
    ViewBob,
    ViewEyes,
    ViewTurnObj,
    ViewLean,
    ViewTwist,
    ViewHideHead,
    /// A Gun's kind, weight, length, rate of fire and kick; its grip and
    /// support sockets, sights and muzzle.
    GunKind,
    GunMass,
    GunLength,
    GunFireRate,
    GunRecoil,
    GunGripAt,
    GunGripTurn,
    GunSupportAt,
    GunSupportTurn,
    GunRearSight,
    GunFrontSight,
    GunEyeRelief,
    GunAimFov,
    GunMuzzle,
    GunButt,
    /// A Gun Holder's hip hold, how long aiming takes, how much it sways,
    /// bobs and breathes, and whether it pulls back from walls.
    HolderHip,
    HolderHipTurn,
    HolderShoulder,
    HolderAimTime,
    HolderSway,
    HolderBob,
    HolderBreathing,
    HolderWallCheck,
    /// A Gun Part's role, how it moves (sliding or turning, along or about
    /// an axis, by how much, through what), where a hand takes it, and the
    /// pull back after a turn that a turn-bolt has.
    PartRole,
    PartMotion,
    PartAxis,
    PartAmount,
    PartPivot,
    PartHandle,
    PartThenPull,
    /// An Ocean's weather, and for Custom weather its swell and wind; its
    /// crests, colours and foam.
    OceanWeather,
    /// Where an Ocean's wind comes from, and how long its waves take to follow it.
    OceanWindSource,
    OceanTransition,
    OceanSwellWind,
    OceanSwellFetch,
    OceanWind,
    OceanFetch,
    OceanChoppiness,
    OceanWaveScale,
    OceanDeep,
    OceanScatter,
    OceanFoam,
    OceanSpray,
    OceanClarity,
    OceanMurk,
    /// An Ocean's costlier looks, each turned on or off.
    OceanShores,
    OceanFoamTrails,
    OceanFoam3d,
    OceanLightBending,
    OceanLightShafts,
    OceanWetLens,
    OceanFineRipples,
    /// A Pool's footprint and liquid, its waves, and its water's colours.
    PoolSize,
    PoolDepth,
    PoolLiquid,
    PoolWaves,
    PoolDeep,
    PoolScatter,
    PoolClarity,
    PoolMurk,
    /// Lava's: how long it takes to crust over, and how brightly it glows.
    PoolCooling,
    PoolGlow,
    /// A Path's points, edited in the Scene view, and whether it loops.
    PathPoints,
    PathClosed,
    /// A Stream's liquid, depth and speed, where it breaks into rapids, and
    /// its water's colours.
    StreamLiquid,
    StreamDepth,
    StreamSpeed,
    StreamRapids,
    StreamDeep,
    StreamScatter,
    StreamClarity,
    StreamMurk,
    StreamCooling,
    StreamGlow,
    /// How much of a floating Obj sits under still water, and how quickly
    /// the water slows it.
    BuoyancySubmerged,
    BuoyancyDrag,
    BuoyancyAngularDrag,
    /// A Dry Area's shape, and its size as that shape has it.
    DryShape,
    DrySize,
    DryWidth,
    DryHeight,
    /// A mesh Dry Area's mesh, or its Obj's Mesh Renderer's.
    DryMesh,
    /// How deep the water in a Dry Area stands.
    DryBilge,
    /// A Seasons' time of year, and the season it is nearest, which the
    /// rows of a season's look below it are for.
    TimeOfYear,
    Season,
    SeasonColor(Season),
    SeasonColorAmount(Season),
    SeasonSnow(Season),
    SeasonVariation,
    SeasonPatchSize,
    Dryness,
    DryColor,
    Wetness,
    GroundCover,
    PushRadius,
    PushStrength,
    FoliageDraw,
    /// A Foliage's colours, which its plants are tinted between, and how
    /// wide the patches are.
    FoliageHealthy,
    FoliageDry,
    FoliageSpread,
    /// A Model Renderer's model, its LOD bias, and how many parts it draws.
    ModelAsset,
    ModelLodBias,
    ModelParts,
    FoliageFade,
    FoliageShadow,
    /// Whether far plants are impostors, and from how far when it says.
    FoliageImpostors,
    FoliageImpostorDistance,
    FieldOfView,
    Near,
    Exposure,
    Sound,
    SoundVolume,
    Pitch,
    Loop,
    PlayOnStart,
    SoundSpace,
    MinDistance,
    MaxDistance,
    /// How strongly a 3D source's pitch follows its and the listener's speed.
    DopplerLevel,
    /// Whether walls stop a 3D source's sound, how far round it a
    /// volumetric one is heard from, whether walls let some through, and
    /// whether it comes round them along the level's baked paths.
    Occlusion,
    OcclusionRadius,
    Transmission,
    Pathing,
    SoundBus,
    /// What a LOD Group's levels are measured by.
    LodMeasure,
    /// Whether a LOD Group shows nothing past its last level.
    LodCull,
    /// One of the Objs a LOD Group's level shows, by the level's index and
    /// its own.
    LodObj(usize, usize),
    /// Where an Obj is added to a LOD Group's level, by the level's index.
    LodAdd(usize),
    /// Where a LOD Group's level ends, by the level's index.
    LodUntil(usize),
    /// A collider's shape, where its middle is, and the sizes its shape
    /// uses; whether it is a trigger; and its friction and bounciness.
    ColliderShape,
    ColliderCenter,
    ColliderSize,
    ColliderRadius,
    ColliderHeight,
    /// A Mesh collider's: its mesh's convex hull rather than its triangles.
    ColliderConvex,
    ColliderTrigger,
    Friction,
    Bounciness,
    /// What a collider is made of, to sound.
    AcousticMaterial,
    /// A Rigidbody's mass, whether the game moves it, whether gravity pulls
    /// it, its drags, and whether it sweeps as it moves.
    Mass,
    Kinematic,
    UseGravity,
    Drag,
    AngularDrag,
    Continuous,
    /// A Character Controller's size, steepest slope and highest step.
    CharacterHeight,
    CharacterRadius,
    SlopeLimit,
    StepOffset,
    /// A Joint's type, the Obj it holds to, its anchor and axis, and whether
    /// it has limits, with the least and the greatest.
    JointKind,
    JointConnected,
    JointAnchor,
    JointAxis,
    JointLimited,
    JointMin,
    JointMax,
    /// A Collider 2D's, as a Collider's, in the XY plane.
    Shape2D,
    Offset2D,
    Size2D,
    Radius2D,
    Height2D,
    Trigger2D,
    Friction2D,
    Bounciness2D,
    /// A Rigidbody 2D's mass, whether the game moves it, how hard gravity
    /// pulls it, its drags, whether it never turns, and whether it sweeps.
    Mass2D,
    Kinematic2D,
    GravityScale,
    Drag2D,
    AngularDrag2D,
    FixedRotation,
    Continuous2D,
    /// A Vehicle's kind, engine and gearbox.
    VehicleKind,
    MaxTorque,
    MinRpm,
    MaxRpm,
    Automatic,
    Gears,
    FirstGear,
    TopGear,
    AntiRoll,
    /// A Wheel's size, suspension, steering, drive and brakes.
    WheelRadius,
    WheelWidth,
    Suspension,
    Spring,
    Damping,
    SteerAngle,
    Powered,
    WheelBrake,
    Handbrake,
    Grip,
    SideGrip,
    /// A script field, or a value inside one: its behaviour's type name,
    /// and the path to it from the field.
    Field {
        behaviour: String,
        path: Vec<Step>,
    },
}

impl Prop {
    /// The component the value belongs to.
    pub fn part(&self) -> Part {
        match self {
            Prop::Mesh | Prop::Material | Prop::LodBias => Part::Renderer,
            Prop::LightKind
            | Prop::LightAppearance
            | Prop::LightTemperature
            | Prop::LightColor
            | Prop::Intensity
            | Prop::Range
            | Prop::SpotInner
            | Prop::SpotOuter
            | Prop::LightWidth
            | Prop::LightHeight
            | Prop::LightRadius
            | Prop::LightLength
            | Prop::LightCookie
            | Prop::LightCookieSize
            | Prop::LightProfile
            | Prop::LightShadows
            | Prop::LightSoftness
            | Prop::LightShadowTint
            | Prop::LightBody => {
                Part::Light
            }
            Prop::FieldOfView | Prop::Near | Prop::Exposure => Part::Camera,
            Prop::VolumeShape | Prop::VolumeBlend | Prop::VolumeWeight | Prop::VolumePriority => Part::PostVolume,
            Prop::WindStrength
            | Prop::WindGusts
            | Prop::WindGustSize
            | Prop::WindGustSpeed
            | Prop::WindTurbulence
            | Prop::WindShape
            | Prop::WindSize
            | Prop::WindWidth
            | Prop::WindFalloff => {
                Part::WindZone
            }
            Prop::ParticleDuration
            | Prop::ParticleLooping
            | Prop::ParticleRate
            | Prop::ParticleBurst
            | Prop::ParticleLifetime
            | Prop::ParticleSpeed
            | Prop::ParticleSize
            | Prop::ParticleSizeEnd
            | Prop::ParticleColor
            | Prop::ParticleOpacity
            | Prop::ParticleColorEnd
            | Prop::ParticleOpacityEnd
            | Prop::ParticleIntensity
            | Prop::ParticleBlend
            | Prop::ParticleLit
            | Prop::ParticleSoftness
            | Prop::ParticleShape
            | Prop::ParticleRadius
            | Prop::ParticleAngle
            | Prop::ParticleBoxSize
            | Prop::ParticleGravity
            | Prop::ParticleDrag
            | Prop::ParticleSpin
            | Prop::ParticleSeed
            | Prop::ParticleSizeCurve
            | Prop::ParticleOpacityCurve
            | Prop::ParticleSpeedCurve
            | Prop::ParticleColorGradient => Part::ParticleSystem,
            Prop::SmokeSize
            | Prop::SmokeCell
            | Prop::SmokeDensity
            | Prop::SmokeColor
            | Prop::SmokeSpread
            | Prop::SmokeLasts
            | Prop::SmokeFade
            | Prop::SmokeSeed
            | Prop::SmokePlayOnStart => Part::Smoke,
            Prop::MistEmitting
            | Prop::MistWidth
            | Prop::MistDensity
            | Prop::MistColor
            | Prop::MistLinger
            | Prop::MistSeed => Part::Mist,
            Prop::NetPriority | Prop::NetRange => Part::NetObject,
            Prop::AnimatorClip
            | Prop::AnimatorSpeed
            | Prop::AnimatorLooping
            | Prop::AnimatorBlend
            | Prop::AnimatorRootMotion => Part::Animator,
            Prop::MatcherMotionSet
            | Prop::MatcherIdleSpeed
            | Prop::MatcherSearchEvery
            | Prop::MatcherBlend
            | Prop::MatcherTurnRate
            | Prop::MatcherTurnToMovement
            | Prop::MatcherLegTurn
            | Prop::MatcherMatchSpeed
            | Prop::MatcherRootMotion
            | Prop::MatcherFootLocking
            | Prop::MatcherDebug => Part::MotionMatcher,
            Prop::AttachBone | Prop::AttachOffset | Prop::AttachTurn | Prop::AttachHide => Part::BoneAttachment,
            Prop::ViewMouseLook
            | Prop::ViewSensitivity
            | Prop::ViewPitchLimits
            | Prop::ViewBob
            | Prop::ViewEyes
            | Prop::ViewTurnObj
            | Prop::ViewLean
            | Prop::ViewTwist
            | Prop::ViewHideHead => Part::FirstPersonView,
            Prop::GunKind
            | Prop::GunMass
            | Prop::GunLength
            | Prop::GunFireRate
            | Prop::GunRecoil
            | Prop::GunGripAt
            | Prop::GunGripTurn
            | Prop::GunSupportAt
            | Prop::GunSupportTurn
            | Prop::GunRearSight
            | Prop::GunFrontSight
            | Prop::GunEyeRelief
            | Prop::GunAimFov
            | Prop::GunMuzzle
            | Prop::GunButt => Part::Gun,
            Prop::HolderHip
            | Prop::HolderHipTurn
            | Prop::HolderShoulder
            | Prop::HolderAimTime
            | Prop::HolderSway
            | Prop::HolderBob
            | Prop::HolderBreathing
            | Prop::HolderWallCheck => Part::GunHolder,
            Prop::PartRole
            | Prop::PartMotion
            | Prop::PartAxis
            | Prop::PartAmount
            | Prop::PartPivot
            | Prop::PartHandle
            | Prop::PartThenPull => Part::GunPart,
            Prop::OceanWeather
            | Prop::OceanWindSource
            | Prop::OceanTransition
            | Prop::OceanSwellWind
            | Prop::OceanSwellFetch
            | Prop::OceanWind
            | Prop::OceanFetch
            | Prop::OceanChoppiness
            | Prop::OceanWaveScale
            | Prop::OceanDeep
            | Prop::OceanScatter
            | Prop::OceanFoam
            | Prop::OceanSpray
            | Prop::OceanClarity
            | Prop::OceanMurk
            | Prop::OceanShores
            | Prop::OceanFoamTrails
            | Prop::OceanFoam3d
            | Prop::OceanLightBending
            | Prop::OceanLightShafts
            | Prop::OceanWetLens
            | Prop::OceanFineRipples => Part::Ocean,
            Prop::PoolSize
            | Prop::PoolDepth
            | Prop::PoolLiquid
            | Prop::PoolWaves
            | Prop::PoolDeep
            | Prop::PoolScatter
            | Prop::PoolClarity
            | Prop::PoolMurk
            | Prop::PoolCooling
            | Prop::PoolGlow => Part::Pool,
            Prop::DryShape | Prop::DrySize | Prop::DryWidth | Prop::DryHeight | Prop::DryMesh | Prop::DryBilge => {
                Part::DryArea
            }
            Prop::PathPoints | Prop::PathClosed => Part::Path,
            Prop::StreamLiquid
            | Prop::StreamDepth
            | Prop::StreamSpeed
            | Prop::StreamRapids
            | Prop::StreamDeep
            | Prop::StreamScatter
            | Prop::StreamClarity
            | Prop::StreamMurk
            | Prop::StreamCooling
            | Prop::StreamGlow => Part::Stream,
            Prop::BuoyancySubmerged | Prop::BuoyancyDrag | Prop::BuoyancyAngularDrag => Part::Buoyancy,
            Prop::TimeOfYear
            | Prop::Season
            | Prop::SeasonColor(_)
            | Prop::SeasonColorAmount(_)
            | Prop::SeasonSnow(_)
            | Prop::SeasonVariation
            | Prop::SeasonPatchSize
            | Prop::Dryness
            | Prop::DryColor
            | Prop::Wetness
            | Prop::GroundCover => Part::Seasons,
            Prop::PushRadius | Prop::PushStrength => Part::FoliageInteraction,
            Prop::FoliageDraw
            | Prop::FoliageFade
            | Prop::FoliageShadow
            | Prop::FoliageHealthy
            | Prop::FoliageDry
            | Prop::FoliageSpread
            | Prop::FoliageImpostors
            | Prop::FoliageImpostorDistance => Part::Foliage,
            Prop::ModelAsset | Prop::ModelLodBias | Prop::ModelParts => Part::Model,
            Prop::Sound
            | Prop::SoundVolume
            | Prop::Pitch
            | Prop::Loop
            | Prop::PlayOnStart
            | Prop::SoundSpace
            | Prop::MinDistance
            | Prop::MaxDistance
            | Prop::DopplerLevel
            | Prop::Occlusion
            | Prop::OcclusionRadius
            | Prop::Transmission
            | Prop::Pathing
            | Prop::SoundBus => Part::Audio,
            Prop::LodMeasure | Prop::LodCull | Prop::LodObj(..) | Prop::LodAdd(_) | Prop::LodUntil(_) => {
                Part::LodGroup
            }
            Prop::ColliderShape
            | Prop::ColliderCenter
            | Prop::ColliderSize
            | Prop::ColliderRadius
            | Prop::ColliderHeight
            | Prop::ColliderConvex
            | Prop::ColliderTrigger
            | Prop::Friction
            | Prop::Bounciness
            | Prop::AcousticMaterial => Part::Collider,
            Prop::Mass | Prop::Kinematic | Prop::UseGravity | Prop::Drag | Prop::AngularDrag | Prop::Continuous => {
                Part::Rigidbody
            }
            Prop::CharacterHeight | Prop::CharacterRadius | Prop::SlopeLimit | Prop::StepOffset => Part::Character,
            Prop::JointKind
            | Prop::JointConnected
            | Prop::JointAnchor
            | Prop::JointAxis
            | Prop::JointLimited
            | Prop::JointMin
            | Prop::JointMax => Part::Joint,
            Prop::Shape2D
            | Prop::Offset2D
            | Prop::Size2D
            | Prop::Radius2D
            | Prop::Height2D
            | Prop::Trigger2D
            | Prop::Friction2D
            | Prop::Bounciness2D => Part::Collider2D,
            Prop::Mass2D
            | Prop::Kinematic2D
            | Prop::GravityScale
            | Prop::Drag2D
            | Prop::AngularDrag2D
            | Prop::FixedRotation
            | Prop::Continuous2D => Part::Rigidbody2D,
            Prop::VehicleKind
            | Prop::MaxTorque
            | Prop::MinRpm
            | Prop::MaxRpm
            | Prop::Automatic
            | Prop::Gears
            | Prop::FirstGear
            | Prop::TopGear
            | Prop::AntiRoll => Part::Vehicle,
            Prop::WheelRadius
            | Prop::WheelWidth
            | Prop::Suspension
            | Prop::Spring
            | Prop::Damping
            | Prop::SteerAngle
            | Prop::Powered
            | Prop::WheelBrake
            | Prop::Handbrake
            | Prop::Grip
            | Prop::SideGrip => Part::Wheel,
            Prop::Field { behaviour, .. } => Part::Behaviour(behaviour.clone()),
        }
    }

    /// The value as a prefab's locks name the fields shown to designers: a
    /// script value by its field, and the rest by a name of their own.
    pub fn lock_key(&self) -> String {
        let name = match self {
            Prop::Mesh => "mesh",
            Prop::Material => "material",
            Prop::LodBias => "lod_bias",
            Prop::LightKind => "type",
            Prop::LightAppearance => "appearance",
            Prop::LightColor => "color",
            Prop::LightTemperature => "temperature",
            Prop::Intensity => "intensity",
            Prop::Range => "range",
            Prop::SpotInner => "spot_inner",
            Prop::SpotOuter => "spot_outer",
            Prop::LightWidth => "width",
            Prop::LightHeight => "height",
            Prop::LightRadius => "radius",
            Prop::LightLength => "length",
            Prop::LightCookie => "cookie",
            Prop::LightCookieSize => "cookie_size",
            Prop::LightProfile => "profile",
            Prop::LightShadows => "shadows",
            Prop::LightSoftness => "softness",
            Prop::LightShadowTint => "shadow_tint",
            Prop::LightBody => "body",
            Prop::VolumeShape => "shape",
            Prop::VolumeBlend => "blend_distance",
            Prop::VolumeWeight => "weight",
            Prop::VolumePriority => "priority",
            Prop::WindStrength => "strength",
            Prop::WindGusts => "gusts",
            Prop::WindGustSize => "gust_size",
            Prop::WindGustSpeed => "gust_speed",
            Prop::WindTurbulence => "turbulence",
            Prop::WindShape => "shape",
            Prop::WindSize | Prop::WindWidth => "size",
            Prop::WindFalloff => "falloff",
            Prop::ParticleDuration => "duration",
            Prop::ParticleLooping => "looping",
            Prop::ParticleRate => "rate",
            Prop::ParticleBurst => "burst",
            Prop::ParticleLifetime => "lifetime",
            Prop::ParticleSpeed => "speed",
            Prop::ParticleSize => "size",
            Prop::ParticleSizeEnd => "size_end",
            Prop::ParticleColor => "color",
            Prop::ParticleOpacity => "color",
            Prop::ParticleColorEnd => "color_end",
            Prop::ParticleOpacityEnd => "color_end",
            Prop::ParticleIntensity => "intensity",
            Prop::ParticleBlend => "blend",
            Prop::ParticleLit => "lit",
            Prop::ParticleSoftness => "softness",
            Prop::ParticleShape => "shape",
            Prop::ParticleRadius => "radius",
            Prop::ParticleAngle => "angle",
            Prop::ParticleBoxSize => "box_size",
            Prop::ParticleGravity => "gravity",
            Prop::ParticleDrag => "drag",
            Prop::ParticleSpin => "spin",
            Prop::ParticleSeed => "seed",
            Prop::ParticleSizeCurve => "size_curve",
            Prop::ParticleOpacityCurve => "opacity_curve",
            Prop::ParticleSpeedCurve => "speed_curve",
            Prop::ParticleColorGradient => "color_gradient",
            Prop::SmokeSize => "size",
            Prop::SmokeCell => "cell",
            Prop::SmokeDensity => "density",
            Prop::SmokeColor => "color",
            Prop::SmokeSpread => "spread",
            Prop::SmokeLasts => "lasts",
            Prop::SmokeFade => "fade",
            Prop::SmokeSeed => "seed",
            Prop::SmokePlayOnStart => "play_on_start",
            Prop::MistEmitting => "emitting",
            Prop::NetPriority => "priority",
            Prop::NetRange => "range",
            Prop::MistWidth => "width",
            Prop::MistDensity => "density",
            Prop::MistColor => "color",
            Prop::MistLinger => "linger",
            Prop::MistSeed => "seed",
            Prop::AnimatorClip => "clip",
            Prop::AnimatorSpeed => "speed",
            Prop::AnimatorLooping => "looping",
            Prop::AnimatorBlend => "blend",
            Prop::AnimatorRootMotion => "root_motion",
            Prop::MatcherMotionSet => "motion_set",
            Prop::MatcherIdleSpeed => "idle_speed",
            Prop::MatcherSearchEvery => "search_every",
            Prop::MatcherBlend => "blend",
            Prop::MatcherTurnRate => "turn_rate",
            Prop::MatcherTurnToMovement => "turn_to_movement",
            Prop::MatcherLegTurn => "leg_turn",
            Prop::MatcherMatchSpeed => "match_speed",
            Prop::MatcherRootMotion => "root_motion",
            Prop::MatcherFootLocking => "foot_locking",
            Prop::MatcherDebug => "debug",
            Prop::AttachBone => "bone",
            Prop::AttachOffset => "offset",
            Prop::AttachTurn => "turn",
            Prop::AttachHide => "hide",
            Prop::ViewMouseLook => "mouse_look",
            Prop::ViewSensitivity => "sensitivity",
            Prop::ViewPitchLimits => "pitch_limits",
            Prop::ViewBob => "bob",
            Prop::ViewEyes => "eyes",
            Prop::ViewTurnObj => "turn_obj",
            Prop::ViewLean => "lean",
            Prop::ViewTwist => "twist",
            Prop::ViewHideHead => "hide_head",
            Prop::GunKind => "kind",
            Prop::GunMass => "mass",
            Prop::GunLength => "length",
            Prop::GunFireRate => "fire_rate",
            Prop::GunRecoil => "recoil",
            Prop::GunGripAt => "grip.position",
            Prop::GunGripTurn => "grip.rotation",
            Prop::GunSupportAt => "support.position",
            Prop::GunSupportTurn => "support.rotation",
            Prop::GunRearSight => "rear_sight",
            Prop::GunFrontSight => "front_sight",
            Prop::GunEyeRelief => "eye_relief",
            Prop::GunAimFov => "aim_fov",
            Prop::GunMuzzle => "muzzle",
            Prop::GunButt => "butt",
            Prop::HolderHip => "hip",
            Prop::HolderHipTurn => "hip_turn",
            Prop::HolderShoulder => "shoulder",
            Prop::HolderAimTime => "aim_time",
            Prop::HolderSway => "sway",
            Prop::HolderBob => "bob",
            Prop::HolderBreathing => "breathing",
            Prop::HolderWallCheck => "wall_check",
            Prop::PartRole => "role",
            Prop::PartMotion => "motion.type",
            Prop::PartAxis => "motion.axis",
            Prop::PartAmount => "motion.amount",
            Prop::PartPivot => "motion.pivot",
            Prop::PartHandle => "handle",
            Prop::PartThenPull => "then",
            Prop::OceanWeather => "weather",
            Prop::OceanWindSource => "wind_source",
            Prop::OceanTransition => "transition",
            Prop::OceanSwellWind => "swell_wind",
            Prop::OceanSwellFetch => "swell_fetch",
            Prop::OceanWind => "wind",
            Prop::OceanFetch => "fetch",
            Prop::OceanChoppiness => "choppiness",
            Prop::OceanWaveScale => "wave_scale",
            Prop::OceanDeep => "deep",
            Prop::OceanScatter => "scatter",
            Prop::OceanFoam => "foam",
            Prop::OceanSpray => "spray",
            Prop::OceanClarity => "clarity",
            Prop::OceanMurk => "murk",
            Prop::OceanShores => "shores",
            Prop::OceanFoamTrails => "foam_trails",
            Prop::OceanFoam3d => "foam_3d",
            Prop::OceanLightBending => "light_bending",
            Prop::OceanLightShafts => "light_shafts",
            Prop::OceanWetLens => "wet_lens",
            Prop::OceanFineRipples => "fine_ripples",
            Prop::PoolSize => "size",
            Prop::PoolDepth => "depth",
            Prop::PoolLiquid => "liquid",
            Prop::PoolWaves => "waves",
            Prop::PoolDeep => "deep",
            Prop::PoolScatter => "scatter",
            Prop::PoolClarity => "clarity",
            Prop::PoolMurk => "murk",
            Prop::PoolCooling | Prop::StreamCooling => "cooling",
            Prop::PoolGlow | Prop::StreamGlow => "glow",
            Prop::BuoyancySubmerged => "submerged",
            Prop::BuoyancyDrag => "drag",
            Prop::BuoyancyAngularDrag => "angular_drag",
            Prop::PathPoints => "points",
            Prop::PathClosed => "closed",
            Prop::StreamLiquid => "liquid",
            Prop::StreamDepth => "depth",
            Prop::StreamSpeed => "speed",
            Prop::StreamRapids => "rapids",
            Prop::StreamDeep => "deep",
            Prop::StreamScatter => "scatter",
            Prop::StreamClarity => "clarity",
            Prop::StreamMurk => "murk",
            Prop::DryShape => "shape",
            Prop::DrySize | Prop::DryWidth | Prop::DryHeight => "size",
            Prop::DryMesh => "mesh",
            Prop::DryBilge => "bilge",
            Prop::TimeOfYear | Prop::Season => "time_of_year",
            Prop::SeasonColor(season) => return format!("{}.color", season_key(*season)),
            Prop::SeasonColorAmount(season) => return format!("{}.color_amount", season_key(*season)),
            Prop::SeasonSnow(season) => return format!("{}.snow", season_key(*season)),
            Prop::SeasonVariation => "variation",
            Prop::SeasonPatchSize => "patch_size",
            Prop::Dryness => "dryness",
            Prop::DryColor => "dry_color",
            Prop::Wetness => "wetness",
            Prop::GroundCover => "ground_cover",
            Prop::PushRadius => "radius",
            Prop::PushStrength => "strength",
            Prop::FoliageDraw => "draw_distance",
            Prop::FoliageHealthy => "healthy_color",
            Prop::FoliageDry => "dry_color",
            Prop::FoliageSpread => "color_spread",
            Prop::ModelAsset => "model",
            Prop::ModelLodBias => "lod_bias",
            Prop::ModelParts => "parts",
            Prop::FoliageFade => "fade_distance",
            Prop::FoliageShadow => "shadow_distance",
            Prop::FoliageImpostors => "impostors",
            Prop::FoliageImpostorDistance => "impostor_distance",
            Prop::FieldOfView => "fov",
            Prop::Near => "near",
            Prop::Exposure => "exposure",
            Prop::Sound => "sound",
            Prop::SoundVolume => "volume",
            Prop::Pitch => "pitch",
            Prop::Loop => "loop",
            Prop::PlayOnStart => "play_on_start",
            Prop::SoundSpace => "mode",
            Prop::MinDistance => "min_distance",
            Prop::MaxDistance => "max_distance",
            Prop::DopplerLevel => "doppler",
            Prop::Occlusion => "occlusion",
            Prop::OcclusionRadius => "occlusion_radius",
            Prop::Transmission => "transmission",
            Prop::Pathing => "pathing",
            Prop::SoundBus => "bus",
            Prop::LodMeasure => "measure",
            Prop::LodCull => "cull",
            Prop::LodObj(..) | Prop::LodAdd(_) | Prop::LodUntil(_) => "levels",
            Prop::ColliderShape => "shape",
            Prop::ColliderCenter => "center",
            Prop::ColliderSize => "size",
            Prop::ColliderRadius | Prop::CharacterRadius => "radius",
            Prop::ColliderHeight | Prop::CharacterHeight => "height",
            Prop::ColliderConvex => "convex",
            Prop::ColliderTrigger => "trigger",
            Prop::Friction => "friction",
            Prop::Bounciness => "bounciness",
            Prop::AcousticMaterial => "acoustic",
            Prop::Mass => "mass",
            Prop::Kinematic => "kinematic",
            Prop::UseGravity => "gravity",
            Prop::Drag => "drag",
            Prop::AngularDrag => "angular_drag",
            Prop::Continuous => "continuous",
            Prop::SlopeLimit => "slope_limit",
            Prop::StepOffset => "step_offset",
            Prop::JointKind => "type",
            Prop::JointConnected => "connected",
            Prop::JointAnchor => "anchor",
            Prop::JointAxis => "axis",
            Prop::JointLimited | Prop::JointMin | Prop::JointMax => "limits",
            Prop::Shape2D => "shape",
            Prop::Offset2D => "offset",
            Prop::Size2D => "size",
            Prop::Radius2D => "radius",
            Prop::Height2D => "height",
            Prop::Trigger2D => "trigger",
            Prop::Friction2D => "friction",
            Prop::Bounciness2D => "bounciness",
            Prop::Mass2D => "mass",
            Prop::Kinematic2D => "kinematic",
            Prop::GravityScale => "gravity_scale",
            Prop::Drag2D => "drag",
            Prop::AngularDrag2D => "angular_drag",
            Prop::FixedRotation => "fixed_rotation",
            Prop::Continuous2D => "continuous",
            Prop::VehicleKind => "kind",
            Prop::MaxTorque => "max_torque",
            Prop::MinRpm => "min_rpm",
            Prop::MaxRpm => "max_rpm",
            Prop::Automatic => "automatic",
            Prop::Gears => "gears",
            Prop::FirstGear => "first_gear",
            Prop::TopGear => "top_gear",
            Prop::AntiRoll => "anti_roll",
            Prop::WheelRadius => "radius",
            Prop::WheelWidth => "width",
            Prop::Suspension => "suspension",
            Prop::Spring => "spring",
            Prop::Damping => "damping",
            Prop::SteerAngle => "steer_angle",
            Prop::Powered => "powered",
            Prop::WheelBrake => "brake",
            Prop::Handbrake => "handbrake",
            Prop::Grip => "grip",
            Prop::SideGrip => "side_grip",
            Prop::Field { path, .. } => match path.first() {
                Some(crate::fields::Step::Key(field)) => return field.clone(),
                _ => "",
            },
        };
        name.to_string()
    }
}

/// A value as the Inspector shows and edits it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Whole(i64),
    Toggle(bool),
    Text(String),
    /// Two to four numbers.
    Vector(Vec<f32>),
    /// A colour as people pick them, in sRGB from 0 to 1.
    Color([f32; 3]),
    /// One of a list, by index.
    Choice(usize, Vec<String>),
    /// A reference to an Obj or a behaviour on one, by the Obj's name, picked
    /// from a menu.
    Obj(String),
    /// An asset, by its path, picked from a menu.
    Asset(String),
    /// A list's header: how many items it has. Its items follow as props.
    List(usize),
    /// A `[Serializable]` table's header. Its fields follow as props.
    Group,
    /// Shown, but not edited here.
    Fixed(String),
    /// Numbers along the way, edited on a small graph.
    Curve(voxear_core::curve::Curve),
    /// Colours along the way, linear, edited on a strip.
    Gradient(voxear_core::curve::Gradient),
}

/// The components an Obj has, in the order the Inspector shows them. While
/// playing, its scripts are the running ones.
pub fn parts(world: &World, entity: Entity) -> Vec<Part> {
    let mut parts = Vec::new();
    if world.has::<MeshRenderer>(entity) {
        parts.push(Part::Renderer);
    }
    if world.has::<Light>(entity) {
        parts.push(Part::Light);
    }
    if world.has::<Camera>(entity) {
        parts.push(Part::Camera);
    }
    if world.has::<PostVolume>(entity) {
        parts.push(Part::PostVolume);
    }
    if world.has::<WindZone>(entity) {
        parts.push(Part::WindZone);
    }
    if world.has::<ParticleSystem>(entity) {
        parts.push(Part::ParticleSystem);
    }
    if world.has::<voxear_render::Smoke>(entity) {
        parts.push(Part::Smoke);
    }
    if world.has::<voxear_render::Mist>(entity) {
        parts.push(Part::Mist);
    }
    if world.has::<voxear_engine::net::NetObject>(entity) {
        parts.push(Part::NetObject);
    }
    if world.has::<voxear_engine::animation::Animator>(entity) {
        parts.push(Part::Animator);
    }
    if world.has::<voxear_engine::animation::MotionMatcher>(entity) {
        parts.push(Part::MotionMatcher);
    }
    if world.has::<voxear_engine::animation::BoneAttachment>(entity) {
        parts.push(Part::BoneAttachment);
    }
    if world.has::<FirstPersonView>(entity) {
        parts.push(Part::FirstPersonView);
    }
    if world.has::<Gun>(entity) {
        parts.push(Part::Gun);
    }
    if world.has::<GunHolder>(entity) {
        parts.push(Part::GunHolder);
    }
    if world.has::<GunPart>(entity) {
        parts.push(Part::GunPart);
    }
    if world.has::<Ocean>(entity) {
        parts.push(Part::Ocean);
    }
    if world.has::<Pool>(entity) {
        parts.push(Part::Pool);
    }
    if world.has::<DryArea>(entity) {
        parts.push(Part::DryArea);
    }
    if world.has::<voxear_scene::Path>(entity) {
        parts.push(Part::Path);
    }
    if world.has::<Stream>(entity) {
        parts.push(Part::Stream);
    }
    if world.has::<voxear_engine::water::Buoyancy>(entity) {
        parts.push(Part::Buoyancy);
    }
    if world.has::<Seasons>(entity) {
        parts.push(Part::Seasons);
    }
    if world.has::<FoliageInteraction>(entity) {
        parts.push(Part::FoliageInteraction);
    }
    if world.has::<Foliage>(entity) {
        parts.push(Part::Foliage);
    }
    if world.has::<ModelSource>(entity) {
        parts.push(Part::Model);
    }
    if world.has::<AudioSource>(entity) {
        parts.push(Part::Audio);
    }
    if world.has::<AudioListener>(entity) {
        parts.push(Part::Listener);
    }
    if world.has::<LodGroup>(entity) {
        parts.push(Part::LodGroup);
    }
    if world.has::<Collider>(entity) {
        parts.push(Part::Collider);
    }
    if world.has::<Rigidbody>(entity) {
        parts.push(Part::Rigidbody);
    }
    if world.has::<CharacterController>(entity) {
        parts.push(Part::Character);
    }
    if world.has::<Joint>(entity) {
        parts.push(Part::Joint);
    }
    if world.has::<Collider2D>(entity) {
        parts.push(Part::Collider2D);
    }
    if world.has::<Rigidbody2D>(entity) {
        parts.push(Part::Rigidbody2D);
    }
    if world.has::<Vehicle>(entity) {
        parts.push(Part::Vehicle);
    }
    if world.has::<Wheel>(entity) {
        parts.push(Part::Wheel);
    }
    parts.extend(specs(world, entity).into_iter().map(|spec| Part::Behaviour(spec.type_name)));
    parts
}

/// An Obj's scripts, behaviours and C# components: the running ones while
/// playing, or in edit mode the same kept as data.
pub fn specs(world: &World, entity: Entity) -> Vec<BehaviourSpec> {
    let mut specs = match (world.get::<Behaviours>(entity), world.get::<EditBehaviours>(entity)) {
        (Some(running), _) => running.0.iter().map(|b| b.spec.clone()).collect(),
        (None, Some(kept)) => kept.0.clone(),
        (None, None) => Vec::new(),
    };
    specs.extend(world.get::<SceneComponents>(entity).into_iter().flat_map(|made| made.0.iter().cloned()));
    specs
}

/// The rows a 3D Audio Source's occlusion shows: none with it off, and the
/// radius only for a volumetric one.
fn occlusion_props(occlusion: SoundOcclusion) -> Vec<Prop> {
    match occlusion {
        SoundOcclusion::Off => Vec::new(),
        SoundOcclusion::Raycast => vec![Prop::Transmission, Prop::Pathing],
        SoundOcclusion::Volumetric => vec![Prop::OcclusionRadius, Prop::Transmission, Prop::Pathing],
    }
}

/// The values a component shows. A light shows only what its kind uses; a
/// script, the fields it declares, then any others its scene sets.
pub fn props(world: &World, entity: Entity, part: &Part, types: &ScriptTypes) -> Vec<Prop> {
    match part {
        Part::Renderer => vec![Prop::Mesh, Prop::Material, Prop::LodBias],
        Part::Light => {
            let kind = world.get::<Light>(entity).map_or(LightKind::Point, |l| l.kind);
            let temperature = world.get::<Light>(entity).and_then(|l| l.temperature).is_some();
            let mut props = vec![Prop::LightKind, Prop::LightAppearance, Prop::LightColor];
            if temperature {
                props.push(Prop::LightTemperature);
            }
            props.push(Prop::Intensity);
            if kind != LightKind::Directional {
                props.extend([Prop::Range, Prop::LightShadows]);
            }
            match kind {
                LightKind::Directional => props.extend([
                    Prop::LightBody,
                    Prop::LightSoftness,
                    Prop::LightShadowTint,
                    Prop::LightCookie,
                    Prop::LightCookieSize,
                ]),
                LightKind::Point => props.push(Prop::LightProfile),
                LightKind::Spot => props.extend([Prop::SpotInner, Prop::SpotOuter, Prop::LightCookie, Prop::LightProfile]),
                LightKind::Rectangle => props.extend([Prop::LightWidth, Prop::LightHeight]),
                LightKind::Disc => props.push(Prop::LightRadius),
                LightKind::Tube => props.extend([Prop::LightLength, Prop::LightRadius]),
            }
            props
        }
        Part::Camera => vec![Prop::FieldOfView, Prop::Near, Prop::Exposure],
        Part::PostVolume => vec![Prop::VolumeShape, Prop::VolumeBlend, Prop::VolumeWeight, Prop::VolumePriority],
        Part::WindZone => {
            // A Global one's gusts for the plants; a Sphere's or a Box's size over the sea.
            match world.get::<WindZone>(entity).map_or(WindShape::Global, |wind| wind.shape) {
                WindShape::Global => vec![
                    Prop::WindShape,
                    Prop::WindStrength,
                    Prop::WindGusts,
                    Prop::WindGustSize,
                    Prop::WindGustSpeed,
                    Prop::WindTurbulence,
                ],
                WindShape::Sphere => vec![Prop::WindShape, Prop::WindStrength, Prop::WindWidth, Prop::WindFalloff],
                WindShape::Box => vec![Prop::WindShape, Prop::WindStrength, Prop::WindSize, Prop::WindFalloff],
            }
        }
        Part::Smoke => vec![
            Prop::SmokeSize,
            Prop::SmokeCell,
            Prop::SmokeDensity,
            Prop::SmokeColor,
            Prop::SmokeSpread,
            Prop::SmokeLasts,
            Prop::SmokeFade,
            Prop::SmokeSeed,
            Prop::SmokePlayOnStart,
        ],
        Part::Mist => vec![
            Prop::MistEmitting,
            Prop::MistWidth,
            Prop::MistDensity,
            Prop::MistColor,
            Prop::MistLinger,
            Prop::MistSeed,
        ],
        Part::NetObject => vec![Prop::NetPriority, Prop::NetRange],
        Part::Animator => vec![
            Prop::AnimatorClip,
            Prop::AnimatorSpeed,
            Prop::AnimatorLooping,
            Prop::AnimatorBlend,
            Prop::AnimatorRootMotion,
        ],
        Part::MotionMatcher => vec![
            Prop::MatcherMotionSet,
            Prop::MatcherIdleSpeed,
            Prop::MatcherSearchEvery,
            Prop::MatcherBlend,
            Prop::MatcherTurnRate,
            Prop::MatcherTurnToMovement,
            Prop::MatcherLegTurn,
            Prop::MatcherMatchSpeed,
            Prop::MatcherRootMotion,
            Prop::MatcherFootLocking,
            Prop::MatcherDebug,
        ],
        Part::BoneAttachment => vec![Prop::AttachBone, Prop::AttachOffset, Prop::AttachTurn, Prop::AttachHide],
        Part::FirstPersonView => vec![
            Prop::ViewMouseLook,
            Prop::ViewSensitivity,
            Prop::ViewPitchLimits,
            Prop::ViewBob,
            Prop::ViewEyes,
            Prop::ViewTurnObj,
            Prop::ViewLean,
            Prop::ViewTwist,
            Prop::ViewHideHead,
        ],
        Part::Gun => vec![
            Prop::GunKind,
            Prop::GunMass,
            Prop::GunLength,
            Prop::GunFireRate,
            Prop::GunRecoil,
            Prop::GunGripAt,
            Prop::GunGripTurn,
            Prop::GunSupportAt,
            Prop::GunSupportTurn,
            Prop::GunRearSight,
            Prop::GunFrontSight,
            Prop::GunEyeRelief,
            Prop::GunAimFov,
            Prop::GunMuzzle,
            Prop::GunButt,
        ],
        Part::GunHolder => vec![
            Prop::HolderHip,
            Prop::HolderHipTurn,
            Prop::HolderShoulder,
            Prop::HolderAimTime,
            Prop::HolderSway,
            Prop::HolderBob,
            Prop::HolderBreathing,
            Prop::HolderWallCheck,
        ],
        Part::GunPart => vec![
            Prop::PartRole,
            Prop::PartMotion,
            Prop::PartAxis,
            Prop::PartAmount,
            Prop::PartPivot,
            Prop::PartHandle,
            Prop::PartThenPull,
        ],
        Part::ParticleSystem => {
            let shape = world.get::<ParticleSystem>(entity).map_or(EmitShape::Cone, |system| system.shape);
            let mut props = vec![
                Prop::ParticleDuration,
                Prop::ParticleLooping,
                Prop::ParticleRate,
                Prop::ParticleBurst,
                Prop::ParticleLifetime,
                Prop::ParticleSpeed,
                Prop::ParticleSpeedCurve,
                Prop::ParticleSize,
                Prop::ParticleSizeEnd,
                Prop::ParticleSizeCurve,
                Prop::ParticleColor,
                Prop::ParticleOpacity,
                Prop::ParticleColorEnd,
                Prop::ParticleOpacityEnd,
                Prop::ParticleColorGradient,
                Prop::ParticleOpacityCurve,
                Prop::ParticleIntensity,
                Prop::ParticleBlend,
                Prop::ParticleLit,
                Prop::ParticleSoftness,
                Prop::ParticleShape,
            ];
            // Only what its shape uses.
            props.extend(match shape {
                EmitShape::Point => vec![],
                EmitShape::Sphere => vec![Prop::ParticleRadius],
                EmitShape::Cone => vec![Prop::ParticleRadius, Prop::ParticleAngle],
                EmitShape::Box => vec![Prop::ParticleBoxSize],
            });
            props.extend([Prop::ParticleGravity, Prop::ParticleDrag, Prop::ParticleSpin, Prop::ParticleSeed]);
            props
        }
        // The swell and wind only for Custom weather: the others set their own.
        Part::Ocean => {
            let custom = world.get::<Ocean>(entity).is_some_and(|ocean| ocean.weather == Weather::Custom);
            let mut props = vec![Prop::OceanWeather, Prop::OceanWindSource, Prop::OceanTransition];
            if custom {
                props.extend([Prop::OceanSwellWind, Prop::OceanSwellFetch, Prop::OceanWind, Prop::OceanFetch]);
            }
            props.extend([
                Prop::OceanWaveScale,
                Prop::OceanChoppiness,
                Prop::OceanDeep,
                Prop::OceanScatter,
                Prop::OceanFoam,
                Prop::OceanSpray,
                Prop::OceanClarity,
                Prop::OceanMurk,
                Prop::OceanShores,
                Prop::OceanFoamTrails,
                Prop::OceanFoam3d,
                Prop::OceanLightBending,
                Prop::OceanLightShafts,
                Prop::OceanWetLens,
                Prop::OceanFineRipples,
            ]);
            props
        }
        // Water's colours only for water, lava's cooling only for lava.
        Part::Pool => {
            let water = world.get::<Pool>(entity).is_some_and(|pool| pool.liquid == Liquid::Water);
            let mut props = vec![Prop::PoolSize, Prop::PoolDepth, Prop::PoolLiquid];
            match water {
                true => props.extend([Prop::PoolWaves, Prop::PoolDeep, Prop::PoolScatter, Prop::PoolClarity, Prop::PoolMurk]),
                false => props.extend([Prop::PoolCooling, Prop::PoolGlow]),
            }
            props
        }
        Part::Path => vec![Prop::PathPoints, Prop::PathClosed],
        Part::Buoyancy => vec![Prop::BuoyancySubmerged, Prop::BuoyancyDrag, Prop::BuoyancyAngularDrag],
        Part::Stream => {
            let water = world.get::<Stream>(entity).is_some_and(|stream| stream.liquid == Liquid::Water);
            let mut props = vec![Prop::StreamLiquid, Prop::StreamDepth, Prop::StreamSpeed];
            match water {
                true => props.extend([Prop::StreamRapids, Prop::StreamDeep, Prop::StreamScatter, Prop::StreamClarity, Prop::StreamMurk]),
                false => props.extend([Prop::StreamCooling, Prop::StreamGlow]),
            }
            props
        }
        // Each shape's own measures: a box's three sides, a ball's width, a capsule's width and height.
        Part::DryArea => match world.get::<DryArea>(entity).map_or(DryShape::Box, |dry| dry.shape) {
            DryShape::Box => vec![Prop::DryShape, Prop::DrySize, Prop::DryBilge],
            DryShape::Sphere => vec![Prop::DryShape, Prop::DryWidth, Prop::DryBilge],
            DryShape::Capsule => vec![Prop::DryShape, Prop::DryWidth, Prop::DryHeight, Prop::DryBilge],
            DryShape::Mesh => vec![Prop::DryShape, Prop::DryMesh, Prop::DryBilge],
        },
        // The look of the season the year is nearest, under the choice of it.
        Part::Seasons => {
            let season = world.get::<Seasons>(entity).map_or(Season::Summer, |seasons| seasons.season());
            vec![
                Prop::TimeOfYear,
                Prop::Season,
                Prop::SeasonColor(season),
                Prop::SeasonColorAmount(season),
                Prop::SeasonSnow(season),
                Prop::SeasonVariation,
                Prop::SeasonPatchSize,
                Prop::Dryness,
                Prop::DryColor,
                Prop::Wetness,
                Prop::GroundCover,
            ]
        }
        Part::FoliageInteraction => vec![Prop::PushRadius, Prop::PushStrength],
        Part::Foliage => {
            let mut props = vec![Prop::FoliageDraw, Prop::FoliageFade, Prop::FoliageShadow, Prop::FoliageImpostors];
            if world.get::<Foliage>(entity).is_some_and(|f| f.impostors == Impostors::From) {
                props.push(Prop::FoliageImpostorDistance);
            }
            props.extend([Prop::FoliageHealthy, Prop::FoliageDry, Prop::FoliageSpread]);
            props
        }
        Part::Model => vec![Prop::ModelAsset, Prop::ModelLodBias, Prop::ModelParts],
        Part::Audio => {
            let mut props =
                vec![Prop::Sound, Prop::SoundVolume, Prop::Pitch, Prop::Loop, Prop::PlayOnStart, Prop::SoundBus];
            props.push(Prop::SoundSpace);
            let source = world.get::<AudioSource>(entity).copied().unwrap_or_default();
            if source.space == SoundSpace::ThreeD {
                props.extend([Prop::MinDistance, Prop::MaxDistance, Prop::DopplerLevel, Prop::Occlusion]);
                props.extend(occlusion_props(source.occlusion));
            }
            props
        }
        Part::Listener => Vec::new(),
        // A collider shows the sizes its shape uses; a mesh is the Obj's own, at its own middle.
        Part::Collider => {
            let shape = world.get::<Collider>(entity).map_or(ColliderShape::Box, |c| c.shape);
            let mut props = vec![Prop::ColliderShape];
            props.extend(match shape {
                ColliderShape::Box => vec![Prop::ColliderCenter, Prop::ColliderSize],
                ColliderShape::Sphere => vec![Prop::ColliderCenter, Prop::ColliderRadius],
                ColliderShape::Capsule => vec![Prop::ColliderCenter, Prop::ColliderRadius, Prop::ColliderHeight],
                ColliderShape::Mesh => vec![Prop::ColliderConvex],
            });
            props.extend([Prop::ColliderTrigger, Prop::Friction, Prop::Bounciness, Prop::AcousticMaterial]);
            props
        }
        Part::Rigidbody => {
            vec![Prop::Mass, Prop::Kinematic, Prop::UseGravity, Prop::Drag, Prop::AngularDrag, Prop::Continuous]
        }
        Part::Character => vec![Prop::CharacterHeight, Prop::CharacterRadius, Prop::SlopeLimit, Prop::StepOffset],
        // A joint shows the axis a hinge turns about, and limits for what has them.
        Part::Joint => {
            let joint = world.get::<Joint>(entity).copied().unwrap_or_default();
            let mut props = vec![Prop::JointKind, Prop::JointConnected, Prop::JointAnchor];
            if joint.kind == JointType::Hinge {
                props.push(Prop::JointAxis);
            }
            if matches!(joint.kind, JointType::Hinge | JointType::Distance) {
                props.push(Prop::JointLimited);
                if joint.limits.is_some() {
                    props.extend([Prop::JointMin, Prop::JointMax]);
                }
            }
            props
        }
        Part::Collider2D => {
            let shape = world.get::<Collider2D>(entity).map_or(ColliderShape2D::Box, |c| c.shape);
            let mut props = vec![Prop::Shape2D, Prop::Offset2D];
            props.extend(match shape {
                ColliderShape2D::Box => vec![Prop::Size2D],
                ColliderShape2D::Circle => vec![Prop::Radius2D],
                ColliderShape2D::Capsule => vec![Prop::Radius2D, Prop::Height2D],
            });
            props.extend([Prop::Trigger2D, Prop::Friction2D, Prop::Bounciness2D]);
            props
        }
        Part::Rigidbody2D => vec![
            Prop::Mass2D,
            Prop::Kinematic2D,
            Prop::GravityScale,
            Prop::Drag2D,
            Prop::AngularDrag2D,
            Prop::FixedRotation,
            Prop::Continuous2D,
        ],
        Part::Vehicle => vec![
            Prop::VehicleKind,
            Prop::MaxTorque,
            Prop::MinRpm,
            Prop::MaxRpm,
            Prop::Automatic,
            Prop::Gears,
            Prop::FirstGear,
            Prop::TopGear,
            Prop::AntiRoll,
        ],
        Part::Wheel => vec![
            Prop::WheelRadius,
            Prop::WheelWidth,
            Prop::Suspension,
            Prop::Spring,
            Prop::Damping,
            Prop::SteerAngle,
            Prop::Powered,
            Prop::WheelBrake,
            Prop::Handbrake,
            Prop::Grip,
            Prop::SideGrip,
        ],
        // Each level's Objs and a place to add another, then where it ends, which the last has only when the
        // group hides past it.
        Part::LodGroup => {
            let Some(group) = world.get::<LodGroup>(entity) else { return Vec::new() };
            let mut props = vec![Prop::LodMeasure];
            for (level, at) in group.levels.iter().enumerate() {
                props.extend((0..at.objs.len()).map(|index| Prop::LodObj(level, index)));
                props.push(Prop::LodAdd(level));
                if group.cull || level + 1 < group.levels.len() {
                    props.push(Prop::LodUntil(level));
                }
            }
            props.push(Prop::LodCull);
            props
        }
        Part::Behaviour(behaviour) => {
            let mut rows = Vec::new();
            let declared = types.behaviour(behaviour);
            for field in declared.into_iter().flat_map(|b| b.fields.iter()) {
                fields::expand(world, types, entity, behaviour, vec![Step::Key(field.name.clone())], &mut rows);
            }
            // Values the scene sets that the script does not declare, or before it is described.
            let spec = specs(world, entity).into_iter().find(|s| s.type_name == *behaviour);
            let unknown =
                spec.iter().flat_map(|s| s.fields.keys()).filter(|key| declared.is_none_or(|b| b.field(key).is_none()));
            rows.extend(unknown.map(|key| vec![Step::Key(key.clone())]).collect::<Vec<_>>());
            rows.into_iter().map(|path| Prop::Field { behaviour: behaviour.clone(), path }).collect()
        }
    }
}

/// What a prop's row says.
pub fn label(prop: &Prop) -> String {
    let label = match prop {
        Prop::Mesh => "Mesh",
        Prop::Material => "Material",
        Prop::LodBias => "LOD Bias",
        Prop::LightKind => "Type",
        Prop::LightAppearance => "Appearance",
        Prop::LightColor => "Colour",
        Prop::LightTemperature => "Temperature",
        Prop::Intensity => "Intensity",
        Prop::Range => "Range",
        Prop::SpotInner => "Inner angle",
        Prop::SpotOuter => "Outer angle",
        Prop::LightWidth => "Width",
        Prop::LightHeight => "Height",
        Prop::LightRadius => "Radius",
        Prop::LightLength => "Length",
        Prop::LightCookie => "Cookie",
        Prop::LightCookieSize => "Cookie size",
        Prop::LightProfile => "IES profile",
        Prop::LightShadows => "Shadows",
        Prop::LightSoftness => "Shadow softness",
        Prop::LightShadowTint => "Shadow tint",
        Prop::LightBody => "Sky body",
        Prop::VolumeShape => "Shape",
        Prop::VolumeBlend => "Blend distance",
        Prop::VolumeWeight => "Weight",
        Prop::VolumePriority => "Priority",
        Prop::WindStrength => "Strength",
        Prop::WindGusts => "Gusts",
        Prop::WindGustSize => "Gust size",
        Prop::WindGustSpeed => "Gust speed",
        Prop::WindShape => "Shape",
        Prop::WindSize => "Size",
        Prop::WindWidth => "Width",
        Prop::WindFalloff => "Falloff",
        Prop::WindTurbulence => "Turbulence",
        Prop::ParticleDuration => "Duration",
        Prop::ParticleLooping => "Looping",
        Prop::ParticleRate => "Rate",
        Prop::ParticleBurst => "Burst",
        Prop::ParticleLifetime => "Lifetime",
        Prop::ParticleSpeed => "Speed",
        Prop::ParticleSize => "Size",
        Prop::ParticleSizeEnd => "End Size",
        Prop::ParticleColor => "Colour",
        Prop::ParticleOpacity => "Opacity",
        Prop::ParticleColorEnd => "End Colour",
        Prop::ParticleOpacityEnd => "End Opacity",
        Prop::ParticleIntensity => "Intensity",
        Prop::ParticleBlend => "Blend",
        Prop::ParticleLit => "Lit",
        Prop::ParticleSoftness => "Softness",
        Prop::ParticleShape => "Shape",
        Prop::ParticleRadius => "Radius",
        Prop::ParticleAngle => "Angle",
        Prop::ParticleBoxSize => "Box Size",
        Prop::ParticleGravity => "Gravity",
        Prop::ParticleDrag => "Drag",
        Prop::ParticleSpin => "Spin",
        Prop::ParticleSeed => "Seed",
        Prop::ParticleSizeCurve => "Size Over Life",
        Prop::ParticleOpacityCurve => "Opacity Over Life",
        Prop::ParticleSpeedCurve => "Speed Over Life",
        Prop::ParticleColorGradient => "Colour Over Life",
        Prop::SmokeSize => "Size",
        Prop::SmokeCell => "Cell Size",
        Prop::SmokeDensity => "Density",
        Prop::SmokeColor => "Colour",
        Prop::SmokeSpread => "Spread Time",
        Prop::SmokeLasts => "Lasts",
        Prop::SmokeFade => "Disperse Time",
        Prop::SmokeSeed => "Seed",
        Prop::SmokePlayOnStart => "Play On Start",
        Prop::MistEmitting => "Emitting",
        Prop::NetPriority => "Priority",
        Prop::NetRange => "Range",
        Prop::MistWidth => "Width",
        Prop::MistDensity => "Density",
        Prop::MistColor => "Colour",
        Prop::MistLinger => "Linger",
        Prop::MistSeed => "Seed",
        Prop::AnimatorClip => "Clip",
        Prop::AnimatorSpeed => "Speed",
        Prop::AnimatorLooping => "Looping",
        Prop::AnimatorBlend => "Blend Time",
        Prop::AnimatorRootMotion => "Root Motion",
        Prop::MatcherMotionSet => "Motion Set",
        Prop::MatcherRootMotion => "Root Motion",
        Prop::MatcherIdleSpeed => "Idle Speed",
        Prop::MatcherSearchEvery => "Search Every",
        Prop::MatcherBlend => "Blend Time",
        Prop::MatcherTurnRate => "Turn Rate",
        Prop::MatcherTurnToMovement => "Turn To Movement",
        Prop::MatcherLegTurn => "Leg Turn",
        Prop::MatcherMatchSpeed => "Match Speed",
        Prop::MatcherFootLocking => "Foot Locking",
        Prop::MatcherDebug => "Debug",
        Prop::AttachBone => "Bone",
        Prop::AttachOffset => "Offset",
        Prop::AttachTurn => "Turn With Bone",
        Prop::AttachHide => "Hide Bone",
        Prop::ViewMouseLook => "Mouse Look",
        Prop::ViewSensitivity => "Sensitivity",
        Prop::ViewPitchLimits => "Tilt Limits",
        Prop::ViewBob => "Head Bob",
        Prop::ViewEyes => "Eyes",
        Prop::ViewTurnObj => "Turn Obj",
        Prop::ViewLean => "Lean",
        Prop::ViewTwist => "Twist",
        Prop::ViewHideHead => "Hide Head",
        Prop::GunKind => "Kind",
        Prop::GunMass => "Mass",
        Prop::GunLength => "Length",
        Prop::GunFireRate => "Fire Rate",
        Prop::GunRecoil => "Recoil",
        Prop::GunGripAt => "Grip",
        Prop::GunGripTurn => "Grip Turn",
        Prop::GunSupportAt => "Support",
        Prop::GunSupportTurn => "Support Turn",
        Prop::GunRearSight => "Rear Sight",
        Prop::GunFrontSight => "Front Sight",
        Prop::GunEyeRelief => "Eye Relief",
        Prop::GunAimFov => "Aim FOV",
        Prop::GunMuzzle => "Muzzle",
        Prop::GunButt => "Butt",
        Prop::HolderHip => "Hip",
        Prop::HolderHipTurn => "Hip Turn",
        Prop::HolderShoulder => "Shoulder",
        Prop::HolderAimTime => "Aim Time",
        Prop::HolderSway => "Sway",
        Prop::HolderBob => "Bob",
        Prop::HolderBreathing => "Breathing",
        Prop::HolderWallCheck => "Wall Check",
        Prop::PartRole => "Role",
        Prop::PartMotion => "Moves",
        Prop::PartAxis => "Axis",
        Prop::PartAmount => "Amount",
        Prop::PartPivot => "Pivot",
        Prop::PartHandle => "Handle",
        Prop::PartThenPull => "Then Pull",
        Prop::OceanWeather => "Weather",
        Prop::OceanWindSource => "Wind Source",
        Prop::OceanTransition => "Transition",
        Prop::OceanSwellWind => "Swell wind",
        Prop::OceanSwellFetch => "Swell fetch",
        Prop::OceanWind => "Wind",
        Prop::OceanFetch => "Fetch",
        Prop::OceanChoppiness => "Choppiness",
        Prop::OceanWaveScale => "Wave scale",
        Prop::OceanDeep => "Deep",
        Prop::OceanScatter => "Scatter",
        Prop::OceanFoam => "Foam",
        Prop::OceanSpray => "Spray",
        Prop::OceanClarity => "Clarity",
        Prop::OceanMurk => "Murk",
        Prop::OceanShores => "Shores",
        Prop::OceanFoamTrails => "Foam Trails",
        Prop::OceanFoam3d => "3D Foam",
        Prop::OceanLightBending => "Light Bending",
        Prop::OceanLightShafts => "Light Shafts",
        Prop::OceanWetLens => "Wet Lens",
        Prop::OceanFineRipples => "Fine Ripples",
        Prop::PoolSize => "Size",
        Prop::PoolDepth => "Depth",
        Prop::PoolLiquid => "Liquid",
        Prop::PoolWaves => "Waves",
        Prop::PoolDeep => "Deep",
        Prop::PoolScatter => "Scatter",
        Prop::PoolClarity => "Clarity",
        Prop::PoolMurk => "Murk",
        Prop::PoolCooling | Prop::StreamCooling => "Cooling",
        Prop::PoolGlow | Prop::StreamGlow => "Glow",
        Prop::BuoyancySubmerged => "Submerged",
        Prop::BuoyancyDrag => "Drag",
        Prop::BuoyancyAngularDrag => "Angular drag",
        Prop::PathPoints => "Points",
        Prop::PathClosed => "Closed",
        Prop::StreamLiquid => "Liquid",
        Prop::StreamDepth => "Depth",
        Prop::StreamSpeed => "Speed",
        Prop::StreamRapids => "Rapids",
        Prop::StreamDeep => "Deep",
        Prop::StreamScatter => "Scatter",
        Prop::StreamClarity => "Clarity",
        Prop::StreamMurk => "Murk",
        Prop::DryShape => "Shape",
        Prop::DrySize => "Size",
        Prop::DryWidth => "Width",
        Prop::DryHeight => "Height",
        Prop::DryMesh => "Mesh",
        Prop::DryBilge => "Bilge",
        Prop::TimeOfYear => "Time of year",
        Prop::Season => "Season",
        Prop::SeasonColor(_) => "Colour",
        Prop::SeasonColorAmount(_) => "Colour amount",
        Prop::SeasonSnow(_) => "Snow",
        Prop::SeasonVariation => "Variation",
        Prop::SeasonPatchSize => "Patch size",
        Prop::Dryness => "Dryness",
        Prop::DryColor => "Dry colour",
        Prop::Wetness => "Wetness",
        Prop::GroundCover => "Ground and rocks",
        Prop::PushRadius => "Radius",
        Prop::PushStrength => "Strength",
        Prop::FoliageDraw => "Draw distance",
        Prop::FoliageHealthy => "Healthy colour",
        Prop::FoliageDry => "Dry colour",
        Prop::FoliageSpread => "Colour patches",
        Prop::ModelAsset => "Model",
        Prop::ModelLodBias => "LOD bias",
        Prop::ModelParts => "Parts",
        Prop::FoliageFade => "Fade distance",
        Prop::FoliageShadow => "Shadow distance",
        Prop::FoliageImpostors => "Impostors",
        Prop::FoliageImpostorDistance => "Impostors from",
        Prop::FieldOfView => "Field of view",
        Prop::Near => "Near",
        Prop::Exposure => "Exposure",
        Prop::Sound => "Sound",
        Prop::SoundVolume => "Volume",
        Prop::Pitch => "Pitch",
        Prop::Loop => "Loop",
        Prop::PlayOnStart => "Play on start",
        Prop::SoundSpace => "Mode",
        Prop::MinDistance => "Min distance",
        Prop::MaxDistance => "Max distance",
        Prop::DopplerLevel => "Doppler level",
        Prop::Occlusion => "Occlusion",
        Prop::OcclusionRadius => "Occlusion radius",
        Prop::Transmission => "Transmission",
        Prop::Pathing => "Pathing",
        Prop::SoundBus => "Bus",
        Prop::LodMeasure => "Switch by",
        Prop::LodCull => "Culled",
        // The first of a level's Objs heads the list.
        Prop::LodObj(_, 0) => "Objs",
        Prop::LodObj(..) => "",
        Prop::LodAdd(_) => "Add Obj",
        Prop::LodUntil(_) => "Transition",
        Prop::ColliderShape => "Shape",
        Prop::ColliderCenter => "Center",
        Prop::ColliderSize => "Size",
        Prop::ColliderRadius | Prop::CharacterRadius => "Radius",
        Prop::ColliderHeight | Prop::CharacterHeight => "Height",
        Prop::ColliderConvex => "Convex",
        Prop::ColliderTrigger => "Is trigger",
        Prop::Friction => "Friction",
        Prop::Bounciness => "Bounciness",
        Prop::AcousticMaterial => "Acoustic material",
        Prop::Mass => "Mass",
        Prop::Kinematic => "Is kinematic",
        Prop::UseGravity => "Use gravity",
        Prop::Drag => "Drag",
        Prop::AngularDrag => "Angular drag",
        Prop::Continuous => "Continuous",
        Prop::SlopeLimit => "Slope limit",
        Prop::StepOffset => "Step offset",
        Prop::JointKind => "Type",
        Prop::JointConnected => "Connected",
        Prop::JointAnchor => "Anchor",
        Prop::JointAxis => "Axis",
        Prop::JointLimited => "Use limits",
        Prop::JointMin => "Min",
        Prop::JointMax => "Max",
        Prop::Shape2D => "Shape",
        Prop::Offset2D => "Offset",
        Prop::Size2D => "Size",
        Prop::Radius2D => "Radius",
        Prop::Height2D => "Height",
        Prop::Trigger2D => "Is trigger",
        Prop::Friction2D => "Friction",
        Prop::Bounciness2D => "Bounciness",
        Prop::Mass2D => "Mass",
        Prop::Kinematic2D => "Is kinematic",
        Prop::GravityScale => "Gravity scale",
        Prop::Drag2D => "Drag",
        Prop::AngularDrag2D => "Angular drag",
        Prop::FixedRotation => "Fixed rotation",
        Prop::Continuous2D => "Continuous",
        Prop::VehicleKind => "Kind",
        Prop::MaxTorque => "Max torque",
        Prop::MinRpm => "Min RPM",
        Prop::MaxRpm => "Max RPM",
        Prop::Automatic => "Automatic",
        Prop::Gears => "Gears",
        Prop::FirstGear => "First gear",
        Prop::TopGear => "Top gear",
        Prop::AntiRoll => "Anti-roll",
        Prop::WheelRadius => "Radius",
        Prop::WheelWidth => "Width",
        Prop::Suspension => "Suspension",
        Prop::Spring => "Spring",
        Prop::Damping => "Damping",
        Prop::SteerAngle => "Steer angle",
        Prop::Powered => "Powered",
        Prop::WheelBrake => "Brake",
        Prop::Handbrake => "Handbrake",
        Prop::Grip => "Grip",
        Prop::SideGrip => "Side grip",
        Prop::Field { path, .. } => {
            return match path.last() {
                Some(Step::Key(name)) => nice_name(name),
                Some(Step::Item(index)) => format!("Element {index}"),
                None => String::new(),
            };
        }
    };
    label.to_string()
}

/// The unit after a prop's value on an Obj, if it has one: a LOD Group's
/// transitions are a share of the screen or a distance.
pub fn unit_on(world: &World, entity: Entity, prop: &Prop) -> Option<&'static str> {
    match (prop, world.get::<LodGroup>(entity).map(|group| group.measure)) {
        (Prop::LodUntil(_), Some(LodMeasure::ScreenSize)) => Some("%"),
        (Prop::LodUntil(_), Some(LodMeasure::Distance)) => Some("m"),
        (Prop::SeasonPatchSize | Prop::FoliageSpread | Prop::FoliageImpostorDistance | Prop::NetRange, _) => Some("m"),
        (Prop::JointMin | Prop::JointMax, _) => match world.get::<Joint>(entity)?.kind {
            JointType::Hinge => Some("°"),
            _ => Some("m"),
        },
        _ => None,
    }
}

/// How deep a value sits inside its script field: 0 for a field itself.
pub fn depth(prop: &Prop) -> usize {
    match prop {
        Prop::Field { path, .. } => path.len().saturating_sub(1),
        _ => 0,
    }
}

/// A component as an Obj gets it from Add Component: a grey cube, a point
/// light, a camera, an Audio Source with no sound yet, an Audio Listener, a
/// LOD Group with levels of the Objs inside it (see `lod_levels_inside`),
/// or a script (a behaviour or, as `types` tell, a C# component) with every
/// field as it starts.
pub fn add(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    part: &Part,
    types: &ScriptTypes,
) -> anyhow::Result<()> {
    if *part == Part::Renderer {
        let renderer = renderer.ok_or_else(|| anyhow::anyhow!("meshes need the renderer"))?;
        let cube = game.content.parse(BUILTIN_MESHES[0].1)?;
        let mesh = game.content.mesh(renderer, &cube)?;
        let material = game.content.material(renderer, &MaterialSource::Inline(default_material()))?;
        game.engine.world.insert(entity, MeshRenderer::new(mesh, material));
        return Ok(());
    }
    if *part == Part::Foliage {
        // Small cubes scattered over ten metres, to swap for a plant of the project's.
        let renderer = renderer.ok_or_else(|| anyhow::anyhow!("foliage needs the renderer"))?;
        let scatter = ScatterData { size: [10.0, 10.0], density: 2.0, seed: 0, scale: [0.15, 0.3] };
        let data: FoliageData = toml::from_str(&format!("mesh = \"{}\"", BUILTIN_MESHES[0].1))?;
        let data = FoliageData { scatter: Some(scatter), ..data };
        let (foliage, source) = voxear_engine::prefab::foliage(&mut game.content, renderer, &data)?;
        game.engine.world.insert(entity, foliage);
        game.engine.world.insert(entity, source);
        return Ok(());
    }
    if *part == Part::LodGroup {
        let group = LodGroup::of_levels(lod_levels_inside(game, entity));
        game.engine.world.insert(entity, group);
        return Ok(());
    }
    let world = &mut game.engine.world;
    match part {
        // A model is placed from the Project panel, so Add Component does not offer one.
        Part::Renderer | Part::LodGroup | Part::Foliage | Part::Model => {}
        Part::Light => {
            world.insert(entity, default_light(LightKind::Point));
        }
        Part::Camera => {
            world.insert(entity, Camera::default());
        }
        Part::Audio => {
            world.insert(entity, AudioSource::default());
        }
        Part::Listener => {
            world.insert(entity, AudioListener);
        }
        Part::Collider => {
            world.insert(entity, Collider::default());
        }
        Part::Rigidbody => {
            world.insert(entity, Rigidbody::default());
        }
        Part::Character => {
            world.insert(entity, CharacterController::default());
        }
        Part::Joint => {
            world.insert(entity, Joint::default());
        }
        Part::Collider2D => {
            world.insert(entity, Collider2D::default());
        }
        Part::Rigidbody2D => {
            world.insert(entity, Rigidbody2D::default());
        }
        Part::Vehicle => {
            world.insert(entity, Vehicle::default());
        }
        Part::Wheel => {
            world.insert(entity, Wheel::default());
        }
        Part::PostVolume => {
            world.insert(entity, PostVolume::default());
        }
        Part::WindZone => {
            world.insert(entity, WindZone::default());
        }
        Part::ParticleSystem => {
            world.insert(entity, ParticleSystem::default());
        }
        Part::Smoke => {
            world.insert(entity, voxear_render::Smoke::default());
        }
        Part::Mist => {
            world.insert(entity, voxear_render::Mist::default());
        }
        Part::NetObject => {
            world.insert(entity, voxear_engine::net::NetObject::default());
        }
        Part::Animator => {
            world.insert(entity, voxear_engine::animation::Animator::default());
        }
        Part::MotionMatcher => {
            world.insert(entity, voxear_engine::animation::MotionMatcher::default());
        }
        Part::BoneAttachment => {
            world.insert(entity, voxear_engine::animation::BoneAttachment::default());
        }
        Part::FirstPersonView => {
            world.insert(entity, FirstPersonView::default());
        }
        Part::Gun => {
            world.insert(entity, Gun::default());
        }
        Part::GunHolder => {
            world.insert(entity, GunHolder::default());
        }
        Part::GunPart => {
            world.insert(entity, GunPart::default());
        }
        Part::Ocean => {
            world.insert(entity, Ocean::default());
        }
        Part::Pool => {
            world.insert(entity, Pool::default());
        }
        Part::DryArea => {
            world.insert(entity, DryArea::default());
        }
        Part::Path => {
            world.insert(entity, voxear_scene::Path::default());
        }
        // Floating needs a body physics moves, and a shape for the water to push.
        Part::Buoyancy => {
            if !world.has::<Collider>(entity) {
                world.insert(entity, Collider::default());
            }
            if !world.has::<Rigidbody>(entity) {
                world.insert(entity, Rigidbody::default());
            }
            world.insert(entity, voxear_engine::water::Buoyancy::default());
        }
        // A Stream runs along a Path: one is added with it where the Obj has none.
        Part::Stream => {
            if !world.has::<voxear_scene::Path>(entity) {
                world.insert(entity, voxear_scene::Path::default());
            }
            world.insert(entity, Stream::default());
        }
        Part::Seasons => {
            world.insert(entity, Seasons::default());
        }
        Part::FoliageInteraction => {
            world.insert(entity, FoliageInteraction::default());
        }
        Part::Behaviour(name) => {
            let spec = if types.is_component(name) { BehaviourSpec::component(name) } else { BehaviourSpec::new(name) };
            match world.get_mut::<EditBehaviours>(entity) {
                Some(kept) => kept.0.push(spec),
                None => {
                    world.insert(entity, EditBehaviours(vec![spec]));
                }
            }
        }
    }
    Ok(())
}

/// Takes a component off an Obj.
pub fn remove(world: &mut World, entity: Entity, part: &Part) {
    match part {
        Part::Renderer => drop(world.remove::<MeshRenderer>(entity)),
        Part::Light => drop(world.remove::<Light>(entity)),
        Part::Camera => drop(world.remove::<Camera>(entity)),
        Part::PostVolume => drop(world.remove::<PostVolume>(entity)),
        Part::WindZone => drop(world.remove::<WindZone>(entity)),
        Part::Smoke => {
            world.remove::<voxear_render::Smoke>(entity);
            world.remove::<voxear_render::SmokeFill>(entity);
            world.remove::<voxear_render::SmokeCue>(entity);
        }
        Part::Mist => {
            world.remove::<voxear_render::Mist>(entity);
            world.remove::<voxear_render::MistTrail>(entity);
        }
        Part::NetObject => {
            world.remove::<voxear_engine::net::NetObject>(entity);
        }
        Part::Animator => {
            world.remove::<voxear_engine::animation::Animator>(entity);
            world.remove::<voxear_engine::animation::AnimatorState>(entity);
            world.remove::<voxear_render::Pose>(entity);
        }
        Part::MotionMatcher => {
            world.remove::<voxear_engine::animation::MotionMatcher>(entity);
            world.remove::<voxear_engine::animation::MotionMatcherState>(entity);
            world.remove::<voxear_render::Pose>(entity);
        }
        Part::BoneAttachment => {
            world.remove::<voxear_engine::animation::BoneAttachment>(entity);
        }
        Part::FirstPersonView => {
            world.remove::<FirstPersonView>(entity);
        }
        Part::Gun => {
            world.remove::<Gun>(entity);
        }
        Part::GunHolder => {
            world.remove::<GunHolder>(entity);
            world.remove::<voxear_engine::guns::GunHolderState>(entity);
        }
        Part::GunPart => {
            world.remove::<GunPart>(entity);
        }
        Part::ParticleSystem => {
            world.remove::<ParticleSystem>(entity);
            world.remove::<voxear_render::ParticlePlayback>(entity);
        }
        Part::Ocean => drop(world.remove::<Ocean>(entity)),
        Part::Pool => drop(world.remove::<Pool>(entity)),
        Part::DryArea => drop(world.remove::<DryArea>(entity)),
        Part::Path => drop(world.remove::<voxear_scene::Path>(entity)),
        Part::Stream => drop(world.remove::<Stream>(entity)),
        Part::Buoyancy => drop(world.remove::<voxear_engine::water::Buoyancy>(entity)),
        Part::Seasons => drop(world.remove::<Seasons>(entity)),
        Part::FoliageInteraction => drop(world.remove::<FoliageInteraction>(entity)),
        Part::Foliage => {
            world.remove::<Foliage>(entity);
            drop(world.remove::<FoliageSource>(entity));
        }
        Part::Model => {
            world.remove::<ModelRenderer>(entity);
            drop(world.remove::<ModelSource>(entity));
        }
        Part::Audio => drop(world.remove::<AudioSource>(entity)),
        Part::Listener => drop(world.remove::<AudioListener>(entity)),
        Part::LodGroup => drop(world.remove::<LodGroup>(entity)),
        Part::Collider => drop(world.remove::<Collider>(entity)),
        Part::Rigidbody => drop(world.remove::<Rigidbody>(entity)),
        Part::Character => drop(world.remove::<CharacterController>(entity)),
        Part::Joint => drop(world.remove::<Joint>(entity)),
        Part::Collider2D => drop(world.remove::<Collider2D>(entity)),
        Part::Rigidbody2D => drop(world.remove::<Rigidbody2D>(entity)),
        Part::Vehicle => drop(world.remove::<Vehicle>(entity)),
        Part::Wheel => drop(world.remove::<Wheel>(entity)),
        Part::Behaviour(name) => {
            let Some(kept) = world.get_mut::<EditBehaviours>(entity) else { return };
            kept.0.retain(|spec| spec.type_name != *name);
            if kept.0.is_empty() {
                world.remove::<EditBehaviours>(entity);
            }
        }
    }
}

/// A prop's value now, or `None` when the Obj no longer has it. `types`
/// say what script fields hold and start as.
pub fn get(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> Option<Value> {
    let world = &game.engine.world;
    let light = || world.get::<Light>(entity);
    let camera = || world.get::<Camera>(entity);
    let audio = || world.get::<AudioSource>(entity);
    let lod = || world.get::<LodGroup>(entity);
    let collider = || world.get::<Collider>(entity);
    let body = || world.get::<Rigidbody>(entity);
    let character = || world.get::<CharacterController>(entity);
    let collider2d = || world.get::<Collider2D>(entity);
    let body2d = || world.get::<Rigidbody2D>(entity);
    let vehicle = || world.get::<Vehicle>(entity);
    let wheel = || world.get::<Wheel>(entity);
    Some(match prop {
        Prop::Shape2D => {
            let shape = collider2d()?.shape;
            let index = ColliderShape2D::ALL.iter().position(|s| *s == shape).unwrap_or_default();
            Value::Choice(index, ColliderShape2D::ALL.iter().map(|s| s.label().to_string()).collect())
        }
        Prop::Offset2D => Value::Vector(collider2d()?.offset.to_vec()),
        Prop::Size2D => Value::Vector(collider2d()?.size.to_vec()),
        Prop::Radius2D => Value::Number(collider2d()?.radius),
        Prop::Height2D => Value::Number(collider2d()?.height),
        Prop::Trigger2D => Value::Toggle(collider2d()?.trigger),
        Prop::Friction2D => Value::Number(collider2d()?.friction),
        Prop::Bounciness2D => Value::Number(collider2d()?.bounciness),
        Prop::Mass2D => Value::Number(body2d()?.mass),
        Prop::Kinematic2D => Value::Toggle(body2d()?.kinematic),
        Prop::GravityScale => Value::Number(body2d()?.gravity_scale),
        Prop::Drag2D => Value::Number(body2d()?.drag),
        Prop::AngularDrag2D => Value::Number(body2d()?.angular_drag),
        Prop::FixedRotation => Value::Toggle(body2d()?.fixed_rotation),
        Prop::Continuous2D => Value::Toggle(body2d()?.continuous),
        Prop::VehicleKind => {
            let kind = vehicle()?.kind;
            let index = VehicleKind::ALL.iter().position(|k| *k == kind).unwrap_or_default();
            Value::Choice(index, VehicleKind::ALL.iter().map(|k| k.label().to_string()).collect())
        }
        Prop::MaxTorque => Value::Number(vehicle()?.max_torque),
        Prop::MinRpm => Value::Number(vehicle()?.min_rpm),
        Prop::MaxRpm => Value::Number(vehicle()?.max_rpm),
        Prop::Automatic => Value::Toggle(vehicle()?.automatic),
        Prop::Gears => Value::Number(vehicle()?.gears as f32),
        Prop::FirstGear => Value::Number(vehicle()?.first_gear),
        Prop::TopGear => Value::Number(vehicle()?.top_gear),
        Prop::AntiRoll => Value::Number(vehicle()?.anti_roll),
        Prop::WheelRadius => Value::Number(wheel()?.radius),
        Prop::WheelWidth => Value::Number(wheel()?.width),
        Prop::Suspension => Value::Number(wheel()?.suspension),
        Prop::Spring => Value::Number(wheel()?.spring),
        Prop::Damping => Value::Number(wheel()?.damping),
        Prop::SteerAngle => Value::Number(wheel()?.steer_angle),
        Prop::Powered => Value::Toggle(wheel()?.powered),
        Prop::WheelBrake => Value::Number(wheel()?.brake),
        Prop::Handbrake => Value::Number(wheel()?.handbrake),
        Prop::Grip => Value::Number(wheel()?.grip),
        Prop::SideGrip => Value::Number(wheel()?.side_grip),
        Prop::ColliderShape => {
            let shape = collider()?.shape;
            let index = ColliderShape::ALL.iter().position(|s| *s == shape).unwrap_or_default();
            Value::Choice(index, ColliderShape::ALL.iter().map(|s| s.label().to_string()).collect())
        }
        Prop::ColliderCenter => Value::Vector(collider()?.center.to_vec()),
        Prop::ColliderSize => Value::Vector(collider()?.size.to_vec()),
        Prop::ColliderRadius => Value::Number(collider()?.radius),
        Prop::ColliderHeight => Value::Number(collider()?.height),
        Prop::ColliderConvex => Value::Toggle(collider()?.convex),
        Prop::ColliderTrigger => Value::Toggle(collider()?.trigger),
        Prop::Friction => Value::Number(collider()?.friction),
        Prop::Bounciness => Value::Number(collider()?.bounciness),
        Prop::AcousticMaterial => {
            let acoustic = collider()?.acoustic;
            let index = Acoustic::ALL.iter().position(|a| *a == acoustic).unwrap_or_default();
            Value::Choice(index, Acoustic::ALL.iter().map(|a| a.label().to_string()).collect())
        }
        Prop::Mass => Value::Number(body()?.mass),
        Prop::Kinematic => Value::Toggle(body()?.kinematic),
        Prop::UseGravity => Value::Toggle(body()?.gravity),
        Prop::Drag => Value::Number(body()?.drag),
        Prop::AngularDrag => Value::Number(body()?.angular_drag),
        Prop::Continuous => Value::Toggle(body()?.continuous),
        Prop::CharacterHeight => Value::Number(character()?.height),
        Prop::CharacterRadius => Value::Number(character()?.radius),
        Prop::SlopeLimit => Value::Number(character()?.slope_limit),
        Prop::StepOffset => Value::Number(character()?.step_offset),
        Prop::JointKind => {
            let kind = world.get::<Joint>(entity)?.kind;
            let index = JointType::ALL.iter().position(|k| *k == kind).unwrap_or_default();
            Value::Choice(index, JointType::ALL.iter().map(|k| k.label().to_string()).collect())
        }
        Prop::JointConnected => {
            let connected = world.get::<Joint>(entity)?.connected;
            Value::Obj(connected.and_then(|obj| world.get::<Name>(obj)).map_or_else(|| "None".into(), |n| n.0.clone()))
        }
        Prop::JointAnchor => Value::Vector(world.get::<Joint>(entity)?.anchor.to_vec()),
        Prop::JointAxis => Value::Vector(world.get::<Joint>(entity)?.axis.to_vec()),
        Prop::JointLimited => Value::Toggle(world.get::<Joint>(entity)?.limits.is_some()),
        Prop::JointMin => Value::Number(world.get::<Joint>(entity)?.limits?[0]),
        Prop::JointMax => Value::Number(world.get::<Joint>(entity)?.limits?[1]),
        Prop::Mesh => {
            let reference = game.content.mesh_ref(world.get::<MeshRenderer>(entity)?.mesh)?.to_string();
            match BUILTIN_MESHES.iter().find(|(_, r)| *r == reference) {
                Some((label, _)) => Value::Asset(label.to_string()),
                None => Value::Asset(mesh_label(game, &reference)),
            }
        }
        Prop::Material => {
            let source = game.content.material_source(world.get::<MeshRenderer>(entity)?.material)?;
            let builtin = |reference: &voxear_assets::AssetRef| {
                BUILTIN_MATERIALS.iter().find(|(_, r)| *r == reference.to_string()).map(|(label, _)| label.to_string())
            };
            Value::Asset(match source {
                MaterialSource::Inline(data) if *data == default_material() => "Default".into(),
                MaterialSource::Asset(reference) if builtin(reference).is_some() => builtin(reference).unwrap_or_default(),
                MaterialSource::Asset(reference) => reference
                    .guid()
                    .and_then(|guid| game.content.db().get(guid))
                    .map_or_else(|| reference.to_string(), |e| e.path.clone()),
                MaterialSource::Inline(_) => "Custom".into(),
            })
        }
        Prop::LightKind => {
            let kind = light()?.kind;
            let index = LIGHT_KIND_ORDER.iter().position(|&known| known == kind).unwrap_or(0);
            Value::Choice(index, LIGHT_KINDS.iter().map(|k| k.to_string()).collect())
        }
        Prop::LodBias => Value::Number(world.get::<MeshRenderer>(entity)?.lod_bias),
        Prop::LightColor => Value::Color(srgb(light()?.color.to_array()).map(round3)),
        Prop::LightAppearance => {
            let index = usize::from(light()?.temperature.is_some());
            Value::Choice(index, LIGHT_APPEARANCES.iter().map(|a| a.to_string()).collect())
        }
        Prop::LightTemperature => Value::Number(light()?.temperature?),
        Prop::Intensity => Value::Number(light()?.intensity),
        Prop::Range => Value::Number(light()?.range),
        Prop::SpotInner => Value::Number(light()?.spot_inner_degrees),
        Prop::SpotOuter => Value::Number(light()?.spot_outer_degrees),
        Prop::LightWidth => Value::Number(light()?.width),
        Prop::LightHeight => Value::Number(light()?.height),
        Prop::LightRadius => Value::Number(light()?.radius),
        Prop::LightLength => Value::Number(light()?.length),
        Prop::LightCookie => Value::Asset(guid_label(game, light()?.cookie.and_then(|t| game.content.picture_guid(t)))),
        Prop::LightCookieSize => Value::Number(light()?.cookie_size),
        Prop::ModelAsset => Value::Asset(path_of(game, &world.get::<ModelSource>(entity)?.0.model)?),
        Prop::ModelLodBias => Value::Number(world.get::<ModelRenderer>(entity)?.lod_bias),
        Prop::ModelParts => Value::Fixed(world.get::<ModelRenderer>(entity)?.parts.len().to_string()),
        Prop::LightSoftness => Value::Number(light()?.softness),
        Prop::LightShadows => Value::Toggle(light()?.shadows),
        Prop::LightShadowTint => Value::Color(srgb(light()?.shadow_tint.to_array()).map(round3)),
        Prop::LightBody => {
            let body = light()?.body;
            let index = SKY_BODY_ORDER.iter().position(|&known| known == body).unwrap_or(0);
            Value::Choice(index, SKY_BODIES.iter().map(|b| b.to_string()).collect())
        }
        Prop::LightProfile => {
            Value::Asset(guid_label(game, light()?.profile.and_then(|t| game.content.light_profile_guid(t))))
        }
        Prop::VolumeShape => {
            let shape = world.get::<PostVolume>(entity)?.shape;
            let index = VOLUME_SHAPE_ORDER.iter().position(|&known| known == shape).unwrap_or(0);
            Value::Choice(index, VOLUME_SHAPES.iter().map(|k| k.to_string()).collect())
        }
        Prop::VolumeBlend => Value::Number(world.get::<PostVolume>(entity)?.blend_distance),
        Prop::VolumeWeight => Value::Number(world.get::<PostVolume>(entity)?.weight),
        Prop::VolumePriority => Value::Number(world.get::<PostVolume>(entity)?.priority as f32),
        Prop::WindStrength => Value::Number(world.get::<WindZone>(entity)?.strength),
        Prop::WindGusts => Value::Number(world.get::<WindZone>(entity)?.gusts),
        Prop::WindGustSize => Value::Number(world.get::<WindZone>(entity)?.gust_size),
        Prop::WindGustSpeed => Value::Number(world.get::<WindZone>(entity)?.gust_speed),
        Prop::WindShape => {
            let shape = world.get::<WindZone>(entity)?.shape;
            let index = WindShape::ALL.iter().position(|s| *s == shape).unwrap_or(0);
            Value::Choice(index, WindShape::ALL.iter().map(|s| s.name().to_string()).collect())
        }
        Prop::WindSize => Value::Vector(world.get::<WindZone>(entity)?.size.to_vec()),
        Prop::WindWidth => Value::Number(world.get::<WindZone>(entity)?.size[0]),
        Prop::WindFalloff => Value::Number(world.get::<WindZone>(entity)?.falloff),
        Prop::WindTurbulence => Value::Number(world.get::<WindZone>(entity)?.turbulence),
        Prop::ParticleDuration
        | Prop::ParticleLooping
        | Prop::ParticleRate
        | Prop::ParticleBurst
        | Prop::ParticleLifetime
        | Prop::ParticleSpeed
        | Prop::ParticleSize
        | Prop::ParticleSizeEnd
        | Prop::ParticleColor
        | Prop::ParticleOpacity
        | Prop::ParticleColorEnd
        | Prop::ParticleOpacityEnd
        | Prop::ParticleIntensity
        | Prop::ParticleBlend
        | Prop::ParticleLit
        | Prop::ParticleSoftness
        | Prop::ParticleShape
        | Prop::ParticleRadius
        | Prop::ParticleAngle
        | Prop::ParticleBoxSize
        | Prop::ParticleGravity
        | Prop::ParticleDrag
        | Prop::ParticleSpin
        | Prop::ParticleSeed
        | Prop::ParticleSizeCurve
        | Prop::ParticleOpacityCurve
        | Prop::ParticleSpeedCurve
        | Prop::ParticleColorGradient => particle_value(world.get::<ParticleSystem>(entity)?, prop)?,
        Prop::SmokeSize
        | Prop::SmokeCell
        | Prop::SmokeDensity
        | Prop::SmokeColor
        | Prop::SmokeSpread
        | Prop::SmokeLasts
        | Prop::SmokeFade
        | Prop::SmokeSeed
        | Prop::SmokePlayOnStart => smoke_value(world.get::<voxear_render::Smoke>(entity)?, prop)?,
        Prop::MistEmitting
        | Prop::MistWidth
        | Prop::MistDensity
        | Prop::MistColor
        | Prop::MistLinger
        | Prop::MistSeed => mist_value(world.get::<voxear_render::Mist>(entity)?, prop)?,
        Prop::NetPriority => Value::Number(world.get::<voxear_engine::net::NetObject>(entity)?.priority),
        Prop::NetRange => Value::Number(world.get::<voxear_engine::net::NetObject>(entity)?.range),
        Prop::AnimatorClip => {
            Value::Asset(clip_label(game, &world.get::<voxear_engine::animation::Animator>(entity)?.clip))
        }
        Prop::AnimatorSpeed => Value::Number(world.get::<voxear_engine::animation::Animator>(entity)?.speed),
        Prop::AnimatorLooping => Value::Toggle(world.get::<voxear_engine::animation::Animator>(entity)?.looping),
        Prop::AnimatorBlend => Value::Number(world.get::<voxear_engine::animation::Animator>(entity)?.blend),
        Prop::AnimatorRootMotion => Value::Toggle(world.get::<voxear_engine::animation::Animator>(entity)?.root_motion),
        Prop::AttachBone => {
            // The parent's model's bones, none first.
            let attachment = world.get::<voxear_engine::animation::BoneAttachment>(entity)?;
            let parent = world.get::<voxear_scene::Parent>(entity).map(|parent| parent.0);
            let model = parent
                .and_then(|p| world.get::<voxear_render::ModelRenderer>(p))
                .and_then(|m| m.animation.clone());
            let mut names = vec!["None".to_string()];
            names.extend(model.iter().flat_map(|a| a.skeleton.bones.iter().map(|bone| bone.name.clone())));
            let index = names.iter().skip(1).position(|name| *name == attachment.bone).map_or(0, |i| i + 1);
            Value::Choice(index, names)
        }
        Prop::AttachOffset => Value::Vector(world.get::<BoneAttachment>(entity)?.offset.to_vec()),
        Prop::AttachTurn => Value::Toggle(world.get::<voxear_engine::animation::BoneAttachment>(entity)?.turn),
        Prop::AttachHide => Value::Toggle(world.get::<voxear_engine::animation::BoneAttachment>(entity)?.hide),
        Prop::ViewMouseLook => Value::Toggle(world.get::<FirstPersonView>(entity)?.mouse_look),
        Prop::ViewSensitivity => Value::Number(world.get::<FirstPersonView>(entity)?.sensitivity),
        Prop::ViewPitchLimits => Value::Vector(world.get::<FirstPersonView>(entity)?.pitch_limits.to_vec()),
        Prop::ViewBob => Value::Number(world.get::<FirstPersonView>(entity)?.bob),
        Prop::ViewEyes => Value::Vector(world.get::<FirstPersonView>(entity)?.eyes.to_vec()),
        Prop::ViewTurnObj => Value::Toggle(world.get::<FirstPersonView>(entity)?.turn_obj),
        Prop::ViewLean => Value::Number(world.get::<FirstPersonView>(entity)?.lean),
        Prop::ViewTwist => Value::Number(world.get::<FirstPersonView>(entity)?.twist),
        Prop::ViewHideHead => Value::Toggle(world.get::<FirstPersonView>(entity)?.hide_head),
        Prop::GunKind => {
            let kind = world.get::<Gun>(entity)?.kind;
            let at = GunKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
            Value::Choice(at, GunKind::ALL.iter().map(|k| k.title().to_string()).collect())
        }
        Prop::GunMass => Value::Number(world.get::<Gun>(entity)?.mass),
        Prop::GunLength => Value::Number(world.get::<Gun>(entity)?.length),
        Prop::GunFireRate => Value::Number(world.get::<Gun>(entity)?.fire_rate),
        Prop::GunRecoil => Value::Number(world.get::<Gun>(entity)?.recoil),
        Prop::GunGripAt => Value::Vector(world.get::<Gun>(entity)?.grip.position.to_vec()),
        Prop::GunGripTurn => Value::Vector(world.get::<Gun>(entity)?.grip.rotation.to_vec()),
        Prop::GunSupportAt => Value::Vector(world.get::<Gun>(entity)?.support.position.to_vec()),
        Prop::GunSupportTurn => Value::Vector(world.get::<Gun>(entity)?.support.rotation.to_vec()),
        Prop::GunRearSight => Value::Vector(world.get::<Gun>(entity)?.rear_sight.to_vec()),
        Prop::GunFrontSight => Value::Vector(world.get::<Gun>(entity)?.front_sight.to_vec()),
        Prop::GunEyeRelief => Value::Number(world.get::<Gun>(entity)?.eye_relief),
        Prop::GunAimFov => Value::Number(world.get::<Gun>(entity)?.aim_fov),
        Prop::GunMuzzle => Value::Vector(world.get::<Gun>(entity)?.muzzle.to_vec()),
        Prop::GunButt => Value::Vector(world.get::<Gun>(entity)?.butt.to_vec()),
        Prop::HolderHip => Value::Vector(world.get::<GunHolder>(entity)?.hip.to_vec()),
        Prop::HolderHipTurn => Value::Vector(world.get::<GunHolder>(entity)?.hip_turn.to_vec()),
        Prop::HolderShoulder => Value::Vector(world.get::<GunHolder>(entity)?.shoulder.to_vec()),
        Prop::HolderAimTime => Value::Number(world.get::<GunHolder>(entity)?.aim_time),
        Prop::HolderSway => Value::Number(world.get::<GunHolder>(entity)?.sway),
        Prop::HolderBob => Value::Number(world.get::<GunHolder>(entity)?.bob),
        Prop::HolderBreathing => Value::Number(world.get::<GunHolder>(entity)?.breathing),
        Prop::HolderWallCheck => Value::Toggle(world.get::<GunHolder>(entity)?.wall_check),
        Prop::PartRole => {
            let role = world.get::<GunPart>(entity)?.role;
            let at = PART_ROLES.iter().position(|(r, _)| *r == role).unwrap_or(0);
            Value::Choice(at, PART_ROLES.iter().map(|(_, name)| name.to_string()).collect())
        }
        Prop::PartMotion => {
            let turns = matches!(world.get::<GunPart>(entity)?.motion, PartMotion::Turn { .. });
            Value::Choice(usize::from(turns), vec!["Slide".into(), "Turn".into()])
        }
        Prop::PartAxis => Value::Vector(match world.get::<GunPart>(entity)?.motion {
            PartMotion::Slide { axis, .. } | PartMotion::Turn { axis, .. } => axis.to_vec(),
        }),
        Prop::PartAmount => Value::Number(match world.get::<GunPart>(entity)?.motion {
            PartMotion::Slide { travel, .. } => travel,
            PartMotion::Turn { angle, .. } => angle,
        }),
        Prop::PartPivot => Value::Vector(match world.get::<GunPart>(entity)?.motion {
            PartMotion::Turn { pivot, .. } => pivot.to_vec(),
            PartMotion::Slide { .. } => vec![0.0; 3],
        }),
        Prop::PartHandle => Value::Vector(world.get::<GunPart>(entity)?.handle.to_vec()),
        Prop::PartThenPull => Value::Number(match world.get::<GunPart>(entity)?.then {
            Some(PartMotion::Slide { travel, .. }) => travel,
            _ => 0.0,
        }),
        Prop::MatcherMotionSet => {
            let set = &world.get::<voxear_engine::animation::MotionMatcher>(entity)?.motion_set;
            let guid = game.content.parse(set).ok().and_then(|reference| reference.guid());
            Value::Asset(guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |e| e.path.clone()))
        }
        Prop::MatcherIdleSpeed
        | Prop::MatcherSearchEvery
        | Prop::MatcherBlend
        | Prop::MatcherTurnRate
        | Prop::MatcherTurnToMovement
            | Prop::MatcherLegTurn
        | Prop::MatcherMatchSpeed
        | Prop::MatcherRootMotion
        | Prop::MatcherFootLocking
        | Prop::MatcherDebug => {
            matcher_value(world.get::<voxear_engine::animation::MotionMatcher>(entity)?, prop)?
        }
        Prop::OceanWeather => {
            let weather = world.get::<Ocean>(entity)?.weather;
            let index = Weather::ALL.iter().position(|w| *w == weather).unwrap_or(0);
            Value::Choice(index, Weather::ALL.iter().map(|w| w.name().to_string()).collect())
        }
        Prop::OceanWindSource => {
            let source = world.get::<Ocean>(entity)?.wind_source;
            let index = WindSource::ALL.iter().position(|w| *w == source).unwrap_or(0);
            Value::Choice(index, WindSource::ALL.iter().map(|w| w.name().to_string()).collect())
        }
        Prop::OceanTransition => Value::Number(world.get::<Ocean>(entity)?.transition),
        Prop::OceanSwellWind => Value::Number(world.get::<Ocean>(entity)?.swell_wind),
        Prop::OceanSwellFetch => Value::Number(world.get::<Ocean>(entity)?.swell_fetch),
        Prop::OceanWind => Value::Number(world.get::<Ocean>(entity)?.wind),
        Prop::OceanFetch => Value::Number(world.get::<Ocean>(entity)?.fetch),
        Prop::OceanChoppiness => Value::Number(world.get::<Ocean>(entity)?.choppiness),
        Prop::OceanWaveScale => Value::Number(world.get::<Ocean>(entity)?.wave_scale),
        Prop::OceanDeep => Value::Color(world.get::<Ocean>(entity)?.deep.map(round3)),
        Prop::OceanScatter => Value::Color(world.get::<Ocean>(entity)?.scatter.map(round3)),
        Prop::OceanFoam => Value::Number(world.get::<Ocean>(entity)?.foam),
        Prop::OceanSpray => Value::Number(world.get::<Ocean>(entity)?.spray),
        Prop::OceanClarity => Value::Color(world.get::<Ocean>(entity)?.clarity.map(round3)),
        Prop::OceanMurk => Value::Number(world.get::<Ocean>(entity)?.murk),
        Prop::OceanShores => Value::Toggle(world.get::<Ocean>(entity)?.shores),
        Prop::OceanFoamTrails => Value::Toggle(world.get::<Ocean>(entity)?.foam_trails),
        Prop::OceanFoam3d => Value::Toggle(world.get::<Ocean>(entity)?.foam_3d),
        Prop::OceanLightBending => Value::Toggle(world.get::<Ocean>(entity)?.light_bending),
        Prop::OceanLightShafts => Value::Toggle(world.get::<Ocean>(entity)?.light_shafts),
        Prop::OceanWetLens => Value::Toggle(world.get::<Ocean>(entity)?.wet_lens),
        Prop::OceanFineRipples => Value::Toggle(world.get::<Ocean>(entity)?.fine_ripples),
        Prop::PoolSize => Value::Vector(world.get::<Pool>(entity)?.size.to_vec()),
        Prop::PoolDepth => Value::Number(world.get::<Pool>(entity)?.depth),
        Prop::PoolLiquid => {
            let liquid = world.get::<Pool>(entity)?.liquid;
            let index = Liquid::ALL.iter().position(|l| *l == liquid).unwrap_or(0);
            Value::Choice(index, Liquid::ALL.iter().map(|l| l.name().to_string()).collect())
        }
        Prop::PoolWaves => Value::Number(world.get::<Pool>(entity)?.waves),
        Prop::PoolDeep => Value::Color(world.get::<Pool>(entity)?.deep.map(round3)),
        Prop::PoolScatter => Value::Color(world.get::<Pool>(entity)?.scatter.map(round3)),
        Prop::PoolClarity => Value::Color(world.get::<Pool>(entity)?.clarity.map(round3)),
        Prop::PoolMurk => Value::Number(world.get::<Pool>(entity)?.murk),
        Prop::PoolCooling => Value::Number(world.get::<Pool>(entity)?.cooling),
        Prop::PoolGlow => Value::Number(world.get::<Pool>(entity)?.glow),
        Prop::PathPoints => Value::Fixed(world.get::<voxear_scene::Path>(entity)?.points.len().to_string()),
        Prop::PathClosed => Value::Toggle(world.get::<voxear_scene::Path>(entity)?.closed),
        Prop::StreamLiquid => {
            let liquid = world.get::<Stream>(entity)?.liquid;
            let index = Liquid::ALL.iter().position(|l| *l == liquid).unwrap_or(0);
            Value::Choice(index, Liquid::ALL.iter().map(|l| l.name().to_string()).collect())
        }
        Prop::StreamDepth => Value::Number(world.get::<Stream>(entity)?.depth),
        Prop::StreamSpeed => Value::Number(world.get::<Stream>(entity)?.speed),
        Prop::StreamRapids => Value::Number(world.get::<Stream>(entity)?.rapids),
        Prop::StreamDeep => Value::Color(world.get::<Stream>(entity)?.deep.map(round3)),
        Prop::StreamScatter => Value::Color(world.get::<Stream>(entity)?.scatter.map(round3)),
        Prop::StreamClarity => Value::Color(world.get::<Stream>(entity)?.clarity.map(round3)),
        Prop::StreamMurk => Value::Number(world.get::<Stream>(entity)?.murk),
        Prop::StreamCooling => Value::Number(world.get::<Stream>(entity)?.cooling),
        Prop::StreamGlow => Value::Number(world.get::<Stream>(entity)?.glow),
        Prop::BuoyancySubmerged => Value::Number(world.get::<voxear_engine::water::Buoyancy>(entity)?.submerged),
        Prop::BuoyancyDrag => Value::Number(world.get::<voxear_engine::water::Buoyancy>(entity)?.drag),
        Prop::BuoyancyAngularDrag => Value::Number(world.get::<voxear_engine::water::Buoyancy>(entity)?.angular_drag),
        Prop::DryShape => {
            let shape = world.get::<DryArea>(entity)?.shape;
            let index = DryShape::ALL.iter().position(|s| *s == shape).unwrap_or(0);
            Value::Choice(index, DryShape::ALL.iter().map(|s| s.name().to_string()).collect())
        }
        Prop::DrySize => Value::Vector(world.get::<DryArea>(entity)?.size.to_vec()),
        Prop::DryWidth => Value::Number(world.get::<DryArea>(entity)?.size[0]),
        Prop::DryHeight => Value::Number(world.get::<DryArea>(entity)?.size[1]),
        Prop::DryBilge => Value::Number(world.get::<DryArea>(entity)?.bilge),
        Prop::DryMesh => Value::Asset(match world.get::<DryArea>(entity)?.mesh {
            None => OWN_MESH.into(),
            Some(mesh) => {
                let reference = game.content.mesh_ref(mesh)?.to_string();
                match BUILTIN_MESHES.iter().find(|(_, r)| *r == reference) {
                    Some((label, _)) => label.to_string(),
                    None => mesh_label(game, &reference),
                }
            }
        }),
        Prop::TimeOfYear => Value::Number(round3(world.get::<Seasons>(entity)?.time_of_year)),
        Prop::Season => {
            let season = world.get::<Seasons>(entity)?.season();
            Value::Choice(season as usize, Season::ALL.iter().map(|s| s.name().to_string()).collect())
        }
        Prop::SeasonColor(season) => Value::Color(world.get::<Seasons>(entity)?.look(*season).color.map(round3)),
        Prop::SeasonColorAmount(season) => Value::Number(world.get::<Seasons>(entity)?.look(*season).color_amount),
        Prop::SeasonSnow(season) => Value::Number(world.get::<Seasons>(entity)?.look(*season).snow),
        Prop::SeasonVariation => Value::Number(world.get::<Seasons>(entity)?.variation),
        Prop::SeasonPatchSize => Value::Number(world.get::<Seasons>(entity)?.patch_size),
        Prop::Dryness => Value::Number(world.get::<Seasons>(entity)?.dryness),
        Prop::DryColor => Value::Color(world.get::<Seasons>(entity)?.dry_color.map(round3)),
        Prop::Wetness => Value::Number(world.get::<Seasons>(entity)?.wetness),
        Prop::GroundCover => Value::Number(world.get::<Seasons>(entity)?.ground_cover),
        Prop::PushRadius => Value::Number(world.get::<FoliageInteraction>(entity)?.radius),
        Prop::PushStrength => Value::Number(world.get::<FoliageInteraction>(entity)?.strength),
        Prop::FoliageDraw => Value::Number(world.get::<Foliage>(entity)?.draw_distance),
        Prop::FoliageHealthy => Value::Color(world.get::<Foliage>(entity)?.healthy_color.map(round3)),
        Prop::FoliageDry => Value::Color(world.get::<Foliage>(entity)?.dry_color.map(round3)),
        Prop::FoliageSpread => Value::Number(world.get::<Foliage>(entity)?.color_spread),
        Prop::FoliageFade => Value::Number(world.get::<Foliage>(entity)?.fade_distance),
        Prop::FoliageShadow => Value::Number(world.get::<Foliage>(entity)?.shadow_distance),
        Prop::FoliageImpostors => {
            let impostors = world.get::<Foliage>(entity)?.impostors;
            let index = Impostors::ALL.iter().position(|i| *i == impostors).unwrap_or(0);
            Value::Choice(index, Impostors::ALL.iter().map(|i| i.name().to_string()).collect())
        }
        Prop::FoliageImpostorDistance => Value::Number(world.get::<Foliage>(entity)?.impostor_distance),
        Prop::FieldOfView => Value::Number(camera()?.fov_y_degrees),
        Prop::Near => Value::Number(camera()?.near),
        Prop::Exposure => Value::Number(camera()?.exposure),
        Prop::Sound => Value::Asset(match audio()?.sound.and_then(|id| game.content.sound_guid(id)) {
            Some(guid) => game.content.db().get(guid).map_or_else(|| guid.to_string(), |e| e.path.clone()),
            None => "None".into(),
        }),
        Prop::SoundVolume => Value::Number(audio()?.volume),
        Prop::Pitch => Value::Number(audio()?.pitch),
        Prop::Loop => Value::Toggle(audio()?.looping),
        Prop::PlayOnStart => Value::Toggle(audio()?.play_on_start),
        Prop::SoundSpace => {
            let index = usize::from(audio()?.space == SoundSpace::TwoD);
            Value::Choice(index, SOUND_SPACES.iter().map(|k| k.to_string()).collect())
        }
        Prop::MinDistance => Value::Number(audio()?.min_distance),
        Prop::MaxDistance => Value::Number(audio()?.max_distance),
        Prop::DopplerLevel => Value::Number(audio()?.doppler),
        Prop::Occlusion => {
            let occlusion = audio()?.occlusion;
            let index = SoundOcclusion::ALL.iter().position(|o| *o == occlusion).unwrap_or_default();
            Value::Choice(index, SoundOcclusion::ALL.iter().map(|o| o.label().to_string()).collect())
        }
        Prop::OcclusionRadius => Value::Number(audio()?.occlusion_radius),
        Prop::Transmission => Value::Toggle(audio()?.transmission),
        Prop::Pathing => Value::Toggle(audio()?.pathing),
        Prop::SoundBus => {
            let index = usize::from(audio()?.bus == AudioBus::Music);
            Value::Choice(index, AUDIO_BUSES.iter().map(|k| k.to_string()).collect())
        }
        Prop::LodMeasure => {
            let index = usize::from(lod()?.measure == LodMeasure::Distance);
            Value::Choice(index, LOD_MEASURES.iter().map(|k| k.to_string()).collect())
        }
        Prop::LodCull => Value::Toggle(lod()?.cull),
        Prop::LodObj(level, index) => {
            let obj = *lod()?.levels.get(*level)?.objs.get(*index)?;
            Value::Obj(world.get::<Name>(obj).map_or_else(|| "None".into(), |n| n.0.clone()))
        }
        Prop::LodAdd(level) => {
            lod()?.levels.get(*level)?;
            Value::Obj("None".into())
        }
        // Screen sizes show as percentages, and distances in metres.
        Prop::LodUntil(level) => {
            let group = lod()?;
            let until = group.levels.get(*level)?.until;
            Value::Number(match group.measure {
                LodMeasure::ScreenSize => (until * 1e4).round() / 1e2,
                LodMeasure::Distance => (until * 1e2).round() / 1e2,
            })
        }
        Prop::Field { behaviour, path } => {
            specs(world, entity).iter().find(|s| s.type_name == *behaviour)?;
            fields::get(game, types, entity, behaviour, path)
        }
    })
}

/// Sets a prop. Returns whether anything changed.
pub fn set(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    types: &ScriptTypes,
    entity: Entity,
    prop: &Prop,
    value: Value,
) -> bool {
    if get(game, types, entity, prop).as_ref() == Some(&value) {
        return false;
    }
    let mut renderer = renderer;
    let changed = match (prop, value) {
        // A mesh, material, sound or level's Obj kept after play comes back by its name, as its list has it.
        (Prop::Mesh | Prop::DryMesh | Prop::Material | Prop::Sound | Prop::LightCookie | Prop::LightProfile, Value::Asset(label))
        | (Prop::LodObj(..) | Prop::LodAdd(_) | Prop::JointConnected, Value::Obj(label)) => {
            let choice = choices(game, types, entity, prop).into_iter().find(|(l, _)| *l == label);
            choice.is_some_and(|(_, choice)| pick(game, renderer.as_deref_mut(), types, entity, prop, &choice))
        }
        (Prop::Field { behaviour, path }, value) => {
            fields::set(&mut game.engine.world, types, entity, behaviour, path, &value)
        }
        (prop, value) => set_component(&mut game.engine.world, entity, prop, value),
    };
    if changed && matches!(prop, Prop::DryShape | Prop::DryMesh | Prop::Mesh) {
        dry_bounds(game, renderer.as_deref(), entity);
    }
    changed
}

/// Keeps a mesh Dry Area's box, which scripts asking about water take as its
/// inside, round the mesh it now has: its own, or its Mesh Renderer's.
fn dry_bounds(game: &mut Game, renderer: Option<&Renderer>, entity: Entity) {
    let world = &game.engine.world;
    let Some(dry) = world.get::<DryArea>(entity) else { return };
    let mesh = dry.mesh.or(world.get::<MeshRenderer>(entity).map(|drawn| drawn.mesh));
    let bounds = mesh.zip(renderer).and_then(|(mesh, renderer)| renderer.mesh_bounds(mesh)).map(|(low, high)| [low, high]);
    if let Some(dry) = game.engine.world.get_mut::<DryArea>(entity) {
        dry.bounds = bounds;
    }
}

/// Takes a script field back to what the script starts it as.
pub fn reset_field(world: &mut World, entity: Entity, prop: &Prop) -> bool {
    let Prop::Field { behaviour, path } = prop else { return false };
    fields::reset(world, entity, behaviour, path)
}

/// Points a reference at what was picked for it. A mesh or material is
/// loaded for the renderer.
pub fn pick(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    types: &ScriptTypes,
    entity: Entity,
    prop: &Prop,
    choice: &Pick,
) -> bool {
    match (prop, choice) {
        (Prop::Mesh, Pick::Asset(Some(reference))) => {
            let Some(renderer) = renderer else { return false };
            // A model picked whole, not loaded when the list was made, gives its first mesh.
            let reference = match first_mesh(game, renderer, reference) {
                Some(part) => part,
                None => reference.clone(),
            };
            let Ok(mesh) = game.content.parse(&reference).and_then(|r| game.content.mesh(renderer, &r)) else {
                return false;
            };
            with(&mut game.engine.world, entity, |r: &mut MeshRenderer| r.mesh = mesh)
        }
        (Prop::DryMesh, Pick::Asset(None)) => with(&mut game.engine.world, entity, |d: &mut DryArea| d.mesh = None),
        (Prop::DryMesh, Pick::Asset(Some(reference))) => {
            let Some(renderer) = renderer else { return false };
            let reference = match first_mesh(game, renderer, reference) {
                Some(part) => part,
                None => reference.clone(),
            };
            let Ok(mesh) = game.content.parse(&reference).and_then(|r| game.content.mesh(renderer, &r)) else {
                return false;
            };
            with(&mut game.engine.world, entity, |d: &mut DryArea| d.mesh = Some(mesh))
        }
        (Prop::ModelAsset, Pick::Asset(Some(path))) => {
            let Some(renderer) = renderer else { return false };
            let Some(source) = game.engine.world.get::<ModelSource>(entity).cloned() else { return false };
            // Another model's nodes are other parts, so what this Obj changed of the last one goes.
            let data = ModelData { model: path.clone(), parts: Vec::new(), ..source.0 };
            match voxear_engine::prefab::model_renderer(&mut game.content, renderer, &data) {
                Ok(drawn) => {
                    let world = &mut game.engine.world;
                    let lod_bias = world.get::<ModelRenderer>(entity).map_or(0.0, |r| r.lod_bias);
                    world.insert(entity, ModelRenderer { lod_bias, ..drawn });
                    world.insert(entity, ModelSource(data));
                    true
                }
                Err(error) => {
                    tracing::error!("{error:#}");
                    false
                }
            }
        }
        (Prop::Material, Pick::Asset(path)) => {
            let Some(renderer) = renderer else { return false };
            let source = match path {
                None => MaterialSource::Inline(default_material()),
                Some(path) => match game.content.parse(path) {
                    Ok(reference) => MaterialSource::Asset(reference),
                    Err(_) => return false,
                },
            };
            let Ok(material) = game.content.material(renderer, &source) else { return false };
            with(&mut game.engine.world, entity, |r: &mut MeshRenderer| r.material = material)
        }
        (Prop::LightCookie | Prop::LightProfile, Pick::Asset(path)) => {
            let Some(renderer) = renderer else { return false };
            let texture = match path {
                None => None,
                Some(path) => {
                    let loaded = game.content.parse(path).and_then(|reference| match prop {
                        Prop::LightCookie => game.content.cookie(renderer, &reference),
                        _ => game.content.light_profile(renderer, &reference),
                    });
                    match loaded {
                        Ok(texture) => Some(texture),
                        Err(error) => {
                            tracing::error!("{error:#}");
                            return false;
                        }
                    }
                }
            };
            let cookie = *prop == Prop::LightCookie;
            with(&mut game.engine.world, entity, |light: &mut Light| match cookie {
                true => light.cookie = texture,
                false => light.profile = texture,
            })
        }
        (Prop::MatcherMotionSet, Pick::Asset(choice)) => {
            let set = match choice {
                None => String::new(),
                Some(path) => match game.content.parse(path) {
                    Ok(reference) => reference.to_string(),
                    Err(_) => return false,
                },
            };
            let put = |matcher: &mut voxear_engine::animation::MotionMatcher| matcher.motion_set = set;
            with(&mut game.engine.world, entity, put)
        }
        (Prop::AnimatorClip, Pick::Asset(choice)) => {
            let clip = match choice {
                None => String::new(),
                Some(choice) => match clip_reference(game, choice) {
                    Some(reference) => reference,
                    None => return false,
                },
            };
            let set = |animator: &mut voxear_engine::animation::Animator| animator.clip = clip;
            with(&mut game.engine.world, entity, set)
        }
        (Prop::Sound, Pick::Asset(path)) => {
            let sound = match path {
                None => None,
                Some(path) => match game.content.parse(path).and_then(|r| game.content.sound(&r)) {
                    Ok(sound) => Some(sound),
                    Err(error) => {
                        tracing::error!("{error:#}");
                        return false;
                    }
                },
            };
            with(&mut game.engine.world, entity, |source: &mut AudioSource| source.sound = sound)
        }
        // None takes an Obj out of its level; another takes its place, leaving any level it was in.
        (Prop::LodObj(level, index), Pick::Obj(obj)) => with(&mut game.engine.world, entity, |group: &mut LodGroup| {
            let Some(was) = group.levels.get(*level).and_then(|at| at.objs.get(*index)).copied() else { return };
            group.levels[*level].objs.retain(|&o| o != was);
            if let Some(obj) = obj {
                group.put(*level, &[*obj]);
            }
        }),
        (Prop::LodAdd(level), Pick::Obj(Some(obj))) => {
            with(&mut game.engine.world, entity, |group: &mut LodGroup| group.put(*level, &[*obj]))
        }
        // None holds it to the world.
        (Prop::JointConnected, Pick::Obj(obj)) => {
            let obj = obj.filter(|&obj| obj != entity);
            with(&mut game.engine.world, entity, |joint: &mut Joint| joint.connected = obj)
        }
        (Prop::Field { behaviour, path }, choice) => {
            fields::pick(&mut game.engine.world, types, entity, behaviour, path, choice)
        }
        _ => false,
    }
}

/// The Obj a reference that is not a script field points at, as a LOD
/// Group's level's.
pub fn obj_target(world: &World, entity: Entity, prop: &Prop) -> Option<Entity> {
    match prop {
        Prop::LodObj(level, index) => world.get::<LodGroup>(entity)?.levels.get(*level)?.objs.get(*index).copied(),
        Prop::JointConnected => world.get::<Joint>(entity)?.connected,
        _ => None,
    }
}

/// The Objs directly inside an Obj, in the order the Hierarchy shows them.
pub fn children(game: &Game, entity: Entity) -> Vec<Entity> {
    let world = &game.engine.world;
    let mut inside: Vec<Entity> = world
        .query_ref::<(Entity, &Parent)>()
        .into_iter()
        .filter(|(_, parent)| parent.0 == entity)
        .map(|(child, _)| child)
        .collect();
    let order: Vec<Entity> = game.levels.scene_order().into_iter().flat_map(|(.., objs)| objs).collect();
    inside.sort_by_key(|child| (order.iter().position(|e| e == child).unwrap_or(usize::MAX), child.index()));
    inside
}

/// The levels Add Component gives a LOD Group, from the Objs directly
/// inside its Obj, in the Hierarchy's order: those named as a model's LODs
/// are (`Rock_LOD0`, `Rock_LOD1`) in the level of their number, and the rest
/// share a level with those of the same name, as a model's parts often do,
/// a level for each name.
pub fn lod_levels_inside(game: &Game, entity: Entity) -> Vec<Vec<Entity>> {
    let world = &game.engine.world;
    let name = |obj: Entity| world.get::<Name>(obj).map_or_else(String::new, |n| n.0.clone());
    let inside = children(game, entity);
    let numbered: Vec<(Entity, u32)> =
        inside.iter().filter_map(|&obj| voxear_render::lod_of(&name(obj)).map(|(_, level)| (obj, level))).collect();
    if !numbered.is_empty() {
        let deepest = numbered.iter().map(|&(_, level)| level as usize).max().unwrap_or(0);
        let mut levels = vec![Vec::new(); deepest + 1];
        for (obj, level) in numbered {
            levels[level as usize].push(obj);
        }
        levels.retain(|level| !level.is_empty());
        return levels;
    }
    let mut names: Vec<(String, Vec<Entity>)> = Vec::new();
    for obj in inside {
        let named = name(obj);
        match names.iter_mut().find(|(n, _)| *n == named) {
            Some((_, objs)) => objs.push(obj),
            None => names.push((named, vec![obj])),
        }
    }
    names.into_iter().map(|(_, objs)| objs).collect()
}

/// Puts Objs in a LOD Group's level, as dropping them on it does, each
/// leaving any other level it was in. Only Objs inside the group's Obj.
pub fn put_in_lod_level(game: &mut Game, entity: Entity, level: usize, objs: &[Entity]) -> bool {
    let inside = descendants(game, entity);
    let objs: Vec<Entity> = objs.iter().copied().filter(|obj| inside.contains(obj)).collect();
    !objs.is_empty() && with(&mut game.engine.world, entity, |group: &mut LodGroup| group.put(level, &objs))
}

/// Every Obj inside an Obj, at any depth, each before what is inside it,
/// in the order the Hierarchy shows them.
pub fn descendants(game: &Game, entity: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    for child in children(game, entity) {
        found.push(child);
        found.extend(descendants(game, child));
    }
    found
}

/// Puts a new level into a LOD Group before the level at `at`, or after the
/// last, showing the first Obj inside the group's Obj that no level shows
/// yet. Before a level, it takes the nearer half of that level's part of
/// the bar, as Unity's Insert Before does.
pub fn insert_lod_level(game: &mut Game, entity: Entity, at: usize) -> bool {
    let inside = children(game, entity);
    with(&mut game.engine.world, entity, |group: &mut LodGroup| {
        let obj = inside.into_iter().find(|child| group.levels.iter().all(|level| !level.objs.contains(child)));
        let at = at.min(group.levels.len());
        let until = match (group.levels.get(at), group.measure) {
            (None, _) => group.next_until(),
            (Some(level), LodMeasure::ScreenSize) => {
                let above = at.checked_sub(1).map_or(1.0, |before| group.levels[before].until);
                (above + level.until) * 0.5
            }
            (Some(level), LodMeasure::Distance) => {
                let nearer = at.checked_sub(1).map_or(0.0, |before| group.levels[before].until);
                (nearer + level.until) * 0.5
            }
        };
        group.levels.insert(at, LodLevel { objs: obj.into_iter().collect(), until });
    })
}

/// Says where each LOD of a Mesh Renderer's mesh ends, as a share of the
/// screen's height, or lets the engine switch them again.
pub fn set_lod_transitions(world: &mut World, entity: Entity, ends: Option<[f32; MAX_TRANSITIONS]>) -> bool {
    with(world, entity, |renderer: &mut MeshRenderer| renderer.lod_transitions = ends)
}

/// Makes a Mesh Renderer draw one LOD of its mesh at every distance, or
/// leaves the LOD to the distance again.
pub fn set_forced_lod(world: &mut World, entity: Entity, lod: Option<u8>) -> bool {
    with(world, entity, |renderer: &mut MeshRenderer| renderer.forced_lod = lod)
}

/// Takes the level at `at` out of a LOD Group.
pub fn delete_lod_level(world: &mut World, entity: Entity, at: usize) -> bool {
    with(world, entity, |group: &mut LodGroup| {
        if at < group.levels.len() {
            group.levels.remove(at);
        }
    })
}

/// Moves where a LOD Group's level ends, in the group's own terms: a share
/// of the screen from 0 to 1, or metres.
pub fn set_lod_until(world: &mut World, entity: Entity, at: usize, until: f32) -> bool {
    with(world, entity, |group: &mut LodGroup| {
        if let Some(level) = group.levels.get_mut(at) {
            level.until = until;
        }
    })
}

/// What a reference can point at, each with what it is called: for a Mesh,
/// the built-in meshes, then each mesh of the project's models, or the
/// model whole where it is not loaded yet, and the one it has; the default
/// and the project's materials for a Material; and for a script field, the
/// Objs or assets of its kind. Nothing comes first where a reference can be
/// empty.
pub fn choices(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> Vec<(String, Pick)> {
    match prop {
        // The Obj's own Mesh Renderer's first, then any mesh.
        Prop::DryMesh => {
            let mut found = vec![(OWN_MESH.to_string(), Pick::Asset(None))];
            found.extend(BUILTIN_MESHES.iter().map(|(label, r)| (label.to_string(), Pick::Asset(Some(r.to_string())))));
            found.extend(model_meshes(game));
            let mesh = game.engine.world.get::<DryArea>(entity).and_then(|d| d.mesh);
            let reference = mesh.and_then(|mesh| game.content.mesh_ref(mesh)).map(|r| r.to_string());
            if let Some(reference) = reference.filter(|r| !found.iter().any(|(_, pick)| matches!(pick, Pick::Asset(Some(p)) if p == r))) {
                found.push((mesh_label(game, &reference), Pick::Asset(Some(reference))));
            }
            return found;
        }
        Prop::Mesh => {
            let mut found: Vec<(String, Pick)> =
                BUILTIN_MESHES.iter().map(|(label, r)| (label.to_string(), Pick::Asset(Some(r.to_string())))).collect();
            found.extend(model_meshes(game));
            let mesh = game.engine.world.get::<MeshRenderer>(entity).map(|r| r.mesh);
            let reference = mesh.and_then(|mesh| game.content.mesh_ref(mesh)).map(|r| r.to_string());
            let listed = |reference: &str| {
                let parsed = game.content.parse(reference).ok();
                found.iter().any(|(_, pick)| matches!(pick, Pick::Asset(Some(r)) if game.content.parse(r).ok() == parsed))
            };
            if let Some(reference) = reference.filter(|r| !listed(r)) {
                found.push((mesh_label(game, &reference), Pick::Asset(Some(reference))));
            }
            return found;
        }
        Prop::Material => {
            let mut found = vec![("Default".to_string(), Pick::Asset(None))];
            found.extend(BUILTIN_MATERIALS.iter().map(|(label, r)| (label.to_string(), Pick::Asset(Some(r.to_string())))));
            found.extend(material_assets(game).into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
            return found;
        }
        Prop::Sound => {
            let kind = voxear_engine::script_types::FieldKind::Asset { asset: "Voxear.Sound".into() };
            return fields::asset_choices(game, &kind);
        }
        Prop::MatcherMotionSet => return fields::assets_of(game, Some(AssetKind::MotionSet)),
        Prop::AnimatorClip => {
            let mut found = vec![("None".to_string(), Pick::Asset(None))];
            found.extend(model_clips(game));
            return found;
        }
        Prop::LightCookie => return fields::assets_of(game, Some(AssetKind::Texture)),
        Prop::LightProfile => return fields::assets_of(game, Some(AssetKind::LightProfile)),
        Prop::ModelAsset => return fields::assets_of(game, Some(AssetKind::Model)),
        // A level shows Objs inside the group's Obj, at any depth: those not in it already, each in another
        // level saying which, as picking it moves it here.
        // A joint holds to an Obj physics moves: every other with a Rigidbody, or the world.
        Prop::JointConnected => {
            let world = &game.engine.world;
            let mut found = vec![("None".to_string(), Pick::Obj(None))];
            let bodies = world.query_ref::<(Entity, &Rigidbody, &Name)>().without::<voxear_scene::Destroying>();
            let mut others: Vec<(String, Pick)> = bodies
                .into_iter()
                .filter(|(other, ..)| *other != entity && !world.has::<voxear_scene::EditorOnly>(*other))
                .map(|(other, _, name)| (name.0.clone(), Pick::Obj(Some(other))))
                .collect();
            others.sort_by(|a, b| a.0.cmp(&b.0));
            found.extend(others);
            return found;
        }
        Prop::LodObj(level, _) | Prop::LodAdd(level) => {
            let world = &game.engine.world;
            let group = world.get::<LodGroup>(entity);
            let mut found = vec![("None".to_string(), Pick::Obj(None))];
            for inside in descendants(game, entity) {
                let name = world.get::<Name>(inside).map_or_else(|| "Obj".into(), |n| n.0.clone());
                let label = match group.and_then(|group| group.level_of(inside)) {
                    Some(at) if at == *level => continue,
                    Some(at) => format!("{name} · LOD {at}"),
                    None => name,
                };
                found.push((label, Pick::Obj(Some(inside))));
            }
            return found;
        }
        _ => {}
    }
    let Prop::Field { behaviour, path } = prop else { return Vec::new() };
    match fields::kind_at(types, behaviour, path) {
        Some(
            kind @ (voxear_engine::script_types::FieldKind::Asset { .. }
            | voxear_engine::script_types::FieldKind::Ref { .. }),
        ) => fields::asset_choices(game, &kind),
        Some(voxear_engine::script_types::FieldKind::Localized) => fields::key_choices(game),
        Some(kind) => fields::obj_choices(game, types, entity, &kind),
        None => fields::obj_choices(game, types, entity, &voxear_engine::script_types::FieldKind::Obj),
    }
}

/// Why a reference cannot take what was dropped on it.
pub fn why_not(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, choice: &Pick) -> String {
    if let Prop::LodObj(..) | Prop::LodAdd(_) = prop {
        return "A LOD Group's levels take Objs inside its Obj".to_string();
    }
    if *prop == Prop::JointConnected {
        return "A joint holds to another Obj with a Rigidbody".to_string();
    }
    let Prop::Field { behaviour, path } = prop else {
        return format!("The {} takes a {} from its list", label(prop), title(types, prop));
    };
    let kind = fields::kind_at(types, behaviour, path).unwrap_or(voxear_engine::script_types::FieldKind::Obj);
    fields::why_not(game, entity, &kind, choice)
}

/// What a reference takes, as its picker's title says: "Material",
/// "Prefab", "Spinner" or "Obj".
pub fn title(types: &ScriptTypes, prop: &Prop) -> String {
    match prop {
        Prop::Mesh | Prop::DryMesh => "Mesh".into(),
        Prop::Material => "Material".into(),
        Prop::Sound => "Sound".into(),
        Prop::LightCookie => "Texture".into(),
        Prop::LightProfile => "IES profile".into(),
        Prop::LodObj(..) | Prop::LodAdd(_) => "Obj".into(),
        Prop::JointConnected => "Rigidbody".into(),
        Prop::Field { behaviour, path } => fields::kind_at(types, behaviour, path)
            .map_or_else(|| "Obj".into(), |kind| fields::takes(&kind).to_string()),
        _ => String::new(),
    }
}

/// Adds an item to a list, or takes one out.
pub fn add_item(world: &mut World, types: &ScriptTypes, entity: Entity, list: &Prop) -> bool {
    let Prop::Field { behaviour, path } = list else { return false };
    fields::add_item(world, types, entity, behaviour, path)
}

pub fn remove_item(world: &mut World, types: &ScriptTypes, entity: Entity, item: &Prop) -> bool {
    let Prop::Field { behaviour, path } = item else { return false };
    fields::remove_item(world, types, entity, behaviour, path)
}

/// Moves a list's item to where another of the same list is, as dropping
/// one onto the other does. False when they are not of one list.
pub fn move_item(world: &mut World, types: &ScriptTypes, entity: Entity, item: &Prop, onto: &Prop) -> bool {
    let (Prop::Field { behaviour, path }, Prop::Field { behaviour: other, path: target }) = (item, onto) else {
        return false;
    };
    let (Some((Step::Item(from), list)), Some((Step::Item(to), same))) = (path.split_last(), target.split_last())
    else {
        return false;
    };
    behaviour == other
        && list == same
        && from != to
        && fields::move_item(world, types, entity, behaviour, list, *from, *to)
}

/// Whether a script's value is inside `outer`, as an item's fields are
/// inside the item.
pub fn within(value: &Prop, outer: &Prop) -> bool {
    let (Prop::Field { behaviour, path }, Prop::Field { behaviour: of, path: outside }) = (value, outer) else {
        return false;
    };
    behaviour == of && path.len() > outside.len() && path.starts_with(outside)
}

/// Whether a prop is an item of a list, which can be taken out.
pub fn is_item(prop: &Prop) -> bool {
    matches!(prop, Prop::Field { path, .. } if matches!(path.last(), Some(Step::Item(_))))
}

/// Lights, cameras and Audio Sources, whose values are plain numbers,
/// switches and choices on the component.
fn set_component(world: &mut World, entity: Entity, prop: &Prop, value: Value) -> bool {
    let three = |values: &[f32]| [0, 1, 2].map(|at| values.get(at).copied().unwrap_or_default());
    let two = |values: &[f32]| [0, 1].map(|at| values.get(at).copied().unwrap_or_default());
    match (prop, value) {
        (Prop::Shape2D, Value::Choice(index, _)) => with(world, entity, |c: &mut Collider2D| {
            c.shape = ColliderShape2D::ALL[index.min(ColliderShape2D::ALL.len() - 1)];
        }),
        (Prop::Offset2D, Value::Vector(v)) => with(world, entity, |c: &mut Collider2D| c.offset = two(&v)),
        (Prop::Size2D, Value::Vector(v)) => {
            with(world, entity, |c: &mut Collider2D| c.size = two(&v).map(|side| side.max(0.001)))
        }
        (Prop::Radius2D, Value::Number(n)) => with(world, entity, |c: &mut Collider2D| c.radius = n.max(0.001)),
        (Prop::Height2D, Value::Number(n)) => with(world, entity, |c: &mut Collider2D| c.height = n.max(0.001)),
        (Prop::Trigger2D, Value::Toggle(on)) => with(world, entity, |c: &mut Collider2D| c.trigger = on),
        (Prop::Friction2D, Value::Number(n)) => with(world, entity, |c: &mut Collider2D| c.friction = n.max(0.0)),
        (Prop::Bounciness2D, Value::Number(n)) => {
            with(world, entity, |c: &mut Collider2D| c.bounciness = n.clamp(0.0, 1.0))
        }
        (Prop::Mass2D, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody2D| b.mass = n.max(0.001)),
        (Prop::Kinematic2D, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody2D| b.kinematic = on),
        (Prop::GravityScale, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody2D| b.gravity_scale = n),
        (Prop::Drag2D, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody2D| b.drag = n.max(0.0)),
        (Prop::AngularDrag2D, Value::Number(n)) => {
            with(world, entity, |b: &mut Rigidbody2D| b.angular_drag = n.max(0.0))
        }
        (Prop::FixedRotation, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody2D| b.fixed_rotation = on),
        (Prop::Continuous2D, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody2D| b.continuous = on),
        (Prop::VehicleKind, Value::Choice(index, _)) => with(world, entity, |v: &mut Vehicle| {
            v.kind = VehicleKind::ALL[index.min(VehicleKind::ALL.len() - 1)];
        }),
        (Prop::MaxTorque, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.max_torque = n.max(0.0)),
        (Prop::MinRpm, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.min_rpm = n.clamp(0.0, v.max_rpm)),
        (Prop::MaxRpm, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.max_rpm = n.max(v.min_rpm)),
        (Prop::Automatic, Value::Toggle(on)) => with(world, entity, |v: &mut Vehicle| v.automatic = on),
        (Prop::Gears, Value::Number(n)) => {
            with(world, entity, |v: &mut Vehicle| v.gears = n.round().clamp(1.0, 10.0) as u32)
        }
        (Prop::FirstGear, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.first_gear = n.max(0.1)),
        (Prop::TopGear, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.top_gear = n.max(0.1)),
        (Prop::AntiRoll, Value::Number(n)) => with(world, entity, |v: &mut Vehicle| v.anti_roll = n.max(0.0)),
        (Prop::WheelRadius, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.radius = n.max(0.01)),
        (Prop::WheelWidth, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.width = n.max(0.01)),
        (Prop::Suspension, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.suspension = n.max(0.01)),
        (Prop::Spring, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.spring = n.max(0.01)),
        (Prop::Damping, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.damping = n.max(0.0)),
        (Prop::SteerAngle, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.steer_angle = n.clamp(0.0, 90.0)),
        (Prop::Powered, Value::Toggle(on)) => with(world, entity, |w: &mut Wheel| w.powered = on),
        (Prop::WheelBrake, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.brake = n.max(0.0)),
        (Prop::Handbrake, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.handbrake = n.max(0.0)),
        (Prop::Grip, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.grip = n.max(0.0)),
        (Prop::SideGrip, Value::Number(n)) => with(world, entity, |w: &mut Wheel| w.side_grip = n.max(0.0)),
        (Prop::ColliderShape, Value::Choice(index, _)) => with(world, entity, |c: &mut Collider| {
            c.shape = ColliderShape::ALL[index.min(ColliderShape::ALL.len() - 1)];
        }),
        (Prop::ColliderCenter, Value::Vector(v)) => with(world, entity, |c: &mut Collider| c.center = three(&v)),
        (Prop::ColliderSize, Value::Vector(v)) => {
            with(world, entity, |c: &mut Collider| c.size = three(&v).map(|side| side.max(0.001)))
        }
        (Prop::ColliderRadius, Value::Number(n)) => with(world, entity, |c: &mut Collider| c.radius = n.max(0.001)),
        (Prop::ColliderHeight, Value::Number(n)) => with(world, entity, |c: &mut Collider| c.height = n.max(0.001)),
        (Prop::ColliderConvex, Value::Toggle(on)) => with(world, entity, |c: &mut Collider| c.convex = on),
        (Prop::ColliderTrigger, Value::Toggle(on)) => with(world, entity, |c: &mut Collider| c.trigger = on),
        (Prop::Friction, Value::Number(n)) => with(world, entity, |c: &mut Collider| c.friction = n.max(0.0)),
        (Prop::Bounciness, Value::Number(n)) => {
            with(world, entity, |c: &mut Collider| c.bounciness = n.clamp(0.0, 1.0))
        }
        (Prop::AcousticMaterial, Value::Choice(index, _)) => with(world, entity, |c: &mut Collider| {
            c.acoustic = Acoustic::ALL[index.min(Acoustic::ALL.len() - 1)];
        }),
        (Prop::Mass, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody| b.mass = n.max(0.001)),
        (Prop::Kinematic, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody| b.kinematic = on),
        (Prop::UseGravity, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody| b.gravity = on),
        (Prop::Drag, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody| b.drag = n.max(0.0)),
        (Prop::AngularDrag, Value::Number(n)) => with(world, entity, |b: &mut Rigidbody| b.angular_drag = n.max(0.0)),
        (Prop::Continuous, Value::Toggle(on)) => with(world, entity, |b: &mut Rigidbody| b.continuous = on),
        (Prop::CharacterHeight, Value::Number(n)) => {
            with(world, entity, |c: &mut CharacterController| c.height = n.max(c.radius * 2.0))
        }
        (Prop::CharacterRadius, Value::Number(n)) => {
            with(world, entity, |c: &mut CharacterController| c.radius = n.clamp(0.01, c.height * 0.5))
        }
        (Prop::SlopeLimit, Value::Number(n)) => {
            with(world, entity, |c: &mut CharacterController| c.slope_limit = n.clamp(0.0, 90.0))
        }
        (Prop::StepOffset, Value::Number(n)) => {
            with(world, entity, |c: &mut CharacterController| c.step_offset = n.max(0.0))
        }
        (Prop::JointKind, Value::Choice(index, _)) => with(world, entity, |joint: &mut Joint| {
            joint.kind = JointType::ALL[index.min(JointType::ALL.len() - 1)];
        }),
        (Prop::JointAnchor, Value::Vector(v)) => with(world, entity, |joint: &mut Joint| joint.anchor = three(&v)),
        (Prop::JointAxis, Value::Vector(v)) => with(world, entity, |joint: &mut Joint| joint.axis = three(&v)),
        // Limits start as a door's quarter turn, or a rope from no length to two metres.
        (Prop::JointLimited, Value::Toggle(on)) => with(world, entity, |joint: &mut Joint| {
            let start = if joint.kind == JointType::Hinge { [0.0, 90.0] } else { [0.0, 2.0] };
            joint.limits = on.then(|| joint.limits.unwrap_or(start));
        }),
        (Prop::JointMin, Value::Number(n)) => with(world, entity, |joint: &mut Joint| {
            if let Some(limits) = &mut joint.limits {
                limits[0] = n;
            }
        }),
        (Prop::JointMax, Value::Number(n)) => with(world, entity, |joint: &mut Joint| {
            if let Some(limits) = &mut joint.limits {
                limits[1] = n;
            }
        }),
        (Prop::LightKind, Value::Choice(index, _)) => with(world, entity, |light: &mut Light| {
            let kind = LIGHT_KIND_ORDER[index.min(LIGHT_KIND_ORDER.len() - 1)];
            // What the old kind did not use starts as a new light of this kind has it.
            let fresh = default_light(kind);
            light.kind = kind;
            if light.range <= 0.0 {
                light.range = fresh.range;
            }
            if light.spot_outer_degrees <= 0.0 {
                (light.spot_inner_degrees, light.spot_outer_degrees) =
                    (fresh.spot_inner_degrees, fresh.spot_outer_degrees);
            }
            if light.width <= 0.0 || light.height <= 0.0 {
                (light.width, light.height) = (fresh.width, fresh.height);
            }
            if light.radius <= 0.0 {
                light.radius = fresh.radius;
            }
            if light.length <= 0.0 {
                light.length = fresh.length;
            }
        }),
        (Prop::LightColor, Value::Color(color)) => {
            with(world, entity, |light: &mut Light| light.color = Vec3::from(linear(color.map(|c| c.clamp(0.0, 1.0)))))
        }
        (Prop::LightAppearance, Value::Choice(index, _)) => with(world, entity, |light: &mut Light| {
            // Daylight to start from, which leaves the colour as it was.
            light.temperature = (index == 1).then(|| light.temperature.unwrap_or(DAYLIGHT_KELVIN));
        }),
        (Prop::LightTemperature, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.temperature = Some(n.clamp(1000.0, 20000.0)))
        }
        (Prop::Intensity, Value::Number(n)) => with(world, entity, |light: &mut Light| light.intensity = n.max(0.0)),
        (Prop::Range, Value::Number(n)) => with(world, entity, |light: &mut Light| light.range = n.max(0.0)),
        (Prop::SpotInner, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.spot_inner_degrees = n.clamp(0.0, light.spot_outer_degrees))
        }
        (Prop::SpotOuter, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.spot_outer_degrees = n.clamp(light.spot_inner_degrees, 179.0))
        }
        (Prop::LightWidth, Value::Number(n)) => with(world, entity, |light: &mut Light| light.width = n.max(0.0)),
        (Prop::LightHeight, Value::Number(n)) => with(world, entity, |light: &mut Light| light.height = n.max(0.0)),
        (Prop::LightRadius, Value::Number(n)) => with(world, entity, |light: &mut Light| light.radius = n.max(0.0)),
        (Prop::LightLength, Value::Number(n)) => with(world, entity, |light: &mut Light| light.length = n.max(0.0)),
        (Prop::LightShadowTint, Value::Color(color)) => with(world, entity, |light: &mut Light| {
            light.shadow_tint = Vec3::from(linear(color.map(|c| c.clamp(0.0, 1.0))));
        }),
        (Prop::LightBody, Value::Choice(index, _)) => with(world, entity, |light: &mut Light| {
            light.body = SKY_BODY_ORDER[index.min(SKY_BODY_ORDER.len() - 1)];
        }),
        (Prop::LightShadows, Value::Toggle(on)) => with(world, entity, |light: &mut Light| light.shadows = on),
        (Prop::LightSoftness, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.softness = n.clamp(0.0, 20.0))
        }
        (Prop::LightCookieSize, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.cookie_size = n.max(0.01))
        }
        (Prop::VolumeShape, Value::Choice(index, _)) => with(world, entity, |volume: &mut PostVolume| {
            volume.shape = VOLUME_SHAPE_ORDER[index.min(VOLUME_SHAPE_ORDER.len() - 1)];
        }),
        (Prop::VolumeBlend, Value::Number(n)) => {
            with(world, entity, |volume: &mut PostVolume| volume.blend_distance = n.max(0.0))
        }
        (Prop::VolumeWeight, Value::Number(n)) => {
            with(world, entity, |volume: &mut PostVolume| volume.weight = n.clamp(0.0, 1.0))
        }
        (Prop::WindStrength, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.strength = n.max(0.0)),
        (Prop::WindGusts, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.gusts = n.max(0.0)),
        (Prop::WindGustSize, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.gust_size = n.max(0.1)),
        (Prop::WindGustSpeed, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.gust_speed = n),
        (Prop::WindShape, Value::Choice(index, _)) => {
            let shape = WindShape::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |wind: &mut WindZone| wind.shape = shape)
        }
        (Prop::WindSize, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |wind: &mut WindZone| wind.size = [v[0], v[1], v[2]].map(|s| s.max(0.01)))
        }
        (Prop::WindWidth, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.size[0] = n.max(0.01)),
        (Prop::WindFalloff, Value::Number(n)) => with(world, entity, |wind: &mut WindZone| wind.falloff = n.max(0.01)),
        (Prop::ModelLodBias, Value::Number(n)) => {
            with(world, entity, |model: &mut ModelRenderer| model.lod_bias = n.clamp(-4.0, 4.0))
        }
        (Prop::FoliageHealthy, Value::Color(color)) => {
            with(world, entity, |f: &mut Foliage| f.healthy_color = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::FoliageDry, Value::Color(color)) => {
            with(world, entity, |f: &mut Foliage| f.dry_color = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::FoliageSpread, Value::Number(n)) => with(world, entity, |f: &mut Foliage| f.color_spread = n.max(0.1)),
        (Prop::FoliageDraw, Value::Number(n)) => with(world, entity, |f: &mut Foliage| f.draw_distance = n.max(0.0)),
        (Prop::FoliageFade, Value::Number(n)) => with(world, entity, |f: &mut Foliage| f.fade_distance = n.max(0.0)),
        (Prop::FoliageImpostors, Value::Choice(index, _)) => {
            let impostors = Impostors::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |f: &mut Foliage| f.impostors = impostors)
        }
        (Prop::FoliageImpostorDistance, Value::Number(n)) => {
            with(world, entity, |f: &mut Foliage| f.impostor_distance = n.max(0.0))
        }
        (Prop::FoliageShadow, Value::Number(n)) => {
            with(world, entity, |f: &mut Foliage| f.shadow_distance = n.max(0.0))
        }
        (Prop::PushRadius, Value::Number(n)) => {
            with(world, entity, |push: &mut FoliageInteraction| push.radius = n.max(0.01))
        }
        (Prop::PushStrength, Value::Number(n)) => {
            with(world, entity, |push: &mut FoliageInteraction| push.strength = n.max(0.0))
        }
        (Prop::WindTurbulence, Value::Number(n)) => {
            with(world, entity, |wind: &mut WindZone| wind.turbulence = n.max(0.0))
        }
        (prop, value) if prop.part() == Part::Smoke => {
            with(world, entity, |smoke: &mut voxear_render::Smoke| set_smoke(smoke, prop, value))
        }
        (Prop::AnimatorSpeed, Value::Number(n)) => {
            with(world, entity, |animator: &mut voxear_engine::animation::Animator| animator.speed = n.clamp(-10.0, 10.0))
        }
        (Prop::AnimatorLooping, Value::Toggle(on)) => with(world, entity, |animator: &mut voxear_engine::animation::Animator| animator.looping = on),
        (Prop::AnimatorBlend, Value::Number(n)) => {
            with(world, entity, |animator: &mut voxear_engine::animation::Animator| animator.blend = n.clamp(0.0, 5.0))
        }
        (Prop::AnimatorRootMotion, Value::Toggle(on)) => {
            with(world, entity, |animator: &mut voxear_engine::animation::Animator| animator.root_motion = on)
        }
        (Prop::AttachBone, Value::Choice(index, _)) => {
            let parent = world.get::<voxear_scene::Parent>(entity).map(|parent| parent.0);
            let model = parent
                .and_then(|p| world.get::<voxear_render::ModelRenderer>(p))
                .and_then(|m| m.animation.clone());
            let name = index.checked_sub(1).and_then(|i| model?.skeleton.bones.get(i).map(|bone| bone.name.clone()));
            with(world, entity, |attachment: &mut BoneAttachment| attachment.bone = name.unwrap_or_default())
        }
        (Prop::AttachOffset, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |attachment: &mut BoneAttachment| attachment.offset = [v[0], v[1], v[2]])
        }
        (Prop::AttachTurn, Value::Toggle(on)) => with(world, entity, |it: &mut BoneAttachment| it.turn = on),
        (Prop::AttachHide, Value::Toggle(on)) => with(world, entity, |it: &mut BoneAttachment| it.hide = on),
        (Prop::ViewMouseLook, Value::Toggle(on)) => with(world, entity, |it: &mut FirstPersonView| it.mouse_look = on),
        (Prop::ViewSensitivity, Value::Number(n)) => {
            with(world, entity, |it: &mut FirstPersonView| it.sensitivity = n.max(0.0))
        }
        (Prop::ViewPitchLimits, Value::Vector(v)) if v.len() == 2 => {
            with(world, entity, |it: &mut FirstPersonView| it.pitch_limits = [v[0].clamp(-90.0, 0.0), v[1].clamp(0.0, 90.0)])
        }
        (Prop::ViewBob, Value::Number(n)) => with(world, entity, |it: &mut FirstPersonView| it.bob = n.clamp(0.0, 1.0)),
        (Prop::ViewEyes, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |it: &mut FirstPersonView| it.eyes = [v[0], v[1], v[2]])
        }
        (Prop::ViewTurnObj, Value::Toggle(on)) => with(world, entity, |it: &mut FirstPersonView| it.turn_obj = on),
        (Prop::ViewLean, Value::Number(n)) => with(world, entity, |it: &mut FirstPersonView| it.lean = n.clamp(0.0, 1.0)),
        (Prop::ViewTwist, Value::Number(n)) => with(world, entity, |it: &mut FirstPersonView| it.twist = n.clamp(0.0, 1.0)),
        (Prop::ViewHideHead, Value::Toggle(on)) => with(world, entity, |it: &mut FirstPersonView| it.hide_head = on),
        // A kind fills in its typical numbers and sockets.
        (Prop::GunKind, Value::Choice(index, _)) => {
            let kind = GunKind::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |gun: &mut Gun| *gun = Gun::preset(kind))
        }
        (Prop::GunMass, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.mass = n.max(0.1)),
        (Prop::GunLength, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.length = n.max(0.05)),
        (Prop::GunFireRate, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.fire_rate = n.max(1.0)),
        (Prop::GunRecoil, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.recoil = n.max(0.0)),
        (Prop::GunEyeRelief, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.eye_relief = n.max(0.0)),
        (Prop::GunAimFov, Value::Number(n)) => with(world, entity, |gun: &mut Gun| gun.aim_fov = n.clamp(0.0, 120.0)),
        (
            Prop::GunGripAt
            | Prop::GunGripTurn
            | Prop::GunSupportAt
            | Prop::GunSupportTurn
            | Prop::GunRearSight
            | Prop::GunFrontSight
            | Prop::GunMuzzle
            | Prop::GunButt,
            Value::Vector(v),
        ) if v.len() == 3 => {
            let v = [v[0], v[1], v[2]];
            with(world, entity, |gun: &mut Gun| match prop {
                Prop::GunGripAt => gun.grip.position = v,
                Prop::GunGripTurn => gun.grip.rotation = v,
                Prop::GunSupportAt => gun.support.position = v,
                Prop::GunSupportTurn => gun.support.rotation = v,
                Prop::GunRearSight => gun.rear_sight = v,
                Prop::GunFrontSight => gun.front_sight = v,
                Prop::GunButt => gun.butt = v,
                _ => gun.muzzle = v,
            })
        }
        (Prop::HolderHip, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |holder: &mut GunHolder| holder.hip = [v[0], v[1], v[2]])
        }
        (Prop::HolderShoulder, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |holder: &mut GunHolder| holder.shoulder = [v[0], v[1], v[2]])
        }
        (Prop::HolderHipTurn, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |holder: &mut GunHolder| holder.hip_turn = [v[0], v[1], v[2]])
        }
        (Prop::HolderAimTime, Value::Number(n)) => with(world, entity, |h: &mut GunHolder| h.aim_time = n.max(0.02)),
        (Prop::HolderSway, Value::Number(n)) => with(world, entity, |h: &mut GunHolder| h.sway = n.max(0.0)),
        (Prop::HolderBob, Value::Number(n)) => with(world, entity, |h: &mut GunHolder| h.bob = n.max(0.0)),
        (Prop::HolderBreathing, Value::Number(n)) => with(world, entity, |h: &mut GunHolder| h.breathing = n.max(0.0)),
        (Prop::HolderWallCheck, Value::Toggle(on)) => with(world, entity, |h: &mut GunHolder| h.wall_check = on),
        // A role moves as most such parts do, from where its handle is.
        (Prop::PartRole, Value::Choice(index, _)) => {
            let role = PART_ROLES.get(index).map_or(PartRole::Bolt, |(role, _)| *role);
            with(world, entity, |part: &mut GunPart| *part = GunPart::of(role, part.handle))
        }
        (Prop::PartMotion, Value::Choice(index, _)) => with(world, entity, |part: &mut GunPart| {
            let axis = match part.motion {
                PartMotion::Slide { axis, .. } | PartMotion::Turn { axis, .. } => axis,
            };
            part.motion = match index {
                0 => PartMotion::Slide { axis, travel: 0.08 },
                _ => PartMotion::Turn { axis, pivot: part.handle, angle: 60.0 },
            };
        }),
        (Prop::PartAxis, Value::Vector(v)) if v.len() == 3 => with(world, entity, |part: &mut GunPart| match &mut part.motion {
            PartMotion::Slide { axis, .. } | PartMotion::Turn { axis, .. } => *axis = [v[0], v[1], v[2]],
        }),
        (Prop::PartAmount, Value::Number(n)) => with(world, entity, |part: &mut GunPart| match &mut part.motion {
            PartMotion::Slide { travel, .. } => *travel = n,
            PartMotion::Turn { angle, .. } => *angle = n,
        }),
        (Prop::PartPivot, Value::Vector(v)) if v.len() == 3 => with(world, entity, |part: &mut GunPart| {
            if let PartMotion::Turn { pivot, .. } = &mut part.motion {
                *pivot = [v[0], v[1], v[2]];
            }
        }),
        (Prop::PartHandle, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |part: &mut GunPart| part.handle = [v[0], v[1], v[2]])
        }
        (Prop::PartThenPull, Value::Number(n)) => with(world, entity, |part: &mut GunPart| {
            part.then = (n > 0.0).then_some(PartMotion::Slide { axis: [0.0, 0.0, -1.0], travel: n });
        }),
        (prop, value) if prop.part() == Part::MotionMatcher => {
            let set = |matcher: &mut voxear_engine::animation::MotionMatcher| set_matcher(matcher, prop, value);
            with(world, entity, set)
        }
        (prop, value) if prop.part() == Part::Mist => {
            with(world, entity, |mist: &mut voxear_render::Mist| set_mist(mist, prop, value))
        }
        (Prop::NetPriority, Value::Number(n)) => {
            with(world, entity, |net: &mut voxear_engine::net::NetObject| net.priority = n.max(0.0))
        }
        (Prop::NetRange, Value::Number(n)) => {
            with(world, entity, |net: &mut voxear_engine::net::NetObject| net.range = n.max(0.0))
        }
        (prop, value) if prop.part() == Part::ParticleSystem => {
            with(world, entity, |system: &mut ParticleSystem| set_particle(system, prop, value))
        }
        (Prop::OceanWeather, Value::Choice(index, _)) => {
            let weather = Weather::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |ocean: &mut Ocean| ocean.weather = weather)
        }
        (Prop::OceanWindSource, Value::Choice(index, _)) => {
            let source = WindSource::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |ocean: &mut Ocean| ocean.wind_source = source)
        }
        (Prop::OceanTransition, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.transition = n.clamp(0.0, 600.0)),
        (Prop::OceanSwellWind, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.swell_wind = n.clamp(0.1, 40.0)),
        (Prop::OceanSwellFetch, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.swell_fetch = n.clamp(0.1, 2000.0)),
        (Prop::OceanWind, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.wind = n.clamp(0.1, 40.0)),
        (Prop::OceanFetch, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.fetch = n.clamp(0.1, 2000.0)),
        (Prop::OceanChoppiness, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.choppiness = n.clamp(0.0, 1.5)),
        (Prop::OceanWaveScale, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.wave_scale = n.clamp(0.0, 4.0)),
        (Prop::OceanDeep, Value::Color(color)) => with(world, entity, |o: &mut Ocean| o.deep = color.map(|c| c.clamp(0.0, 1.0))),
        (Prop::OceanScatter, Value::Color(color)) => {
            with(world, entity, |o: &mut Ocean| o.scatter = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::OceanFoam, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.foam = n.clamp(0.0, 1.0)),
        (Prop::OceanSpray, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.spray = n.clamp(0.0, 2.0)),
        (Prop::OceanClarity, Value::Color(color)) => {
            with(world, entity, |o: &mut Ocean| o.clarity = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::OceanMurk, Value::Number(n)) => with(world, entity, |o: &mut Ocean| o.murk = n.clamp(0.0, 20.0)),
        (Prop::OceanShores, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.shores = on),
        (Prop::OceanFoamTrails, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.foam_trails = on),
        (Prop::OceanFoam3d, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.foam_3d = on),
        (Prop::OceanLightBending, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.light_bending = on),
        (Prop::OceanLightShafts, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.light_shafts = on),
        (Prop::OceanWetLens, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.wet_lens = on),
        (Prop::OceanFineRipples, Value::Toggle(on)) => with(world, entity, |o: &mut Ocean| o.fine_ripples = on),
        (Prop::PoolSize, Value::Vector(v)) if v.len() == 2 => {
            with(world, entity, |p: &mut Pool| p.size = [v[0].max(0.1), v[1].max(0.1)])
        }
        (Prop::PoolDepth, Value::Number(n)) => with(world, entity, |p: &mut Pool| p.depth = n.max(0.01)),
        (Prop::PoolLiquid, Value::Choice(index, _)) => {
            let liquid = Liquid::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |p: &mut Pool| p.liquid = liquid)
        }
        (Prop::PoolWaves, Value::Number(n)) => with(world, entity, |p: &mut Pool| p.waves = n.clamp(0.0, 2.0)),
        (Prop::PoolDeep, Value::Color(color)) => with(world, entity, |p: &mut Pool| p.deep = color.map(|c| c.clamp(0.0, 1.0))),
        (Prop::PoolScatter, Value::Color(color)) => {
            with(world, entity, |p: &mut Pool| p.scatter = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::PoolClarity, Value::Color(color)) => {
            with(world, entity, |p: &mut Pool| p.clarity = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::PoolMurk, Value::Number(n)) => with(world, entity, |p: &mut Pool| p.murk = n.clamp(0.0, 20.0)),
        (Prop::PoolCooling, Value::Number(n)) => with(world, entity, |p: &mut Pool| p.cooling = n.clamp(0.1, 3600.0)),
        (Prop::PoolGlow, Value::Number(n)) => with(world, entity, |p: &mut Pool| p.glow = n.clamp(0.0, 20.0)),
        (Prop::PathClosed, Value::Toggle(on)) => with(world, entity, |p: &mut voxear_scene::Path| p.closed = on),
        (Prop::StreamLiquid, Value::Choice(index, _)) => {
            let liquid = Liquid::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |s: &mut Stream| s.liquid = liquid)
        }
        (Prop::StreamDepth, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.depth = n.clamp(0.01, 100.0)),
        (Prop::StreamSpeed, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.speed = n.clamp(0.0, 20.0)),
        (Prop::StreamRapids, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.rapids = n.clamp(0.001, 10.0)),
        (Prop::StreamDeep, Value::Color(color)) => with(world, entity, |s: &mut Stream| s.deep = color.map(|c| c.clamp(0.0, 1.0))),
        (Prop::StreamScatter, Value::Color(color)) => {
            with(world, entity, |s: &mut Stream| s.scatter = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::StreamClarity, Value::Color(color)) => {
            with(world, entity, |s: &mut Stream| s.clarity = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::StreamMurk, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.murk = n.clamp(0.0, 20.0)),
        (Prop::StreamCooling, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.cooling = n.clamp(0.1, 3600.0)),
        (Prop::StreamGlow, Value::Number(n)) => with(world, entity, |s: &mut Stream| s.glow = n.clamp(0.0, 20.0)),
        (Prop::BuoyancySubmerged, Value::Number(n)) => {
            with(world, entity, |b: &mut voxear_engine::water::Buoyancy| b.submerged = n.clamp(0.01, 2.0))
        }
        (Prop::BuoyancyDrag, Value::Number(n)) => with(world, entity, |b: &mut voxear_engine::water::Buoyancy| b.drag = n.clamp(0.0, 10.0)),
        (Prop::BuoyancyAngularDrag, Value::Number(n)) => {
            with(world, entity, |b: &mut voxear_engine::water::Buoyancy| b.angular_drag = n.clamp(0.0, 10.0))
        }
        (Prop::DryShape, Value::Choice(index, _)) => {
            let shape = DryShape::ALL.get(index).copied().unwrap_or_default();
            with(world, entity, |d: &mut DryArea| d.shape = shape)
        }
        (Prop::DrySize, Value::Vector(v)) if v.len() == 3 => {
            with(world, entity, |d: &mut DryArea| d.size = [v[0].max(0.01), v[1].max(0.01), v[2].max(0.01)])
        }
        (Prop::DryWidth, Value::Number(n)) => with(world, entity, |d: &mut DryArea| d.size[0] = n.max(0.01)),
        (Prop::DryHeight, Value::Number(n)) => with(world, entity, |d: &mut DryArea| d.size[1] = n.max(0.01)),
        (Prop::DryBilge, Value::Number(n)) => with(world, entity, |d: &mut DryArea| d.bilge = n.max(0.0)),
        (Prop::TimeOfYear, Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.time_of_year = n.rem_euclid(4.0))
        }
        (Prop::Season, Value::Choice(index, _)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.set(Season::ALL[index.min(3)]))
        }
        (Prop::SeasonColor(season), Value::Color(color)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.look_mut(*season).color = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::SeasonColorAmount(season), Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.look_mut(*season).color_amount = n.clamp(0.0, 1.0))
        }
        (Prop::SeasonSnow(season), Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.look_mut(*season).snow = n.clamp(0.0, 1.0))
        }
        (Prop::SeasonVariation, Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.variation = n.clamp(0.0, 1.0))
        }
        (Prop::SeasonPatchSize, Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.patch_size = n.max(0.1))
        }
        (Prop::Dryness, Value::Number(n)) => with(world, entity, |seasons: &mut Seasons| seasons.dryness = n.clamp(0.0, 1.0)),
        (Prop::DryColor, Value::Color(color)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.dry_color = color.map(|c| c.clamp(0.0, 1.0)))
        }
        (Prop::Wetness, Value::Number(n)) => with(world, entity, |seasons: &mut Seasons| seasons.wetness = n.clamp(0.0, 1.0)),
        (Prop::GroundCover, Value::Number(n)) => {
            with(world, entity, |seasons: &mut Seasons| seasons.ground_cover = n.clamp(0.0, 1.0))
        }
        (Prop::VolumePriority, Value::Number(n)) => {
            with(world, entity, |volume: &mut PostVolume| volume.priority = n.round() as i32)
        }
        (Prop::FieldOfView, Value::Number(n)) => {
            with(world, entity, |camera: &mut Camera| camera.fov_y_degrees = n.clamp(1.0, 179.0))
        }
        (Prop::Near, Value::Number(n)) => with(world, entity, |camera: &mut Camera| camera.near = n.max(0.001)),
        (Prop::Exposure, Value::Number(n)) => with(world, entity, |camera: &mut Camera| camera.exposure = n.max(0.0)),
        (Prop::SoundVolume, Value::Number(n)) => with(world, entity, |a: &mut AudioSource| a.volume = n.max(0.0)),
        (Prop::Pitch, Value::Number(n)) => with(world, entity, |a: &mut AudioSource| a.pitch = n.clamp(0.05, 4.0)),
        (Prop::Loop, Value::Toggle(on)) => with(world, entity, |a: &mut AudioSource| a.looping = on),
        (Prop::PlayOnStart, Value::Toggle(on)) => with(world, entity, |a: &mut AudioSource| a.play_on_start = on),
        (Prop::SoundSpace, Value::Choice(index, _)) => with(world, entity, |a: &mut AudioSource| {
            a.space = if index == 1 { SoundSpace::TwoD } else { SoundSpace::ThreeD };
        }),
        (Prop::MinDistance, Value::Number(n)) => {
            with(world, entity, |a: &mut AudioSource| a.min_distance = n.clamp(0.0, a.max_distance))
        }
        (Prop::MaxDistance, Value::Number(n)) => {
            with(world, entity, |a: &mut AudioSource| a.max_distance = n.max(a.min_distance))
        }
        (Prop::DopplerLevel, Value::Number(n)) => {
            with(world, entity, |a: &mut AudioSource| a.doppler = n.clamp(0.0, 5.0))
        }
        (Prop::Occlusion, Value::Choice(index, _)) => with(world, entity, |a: &mut AudioSource| {
            a.occlusion = SoundOcclusion::ALL[index.min(SoundOcclusion::ALL.len() - 1)];
        }),
        (Prop::OcclusionRadius, Value::Number(n)) => {
            with(world, entity, |a: &mut AudioSource| a.occlusion_radius = n.max(0.01))
        }
        (Prop::Transmission, Value::Toggle(on)) => with(world, entity, |a: &mut AudioSource| a.transmission = on),
        (Prop::Pathing, Value::Toggle(on)) => with(world, entity, |a: &mut AudioSource| a.pathing = on),
        (Prop::SoundBus, Value::Choice(index, _)) => with(world, entity, |a: &mut AudioSource| {
            a.bus = if index == 1 { AudioBus::Music } else { AudioBus::Effects };
        }),
        (Prop::LodBias, Value::Number(n)) => with(world, entity, |r: &mut MeshRenderer| r.lod_bias = n.clamp(-8.0, 8.0)),
        (Prop::LodMeasure, Value::Choice(index, _)) => with(world, entity, |group: &mut LodGroup| {
            group.set_measure(if index == 1 { LodMeasure::Distance } else { LodMeasure::ScreenSize });
        }),
        (Prop::LodCull, Value::Toggle(on)) => with(world, entity, |group: &mut LodGroup| group.cull = on),
        (Prop::LodUntil(level), Value::Number(n)) => with(world, entity, |group: &mut LodGroup| {
            let measure = group.measure;
            if let Some(at) = group.levels.get_mut(*level) {
                at.until = match measure {
                    LodMeasure::ScreenSize => (n / 100.0).clamp(0.0, 1.0),
                    LodMeasure::Distance => n.max(0.0),
                };
            }
        }),
        _ => false,
    }
}

/// Changes a component, if the Obj has it.
/// A Smoke's field as the Inspector shows it, its colour in sRGB.
fn smoke_value(smoke: &voxear_render::Smoke, prop: &Prop) -> Option<Value> {
    Some(match prop {
        Prop::SmokeSize => Value::Number(smoke.size),
        Prop::SmokeCell => Value::Number(smoke.cell),
        Prop::SmokeDensity => Value::Number(smoke.density),
        Prop::SmokeColor => Value::Color(srgb(smoke.color).map(round3)),
        Prop::SmokeSpread => Value::Number(smoke.spread),
        Prop::SmokeLasts => Value::Number(smoke.lasts),
        Prop::SmokeFade => Value::Number(smoke.fade),
        Prop::SmokeSeed => Value::Whole(i64::from(smoke.seed)),
        Prop::SmokePlayOnStart => Value::Toggle(smoke.play_on_start),
        _ => return None,
    })
}

/// Sets a Smoke's field from the Inspector, kept within what it can be.
fn set_smoke(smoke: &mut voxear_render::Smoke, prop: &Prop, value: Value) {
    match (prop, value) {
        (Prop::SmokeSize, Value::Number(n)) => smoke.size = n.clamp(0.1, 30.0),
        (Prop::SmokeCell, Value::Number(n)) => smoke.cell = n.clamp(0.1, 2.0),
        (Prop::SmokeDensity, Value::Number(n)) => smoke.density = n.max(0.0),
        (Prop::SmokeColor, Value::Color(c)) => smoke.color = linear(c.map(|c| c.clamp(0.0, 1.0))),
        (Prop::SmokeSpread, Value::Number(n)) => smoke.spread = n.max(0.0),
        (Prop::SmokeLasts, Value::Number(n)) => smoke.lasts = n.max(0.0),
        (Prop::SmokeFade, Value::Number(n)) => smoke.fade = n.max(0.0),
        (Prop::SmokeSeed, Value::Whole(n)) => smoke.seed = n.clamp(0, i64::from(u32::MAX)) as u32,
        (Prop::SmokePlayOnStart, Value::Toggle(on)) => smoke.play_on_start = on,
        _ => {}
    }
}

/// A Mist's field as the Inspector shows it, its colour in sRGB.
fn mist_value(mist: &voxear_render::Mist, prop: &Prop) -> Option<Value> {
    Some(match prop {
        Prop::MistEmitting => Value::Toggle(mist.emitting),
        Prop::MistWidth => Value::Number(mist.width),
        Prop::MistDensity => Value::Number(mist.density),
        Prop::MistColor => Value::Color(srgb(mist.color).map(round3)),
        Prop::MistLinger => Value::Number(mist.linger),
        Prop::MistSeed => Value::Whole(i64::from(mist.seed)),
        _ => return None,
    })
}

/// Sets a Mist's field from the Inspector, kept within what it can be.
fn set_mist(mist: &mut voxear_render::Mist, prop: &Prop, value: Value) {
    match (prop, value) {
        (Prop::MistEmitting, Value::Toggle(on)) => mist.emitting = on,
        (Prop::MistWidth, Value::Number(n)) => mist.width = n.clamp(0.05, 10.0),
        (Prop::MistDensity, Value::Number(n)) => mist.density = n.max(0.0),
        (Prop::MistColor, Value::Color(c)) => mist.color = linear(c.map(|c| c.clamp(0.0, 1.0))),
        (Prop::MistLinger, Value::Number(n)) => mist.linger = n.clamp(0.05, 30.0),
        (Prop::MistSeed, Value::Whole(n)) => mist.seed = n.clamp(0, i64::from(u32::MAX)) as u32,
        _ => {}
    }
}

/// A Motion Matcher's field as the Inspector shows it.
fn matcher_value(matcher: &voxear_engine::animation::MotionMatcher, prop: &Prop) -> Option<Value> {
    Some(match prop {
        Prop::MatcherTurnToMovement => Value::Toggle(matcher.turn_to_movement),
        Prop::MatcherMatchSpeed => Value::Toggle(matcher.match_speed),
        Prop::MatcherRootMotion => Value::Toggle(matcher.root_motion),
        Prop::MatcherFootLocking => Value::Toggle(matcher.foot_locking),
        Prop::MatcherDebug => Value::Toggle(matcher.debug),
        _ => Value::Number(*matcher_number(&mut matcher.clone(), prop)?),
    })
}

/// Sets a Motion Matcher's field from the Inspector, kept within what it
/// can be.
fn set_matcher(matcher: &mut voxear_engine::animation::MotionMatcher, prop: &Prop, value: Value) {
    let (low, high) = match prop {
        Prop::MatcherIdleSpeed => (0.0, 5.0),
        Prop::MatcherSearchEvery => (0.02, 1.0),
        Prop::MatcherBlend => (0.0, 2.0),
        Prop::MatcherLegTurn => (0.0, 180.0),
        _ => (0.0, 3600.0),
    };
    match (prop, value) {
        (Prop::MatcherTurnToMovement, Value::Toggle(on)) => matcher.turn_to_movement = on,
        (Prop::MatcherMatchSpeed, Value::Toggle(on)) => matcher.match_speed = on,
        (Prop::MatcherRootMotion, Value::Toggle(on)) => matcher.root_motion = on,
        (Prop::MatcherFootLocking, Value::Toggle(on)) => matcher.foot_locking = on,
        (Prop::MatcherDebug, Value::Toggle(on)) => matcher.debug = on,
        (_, Value::Number(n)) => {
            if let Some(field) = matcher_number(matcher, prop) {
                *field = n.clamp(low, high);
            }
        }
        _ => {}
    }
}

/// A Motion Matcher's number a prop shows.
fn matcher_number<'a>(matcher: &'a mut voxear_engine::animation::MotionMatcher, prop: &Prop) -> Option<&'a mut f32> {
    Some(match prop {
        Prop::MatcherIdleSpeed => &mut matcher.idle_speed,
        Prop::MatcherSearchEvery => &mut matcher.search_every,
        Prop::MatcherBlend => &mut matcher.blend,
        Prop::MatcherTurnRate => &mut matcher.turn_rate,
        Prop::MatcherLegTurn => &mut matcher.leg_turn,
        _ => return None,
    })
}

/// A Particle System's field as the Inspector shows it: its colours in sRGB,
/// their opacity apart.
fn particle_value(system: &ParticleSystem, prop: &Prop) -> Option<Value> {
    let choice = |index: usize, names: Vec<&str>| Value::Choice(index, names.into_iter().map(String::from).collect());
    let color = |c: [f32; 4]| Value::Color(srgb([c[0], c[1], c[2]]).map(round3));
    Some(match prop {
        Prop::ParticleDuration => Value::Number(system.duration),
        Prop::ParticleLooping => Value::Toggle(system.looping),
        Prop::ParticleRate => Value::Number(system.rate),
        Prop::ParticleBurst => Value::Whole(i64::from(system.burst)),
        Prop::ParticleLifetime => Value::Vector(system.lifetime.to_vec()),
        Prop::ParticleSpeed => Value::Vector(system.speed.to_vec()),
        Prop::ParticleSize => Value::Vector(system.size.to_vec()),
        Prop::ParticleSizeEnd => Value::Number(system.size_end),
        Prop::ParticleColor => color(system.color),
        Prop::ParticleOpacity => Value::Number(round3(system.color[3])),
        Prop::ParticleColorEnd => color(system.color_end),
        Prop::ParticleOpacityEnd => Value::Number(round3(system.color_end[3])),
        Prop::ParticleIntensity => Value::Number(system.intensity),
        Prop::ParticleBlend => {
            let index = ParticleBlend::ALL.iter().position(|b| *b == system.blend).unwrap_or(0);
            choice(index, ParticleBlend::ALL.iter().map(|b| b.name()).collect())
        }
        Prop::ParticleLit => Value::Toggle(system.lit),
        Prop::ParticleSoftness => Value::Number(system.softness),
        Prop::ParticleShape => {
            let index = EmitShape::ALL.iter().position(|s| *s == system.shape).unwrap_or(0);
            choice(index, EmitShape::ALL.iter().map(|s| s.name()).collect())
        }
        Prop::ParticleRadius => Value::Number(system.radius),
        Prop::ParticleAngle => Value::Number(system.angle),
        Prop::ParticleBoxSize => Value::Vector(system.box_size.to_vec()),
        Prop::ParticleGravity => Value::Number(system.gravity),
        Prop::ParticleDrag => Value::Number(system.drag),
        Prop::ParticleSpin => Value::Number(system.spin),
        Prop::ParticleSeed => Value::Whole(i64::from(system.seed)),
        Prop::ParticleSizeCurve => Value::Curve(system.size_curve),
        Prop::ParticleOpacityCurve => Value::Curve(system.opacity_curve),
        Prop::ParticleSpeedCurve => Value::Curve(system.speed_curve),
        Prop::ParticleColorGradient => Value::Gradient(system.color_gradient),
        _ => return None,
    })
}

/// Sets a Particle System's field from the Inspector, kept within what it
/// can be.
fn set_particle(system: &mut ParticleSystem, prop: &Prop, value: Value) {
    let pair = |v: &[f32], least: f32| [v[0].max(least), v[1].max(least)];
    let whole = |n: i64| n.clamp(0, i64::from(u32::MAX)) as u32;
    let rgb = |c: [f32; 3], alpha: f32| {
        let [r, g, b] = linear(c.map(|c| c.clamp(0.0, 1.0)));
        [r, g, b, alpha]
    };
    match (prop, value) {
        (Prop::ParticleDuration, Value::Number(n)) => system.duration = n.max(0.01),
        (Prop::ParticleLooping, Value::Toggle(on)) => system.looping = on,
        (Prop::ParticleRate, Value::Number(n)) => system.rate = n.max(0.0),
        (Prop::ParticleBurst, Value::Whole(n)) => system.burst = whole(n),
        (Prop::ParticleLifetime, Value::Vector(v)) if v.len() == 2 => system.lifetime = pair(&v, 0.01),
        (Prop::ParticleSpeed, Value::Vector(v)) if v.len() == 2 => system.speed = [v[0], v[1]],
        (Prop::ParticleSize, Value::Vector(v)) if v.len() == 2 => system.size = pair(&v, 0.0),
        (Prop::ParticleSizeEnd, Value::Number(n)) => system.size_end = n.max(0.0),
        (Prop::ParticleColor, Value::Color(c)) => system.color = rgb(c, system.color[3]),
        (Prop::ParticleOpacity, Value::Number(n)) => system.color[3] = n.clamp(0.0, 1.0),
        (Prop::ParticleColorEnd, Value::Color(c)) => system.color_end = rgb(c, system.color_end[3]),
        (Prop::ParticleOpacityEnd, Value::Number(n)) => system.color_end[3] = n.clamp(0.0, 1.0),
        (Prop::ParticleIntensity, Value::Number(n)) => system.intensity = n.max(0.0),
        (Prop::ParticleBlend, Value::Choice(index, _)) => {
            system.blend = ParticleBlend::ALL.get(index).copied().unwrap_or_default();
        }
        (Prop::ParticleLit, Value::Toggle(on)) => system.lit = on,
        (Prop::ParticleSoftness, Value::Number(n)) => system.softness = n.max(0.0),
        (Prop::ParticleShape, Value::Choice(index, _)) => {
            system.shape = EmitShape::ALL.get(index).copied().unwrap_or_default();
        }
        (Prop::ParticleRadius, Value::Number(n)) => system.radius = n.max(0.0),
        (Prop::ParticleAngle, Value::Number(n)) => system.angle = n.clamp(0.0, 180.0),
        (Prop::ParticleBoxSize, Value::Vector(v)) if v.len() == 3 => {
            system.box_size = [v[0], v[1], v[2]].map(|s| s.max(0.0));
        }
        (Prop::ParticleGravity, Value::Number(n)) => system.gravity = n,
        (Prop::ParticleDrag, Value::Number(n)) => system.drag = n.max(0.0),
        (Prop::ParticleSpin, Value::Number(n)) => system.spin = n,
        (Prop::ParticleSeed, Value::Whole(n)) => system.seed = whole(n),
        (Prop::ParticleSizeCurve, Value::Curve(curve)) => system.size_curve = curve,
        (Prop::ParticleOpacityCurve, Value::Curve(curve)) => system.opacity_curve = curve,
        (Prop::ParticleSpeedCurve, Value::Curve(curve)) => system.speed_curve = curve,
        (Prop::ParticleColorGradient, Value::Gradient(gradient)) => system.color_gradient = gradient,
        _ => {}
    }
}

fn with<T: voxear_ecs::Component>(world: &mut World, entity: Entity, change: impl FnOnce(&mut T)) -> bool {
    let Some(component) = world.get_mut::<T>(entity) else { return false };
    change(component);
    true
}

/// Every `.material` asset in the project, by path.
fn material_assets(game: &Game) -> Vec<String> {
    let mut paths: Vec<String> =
        game.content.db().entries().filter(|e| e.kind == AssetKind::Material).map(|e| e.path.clone()).collect();
    paths.sort();
    paths
}

/// Each clip of the project's model files that have been read, by its name,
/// and each file not read yet whole, as a clip field picks from them.
/// Listing them reads nothing, so a picker opens at once.
pub(crate) fn model_clips(game: &Game) -> Vec<(String, Pick)> {
    let db = game.content.db();
    let models = db.entries().filter(|e| e.kind == AssetKind::Model);
    let mut models: Vec<_> = models.map(|e| (e.path.clone(), e.guid)).collect();
    models.sort();
    let mut found = Vec::new();
    for (path, guid) in models {
        match game.content.known_clips(guid) {
            Some(clips) => {
                let reference = |clip: &String| format!("{path}#{CLIP_PART}{clip}");
                found.extend(clips.iter().map(|clip| (clip.clone(), Pick::Asset(Some(reference(clip))))));
            }
            None => found.push((file_name(&path).to_string(), Pick::Asset(Some(path)))),
        }
    }
    found
}

/// A clip picked, as the reference a clip field keeps: the file's GUID and
/// the clip, `…#clip:Walk_Loop`. A file picked whole gives its first clip,
/// reading it. `None` for a file without clips.
pub(crate) fn clip_reference(game: &mut Game, choice: &str) -> Option<String> {
    let reference = game.content.parse(choice).ok()?;
    if reference.part().is_some() {
        return Some(reference.to_string());
    }
    let guid = reference.guid()?;
    let first = game.content.clip_file(guid)?.clips.first()?.name.clone();
    Some(format!("{guid}#{CLIP_PART}{first}"))
}

/// A clip as a field shows it: its name, then its file, `Walk_Loop ·
/// Walks.glb`; "None" for none.
pub(crate) fn clip_label(game: &Game, reference: &str) -> String {
    if reference.is_empty() {
        return "None".into();
    }
    let Some((file, clip)) = reference.split_once(&format!("#{CLIP_PART}")) else { return reference.to_string() };
    let path = game.content.parse(file).ok().and_then(|r| r.guid()).and_then(|guid| game.content.db().get(guid));
    format!("{clip} · {}", file_name(path.map_or(file, |entry| entry.path.as_str())))
}

/// The bones of a clip's skeleton, when its file has been read; none
/// before.
pub(crate) fn clip_bones(game: &Game, reference: &str) -> Vec<String> {
    let guid = game.content.parse(reference).ok().and_then(|parsed| parsed.guid());
    let animation = guid.and_then(|guid| game.content.known_clip_file(guid));
    animation.map_or_else(Vec::new, |animation| animation.skeleton.bones.iter().map(|bone| bone.name.clone()).collect())
}

/// Each mesh of the project's models that is loaded, by the name its model
/// gives it, and each model not loaded yet whole, by its file, as a Mesh
/// picks from them. Listing them loads nothing, so a picker opens at once
/// however many models a project has.
fn model_meshes(game: &Game) -> Vec<(String, Pick)> {
    let db = game.content.db();
    let mut models: Vec<_> = db.entries().filter(|e| e.kind == AssetKind::Model).map(|e| (e.path.clone(), e.guid)).collect();
    models.sort();
    let mut found = Vec::new();
    for (path, guid) in models {
        let Some(nodes) = game.content.loaded_model(guid) else {
            found.push((file_name(&path).to_string(), Pick::Asset(Some(path))));
            continue;
        };
        let mut parts: Vec<&str> = Vec::new();
        for node in nodes {
            if parts.contains(&node.mesh_part.as_str()) {
                continue;
            }
            parts.push(&node.mesh_part);
            let reference = format!("{path}#{}", node.mesh_part);
            found.push((mesh_label(game, &reference), Pick::Asset(Some(reference))));
        }
    }
    found
}

/// The first mesh of a model picked whole, `Models/Gate.fbx#mesh0.0`,
/// loading it; `None` for a reference to a part already, or not a model.
fn first_mesh(game: &mut Game, renderer: &mut Renderer, reference: &str) -> Option<String> {
    if reference.contains('#') || reference.starts_with("builtin:") {
        return None;
    }
    let guid = game.content.db().guid_of(reference)?;
    let part = game.content.model(renderer, guid).ok()?.first()?.mesh_part.clone();
    Some(format!("{reference}#{part}"))
}

/// A model's mesh as fields and pickers name it: the name the model gives
/// the part, then the part, `Gate #mesh0.3`, which tells apart the parts of
/// a model that gives several the same name; or, for a model not loaded,
/// its file and the part.
fn mesh_label(game: &Game, reference: &str) -> String {
    let Some((asset, part)) = reference.split_once('#') else { return asset_label(game, reference) };
    let guid = game.content.parse(asset).ok().and_then(|r| r.guid());
    let nodes = guid.and_then(|guid| game.content.loaded_model(guid));
    let name = nodes.and_then(|nodes| nodes.iter().find(|node| node.mesh_part == part)).map(|node| node.name.as_str());
    match name.filter(|name| !name.is_empty()) {
        Some(name) => format!("{name} #{part}"),
        None => asset_label(game, reference),
    }
}

/// An asset reference by the name of its file: `Pyramid.gltf #mesh0.0`.
fn asset_label(game: &Game, reference: &str) -> String {
    let (asset, part) = reference.split_once('#').unwrap_or((reference, ""));
    let path = game
        .content
        .parse(asset)
        .ok()
        .and_then(|r| r.guid())
        .and_then(|g| game.content.db().get(g))
        .map(|e| e.path.clone());
    let name = path.as_deref().map_or(asset, file_name);
    if part.is_empty() { name.to_string() } else { format!("{name} #{part}") }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub use voxear_engine::script_types::nice_name;

fn round3(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// Moves a script `by` places among the Obj's scripts, as their order runs
/// and saves. In edit mode only: running behaviours keep their order.
pub fn move_behaviour(world: &mut World, entity: Entity, type_name: &str, by: isize) {
    let Some(behaviours) = world.get_mut::<EditBehaviours>(entity) else { return };
    let Some(at) = behaviours.0.iter().position(|spec| spec.type_name == type_name) else { return };
    let to = at as isize + by;
    if (0..behaviours.0.len() as isize).contains(&to) {
        behaviours.0.swap(at, to as usize);
    }
}

/// Locks a component for designers, showing them none of its values until
/// some are chosen, or unlocks it.
pub fn set_locked(world: &mut World, entity: Entity, part: &Part, locked: bool) {
    let mut locks = world.get::<Locks>(entity).cloned().unwrap_or_default();
    if locked {
        locks.0.entry(part.lock_key()).or_default();
    } else {
        locks.0.remove(&part.lock_key());
    }
    put_locks(world, entity, locks);
}

/// Shows one of a locked component's values to designers, or hides it.
pub fn show_to_designers(world: &mut World, entity: Entity, prop: &Prop, shown: bool) {
    let mut locks = world.get::<Locks>(entity).cloned().unwrap_or_default();
    let Some(fields) = locks.0.get_mut(&prop.part().lock_key()) else { return };
    let key = prop.lock_key();
    fields.retain(|field| *field != key);
    if shown {
        fields.push(key);
        fields.sort();
    }
    put_locks(world, entity, locks);
}

fn put_locks(world: &mut World, entity: Entity, locks: Locks) {
    if locks.0.is_empty() {
        world.remove::<Locks>(entity);
    } else {
        world.insert(entity, locks);
    }
}

/// An asset by GUID as a field shows it: its path, or "None".
fn guid_label(game: &Game, guid: Option<voxear_assets::AssetGuid>) -> String {
    match guid {
        Some(guid) => game.content.db().get(guid).map_or_else(|| guid.to_string(), |e| e.path.clone()),
        None => "None".into(),
    }
}

/// The project's file an asset reference is to: a path, a GUID, or either
/// with a part (`Sponza.gltf#mesh0.0`), whose file is the model's.
pub fn path_of(game: &Game, reference: &str) -> Option<String> {
    let asset = reference.split('#').next().unwrap_or(reference);
    let guid = game.content.parse(asset).ok().and_then(|r| r.guid())?;
    game.content.db().get(guid).map(|entry| entry.path.clone())
}

/// The project's file a prop of `entity` names, when it names one: a mesh's
/// model or file, a material's file, a sound, a cookie or an IES profile.
pub fn asset_path(game: &Game, entity: Entity, prop: &Prop) -> Option<String> {
    let world = &game.engine.world;
    match prop {
        Prop::Mesh => path_of(game, &game.content.mesh_ref(world.get::<MeshRenderer>(entity)?.mesh)?.to_string()),
        Prop::DryMesh => path_of(game, &game.content.mesh_ref(world.get::<DryArea>(entity)?.mesh?)?.to_string()),
        Prop::Material => match game.content.material_source(world.get::<MeshRenderer>(entity)?.material)? {
            MaterialSource::Asset(reference) => path_of(game, &reference.to_string()),
            MaterialSource::Inline(_) => None,
        },
        // The rest show their file's path.
        _ => match get(game, &ScriptTypes::default(), entity, prop)? {
            Value::Asset(path) => game.content.db().guid_of(&path).map(|_| path),
            _ => None,
        },
    }
}

/// A season's table in a Seasons, as scene files write it.
fn season_key(season: Season) -> &'static str {
    match season {
        Season::Spring => "spring",
        Season::Summer => "summer",
        Season::Autumn => "autumn",
        Season::Winter => "winter",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_names_read_as_unity_shows_them() {
        assert_eq!(nice_name("degreesPerSecond"), "Degrees Per Second");
        assert_eq!(nice_name("rare_chance"), "Rare Chance");
        assert_eq!(nice_name("_speed"), "Speed");
        assert_eq!(nice_name("Level"), "Level");
        assert_eq!(nice_name("lamp2"), "Lamp 2");
    }

    #[test]
    fn seasons_show_the_look_of_the_season_picked() {
        let mut world = World::new();
        let entity = world.spawn((Seasons::default(),));
        let types = ScriptTypes::default();
        let shown = |world: &World| props(world, entity, &Part::Seasons, &types);
        assert!(shown(&world).contains(&Prop::SeasonColor(Season::Summer)), "summer's look, where the year starts");
        assert!(set_component(&mut world, entity, &Prop::Season, Value::Choice(2, Vec::new())));
        assert_eq!(world.get::<Seasons>(entity).unwrap().time_of_year, 2.0, "picking autumn turns the year to it");
        assert!(shown(&world).contains(&Prop::SeasonSnow(Season::Autumn)), "and shows autumn's look");
        assert!(set_component(&mut world, entity, &Prop::SeasonColorAmount(Season::Autumn), Value::Number(0.4)));
        assert_eq!(world.get::<Seasons>(entity).unwrap().autumn.color_amount, 0.4);
        assert_eq!(Prop::SeasonColorAmount(Season::Autumn).lock_key(), "autumn.color_amount");
    }

    #[test]
    fn a_particle_systems_rows_follow_its_shape_and_its_colours_show_in_srgb() {
        let mut world = World::new();
        let entity = world.spawn((ParticleSystem::default(),));
        let types = ScriptTypes::default();
        let shown = |world: &World| props(world, entity, &Part::ParticleSystem, &types);
        assert!(shown(&world).contains(&Prop::ParticleAngle), "a Cone's angle");
        assert!(set_component(&mut world, entity, &Prop::ParticleShape, Value::Choice(3, Vec::new())));
        assert_eq!(world.get::<ParticleSystem>(entity).unwrap().shape, EmitShape::Box);
        assert!(shown(&world).contains(&Prop::ParticleBoxSize) && !shown(&world).contains(&Prop::ParticleAngle));
        // Mid grey as picked is a fifth of white's light, and keeps its opacity.
        assert!(set_component(&mut world, entity, &Prop::ParticleOpacity, Value::Number(0.25)));
        assert!(set_component(&mut world, entity, &Prop::ParticleColor, Value::Color([0.5; 3])));
        let color = world.get::<ParticleSystem>(entity).unwrap().color;
        assert!((color[0] - 0.214).abs() < 0.01 && color[3] == 0.25, "{color:?}");
        assert!(set_component(&mut world, entity, &Prop::ParticleLifetime, Value::Vector(vec![-1.0, 4.0])));
        assert_eq!(world.get::<ParticleSystem>(entity).unwrap().lifetime, [0.01, 4.0], "never less than a moment");
        assert!(set_component(&mut world, entity, &Prop::ParticleBurst, Value::Whole(12)));
        assert_eq!(get_particle(&world, entity, Prop::ParticleBurst), Some(Value::Whole(12)));
        assert_eq!(Prop::ParticleOpacityEnd.lock_key(), "color_end");
    }

    fn get_particle(world: &World, entity: Entity, prop: Prop) -> Option<Value> {
        particle_value(world.get::<ParticleSystem>(entity)?, &prop)
    }

    #[test]
    fn a_foliages_impostor_distance_shows_only_when_it_is_its_own() {
        let mut world = World::new();
        let foliage = Foliage {
            mesh: voxear_render::MeshHandle(0),
            material: voxear_render::MaterialHandle(0),
            plants: Default::default(),
            draw_distance: 200.0,
            fade_distance: 5.0,
            shadow_distance: 30.0,
            healthy_color: [1.0; 3],
            dry_color: [1.0; 3],
            color_spread: 8.0,
            impostors: Impostors::Auto,
            impostor_distance: 60.0,
        };
        let entity = world.spawn((foliage,));
        let types = ScriptTypes::default();
        let shown = |world: &World| props(world, entity, &Part::Foliage, &types);
        assert!(shown(&world).contains(&Prop::FoliageImpostors));
        assert!(!shown(&world).contains(&Prop::FoliageImpostorDistance), "Auto chooses its own distance");
        assert!(set_component(&mut world, entity, &Prop::FoliageImpostors, Value::Choice(2, Vec::new())));
        assert_eq!(world.get::<Foliage>(entity).unwrap().impostors, Impostors::From);
        assert!(shown(&world).contains(&Prop::FoliageImpostorDistance), "From distance shows the distance");
        assert!(set_component(&mut world, entity, &Prop::FoliageImpostorDistance, Value::Number(-4.0)));
        assert_eq!(world.get::<Foliage>(entity).unwrap().impostor_distance, 0.0, "never nearer than the camera");
    }
}
