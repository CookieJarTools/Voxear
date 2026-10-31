//! The components the Inspector shows below the Transform, and the values
//! in each that it edits. A component is a [`Part`]; each value in it is
//! named by a [`Prop`], read from the world as a [`Value`] and written
//! back. Script fields are props too, typed by what the script declares
//! (`script_types`), or when it is not found, by the value the scene gave.

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours, Game, MaterialSource, linear,
    scene_file::{default_light, default_material},
    script_types::{BehaviourType, same_field},
    srgb,
};
use voxear_render::{Camera, Light, LightKind, MeshRenderer, Renderer};
use voxear_scene::{BehaviourSpec, Behaviours, Name};

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
    /// A script field: its behaviour's type name, the field's name, and its
    /// C# type when the script was found.
    Field {
        behaviour: String,
        name: String,
        type_name: Option<String>,
    },
}

/// A value as the Inspector shows and edits it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Whole(i64),
    Toggle(bool),
    Text(String),
    Vector([f32; 3]),
    /// A colour as people pick them, in sRGB from 0 to 1.
    Color([f32; 3]),
    /// One of a list, by index.
    Choice(usize, Vec<String>),
    /// Shown, but not edited here, such as a reference to another Obj.
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

/// An Obj's scripts: the running ones while playing, or in edit mode the
/// same kept as data.
pub fn specs(world: &World, entity: Entity) -> Vec<BehaviourSpec> {
    match (world.get::<Behaviours>(entity), world.get::<EditBehaviours>(entity)) {
        (Some(running), _) => running.0.iter().map(|b| b.spec.clone()).collect(),
        (None, Some(kept)) => kept.0.clone(),
        (None, None) => Vec::new(),
    }
}

/// The values a component shows. A light shows only what its kind uses; a
/// script, the fields it declares, then any others its scene sets.
pub fn props(world: &World, entity: Entity, part: &Part, types: &[BehaviourType]) -> Vec<Prop> {
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
            let declared = types.iter().find(|t| t.name == *behaviour);
            let spec = specs(world, entity).into_iter().find(|s| s.type_name == *behaviour);
            let field = |name: &str, type_name: Option<String>| Prop::Field {
                behaviour: behaviour.clone(),
                name: name.to_string(),
                type_name,
            };
            let mut props: Vec<Prop> = declared
                .into_iter()
                .flat_map(|t| t.fields.iter())
                .map(|f| field(&f.name, Some(f.type_name.clone())))
                .collect();
            let unknown: Vec<String> = spec
                .iter()
                .flat_map(|s| s.fields.keys())
                .filter(|key| declared.is_none_or(|t| t.field(key).is_none()))
                .cloned()
                .collect();
            props.extend(unknown.iter().map(|key| field(key, None)));
            props
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
        Prop::Field { name, .. } => return nice_name(name),
    };
    label.to_string()
}

