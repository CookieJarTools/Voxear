//! The components the Inspector shows below the Transform, and the values
//! in each that it edits. A component is a [`Part`]; each value in it is
//! named by a [`Prop`], read from the world as a [`Value`] and written
//! back. Script fields, and the values inside them, are props too (see
//! `fields`).

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours, Game, MaterialSource, linear,
    prefab::Locks,
    scene_file::{default_light, default_material},
    script_types::ScriptTypes,
    srgb,
};
use voxear_render::{Camera, Light, LightKind, MeshRenderer, Renderer};
use voxear_scene::{BehaviourSpec, Behaviours, SceneComponents};

use crate::fields::{self, Pick, Step};

/// The meshes every project has, as the Mesh menu lists them first.
const BUILTIN_MESHES: [(&str, &str); 3] =
    [("Cube", "builtin:cube"), ("Sphere", "builtin:sphere"), ("Plane", "builtin:plane")];
const LIGHT_KINDS: [&str; 3] = ["Directional", "Point", "Spot"];

/// A component the Inspector shows as a section of its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Renderer,
    Light,
    Camera,
    /// A script, by its full type name.
    Behaviour(String),
}

impl Part {
    /// The component as a prefab's locks name it: a behaviour by its type, or
    /// `renderer`, `light` or `camera`.
    pub fn lock_key(&self) -> String {
        match self {
            Part::Renderer => "renderer".into(),
            Part::Light => "light".into(),
            Part::Camera => "camera".into(),
            Part::Behaviour(name) => name.clone(),
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Part::Renderer => "Mesh Renderer",
            Part::Light => "Light",
            Part::Camera => "Camera",
            Part::Behaviour(name) => name.rsplit('.').next().unwrap_or(name),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Part::Renderer => "cube",
            Part::Light => "light",
            Part::Camera => "camera",
            Part::Behaviour(_) => "code",
        }
    }
}

