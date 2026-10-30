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
//! the root's. `overrides`
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
//! whose Objs are addressed as `"<instance id>/<id>"`. A **variant** is a
//! prefab whose root is an instance of another prefab, with its overrides
//! and any children it adds, as in Unity.

use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{Camera, Light, MeshRenderer, Renderer};
use voxear_scene::{Name, Parent, Placement, ScenePersistence, Transform};

use crate::{
    content::{Content, MaterialSource},
    scene_file::{MaterialRef, ObjData, default_material, light_component},
};

pub const PREFAB_FORMAT: u32 = 1;

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

    pub fn parse(text: &str) -> Result<Self> {
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
        let instance = expand_instance(content, obj, reference, stack)
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

fn expand_instance(
    content: &mut Content,
    instance: &ObjData,
    reference: &str,
    stack: &mut Vec<AssetGuid>,
) -> Result<Vec<FlatObj>> {
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a prefab"))?;
    let mut objs = expand_prefab_inner(content, guid, stack)?;
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
    }
    let root = objs.iter_mut().find(|o| o.path.is_root()).expect("the root was expanded");
    root.prefab = Some(guid);
    Ok(objs)
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
    root.behaviours.extend(instance.behaviours.iter().cloned());
    if instance.camera.is_some() {
        root.camera = instance.camera;
    }
    if instance.light.is_some() {
        root.light = instance.light;
    }
    if instance.renderer.is_some() {
        root.renderer = instance.renderer.clone();
    }
}

/// Expanded Objs with their meshes and materials loaded, ready to spawn any
/// number of times without going back to content.
#[derive(Debug, Default)]
pub struct Template {
    pub objs: Vec<TemplateObj>,
}

#[derive(Clone, Debug)]
pub struct TemplateObj {
    pub path: ObjPath,
    /// The parent's index in the template. Roots have none.
    pub parent: Option<usize>,
    pub name: String,
    pub transform: Transform,
    pub persistent: bool,
    pub behaviours: Vec<String>,
    pub camera: Option<Camera>,
    pub light: Option<Light>,
    pub renderer: Option<MeshRenderer>,
    /// Set on the root of each prefab instance.
    pub prefab: Option<AssetGuid>,
}

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
                .with_context(|| format!("Obj {} ({})", obj.path, obj.data.name))?;
            objs.push(resolved);
        }
        Ok(Self { objs })
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
            if let Some(renderer) = obj.renderer {
                world.insert(entity, renderer);
            }
            if obj.persistent {
                world.insert(entity, ScenePersistence);
            }
            if let Some(prefab) = obj.prefab {
                world.insert(entity, PrefabInstance(prefab));
            }
            entities.push(entity);
        }
        for (obj, &entity) in self.objs.iter().zip(&entities) {
            if let Some(parent) = obj.parent.map(|index| entities[index]).or(parent) {
                world.insert(entity, Parent(parent));
            }
        }
        link_instances(world, &self.objs, &entities);
        entities
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

fn resolve_obj(
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    obj: &FlatObj,
    parent: Option<usize>,
) -> Result<TemplateObj> {
    let data = &obj.data;
    let [x, y, z] = data.rotation().map(f32::to_radians);
    // Unity applies Z, then X, then Y: R = Ry · Rx · Rz.
    let rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
    let renderer = match (&data.renderer, renderer) {
        (Some(data), Some(renderer)) => {
            let mesh = content.mesh(renderer, &content.parse(&data.mesh)?)?;
            let source = match &data.material {
                Some(MaterialRef::Asset(reference)) => MaterialSource::Asset(content.parse(reference)?),
                Some(MaterialRef::Inline(material)) => MaterialSource::Inline(*material),
                None => MaterialSource::Inline(default_material()),
            };
            let material = content.material(renderer, &source)?;
            Some(MeshRenderer { mesh, material })
        }
        _ => None,
    };
    Ok(TemplateObj {
        path: obj.path.clone(),
        parent,
        name: if data.name.is_empty() { "Obj".to_string() } else { data.name.clone() },
        transform: Transform::new(Vec3::from(data.position()), rotation, Vec3::from(data.scale())),
        persistent: data.persistent,
        behaviours: data.behaviours.clone(),
        camera: data.camera.map(|c| Camera { fov_y_degrees: c.fov, exposure: c.exposure, ..Camera::default() }),
        light: data.light.map(light_component),
        renderer,
        prefab: obj.prefab,
    })
}

/// A loaded prefab, by the id scripts hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrefabId(pub u32);

/// Loaded prefab files and templates, kept by [`Content`].
#[derive(Default)]
pub(crate) struct PrefabCache {
    pub files: HashMap<AssetGuid, Arc<PrefabFile>>,
    pub templates: Vec<Arc<Template>>,
    pub ids: HashMap<AssetGuid, PrefabId>,
}
