//! Cutting endless Voxel Volumes while the game runs ([`cut`]): the grass,
//! cover and crops of the columns a cut reaches fade away over a short
//! time ([`FADE`]), then stay cut until they grow back over the volume's
//! `regrow` seconds, or for good; and the trees whose logs it cuts through
//! are felled, what comes loose of them a body of its own.
//!
//! A felled piece ([`VoxelPieceBody`]) is an Obj with a Rigidbody, its
//! collider boxes of its logs, drawn from its voxels as a mesh of cubes. It
//! hangs where it was cut until the ground's collision round it has been
//! made again without it, then tips the way it was cut and falls. Once it
//! has lain still a moment it rests: it stays a body, which can be pushed,
//! or is stamped back into the volume where it lies, as the volume's
//! `felled` says.

use std::sync::Arc;

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{VoxelCut, VoxelFelled, VoxelPiece, VoxelVolume};
use voxear_scene::Transform;

use crate::physics::{Acoustic, Collider, ColliderBoxes, PhysicsScene, Rigidbody};

/// How long cut blades take to fade away, in seconds.
pub const FADE: f32 = 0.6;
/// How many steps they fade and grow back in: each makes their columns
/// again.
const FADE_STEPS: f32 = 8.0;
const GROW_STEPS: f32 = 16.0;
/// The most boxes a piece's collider has.
const MOST_BOXES: usize = 192;
/// How heavy wood is, in kilograms a cubic metre.
const WOOD: f32 = 650.0;
/// How fast a piece tips over the way it was cut, in turns a second.
const TIP: f32 = 0.35;
/// How slow a piece must move, in metres and in radians a second, and for
/// how long, to be at rest; and the longest it falls before it is taken as
/// at rest all the same.
const STILL_SPEED: f32 = 0.12;
const STILL_FOR: f32 = 1.0;
const LONGEST_FALL: f32 = 20.0;

/// What becomes of the pieces a cut makes come loose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CutPieces {
    /// They fall as bodies of their own.
    #[default]
    Fall,
    /// They are taken away: as in the editor, where nothing falls.
    Gone,
}

/// Where a felled piece is in its fall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PieceFall {
    /// Held where it was cut while the ground's collision round it is made
    /// again without it.
    #[default]
    Waiting,
    /// Let go, to be tipped the way it was cut.
    Tipping,
    Falling,
    /// Come to rest, a body of its own.
    Resting,
}

/// A piece of a tree felled in an endless Voxel Volume, falling or at rest.
#[derive(Clone, Debug)]
pub struct VoxelPieceBody {
    /// The volume it came from.
    pub volume: Entity,
    pub piece: Arc<VoxelPiece>,
    /// Where its Obj's place is in it, in metres from its box's least corner:
    /// the middle of its logs.
    pub middle: Vec3,
    /// Which way across it was cut to fall.
    pub toward: Vec3,
    /// How far its logs reach from its place, in metres.
    pub reach: f32,
    pub fall: PieceFall,
    /// How long it has fallen, and lain still, in seconds.
    pub falling: f32,
    pub still: f32,
}

impl Component for VoxelPieceBody {}

/// What a cut did: how many volumes it reached, how many columns' grass
/// and how many voxels of logs and leaves it cut, and the pieces' Objs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutDone {
    pub reached: usize,
    pub columns: usize,
    pub voxels: usize,
    pub pieces: Vec<Entity>,
}

/// Cuts every endless Voxel Volume `cut` reaches (in the world), or
/// `only`'s alone: the grass, cover and crops of the columns it reaches,
/// and the logs and leaves, the trees felled. What comes loose falls the
/// way `toward` says across, or away from the cut, or is taken away, as
/// `pieces` says. Kept with the volume and saved with its other changes.
pub fn cut(world: &mut World, cut: &VoxelCut, toward: Option<Vec3>, pieces: CutPieces, only: Option<Entity>) -> CutDone {
    let volumes: Vec<(Entity, Vec3)> = world
        .query_ref::<(Entity, &VoxelVolume, &Transform, Option<&voxear_scene::WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .map(|(entity, _, local, placed)| (entity, placed.map_or(local.position, |placed| placed.0.w_axis.truncate())))
        .collect();
    let mut done = CutDone::default();
    for (entity, corner) in volumes {
        let local = match cut {
            VoxelCut::Points(points) => VoxelCut::Points(points.iter().map(|p| *p - corner).collect()),
            VoxelCut::Ball { at, radius } => VoxelCut::Ball { at: *at - corner, radius: *radius },
            VoxelCut::Slice { at, across, radius } => VoxelCut::Slice { at: *at - corner, across: *across, radius: *radius },
        };
        let Some(volume) = world.get::<VoxelVolume>(entity).cloned() else { continue };
        let (changes, made) = voxear_render::voxel_cut(&volume, &local);
        if made.columns == 0 && made.voxels == 0 {
            continue;
        }
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.changes = Arc::new(changes);
        }
        done.reached += 1;
        done.columns += made.columns;
        done.voxels += made.voxels;
        if pieces == CutPieces::Gone {
            continue;
        }
        let middle_of_cut = match &local {
            VoxelCut::Points(points) => points.iter().fold(Vec3::ZERO, |sum, p| sum + *p) / points.len().max(1) as f32,
            VoxelCut::Ball { at, .. } | VoxelCut::Slice { at, .. } => *at,
        };
        for piece in made.pieces {
            done.pieces.push(spawn(world, entity, corner, piece, middle_of_cut + corner, toward));
        }
    }
    done
}

