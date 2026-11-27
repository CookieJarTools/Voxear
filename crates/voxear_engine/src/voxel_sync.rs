//! Endless Voxel Volumes in a networked game: what a game changes in them
//! as it plays reaches every player.
//!
//! Every machine makes the scene's own volume. The server is the one that
//! changes it: a client's `Voxels.Dig`, `Voxels.Box`, `Voxels.Print`,
//! `Voxels.Cut` or `Voxels.Blast` is a request ([`VoxelAsk`]), which the
//! server checks and makes as its own scripts' are. What the server's
//! volume became then goes to every player on a reliable channel, each
//! frame it changed:
//!
//! - **edits** it added (shaped edits and prints), each in a few numbers, a
//!   print by its GUID, for every machine to make the same to the voxel;
//! - **what came of them** as a delta of its changes against what players
//!   were last sent: each brick that differs voxel by voxel, each column of
//!   cut grass and each felled tree whole ([`voxear_render::VoxelDelta`]);
//! - **hints** that let a player work out a change itself: a cut through
//!   grass and trees, a blast's dig, cut grass growing back, a piece stamped
//!   back where it rests (at its pose to a millimetre, the server stamping
//!   at the same pose), a felled piece's leaves pressed on the ground. A
//!   hint is sent only where what players have is just what the server's
//!   change started from, so they work out the same and the server takes
//!   its own result as theirs; otherwise the delta carries it. Whatever a
//!   hint does, the delta after it makes every machine's volume the
//!   server's;
//! - **pieces** come loose (felled trees and what was built), each by a
//!   number: a tree's by its place among what the cut made on every
//!   machine, what was built with its voxels; and its pose twenty times a
//!   second, as a delta against the last pose that player said they had.
//!
//! A player joining late is sent the edits added since the scene loaded,
//! then the region files' bytes of each region whose changes differ from
//! the scene's own, nearest them first, within a share of the connection,
//! then the pieces still falling; then what changes from there on.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use voxear_core::math::{IVec3, Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_net::{BitReader, BitWriter};
use voxear_render::{
    VoxelBlast, VoxelChanges, VoxelCut, VoxelDelta, VoxelKind, VoxelMaterial, VoxelPiece, VoxelVolume,
};
use voxear_scene::Transform;

use crate::content::Content;
use crate::levels::Levels;
use crate::net::{self, NetRole, NetSession, PlayerId};
use crate::voxel_felling::{CutPieces, PieceRest, VoxelPieceBody};
use crate::voxel_volumes::{VoxelEditData, VoxelEditShape, VoxelVolumeSource};

/// A volume as its scene made it, before anything the game did: what a
/// player's machine has of it before it joins.
#[derive(Clone, Debug)]
pub struct VoxelVolumeLoaded {
    /// How many edits its scene wrote, and how many of them were made.
    pub edits: usize,
    pub made: usize,
    pub changes: Arc<VoxelChanges>,
}

impl Component for VoxelVolumeLoaded {}

/// A piece's pose in its volume, as it goes over the network: its place in
/// whole millimetres from the volume's corner, and its turn as the three
/// smallest parts of its quaternion in [`TURN_BITS`] bits each, with which
/// was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PiecePose {
    pub at: [i32; 3],
    pub turn: u64,
}

/// Bits a part of a turn takes.
pub const TURN_BITS: u32 = 11;
const TURN_STEPS: u32 = (1 << TURN_BITS) - 1;

impl PiecePose {
    /// A pose to a millimetre, `local` metres from its volume's corner.
    pub fn of(local: Vec3, rotation: Quat) -> Self {
        let at = local.to_array().map(|a| (f64::from(a) * 1000.0).round().clamp(-2.0e9, 2.0e9) as i32);
        let q = rotation.normalize().to_array();
        let largest = (0..4).max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs())).unwrap_or(3);
        let sign = if q[largest] < 0.0 { -1.0 } else { 1.0 };
        let mut turn = largest as u64;
        let mut shift = 2;
        for (i, &c) in q.iter().enumerate() {
            if i != largest {
                let t = ((c * sign + std::f32::consts::FRAC_1_SQRT_2) / std::f32::consts::SQRT_2).clamp(0.0, 1.0);
                turn |= u64::from((t * TURN_STEPS as f32).round() as u32) << shift;
                shift += TURN_BITS;
            }
        }
        Self { at, turn }
    }

    /// Where it is, in metres from its volume's corner.
    pub fn position(&self) -> Vec3 {
        Vec3::from(self.at.map(|a| (f64::from(a) / 1000.0) as f32))
    }

    pub fn rotation(&self) -> Quat {
        let largest = (self.turn & 3) as usize;
        let mut q = [0.0f32; 4];
        let mut sum = 0.0;
        let mut shift = 2;
        for (i, c) in q.iter_mut().enumerate() {
            if i != largest {
                let t = ((self.turn >> shift) & u64::from(TURN_STEPS)) as f32 / TURN_STEPS as f32;
                *c = t * std::f32::consts::SQRT_2 - std::f32::consts::FRAC_1_SQRT_2;
                sum += *c * *c;
                shift += TURN_BITS;
            }
        }
        q[largest] = (1.0 - sum).max(0.0).sqrt();
        Quat::from_array(q).normalize()
    }

    /// From metres of a piece's box, whose Obj's place is `middle` metres
    /// from its least corner, to metres from its volume's corner.
    pub fn matrix(&self, middle: Vec3) -> Mat4 {
        Mat4::from_translation(self.position()) * Mat4::from_quat(self.rotation()) * Mat4::from_translation(-middle)
    }
}

/// What changed a server's volume that a player's machine can work out for
/// itself, from the volume as it was.
#[derive(Clone, Debug)]
pub(crate) enum HintWhat {
    /// A blast dug what was built in a ball.
    Blast(VoxelBlast),
    /// Its cut grass grew back a step.
    Tend { grow: u8, tallest: u8 },
    /// A piece was stamped back into it at a pose, its voxels as they were then.
    Stamp { piece: Entity, pose: PiecePose, shape: Arc<VoxelPiece> },
    /// Grass cut and trees felled, each piece come loose its Obj (none where taken away).
    Cut { cut: VoxelCut, pieces: Vec<Option<Entity>> },
}

/// What a server's game did that every machine works out alike: a change of
/// a volume, with its changes before and after and how many edits it had;
/// or a piece's leaves pressed against the ground where it rests.
#[derive(Clone, Debug)]
pub(crate) enum Hint {
    Changed { volume: Entity, what: HintWhat, before: Arc<VoxelChanges>, after: Arc<VoxelChanges>, edits: usize },
    Pressed { piece: Entity, pose: PiecePose, before: Arc<VoxelPiece>, after: Arc<VoxelPiece>, depth: f32, to: f32 },
}

/// The hints of this frame, kept only on a server.
#[derive(Default)]
pub(crate) struct Hints(Vec<Hint>);

/// Notes a hint, on a server.
pub(crate) fn hint(world: &mut World, hint: Hint) {
    if let Some(hints) = world.resource_mut::<Hints>() {
        hints.0.push(hint);
    }
}

/// Notes that `what` changed `entity`'s volume from `was` to `after`, on a
/// server.
pub(crate) fn hint_change(
    world: &mut World,
    entity: Entity,
    was: &VoxelVolume,
    after: &Arc<VoxelChanges>,
    what: HintWhat,
) {
    if world.resource::<Hints>().is_none() {
        return;
    }
    let (before, edits) = (was.changes.clone(), was.edits.len());
    hint(world, Hint::Changed { volume: entity, what, before, after: after.clone(), edits });
}

/// A request to the server to change its endless volumes, from a player's
/// machine: what a script there asked of `Voxels`, in the level's own
/// places (this machine's world's middle added), applied when the server
/// accepts it.
#[derive(Clone, Debug, PartialEq)]
pub enum VoxelAsk {
    /// A shaped edit, placed in the world.
    Shape(VoxelEditData),
    /// A print, by its GUID or path, placed in the world.
    Print(VoxelEditData),
    /// Voxels dug (none) or placed one at a time.
    Set { points: Vec<Vec3>, to: Option<VoxelKind> },
    /// Voxels let go back to what the ground and edits make.
    Restore { points: Vec<Vec3> },
    /// Grass cut and trees felled.
    Cut { cut: VoxelCut, toward: Option<Vec3>, pieces: CutPieces, rest: Option<PieceRest> },
    /// What was built blown out.
    Blast { at: Vec3, radius: f32, pieces: CutPieces, throw: f32 },
}

