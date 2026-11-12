//! Maps as scripts reach them: an Obj's Map and its Map Marker, by their
//! kinds (see `voxear_scripting::ComponentKind`), and what every map
//! follows, is titled and has placed on it, kept in the world as a
//! [`MapScript`] for the game's maps to read.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scripting::ComponentKind;
use voxear_terrain::blocks::map::{MAP_TILE, MapTile};
use voxear_ui::MapViewSettings;

use super::{
    ShownMap,
    hud::PlacedMarker,
    known::{MapKnown, MapLayerLook, MapShows},
    parts::{Map, MapCorner, MapIcon, MapMarker, MarkerShows},
};
use voxear_terrain::blocks::map_known::MapKnowledge;

/// What scripts asked of every map: the Obj it follows in place of the
/// camera, its title in place of the country it looks at, and the markers
/// placed on it, each by the number its script holds it by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapScript {
    pub follow: Option<Entity>,
    pub title: Option<String>,
    pub placed: HashMap<u32, PlacedMarker>,
    /// What scripts asked of a map view since it was last shown, by the
    /// number they hold its element by: done the next time it is.
    pub wishes: HashMap<u32, MapViewWish>,
    /// How each map view was last seen, for scripts to read.
    pub seen: HashMap<u32, MapViewSeen>,
}

/// What a script asked of one map view: each part none for as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapViewWish {
    /// Radians from north, and over the ground.
    pub turn: Option<f32>,
    pub tilt: Option<f32>,
    /// Blocks to a cell: any number, not only the powers of two.
    pub zoom: Option<f32>,
    /// Metres along x and z from what the maps follow.
    pub offset: Option<Vec2>,
    /// A place in the scene to look at, in place of an offset.
    pub look_at: Option<Vec3>,
    /// Back to how its settings have it: north up, over what it follows.
    pub reset: bool,
    /// Settings by their names, as `MapViewSettings::set_all` reads them.
    pub settings: Vec<String>,
}

/// How a map view was seen when it was last shown.
#[derive(Clone, Debug, PartialEq)]
pub struct MapViewSeen {
    pub turn: f32,
    pub tilt: f32,
    pub zoom: f32,
    pub offset: Vec2,
    pub shown: ShownMap,
    pub settings: MapViewSettings,
    /// The tiles its window held, by where each is, for finding the ground
    /// under a point of the view.
    pub tiles: Arc<HashMap<[i32; 2], Arc<MapTile>>>,
}

impl MapViewSeen {
    /// The place in the scene under a point of the view, in the UI's pixels
    /// from its top left: where a ray through it first comes down onto the
    /// map's ground. None off the ground it shows.
    pub fn place(&self, point: Vec2) -> Option<Vec3> {
        let shown = &self.shown;
        let (origin, direction) = shown.camera.ray(point, shown.size);
        let tile = MAP_TILE as i32;
        // From a little before the slab to a little past it, a quarter of a cell at a time.
        let (near, far) = (shown.camera.distance - shown.reach * 3.0, shown.camera.distance + shown.reach * 3.0);
        let steps = ((far - near.max(0.0)) * 4.0) as usize;
        (0..steps).map(|step| origin + direction * (near.max(0.0) + step as f32 * 0.25)).find_map(|at| {
            if shown.past(at) > 1.0 {
                return None;
            }
            let cell = [at.x.floor() as i32 + shown.corner[0], at.z.floor() as i32 + shown.corner[1]];
            let held = self.tiles.get(&[cell[0].div_euclid(tile), cell[1].div_euclid(tile)])?;
            let column = held.column([cell[0].rem_euclid(tile) as u32, cell[1].rem_euclid(tile) as u32]);
            let under = column.is_some() && at.y + shown.base as f32 <= column.highest() as f32;
            let cells = Vec3::new(
                cell[0] as f32 + at.x.fract().abs(),
                column.highest() as f32,
                cell[1] as f32 + at.z.fract().abs(),
            );
            under.then(|| shown.origin + cells * shown.cell)
        })
    }
}

/// Sets what answers `Map` and `MapMarker` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Map, map_values);
    voxear_scripting::set_kind_texts(ComponentKind::Map, map_texts);
    voxear_scripting::set_kind_values(ComponentKind::MapMarker, marker_values);
    voxear_scripting::set_kind_texts(ComponentKind::MapMarker, marker_texts);
}

