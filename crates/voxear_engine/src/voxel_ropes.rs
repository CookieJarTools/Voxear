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
//! of where the rope is tied, let go into it rather than thrown, and heavy
//! in the air: it sways a little and settles. It stays a body while it hangs.
//! A piece may hang from several ropes, sharing its weight as they must for
//! it to hang still: cut one and the rest take its share, the piece leaning
//! toward them, and may snap in turn.
//! A rope snaps when what hangs from it pulls harder than its strength, its
//! share of its weight and its swing together, when a blast reaches it, or when the game
//! cuts it ([`cut`]); its piece then falls, and rests as pieces do. Each
//! rope is drawn as a line of voxels from where it is tied to the point it
//! holds.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::VoxelPiece;
use voxear_scene::{Name, Transform, WorldTransform};

use crate::{
    physics::{Joint, JointType, MoreJoints, PhysicsScene},
    voxel_felling::{PieceFall, PieceRest, VoxelPieceBody},
};

/// A rope tied where its Obj is, holding the point of what was built
/// `hangs` metres from there (along the world's axes), `length` metres long
/// (0 for as long as from its Obj to that point), snapping when pulled
/// harder than `strength` newtons (0 for never).
///
/// ```toml
/// [obj.voxel_rope]
/// hangs = [0.0, -3.0, 0.0]
/// length = 3.5        # left out, as far as to the point it holds
/// strength = 20000.0  # newtons; left out, it never snaps
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoxelRope {
    pub hangs: [f32; 3],
    pub length: f32,
    pub strength: f32,
}

impl Default for VoxelRope {
    fn default() -> Self {
        Self { hangs: [0.0, -2.0, 0.0], length: 0.0, strength: 0.0 }
    }
}

impl Component for VoxelRope {}

