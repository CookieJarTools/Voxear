//! Components scripts read and write whole, as their scene files write
//! them: a Post Volume, a Wind Zone, an Ocean, a Pool and a Stream through
//! each Obj's own (`PostVolume`, `WindZone`, `Ocean`, `Pool` and
//! `WaterStream` in Voxear.Core); a Bone Attachment, a Particle System, a
//! Nav Area, a Nav Link, a Foliage and a LOD Group through one kind for
//! them all, [`ComponentKind::Whole`], beside the calls some have of their
//! own; and the level's Seasons, Climate and storm rules through the `Post`
//! call (`Seasons`, `Climate` and `Storms`). Each reads as JSON with
//! every setting, and takes JSON of the settings to change: the rest stay
//! as they were, and nothing changes where any is refused.

use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{
    Foliage, Impostors, LodGroup, LodLevel, LodMeasure, Ocean, ParticleSystem, Pool, PostVolume, Seasons, Stream,
    WindZone,
};
use voxear_scripting::ComponentKind;

use crate::{
    animation::BoneAttachment,
    ground_fit::GroundFit,
    ground_streaming::StreamsWithGround,
    prefab_placing::{Placement, PrefabBrush},
    roads::{AreaFill, Road},
    walkable_models::Walkable,
    heat::Climate,
    navigation::{NavArea, NavLink},
    scene_file::PostVolumeData,
    storms::StormRules,
};

/// Sets what answers each kind in scripts.
pub(crate) fn register() {
    whole::<PostVolume>(ComponentKind::PostVolume);
    whole::<WindZone>(ComponentKind::WindZone);
    whole::<Ocean>(ComponentKind::Ocean);
    whole::<Pool>(ComponentKind::Pool);
    whole::<Stream>(ComponentKind::Stream);
    voxear_scripting::set_kind_values(ComponentKind::Whole, whole_values);
    voxear_scripting::set_kind_texts(ComponentKind::Whole, whole_texts);
}

// Which component the Whole kind's calls are of, in the number's higher bits, as `NativeWhole` in Voxear.Core
// numbers them.
const BONE_ATTACHMENT: i32 = 0;
const PARTICLE_SYSTEM: i32 = 1;
const NAV_AREA: i32 = 2;
const NAV_LINK: i32 = 3;
const FOLIAGE: i32 = 4;
const LOD_GROUP: i32 = 5;
// Assets only the content and the renderer load, read and set as text: a Dry Area's mesh, a Light's cookie and its
// IES profile.
const DRY_AREA_MESH: i32 = 6;
const LIGHT_COOKIE: i32 = 7;
const LIGHT_PROFILE: i32 = 8;
const GROUND_FIT: i32 = 9;
const STREAMS_WITH_GROUND: i32 = 10;
const ROAD: i32 = 11;
const AREA_FILL: i32 = 12;
const PLACEMENT: i32 = 13;
const PREFAB_BRUSH: i32 = 14;
const WALKABLE: i32 = 15;

/// Whether an Obj has one, and adding and taking off those scripts may add:
/// the component in the number's bits above the lowest two.
fn whole_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let (which, asked) = (what >> 2, what & 3);
    match which {
        NAV_AREA => values::<NavArea>(world, entity, asked, written),
        NAV_LINK => values::<NavLink>(world, entity, asked, written),
        GROUND_FIT => values::<GroundFit>(world, entity, asked, written),
        STREAMS_WITH_GROUND => values::<StreamsWithGround>(world, entity, asked, written),
        ROAD => values::<Road>(world, entity, asked, written),
        AREA_FILL => values::<AreaFill>(world, entity, asked, written),
        PLACEMENT => values::<Placement>(world, entity, asked, written),
        PREFAB_BRUSH => values::<PrefabBrush>(world, entity, asked, written),
        WALKABLE => values::<Walkable>(world, entity, asked, written),
        BONE_ATTACHMENT => has::<BoneAttachment>(world, entity, asked, written),
        PARTICLE_SYSTEM => has::<ParticleSystem>(world, entity, asked, written),
        FOLIAGE => has::<Foliage>(world, entity, asked, written),
        LOD_GROUP => has::<LodGroup>(world, entity, asked, written),
        _ => None,
    }
}

