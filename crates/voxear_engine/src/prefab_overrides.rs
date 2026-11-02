//! What an Obj of a prefab instance changes from its prefab, as Unity's
//! prefab overrides, and putting values back from the prefab, or onto it.
//!
//! An override is named by where its value is in a scene file's Obj: a
//! [`ValuePath`] such as `position`, `light.color` or
//! `fields.Game.Spinner.speed`, the same comparison a scene's save makes
//! when it writes an instance's overrides. The root's position, rotation
//! and name place the instance, as in Unity, so they are never counted.
//!
//! Reverting copies the prefab's value over the Obj's, and applying copies
//! the Obj's into the prefab's file, for the prefab's own Objs; an Obj of a
//! prefab nested inside it is changed in that prefab, opened on its own.
//! Either way the Obj's components are set again from its file form
//! ([`put`]), as loading it would set them.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use voxear_assets::AssetGuid;
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, Light, LodGroup, MeshRenderer, Renderer};
use voxear_scene::{BehaviourSpec, Layer, Name, ScenePersistence};

use crate::{
    EditBehaviours,
    audio::{AudioListener, AudioSource},
    content::Content,
    physics::{CharacterController, Collider, Joint, Rigidbody},
    physics2d::{Collider2D, Rigidbody2D},
    prefab::{self, FlatObj, FromPrefab, ObjPath, PrefabFile, PrefabInstance},
    scene_file::{self, ObjData},
};

/// The id an instance's root has in the file forms here: references inside
/// the instance are written as paths from it.
const ROOT: u32 = 1;

/// Values that place an instance rather than change it, on its root.
const PLACING: [&str; 3] = ["position", "rotation", "name"];

/// Where a value is in a scene file's Obj: a key, then the keys inside it,
/// as `["light", "color"]` or `["fields", "Game.Spinner", "speed"]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValuePath(pub Vec<String>);

impl ValuePath {
    pub fn new(keys: &[&str]) -> Self {
        Self(keys.iter().map(|key| key.to_string()).collect())
    }

    /// Whether either path lies inside the other, as `light` and
    /// `light.color` do: a change to one is a change to the other.
    pub fn overlaps(&self, other: &ValuePath) -> bool {
        self.0.iter().zip(&other.0).all(|(a, b)| a == b)
    }
}

/// An Obj of a prefab instance: the instance's root, its prefab, and the
/// Obj's path inside the prefab, empty for the root.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub root: Entity,
    pub prefab: AssetGuid,
    pub path: ObjPath,
}

/// Where an Obj is in a prefab instance, when it is in one.
pub fn member(world: &World, entity: Entity) -> Option<Member> {
    let (root, path) = match world.get::<FromPrefab>(entity) {
        Some(from) if !from.path.is_root() => (from.instance, from.path.clone()),
        _ => (entity, ObjPath::default()),
    };
    let PrefabInstance(prefab) = *world.get::<PrefabInstance>(root)?;
    Some(Member { root, prefab, path })
}

/// Every Obj of the instance whose root is `root`, by its path in the
/// prefab, the root's being empty.
pub fn members(world: &World, root: Entity) -> HashMap<ObjPath, Entity> {
    let inside = world.query_ref::<(Entity, &FromPrefab)>();
    let mut found: HashMap<ObjPath, Entity> = inside
        .into_iter()
        .filter(|(_, from)| from.instance == root && !from.path.is_root())
        .map(|(entity, from)| (from.path.clone(), entity))
        .collect();
    found.insert(ObjPath::default(), root);
    found
}

/// The Obj as a scene file would write it, its references inside its
/// instance as paths from the root.
pub fn file_form(world: &World, content: &Content, entity: Entity) -> Option<ObjData> {
    let member = member(world, entity)?;
    Some(scene_file::capture_obj(world, content, entity, &HashMap::from([(member.root, ROOT)])))
}

