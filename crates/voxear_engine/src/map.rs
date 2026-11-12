//! Maps of the game's world of blocks: the ground round where the game is
//! seen from, drawn as a small model of itself into every map view on the
//! screen (see `voxear_ui::map_view`), a minimap on the HUD or a map opened
//! over the game.
//!
//! A map is made of tiles (see `voxear_terrain::blocks::map`), each made
//! on the terrain's background threads from the same ground the world is
//! made from, with what was built and dug, and kept while a view shows it.
//! A change to the world makes again only the tiles it lies in. Each view
//! keeps a window of tiles in the renderer (see `voxear_render::map`),
//! moved a tile at a time as what it shows moves, and is drawn only when
//! what it shows or how it is seen changed.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc},
};

use voxear_core::math::{Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{Camera, GpuMapColumn, MAP_LEAST_TILT, MapCamera, MapDraw, MapId, MapLook, Renderer};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockFaces, BlockMap, BlockWorld,
    edits::BlockEdits,
    map::{MAP_TILE, MapGround, MapTile, map_tile},
    stream::all_changes,
};
use voxear_ui::{ImageRef, MapViewKind, MapViewSettings, NodeId, Ui};

use crate::{Content, blocks::BlockWorldAsset};

mod hud;
mod known;
mod parts;
mod scripts;

pub use hud::PlacedMarker;
pub use known::{MapKnown, MapLayerLook, MapShows};
pub use parts::{Map, MapCorner, MapData, MapIcon, MapMarker, MarkerShows, map_data, map_of};
pub use scripts::MapScript;
pub(crate) use scripts::register;

/// The cells a minimap's window has along a side, and a map's.
const MINIMAP_SIDE: u32 = 128;
const MAP_SIDE: u32 = 256;

/// Tiles being made at once, at most: the rest wait, nearest first.
const MOST_MAKING: usize = 12;

/// Tiles kept that no view has shown for this many frames are let go.
const KEPT_FRAMES: u64 = 600;

/// A map's field of view, bottom to top, in radians: narrow, so the model
/// is seen nearly without perspective, as a map is.
const FOV: f32 = 0.4;

/// A tile of a map, by the world it is of, its level and its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    world: Entity,
    level: u32,
    at: [i32; 2],
}

/// A tile kept: as made, the lowest ground and highest block in it, the
/// frame a view last showed it, and whether the world changed under it
/// since, so it is being made again.
struct KeptTile {
    tile: Arc<MapTile>,
    heights: Option<[i32; 2]>,
    /// Counted up each time the tile is made, so a view knows the one it
    /// wrote from the one kept now.
    made: u64,
    shown: u64,
    stale: bool,
}

/// A tile made in the background: which, the number of the ground it was
/// made from, and the tile.
type Made = (TileKey, u64, MapTile);

/// What a world's tiles are made from now, and what that was made from: its
/// blocks, the changes kept for chunks not near, and its faces.
struct KeptGround {
    ground: MapGround,
    blocks: Arc<BlockMap>,
    changes: Arc<BlockEdits>,
    faces: Arc<[BlockFaces]>,
    block_size: f32,
    corner: Vec3,
    /// Counted up each time the ground changes, so a tile made from the
    /// ground as it was is known for it.
    number: u64,
}

/// A map view's own state: its window in the renderer and how it is seen.
struct View {
    map: Option<MapId>,
    side: u32,
    world: Option<Entity>,
    level: u32,
    /// The window's lowest cell, a multiple of a tile's side.
    corner: [i32; 2],
    /// The tiles written into the window, each by the time it was made (0
    /// for one written empty, waiting to be made) and by the stamp of what
    /// the maps knew of it then.
    written: HashMap<[i32; 2], (u64, u64)>,
    /// The blocks' colours its palette was made from, and the paper's.
    faces: Option<Arc<[BlockFaces]>>,
    paper: [u8; 3],
    /// A map's own turn and tilt, in radians, how far it is zoomed in, and
    /// how far it was moved from what it follows, in cells.
    turn: f32,
    tilt: f32,
    zoom: f32,
    moved: Vec2,
    /// Where the pointer was when last looked at, while it is held on the
    /// map.
    held: Option<(f32, f32)>,
    /// How many frames it has been drawn the same way: left alone, it is
    /// drawn once more with smoother edges.
    still: u32,
    /// How far its slab reached from its middle when last shown, in cells.
    reach: f32,
    drawn: Option<MapDraw>,
    /// The frame it was last seen in the UI.
    seen: u64,
}