/// A component as an Obj gets it from Add Component: a grey cube, a point
/// light, a camera, or a script with every field as it starts.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, part: &Part) -> anyhow::Result<()> {
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
            let spec = BehaviourSpec::new(name);
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
/// give script fields the values they start as.
pub fn get(game: &Game, types: &[BehaviourType], entity: Entity, prop: &Prop) -> Option<Value> {
    let world = &game.engine.world;
    let light = || world.get::<Light>(entity);
    let camera = || world.get::<Camera>(entity);
    Some(match prop {
        Prop::Mesh => {
            let reference = game.content.mesh_ref(world.get::<MeshRenderer>(entity)?.mesh)?.to_string();
            let mut options: Vec<String> = BUILTIN_MESHES.iter().map(|(label, _)| label.to_string()).collect();
            let index = match BUILTIN_MESHES.iter().position(|(_, r)| *r == reference) {
                Some(index) => index,
                None => {
                    options.push(asset_label(game, &reference));
                    options.len() - 1
                }
            };
            Value::Choice(index, options)
        }
        Prop::Material => {
            let source = game.content.material_source(world.get::<MeshRenderer>(entity)?.material)?;
            let mut options = vec!["Default".to_string()];
            let assets = material_assets(game);
            options.extend(assets.iter().map(|path| file_name(path).to_string()));
            let index = match source {
                MaterialSource::Inline(data) if *data == default_material() => 0,
                MaterialSource::Asset(reference) => {
                    let path = reference.guid().and_then(|guid| game.content.db().get(guid)).map(|e| e.path.clone());
                    assets.iter().position(|a| Some(a) == path.as_ref()).map_or(0, |i| i + 1)
                }
                MaterialSource::Inline(_) => {
                    options.push("Custom".into());
                    options.len() - 1
                }
            };
            Value::Choice(index, options)
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
        Prop::Field { behaviour, name, type_name } => {
            let spec = specs(world, entity).into_iter().find(|s| s.type_name == *behaviour)?;
            let set = spec.fields.iter().find(|(key, _)| same_field(key, name)).map(|(_, v)| v.clone());
            let declared = types.iter().find(|t| t.name == *behaviour).and_then(|t| t.field(name));
            field_value(world, set.as_ref(), type_name.as_deref(), declared.and_then(|f| f.default.clone()))
        }
    })
}

/// Sets a prop. Returns whether anything changed.
pub fn set(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    types: &[BehaviourType],
    entity: Entity,
    prop: &Prop,
    value: Value,
) -> bool {
    if get(game, types, entity, prop).as_ref() == Some(&value) {
        return false;
    }
    match (prop, value) {
        (Prop::Mesh, Value::Choice(index, _)) => {
            let (Some(renderer), Some((_, reference))) = (renderer, BUILTIN_MESHES.get(index)) else { return false };
            let Ok(mesh) = game.content.parse(reference).and_then(|r| game.content.mesh(renderer, &r)) else {
                return false;
            };
            with(&mut game.engine.world, entity, |r: &mut MeshRenderer| r.mesh = mesh)
        }
        (Prop::Material, Value::Choice(index, _)) => {
            let Some(renderer) = renderer else { return false };
            let source = match index.checked_sub(1) {
                None => MaterialSource::Inline(default_material()),
                Some(at) => match material_assets(game).get(at).map(|path| game.content.parse(path)) {
                    Some(Ok(reference)) => MaterialSource::Asset(reference),
                    _ => return false,
                },
            };
            let Ok(material) = game.content.material(renderer, &source) else { return false };
            with(&mut game.engine.world, entity, |r: &mut MeshRenderer| r.material = material)
        }
        (Prop::Field { behaviour, name, .. }, value) => {
            let Some(value) = toml_value(&value) else { return false };
            let Some(kept) = game.engine.world.get_mut::<EditBehaviours>(entity) else { return false };
            let Some(spec) = kept.0.iter_mut().find(|s| s.type_name == *behaviour) else { return false };
            let fields = std::sync::Arc::make_mut(&mut spec.fields);
            let key = fields.keys().find(|key| same_field(key, name)).cloned().unwrap_or_else(|| name.clone());
            fields.insert(key, value);
            true
        }
        (prop, value) => set_component(&mut game.engine.world, entity, prop, value),
    }
}

/// Takes a script field back to what the script starts it as.
pub fn reset_field(world: &mut World, entity: Entity, prop: &Prop) -> bool {
    let Prop::Field { behaviour, name, .. } = prop else { return false };
    let Some(kept) = world.get_mut::<EditBehaviours>(entity) else { return false };
    let Some(spec) = kept.0.iter_mut().find(|s| s.type_name == *behaviour) else { return false };
    let Some(key) = spec.fields.keys().find(|key| same_field(key, name)).cloned() else { return false };
    std::sync::Arc::make_mut(&mut spec.fields).remove(&key);
    true
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

/// A script field's value as the Inspector shows it: what the scene set,
/// or else what the script starts it as, typed by the field's C# type.
fn field_value(
    world: &World,
    set: Option<&toml::Value>,
    type_name: Option<&str>,
    default: Option<toml::Value>,
) -> Value {
    let value = set.cloned().or(default);
    let number = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
    match (type_name.map(|t| t.trim_end_matches('?')), value.as_ref()) {
        (Some("float" | "double" | "decimal"), v) => Value::Number(v.and_then(number).unwrap_or_default() as f32),
        (Some("int" | "long" | "short" | "byte" | "uint" | "ulong" | "ushort" | "sbyte"), v) => {
            Value::Whole(v.and_then(|v| v.as_integer().or_else(|| v.as_float().map(|f| f as i64))).unwrap_or_default())
        }
        (Some("bool"), v) => Value::Toggle(v.and_then(toml::Value::as_bool).unwrap_or_default()),
        (Some("string"), v) => Value::Text(v.and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()),
        (Some("Vector3" | "System.Numerics.Vector3"), v) => Value::Vector(vector(v).unwrap_or_default()),
        (Some(_), v) => Value::Fixed(v.map_or("None".into(), |v| summary(world, v))),
        (None, Some(toml::Value::Float(f))) => Value::Number(*f as f32),
        (None, Some(toml::Value::Integer(i))) => Value::Whole(*i),
        (None, Some(toml::Value::Boolean(b))) => Value::Toggle(*b),
        (None, Some(toml::Value::String(s))) => Value::Text(s.clone()),
        (None, Some(v)) => match vector(Some(v)) {
            Some(xyz) => Value::Vector(xyz),
            None => Value::Fixed(summary(world, v)),
        },
        (None, None) => Value::Fixed("None".into()),
    }
}

/// A value as a scene writes it.
fn toml_value(value: &Value) -> Option<toml::Value> {
    Some(match value {
        Value::Number(n) => toml::Value::Float(f64::from(*n)),
        Value::Whole(n) => toml::Value::Integer(*n),
        Value::Toggle(b) => toml::Value::Boolean(*b),
        Value::Text(text) => toml::Value::String(text.clone()),
        Value::Vector(xyz) | Value::Color(xyz) => {
            toml::Value::Array(xyz.iter().map(|&c| toml::Value::Float(f64::from(c))).collect())
        }
        Value::Choice(..) | Value::Fixed(_) => return None,
    })
}

/// Three numbers in a list, as scenes write a Vector3.
fn vector(value: Option<&toml::Value>) -> Option<[f32; 3]> {
    let items = value?.as_array()?;
    let numbers: Vec<f32> = items
        .iter()
        .filter_map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .map(|f| f as f32)
        .collect();
    (items.len() == 3 && numbers.len() == 3).then(|| [numbers[0], numbers[1], numbers[2]])
}

/// A value that is shown rather than edited: a reference by the name of
/// the Obj it points at, anything else as the scene would write it.
fn summary(world: &World, value: &toml::Value) -> String {
    match value.as_table().and_then(|t| t.get("entity")).and_then(toml::Value::as_integer) {
        Some(0) => "None".into(),
        Some(bits) => {
            let entity = Entity::from_bits(bits as u64);
            world.get::<Name>(entity).map_or("Missing".into(), |n| n.0.clone())
        }
        None => match value {
            toml::Value::Array(items) => format!("{} items", items.len()),
            toml::Value::Table(table) if table.is_empty() => "None".into(),
            other => other.to_string(),
        },
    }
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
