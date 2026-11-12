//! The components maps are set up with: a Map, which puts a minimap and a
//! map of the world on the screen with no UI of the game's own, and a Map
//! Marker, which marks its Obj on every map.

use serde::{Deserialize, Serialize};

use super::known::MapShows;

/// The corner of the screen a Map's minimap sits in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapCorner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

impl MapCorner {
    pub const ALL: [MapCorner; 4] =
        [MapCorner::TopLeft, MapCorner::TopRight, MapCorner::BottomLeft, MapCorner::BottomRight];

    pub fn name(self) -> &'static str {
        match self {
            MapCorner::TopLeft => "Top Left",
            MapCorner::TopRight => "Top Right",
            MapCorner::BottomLeft => "Bottom Left",
            MapCorner::BottomRight => "Bottom Right",
        }
    }
}

/// A Map: shows the scene's world of blocks as a small model of itself, as
/// a minimap in a corner of the screen and a map opened over the game.
#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    /// Whether the minimap shows, the corner it sits in, and how wide it
    /// is, in pixels on a screen 1080 high: it is scaled with the screen.
    pub minimap: bool,
    pub corner: MapCorner,
    pub size: f32,
    /// How many blocks a cell of the minimap is, a power of two.
    pub zoom: u32,
    /// Whether the minimap turns with the view, is cut round, and how far
    /// over the ground it is seen from, in degrees.
    pub turns: bool,
    pub round: bool,
    pub tilt: f32,
    /// How many blocks a cell of the opened map is to begin with, how wide
    /// its ground is drawn as a share of the screen's height, how far over
    /// the ground it is seen from, in degrees, and whether the pointer
    /// turns, moves and zooms it.
    pub map_zoom: u32,
    pub map_size: f32,
    pub map_tilt: f32,
    pub pointer_control: bool,
    /// The input action that opens and closes the map, by its id.
    pub open_action: Option<String>,
    /// What the maps show of the world: all of it, what was explored, or
    /// what the game reveals.
    pub shows: MapShows,
    /// Whether the map is open now: not saved.
    pub open: bool,
    /// The numbers scripts hold its minimap and its map by, once they are
    /// on the screen: not saved.
    pub views: Option<[u32; 2]>,
}

impl Default for Map {
    fn default() -> Self {
        Map {
            minimap: true,
            corner: MapCorner::TopRight,
            size: 380.0,
            zoom: 2,
            turns: true,
            round: false,
            tilt: 58.0,
            map_zoom: 4,
            map_size: 0.92,
            map_tilt: 42.0,
            pointer_control: true,
            open_action: None,
            shows: MapShows::Everything,
            open: false,
            views: None,
        }
    }
}

impl voxear_ecs::Component for Map {}

/// A Map as scenes write it. Left out, a setting has its default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapData {
    pub minimap: bool,
    pub corner: MapCorner,
    pub size: f32,
    pub zoom: u32,
    pub turns: bool,
    pub round: bool,
    pub tilt: f32,
    pub map_zoom: u32,
    pub map_size: f32,
    pub map_tilt: f32,
    pub pointer_control: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_action: Option<String>,
    pub shows: MapShows,
}

impl Default for MapData {
    fn default() -> Self {
        map_data(&Map::default())
    }
}

/// What a scene writes for a Map.
pub fn map_data(map: &Map) -> MapData {
    MapData {
        minimap: map.minimap,
        corner: map.corner,
        size: map.size,
        zoom: map.zoom,
        turns: map.turns,
        round: map.round,
        tilt: map.tilt,
        map_zoom: map.map_zoom,
        map_size: map.map_size,
        map_tilt: map.map_tilt,
        pointer_control: map.pointer_control,
        open_action: map.open_action.clone(),
        shows: map.shows,
    }
}

/// A Map from a scene's data.
pub fn map_of(data: &MapData) -> Map {
    Map {
        minimap: data.minimap,
        corner: data.corner,
        size: data.size.clamp(120.0, 1024.0),
        zoom: voxear_ui::MapViewSettings::zoom_of(data.zoom as f32),
        turns: data.turns,
        round: data.round,
        tilt: data.tilt.clamp(20.0, 90.0),
        map_zoom: voxear_ui::MapViewSettings::zoom_of(data.map_zoom as f32),
        map_size: data.map_size.clamp(0.1, 3.0),
        map_tilt: data.map_tilt.clamp(20.0, 90.0),
        pointer_control: data.pointer_control,
        open_action: data.open_action.clone(),
        shows: data.shows,
        open: false,
        views: None,
    }
}

/// The settings of `data` that differ from `expected`'s, as a prefab
/// instance's override keeps them.
fn changes<T: Serialize>(data: &T, expected: Option<&T>) -> toml::Table {
    let table = |data: &T| toml::Table::try_from(data).unwrap_or_default();
    let mut changes = table(data);
    if let Some(expected) = expected.map(table) {
        changes.retain(|key, value| expected.get(key) != Some(value));
    }
    changes
}