/// A component read whole and changed: the component in the number's bits
/// above the lowest.
fn whole_texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    let (which, asked) = (what >> 1, what & 1);
    match which {
        BONE_ATTACHMENT => texts::<BoneAttachment>(world, entity, asked, text),
        PARTICLE_SYSTEM => texts::<ParticleSystem>(world, entity, asked, text),
        NAV_AREA => texts::<NavArea>(world, entity, asked, text),
        NAV_LINK => texts::<NavLink>(world, entity, asked, text),
        GROUND_FIT => texts::<GroundFit>(world, entity, asked, text),
        STREAMS_WITH_GROUND => texts::<StreamsWithGround>(world, entity, asked, text),
        ROAD => texts::<Road>(world, entity, asked, text),
        AREA_FILL => texts::<AreaFill>(world, entity, asked, text),
        PLACEMENT => texts::<Placement>(world, entity, asked, text),
        PREFAB_BRUSH => texts::<PrefabBrush>(world, entity, asked, text),
        WALKABLE => texts::<Walkable>(world, entity, asked, text),
        FOLIAGE => texts::<Foliage>(world, entity, asked, text),
        LOD_GROUP => texts::<LodGroup>(world, entity, asked, text),
        DRY_AREA_MESH | LIGHT_COOKIE | LIGHT_PROFILE => {
            use voxear_scripting::ObjAsset;
            let asset = match which {
                DRY_AREA_MESH => ObjAsset::DryAreaMesh,
                LIGHT_COOKIE => ObjAsset::LightCookie,
                _ => ObjAsset::LightProfile,
            };
            let set = (asked == CHANGE).then(|| Some(text).filter(|text| !text.is_empty()));
            match voxear_scripting::obj_asset(world, entity, asset, set)? {
                Ok(named) => Some(named.unwrap_or_default()),
                Err(error) => {
                    tracing::error!(target: "script", "{error:#}");
                    None
                }
            }
        }
        _ => None,
    }
}

/// `entity`'s asset `which` as content names it, after loading `set` in its
/// place when given: a Light's cookie or IES profile by GUID, a Dry Area's
/// mesh as meshes are named. None where it has none.
pub fn obj_asset(
    world: &mut World,
    content: &mut crate::content::Content,
    renderer: Option<&mut voxear_render::Renderer>,
    entity: Entity,
    which: voxear_scripting::ObjAsset,
    set: Option<Option<&str>>,
) -> Result<Option<String>> {
    use voxear_render::{DryArea, Light};
    use voxear_scripting::ObjAsset;
    if let Some(set) = set {
        let reference = set.map(|named| content.parse(named)).transpose()?;
        let loaded = match (which, &reference, renderer) {
            (_, None, _) => None,
            (_, Some(_), None) => bail!("there is no renderer to load it with"),
            (ObjAsset::LightCookie, Some(named), Some(renderer)) => {
                Some(Loaded::Texture(content.cookie(renderer, named)?))
            }
            (ObjAsset::LightProfile, Some(named), Some(renderer)) => {
                Some(Loaded::Texture(content.light_profile(renderer, named)?))
            }
            (ObjAsset::DryAreaMesh, Some(named), Some(renderer)) => {
                let mesh = content.mesh(renderer, named)?;
                Some(Loaded::Mesh(mesh, renderer.mesh_bounds(mesh).map(|(low, high)| [low, high])))
            }
        };
        match which {
            ObjAsset::LightCookie | ObjAsset::LightProfile => {
                let light = world.get_mut::<Light>(entity).context("it has no Light")?;
                let texture = match loaded {
                    Some(Loaded::Texture(texture)) => Some(texture),
                    _ => None,
                };
                match which {
                    ObjAsset::LightCookie => light.cookie = texture,
                    _ => light.profile = texture,
                }
            }
            ObjAsset::DryAreaMesh => {
                let area = world.get_mut::<DryArea>(entity).context("it has no Dry Area")?;
                (area.mesh, area.bounds) = match loaded {
                    Some(Loaded::Mesh(mesh, bounds)) => (Some(mesh), bounds),
                    _ => (None, None),
                };
            }
        }
    }
    Ok(match which {
        ObjAsset::LightCookie => {
            let light = world.get::<Light>(entity).context("it has no Light")?;
            light.cookie.and_then(|texture| content.picture_guid(texture)).map(|guid| guid.to_string())
        }
        ObjAsset::LightProfile => {
            let light = world.get::<Light>(entity).context("it has no Light")?;
            light.profile.and_then(|texture| content.light_profile_guid(texture)).map(|guid| guid.to_string())
        }
        ObjAsset::DryAreaMesh => {
            let area = world.get::<DryArea>(entity).context("it has no Dry Area")?;
            area.mesh.and_then(|mesh| content.mesh_ref(mesh)).map(|reference| reference.to_string())
        }
    })
}

