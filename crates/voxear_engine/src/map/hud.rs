//! What shows with a map besides the ground: the minimap and the map a Map
//! component puts on the screen, opened and closed by its action, and over
//! every map view its markers, their labels, the arrow of what it follows,
//! and a map's title, readout and compass.

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldOrigin, WorldTransform};
use voxear_scripting::CursorLock;
use voxear_terrain::{blocks::map::MapGround, surfaces::Biome};
use voxear_ui::{Color, ElementKind, MapViewKind, MapViewSettings, NodeId, Shape, Ui};

use super::{
    MapScript, MapViewer, Maps, ShownMap,
    parts::{Map, MapCorner, MapMarker, MarkerShows},
};
use crate::{game::Display, game_ui::GameUi};

/// A Map's minimap and map on the screen: their document, the settings
/// they were last given, and what opening the map changed, to put back.
pub(super) struct Hud {
    document: u32,
    mini: NodeId,
    large: NodeId,
    given: Option<Map>,
    held: Option<Held>,
}

/// The mouse and the input maps as they were before a map opened.
struct Held {
    cursor_visible: bool,
    cursor_lock: CursorLock,
    switched_off: Vec<String>,
}

/// A marker at a place, put there by a script: where it is in the scene,
/// and how it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedMarker {
    pub at: Vec3,
    pub marker: MapMarker,
}

/// How far a minimap's edge markers stay inside it, in pixels.
const EDGE: f32 = 4.0;

impl Maps {
    /// Keeps a minimap and a map on the screen for each Map in `world`,
    /// set as it says: opens and closes its map when its action is
    /// pressed, frees the mouse while it is open and leaves on only the
    /// input map its action is in, and puts both back as it closes.
    pub fn sync_huds(
        &mut self,
        world: &mut World,
        ui: &mut GameUi,
        input: &mut voxear_input::Input,
        display: &mut Display,
    ) {
        let found: Vec<(Entity, Map)> = world
            .query_ref::<(Entity, &Map)>()
            .without::<Destroying>()
            .into_iter()
            .filter(|(entity, _)| voxear_scene::is_active(world, *entity))
            .map(|(entity, map)| (entity, map.clone()))
            .collect();
        let gone: Vec<Entity> =
            self.huds.keys().filter(|kept| !found.iter().any(|(entity, _)| entity == *kept)).copied().collect();
        for entity in gone {
            let hud = self.huds.remove(&entity).expect("listed");
            release(hud.held, input, display);
            ui.close(hud.document);
        }
        for (entity, mut map) in found {
            if !self.huds.contains_key(&entity) {
                let Some(hud) = make(ui) else { continue };
                self.huds.insert(entity, hud);
            }
            let hud = self.huds.get_mut(&entity).expect("made above");
            let action = map.open_action.as_deref().and_then(|id| input.action(id));
            if action.is_some_and(|action| input.state(action).pressed) {
                map.open = !map.open;
                if let Some(kept) = world.get_mut::<Map>(entity) {
                    kept.open = map.open;
                }
            }
            if hud.given.as_ref() != Some(&map) {
                give(ui.ui(), hud, &map);
                hud.given = Some(map.clone());
            }
            match (map.open, hud.held.is_some()) {
                (true, false) => {
                    // Only the map its action is in stays on, so the game hears nothing meant for the map.
                    let own = action.and_then(|action| input.action_name(action).map(|(map, _)| map.to_string()));
                    let on = input.maps().into_iter().filter(|name| input.map_enabled(name));
                    let switched_off: Vec<String> =
                        on.filter(|name| own.as_ref().is_some_and(|own| own != name)).collect();
                    switched_off.iter().for_each(|name| input.set_map_enabled(name, false));
                    let held =
                        Held { cursor_visible: display.cursor_visible, cursor_lock: display.cursor_lock, switched_off };
                    (display.cursor_visible, display.cursor_lock, display.cursor_changed) =
                        (true, CursorLock::None, true);
                    hud.held = Some(held);
                }
                (false, true) => release(hud.held.take(), input, display),
                _ => {}
            }
        }
    }

