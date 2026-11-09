//! Prefabs: Obj hierarchies saved as `.prefab` files, placed in scenes and
//! instantiated at runtime.
//!
//! A prefab file lists Objs exactly as a scene does, and has one root:
//!
//! ```toml
//! format = 1
//!
//! [[obj]]
//! id = 1
//! name = "Lamp"
//! behaviours = ["Game.Flicker"]
//! [obj.fields."Game.Flicker"]
//! speed = 2.0
//! [obj.renderer]
//! mesh = "builtin:sphere"
//!
//! [[obj]]
//! id = 2
//! name = "Glow"
//! parent = 1
//! light = { type = "point", color = [1.0, 0.8, 0.5] }
//! ```
//!
//! An Obj with `prefab` set, in a scene or in another prefab, is an instance.
//! The prefab's root takes the instance's id and parent, and whatever else
//! the instance gives: name, position, rotation, scale, camera, light,
//! renderer and `persistent`. Behaviours listed on the instance are added to
//! the root's, and `fields` on the instance set the root's one by one. `overrides`
//! changes Objs inside the prefab, addressed by their path from the instance,
//! and only the fields written change:
//!
//! ```toml
//! [[obj]]
//! id = 7
//! prefab = "Prefabs/Lamp.prefab"
//! position = [2.0, 0.0, 0.0]
//! [obj.overrides.2]
//! light = { type = "point", color = [0.4, 0.6, 1.0] }
//! ```
//!
//! Nesting needs nothing more: a prefab can hold instances of other prefabs,
//! whose Objs are addressed as `"<instance id>/<id>"`. Behaviour fields can
//! point at Objs in the same file the same way, as `{ obj = 2 }` or
//! `{ obj = "3/2" }`; each instance's references point at its own copies. A **variant** is a
//! prefab whose root is an instance of another prefab, with its overrides
//! and any children it adds, as in Unity.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{
    Camera, Foliage, FoliageInteraction, Light, MaterialHandle, MeshRenderer, ModelPart, ModelRenderer, Ocean, PostVolume, Renderer,
    ParticleSystem, Seasons, Stream, WindZone, DryArea, Pool,
};
use voxear_scene::{BehaviourSpec, Inactive, Name, OffComponents, Parent, Path, Placement, ScenePersistence, Transform};

use crate::{
    audio::{AudioListener, AudioSource, audio_source},
    content::{Content, MaterialSource},
    joints::JointData,
    lod_groups::LodGroupData,
    physics::{CharacterController, Collider, Rigidbody, Vehicle, Wheel},
    physics2d::{Collider2D, Rigidbody2D},
    scene_file::{
        FoliageData, FoliageSource, LayerRef, MaterialRef, ModelData, ModelSource, ObjData, PostVolumeData, TerrainData,
        TerrainSource, default_material, light_component, light_with_assets,
    },
};

pub const PREFAB_FORMAT: u32 = 1;

/// The mesh an Obj's mesh collider takes its shape from, kept where nothing
/// draws it, as on a server, so the collider has its triangles all the same.
#[derive(Clone, Debug, PartialEq)]
pub struct ColliderMeshSource(pub voxear_assets::AssetRef);

impl voxear_ecs::Component for ColliderMeshSource {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabFile {
    pub format: u32,
    #[serde(default, rename = "obj")]
    pub objs: Vec<ObjData>,
}

/// Where an Obj comes from: its id in the scene or prefab file, then its id
/// inside each prefab instance it is nested in. `5` is Obj 5 of the file;
/// `5/2` is Obj 2 of the prefab that Obj 5 is an instance of. The empty path
/// is a prefab's own root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjPath(Vec<u32>);

impl ObjPath {
    pub fn new(id: u32) -> Self {
        Self(vec![id])
    }

    /// `2`, `3/2`, or the empty string for a prefab's own root.
    pub fn parse(text: &str) -> Result<Self> {
        if text.is_empty() {
            return Ok(Self::default());
        }
        let ids = text.split('/').map(|id| id.trim().parse::<u32>()).collect::<Result<_, _>>();
        Ok(Self(ids.with_context(|| format!("`{text}` is not an Obj path such as `2` or `3/2`"))?))
    }

    pub fn ids(&self) -> &[u32] {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// `self` followed by `rest`.
    pub fn join(&self, rest: &ObjPath) -> Self {
        Self([self.0.as_slice(), rest.0.as_slice()].concat())
    }

    /// `self` relative to `base`, when it lies under it.
    pub fn strip(&self, base: &ObjPath) -> Option<Self> {
        self.0.strip_prefix(base.0.as_slice()).map(|rest| Self(rest.to_vec()))
    }
}

impl fmt::Display for ObjPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids: Vec<String> = self.0.iter().map(u32::to_string).collect();
        f.write_str(&ids.join("/"))
    }
}

/// One Obj after prefab instances are expanded.
#[derive(Clone, Debug)]
pub struct FlatObj {
    pub path: ObjPath,
    pub parent: Option<ObjPath>,
    /// Its settings with every override applied. `prefab` and `overrides`
    /// are always empty here.
    pub data: ObjData,
    /// Set on the root of each prefab instance.
    pub prefab: Option<AssetGuid>,
}

/// Expands every prefab instance in `objs`, recursively, in file order.
pub fn expand(content: &mut Content, objs: &[ObjData]) -> Result<Vec<FlatObj>> {
    let mut out = Vec::with_capacity(objs.len());
    expand_into(content, objs, &mut Vec::new(), &mut out)?;
    Ok(out)
}

/// A prefab's Objs, relative to its root (the empty path).
pub fn expand_prefab(content: &mut Content, guid: AssetGuid) -> Result<Vec<FlatObj>> {
    expand_prefab_inner(content, guid, &mut Vec::new())
}

