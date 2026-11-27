//! The top of a Terrain's Inspector section: its tools, **Layers**,
//! **Sculpt**, **Paint** and **Settings**, as Unity's Terrain has them.
//! Under Layers, the list of its layers, the top one first, as image
//! editors list theirs. A click picks a layer, whose settings show below;
//! its box switches it on and off; a drag by its grip moves it; its cross
//! takes it out; Add Layer offers a new one of each kind. Under Paint, the
//! list of its surfaces the same way, the top one first and the ground's
//! own at the bottom: a click picks the one the brush paints, and Add
//! Surface opens the picker of materials for a new one. Under Sculpt, Paint
//! and Grass, a picture of the brush: as wide as it is against the largest,
//! as strong as it is, fading out to its edge, with what it is beside it.
//! Each change to
//! either list is a step to undo; picking a tool, a layer or a surface is
//! not.

use std::hash::{Hash, Hasher};

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, terrain::TerrainAsset};
use voxear_terrain::{layers::LayerKind, surfaces::Surface};
use voxear_ui::{Color, NodeId, Shape, Style, Ui};

use crate::{
    collection::{self, Collection},
    terrain_props::{self, TerrainTool},
};

/// One row of a list: the place in the stack of the layer or surface it
/// shows, the part a click picks it by, its box, and its cross.
struct Row {
    place: usize,
    body: NodeId,
    toggle: Option<NodeId>,
    remove: Option<NodeId>,
}

/// What the panel's list shows.
#[derive(Clone, Copy, PartialEq)]
enum Listing {
    Layers,
    Surfaces,
    Grass,
}

/// The tools and the list of layers or surfaces of the Terrain the
/// Inspector shows.
pub struct TerrainPanel {
    tools: NodeId,
    list: Option<(Listing, Collection)>,
    rows: Vec<Row>,
    /// The picked Erosion or Rivers layer's button to work it out again, and
    /// a Rivers layer's to make water for its rivers and lakes.
    erode: Option<NodeId>,
    water: Option<NodeId>,
    /// What the panel was built from, to build it again when that changes.
    signature: u64,
    /// The brush's picture, its words, and the brush and size it last drew.
    brush: Option<(NodeId, NodeId, Option<(voxear_terrain::brush::Brush, [f32; 4])>)>,
}

/// The largest brush, which fills the picture's height.
const LARGEST_BRUSH: f32 = 100.0;

/// The icon a layer of its kind shows.
fn icon(kind: &LayerKind) -> &'static str {
    match kind {
        LayerKind::Import { .. } => "texture",
        LayerKind::Stamp { .. } => "mountain",
        LayerKind::Generate { .. } => "sprout",
        LayerKind::Erosion { .. } => "drop",
        LayerKind::Rivers { .. } => "branch",
        LayerKind::Noise { .. } => "grid",
        LayerKind::Sculpt { .. } => "edit",
        LayerKind::Spline { dig: Some(_), .. } => "spade",
        LayerKind::Spline { .. } => "branch",
        LayerKind::Terrace { .. } => "layers",
        LayerKind::Smooth { .. } => "globe",
        LayerKind::Dig { .. } => "spade",
    }
}

