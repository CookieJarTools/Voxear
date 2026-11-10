//! The Terrain component in the Inspector (the Terrain module).
//!
//! Its section has five tools along its top, as Unity's Terrain has
//! (`terrain_panel`): **Layers**, the layers its ground is made of, a list
//! to pick one from with the picked one's settings below; **Sculpt**, the
//! brush the Scene view shapes the ground with, or cuts holes with
//! (`terrain_tools`); **Paint**, its surfaces, a list to pick the one the
//! brush paints, with its settings below; **Grass**, its kinds of grass the
//! same way, the brush growing or digging up the picked one; and
//! **Settings**, its asset, how large and fine it is, and how it is drawn. Which tool shows, which layer
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
    file::{RESOLUTIONS, SPACINGS, TerrainFile},
    layers::{Area, Blend, Layer, LayerKind, Mask},
    noise::NoiseStyle,
    raster::Rasters,
    grass::{Grass, GrassCover, MAX_GRASS},
    surfaces::{Biome, Cover, Liquid, MAX_SURFACES, Surface},
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
    Grass,
    Settings,
}

impl TerrainTool {
    pub const ALL: [TerrainTool; 5] =
        [TerrainTool::Layers, TerrainTool::Sculpt, TerrainTool::Paint, TerrainTool::Grass, TerrainTool::Settings];

    pub fn name(self) -> &'static str {
        match self {
            TerrainTool::Layers => "Layers",
            TerrainTool::Sculpt => "Sculpt",
            TerrainTool::Paint => "Paint",
            TerrainTool::Grass => "Grass",
            TerrainTool::Settings => "Settings",
        }
    }

    /// Whether the Scene view's brush works with it.
    pub fn brushes(self) -> bool {
        matches!(self, TerrainTool::Sculpt | TerrainTool::Paint | TerrainTool::Grass)
    }
}

/// What the editor is doing with terrains: the tool shown, the layer and
/// the surface picked, and the brush. Kept in the edit world, not saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainUi {
    pub tool: TerrainTool,
    pub layer: usize,
    pub surface: usize,
    pub grass: usize,
    pub brush: Brush,
    /// The surface the Dig brush lays on what it bares, by its number.
    pub bares: Option<usize>,
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
    /// Whether the terrain goes on without end, made round the camera, and
    /// then how far apart its samples are and how far round it is made.
    Endless,
    Spacing,
    ViewDistance,
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
    /// A Generate layer's world generator, and where the terrain's corner
    /// is in its world.
    World,
    Offset,
    /// A surface's climate rules: a biome to set them to at once, and its
    /// temperatures and humidities, each switched on or off.
    Biome,
    TemperatureMask,
    Temperatures,
    HumidityMask,
    Humidities,
    /// An Erosion layer's weather: rain a step, how many steps, how readily
    /// water picks up ground, and the steepest loose ground stands.
    Rain,
    Steps,
    Wear,
    SlumpAngle,
    /// A Rivers layer's: how much ground drains into a river before it
    /// starts, and how wide and deep a river is where a square kilometre does.
    RiverSource,
    RiverWidth,
    RiverDepth,
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
    /// The Dig brush's surface laid on what it bares.
    DigBares,
    /// A Spline layer's trench: whether it is dug, how deep, how its walls
    /// lean, its fire step's height and depth, its parapet's height, which
    /// side is its front, and the surface laid on what it bares.
    TrenchDig,
    TrenchDepth,
    TrenchBatter,
    StepHeight,
    StepWidth,
    Parapet,
    TrenchFront,
    TrenchSurface,
    /// The picked surface's: its material, how large its pictures are, how
    /// sharply it meets the others, whether it is drawn from the side on
    /// steep ground, how far tracks press into it and how soon they fill
    /// in, where it covers, and how far its rules' edges wander.
    SurfaceMaterial,
    TileSize,
    HeightBlend,
    Triplanar,
    Tracks,
    FillsIn,
    Liquid,
    Glitter,
    Cover,
    Breakup,
    BreakupSize,
    /// Add Surface's picker: a new surface of the material picked.
    NewSurface,
    /// The picked kind of grass's: its mesh and material, how many to a
    /// square metre, its sizes, how far it is drawn, where it grows, the
    /// surface it grows on, the steepest ground it grows on, and how soon it
    /// stands up again where it was trodden flat.
    GrassMesh,
    GrassMaterial,
    GrassDensity,
    GrassSizes,
    GrassDistance,
    GrassCover,
    GrassSurface,
    GrassSteepest,
    GrassSpringsBack,
    /// Add Grass's picker: a new kind of grass of the mesh picked.
    NewGrass,
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

/// The picked kind of grass of an Obj's terrain, if its asset has one there.
fn picked_grass(world: &World, entity: Entity) -> Option<(usize, Grass)> {
    let at = ui(world).grass;
    Some((at, world.get::<TerrainAsset>(entity)?.file.grass.get(at)?.clone()))
}