fn expand_into(
    content: &mut Content,
    objs: &[ObjData],
    stack: &mut Vec<AssetGuid>,
    out: &mut Vec<FlatObj>,
) -> Result<()> {
    for obj in objs {
        let path = ObjPath::new(obj.id);
        let parent = obj.parent.map(ObjPath::new);
        let Some(reference) = &obj.prefab else {
            ensure!(obj.overrides.is_empty(), "Obj {} has overrides but no prefab", obj.id);
            out.push(FlatObj { path, parent, data: obj.clone(), prefab: None });
            continue;
        };
        let instance = expand_instance(content, obj, reference, &path, stack)
            .with_context(|| format!("Obj {} (an instance of {reference})", obj.id))?;
        out.extend(instance.into_iter().map(|flat| FlatObj {
            path: path.join(&flat.path),
            parent: match &flat.parent {
                Some(inner) => Some(path.join(inner)),
                None => parent.clone(),
            },
            ..flat
        }));
    }
    Ok(())
}

/// The instance's Objs with paths relative to it. Their Obj references are
/// already in the file's own terms, like those its overrides write.
fn expand_instance(
    content: &mut Content,
    instance: &ObjData,
    reference: &str,
    path: &ObjPath,
    stack: &mut Vec<AssetGuid>,
) -> Result<Vec<FlatObj>> {
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a prefab"))?;
    let mut objs = expand_prefab_inner(content, guid, stack)?;
    for obj in &mut objs {
        map_obj_refs(&mut obj.data, &|target| path.join(target))?;
    }
    for (key, changes) in &instance.overrides {
        let target = ObjPath::parse(key)?;
        ensure!(!target.is_root(), "set the root's fields on the instance itself, not in `overrides`");
        let obj = objs
            .iter_mut()
            .find(|o| o.path == target)
            .with_context(|| format!("override `{key}`: the prefab has no Obj at that path"))?;
        apply_override(&mut obj.data, changes).with_context(|| format!("override `{key}`"))?;
    }
    let root = objs.iter_mut().find(|o| o.path.is_root()).expect("a prefab always has its root");
    apply_instance(&mut root.data, instance);
    Ok(objs)
}

fn expand_prefab_inner(content: &mut Content, guid: AssetGuid, stack: &mut Vec<AssetGuid>) -> Result<Vec<FlatObj>> {
    let file = content.prefab_file(guid)?;
    let name = content.db().get(guid).map_or_else(|| guid.to_string(), |e| e.path.clone());
    ensure!(!stack.contains(&guid), "{name} contains an instance of itself");
    let mut roots = file.objs.iter().filter(|o| o.parent.is_none());
    let root = roots.next().with_context(|| format!("{name} has no root Obj"))?;
    ensure!(roots.next().is_none(), "{name} has more than one root Obj; a prefab has exactly one");

    stack.push(guid);
    let mut objs = Vec::with_capacity(file.objs.len());
    let expanded = expand_into(content, &file.objs, stack, &mut objs).with_context(|| format!("in {name}"));
    stack.pop();
    expanded?;

    // The root becomes the empty path, so paths inside read from the instance.
    let root_path = ObjPath::new(root.id);
    let rebase = |path: ObjPath| if path == root_path { ObjPath::default() } else { path };
    for obj in &mut objs {
        obj.path = rebase(std::mem::take(&mut obj.path));
        obj.parent = obj.parent.take().map(rebase);
        map_obj_refs(&mut obj.data, &|target| rebase(target.clone()))?;
    }
    let root = objs.iter_mut().find(|o| o.path.is_root()).expect("the root was expanded");
    root.prefab = Some(guid);
    Ok(objs)
}

/// `{ obj = 3 }` or `{ obj = "3/2" }` in a behaviour field: a reference to
/// an Obj in the same file. A `Bone` field's names one of its model's bones
/// too, `{ obj = 3, bone = "hand_r" }`, which stays with it wherever the
/// reference is rewritten.
pub(crate) fn obj_ref(table: &toml::Table) -> Result<Option<ObjPath>> {
    if table.len() != 1 && !(table.len() == 2 && bone_of(table).is_some()) {
        return Ok(None);
    }
    match table.get("obj") {
        Some(toml::Value::Integer(id)) => Ok(Some(ObjPath::new(u32::try_from(*id)?))),
        Some(toml::Value::String(path)) => ObjPath::parse(path).map(Some),
        Some(_) => bail!("`obj` takes an Obj id such as 3, or a path such as \"3/2\""),
        None => Ok(None),
    }
}

/// The bone a `Bone` field's reference names, `bone = "hand_r"`, if it is
/// one.
pub(crate) fn bone_of(table: &toml::Table) -> Option<&toml::Value> {
    table.get(BONE_KEY).filter(|bone| bone.is_str())
}

/// The key a `Bone` field's reference names its bone by, beside its Obj.
pub(crate) const BONE_KEY: &str = "bone";

/// How files write a reference: a plain id when it can be one.
pub(crate) fn ref_value(path: &ObjPath) -> toml::Value {
    match path.ids() {
        [id] => toml::Value::Integer(i64::from(*id)),
        _ => toml::Value::String(path.to_string()),
    }
}

/// Rewrites every Obj reference an Obj holds: in its behaviours' fields,
/// its LOD Group's levels, and its joint.
pub(crate) fn map_obj_refs(data: &mut ObjData, f: &dyn Fn(&ObjPath) -> ObjPath) -> Result<()> {
    map_refs(&mut data.fields, f)?;
    if let Some(joint) = &mut data.joint {
        joint.map_refs(f)?;
    }
    match &mut data.lod_group {
        Some(lod) => lod.map_refs(f),
        None => Ok(()),
    }
}

/// Rewrites every Obj reference in behaviour fields.
pub(crate) fn map_refs(fields: &mut BTreeMap<String, toml::Table>, f: &dyn Fn(&ObjPath) -> ObjPath) -> Result<()> {
    for table in fields.iter_mut().map(|(_, v)| v) {
        for value in table.iter_mut().map(|(_, v)| v) {
            map_value(value, f)?;
        }
    }
    Ok(())
}

