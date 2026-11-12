//! A map view: where a map of the game's world shows, small in a corner of
//! the HUD as a minimap, or large as a map opened over the game. The UI
//! holds its place, its parts and its settings; its owner draws the world's
//! ground into its picture and keeps what its parts say (see
//! `voxear_engine::map`).
//!
//! A map covers what it is in: the ground (`map-picture`) with markers over
//! it (`map-overlay`), a panel naming the country it shows and where
//! (`map-region`), a strip with the time, how warm and how wet it is there
//! (`map-environment`), a name tag for what the maps follow (`map-tag`),
//! and at its foot a compass over its zoom (`map-nav`, `map-compass`,
//! `map-zoom`). A minimap is a panel (`minimap`) with a window on the
//! ground (`minimap-window`), a small compass and its zoom in the window's
//! corners, and under it a strip saying the same as a map's panels
//! (`minimap-info`). Sheets restyle any of them by those classes.
//!
//! Their sizes are a design's, for a screen 1080 pixels high, and are set
//! on each part as its own style, scaled to the screen there is (see
//! [`Ui::size_map_view`]): so a map takes up as much of a small window as of
//! a large screen.

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
    /// Whether its zoom shows: a map's buttons, a minimap's tag.
    pub zoom_control: bool,
    /// How high the screen its sizes are for is, in pixels: its parts are
    /// scaled by the screen's height over this. 0 for sizes as they are.
    pub reference: f32,
}

impl MapViewSettings {
    /// Every setting's name, as markup and scripts name it.
    pub const NAMES: [&'static str; 19] = [
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
        "zoom-control",
        "reference",
    ];