/// The most points a request sets, and the largest ball or box it reaches,
/// in metres, unless the game checks requests itself.
pub const MOST_ASKED_POINTS: usize = 4096;
pub const MOST_ASKED_REACH: f32 = 24.0;

/// The game's own check of what players ask of the volumes, on a server:
/// true to make it. See [`set_ask_check`].
pub type AskCheck = Arc<dyn Fn(&World, PlayerId, &VoxelAsk) -> bool + Send + Sync>;

struct AskChecker(AskCheck);

/// Has the server ask `check` of every request a player makes of its
/// volumes before making it, in place of the usual limits
/// ([`MOST_ASKED_POINTS`], [`MOST_ASKED_REACH`]).
pub fn set_ask_check(world: &mut World, check: AskCheck) {
    world.insert_resource(AskChecker(check));
}

impl VoxelAsk {
    /// It moved by `by` metres, as from one machine's world into another's.
    pub fn moved(&self, by: Vec3) -> Self {
        let cut = |cut: &VoxelCut| match cut {
            VoxelCut::Points(points) => VoxelCut::Points(points.iter().map(|p| *p + by).collect()),
            VoxelCut::Ball { at, radius } => VoxelCut::Ball { at: *at + by, radius: *radius },
            VoxelCut::Slice { at, across, radius } => {
                VoxelCut::Slice { at: *at + by, across: *across, radius: *radius }
            }
        };
        match self {
            Self::Shape(data) => Self::Shape(data.moved(by)),
            Self::Print(data) => Self::Print(data.moved(by)),
            Self::Set { points, to } => Self::Set { points: points.iter().map(|p| *p + by).collect(), to: *to },
            Self::Restore { points } => Self::Restore { points: points.iter().map(|p| *p + by).collect() },
            Self::Cut { cut: c, toward, pieces, rest } => {
                Self::Cut { cut: cut(c), toward: *toward, pieces: *pieces, rest: *rest }
            }
            Self::Blast { at, radius, pieces, throw } => {
                Self::Blast { at: *at + by, radius: *radius, pieces: *pieces, throw: *throw }
            }
        }
    }

    /// Whether it keeps within the usual limits.
    fn within_limits(&self) -> bool {
        let small = |size: f32| size.is_finite() && size <= MOST_ASKED_REACH;
        match self {
            Self::Shape(data) | Self::Print(data) => {
                small(data.radius) && data.size.iter().all(|s| small(*s)) && data.points.len() <= 256
            }
            Self::Set { points, .. } | Self::Restore { points } => points.len() <= MOST_ASKED_POINTS,
            Self::Cut { cut, .. } => match cut {
                VoxelCut::Points(points) => points.len() <= MOST_ASKED_POINTS,
                VoxelCut::Ball { radius, .. } | VoxelCut::Slice { radius, .. } => small(*radius),
            },
            Self::Blast { radius, throw, .. } => small(*radius) && throw.is_finite(),
        }
    }

    /// As bytes: a byte for whether the rest is packed with zstd (as many
    /// points are), then what it asks.
    pub fn to_bytes(&self) -> Vec<u8> {
        let plain = self.unpacked_bytes();
        if plain.len() > 128
            && let Ok(packed) = zstd::bulk::compress(&plain, 9)
            && packed.len() + 4 < plain.len()
        {
            let mut out = vec![1];
            out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
            out.extend_from_slice(&packed);
            return out;
        }
        let mut out = vec![0];
        out.extend_from_slice(&plain);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? {
            0 => Self::from_unpacked(&bytes[1..]),
            1 => {
                let length = u32::from_le_bytes(bytes.get(1..5)?.try_into().ok()?) as usize;
                if length > 1 << 20 {
                    return None;
                }
                Self::from_unpacked(&zstd::bulk::decompress(&bytes[5..], length).ok()?)
            }
            _ => None,
        }
    }

    fn unpacked_bytes(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        match self {
            Self::Shape(data) => {
                w.write_bits(0, 3);
                write_edit(&mut w, data);
            }
            Self::Print(data) => {
                w.write_bits(1, 3);
                write_edit(&mut w, data);
            }
            Self::Set { points, to } => {
                w.write_bits(2, 3);
                write_points(&mut w, points);
                w.write_bool(to.is_some());
                if let Some(kind) = to {
                    write_kind(&mut w, kind);
                }
            }
            Self::Restore { points } => {
                w.write_bits(3, 3);
                write_points(&mut w, points);
            }
            Self::Cut { cut, toward, pieces, rest } => {
                w.write_bits(4, 3);
                match cut {
                    VoxelCut::Points(points) => {
                        w.write_bits(0, 2);
                        write_points(&mut w, points);
                    }
                    VoxelCut::Ball { at, radius } => {
                        w.write_bits(1, 2);
                        write_vec(&mut w, *at);
                        w.write_f32(*radius);
                    }
                    VoxelCut::Slice { at, across, radius } => {
                        w.write_bits(2, 2);
                        write_vec(&mut w, *at);
                        write_vec(&mut w, *across);
                        w.write_f32(*radius);
                    }
                }
                w.write_bool(toward.is_some());
                if let Some(toward) = toward {
                    write_vec(&mut w, *toward);
                }
                w.write_bool(*pieces == CutPieces::Gone);
                w.write_bits(rest.map_or(0, |rest| rest as u32 + 1), 2);
            }
            Self::Blast { at, radius, pieces, throw } => {
                w.write_bits(5, 3);
                write_vec(&mut w, *at);
                w.write_f32(*radius);
                w.write_bool(*pieces == CutPieces::Gone);
                w.write_f32(*throw);
            }
        }
        w.finish()
    }

    fn from_unpacked(bytes: &[u8]) -> Option<Self> {
        let mut r = BitReader::new(bytes);
        let ask = match r.read_bits(3)? {
            0 => Self::Shape(read_edit(&mut r)?),
            1 => Self::Print(read_edit(&mut r)?),
            2 => {
                let points = read_points(&mut r)?;
                let to = if r.read_bool()? { Some(read_kind(&mut r)?) } else { None };
                Self::Set { points, to }
            }
            3 => Self::Restore { points: read_points(&mut r)? },
            4 => {
                let cut = match r.read_bits(2)? {
                    0 => VoxelCut::Points(read_points(&mut r)?),
                    1 => VoxelCut::Ball { at: read_vec(&mut r)?, radius: r.read_f32()? },
                    _ => VoxelCut::Slice { at: read_vec(&mut r)?, across: read_vec(&mut r)?, radius: r.read_f32()? },
                };
                let toward = if r.read_bool()? { Some(read_vec(&mut r)?) } else { None };
                let pieces = if r.read_bool()? { CutPieces::Gone } else { CutPieces::Fall };
                let rest = match r.read_bits(2)? {
                    1 => Some(PieceRest::Stay),
                    2 => Some(PieceRest::Stamp),
                    3 => Some(PieceRest::Remove),
                    _ => None,
                };
                Self::Cut { cut, toward, pieces, rest }
            }
            5 => {
                let (at, radius) = (read_vec(&mut r)?, r.read_f32()?);
                let pieces = if r.read_bool()? { CutPieces::Gone } else { CutPieces::Fall };
                Self::Blast { at, radius, pieces, throw: r.read_f32()? }
            }
            _ => return None,
        };
        Some(ask)
    }
}

fn write_vec(w: &mut BitWriter, v: Vec3) {
    v.to_array().into_iter().for_each(|a| w.write_f32(a));
}

fn read_vec(r: &mut BitReader) -> Option<Vec3> {
    Some(Vec3::new(r.read_f32()?, r.read_f32()?, r.read_f32()?))
}

fn write_points(w: &mut BitWriter, points: &[Vec3]) {
    w.write_varint(points.len() as u64);
    points.iter().for_each(|p| write_vec(w, *p));
}

fn read_points(r: &mut BitReader) -> Option<Vec<Vec3>> {
    let count = r.read_varint()? as usize;
    if count > MOST_ASKED_POINTS * 4 {
        return None;
    }
    (0..count).map(|_| read_vec(r)).collect()
}

