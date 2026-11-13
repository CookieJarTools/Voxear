//! Blocks that are not whole cubes, and what a placed block keeps of
//! itself: which way it faces, which half of its block it is in, whether it
//! is open and whether it is on.
//!
//! A slab, a stair, a fence, a door: each is drawn as a few boxes in
//! sixteenths of a block (its **parts**, [`part_boxes`]), worked out from
//! its shape, its placed state and the blocks beside it. A fence joins the
//! fences beside it and the whole blocks it meets, a pane the panes, and a
//! stair turns a corner where another stair meets it from the side: all
//! read from the blocks round it as its chunk's faces are made, never kept.
//!
//! A placed block's state is one byte ([`BlockState`]). It is part of the
//! block's number: a world numbers each block and state it holds, as it
//! numbers its blocks (see `file::BlockKind::state`), so a world, its saved
//! edits and what is sent of them over the network are as small as they
//! were, and opening a door is one block changed.
//!
//! The boxes of a block that opens, or turns on, are drawn shut (or off),
//! and the parts of them that move are posed by its **motion** (a few
//! keys, [`BlockMotion`]) at its end where it is open: a door turned about
//! its hinge, a lever's handle tilted. While one moves, the renderer poses
//! it by the motion as it goes (see `shaders/block_quads.slang`), from the
//! time it changed, so every player sees it move alike.

use serde::{Deserialize, Serialize};

use super::block::BlockShape;

/// The ways a placed block can face, in the order its state keeps them:
/// north (towards -z), east (+x), south (+z), west (-x), up and down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockFacing {
    #[default]
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl BlockFacing {
    pub const ALL: [BlockFacing; 6] =
        [BlockFacing::North, BlockFacing::East, BlockFacing::South, BlockFacing::West, BlockFacing::Up, BlockFacing::Down];

    /// The four ways round, as a quarter turn at a time takes them.
    pub const AROUND: [BlockFacing; 4] = [BlockFacing::North, BlockFacing::East, BlockFacing::South, BlockFacing::West];

    pub fn name(self) -> &'static str {
        match self {
            BlockFacing::North => "North",
            BlockFacing::East => "East",
            BlockFacing::South => "South",
            BlockFacing::West => "West",
            BlockFacing::Up => "Up",
            BlockFacing::Down => "Down",
        }
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(index: u8) -> Self {
        Self::ALL[usize::from(index % 6)]
    }

    /// One block that way.
    pub fn step(self) -> [i32; 3] {
        match self {
            BlockFacing::North => [0, 0, -1],
            BlockFacing::East => [1, 0, 0],
            BlockFacing::South => [0, 0, 1],
            BlockFacing::West => [-1, 0, 0],
            BlockFacing::Up => [0, 1, 0],
            BlockFacing::Down => [0, -1, 0],
        }
    }

    /// The way along `step`, where it is one block along an axis.
    pub fn of_step(step: [i32; 3]) -> Option<Self> {
        Self::ALL.into_iter().find(|facing| facing.step() == step)
    }

    pub fn opposite(self) -> Self {
        match self {
            BlockFacing::North => BlockFacing::South,
            BlockFacing::East => BlockFacing::West,
            BlockFacing::South => BlockFacing::North,
            BlockFacing::West => BlockFacing::East,
            BlockFacing::Up => BlockFacing::Down,
            BlockFacing::Down => BlockFacing::Up,
        }
    }

    /// Whether it is one of the four ways round, not up or down.
    pub fn is_around(self) -> bool {
        !matches!(self, BlockFacing::Up | BlockFacing::Down)
    }

    /// Turned `turns` quarter turns about the up axis, as a structure turns
    /// (`structure::turn`): north to east, east to south.
    pub fn turned(self, turns: u8) -> Self {
        if !self.is_around() {
            return self;
        }
        Self::AROUND[(usize::from(self.index()) + usize::from(turns % 4)) % 4]
    }

    /// Mirrored along x, as a structure mirrors: east and west swapped.
    pub fn mirrored(self) -> Self {
        match self {
            BlockFacing::East => BlockFacing::West,
            BlockFacing::West => BlockFacing::East,
            other => other,
        }
    }

    /// The way a face facing this way is kept among a chunk's quads (see
    /// `mesh::FACINGS`): +x, -x, +y, -y, +z, -z.
    pub fn side(self) -> usize {
        match self {
            BlockFacing::East => 0,
            BlockFacing::West => 1,
            BlockFacing::Up => 2,
            BlockFacing::Down => 3,
            BlockFacing::South => 4,
            BlockFacing::North => 5,
        }
    }

    /// The way a face kept as `side` among a chunk's quads faces.
    pub fn of_side(side: usize) -> Self {
        [BlockFacing::East, BlockFacing::West, BlockFacing::Up, BlockFacing::Down, BlockFacing::South, BlockFacing::North]
            [side % 6]
    }

    /// The way most along `way`, of the four round when `around`.
    pub fn nearest(way: [f32; 3], around: bool) -> Self {
        let candidates: &[BlockFacing] = if around { &Self::AROUND } else { &Self::ALL };
        let dot = |facing: &BlockFacing| {
            let step = facing.step();
            (0..3).map(|i| step[i] as f32 * way[i]).sum::<f32>()
        };
        *candidates.iter().max_by(|a, b| dot(a).total_cmp(&dot(b))).expect("some way")
    }
}

/// Which half of its block a placed block is in: a stair's or a trap door's
/// lower or upper, a door's lower or upper block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockHalf {
    #[default]
    Lower,
    Upper,
}

impl BlockHalf {
    pub const ALL: [BlockHalf; 2] = [BlockHalf::Lower, BlockHalf::Upper];

    pub fn name(self) -> &'static str {
        match self {
            BlockHalf::Lower => "Lower",
            BlockHalf::Upper => "Upper",
        }
    }
}

/// Which side a door turns on, seen from the side it shuts against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoorHinge {
    #[default]
    Left,
    Right,
}

impl DoorHinge {
    pub const ALL: [DoorHinge; 2] = [DoorHinge::Left, DoorHinge::Right];