/// One value in a component.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    Mesh,
    Material,
    LightKind,
    LightColor,
    Intensity,
    Range,
    SpotInner,
    SpotOuter,
    FieldOfView,
    Near,
    Exposure,
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
            Prop::Mesh | Prop::Material => Part::Renderer,
            Prop::LightKind | Prop::LightColor | Prop::Intensity | Prop::Range | Prop::SpotInner | Prop::SpotOuter => {
                Part::Light
            }
            Prop::FieldOfView | Prop::Near | Prop::Exposure => Part::Camera,
            Prop::Field { behaviour, .. } => Part::Behaviour(behaviour.clone()),
        }
    }

    /// The value as a prefab's locks name the fields shown to designers: a
    /// script value by its field, and the rest by a name of their own.
    pub fn lock_key(&self) -> String {
        let name = match self {
            Prop::Mesh => "mesh",
            Prop::Material => "material",
            Prop::LightKind => "type",
            Prop::LightColor => "color",
            Prop::Intensity => "intensity",
            Prop::Range => "range",
            Prop::SpotInner => "spot_inner",
            Prop::SpotOuter => "spot_outer",
            Prop::FieldOfView => "fov",
            Prop::Near => "near",
            Prop::Exposure => "exposure",
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

/// The values a component shows. A light shows only what its kind uses; a
/// script, the fields it declares, then any others its scene sets.
pub fn props(world: &World, entity: Entity, part: &Part, types: &ScriptTypes) -> Vec<Prop> {
    match part {
        Part::Renderer => vec![Prop::Mesh, Prop::Material],
        Part::Light => {
            let kind = world.get::<Light>(entity).map_or(LightKind::Point, |l| l.kind);
            let mut props = vec![Prop::LightKind, Prop::LightColor, Prop::Intensity];
            if kind != LightKind::Directional {
                props.push(Prop::Range);
            }
            if kind == LightKind::Spot {
                props.extend([Prop::SpotInner, Prop::SpotOuter]);
            }
            props
        }
        Part::Camera => vec![Prop::FieldOfView, Prop::Near, Prop::Exposure],
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
        Prop::LightKind => "Type",
        Prop::LightColor => "Colour",
        Prop::Intensity => "Intensity",
        Prop::Range => "Range",
        Prop::SpotInner => "Inner angle",
        Prop::SpotOuter => "Outer angle",
        Prop::FieldOfView => "Field of view",
        Prop::Near => "Near",
        Prop::Exposure => "Exposure",
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

/// How deep a value sits inside its script field: 0 for a field itself.
pub fn depth(prop: &Prop) -> usize {
    match prop {
        Prop::Field { path, .. } => path.len().saturating_sub(1),
        _ => 0,
    }
}

/// A component as an Obj gets it from Add Component: a grey cube, a point
/// light, a camera, or a script (a behaviour or, as `types` tell, a C#
/// component) with every field as it starts.
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
        game.engine.world.insert(entity, MeshRenderer { mesh, material });
        return Ok(());
    }
    let world = &mut game.engine.world;
    match part {
        Part::Renderer => {}
        Part::Light => {
            world.insert(entity, default_light(LightKind::Point));
        }
        Part::Camera => {
            world.insert(entity, Camera::default());
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
    Some(match prop {
        Prop::Mesh => {
            let reference = game.content.mesh_ref(world.get::<MeshRenderer>(entity)?.mesh)?.to_string();
            match BUILTIN_MESHES.iter().find(|(_, r)| *r == reference) {
                Some((label, _)) => Value::Asset(label.to_string()),
                None => Value::Asset(asset_label(game, &reference)),
            }
        }
        Prop::Material => {
            let source = game.content.material_source(world.get::<MeshRenderer>(entity)?.material)?;
            Value::Asset(match source {
                MaterialSource::Inline(data) if *data == default_material() => "Default".into(),
                MaterialSource::Asset(reference) => reference
                    .guid()
                    .and_then(|guid| game.content.db().get(guid))
                    .map_or_else(|| reference.to_string(), |e| e.path.clone()),
                MaterialSource::Inline(_) => "Custom".into(),
            })
        }
        Prop::LightKind => {
            let index = match light()?.kind {
                LightKind::Directional => 0,
                LightKind::Point => 1,
                LightKind::Spot => 2,
            };
            Value::Choice(index, LIGHT_KINDS.iter().map(|k| k.to_string()).collect())
        }
        Prop::LightColor => Value::Color(srgb(light()?.color.to_array()).map(round3)),
        Prop::Intensity => Value::Number(light()?.intensity),
        Prop::Range => Value::Number(light()?.range),
        Prop::SpotInner => Value::Number(light()?.spot_inner_degrees),
        Prop::SpotOuter => Value::Number(light()?.spot_outer_degrees),
        Prop::FieldOfView => Value::Number(camera()?.fov_y_degrees),
        Prop::Near => Value::Number(camera()?.near),
        Prop::Exposure => Value::Number(camera()?.exposure),
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
    match (prop, value) {
        // A mesh or material kept after play comes back by its name, as its list has it.
        (Prop::Mesh | Prop::Material, Value::Asset(label)) => {
            let choice = choices(game, types, entity, prop).into_iter().find(|(l, _)| *l == label);
            choice.is_some_and(|(_, choice)| pick(game, renderer, types, entity, prop, &choice))
        }
        (Prop::Field { behaviour, path }, value) => {
            fields::set(&mut game.engine.world, types, entity, behaviour, path, &value)
        }
        (prop, value) => set_component(&mut game.engine.world, entity, prop, value),
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
            let Ok(mesh) = game.content.parse(reference).and_then(|r| game.content.mesh(renderer, &r)) else {
                return false;
            };
            with(&mut game.engine.world, entity, |r: &mut MeshRenderer| r.mesh = mesh)
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
        (Prop::Field { behaviour, path }, choice) => {
            fields::pick(&mut game.engine.world, types, entity, behaviour, path, choice)
        }
        _ => false,
    }
}

/// What a reference can point at, each with what it is called: the built-in
/// meshes and the one it has for a Mesh, the default and the project's
/// materials for a Material, and for a script field, the Objs or assets of
/// its kind. Nothing comes first where a reference can be empty.
pub fn choices(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> Vec<(String, Pick)> {
    match prop {
        Prop::Mesh => {
            let mut found: Vec<(String, Pick)> =
                BUILTIN_MESHES.iter().map(|(label, r)| (label.to_string(), Pick::Asset(Some(r.to_string())))).collect();
            let mesh = game.engine.world.get::<MeshRenderer>(entity).map(|r| r.mesh);
            let reference = mesh.and_then(|mesh| game.content.mesh_ref(mesh)).map(|r| r.to_string());
            if let Some(reference) = reference.filter(|r| !BUILTIN_MESHES.iter().any(|(_, b)| b == r)) {
                found.push((asset_label(game, &reference), Pick::Asset(Some(reference))));
            }
            return found;
        }
        Prop::Material => {
            let mut found = vec![("Default".to_string(), Pick::Asset(None))];
            found.extend(material_assets(game).into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
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
        Some(kind) => fields::obj_choices(game, types, entity, &kind),
        None => fields::obj_choices(game, types, entity, &voxear_engine::script_types::FieldKind::Obj),
    }
}

/// Why a reference cannot take what was dropped on it.
pub fn why_not(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, choice: &Pick) -> String {
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
        Prop::Mesh => "Mesh".into(),
        Prop::Material => "Material".into(),
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

/// Whether a prop is an item of a list, which can be taken out.
pub fn is_item(prop: &Prop) -> bool {
    matches!(prop, Prop::Field { path, .. } if matches!(path.last(), Some(Step::Item(_))))
}

/// Lights and cameras, whose values are plain numbers on the component.
fn set_component(world: &mut World, entity: Entity, prop: &Prop, value: Value) -> bool {
    match (prop, value) {
        (Prop::LightKind, Value::Choice(index, _)) => with(world, entity, |light: &mut Light| {
            let kind = [LightKind::Directional, LightKind::Point, LightKind::Spot][index.min(2)];
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
        }),
        (Prop::LightColor, Value::Color(color)) => {
            with(world, entity, |light: &mut Light| light.color = Vec3::from(linear(color.map(|c| c.clamp(0.0, 1.0)))))
        }
        (Prop::Intensity, Value::Number(n)) => with(world, entity, |light: &mut Light| light.intensity = n.max(0.0)),
        (Prop::Range, Value::Number(n)) => with(world, entity, |light: &mut Light| light.range = n.max(0.0)),
        (Prop::SpotInner, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.spot_inner_degrees = n.clamp(0.0, light.spot_outer_degrees))
        }
        (Prop::SpotOuter, Value::Number(n)) => {
            with(world, entity, |light: &mut Light| light.spot_outer_degrees = n.clamp(light.spot_inner_degrees, 179.0))
        }
        (Prop::FieldOfView, Value::Number(n)) => {
            with(world, entity, |camera: &mut Camera| camera.fov_y_degrees = n.clamp(1.0, 179.0))
        }
        (Prop::Near, Value::Number(n)) => with(world, entity, |camera: &mut Camera| camera.near = n.max(0.001)),
        (Prop::Exposure, Value::Number(n)) => with(world, entity, |camera: &mut Camera| camera.exposure = n.max(0.0)),
        _ => false,
    }
}

/// Changes a component, if the Obj has it.
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

/// A field's name as a label, as Unity shows it: `degreesPerSecond` is
/// "Degrees Per Second", and `rare_chance` "Rare Chance".
pub fn nice_name(name: &str) -> String {
    let mut words = String::new();
    let mut previous: Option<char> = None;
    for c in name.trim_start_matches('_').chars() {
        if c == '_' {
            previous = Some(' ');
            continue;
        }
        let starts_word = previous.is_some_and(|p| {
            p == ' ' || (c.is_uppercase() && p.is_lowercase()) || (c.is_ascii_digit() && !p.is_ascii_digit())
        });
        if starts_word {
            words.push(' ');
        }
        let first = words.is_empty() || words.ends_with(' ');
        words.extend(if first { c.to_uppercase().collect::<Vec<_>>() } else { vec![c] });
        previous = Some(c);
    }
    words
}

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
}