fn write_kind(w: &mut BitWriter, kind: &VoxelKind) {
    w.write_bits(kind.material as u32, 8);
    w.write_bool(kind.tint.is_some());
    if let Some(tint) = kind.tint {
        w.write_bytes(&tint);
    }
}

fn read_kind(r: &mut BitReader) -> Option<VoxelKind> {
    let material = *VoxelMaterial::ALL.get(r.read_bits(8)? as usize)?;
    let tint = if r.read_bool()? {
        let mut tint = [0u8; 3];
        r.read_bytes(&mut tint)?;
        Some(tint)
    } else {
        None
    };
    Some(VoxelKind { material, tint })
}

fn write_text(w: &mut BitWriter, text: &str) {
    w.write_varint(text.len() as u64);
    w.write_bytes(text.as_bytes());
}

fn read_text(r: &mut BitReader) -> Option<String> {
    let length = r.read_varint()? as usize;
    if length > 4096 {
        return None;
    }
    let mut bytes = vec![0u8; length];
    r.read_bytes(&mut bytes)?;
    String::from_utf8(bytes).ok()
}

fn write_bytes(w: &mut BitWriter, bytes: &[u8]) {
    w.write_varint(bytes.len() as u64);
    w.write_bytes(bytes);
}

fn read_bytes(r: &mut BitReader) -> Option<Vec<u8>> {
    let length = r.read_varint()? as usize;
    if length > r.remaining() / 8 {
        return None;
    }
    let mut bytes = vec![0u8; length];
    r.read_bytes(&mut bytes)?;
    Some(bytes)
}

/// An edit as it goes over the network: a bit for each part that is not as
/// an edit has it unless told, then those parts, numbers exactly as they
/// are (every machine works the same whole numbers out of them).
pub fn write_edit(w: &mut BitWriter, data: &VoxelEditData) {
    let plain = VoxelEditData::default();
    let shape = match data.shape {
        VoxelEditShape::Ball => 0,
        VoxelEditShape::Box => 1,
        VoxelEditShape::Ramp => 2,
        VoxelEditShape::Area => 3,
        VoxelEditShape::Cover => 4,
    };
    w.write_bits(shape, 3);
    let has = [
        data.print.is_some(),
        data.at != plain.at,
        data.radius != plain.radius,
        data.size != plain.size,
        data.rotation != plain.rotation,
        !data.points.is_empty(),
        data.low != 0.0 || data.high != 0.0,
        data.depth != 0.0,
        data.fill.is_some(),
        data.tint.is_some(),
        data.clear != 0.0,
        data.fit,
    ];
    has.iter().for_each(|&h| w.write_bool(h));
    if let Some(print) = &data.print {
        write_text(w, print);
    }
    if has[1] {
        data.at.iter().for_each(|&a| w.write_f32(a));
    }
    if has[2] {
        w.write_f32(data.radius);
    }
    if has[3] {
        data.size.iter().for_each(|&a| w.write_f32(a));
    }
    if has[4] {
        data.rotation.iter().for_each(|&a| w.write_f32(a));
    }
    if has[5] {
        w.write_varint(data.points.len() as u64);
        data.points.iter().flatten().for_each(|&a| w.write_f32(a));
    }
    if has[6] {
        w.write_f32(data.low);
        w.write_f32(data.high);
    }
    if has[7] {
        w.write_f32(data.depth);
    }
    if let Some(fill) = data.fill {
        w.write_bits(fill as u32, 8);
    }
    if let Some(tint) = data.tint {
        w.write_bytes(&tint);
    }
    if has[10] {
        w.write_f32(data.clear);
    }
}

pub fn read_edit(r: &mut BitReader) -> Option<VoxelEditData> {
    let shape = match r.read_bits(3)? {
        0 => VoxelEditShape::Ball,
        1 => VoxelEditShape::Box,
        2 => VoxelEditShape::Ramp,
        3 => VoxelEditShape::Area,
        4 => VoxelEditShape::Cover,
        _ => return None,
    };
    let mut has = [false; 12];
    for h in &mut has {
        *h = r.read_bool()?;
    }
    let mut data = VoxelEditData { shape, fit: has[11], ..VoxelEditData::default() };
    if has[0] {
        data.print = Some(read_text(r)?);
    }
    if has[1] {
        data.at = [r.read_f32()?, r.read_f32()?, r.read_f32()?];
    }
    if has[2] {
        data.radius = r.read_f32()?;
    }
    if has[3] {
        data.size = [r.read_f32()?, r.read_f32()?, r.read_f32()?];
    }
    if has[4] {
        data.rotation = [r.read_f32()?, r.read_f32()?, r.read_f32()?];
    }
    if has[5] {
        let count = r.read_varint()? as usize;
        if count > 4096 {
            return None;
        }
        data.points = (0..count).map(|_| Some([r.read_f32()?, r.read_f32()?])).collect::<Option<_>>()?;
    }
    if has[6] {
        (data.low, data.high) = (r.read_f32()?, r.read_f32()?);
    }
    if has[7] {
        data.depth = r.read_f32()?;
    }
    if has[8] {
        data.fill = Some(*VoxelMaterial::ALL.get(r.read_bits(8)? as usize)?);
    }
    if has[9] {
        let mut tint = [0u8; 3];
        r.read_bytes(&mut tint)?;
        data.tint = Some(tint);
    }
    if has[10] {
        data.clear = r.read_f32()?;
    }
    Some(data)
}