    pub fn name(self) -> &'static str {
        match self {
            DoorHinge::Left => "Left",
            DoorHinge::Right => "Right",
        }
    }

    pub fn other(self) -> Self {
        match self {
            DoorHinge::Left => DoorHinge::Right,
            DoorHinge::Right => DoorHinge::Left,
        }
    }
}

/// What a placed block keeps of itself, as one byte: which way it faces
/// (three bits), its half, whether it is open, whether it is on, and which
/// side a door turns on. Each shape keeps only what it uses
/// ([`BlockState::normalized`]), so a block takes as few numbers as it can.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockState {
    pub facing: BlockFacing,
    pub half: BlockHalf,
    pub open: bool,
    pub on: bool,
    pub hinge: DoorHinge,
}

impl BlockState {
    pub fn to_byte(self) -> u8 {
        self.facing.index()
            | u8::from(self.half == BlockHalf::Upper) << 3
            | u8::from(self.open) << 4
            | u8::from(self.on) << 5
            | u8::from(self.hinge == DoorHinge::Right) << 6
    }

    pub fn from_byte(byte: u8) -> Self {
        Self {
            facing: BlockFacing::from_index(byte & 7),
            half: if byte & 8 != 0 { BlockHalf::Upper } else { BlockHalf::Lower },
            open: byte & 16 != 0,
            on: byte & 32 != 0,
            hinge: if byte & 64 != 0 { DoorHinge::Right } else { DoorHinge::Left },
        }
    }

    /// The state a block of `shape` is placed in when nothing says: a slab
    /// lying on the ground, a lever and a button on the floor, the rest
    /// facing north, shut and off.
    pub fn default_for(shape: BlockShape) -> Self {
        let facing = match shape {
            BlockShape::Slab | BlockShape::Lever | BlockShape::Button => BlockFacing::Down,
            _ => BlockFacing::North,
        };
        Self { facing, ..Self::default() }
    }

    /// The same state keeping only what a block of `shape` uses, its facing
    /// one the shape can have.
    pub fn normalized(self, shape: BlockShape) -> Self {
        let around = |facing: BlockFacing| if facing.is_around() { facing } else { BlockFacing::North };
        let blank = Self::default();
        match shape {
            BlockShape::Slab => Self { facing: self.facing, ..blank },
            BlockShape::Stairs => Self { facing: around(self.facing), half: self.half, ..blank },
            BlockShape::Door => Self { facing: around(self.facing), half: self.half, open: self.open, hinge: self.hinge, ..blank },
            BlockShape::TrapDoor => Self { facing: around(self.facing), half: self.half, open: self.open, ..blank },
            BlockShape::FenceGate => Self { facing: around(self.facing), open: self.open, ..blank },
            BlockShape::Lever | BlockShape::Button => Self { facing: self.facing, on: self.on, ..blank },
            BlockShape::Boxes => Self { facing: around(self.facing), open: self.open, ..blank },
            _ => blank,
        }
    }

    /// Turned `turns` quarter turns about the up axis and, first, mirrored
    /// along x where `mirrored` says, as a structure is placed: a door's
    /// hinge goes to its other side in a mirror.
    pub fn turned(self, turns: u8, mirrored: bool) -> Self {
        let facing = if mirrored { self.facing.mirrored() } else { self.facing };
        let hinge = if mirrored { self.hinge.other() } else { self.hinge };
        Self { facing: facing.turned(turns), hinge, ..self }
    }

    /// Whether it is open or on: what its motion's end shows.
    pub fn is_moved(self) -> bool {
        self.open || self.on
    }

    /// The same state open or shut, on or off, as `moved` says, for a block
    /// of `shape`: a lever and a button turn on, the rest open.
    pub fn moved(self, shape: BlockShape, moved: bool) -> Self {
        match shape {
            BlockShape::Lever | BlockShape::Button => Self { on: moved, ..self },
            _ => Self { open: moved, ..self },
        }
    }
}

/// The state a block of `shape` is placed in against a face: `normal` the
/// way the face looks, out of the block it is on, `within` where on the
/// face it was put, from 0 to 1 each way across the block it goes in, and
/// `look` the way whoever puts it down looks. A slab lies against the face
/// it is put on, high or low on a side as it is put; a stair rises away
/// from who puts it down, upside down where put under something or high
/// on a side; a door stands on the far side, its hinge on the side of the
/// face it was put nearer; a trap door hangs on the face it is put on; a
/// lever and a button stand out of it; a block of boxes faces back.
pub fn placed_state(shape: BlockShape, normal: [i32; 3], within: [f32; 3], look: [f32; 3]) -> BlockState {
    let base = BlockState::default_for(shape);
    let against = BlockFacing::of_step(normal).map(BlockFacing::opposite);
    let ahead = BlockFacing::nearest(look, true);
    let high = match against {
        Some(BlockFacing::Up) => true,
        Some(BlockFacing::Down) => false,
        _ => within[1] >= 0.5,
    };
    let half = if high { BlockHalf::Upper } else { BlockHalf::Lower };
    let state = match shape {
        BlockShape::Slab => match against {
            Some(facing) if !facing.is_around() => BlockState { facing, ..base },
            _ => BlockState { facing: if high { BlockFacing::Up } else { BlockFacing::Down }, ..base },
        },
        BlockShape::Stairs => BlockState { facing: ahead, half, ..base },
        BlockShape::Door => {
            // Its hinge on the side, across the way it faces, that it was put nearer.
            let side = ahead.turned(1).step();
            let off = (within[0] - 0.5) * side[0] as f32 + (within[2] - 0.5) * side[2] as f32;
            let hinge = if off > 0.0 { DoorHinge::Right } else { DoorHinge::Left };
            BlockState { facing: ahead, hinge, ..base }
        }
        BlockShape::TrapDoor => match against {
            Some(facing) if facing.is_around() => BlockState { facing, half, ..base },
            _ => BlockState { facing: ahead.opposite(), half, ..base },
        },
        BlockShape::FenceGate => BlockState { facing: ahead, ..base },
        BlockShape::Lever | BlockShape::Button => BlockState { facing: against.unwrap_or(BlockFacing::Down), ..base },
        BlockShape::Boxes => BlockState { facing: ahead.opposite(), ..base },
        _ => base,
    };
    state.normalized(shape)
}

