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
pub(crate) use scripts::register;
pub use scripts::{MapScript, MapViewSeen, MapViewWish};

/// The cells a minimap's window has along a side, and a map's.
const MINIMAP_SIDE: u32 = 256;
const MAP_SIDE: u32 = 256;

/// How far from its middle a minimap shows ground, in cells: what its fill
/// is measured by. Its window holds more, so that ground lower than what it
/// looks at, seen further off, is there to be drawn.
const MINIMAP_SHOWS: f32 = 46.0;

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
    /// The number it had when all of it last changed, and each chunk column
    /// a change lay in since, by the number it had then: a tile made from
    /// an older ground is as good as one made now unless one lies in it. A
    /// world being made round the camera is another ground with every chunk
    /// that comes, and none of those change what a map shows.
    remade: u64,
    touched: HashMap<[i32; 2], u64>,
}

impl KeptGround {
    /// Whether tile `key`, made from the ground when its number was
    /// `number`, would be made another way now.
    fn changed_since(&self, key: &TileKey, number: u64) -> bool {
        let lies_in = |column: &[i32; 2]| [column[0] >> key.level, column[1] >> key.level] == key.at;
        self.remade > number || self.touched.iter().any(|(column, at)| *at > number && lies_in(column))
    }
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
    /// Its own turn and tilt, in radians, how far it is zoomed in from its
    /// settings' zoom, and how far it was moved from what it follows, in
    /// metres along x and z.
    turn: f32,
    tilt: f32,
    zoom: f32,
    moved: Vec2,
    /// The zoom and tilt its settings last gave it: changed, they are its
    /// own again.
    given: Option<(u32, f32)>,
    /// How many metres a cell was when it was last shown.
    cell: f32,
    /// Where the pointer was when last looked at, while it is held on the
    /// map.
    held: Option<(f32, f32)>,
    /// How many frames it has been drawn the same way: left alone, it is
    /// drawn once more with smoother edges.
    still: u32,
    /// How far its slab reached from its middle when last shown, in cells.
    reach: f32,
    /// How wide its ground was last drawn, as a share of its view's height.
    fill: f32,
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
            given: None,
            cell: 1.0,
            held: None,
            still: 0,
            reach: side as f32 * 0.3,
            fill: settings.fill,
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
    /// How many blocks a cell is drawn as: its zoom, any number.
    pub blocks: f32,
    /// How high the ground, or the water over it, stands under what the
    /// map follows, in the scene: none until that ground is made.
    pub ground: Option<f32>,
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
    pub fn past(&self, cells: Vec3) -> f32 {
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

/// Where maps are seen from: `follow` where it is an Obj that is there, or
/// else the game's camera.
fn follows(world: &World, follow: Option<Entity>) -> Option<MapViewer> {
    let followed = follow.filter(|entity| world.is_alive(*entity)).and_then(|entity| {
        let local = world.get::<Transform>(entity)?;
        Some(viewer_of(local, world.get::<WorldTransform>(entity)))
    });
    followed.or_else(|| camera(world))
}

impl Maps {
    /// Where maps are seen from: the Obj scripts gave them to follow (see
    /// [`MapScript`]), or else the game's camera.
    pub fn viewer(&self, world: &World) -> Option<MapViewer> {
        follows(world, world.resource::<MapScript>().and_then(|script| script.follow))
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
    ///
    /// `numbers` are the numbers scripts hold each map view by: what a
    /// script asked of a view (see [`MapViewWish`]) is done here, and how
    /// each was seen is left for scripts to read (see [`MapViewSeen`]).
    pub fn update(
        &mut self,
        world: &mut World,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        (ui, scale): (&mut Ui, f32),
        numbers: &HashMap<NodeId, u32>,
    ) {
        // Taken out of the world while the maps are shown, so the world is only read meanwhile.
        let mut script = world.remove_resource::<MapScript>().unwrap_or_default();
        self.update_with(world, &mut script, content, renderer, (ui, scale), numbers);
        script.seen.retain(|number, _| numbers.values().any(|kept| kept == number));
        world.insert_resource(script);
    }

    fn update_with(
        &mut self,
        world: &World,
        script: &mut MapScript,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
        (ui, scale): (&mut Ui, f32),
        numbers: &HashMap<NodeId, u32>,
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
            self.keep_ground((world, content), *entity, blocks, asset, *corner);
        }
        self.grounds.retain(|entity, _| found.as_ref().is_some_and(|(found, ..)| found == entity));
        self.take_made();
        let viewer = follows(world, script.follow);
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
            let number = numbers.get(&node).copied();
            let wish = number.and_then(|number| script.wishes.remove(&number));
            let seen =
                self.show((*entity, known), viewer, (node, settings, wish), renderer.as_deref_mut(), (ui, scale));
            // Its settings as scripts may just have changed them.
            let settings = ui.map_view_settings(node).unwrap_or(settings);
            self.overlay((world, &*script), ui, (node, &settings), viewer);
            if let (Some(number), Some(seen)) = (number, seen) {
                script.seen.insert(number, seen);
            }
        }
        self.written.retain(|node, _| ui.is_visible(*node));
        self.forget_unshown(frame);
    }

    /// Keeps what `entity`'s tiles are made from, as its world is now, and
    /// marks the tiles a change to it lies in to be made again.
    fn keep_ground(
        &mut self,
        (world, content): (&World, &mut Content),
        entity: Entity,
        blocks: &BlockWorld,
        asset: &BlockWorldAsset,
        corner: Vec3,
    ) {
        let endless = asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
        // The plants the world grows by itself that a map shows, as their Foliage are now.
        let plants = if endless { crate::grown_plants::on_maps(world, content, entity) } else { Vec::new() };
        if let Some(kept) = self.grounds.get_mut(&entity) {
            (kept.corner, kept.block_size) = (corner, blocks.block_size);
            let same = Arc::ptr_eq(&kept.blocks, &blocks.blocks)
                && Arc::ptr_eq(&kept.changes, &asset.pending)
                && Arc::ptr_eq(&kept.faces, &blocks.faces)
                && kept.ground.plants() == &plants[..];
            if same {
                return;
            }
        }
        let making = if endless { crate::blocks::making(content, asset) } else { None };
        let (ground, changes) = match making {
            Some(making) => {
                let changes = Arc::new(all_changes(&asset.base, &blocks.blocks, &asset.pending));
                (MapGround::made(making, changes.clone()).growing(plants), Some(changes))
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
            ) if old.file == making.file
                && Arc::ptr_eq(&old.world, &making.world)
                && before.is_some_and(|kept| kept.ground.plants() == ground.plants()) =>
            {
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
        // What is being made was asked of the old ground: made again when it comes, where a change lies in it.
        let number = before.map_or(0, |kept| kept.number) + 1;
        let (remade, touched) = match (&changed, before) {
            (Some(columns), Some(kept)) => {
                // With no tile being made, none can come that was asked of an older ground.
                let mut touched = if self.asked.is_empty() { HashMap::new() } else { kept.touched.clone() };
                touched.extend(columns.iter().map(|column| (*column, number)));
                (kept.remade, touched)
            }
            _ => (number, HashMap::new()),
        };
        let kept = KeptGround {
            number,
            remade,
            touched,
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
            let stale = self.grounds.get(&key.world).is_some_and(|ground| ground.changed_since(&key, number));
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

    /// Shows the world round `viewer` in one view, seen as its settings,
    /// the pointer and what a script wished of it say; how it was seen.
    fn show(
        &mut self,
        (entity, known): (Entity, &MapKnown),
        viewer: MapViewer,
        (node, mut settings, wish): (NodeId, MapViewSettings, Option<MapViewWish>),
        mut renderer: Option<&mut Renderer>,
        (ui, scale): (&mut Ui, f32),
    ) -> Option<MapViewSeen> {
        let ground = self.grounds.get(&entity)?;
        let (faces, origin, block_size) = (ground.faces.clone(), ground.corner, ground.block_size.max(0.01));
        let parts = ui.map_view_parts(node)?;
        // Its parts as large as its design has them on a screen of the height it is for.
        let screen = ui.rect(ui.root())[3];
        if screen >= 1.0 {
            ui.size_map_view(node, if settings.reference > 0.0 { screen / settings.reference } else { 1.0 });
        }
        // The ground is drawn where its picture is: all of a map, a minimap's window.
        let rect = ui.rect(parts.picture);
        let size = [(rect[2] * scale).round().max(1.0) as u32, (rect[3] * scale).round().max(1.0) as u32];
        if rect[2] < 2.0 || rect[3] < 2.0 {
            return None;
        }
        let large = settings.kind == MapViewKind::Map;
        let frame = self.frame;
        let view = self.views.get_mut(&node).expect("kept by update");
        // What a script wished of it: settings by their names first, then how it is seen.
        let wish = wish.unwrap_or_default();
        if !wish.settings.is_empty() {
            wish.settings.iter().for_each(|text| drop(settings.set_all(text)));
            ui.set_map_view_settings(node, settings);
        }
        // Its settings' zoom and tilt are its own until the pointer or a script changes them, and again when
        // they change.
        if view.given != Some((settings.zoom, settings.tilt)) || wish.reset {
            (view.tilt, view.zoom) = (settings.tilt.to_radians(), 1.0);
            view.given = Some((settings.zoom, settings.tilt));
        }
        if wish.reset {
            (view.turn, view.moved) = (0.0, Vec2::ZERO);
        }
        view.turn = wish.turn.unwrap_or(view.turn);
        view.tilt = wish.tilt.unwrap_or(view.tilt);
        view.zoom = wish.zoom.map_or(view.zoom, |blocks| settings.zoom as f32 / blocks.max(0.01));
        view.moved = wish.offset.unwrap_or(view.moved);
        if let Some(place) = wish.look_at {
            view.moved = Vec2::new(place.x - viewer.position.x, place.z - viewer.position.z);
        }
        // A map's buttons zoom to the next power of two, nearer or further.
        let now = settings.zoom as f32 / view.zoom.max(1e-3);
        if parts.zoom_in.is_some_and(|button| ui.clicked(button)) {
            view.zoom = settings.zoom as f32 / 2f32.powf((now.log2() - 0.01).ceil() - 1.0);
        }
        if parts.zoom_out.is_some_and(|button| ui.clicked(button)) {
            view.zoom = settings.zoom as f32 / 2f32.powf((now.log2() + 0.01).floor() + 1.0);
        }
        if settings.controls {
            hear_pointer(view, ui, (node, parts.picture));
        }
        if settings.turns {
            view.turn = viewer.heading;
        }
        // How many blocks a cell is, within what its settings let it be: past half way to the next power of
        // two, the map goes to the next level, its cells twice or half the size.
        let [least, most_blocks] = settings.zoom_range.map(|blocks| blocks.max(1) as f32);
        let blocks = (settings.zoom as f32 / view.zoom.max(1e-3)).clamp(least * 0.75, most_blocks * 1.4);
        view.zoom = settings.zoom as f32 / blocks;
        let level =
            (blocks.log2().round() as i32).clamp(least.log2() as i32, most_blocks.log2() as i32).clamp(0, 8) as u32;
        let within = (1 << level) as f32 / blocks;
        let cell = block_size * (1 << level) as f32;
        view.cell = cell;
        // What it looks at: what it follows, moved as the player moved the map, in this level's cells.
        let followed = (viewer.position - origin) / cell;
        if view.world != Some(entity) || view.level != level {
            // Another world, or another size of cell: everything in the window is something else now.
            if view.world != Some(entity) {
                view.moved = Vec2::ZERO;
            }
            (view.world, view.level) = (Some(entity), level);
            view.written.clear();
        }
        let middle = Vec2::new(followed.x, followed.z) + view.moved / cell;
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
        let rest = if large { most / std::f32::consts::SQRT_2 } else { MINIMAP_SHOWS.min(most) };
        let shows = (rest / within.max(0.05)).min(most);
        // A view its ground is to cover has all its window's ground to draw, past what it shows: so the
        // ground is there wherever a corner of the view looks, however much lower it lies.
        let covering = settings.cover && !settings.round;
        let reach = if covering { most } else { shows };
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
        let mut held: HashMap<[i32; 2], Arc<MapTile>> = HashMap::new();
        for at in places {
            let key = TileKey { world: entity, level, at };
            let cells = at.map(|tile_at| tile_at * tile);
            let view = self.views.get_mut(&node).expect("kept by update");
            match self.tiles.get_mut(&key) {
                Some(kept) => {
                    kept.shown = frame;
                    held.insert(at, kept.tile.clone());
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

        // The ground under what it follows, or the water over it: what the map looks at, so one who flies over
        // the ground or walks under it is over the same ground on the map.
        let ground = self.column(entity, level, viewer.position).filter(|column| column.is_some());
        let ground = ground.map(|column| column.top.max(column.water) as f32);
        let view = self.views.get_mut(&node).expect("kept by update");
        let Some([low, high]) = heights else {
            ui.set_image(parts.picture, None);
            return None;
        };
        // Its camera: looking at what it follows, as high as the ground there, far enough to see the slab
        // whole.
        let stands = ground.unwrap_or(followed.y);
        let looked = Vec3::new(middle.x - corner[0] as f32, stands - low as f32, middle.y - corner[1] as f32);
        let looked = Vec3::new(looked.x, looked.y.clamp(0.0, (high - low) as f32), looked.z);
        let tilt = view.tilt.clamp(MAP_LEAST_TILT, std::f32::consts::FRAC_PI_2);
        // As wide on the screen as its settings say: its fill is the width of what it shows over the view's
        // height. Nearer where it is to cover its view and that takes it: first for what it shows to run past
        // the view on ground about as high as what it looks at, which is how large its ground is drawn; then,
        // where the ground lies so much lower that even its window's would not reach, for that to.
        let fits = shows / settings.fill.max(0.05) / (FOV * 0.5).tan();
        let camera = MapCamera { middle: looked, turn: view.turn, tilt, distance: fits, fov: FOV };
        let (aspect, any_turn) = (rect[2] / rect[3], settings.turns);
        let distance = match covering {
            true => {
                let near = covers(camera, aspect, (shows, shows * 0.1), any_turn);
                covers(MapCamera { distance: near, ..camera }, aspect, (reach, looked.y), any_turn)
            }
            false => fits,
        };
        let camera = MapCamera { distance, ..camera };
        view.fill = reach / distance.max(1e-3) / (FOV * 0.5).tan();
        let mut draw = MapDraw {
            size,
            corner,
            base: low,
            camera,
            reach,
            round: settings.round,
            heights: [0.0, (high - low) as f32],
            thick: settings.thick,
            samples: if large { 1 } else { 2 },
            look: MapLook { shadows: settings.shadows, edges: settings.edges, ..MapLook::default() },
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
            blocks,
            ground: ground.map(|stands| origin.y + stands * cell),
        };
        self.shown.insert(node, shown);
        let seen = MapViewSeen {
            turn: view.turn,
            tilt,
            zoom: blocks,
            offset: view.moved,
            shown,
            settings,
            tiles: Arc::new(held),
        };
        let (Some(map), Some(renderer)) = (map, renderer.as_deref_mut()) else { return Some(seen) };
        renderer.draw_map(map, draw);
        let image = renderer.map_image(map).filter(|image| image.size == size);
        ui.set_image(parts.picture, image.map(|image| ImageRef { texture: image.texture.0, uv: [0.0, 0.0, 1.0, 1.0] }));
        Some(seen)
    }
}

/// How far `camera` may be from what it looks at, at most its own distance,
/// for a slab reaching `reach` cells to cover a view `aspect` times as wide
/// as high: every corner of the view looks at ground within the slab,
/// however the view is turned when `any_turn`. Ground may lie lower than
/// what the camera looks at, further off along the view than it is, so the
/// corners are held to ground `lower` cells under it.
fn covers(camera: MapCamera, aspect: f32, (reach, lower): (f32, f32), any_turn: bool) -> f32 {
    let half = (camera.fov * 0.5).tan();
    let (down, along) = camera.tilt.clamp(MAP_LEAST_TILT, std::f32::consts::FRAC_PI_2).sin_cos();
    let (sin, cos) = camera.turn.sin_cos();
    let (lower, within) = (lower.max(0.0), reach * 0.98);
    // Where a corner of the view meets that ground, from the middle: to the view's right and ahead of it.
    let meets = |distance: f32, [across, up]: [f32; 2]| {
        let falls = down - up * along;
        let far = (distance * down + lower) / falls.max(1e-3);
        Vec2::new(far * across, far * (along + up * down) - distance * along)
    };
    let inside = |distance: f32| {
        let corners =
            [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]].map(|[x, y]: [f32; 2]| [x * aspect * half, y * half]);
        corners.into_iter().all(|corner| {
            let at = meets(distance, corner);
            // East and north of the middle.
            let over = Vec2::new(cos * at.x + sin * at.y, cos * at.y - sin * at.x);
            match any_turn {
                true => at.length() <= within,
                false => over.x.abs().max(over.y.abs()) <= within,
            }
        })
    };
    if inside(camera.distance) {
        return camera.distance;
    }
    // The furthest that covers, between as near as a twentieth of its distance and where it is.
    let (mut near, mut far) = (camera.distance * 0.05, camera.distance);
    for _ in 0..24 {
        let middle = (near + far) * 0.5;
        if inside(middle) { near = middle } else { far = middle }
    }
    near
}

/// Hears the pointer on a map: a drag turns and tilts it, a drag with the
/// middle button moves it over the ground, and the wheel zooms it.
fn hear_pointer(view: &mut View, ui: &mut Ui, (node, picture): (NodeId, NodeId)) {
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
    // Moved with the ground under the pointer: a pixel is as many metres as the slab is wide over the view.
    let rect = ui.rect(picture);
    let metres = view.reach * 2.0 / view.fill.max(0.05) / rect[3].max(1.0) * view.cell;
    let (sin, cos) = view.turn.sin_cos();
    let (right, ahead) = (Vec2::new(cos, -sin), Vec2::new(sin, cos));
    view.moved -= (right * dx - ahead * dy / view.tilt.sin().max(0.3)) * metres;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far from the middle, in the slab's own reach, the ground a corner of the view looks at is, where
    /// the ground lies `lower` than what the camera looks at: east or north, whichever is more, or straight
    /// off when the slab is to cover however it is turned.
    fn furthest(camera: MapCamera, size: Vec2, (reach, lower): (f32, f32), any_turn: bool) -> f32 {
        let corners = [Vec2::ZERO, Vec2::new(size.x, 0.0), Vec2::new(0.0, size.y), size];
        let far = corners.map(|pixel| {
            let (from, way) = camera.ray(pixel, size);
            let at = from + way * ((camera.middle.y - lower - from.y) / way.y) - camera.middle;
            if any_turn { Vec2::new(at.x, at.z).length() } else { at.x.abs().max(at.z.abs()) }
        });
        far.into_iter().fold(0.0, f32::max) / reach
    }

    #[test]
    fn a_tile_made_from_older_ground_is_made_again_only_where_a_change_lies_in_it() {
        let world = World::new().spawn(());
        let ground = MapGround::held(Arc::new(BlockMap::default()), Arc::from(Vec::new()));
        let kept = |remade: u64, touched: &[([i32; 2], u64)]| KeptGround {
            ground: ground.clone(),
            blocks: Arc::new(BlockMap::default()),
            changes: Arc::default(),
            faces: Arc::from(Vec::new()),
            block_size: 1.0,
            corner: Vec3::ZERO,
            number: 9,
            remade,
            touched: touched.iter().copied().collect(),
        };
        // Cells of four blocks: a tile is four chunk columns a side.
        let tile = TileKey { world, level: 2, at: [1, -1] };
        // The ground is another with every chunk that comes round the camera: a tile asked for long before
        // is still the tile, while nothing changed in it.
        assert!(!kept(3, &[]).changed_since(&tile, 5));
        // All of it changed since: its blocks' colours, or what it grows.
        assert!(kept(6, &[]).changed_since(&tile, 5));
        // A block set in one of its chunk columns since, and not before, nor in another tile's.
        assert!(kept(3, &[([5, -2], 7)]).changed_since(&tile, 5));
        assert!(!kept(3, &[([5, -2], 4)]).changed_since(&tile, 5));
        assert!(!kept(3, &[([8, -2], 7), ([5, 0], 7)]).changed_since(&tile, 5));
    }

    #[test]
    fn a_covering_view_comes_no_nearer_than_it_takes_for_its_corners_to_look_at_the_slab() {
        let (size, reach) = (Vec2::new(368.0, 254.0), 100.0);
        let (aspect, slab) = (size.x / size.y, (reach, reach * 0.1));
        let fits = reach / 1.7 / (FOV * 0.5).tan();
        let camera =
            |turn: f32| MapCamera { middle: Vec3::new(0.0, 80.0, 0.0), turn, tilt: 1.0, distance: fits, fov: FOV };

        // Turned an eighth round, a square slab's corners are in the window: nearer, they are not, and no
        // nearer than that takes.
        let turned = camera(0.8);
        assert!(furthest(turned, size, (reach, 0.0), false) > 1.0);
        let near = covers(turned, aspect, slab, false);
        let seen = furthest(MapCamera { distance: near, ..turned }, size, (reach, 0.0), false);
        assert!(near < fits && seen < 1.0 && seen > 0.85, "{near} of {fits}: {seen}");

        // One that turns with the view covers it however it is turned, so it is as near at every turn.
        let any = covers(turned, aspect, slab, true);
        assert!(any <= near && (covers(camera(2.1), aspect, slab, true) - any).abs() < 1e-3);
        assert!(furthest(MapCamera { distance: any, ..camera(2.1) }, size, (reach, 0.0), true) < 1.0);

        // A view its slab covers already stays where it is.
        let far = MapCamera { distance: fits * 0.3, ..camera(0.0) };
        assert_eq!(covers(far, aspect, slab, false), far.distance);

        // Over ground that lies far lower than what it looks at, a slab twice as far reaching still covers
        // the view from where it is; one that does not reach comes nearer until it does.
        let wide = MapCamera { distance: any, ..camera(2.1) };
        let over = |distance: f32, reach: f32| furthest(MapCamera { distance, ..wide }, size, (reach, 60.0), true);
        assert!(over(any, reach) > 1.0, "the far corners look past it: {}", over(any, reach));
        assert_eq!(covers(wide, aspect, (reach * 2.2, 60.0), true), any);
        let nearer = covers(wide, aspect, (reach, 60.0), true);
        assert!(nearer < any && over(nearer, reach) < 1.0, "{nearer} of {any}: {}", over(nearer, reach));
    }
}
