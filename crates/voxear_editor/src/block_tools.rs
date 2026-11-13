//! The Scene view's brush for a Block World under its Build tool: a click
//! puts the block picked against the face under the pointer, takes the
//! block there away, or turns it into the block picked, as the brush says
//! (Shift places where it would break, and breaks where it would place);
//! dragging goes on block by block. A brush larger than a block reaches a
//! ball of blocks that many across. The Model brush builds the picked
//! `.vox` model standing on the face clicked, once a click. A stroke is one
//! step to undo.
//!
//! The block picked is one of the project's, and any world can hold it: a
//! world that has not held it yet numbers it as a stroke first builds with
//! it (see `block_props::number_block`).
//!
//! A brush that builds reaches through water to the ground under it, and
//! what it places takes the water's place; building with water itself, or
//! breaking, meets the water.
//!
//! A block that faces a way is put down as a player would put it (see
//! `voxear_terrain::blocks::shape::placed_state`): a stair rising away from
//! the view, a slab on the face clicked, a door's hinge on the side of the
//! face it was put nearer; R turns it on from there, and its outline shows
//! its parts as they will stand. A door put down takes its upper block, and
//! breaking either takes both. The Use brush uses the block clicked, as a
//! player would; the Link brush links a lever or a button clicked to each
//! block clicked after it, or takes a link away again.

use std::sync::Arc;

use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_render::Renderer;
use voxear_terrain::blocks::{
    BlockFaces, BlockHit, BlockMap, BlockWorld,
    block::Liquid,
    chunk::{AIR, BlockId, Fill},
    is_water,
    shape::{Around, BlockState, PartBox, StateUse, UseKind, part_boxes, placed_state, turned_on},
    smooth::{self, Ball},
    usable::use_kind,
    vox::VoxFile,
};
use voxear_ui::{Color, Shape};

use crate::{
    block_props::{self, BlockBrush, BlockTool},
    scene_tools::ViewCamera,
};

/// The brush's outline's colour.
const COLOR: u32 = 0xF5C542;

/// A box's twelve edges, by its corners numbered a bit an axis.
const EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// The ray under `point` in `entity`'s world, in blocks from its corner:
/// where it starts, and the way it goes.
fn ray_in(game: &Game, camera: &ViewCamera, entity: Entity, point: [f32; 2]) -> Option<([f32; 3], [f32; 3])> {
    let blocks = game.engine.world.get::<BlockWorld>(entity)?;
    let corner = corner_of(game, entity)?;
    let (origin, direction) = camera.ray(point);
    Some((((origin - corner) / blocks.block_size).to_array(), direction.normalize_or_zero().to_array()))
}

/// How far into the world a click reaches for a block, in metres.
const REACH: f32 = 2000.0;

/// How far the pointer moves, in points, before a stroke works again.
const STROKE_MOVE: f32 = 4.0;

/// A model the Model brush builds, and the blocks it reaches from where it
/// stands: from its lowest corner to past its highest.
struct Model {
    file: Arc<VoxFile>,
    reach: [[i32; 3]; 2],
}

/// The Scene view's brush for worlds of blocks: the block under the
/// pointer, while a stroke goes on its world, the block it last worked on
/// and where the pointer was then, and the picked model as last read, none
/// if it could not be.
#[derive(Default)]
pub struct BlockTools {
    under: Option<(Entity, BlockHit)>,
    stroke: Option<(Entity, [i32; 3], [f32; 2])>,
    model: Option<(AssetGuid, Option<Model>)>,
    /// How many times R turned the block put down on from how it would
    /// face, and that block placed so, as it stands under the pointer.
    turns: u8,
    placing: Option<(Entity, BlockState)>,
    /// The lever or button the Link brush links from, while one is picked.
    linking: Option<(Entity, [i32; 3])>,
}

