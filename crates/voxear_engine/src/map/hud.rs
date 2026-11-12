//! What shows with a map besides the ground: the minimap and the map a Map
//! component puts on the screen, opened and closed by its action, and for
//! every map view what its parts say and where its marks stand: its
//! markers and their labels, the mark of what it follows, the country it
//! shows and where, the time and the air there, its compass and its zoom.

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldOrigin, WorldTransform};
use voxear_scripting::CursorLock;
use voxear_terrain::{blocks::map::MapGround, surfaces::Biome};
use voxear_ui::{Color, ElementKind, MapViewKind, MapViewSettings, NodeId, Shape, Ui};

use super::{
    MapKnown, MapScript, MapShows, MapViewer, Maps, ShownMap,
    parts::{Map, MapCorner, MapMarker, MarkerShows},
};
use crate::{game::Display, game_ui::GameUi};

/// A Map's minimap and map on the screen: their document, the settings
/// they were last given, and what opening the map changed, to put back.
pub(super) struct Hud {
    document: u32,
    mini: NodeId,
    large: NodeId,
    /// How far the minimap's sizes were scaled when it was last placed.
    scale: f32,
    /// The numbers scripts hold the two by.
    numbers: [u32; 2],
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

/// How far a view's edge markers stay inside it, in pixels.
const EDGE: f32 = 4.0;

/// The colour of what the maps follow and of north, and the ink round it.
const ACCENT: Color = Color::rgb(0xff, 0xd2, 0x3f);
const INK: Color = Color::rgb(0x0d, 0x10, 0x16);

/// What a map's panels say of where it looks: the country's name, where
/// along x and z, and how warm and how wet it is there, where the world's
/// generator says.
struct Readout {
    name: String,
    at: [f64; 2],
    air: Option<(f32, f32)>,
    blocks: f32,
}

/// A number as a map's panels write it: three places, and a true minus.
fn written(number: f64) -> String {
    format!("{number:.3}").replace('-', "\u{2212}")
}

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
        // What the first Map says maps show is what they show, and seen from where they follow, ground is explored.
        if let Some((_, map)) = found.first() {
            if world.resource::<MapKnown>().is_none_or(|known| known.shows != map.shows) {
                let mut known = world.remove_resource::<MapKnown>().unwrap_or_default();
                known.shows = map.shows;
                world.insert_resource(known);
            }
        }
        self.explore(world);
        for (entity, mut map) in found {
            if !self.huds.contains_key(&entity) {
                let Some(hud) = make(ui) else { continue };
                self.huds.insert(entity, hud);
            }
            let hud = self.huds.get_mut(&entity).expect("made above");
            // Its views' numbers, for scripts to reach them as elements.
            if map.views != Some(hud.numbers) {
                map.views = Some(hud.numbers);
                if let Some(kept) = world.get_mut::<Map>(entity) {
                    kept.views = map.views;
                }
            }
            let action = map.open_action.as_deref().and_then(|id| input.action(id));
            if action.is_some_and(|action| input.state(action).pressed) {
                map.open = !map.open;
                if let Some(kept) = world.get_mut::<Map>(entity) {
                    kept.open = map.open;
                }
            }
            // Set again when its Map changes, and when the screen's size scaled its parts anew.
            let scale = ui.ui().map_view_scale(hud.mini);
            if hud.given.as_ref() != Some(&map) || hud.scale != scale {
                give(ui.ui(), hud, &map);
                (hud.given, hud.scale) = (Some(map.clone()), scale);
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

    /// Where the maps show what was explored, sees the ground round where
    /// they follow, in the knowledge they show.
    fn explore(&self, world: &mut World) {
        if world.resource::<MapKnown>().is_none_or(|known| known.shows != MapShows::Explored) {
            return;
        }
        let (Some(viewer), Some((_, _, _, corner))) = (self.viewer(world), super::block_world(world)) else { return };
        let Some(known) = world.resource_mut::<MapKnown>() else { return };
        let (shown, radius) = (known.shown, known.explore);
        if let Some(knowledge) = known.all.get_mut(&shown) {
            knowledge.reveal(viewer.position.x - corner.x, viewer.position.z - corner.z, radius);
        }
    }

    /// Keeps what one map view's parts say and where its marks stand: its
    /// markers and their labels, the mark of what it follows, the country
    /// it shows, the air there, its compass and its zoom.
    pub(super) fn overlay(
        &mut self,
        (world, script): (&World, &MapScript),
        ui: &mut Ui,
        (node, settings): (NodeId, &MapViewSettings),
        viewer: MapViewer,
    ) {
        let Some(parts) = ui.map_view_parts(node) else { return };
        let Some(shown) = self.shown.get(&node).copied() else {
            ui.set_shapes(parts.overlay, Vec::new());
            ui.set_shapes(parts.compass, Vec::new());
            self.label(ui, node, parts.overlay, Vec::new());
            show(ui, parts.tag, false);
            return;
        };
        let large = settings.kind == MapViewKind::Map;
        let scale = ui.map_view_scale(node);
        let centre = shown.size * 0.5;
        let in_view = |at: Vec2| at.x >= 0.0 && at.y >= 0.0 && at.x <= shown.size.x && at.y <= shown.size.y;
        // On the slab's edge, towards a place past it; kept inside the view where the slab runs past it.
        let at_edge = |place: Vec3, radius: f32| {
            let edge = shown.edge_towards(place)?;
            let (from, room) = (edge - centre, centre - Vec2::splat(radius + EDGE));
            let fits = (room.x / from.x.abs().max(1e-3)).min(room.y / from.y.abs().max(1e-3)).min(1.0);
            Some(centre + from * fits)
        };
        let mut shapes = Vec::new();
        let mut labels: Vec<(Vec2, String)> = Vec::new();
        for (position, marker) in self.markers(world, script) {
            let on_this = match marker.shows {
                MarkerShows::Both => true,
                MarkerShows::Minimap => !large,
                MarkerShows::Map => large,
            };
            let Some((mut at, inside)) = shown.point(position).filter(|_| on_this) else { continue };
            let mut radius = if large { 13.0 } else { 9.0 } * scale;
            let on_ground = inside && in_view(at);
            if !on_ground {
                radius *= 0.8;
                let Some(edge) = at_edge(position, radius).filter(|_| marker.stays_at_edge) else { continue };
                at = edge;
            }
            let [r, g, b] = marker.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let border = Some((2.0 * scale, INK));
            shapes.push(Shape::Circle { center: at.to_array(), radius, fill: Color::rgb(r, g, b), border });
            let icon = Shape::Icon {
                name: marker.icon.icon(),
                center: at.to_array(),
                size: radius * 1.3,
                color: Color::WHITE,
            };
            shapes.push(icon);
            if settings.labels && on_ground && !marker.label.is_empty() {
                labels.push((at + Vec2::new(radius + 6.0 * scale, -10.0), marker.label.clone()));
            }
        }
        // What the map follows: a dot in a ring; at the slab's edge, towards where it is, once the map was moved
        // off it. A map names it over the dot, on a stem.
        let followed = shown.point(viewer.position).filter(|_| settings.arrow);
        let dot = if large { 8.0 } else { 6.0 } * scale;
        let ring = dot + if large { 5.0 } else { 8.0 } * scale;
        let mark = followed.and_then(|(at, inside)| match inside && in_view(at) {
            true => Some((at, true)),
            false => Some((at_edge(viewer.position, ring).filter(|_| settings.arrow_at_edge)?, false)),
        });
        let name = script.player.clone().or_else(|| {
            let marker = world.get::<MapMarker>(script.follow?)?;
            Some(marker.label.clone()).filter(|label| !label.is_empty())
        });
        let named = mark.filter(|(_, on_ground)| *on_ground).zip(name).zip(parts.tag.zip(parts.tag_name));
        if let Some((at, _)) = mark {
            if named.is_some() {
                let stem = Shape::Line {
                    from: (at - Vec2::Y * dot).to_array(),
                    to: (at - Vec2::Y * (dot + 26.0 * scale)).to_array(),
                    width: 2.0 * scale,
                    color: Color::WHITE,
                };
                shapes.push(stem);
            }
            let faint = Color::rgba(255, 255, 255, if large { 87 } else { 102 });
            shapes.push(Shape::Circle { center: at.to_array(), radius: ring, fill: faint, border: None });
            let border = Some((3.0 * scale, INK));
            shapes.push(Shape::Circle { center: at.to_array(), radius: dot, fill: ACCENT, border });
        }
        match named {
            Some((((at, _), name), (tag, tag_name))) => {
                self.write(ui, tag_name, name);
                // Over the stem, as wide as it was last laid out.
                let [_, _, width, height] = ui.rect(tag);
                let top = at.y - dot - 26.0 * scale - height;
                ui.set_inline_property(tag, "left", &format!("{:.1}px", at.x - width * 0.5));
                ui.set_inline_property(tag, "top", &format!("{top:.1}px"));
                show(ui, Some(tag), true);
            }
            None => show(ui, parts.tag, false),
        }
        ui.set_shapes(parts.overlay, shapes);
        self.label(ui, node, parts.overlay, labels);

        // The country it looks at and where; the time, as the game tells it, and the air there.
        let read = self.readout(world, &shown);
        self.write(ui, parts.title, script.title.clone().unwrap_or(read.name));
        self.write(ui, parts.coords[0], written(read.at[0]));
        self.write(ui, parts.coords[1], written(read.at[1]));
        self.write(ui, parts.time, script.time.clone().unwrap_or_default());
        let air = |value: Option<f32>| value.map_or("\u{2013}".to_string(), |value| format!("{value:.0}"));
        self.write(ui, parts.warmth, air(read.air.map(|(warm, _)| warm)));
        self.write(ui, parts.wetness, air(read.air.map(|(_, wet)| wet * 100.0)));
        let zoom = if read.blocks.fract().abs() < 0.05 {
            format!("{:.0}", read.blocks)
        } else {
            format!("{:.1}", read.blocks)
        };
        self.write(ui, parts.zoom_value, format!("1:{zoom}"));
        // Each part shows or not as the view's settings say: a minimap's strip is its region and its air at once.
        show(ui, Some(parts.time_cell), script.time.is_some());
        show(ui, Some(parts.title), settings.title);
        show(ui, Some(parts.region), if large { settings.title } else { settings.readout });
        show(ui, Some(parts.environment), settings.readout);
        show(ui, Some(parts.zoom), settings.zoom_control);
        show(ui, Some(parts.compass), settings.compass);
        if settings.compass {
            draw_compass(ui, &parts, &shown, (scale, large));
        }
    }

    /// Every marker to show: each Map Marker's, where its Obj is, and those
    /// scripts placed.
    fn markers(&self, world: &World, script: &MapScript) -> Vec<(Vec3, MapMarker)> {
        let objs =
            world.query_ref::<(Entity, &MapMarker, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
        let objs = objs.into_iter().filter(|(entity, ..)| voxear_scene::is_active(world, *entity));
        // What the maps follow has its own mark.
        let objs = objs.filter(|(entity, ..)| script.follow != Some(*entity));
        let objs = objs.map(|(_, marker, local, placed)| (voxear_terrain::corner(local, placed), marker.clone()));
        objs.chain(script.placed.values().map(|placed| (placed.at, placed.marker.clone()))).collect()
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

    /// What a map's panels say of where it looks.
    fn readout(&self, world: &World, shown: &ShownMap) -> Readout {
        let cells = Vec3::new(
            shown.corner[0] as f32 + shown.camera.middle.x,
            0.0,
            shown.corner[1] as f32 + shown.camera.middle.z,
        );
        let level = WorldOrigin::of(world).to_level(shown.origin + cells * shown.cell);
        let kept = self.grounds.get(&shown.world);
        let mut read = Readout { name: String::new(), at: [level.x, level.z], air: None, blocks: shown.blocks };
        let Some(MapGround::Made { making, .. }) = kept.map(|ground| &ground.ground) else { return read };
        // The generator's own place: the world's blocks are counted from where its ground starts in it.
        let [x, z] = [0, 1].map(|axis| making.generate.position[axis] + [cells.x, cells.z][axis] * shown.cell);
        let (warmth, wetness) = making.world.climate(x, z, making.world.height_at(x, z));
        let within =
            |range: Option<[f32; 2]>, value: f32| range.is_none_or(|[low, high]| value >= low && value <= high);
        let country = Biome::ALL.into_iter().find(|biome| {
            let (warm, wet) = biome.ranges();
            within(warm, warmth) && within(wet, wetness)
        });
        read.name = country.map_or(String::new(), |biome| biome.name().to_string());
        read.air = Some((warmth, wetness));
        read
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
    let mut create = |kind| ui.create(root?, kind, "").and_then(|element| Some((element, ui.node(element)?)));
    let (mini, large) = (create(ElementKind::Minimap), create(ElementKind::Map));
    let (Some((mini_number, mini)), Some((large_number, large))) = (mini, large) else {
        ui.close(document);
        return None;
    };
    ui.ui().set_visible(large, false);
    Some(Hud { document, mini, large, scale: 0.0, numbers: [mini_number, large_number], given: None, held: None })
}

/// Sets a Map's minimap and map as it says: the minimap in its corner, as
/// wide as its size at the scale its parts are drawn at.
fn give(ui: &mut Ui, hud: &Hud, map: &Map) {
    let scale = ui.map_view_scale(hud.mini);
    let (gap, width) = (40.0 * scale, map.size * scale);
    let (across, down) = match map.corner {
        MapCorner::TopLeft => ("left", "top"),
        MapCorner::TopRight => ("right", "top"),
        MapCorner::BottomLeft => ("left", "bottom"),
        MapCorner::BottomRight => ("right", "bottom"),
    };
    for side in ["left", "top", "right", "bottom"] {
        let at = if side == across || side == down { format!("{gap:.1}px") } else { String::new() };
        ui.set_inline_property(hud.mini, side, &at);
    }
    ui.set_inline_property(hud.mini, "position", "absolute");
    ui.set_inline_property(hud.mini, "width", &format!("{width:.1}px"));
    // Its own settings over whatever else scripts set on the two views.
    let mini = ui.map_view_settings(hud.mini).unwrap_or(MapViewSettings::of(MapViewKind::Minimap));
    let mini = MapViewSettings { zoom: map.zoom, turns: map.turns, round: map.round, tilt: map.tilt, ..mini };
    ui.set_map_view_settings(hud.mini, mini);
    let large = ui.map_view_settings(hud.large).unwrap_or(MapViewSettings::of(MapViewKind::Map));
    let large = MapViewSettings {
        zoom: map.map_zoom,
        fill: map.map_size,
        tilt: map.map_tilt,
        controls: map.pointer_control,
        ..large
    };
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

/// Shows or hides a part, when it is not so already.
fn show(ui: &mut Ui, part: Option<NodeId>, on: bool) {
    if let Some(part) = part.filter(|part| ui.is_visible(*part) != on) {
        ui.set_visible(part, on);
    }
}

/// Draws a view's compass: a disc lying on the ground as the map is turned
/// and tilted, hairlines across it, a needle from its middle to north, and
/// its letters where each way lies.
fn draw_compass(ui: &mut Ui, parts: &voxear_ui::MapViewParts, shown: &ShownMap, (scale, large): (f32, bool)) {
    let rect = ui.rect(parts.compass);
    let centre = Vec2::new(rect[2], rect[3]) * 0.5;
    let reach = centre.x.min(centre.y) - 1.0;
    if reach < 4.0 {
        return;
    }
    // The ground's ways as the view sees them: east along its right, north up it by how far it is tilted.
    let [_, right, up] = shown.camera.axes();
    let way = |east: f32, north: f32| Vec2::new(right.x * east + right.z * north, -(up.x * east + up.z * north));
    let flat = Vec2::new(right.x, right.z).length().max(1e-3);
    let squash = (Vec2::new(up.x, up.z).length() / flat).clamp(0.05, 1.0);
    let mut shapes = Vec::new();
    // The disc: an ellipse as wide as the compass and as high as it is tilted, filled a row of pixels at a time,
    // each row on the screen's own rows so no seam shows between two.
    let fill = Color::rgba(13, 16, 22, 209);
    let (middle_row, high) = (rect[1] + centre.y, reach * squash);
    for row in (middle_row - high).ceil() as i32..(middle_row + high).floor() as i32 {
        let y = row as f32 + 0.5 - middle_row;
        let half = reach * (1.0 - (y / high).powi(2)).max(0.0).sqrt();
        let strip = [centre.x - half, row as f32 - rect[1], half * 2.0, 1.0];
        shapes.push(Shape::Rect { rect: strip, radius: 0.0, fill });
    }
    let line = |from: Vec2, to: Vec2, width: f32, color: Color| Shape::Line {
        from: (centre + from).to_array(),
        to: (centre + to).to_array(),
        width,
        color,
    };
    // Its rim, which also smooths the strips' ends.
    let rim = Color::rgba(255, 255, 255, 70);
    let around = |step: i32| {
        let (sin, cos) = (step as f32 / 48.0 * std::f32::consts::TAU).sin_cos();
        Vec2::new(cos * reach, sin * reach * squash)
    };
    shapes.extend((0..48).map(|step| line(around(step), around(step + 1), (1.5 * scale).max(1.0), rim)));
    let hair = Color::rgba(255, 255, 255, 36);
    let inner = reach * 0.92;
    shapes.push(line(way(0.0, -1.0) * inner, way(0.0, 1.0) * inner, 1.0, hair));
    shapes.push(line(way(-1.0, 0.0) * inner, way(1.0, 0.0) * inner, 1.0, hair));
    let (needle, width, middle) = if large { (42.0, 4.0, 5.0) } else { (10.0, 3.0, 3.0) };
    shapes.push(line(Vec2::ZERO, way(0.0, 1.0) * needle * scale / flat, width * scale, ACCENT));
    shapes.push(Shape::Circle { center: centre.to_array(), radius: middle * scale, fill: Color::WHITE, border: None });
    ui.set_shapes(parts.compass, shapes);
    // Its letters, each a little in from the rim the way it lies.
    let ways = [(0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)];
    for (letter, (east, north)) in parts.letters.iter().zip(ways).filter_map(|(letter, way)| Some(((*letter)?, way))) {
        let [_, _, width, height] = ui.rect(letter);
        let at = centre + way(east, north) / flat * reach * if large { 0.74 } else { 0.62 };
        ui.set_inline_property(letter, "left", &format!("{:.1}px", at.x - width * 0.5));
        ui.set_inline_property(letter, "top", &format!("{:.1}px", at.y - height * 0.5));
    }
}