fn map_value(value: &mut toml::Value, f: &dyn Fn(&ObjPath) -> ObjPath) -> Result<()> {
    match value {
        toml::Value::Table(table) => match obj_ref(table)? {
            Some(path) => {
                table.insert("obj".into(), ref_value(&f(&path)));
            }
            None => table.iter_mut().map(|(_, v)| v).try_for_each(|v| map_value(v, f))?,
        },
        toml::Value::Array(items) => items.iter_mut().try_for_each(|v| map_value(v, f))?,
        _ => {}
    }
    Ok(())
}

/// Every Obj a value refers to.
fn collect_refs(value: &toml::Value, out: &mut Vec<ObjPath>) -> Result<()> {
    match value {
        toml::Value::Table(table) => match obj_ref(table)? {
            Some(path) => out.push(path),
            None => table.values().try_for_each(|v| collect_refs(v, out))?,
        },
        toml::Value::Array(items) => items.iter().try_for_each(|v| collect_refs(v, out))?,
        _ => {}
    }
    Ok(())
}

/// Fields that place an Obj in its file, which overrides cannot change.
const FIXED_FIELDS: [&str; 4] = ["id", "parent", "prefab", "overrides"];

/// Layers `changes` onto an Obj's fields, as a `[post]` section layers onto
/// its preset: tables merge, and every other value replaces the old one.
pub fn apply_override(data: &mut ObjData, changes: &toml::Table) -> Result<()> {
    if let Some(field) = FIXED_FIELDS.iter().find(|f| changes.contains_key(**f)) {
        bail!("`{field}` cannot be overridden");
    }
    let mut table = toml::Table::try_from(&*data)?;
    merge(&mut table, changes.clone());
    *data = toml::Value::Table(table).try_into()?;
    Ok(())
}

fn merge(base: &mut toml::Table, changes: toml::Table) {
    for (key, value) in changes {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(value)) => merge(base, value),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// What an instance's own fields do to the prefab's root.
fn apply_instance(root: &mut ObjData, instance: &ObjData) {
    root.id = instance.id;
    root.parent = instance.parent;
    if !instance.name.is_empty() {
        root.name = instance.name.clone();
    }
    root.position = instance.position.or(root.position);
    root.rotation = instance.rotation.or(root.rotation);
    root.scale = instance.scale.or(root.scale);
    root.persistent |= instance.persistent;
    root.active = instance.active.or(root.active);
    root.behaviours.extend(instance.behaviours.iter().cloned());
    root.enabled.extend(instance.enabled.iter().map(|(name, on)| (name.clone(), *on)));
    root.components.extend(instance.components.iter().cloned());
    for (behaviour, values) in &instance.fields {
        merge(root.fields.entry(behaviour.clone()).or_default(), values.clone());
    }
    if instance.camera.is_some() {
        root.camera = instance.camera;
    }
    if instance.light.is_some() {
        root.light = instance.light.clone();
    }
    if instance.post_volume.is_some() {
        root.post_volume = instance.post_volume.clone();
    }
    if instance.renderer.is_some() {
        root.renderer = instance.renderer.clone();
    }
    if instance.audio.is_some() {
        root.audio = instance.audio.clone();
    }
    root.audio_listener |= instance.audio_listener;
    if instance.ui_panel.is_some() {
        root.ui_panel = instance.ui_panel.clone();
    }
    if instance.lod_group.is_some() {
        root.lod_group = instance.lod_group.clone();
    }
    if instance.layer.is_some() {
        root.layer = instance.layer.clone();
    }
    root.collider = instance.collider.or(root.collider);
    root.wind_zone = instance.wind_zone.or(root.wind_zone);
    root.particles = instance.particles.or(root.particles);
    root.smoke = instance.smoke.or(root.smoke);
    root.mist = instance.mist.or(root.mist);
    root.net = instance.net.or(root.net);
    root.animator = instance.animator.clone().or(root.animator.clone());
    root.motion_matcher = instance.motion_matcher.clone().or(root.motion_matcher.clone());
    root.bone_attachment = instance.bone_attachment.clone().or(root.bone_attachment.clone());
    root.first_person = instance.first_person.clone().or(root.first_person.clone());
    root.gun = instance.gun.clone().or(root.gun.clone());
    root.gun_holder = instance.gun_holder.clone().or(root.gun_holder.clone());
    root.gun_part = instance.gun_part.clone().or(root.gun_part.clone());
    root.ocean = instance.ocean.or(root.ocean);
    root.pool = instance.pool.or(root.pool);
    if instance.dry_area.is_some() {
        root.dry_area.clone_from(&instance.dry_area);
    }
    if instance.path.is_some() {
        root.path.clone_from(&instance.path);
    }
    root.stream = instance.stream.or(root.stream);
    root.buoyancy = instance.buoyancy.or(root.buoyancy);
    if instance.model.is_some() {
        root.model.clone_from(&instance.model);
    }
    root.seasons = instance.seasons.or(root.seasons);
    root.foliage_interaction = instance.foliage_interaction.or(root.foliage_interaction);
    root.rigidbody = instance.rigidbody.or(root.rigidbody);
    root.character = instance.character.or(root.character);
    if instance.joint.is_some() {
        root.joint = instance.joint.clone();
    }
    root.collider2d = instance.collider2d.or(root.collider2d);
    root.rigidbody2d = instance.rigidbody2d.or(root.rigidbody2d);
    root.vehicle = instance.vehicle.or(root.vehicle);
    root.wheel = instance.wheel.or(root.wheel);
}

/// Expanded Objs with their meshes and materials loaded, ready to spawn any
/// number of times without going back to content.
#[derive(Debug, Default)]
pub struct Template {
    pub objs: Vec<TemplateObj>,
    /// Each Obj's index by path, for resolving references between them.
    paths: HashMap<ObjPath, usize>,
}

#[derive(Clone, Debug)]
pub struct TemplateObj {
    pub path: ObjPath,
    /// The parent's index in the template. Roots have none.
    pub parent: Option<usize>,
    pub name: String,
    pub transform: Transform,
    pub persistent: bool,
    /// Its own switch, and the components it switches off.
    pub active: bool,
    pub off: OffComponents,
    pub behaviours: Vec<BehaviourSpec>,
    pub camera: Option<Camera>,
    pub light: Option<Light>,
    pub post_volume: Option<PostVolume>,
    pub renderer: Option<MeshRenderer>,
    /// The mesh a mesh collider takes its shape from, where there is no
    /// renderer to draw it, as on a server.
    pub collider_mesh: Option<ColliderMeshSource>,
    pub audio: Option<AudioSource>,
    pub audio_listener: bool,
    pub ui_panel: Option<voxear_render::UiPanel>,
    pub collider: Option<Collider>,
    pub rigidbody: Option<Rigidbody>,
    pub character: Option<CharacterController>,
    pub wind_zone: Option<WindZone>,
    pub particles: Option<ParticleSystem>,
    pub smoke: Option<voxear_render::Smoke>,
    pub mist: Option<voxear_render::Mist>,
    pub net: Option<crate::net::NetObject>,
    pub animator: Option<crate::animation::Animator>,
    pub motion_matcher: Option<crate::animation::MotionMatcher>,
    pub bone_attachment: Option<crate::animation::BoneAttachment>,
    pub first_person: Option<crate::first_person::FirstPersonView>,
    pub gun: Option<crate::guns::Gun>,
    pub gun_holder: Option<crate::guns::GunHolder>,
    pub gun_part: Option<crate::guns::mechanism::GunPart>,
    pub ocean: Option<Ocean>,
    pub pool: Option<Pool>,
    pub dry_area: Option<DryArea>,
    /// A Path component (`path` is where the Obj is in the prefab).
    pub curve: Option<Path>,
    pub stream: Option<Stream>,
    pub buoyancy: Option<crate::water::Buoyancy>,
    pub seasons: Option<Seasons>,
    pub foliage_interaction: Option<FoliageInteraction>,
    /// A Foliage, where there is a renderer to draw it, and how it was
    /// written, which few Objs have.
    pub foliage: Option<Box<(Option<Foliage>, FoliageSource)>>,
    /// A Terrain, which few Objs have.
    pub terrain: Option<Box<TerrainParts>>,
    /// A whole model it draws, where there is a renderer to draw it, and how
    /// it was written.
    pub model: Option<Box<(Option<ModelRenderer>, ModelSource)>>,
    /// Its layer by name, found in the world it is spawned into.
    pub layer: Option<LayerRef>,
    /// Its connected Obj by path, bound to an entity when spawned.
    pub joint: Option<JointData>,
    pub collider2d: Option<Collider2D>,
    pub rigidbody2d: Option<Rigidbody2D>,
    pub vehicle: Option<Vehicle>,
    pub wheel: Option<Wheel>,
    /// Its levels by path, bound to entities when spawned.
    pub lod_group: Option<LodGroupData>,
    /// Set on the root of each prefab instance.
    pub prefab: Option<AssetGuid>,
    /// Components the prefab locks (see [`Locks`]).
    pub locks: BTreeMap<String, Vec<String>>,
}

/// Components a prefab locks, as its file's `locks` says: by component (a
/// behaviour's type, or `renderer`, `light`, `camera` or `audio`), each with the
/// fields designers still see in the Inspector. Only the prefab, opened on
/// its own, changes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Locks(pub BTreeMap<String, Vec<String>>);
impl Component for Locks {}

/// Marks the root of a prefab instance, so scenes save it as an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefabInstance(pub AssetGuid);
impl Component for PrefabInstance {}

/// Marks an Obj that came from a prefab instance: the instance's root, and
/// the Obj's path inside it. Nested instances are folded into the outermost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FromPrefab {
    pub instance: Entity,
    pub path: ObjPath,
}
impl Component for FromPrefab {}