// What the values read or write, as `Map` and `MapMarker` in Voxear.Core number them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const MINIMAP: i32 = 3;
const CORNER: i32 = 4;
const SIZE: i32 = 5;
const ZOOM: i32 = 6;
const TURNS: i32 = 7;
const ROUND: i32 = 8;
const TILT: i32 = 9;
const MAP_ZOOM: i32 = 10;
const OPEN: i32 = 11;
const SHOWS_WHAT: i32 = 12;
const MAP_SIZE: i32 = 13;
const MAP_TILT: i32 = 14;
const POINTER_CONTROL: i32 = 15;
// The numbers scripts hold its minimap and its map by, bit for bit, and whether they are on the screen yet.
const MINIMAP_VIEW: i32 = 20;
const MAP_VIEW: i32 = 21;
// Of every map, whatever Obj is named.
const FOLLOW: i32 = 100;
const PLACE_AT: i32 = 101;
const PLACE_LOOK: i32 = 102;
const PLACE_COLOR: i32 = 103;
const PLACE_REMOVE: i32 = 104;
const PLACE_CLEAR: i32 = 105;

const ICON: i32 = 3;
const COLOR: i32 = 4;
const SHOWS: i32 = 5;
const EDGE: i32 = 6;

// What the texts read or write.
const TEXT_GET: i32 = 1;
const TEXT_SET: i32 = 2;
const TITLE: i32 = 100;
const PLACE_LABEL: i32 = 101;

const DONE: Option<[f32; 4]> = Some([0.0; 4]);

fn flag(on: bool) -> Option<[f32; 4]> {
    Some([f32::from(u8::from(on)), 0.0, 0.0, 0.0])
}

fn number(value: f32) -> Option<[f32; 4]> {
    Some([value, 0.0, 0.0, 0.0])
}

/// The one of `all` scripts number `number`, or the first.
fn pick<T: Copy>(all: &[T], number: f32) -> T {
    all.get(number.round().max(0.0) as usize).copied().unwrap_or(all[0])
}

fn place_of<T: PartialEq>(all: &[T], one: &T) -> f32 {
    all.iter().position(|other| other == one).unwrap_or(0) as f32
}

fn script(world: &mut World) -> &mut MapScript {
    if world.resource::<MapScript>().is_none() {
        world.insert_resource(MapScript::default());
    }
    world.resource_mut::<MapScript>().expect("put there above")
}

/// A placed marker scripts hold by the number in `bits`, made where there
/// is none.
fn placed(world: &mut World, bits: f32) -> &mut PlacedMarker {
    let fresh = || PlacedMarker { at: Vec3::ZERO, marker: MapMarker::default() };
    script(world).placed.entry(bits.to_bits()).or_insert_with(fresh)
}