/// The `.vox` model `guid`, read from its file.
fn read_model(game: &Game, guid: AssetGuid) -> Option<Model> {
    let db = game.content.db();
    let entry = db.get(guid)?;
    let read = std::fs::read(db.file(&entry.path)).map_err(anyhow::Error::from).and_then(|b| VoxFile::read(&b));
    let file = read.map_err(|error| tracing::warn!("{}: {error:#}", entry.path)).ok()?;
    let blocks = file.blocks();
    let low = [0, 1, 2].map(|i| blocks.iter().map(|(p, _)| p[i]).min().unwrap_or(0));
    let high = [0, 1, 2].map(|i| blocks.iter().map(|(p, _)| p[i] + 1).max().unwrap_or(1));
    Some(Model { file: Arc::new(file), reach: [low, high] })
}

/// Where the Obj's world of blocks starts.
fn corner_of(game: &Game, entity: Entity) -> Option<Vec3> {
    let world = &game.engine.world;
    Some(voxear_terrain::corner(world.get(entity)?, world.get(entity)))
}

/// The number of the block picked to build with in `entity`'s world, where
/// the world holds it.
fn picked(game: &Game, entity: Entity) -> Option<BlockId> {
    let guid = block_props::built_with(game)?;
    game.engine.world.get::<BlockWorldAsset>(entity)?.drawn.number_of(guid)
}

/// The state the block picked to build with is put down in on `hit`, by a
/// view looking along `look` (in blocks from the world's corner, `from`
/// where the ray started), turned on `turns` times: none for a block that
/// keeps no state.
fn state_for(game: &Game, hit: &BlockHit, (from, look): ([f32; 3], [f32; 3]), turns: u8) -> Option<BlockState> {
    let block = block_props::built_with(game).and_then(|guid| game.content.loaded_block(guid))?;
    let shape = if block.liquid == Liquid::None { block.shape } else { return None };
    if !StateUse::of(shape).any() {
        return None;
    }
    // Where on the face clicked, across the block that goes before it.
    let point: [f32; 3] = std::array::from_fn(|i| from[i] + look[i] * hit.distance);
    let cell: [i32; 3] = std::array::from_fn(|i| hit.block[i] + hit.normal[i]);
    let within: [f32; 3] = std::array::from_fn(|i| (point[i] - cell[i] as f32).clamp(0.0, 1.0));
    let mut state = placed_state(shape, hit.normal, within, look);
    for _ in 0..turns {
        state = turned_on(shape, state);
    }
    Some(state)
}

/// Whether the block picked to build with is water.
fn builds_water(game: &Game) -> bool {
    let picked = block_props::built_with(game).and_then(|guid| game.content.loaded_block(guid));
    picked.is_some_and(|block| block.liquid == Liquid::Water)
}

/// The block under `point` in the selected Obj's world for `brush`, and the
/// face the ray came through. A brush that builds with ground passes
/// through water to what lies under it.
fn block_under(
    game: &Game,
    camera: &ViewCamera,
    entity: Entity,
    point: [f32; 2],
    brush: BlockBrush,
) -> Option<BlockHit> {
    let blocks = game.engine.world.get::<BlockWorld>(entity)?;
    let corner = corner_of(game, entity)?;
    let (origin, direction) = camera.ray(point);
    let size = blocks.block_size;
    let from = ((origin - corner) / size).to_array();
    let through = brush != BlockBrush::Break && !builds_water(game);
    let meets = |block: BlockId| block != AIR && !(through && is_water(&blocks.faces, block));
    voxear_terrain::blocks::raycast_where(&blocks.blocks, from, direction.to_array(), REACH / size, &meets)
}

/// The blocks a brush `size` blocks across reaches round `middle`: a ball.
fn reached(middle: [i32; 3], size: u32) -> Vec<[i32; 3]> {
    let radius = size as f32 * 0.5;
    let span = radius.floor() as i32;
    let mut found = Vec::new();
    for z in -span..=span {
        for y in -span..=span {
            for x in -span..=span {
                if ((x * x + y * y + z * z) as f32).sqrt() <= radius + 1e-3 {
                    found.push([middle[0] + x, middle[1] + y, middle[2] + z]);
                }
            }
        }
    }
    found
}