/// Marks an Obj that a script made from a prefab while the game ran: the
/// prefab, the root of the copy it is part of, and its place in the
/// prefab's order, so a save of the scene can make the copy again and find
/// each of its Objs in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawned {
    pub prefab: AssetGuid,
    pub root: Entity,
    pub index: u32,
}
impl Component for Spawned {}

impl Template {
    /// The index of the first root, which is a prefab's only one.
    pub fn root(&self) -> Option<usize> {
        self.objs.iter().position(|o| o.parent.is_none())
    }

    /// Loads what the Objs need. Without a renderer (headless runs) meshes
    /// are left out.
    pub fn resolve(content: &mut Content, mut renderer: Option<&mut Renderer>, flat: &[FlatObj]) -> Result<Self> {
        let mut index = HashMap::with_capacity(flat.len());
        for (i, obj) in flat.iter().enumerate() {
            if index.insert(&obj.path, i).is_some() {
                bail!("two Objs have id {}", obj.path);
            }
        }
        let mut objs = Vec::with_capacity(flat.len());
        for obj in flat {
            let parent = match &obj.parent {
                Some(parent) => Some(*index.get(parent).with_context(|| {
                    format!("{} has parent {parent}, which is not in the same file", obj.data.name)
                })?),
                None => None,
            };
            let resolved = resolve_obj(content, renderer.as_deref_mut(), obj, parent)
                .and_then(|resolved| check_refs(obj, &index).map(|()| resolved))
                .with_context(|| format!("Obj {} ({})", obj.path, obj.data.name))?;
            objs.push(resolved);
        }
        let paths = objs.iter().enumerate().map(|(i, o)| (o.path.clone(), i)).collect();
        Ok(Self { objs, paths })
    }

    /// The behaviours to create on the Obj at `index`, with its Obj
    /// references pointing at the entities just spawned, as `{ entity = n }`.
    pub fn behaviours(&self, index: usize, entities: &[Entity]) -> Vec<BehaviourSpec> {
        let lookup = |path: &ObjPath| self.paths.get(path).map(|&i| entities[i]);
        self.objs[index]
            .behaviours
            .iter()
            .map(|spec| match bind_refs(&spec.fields, &lookup) {
                Some(fields) => BehaviourSpec { fields: Arc::new(fields), ..spec.clone() },
                None => spec.clone(),
            })
            .collect()
    }