/// What [`obj_asset`] loaded.
enum Loaded {
    Texture(voxear_render::TextureId),
    Mesh(voxear_render::MeshHandle, Option<[voxear_core::math::Vec3; 2]>),
}

/// Whether an Obj has a component scripts cannot add or take off this way.
fn has<T: Component>(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match (what, written) {
        (HAS, None) if world.is_alive(entity) => Some([f32::from(u8::from(world.has::<T>(entity))), 0.0, 0.0, 0.0]),
        _ => None,
    }
}

/// A component as its scene file writes it.
pub trait AsData: Component + Sized {
    type Data: Serialize + DeserializeOwned;
    fn data(&self) -> Self::Data;
    /// The component `data` describes, in place of `before`.
    fn from_data(data: Self::Data, before: &Self) -> Result<Self>;
}

macro_rules! written_as_is {
    ($($kind:ty),*) => {$(
        impl AsData for $kind {
            type Data = $kind;
            fn data(&self) -> Self {
                self.clone()
            }
            fn from_data(data: Self, _: &Self) -> Result<Self> {
                Ok(data)
            }
        }
    )*};
}

written_as_is!(
    WindZone,
    Ocean,
    Pool,
    Stream,
    Seasons,
    Climate,
    StormRules,
    BoneAttachment,
    ParticleSystem,
    NavArea,
    NavLink,
    GroundFit,
    StreamsWithGround,
    Road,
    AreaFill,
    Placement,
    PrefabBrush,
    Walkable
);

/// A Foliage's settings scripts change: how far its plants are drawn, cast
/// shadows and fade, the colours they are tinted between, and their
/// impostors. Its mesh and material are its scene's.
#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoliageLook {
    pub draw_distance: f32,
    pub fade_distance: f32,
    pub shadow_distance: f32,
    pub healthy_color: [f32; 3],
    pub dry_color: [f32; 3],
    pub color_spread: f32,
    pub impostors: Impostors,
    pub impostor_distance: f32,
}

impl AsData for Foliage {
    type Data = FoliageLook;
    fn data(&self) -> FoliageLook {
        FoliageLook {
            draw_distance: self.draw_distance,
            fade_distance: self.fade_distance,
            shadow_distance: self.shadow_distance,
            healthy_color: self.healthy_color,
            dry_color: self.dry_color,
            color_spread: self.color_spread,
            impostors: self.impostors,
            impostor_distance: self.impostor_distance,
        }
    }
    fn from_data(look: FoliageLook, before: &Self) -> Result<Self> {
        let colour = |c: [f32; 3]| c.map(|part| part.clamp(0.0, 1.0));
        Ok(Foliage {
            draw_distance: look.draw_distance.max(0.0),
            fade_distance: look.fade_distance.max(0.0),
            shadow_distance: look.shadow_distance.max(0.0),
            healthy_color: colour(look.healthy_color),
            dry_color: colour(look.dry_color),
            color_spread: look.color_spread.max(0.01),
            impostors: look.impostors,
            impostor_distance: look.impostor_distance.max(0.0),
            ..before.clone()
        })
    }
}