    pub fn of(kind: MapViewKind) -> Self {
        let large = kind == MapViewKind::Map;
        Self {
            kind,
            zoom: if large { 4 } else { 2 },
            turns: !large,
            round: false,
            tilt: if large { 42.0 } else { 58.0 },
            // A minimap's ground runs past its window on every side.
            fill: if large { 1.15 } else { 1.7 },
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
            zoom_control: true,
            reference: 1080.0,
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
                "turns" | "round" | "controls" | "arrow" | "arrow-at-edge" | "labels" | "title" | "readout" | "compass"
                | "zoom-control",
                _,
                Some(flag),
            ) => (0.0, flag),
            (
                "zoom" | "tilt" | "fill" | "min-zoom" | "max-zoom" | "shadows" | "edges" | "thick" | "reference",
                Some(number),
                _,
            ) => (number, false),
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
            "zoom-control" => self.zoom_control = flag,
            "reference" => self.reference = if number < 100.0 { 0.0 } else { number.min(8640.0) },
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
            "zoom-control" => flag(self.zoom_control),
            "reference" => Some(self.reference.to_string()),
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

/// A map view's own elements. A minimap has no buttons to zoom with, no
/// name tag, and of the compass's letters only north.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapViewParts {
    /// The ground, and what lies over it: markers, their labels, and the
    /// mark of what the maps follow.
    pub picture: NodeId,
    pub overlay: NodeId,
    /// What names the country it shows and says where it looks: a map's
    /// panel, a minimap's strip. In it, the name, and where along x and z.
    pub region: NodeId,
    pub title: NodeId,
    pub coords: [NodeId; 2],
    /// What says the time, how warm and how wet it is: a map's strip, and
    /// for a minimap its strip again. Each one's value, and what to hide
    /// of the time when the game tells none.
    pub environment: NodeId,
    pub time: NodeId,
    pub time_cell: NodeId,
    pub warmth: NodeId,
    pub wetness: NodeId,
    /// Where the compass is drawn, and its letters: north, east, south and
    /// west.
    pub compass: NodeId,
    pub letters: [Option<NodeId>; 4],
    /// What shows the zoom, its value in it, and a map's buttons to zoom in
    /// and out.
    pub zoom: NodeId,
    pub zoom_value: NodeId,
    pub zoom_in: Option<NodeId>,
    pub zoom_out: Option<NodeId>,
    /// A map's name tag for what the maps follow, and the name in it.
    pub tag: Option<NodeId>,
    pub tag_name: Option<NodeId>,
}

/// A part's own sizes, as its style: every number followed by `u` is in the
/// design's pixels, scaled with the screen.
type Sized = Vec<(NodeId, &'static str)>;

#[derive(Clone, Debug)]
pub(crate) struct MapView {
    settings: MapViewSettings,
    parts: MapViewParts,
    sized: Sized,
    /// How far its sizes were last scaled.
    scale: f32,
}

/// A part's style with its design's pixels scaled by `scale`: `12u` becomes
/// `12px` times it.
fn scaled(style: &str, scale: f32) -> String {
    let mut out = String::with_capacity(style.len() + 16);
    let mut number = String::new();
    for c in style.chars() {
        match c {
            '0'..='9' | '.' => number.push(c),
            'u' if !number.is_empty() => {
                let pixels: f32 = number.parse().unwrap_or(0.0);
                out.push_str(&format!("{:.2}px", pixels * scale));
                number.clear();
            }
            _ => {
                out.push_str(&number);
                number.clear();
                out.push(c);
            }
        }
    }
    out + &number
}

impl Ui {
    /// A map view of `kind`, empty until its owner draws the world into it.
    pub fn map_view(&mut self, parent: NodeId, kind: MapViewKind) -> NodeId {
        let node = self.panel(parent, Style::default());
        let mut sized = Sized::new();
        let parts = match kind {
            MapViewKind::Minimap => self.build_minimap(node, &mut sized),
            MapViewKind::Map => self.build_map(node, &mut sized),
        };
        self.map_views.insert(node.0, MapView { settings: MapViewSettings::of(kind), parts, sized, scale: 0.0 });
        self.size_map_view(node, 1.0);
        node
    }

    /// A part of a map view: a panel of class `class`, sized as `style`.
    fn map_part(&mut self, parent: NodeId, class: &str, style: &'static str, sized: &mut Sized) -> NodeId {
        let node = self.panel(parent, Style::default());
        class.split_whitespace().for_each(|class| self.add_class(node, class));
        if !style.is_empty() {
            sized.push((node, style));
        }
        node
    }

    /// Text in a map view, of class `class`, sized as `style`.
    fn map_text(&mut self, parent: NodeId, text: &str, class: &str, style: &'static str, sized: &mut Sized) -> NodeId {
        let node = self.add_label(parent, text);
        self.add_class(node, class);
        sized.push((node, style));
        node
    }

    /// A button of a map's zoom, showing `icon`.
    fn map_button(&mut self, parent: NodeId, icon: &str, sized: &mut Sized) -> NodeId {
        let button = self.button(parent, "");
        self.add_class(button, "map-zoom-button");
        sized.push((button, "height: 56u"));
        let drawn = self.icon(button, icon);
        sized.push((drawn, "width: 22u; height: 22u"));
        button
    }

    /// A number with the letter of its axis before it: the number.
    fn map_coord(&mut self, parent: NodeId, axis: &str, size: &'static str, sized: &mut Sized) -> NodeId {
        let group = self.map_part(parent, "map-group", "gap: 10u", sized);
        self.map_text(group, axis, "map-axis", size, sized);
        self.map_text(group, "", "map-number", size, sized)
    }

    /// A map's parts: see the module's own words.
    fn build_map(&mut self, node: NodeId, sized: &mut Sized) -> MapViewParts {
        self.add_class(node, "map");
        let picture = self.image(node);
        self.add_class(picture, "map-picture");
        let overlay = self.map_part(node, "map-overlay", "", sized);

        // Top left: the country's name over where the map looks.
        let region = self.map_part(
            node,
            "map-panel map-region",
            "left: 48u; top: 48u; padding: 22u 28u 20u 28u; gap: 14u; border-radius: 6u",
            sized,
        );
        let title = self.map_text(region, "", "map-title", "font-size: 44u", sized);
        self.map_part(region, "map-divider", "", sized);
        let coords = self.map_part(region, "map-group", "gap: 32u", sized);
        let x = self.map_coord(coords, "X", "font-size: 22u", sized);
        let z = self.map_coord(coords, "Z", "font-size: 22u", sized);

        // Top right: the time, how warm, how wet, each a name over its value.
        let environment =
            self.map_part(node, "map-panel map-environment", "right: 48u; top: 48u; border-radius: 6u", sized);
        let mut cell = |ui: &mut Ui, name: &str, class: &str, unit: &str| {
            let cell = ui.map_part(environment, class, "padding: 16u 28u; gap: 4u", sized);
            ui.map_text(cell, name, "map-caption", "font-size: 14u", sized);
            let value = ui.map_part(cell, "map-group", "gap: 6u", sized);
            let number = ui.map_text(value, "", "map-number", "font-size: 26u", sized);
            if !unit.is_empty() {
                ui.map_text(value, unit, "map-unit", "font-size: 26u", sized);
            }
            (cell, number)
        };
        let (time_cell, time) = cell(self, "TIME", "map-cell first", "");
        let (_, warmth) = cell(self, "TEMP", "map-cell", "°C");
        let (_, wetness) = cell(self, "HUMIDITY", "map-cell last", "%");

        // Bottom right: the compass over the zoom.
        let nav = self.map_part(node, "map-nav", "right: 48u; bottom: 48u; gap: 18u", sized);
        let compass = self.map_part(nav, "map-compass", "width: 168u; height: 168u", sized);
        let north = self.map_text(compass, "N", "map-compass-north", "font-size: 30u", sized);
        let others =
            ["E", "S", "W"].map(|letter| self.map_text(compass, letter, "map-compass-letter", "font-size: 22u", sized));
        let zoom = self.map_part(nav, "map-panel map-zoom", "width: 68u; border-radius: 6u", sized);
        let zoom_in = self.map_button(zoom, "plus", sized);
        let middle = self.map_part(zoom, "map-zoom-value", "padding: 9u 0u; gap: 2u", sized);
        self.map_text(middle, "ZOOM", "map-caption", "font-size: 12u", sized);
        let zoom_value = self.map_text(middle, "", "map-number", "font-size: 20u", sized);
        let zoom_out = self.map_button(zoom, "minus", sized);

        // Over what the maps follow: a tile and its name.
        let tag = self.map_part(node, "map-panel map-tag", "padding: 6u 14u 6u 6u; gap: 10u; border-radius: 6u", sized);
        self.map_part(tag, "map-avatar", "width: 32u; height: 32u; border-radius: 3u", sized);
        let tag_name = self.map_text(tag, "", "map-tag-name", "font-size: 20u", sized);
        MapViewParts {
            picture,
            overlay,
            region,
            title,
            coords: [x, z],
            environment,
            time,
            time_cell,
            warmth,
            wetness,
            compass,
            letters: [Some(north), Some(others[0]), Some(others[1]), Some(others[2])],
            zoom,
            zoom_value,
            zoom_in: Some(zoom_in),
            zoom_out: Some(zoom_out),
            tag: Some(tag),
            tag_name: Some(tag_name),
        }
    }

    /// A minimap's parts: see the module's own words.
    fn build_minimap(&mut self, node: NodeId, sized: &mut Sized) -> MapViewParts {
        self.add_class(node, "minimap");
        self.add_class(node, "map-panel");
        sized.push((node, "width: 380u; padding: 6u; border-radius: 8u"));
        let window = self.map_part(node, "minimap-window", "height: 254u; border-radius: 4u", sized);
        let picture = self.image(window);
        self.add_class(picture, "map-picture");
        sized.push((picture, "border-radius: 4u"));
        let overlay = self.map_part(window, "map-overlay", "", sized);
        let compass = self.map_part(window, "minimap-compass", "right: 8u; top: 8u; width: 64u; height: 64u", sized);
        let north = self.map_text(compass, "N", "map-compass-north", "font-size: 20u", sized);
        let zoom = self.map_part(
            window,
            "map-panel minimap-zoom",
            "left: 8u; bottom: 8u; padding: 6u 10u; gap: 6u; border-radius: 4u",
            sized,
        );
        self.map_text(zoom, "ZOOM", "map-caption", "font-size: 12u", sized);
        let zoom_value = self.map_text(zoom, "", "map-number", "font-size: 15u", sized);

        // Under the window: the country and the time, then where and how warm and wet.
        let info = self.map_part(node, "minimap-info", "padding: 12u 10u 8u 10u; gap: 8u", sized);
        let first = self.map_part(info, "minimap-row", "gap: 4u 14u", sized);
        let title = self.map_text(first, "", "map-title", "font-size: 22u", sized);
        let time = self.map_text(first, "", "map-number", "font-size: 20u", sized);
        let second = self.map_part(info, "minimap-row", "gap: 4u 14u", sized);
        let coords = self.map_part(second, "map-group", "gap: 14u", sized);
        let x = self.map_coord(coords, "X", "font-size: 15u", sized);
        let z = self.map_coord(coords, "Z", "font-size: 15u", sized);
        let air = self.map_part(second, "map-group", "gap: 14u", sized);
        let warm = self.map_part(air, "map-group", "gap: 4u", sized);
        let warmth = self.map_text(warm, "", "map-number", "font-size: 15u", sized);
        self.map_text(warm, "°C", "map-unit", "font-size: 15u", sized);
        let wet = self.map_part(air, "map-group", "gap: 4u", sized);
        let drop = self.icon(wet, "drop");
        self.add_class(drop, "map-unit");
        sized.push((drop, "width: 14u; height: 14u"));
        let wetness = self.map_text(wet, "", "map-number", "font-size: 15u", sized);
        self.map_text(wet, "%", "map-unit", "font-size: 15u", sized);
        MapViewParts {
            picture,
            overlay,
            region: info,
            title,
            coords: [x, z],
            environment: info,
            time,
            time_cell: time,
            warmth,
            wetness,
            compass,
            letters: [Some(north), None, None, None],
            zoom,
            zoom_value,
            zoom_in: None,
            zoom_out: None,
            tag: None,
            tag_name: None,
        }
    }

    /// Sizes a map view's parts for a screen `scale` times as high as its
    /// design's: each part's own style, its pixels scaled. Whatever else was
    /// set on a part as its own style is set again by whoever set it.
    /// Whether anything changed.
    pub fn size_map_view(&mut self, node: NodeId, scale: f32) -> bool {
        let scale = scale.clamp(0.25, 8.0);
        let Some(view) = self.map_views.get_mut(&node.0) else { return false };
        if (view.scale - scale).abs() < 1e-3 {
            return false;
        }
        view.scale = scale;
        for (part, style) in view.sized.clone() {
            self.set_inline_style(part, &scaled(style, scale));
        }
        true
    }

    /// How far a map view's sizes are scaled now.
    pub fn map_view_scale(&self, node: NodeId) -> f32 {
        self.map_views.get(&node.0).map_or(1.0, |view| view.scale)
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

#[cfg(test)]
mod tests {
    use super::scaled;

    #[test]
    fn a_parts_design_pixels_are_scaled_and_the_rest_of_its_style_is_as_written() {
        assert_eq!(scaled("left: 48u; padding: 6u 14u", 0.5), "left: 24.00px; padding: 3.00px 7.00px");
        assert_eq!(scaled("height: 1px; flex: 1; gap: 2.5u", 2.0), "height: 1px; flex: 1; gap: 5.00px");
        assert_eq!(scaled("", 2.0), "");
    }
}