impl VoxelRope {
    /// How long it is: as given, or as far as from where it is tied to the
    /// point it holds.
    pub fn long(&self) -> f32 {
        if self.length > 0.0 { self.length } else { Vec3::from(self.hangs).length() }
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
    /// How long it has been pulled harder than its strength, in seconds.
    over_for: f32,
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
    let rope = VoxelRope { hangs: (holds - tied).to_array(), length: length.max(0.0), strength: strength.max(0.0) };
    let obj = world.spawn((Transform::from_position(tied), Name("Rope".into())));
    world.insert(obj, rope);
    obj
}

/// What rope `obj` holds now: whether it snapped, the piece hanging from
/// it and the point it holds, in metres of the piece's Obj.
pub(crate) fn state(world: &World, obj: Entity) -> (bool, Option<Entity>, Vec3) {
    let holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
    (holds.snapped, holds.piece.filter(|piece| world.is_alive(*piece)), holds.point)
}

/// Shows rope `obj` as the server has it, on a player's machine: snapped or
/// not, holding `point` of `piece`, if any. Nothing here ties or snaps it.
pub(crate) fn show(world: &mut World, obj: Entity, snapped: bool, piece: Option<Entity>, point: Vec3) {
    let holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
    world.insert(obj, RopeHolds { piece, point, snapped, tied: true, ..holds });
}

/// Takes rope `obj` away, and the voxels it was drawn with.
pub(crate) fn remove(world: &mut World, obj: Entity) {
    if let Some(look) = world.get::<RopeHolds>(obj).and_then(|holds| holds.look) {
        world.despawn(look);
    }
    world.despawn(obj);
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
        // Its joint alone goes: the piece hangs on from the ropes left, leaning, or falls once none is.
        let left = match world.get_mut::<MoreJoints>(piece) {
            Some(joints) => {
                joints.0.retain(|(number, _)| *number != rope_number(obj));
                joints.0.len()
            }
            None => 0,
        };
        if left == 0 {
            world.remove::<MoreJoints>(piece);
        }
        // It moves afresh, woken if it had come to rest hanging; with no rope left, it rests as pieces do.
        if let Some(body) = world.get_mut::<VoxelPieceBody>(piece) {
            if left == 0 {
                body.rest = Some(PieceRest::Stamp);
            }
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
        None => tied + Vec3::from(rope.hangs),
    }
}

/// Hangs piece `obj`, just come loose from a volume whose corner is at
/// `corner`, from each rope not holding anything whose point is in it: the
/// first it finds, as a piece hangs from one rope.
pub(crate) fn catch(world: &mut World, obj: Entity, piece: &VoxelPiece, corner: Vec3) {
    let Some(place) = world.get::<Transform>(obj).map(|t| t.position) else { return };
    let cells: std::collections::HashSet<[u16; 3]> = piece.voxels.iter().map(|(c, _)| *c).collect();
    let mut caught = false;
    for (rope_obj, rope, tied) in ropes(world) {
        let holds = world.get::<RopeHolds>(rope_obj).copied().unwrap_or_default();
        if holds.snapped || holds.piece.is_some_and(|p| world.is_alive(p)) {
            continue;
        }
        let point = tied + Vec3::from(rope.hangs);
        let local = (point - corner) / piece.voxel - piece.low.as_vec3();
        let inside = (0..27).any(|i| {
            let c = (local + Vec3::new((i % 3 - 1) as f32, (i / 3 % 3 - 1) as f32, (i / 9 - 1) as f32)).floor();
            c.cmpge(Vec3::ZERO).all() && cells.contains(&c.to_array().map(|a| a as u16))
        });
        if !inside {
            continue;
        }
        let joint = Joint {
            kind: JointType::Distance,
            anchor: (tied - place).to_array(),
            hold: Some((point - place).to_array()),
            limits: Some([0.0, rope.long()]),
            ..Joint::default()
        };
        // A joint of its own for each rope, beside any other.
        let mut joints = world.get::<MoreJoints>(obj).cloned().unwrap_or_default();
        joints.0.push((rope_number(rope_obj), joint));
        world.insert(obj, joints);
        let kept = RopeHolds { piece: Some(obj), point: point - place, tied: true, ..holds };
        world.insert(rope_obj, kept);
        caught = true;
    }
    if !caught {
        return;
    }
    // Let go into its ropes, not thrown, and heavy in the air: it sways a little and settles, as a building
    // hanging would, not as a light thing on a string.
    if let Some(body) = world.get_mut::<VoxelPieceBody>(obj) {
        (body.rest, body.push) = (Some(PieceRest::Stay), crate::voxel_felling::PiecePush::Drop);
    }
    if let Some(rigidbody) = world.get_mut::<crate::physics::Rigidbody>(obj) {
        (rigidbody.drag, rigidbody.angular_drag) = (HANGING_DRAG, HANGING_TURN_DRAG);
    }
}

/// The number rope `obj`'s joint goes by among a piece's joints.
fn rope_number(obj: Entity) -> u32 {
    let bits = obj.to_bits();
    (bits ^ (bits >> 32)) as u32
}

/// How much of what hangs from a piece each rope holding it bears, its
/// point each of `holding` in the world and its weight's middle at
/// `middle`: the pulls that together bear its weight and turn it neither
/// way, the least of those, a rope that would have to push slack. Two
/// ropes across a square's diagonal from where a third was cut bear half
/// each; the third, little.
fn shares(middle: Vec3, holding: &[Vec3]) -> Vec<f32> {
    let n = holding.len();
    let mut taut = vec![true; n];
    for _ in 0..n {
        let at: Vec<usize> = (0..n).filter(|&i| taut[i]).collect();
        let rows: Vec<[f64; 3]> =
            at.iter().map(|&i| [1.0, f64::from(holding[i].x - middle.x), f64::from(holding[i].z - middle.z)]).collect();
        // The least pulls F with Σ F = 1 and Σ F·across = 0: F = Aᵀ (A Aᵀ)⁻¹ b.
        let mut m = [[0.0f64; 3]; 3];
        for r in &rows {
            for i in 0..3 {
                for k in 0..3 {
                    m[i][k] += r[i] * r[k];
                }
            }
        }
        let pulls: Vec<f64> = match solve3(m, [1.0, 0.0, 0.0]) {
            Some(y) => rows.iter().map(|r| r[0] * y[0] + r[1] * y[1] + r[2] * y[2]).collect(),
            // In a line, or one alone: shared alike.
            None => vec![1.0 / rows.len().max(1) as f64; rows.len()],
        };
        if let Some(k) = (0..at.len()).filter(|&k| pulls[k] < 0.0).min_by(|&a, &b| pulls[a].total_cmp(&pulls[b])) {
            taut[at[k]] = false;
            continue;
        }
        let mut out = vec![0.0; n];
        for (k, &i) in at.iter().enumerate() {
            out[i] = pulls[k].max(0.0) as f32;
        }
        return out;
    }
    vec![1.0 / n.max(1) as f32; n]
}

/// `x` with `m x = b`, none where `m` has none.
fn solve3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-6 {
        return None;
    }
    let mut x = [0.0; 3];
    for c in 0..3 {
        let mut mc = m;
        for r in 0..3 {
            mc[r][c] = b[r];
        }
        x[c] = det(&mc) / d;
    }
    Some(x)
}