impl View {
    fn new(settings: &MapViewSettings) -> Self {
        let side = if settings.kind == MapViewKind::Minimap { MINIMAP_SIDE } else { MAP_SIDE };
        View {
            map: None,
            side,
            world: None,
            level: 0,
            corner: [0, 0],
            written: HashMap::new(),
            faces: None,
            paper: [0; 3],
            turn: 0.0,
            tilt: settings.tilt.to_radians(),
            zoom: 1.0,
            moved: Vec2::ZERO,
            held: None,
            still: 0,
            reach: side as f32 * 0.3,
            drawn: None,
            seen: 0,
        }
    }
}

/// Where a map is seen from: the place it follows, and the way that looks,
/// as radians from north.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewer {
    pub position: Vec3,
    pub heading: f32,
}

/// What a map view showed when it was last drawn, for markers and scripts:
/// its camera and the size it was drawn at, the world it is of, and what
/// its cells are counted from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShownMap {
    pub world: Entity,
    pub camera: MapCamera,
    pub size: Vec2,
    /// The window's lowest cell, the height its camera's is counted from,
    /// and how many metres a cell is.
    pub corner: [i32; 2],
    pub base: i32,
    pub cell: f32,
    /// The world of blocks' corner, in the scene.
    pub origin: Vec3,
    /// How far the slab reaches from what the camera looks at, in cells,
    /// and whether it is cut round.
    pub reach: f32,
    pub round: bool,
}

impl ShownMap {
    /// Where a place in the scene is on the map, in cells from its window's
    /// corner and base.
    pub fn cells(&self, at: Vec3) -> Vec3 {
        let from = (at - self.origin) / self.cell;
        Vec3::new(from.x - self.corner[0] as f32, from.y - self.base as f32, from.z - self.corner[1] as f32)
    }

    /// Where a place in the scene shows in the view, in pixels from its top
    /// left, and whether that is inside the slab the map shows.
    pub fn point(&self, at: Vec3) -> Option<(Vec2, bool)> {
        let cells = self.cells(at);
        Some((self.camera.project(cells, self.size)?, self.past(cells) <= 1.0))
    }

    /// How far past the slab's edge a place is, in cells from the window's
    /// corner: 1 at the edge, less inside it.
    fn past(&self, cells: Vec3) -> f32 {
        let off = Vec2::new(cells.x - self.camera.middle.x, cells.z - self.camera.middle.z);
        let far = if self.round { off.length() } else { off.x.abs().max(off.y.abs()) };
        far / self.reach.max(1.0)
    }

    /// Where the slab's edge is towards a place in the scene past it, in
    /// the view: where a marker for something off the map stands.
    pub fn edge_towards(&self, at: Vec3) -> Option<Vec2> {
        let cells = self.cells(at);
        let middle = self.camera.middle;
        let towards = (cells - middle) / self.past(cells).max(1e-3);
        self.camera.project(Vec3::new(middle.x + towards.x, middle.y, middle.z + towards.z), self.size)
    }
}

/// The game's maps: the tiles kept, and each map view's window.
pub struct Maps {
    grounds: HashMap<Entity, KeptGround>,
    tiles: HashMap<TileKey, KeptTile>,
    asked: HashSet<TileKey>,
    made: (mpsc::Sender<Made>, mpsc::Receiver<Made>),
    made_count: u64,
    views: HashMap<NodeId, View>,
    shown: HashMap<NodeId, ShownMap>,
    frame: u64,
    /// Each Map's minimap and map on the screen.
    huds: HashMap<Entity, hud::Hud>,
    /// Each view's labels, with what each says, and what was last written
    /// in each title and readout.
    labels: HashMap<NodeId, Vec<(NodeId, String)>>,
    written: HashMap<NodeId, String>,
}

