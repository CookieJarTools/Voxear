//! The Terrain component in the Inspector (the Terrain module).
//!
//! Its section has four tools along its top, as Unity's Terrain has
//! (`terrain_panel`): **Layers**, the layers its ground is made of, a list
//! to pick one from with the picked one's settings below; **Sculpt**, the
//! brush the Scene view shapes the ground with, or cuts holes with
//! (`terrain_tools`); **Paint**, its surfaces, a list to pick the one the
//! brush paints, with its settings below; and **Settings**, its asset, how
//! large and fine it is, and how it is drawn. Which tool shows, which layer
//! and surface are picked and the brush are the editor's, kept in the world
//! as [`TerrainUi`], not saved.
//!
//! A change to a layer makes the ground again where it reaches; one to the
//! terrain's size or resolution, everywhere. The surfaces' shares follow
//! the ground wherever it changes. The asset is written when the level is
//! saved (`voxear_engine::terrain`).

use std::sync::Arc;

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game, MaterialSource,
    scene_file::{MaterialRef, TerrainData, TerrainSource, default_material},
    terrain::TerrainAsset,
};
use voxear_render::{Renderer, TerrainLook};
use voxear_terrain::{
    Terrain,
    brush::{Brush, BrushKind},
    composite::{Compositor, Dirty},
    file::{RESOLUTIONS, TerrainFile},
    layers::{Area, Blend, Layer, LayerKind, Mask},
    noise::NoiseStyle,
    raster::Rasters,
    surfaces::{Cover, MAX_SURFACES, Surface},
};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Where a new terrain's asset is made, in the project's folder.
const FOLDER: &str = "Terrains";

/// Which of a Terrain's tools its Inspector shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TerrainTool {
    #[default]
    Layers,
    Sculpt,
    Paint,
    Settings,
}

impl TerrainTool {
    pub const ALL: [TerrainTool; 4] =
        [TerrainTool::Layers, TerrainTool::Sculpt, TerrainTool::Paint, TerrainTool::Settings];

    pub fn name(self) -> &'static str {
        match self {
            TerrainTool::Layers => "Layers",
            TerrainTool::Sculpt => "Sculpt",
            TerrainTool::Paint => "Paint",
            TerrainTool::Settings => "Settings",
        }
    }

    /// Whether the Scene view's brush works with it.
    pub fn brushes(self) -> bool {
        matches!(self, TerrainTool::Sculpt | TerrainTool::Paint)
    }
}

/// What the editor is doing with terrains: the tool shown, the layer and
/// the surface picked, and the brush. Kept in the edit world, not saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainUi {
    pub tool: TerrainTool,
    pub layer: usize,
    pub surface: usize,
    pub brush: Brush,
}

/// The editor's terrain tools as they are.
pub fn ui(world: &World) -> TerrainUi {
    world.resource::<TerrainUi>().cloned().unwrap_or_default()
}

/// The editor's terrain tools, to change.
pub fn ui_mut(world: &mut World) -> &mut TerrainUi {
    if world.resource::<TerrainUi>().is_none() {
        world.insert_resource(TerrainUi::default());
    }
    world.resource_mut::<TerrainUi>().expect("inserted above")
}

/// A value a Terrain shows: its settings, its picked layer's, or its brush's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TerrainField {
    Data,
    Size,
    Resolution,
    Base,
    Material,
    TextureSize,
    Detail,
    Shadows,
    LayerName,
    Blend,
    Opacity,
    Picture,
    Height,
    Position,
    Rotation,
    StampSize,
    Falloff,
    Style,
    Scale,
    Octaves,
    Seed,
    Width,
    Step,
    Sharpness,
    Radius,
    HeightMask,
    Heights,
    SlopeMask,
    Slopes,
    Feather,
    Brush,
    BrushSize,
    Strength,
    BrushHeight,
    BrushStep,
    BumpSize,
    /// The picked surface's: its material, how large its pictures are, how
    /// sharply it meets the others, whether it is drawn from the side on
    /// steep ground, where it covers, and how far its rules' edges wander.
    SurfaceMaterial,
    TileSize,
    HeightBlend,
    Triplanar,
    Cover,
    Breakup,
    BreakupSize,
    /// Add Surface's picker: a new surface of the material picked.
    NewSurface,
}

use TerrainField as F;

/// The picked layer of an Obj's terrain, if its asset has one there.
fn picked(world: &World, entity: Entity) -> Option<(usize, Layer)> {
    let at = ui(world).layer;
    Some((at, world.get::<TerrainAsset>(entity)?.file.layers.get(at)?.clone()))
}

/// The picked surface of an Obj's terrain, if its asset has one there.
fn picked_surface(world: &World, entity: Entity) -> Option<(usize, Surface)> {
    let at = ui(world).surface;
    Some((at, world.get::<TerrainAsset>(entity)?.file.surfaces.get(at)?.clone()))
}

/// The mask the mask's rows show: the picked surface's under Paint, else
/// the picked layer's.
fn shown_mask(world: &World, entity: Entity) -> Option<Mask> {
    match ui(world).tool {
        TerrainTool::Paint => picked_surface(world, entity).map(|(_, surface)| surface.mask),
        _ => picked(world, entity).map(|(_, layer)| layer.mask),
    }
}

/// The rows of a mask's ranges, as many as are on.
fn mask_fields(mask: &Mask) -> Vec<TerrainField> {
    let mut fields = vec![F::HeightMask];
    if mask.height.is_some() {
        fields.push(F::Heights);
    }
    fields.push(F::SlopeMask);
    if mask.slope.is_some() {
        fields.push(F::Slopes);
    }
    if !mask.is_none() {
        fields.push(F::Feather);
    }
    fields
}