    /// Creates the Objs' entities and components, but not their behaviours.
    /// Roots go under `parent`, and take `placement`, relative to it, when it
    /// is given. Returns the entities in template order.
    pub fn spawn_objs(&self, world: &mut World, parent: Option<Entity>, placement: Option<Placement>) -> Vec<Entity> {
        let mut entities = Vec::with_capacity(self.objs.len());
        for obj in &self.objs {
            let transform = match (obj.parent, placement) {
                (None, Some(p)) => Transform::new(p.position, p.rotation, obj.transform.scale),
                _ => obj.transform,
            };
            let entity = world.spawn((Name(obj.name.clone()), transform));
            if let Some(camera) = obj.camera {
                world.insert(entity, camera);
            }
            if let Some(light) = obj.light {
                world.insert(entity, light);
            }
            if let Some(volume) = &obj.post_volume {
                world.insert(entity, volume.clone());
            }
            if let Some(renderer) = obj.renderer {
                world.insert(entity, renderer);
            }
            if let Some(source) = obj.collider_mesh.clone() {
                world.insert(entity, source);
            }
            if let Some(audio) = obj.audio {
                world.insert(entity, audio);
            }
            if let Some(panel) = obj.ui_panel {
                world.insert(entity, panel);
            }
            if let Some(layer) = obj.layer.as_ref().map(|layer| layer.layer(world)).filter(|layer| layer.0 != 0) {
                world.insert(entity, layer);
            }
            if let Some(collider) = obj.collider {
                world.insert(entity, collider);
            }
            if let Some(wind) = obj.wind_zone {
                world.insert(entity, wind);
            }
            if let Some(particles) = obj.particles {
                world.insert(entity, particles);
            }
            if let Some(smoke) = obj.smoke {
                world.insert(entity, smoke);
            }
            if let Some(mist) = obj.mist {
                world.insert(entity, mist);
            }
            if let Some(net) = obj.net {
                world.insert(entity, net);
            }
            if let Some(animator) = obj.animator.clone() {
                world.insert(entity, animator);
            }
            if let Some(matcher) = obj.motion_matcher.clone() {
                world.insert(entity, matcher);
            }
            if let Some(attachment) = obj.bone_attachment.clone() {
                world.insert(entity, attachment);
            }
            if let Some(view) = obj.first_person.clone() {
                world.insert(entity, view);
            }
            if let Some(gun) = obj.gun.clone() {
                world.insert(entity, gun);
            }
            if let Some(holder) = obj.gun_holder.clone() {
                world.insert(entity, holder);
            }
            if let Some(part) = obj.gun_part.clone() {
                world.insert(entity, part);
            }
            if let Some(ocean) = obj.ocean {
                world.insert(entity, ocean);
            }
            if let Some(pool) = obj.pool {
                world.insert(entity, pool);
            }
            if let Some(dry) = obj.dry_area {
                world.insert(entity, dry);
            }
            if let Some(path) = obj.curve.clone() {
                world.insert(entity, path);
            }
            if let Some(stream) = obj.stream {
                world.insert(entity, stream);
            }
            if let Some(buoyancy) = obj.buoyancy {
                world.insert(entity, buoyancy);
            }
            if let Some(seasons) = obj.seasons {
                world.insert(entity, seasons);
            }
            if let Some(push) = obj.foliage_interaction {
                world.insert(entity, push);
            }
            if let Some(foliage) = &obj.foliage {
                if let Some(drawn) = &foliage.0 {
                    world.insert(entity, drawn.clone());
                }
                world.insert(entity, foliage.1.clone());
            }
            if let Some(model) = &obj.model {
                if let Some(drawn) = &model.0 {
                    world.insert(entity, drawn.clone());
                }
                world.insert(entity, model.1.clone());
            }
            if let Some(parts) = &obj.terrain {
                world.insert(entity, parts.terrain.clone());
                if let Some(look) = &parts.look {
                    world.insert(entity, look.clone());
                }
                if let Some(asset) = &parts.asset {
                    world.insert(entity, asset.clone());
                }
                world.insert(entity, parts.source.clone());
            }
            if let Some(body) = obj.rigidbody {
                world.insert(entity, body);
            }
            if let Some(character) = obj.character {
                world.insert(entity, character);
            }
            if let Some(collider) = obj.collider2d {
                world.insert(entity, collider);
            }
            if let Some(body) = obj.rigidbody2d {
                world.insert(entity, body);
            }
            if let Some(vehicle) = obj.vehicle {
                world.insert(entity, vehicle);
            }
            if let Some(wheel) = obj.wheel {
                world.insert(entity, wheel);
            }
            if obj.audio_listener {
                world.insert(entity, AudioListener);
            }
            if obj.persistent {
                world.insert(entity, ScenePersistence);
            }
            if !obj.active {
                world.insert(entity, Inactive);
            }
            if !obj.off.is_empty() {
                world.insert(entity, obj.off);
            }
            if let Some(prefab) = obj.prefab {
                world.insert(entity, PrefabInstance(prefab));
            }
            if !obj.locks.is_empty() {
                world.insert(entity, Locks(obj.locks.clone()));
            }
            entities.push(entity);
        }
        let lookup = |path: &ObjPath| self.paths.get(path).map(|&i| entities[i]);
        for (obj, &entity) in self.objs.iter().zip(&entities) {
            if let Some(parent) = obj.parent.map(|index| entities[index]).or(parent) {
                world.insert(entity, Parent(parent));
            }
            if let Some(lod) = &obj.lod_group {
                world.insert(entity, lod.bind(&lookup));
            }
            if let Some(joint) = &obj.joint {
                world.insert(entity, joint.bind(&lookup));
            }
        }
        link_instances(world, &self.objs, &entities);
        entities
    }
}