/// The same state turned a quarter about the up axis, or, for a shape that
/// faces six ways, on to its next way: as the brush turns what it puts down.
pub fn turned_on(shape: BlockShape, state: BlockState) -> BlockState {
    let next = match StateUse::of(shape).facings {
        6 => BlockFacing::from_index((state.facing.index() + 1) % 6),
        4 => state.facing.turned(1),
        _ => state.facing,
    };
    BlockState { facing: next, ..state }.normalized(shape)
}

/// What a shape uses of a block's state: which ways it can face (none, the
/// four round, or all six), whether it has halves, opens, turns on, or has
/// a hinge. The Inspector and the brush offer only these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateUse {
    pub facings: u8,
    pub half: bool,
    pub open: bool,
    pub on: bool,
    pub hinge: bool,
}

impl StateUse {
    pub fn of(shape: BlockShape) -> Self {
        let none = Self::default();
        match shape {
            BlockShape::Slab => Self { facings: 6, ..none },
            BlockShape::Stairs => Self { facings: 4, half: true, ..none },
            BlockShape::Door => Self { facings: 4, half: true, open: true, hinge: true, ..none },
            BlockShape::TrapDoor => Self { facings: 4, half: true, open: true, ..none },
            BlockShape::FenceGate => Self { facings: 4, open: true, ..none },
            BlockShape::Lever | BlockShape::Button => Self { facings: 6, on: true, ..none },
            BlockShape::Boxes => Self { facings: 4, open: true, ..none },
            _ => none,
        }
    }

    /// Whether it uses anything of a block's state.
    pub fn any(self) -> bool {
        self != Self::default()
    }
}

/// What a block a player uses does: a door, a trap door and a gate open
/// and shut, a lever turns on and off, a button turns on for a while, and
/// any other usable block only says it was used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UseKind {
    #[default]
    None,
    Opens,
    Toggles,
    Presses,
    Says,
}

impl UseKind {
    /// What using a block of `shape` does, `usable` as its file says;
    /// a block of its own boxes opens where it has a motion.
    pub fn of(shape: BlockShape, usable: bool, moves: bool) -> Self {
        match shape {
            BlockShape::Door | BlockShape::TrapDoor | BlockShape::FenceGate => UseKind::Opens,
            BlockShape::Lever => UseKind::Toggles,
            BlockShape::Button => UseKind::Presses,
            BlockShape::Boxes if usable && moves => UseKind::Opens,
            _ if usable => UseKind::Says,
            _ => UseKind::None,
        }
    }
}

/// How a motion's way from one key to the next goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionEase {
    Linear,
    /// Slow at either end.
    #[default]
    Smooth,
    /// Held, then all at once at the next key.
    Step,
}

impl MotionEase {
    pub const ALL: [MotionEase; 3] = [MotionEase::Linear, MotionEase::Smooth, MotionEase::Step];

    pub fn name(self) -> &'static str {
        match self {
            MotionEase::Linear => "Linear",
            MotionEase::Smooth => "Smooth",
            MotionEase::Step => "Step",
        }
    }

    /// How far along its way it is, from 0 to 1, a share `t` of the time.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            MotionEase::Linear => t,
            MotionEase::Smooth => t * t * (3.0 - 2.0 * t),
            MotionEase::Step => {
                if t >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// Its number, as the renderer reads it.
    pub fn code(self) -> u32 {
        self as u32
    }
}

/// A key of a motion: at `time` seconds, the parts that move turned `turn`
/// degrees about their hinge and moved `shift` sixteenths of a block along
/// it; the way on to the next key goes as `ease` says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionKey {
    pub time: f32,
    #[serde(skip_serializing_if = "is_nothing")]
    pub turn: f32,
    #[serde(rename = "move", skip_serializing_if = "is_nothing")]
    pub shift: f32,
    pub ease: MotionEase,
}

fn is_nothing(value: &f32) -> bool {
    *value == 0.0
}

/// The most keys a motion has.
pub const MOST_KEYS: usize = 8;

/// The longest a motion lasts, in seconds.
pub const LONGEST_MOTION: f32 = 10.0;

/// How a block's moving parts go from shut to open (or off to on): its
/// keys, from its first at 0 to its last, which is how long it lasts.
/// Shutting plays it back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockMotion {
    pub keys: Vec<MotionKey>,
}

impl BlockMotion {
    /// How long it lasts, in seconds: its last key's time.
    pub fn length(&self) -> f32 {
        self.keys.last().map_or(0.0, |key| key.time)
    }

    /// Its keys in order of time, as many as it may have, within its
    /// longest, starting at 0.
    pub fn tidied(mut self) -> Self {
        for key in &mut self.keys {
            key.time = if key.time.is_finite() { key.time.clamp(0.0, LONGEST_MOTION) } else { 0.0 };
            key.turn = if key.turn.is_finite() { key.turn.clamp(-360.0, 360.0) } else { 0.0 };
            key.shift = if key.shift.is_finite() { key.shift.clamp(-16.0, 16.0) } else { 0.0 };
        }
        self.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.keys.truncate(MOST_KEYS);
        if let Some(first) = self.keys.first_mut() {
            first.time = 0.0;
        }
        self
    }