impl Default for Maps {
    fn default() -> Self {
        Maps {
            grounds: HashMap::new(),
            tiles: HashMap::new(),
            asked: HashSet::new(),
            made: mpsc::channel(),
            made_count: 0,
            views: HashMap::new(),
            shown: HashMap::new(),
            frame: 0,
            huds: HashMap::new(),
            labels: HashMap::new(),
            written: HashMap::new(),
        }
    }
}

/// The world of blocks a map shows: the first in the scene, with what it
/// was made from.
fn block_world(world: &World) -> Option<(Entity, BlockWorld, BlockWorldAsset, Vec3)> {
    world
        .query_ref::<(Entity, &BlockWorld, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .min_by_key(|(entity, ..)| entity.index())
        .map(|(entity, blocks, asset, local, placed)| {
            (entity, blocks.clone(), asset.clone(), voxear_terrain::corner(local, placed))
        })
}

/// The corner of the world of blocks a map shows, in the scene.
fn corner(world: &World) -> Option<Vec3> {
    let found = world.query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
    let first = found.into_iter().min_by_key(|(entity, ..)| entity.index());
    first.map(|(_, _, local, placed)| voxear_terrain::corner(local, placed))
}

/// Where the game is seen from and the way it looks: the first camera
/// switched on, of the game's own.
fn camera(world: &World) -> Option<MapViewer> {
    world
        .query_ref::<(Entity, &Camera, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| voxear_scene::is_on(world, *entity, voxear_scene::Switch::Camera))
        .min_by_key(|(entity, ..)| entity.index())
        .map(|(_, _, local, placed)| viewer_of(local, placed))
}

fn viewer_of(local: &Transform, placed: Option<&WorldTransform>) -> MapViewer {
    let matrix = placed.map_or_else(|| local.matrix(), |placed| placed.0);
    let forward = matrix.z_axis.truncate();
    MapViewer { position: matrix.w_axis.truncate(), heading: forward.x.atan2(forward.z) }
}

impl Maps {
    /// Where maps are seen from: the Obj scripts gave them to follow (see
    /// [`MapScript`]), or else the game's camera.
    pub fn viewer(&self, world: &World) -> Option<MapViewer> {
        let follow = world.resource::<MapScript>().and_then(|script| script.follow);
        let followed = follow.filter(|entity| world.is_alive(*entity)).and_then(|entity| {
            let local = world.get::<Transform>(entity)?;
            Some(viewer_of(local, world.get::<WorldTransform>(entity)))
        });
        followed.or_else(|| camera(world))
    }

    /// What a map view showed when it was last drawn.
    pub fn shown(&self, view: NodeId) -> Option<&ShownMap> {
        self.shown.get(&view)
    }

    /// The map's column over a place in the scene, at `level`, of the world
    /// of blocks `world`: none until a view has shown its tile.
    pub fn column(&self, world: Entity, level: u32, at: Vec3) -> Option<voxear_terrain::blocks::map::MapColumn> {
        let ground = self.grounds.get(&world)?;
        let cell = ((at - ground.corner) / (ground.block_size.max(0.01) * (1 << level) as f32)).floor().as_ivec3();
        let tile = MAP_TILE as i32;
        let key = TileKey { world, level, at: [cell.x.div_euclid(tile), cell.z.div_euclid(tile)] };
        let kept = self.tiles.get(&key)?;
        Some(*kept.tile.column([cell.x.rem_euclid(tile) as u32, cell.z.rem_euclid(tile) as u32]))
    }

    /// Whether every tile a view shows has been made: nothing is waiting.
    pub fn settled(&self) -> bool {
        self.asked.is_empty() && !self.tiles.values().any(|kept| kept.stale)
    }

    /// Keeps every map view of `ui` showing the world round where the game
    /// is seen from: makes the tiles each needs, moves its window, hears
    /// the pointer on a map, and has `renderer` draw it. `scale` is how
    /// many pixels one of the UI's is.
    pub fn update(
        &mut self,
        world: &World,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
        (ui, scale): (&mut Ui, f32),
    ) {
        voxear_core::profile_scope!("Maps");
        self.frame += 1;
        let frame = self.frame;
        let nodes = ui.map_views();
        // Views taken out of the UI give their windows back.
        let gone: Vec<NodeId> = self.views.keys().filter(|node| !nodes.contains(node)).copied().collect();
        for node in gone {
            let view = self.views.remove(&node).expect("listed");
            self.shown.remove(&node);
            self.labels.remove(&node);
            if let (Some(map), Some(renderer)) = (view.map, renderer.as_deref_mut()) {
                renderer.free_map(map);
            }
        }
        if nodes.is_empty() {
            self.forget_unshown(frame);
            return;
        }
        let found = block_world(world);
        if let Some((entity, blocks, asset, corner)) = &found {
            self.keep_ground(content, *entity, blocks, asset, *corner);
        }
        self.grounds.retain(|entity, _| found.as_ref().is_some_and(|(found, ..)| found == entity));
        self.take_made();
        let viewer = self.viewer(world);
        let unset = MapKnown::default();
        let known = world.resource::<MapKnown>().unwrap_or(&unset);
        for node in nodes {
            let Some(settings) = ui.map_view_settings(node) else { continue };
            let view = self.views.entry(node).or_insert_with(|| View::new(&settings));
            view.seen = frame;
            let (Some((entity, ..)), Some(viewer)) = (&found, viewer) else {
                self.shown.remove(&node);
                continue;
            };
            if !ui.is_visible(node) {
                continue;
            }
            self.show((*entity, known), viewer, (node, &settings), renderer.as_deref_mut(), (ui, scale));
            self.overlay(world, ui, (node, &settings), viewer);
        }
        self.written.retain(|node, _| ui.is_visible(*node));
        self.forget_unshown(frame);
    }

    /// Keeps what `entity`'s tiles are made from, as its world is now, and
    /// marks the tiles a change to it lies in to be made again.
    fn keep_ground(
        &mut self,
        content: &mut Content,
        entity: Entity,
        blocks: &BlockWorld,
        asset: &BlockWorldAsset,
        corner: Vec3,
    ) {
        if let Some(kept) = self.grounds.get_mut(&entity) {
            (kept.corner, kept.block_size) = (corner, blocks.block_size);
            let same = Arc::ptr_eq(&kept.blocks, &blocks.blocks)
                && Arc::ptr_eq(&kept.changes, &asset.pending)
                && Arc::ptr_eq(&kept.faces, &blocks.faces);
            if same {
                return;
            }
        }
        let endless = asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
        let making = if endless { crate::blocks::making(content, asset) } else { None };
        let (ground, changes) = match making {
            Some(making) => {
                let changes = Arc::new(all_changes(&asset.base, &blocks.blocks, &asset.pending));
                (MapGround::made(making, changes.clone()), Some(changes))
            }
            None => (MapGround::held(blocks.blocks.clone(), blocks.faces.clone()), None),
        };
        // The chunk columns that changed: all of them where the blocks are drawn another way, or the ground is
        // made another way.
        let before = self.grounds.get(&entity);
        let changed: Option<HashSet<[i32; 2]>> = match (before, &ground, &changes) {
            (Some(kept), _, _) if kept.faces[..] != blocks.faces[..] => None,
            (
                Some(KeptGround { ground: MapGround::Made { making: old, changes: was, .. }, .. }),
                MapGround::Made { making, .. },
                Some(now),
            ) if old.file == making.file && Arc::ptr_eq(&old.world, &making.world) => {
                let differs = |key: &[i32; 3]| was.chunks.get(key) != now.chunks.get(key);
                let keys = was.chunks.keys().chain(now.chunks.keys()).filter(|key| differs(key));
                Some(keys.map(|key| [key[0], key[2]]).collect())
            }
            (
                Some(KeptGround { ground: MapGround::Held { blocks: was, .. }, .. }),
                MapGround::Held { blocks: now, .. },
                _,
            ) => Some(BlockEdits::between(was, now).chunks.keys().map(|key| [key[0], key[2]]).collect()),
            _ => None,
        };
        for (key, kept) in self.tiles.iter_mut().filter(|(key, _)| key.world == entity) {
            let lies_in = |column: &[i32; 2]| [column[0] >> key.level, column[1] >> key.level] == key.at;
            kept.stale |= changed.as_ref().is_none_or(|columns| columns.iter().any(lies_in));
        }
        // What is being made was asked of the old ground: made again when it comes, by its number.
        let number = before.map_or(0, |kept| kept.number) + 1;
        let kept = KeptGround {
            number,
            ground,
            blocks: blocks.blocks.clone(),
            changes: asset.pending.clone(),
            faces: blocks.faces.clone(),
            block_size: blocks.block_size,
            corner,
        };
        self.grounds.insert(entity, kept);
    }

    /// Takes in the tiles made in the background since last frame.
    fn take_made(&mut self) {
        while let Ok((key, number, tile)) = self.made.1.try_recv() {
            self.asked.remove(&key);
            self.made_count += 1;
            let heights = tile.heights();
            let shown = self.tiles.get(&key).map_or(self.frame, |kept| kept.shown);
            // One the world changed under while it was made is shown, and made again.
            let stale = self.grounds.get(&key.world).is_some_and(|ground| ground.number != number);
            let kept = KeptTile { tile: Arc::new(tile), heights, made: self.made_count, shown, stale };
            self.tiles.insert(key, kept);
        }
    }

    /// Lets go the tiles no view showed for a while, and the views no
    /// longer in the UI.
    fn forget_unshown(&mut self, frame: u64) {
        self.tiles.retain(|_, kept| kept.shown + KEPT_FRAMES > frame);
    }

    /// Asks for tile `key` to be made, if it is not being made already and
    /// fewer than the most are.
    fn ask(&mut self, key: TileKey) {
        if self.asked.len() >= MOST_MAKING || self.asked.contains(&key) {
            return;
        }
        let Some(kept) = self.grounds.get(&key.world) else { return };
        let (ground, number, sender) = (kept.ground.clone(), kept.number, self.made.0.clone());
        self.asked.insert(key);
        voxear_terrain::workers::spawn(move || {
            let _ = sender.send((key, number, map_tile(&ground, key.level, key.at)));
        });
    }

    /// Shows the world round `viewer` in one view.
    fn show(
        &mut self,
        (entity, known): (Entity, &MapKnown),
        viewer: MapViewer,
        (node, settings): (NodeId, &MapViewSettings),
        mut renderer: Option<&mut Renderer>,
        (ui, scale): (&mut Ui, f32),
    ) {
        let Some(ground) = self.grounds.get(&entity) else { return };
        let (faces, origin, block_size) = (ground.faces.clone(), ground.corner, ground.block_size.max(0.01));
        let Some(parts) = ui.map_view_parts(node) else { return };
        let rect = ui.rect(node);
        let size = [(rect[2] * scale).round().max(1.0) as u32, (rect[3] * scale).round().max(1.0) as u32];
        if rect[2] < 2.0 || rect[3] < 2.0 {
            return;
        }
        let large = settings.kind == MapViewKind::Map;
        let frame = self.frame;
        let view = self.views.get_mut(&node).expect("kept by update");
        if large {
            hear_pointer(view, ui, node);
        } else {
            (view.tilt, view.moved, view.zoom) = (settings.tilt.to_radians(), Vec2::ZERO, 1.0);
            view.turn = if settings.turns { viewer.heading } else { 0.0 };
        }
        // Zoomed out past twice or in past half, a map goes to the next level, its cells twice the size.
        let mut level = settings.zoom.max(1).ilog2();
        if large {
            let steps = (-view.zoom.log2()).round() as i32;
            level = (level as i32 + steps).clamp(0, 8) as u32;
        }
        let within = if large { view.zoom * (1 << level) as f32 / settings.zoom.max(1) as f32 } else { 1.0 };
        let cell = block_size * (1 << level) as f32;
        // What it looks at: what it follows, moved as the player moved the map, in this level's cells.
        let followed = (viewer.position - origin) / cell;
        if view.world != Some(entity) || view.level != level {
            // Another world, or another size of cell: everything in the window is something else now.
            let per = (1 << view.level) as f32 / (1 << level) as f32;
            view.moved *= if view.world == Some(entity) { per } else { 0.0 };
            (view.world, view.level) = (Some(entity), level);
            view.written.clear();
        }
        let middle = Vec2::new(followed.x, followed.z) + view.moved;
        // The window moves a tile at a time, to keep what is looked at near its middle.
        let (side, tile) = (view.side as i32, MAP_TILE as i32);
        let wanted = [middle.x, middle.y].map(|at| ((at - side as f32 * 0.5) / tile as f32).round() as i32 * tile);
        if view.corner != wanted {
            let gone = |at: &[i32; 2]| (0..2).any(|axis| at[axis] < wanted[axis] || at[axis] >= wanted[axis] + side);
            view.written.retain(|at, _| !gone(at));
            view.corner = wanted;
        }
        let corner = view.corner;
        // A map's slab shows more ground the further it is zoomed out, and stays the size it is on the screen:
        // so as its cells change size at the next level, the same ground is shown as large as before, in
        // cells half or twice the size.
        let most = (side as f32 * 0.5 - tile as f32 * 0.5 - 2.0).max(8.0);
        let reach = if large { (most / std::f32::consts::SQRT_2 / within.max(0.05)).min(most) } else { most };
        view.reach = reach;

        // The renderer's window, made with the view and given the blocks' colours as they are now.
        if let Some(renderer) = renderer.as_deref_mut() {
            if view.map.is_none() {
                view.map = renderer.add_map(view.side).ok();
                view.written.clear();
                view.faces = None;
            }
            let colours_changed =
                view.faces.as_ref().is_none_or(|kept| kept[..] != faces[..]) || view.paper != known.paper;
            if let (Some(map), true) = (view.map, colours_changed) {
                // The paper's colour after every block's, as the block ground not known is made of.
                let mut colors: Vec<[u8; 3]> = faces.iter().map(|faces| faces.color).collect();
                colors.push(known.paper);
                if renderer.set_map_palette(map, &colors).is_ok() {
                    (view.faces, view.paper) = (Some(faces.clone()), known.paper);
                }
            }
        }
        let paper = faces.len().min(usize::from(u16::MAX)) as u16;
        // Each tile of the window: written as kept, or empty until it is made; asked for nearest the middle first.
        let across = side / tile;
        let mut places: Vec<[i32; 2]> =
            (0..across * across).map(|at| [corner[0] / tile + at % across, corner[1] / tile + at / across]).collect();
        let off = |at: &[i32; 2]| {
            let from = [0, 1].map(|axis| (at[axis] as f32 + 0.5) * tile as f32 - middle[axis]);
            (from[0] * from[0] + from[1] * from[1]) as i32
        };
        places.sort_by_key(off);
        let map = view.map;
        let mut heights: Option<[i32; 2]> = None;
        let mut wanted_tiles = Vec::new();
        for at in places {
            let key = TileKey { world: entity, level, at };
            let cells = at.map(|tile_at| tile_at * tile);
            let view = self.views.get_mut(&node).expect("kept by update");
            match self.tiles.get_mut(&key) {
                Some(kept) => {
                    kept.shown = frame;
                    // Written again when it was made again, or when what the maps know of it changed.
                    let low = cells.map(|at| at as f32 * cell);
                    let stamp = known.stamp_within(low, low.map(|at| at + tile as f32 * cell));
                    if view.written.get(&cells) != Some(&(kept.made, stamp)) {
                        if let (Some(map), Some(renderer)) = (map, renderer.as_deref_mut()) {
                            renderer.write_map_tile(map, cells, &known.columns(&kept.tile, cell, paper));
                        }
                        view.written.insert(cells, (kept.made, stamp));
                    }
                    // Only the tiles the slab reaches into count for how high it stands.
                    let reaches = (0..2).all(|axis| {
                        let low = cells[axis] as f32 - corner[axis] as f32;
                        let at = middle[axis] - corner[axis] as f32;
                        low < at + reach && low + tile as f32 > at - reach
                    });
                    if let (Some([low, high]), true) = (kept.heights, reaches) {
                        let [l, h] = heights.unwrap_or([i32::MAX, i32::MIN]);
                        heights = Some([l.min(low), h.max(high)]);
                    }
                    if kept.stale {
                        wanted_tiles.push(key);
                    }
                }
                None => {
                    if !view.written.contains_key(&cells) {
                        if let (Some(map), Some(renderer)) = (map, renderer.as_deref_mut()) {
                            renderer.write_map_tile(map, cells, &[GpuMapColumn::NONE; MAP_TILE * MAP_TILE]);
                        }
                        view.written.insert(cells, (0, 0));
                    }
                    wanted_tiles.push(key);
                }
            }
        }
        wanted_tiles.into_iter().for_each(|key| self.ask(key));

        let view = self.views.get_mut(&node).expect("kept by update");
        let Some([low, high]) = heights else {
            ui.set_image(parts.picture, None);
            return;
        };
        // Its camera: looking at what it follows, as high as that stands, far enough to see the slab whole.
        let looked = Vec3::new(middle.x - corner[0] as f32, followed.y - low as f32, middle.y - corner[1] as f32);
        let looked = Vec3::new(looked.x, looked.y.clamp(0.0, (high - low) as f32), looked.z);
        let tilt = view.tilt.clamp(MAP_LEAST_TILT, std::f32::consts::FRAC_PI_2);
        let fits = reach * if large { 1.45 } else { 1.02 } / (FOV * 0.5).tan();
        let camera = MapCamera { middle: looked, turn: view.turn, tilt, distance: fits, fov: FOV };
        let mut draw = MapDraw {
            size,
            corner,
            base: low,
            camera,
            reach,
            round: settings.round,
            heights: [0.0, (high - low) as f32],
            thick: if large { 4.0 } else { 2.0 },
            samples: if large { 1 } else { 2 },
            look: MapLook::default(),
        };
        // Left alone for a few frames, a map is drawn once more with smoother edges.
        let same = view.drawn.is_some_and(|drawn| MapDraw { samples: draw.samples, ..drawn } == draw);
        view.still = if same && self.asked.is_empty() { view.still + 1 } else { 0 };
        if large && view.still >= 3 {
            draw.samples = 2;
        }
        view.drawn = Some(draw);
        let shown = ShownMap {
            world: entity,
            camera,
            size: Vec2::new(rect[2], rect[3]),
            corner,
            base: low,
            cell,
            origin,
            reach,
            round: settings.round,
        };
        self.shown.insert(node, shown);
        let (Some(map), Some(renderer)) = (map, renderer.as_deref_mut()) else { return };
        renderer.draw_map(map, draw);
        let image = renderer.map_image(map).filter(|image| image.size == size);
        ui.set_image(parts.picture, image.map(|image| ImageRef { texture: image.texture.0, uv: [0.0, 0.0, 1.0, 1.0] }));
    }
}

/// Hears the pointer on a map: a drag turns and tilts it, a drag with the
/// middle button moves it over the ground, and the wheel zooms it.
fn hear_pointer(view: &mut View, ui: &mut Ui, node: NodeId) {
    ui.take_wheel(node, true);
    // The wheel turned away brings the ground nearer, as it does in the editor's views.
    let wheel = ui.wheeled(node)[1];
    if wheel != 0.0 {
        view.zoom = (view.zoom * (1.0 - wheel.signum() * 0.12)).clamp(1.0 / 64.0, 8.0);
    }
    let (turning, moving) = (ui.is_pressed(node), ui.is_middle_pressed(node));
    let (Some(now), true) = (ui.pointer_position(), turning || moving) else {
        view.held = None;
        return;
    };
    let Some(before) = view.held.replace(now) else { return };
    let (dx, dy) = (now.0 - before.0, now.1 - before.1);
    if turning {
        view.turn -= dx * 0.008;
        view.tilt = (view.tilt + dy * 0.006).clamp(MAP_LEAST_TILT, std::f32::consts::FRAC_PI_2);
        return;
    }
    // Moved with the ground under the pointer: a pixel is as many cells as the slab is wide over the view.
    let rect = ui.rect(node);
    let cells = view.reach * 2.0 * 1.45 / rect[3].max(1.0);
    let (sin, cos) = view.turn.sin_cos();
    let (right, ahead) = (Vec2::new(cos, -sin), Vec2::new(sin, cos));
    view.moved -= (right * dx - ahead * dy / view.tilt.sin().max(0.3)) * cells;
}