/// Reads value `what` of `entity`'s Map, or with `written`, writes it; or
/// one of every map's, whatever `entity` is.
fn map_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match (what, written) {
        (FOLLOW, None) => {
            let bits = script(world).follow.filter(|followed| world.is_alive(*followed)).map_or(0, Entity::to_bits);
            return Some([f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32), 0.0, 0.0]);
        }
        (FOLLOW, Some([low, high, ..])) => {
            let bits = u64::from(low.to_bits()) | u64::from(high.to_bits()) << 32;
            script(world).follow = (bits != 0).then(|| Entity::from_bits(bits));
            return DONE;
        }
        (PLACE_AT, Some([id, x, y, z])) => {
            placed(world, id).at = Vec3::new(x, y, z);
            return DONE;
        }
        (PLACE_LOOK, Some([id, icon, shows, edge])) => {
            let marker = &mut placed(world, id).marker;
            (marker.icon, marker.shows) = (pick(&MapIcon::ALL, icon), pick(&MarkerShows::ALL, shows));
            marker.stays_at_edge = edge != 0.0;
            return DONE;
        }
        (PLACE_COLOR, Some([id, r, g, b])) => {
            placed(world, id).marker.color = [r, g, b];
            return DONE;
        }
        (PLACE_REMOVE, Some([id, ..])) => {
            script(world).placed.remove(&id.to_bits());
            return DONE;
        }
        (PLACE_CLEAR, Some(_)) => {
            script(world).placed.clear();
            return DONE;
        }
        _ => {}
    }
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return flag(world.has::<Map>(entity)),
        (ADD, Some(_)) => {
            if !world.has::<Map>(entity) {
                world.insert(entity, Map::default());
            }
            return DONE;
        }
        (REMOVE, Some(_)) => return world.remove::<Map>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let map = world.get_mut::<Map>(entity)?;
    match (what, written) {
        (MINIMAP, None) => flag(map.minimap),
        (CORNER, None) => number(place_of(&MapCorner::ALL, &map.corner)),
        (SIZE, None) => number(map.size),
        (ZOOM, None) => number(map.zoom as f32),
        (TURNS, None) => flag(map.turns),
        (ROUND, None) => flag(map.round),
        (TILT, None) => number(map.tilt),
        (MAP_ZOOM, None) => number(map.map_zoom as f32),
        (OPEN, None) => flag(map.open),
        (SHOWS_WHAT, None) => number(place_of(&MapShows::ALL, &map.shows)),
        (MAP_SIZE, None) => number(map.map_size),
        (MAP_TILT, None) => number(map.map_tilt),
        (POINTER_CONTROL, None) => flag(map.pointer_control),
        (MINIMAP_VIEW | MAP_VIEW, None) => {
            let view = map.views.map(|views| views[usize::from(what == MAP_VIEW)]);
            Some([f32::from_bits(view.unwrap_or(0)), f32::from(u8::from(view.is_some())), 0.0, 0.0])
        }
        (_, None) => None,
        (_, Some([value, ..])) => {
            match what {
                MINIMAP => map.minimap = value != 0.0,
                CORNER => map.corner = pick(&MapCorner::ALL, value),
                SIZE => map.size = value.clamp(48.0, 1024.0),
                ZOOM => map.zoom = voxear_ui::MapViewSettings::zoom_of(value),
                TURNS => map.turns = value != 0.0,
                ROUND => map.round = value != 0.0,
                TILT => map.tilt = value.clamp(20.0, 90.0),
                MAP_ZOOM => map.map_zoom = voxear_ui::MapViewSettings::zoom_of(value),
                OPEN => map.open = value != 0.0,
                SHOWS_WHAT => map.shows = pick(&MapShows::ALL, value),
                MAP_SIZE => map.map_size = value.clamp(0.1, 3.0),
                MAP_TILT => map.map_tilt = value.clamp(20.0, 90.0),
                POINTER_CONTROL => map.pointer_control = value != 0.0,
                _ => return None,
            }
            DONE
        }
    }
}

/// Reads or writes a text of `entity`'s Map: its action's id; or of every
/// map: its title, and a placed marker's label, as its number, a line
/// break and the label.
fn map_texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if what == VIEW {
        return view_texts(world, text);
    }
    if what >= KNOWN_NEW {
        return known_texts(world, what, text);
    }
    match what {
        TITLE => {
            script(world).title = (!text.is_empty()).then(|| text.to_string());
            Some(String::new())
        }
        PLACE_LABEL => {
            let (id, label) = text.split_once('\n')?;
            let id: u32 = id.parse().ok()?;
            placed(world, f32::from_bits(id)).marker.label = label.to_string();
            Some(String::new())
        }
        TEXT_GET => Some(world.get::<Map>(entity)?.open_action.clone().unwrap_or_default()),
        TEXT_SET => {
            world.get_mut::<Map>(entity)?.open_action = (!text.is_empty()).then(|| text.to_string());
            Some(String::new())
        }
        _ => None,
    }
}

// What scripts ask of the maps' knowledge, each with its numbers in its text, a space between: the knowledge's own
// number first, places as the scene has them along x and z.
const KNOWN_NEW: i32 = 200;
const KNOWN_DROP: i32 = 201;
const KNOWN_SHOW: i32 = 202;
const KNOWN_SHOWN: i32 = 203;
const KNOWN_REVEAL: i32 = 204;
const KNOWN_ALL: i32 = 205;
const KNOWN_FORGET: i32 = 206;
const KNOWN_KNOWS: i32 = 207;
const KNOWN_PAINT: i32 = 208;
const KNOWN_NEWS: i32 = 209;
const KNOWN_BYTES: i32 = 210;
const KNOWN_LOAD: i32 = 211;
const KNOWN_MERGE: i32 = 212;
const KNOWN_CHANGES: i32 = 213;
const KNOWN_STAMP: i32 = 214;
const KNOWN_LAYER: i32 = 215;
const KNOWN_PAPER: i32 = 216;
const KNOWN_EXPLORE: i32 = 217;
const KNOWN_JOIN: i32 = 218;