/// What the Paint tool shows: the brush, then the picked surface's values,
/// its cover's rules where it covers by them. The first surface is the
/// ground under the rest, so it has no cover.
fn surface_fields(at: usize, surface: &Surface) -> Vec<TerrainField> {
    let mut fields = vec![F::BrushSize, F::Strength, F::SurfaceMaterial, F::TileSize, F::HeightBlend, F::Triplanar];
    if at == 0 {
        return fields;
    }
    fields.push(F::Cover);
    if surface.cover == Cover::Rules {
        fields.extend(mask_fields(&surface.mask));
        fields.push(F::Breakup);
        if surface.breakup > 0.0 {
            fields.push(F::BreakupSize);
        }
    }
    fields
}

/// The values a Terrain shows, for the tool showing.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let fields = match ui(world).tool {
        TerrainTool::Settings => {
            let mut fields = vec![F::Data];
            if world.has::<TerrainAsset>(entity) {
                fields.extend([F::Size, F::Resolution, F::Base]);
            }
            fields.extend([F::Material, F::TextureSize, F::Detail, F::Shadows]);
            fields
        }
        TerrainTool::Layers => match picked(world, entity) {
            Some((_, layer)) => layer_fields(&layer),
            None => Vec::new(),
        },
        TerrainTool::Sculpt => {
            let brush = ui(world).brush;
            let mut fields = vec![F::Brush, F::BrushSize];
            // A hole is cut whole, or not at all.
            if brush.kind != BrushKind::Holes {
                fields.push(F::Strength);
            }
            match brush.kind {
                BrushKind::SetHeight => fields.push(F::BrushHeight),
                BrushKind::Terrace => fields.push(F::BrushStep),
                BrushKind::Noise => fields.push(F::BumpSize),
                _ => {}
            }
            fields
        }
        TerrainTool::Paint => match picked_surface(world, entity) {
            Some((at, surface)) => surface_fields(at, &surface),
            None => vec![F::BrushSize, F::Strength],
        },
    };
    fields.into_iter().map(Prop::Terrain).collect()
}

/// What a layer of its kind shows.
fn layer_fields(layer: &Layer) -> Vec<TerrainField> {
    let mut fields = vec![F::LayerName];
    // A sculpt layer adds what its brushes painted; how is all it has.
    if !matches!(layer.kind, LayerKind::Sculpt { .. }) {
        fields.push(F::Blend);
    }
    fields.push(F::Opacity);
    fields.extend(match layer.kind {
        LayerKind::Import { .. } => vec![F::Picture, F::Height],
        LayerKind::Stamp { .. } => vec![F::Picture, F::Position, F::Rotation, F::StampSize, F::Height, F::Falloff],
        LayerKind::Noise { .. } => vec![F::Style, F::Scale, F::Height, F::Octaves, F::Seed],
        LayerKind::Sculpt { .. } => vec![],
        LayerKind::Spline { .. } => vec![F::Width, F::Falloff],
        LayerKind::Terrace { .. } => vec![F::Step, F::Sharpness],
        LayerKind::Smooth { .. } => vec![F::Radius],
    });
    fields.extend(mask_fields(&layer.mask));
    fields
}

pub fn label(field: TerrainField) -> &'static str {
    match field {
        F::Data => "Data",
        F::Size => "Size",
        F::Resolution => "Resolution",
        F::Base => "Base height",
        F::Material => "Material",
        F::TextureSize => "Texture size",
        F::Detail => "Detail distance",
        F::Shadows => "Cast shadows",
        F::LayerName => "Name",
        F::Blend => "Blend",
        F::Opacity => "Opacity",
        F::Picture => "Picture",
        F::Height => "Height",
        F::Position => "Position",
        F::Rotation => "Rotation",
        F::StampSize => "Size",
        F::Falloff => "Falloff",
        F::Style => "Style",
        F::Scale => "Scale",
        F::Octaves => "Octaves",
        F::Seed => "Seed",
        F::Width => "Width",
        F::Step => "Step",
        F::Sharpness => "Sharpness",
        F::Radius => "Radius",
        F::HeightMask => "Height mask",
        F::Heights => "Heights",
        F::SlopeMask => "Slope mask",
        F::Slopes => "Slopes",
        F::Feather => "Feather",
        F::Brush => "Brush",
        F::BrushSize => "Size",
        F::Strength => "Strength",
        F::BrushHeight => "Height",
        F::BrushStep => "Step",
        F::BumpSize => "Bump size",
        F::SurfaceMaterial | F::NewSurface => "Material",
        F::TileSize => "Tile size",
        F::HeightBlend => "Height blend",
        F::Triplanar => "Triplanar",
        F::Cover => "Cover",
        F::Breakup => "Breakup",
        F::BreakupSize => "Breakup size",
    }
}

/// The name a prefab's locks give the value.
pub fn key(field: TerrainField) -> &'static str {
    match field {
        F::Data => "data",
        F::Material => "material",
        F::TextureSize => "texture_size",
        F::Detail => "detail_distance",
        F::Shadows => "cast_shadows",
        // The rest are the asset's or the editor's, not the scene's.
        _ => "",
    }
}

pub fn unit(world: &World, entity: Entity, field: TerrainField) -> Option<&'static str> {
    match field {
        F::Size | F::Base | F::TextureSize | F::Detail | F::Height | F::Position | F::StampSize | F::Scale => Some("m"),
        F::Width | F::Step | F::Radius | F::Heights | F::BrushSize | F::BrushHeight | F::BrushStep | F::BumpSize => {
            Some("m")
        }
        F::TileSize | F::BreakupSize => Some("m"),
        F::Rotation | F::Slopes => Some("°"),
        F::Falloff => match picked(world, entity) {
            Some((_, Layer { kind: LayerKind::Spline { .. }, .. })) => Some("m"),
            _ => None,
        },
        _ => None,
    }
}

