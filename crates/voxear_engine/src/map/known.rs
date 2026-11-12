//! What the game's maps show of the world: all of it, what was explored,
//! or what the game says. The knowledge itself (see
//! `voxear_terrain::blocks::map_known`) is kept in the world as a
//! [`MapKnown`], any number of them by number, one of them the one the maps
//! show: so a game can keep a map for each player and show its own.
//!
//! Ground a map does not know is drawn as blank paper, flat, whatever
//! stands there; news of a place colours its ground, by its layer's
//! colours.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use voxear_render::GpuMapColumn;
use voxear_terrain::blocks::{
    map::{MAP_TILE, MapTile},
    map_known::MapKnowledge,
};

/// What maps show of the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapShows {
    /// All of it, known or not.
    #[default]
    Everything,
    /// What was seen from where the maps follow, as it goes.
    Explored,
    /// Only what the game's scripts reveal.
    GameDecides,
}

impl MapShows {
    pub const ALL: [MapShows; 3] = [MapShows::Everything, MapShows::Explored, MapShows::GameDecides];

    pub fn name(self) -> &'static str {
        match self {
            MapShows::Everything => "Everything",
            MapShows::Explored => "Explored",
            MapShows::GameDecides => "Game Decides",
        }
    }
}

/// How a layer of news colours the ground: from one colour, where its
/// value is 0, to another, where it is 1, each sRGB with how strongly it
/// shows as its fourth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapLayerLook {
    pub low: [f32; 4],
    pub high: [f32; 4],
}

/// The knowledge the game keeps, and which of it its maps show.
#[derive(Clone, Debug, PartialEq)]
pub struct MapKnown {
    pub shows: MapShows,
    /// How far round what the maps follow ground is seen, in metres, when
    /// they show what was explored.
    pub explore: f32,
    /// Every map's knowledge, by the number scripts hold it by: 0 is the
    /// one there is to begin with.
    pub all: HashMap<u32, MapKnowledge>,
    /// The one the maps show.
    pub shown: u32,
    pub layers: BTreeMap<u8, MapLayerLook>,
    /// The colour of ground not known, sRGB.
    pub paper: [u8; 3],
    /// Counted up when a layer's look or the paper changes, so maps draw
    /// their news again.
    pub looks: u32,
}

impl Default for MapKnown {
    fn default() -> Self {
        MapKnown {
            shows: MapShows::Everything,
            explore: 64.0,
            all: HashMap::from([(0, MapKnowledge::new())]),
            shown: 0,
            layers: BTreeMap::new(),
            paper: [214, 198, 160],
            looks: 0,
        }
    }
}

impl MapKnown {
    /// The knowledge the maps show: none where they show everything.
    pub fn shown(&self) -> Option<&MapKnowledge> {
        self.all.get(&self.shown).filter(|_| self.shows != MapShows::Everything)
    }

    /// What says whether a tile's columns must be written again: the stamp
    /// of the last change to what is shown within it, with the looks'.
    /// Places are in metres from the world's corner.
    pub fn stamp_within(&self, low: [f32; 2], high: [f32; 2]) -> u64 {
        match self.shown() {
            Some(known) => {
                1 + u64::from(known.changed_within(low, high))
                    + (u64::from(self.looks) << 32)
                    + (u64::from(self.shown) << 48)
            }
            None => 0,
        }
    }

    /// A tile's columns as the maps show them: ground not known as paper,
    /// the block `paper`, and news of a place colouring its ground. `cell`
    /// is how many metres a cell of the tile is.
    pub fn columns(&self, tile: &MapTile, cell: f32, paper: u16) -> Vec<GpuMapColumn> {
        let Some(known) = self.shown() else {
            return tile.columns.iter().map(GpuMapColumn::of).collect();
        };
        let side = MAP_TILE as i32;
        let mut columns = Vec::with_capacity(tile.columns.len());
        for (at, column) in tile.columns.iter().enumerate() {
            let place = [tile.at[0] * side + at as i32 % side, tile.at[1] * side + at as i32 / side];
            let [x, z] = place.map(|at| (at as f32 + 0.5) * cell);
            if !column.is_some() {
                columns.push(GpuMapColumn::NONE);
            } else if !known.knows(x, z) {
                columns.push(GpuMapColumn::paper(paper));
            } else {
                columns.push(self.tinted(GpuMapColumn::of(column), known, x, z));
            }
        }
        columns
    }

    /// A column coloured by the news of its place: the last layer that has
    /// any there.
    fn tinted(&self, column: GpuMapColumn, known: &MapKnowledge, x: f32, z: f32) -> GpuMapColumn {
        let news = self.layers.iter().rev().find_map(|(layer, look)| Some((known.news(*layer, x, z)?.0, look)));
        let Some((value, look)) = news else { return column };
        let mixed: Vec<f32> = (0..4).map(|at| look.low[at] + (look.high[at] - look.low[at]) * value).collect();
        let color = [0, 1, 2].map(|at| (mixed[at].clamp(0.0, 1.0) * 255.0).round() as u8);
        column.tinted(color, mixed[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_terrain::blocks::map::{MapColumn, MapPiece};

    fn tile() -> MapTile {
        let column = MapColumn {
            top: 10,
            blocks: [3, 2, 1],
            depths: [1, 3],
            water: 10,
            water_block: 0,
            pieces: [MapPiece::NONE; 2],
            layered: false,
        };
        MapTile { level: 0, at: [0, 0], columns: vec![column; MAP_TILE * MAP_TILE] }
    }

    #[test]
    fn maps_show_everything_until_told_to_show_what_is_known_then_paper_where_it_is_not() {
        let mut known = MapKnown::default();
        let plain: Vec<GpuMapColumn> = tile().columns.iter().map(GpuMapColumn::of).collect();
        assert_eq!(known.columns(&tile(), 1.0, 9), plain);
        assert_eq!(known.stamp_within([0.0, 0.0], [32.0, 32.0]), 0);

        known.shows = MapShows::GameDecides;
        let none = known.columns(&tile(), 1.0, 9);
        assert!(none.iter().all(|column| *column == GpuMapColumn::paper(9)), "nothing known: all paper");
        let before = known.stamp_within([0.0, 0.0], [32.0, 32.0]);

        // Ground seen round a place shows as it is; the rest stays paper. The tile is told to be written again.
        known.all.get_mut(&0).unwrap().reveal(4.0, 4.0, 6.0);
        let some = known.columns(&tile(), 1.0, 9);
        assert_eq!(some[4 * MAP_TILE + 4], plain[0]);
        assert_eq!(some[30 * MAP_TILE + 30], GpuMapColumn::paper(9));
        assert_ne!(known.stamp_within([0.0, 0.0], [32.0, 32.0]), before);

        // News of a place colours its ground by its layer's colours, between the two by its value.
        known.layers.insert(1, MapLayerLook { low: [0.0, 1.0, 0.0, 0.0], high: [1.0, 0.0, 0.0, 0.8] });
        known.all.get_mut(&0).unwrap().paint(1, [4.0, 4.0], 0.0, 1.0, 7);
        let told = known.columns(&tile(), 1.0, 9);
        assert_eq!(told[4 * MAP_TILE + 4], plain[0].tinted([255, 0, 0], 0.8));

        // Another player's map, shown in its place, knows none of it.
        known.all.insert(5, MapKnowledge::new());
        known.shown = 5;
        assert!(known.columns(&tile(), 1.0, 9).iter().all(|column| *column == GpuMapColumn::paper(9)));
    }
}