/// A piece's Obj, held where it was cut from the volume `volume` whose
/// corner is at `corner`.
fn spawn(world: &mut World, volume: Entity, corner: Vec3, piece: VoxelPiece, cut_at: Vec3, toward: Option<Vec3>) -> Entity {
    let middle = piece.middle();
    let place = corner + piece.low.as_vec3() * piece.voxel + middle;
    let boxes: Vec<(Vec3, Vec3)> = piece.boxes(MOST_BOXES).into_iter().map(|(at, half)| (at - middle, half)).collect();
    let mass = (piece.logs() as f32 * piece.voxel.powi(3) * WOOD).max(1.0);
    // Away from the cut, across; straight down a trunk, a way of its own.
    let away = (place - cut_at) * Vec3::new(1.0, 0.0, 1.0);
    let toward = toward.map(|way| way * Vec3::new(1.0, 0.0, 1.0)).filter(|way| way.length() > 1e-3);
    let toward = toward.unwrap_or(if away.length() > 0.2 {
        away
    } else {
        let turn = (place.x * 12.9898 + place.z * 78.233).sin() * 43758.545;
        Vec3::new(turn.cos(), 0.0, turn.sin())
    });
    let key = place.to_array().iter().fold(piece.voxels.len() as u64, |key, a| key.rotate_left(17) ^ u64::from(a.to_bits()));
    let collider = Collider { friction: 0.8, acoustic: Acoustic::Wood, ..Collider::default() };
    let rigidbody = Rigidbody { mass, kinematic: true, angular_drag: 0.3, ..Rigidbody::default() };
    let reach = boxes.iter().map(|(at, half)| at.length() + half.length()).fold(0.0, f32::max);
    let body = VoxelPieceBody {
        volume,
        piece: Arc::new(piece),
        middle,
        toward: toward.normalize(),
        reach,
        fall: PieceFall::Waiting,
        falling: 0.0,
        still: 0.0,
    };
    let entity = world.spawn((Transform::from_position(place), voxear_scene::Name("Felled tree".into())));
    world.insert(entity, collider);
    world.insert(entity, ColliderBoxes { key, boxes: boxes.into() });
    world.insert(entity, rigidbody);
    world.insert(entity, body);
    entity
}