/// The range a value's slider spans, for those shown as sliders.
pub fn range(world: &World, entity: Entity, field: TerrainField) -> Option<[f32; 2]> {
    match field {
        F::Opacity | F::Strength | F::Sharpness | F::HeightBlend => Some([0.0, 1.0]),
        F::Falloff => match picked(world, entity) {
            Some((_, Layer { kind: LayerKind::Stamp { .. }, .. })) => Some([0.0, 1.0]),
            _ => None,
        },
        _ => None,
    }
}

/// An asset's path, by the reference a file holds, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

pub fn get(game: &Game, entity: Entity, field: TerrainField) -> Option<Value> {
    let world = &game.engine.world;
    let asset = || world.get::<TerrainAsset>(entity);
    let layer = || picked(world, entity).map(|(_, layer)| layer);
    let surface = || picked_surface(world, entity).map(|(_, surface)| surface);
    let mask = || shown_mask(world, entity);
    let brush = ui(world).brush;
    let choice = |at: usize, names: Vec<&str>| Value::Choice(at, names.into_iter().map(str::to_string).collect());
    Some(match field {
        F::Data => Value::Asset(path_of(game, Some(&world.get::<TerrainSource>(entity)?.0.data))),
        F::Size => Value::Number(asset()?.file.size),
        F::Resolution => {
            let resolution = asset()?.file.resolution;
            let at = RESOLUTIONS.iter().position(|&r| r == resolution).unwrap_or_default();
            Value::Choice(at, RESOLUTIONS.iter().map(|r| r.to_string()).collect())
        }
        F::Base => Value::Number(asset()?.file.base),
        F::Material => {
            let look = world.get::<TerrainLook>(entity)?;
            Value::Asset(match game.content.material_source(look.material)? {
                MaterialSource::Inline(data) if *data == default_material() => "Default".into(),
                MaterialSource::Asset(reference) => path_of(game, Some(&reference.to_string())),
                MaterialSource::Inline(_) => "Custom".into(),
            })
        }
        F::TextureSize => Value::Number(world.get::<TerrainLook>(entity)?.texture_size),
        F::Detail => Value::Number(world.get::<Terrain>(entity)?.detail_distance),
        F::Shadows => Value::Toggle(world.get::<Terrain>(entity)?.cast_shadows),
        F::LayerName => Value::Text(layer()?.name),
        F::Blend => {
            let blend = layer()?.blend;
            choice(
                Blend::ALL.iter().position(|&b| b == blend).unwrap_or_default(),
                Blend::ALL.map(Blend::name).to_vec(),
            )
        }
        F::Opacity => Value::Number(layer()?.opacity),
        F::HeightMask => Value::Toggle(mask()?.height.is_some()),
        F::Heights => Value::Vector(mask()?.height?.to_vec()),
        F::SlopeMask => Value::Toggle(mask()?.slope.is_some()),
        F::Slopes => Value::Vector(mask()?.slope?.to_vec()),
        F::Feather => Value::Number(mask()?.feather),
        F::SurfaceMaterial => Value::Asset(path_of(game, surface()?.material.as_deref())),
        F::NewSurface => Value::Asset(String::new()),
        F::TileSize => Value::Number(surface()?.size),
        F::HeightBlend => Value::Number(surface()?.sharpness),
        F::Triplanar => Value::Toggle(surface()?.triplanar),
        F::Cover => {
            let cover = surface()?.cover;
            choice(Cover::ALL.iter().position(|&c| c == cover).unwrap_or_default(), Cover::ALL.map(Cover::name).to_vec())
        }
        F::Breakup => Value::Number(surface()?.breakup),
        F::BreakupSize => Value::Number(surface()?.breakup_size),
        F::Brush => {
            let at = BrushKind::ALL.iter().position(|&k| k == brush.kind).unwrap_or_default();
            choice(at, BrushKind::ALL.map(BrushKind::name).to_vec())
        }
        F::BrushSize => Value::Number(brush.radius),
        F::Strength => Value::Number(brush.strength),
        F::BrushHeight => Value::Number(brush.height),
        F::BrushStep => Value::Number(brush.step),
        F::BumpSize => Value::Number(brush.noise_size),
        _ => match (field, layer()?.kind) {
            (F::Picture, LayerKind::Import { picture, .. } | LayerKind::Stamp { picture, .. }) => {
                Value::Asset(path_of(game, picture.as_deref()))
            }
            (
                F::Height,
                LayerKind::Import { height, .. } | LayerKind::Stamp { height, .. } | LayerKind::Noise { height, .. },
            ) => Value::Number(height),
            (F::Position, LayerKind::Stamp { position, .. }) => Value::Vector(position.to_vec()),
            (F::Rotation, LayerKind::Stamp { rotation, .. }) => Value::Number(rotation),
            (F::StampSize, LayerKind::Stamp { size, .. }) => Value::Vector(size.to_vec()),
            (F::Falloff, LayerKind::Stamp { falloff, .. } | LayerKind::Spline { falloff, .. }) => {
                Value::Number(falloff)
            }
            (F::Style, LayerKind::Noise { style, .. }) => {
                let at = NoiseStyle::ALL.iter().position(|&s| s == style).unwrap_or_default();
                choice(at, NoiseStyle::ALL.map(NoiseStyle::name).to_vec())
            }
            (F::Scale, LayerKind::Noise { scale, .. }) => Value::Number(scale),
            (F::Octaves, LayerKind::Noise { octaves, .. }) => Value::Whole(i64::from(octaves)),
            (F::Seed, LayerKind::Noise { seed, .. }) => Value::Whole(i64::from(seed)),
            (F::Width, LayerKind::Spline { width, .. }) => Value::Number(width),
            (F::Step, LayerKind::Terrace { step, .. }) => Value::Number(step),
            (F::Sharpness, LayerKind::Terrace { sharpness, .. }) => Value::Number(sharpness),
            (F::Radius, LayerKind::Smooth { radius }) => Value::Number(radius),
            _ => return None,
        },
    })
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Whole(n) => Some(*n as f32),
        _ => None,
    }
}