    /// Draws what lies over one map view's ground: its markers, the arrow
    /// of what it follows, and for a map its labels, title, readout and
    /// compass.
    pub(super) fn overlay(
        &mut self,
        world: &World,
        ui: &mut Ui,
        (node, settings): (NodeId, &MapViewSettings),
        viewer: MapViewer,
    ) {
        let Some(parts) = ui.map_view_parts(node) else { return };
        let Some(shown) = self.shown.get(&node).copied() else {
            ui.set_shapes(parts.overlay, Vec::new());
            self.label(ui, node, parts.overlay, Vec::new());
            return;
        };
        let large = settings.kind == MapViewKind::Map;
        let centre = shown.size * 0.5;
        let mut shapes = Vec::new();
        let mut labels: Vec<(Vec2, String)> = Vec::new();
        for (position, marker) in self.markers(world) {
            let on_this = match marker.shows {
                MarkerShows::Both => true,
                MarkerShows::Minimap => !large,
                MarkerShows::Map => large,
            };
            let Some((mut at, inside)) = shown.point(position).filter(|_| on_this) else { continue };
            let mut radius = if large { 11.0 } else { 8.0 };
            let in_view = at.x >= 0.0 && at.y >= 0.0 && at.x <= shown.size.x && at.y <= shown.size.y;
            if !inside || !in_view {
                if !marker.stays_at_edge {
                    continue;
                }
                // On the slab's edge, towards where it is; kept inside the view where the slab runs past it.
                radius *= 0.8;
                let Some(edge) = shown.edge_towards(position) else { continue };
                let (from, room) = (edge - centre, centre - Vec2::splat(radius + EDGE));
                let fits = (room.x / from.x.abs().max(1e-3)).min(room.y / from.y.abs().max(1e-3)).min(1.0);
                at = centre + from * fits;
            }
            let [r, g, b] = marker.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let border = Some((1.5, Color::rgba(0, 0, 0, 190)));
            shapes.push(Shape::Circle { center: at.to_array(), radius, fill: Color::rgb(r, g, b), border });
            shapes.push(Shape::Icon {
                name: marker.icon.icon(),
                center: at.to_array(),
                size: radius * 1.35,
                color: Color::WHITE,
            });
            if large && inside && in_view && !marker.label.is_empty() {
                labels.push((at + Vec2::new(radius + 5.0, -10.0), marker.label.clone()));
            }
        }
        // What the map follows: an arrow the way it faces, as the map is turned.
        if let Some((at, _)) = shown.point(viewer.position) {
            let (sin, cos) = (viewer.heading - shown.camera.turn).sin_cos();
            let ahead = Vec2::new(sin, -cos);
            let side = Vec2::new(-ahead.y, ahead.x);
            let (tip, left, right) = (at + ahead * 9.0, at - ahead * 6.0 - side * 6.0, at - ahead * 6.0 + side * 6.0);
            let notch = at - ahead * 2.5;
            for (width, color) in [(5.0, Color::rgba(0, 0, 0, 200)), (2.5, Color::WHITE)] {
                for (from, to) in [(tip, left), (left, notch), (notch, right), (right, tip)] {
                    shapes.push(Shape::Line { from: from.to_array(), to: to.to_array(), width, color });
                }
            }
        }
        ui.set_shapes(parts.overlay, shapes);
        self.label(ui, node, parts.overlay, labels);
        if let (Some(title), Some(readout), Some(compass)) = (parts.title, parts.readout, parts.compass) {
            let (place, readout_text) = self.readout(world, &shown);
            let named = world.resource::<MapScript>().and_then(|script| script.title.clone());
            let title_text = named.unwrap_or(place);
            self.write(ui, title, title_text);
            self.write(ui, readout, readout_text);
            draw_compass(ui, compass, &shown);
        }
    }