fn known(world: &mut World) -> &mut MapKnown {
    if world.resource::<MapKnown>().is_none() {
        world.insert_resource(MapKnown::default());
    }
    world.resource_mut::<MapKnown>().expect("put there above")
}

/// Answers what a script asks of the maps' knowledge (see the numbers
/// above): `None` for what cannot be read, or a knowledge there is none of.
fn known_texts(world: &mut World, what: i32, text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let numbers: Vec<f32> = text.split_whitespace().map_while(|word| word.parse().ok()).collect();
    let done = Some(String::new());
    // Places are told as the scene has them: the knowledge keeps them from the world of blocks' corner.
    let corner = super::corner(world).unwrap_or_default();
    let place = |x: f32, z: f32| [x - corner.x, z - corner.z];
    let known = known(world);
    match (what, &numbers[..]) {
        (KNOWN_SHOWN, _) => return Some(known.shown.to_string()),
        (KNOWN_LAYER, [layer, lr, lg, lb, la, hr, hg, hb, ha]) => {
            let look = MapLayerLook { low: [*lr, *lg, *lb, *la], high: [*hr, *hg, *hb, *ha] };
            known.layers.insert(*layer as u8, look);
            known.looks += 1;
            return done;
        }
        (KNOWN_PAPER, [r, g, b]) => {
            known.paper = [*r, *g, *b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            known.looks += 1;
            return done;
        }
        (KNOWN_EXPLORE, [radius]) => {
            known.explore = radius.max(0.0);
            return done;
        }
        _ => {}
    }
    let id: u32 = words.next()?.parse().ok()?;
    match (what, &numbers[1.min(numbers.len())..]) {
        (KNOWN_NEW, _) => {
            known.all.insert(id, MapKnowledge::new());
            return done;
        }
        (KNOWN_DROP, _) => {
            // The one there is to begin with stays, and so does the one shown.
            if id != 0 && id != known.shown {
                known.all.remove(&id);
            }
            return done;
        }
        (KNOWN_SHOW, _) => {
            known.shown = if known.all.contains_key(&id) { id } else { known.shown };
            return done;
        }
        (KNOWN_JOIN, [other]) => {
            let other = known.all.get(&(*other as u32))?.clone();
            known.all.get_mut(&id)?.merge(&other);
            return done;
        }
        _ => {}
    }
    let knowledge = known.all.get_mut(&id)?;
    match (what, &numbers[1.min(numbers.len())..]) {
        (KNOWN_REVEAL, [x, z, radius]) => {
            let [x, z] = place(*x, *z);
            Some(u8::from(knowledge.reveal(x, z, *radius)).to_string())
        }
        (KNOWN_ALL, [all]) => {
            knowledge.reveal_all(*all != 0.0);
            done
        }
        (KNOWN_FORGET, _) => {
            knowledge.forget();
            done
        }
        (KNOWN_KNOWS, [x, z]) => {
            let [x, z] = place(*x, *z);
            Some(u8::from(knowledge.knows(x, z)).to_string())
        }
        (KNOWN_PAINT, [layer, x, z, radius, value, heard]) => {
            knowledge.paint(*layer as u8, place(*x, *z), *radius, *value, *heard as u32);
            done
        }
        (KNOWN_NEWS, [layer, x, z]) => {
            let [x, z] = place(*x, *z);
            let news = knowledge.news(*layer as u8, x, z);
            Some(news.map_or(String::new(), |(value, heard)| format!("{value} {heard}")))
        }
        (KNOWN_BYTES, _) => Some(base64::encode(knowledge.to_bytes())),
        (KNOWN_CHANGES, [since]) => Some(base64::encode(knowledge.changes_since(*since as u32))),
        (KNOWN_STAMP, _) => Some(knowledge.stamp().to_string()),
        (KNOWN_LOAD | KNOWN_MERGE, _) => {
            let read = MapKnowledge::from_bytes(&base64::decode(words.next()?).ok()?).ok()?;
            match what {
                KNOWN_LOAD => {
                    // Its stamp goes on from where it was, so what the maps drew of it is drawn again.
                    let stamp = knowledge.stamp();
                    knowledge.forget();
                    knowledge.merge(&read);
                    debug_assert!(knowledge.stamp() > stamp);
                }
                _ => knowledge.merge(&read),
            }
            done
        }
        _ => None,
    }
}

// What scripts ask of one map view: its element's number, a word for what, and that word's numbers. Angles are in
// degrees, as scripts have them.
const VIEW: i32 = 300;

/// Answers what a script asks of a map view: what it sets is done when the
/// view is next shown, and what it reads is the view as it was last shown.
/// `None` for what cannot be read, or for a view not shown yet.
fn view_texts(world: &mut World, text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let view: u32 = words.next()?.parse().ok()?;
    let verb = words.next()?;
    let rest: Vec<&str> = words.collect();
    let numbers: Vec<f32> = rest.iter().map_while(|word| word.parse().ok()).collect();
    let done = Some(String::new());
    let script = script(world);
    if let Some(seen) = script.seen.get(&view) {
        match (verb, &numbers[..]) {
            ("get", _) => {
                let (turn, tilt) = (seen.turn.to_degrees(), seen.tilt.to_degrees());
                return Some(format!("{turn} {tilt} {} {} {}", seen.zoom, seen.offset.x, seen.offset.y));
            }
            ("settings", _) => return Some(seen.settings.to_text()),
            ("point", [x, y, z]) => {
                let (at, inside) = seen.shown.point(Vec3::new(*x, *y, *z))?;
                return Some(format!("{} {} {}", at.x, at.y, u8::from(inside)));
            }
            ("place", [x, y]) => {
                let at = seen.place(Vec2::new(*x, *y));
                return Some(at.map_or(String::new(), |at| format!("{} {} {}", at.x, at.y, at.z)));
            }
            _ => {}
        }
    }
    let wish = script.wishes.entry(view).or_default();
    match (verb, &numbers[..]) {
        ("turn", [degrees]) => wish.turn = Some(degrees.to_radians()),
        ("tilt", [degrees]) => wish.tilt = Some(degrees.to_radians()),
        ("zoom", [blocks]) => wish.zoom = Some(blocks.max(0.1)),
        ("offset", [x, z]) => (wish.offset, wish.look_at) = (Some(Vec2::new(*x, *z)), None),
        ("look", [x, y, z]) => (wish.look_at, wish.offset) = (Some(Vec3::new(*x, *y, *z)), None),
        ("reset", _) => {
            *wish = MapViewWish { reset: true, settings: std::mem::take(&mut wish.settings), ..Default::default() }
        }
        ("set", _) => wish.settings.push(rest.join(" ")),
        _ => return None,
    }
    done
}

/// Reads value `what` of `entity`'s Map Marker, or with `written`, writes
/// it.
fn marker_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return flag(world.has::<MapMarker>(entity)),
        (ADD, Some(_)) => {
            if !world.has::<MapMarker>(entity) {
                world.insert(entity, MapMarker::default());
            }
            return DONE;
        }
        (REMOVE, Some(_)) => return world.remove::<MapMarker>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let marker = world.get_mut::<MapMarker>(entity)?;
    match (what, written) {
        (ICON, None) => number(place_of(&MapIcon::ALL, &marker.icon)),
        (COLOR, None) => Some([marker.color[0], marker.color[1], marker.color[2], 1.0]),
        (SHOWS, None) => number(place_of(&MarkerShows::ALL, &marker.shows)),
        (EDGE, None) => flag(marker.stays_at_edge),
        (ICON, Some([icon, ..])) => {
            marker.icon = pick(&MapIcon::ALL, icon);
            DONE
        }
        (COLOR, Some([r, g, b, _])) => {
            marker.color = [r, g, b];
            DONE
        }
        (SHOWS, Some([shows, ..])) => {
            marker.shows = pick(&MarkerShows::ALL, shows);
            DONE
        }
        (EDGE, Some([edge, ..])) => {
            marker.stays_at_edge = edge != 0.0;
            DONE
        }
        _ => None,
    }
}