/// What a brush `size` blocks across does round `middle` in a world drawn
/// smooth: a ball whose surface lies between blocks as far out as it
/// reaches, adding to the ground where it places, taking from it where it
/// breaks, and turning the ground in it to `picked` where it paints.
fn smooth_ball(
    (map, faces): (&BlockMap, &[BlockFaces]),
    middle: [i32; 3],
    size: u32,
    brush: BlockBrush,
    picked: BlockId,
) -> Vec<([i32; 3], BlockId, Fill)> {
    let does = match brush {
        BlockBrush::Place => Ball::Add(picked),
        BlockBrush::Break => Ball::Take,
        BlockBrush::Paint => Ball::Paint(picked),
        BlockBrush::Model | BlockBrush::Use | BlockBrush::Link => return Vec::new(),
    };
    smooth::ball(map, middle.map(|v| v as f32 + 0.5), size as f32 * 0.5, does, faces)
}

impl BlockTools {
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.under.is_some()
    }

    /// Turns the block the brush puts down on, a quarter about the up axis,
    /// or for one that faces six ways to its next way.
    pub fn turn(&mut self) {
        self.turns = (self.turns + 1) % 12;
    }

    /// Notes the block under the pointer, for the selected Obj `entity` when
    /// it has a world of blocks and its Build tool shows.
    pub fn hover(&mut self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, point: Option<[f32; 2]>) {
        if self.is_dragging() {
            return;
        }
        self.under = None;
        self.placing = None;
        let (Some(entity), Some(point)) = (entity, point) else { return };
        let world = &game.engine.world;
        if !world.has::<BlockWorldAsset>(entity) || block_props::ui(world).tool != BlockTool::Build {
            return;
        }
        self.under = block_under(game, camera, entity, point, Self::brush(game, false)).map(|hit| (entity, hit));
        // How the block put down would stand there, for its outline and its number.
        if let (Some((_, hit)), Some(ray)) = (self.under, ray_in(game, camera, entity, point)) {
            self.placing = state_for(game, &hit, ray, self.turns).map(|state| (entity, state));
        }
        if block_props::ui(world).brush == BlockBrush::Model {
            self.keep_model(game, false);
        }
    }

    /// Keeps the picked model read: again from its file where `again`, else
    /// only once it is another model.
    fn keep_model(&mut self, game: &Game, again: bool) {
        let picked = block_props::ui(&game.engine.world).model;
        if !again && self.model.as_ref().map(|(guid, _)| *guid) == picked {
            return;
        }
        self.model = picked.map(|guid| (guid, read_model(game, guid)));
    }

    /// The block a click on `hit` works on: the air before the face for one
    /// placing, else the block itself.
    fn middle(hit: &BlockHit, brush: BlockBrush) -> [i32; 3] {
        match brush {
            BlockBrush::Place | BlockBrush::Model => [0, 1, 2].map(|i| hit.block[i] + hit.normal[i]),
            _ => hit.block,
        }
    }

    /// The number of the block picked placed as it stands under the
    /// pointer, the world numbering that state now where it has not held
    /// it; none for a block that keeps no state.
    fn placed_number(&self, game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Option<BlockId> {
        let (on, state) = self.placing?;
        let guid = block_props::built_with(game).filter(|_| on == entity)?;
        block_props::number_block_state(game, renderer, entity, guid, state.to_byte())
    }

    /// The brush as it works now: Shift turns placing into breaking, and
    /// breaking into placing.
    fn brush(game: &Game, shift: bool) -> BlockBrush {
        match (block_props::ui(&game.engine.world).brush, shift) {
            (BlockBrush::Place, true) => BlockBrush::Break,
            (BlockBrush::Break, true) => BlockBrush::Place,
            (brush, _) => brush,
        }
    }

    /// Works the brush on `hit` in `entity`'s world, putting down `placed`
    /// where it is a block in a state; the block it worked on.
    fn work(game: &mut Game, entity: Entity, hit: &BlockHit, brush: BlockBrush, placed: Option<BlockId>) -> [i32; 3] {
        let state = block_props::ui(&game.engine.world);
        let middle = Self::middle(hit, brush);
        // With no block picked, or one the world has no room for, there is nothing to place or paint with.
        let picked = match (placed.or_else(|| picked(game, entity)), brush) {
            (Some(block), _) => block,
            (None, BlockBrush::Break) => AIR,
            (None, _) => return middle,
        };
        // One block put down or taken away at a time takes a door's other block with it.
        let one = state.size <= 1 && matches!(brush, BlockBrush::Place | BlockBrush::Break);
        let smooth = game.engine.world.get::<BlockWorld>(entity).is_some_and(|blocks| blocks.smooth);
        if one && !smooth {
            let Some(blocks) = game.engine.world.get::<BlockWorld>(entity) else { return middle };
            let now = blocks.blocks.get(middle);
            let room = now == AIR || is_water(&blocks.faces, now) && !is_water(&blocks.faces, picked);
            let wanted = match brush {
                BlockBrush::Place if room => picked,
                BlockBrush::Break if now != AIR => AIR,
                _ => return middle,
            };
            let changes = voxear_engine::blocks::with_door_halves(
                &mut game.engine.world,
                &mut game.content,
                entity,
                vec![(middle, wanted)],
            );
            if let Some(blocks) = game.engine.world.get_mut::<BlockWorld>(entity) {
                blocks.set_blocks(changes);
            }
            return middle;
        }
        let Some(blocks) = game.engine.world.get_mut::<BlockWorld>(entity) else { return middle };
        let map = blocks.blocks.clone();
        if blocks.smooth {
            let faces = blocks.faces.clone();
            blocks.set_cells(smooth_ball((&map, &faces), middle, state.size, brush, picked));
            return middle;
        }
        // Ground placed takes water's place, and painting with ground leaves water as it is.
        let faces = blocks.faces.clone();
        let water = |block: BlockId| is_water(&faces, block);
        let changes: Vec<([i32; 3], BlockId)> = reached(middle, state.size)
            .into_iter()
            .filter_map(|at| {
                let now = map.get(at);
                let wanted = match brush {
                    BlockBrush::Place => (now == AIR || (water(now) && !water(picked))).then_some(picked),
                    BlockBrush::Break => (now != AIR).then_some(AIR),
                    BlockBrush::Paint => (now != AIR && (!water(now) || water(picked))).then_some(picked),
                    BlockBrush::Model | BlockBrush::Use | BlockBrush::Link => None,
                }?;
                Some((at, wanted))
            })
            .collect();
        blocks.set_blocks(changes);
        middle
    }

    /// Works the brush under `point`, starting a stroke. Returns whether it
    /// took the press.
    pub fn begin(
        &mut self,
        game: &mut Game,
        renderer: &mut Renderer,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
        shift: bool,
    ) -> bool {
        self.hover(game, camera, Some(entity), Some(point));
        let brush = Self::brush(game, shift);
        // With Shift the brush is another, which may meet water where the one shown passes through it.
        if shift && self.under.is_some() {
            self.under = block_under(game, camera, entity, point, brush).map(|hit| (entity, hit));
        }
        let Some((_, hit)) = self.under.filter(|(e, _)| *e == entity) else { return false };
        if brush == BlockBrush::Use {
            // Used as a player would: a stroke, so a door opened is a step to undo.
            voxear_engine::blocks::use_block(&mut game.engine.world, &mut game.content, entity, hit.block);
            self.stroke = Some((entity, hit.block, point));
            return true;
        }
        if brush == BlockBrush::Link {
            let drives = |game: &Game, at: [i32; 3]| {
                let Some(blocks) = game.engine.world.get::<BlockWorld>(entity) else { return false };
                matches!(use_kind(&blocks.faces, blocks.blocks.get(at)), UseKind::Toggles | UseKind::Presses)
            };
            match self.linking.filter(|(on, from)| *on == entity && *from != hit.block) {
                // From a lever or a button to the block clicked, or the link there taken away.
                Some((_, from)) => {
                    if let Some(blocks) = game.engine.world.get_mut::<BlockWorld>(entity)
                        && !blocks.link(from, hit.block)
                    {
                        blocks.unlink(from, Some(hit.block));
                    }
                    self.stroke = Some((entity, hit.block, point));
                }
                None => self.linking = drives(game, hit.block).then_some((entity, hit.block)),
            }
            return true;
        }
        if brush == BlockBrush::Model {
            // Read again, as its file may have changed since.
            self.keep_model(game, true);
            let Some((_, Some(model))) = &self.model else { return false };
            let (file, at) = (model.file.clone(), Self::middle(&hit, brush));
            if !block_props::place_model(game, Some(renderer), entity, &file, at) {
                return false;
            }
            self.stroke = Some((entity, at, point));
            return true;
        }
        // The world numbers the block as it first builds with it, here where there is a renderer for its look.
        let placed = match brush {
            BlockBrush::Place => self.placed_number(game, Some(&mut *renderer), entity),
            _ => None,
        };
        if brush != BlockBrush::Break
            && placed.is_none()
            && let Some(guid) = block_props::built_with(game)
        {
            block_props::number_block(game, Some(renderer), entity, guid);
        }
        let last = Self::work(game, entity, &hit, brush, placed);
        self.stroke = Some((entity, last, point));
        true
    }

    /// The lever or button the Link brush links from, while one is picked.
    pub fn linking(&self) -> Option<(Entity, [i32; 3])> {
        self.linking
    }

    /// Goes on with the stroke under `point`, where the pointer has moved
    /// on from where it last worked, and as far on as half the brush.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2], shift: bool) {
        let Some((entity, last, from)) = self.stroke else { return };
        // A pointer held still works once: the blocks it changed under it are no reason to work again.
        if (point[0] - from[0]).hypot(point[1] - from[1]) < STROKE_MOVE {
            return;
        }
        let brush = Self::brush(game, shift);
        let Some(hit) = block_under(game, camera, entity, point, brush) else { return };
        // A model is built once a click, and a block used or linked once.
        if matches!(brush, BlockBrush::Model | BlockBrush::Use | BlockBrush::Link) {
            return;
        }
        // Placing builds on what it placed, so it waits until the pointer leaves the block it last worked on, and
        // a ball goes on half its width from the last.
        let middle = Self::middle(&hit, brush);
        let apart = (0..3).map(|i| (middle[i] - last[i]).pow(2)).sum::<i32>() as f32;
        let spacing = (block_props::ui(&game.engine.world).size as f32 * 0.5).max(1.0);
        if middle == last || hit.block == last || apart.sqrt() < spacing {
            return;
        }
        let placed = match brush {
            BlockBrush::Place => self.placed_number(game, None, entity),
            _ => None,
        };
        let worked = Self::work(game, entity, &hit, brush, placed);
        self.stroke = Some((entity, worked, point));
    }

    /// Ends the stroke; its Obj, for the step to undo.
    pub fn end(&mut self) -> Option<Entity> {
        self.stroke.take().map(|(entity, ..)| entity)
    }

    /// The outline of what the brush would work on, as a box of lines.
    pub fn shapes(&self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, shift: bool) -> Vec<Shape> {
        let (Some(entity), Some((under, hit))) = (entity, self.under) else { return Vec::new() };
        if under != entity {
            return Vec::new();
        }
        let (Some(blocks), Some(corner)) = (game.engine.world.get::<BlockWorld>(entity), corner_of(game, entity))
        else {
            return Vec::new();
        };
        let state = block_props::ui(&game.engine.world);
        let brush = Self::brush(game, shift);
        let middle = Self::middle(&hit, brush).map(|v| v as f32);
        let size = blocks.block_size;
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let line = |shapes: &mut Vec<Shape>, a: Vec3, b: Vec3, color: u32| {
            if let (Some(from), Some(to)) = (camera.project(a), camera.project(b)) {
                shapes.push(Shape::Line { from: local(from), to: local(to), width: 1.5, color: Color::hex(color) });
            }
        };
        // The Link brush: a line from the lever or button picked to each block it drives, and to the one under it.
        if brush == BlockBrush::Link {
            let mut shapes = Vec::new();
            let at = |p: [i32; 3]| corner + (Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) + 0.5) * size;
            let from = self.linking.filter(|(on, _)| *on == entity).map(|(_, from)| from).unwrap_or(hit.block);
            for to in blocks.links.get(&from).into_iter().flatten() {
                line(&mut shapes, at(from), at(*to), COLOR);
            }
            if self.linking.is_some() {
                line(&mut shapes, at(from), at(hit.block), 0x7FD1FF);
            }
            shapes.extend(self.outline(camera, corner, size, hit.block.map(|v| v as f32), [[0.0; 3], [1.0; 3]]));
            return shapes;
        }
        // A block of parts put down one at a time: its boxes as they will stand.
        if brush == BlockBrush::Place
            && state.size <= 1
            && let Some((_, placed)) = self.placing
            && let Some(block) = block_props::built_with(game).and_then(|guid| game.content.loaded_block(guid))
            && block.shape.is_part()
        {
            let custom = block.part_look().custom;
            let boxes: Vec<PartBox> = part_boxes(block.shape, placed, Around::default(), custom.as_ref());
            let mut shapes = Vec::new();
            for part in boxes {
                let [low, high] = [part.from, part.to].map(|c| c.map(|v| f32::from(v) / 16.0));
                shapes.extend(self.outline(camera, corner, size, middle, [low, high]));
            }
            return shapes;
        }
        // From its lowest corner to its highest, in blocks: the model where it would stand, else the brush's reach.
        let [low, high] = match (brush, &self.model) {
            (BlockBrush::Model, Some((_, Some(model)))) => {
                model.reach.map(|r| [0, 1, 2].map(|i| middle[i] + r[i] as f32))
            }
            (BlockBrush::Model, _) => return Vec::new(),
            _ => {
                let half = (state.size as f32 * 0.5).max(0.5);
                [-half, half].map(|h| middle.map(|v| v + 0.5 + h))
            }
        };
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                let pick = |bit: u32, axis: usize| if i & bit == 0 { low[axis] } else { high[axis] };
                corner + Vec3::new(pick(1, 0), pick(2, 1), pick(4, 2)) * size
            })
            .collect();
        let mut shapes = Vec::new();
        for (a, b) in EDGES {
            line(&mut shapes, corners[a], corners[b], COLOR);
        }
        shapes
    }

    /// The edges of a box from `low` to `high`, in blocks from `at`, a block
    /// of the world from its `corner`, as lines.
    fn outline(&self, camera: &ViewCamera, corner: Vec3, size: f32, at: [f32; 3], [low, high]: [[f32; 3]; 2]) -> Vec<Shape> {
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                let pick = |bit: u32, axis: usize| at[axis] + if i & bit == 0 { low[axis] } else { high[axis] };
                corner + Vec3::new(pick(1, 0), pick(2, 1), pick(4, 2)) * size
            })
            .collect();
        EDGES
            .iter()
            .filter_map(|&(a, b)| {
                let (from, to) = (camera.project(corners[a])?, camera.project(corners[b])?);
                Some(Shape::Line { from: local(from), to: local(to), width: 1.5, color: Color::hex(COLOR) })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brush_reaches_a_ball_of_blocks_as_many_across_as_its_size() {
        assert_eq!(reached([4, 5, 6], 1), vec![[4, 5, 6]]);
        assert_eq!(reached([0, 0, 0], 2).len(), 7, "the block and the six beside it");
        let three = reached([0, 0, 0], 3);
        assert!(three.contains(&[1, 1, 0]) && !three.contains(&[1, 1, 1]), "a ball, not a box: {}", three.len());
    }
}