/// A piece's voxels as bytes, packed with zstd: its box's least voxel and
/// size, how large a voxel is, its mass, its palette and which of it is
/// wood, then a bit for each voxel of its box and each set one's colour.
pub fn piece_to_bytes(piece: &VoxelPiece) -> Vec<u8> {
    let mut out = Vec::new();
    piece.low.to_array().iter().for_each(|a| out.extend_from_slice(&a.to_le_bytes()));
    piece.size.iter().for_each(|a| out.extend_from_slice(&a.to_le_bytes()));
    out.extend_from_slice(&piece.voxel.to_le_bytes());
    out.extend_from_slice(&piece.mass.to_le_bytes());
    out.extend_from_slice(&(piece.palette.len() as u16).to_le_bytes());
    for (colour, wood) in piece.palette.iter().zip(piece.wood.iter().chain(std::iter::repeat(&false))) {
        out.extend_from_slice(colour);
        out.push(u8::from(*wood));
    }
    let [x, y, z] = piece.size.map(|a| a as usize);
    let mut bits = vec![0u8; (x * y * z).div_ceil(8)];
    let mut sorted: Vec<&([u16; 3], u8)> = piece.voxels.iter().collect();
    let index = |c: [u16; 3]| usize::from(c[0]) + x * (usize::from(c[1]) + y * usize::from(c[2]));
    sorted.sort_by_key(|(c, _)| index(*c));
    sorted.dedup_by_key(|(c, _)| index(*c));
    for (c, _) in &sorted {
        let i = index(*c);
        bits[i >> 3] |= 1 << (i & 7);
    }
    out.extend_from_slice(&bits);
    out.extend(sorted.iter().map(|(_, place)| *place));
    let packed = zstd::bulk::compress(&out, 9).unwrap_or_default();
    let mut bytes = (out.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(&packed);
    bytes
}

/// The piece in `bytes`, as [`piece_to_bytes`] wrote it.
pub fn piece_from_bytes(bytes: &[u8]) -> Result<VoxelPiece> {
    ensure!(bytes.len() >= 4, "a piece too short");
    let length = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    ensure!(length <= 256 << 20, "a piece too large");
    let out = zstd::bulk::decompress(&bytes[4..], length)?;
    let mut at = 0;
    let mut take = |n: usize| -> Result<&[u8]> {
        let piece = out.get(at..at + n).context("a piece ends early")?;
        at += n;
        Ok(piece)
    };
    let mut word = || -> Result<[u8; 4]> { Ok(take(4)?.try_into()?) };
    let low = IVec3::new(i32::from_le_bytes(word()?), i32::from_le_bytes(word()?), i32::from_le_bytes(word()?));
    let size = [u32::from_le_bytes(word()?), u32::from_le_bytes(word()?), u32::from_le_bytes(word()?)];
    let (voxel, mass) = (f32::from_le_bytes(word()?), f32::from_le_bytes(word()?));
    ensure!(size.iter().all(|&a| a <= u32::from(u16::MAX)), "a piece too large");
    let colours = u16::from_le_bytes(take(2)?.try_into()?) as usize;
    let (mut palette, mut wood) = (Vec::with_capacity(colours), Vec::with_capacity(colours));
    for _ in 0..colours {
        let c = take(5)?;
        palette.push([c[0], c[1], c[2], c[3]]);
        wood.push(c[4] != 0);
    }
    let [x, y, z] = size.map(|a| a as usize);
    let bits = take((x * y * z).div_ceil(8))?.to_vec();
    let mut voxels = Vec::new();
    for i in (0..x * y * z).filter(|i| bits[i >> 3] >> (i & 7) & 1 != 0) {
        voxels.push(([(i % x) as u16, ((i / x) % y) as u16, (i / (x * y)) as u16], 0u8));
    }
    let places = take(voxels.len())?;
    for (voxel, &place) in voxels.iter_mut().zip(places) {
        voxel.1 = place;
    }
    Ok(VoxelPiece { low, size, voxels, palette, wood, voxel, mass })
}

/// A hint as it goes over the network.
#[derive(Clone, Debug, PartialEq)]
enum WireHint {
    Blast(VoxelBlast),
    Tend { grow: u8, tallest: u8 },
    Stamp { piece: u32, pose: PiecePose },
    Cut(VoxelCut),
}

/// One thing the server tells players of its volumes, in order.
#[derive(Clone, Debug, PartialEq)]
enum News {
    /// What a volume's number stands for: its scene's GUID and its Obj's path there.
    Named {
        number: u16,
        scene: String,
        path: String,
    },
    /// Edits added to a volume. On joining (`keep`), the volume first goes back to as its scene made it,
    /// keeping its scene's own edits and changes, and the edits are those added since.
    Edits {
        volume: u16,
        keep: bool,
        edits: Vec<VoxelEditData>,
    },
    /// A region file of a volume's changes on joining, the first of the region's parts letting go of what was
    /// there; one with no bytes lets go alone.
    Region {
        volume: u16,
        first: bool,
        region: [i32; 2],
        bytes: Vec<u8>,
    },
    /// A piece come loose, by its number, or made again where it was before; dug out of its volume (`dug`)
    /// where it came of what was built. Its voxels are its bytes, or the `made`th piece the cut before made.
    Piece {
        id: u32,
        volume: u16,
        middle: Vec3,
        built: bool,
        dug: bool,
        pose: PiecePose,
        made: Option<u16>,
        bytes: Vec<u8>,
    },
    Hint {
        volume: u16,
        hint: WireHint,
    },
    /// A piece's leaves pressed against the ground where it rests at a pose, those `depth` metres over it to `to`
    /// of their height.
    Pressed {
        id: u32,
        pose: PiecePose,
        depth: f32,
        to: f32,
    },
    Delta {
        volume: u16,
        bytes: Vec<u8>,
    },
    Gone {
        id: u32,
    },
    /// Everything there was when they joined is sent.
    Ready,
}

fn write_pose(w: &mut BitWriter, pose: &PiecePose) {
    pose.at.iter().for_each(|&a| w.write_varint(zigzag(a)));
    w.write_bits(pose.turn as u32, 32);
    w.write_bits((pose.turn >> 32) as u32, 2 + 3 * TURN_BITS - 32);
}

fn read_pose(r: &mut BitReader) -> Option<PiecePose> {
    let at = [unzigzag(r.read_varint()?), unzigzag(r.read_varint()?), unzigzag(r.read_varint()?)];
    let low = u64::from(r.read_bits(32)?);
    let high = u64::from(r.read_bits(2 + 3 * TURN_BITS - 32)?);
    Some(PiecePose { at, turn: low | high << 32 })
}

pub(crate) fn zigzag(value: i32) -> u64 {
    u64::from(((value << 1) ^ (value >> 31)) as u32)
}

pub(crate) fn unzigzag(value: u64) -> i32 {
    let value = value as u32;
    (value >> 1) as i32 ^ -((value & 1) as i32)
}

fn write_news(w: &mut BitWriter, news: &News) {
    match news {
        News::Named { number, scene, path } => {
            w.write_bits(0, 3);
            w.write_varint(u64::from(*number));
            write_text(w, scene);
            write_text(w, path);
        }
        News::Edits { volume, keep, edits } => {
            w.write_bits(1, 3);
            w.write_varint(u64::from(*volume));
            w.write_bool(*keep);
            w.write_varint(edits.len() as u64);
            edits.iter().for_each(|edit| write_edit(w, edit));
        }
        News::Region { volume, first, region, bytes } => {
            w.write_bits(2, 3);
            w.write_varint(u64::from(*volume));
            w.write_bool(*first);
            region.iter().for_each(|&a| w.write_varint(zigzag(a)));
            write_bytes(w, bytes);
        }
        News::Piece { id, volume, middle, built, dug, pose, made, bytes } => {
            w.write_bits(3, 3);
            w.write_varint(u64::from(*id));
            w.write_varint(u64::from(*volume));
            write_vec(w, *middle);
            w.write_bool(*built);
            w.write_bool(*dug);
            write_pose(w, pose);
            w.write_varint(made.map_or(0, |made| u64::from(made) + 1));
            write_bytes(w, bytes);
        }
        News::Pressed { id, pose, depth, to } => {
            // After Ready, as a kind of its own.
            w.write_bits(7, 3);
            w.write_bool(true);
            w.write_varint(u64::from(*id));
            write_pose(w, pose);
            w.write_f32(*depth);
            w.write_f32(*to);
        }
        News::Hint { volume, hint } => {
            w.write_bits(4, 3);
            w.write_varint(u64::from(*volume));
            match hint {
                WireHint::Blast(blast) => {
                    w.write_bits(0, 2);
                    write_vec(w, blast.at);
                    w.write_f32(blast.radius);
                }
                WireHint::Tend { grow, tallest } => {
                    w.write_bits(1, 2);
                    w.write_bits(u32::from(*grow), 8);
                    w.write_bits(u32::from(*tallest), 8);
                }
                WireHint::Stamp { piece, pose } => {
                    w.write_bits(2, 2);
                    w.write_varint(u64::from(*piece));
                    write_pose(w, pose);
                }
                WireHint::Cut(cut) => {
                    w.write_bits(3, 2);
                    match cut {
                        VoxelCut::Points(points) => {
                            w.write_bits(0, 2);
                            write_points(w, points);
                        }
                        VoxelCut::Ball { at, radius } => {
                            w.write_bits(1, 2);
                            write_vec(w, *at);
                            w.write_f32(*radius);
                        }
                        VoxelCut::Slice { at, across, radius } => {
                            w.write_bits(2, 2);
                            write_vec(w, *at);
                            write_vec(w, *across);
                            w.write_f32(*radius);
                        }
                    }
                }
            }
        }
        News::Delta { volume, bytes } => {
            w.write_bits(5, 3);
            w.write_varint(u64::from(*volume));
            write_bytes(w, bytes);
        }
        News::Gone { id } => {
            w.write_bits(6, 3);
            w.write_varint(u64::from(*id));
        }
        News::Ready => {
            w.write_bits(7, 3);
            w.write_bool(false);
        }
    }
}

fn read_news(r: &mut BitReader) -> Option<News> {
    let number = |r: &mut BitReader| r.read_varint().and_then(|n| u16::try_from(n).ok());
    let news = match r.read_bits(3)? {
        0 => News::Named { number: number(r)?, scene: read_text(r)?, path: read_text(r)? },
        1 => {
            let (volume, keep) = (number(r)?, r.read_bool()?);
            let count = r.read_varint()? as usize;
            if count > r.remaining() {
                return None;
            }
            let edits = (0..count).map(|_| read_edit(r)).collect::<Option<_>>()?;
            News::Edits { volume, keep, edits }
        }
        2 => {
            let (volume, first) = (number(r)?, r.read_bool()?);
            let region = [unzigzag(r.read_varint()?), unzigzag(r.read_varint()?)];
            News::Region { volume, first, region, bytes: read_bytes(r)? }
        }
        3 => {
            let id = u32::try_from(r.read_varint()?).ok()?;
            let volume = number(r)?;
            let middle = read_vec(r)?;
            let (built, dug) = (r.read_bool()?, r.read_bool()?);
            let pose = read_pose(r)?;
            let made = match r.read_varint()? {
                0 => None,
                made => Some(u16::try_from(made - 1).ok()?),
            };
            News::Piece { id, volume, middle, built, dug, pose, made, bytes: read_bytes(r)? }
        }
        4 => {
            let volume = number(r)?;
            let hint = match r.read_bits(2)? {
                0 => WireHint::Blast(VoxelBlast { at: read_vec(r)?, radius: r.read_f32()? }),
                1 => WireHint::Tend { grow: r.read_bits(8)? as u8, tallest: r.read_bits(8)? as u8 },
                2 => WireHint::Stamp { piece: u32::try_from(r.read_varint()?).ok()?, pose: read_pose(r)? },
                _ => WireHint::Cut(match r.read_bits(2)? {
                    0 => VoxelCut::Points(read_points(r)?),
                    1 => VoxelCut::Ball { at: read_vec(r)?, radius: r.read_f32()? },
                    _ => VoxelCut::Slice { at: read_vec(r)?, across: read_vec(r)?, radius: r.read_f32()? },
                }),
            };
            News::Hint { volume, hint }
        }
        5 => News::Delta { volume: number(r)?, bytes: read_bytes(r)? },
        6 => News::Gone { id: u32::try_from(r.read_varint()?).ok()? },
        _ if r.read_bool()? => {
            let id = u32::try_from(r.read_varint()?).ok()?;
            News::Pressed { id, pose: read_pose(r)?, depth: r.read_f32()?, to: r.read_f32()? }
        }
        _ => News::Ready,
    };
    Some(news)
}

/// About how many bytes one message of news holds at most: more goes in
/// the next.
const MESSAGE_BYTES: usize = 48 * 1024;
/// About how many bytes, before packing, a part of a region holds on
/// joining.
const REGION_PART: usize = 40 * 1024;

/// News as messages, each a count and its news, none much over
/// [`MESSAGE_BYTES`] unless one piece of news is.
fn messages_of(news: &[News]) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let mut part: Vec<Vec<u8>> = Vec::new();
    let mut size = 0;
    let flush = |part: &mut Vec<Vec<u8>>, messages: &mut Vec<Vec<u8>>| {
        if part.is_empty() {
            return;
        }
        let mut w = BitWriter::new();
        w.write_varint(part.len() as u64);
        for one in part.drain(..) {
            w.write_bytes(&one);
        }
        messages.push(w.finish());
    };
    for one in news {
        let mut w = BitWriter::new();
        write_news(&mut w, one);
        // Each piece of news on whole bytes, so they can be gathered.
        let bytes = w.finish();
        if size + bytes.len() > MESSAGE_BYTES && !part.is_empty() {
            flush(&mut part, &mut messages);
            size = 0;
        }
        size += bytes.len();
        let mut framed = BitWriter::new();
        framed.write_varint(bytes.len() as u64);
        framed.write_bytes(&bytes);
        part.push(framed.finish());
    }
    flush(&mut part, &mut messages);
    messages
}