    /// How far its parts are turned, in degrees, and moved, in sixteenths,
    /// `time` seconds in: as its first key before it, as its last after.
    pub fn at(&self, time: f32) -> (f32, f32) {
        let Some(first) = self.keys.first() else { return (0.0, 0.0) };
        if time <= first.time || self.keys.len() == 1 {
            return (first.turn, first.shift);
        }
        for pair in self.keys.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if time <= to.time {
                let span = (to.time - from.time).max(1e-6);
                let share = from.ease.apply((time - from.time) / span);
                return (from.turn + (to.turn - from.turn) * share, from.shift + (to.shift - from.shift) * share);
            }
        }
        let last = self.keys[self.keys.len() - 1];
        (last.turn, last.shift)
    }

    /// Where it rests: at its start, or at its end where `moved`.
    pub fn rest(&self, moved: bool) -> (f32, f32) {
        if moved { self.at(f32::INFINITY) } else { self.at(0.0) }
    }

    /// The motion a block of `shape` has unless its file gives its own: a
    /// door, a gate and a trap door swing a quarter turn, a lever's handle
    /// tips from one side to the other, a button sinks a sixteenth.
    pub fn default_for(shape: BlockShape) -> Option<Self> {
        let key = |time: f32, turn: f32, shift: f32| MotionKey { time, turn, shift, ease: MotionEase::Smooth };
        let keys = match shape {
            BlockShape::Door | BlockShape::FenceGate => vec![key(0.0, 0.0, 0.0), key(0.3, 90.0, 0.0)],
            BlockShape::TrapDoor => vec![key(0.0, 0.0, 0.0), key(0.25, 90.0, 0.0)],
            BlockShape::Lever => vec![key(0.0, -35.0, 0.0), key(0.15, 35.0, 0.0)],
            BlockShape::Button => vec![key(0.0, 0.0, 0.0), key(0.08, 0.0, 1.0)],
            _ => return None,
        };
        Some(Self { keys })
    }
}

/// How long a button stays on once pressed, in seconds, unless its block
/// says.
pub const PRESS_TIME: f32 = 1.0;

/// How far a hanging block swings at most, in degrees, unless its block
/// says, and the most it may.
pub const SWING: f32 = 3.0;
pub const MOST_SWING: f32 = 20.0;

/// What a part that moves turns about: a point in half-sixteenths of a
/// block from its corner (0 to 31 each way), and the way of its axis, which
/// it turns about as a right hand curls, and moves along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hinge {
    pub pivot: [u8; 3],
    pub axis: BlockFacing,
}

/// One box of a block's parts, in sixteenths of a block from its corner,
/// and what it turns about where it moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PartBox {
    pub from: [u8; 3],
    pub to: [u8; 3],
    pub moves: Option<Hinge>,
}

impl PartBox {
    pub const fn new(from: [u8; 3], to: [u8; 3]) -> Self {
        Self { from, to, moves: None }
    }

    pub const fn moving(from: [u8; 3], to: [u8; 3], hinge: Hinge) -> Self {
        Self { from, to, moves: Some(hinge) }
    }

    /// Whether it has room inside it.
    pub fn is_solid(&self) -> bool {
        (0..3).all(|i| self.from[i] < self.to[i])
    }
}

/// A box of a block of its own boxes, as its file says it: from and to in
/// sixteenths of a block, and whether it moves when the block is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoxDef {
    pub from: [u8; 3],
    pub to: [u8; 3],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub moves: bool,
}

/// The most boxes a block of its own boxes has.
pub const MOST_BOXES: usize = 8;

/// A block of its own boxes, as a world draws it: its boxes, how many, and
/// what its moving boxes turn about, all facing north.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CustomShape {
    pub boxes: [PartBox; MOST_BOXES],
    pub count: u8,
}

impl CustomShape {
    /// The shape of `boxes` (at most [`MOST_BOXES`]), those that move turning
    /// about `pivot`, in sixteenths, along `axis`.
    pub fn of(boxes: &[BoxDef], pivot: [f32; 3], axis: BlockFacing) -> Self {
        let mut shape = Self::default();
        let pivot = pivot.map(|v| (v * 2.0).round().clamp(0.0, 31.0) as u8);
        for (at, def) in boxes.iter().take(MOST_BOXES).enumerate() {
            let from = std::array::from_fn(|i| def.from[i].min(def.to[i]).min(16));
            let to = std::array::from_fn(|i| def.from[i].max(def.to[i]).min(16));
            let moves = def.moves.then_some(Hinge { pivot, axis });
            shape.boxes[at] = PartBox { from, to, moves };
            shape.count = at as u8 + 1;
        }
        shape
    }

    pub fn boxes(&self) -> &[PartBox] {
        &self.boxes[..usize::from(self.count)]
    }
}

/// What a world draws of a block as a block of parts, as its file says
/// (see `block::BlockDef::part_look`): whether it is used, how long a
/// button stays on, how far it swings where it hangs, its motion, and a
/// block of its own boxes' boxes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartLook {
    pub usable: bool,
    pub press_time: f32,
    pub swing: f32,
    pub hangs: bool,
    pub motion: Option<BlockMotion>,
    pub custom: Option<CustomShape>,
}

/// Which way a stair turns where another meets it from the side: straight
/// on, round an inner corner (towards the way given) or an outer one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StairCorner {
    #[default]
    Straight,
    Inner(BlockFacing),
    Outer(BlockFacing),
}

/// What a block's parts take from the blocks round it: which of the four
/// sides round it it joins (a bit each, in [`BlockFacing::AROUND`]'s
/// order), and for a stair, its corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Around {
    pub joins: u8,
    pub corner: StairCorner,
}

/// One quarter turn about the up axis, of a point in sixteenths (`side` 16)
/// or half-sixteenths (`side` 32): north to east.
fn quarter(p: [u8; 3], side: u8) -> [u8; 3] {
    [side - p[2].min(side), p[1], p[0]]
}

/// `p`, in sixteenths (`side` 16) or half-sixteenths (32), from a block
/// facing down to one facing `facing`: up turned over, and each way round
/// stood up against its side.
fn from_down(p: [u8; 3], side: u8, facing: BlockFacing) -> [u8; 3] {
    match facing {
        BlockFacing::Down => p,
        BlockFacing::Up => [p[0], side - p[1].min(side), side - p[2].min(side)],
        around => {
            // Down to north: the floor stood up against the north side.
            let north = [p[0], side - p[2].min(side), p[1]];
            turn_point(north, side, around.index())
        }
    }
}

/// A way, from a block facing down to one facing `facing`, as `from_down`
/// takes points.
fn way_from_down(way: BlockFacing, facing: BlockFacing) -> BlockFacing {
    let [x, y, z] = way.step();
    let turned = match facing {
        BlockFacing::Down => [x, y, z],
        BlockFacing::Up => [x, -y, -z],
        around => {
            let [x, y, z] = [x, -z, y];
            let mut p = [x, y, z];
            for _ in 0..around.index() {
                p = [-p[2], p[1], p[0]];
            }
            p
        }
    };
    BlockFacing::of_step(turned).expect("a way along an axis")
}