/// How long a rope takes being pulled harder than its strength before it
/// snaps, in seconds, and how much harder snaps it at once.
const JOLT: f32 = 0.2;
const SNAPS_AT_ONCE: f32 = 1.5;

/// How hard the air holds back a piece hanging from a rope, moving and
/// turning: enough that it sways a little and settles.
const HANGING_DRAG: f32 = 2.0;
const HANGING_TURN_DRAG: f32 = 6.0;

/// How thick a rope is drawn, in voxels.
const THICK: u32 = 2;
/// What a rope is drawn in.
const ROPE_COLOUR: [u8; 4] = [150, 118, 72, 255];

/// Moves the ropes on, after each physics step: what a rope newly tied
/// holds is let hang; one pulled harder than its strength snaps; and each
/// is drawn from where it is tied to the point it holds.
pub(crate) fn tend(world: &mut World, dt: f32) {
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
    // A player's machine in a session draws its ropes as the server has them, and ties and snaps none.
    if crate::net::is_client(world) {
        for (obj, rope, tied) in found {
            draw(world, obj, rope, tied);
        }
        return;
    }
    for (obj, rope, tied) in found {
        let mut holds = world.get::<RopeHolds>(obj).copied().unwrap_or_default();
        if !holds.tied {
            // Newly tied: what it holds that nothing else holds comes loose and hangs from it.
            holds.tied = true;
            world.insert(obj, holds);
            crate::voxel_collapse::settle_around(world, tied + Vec3::from(rope.hangs));
        }
        // Pulled by what hangs, its share of its weight and its swing, harder than it holds: it snaps.
        if let Some(piece) = holds.piece.filter(|_| !holds.snapped) {
            let alive = world.is_alive(piece)
                && world
                    .get::<MoreJoints>(piece)
                    .is_some_and(|joints| joints.0.iter().any(|(n, _)| *n == rope_number(obj)));
            let moving = world.get::<VoxelPieceBody>(piece).is_some_and(|body| body.fall != PieceFall::Waiting);
            let mass = world.get::<VoxelPieceBody>(piece).map_or(0.0, |body| body.piece.mass);
            let speed = speeds.iter().find(|(p, _)| *p == piece).map_or(0.0, |(_, v)| v.length());
            if !alive {
                if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
                    kept.piece = None;
                }
            } else if moving && rope.strength > 0.0 {
                let placed = world.get::<Transform>(piece).map(|t| (t.position, t.rotation));
                let (middle, turn) = placed.unwrap_or((tied, Quat::IDENTITY));
                // The points of every rope holding it, this one's among them.
                let holding: Vec<(Entity, Vec3)> = ropes(world)
                    .into_iter()
                    .filter_map(|(other, ..)| {
                        let h = world.get::<RopeHolds>(other)?;
                        (h.piece == Some(piece) && !h.snapped).then(|| (other, middle + turn * h.point))
                    })
                    .collect();
                let points: Vec<Vec3> = holding.iter().map(|(_, p)| *p).collect();
                let share = holding.iter().position(|(o, _)| *o == obj).map_or(1.0, |i| shares(middle, &points)[i]);
                let pull = share * mass * (9.81 + speed * speed / rope.long().max(0.1));
                // A jolt it takes a moment of; pulled harder for longer, or much harder at once, it snaps.
                let over_for = if pull > rope.strength { holds.over_for + dt } else { 0.0 };
                if over_for > JOLT || pull > rope.strength * SNAPS_AT_ONCE {
                    cut(world, obj);
                } else if let Some(kept) = world.get_mut::<RopeHolds>(obj) {
                    kept.over_for = over_for;
                }
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