/// The Obj as its prefab has it, in the same form as [`file_form`].
pub fn prefab_form(world: &World, content: &mut Content, entity: Entity) -> Result<ObjData> {
    let member = member(world, entity).context("not part of a prefab instance")?;
    let objs = prefab::expand_prefab(content, member.prefab)?;
    let obj = objs.into_iter().find(|obj| obj.path == member.path).context("the prefab no longer has this Obj")?;
    let mut data = obj.data;
    prefab::map_obj_refs(&mut data, &|target| ObjPath::new(ROOT).join(target))?;
    data.layer = scene_file::LayerRef::as_saved(&data.layer, world);
    Ok(data)
}

/// What the Obj changes from its prefab, without what places its instance.
/// Empty for an Obj outside an instance, or one whose prefab cannot be read.
pub fn overrides(world: &World, content: &mut Content, entity: Entity) -> Vec<ValuePath> {
    let (Some(member), Some(actual)) = (member(world, entity), file_form(world, content, entity)) else {
        return Vec::new();
    };
    let Ok(expected) = prefab_form(world, content, entity) else { return Vec::new() };
    let mut paths = changed_paths(content, &expected, &actual).unwrap_or_default();
    if member.path.is_root() {
        paths.retain(|path| !PLACING.contains(&path.0[0].as_str()));
    }
    paths
}

/// Every Obj of an instance that changes something from its prefab, with
/// what it changes.
pub fn instance_overrides(world: &World, content: &mut Content, root: Entity) -> Vec<(Entity, Vec<ValuePath>)> {
    let mut objs: Vec<(ObjPath, Entity)> = members(world, root).into_iter().collect();
    objs.sort();
    objs.into_iter()
        .map(|(_, entity)| (entity, overrides(world, content, entity)))
        .filter(|(_, paths)| !paths.is_empty())
        .collect()
}

/// Where `actual` differs from `expected`, as a scene's save sees it.
fn changed_paths(content: &Content, expected: &ObjData, actual: &ObjData) -> Result<Vec<ValuePath>> {
    let changes = scene_file::differences(content, expected, actual)?;
    let before = toml::Table::try_from(expected)?;
    let mut paths = Vec::new();
    for (key, value) in &changes {
        match (key.as_str(), value, before.get(key)) {
            ("fields", toml::Value::Table(behaviours), _) => {
                for (behaviour, fields) in behaviours {
                    let fields = fields.as_table().into_iter().flat_map(toml::Table::keys);
                    paths.extend(fields.map(|field| ValuePath::new(&["fields", behaviour, field])));
                }
            }
            // A component the prefab has too: by the values that differ.
            (_, toml::Value::Table(now), Some(toml::Value::Table(was))) if key != "lod_group" && key != "joint" => {
                let differ = now.iter().filter(|(sub, value)| was.get(*sub) != Some(*value));
                paths.extend(differ.map(|(sub, _)| ValuePath::new(&[key, sub])));
            }
            _ => paths.push(ValuePath::new(&[key])),
        }
    }
    Ok(paths)
}

/// Copies the value at `path` from `from` into `data`, or takes it out of
/// `data` when `from` has none there.
pub fn copy_value(data: &mut ObjData, from: &ObjData, path: &ValuePath) -> Result<()> {
    let Some((last, keys)) = path.0.split_last() else { return Ok(()) };
    let source = toml::Table::try_from(from)?;
    let mut table = toml::Table::try_from(&*data)?;
    let holder = keys.iter().try_fold(&source, |table, key| table.get(key)?.as_table());
    let value = holder.and_then(|holder| holder.get(last)).cloned();
    let mut at = &mut table;
    for key in keys {
        let entry = at.entry(key.clone()).or_insert_with(|| toml::Value::Table(toml::Table::new()));
        at = entry.as_table_mut().with_context(|| format!("`{key}` is not a table"))?;
    }
    match value {
        Some(value) => at.insert(last.clone(), value),
        None => at.remove(last),
    };
    *data = toml::Value::Table(table).try_into()?;
    Ok(())
}