fn pair(value: &Value) -> Option<[f32; 2]> {
    match value {
        Value::Vector(v) if v.len() >= 2 => Some([v[0], v[1]]),
        _ => None,
    }
}

/// Sets a number, a toggle, a vector, text or a choice. The asset's size,
/// resolution and base, and a layer's values, make the ground again; the
/// brush's are the editor's.
pub fn set(game: &mut Game, entity: Entity, field: TerrainField, value: Value) -> bool {
    let world = &mut game.engine.world;
    match field {
        F::TextureSize => {
            let Some(size) = number(&value) else { return false };
            return change(world, entity, |look: &mut TerrainLook| look.texture_size = size.max(0.01));
        }
        F::Detail => {
            let Some(distance) = number(&value) else { return false };
            return change(world, entity, |terrain: &mut Terrain| terrain.detail_distance = distance.max(1.0));
        }
        F::Shadows => {
            let Value::Toggle(on) = value else { return false };
            return change(world, entity, |terrain: &mut Terrain| terrain.cast_shadows = on);
        }
        F::Size => {
            let Some(size) = number(&value) else { return false };
            return reshape(game, entity, |file| file.size = size.clamp(1.0, 100_000.0));
        }
        F::Resolution => {
            let Value::Choice(at, _) = value else { return false };
            let Some(&resolution) = RESOLUTIONS.get(at) else { return false };
            return reshape(game, entity, |file| file.resolution = resolution);
        }
        F::Base => {
            let Some(base) = number(&value) else { return false };
            return reshape(game, entity, |file| file.base = base);
        }
        F::Brush | F::BrushSize | F::Strength | F::BrushHeight | F::BrushStep | F::BumpSize => {
            let brush = &mut ui_mut(world).brush;
            let before = *brush;
            match (field, &value) {
                (F::Brush, Value::Choice(at, _)) => brush.kind = BrushKind::ALL[(*at).min(BrushKind::ALL.len() - 1)],
                (F::BrushSize, v) => brush.radius = number(v).unwrap_or(brush.radius).clamp(0.1, 1000.0),
                (F::Strength, v) => brush.strength = number(v).unwrap_or(brush.strength).clamp(0.0, 1.0),
                (F::BrushHeight, v) => brush.height = number(v).unwrap_or(brush.height),
                (F::BrushStep, v) => brush.step = number(v).unwrap_or(brush.step).max(0.01),
                (F::BumpSize, v) => brush.noise_size = number(v).unwrap_or(brush.noise_size).max(0.1),
                _ => {}
            }
            return *brush != before;
        }
        F::TileSize | F::HeightBlend | F::Triplanar | F::Cover | F::Breakup | F::BreakupSize => {
            return change_surface(game, entity, |surface| set_surface(surface, field, &value));
        }
        F::HeightMask | F::Heights | F::SlopeMask | F::Slopes | F::Feather
            if ui(world).tool == TerrainTool::Paint =>
        {
            return change_surface(game, entity, |surface| set_mask(&mut surface.mask, field, &value));
        }
        _ => {}
    }
    change_layer(game, entity, |layer| set_layer(layer, field, &value))
}

/// Sets one of a mask's values.
fn set_mask(mask: &mut Mask, field: TerrainField, value: &Value) {
    match (field, value) {
        (F::HeightMask, Value::Toggle(on)) => mask.height = on.then_some(mask.height.unwrap_or([0.0, 100.0])),
        (F::Heights, v) => mask.height = pair(v).map(|[a, b]| [a.min(b), a.max(b)]),
        (F::SlopeMask, Value::Toggle(on)) => mask.slope = on.then_some(mask.slope.unwrap_or([0.0, 30.0])),
        (F::Slopes, v) => {
            mask.slope = pair(v).map(|[a, b]| [a.min(b).clamp(0.0, 90.0), a.max(b).clamp(0.0, 90.0)]);
        }
        (F::Feather, v) => mask.feather = number(v).unwrap_or(mask.feather).max(0.0),
        _ => {}
    }
}

/// Sets one of a surface's own values.
fn set_surface(surface: &mut Surface, field: TerrainField, value: &Value) {
    match (field, value) {
        (F::TileSize, v) => surface.size = number(v).unwrap_or(surface.size).clamp(0.01, 10_000.0),
        (F::HeightBlend, v) => surface.sharpness = number(v).unwrap_or(surface.sharpness).clamp(0.0, 1.0),
        (F::Triplanar, Value::Toggle(on)) => surface.triplanar = *on,
        (F::Cover, Value::Choice(at, _)) => surface.cover = Cover::ALL[(*at).min(Cover::ALL.len() - 1)],
        (F::Breakup, v) => surface.breakup = number(v).unwrap_or(surface.breakup).max(0.0),
        (F::BreakupSize, v) => surface.breakup_size = number(v).unwrap_or(surface.breakup_size).max(0.1),
        _ => {}
    }
}

