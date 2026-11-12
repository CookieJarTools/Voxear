//! Maps as scripts reach them: an Obj's Map and its Map Marker, by their
//! kinds (see `voxear_scripting::ComponentKind`), and what every map
//! follows, is titled and has placed on it, kept in the world as a
//! [`MapScript`] for the game's maps to read.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scripting::ComponentKind;

use super::{
    hud::PlacedMarker,
    parts::{Map, MapCorner, MapIcon, MapMarker, MarkerShows},
};

/// What scripts asked of every map: the Obj it follows in place of the
/// camera, its title in place of the country it looks at, and the markers
/// placed on it, each by the number its script holds it by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapScript {
    pub follow: Option<Entity>,
    pub title: Option<String>,
    pub placed: HashMap<u32, PlacedMarker>,
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
    }
}