/// References must stay inside the file, since nothing else is guaranteed to
/// be loaded alongside it (as in Unity, where they cannot cross scenes).
fn check_refs(obj: &FlatObj, index: &HashMap<&ObjPath, usize>) -> Result<()> {
    for (behaviour, table) in &obj.data.fields {
        let mut refs = Vec::new();
        table.values().try_for_each(|v| collect_refs(v, &mut refs))?;
        if let Some(missing) = refs.iter().find(|r| !index.contains_key(r)) {
            bail!("a field of {behaviour} refers to Obj {missing}, which is not in the same file");
        }
    }
    let levels = obj.data.lod_group.as_ref().map(LodGroupData::refs).transpose()?.unwrap_or_default();
    if let Some(missing) = levels.iter().find(|r| !index.contains_key(r)) {
        bail!("a level of its LOD Group is Obj {missing}, which is not in the same file");
    }
    let connected = obj.data.joint.as_ref().map(JointData::refs).transpose()?.unwrap_or_default();
    if let Some(missing) = connected.iter().find(|r| !index.contains_key(r)) {
        bail!("its joint is held to Obj {missing}, which is not in the same file");
    }
    Ok(())
}

/// A copy of `fields` with each reference replaced by its entity, or `None`
/// when there are no references, so most behaviours share their fields.
pub(crate) fn bind_refs(fields: &toml::Table, lookup: &dyn Fn(&ObjPath) -> Option<Entity>) -> Option<toml::Table> {
    let mut refs = Vec::new();
    fields.values().try_for_each(|v| collect_refs(v, &mut refs)).ok()?;
    if refs.is_empty() {
        return None;
    }
    let mut bound = fields.clone();
    bound.iter_mut().map(|(_, v)| v).for_each(|v| bind_value(v, lookup));
    Some(bound)
}

fn bind_value(value: &mut toml::Value, lookup: &dyn Fn(&ObjPath) -> Option<Entity>) {
    match value {
        toml::Value::Table(table) => match obj_ref(table).ok().flatten() {
            Some(path) => {
                let entity = lookup(&path).map_or(0, Entity::to_bits);
                let bone = bone_of(table).cloned();
                // Entity bits go through TOML's i64 unchanged.
                *table = toml::Table::from_iter([("entity".to_string(), toml::Value::Integer(entity as i64))]);
                table.extend(bone.map(|bone| (BONE_KEY.to_string(), bone)));
            }
            None => table.iter_mut().map(|(_, v)| v).for_each(|v| bind_value(v, lookup)),
        },
        toml::Value::Array(items) => items.iter_mut().for_each(|v| bind_value(v, lookup)),
        _ => {}
    }
}

/// Gives every Obj inside a prefab instance a [`FromPrefab`] pointing at the
/// outermost instance's root.
fn link_instances(world: &mut World, objs: &[TemplateObj], entities: &[Entity]) {
    let roots: Vec<(usize, &ObjPath)> =
        objs.iter().enumerate().filter(|(_, o)| o.prefab.is_some()).map(|(i, o)| (i, &o.path)).collect();
    for (obj, &entity) in objs.iter().zip(entities) {
        // The shortest instance path that contains this Obj is the outermost.
        let outermost = roots
            .iter()
            .filter_map(|&(i, root)| obj.path.strip(root).map(|rest| (root.ids().len(), i, rest)))
            .min_by_key(|(depth, ..)| *depth);
        if let Some((_, root, path)) = outermost {
            world.insert(entity, FromPrefab { instance: entities[root], path });
        }
    }
}

/// A Foliage as a scene writes it, its mesh and material loaded and its
/// plants placed and scattered, and how it was written, for saving.
pub fn foliage(content: &mut Content, renderer: &mut Renderer, data: &FoliageData) -> Result<(Foliage, FoliageSource)> {
    let mesh = content.mesh(renderer, &content.parse(&data.mesh)?)?;
    let material = material_of(content, renderer, data.material.as_ref())?;
    let foliage = Foliage {
        mesh,
        material,
        plants: std::sync::Arc::new(data.plants()),
        draw_distance: data.draw_distance.max(0.0),
        fade_distance: data.fade_distance.max(0.0),
        shadow_distance: data.shadow_distance.max(0.0),
        healthy_color: data.healthy_color,
        dry_color: data.dry_color,
        color_spread: data.color_spread.max(0.1),
        impostors: data.impostors,
        impostor_distance: data.impostor_distance.max(0.0),
    };
    Ok((foliage, FoliageSource(data.clone())))
}

/// What a Terrain is made of: its ground, its look where there is a
/// renderer to draw it, its asset where it has one, and how it was
/// written, for saving.
#[derive(Clone, Debug)]
pub struct TerrainParts {
    pub terrain: voxear_terrain::Terrain,
    pub look: Option<voxear_render::TerrainLook>,
    pub asset: Option<crate::terrain::TerrainAsset>,
    pub source: TerrainSource,
}

/// A Terrain as a scene writes it: its asset's heights, read with or
/// without a renderer, as physics stands things on them; its look, where
/// there is a renderer to draw it. A terrain whose asset cannot be read is
/// flat, as large as a new one.
pub fn terrain(content: &mut Content, renderer: Option<&mut Renderer>, data: &TerrainData) -> Result<TerrainParts> {
    let guid = content.parse(&data.data).ok().and_then(|reference| reference.guid());
    let loaded = guid.and_then(|guid| Some((guid, content.terrain(guid)?)));
    let (heights, weights, asset) = match loaded {
        Some((guid, loaded)) => {
            let asset = crate::terrain::TerrainAsset { guid, file: loaded.file.clone(), rasters: loaded.rasters.clone() };
            (loaded.heights.clone(), loaded.weights.clone(), Some(asset))
        }
        None => {
            if !data.data.is_empty() {
                tracing::warn!("the terrain {} could not be read; it is flat", data.data);
            }
            (std::sync::Arc::new(voxear_terrain::file::TerrainFile::default().flat()), None, None)
        }
    };
    let terrain = voxear_terrain::Terrain {
        heights,
        weights,
        detail_distance: data.detail_distance.max(1.0),
        cast_shadows: data.cast_shadows,
    };
    let look = match renderer {
        Some(renderer) => Some(terrain_look(content, renderer, data, asset.as_ref().map(|asset| &asset.file))?),
        None => None,
    };
    Ok(TerrainParts { terrain, look, asset, source: TerrainSource(data.clone()) })
}