/// Sets one of a layer's own values.
fn set_layer(layer: &mut Layer, field: TerrainField, value: &Value) {
    match (field, value, &mut layer.kind) {
        (F::LayerName, Value::Text(name), _) => layer.name = name.clone(),
        (F::Blend, Value::Choice(at, _), _) => layer.blend = Blend::ALL[(*at).min(Blend::ALL.len() - 1)],
        (F::Opacity, v, _) => layer.opacity = number(v).unwrap_or(layer.opacity).clamp(0.0, 1.0),
        (F::HeightMask | F::Heights | F::SlopeMask | F::Slopes | F::Feather, v, _) => set_mask(&mut layer.mask, field, v),
        (
            F::Height,
            v,
            LayerKind::Import { height, .. } | LayerKind::Stamp { height, .. } | LayerKind::Noise { height, .. },
        ) => *height = number(v).unwrap_or(*height),
        (F::Position, v, LayerKind::Stamp { position, .. }) => *position = pair(v).unwrap_or(*position),
        (F::Rotation, v, LayerKind::Stamp { rotation, .. }) => *rotation = number(v).unwrap_or(*rotation),
        (F::StampSize, v, LayerKind::Stamp { size, .. }) => *size = pair(v).map_or(*size, |s| s.map(|n| n.max(0.1))),
        (F::Falloff, v, LayerKind::Stamp { falloff, .. }) => *falloff = number(v).unwrap_or(*falloff).clamp(0.0, 1.0),
        (F::Falloff, v, LayerKind::Spline { falloff, .. }) => *falloff = number(v).unwrap_or(*falloff).max(0.0),
        (F::Style, Value::Choice(at, _), LayerKind::Noise { style, .. }) => {
            *style = NoiseStyle::ALL[(*at).min(NoiseStyle::ALL.len() - 1)];
        }
        (F::Scale, v, LayerKind::Noise { scale, .. }) => *scale = number(v).unwrap_or(*scale).max(0.01),
        (F::Octaves, v, LayerKind::Noise { octaves, .. }) => {
            *octaves = number(v).map_or(*octaves, |n| n.clamp(1.0, 12.0) as u32);
        }
        (F::Seed, v, LayerKind::Noise { seed, .. }) => *seed = number(v).map_or(*seed, |n| n.max(0.0) as u32),
        (F::Width, v, LayerKind::Spline { width, .. }) => *width = number(v).unwrap_or(*width).max(0.1),
        (F::Step, v, LayerKind::Terrace { step, .. }) => *step = number(v).unwrap_or(*step).max(0.01),
        (F::Sharpness, v, LayerKind::Terrace { sharpness, .. }) => {
            *sharpness = number(v).unwrap_or(*sharpness).clamp(0.0, 1.0);
        }
        (F::Radius, v, LayerKind::Smooth { radius }) => *radius = number(v).unwrap_or(*radius).max(0.0),
        _ => {}
    }
}

fn change<T: voxear_ecs::Component>(world: &mut World, entity: Entity, how: impl FnOnce(&mut T)) -> bool {
    let Some(component) = world.get_mut::<T>(entity) else { return false };
    how(component);
    true
}

/// What to make again for a layer changed from `before` to `after`: where
/// either reaches, or everywhere.
pub fn dirty_between(before: Option<&Layer>, after: Option<&Layer>) -> Dirty {
    let areas: Option<Vec<Area>> = [before, after].into_iter().flatten().map(|layer| layer.kind.area()).collect();
    match areas {
        Some(areas) if !areas.is_empty() => Dirty::Areas(areas),
        _ => Dirty::All,
    }
}

/// Changes the picked layer and makes the ground again where it reached
/// before and reaches now.
fn change_layer(game: &mut Game, entity: Entity, how: impl FnOnce(&mut Layer)) -> bool {
    let world = &game.engine.world;
    let Some((at, before)) = picked(world, entity) else { return false };
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    how(&mut file.layers[at]);
    if file.layers[at] == before {
        return false;
    }
    let dirty = dirty_between(Some(&before), Some(&file.layers[at]));
    let rasters = asset.rasters.clone();
    remake(game, entity, file, rasters, &dirty)
}

/// Puts `file` and `rasters` on the terrain's asset and makes the `dirty`
/// part of its ground again from them.
pub fn remake(game: &mut Game, entity: Entity, file: TerrainFile, rasters: Arc<Rasters>, dirty: &Dirty) -> bool {
    remake_with(game, entity, file, rasters, dirty, &Compositor::default())
}

/// As [`remake`], with a compositor that may be keeping the ground below a
/// layer being edited.
pub fn remake_with(
    game: &mut Game,
    entity: Entity,
    file: TerrainFile,
    rasters: Arc<Rasters>,
    dirty: &Dirty,
    compositor: &Compositor,
) -> bool {
    let Some(previous) = game.engine.world.get::<Terrain>(entity).map(|terrain| terrain.heights.clone()) else {
        return false;
    };
    let heights = game.content.compose_terrain(&file, &rasters, Some(&previous), compositor, dirty);
    let world = &mut game.engine.world;
    let changed = change(world, entity, |terrain: &mut Terrain| terrain.heights = Arc::new(heights))
        && change(world, entity, |asset: &mut TerrainAsset| (asset.file, asset.rasters) = (file, rasters));
    changed && reweigh(game, entity, dirty)
}

/// Works out the surfaces' shares again where `dirty` says, over the
/// terrain's ground as it is now, from its asset's surfaces and paint.
pub fn reweigh(game: &mut Game, entity: Entity, dirty: &Dirty) -> bool {
    let world = &mut game.engine.world;
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    let Some(terrain) = world.get::<Terrain>(entity) else { return false };
    let previous = terrain.weights.as_deref();
    let weights = voxear_terrain::surfaces::weigh(&asset.file, &terrain.heights, &asset.rasters, previous, dirty);
    change(world, entity, |terrain: &mut Terrain| terrain.weights = weights.map(Arc::new))
}