/// A LOD Group as scripts read and set it: its Objs by their ids.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodData {
    pub measure: LodMeasure,
    pub cull: bool,
    pub levels: Vec<LodLevelData>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodLevelData {
    pub objs: Vec<u64>,
    pub until: f32,
}

impl AsData for LodGroup {
    type Data = LodData;
    fn data(&self) -> LodData {
        let levels = self
            .levels
            .iter()
            .map(|level| LodLevelData { objs: level.objs.iter().map(|e| e.to_bits()).collect(), until: level.until })
            .collect();
        LodData { measure: self.measure, cull: self.cull, levels }
    }
    fn from_data(data: LodData, _: &Self) -> Result<Self> {
        let levels = data
            .levels
            .into_iter()
            .map(|level| LodLevel {
                objs: level.objs.into_iter().map(Entity::from_bits).collect(),
                until: level.until.max(0.0),
            })
            .collect();
        Ok(LodGroup { measure: data.measure, cull: data.cull, levels })
    }
}

impl AsData for PostVolume {
    type Data = PostVolumeData;
    fn data(&self) -> PostVolumeData {
        PostVolumeData::of(self)
    }
    fn from_data(data: PostVolumeData, _: &Self) -> Result<Self> {
        data.component()
    }
}

fn whole<T: AsData + Default>(kind: ComponentKind) {
    voxear_scripting::set_kind_values(kind, values::<T>);
    voxear_scripting::set_kind_texts(kind, texts::<T>);
}

// What `values` and `texts` answer, as Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const READ: i32 = 0;
const CHANGE: i32 = 1;

fn values<T: AsData + Default>(
    world: &mut World,
    entity: Entity,
    what: i32,
    written: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => Some([f32::from(u8::from(world.has::<T>(entity))), 0.0, 0.0, 0.0]),
        (ADD, Some(_)) => {
            if !world.has::<T>(entity) {
                world.insert(entity, T::default());
            }
            Some([0.0; 4])
        }
        (REMOVE, Some(_)) => world.remove::<T>(entity).map(|_| [0.0; 4]),
        _ => None,
    }
}

fn texts<T: AsData>(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if !world.is_alive(entity) {
        return None;
    }
    let component = world.get_mut::<T>(entity)?;
    match what {
        READ => read(component).ok(),
        CHANGE => match change(component, text) {
            Ok(()) => Some(String::new()),
            Err(error) => {
                tracing::error!(target: "script", "{error:#}");
                None
            }
        },
        _ => None,
    }
}

/// Every setting of `component`, as JSON.
pub fn read<T: AsData>(component: &T) -> Result<String> {
    Ok(serde_json::to_string(&component.data())?)
}

/// Changes the settings `text` gives, as JSON, of `component`.
pub fn change<T: AsData>(component: &mut T, text: &str) -> Result<()> {
    let over: serde_json::Value = serde_json::from_str(text).context("reading the settings")?;
    if !over.is_object() {
        bail!("the settings are not a table of them");
    }
    let mut now = serde_json::to_value(component.data())?;
    lay_over(&mut now, &over);
    let data: T::Data = serde_json::from_value(now).context("changing the settings")?;
    *component = T::from_data(data, component)?;
    Ok(())
}

/// Writes `over`'s settings into `base`, table by table.
fn lay_over(base: &mut serde_json::Value, over: &serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(base), serde_json::Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(key) {
                    Some(inner) if inner.is_object() && value.is_object() => lay_over(inner, value),
                    _ => drop(base.insert(key.clone(), value.clone())),
                }
            }
        }
        (base, over) => *base = over.clone(),
    }
}

/// The level's own of a kind there is one of: the first by when it was made.
fn first<T: Component>(world: &World) -> Option<Entity> {
    let found = world.query_ref::<(Entity, &T)>().into_iter().map(|(entity, _)| entity);
    found.min_by_key(|entity| entity.to_bits())
}