fn news_of(bytes: &[u8]) -> Option<Vec<News>> {
    let mut r = BitReader::new(bytes);
    let count = r.read_varint()? as usize;
    let mut news = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let one = read_bytes(&mut r)?;
        news.push(read_news(&mut BitReader::new(&one))?);
    }
    Some(news)
}

/// The corner of a volume's Obj in the world.
fn corner_of(world: &World, entity: Entity) -> Option<Vec3> {
    let placed = world.get::<voxear_scene::WorldTransform>(entity).map(|t| t.0.w_axis.truncate());
    placed.or_else(|| world.get::<Transform>(entity).map(|t| t.position))
}

// --- The server's part. ---

/// A volume the server shares, by its number, with what players were last
/// sent of it.
struct Shared {
    entity: Entity,
    number: u16,
    scene: String,
    path: String,
    /// How many of its scene's edits were sent.
    edits: usize,
    /// Its changes as players have them.
    changes: Arc<VoxelChanges>,
}

/// A piece the server shares, as players have it.
struct SentPiece {
    id: u32,
    volume: u16,
    piece: Arc<VoxelPiece>,
    middle: Vec3,
    built: bool,
    pose: PiecePose,
}

/// What a server keeps of the volumes it shares.
#[derive(Default)]
struct Serving {
    volumes: Vec<Shared>,
    next_number: u16,
    pieces: HashMap<Entity, SentPiece>,
    next_piece: u32,
}

/// Whether this machine asks the server to change its volumes: a player's
/// machine in a session.
pub fn asks(world: &World) -> bool {
    net::is_client(world)
}

/// Asks the server to change its volumes, from a player's machine, `ask`
/// placed in this machine's world: the server makes it, if it accepts it,
/// and sends what came of it back. Gives whether it was sent.
pub fn ask(world: &mut World, ask: &VoxelAsk) -> bool {
    let origin = voxear_scene::WorldOrigin::of(world).0.as_vec3();
    net::ask_voxels(world, ask.moved(origin).to_bytes())
}

/// On a server or a host, each frame before what it sends goes: makes what
/// players asked of its volumes, then sends what changed in them.
pub(crate) fn serve(content: &mut Content, world: &mut World, levels: &Levels) {
    let serving = world.resource::<NetSession>().is_some_and(|s| matches!(s.role(), NetRole::Server | NetRole::Host));
    if !serving {
        world.remove_resource::<Serving>();
        world.remove_resource::<Hints>();
        return;
    }
    if world.resource::<Hints>().is_none() {
        world.insert_resource(Hints::default());
    }
    make_asks(content, world);
    let mut serving = world.remove_resource::<Serving>().unwrap_or_default();
    let mut news = Vec::new();
    // Volumes new to the session: told whole, as to a player joining.
    let fresh: Vec<(Entity, String, String)> = world
        .query_ref::<(Entity, &VoxelVolume, &VoxelVolumeLoaded)>()
        .into_iter()
        .filter(|(entity, volume, _)| volume.endless && serving.volumes.iter().all(|shared| shared.entity != *entity))
        .filter_map(|(entity, ..)| {
            let (scene, path) = levels.place_of(entity)?;
            Some((entity, scene.to_string(), path.to_string()))
        })
        .collect();
    serving.volumes.retain(|shared| world.has::<VoxelVolume>(shared.entity));
    let mut resynced = Vec::new();
    for (entity, scene, path) in fresh {
        let number = serving.next_number;
        serving.next_number = serving.next_number.wrapping_add(1);
        let Some(volume) = world.get::<VoxelVolume>(entity) else { continue };
        let sent = world.get::<VoxelVolumeSource>(entity).map_or(0, |source| source.0.edits.len());
        let mut shared = Shared { entity, number, scene, path, edits: sent, changes: volume.changes.clone() };
        news.push(News::Named { number, scene: shared.scene.clone(), path: shared.path.clone() });
        news.extend(whole(world, &mut shared, None, true));
        resynced.push(entity);
        serving.volumes.push(shared);
    }
    // Players joining: everything as players have it now, before this frame's news.
    for player in net::take_voxel_joins(world) {
        let toward = net::player_place(world, player);
        let mut joining = Vec::new();
        for shared in &mut serving.volumes {
            joining.push(News::Named { number: shared.number, scene: shared.scene.clone(), path: shared.path.clone() });
            joining.extend(whole(world, shared, toward, false));
        }
        for sent in serving.pieces.values() {
            let bytes = piece_to_bytes(&sent.piece);
            let (id, volume, middle, built, pose) = (sent.id, sent.volume, sent.middle, sent.built, sent.pose);
            joining.push(News::Piece { id, volume, middle, built, dug: false, pose, made: None, bytes });
        }
        joining.push(News::Ready);
        net::owe_voxels(world, player, messages_of(&joining));
    }
    news.extend(this_frame(world, &mut serving, &resynced));
    if !news.is_empty() {
        net::send_voxels(world, messages_of(&news));
    }
    // Where each piece is, for players' snapshots.
    let mut poses = Vec::new();
    for (entity, sent) in &mut serving.pieces {
        let Some(shared) = serving.volumes.iter().find(|shared| shared.number == sent.volume) else { continue };
        let (Some(corner), Some(place)) = (corner_of(world, shared.entity), world.get::<Transform>(*entity)) else {
            continue;
        };
        sent.pose = PiecePose::of(place.position - corner, place.rotation);
        poses.push((sent.id, sent.pose));
    }
    net::set_piece_poses(world, poses);
    world.insert_resource(serving);
}