/// How a Terrain looks: the material its scene gives it, and its asset's
/// surfaces, each with its material loaded.
pub fn terrain_look(
    content: &mut Content,
    renderer: &mut Renderer,
    data: &TerrainData,
    file: Option<&voxear_terrain::file::TerrainFile>,
) -> Result<voxear_render::TerrainLook> {
    let material = material_of(content, renderer, data.material.as_ref())?;
    let mut surfaces = Vec::new();
    for surface in file.map_or(&[][..], |file| &file.surfaces) {
        surfaces.push(surface_look(content, renderer, surface)?);
    }
    Ok(voxear_render::TerrainLook { material, texture_size: data.texture_size.max(0.01), surfaces: surfaces.into() })
}

/// How one of a terrain's surfaces looks, its material loaded: a plain one
/// where it names none or its material cannot be read.
pub fn surface_look(
    content: &mut Content,
    renderer: &mut Renderer,
    surface: &voxear_terrain::surfaces::Surface,
) -> Result<voxear_render::SurfaceLook> {
    let named = surface.material.as_ref().map(|reference| MaterialRef::Asset(reference.clone()));
    let material = match material_of(content, renderer, named.as_ref()) {
        Ok(material) => material,
        Err(error) => {
            tracing::warn!("a terrain surface's material could not be read: {error:#}");
            material_of(content, renderer, None)?
        }
    };
    Ok(voxear_render::SurfaceLook {
        material,
        size: surface.size.max(0.01),
        sharpness: surface.sharpness,
        triplanar: surface.triplanar,
    })
}

/// A Model Renderer drawing every part of the model `data` names, loading
/// it, each part where the model puts it, as `data` changes it.
pub fn model_renderer(content: &mut Content, renderer: &mut Renderer, data: &ModelData) -> Result<ModelRenderer> {
    let guid = content.parse(&data.model)?.guid().with_context(|| format!("{} is not a model asset", data.model))?;
    let nodes = content.model(renderer, guid)?.to_vec();
    let parts = model_parts(content, renderer, &nodes, data)?;
    let animation = content.model_animation(guid);
    Ok(ModelRenderer { lod_bias: data.lod_bias, animation, ..ModelRenderer::new(parts) })
}

/// A Model Renderer with no parts to draw but the model's skeleton and
/// clips, read without the GPU: `None` for a model without any.
fn animated(content: &mut Content, data: &ModelData) -> Option<ModelRenderer> {
    let guid = content.parse(&data.model).ok()?.guid()?;
    let animation = content.clip_file(guid)?;
    Some(ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) })
}

/// A loaded model's parts as a Model Renderer draws them, as `data`
/// changes them: moved, drawn with another material, or left out where an
/// Obj of their own draws them.
pub fn model_parts(
    content: &mut Content,
    renderer: &mut Renderer,
    nodes: &[voxear_render::ImportedNode],
    data: &ModelData,
) -> Result<Vec<ModelPart>> {
    let mut parts = Vec::with_capacity(nodes.len());
    for (node, part) in (0u32..).zip(nodes) {
        let changed = data.part(node);
        if changed.is_some_and(|changed| changed.apart) {
            continue;
        }
        let placed = (part.position, part.rotation, part.scale);
        let (position, rotation, scale) = changed.map_or(placed, |changed| changed.transform(placed));
        let material = match changed.and_then(|changed| changed.material.as_deref()) {
            Some(reference) => content.material(renderer, &MaterialSource::Asset(content.parse(reference)?))?,
            None => part.material,
        };
        let transform = voxear_core::math::Mat4::from_scale_rotation_translation(scale, rotation, position);
        let skin = part.skin.map(|skin| skin as u32);
        parts.push(ModelPart { mesh: part.mesh, material, transform, node, skin });
    }
    Ok(parts)
}

/// The material a Mesh Renderer or a Foliage names, or the default one.
fn material_of(content: &mut Content, renderer: &mut Renderer, named: Option<&MaterialRef>) -> Result<MaterialHandle> {
    let source = match named {
        Some(MaterialRef::Asset(reference)) => MaterialSource::Asset(content.parse(reference)?),
        Some(MaterialRef::Inline(material)) => MaterialSource::Inline(material.clone()),
        None => MaterialSource::Inline(default_material()),
    };
    content.material(renderer, &source)
}