/// What the Grass tool shows: the brush, then the picked kind's values.
fn grass_fields(grass: &Grass) -> Vec<TerrainField> {
    let mut fields = vec![F::BrushSize, F::Strength, F::GrassMesh, F::GrassMaterial, F::GrassDensity, F::GrassSizes];
    fields.extend([F::GrassDistance, F::GrassCover]);
    if grass.cover == GrassCover::Surfaces {
        fields.push(F::GrassSurface);
    }
    fields.extend([F::GrassSteepest, F::GrassSpringsBack]);
    fields
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
    fields.push(F::Tracks);
    if surface.tracks > 0.0 {
        fields.push(F::FillsIn);
    }
    fields.extend([F::Liquid, F::Glitter]);
    if at == 0 {
        return fields;
    }
    fields.push(F::Cover);
    if surface.cover == Cover::Rules {
        fields.extend(mask_fields(&surface.mask));
        fields.extend([F::Biome, F::TemperatureMask]);
        if surface.temperature.is_some() {
            fields.push(F::Temperatures);
        }
        fields.push(F::HumidityMask);
        if surface.humidity.is_some() {
            fields.push(F::Humidities);
        }
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
            if let Some(asset) = world.get::<TerrainAsset>(entity) {
                fields.push(F::Endless);
                match asset.file.endless {
                    Some(_) => fields.extend([F::Spacing, F::ViewDistance, F::Base]),
                    None => fields.extend([F::Size, F::Resolution, F::Base]),
                }
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
            if brush.kind == BrushKind::Dig {
                fields.push(F::DigBares);
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
        TerrainTool::Grass => match picked_grass(world, entity) {
            Some((_, grass)) => grass_fields(&grass),
            None => vec![F::BrushSize, F::Strength],
        },
    };
    fields.into_iter().map(Prop::Terrain).collect()
}

/// What a layer of its kind shows.
fn layer_fields(layer: &Layer) -> Vec<TerrainField> {
    let mut fields = vec![F::LayerName];
    // Digging lays no heights to blend, weigh or mask: a trench has its shape, and a Dig layer its brush strokes.
    match &layer.kind {
        LayerKind::Dig { .. } => return fields,
        LayerKind::Spline { dig: Some(_), .. } => {
            fields.extend([F::Width, F::TrenchDig, F::TrenchDepth, F::TrenchBatter, F::StepHeight, F::StepWidth]);
            fields.extend([F::Parapet, F::TrenchFront, F::TrenchSurface]);
            return fields;
        }
        _ => {}
    }
    // A sculpt layer adds what its brushes painted; how is all it has.
    if !matches!(layer.kind, LayerKind::Sculpt { .. }) {
        fields.push(F::Blend);
    }
    fields.push(F::Opacity);
    fields.extend(match layer.kind {
        LayerKind::Import { .. } => vec![F::Picture, F::Height],
        LayerKind::Stamp { .. } => vec![F::Picture, F::Position, F::Rotation, F::StampSize, F::Height, F::Falloff],
        LayerKind::Generate { .. } => vec![F::World, F::Offset],
        LayerKind::Noise { .. } => vec![F::Style, F::Scale, F::Height, F::Octaves, F::Seed],
        LayerKind::Sculpt { .. } => vec![],
        LayerKind::Spline { .. } => vec![F::Width, F::Falloff, F::TrenchDig],
        LayerKind::Dig { .. } => vec![],
        LayerKind::Terrace { .. } => vec![F::Step, F::Sharpness],
        LayerKind::Smooth { .. } => vec![F::Radius],
        LayerKind::Erosion { .. } => vec![F::Rain, F::Steps, F::Wear, F::SlumpAngle],
        LayerKind::Rivers { .. } => vec![F::RiverSource, F::RiverWidth, F::RiverDepth],
    });
    fields.extend(mask_fields(&layer.mask));
    fields
}

pub fn label(field: TerrainField) -> &'static str {
    match field {
        F::Data => "Data",
        F::Endless => "Endless",
        F::Spacing => "Spacing",
        F::ViewDistance => "View distance",
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
        F::World => "Generator",
        F::Offset => "Offset",
        F::Rain => "Rain",
        F::Steps => "Steps",
        F::Wear => "Wear",
        F::SlumpAngle => "Slump angle",
        F::RiverSource => "Source",
        F::RiverWidth => "Width",
        F::RiverDepth => "Depth",
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
        F::DigBares => "Bares",
        F::TrenchDig => "Dig",
        F::TrenchDepth => "Depth",
        F::TrenchBatter => "Wall lean",
        F::StepHeight => "Step height",
        F::StepWidth => "Step width",
        F::Parapet => "Parapet",
        F::TrenchFront => "Front",
        F::TrenchSurface => "Surface",
        F::SurfaceMaterial | F::NewSurface => "Material",
        F::TileSize => "Tile size",
        F::HeightBlend => "Height blend",
        F::Triplanar => "Triplanar",
        F::Tracks => "Tracks",
        F::FillsIn => "Fills in",
        F::Liquid => "Liquid",
        F::Glitter => "Glitter",
        F::Cover => "Cover",
        F::Biome => "Biome",
        F::TemperatureMask => "Temperature",
        F::Temperatures => "Temperatures",
        F::HumidityMask => "Humidity",
        F::Humidities => "Humidities",
        F::Breakup => "Breakup",
        F::BreakupSize => "Breakup size",
        F::GrassMesh | F::NewGrass => "Mesh",
        F::GrassMaterial => "Material",
        F::GrassDensity => "Density",
        F::GrassSizes => "Sizes",
        F::GrassDistance => "Draw distance",
        F::GrassCover => "Grows",
        F::GrassSurface => "On surface",
        F::GrassSteepest => "Steepest ground",
        F::GrassSpringsBack => "Springs back",
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
        F::ViewDistance => Some("m"),
        F::TrenchDepth | F::StepHeight | F::StepWidth | F::Parapet => Some("m"),
        F::Width | F::Step | F::Radius | F::Heights | F::BrushSize | F::BrushHeight | F::BrushStep | F::BumpSize => {
            Some("m")
        }
        F::TileSize | F::BreakupSize | F::GrassDistance => Some("m"),
        F::GrassDensity => Some("per m²"),
        F::GrassSteepest => Some("°"),
        F::FillsIn | F::GrassSpringsBack => Some("s"),
        F::Rain => Some("mm"),
        F::RiverSource => Some("ha"),
        F::RiverWidth | F::RiverDepth => Some("m"),
        F::SlumpAngle => Some("°"),
        F::Rotation | F::Slopes => Some("°"),
        F::Temperatures => Some("°C"),
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
        F::Opacity | F::Strength | F::Sharpness | F::HeightBlend | F::Tracks | F::Wear => Some([0.0, 1.0]),
        F::GrassSteepest => Some([0.0, 90.0]),
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
    let grass = || picked_grass(world, entity).map(|(_, grass)| grass);
    let mask = || shown_mask(world, entity);
    let brush = ui(world).brush;
    let choice = |at: usize, names: Vec<&str>| Value::Choice(at, names.into_iter().map(str::to_string).collect());
    Some(match field {
        F::Data => Value::Asset(path_of(game, Some(&world.get::<TerrainSource>(entity)?.0.data))),
        F::Endless => Value::Toggle(asset()?.file.endless.is_some()),
        F::Spacing => {
            let spacing = asset()?.file.endless?.spacing;
            let at = SPACINGS.iter().position(|&s| s == spacing).unwrap_or_default();
            Value::Choice(at, SPACINGS.iter().map(|s| format!("{s} m")).collect())
        }
        F::ViewDistance => Value::Number(asset()?.file.endless?.distance),
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
        F::Tracks => Value::Number(surface()?.tracks),
        F::FillsIn => Value::Number(surface()?.fills_in),
        F::Liquid => {
            let liquid = surface()?.liquid;
            let at = Liquid::ALL.iter().position(|&l| l == liquid).unwrap_or_default();
            choice(at, Liquid::ALL.map(Liquid::name).to_vec())
        }
        F::Glitter => Value::Toggle(surface()?.glitter),
        F::Cover => {
            let cover = surface()?.cover;
            choice(Cover::ALL.iter().position(|&c| c == cover).unwrap_or_default(), Cover::ALL.map(Cover::name).to_vec())
        }
        F::Breakup => Value::Number(surface()?.breakup),
        F::BreakupSize => Value::Number(surface()?.breakup_size),
        F::Biome => {
            // Any, each biome, and its own ranges where they are none's.
            let surface = surface()?;
            let mut names = vec!["Any".to_string()];
            names.extend(Biome::ALL.map(|biome| biome.name().to_string()));
            let at = match (surface.biome(), surface.temperature.is_none() && surface.humidity.is_none()) {
                (_, true) => 0,
                (Some(biome), _) => 1 + Biome::ALL.iter().position(|&b| b == biome).unwrap_or_default(),
                (None, false) => {
                    names.push("Custom".to_string());
                    names.len() - 1
                }
            };
            Value::Choice(at, names)
        }
        F::TemperatureMask => Value::Toggle(surface()?.temperature.is_some()),
        F::Temperatures => Value::Vector(surface()?.temperature?.to_vec()),
        F::HumidityMask => Value::Toggle(surface()?.humidity.is_some()),
        F::Humidities => Value::Vector(surface()?.humidity?.to_vec()),
        F::GrassMesh => Value::Asset(grass()?.mesh.map_or("None".into(), |r| crate::props::mesh_label(game, &r))),
        F::NewGrass => Value::Asset(String::new()),
        F::GrassMaterial => Value::Asset(path_of(game, grass()?.material.as_deref())),
        F::GrassDensity => Value::Number(grass()?.density),
        F::GrassSizes => Value::Vector(grass()?.scale.to_vec()),
        F::GrassDistance => Value::Number(grass()?.draw_distance),
        F::GrassCover => {
            let cover = grass()?.cover;
            let names = GrassCover::ALL.map(GrassCover::name).to_vec();
            choice(GrassCover::ALL.iter().position(|&c| c == cover).unwrap_or_default(), names)
        }
        F::GrassSurface => {
            let on = grass()?.surfaces.first().copied().unwrap_or_default() as usize;
            let file = &world.get::<TerrainAsset>(entity)?.file;
            let names: Vec<String> = (0..file.surfaces.len()).map(|at| surface_name(game, &file.surfaces[at], at)).collect();
            Value::Choice(on.min(names.len().saturating_sub(1)), names)
        }
        F::GrassSteepest => Value::Number(grass()?.steepest),
        F::GrassSpringsBack => Value::Number(grass()?.springs_back),
        F::Brush => {
            let at = BrushKind::ALL.iter().position(|&k| k == brush.kind).unwrap_or_default();
            choice(at, BrushKind::ALL.map(BrushKind::name).to_vec())
        }
        F::BrushSize => Value::Number(brush.radius),
        F::Strength => Value::Number(brush.strength),
        F::BrushHeight => Value::Number(brush.height),
        F::BrushStep => Value::Number(brush.step),
        F::BumpSize => Value::Number(brush.noise_size),
        F::DigBares => surface_choice(game, entity, ui(world).bares)?,
        _ => match (field, layer()?.kind) {
            (F::Picture, LayerKind::Import { picture, .. } | LayerKind::Stamp { picture, .. }) => {
                Value::Asset(path_of(game, picture.as_deref()))
            }
            (F::World, LayerKind::Generate { world, .. }) => Value::Asset(path_of(game, world.as_deref())),
            (F::Rain, LayerKind::Erosion { weather, .. }) => Value::Number(weather.rain),
            (F::Steps, LayerKind::Erosion { weather, .. }) => Value::Whole(i64::from(weather.steps)),
            (F::Wear, LayerKind::Erosion { weather, .. }) => Value::Number(weather.wear),
            (F::SlumpAngle, LayerKind::Erosion { weather, .. }) => Value::Number(weather.slump),
            (F::RiverSource, LayerKind::Rivers { settings, .. }) => Value::Number(settings.source),
            (F::RiverWidth, LayerKind::Rivers { settings, .. }) => Value::Number(settings.width),
            (F::RiverDepth, LayerKind::Rivers { settings, .. }) => Value::Number(settings.depth),
            (F::Offset, LayerKind::Generate { position, .. }) => Value::Vector(position.to_vec()),
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
            (F::TrenchDig, LayerKind::Spline { dig, .. }) => Value::Toggle(dig.is_some()),
            (F::TrenchDepth, LayerKind::Spline { dig: Some(trench), .. }) => Value::Number(trench.depth),
            (F::TrenchBatter, LayerKind::Spline { dig: Some(trench), .. }) => Value::Number(trench.batter),
            (F::StepHeight, LayerKind::Spline { dig: Some(trench), .. }) => Value::Number(trench.step_height),
            (F::StepWidth, LayerKind::Spline { dig: Some(trench), .. }) => Value::Number(trench.step_width),
            (F::Parapet, LayerKind::Spline { dig: Some(trench), .. }) => Value::Number(trench.parapet),
            (F::TrenchFront, LayerKind::Spline { dig: Some(trench), .. }) => {
                choice(if trench.front_left { 0 } else { 1 }, vec!["Left", "Right"])
            }
            (F::TrenchSurface, LayerKind::Spline { dig: Some(trench), .. }) => {
                surface_choice(game, entity, trench.surface.map(usize::from))?
            }
            _ => return None,
        },
    })
}

/// A choice of none or one of a terrain's surfaces, `picked` (its number)
/// chosen.
fn surface_choice(game: &Game, entity: Entity, picked: Option<usize>) -> Option<Value> {
    let file = &game.engine.world.get::<TerrainAsset>(entity)?.file;
    let mut names = vec!["None".to_string()];
    names.extend((0..file.surfaces.len()).map(|at| surface_name(game, &file.surfaces[at], at)));
    let at = picked.map_or(0, |picked| (picked + 1).min(names.len() - 1));
    Some(Value::Choice(at, names))
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
        F::Endless => {
            let Value::Toggle(on) = value else { return false };
            return reshape(game, entity, |file| file.endless = on.then(|| file.endless.unwrap_or_default()));
        }
        F::Spacing => {
            let Value::Choice(at, _) = value else { return false };
            let Some(&spacing) = SPACINGS.get(at) else { return false };
            return reshape(game, entity, |file| {
                if let Some(endless) = &mut file.endless {
                    endless.spacing = spacing;
                }
            });
        }
        F::ViewDistance => {
            let Some(distance) = number(&value) else { return false };
            return reshape(game, entity, |file| {
                if let Some(endless) = &mut file.endless {
                    endless.distance = distance.clamp(10.0, 100_000.0);
                }
            });
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
        F::DigBares => {
            let Value::Choice(at, _) = value else { return false };
            let bares = (at > 0).then(|| at - 1);
            return std::mem::replace(&mut ui_mut(world).bares, bares) != bares;
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
        F::TileSize
        | F::HeightBlend
        | F::Triplanar
        | F::Tracks
        | F::FillsIn
        | F::Liquid
        | F::Glitter
        | F::Cover
        | F::Breakup
        | F::BreakupSize
        | F::Biome
        | F::TemperatureMask
        | F::Temperatures
        | F::HumidityMask
        | F::Humidities => {
            return change_surface(game, entity, |surface| set_surface(surface, field, &value));
        }
        F::GrassDensity
        | F::GrassSizes
        | F::GrassDistance
        | F::GrassCover
        | F::GrassSurface
        | F::GrassSteepest
        | F::GrassSpringsBack => {
            return change_grass(game, entity, |grass| set_grass(grass, field, &value));
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

/// Sets one of a kind of grass's own values.
fn set_grass(grass: &mut Grass, field: TerrainField, value: &Value) {
    match (field, value) {
        (F::GrassDensity, v) => grass.density = number(v).unwrap_or(grass.density).clamp(0.0, 400.0),
        (F::GrassSizes, v) => {
            if let Some([a, b]) = pair(v) {
                let [a, b] = [a.max(0.01), b.max(0.01)];
                grass.scale = [a.min(b), a.max(b)];
            }
        }
        (F::GrassDistance, v) => grass.draw_distance = number(v).unwrap_or(grass.draw_distance).clamp(1.0, 1000.0),
        (F::GrassCover, Value::Choice(at, _)) => grass.cover = GrassCover::ALL[(*at).min(GrassCover::ALL.len() - 1)],
        (F::GrassSurface, Value::Choice(at, _)) => grass.surfaces = vec![*at as u32],
        (F::GrassSteepest, v) => grass.steepest = number(v).unwrap_or(grass.steepest).clamp(0.0, 90.0),
        (F::GrassSpringsBack, v) => grass.springs_back = number(v).unwrap_or(grass.springs_back).clamp(0.0, 3600.0),
        _ => {}
    }
}

/// What a surface is called in a list: its material's file name.
fn surface_name(game: &Game, surface: &Surface, at: usize) -> String {
    let path = path_of(game, surface.material.as_deref());
    let name = std::path::Path::new(&path).file_stem().map_or(path.clone(), |stem| stem.to_string_lossy().into());
    format!("{} {name}", at + 1)
}

/// Sets one of a surface's own values.
fn set_surface(surface: &mut Surface, field: TerrainField, value: &Value) {
    match (field, value) {
        (F::TileSize, v) => surface.size = number(v).unwrap_or(surface.size).clamp(0.01, 10_000.0),
        (F::HeightBlend, v) => surface.sharpness = number(v).unwrap_or(surface.sharpness).clamp(0.0, 1.0),
        (F::Triplanar, Value::Toggle(on)) => surface.triplanar = *on,
        (F::Tracks, v) => surface.tracks = number(v).unwrap_or(surface.tracks).clamp(0.0, 1.0),
        (F::FillsIn, v) => surface.fills_in = number(v).unwrap_or(surface.fills_in).max(0.0),
        (F::Liquid, Value::Choice(at, _)) => surface.liquid = Liquid::ALL[(*at).min(Liquid::ALL.len() - 1)],
        (F::Glitter, Value::Toggle(on)) => surface.glitter = *on,
        (F::Cover, Value::Choice(at, _)) => surface.cover = Cover::ALL[(*at).min(Cover::ALL.len() - 1)],
        (F::Breakup, v) => surface.breakup = number(v).unwrap_or(surface.breakup).max(0.0),
        (F::BreakupSize, v) => surface.breakup_size = number(v).unwrap_or(surface.breakup_size).max(0.1),
        (F::Biome, Value::Choice(at, _)) => match at.checked_sub(1).and_then(|at| Biome::ALL.get(at)) {
            Some(biome) => (surface.temperature, surface.humidity) = biome.ranges(),
            // Any: its climate left alone; Custom: as it is.
            None if *at == 0 => (surface.temperature, surface.humidity) = (None, None),
            None => {}
        },
        (F::TemperatureMask, Value::Toggle(on)) => {
            surface.temperature = on.then_some(surface.temperature.unwrap_or([-10.0, 10.0]));
        }
        (F::Temperatures, v) => surface.temperature = pair(v).map(|[a, b]| [a.min(b), a.max(b)]),
        (F::HumidityMask, Value::Toggle(on)) => surface.humidity = on.then_some(surface.humidity.unwrap_or([0.0, 0.5])),
        (F::Humidities, v) => {
            surface.humidity = pair(v).map(|[a, b]| [a.min(b).clamp(0.0, 1.0), a.max(b).clamp(0.0, 1.0)]);
        }
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
        (F::Offset, v, LayerKind::Generate { position, .. }) => *position = pair(v).unwrap_or(*position),
        (F::Rain, v, LayerKind::Erosion { weather, .. }) => weather.rain = number(v).unwrap_or(weather.rain).max(0.0),
        (F::Steps, v, LayerKind::Erosion { weather, .. }) => {
            let most = voxear_render::MOST_EROSION_STEPS as f32;
            weather.steps = number(v).map_or(weather.steps, |n| n.clamp(1.0, most) as u32);
        }
        (F::Wear, v, LayerKind::Erosion { weather, .. }) => {
            weather.wear = number(v).unwrap_or(weather.wear).clamp(0.0, 1.0);
        }
        (F::RiverSource, v, LayerKind::Rivers { settings, .. }) => {
            settings.source = number(v).unwrap_or(settings.source).max(0.01);
        }
        (F::RiverWidth, v, LayerKind::Rivers { settings, .. }) => {
            settings.width = number(v).unwrap_or(settings.width).max(0.1);
        }
        (F::RiverDepth, v, LayerKind::Rivers { settings, .. }) => {
            settings.depth = number(v).unwrap_or(settings.depth).max(0.0);
        }
        (F::SlumpAngle, v, LayerKind::Erosion { weather, .. }) => {
            weather.slump = number(v).unwrap_or(weather.slump).clamp(1.0, 89.0);
        }
        (F::Rotation, v, LayerKind::Stamp { rotation, .. }) => *rotation = number(v).unwrap_or(*rotation),
        (F::StampSize, v, LayerKind::Stamp { size, .. }) => *size = pair(v).map_or(*size, |s| s.map(|n| n.max(0.1))),
        (F::Falloff, v, LayerKind::Stamp { falloff, .. }) => *falloff = number(v).unwrap_or(*falloff).clamp(0.0, 1.0),
        (F::TrenchDig, Value::Toggle(on), LayerKind::Spline { dig, .. }) => *dig = on.then(|| dig.unwrap_or_default()),
        (F::TrenchDepth, v, LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.depth = number(v).unwrap_or(trench.depth).clamp(0.1, 20.0);
        }
        (F::TrenchBatter, v, LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.batter = number(v).unwrap_or(trench.batter).clamp(0.0, 2.0);
        }
        (F::StepHeight, v, LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.step_height = number(v).unwrap_or(trench.step_height).max(0.0);
        }
        (F::StepWidth, v, LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.step_width = number(v).unwrap_or(trench.step_width).max(0.0);
        }
        (F::Parapet, v, LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.parapet = number(v).unwrap_or(trench.parapet).clamp(0.0, 5.0);
        }
        (F::TrenchFront, Value::Choice(at, _), LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.front_left = *at == 0;
        }
        (F::TrenchSurface, Value::Choice(at, _), LayerKind::Spline { dig: Some(trench), .. }) => {
            trench.surface = (*at > 0).then(|| (*at - 1).min(255) as u8);
        }
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
    let Some((previous, dug, origin)) = game
        .engine
        .world
        .get::<Terrain>(entity)
        .map(|terrain| (terrain.heights.clone(), terrain.dug.clone(), terrain.origin))
    else {
        return false;
    };
    // A terrain without end is made round where it is seen as it goes (see `voxear_engine::terrain::stream`), from
    // its file: only that changes here. Just made endless, it starts as holes.
    if file.endless.is_some() {
        let world = &mut game.engine.world;
        if origin.is_none()
            && let Some((window, origin)) = voxear_terrain::endless::unmade_window(&file)
        {
            change(world, entity, |terrain: &mut Terrain| {
                (terrain.heights, terrain.origin) = (Arc::new(window), Some(origin));
                (terrain.weights, terrain.grass, terrain.dug) = (None, None, None);
            });
        }
        return change(world, entity, |asset: &mut TerrainAsset| (asset.file, asset.rasters) = (file, rasters));
    }
    // One just made of a set size is made whole, from its Obj.
    let previous = origin.is_none().then_some(previous);
    if origin.is_some() {
        change(&mut game.engine.world, entity, |terrain: &mut Terrain| terrain.origin = None);
    }
    let heights = game.content.compose_terrain(&file, &rasters, previous.as_deref(), compositor, dirty);
    // Then what the layers dig, kept as it was where they and the heights under it are.
    let kept = dug.as_ref().zip(previous.as_deref()).map(|(dug, previous)| (previous, dug));
    let (heights, dug) = voxear_terrain::dig::redig(&file.layers, heights, kept);
    let world = &mut game.engine.world;
    let ground = (Arc::new(heights), dug);
    let changed = change(world, entity, |terrain: &mut Terrain| (terrain.heights, terrain.dug) = ground)
        && change(world, entity, |asset: &mut TerrainAsset| (asset.file, asset.rasters) = (file, rasters));
    changed && reweigh(game, entity, dirty)
}

/// Works out the surfaces' shares again where `dirty` says, over the
/// terrain's ground as it is now, from its asset's surfaces and paint.
pub fn reweigh(game: &mut Game, entity: Entity, dirty: &Dirty) -> bool {
    let Some(file) = game.engine.world.get::<TerrainAsset>(entity).map(|asset| asset.file.clone()) else {
        return false;
    };
    // A terrain without end's tiles are each made with their shares and grass, again as its file changes.
    if file.endless.is_some() {
        return true;
    }
    let (generator, position) = game.content.terrain_climate(&file);
    let climate = voxear_terrain::surfaces::ClimateSource { world: generator.as_deref(), position };
    let world = &mut game.engine.world;
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    let Some(terrain) = world.get::<Terrain>(entity) else { return false };
    let previous = terrain.weights.as_deref();
    let weights =
        voxear_terrain::surfaces::weigh(&asset.file, &terrain.heights, &asset.rasters, climate, previous, dirty);
    // And the grass, which grows on the surfaces.
    let ground = (terrain.heights.as_ref(), weights.as_ref());
    let grass = voxear_terrain::grass::grow(&asset.file, ground, &asset.rasters, terrain.grass.as_deref(), dirty);
    // None where it is dug open.
    let grass = match (grass, &terrain.dug) {
        (Some(grass), Some(dug)) => Some(voxear_terrain::dig::clear_grass(&grass, &terrain.heights, dug, None)),
        (grass, _) => grass,
    };
    change(world, entity, |terrain: &mut Terrain| {
        terrain.weights = weights.map(Arc::new);
        terrain.grass = grass.map(Arc::new);
    })
}

/// Changes the picked kind of grass, then its look and, if where it grows
/// changed, every kind's densities.
fn change_grass(game: &mut Game, entity: Entity, how: impl FnOnce(&mut Grass)) -> bool {
    let world = &game.engine.world;
    let Some((at, before)) = picked_grass(world, entity) else { return false };
    let Some(asset) = world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    how(&mut file.grass[at]);
    if file.grass[at] == before {
        return false;
    }
    let grows = voxear_terrain::grass::hash_grass(&asset.file.grass, 0) != voxear_terrain::grass::hash_grass(&file.grass, 0);
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    if grows {
        reweigh(game, entity, &Dirty::All);
    }
    true
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
            tracks: surface.tracks,
            fills_in: surface.fills_in,
            liquid: surface.liquid,
            glitter: surface.glitter,
        });
    }
    look.surfaces = surfaces.into();
    let grass: Vec<Option<voxear_render::GrassLook>> = file
        .grass
        .iter()
        .map(|kind| {
            let mesh = game.content.loaded_mesh(&game.content.parse(kind.mesh.as_deref()?).ok()?)?;
            let material = kind.material.as_deref().and_then(|reference| {
                game.content.loaded_material(&MaterialSource::Asset(game.content.parse(reference).ok()?))
            });
            Some(voxear_render::GrassLook {
                mesh,
                material: material.unwrap_or(look.material),
                density: kind.density.max(0.0),
                scale: kind.scale,
                draw_distance: kind.draw_distance.max(1.0),
                springs_back: kind.springs_back.max(0.0),
            })
        })
        .collect();
    look.grass = grass.into();
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
        F::World => fields::assets_of(game, Some(AssetKind::WorldGen)),
        F::SurfaceMaterial | F::GrassMaterial => fields::assets_of(game, Some(AssetKind::Material)),
        F::GrassMesh | F::NewGrass => crate::props::mesh_choices(game),
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
    matches!(
        field,
        F::Data
            | F::Picture
            | F::World
            | F::Material
            | F::SurfaceMaterial
            | F::NewSurface
            | F::GrassMesh
            | F::GrassMaterial
            | F::NewGrass
    )
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
        F::World => {
            let written = reference.map(|r| r.to_string());
            change_layer(game, entity, |layer| {
                if let LayerKind::Generate { world, .. } = &mut layer.kind {
                    *world = written;
                }
            })
        }
        F::GrassMesh | F::GrassMaterial | F::NewGrass => {
            let written = reference.as_ref().map(|r| r.to_string());
            // Loaded now, so the look can take it.
            if let (Some(renderer), Some(reference)) = (renderer, &reference) {
                let loaded = match field {
                    F::GrassMaterial => game.content.material(renderer, &MaterialSource::Asset(reference.clone())).map(drop),
                    _ => game.content.mesh(renderer, reference).map(drop),
                };
                if let Err(error) = loaded {
                    tracing::error!("{error:#}");
                }
            }
            match field {
                F::NewGrass => add_grass(game, entity, written),
                F::GrassMesh => change_grass(game, entity, |grass| grass.mesh = written),
                _ => change_grass(game, entity, |grass| grass.material = written),
            }
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

/// Adds a kind of grass of `mesh` on top, growing everywhere, and picks it.
pub fn add_grass(game: &mut Game, entity: Entity, mesh: Option<String>) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if file.grass.len() >= MAX_GRASS {
        return false;
    }
    file.grass.push(Grass::new(mesh));
    ui_mut(&mut game.engine.world).grass = file.grass.len() - 1;
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// Picks kind `at` of grass to paint with and show.
pub fn pick_grass(world: &mut World, at: usize) {
    ui_mut(world).grass = at;
}

/// Moves kind `from` of grass to place `to`, keeping it picked.
pub fn move_grass(game: &mut Game, entity: Entity, from: usize, to: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if from >= file.grass.len() || to >= file.grass.len() || from == to {
        return false;
    }
    let grass = file.grass.remove(from);
    file.grass.insert(to, grass);
    ui_mut(&mut game.engine.world).grass = to;
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| asset.file = file);
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// Takes kind `at` of grass out, and its painting with it.
pub fn remove_grass(game: &mut Game, entity: Entity, at: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    if at >= file.grass.len() {
        return false;
    }
    let grass = file.grass.remove(at);
    let mut rasters = asset.rasters.clone();
    if grass.paint != 0 {
        Arc::make_mut(&mut rasters).remove(grass.paint);
    }
    let ui = ui_mut(&mut game.engine.world);
    ui.grass = ui.grass.min(file.grass.len().saturating_sub(1));
    change(&mut game.engine.world, entity, |asset: &mut TerrainAsset| (asset.file, asset.rasters) = (file, rasters));
    restyle(game, entity);
    reweigh(game, entity, &Dirty::All)
}

/// The kind of grass the Grass brush paints: the picked one, ready to
/// paint; `None` for a terrain with no grass.
pub fn paint_grass(game: &mut Game, entity: Entity) -> Option<usize> {
    let world = &mut game.engine.world;
    let asset = world.get::<TerrainAsset>(entity)?;
    if asset.file.grass.is_empty() {
        return None;
    }
    let kind = ui(world).grass.min(asset.file.grass.len() - 1);
    let mut file = asset.file.clone();
    if voxear_terrain::grass::ready_to_paint(&mut file, &asset.rasters, kind) {
        change(world, entity, |asset: &mut TerrainAsset| asset.file = file);
    }
    ui_mut(world).grass = kind;
    Some(kind)
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
pub const NEW_LAYERS: [&str; 12] = [
    "Generate", "Stamp", "Noise", "Sculpt", "Spline", "Trench", "Dig", "Terrace", "Smooth", "Erosion", "Rivers",
    "Import",
];

/// A new layer of the kind `NEW_LAYERS[choice]` names, to start from on a
/// terrain as `file` says, numbering a sculpt layer's raster past `rasters`'.
pub fn new_layer(choice: usize, file: &TerrainFile, rasters: &Rasters) -> Layer {
    let (size, middle) = (file.size, file.size * 0.5);
    let kind = match NEW_LAYERS.get(choice).copied().unwrap_or("Stamp") {
        // Its generator is picked as it is added (see `add_layer`).
        "Generate" => LayerKind::Generate { world: None, position: [0.0, 0.0] },
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
                dig: None,
            }
        }
        // A trench across the middle, its floor a metre wide.
        "Trench" => LayerKind::Spline {
            points: vec![[size * 0.4, file.base, middle], [size * 0.6, file.base, middle]],
            width: 1.0,
            falloff: 1.0,
            dig: Some(voxear_terrain::dig::Trench::default()),
        },
        "Dig" => LayerKind::Dig { strokes: Vec::new() },
        "Terrace" => LayerKind::Terrace { step: 4.0, sharpness: 0.5 },
        "Smooth" => LayerKind::Smooth { radius: 2.0 },
        // Cut as it is added (see `erode`).
        "Rivers" => LayerKind::Rivers {
            raster: rasters.next_raster(file.rasters_in_use()),
            settings: voxear_terrain::rivers::RiverSettings::default(),
            made_from: 0,
        },
        // Worn as it is added (see `erode`).
        "Erosion" => LayerKind::Erosion {
            raster: rasters.next_raster(file.rasters_in_use()),
            weather: voxear_terrain::erosion::Weather::default(),
            made_from: 0,
        },
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
    let mut layer = Layer::new(kind);
    if NEW_LAYERS.get(choice) == Some(&"Trench") {
        layer.name = "Trench".into();
    }
    layer
}

/// Whether a layer of the kind `NEW_LAYERS[choice]` names makes ground on a
/// terrain without end.
pub fn makes_endless(choice: usize) -> bool {
    new_layer(choice, &TerrainFile::default(), &Rasters::default()).kind.endless()
}

/// Adds a layer of the kind `NEW_LAYERS[choice]` names on top, and picks it.
pub fn add_layer(game: &mut Game, entity: Entity, choice: usize) -> bool {
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let mut file = asset.file.clone();
    let mut layer = new_layer(choice, &file, &asset.rasters);
    if let LayerKind::Generate { world, .. } = &mut layer.kind {
        *world = world_for(game, entity);
    }
    let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
    let dirty = dirty_between(None, Some(&layer));
    file.layers.push(layer);
    let (at, rasters) = (file.layers.len() - 1, asset.rasters.clone());
    ui_mut(&mut game.engine.world).layer = at;
    remake(game, entity, file, rasters, &dirty)
}

/// A hash of the ground the layers below layer `at` make and, where it is
/// an Erosion or Rivers layer, its settings: for knowing when what it worked
/// out has changed since.
pub fn worn_from(file: &TerrainFile, at: usize) -> u64 {
    let below = TerrainFile { layers: file.layers[..at.min(file.layers.len())].to_vec(), ..file.clone() };
    let settings = match file.layers.get(at).map(|layer| &layer.kind) {
        Some(LayerKind::Erosion { weather, .. }) => format!("{weather:?}"),
        Some(LayerKind::Rivers { settings, .. }) => format!("{settings:?}"),
        _ => String::new(),
    };
    voxear_terrain::layers::stable_hash(format!("{} {settings}", below.shape_hash()).as_bytes()).max(1)
}

/// Whether the picked layer is worked out over the ground below it, an
/// Erosion layer (false) or a Rivers layer (true), and whether that ground or
/// its settings changed since it last was: none for another kind.
pub fn worked_stale(world: &World, entity: Entity) -> Option<(bool, bool)> {
    let at = ui(world).layer;
    // Worked out over the whole terrain at once, which a terrain without end never is.
    let file = &world.get::<TerrainAsset>(entity)?.file;
    if file.endless.is_some() {
        return None;
    }
    let (rivers, made_from) = match file.layers.get(at)?.kind {
        LayerKind::Erosion { made_from, .. } => (false, made_from),
        LayerKind::Rivers { made_from, .. } => (true, made_from),
        _ => return None,
    };
    Some((rivers, made_from != worn_from(file, at)))
}

/// The ground the layers below layer `at` make, as erosion and rivers work
/// on it, and the sea's height over it: the topmost generator's below it
/// whose sea reaches the ground, if any.
fn ground_under(
    game: &mut Game,
    file: &TerrainFile,
    rasters: &Rasters,
    at: usize,
) -> (voxear_terrain::Heightmap, Option<f32>) {
    let below = TerrainFile { layers: file.layers[..at].to_vec(), ..file.clone() };
    let ground = game.content.compose_terrain(&below, rasters, None, &Compositor::default(), &Dirty::All);
    let generator = below.layers.iter().rev().find_map(|layer| match &layer.kind {
        LayerKind::Generate { world: Some(reference), .. } if layer.enabled => game.content.world_gen(reference),
        _ => None,
    });
    let lowest = ground.tiles().iter().flat_map(|tile| tile.samples().iter().copied()).fold(f32::MAX, f32::min);
    let sea = generator.map(|world| world.sea_level).filter(|&level| level > lowest);
    (ground, sea)
}

/// Works the picked Erosion or Rivers layer out over the ground below it:
/// wears it down on the GPU, or cuts rivers into it; and keeps what that
/// changed in its raster. False when the picked layer is neither, or the
/// GPU could not do it.
pub fn erode(game: &mut Game, renderer: &Renderer, entity: Entity) -> bool {
    let world = &game.engine.world;
    let Some((at, layer)) = picked(world, entity) else { return false };
    let Some(asset) = world.get::<TerrainAsset>(entity).filter(|asset| asset.file.endless.is_none()) else {
        return false;
    };
    let (mut file, mut rasters) = (asset.file.clone(), (*asset.rasters).clone());
    let (ground, sea) = ground_under(game, &file, &rasters, at);
    let grid = voxear_terrain::erosion::grid(&ground);
    let (raster, worked) = match layer.kind {
        LayerKind::Erosion { raster, weather, .. } => match renderer.erode(&grid, &weather) {
            Ok(eroded) => (raster, eroded),
            Err(error) => {
                tracing::error!("eroding the terrain: {error:#}");
                return false;
            }
        },
        LayerKind::Rivers { raster, settings, .. } => {
            (raster, voxear_terrain::rivers::carve(&grid, &settings, sea).heights)
        }
        _ => return false,
    };
    rasters.remove(raster);
    voxear_terrain::erosion::keep(&mut rasters, raster, &ground, &grid, &worked);
    let worn = worn_from(&file, at);
    if let LayerKind::Erosion { made_from, .. } | LayerKind::Rivers { made_from, .. } = &mut file.layers[at].kind {
        *made_from = worn;
    }
    remake(game, entity, file, Arc::new(rasters), &Dirty::All)
}

/// The picked Rivers layer's rivers and lakes as a scene's Objs, to add: an
/// Obj at the terrain's corner holding a Stream along each river and a Pool
/// over each part of each lake. None when the picked layer is no Rivers
/// layer, or it has neither.
pub fn river_water(game: &mut Game, entity: Entity) -> Option<String> {
    let world = &game.engine.world;
    let (at, layer) = picked(world, entity)?;
    let LayerKind::Rivers { settings, .. } = layer.kind else { return None };
    let corner = voxear_terrain::corner(world.get(entity)?, world.get(entity));
    let asset = world.get::<TerrainAsset>(entity)?;
    let (file, rasters) = (asset.file.clone(), asset.rasters.clone());
    let (ground, sea) = ground_under(game, &file, &rasters, at);
    let grid = voxear_terrain::erosion::grid(&ground);
    let carved = voxear_terrain::rivers::carve(&grid, &settings, sea);
    if carved.rivers.is_empty() && carved.lakes.is_empty() {
        return None;
    }
    let number = |v: f32| format!("{v:?}");
    let triple = |[x, y, z]: [f32; 3]| format!("[{}, {}, {}]", number(x), number(y), number(z));
    let mut text = String::from("format = 1\n\n[[obj]]\nid = 1\nname = \"Rivers and Lakes\"\n");
    text += &format!("position = {}\n", triple(corner.to_array()));
    let mut id = 1;
    for (n, river) in carved.rivers.iter().enumerate() {
        id += 1;
        let start = river.points[0].at;
        let points: Vec<String> = river
            .points
            .iter()
            .map(|point| {
                let p = point.at;
                let local = [p[0] - start[0], p[1] - start[1], p[2] - start[2]];
                format!("{{ position = {}, width = {} }}", triple(local), number(point.water_width()))
            })
            .collect();
        let depth = river.points.last().map_or(1.0, |point| point.depth);
        let place = triple(start);
        text += &format!("\n[[obj]]\nid = {id}\nname = \"River {}\"\nparent = 1\nposition = {place}\n", n + 1);
        text += &format!("stream = {{ depth = {} }}\npath = {{ points = [{}] }}\n", number(depth), points.join(", "));
    }
    for (n, lake) in carved.lakes.iter().enumerate() {
        for (part, (bounds, depth)) in lake.boxes.iter().enumerate() {
            id += 1;
            let spacing = grid.spacing;
            let [x0, z0, x1, z1] = *bounds;
            let middle = [(x0 + x1) * 0.5, lake.level, (z0 + z1) * 0.5];
            let name = match lake.boxes.len() {
                1 => format!("Lake {}", n + 1),
                _ => format!("Lake {} ({})", n + 1, part + 1),
            };
            text += &format!("\n[[obj]]\nid = {id}\nname = \"{name}\"\nparent = 1\nposition = {}\n", triple(middle));
            let size = [number(x1 - x0 + spacing), number(z1 - z0 + spacing)];
            text += &format!("pool = {{ size = [{}, {}], depth = {} }}\n", size[0], size[1], number(*depth));
        }
    }
    Some(text)
}

/// The world generator a new Generate layer starts with: the project's
/// first, by its path, or a new one made beside the terrain's file and
/// named for it.
fn world_for(game: &mut Game, entity: Entity) -> Option<String> {
    let db = game.content.db();
    let mut found: Vec<String> =
        db.entries().filter(|entry| entry.kind == AssetKind::WorldGen).map(|entry| entry.path.clone()).collect();
    found.sort();
    let path = match found.into_iter().next() {
        Some(path) => path,
        None => {
            let guid = game.engine.world.get::<TerrainAsset>(entity)?.guid;
            let terrain = db.get(guid)?.path.clone();
            let (folder, name) = terrain.rsplit_once('/').unwrap_or(("", &terrain));
            let stem = name.strip_suffix(".terrain").unwrap_or(name);
            let made = crate::project_files::create_world_gen(&game.project.root, folder, stem)
                .inspect_err(|error| tracing::error!("{error:#}"))
                .ok()?;
            game.content.rescan().inspect_err(|error| tracing::error!("{error:#}")).ok()?;
            made
        }
    };
    game.content.parse(&path).ok().map(|reference| reference.to_string())
}

/// Makes every terrain whose Generate layers read the world generator
/// `guid` again from its layers, as after the generator changed.
pub fn regenerate(game: &mut Game, guid: voxear_assets::AssetGuid) {
    let world = &game.engine.world;
    let using: Vec<(Entity, TerrainFile, Arc<Rasters>)> = world
        .query_ref::<(Entity, &TerrainAsset)>()
        .into_iter()
        .filter(|(_, asset)| {
            asset.file.layers.iter().any(|layer| match &layer.kind {
                LayerKind::Generate { world: Some(reference), .. } => {
                    game.content.parse(reference).ok().and_then(|parsed| parsed.guid()) == Some(guid)
                }
                _ => false,
            })
        })
        .map(|(entity, asset)| (entity, asset.file.clone(), asset.rasters.clone()))
        .collect();
    for (entity, file, rasters) in using {
        remake(game, entity, file, rasters, &Dirty::All);
    }
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

/// The Dig layer the Dig and Fill brushes add to: the picked one, or the
/// topmost, or a new one on top.
pub fn dig_layer(game: &mut Game, entity: Entity) -> Option<usize> {
    let world = &game.engine.world;
    let file = &world.get::<TerrainAsset>(entity)?.file;
    let digs = |at: usize| matches!(file.layers.get(at)?.kind, LayerKind::Dig { .. }).then_some(at);
    let topmost = (0..file.layers.len()).rev().find_map(digs);
    if let Some(found) = digs(ui(world).layer).or(topmost) {
        ui_mut(&mut game.engine.world).layer = found;
        return Some(found);
    }
    let dig = NEW_LAYERS.iter().position(|&kind| kind == "Dig").expect("Dig is offered");
    add_layer(game, entity, dig);
    let file = &game.engine.world.get::<TerrainAsset>(entity)?.file;
    Some(file.layers.len() - 1)
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
    if let LayerKind::Sculpt { raster } | LayerKind::Erosion { raster, .. } | LayerKind::Rivers { raster, .. } =
        layer.kind
    {
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
        let kind = |name: &str| NEW_LAYERS.iter().position(|&kind| kind == name).expect("offered");
        let stamp = new_layer(kind("Stamp"), &TerrainFile::default(), &Rasters::default());
        let fields = layer_fields(&stamp);
        assert!(fields.contains(&F::Position) && fields.contains(&F::Picture) && !fields.contains(&F::Heights));
        let mut masked = stamp.clone();
        masked.mask.height = Some([0.0, 10.0]);
        assert!(layer_fields(&masked).contains(&F::Heights) && layer_fields(&masked).contains(&F::Feather));
        let sculpt = new_layer(kind("Sculpt"), &TerrainFile::default(), &Rasters::default());
        assert!(matches!(sculpt.kind, LayerKind::Sculpt { raster: 1 }));
        assert!(!layer_fields(&sculpt).contains(&F::Blend), "a sculpt layer adds");
        // A stamp starts in the middle, an eighth of the terrain across.
        assert!(matches!(stamp.kind, LayerKind::Stamp { position: [256.0, 256.0], size: [64.0, 64.0], .. }));
    }
}