/// Changes the picked surface, then its look and, if where it covers
/// changed, everyone's shares.
fn change_surface(game: &mut Game, entity: Entity, how: impl FnOnce(&mut Surface)) -> bool {
    let world = &game.engine.world;
    let Some((at, before)) = picked_surface(world, entity) else { return false };
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    how(&mut file.surfaces[at]);
    if file.surfaces[at] == before {
        return false;
    }
    let covers = voxear_terrain::surfaces::hash_surfaces(&asset.file.surfaces)
        != voxear_terrain::surfaces::hash_surfaces(&file.surfaces);
    let world = &mut game.engine.world;
    change(world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    if covers {
        reweigh(game, entity, &Dirty::All);
    }
    true
}

/// Gives the terrain's look its asset's surfaces again, each material
/// loaded, where there is a renderer's look to change.
fn restyle(game: &mut Game, entity: Entity) {
    let Some(file) = game.engine.world.get::<TerrainAsset>(entity).map(|asset| asset.file.clone()) else { return };
    let Some(mut look) = game.engine.world.get::<TerrainLook>(entity).cloned() else { return };
    let mut surfaces = Vec::with_capacity(file.surfaces.len());
    for surface in &file.surfaces {
        let material = surface.material.as_deref().and_then(|reference| {
            let parsed = game.content.parse(reference).ok()?;
            game.content.loaded_material(&MaterialSource::Asset(parsed))
        });
        surfaces.push(voxear_render::SurfaceLook {
            material: material.unwrap_or(look.material),
            size: surface.size.max(0.01),
            sharpness: surface.sharpness,
            triplanar: surface.triplanar,
        });
    }
    look.surfaces = surfaces.into();
    game.engine.world.insert(entity, look);
}

/// Changes the terrain's size, resolution or base and makes all its ground
/// again, its rasters stretched to fit.
fn reshape(game: &mut Game, entity: Entity, how: impl FnOnce(&mut TerrainFile)) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    how(&mut file);
    if file == asset.file {
        return false;
    }
    let old = ([asset.file.tiles(); 2], asset.file.spacing());
    let new = ([file.tiles(); 2], file.spacing());
    let rasters = if old == new { asset.rasters.clone() } else { Arc::new(asset.rasters.resampled(old, new)) };
    remake(game, entity, file, rasters, &Dirty::All)
}

/// What a picker shows: the project's terrains, its pictures for a layer,
/// or its materials.
pub fn choices(game: &Game, field: TerrainField) -> Vec<(String, Pick)> {
    match field {
        F::Data => fields::assets_of(game, Some(AssetKind::Terrain)),
        F::Picture => fields::assets_of(game, Some(AssetKind::Texture)),
        F::SurfaceMaterial => fields::assets_of(game, Some(AssetKind::Material)),
        // A new surface is of a material.
        F::NewSurface => {
            let mut found = fields::assets_of(game, Some(AssetKind::Material));
            found.retain(|(_, pick)| *pick != Pick::Asset(None));
            found
        }
        F::Material => {
            let mut found = vec![("Default".to_string(), Pick::Asset(None))];
            let mut paths: Vec<String> = (game.content.db().entries())
                .filter(|entry| entry.kind == AssetKind::Material)
                .map(|entry| entry.path.clone())
                .collect();
            paths.sort();
            found.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
            found
        }
        _ => Vec::new(),
    }
}

/// Whether a value is picked from a menu of assets.
pub fn is_picked(field: TerrainField) -> bool {
    matches!(field, F::Data | F::Picture | F::Material | F::SurfaceMaterial | F::NewSurface)
}

/// Takes what was picked: another asset for its ground, a picture for its
/// picked layer, or a material.
pub fn pick(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    field: TerrainField,
    choice: &Pick,
) -> bool {
    let Pick::Asset(path) = choice else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference),
            Err(_) => return false,
        },
        None => None,
    };
    match field {
        F::Data => {
            let Some(source) = game.engine.world.get::<TerrainSource>(entity).cloned() else { return false };
            let data = TerrainData { data: reference.map_or_else(String::new, |r| r.to_string()), ..source.0 };
            match voxear_engine::prefab::terrain(&mut game.content, renderer, &data) {
                Ok(parts) => {
                    put(&mut game.engine.world, entity, parts);
                    ui_mut(&mut game.engine.world).layer = 0;
                    true
                }
                Err(error) => {
                    tracing::error!("{error:#}");
                    false
                }
            }
        }
        F::Picture => {
            let written = reference.map(|r| r.to_string());
            change_layer(game, entity, |layer| {
                if let LayerKind::Import { picture, .. } | LayerKind::Stamp { picture, .. } = &mut layer.kind {
                    *picture = written;
                }
            })
        }
        F::SurfaceMaterial | F::NewSurface => {
            let written = reference.as_ref().map(|r| r.to_string());
            // The material loaded now, so the look can take it.
            if let (Some(renderer), Some(reference)) = (renderer, &reference)
                && let Err(error) = game.content.material(renderer, &MaterialSource::Asset(reference.clone()))
            {
                tracing::error!("{error:#}");
            }
            if field == F::NewSurface {
                return add_surface(game, entity, written);
            }
            change_surface(game, entity, |surface| surface.material = written)
        }
        F::Material => {
            let Some(renderer) = renderer else { return false };
            let source = match &reference {
                None => MaterialSource::Inline(default_material()),
                Some(reference) => MaterialSource::Asset(reference.clone()),
            };
            let Ok(material) = game.content.material(renderer, &source) else { return false };
            let world = &mut game.engine.world;
            let written = reference.map(|r| MaterialRef::Asset(r.to_string()));
            change(world, entity, |look: &mut TerrainLook| look.material = material)
                && change(world, entity, |source: &mut TerrainSource| source.0.material = written)
        }
        _ => false,
    }
}