/// A volume as players have it now, for one joining, or for one new to the
/// session (`fresh`, its changes made the server's own now): its edits
/// since its scene, and the regions of its changes that differ from its
/// scene's, those nearest `toward` first.
fn whole(world: &World, shared: &mut Shared, toward: Option<Vec3>, fresh: bool) -> Vec<News> {
    let Some(loaded) = world.get::<VoxelVolumeLoaded>(shared.entity) else { return Vec::new() };
    if fresh && let Some(volume) = world.get::<VoxelVolume>(shared.entity) {
        shared.changes = volume.changes.clone();
        shared.edits = world.get::<VoxelVolumeSource>(shared.entity).map_or(0, |source| source.0.edits.len());
    }
    let source = world.get::<VoxelVolumeSource>(shared.entity);
    let edits: Vec<VoxelEditData> = source
        .map(|source| source.0.edits.get(loaded.edits.min(shared.edits)..shared.edits).unwrap_or_default().to_vec())
        .unwrap_or_default();
    let volume = shared.number;
    let mut news = vec![News::Edits { volume, keep: true, edits }];
    let mut regions: Vec<[i32; 2]> = shared.changes.region_keys();
    regions.extend(loaded.changes.region_keys());
    regions.sort();
    regions.dedup();
    regions.retain(|&region| {
        shared.changes.region_parts(region, REGION_PART) != loaded.changes.region_parts(region, REGION_PART)
    });
    // Nearest the player first, from the middle of each region.
    let per = world.get::<VoxelVolume>(shared.entity).map_or(16.0, |v| v.voxels_per_metre.max(1) as f32);
    let side = voxear_render::VOXEL_REGION_BRICKS as f32 * 8.0 / per;
    let corner = corner_of(world, shared.entity).unwrap_or_default();
    let toward = toward.unwrap_or(corner);
    let away = |region: &[i32; 2]| {
        let middle = corner + Vec3::new(region[0] as f32 + 0.5, 0.0, region[1] as f32 + 0.5) * side;
        (middle - toward).truncate().length_squared()
    };
    regions.sort_by(|a, b| away(a).total_cmp(&away(b)));
    for region in regions {
        let parts = shared.changes.region_parts(region, REGION_PART);
        if parts.is_empty() {
            news.push(News::Region { volume, first: true, region, bytes: Vec::new() });
        }
        for (k, bytes) in parts.into_iter().enumerate() {
            news.push(News::Region { volume, first: k == 0, region, bytes });
        }
    }
    news
}

/// This frame's news of every shared volume: the edits added, the pieces
/// come loose and made again, the hints, the pieces gone and the delta
/// left; each made on what players have of it, as they will make it.
fn this_frame(world: &mut World, serving: &mut Serving, resynced: &[Entity]) -> Vec<News> {
    let mut news = Vec::new();
    let number_of = |serving: &Serving, entity: Entity| {
        serving.volumes.iter().find(|shared| shared.entity == entity && !resynced.contains(&entity)).map(|s| s.number)
    };
    for shared in serving.volumes.iter_mut().filter(|shared| !resynced.contains(&shared.entity)) {
        let Some(source) = world.get::<VoxelVolumeSource>(shared.entity) else { continue };
        if source.0.edits.len() > shared.edits {
            let edits = source.0.edits[shared.edits..].to_vec();
            shared.edits = source.0.edits.len();
            news.push(News::Edits { volume: shared.number, keep: false, edits });
        }
    }
    let bodies: Vec<(Entity, VoxelPieceBody, Transform)> = world
        .query_ref::<(Entity, &VoxelPieceBody, &Transform)>()
        .into_iter()
        .map(|(entity, body, place)| (entity, body.clone(), *place))
        .collect();
    let pose_of = |world: &World, entity: Entity, volume: Entity| {
        let place = world.get::<Transform>(entity)?;
        Some(PiecePose::of(place.position - corner_of(world, volume)?, place.rotation))
    };
    // Hints, where players' volume is as it was before them: they work out the same, and have the same after.
    let hints = world.resource_mut::<Hints>().map(|hints| std::mem::take(&mut hints.0)).unwrap_or_default();
    for hint in hints {
        match hint {
            Hint::Changed { volume, what, before, after, edits } => {
                let Some(number) = number_of(serving, volume) else { continue };
                let made_edits = world.get::<VoxelVolume>(volume).map(|v| v.edits.len());
                let Some(shared) = serving.volumes.iter_mut().find(|s| s.number == number) else { continue };
                // Otherwise the delta carries what it did.
                if !Arc::ptr_eq(&shared.changes, &before) || made_edits != Some(edits) {
                    continue;
                }
                let mut cut_pieces = Vec::new();
                let wire = match what {
                    HintWhat::Blast(blast) => WireHint::Blast(blast),
                    HintWhat::Tend { grow, tallest } => WireHint::Tend { grow, tallest },
                    HintWhat::Stamp { piece, pose, shape } => {
                        let Some(sent) = serving.pieces.get(&piece).filter(|sent| Arc::ptr_eq(&sent.piece, &shape))
                        else {
                            continue;
                        };
                        WireHint::Stamp { piece: sent.id, pose }
                    }
                    HintWhat::Cut { cut, pieces } => {
                        cut_pieces = pieces;
                        WireHint::Cut(cut)
                    }
                };
                shared.changes = after;
                news.push(News::Hint { volume: number, hint: wire });
                // A cut's pieces, made alike on every machine: by their places among what it made.
                for (made, entity) in cut_pieces.into_iter().enumerate() {
                    let Some(entity) = entity else { continue };
                    let (Some(body), Some(pose)) =
                        (world.get::<VoxelPieceBody>(entity), pose_of(world, entity, volume))
                    else {
                        continue;
                    };
                    let id = serving.next_piece;
                    serving.next_piece = serving.next_piece.wrapping_add(1);
                    let (middle, made) = (body.middle, u16::try_from(made).ok());
                    news.push(News::Piece {
                        id,
                        volume: number,
                        middle,
                        built: false,
                        dug: false,
                        pose,
                        made,
                        bytes: Vec::new(),
                    });
                    let sent = SentPiece { id, volume: number, piece: body.piece.clone(), middle, built: false, pose };
                    serving.pieces.insert(entity, sent);
                }
            }
            Hint::Pressed { piece, pose, before, after, depth, to } => {
                if let Some(sent) = serving.pieces.get_mut(&piece).filter(|sent| Arc::ptr_eq(&sent.piece, &before)) {
                    sent.piece = after;
                    news.push(News::Pressed { id: sent.id, pose, depth, to });
                }
            }
        }
    }
    // Pieces new, and made again as no hint says, with their voxels.
    for (entity, body, place) in &bodies {
        let Some(volume) = number_of(serving, body.volume) else { continue };
        let Some(corner) = corner_of(world, body.volume) else { continue };
        let pose = PiecePose::of(place.position - corner, place.rotation);
        match serving.pieces.get_mut(entity) {
            Some(sent) if Arc::ptr_eq(&sent.piece, &body.piece) => {}
            Some(sent) => {
                sent.piece = body.piece.clone();
                let (id, middle, built) = (sent.id, sent.middle, sent.built);
                let bytes = piece_to_bytes(&body.piece);
                news.push(News::Piece { id, volume, middle, built, dug: false, pose, made: None, bytes });
            }
            None => {
                let id = serving.next_piece;
                serving.next_piece = serving.next_piece.wrapping_add(1);
                let bytes = piece_to_bytes(&body.piece);
                let (middle, built) = (body.middle, body.built);
                news.push(News::Piece { id, volume, middle, built, dug: built, pose, made: None, bytes });
                if built && let Some(shared) = serving.volumes.iter_mut().find(|s| s.number == volume) {
                    dig_piece(Arc::make_mut(&mut shared.changes), &body.piece);
                }
                let sent = SentPiece { id, volume, piece: body.piece.clone(), middle, built, pose };
                serving.pieces.insert(*entity, sent);
            }
        }
    }
    // Pieces gone: stamped, taken away or fallen to dust.
    let gone: Vec<Entity> =
        serving.pieces.keys().copied().filter(|entity| bodies.iter().all(|(e, ..)| e != entity)).collect();
    for entity in gone {
        if let Some(sent) = serving.pieces.remove(&entity) {
            news.push(News::Gone { id: sent.id });
        }
    }
    // What is left: the delta from what players have.
    for shared in serving.volumes.iter_mut().filter(|shared| !resynced.contains(&shared.entity)) {
        let Some(volume) = world.get::<VoxelVolume>(shared.entity) else { continue };
        if Arc::ptr_eq(&volume.changes, &shared.changes) {
            continue;
        }
        let delta = volume.changes.delta_from(&shared.changes);
        shared.changes = volume.changes.clone();
        for part in split(delta) {
            news.push(News::Delta { volume: shared.number, bytes: part.to_bytes() });
        }
    }
    news
}