/// Moves the felled pieces on, after each physics step: those waiting let
/// go once the ground round them is made again without them, those let go
/// tipped, and those that have lain still a moment at rest, stamped back
/// into their volume where they lie if it says so.
pub fn fall(world: &mut World, dt: f32) {
    let found: Vec<(Entity, VoxelPieceBody)> =
        world.query_ref::<(Entity, &VoxelPieceBody)>().into_iter().map(|(e, body)| (e, body.clone())).collect();
    if found.is_empty() {
        return;
    }
    let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
    let mut stamp = Vec::new();
    for (entity, mut body) in found {
        let Some(place) = world.get::<Transform>(entity).map(|t| (t.position, t.rotation)) else { continue };
        match body.fall {
            PieceFall::Waiting => {
                // Where it may fall: round it and under it.
                let (low, high) = (place.0 - body.reach - 1.0, place.0 + Vec3::new(body.reach + 1.0, 1.0, body.reach + 1.0));
                if physics.body(entity).is_some() && physics.voxels_ready(body.volume, low, high) {
                    if let Some(rigidbody) = world.get_mut::<Rigidbody>(entity) {
                        rigidbody.kinematic = false;
                    }
                    body.fall = PieceFall::Tipping;
                }
            }
            PieceFall::Tipping => {
                // Made again as a dynamic body: it leans over the way it was cut.
                if let Some(id) = physics.body(entity) {
                    let axis = Vec3::Y.cross(body.toward).normalize_or_zero();
                    physics.world_mut().set_velocity(id, body.toward * 0.2, axis * TIP * std::f32::consts::TAU);
                    body.fall = PieceFall::Falling;
                }
            }
            PieceFall::Falling => {
                body.falling += dt;
                let (linear, angular) = physics.body(entity).map_or((Vec3::ZERO, Vec3::ZERO), |id| physics.world().velocity(id));
                let asleep = physics.body(entity).is_some_and(|id| !physics.world().is_active(id));
                body.still = if asleep || (linear.length() < STILL_SPEED && angular.length() < STILL_SPEED) {
                    body.still + dt
                } else {
                    0.0
                };
                if (body.still >= STILL_FOR && body.falling > 0.5) || body.falling > LONGEST_FALL {
                    body.fall = PieceFall::Resting;
                    let felled = world.get::<VoxelVolume>(body.volume).map(|volume| volume.felled);
                    if felled == Some(VoxelFelled::Stamped) {
                        stamp.push((entity, body.clone(), place));
                    }
                }
            }
            PieceFall::Resting => {}
        }
        if let Some(kept) = world.get_mut::<VoxelPieceBody>(entity) {
            *kept = body;
        }
    }
    world.insert_resource(physics);
    for (entity, body, (position, rotation)) in stamp {
        stamp_back(world, &body, position, rotation);
        world.despawn(entity);
    }
}

/// Stamps a piece back into its volume where it lies, its Obj at
/// `position`, turned `rotation`.
fn stamp_back(world: &mut World, body: &VoxelPieceBody, position: Vec3, rotation: Quat) {
    let Some(corner) = world.get::<Transform>(body.volume).map(|t| t.position) else { return };
    let corner = world.get::<voxear_scene::WorldTransform>(body.volume).map_or(corner, |placed| placed.0.w_axis.truncate());
    let Some(volume) = world.get::<VoxelVolume>(body.volume).cloned() else { return };
    // From metres of the piece's box to metres from the volume's corner.
    let pose = Mat4::from_translation(position - corner) * Mat4::from_quat(rotation) * Mat4::from_translation(-body.middle);
    let mut changes = (*volume.changes).clone();
    voxear_render::voxel_stamp(&volume, &mut changes, &body.piece, pose);
    if let Some(volume) = world.get_mut::<VoxelVolume>(body.volume) {
        volume.changes = Arc::new(changes);
    }
}

/// How long since each endless volume's cut grass last faded and grew a
/// step, in seconds.
#[derive(Default)]
struct CutClocks(std::collections::HashMap<Entity, (f32, f32)>);

/// Moves the cut grass, cover and crops of every endless volume on by
/// `dt` seconds: fading from where they were cut over [`FADE`] seconds,
/// then growing back over the volume's `regrow` seconds, where it has any.
pub fn tend_cuts(world: &mut World, dt: f32) {
    let moving: Vec<(Entity, f32)> = world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .filter(|(_, volume)| volume.endless && volume.changes.cuts_moving(volume.regrow > 0.0))
        .map(|(entity, volume)| (entity, volume.regrow))
        .collect();
    let mut clocks = world.remove_resource::<CutClocks>().unwrap_or_default();
    clocks.0.retain(|entity, _| moving.iter().any(|(e, _)| e == entity));
    for (entity, regrow) in moving {
        let clock = clocks.0.entry(entity).or_default();
        clock.0 += dt;
        clock.1 += dt;
        let fade_every = FADE / FADE_STEPS;
        let grow_every = regrow / GROW_STEPS;
        let fade = if clock.0 >= fade_every { (255.0 / FADE_STEPS).ceil() as u8 } else { 0 };
        let grow = if regrow > 0.0 && clock.1 >= grow_every { (255.0 / GROW_STEPS).ceil() as u8 } else { 0 };
        if fade == 0 && grow == 0 {
            continue;
        }
        if fade > 0 {
            clock.0 = 0.0;
        }
        if grow > 0 {
            clock.1 = 0.0;
        }
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            let mut changes = (*volume.changes).clone();
            // Fading and growing are told apart by the column: a step of each moves only its own.
            if changes.tend_cuts(fade, grow) {
                volume.changes = Arc::new(changes);
            }
        }
    }
    world.insert_resource(clocks);
}