pub(crate) fn resolve_obj(
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    obj: &FlatObj,
    parent: Option<usize>,
) -> Result<TemplateObj> {
    let data = &obj.data;
    let [x, y, z] = data.rotation().map(f32::to_radians);
    // Unity applies Z, then X, then Y: R = Ry · Rx · Rz.
    let rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
    let mut gpu = renderer;
    // Without a renderer, as when only checking a scene, a light goes without its cookie and profile.
    let light = match (&data.light, gpu.as_deref_mut()) {
        (Some(light), Some(renderer)) => Some(light_with_assets(content, renderer, light)?),
        (Some(light), None) => Some(light_component(light)),
        (None, _) => None,
    };
    let renderer = match (&data.renderer, gpu.as_deref_mut()) {
        (Some(data), Some(renderer)) => {
            let mesh = content.mesh(renderer, &content.parse(&data.mesh)?)?;
            let material = material_of(content, renderer, data.material.as_ref())?;
            Some(MeshRenderer {
                lod_bias: data.lod_bias,
                forced_lod: data.forced_lod(),
                lod_transitions: data.transitions(),
                ..MeshRenderer::new(mesh, material)
            })
        }
        _ => None,
    };
    // Without a renderer, as on a server, a mesh collider still needs its mesh's shape.
    let collider_mesh = match (&data.renderer, &renderer, &data.collider) {
        (Some(drawn), None, Some(collider)) if collider.shape == crate::physics::ColliderShape::Mesh => {
            Some(ColliderMeshSource(content.parse(&drawn.mesh)?))
        }
        _ => None,
    };
    // Without a renderer, as when only checking a scene, foliage is not drawn, as meshes are not, but is kept to save.
    let foliage = match (&data.foliage, gpu.as_deref_mut()) {
        (Some(data), Some(renderer)) => {
            let (foliage, source) = foliage(content, renderer, data)?;
            Some(Box::new((Some(foliage), source)))
        }
        (Some(data), None) => Some(Box::new((None, FoliageSource(data.clone())))),
        (None, _) => None,
    };
    // A terrain's heights are read with or without a renderer: physics stands things on them.
    let terrain = match &data.terrain {
        Some(written) => Some(Box::new(terrain(content, gpu.as_deref_mut(), written)?)),
        None => None,
    };
    // A mesh Dry Area's mesh needs a renderer to load it, and its box for scripts' questions; without one it keeps
    // to the Obj's own.
    let dry_area = match &data.dry_area {
        Some(dry) => {
            let mesh = match (&dry.mesh, gpu.as_deref_mut()) {
                (Some(mesh), Some(renderer)) => Some(content.mesh(renderer, &content.parse(mesh)?)?),
                _ => None,
            };
            let shaped = mesh.or(renderer.as_ref().map(|drawn| drawn.mesh));
            let bounds = match (shaped, gpu.as_deref()) {
                (Some(shaped), Some(gpu)) => gpu.mesh_bounds(shaped).map(|(low, high)| [low, high]),
                _ => None,
            };
            Some(DryArea { shape: dry.shape, size: dry.size, mesh, bounds, bilge: dry.bilge })
        }
        None => None,
    };
    // Without a GPU nothing is drawn, but a model something animates still has its skeleton and clips, for a server's
    // characters to move, pose and pass their actions' marks. Reading a model is costly, so only those are read.
    let animates = data.animator.is_some() || data.motion_matcher.is_some();
    let model = match (&data.model, gpu) {
        (Some(data), Some(renderer)) => Some(Box::new((Some(model_renderer(content, renderer, data)?), ModelSource(data.clone())))),
        (Some(data), None) if animates => Some(Box::new((animated(content, data), ModelSource(data.clone())))),
        (Some(data), None) => Some(Box::new((None, ModelSource(data.clone())))),
        (None, _) => None,
    };
    Ok(TemplateObj {
        path: obj.path.clone(),
        foliage,
        terrain,
        model,
        parent,
        name: if data.name.is_empty() { "Obj".to_string() } else { data.name.clone() },
        transform: Transform::new(Vec3::from(data.position()), rotation, Vec3::from(data.scale())),
        persistent: data.persistent,
        active: data.is_active(),
        off: data.off_components(),
        behaviours: behaviour_specs(data)?,
        camera: data.camera.map(|c| Camera { fov_y_degrees: c.fov, exposure: c.exposure, near: c.near }),
        light,
        post_volume: data.post_volume.as_ref().map(PostVolumeData::component).transpose()?,
        renderer,
        collider_mesh,
        audio: data.audio.as_ref().map(|audio| audio_source(content, audio)).transpose()?,
        audio_listener: data.audio_listener,
        ui_panel: data.ui_panel.as_ref().map(|panel| crate::ui_panels::ui_panel(content, panel)).transpose()?,
        collider: data.collider,
        rigidbody: data.rigidbody,
        character: data.character,
        wind_zone: data.wind_zone,
        particles: data.particles,
        smoke: data.smoke,
        mist: data.mist,
        net: data.net,
        animator: data.animator.clone(),
        motion_matcher: data.motion_matcher.clone(),
        bone_attachment: data.bone_attachment.clone(),
        first_person: data.first_person.clone(),
        gun: data.gun.clone(),
        gun_holder: data.gun_holder.clone(),
        gun_part: data.gun_part.clone(),
        ocean: data.ocean,
        pool: data.pool,
        dry_area,
        curve: data.path.clone(),
        stream: data.stream,
        buoyancy: data.buoyancy,
        seasons: data.seasons,
        foliage_interaction: data.foliage_interaction,
        layer: data.layer.clone(),
        joint: data.joint.clone(),
        collider2d: data.collider2d,
        rigidbody2d: data.rigidbody2d,
        vehicle: data.vehicle,
        wheel: data.wheel,
        lod_group: data.lod_group.clone(),
        prefab: obj.prefab,
        locks: data.locks.clone(),
    })
}

/// Pairs each behaviour, then each C# component, with its field values,
/// and each behaviour with whether it starts enabled. Values for a script
/// the Obj does not have are an error, since they would otherwise vanish
/// unnoticed.
fn behaviour_specs(data: &ObjData) -> Result<Vec<BehaviourSpec>> {
    let (behaviours, components, fields) = (&data.behaviours, &data.components, &data.fields);
    if let Some(stray) = fields.keys().find(|t| !behaviours.contains(t) && !components.contains(t)) {
        bail!("it has fields for {stray}, which is not one of its behaviours or components");
    }
    let spec = |type_name: &String, component: bool| BehaviourSpec {
        type_name: type_name.clone(),
        fields: Arc::new(fields.get(type_name).cloned().unwrap_or_default()),
        component,
        enabled: component || data.is_enabled(type_name),
    };
    Ok(behaviours.iter().map(|t| spec(t, false)).chain(components.iter().map(|t| spec(t, true))).collect())
}

/// A loaded prefab, by the id scripts hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrefabId(pub u32);

/// Loaded prefab files and templates, kept by [`Content`].
#[derive(Default)]
pub(crate) struct PrefabCache {
    pub files: HashMap<AssetGuid, Arc<PrefabFile>>,
    /// By id; `None` once released, until it is used again.
    pub templates: Vec<Option<Arc<Template>>>,
    pub guids: Vec<AssetGuid>,
    pub ids: HashMap<AssetGuid, PrefabId>,
    /// Kept loaded across sweeps until released.
    pub kept: HashSet<PrefabId>,
    /// Loaded or instantiated since the last sweep.
    pub used: HashSet<PrefabId>,
}
