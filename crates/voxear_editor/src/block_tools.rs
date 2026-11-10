//! The Scene view's brush for a Block World under its Build tool: a click
//! puts the picked kind of block against the face under the pointer, takes
//! the block there away, or turns it into the picked kind, as the brush
//! says (Shift places where it would break, and breaks where it would
//! place); dragging goes on block by block. A brush larger than a block
//! reaches a ball of blocks that many across. A stroke is one step to undo.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Game, blocks::BlockWorldAsset};
use voxear_terrain::blocks::{
    BlockHit, BlockWorld,
    chunk::{AIR, BlockId},
};
use voxear_ui::{Color, Shape};

use crate::{
    block_props::{self, BlockBrush, BlockTool},
    scene_tools::ViewCamera,
};

/// The brush's outline's colour.
const COLOR: u32 = 0xF5C542;

/// How far into the world a click reaches for a block, in metres.
const REACH: f32 = 2000.0;

/// The Scene view's brush for worlds of blocks: the block under the
/// pointer, and while a stroke goes on, its world and the block it last
/// worked on.
#[derive(Default)]
pub struct BlockTools {
    under: Option<(Entity, BlockHit)>,
    stroke: Option<(Entity, [i32; 3])>,
}

/// Where the Obj's world of blocks starts.
fn corner_of(game: &Game, entity: Entity) -> Option<Vec3> {
    let world = &game.engine.world;
    Some(voxear_terrain::corner(world.get(entity)?, world.get(entity)))
}

/// The block under `point` in the selected Obj's world, and the face the
/// ray came through.
fn block_under(game: &Game, camera: &ViewCamera, entity: Entity, point: [f32; 2]) -> Option<BlockHit> {
    let blocks = game.engine.world.get::<BlockWorld>(entity)?;
    let corner = corner_of(game, entity)?;
    let (origin, direction) = camera.ray(point);
    let size = blocks.block_size;
    let from = ((origin - corner) / size).to_array();
    voxear_terrain::blocks::raycast(&blocks.blocks, from, direction.to_array(), REACH / size)
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

impl BlockTools {
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.under.is_some()
    }

    /// Notes the block under the pointer, for the selected Obj `entity` when
    /// it has a world of blocks and its Build tool shows.
    pub fn hover(&mut self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, point: Option<[f32; 2]>) {
        if self.is_dragging() {
            return;
        }
        self.under = None;
        let (Some(entity), Some(point)) = (entity, point) else { return };
        let world = &game.engine.world;
        if !world.has::<BlockWorldAsset>(entity) || block_props::ui(world).tool != BlockTool::Build {
            return;
        }
        self.under = block_under(game, camera, entity, point).map(|hit| (entity, hit));
    }

    /// The block a click on `hit` works on: the air before the face for one
    /// placing, else the block itself.
    fn middle(hit: &BlockHit, brush: BlockBrush) -> [i32; 3] {
        match brush {
            BlockBrush::Place => [0, 1, 2].map(|i| hit.block[i] + hit.normal[i]),
            _ => hit.block,
        }
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

    /// Works the brush on `hit` in `entity`'s world; the block it worked on.
    fn work(game: &mut Game, entity: Entity, hit: &BlockHit, brush: BlockBrush) -> [i32; 3] {
        let state = block_props::ui(&game.engine.world);
        let middle = Self::middle(hit, brush);
        let picked = (state.block + 1) as BlockId;
        let Some(blocks) = game.engine.world.get_mut::<BlockWorld>(entity) else { return middle };
        let map = blocks.blocks.clone();
        let changes: Vec<([i32; 3], BlockId)> = reached(middle, state.size)
            .into_iter()
            .filter_map(|at| {
                let now = map.get(at);
                let wanted = match brush {
                    BlockBrush::Place => (now == AIR).then_some(picked),
                    BlockBrush::Break => (now != AIR).then_some(AIR),
                    BlockBrush::Paint => (now != AIR).then_some(picked),
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
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
        shift: bool,
    ) -> bool {
        self.hover(game, camera, Some(entity), Some(point));
        let Some((_, hit)) = self.under.filter(|(e, _)| *e == entity) else { return false };
        let brush = Self::brush(game, shift);
        let last = Self::work(game, entity, &hit, brush);
        self.stroke = Some((entity, last));
        true
    }

    /// Goes on with the stroke under `point`, where it has moved to another
    /// block.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2], shift: bool) {
        let Some((entity, last)) = self.stroke else { return };
        let Some(hit) = block_under(game, camera, entity, point) else { return };
        let brush = Self::brush(game, shift);
        // Placing builds on what it placed, so it waits until the pointer leaves the block it last worked on.
        if Self::middle(&hit, brush) == last || hit.block == last {
            return;
        }
        let worked = Self::work(game, entity, &hit, brush);
        self.stroke = Some((entity, worked));
    }

    /// Ends the stroke; its Obj, for the step to undo.
    pub fn end(&mut self) -> Option<Entity> {
        self.stroke.take().map(|(entity, _)| entity)
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
        let middle = Self::middle(&hit, Self::brush(game, shift));
        let size = blocks.block_size;
        let half = (state.size as f32 * 0.5).max(0.5);
        let centre = Vec3::new(middle[0] as f32 + 0.5, middle[1] as f32 + 0.5, middle[2] as f32 + 0.5);
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                let sign = |bit: u32| if i & bit == 0 { -half } else { half };
                corner + (centre + Vec3::new(sign(1), sign(2), sign(4))) * size
            })
            .collect();
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let mut shapes = Vec::new();
        for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            if let (Some(from), Some(to)) = (camera.project(corners[a]), camera.project(corners[b])) {
                shapes.push(Shape::Line { from: local(from), to: local(to), width: 1.5, color: Color::hex(COLOR) });
            }
        }
        shapes
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