/// The level's Seasons or Climate, as `json` names it (`{"component":
/// "seasons"}`), read; with `"fields"`, those changed first. None where the
/// level has none.
pub fn level_component(world: &mut World, json: &str) -> Result<Option<String>> {
    let asked: serde_json::Value = serde_json::from_str(json).context("reading what is asked")?;
    let fields = asked.get("fields").map(|fields| fields.to_string());
    match asked.get("component").and_then(|name| name.as_str()) {
        Some("seasons") => level::<Seasons>(world, fields.as_deref()),
        Some("climate") => level::<Climate>(world, fields.as_deref()),
        Some("storm_rules") => level::<StormRules>(world, fields.as_deref()),
        other => bail!("no level component {other:?}"),
    }
}

fn level<T: AsData>(world: &mut World, fields: Option<&str>) -> Result<Option<String>> {
    let Some(component) = first::<T>(world).and_then(|entity| world.get_mut::<T>(entity)) else {
        return Ok(None);
    };
    if let Some(fields) = fields {
        change(component, fields)?;
    }
    read(component).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_component_reads_whole_and_changes_only_what_is_given() {
        let mut ocean = Ocean::default();
        let before = ocean;
        change(&mut ocean, r#"{"weather":"stormy","deep":[0.1,0.2,0.3]}"#).unwrap();
        assert_eq!((ocean.weather, ocean.deep), (voxear_render::ocean::Weather::Stormy, [0.1, 0.2, 0.3]));
        assert_eq!(ocean.murk, before.murk, "the rest as it was");
        assert!(change(&mut ocean, r#"{"weather":"windy"}"#).is_err(), "a weather there is not");
        assert!(change(&mut ocean, r#"{"glow":1}"#).is_err(), "a setting there is not");
        assert_eq!(ocean.weather, voxear_render::ocean::Weather::Stormy, "kept after a refusal");
        let read: serde_json::Value = serde_json::from_str(&read(&ocean).unwrap()).unwrap();
        assert_eq!(read["weather"], "stormy");

        let mut volume = PostVolume::default();
        change(&mut volume, r#"{"shape":"global","post":{"bloom":{"intensity":0.4}}}"#).unwrap();
        assert_eq!(volume.shape, voxear_render::VolumeShape::Global);
        assert_eq!(volume.overrides["bloom"]["intensity"].as_float(), Some(0.4));
        assert!(change(&mut volume, r#"{"post":{"bloom":{"glow":1}}}"#).is_err(), "checked as a scene's are");
    }

    #[test]
    fn the_levels_seasons_and_climate_are_its_first() {
        let mut world = World::new();
        assert_eq!(level_component(&mut world, r#"{"component":"climate"}"#).unwrap(), None);
        world.spawn((Climate::default(),));
        let read = level_component(&mut world, r#"{"component":"climate","fields":{"humidity":0.9}}"#).unwrap();
        let read: serde_json::Value = serde_json::from_str(&read.unwrap()).unwrap();
        assert_eq!(read["humidity"].as_f64().map(|h| (h * 100.0).round()), Some(90.0));
        assert!(level_component(&mut world, r#"{"component":"weather"}"#).is_err());
    }

    #[test]
    fn the_whole_kind_reads_and_changes_each_component_by_its_number() {
        let mut world = World::new();
        let obj = world.spawn((voxear_scene::Transform::default(),));
        let has = |world: &mut World, which: i32| whole_values(world, obj, which << 2 | HAS, None).map(|v| v[0]);
        assert_eq!(has(&mut world, NAV_LINK), Some(0.0));
        whole_values(&mut world, obj, NAV_LINK << 2 | ADD, Some([0.0; 4])).unwrap();
        assert_eq!(has(&mut world, NAV_LINK), Some(1.0));
        assert_eq!(whole_values(&mut world, obj, LOD_GROUP << 2 | ADD, Some([0.0; 4])), None, "not added this way");
        whole_texts(&mut world, obj, NAV_LINK << 1 | CHANGE, r#"{"kind":"jump","cost":3}"#).unwrap();
        let link = *world.get::<NavLink>(obj).unwrap();
        assert_eq!((link.kind, link.cost, link.both_ways), (crate::navigation::NavLinkKind::Jump, 3.0, true));
        assert!(whole_texts(&mut world, obj, NAV_LINK << 1 | CHANGE, r#"{"kind":"fly"}"#).is_none());
        // A Ground Fit, added and changed a setting at a time, its blocks by their GUIDs.
        whole_values(&mut world, obj, GROUND_FIT << 2 | ADD, Some([0.0; 4])).unwrap();
        let fill = r#"{"mode":"cut","slope_out":0,"fill":"0123456789abcdef0123456789abcdef"}"#;
        whole_texts(&mut world, obj, GROUND_FIT << 1 | CHANGE, fill).unwrap();
        let fit = world.get::<GroundFit>(obj).unwrap().clone();
        assert_eq!((fit.mode, fit.slope_out, fit.margin), (crate::ground_fit::FitMode::Cut, 0, 1.0));
        assert!(fit.fill.is_some() && fit.top.is_none());
        // A Road and an Area Fill, the same way.
        whole_values(&mut world, obj, ROAD << 2 | ADD, Some([0.0; 4])).unwrap();
        let points =
            r#"{"points":[{"at":[0,0,0],"width":3},{"at":[0,0,9],"width":7}],"ground":"follow","bridge":true}"#;
        whole_texts(&mut world, obj, ROAD << 1 | CHANGE, points).unwrap();
        let road = world.get::<Road>(obj).unwrap().clone();
        assert_eq!((road.points[1].width, road.ground, road.bridge), (7.0, crate::roads::RoadGround::Follow, true));
        whole_values(&mut world, obj, AREA_FILL << 2 | ADD, Some([0.0; 4])).unwrap();
        whole_texts(&mut world, obj, AREA_FILL << 1 | CHANGE, r#"{"level":"level","pattern":["12"]}"#).unwrap();
        assert_eq!(world.get::<AreaFill>(obj).unwrap().level, crate::roads::AreaLevel::Level);
        // A Placement, a Prefab Brush and a Walkable, the same way.
        for which in [PLACEMENT, PREFAB_BRUSH, WALKABLE] {
            whole_values(&mut world, obj, which << 2 | ADD, Some([0.0; 4])).unwrap();
        }
        whole_texts(&mut world, obj, PLACEMENT << 1 | CHANGE, r#"{"size":[10,8],"front":90}"#).unwrap();
        assert_eq!(world.get::<Placement>(obj).unwrap().size, [10.0, 8.0]);
        whole_texts(&mut world, obj, PREFAB_BRUSH << 1 | CHANGE, r#"{"mode":"single","radius":3}"#).unwrap();
        assert_eq!(world.get::<PrefabBrush>(obj).unwrap().mode, crate::prefab_placing::BrushMode::Single);
        whole_texts(&mut world, obj, WALKABLE << 1 | CHANGE, r#"{"door":true}"#).unwrap();
        assert!(world.get::<Walkable>(obj).unwrap().door);

        let near = world.spawn((voxear_scene::Transform::default(),));
        world.insert(obj, LodGroup::of(&[near]));
        let read: serde_json::Value =
            serde_json::from_str(&whole_texts(&mut world, obj, LOD_GROUP << 1 | READ, "").unwrap()).unwrap();
        assert_eq!(read["levels"][0]["objs"][0].as_u64(), Some(near.to_bits()));
        let far = serde_json::json!({ "measure": "distance", "levels": [{ "objs": [near.to_bits()], "until": 40 }] });
        whole_texts(&mut world, obj, LOD_GROUP << 1 | CHANGE, &far.to_string()).unwrap();
        let group = world.get::<LodGroup>(obj).unwrap();
        assert_eq!((group.measure, group.levels[0].until, group.levels[0].objs[0]), (LodMeasure::Distance, 40.0, near));
    }
}