impl MapData {
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        changes(self, expected)
    }
}

/// What a marker is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapIcon {
    #[default]
    Pin,
    Player,
    Town,
    Dungeon,
    Camp,
    Note,
    Chest,
    Skull,
    Star,
    Arrow,
}

impl MapIcon {
    pub const ALL: [MapIcon; 10] = [
        MapIcon::Pin,
        MapIcon::Player,
        MapIcon::Town,
        MapIcon::Dungeon,
        MapIcon::Camp,
        MapIcon::Note,
        MapIcon::Chest,
        MapIcon::Skull,
        MapIcon::Star,
        MapIcon::Arrow,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MapIcon::Pin => "Pin",
            MapIcon::Player => "Player",
            MapIcon::Town => "Town",
            MapIcon::Dungeon => "Dungeon",
            MapIcon::Camp => "Camp",
            MapIcon::Note => "Note",
            MapIcon::Chest => "Chest",
            MapIcon::Skull => "Skull",
            MapIcon::Star => "Star",
            MapIcon::Arrow => "Arrow",
        }
    }

    /// The UI's icon it is drawn with.
    pub fn icon(self) -> &'static str {
        match self {
            MapIcon::Pin => "map-pin",
            MapIcon::Player => "map-player",
            MapIcon::Town => "map-town",
            MapIcon::Dungeon => "map-dungeon",
            MapIcon::Camp => "map-camp",
            MapIcon::Note => "map-note",
            MapIcon::Chest => "map-chest",
            MapIcon::Skull => "map-skull",
            MapIcon::Star => "map-star",
            MapIcon::Arrow => "map-arrow",
        }
    }
}

/// Which maps a marker shows on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerShows {
    #[default]
    Both,
    Minimap,
    Map,
}

impl MarkerShows {
    pub const ALL: [MarkerShows; 3] = [MarkerShows::Both, MarkerShows::Minimap, MarkerShows::Map];

    pub fn name(self) -> &'static str {
        match self {
            MarkerShows::Both => "Both",
            MarkerShows::Minimap => "Minimap",
            MarkerShows::Map => "Map",
        }
    }
}

/// A Map Marker: marks its Obj on every map, where it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapMarker {
    pub icon: MapIcon,
    /// What is written beside it on a map opened large.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Its colour, sRGB.
    pub color: [f32; 3],
    pub shows: MarkerShows,
    /// Whether it stays at a minimap's edge, towards where it is, once it
    /// is past it.
    pub stays_at_edge: bool,
}

impl Default for MapMarker {
    fn default() -> Self {
        MapMarker {
            icon: MapIcon::Pin,
            label: String::new(),
            color: [0.95, 0.75, 0.2],
            shows: MarkerShows::Both,
            stays_at_edge: false,
        }
    }
}

impl voxear_ecs::Component for MapMarker {}

impl MapMarker {
    pub fn changes_from(&self, expected: Option<&Self>) -> toml::Table {
        changes(self, expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_is_written_with_every_setting_and_read_back_within_what_it_can_show() {
        let map = Map { corner: MapCorner::BottomLeft, zoom: 8, open_action: Some("a1".into()), ..Map::default() };
        let text = toml::to_string(&map_data(&map)).unwrap();
        assert!(text.contains("corner = \"bottom_left\"") && text.contains("open_action = \"a1\""), "{text}");
        assert_eq!(map_of(&toml::from_str(&text).unwrap()), map);
        // Left out, a setting has its default; a zoom is the nearest power of two.
        let read: MapData = toml::from_str("zoom = 5\nsize = 9000.0\n").unwrap();
        let map = map_of(&read);
        assert_eq!((map.zoom, map.size, map.minimap, map.corner), (4, 1024.0, true, MapCorner::TopRight));
        // A prefab instance keeps only what differs.
        let changed = MapData { round: true, ..MapData::default() };
        let changes = changed.changes_from(Some(&MapData::default()));
        assert_eq!(changes.keys().collect::<Vec<_>>(), ["round"]);
    }

    #[test]
    fn a_marker_is_written_as_its_icon_label_and_colour() {
        let marker = MapMarker { icon: MapIcon::Town, label: "Oakrest".into(), ..MapMarker::default() };
        let text = toml::to_string(&marker).unwrap();
        assert!(text.contains("icon = \"town\"") && text.contains("label = \"Oakrest\""), "{text}");
        assert_eq!(toml::from_str::<MapMarker>(&text).unwrap(), marker);
        assert_eq!(toml::from_str::<MapMarker>("").unwrap(), MapMarker::default());
        assert!(MapIcon::ALL.iter().all(|icon| voxear_ui::ICONS.contains(&icon.icon())));
    }
}
