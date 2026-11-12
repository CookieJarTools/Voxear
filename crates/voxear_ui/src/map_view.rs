//! A map view: where a map of the game's world shows, small in a corner of
//! the HUD as a minimap, or large as a map opened over the game. The UI
//! holds only its place and its settings; its owner draws the world's
//! ground into its picture and keeps its title, its readout, its compass
//! and its markers (see `voxear_engine::map`).
//!
//! A minimap is a `minimap` element and a map a `map` one. Each holds a
//! `map-picture`, the ground itself, and a `map-overlay` over it for
//! markers and their labels. A map also has a `map-header` with its
//! `map-readout` and `map-title`, and a `map-compass`. Sheets restyle any
//! of them by those classes.

use crate::{NodeId, Style, Ui};

/// What a map view is: a minimap on the HUD, or a map opened large.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapViewKind {
    #[default]
    Minimap,
    Map,
}

/// How a map view shows the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewSettings {
    pub kind: MapViewKind,
    /// How many blocks a cell of the map is, a power of two: 4 is "1:4".
    pub zoom: u32,
    /// Whether it turns with the view, what is ahead at its top, or keeps
    /// north at its top.
    pub turns: bool,
    /// Whether the ground is cut round or square.
    pub round: bool,
    /// How far its camera is tilted over the ground, in degrees: 90 looks
    /// straight down.
    pub tilt: f32,
    /// How wide the ground is drawn, as a share of the view's height: at 1
    /// it is as wide as the view is high.
    pub fill: f32,
    /// Whether the pointer turns, moves and zooms it.
    pub controls: bool,
    /// The fewest and the most blocks a cell may be, as it is zoomed.
    pub zoom_range: [u32; 2],
    /// How dark its shadows are, and the lines along its blocks' edges,
    /// each from 0 for none to 1.
    pub shadows: f32,
    pub edges: f32,
    /// How thick the ground is under its lowest place, in cells.
    pub thick: f32,
    /// Whether what the maps follow is marked with an arrow, and whether
    /// that stays at the ground's edge once it is past it.
    pub arrow: bool,
    pub arrow_at_edge: bool,
    /// Whether markers' labels show, and a map's title, readout and
    /// compass.
    pub labels: bool,
    pub title: bool,
    pub readout: bool,
    pub compass: bool,
}

impl MapViewSettings {
    /// Every setting's name, as markup and scripts name it.
    pub const NAMES: [&'static str; 17] = [
        "zoom",
        "turns",
        "round",
        "tilt",
        "fill",
        "controls",
        "min-zoom",
        "max-zoom",
        "shadows",
        "edges",
        "thick",
        "arrow",
        "arrow-at-edge",
        "labels",
        "title",
        "readout",
        "compass",
    ];

    pub fn of(kind: MapViewKind) -> Self {
        let large = kind == MapViewKind::Map;
        Self {
            kind,
            zoom: if large { 4 } else { 2 },
            turns: !large,
            round: false,
            tilt: if large { 42.0 } else { 58.0 },
            fill: if large { 1.15 } else { 0.98 },
            controls: large,
            zoom_range: [1, 64],
            shadows: 0.4,
            edges: 0.6,
            thick: if large { 4.0 } else { 2.0 },
            arrow: true,
            arrow_at_edge: true,
            labels: large,
            title: true,
            readout: true,
            compass: true,
        }
    }

    /// The zoom nearest `zoom` a map can show: a power of two from 1 to 64.
    pub fn zoom_of(zoom: f32) -> u32 {
        let level = zoom.max(1.0).log2().round().clamp(0.0, 6.0);
        1 << level as u32
    }