/// A delta in parts of at most a few thousand bricks.
fn split(delta: VoxelDelta) -> Vec<VoxelDelta> {
    if delta.is_empty() {
        return Vec::new();
    }
    const BRICKS: usize = 1024;
    if delta.bricks.len() <= BRICKS {
        return vec![delta];
    }
    let VoxelDelta { bricks, cuts, felled } = delta;
    let mut parts: Vec<VoxelDelta> =
        bricks.chunks(BRICKS).map(|chunk| VoxelDelta { bricks: chunk.to_vec(), ..VoxelDelta::default() }).collect();
    parts[0].cuts = cuts;
    parts[0].felled = felled;
    parts
}

/// Digs a piece of what was built out of `changes`, where it came loose.
fn dig_piece(changes: &mut VoxelChanges, piece: &VoxelPiece) {
    for (c, _) in &piece.voxels {
        let cell = piece.low + IVec3::new(i32::from(c[0]), i32::from(c[1]), i32::from(c[2]));
        changes.set(cell, None);
    }
}

/// `volume`'s changes after `hint`, its pieces found by number with
/// `piece`.
fn hinted(
    volume: &VoxelVolume,
    hint: &WireHint,
    piece: impl Fn(u32) -> Option<(Arc<VoxelPiece>, Vec3)>,
) -> VoxelChanges {
    match hint {
        WireHint::Blast(blast) => voxear_render::voxel_blast_dig(volume, *blast).0,
        WireHint::Tend { grow, tallest } => {
            let mut changes = (*volume.changes).clone();
            changes.tend_cuts(*grow, *tallest);
            changes
        }
        WireHint::Stamp { piece: id, pose } => {
            let mut changes = (*volume.changes).clone();
            if let Some((piece, middle)) = piece(*id) {
                voxear_render::voxel_stamp(volume, &mut changes, &piece, pose.matrix(middle));
            }
            changes
        }
        WireHint::Cut(cut) => voxear_render::voxel_cut(volume, cut).0,
    }
}

/// Makes what players asked of the volumes, those the game's check (or the
/// usual limits) let through.
fn make_asks(content: &mut Content, world: &mut World) {
    let asked = net::take_voxel_asks(world);
    if asked.is_empty() {
        return;
    }
    let origin = voxear_scene::WorldOrigin::of(world).0.as_vec3();
    let check = world.resource::<AskChecker>().map(|checker| checker.0.clone());
    for (player, bytes) in asked {
        let Some(ask) = VoxelAsk::from_bytes(&bytes) else {
            tracing::warn!("player {} asked something of the voxel volumes that could not be read", player.0);
            continue;
        };
        let ask = ask.moved(-origin);
        let allowed = match &check {
            Some(check) => check(world, player, &ask),
            None => ask.within_limits(),
        };
        if allowed {
            crate::voxel_volumes::make_ask(content, world, &ask);
        }
    }
}

// --- A player's machine's part. ---

/// A piece the server let loose, as a player's machine shows it: a body
/// it moves where the server says, and its voxels as a volume of their own.
#[derive(Clone, Debug)]
pub struct ShownPiece {
    pub id: u32,
    /// The volume it came from.
    pub volume: Entity,
    pub piece: Arc<VoxelPiece>,
    pub middle: Vec3,
    pub look: Entity,
}

impl Component for ShownPiece {}

/// What a player's machine keeps of the server's volumes.
#[derive(Default)]
struct Joined {
    /// Each volume's number, what it was named and this machine's Obj.
    named: HashMap<u16, (String, String, Option<Entity>)>,
    waiting: VecDeque<News>,
    pieces: HashMap<u32, Entity>,
    /// What the last cut made come loose here, for the server's pieces of it to be.
    cut_made: Vec<Arc<VoxelPiece>>,
    ready: bool,
}

/// Whether a player's machine has everything the server's volumes had when
/// it joined.
pub fn is_joined(world: &World) -> bool {
    world.resource::<Joined>().is_some_and(|joined| joined.ready && joined.waiting.is_empty())
}

/// On a player's machine, each frame after what came in: makes what the
/// server's volumes became here, and moves the pieces falling where the
/// server shows them.
pub(crate) fn take_in(content: &mut Content, world: &mut World, levels: &Levels) {
    if !net::is_client(world) {
        world.remove_resource::<Joined>();
        return;
    }
    let mut joined = world.remove_resource::<Joined>().unwrap_or_default();
    for bytes in net::take_voxel_news(world) {
        match news_of(&bytes) {
            Some(news) => joined.waiting.extend(news),
            None => tracing::warn!("news of the server's voxel volumes could not be read"),
        }
    }
    while let Some(news) = joined.waiting.front() {
        let number = match news {
            News::Edits { volume, .. } | News::Region { volume, .. } | News::Piece { volume, .. } => Some(*volume),
            News::Hint { volume, .. } | News::Delta { volume, .. } => Some(*volume),
            _ => None,
        };
        // A volume this machine has not made yet: everything after it waits.
        let entity = match number {
            Some(number) => match volume_here(world, levels, &mut joined, number) {
                Some(entity) => Some(entity),
                None => break,
            },
            None => None,
        };
        let Some(news) = joined.waiting.pop_front() else { break };
        take_news(content, world, &mut joined, news, entity);
    }
    show_pieces(world, &joined);
    world.insert_resource(joined);
}

/// This machine's volume the server numbered `number`, found by its scene
/// and path.
fn volume_here(world: &World, levels: &Levels, joined: &mut Joined, number: u16) -> Option<Entity> {
    let (scene, path, found) = joined.named.get_mut(&number)?;
    if let Some(entity) = found.filter(|entity| world.has::<VoxelVolume>(*entity)) {
        return Some(entity);
    }
    let scene = scene.parse().ok()?;
    let path = crate::prefab::ObjPath::parse(path).ok()?;
    let entity = levels.entity_at(scene, &path).filter(|entity| world.has::<VoxelVolume>(*entity))?;
    *found = Some(entity);
    Some(entity)
}