/// Sets the Obj's name, Transform and components to what `data` says, as
/// loading it would, its references bound to the Objs of its instance. Its
/// place in the hierarchy and its prefab links stay. Without a renderer its
/// Mesh Renderer is left as it is. Only in edit mode, where scripts are
/// kept as data.
pub fn put(
    world: &mut World,
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    data: &ObjData,
) -> Result<()> {
    let member = member(world, entity).context("not part of a prefab instance")?;
    let drawn = renderer.is_some();
    let flat = FlatObj { path: ObjPath::default(), parent: None, data: data.clone(), prefab: None };
    let obj = prefab::resolve_obj(content, renderer, &flat, None)?;
    let inside = members(world, member.root);
    let lookup = |path: &ObjPath| path.strip(&ObjPath::new(ROOT)).and_then(|path| inside.get(&path).copied());
    let behaviours: Vec<BehaviourSpec> = obj
        .behaviours
        .iter()
        .map(|spec| match prefab::bind_refs(&spec.fields, &lookup) {
            Some(fields) => BehaviourSpec { fields: std::sync::Arc::new(fields), ..spec.clone() },
            None => spec.clone(),
        })
        .collect();
    if world.has::<voxear_scene::Behaviours>(entity) {
        bail!("scripts are running; stop playing first");
    }
    world.insert(entity, (Name(obj.name), obj.transform));
    set(world, entity, obj.persistent.then_some(ScenePersistence));
    set::<Camera>(world, entity, obj.camera);
    set::<Light>(world, entity, obj.light);
    if drawn {
        set::<MeshRenderer>(world, entity, obj.renderer);
    }
    set::<AudioSource>(world, entity, obj.audio);
    set(world, entity, obj.audio_listener.then_some(AudioListener));
    let layer = obj.layer.as_ref().map(|layer| layer.layer(world)).filter(|layer| layer.0 != 0);
    set::<Layer>(world, entity, layer);
    set::<Collider>(world, entity, obj.collider);
    set::<Rigidbody>(world, entity, obj.rigidbody);
    set::<CharacterController>(world, entity, obj.character);
    set::<Collider2D>(world, entity, obj.collider2d);
    set::<Rigidbody2D>(world, entity, obj.rigidbody2d);
    set::<LodGroup>(world, entity, obj.lod_group.map(|lod| lod.bind(&lookup)));
    set::<Joint>(world, entity, obj.joint.map(|joint| joint.bind(&lookup)));
    set(world, entity, (!behaviours.is_empty()).then_some(EditBehaviours(behaviours)));
    Ok(())
}

/// Puts a component on an Obj, or takes it off for none.
fn set<T: voxear_ecs::Component>(world: &mut World, entity: Entity, component: Option<T>) {
    match component {
        Some(component) => {
            world.insert(entity, component);
        }
        None => {
            world.remove::<T>(entity);
        }
    }
}

/// Puts the prefab's values at `paths` back on the Obj.
pub fn revert(
    world: &mut World,
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    paths: &[ValuePath],
) -> Result<()> {
    let mut data = file_form(world, content, entity).context("not part of a prefab instance")?;
    let expected = prefab_form(world, content, entity)?;
    for path in paths {
        copy_value(&mut data, &expected, path)?;
    }
    put(world, content, renderer, entity, &data)
}

/// Writes the Objs' values at their paths into their prefab's file, and
/// returns the prefab's project path. Every Obj must be in the same
/// instance. The prefab is read again when next used; the instances in
/// the world are left for [`follow_prefab`].
pub fn apply(world: &World, content: &mut Content, changes: &[(Entity, Vec<ValuePath>)]) -> Result<String> {
    let (first, _) = changes.first().context("nothing to apply")?;
    let prefab = member(world, *first).context("not part of a prefab instance")?.prefab;
    let entry = content.db().get(prefab).context("the prefab is not in the project")?;
    let (path, file) = (entry.path.clone(), content.db().root().join(&entry.path));
    let text = std::fs::read_to_string(&file).with_context(|| format!("reading {path}"))?;
    let mut prefab_file: PrefabFile = toml::from_str(&text).with_context(|| format!("reading {path}"))?;
    let root_id = prefab_file.objs.iter().find(|obj| obj.parent.is_none()).map(|obj| obj.id).context("no root")?;
    for (entity, paths) in changes {
        let member = member(world, *entity).context("not part of a prefab instance")?;
        let id = match member.path.ids() {
            [] => root_id,
            [id] => *id,
            _ => bail!("an Obj of a prefab inside {path} changes in that prefab; open it to change it"),
        };
        // References from the instance's root, as the prefab's file names them.
        let mut actual = file_form(world, content, *entity).context("not part of a prefab instance")?;
        prefab::map_obj_refs(&mut actual, &|target| match target.strip(&ObjPath::new(ROOT)) {
            Some(inside) if inside.is_root() => ObjPath::new(root_id),
            Some(inside) => inside,
            None => target.clone(),
        })?;
        let obj = prefab_file.objs.iter_mut().find(|obj| obj.id == id).context("the prefab no longer has this Obj")?;
        for path in paths.iter().filter(|p| !(member.path.is_root() && PLACING.contains(&p.0[0].as_str()))) {
            copy_value(obj, &actual, path)?;
        }
    }
    std::fs::write(&file, toml::to_string(&prefab_file)?).with_context(|| format!("writing {path}"))?;
    content.forget(&path);
    Ok(path)
}