/// Reads or writes `entity`'s Map Marker's label.
fn marker_texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    match what {
        TEXT_GET => Some(world.get::<MapMarker>(entity)?.label.clone()),
        TEXT_SET => {
            world.get_mut::<MapMarker>(entity)?.label = text.to_string();
            Some(String::new())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_set_a_map_and_a_marker_by_their_numbers_and_place_markers_by_a_number_of_their_own() {
        let mut world = World::new();
        let obj = world.spawn(());
        assert_eq!(map_values(&mut world, obj, HAS, None), flag(false));
        map_values(&mut world, obj, ADD, Some([0.0; 4]));
        map_values(&mut world, obj, CORNER, Some([2.0, 0.0, 0.0, 0.0]));
        map_values(&mut world, obj, ZOOM, Some([7.0, 0.0, 0.0, 0.0]));
        map_values(&mut world, obj, OPEN, Some([1.0, 0.0, 0.0, 0.0]));
        map_texts(&mut world, obj, TEXT_SET, "open-map");
        let map = world.get::<Map>(obj).unwrap();
        assert_eq!((map.corner, map.zoom, map.open), (MapCorner::BottomLeft, 8, true));
        assert_eq!(map_texts(&mut world, obj, TEXT_GET, ""), Some("open-map".into()));
        assert_eq!(map_values(&mut world, obj, ZOOM, None), number(8.0));

        marker_values(&mut world, obj, ADD, Some([0.0; 4]));
        marker_values(&mut world, obj, ICON, Some([2.0, 0.0, 0.0, 0.0]));
        marker_texts(&mut world, obj, TEXT_SET, "Oakrest");
        let marker = world.get::<MapMarker>(obj).unwrap();
        assert_eq!((marker.icon, marker.label.as_str()), (MapIcon::Town, "Oakrest"));

        // Every map's: what it follows, its title, and markers at places, whatever Obj is named.
        let none = Entity::from_bits(0);
        let bits = obj.to_bits();
        let followed = [f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32), 0.0, 0.0];
        map_values(&mut world, none, FOLLOW, Some(followed));
        assert_eq!(map_values(&mut world, none, FOLLOW, None), Some(followed));
        map_texts(&mut world, none, TITLE, "Asrior");
        let id = f32::from_bits(7);
        map_values(&mut world, none, PLACE_AT, Some([id, 1.0, 2.0, 3.0]));
        map_values(&mut world, none, PLACE_LOOK, Some([id, 3.0, 2.0, 1.0]));
        map_texts(&mut world, none, PLACE_LABEL, "7\nThe Pit");
        let script = world.resource::<MapScript>().unwrap();
        assert_eq!((script.follow, script.title.as_deref()), (Some(obj), Some("Asrior")));
        let pit = &script.placed[&7];
        assert_eq!(
            (pit.at, pit.marker.icon, pit.marker.shows),
            (Vec3::new(1.0, 2.0, 3.0), MapIcon::Dungeon, MarkerShows::Map)
        );
        assert!(pit.marker.stays_at_edge && pit.marker.label == "The Pit");
        map_values(&mut world, none, PLACE_REMOVE, Some([id, 0.0, 0.0, 0.0]));
        assert!(world.resource::<MapScript>().unwrap().placed.is_empty());

        // How a view is seen: what a script asks waits for the view to be shown; nothing is read of one not shown.
        assert_eq!(map_texts(&mut world, none, VIEW, "12 get"), None);
        map_texts(&mut world, none, VIEW, "12 turn 90");
        map_texts(&mut world, none, VIEW, "12 zoom 6");
        map_texts(&mut world, none, VIEW, "12 look 5 0 7");
        map_texts(&mut world, none, VIEW, "12 set fill=1.5 controls=false");
        let wish = &world.resource::<MapScript>().unwrap().wishes[&12];
        assert!((wish.turn.unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!((wish.zoom, wish.look_at, wish.offset), (Some(6.0), Some(Vec3::new(5.0, 0.0, 7.0)), None));
        assert_eq!(wish.settings, ["fill=1.5 controls=false"]);
        // Reset forgets how it was asked to be seen, and keeps the settings asked.
        map_texts(&mut world, none, VIEW, "12 reset");
        let wish = &world.resource::<MapScript>().unwrap().wishes[&12];
        assert!(wish.reset && wish.turn.is_none() && wish.settings.len() == 1);
        // The Map's own settings for its opened map.
        map_values(&mut world, obj, MAP_SIZE, Some([1.4, 0.0, 0.0, 0.0]));
        map_values(&mut world, obj, POINTER_CONTROL, Some([0.0; 4]));
        let map = world.get::<Map>(obj).unwrap();
        assert_eq!((map.map_size, map.pointer_control), (1.4, false));
        assert_eq!(map_values(&mut world, obj, MAP_VIEW, None).unwrap()[1], 0.0, "not on the screen yet");
    }

    #[test]
    fn scripts_keep_a_map_for_each_player_reveal_and_paint_them_and_pass_them_as_text() {
        let mut world = World::new();
        let none = Entity::from_bits(0);
        let mut ask = |what: i32, text: &str| map_texts(&mut world, none, what, text);
        // The one there is to begin with, 0, is the one shown; another is made for a second player.
        assert_eq!(ask(KNOWN_SHOWN, ""), Some("0".into()));
        ask(KNOWN_NEW, "7");
        assert_eq!(ask(KNOWN_REVEAL, "0 100 -40 30"), Some("1".into()));
        assert_eq!(ask(KNOWN_REVEAL, "0 100 -40 30"), Some("0".into()), "nothing new the second time");
        assert_eq!(ask(KNOWN_KNOWS, "0 100 -40"), Some("1".into()));
        assert_eq!(ask(KNOWN_KNOWS, "7 100 -40"), Some("0".into()), "the other player has not been there");
        assert_eq!(ask(KNOWN_KNOWS, "9 100 -40"), None, "there is no such map");
        // News heard by one, then passed to the other as text, as over the network or into a save.
        ask(KNOWN_PAINT, "0 1 500 500 0 0.75 42");
        let before: u32 = ask(KNOWN_STAMP, "7").unwrap().parse().unwrap();
        let sent = ask(KNOWN_BYTES, "0").unwrap();
        assert!(sent.len() < 400 && !sent.contains(' '), "{sent}");
        assert_eq!(ask(KNOWN_MERGE, &format!("7 {sent}")), Some(String::new()));
        assert_eq!(ask(KNOWN_KNOWS, "7 100 -40"), Some("1".into()));
        let news = ask(KNOWN_NEWS, "7 1 500 500").unwrap();
        let heard: Vec<f32> = news.split(' ').map(|word| word.parse().unwrap()).collect();
        assert!((heard[0] - 0.75).abs() < 0.01 && heard[1] == 42.0, "{news}");
        assert_eq!(ask(KNOWN_NEWS, "7 1 0 0"), Some(String::new()), "nothing heard of there");
        assert!(ask(KNOWN_STAMP, "7").unwrap().parse::<u32>().unwrap() > before);
        // Loaded, a map is what was saved and no more; joined, it has both.
        ask(KNOWN_REVEAL, "7 -900 -900 10");
        ask(KNOWN_LOAD, &format!("7 {sent}"));
        assert_eq!(ask(KNOWN_KNOWS, "7 -900 -900"), Some("0".into()));
        ask(KNOWN_REVEAL, "7 -900 -900 10");
        ask(KNOWN_JOIN, "0 7");
        assert_eq!(ask(KNOWN_KNOWS, "0 -900 -900"), Some("1".into()));
        // The other player's map shown in place of the first; layers' colours and the paper's are the game's.
        ask(KNOWN_SHOW, "7");
        ask(KNOWN_LAYER, "1 0 1 0 0 1 0 0 0.8");
        ask(KNOWN_PAPER, "1 1 1");
        ask(KNOWN_DROP, "7");
        let known = world.resource::<MapKnown>().unwrap();
        assert_eq!((known.shown, known.paper, known.all.len()), (7, [255; 3], 2), "the one shown is not dropped");
        assert_eq!(known.layers[&1].high, [1.0, 0.0, 0.0, 0.8]);
    }
}