fn take_news(content: &mut Content, world: &mut World, joined: &mut Joined, news: News, volume: Option<Entity>) {
    match (news, volume) {
        (News::Named { number, scene, path }, _) => drop(joined.named.insert(number, (scene, path, None))),
        (News::Edits { keep, edits, .. }, Some(entity)) => {
            if keep {
                back_to_scene(world, entity);
            }
            for data in edits {
                match data.made(content) {
                    Some(made) => crate::voxel_volumes::keep_edit(world, entity, made, data),
                    None => tracing::warn!("an edit the server made could not be made here"),
                }
            }
        }
        (News::Region { first, region, bytes, .. }, Some(entity)) => {
            let Some(volume) = world.get_mut::<VoxelVolume>(entity) else { return };
            let changes = Arc::make_mut(&mut volume.changes);
            if first {
                changes.clear_region(region);
            }
            if !bytes.is_empty()
                && let Err(error) = changes.read_region(&bytes)
            {
                tracing::warn!("a region of the server's voxel changes could not be read: {error:#}");
            }
        }
        (News::Piece { id, middle, built, dug, pose, made, bytes, .. }, Some(entity)) => {
            let piece = match made {
                Some(made) => match joined.cut_made.get(usize::from(made)) {
                    Some(piece) => piece.clone(),
                    None => return tracing::warn!("a piece the server's cut made did not come loose here"),
                },
                None => match piece_from_bytes(&bytes) {
                    Ok(piece) => Arc::new(piece),
                    Err(error) => return tracing::warn!("a piece the server let loose could not be read: {error:#}"),
                },
            };
            if dug && let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
                dig_piece(Arc::make_mut(&mut volume.changes), &piece);
            }
            match joined.pieces.get(&id).copied().filter(|shown| world.has::<ShownPiece>(*shown)) {
                Some(shown) => crate::voxel_felling::reshape_shown(world, shown, piece),
                None => {
                    let corner = corner_of(world, entity).unwrap_or_default();
                    let place = (corner + pose.position(), pose.rotation());
                    let shown = crate::voxel_felling::spawn_shown(world, entity, piece, middle, place, built);
                    if let Some(look) = world.get::<ShownPiece>(shown).map(|s| s.look)
                        && let Some(kept) = world.get_mut::<ShownPiece>(shown)
                    {
                        (kept.id, kept.look) = (id, look);
                    }
                    joined.pieces.insert(id, shown);
                }
            }
        }
        (News::Hint { hint, .. }, Some(entity)) => {
            let Some(volume) = world.get::<VoxelVolume>(entity).cloned() else { return };
            let piece = |id: u32| {
                let shown = joined.pieces.get(&id)?;
                world.get::<ShownPiece>(*shown).map(|s| (s.piece.clone(), s.middle))
            };
            let changes = match &hint {
                // A cut's pieces come loose here as on the server, for its pieces to be.
                WireHint::Cut(cut) => {
                    let (changes, made) = voxear_render::voxel_cut(&volume, cut);
                    joined.cut_made = made.pieces.into_iter().map(Arc::new).collect();
                    changes
                }
                hint => hinted(&volume, hint, piece),
            };
            if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
                volume.changes = Arc::new(changes);
            }
            // A piece stamped back is let go, its voxels shown a moment longer where it lay.
            if let WireHint::Stamp { piece, pose } = hint
                && let Some(shown) = joined.pieces.remove(&piece)
            {
                let corner = corner_of(world, entity).unwrap_or_default();
                crate::voxel_felling::let_go_shown(world, shown, Some((corner + pose.position(), pose.rotation())));
            }
        }
        (News::Delta { bytes, .. }, Some(entity)) => match VoxelDelta::from_bytes(&bytes) {
            Ok(delta) => {
                if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
                    Arc::make_mut(&mut volume.changes).apply_delta(&delta);
                }
            }
            Err(error) => tracing::warn!("the server's voxel changes could not be read: {error:#}"),
        },
        (News::Pressed { id, pose, depth, to }, _) => {
            let Some(shown) =
                joined.pieces.get(&id).and_then(|shown| world.get::<ShownPiece>(*shown).map(|s| (*shown, s.clone())))
            else {
                return;
            };
            let Some(volume) = world.get::<VoxelVolume>(shown.1.volume) else { return };
            if let Some(pressed) =
                crate::voxel_felling::pressed(volume, &shown.1.piece, pose, shown.1.middle, depth, to)
            {
                crate::voxel_felling::reshape_shown(world, shown.0, Arc::new(pressed));
            }
        }
        (News::Gone { id }, _) => {
            if let Some(shown) = joined.pieces.remove(&id) {
                crate::voxel_felling::let_go_shown(world, shown, None);
            }
        }
        (News::Ready, _) => joined.ready = true,
        (_, None) => {}
    }
}

/// Takes a volume back to as its scene made it: its scene's own edits and
/// changes.
fn back_to_scene(world: &mut World, entity: Entity) {
    let Some(loaded) = world.get::<VoxelVolumeLoaded>(entity).cloned() else { return };
    if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
        let edits = Arc::make_mut(&mut volume.edits);
        edits.truncate(loaded.made);
        volume.changes = loaded.changes.clone();
    }
    if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
        source.0.edits.truncate(loaded.edits);
    }
}

/// Moves each piece shown where the server's snapshots put it, a moment
/// behind.
fn show_pieces(world: &mut World, joined: &Joined) {
    for (id, position, rotation) in net::piece_poses(world) {
        let Some(&shown) = joined.pieces.get(&id) else { continue };
        let Some(volume) = world.get::<ShownPiece>(shown).map(|s| s.volume) else { continue };
        let Some(corner) = corner_of(world, volume) else { continue };
        if let Some(place) = world.get_mut::<Transform>(shown) {
            (place.position, place.rotation) = (corner + position, rotation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pose_goes_to_a_millimetre_and_a_small_turn() {
        let turn = Quat::from_euler(voxear_core::math::EulerRot::YXZ, 1.1, -0.4, 2.9);
        let pose = PiecePose::of(Vec3::new(12.3456, -3.2101, 200.0004), turn);
        assert!((pose.position() - Vec3::new(12.346, -3.210, 200.0)).abs().max_element() < 1e-3);
        assert!(pose.rotation().angle_between(turn) < 0.002, "{}", pose.rotation().angle_between(turn));
        assert_eq!(PiecePose::of(pose.position(), pose.rotation()), pose, "the same again once quantised");
    }

    #[test]
    fn asks_edits_and_pieces_read_back_as_they_were_written() {
        let data = VoxelEditData {
            shape: VoxelEditShape::Area,
            points: vec![[1.0, 2.0], [3.5, -4.0], [0.1, 9.0]],
            low: -2.0,
            high: 6.25,
            fill: Some(VoxelMaterial::Cobbles),
            tint: Some([10, 20, 30]),
            clear: 1.5,
            ..VoxelEditData::default()
        };
        let asks = [
            VoxelAsk::Shape(data.clone()),
            VoxelAsk::Print(VoxelEditData {
                fit: true,
                ..VoxelEditData::printed("abc".into(), [1.0, 2.0, 3.0], [0.0, 90.0, 0.0])
            }),
            VoxelAsk::Set {
                points: vec![Vec3::ONE, Vec3::X],
                to: Some(VoxelKind { material: VoxelMaterial::Bricks, tint: None }),
            },
            VoxelAsk::Restore { points: vec![Vec3::Z] },
            VoxelAsk::Cut {
                cut: VoxelCut::Slice { at: Vec3::ONE, across: Vec3::Y, radius: 0.5 },
                toward: Some(Vec3::X),
                pieces: CutPieces::Fall,
                rest: Some(PieceRest::Stamp),
            },
            VoxelAsk::Blast { at: Vec3::new(4.0, 2.5, 4.0), radius: 1.8, pieces: CutPieces::Fall, throw: 3.0 },
        ];
        for ask in asks {
            assert_eq!(VoxelAsk::from_bytes(&ask.to_bytes()), Some(ask.clone()), "{ask:?}");
        }
        let piece = VoxelPiece {
            low: IVec3::new(-5, 30, 7),
            size: [3, 4, 2],
            voxels: vec![([0, 0, 0], 1), ([2, 3, 1], 2), ([1, 1, 0], 1)],
            palette: vec![[100, 70, 50, 255], [70, 120, 40, 255]],
            wood: vec![true, false],
            voxel: 1.0 / 16.0,
            mass: 12.5,
        };
        let back = piece_from_bytes(&piece_to_bytes(&piece)).unwrap();
        let mut voxels = piece.voxels.clone();
        voxels.sort_by_key(|(c, _)| (c[2], c[1], c[0]));
        assert_eq!(back, VoxelPiece { voxels, ..piece });
        let news = vec![
            News::Named { number: 3, scene: "guid".into(), path: "1/2".into() },
            News::Edits { volume: 3, keep: true, edits: vec![data] },
            News::Hint {
                volume: 3,
                hint: WireHint::Stamp { piece: 9, pose: PiecePose::of(Vec3::ONE, Quat::IDENTITY) },
            },
            News::Gone { id: 9 },
            News::Ready,
        ];
        let messages = messages_of(&news);
        assert_eq!(messages.len(), 1);
        assert_eq!(news_of(&messages[0]), Some(news));
    }
}