/// Puts a Terrain's parts on an Obj, in place of any it had.
fn put(world: &mut World, entity: Entity, parts: voxear_engine::prefab::TerrainParts) {
    world.insert(entity, parts.terrain);
    match parts.look {
        Some(look) => world.insert(entity, look),
        None => world.remove::<TerrainLook>(entity).is_some(),
    };
    match parts.asset {
        Some(asset) => world.insert(entity, asset),
        None => world.remove::<TerrainAsset>(entity).is_some(),
    };
    world.insert(entity, parts.source);
}

/// Add Component's Terrain: flat ground on a new asset of its own, named
/// for its Obj, in the project's Terrains folder.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<()> {
    let name = game.engine.world.get::<voxear_scene::Name>(entity).map_or("Terrain".into(), |n| n.0.clone());
    let root = game.project.root.clone();
    let path = crate::project_files::create_terrain(&root, FOLDER, &name)?;
    game.content.rescan()?;
    let guid = game.content.parse(&path)?.guid().with_context(|| format!("{path} is not an asset"))?;
    let data = TerrainData {
        data: guid.to_string(),
        material: None,
        texture_size: TerrainLook::DEFAULT_TEXTURE_SIZE,
        detail_distance: Terrain::DEFAULT_DETAIL_DISTANCE,
        cast_shadows: true,
    };
    let parts = voxear_engine::prefab::terrain(&mut game.content, renderer, &data)?;
    put(&mut game.engine.world, entity, parts);
    Ok(())
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<Terrain>(entity);
    world.remove::<TerrainLook>(entity);
    world.remove::<TerrainAsset>(entity);
    world.remove::<TerrainSource>(entity);
}

/// Adds a surface of `material` on top, and picks it: the first one covers
/// all the ground, those after it only where painted.
pub fn add_surface(game: &mut Game, entity: Entity, material: Option<String>) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if file.surfaces.len() >= MAX_SURFACES {
        return false;
    }
    let mut surface = Surface::new(material);
    if file.surfaces.is_empty() {
        surface.cover = Cover::Everywhere;
    }
    file.surfaces.push(surface);
    ui_mut(&mut game.engine.world).surface = file.surfaces.len() - 1;
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// Picks surface `at` to paint with and show.
pub fn pick_surface(world: &mut World, at: usize) {
    ui_mut(world).surface = at;
}

/// Moves surface `from` to place `to`, keeping it picked, its painting
/// with it.
pub fn move_surface(game: &mut Game, entity: Entity, from: usize, to: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if from >= file.surfaces.len() || to >= file.surfaces.len() || from == to {
        return false;
    }
    let surface = file.surfaces.remove(from);
    file.surfaces.insert(to, surface);
    ui_mut(&mut game.engine.world).surface = to;
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// Takes surface `at` out, and its painting with it.
pub fn remove_surface(game: &mut Game, entity: Entity, at: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if at >= file.surfaces.len() {
        return false;
    }
    let surface = file.surfaces.remove(at);
    let mut rasters = asset.rasters.clone();
    if surface.paint != 0 {
        Arc::make_mut(&mut rasters).remove(surface.paint);
    }
    let ui = ui_mut(&mut game.engine.world);
    ui.surface = ui.surface.min(file.surfaces.len().saturating_sub(1));
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| (asset.file, asset.rasters) = (file, rasters));
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// The kinds of layer Add offers, in its menu's order.
pub const NEW_LAYERS: [&str; 7] = ["Stamp", "Noise", "Sculpt", "Spline", "Terrace", "Smooth", "Import"];

/// A new layer of the kind `NEW_LAYERS[choice]` names, to start from on a
/// terrain as `file` says, numbering a sculpt layer's raster past `rasters`'.
pub fn new_layer(choice: usize, file: &TerrainFile, rasters: &Rasters) -> Layer {
    let (size, middle) = (file.size, file.size * 0.5);
    let kind = match NEW_LAYERS.get(choice).copied().unwrap_or("Stamp") {
        "Noise" => LayerKind::Noise {
            style: NoiseStyle::Fractal,
            scale: (size * 0.25).max(10.0),
            height: 20.0,
            octaves: 5,
            seed: 0,
        },
        "Sculpt" => LayerKind::Sculpt { raster: rasters.next_raster(file.rasters_in_use()) },
        "Spline" => {
            let y = file.base;
            LayerKind::Spline {
                points: vec![[size * 0.3, y, middle], [size * 0.7, y, middle]],
                width: 8.0,
                falloff: 4.0,
            }
        }
        "Terrace" => LayerKind::Terrace { step: 4.0, sharpness: 0.5 },
        "Smooth" => LayerKind::Smooth { radius: 2.0 },
        "Import" => LayerKind::Import { picture: None, height: 100.0 },
        _ => {
            let across = (size / 8.0).max(4.0);
            LayerKind::Stamp {
                picture: None,
                position: [middle, middle],
                rotation: 0.0,
                size: [across; 2],
                height: across * 0.25,
                falloff: 0.25,
            }
        }
    };
    Layer::new(kind)
}

/// Adds a layer of the kind `NEW_LAYERS[choice]` names on top, and picks it.
pub fn add_layer(game: &mut Game, entity: Entity, choice: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    let layer = new_layer(choice, &file, &asset.rasters);
    let dirty = dirty_between(None, Some(&layer));
    file.layers.push(layer);
    let (at, rasters) = (file.layers.len() - 1, asset.rasters.clone());
    ui_mut(&mut game.engine.world).layer = at;
    remake(game, entity, file, rasters, &dirty)
}

/// The sculpt layer the brush paints into: the picked layer if it is one,
/// else the topmost, else a new one on top, added and picked. Its place,
/// and its raster.
pub fn sculpt_layer(game: &mut Game, entity: Entity) -> Option<(usize, u32)> {
    let world = &game.engine.world;
    let file = &world.get::<TerrainAsset>(entity)?.file;
    let raster_of = |at: usize| match file.layers.get(at)?.kind {
        LayerKind::Sculpt { raster } => Some((at, raster)),
        _ => None,
    };
    let topmost = (0..file.layers.len()).rev().find_map(raster_of);
    if let Some(found) = raster_of(ui(world).layer).or(topmost) {
        ui_mut(&mut game.engine.world).layer = found.0;
        return Some(found);
    }
    let sculpt = NEW_LAYERS.iter().position(|&kind| kind == "Sculpt").expect("Sculpt is offered");
    add_layer(game, entity, sculpt);
    let file = &game.engine.world.get::<TerrainAsset>(entity)?.file;
    let at = file.layers.len() - 1;
    match file.layers[at].kind {
        LayerKind::Sculpt { raster } => Some((at, raster)),
        _ => None,
    }
}

/// The surface the Paint brush paints: the picked one, ready to paint with
/// those above it; `None` for a terrain with no surfaces.
pub fn paint_surface(game: &mut Game, entity: Entity) -> Option<usize> {
    let world = &mut game.engine.world;
    let asset = world.get::<TerrainAsset>(entity)?;
    if asset.file.surfaces.is_empty() {
        return None;
    }
    let surface = ui(world).surface.min(asset.file.surfaces.len() - 1);
    let mut file = asset.file.clone();
    if voxear_terrain::surfaces::ready_to_paint(&mut file, &asset.rasters, surface) {
        change(world, entity, |asset: &mut TerrainAsset| asset.file = file);
    }
    ui_mut(world).surface = surface;
    Some(surface)
}

/// Gives the terrain a raster for its holes, if it has none yet; whether it
/// has one.
pub fn ready_for_holes(game: &mut Game, entity: Entity) -> bool {
    let world = &mut game.engine.world;
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    if asset.file.holes == 0 {
        let raster = asset.rasters.next_raster(asset.file.rasters_in_use());
        change(world, entity, |asset: &mut TerrainAsset| asset.file.holes = raster);
    }
    true
}

/// Puts painted `rasters` on the terrain's asset and works the surfaces'
/// shares out again where `dirty` says.
pub fn repaint(game: &mut Game, entity: Entity, rasters: Arc<Rasters>, dirty: &Dirty) -> bool {
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.rasters = rasters)
        && reweigh(game, entity, dirty)
}