fn turn_point(mut p: [u8; 3], side: u8, turns: u8) -> [u8; 3] {
    for _ in 0..turns % 4 {
        p = quarter(p, side);
    }
    p
}

/// `part`, made facing north, turned to face `facing` (one of the four
/// round).
fn turn_box(part: PartBox, facing: BlockFacing) -> PartBox {
    let turns = if facing.is_around() { facing.index() } else { 0 };
    let (a, b) = (turn_point(part.from, 16, turns), turn_point(part.to, 16, turns));
    let moves = part.moves.map(|hinge| Hinge { pivot: turn_point(hinge.pivot, 32, turns), axis: hinge.axis.turned(turns) });
    PartBox { from: std::array::from_fn(|i| a[i].min(b[i])), to: std::array::from_fn(|i| a[i].max(b[i])), moves }
}

/// `part`, made facing down, turned to face `facing`.
fn stand_box(part: PartBox, facing: BlockFacing) -> PartBox {
    let (a, b) = (from_down(part.from, 16, facing), from_down(part.to, 16, facing));
    let moves =
        part.moves.map(|hinge| Hinge { pivot: from_down(hinge.pivot, 32, facing), axis: way_from_down(hinge.axis, facing) });
    PartBox { from: std::array::from_fn(|i| a[i].min(b[i])), to: std::array::from_fn(|i| a[i].max(b[i])), moves }
}

/// The boxes a placed block of `shape` is drawn as, in its state, with what
/// it takes from the blocks round it, as they rest shut (or off): those
/// that move are posed by its motion (see [`posed_box`]). `custom` is a
/// block of its own boxes'. None for a whole cube, or a shape drawn
/// otherwise.
pub fn part_boxes(shape: BlockShape, state: BlockState, around: Around, custom: Option<&CustomShape>) -> Vec<PartBox> {
    let b = PartBox::new;
    let facing = state.facing;
    let upper = state.half == BlockHalf::Upper;
    match shape {
        BlockShape::Slab => {
            // Half a block against the side it faces.
            let [mut from, mut to] = [[0u8; 3], [16u8; 3]];
            let axis = match facing {
                BlockFacing::East | BlockFacing::West => 0,
                BlockFacing::Up | BlockFacing::Down => 1,
                _ => 2,
            };
            let far = facing.step()[axis] > 0;
            if far {
                from[axis] = 8;
            } else {
                to[axis] = 8;
            }
            vec![b(from, to)]
        }
        BlockShape::Stairs => {
            let (low, high) = if upper { (8, 16) } else { (0, 8) };
            let (top_low, top_high) = if upper { (0, 8) } else { (8, 16) };
            let mut boxes = vec![b([0, low, 0], [16, high, 16])];
            // The quarters of the other half that stand up: (x, z) as 0 for the low side, 1 for the high.
            let quarter_of = |a: BlockFacing, c: BlockFacing| {
                let step = [a.step(), c.step()];
                let x = if step[0][0] != 0 { step[0][0] } else { step[1][0] };
                let z = if step[0][2] != 0 { step[0][2] } else { step[1][2] };
                (x > 0, z > 0)
            };
            let side = |f: BlockFacing| (f.step()[0], f.step()[2]);
            let mut quarters: Vec<(bool, bool)> = Vec::new();
            let back = side(facing);
            let halves: Vec<(bool, bool)> = [(false, false), (false, true), (true, false), (true, true)]
                .into_iter()
                .filter(|&(x, z)| {
                    let (sx, sz) = (if x { 1 } else { -1 }, if z { 1 } else { -1 });
                    (back.0 != 0 && sx == back.0) || (back.1 != 0 && sz == back.1)
                })
                .collect();
            match around.corner {
                StairCorner::Straight => quarters.extend(halves),
                StairCorner::Outer(other) => quarters.push(quarter_of(facing, other)),
                StairCorner::Inner(other) => {
                    quarters.extend(halves);
                    quarters.push(quarter_of(facing.opposite(), other));
                }
            }
            for (x, z) in quarters {
                let (x0, z0) = (if x { 8 } else { 0 }, if z { 8 } else { 0 });
                boxes.push(b([x0, top_low, z0], [x0 + 8, top_high, z0 + 8]));
            }
            boxes
        }
        BlockShape::Fence => {
            let mut boxes = vec![b([6, 0, 6], [10, 16, 10])];
            for (bit, way) in BlockFacing::AROUND.into_iter().enumerate() {
                if around.joins & (1 << bit) != 0 {
                    boxes.push(turn_box(b([7, 6, 0], [9, 9, 6]), way));
                    boxes.push(turn_box(b([7, 12, 0], [9, 15, 6]), way));
                }
            }
            boxes
        }
        BlockShape::Wall => {
            let straight = around.joins == 0b0101 || around.joins == 0b1010;
            let mut boxes = Vec::new();
            if straight {
                let way = if around.joins == 0b0101 { BlockFacing::North } else { BlockFacing::East };
                boxes.push(turn_box(b([5, 0, 0], [11, 14, 16]), way));
            } else {
                boxes.push(b([4, 0, 4], [12, 16, 12]));
                for (bit, way) in BlockFacing::AROUND.into_iter().enumerate() {
                    if around.joins & (1 << bit) != 0 {
                        boxes.push(turn_box(b([5, 0, 0], [11, 14, 4]), way));
                    }
                }
            }
            boxes
        }
        BlockShape::Pane => {
            let joins = if around.joins == 0 { 0b1111 } else { around.joins };
            let mut boxes = vec![b([7, 0, 7], [9, 16, 9])];
            for (bit, way) in BlockFacing::AROUND.into_iter().enumerate() {
                if joins & (1 << bit) != 0 {
                    boxes.push(turn_box(b([7, 0, 0], [9, 16, 7]), way));
                }
            }
            boxes
        }
        BlockShape::Door => {
            // Against the side it faces, three sixteenths thick, turning on its hinge into the block.
            let (pivot, axis) = match state.hinge {
                DoorHinge::Left => ([3, 16, 3], BlockFacing::Down),
                DoorHinge::Right => ([29, 16, 3], BlockFacing::Up),
            };
            vec![turn_box(PartBox::moving([0, 0, 0], [16, 16, 3], Hinge { pivot, axis }), facing)]
        }
        BlockShape::TrapDoor => {
            // Lying in its half, turning on its hinge along the side it faces, up against that side.
            let (from, to, pivot, axis) = match upper {
                false => ([0, 0, 0], [16, 3, 16], [16, 3, 3], BlockFacing::West),
                true => ([0, 13, 0], [16, 16, 16], [16, 29, 3], BlockFacing::East),
            };
            vec![turn_box(PartBox::moving(from, to, Hinge { pivot, axis }), facing)]
        }
        BlockShape::FenceGate => {
            // Across the way it faces, between two posts, its two leaves each turning on its post into the block.
            let left = Hinge { pivot: [3, 16, 16], axis: BlockFacing::Down };
            let right = Hinge { pivot: [29, 16, 16], axis: BlockFacing::Up };
            let m = PartBox::moving;
            let boxes = [
                b([0, 5, 7], [2, 16, 9]),
                b([14, 5, 7], [16, 16, 9]),
                m([2, 6, 7], [8, 9, 9], left),
                m([2, 12, 7], [8, 15, 9], left),
                m([6, 9, 7], [8, 12, 9], left),
                m([8, 6, 7], [14, 9, 9], right),
                m([8, 12, 7], [14, 15, 9], right),
                m([8, 9, 7], [10, 12, 9], right),
            ];
            boxes.into_iter().map(|part| turn_box(part, facing)).collect()
        }
        BlockShape::Lever => {
            let handle = Hinge { pivot: [16, 2, 16], axis: BlockFacing::East };
            let boxes = [b([5, 0, 4], [11, 3, 12]), PartBox::moving([7, 1, 7], [9, 11, 9], handle)];
            boxes.into_iter().map(|part| stand_box(part, facing)).collect()
        }
        BlockShape::Button => {
            let press = Hinge { pivot: [16, 0, 16], axis: BlockFacing::Down };
            vec![stand_box(PartBox::moving([5, 0, 6], [11, 2, 10], press), facing)]
        }
        BlockShape::Chain => vec![b([5, 0, 7], [11, 16, 9]), b([7, 0, 5], [9, 16, 11])],
        BlockShape::Rope => vec![b([7, 0, 7], [9, 16, 9])],
        BlockShape::Boxes => match custom {
            Some(custom) => custom.boxes().iter().map(|&part| turn_box(part, facing)).collect(),
            None => vec![b([4, 0, 4], [12, 8, 12])],
        },
        BlockShape::Cube | BlockShape::Bushy | BlockShape::Vine => Vec::new(),
    }
}