/// What the panel shows of an Obj's terrain: the tool, the picked layer and
/// surface, each layer's name, kind and switch, and each surface's material
/// and cover.
fn signature(world: &World, entity: Entity) -> u64 {
    let ui = terrain_props::ui(world);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (ui.tool, ui.layer, ui.surface, ui.grass).hash(&mut hasher);
    terrain_props::worked_stale(world, entity).hash(&mut hasher);
    if let Some(asset) = world.get::<TerrainAsset>(entity) {
        for layer in &asset.file.layers {
            (&layer.name, layer.kind.title(), layer.enabled).hash(&mut hasher);
        }
        for surface in &asset.file.surfaces {
            (&surface.material, surface.cover).hash(&mut hasher);
        }
        for grass in &asset.file.grass {
            (&grass.mesh, grass.cover).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// What a surface's row is called: its material's file name, without its
/// folders or extension.
fn surface_name(game: &Game, surface: &Surface) -> String {
    let path = surface.material.as_deref().and_then(|reference| {
        let guid = game.content.parse(reference).ok()?.guid()?;
        Some(game.content.db().get(guid)?.path.clone())
    });
    match path {
        Some(path) => std::path::Path::new(&path).file_stem().map_or(path.clone(), |s| s.to_string_lossy().into()),
        None => "No material".into(),
    }
}

impl TerrainPanel {
    pub fn new(ui: &mut Ui, rows: NodeId, game: &Game, entity: Entity) -> Self {
        let world = &game.engine.world;
        let state = terrain_props::ui(world);
        let tools = ui.segmented(rows);
        ui.add_class(tools, "terrain-tools");
        for tool in TerrainTool::ALL {
            let segment = ui.add_segment(tools, tool.name(), Some(tool.icon()));
            ui.set_tooltip(segment, tool.name(), None);
        }
        let at = TerrainTool::ALL.iter().position(|&tool| tool == state.tool).unwrap_or_default();
        ui.set_segment(tools, at);
        let file = world.get::<TerrainAsset>(entity).map(|asset| &asset.file);
        let mut shown = Vec::new();
        let list = match (state.tool, file) {
            (TerrainTool::Layers, Some(file)) => {
                let layers = &file.layers;
                let list = collection::collection(ui, rows, layers.len(), true, Some("Add Layer"));
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No layers: flat ground at its base height");
                // The top layer first.
                for (place, layer_at) in (0..layers.len()).rev().enumerate() {
                    let layer = &layers[layer_at];
                    let item = collection::item(ui, list.items, place, true);
                    ui.set_class(item.node, "selected", layer_at == state.layer);
                    ui.set_class(item.node, "off", !layer.enabled);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let mark = ui.icon(line, icon(&layer.kind));
                    ui.add_class(mark, "terrain-layer-icon");
                    let name = ui.add_label(line, &layer.name);
                    ui.add_class(name, "terrain-layer-name");
                    let kind = ui.add_label(line, layer.kind.title());
                    ui.add_class(kind, "terrain-layer-kind");
                    let toggle = ui.toggle(line, "", layer.enabled);
                    ui.add_class(toggle, "terrain-layer-on");
                    ui.set_tooltip(toggle, "On", None);
                    shown.push(Row { place: layer_at, body: line, toggle: Some(toggle), remove: item.remove });
                }
                Some((Listing::Layers, list))
            }
            (TerrainTool::Paint, Some(file)) => {
                let surfaces = &file.surfaces;
                let list = collection::collection(ui, rows, surfaces.len(), true, Some("Add Surface"));
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No surfaces: drawn with its material");
                for (place, surface_at) in (0..surfaces.len()).rev().enumerate() {
                    let surface = &surfaces[surface_at];
                    let item = collection::item(ui, list.items, place, true);
                    ui.set_class(item.node, "selected", surface_at == state.surface);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let mark = ui.icon(line, "texture");
                    ui.add_class(mark, "terrain-layer-icon");
                    let name = ui.add_label(line, &surface_name(game, surface));
                    ui.add_class(name, "terrain-layer-name");
                    // The first is the ground under the rest, wherever they leave it.
                    let cover = if surface_at == 0 { "Ground" } else { surface.cover.name() };
                    let kind = ui.add_label(line, cover);
                    ui.add_class(kind, "terrain-layer-kind");
                    shown.push(Row { place: surface_at, body: line, toggle: None, remove: item.remove });
                }
                Some((Listing::Surfaces, list))
            }
            (TerrainTool::Grass, Some(file)) => {
                let grass = &file.grass;
                let list = collection::collection(ui, rows, grass.len(), true, Some("Add Grass"));
                ui.add_class(list.frame, "terrain-layers");
                ui.set_text(list.empty, "No grass");
                for (place, kind_at) in (0..grass.len()).rev().enumerate() {
                    let kind = &grass[kind_at];
                    let item = collection::item(ui, list.items, place, true);
                    ui.set_class(item.node, "selected", kind_at == state.grass);
                    let line = ui.panel(item.body, Style::row());
                    ui.add_class(line, "terrain-layer");
                    let mark = ui.icon(line, "leaf");
                    ui.add_class(mark, "terrain-layer-icon");
                    let label = kind.mesh.as_deref().map_or("No mesh".into(), |mesh| crate::props::mesh_label(game, mesh));
                    let name = ui.add_label(line, &label);
                    ui.add_class(name, "terrain-layer-name");
                    let cover = ui.add_label(line, kind.cover.name());
                    ui.add_class(cover, "terrain-layer-kind");
                    shown.push(Row { place: kind_at, body: line, toggle: None, remove: item.remove });
                }
                Some((Listing::Grass, list))
            }
            _ => None,
        };
        // An Erosion or Rivers layer picked: a button to work it out, again once the ground below or its settings
        // changed; and for rivers, one to make their water.
        let (mut erode, mut water) = (None, None);
        if let (TerrainTool::Layers, Some((rivers, stale))) = (state.tool, terrain_props::worked_stale(world, entity)) {
            let label = match (rivers, stale) {
                (false, false) => "Erode",
                (false, true) => "Erode Again",
                (true, false) => "Carve",
                (true, true) => "Carve Again",
            };
            let button = ui.button(rows, label);
            ui.add_class(button, "terrain-erode");
            erode = Some(button);
            if rivers {
                let button = ui.button(rows, "Add Water");
                ui.add_class(button, "terrain-water");
                water = Some(button);
            }
        }
        // The brush, over its settings, for the tools that paint with it.
        let brush = state.tool.brushes().then(|| {
            let card = ui.panel(rows, Style::default());
            ui.add_class(card, "brush-card");
            let picture = ui.panel(card, Style::default());
            ui.add_class(picture, "brush-card-picture");
            let words = ui.add_label(card, "");
            ui.add_class(words, "brush-card-words");
            (picture, words, None)
        });
        Self { tools, list, rows: shown, erode, water, signature: signature(world, entity), brush }
    }

    /// Whether what it shows changed since it was built.
    pub fn stale(&self, world: &World, entity: Entity) -> bool {
        signature(world, entity) != self.signature
    }

    /// Does what was clicked or dragged: a tool, a layer or a surface
    /// picked, a layer switched, or either moved or taken out. Whether
    /// anything changed.
    pub fn update(&self, ui: &Ui, game: &mut Game, entity: Entity) -> bool {
        let world = &mut game.engine.world;
        let tool = TerrainTool::ALL[ui.segment(self.tools).min(TerrainTool::ALL.len() - 1)];
        if tool != terrain_props::ui(world).tool {
            terrain_props::show_tool(world, tool);
            return true;
        }
        let Some((listing, list)) = &self.list else { return false };
        if let Some((from, to)) = collection::moved(ui, list.items) {
            // Places count from the top; the stacks from the bottom.
            let last = self.rows.len().saturating_sub(1);
            let (from, to) = (last - from.min(last), last - to.min(last));
            return match listing {
                Listing::Layers => terrain_props::move_layer(game, entity, from, to),
                Listing::Surfaces => terrain_props::move_surface(game, entity, from, to),
                Listing::Grass => terrain_props::move_grass(game, entity, from, to),
            };
        }
        for row in &self.rows {
            if row.toggle.is_some_and(|toggle| ui.clicked(toggle)) {
                return terrain_props::toggle_layer(game, entity, row.place);
            }
            if row.remove.is_some_and(|remove| ui.clicked(remove)) {
                return match listing {
                    Listing::Layers => terrain_props::remove_layer(game, entity, row.place),
                    Listing::Surfaces => terrain_props::remove_surface(game, entity, row.place),
                    Listing::Grass => terrain_props::remove_grass(game, entity, row.place),
                };
            }
            if !ui.clicked(row.body) {
                continue;
            }
            let state = terrain_props::ui(&game.engine.world);
            let world = &mut game.engine.world;
            match listing {
                Listing::Layers if state.layer != row.place => terrain_props::pick_layer(world, row.place),
                Listing::Surfaces if state.surface != row.place => terrain_props::pick_surface(world, row.place),
                Listing::Grass if state.grass != row.place => terrain_props::pick_grass(world, row.place),
                _ => return false,
            }
            return true;
        }
        false
    }

    /// Draws the brush as it is now, when it changed.
    pub fn show_brush(&mut self, ui: &mut Ui, world: &World) {
        let Some((picture, words, drawn)) = &mut self.brush else { return };
        let brush = terrain_props::ui(world).brush;
        let rect = ui.rect(*picture);
        if *drawn == Some((brush, rect)) || rect[2] <= 0.0 {
            return;
        }
        *drawn = Some((brush, rect));
        ui.set_shapes(*picture, brush_shapes(&brush, rect[2], rect[3]));
        let tool = terrain_props::ui(world).tool;
        let name = if tool == TerrainTool::Sculpt { brush.kind.name() } else { tool.name() };
        let strength = if brush.kind == voxear_terrain::brush::BrushKind::Holes && tool == TerrainTool::Sculpt {
            String::new()
        } else {
            format!(" · {:.0}%", brush.strength * 100.0)
        };
        ui.set_text(*words, &format!("{name} · {} m{strength}", trim(brush.radius)));
    }

    /// Whether the Erode or Carve button was clicked.
    pub fn erode_clicked(&self, ui: &Ui) -> bool {
        self.erode.is_some_and(|button| ui.clicked(button))
    }

    /// Whether the Add Water button was clicked.
    pub fn water_clicked(&self, ui: &Ui) -> bool {
        self.water.is_some_and(|button| ui.clicked(button))
    }

    /// The Add Layer button, when it was clicked, for its menu to open under.
    pub fn add_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.add_of(ui, Listing::Layers)
    }

    /// The Add Surface button, when it was clicked, for the picker of
    /// materials to open under.
    pub fn add_surface_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.add_of(ui, Listing::Surfaces)
    }

    /// The Add Grass button, when it was clicked, for the picker of meshes
    /// to open under.
    pub fn add_grass_clicked(&self, ui: &Ui) -> Option<NodeId> {
        self.add_of(ui, Listing::Grass)
    }

    fn add_of(&self, ui: &Ui, wanted: Listing) -> Option<NodeId> {
        let (listing, list) = self.list.as_ref()?;
        list.add.filter(|&add| *listing == wanted && ui.clicked(add))
    }
}

/// A size without trailing zeros: 8, 2.5.
fn trim(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A brush seen from above, on the left, its circle as wide against the
/// picture's height as it is against the largest brush, filled as strongly
/// as it paints and fading to its edge; and across it on the right, how
/// much it does from its middle out to its edge.
fn brush_shapes(brush: &voxear_terrain::brush::Brush, width: f32, height: f32) -> Vec<Shape> {
    let accent = |alpha: f32| Color::rgba(121, 136, 255, (alpha.clamp(0.0, 1.0) * 255.0) as u8);
    let line = Color::rgba(255, 255, 255, 46);
    let mut shapes = vec![Shape::Rect { rect: [0.0, 0.0, width, height], radius: 0.0, fill: Color::rgba(17, 18, 21, 255) }];
    let centre = [height / 2.0 + 6.0, height / 2.0];
    let most = height / 2.0 - 6.0;
    let radius = (most * (brush.radius / LARGEST_BRUSH).sqrt()).clamp(3.0, most);
    let strength = brush.strength.clamp(0.05, 1.0);
    const RINGS: usize = 16;
    for ring in 0..RINGS {
        let t = 1.0 - ring as f32 / RINGS as f32;
        let falloff = 1.0 - (1.0 - t * t).powi(2);
        shapes.push(Shape::Circle { center: centre, radius: radius * t, fill: accent(strength * (1.0 - falloff) * 0.18), border: None });
    }
    shapes.push(Shape::Circle { center: centre, radius, fill: Color::rgba(0, 0, 0, 0), border: Some((1.0, accent(0.9))) });
    // The cross-section: full in the middle, nothing at the edge, as high as it is strong.
    let (left, right) = (height + 12.0, width - 10.0);
    let (base, top) = (height - 10.0, 10.0);
    shapes.push(Shape::Line { from: [left, base], to: [right, base], width: 1.0, color: line });
    let mut last = None;
    for step in 0..=40 {
        let t = step as f32 / 40.0;
        let from_middle = (t * 2.0 - 1.0).abs();
        let amount = (1.0 - from_middle * from_middle).powi(2) * strength;
        let point = [left + (right - left) * t, base - (base - top) * amount];
        if let Some(from) = last {
            shapes.push(Shape::Line { from, to: point, width: 1.5, color: accent(1.0) });
        }
        last = Some(point);
    }
    shapes
}