    /// Sets the setting named `name` (one of [`Self::NAMES`]) from text: a
    /// number, or `true` or `false` (or 1 or 0). False for a name there is
    /// none of, or text that is not what it takes; within what a map can
    /// show otherwise.
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        let value = value.trim();
        let number: Option<f32> = value.parse().ok();
        let flag = match value {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        };
        let (number, flag) = match (name, number, flag) {
            (
                "turns" | "round" | "controls" | "arrow" | "arrow-at-edge" | "labels" | "title" | "readout" | "compass",
                _,
                Some(flag),
            ) => (0.0, flag),
            ("zoom" | "tilt" | "fill" | "min-zoom" | "max-zoom" | "shadows" | "edges" | "thick", Some(number), _) => {
                (number, false)
            }
            _ => return false,
        };
        match name {
            "zoom" => self.zoom = Self::zoom_of(number),
            "turns" => self.turns = flag,
            "round" => self.round = flag,
            "tilt" => self.tilt = number.clamp(20.0, 90.0),
            "fill" => self.fill = number.clamp(0.1, 3.0),
            "controls" => self.controls = flag,
            "min-zoom" => self.zoom_range[0] = Self::zoom_of(number).min(self.zoom_range[1]),
            "max-zoom" => self.zoom_range[1] = Self::zoom_of(number).max(self.zoom_range[0]),
            "shadows" => self.shadows = number.clamp(0.0, 1.0),
            "edges" => self.edges = number.clamp(0.0, 1.0),
            "thick" => self.thick = number.clamp(0.0, 64.0),
            "arrow" => self.arrow = flag,
            "arrow-at-edge" => self.arrow_at_edge = flag,
            "labels" => self.labels = flag,
            "title" => self.title = flag,
            "readout" => self.readout = flag,
            "compass" => self.compass = flag,
            _ => return false,
        }
        true
    }

    /// The setting named `name`, as text [`Self::set`] takes back.
    pub fn get(&self, name: &str) -> Option<String> {
        let flag = |on: bool| Some(if on { "true" } else { "false" }.to_string());
        match name {
            "zoom" => Some(self.zoom.to_string()),
            "turns" => flag(self.turns),
            "round" => flag(self.round),
            "tilt" => Some(self.tilt.to_string()),
            "fill" => Some(self.fill.to_string()),
            "controls" => flag(self.controls),
            "min-zoom" => Some(self.zoom_range[0].to_string()),
            "max-zoom" => Some(self.zoom_range[1].to_string()),
            "shadows" => Some(self.shadows.to_string()),
            "edges" => Some(self.edges.to_string()),
            "thick" => Some(self.thick.to_string()),
            "arrow" => flag(self.arrow),
            "arrow-at-edge" => flag(self.arrow_at_edge),
            "labels" => flag(self.labels),
            "title" => flag(self.title),
            "readout" => flag(self.readout),
            "compass" => flag(self.compass),
            _ => None,
        }
    }

    /// Every setting, as text [`Self::set_all`] takes back.
    pub fn to_text(&self) -> String {
        let pairs = Self::NAMES.iter().filter_map(|name| Some(format!("{name}={}", self.get(name)?)));
        pairs.collect::<Vec<_>>().join(" ")
    }

    /// Sets each setting of `text`, as `zoom=4 turns=false fill=1.2`; the
    /// names it could not set.
    pub fn set_all<'a>(&mut self, text: &'a str) -> Vec<&'a str> {
        let pairs = text.split_whitespace().map(|pair| pair.split_once('=').unwrap_or((pair, "")));
        pairs.filter(|(name, value)| !self.set(name, value)).map(|(name, _)| name).collect()
    }
}

/// A map view's own elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapViewParts {
    pub picture: NodeId,
    pub overlay: NodeId,
    /// A map's alone.
    pub title: Option<NodeId>,
    pub readout: Option<NodeId>,
    pub compass: Option<NodeId>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MapView {
    settings: MapViewSettings,
    parts: MapViewParts,
}

impl Ui {
    /// A map view of `kind`, empty until its owner draws the world into it.
    pub fn map_view(&mut self, parent: NodeId, kind: MapViewKind) -> NodeId {
        let node = self.panel(parent, Style::default());
        self.add_class(node, if kind == MapViewKind::Minimap { "minimap" } else { "map" });
        let picture = self.image(node);
        self.add_class(picture, "map-picture");
        let overlay = self.panel(node, Style::default());
        self.add_class(overlay, "map-overlay");
        let mut parts = MapViewParts { picture, overlay, title: None, readout: None, compass: None };
        if kind == MapViewKind::Map {
            let header = self.panel(node, Style::default());
            self.add_class(header, "map-header");
            let readout = self.add_label(header, "");
            self.add_class(readout, "map-readout");
            let title = self.add_label(header, "");
            self.add_class(title, "map-title");
            let compass = self.panel(node, Style::default());
            self.add_class(compass, "map-compass");
            (parts.title, parts.readout, parts.compass) = (Some(title), Some(readout), Some(compass));
        }
        self.map_views.insert(node.0, MapView { settings: MapViewSettings::of(kind), parts });
        node
    }

    /// Every map view, in no order.
    pub fn map_views(&self) -> Vec<NodeId> {
        self.map_views.keys().map(|&node| NodeId(node)).collect()
    }

    pub fn map_view_settings(&self, node: NodeId) -> Option<MapViewSettings> {
        self.map_views.get(&node.0).map(|view| view.settings)
    }

    pub fn set_map_view_settings(&mut self, node: NodeId, settings: MapViewSettings) {
        if let Some(view) = self.map_views.get_mut(&node.0) {
            view.settings = MapViewSettings { kind: view.settings.kind, ..settings };
        }
    }

    pub fn map_view_parts(&self, node: NodeId) -> Option<MapViewParts> {
        self.map_views.get(&node.0).map(|view| view.parts)
    }

    pub(crate) fn forget_map_view(&mut self, node: taffy::NodeId) {
        self.map_views.remove(&node);
    }
}