/// The boxes a placed block collides as besides those it is drawn as: a
/// fence, a wall and a shut gate a block and a half high, so nothing steps
/// or jumps over them, in sixteenths (up to 24).
pub fn tall_boxes(shape: BlockShape, state: BlockState, around: Around) -> Vec<PartBox> {
    let b = PartBox::new;
    let arms = |width: (u8, u8)| -> Vec<PartBox> {
        BlockFacing::AROUND
            .into_iter()
            .enumerate()
            .filter(|(bit, _)| around.joins & (1 << bit) != 0)
            .map(|(_, way)| turn_box(b([width.0, 0, 0], [width.1, 24, 8]), way))
            .collect()
    };
    match shape {
        BlockShape::Fence => [vec![b([6, 0, 6], [10, 24, 10])], arms((6, 10))].concat(),
        BlockShape::Wall => [vec![b([4, 0, 4], [12, 24, 12])], arms((5, 11))].concat(),
        BlockShape::FenceGate if !state.open => vec![turn_box(b([0, 0, 7], [16, 24, 9]), state.facing)],
        _ => Vec::new(),
    }
}

/// Which of a block's six sides its boxes cover whole, a bit each as a
/// chunk keeps its faces (see [`BlockFacing::side`]), whatever is round it:
/// a face of the block beside it against a covered side is never seen.
pub fn covers(shape: BlockShape, state: BlockState, custom: Option<&CustomShape>) -> u8 {
    let boxes = match shape {
        BlockShape::Slab | BlockShape::Boxes => part_boxes(shape, state, Around::default(), custom),
        // A stair's half under (or over) its step, whatever corner it turns.
        BlockShape::Stairs => part_boxes(shape, state, Around::default(), None).into_iter().take(1).collect(),
        _ => return 0,
    };
    let mut covered = 0u8;
    for facing in BlockFacing::ALL {
        let side = facing.side();
        let axis = side / 2;
        let far = side % 2 == 0;
        let whole = boxes.iter().filter(|part| part.moves.is_none()).any(|part| {
            let touches = if far { part.to[axis] == 16 } else { part.from[axis] == 0 };
            touches && (0..3).filter(|&i| i != axis).all(|i| part.from[i] == 0 && part.to[i] == 16)
        });
        covered |= u8::from(whole) << side;
    }
    covered
}

/// Where point `p`, in sixteenths of a block from its corner, is with its
/// part turned `turn` degrees about `hinge` and moved `shift` sixteenths
/// along its axis.
pub fn posed(p: [f32; 3], hinge: Hinge, turn: f32, shift: f32) -> [f32; 3] {
    let pivot = hinge.pivot.map(|v| f32::from(v) * 0.5);
    let axis = hinge.axis.step().map(|v| v as f32);
    let rel = [p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]];
    let (sin, cos) = turn.to_radians().sin_cos();
    // Rodrigues: about a unit axis, turned as a right hand curls.
    let dot = rel[0] * axis[0] + rel[1] * axis[1] + rel[2] * axis[2];
    let cross = [axis[1] * rel[2] - axis[2] * rel[1], axis[2] * rel[0] - axis[0] * rel[2], axis[0] * rel[1] - axis[1] * rel[0]];
    std::array::from_fn(|i| {
        pivot[i] + rel[i] * cos + cross[i] * sin + axis[i] * dot * (1.0 - cos) + axis[i] * shift
    })
}

