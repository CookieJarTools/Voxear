//! Ropes that hold up what was built in endless Voxel Volumes
//! ([`VoxelRope`]): an Obj where the rope is tied, the point of what was
//! built it holds below it.
//!
//! A rope holds what nothing else holds up: once tied, what was built round
//! the point it holds and held to nothing that holds comes loose and hangs
//! from it, and so does whatever comes loose later with that point in it,
//! a cage blown from its wall, a platform whose posts are blown out. The
//! piece is a body of its own, as any piece is (`voxel_collapse`), hung
//! by a Distance joint holding that point of it within the rope's length
//! of where the rope is tied: it swings. It stays a body while it hangs.
//! A rope snaps when what hangs from it pulls harder than its strength, its
//! weight and its swing together, when a blast reaches it, or when the game
//! cuts it ([`cut`]); its piece then falls, and rests as pieces do. Each
//! rope is drawn as a line of voxels from where it is tied to the point it
//! holds.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::VoxelPiece;
use voxear_scene::{Name, Transform, WorldTransform};

use crate::{
    physics::{Joint, JointType, PhysicsScene},
    voxel_felling::{PieceFall, PieceRest, VoxelPieceBody},
};

/// A rope tied where its Obj is, holding the point of what was built
/// `hangs` metres from there (along the world's axes), `length` metres long
/// (0 for as long as from its Obj to that point), snapping when pulled
/// harder than `strength` newtons (0 for never).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelRope {
    pub hangs: Vec3,
    pub length: f32,
    pub strength: f32,
}

impl Default for VoxelRope {
    fn default() -> Self {
        Self { hangs: Vec3::new(0.0, -2.0, 0.0), length: 0.0, strength: 0.0 }
    }
}

impl Component for VoxelRope {}

impl VoxelRope {
    /// How long it is: as given, or as far as from where it is tied to the
    /// point it holds.
    pub fn long(&self) -> f32 {
        if self.length > 0.0 { self.length } else { self.hangs.length() }
    }
}

/// What a rope holds now: the piece hanging from it and its point, from
/// the piece's Obj; whether it has snapped; whether what it holds has been
/// let hang; and the Obj drawing it, and how many voxels long it is drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct RopeHolds {
    pub piece: Option<Entity>,
    pub point: Vec3,
    pub snapped: bool,
    tied: bool,
    look: Option<Entity>,
    drawn: u32,
}

impl Component for RopeHolds {}

/// Each rope, where it is tied and the point it holds, in the world.
fn ropes(world: &World) -> Vec<(Entity, VoxelRope, Vec3)> {
    world
        .query_ref::<(Entity, &VoxelRope, &Transform, Option<&WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .map(|(e, rope, local, placed)| (e, *rope, placed.map_or(local.position, |p| p.0.w_axis.truncate())))
        .collect()
}

/// Ties a rope at `tied` in the world holding the point `holds` of what was
/// built, `length` metres long (0 for as far as between them) and snapping
/// at `strength` newtons (0 for never): its Obj.
pub fn hang(world: &mut World, tied: Vec3, holds: Vec3, length: f32, strength: f32) -> Entity {
    let rope = VoxelRope { hangs: holds - tied, length: length.max(0.0), strength: strength.max(0.0) };
    let obj = world.spawn((Transform::from_position(tied), Name("Rope".into())));
    world.insert(obj, rope);
    obj
}

/// Cuts rope `obj`: what hangs from it falls. Whether it was a rope not cut
/// already.
pub fn cut(world: &mut World, obj: Entity) -> bool {
    let Some(holds) = world.get::<RopeHolds>(obj).copied() else {
        if world.get::<VoxelRope>(obj).is_none() {
            return false;
        }
        world.insert(obj, RopeHolds { snapped: true, tied: true, ..RopeHolds::default() });
        return true;
    };
    if holds.snapped {
        return false;
    }
    if let Some(piece) = holds.piece.filter(|piece| world.is_alive(*piece)) {
        world.remove::<Joint>(piece);
        // It falls afresh, woken if it had come to rest hanging, and rests as pieces do.
        if let Some(body) = world.get_mut::<VoxelPieceBody>(piece) {
            body.rest = Some(PieceRest::Stamp);
            if body.fall == PieceFall::Resting {
                (body.fall, body.falling, body.still) = (PieceFall::Falling, 0.0, 0.0);
            }
        }
        if let Some(mut physics) = world.remove_resource::<PhysicsScene>() {
            if let Some(id) = physics.body(piece) {
                physics.world_mut().set_active(id, true);
            }
            world.insert_resource(physics);
        }
    }
    if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
        (kept.snapped, kept.piece) = (true, None);
    }
    true
}

/// Cuts every rope a blast of `radius` metres round `at` reaches.
pub(crate) fn cut_within(world: &mut World, at: Vec3, radius: f32) {
    for (obj, rope, tied) in ropes(world) {
        let end = held_point(world, obj, rope, tied);
        let along = end - tied;
        let t = ((at - tied).dot(along) / along.length_squared().max(1e-6)).clamp(0.0, 1.0);
        if (tied + along * t).distance(at) <= radius {
            cut(world, obj);
        }
    }
}

/// Where rope `obj` holds now, in the world: on the piece hanging from it,
/// or where it was tied to hold.
fn held_point(world: &World, obj: Entity, rope: VoxelRope, tied: Vec3) -> Vec3 {
    let holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
    match holds.piece.and_then(|piece| world.get::<Transform>(piece)) {
        Some(placed) => placed.position + placed.rotation * holds.point,
        None => tied + rope.hangs,
    }
}

