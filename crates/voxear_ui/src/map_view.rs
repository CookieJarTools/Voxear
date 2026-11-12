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
}

impl MapViewSettings {
    pub fn of(kind: MapViewKind) -> Self {
        match kind {
            MapViewKind::Minimap => Self { kind, zoom: 2, turns: true, round: false, tilt: 58.0 },
            MapViewKind::Map => Self { kind, zoom: 4, turns: false, round: false, tilt: 42.0 },
        }
    }

    /// The zoom nearest `zoom` a map can show: a power of two from 1 to 64.
    pub fn zoom_of(zoom: f32) -> u32 {
        let level = zoom.max(1.0).log2().round().clamp(0.0, 6.0);
        1 << level as u32
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