/// The box a part fills turned `turn` degrees and moved `shift` sixteenths
/// about its hinge, in sixteenths: the box round its eight corners, which
/// is the part itself where it is turned a whole number of quarter turns.
pub fn posed_box(part: &PartBox, turn: f32, shift: f32) -> [[f32; 3]; 2] {
    let (from, to) = (part.from.map(f32::from), part.to.map(f32::from));
    let Some(hinge) = part.moves else { return [from, to] };
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for corner in 0..8 {
        let p = [0, 1, 2].map(|i| if corner >> i & 1 == 0 { from[i] } else { to[i] });
        let q = posed(p, hinge, turn, shift);
        for i in 0..3 {
            low[i] = low[i].min(q[i]);
            high[i] = high[i].max(q[i]);
        }
    }
    [low, high]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_is_one_byte_and_keeps_only_what_its_shape_uses() {
        for byte in (0..128u8).filter(|byte| byte & 7 < 6) {
            assert_eq!(BlockState::from_byte(byte).to_byte(), byte);
        }
        let door = BlockState { facing: BlockFacing::West, half: BlockHalf::Upper, open: true, on: true, hinge: DoorHinge::Right };
        assert_eq!(BlockState::from_byte(door.to_byte()), door);
        let kept = door.normalized(BlockShape::Door);
        assert!(kept.open && !kept.on && kept.hinge == DoorHinge::Right);
        assert_eq!(door.normalized(BlockShape::Fence), BlockState::default(), "a fence keeps nothing");
        assert_eq!(BlockState { facing: BlockFacing::Up, ..door }.normalized(BlockShape::Stairs).facing, BlockFacing::North);
        assert_eq!(BlockState::default_for(BlockShape::Slab).facing, BlockFacing::Down);
    }

    #[test]
    fn a_state_turns_and_mirrors_as_a_structure_does() {
        let door = BlockState { facing: BlockFacing::North, hinge: DoorHinge::Left, ..BlockState::default() };
        assert_eq!(door.turned(1, false).facing, BlockFacing::East);
        assert_eq!(door.turned(3, false).facing, BlockFacing::West);
        let mirrored = BlockState { facing: BlockFacing::East, ..door }.turned(0, true);
        assert_eq!((mirrored.facing, mirrored.hinge), (BlockFacing::West, DoorHinge::Right));
        assert_eq!(BlockState { facing: BlockFacing::Up, ..door }.turned(2, false).facing, BlockFacing::Up);
        // As the structure's own turn takes a step east to south.
        let step = BlockFacing::East.turned(1).step();
        assert_eq!([step[0], step[2]], [0, 1]);
    }

    #[test]
    fn slabs_and_stairs_are_half_blocks_and_cover_the_sides_they_fill() {
        let slab = |facing| part_boxes(BlockShape::Slab, BlockState { facing, ..BlockState::default() }, Around::default(), None);
        assert_eq!(slab(BlockFacing::Down), vec![PartBox::new([0, 0, 0], [16, 8, 16])]);
        assert_eq!(slab(BlockFacing::Up), vec![PartBox::new([0, 8, 0], [16, 16, 16])]);
        assert_eq!(slab(BlockFacing::East), vec![PartBox::new([8, 0, 0], [16, 16, 16])]);
        assert_eq!(slab(BlockFacing::North), vec![PartBox::new([0, 0, 0], [16, 16, 8])]);
        let lower = BlockState::default_for(BlockShape::Slab);
        assert_eq!(covers(BlockShape::Slab, lower, None), 1 << BlockFacing::Down.side());
        // A stair facing north: its step's back on the north side, its base covering the bottom.
        let stair = BlockState::default();
        let boxes = part_boxes(BlockShape::Stairs, stair, Around::default(), None);
        assert_eq!(boxes.len(), 3);
        assert!(boxes[1..].iter().all(|part| part.from[1] == 8 && part.to[2] == 8), "{boxes:?}");
        assert_eq!(covers(BlockShape::Stairs, stair, None), 1 << BlockFacing::Down.side());
        let outer = Around { corner: StairCorner::Outer(BlockFacing::East), ..Around::default() };
        let corner = part_boxes(BlockShape::Stairs, stair, outer, None);
        assert_eq!(corner[1..], [PartBox::new([8, 8, 0], [16, 16, 8])]);
        let inner = Around { corner: StairCorner::Inner(BlockFacing::East), ..Around::default() };
        assert_eq!(part_boxes(BlockShape::Stairs, stair, inner, None).len(), 4);
        let upside = BlockState { half: BlockHalf::Upper, ..stair };
        assert_eq!(part_boxes(BlockShape::Stairs, upside, Around::default(), None)[0], PartBox::new([0, 8, 0], [16, 16, 16]));
    }

    #[test]
    fn fences_walls_and_panes_reach_to_what_they_join() {
        let east = Around { joins: 1 << 1, ..Around::default() };
        let fence = part_boxes(BlockShape::Fence, BlockState::default(), east, None);
        assert_eq!(fence.len(), 3);
        assert!(fence[1..].iter().all(|part| part.from[0] == 10 && part.to[0] == 16), "{fence:?}");
        let tall = tall_boxes(BlockShape::Fence, BlockState::default(), east);
        assert!(tall.iter().all(|part| part.to[1] == 24));
        let straight = Around { joins: 0b0101, ..Around::default() };
        assert_eq!(part_boxes(BlockShape::Wall, BlockState::default(), straight, None), vec![PartBox::new([5, 0, 0], [11, 14, 16])]);
        assert_eq!(part_boxes(BlockShape::Pane, BlockState::default(), Around::default(), None).len(), 5, "alone, a cross");
        assert_eq!(covers(BlockShape::Pane, BlockState::default(), None), 0);
    }

    #[test]
    fn a_door_swings_into_its_block_on_its_hinge_and_a_lever_tips_over() {
        let shut = BlockState::default();
        let [door] = part_boxes(BlockShape::Door, shut, Around::default(), None)[..] else { panic!() };
        assert_eq!((door.from, door.to), ([0, 0, 0], [16, 16, 3]));
        let open = posed_box(&door, 90.0, 0.0);
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3);
        assert!(near(open[0], [0.0, 0.0, 0.0]) && near(open[1], [3.0, 16.0, 16.0]), "against the west side: {open:?}");
        let right = BlockState { hinge: DoorHinge::Right, ..shut };
        let [door] = part_boxes(BlockShape::Door, right, Around::default(), None)[..] else { panic!() };
        let open = posed_box(&door, 90.0, 0.0);
        assert!(near(open[0], [13.0, 0.0, 0.0]) && near(open[1], [16.0, 16.0, 16.0]), "against the east side: {open:?}");
        // Facing east, the same door is against the east side and opens onto the north.
        let east = BlockState { facing: BlockFacing::East, ..shut };
        let [door] = part_boxes(BlockShape::Door, east, Around::default(), None)[..] else { panic!() };
        assert_eq!((door.from, door.to), ([13, 0, 0], [16, 16, 16]));
        let open = posed_box(&door, 90.0, 0.0);
        assert!(near(open[0], [0.0, 0.0, 0.0]) && near(open[1], [16.0, 16.0, 3.0]), "{open:?}");
        // A trap door stands up against its hinge's side.
        let [trap] = part_boxes(BlockShape::TrapDoor, shut, Around::default(), None)[..] else { panic!() };
        let up = posed_box(&trap, 90.0, 0.0);
        assert!(near(up[0], [0.0, 0.0, 0.0]) && near(up[1], [16.0, 16.0, 3.0]), "{up:?}");
        let high = BlockState { half: BlockHalf::Upper, ..shut };
        let [trap] = part_boxes(BlockShape::TrapDoor, high, Around::default(), None)[..] else { panic!() };
        let down = posed_box(&trap, 90.0, 0.0);
        assert!(near(down[0], [0.0, 0.0, 0.0]) && near(down[1], [16.0, 16.0, 3.0]), "{down:?}");
        // A lever on the floor, on a wall facing north and on a ceiling: its handle stands out of what holds it.
        for (facing, out) in [(BlockFacing::Down, [0, 1, 0]), (BlockFacing::North, [0, 0, 1]), (BlockFacing::Up, [0, -1, 0])] {
            let lever = part_boxes(BlockShape::Lever, BlockState { facing, ..shut }, Around::default(), None);
            let (base, handle) = (lever[0], lever[1]);
            let reach = |part: &PartBox| (0..3).map(|i| (f32::from(part.from[i]) + f32::from(part.to[i])) * out[i] as f32).sum::<f32>();
            assert!(reach(&handle) > reach(&base), "{facing:?}: {lever:?}");
            let hinge = handle.moves.unwrap();
            assert!(hinge.pivot.iter().all(|&v| v <= 31));
        }
    }

    #[test]
    fn a_block_put_down_faces_as_who_puts_it_down_looks_and_where_it_goes() {
        let looking_north = [0.0, -0.3, -1.0];
        // A slab on top of a block lies low; under one, high; on a side, as high as it was put.
        let up = [0, 1, 0];
        assert_eq!(placed_state(BlockShape::Slab, up, [0.5, 0.0, 0.5], looking_north).facing, BlockFacing::Down);
        assert_eq!(placed_state(BlockShape::Slab, [0, -1, 0], [0.5, 1.0, 0.5], looking_north).facing, BlockFacing::Up);
        assert_eq!(placed_state(BlockShape::Slab, [0, 0, 1], [0.5, 0.8, 0.0], looking_north).facing, BlockFacing::Up);
        // A stair rises away from who puts it; a lever stands out of the wall it is put on.
        assert_eq!(placed_state(BlockShape::Stairs, up, [0.5; 3], looking_north).facing, BlockFacing::North);
        assert_eq!(placed_state(BlockShape::Lever, [0, 0, 1], [0.5; 3], looking_north).facing, BlockFacing::North);
        // A door's hinge goes to the side it was put nearer.
        let left = placed_state(BlockShape::Door, up, [0.2, 0.0, 0.5], looking_north);
        let right = placed_state(BlockShape::Door, up, [0.8, 0.0, 0.5], looking_north);
        assert_eq!((left.hinge, right.hinge), (DoorHinge::Left, DoorHinge::Right));
        // Turned on: a quarter for one that faces round, the next of six for a slab.
        assert_eq!(turned_on(BlockShape::Stairs, BlockState::default()).facing, BlockFacing::East);
        let slab = BlockState::default_for(BlockShape::Slab);
        assert_eq!(turned_on(BlockShape::Slab, slab).facing, BlockFacing::North);
    }

    #[test]
    fn a_motion_goes_through_its_keys_as_each_eases() {
        let door = BlockMotion::default_for(BlockShape::Door).unwrap();
        assert_eq!(door.length(), 0.3);
        assert_eq!(door.at(0.0), (0.0, 0.0));
        assert_eq!(door.at(1.0), (90.0, 0.0));
        let (half, _) = door.at(0.15);
        assert!((half - 45.0).abs() < 1e-3, "smooth is halfway at half the time");
        let (early, _) = door.at(0.03);
        assert!(early < 9.0, "and slow at first: {early}");
        let stepped = BlockMotion {
            keys: vec![MotionKey { time: 0.0, turn: 0.0, shift: 0.0, ease: MotionEase::Step }, MotionKey { time: 1.0, turn: 10.0, ..MotionKey::default() }],
        };
        assert_eq!(stepped.at(0.99).0, 0.0);
        let messy = BlockMotion { keys: vec![MotionKey { time: 2.0, turn: 5.0, ..MotionKey::default() }, MotionKey { time: 0.5, ..MotionKey::default() }] };
        let tidy = messy.tidied();
        assert_eq!((tidy.keys[0].time, tidy.keys[1].time), (0.0, 2.0));
        let text = toml::to_string(&door).unwrap();
        assert_eq!(toml::from_str::<BlockMotion>(&text).unwrap(), door);
    }
}
