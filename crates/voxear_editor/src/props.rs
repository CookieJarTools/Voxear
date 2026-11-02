//! The components the Inspector shows below the Transform, and the values
//! in each that it edits. A component is a [`Part`]; each value in it is
//! named by a [`Prop`], read from the world as a [`Value`] and written
//! back. Script fields, and the values inside them, are props too (see
//! `fields`).

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours, Game, MaterialSource,
    audio::{AudioBus, AudioListener, AudioSource, SoundSpace},
    linear,
    prefab::Locks,
    scene_file::{default_light, default_material},
    script_types::ScriptTypes,
    srgb,
};
use voxear_render::{Camera, Light, LightKind, LodGroup, LodLevel, LodMeasure, MeshRenderer, Renderer};
use voxear_scene::{BehaviourSpec, Behaviours, Name, Parent, SceneComponents};

use crate::fields::{self, Pick, Step};

/// The meshes every project has, as the Mesh menu lists them first.
const BUILTIN_MESHES: [(&str, &str); 3] =
    [("Cube", "builtin:cube"), ("Sphere", "builtin:sphere"), ("Plane", "builtin:plane")];
const LIGHT_KINDS: [&str; 3] = ["Directional", "Point", "Spot"];
const SOUND_SPACES: [&str; 2] = ["3D", "2D"];
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
            Part::Behaviour(name) => name.clone(),
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
    LightColor,
    Intensity,
    Range,
    SpotInner,
    SpotOuter,
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
    SoundBus,
    /// What a LOD Group's levels are measured by.
    LodMeasure,
    /// Whether a LOD Group shows nothing past its last level.
    LodCull,
    /// The Obj a LOD Group's level shows, by the level's index.
    LodObj(usize),
    /// Where a LOD Group's level ends, by the level's index.
    LodUntil(usize),
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
            Prop::LightKind | Prop::LightColor | Prop::Intensity | Prop::Range | Prop::SpotInner | Prop::SpotOuter => {
                Part::Light
            }
            Prop::FieldOfView | Prop::Near | Prop::Exposure => Part::Camera,
            Prop::Sound
            | Prop::SoundVolume
            | Prop::Pitch
            | Prop::Loop
            | Prop::PlayOnStart
            | Prop::SoundSpace
            | Prop::MinDistance
            | Prop::MaxDistance
            | Prop::SoundBus => Part::Audio,
            Prop::LodMeasure | Prop::LodCull | Prop::LodObj(_) | Prop::LodUntil(_) => Part::LodGroup,
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
            Prop::LightColor => "color",
            Prop::Intensity => "intensity",
            Prop::Range => "range",
            Prop::SpotInner => "spot_inner",
            Prop::SpotOuter => "spot_outer",
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
            Prop::SoundBus => "bus",
            Prop::LodMeasure => "measure",
            Prop::LodCull => "cull",
            Prop::LodObj(_) | Prop::LodUntil(_) => "levels",
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
    if world.has::<AudioSource>(entity) {
        parts.push(Part::Audio);
    }
    if world.has::<AudioListener>(entity) {
        parts.push(Part::Listener);
    }
    if world.has::<LodGroup>(entity) {
        parts.push(Part::LodGroup);
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
        Part::Renderer => vec![Prop::Mesh, Prop::Material, Prop::LodBias],
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
        Part::Audio => {
            let mut props =
                vec![Prop::Sound, Prop::SoundVolume, Prop::Pitch, Prop::Loop, Prop::PlayOnStart, Prop::SoundBus];
            props.push(Prop::SoundSpace);
            if world.get::<AudioSource>(entity).is_none_or(|source| source.space == SoundSpace::ThreeD) {
                props.extend([Prop::MinDistance, Prop::MaxDistance]);
            }
            props
        }
        Part::Listener => Vec::new(),
        // Each level's Obj, then where it ends, which the last has only when the group hides past it.
        Part::LodGroup => {
            let Some(group) = world.get::<LodGroup>(entity) else { return Vec::new() };
            let mut props = vec![Prop::LodMeasure];
            for level in 0..group.levels.len() {
                props.push(Prop::LodObj(level));
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
        Prop::LightColor => "Colour",
        Prop::Intensity => "Intensity",
        Prop::Range => "Range",
        Prop::SpotInner => "Inner angle",
        Prop::SpotOuter => "Outer angle",
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
        Prop::SoundBus => "Bus",
        Prop::LodMeasure => "Switch by",
        Prop::LodCull => "Culled",
        Prop::LodObj(level) => return format!("LOD {level}"),
        Prop::LodUntil(_) => "Transition",
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
/// LOD Group with a level for each Obj inside it, in the Hierarchy's order,
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
    if *part == Part::LodGroup {
        let group = LodGroup::of(&children(game, entity));
        game.engine.world.insert(entity, group);
        return Ok(());
    }
    let world = &mut game.engine.world;
    match part {
        Part::Renderer | Part::LodGroup => {}
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
        Part::Audio => drop(world.remove::<AudioSource>(entity)),
        Part::Listener => drop(world.remove::<AudioListener>(entity)),
        Part::LodGroup => drop(world.remove::<LodGroup>(entity)),
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
        Prop::LodBias => Value::Number(world.get::<MeshRenderer>(entity)?.lod_bias),
        Prop::LightColor => Value::Color(srgb(light()?.color.to_array()).map(round3)),
        Prop::Intensity => Value::Number(light()?.intensity),
        Prop::Range => Value::Number(light()?.range),
        Prop::SpotInner => Value::Number(light()?.spot_inner_degrees),
        Prop::SpotOuter => Value::Number(light()?.spot_outer_degrees),
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
        Prop::SoundBus => {
            let index = usize::from(audio()?.bus == AudioBus::Music);
            Value::Choice(index, AUDIO_BUSES.iter().map(|k| k.to_string()).collect())
        }
        Prop::LodMeasure => {
            let index = usize::from(lod()?.measure == LodMeasure::Distance);
            Value::Choice(index, LOD_MEASURES.iter().map(|k| k.to_string()).collect())
        }
        Prop::LodCull => Value::Toggle(lod()?.cull),
        Prop::LodObj(level) => {
            let obj = lod()?.levels.get(*level)?.obj;
            Value::Obj(obj.and_then(|obj| world.get::<Name>(obj)).map_or_else(|| "None".into(), |n| n.0.clone()))
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
    match (prop, value) {
        // A mesh, material, sound or level's Obj kept after play comes back by its name, as its list has it.
        (Prop::Mesh | Prop::Material | Prop::Sound, Value::Asset(label)) | (Prop::LodObj(_), Value::Obj(label)) => {
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
        (Prop::LodObj(level), Pick::Obj(obj)) => with(&mut game.engine.world, entity, |group: &mut LodGroup| {
            if let Some(at) = group.levels.get_mut(*level) {
                at.obj = *obj;
            }
        }),
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
        Prop::LodObj(level) => world.get::<LodGroup>(entity)?.levels.get(*level)?.obj,
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

/// Puts a new level into a LOD Group before the level at `at`, or after the
/// last, showing the first Obj inside the group's Obj that no level shows
/// yet. Before a level, it takes the nearer half of that level's part of
/// the bar, as Unity's Insert Before does.
pub fn insert_lod_level(game: &mut Game, entity: Entity, at: usize) -> bool {
    let inside = children(game, entity);
    with(&mut game.engine.world, entity, |group: &mut LodGroup| {
        let obj = inside.into_iter().find(|child| group.levels.iter().all(|level| level.obj != Some(*child)));
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
        group.levels.insert(at, LodLevel { obj, until });
    })
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
        Prop::Sound => {
            let kind = voxear_engine::script_types::FieldKind::Asset { asset: "Voxear.Sound".into() };
            return fields::asset_choices(game, &kind);
        }
        // A level shows an Obj inside the group's Obj.
        Prop::LodObj(_) => {
            let world = &game.engine.world;
            let mut found = vec![("None".to_string(), Pick::Obj(None))];
            for child in children(game, entity) {
                let name = world.get::<Name>(child).map_or_else(|| "Obj".into(), |n| n.0.clone());
                found.push((name, Pick::Obj(Some(child))));
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
        Some(kind) => fields::obj_choices(game, types, entity, &kind),
        None => fields::obj_choices(game, types, entity, &voxear_engine::script_types::FieldKind::Obj),
    }
}

/// Why a reference cannot take what was dropped on it.
pub fn why_not(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, choice: &Pick) -> String {
    if let Prop::LodObj(_) = prop {
        return format!("{} takes an Obj inside the LOD Group's Obj", label(prop));
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
        Prop::Mesh => "Mesh".into(),
        Prop::Material => "Material".into(),
        Prop::Sound => "Sound".into(),
        Prop::LodObj(_) => "Obj".into(),
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

/// Whether a prop is an item of a list, which can be taken out.
pub fn is_item(prop: &Prop) -> bool {
    matches!(prop, Prop::Field { path, .. } if matches!(path.last(), Some(Step::Item(_))))
}

/// Lights, cameras and Audio Sources, whose values are plain numbers,
/// switches and choices on the component.
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