/// After [`apply`], gives the other instances of the prefab the values it
/// wrote, where they had not changed them: `kept` is what each of their
/// Objs changed from the prefab before. Returns the Objs changed.
pub fn follow_prefab(
    world: &mut World,
    content: &mut Content,
    mut renderer: Option<&mut Renderer>,
    applied: &[(ObjPath, Vec<ValuePath>)],
    kept: &HashMap<Entity, Vec<ValuePath>>,
    others: &[Entity],
) -> Result<Vec<Entity>> {
    let mut changed = Vec::new();
    for &root in others {
        let inside = members(world, root);
        for (path, paths) in applied {
            let Some(&entity) = inside.get(path) else { continue };
            let own = kept.get(&entity).map_or(&[][..], Vec::as_slice);
            let follow: Vec<&ValuePath> = paths.iter().filter(|p| !own.iter().any(|o| o.overlaps(p))).collect();
            if follow.is_empty() {
                continue;
            }
            let mut data = file_form(world, content, entity).context("not part of a prefab instance")?;
            let expected = prefab_form(world, content, entity)?;
            for path in follow.into_iter().filter(|p| !(path.is_root() && PLACING.contains(&p.0[0].as_str()))) {
                copy_value(&mut data, &expected, path)?;
            }
            put(world, content, renderer.as_deref_mut(), entity, &data)?;
            changed.push(entity);
        }
    }
    Ok(changed)
}

/// What unpacking the instance whose root is `root` makes of its Objs'
/// links, as Unity's Unpack Prefab: each Obj's instance and the prefab its
/// root is an instance of, for every Obj that changes. The instance's Objs
/// become Objs of the scene; those of a prefab nested in it stay that
/// prefab's instance, rooted at its own root. Nothing changes until the
/// links are put on.
pub fn unpacked(world: &World, root: Entity) -> Vec<(Entity, Option<FromPrefab>, Option<PrefabInstance>)> {
    let inside = members(world, root);
    let nested: Vec<(&ObjPath, Entity)> = inside
        .iter()
        .filter(|(path, entity)| !path.is_root() && world.has::<PrefabInstance>(**entity))
        .map(|(path, &entity)| (path, entity))
        .collect();
    let mut links = vec![(root, None, None)];
    for (path, &entity) in inside.iter().filter(|(path, _)| !path.is_root()) {
        // The innermost nested instance it lies in, if any.
        let holder = nested
            .iter()
            .filter(|(at, _)| path.strip(at).is_some())
            .max_by_key(|(at, _)| at.ids().len());
        let link = match holder {
            Some((at, _)) if *at == path => None,
            Some((at, instance)) => Some(FromPrefab { instance: *instance, path: path.strip(at).unwrap_or_default() }),
            None => None,
        };
        links.push((entity, link, world.get::<PrefabInstance>(entity).copied()));
    }
    links
}

/// The roots of the instances of `prefab` in the world.
pub fn instances_of(world: &World, prefab: AssetGuid) -> Vec<Entity> {
    let roots = world.query_ref::<(Entity, &PrefabInstance)>();
    roots.into_iter().filter(|(_, instance)| instance.0 == prefab).map(|(entity, _)| entity).collect()
}