/// Hangs piece `obj`, just come loose from a volume whose corner is at
/// `corner`, from each rope not holding anything whose point is in it: the
/// first it finds, as a piece hangs from one rope.
pub(crate) fn catch(world: &mut World, obj: Entity, piece: &VoxelPiece, corner: Vec3) {
    let Some(place) = world.get::<Transform>(obj).map(|t| t.position) else { return };
    let cells: std::collections::HashSet<[u16; 3]> = piece.voxels.iter().map(|(c, _)| *c).collect();
    for (rope_obj, rope, tied) in ropes(world) {
        let holds = world.get::<RopeHolds>(rope_obj).copied().unwrap_or_default();
        if holds.snapped || holds.piece.is_some_and(|p| world.is_alive(p)) {
            continue;
        }
        let point = tied + rope.hangs;
        let local = (point - corner) / piece.voxel - piece.low.as_vec3();
        let inside = (0..27).any(|i| {
            let c = (local + Vec3::new((i % 3 - 1) as f32, (i / 3 % 3 - 1) as f32, (i / 9 - 1) as f32)).floor();
            c.cmpge(Vec3::ZERO).all() && cells.contains(&c.to_array().map(|a| a as u16))
        });
        if !inside || world.get::<Joint>(obj).is_some() {
            continue;
        }
        let joint = Joint {
            kind: JointType::Distance,
            anchor: (tied - place).to_array(),
            hold: Some((point - place).to_array()),
            limits: Some([0.0, rope.long()]),
            ..Joint::default()
        };
        world.insert(obj, joint);
        if let Some(body) = world.get_mut::<VoxelPieceBody>(obj) {
            body.rest = Some(PieceRest::Stay);
        }
        let kept = RopeHolds { piece: Some(obj), point: point - place, tied: true, ..holds };
        world.insert(rope_obj, kept);
        return;
    }
}

/// How thick a rope is drawn, in voxels.
const THICK: u32 = 2;
/// What a rope is drawn in.
const ROPE_COLOUR: [u8; 4] = [150, 118, 72, 255];

/// Moves the ropes on, after each physics step: what a rope newly tied
/// holds is let hang; one pulled harder than its strength snaps; and each
/// is drawn from where it is tied to the point it holds.
pub(crate) fn tend(world: &mut World) {
    let found = ropes(world);
    if found.is_empty() {
        return;
    }
    let speeds: Vec<(Entity, Vec3)> = match world.resource::<PhysicsScene>() {
        Some(physics) => found
            .iter()
            .filter_map(|(obj, ..)| world.get::<RopeHolds>(*obj).and_then(|h| h.piece))
            .filter_map(|piece| physics.body(piece).map(|id| (piece, physics.world().velocity(id).0)))
            .collect(),
        None => Vec::new(),
    };
    for (obj, rope, tied) in found {
        let mut holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
        if !holds.tied {
            // Newly tied: what it holds that nothing else holds comes loose and hangs from it.
            holds.tied = true;
            world.insert(obj, holds);
            crate::voxel_collapse::settle_around(world, tied + rope.hangs);
        }
        // Pulled by what hangs, its weight and its swing, harder than it holds: it snaps.
        if let Some(piece) = holds.piece.filter(|_| !holds.snapped) {
            let alive = world.is_alive(piece) && world.get::<Joint>(piece).is_some();
            let falling = world.get::<VoxelPieceBody>(piece).is_some_and(|body| body.fall == PieceFall::Falling);
            let mass = world.get::<VoxelPieceBody>(piece).map_or(0.0, |body| body.piece.mass);
            let speed = speeds.iter().find(|(p, _)| *p == piece).map_or(0.0, |(_, v)| v.length());
            let pull = mass * (9.81 + speed * speed / rope.long().max(0.1));
            if !alive {
                if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
                    kept.piece = None;
                }
            } else if falling && rope.strength > 0.0 && pull > rope.strength {
                cut(world, obj);
            }
        }
        draw(world, obj, rope, tied);
    }
}

/// Draws rope `obj` as a line of voxels from where it is tied to the point
/// it holds, or hanging slack from where it is tied once snapped.
fn draw(world: &mut World, obj: Entity, rope: VoxelRope, tied: Vec3) {
    let holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
    let end = if holds.snapped { tied - Vec3::Y * rope.long() * 0.4 } else { held_point(world, obj, rope, tied) };
    let along = end - tied;
    let voxel = 1.0 / 16.0;
    let count = (along.length() / voxel).ceil().max(1.0) as u32;
    let look = match holds.look.filter(|look| world.is_alive(*look)) {
        Some(look) => look,
        None => {
            let look = world.spawn((Transform::from_position(tied), Name("Rope voxels".into())));
            if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
                (kept.look, kept.drawn) = (Some(look), 0);
            }
            look
        }
    };
    if holds.drawn != count || holds.look != Some(look) {
        let mut voxels = Vec::new();
        for y in 0..count {
            for x in 0..THICK {
                for z in 0..THICK {
                    voxels.push(([x as u16, y as u16, z as u16], 1u8));
                }
            }
        }
        let line = VoxelPiece {
            low: voxear_core::math::IVec3::ZERO,
            size: [THICK, count, THICK],
            voxels,
            palette: vec![ROPE_COLOUR],
            wood: vec![true],
            voxel,
            mass: 0.0,
        };
        world.insert(look, crate::voxel_felling::piece_volume(&line, 0.9));
        if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
            kept.drawn = count;
        }
    }
    // Its column of voxels from where it is tied toward where it holds, its middle on the line between.
    let turn = Quat::from_rotation_arc(Vec3::Y, along.normalize_or(Vec3::NEG_Y));
    let half = THICK as f32 * voxel * 0.5;
    if let Some(placed) = world.get_mut::<Transform>(look) {
        (placed.position, placed.rotation) = (tied - turn * Vec3::new(half, 0.0, half), turn);
    }
}