/// Picks layer `at` to show.
pub fn pick_layer(world: &mut World, at: usize) {
    ui_mut(world).layer = at;
}

/// Shows tool `tool`.
pub fn show_tool(world: &mut World, tool: TerrainTool) {
    ui_mut(world).tool = tool;
}

/// Switches layer `at` on or off.
pub fn toggle_layer(game: &mut Game, entity: Entity, at: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    let Some(layer) = file.layers.get_mut(at) else { return false };
    layer.enabled = !layer.enabled;
    let dirty = dirty_between(Some(layer), None);
    let rasters = asset.rasters.clone();
    remake(game, entity, file, rasters, &dirty)
}

/// Moves layer `from` to place `to`, keeping it picked.
pub fn move_layer(game: &mut Game, entity: Entity, from: usize, to: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if from >= file.layers.len() || to >= file.layers.len() || from == to {
        return false;
    }
    let layer = file.layers.remove(from);
    let dirty = dirty_between(Some(&layer), None);
    file.layers.insert(to, layer);
    let rasters = asset.rasters.clone();
    ui_mut(&mut game.engine.world).layer = to;
    remake(game, entity, file, rasters, &dirty)
}

/// Takes layer `at` out, and what its brushes painted with it.
pub fn remove_layer(game: &mut Game, entity: Entity, at: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if at >= file.layers.len() {
        return false;
    }
    let layer = file.layers.remove(at);
    let mut rasters = asset.rasters.clone();
    if let LayerKind::Sculpt { raster } = layer.kind {
        Arc::make_mut(&mut rasters).remove(raster);
    }
    let dirty = dirty_between(Some(&layer), None);
    let ui = ui_mut(&mut game.engine.world);
    ui.layer = ui.layer.min(file.layers.len().saturating_sub(1));
    remake(game, entity, file, rasters, &dirty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_layer_shows_its_own_values_and_a_mask_shows_its_ranges_once_on() {
        let stamp = new_layer(0, &TerrainFile::default(), &Rasters::default());
        let fields = layer_fields(&stamp);
        assert!(fields.contains(&F::Position) && fields.contains(&F::Picture) && !fields.contains(&F::Heights));
        let mut masked = stamp.clone();
        masked.mask.height = Some([0.0, 10.0]);
        assert!(layer_fields(&masked).contains(&F::Heights) && layer_fields(&masked).contains(&F::Feather));
        let sculpt = new_layer(2, &TerrainFile::default(), &Rasters::default());
        assert!(matches!(sculpt.kind, LayerKind::Sculpt { raster: 1 }));
        assert!(!layer_fields(&sculpt).contains(&F::Blend), "a sculpt layer adds");
        // A stamp starts in the middle, an eighth of the terrain across.
        assert!(matches!(stamp.kind, LayerKind::Stamp { position: [256.0, 256.0], size: [64.0, 64.0], .. }));
    }
}