    /// Every marker to show: each Map Marker's, where its Obj is, and those
    /// scripts placed.
    fn markers(&self, world: &World) -> Vec<(Vec3, MapMarker)> {
        let objs =
            world.query_ref::<(Entity, &MapMarker, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
        let objs = objs.into_iter().filter(|(entity, ..)| voxear_scene::is_active(world, *entity));
        let objs = objs.map(|(_, marker, local, placed)| (voxear_terrain::corner(local, placed), marker.clone()));
        let placed = world.resource::<MapScript>().into_iter().flat_map(|script| script.placed.values());
        objs.chain(placed.map(|placed| (placed.at, placed.marker.clone()))).collect()
    }

    /// Keeps a view's labels as `labels` says: each where it is, with its
    /// text, as many elements as there are labels.
    fn label(&mut self, ui: &mut Ui, view: NodeId, overlay: NodeId, labels: Vec<(Vec2, String)>) {
        let kept = self.labels.entry(view).or_default();
        while kept.len() > labels.len() {
            let (node, _) = kept.pop().expect("longer than none");
            ui.remove(node);
        }
        for (at, (place, text)) in labels.into_iter().enumerate() {
            if at == kept.len() {
                let node = ui.add_label(overlay, &text);
                ui.add_class(node, "map-label");
                kept.push((node, text.clone()));
            }
            let (node, shown) = &mut kept[at];
            if *shown != text {
                ui.set_text(*node, &text);
                *shown = text;
            }
            ui.set_inline_style(*node, &format!("left: {:.0}px; top: {:.0}px", place.x, place.y));
        }
    }

    /// Sets a label's text, when it is not that already.
    fn write(&mut self, ui: &mut Ui, node: NodeId, text: String) {
        if self.written.get(&node) != Some(&text) {
            ui.set_text(node, &text);
            self.written.insert(node, text);
        }
    }

    /// The name of the country a map looks at, by how warm and wet it is
    /// there, and its readout: where that is, how warm and how wet, and the
    /// map's zoom.
    fn readout(&self, world: &World, shown: &ShownMap) -> (String, String) {
        let cells = Vec3::new(
            shown.corner[0] as f32 + shown.camera.middle.x,
            0.0,
            shown.corner[1] as f32 + shown.camera.middle.z,
        );
        let scene = shown.origin + cells * shown.cell;
        let level = WorldOrigin::of(world).to_level(scene);
        let blocks = self.grounds.get(&shown.world).map_or(1.0, |ground| shown.cell / ground.block_size.max(0.01));
        let zoom = format!("1:{}", blocks.round().max(1.0) as u32);
        let place = format!("X {:.0}   Z {:.0}", level.x, level.z);
        let making = match self.grounds.get(&shown.world).map(|ground| &ground.ground) {
            Some(MapGround::Made { making, .. }) => making,
            _ => return (String::new(), format!("{place}   {zoom}")),
        };
        // The generator's own place: the world's blocks are counted from where its ground starts in it.
        let [x, z] = [0, 1].map(|axis| making.generate.position[axis] + [cells.x, cells.z][axis] * shown.cell);
        let (warmth, wetness) = making.world.climate(x, z, making.world.height_at(x, z));
        let within =
            |range: Option<[f32; 2]>, value: f32| range.is_none_or(|[low, high]| value >= low && value <= high);
        let country = Biome::ALL.into_iter().find(|biome| {
            let (warm, wet) = biome.ranges();
            within(warm, warmth) && within(wet, wetness)
        });
        let name = country.map_or(String::new(), |biome| biome.name().to_string());
        (name, format!("{place}   {warmth:.0} °C   {:.0} %   {zoom}", wetness * 100.0))
    }

    /// Opens or closes `entity`'s map, as its action does.
    pub fn set_open(world: &mut World, entity: Entity, open: bool) {
        if let Some(map) = world.get_mut::<Map>(entity) {
            map.open = open;
        }
    }
}

/// Makes a Map's document: its minimap, and its map, shut.
fn make(ui: &mut GameUi) -> Option<Hud> {
    let document = ui.new_document();
    // Elements are made in a document's own container, found by no name.
    let root = ui.find(document, "");
    let mut create = |kind| ui.create(root?, kind, "").and_then(|element| ui.node(element));
    let (mini, large) = (create(ElementKind::Minimap), create(ElementKind::Map));
    let (Some(mini), Some(large)) = (mini, large) else {
        ui.close(document);
        return None;
    };
    ui.ui().set_visible(large, false);
    Some(Hud { document, mini, large, given: None, held: None })
}

/// Sets a Map's minimap and map as it says.
fn give(ui: &mut Ui, hud: &Hud, map: &Map) {
    let gap = 16.0;
    let (across, down) = match map.corner {
        MapCorner::TopLeft => ("left", "top"),
        MapCorner::TopRight => ("right", "top"),
        MapCorner::BottomLeft => ("left", "bottom"),
        MapCorner::BottomRight => ("right", "bottom"),
    };
    let size = map.size;
    let place = format!("position: absolute; {across}: {gap}px; {down}: {gap}px; width: {size}px; height: {size}px");
    ui.set_inline_style(hud.mini, &place);
    let mini = MapViewSettings {
        zoom: map.zoom,
        turns: map.turns,
        round: map.round,
        tilt: map.tilt,
        ..MapViewSettings::of(MapViewKind::Minimap)
    };
    ui.set_map_view_settings(hud.mini, mini);
    let large = MapViewSettings { zoom: map.map_zoom, ..MapViewSettings::of(MapViewKind::Map) };
    ui.set_map_view_settings(hud.large, large);
    // The minimap gives way to the map while it is open.
    ui.set_visible(hud.mini, map.minimap && !map.open);
    ui.set_visible(hud.large, map.open);
}

/// Puts the mouse and the input maps back as they were before a map
/// opened.
fn release(held: Option<Held>, input: &mut voxear_input::Input, display: &mut Display) {
    let Some(held) = held else { return };
    held.switched_off.iter().for_each(|name| input.set_map_enabled(name, true));
    (display.cursor_visible, display.cursor_lock, display.cursor_changed) =
        (held.cursor_visible, held.cursor_lock, true);
}

/// Draws a map's compass: a cross lying on the ground as the map is
/// turned and tilted, its north arm red, with an N at its end.
fn draw_compass(ui: &mut Ui, compass: NodeId, shown: &ShownMap) {
    let rect = ui.rect(compass);
    let centre = Vec2::new(rect[2], rect[3]) * 0.5;
    let reach = centre.x.min(centre.y) - 10.0;
    if reach < 4.0 {
        return;
    }
    let [_, right, up] = shown.camera.axes();
    let (north, east) = (Vec2::new(right.z, -up.z) * reach, Vec2::new(right.x, -up.x) * reach);
    let line = |from: Vec2, to: Vec2, width: f32, color: Color| Shape::Line {
        from: (centre + from).to_array(),
        to: (centre + to).to_array(),
        width,
        color,
    };
    let pale = Color::rgba(255, 255, 255, 215);
    let shapes = vec![
        line(-north, north, 5.0, Color::rgba(0, 0, 0, 170)),
        line(-east, east, 5.0, Color::rgba(0, 0, 0, 170)),
        line(-east, east, 2.0, pale),
        line(-north, Vec2::ZERO, 2.0, pale),
        line(Vec2::ZERO, north, 2.5, Color::rgb(0xe5, 0x48, 0x3b)),
    ];
    ui.set_shapes(compass, shapes);
    let letter = match ui.children(compass).first() {
        Some(letter) => *letter,
        None => {
            let letter = ui.add_label(compass, "N");
            ui.add_class(letter, "map-compass-north");
            letter
        }
    };
    let at = centre + north * 1.0 + north.normalize_or_zero() * 9.0 - Vec2::new(5.0, 8.0);
    ui.set_inline_style(letter, &format!("left: {:.0}px; top: {:.0}px", at.x, at.y));
}