/// The meshes drawn for felled pieces, by their Objs, to let go once those
/// are gone.
#[derive(Default)]
struct PieceMeshes(Vec<(Entity, voxear_render::MeshHandle, Vec<voxear_render::MaterialHandle>)>);

/// Gives each felled piece a mesh of its voxels' cubes to be drawn with,
/// and lets go of the meshes of those gone.
pub fn draw(world: &mut World, renderer: &mut voxear_render::Renderer) {
    let mut meshes = world.remove_resource::<PieceMeshes>().unwrap_or_default();
    meshes.0.retain(|(entity, mesh, materials)| {
        let kept = world.get::<VoxelPieceBody>(*entity).is_some();
        if !kept {
            renderer.free_mesh(*mesh);
            materials.iter().for_each(|material| renderer.free_material(*material));
        }
        kept
    });
    let undrawn: Vec<(Entity, Arc<VoxelPiece>, Vec3)> = world
        .query_ref::<(Entity, &VoxelPieceBody)>()
        .without::<voxear_render::MeshRenderer>()
        .into_iter()
        .map(|(entity, body)| (entity, body.piece.clone(), body.middle))
        .collect();
    for (entity, piece, middle) in undrawn {
        let model = piece_model(&piece, middle);
        let mut mesh = model.mesh();
        mesh.skin.clear();
        let made = renderer.add_mesh(&mesh).and_then(|mesh| Ok((mesh, renderer.upload_look(&model.prepared(""))?)));
        match made {
            Ok((mesh, materials)) => {
                if let Some(&material) = materials.first() {
                    world.insert(entity, voxear_render::MeshRenderer::new(mesh, material));
                }
                meshes.0.push((entity, mesh, materials));
            }
            Err(error) => tracing::warn!("a felled tree's piece could not be drawn: {error:#}"),
        }
    }
    world.insert_resource(meshes);
}

/// A piece as a box model of its voxels' cubes, one bone, in boxes of at
/// most the largest grid a side, its origin at `middle` metres from its
/// box's least corner.
pub fn piece_model(piece: &VoxelPiece, middle: Vec3) -> voxear_render::BoxModel {
    use voxear_render::{BoxModel, LARGEST_GRID, Voxels};
    let per = 1.0 / piece.voxel;
    let mut model = BoxModel { pixels_per_block: per, palette: piece.palette.clone(), ..BoxModel::default() };
    let bone = model.add_bone("Piece", None, Vec3::ZERO);
    let side = LARGEST_GRID as usize;
    let parts = piece.size.map(|a| (a as usize).div_ceil(side).max(1));
    let mut grids: Vec<Option<(Voxels, [u32; 3])>> = vec![None; parts[0] * parts[1] * parts[2]];
    for (c, place) in &piece.voxels {
        let part = [0, 1, 2].map(|a| usize::from(c[a]) / side);
        let index = part[0] + parts[0] * (part[1] + parts[1] * part[2]);
        let size = [0, 1, 2].map(|a| (piece.size[a] as usize - part[a] * side).min(side) as u32);
        let (voxels, _) = grids[index].get_or_insert_with(|| (Voxels::filled(size, 0), size));
        let local = [0, 1, 2].map(|a| (usize::from(c[a]) - part[a] * side) as i32);
        voxels.set(size, local, *place);
    }
    for (index, grid) in grids.into_iter().enumerate() {
        let Some((voxels, size)) = grid else { continue };
        let part = [index % parts[0], (index / parts[0]) % parts[1], index / (parts[0] * parts[1])];
        let from = Vec3::new(part[0] as f32, part[1] as f32, part[2] as f32) * side as f32 - middle * per;
        model.add_box(bone, from, size, [0, 0]).voxels = Some(voxels);
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pieces_model_is_its_voxels_round_its_middle() {
        let piece = VoxelPiece {
            low: voxear_core::math::IVec3::ZERO,
            size: [2, 300, 1],
            voxels: vec![([0, 0, 0], 1), ([1, 299, 0], 2)],
            palette: vec![[100, 70, 50, 255], [70, 120, 40, 255]],
            wood: vec![true, false],
            voxel: 1.0 / 16.0,
        };
        let model = piece_model(&piece, Vec3::new(0.0, 1.0, 0.0));
        // Taller than the largest grid: in two boxes, one over the other.
        let boxes: Vec<_> = model.boxes().map(|(_, _, b)| (b.from, b.size)).collect();
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        assert!(boxes.iter().any(|(from, _)| from.y == -16.0), "the first a metre under its middle: {boxes:?}");
        assert!(!model.mesh().indices.is_empty());
    }
}
