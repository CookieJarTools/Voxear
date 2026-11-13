//! Blocks a player uses: doors, trap doors and gates open and shut, a lever
//! turns on and off, a button turns on for a moment; any other block marked
//! usable only says it was used, for a game's scripts to answer.
//!
//! Using a block changes its state (see `shape::BlockState`), which is part
//! of its number, so what using it asks for is each block to change and the
//! state it takes ([`StateChange`]): the world numbers a state it has not
//! held yet first. A door is used whole: both its blocks, and the door
//! beside it hung on the other side, as a pair of double doors is.
//!
//! A lever or a button drives the blocks it is linked to ([`Links`]): on, a
//! linked door opens; off, it shuts. Links are kept with the world's saved
//! edits, as small as can be.
//!
//! While a block moves, the world keeps when it started ([`MovingBlock`]),
//! by its own clock, and the renderer poses it by its motion from then; a
//! pressed button is kept with when it turns off again.

use std::collections::BTreeMap;

use super::{
    BlockFaces, BlockMap, PART_USED,
    block::BlockShape,
    chunk::{AIR, BlockId},
    shape::{BlockFacing, BlockHalf, BlockState, DoorHinge, UseKind},
};

/// A block changing state: where it is, the first number of its block (see
/// `BlockFaces::family`), the state it takes as a byte, and whether it
/// moves there by its motion or is put there at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateChange {
    pub at: [i32; 3],
    pub family: BlockId,
    pub state: u8,
    pub animate: bool,
}

/// A block moving by its motion: where it is, when it started by its
/// world's clock, how long its motion lasts, and whether it is opening (or
/// turning on) rather than shutting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovingBlock {
    pub at: [i32; 3],
    pub started: f64,
    pub length: f32,
    pub opening: bool,
}

/// A button pressed: where it is, and when it turns off again by its
/// world's clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pressed {
    pub at: [i32; 3],
    pub until: f64,
}

/// A world's links: from each lever or button to the blocks it drives.
pub type Links = BTreeMap<[i32; 3], Vec<[i32; 3]>>;

/// How far a lever or a button drives a block, in blocks each way.
pub const LINK_REACH: i32 = 64;

/// The most blocks one lever or button drives.
pub const MOST_LINKED: usize = 64;

/// What using a block does: what kind of use it is, whether it is open (or
/// on) after, and every block that changes state for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Using {
    pub kind: UseKind,
    pub now: bool,
    pub changes: Vec<StateChange>,
}

/// What using the block at `at` of `map` does, its blocks drawn as `faces`
/// says, with the world's `links`; `None` where it is no block a player
/// uses. A button already on is pressed again for nothing.
pub fn use_at(map: &BlockMap, faces: &[BlockFaces], links: &Links, at: [i32; 3]) -> Option<Using> {
    let id = map.get(at);
    let kind = use_kind(faces, id);
    let state = state_of(faces, id)?;
    match kind {
        UseKind::None => None,
        UseKind::Says => Some(Using { kind, now: false, changes: Vec::new() }),
        UseKind::Opens => {
            let now = !state.open;
            Some(Using { kind, now, changes: set_moved(map, faces, at, now) })
        }
        UseKind::Toggles | UseKind::Presses => {
            if kind == UseKind::Presses && state.on {
                return Some(Using { kind, now: true, changes: Vec::new() });
            }
            let now = !state.on;
            let mut changes = set_moved(map, faces, at, now);
            changes.extend(driven(map, faces, links, at, now));
            Some(Using { kind, now, changes })
        }
    }
}

/// The changes that drive every block linked from `from` open (or on) as
/// `now` says.
pub fn driven(map: &BlockMap, faces: &[BlockFaces], links: &Links, from: [i32; 3], now: bool) -> Vec<StateChange> {
    let mut changes: Vec<StateChange> = Vec::new();
    for &to in links.get(&from).into_iter().flatten() {
        if to == from || changes.iter().any(|change| change.at == to) {
            continue;
        }
        let id = map.get(to);
        if matches!(use_kind(faces, id), UseKind::Opens | UseKind::Toggles | UseKind::Presses) {
            let more: Vec<StateChange> = set_moved(map, faces, to, now);
            for change in more {
                if changes.iter().all(|c| c.at != change.at) {
                    changes.push(change);
                }
            }
        }
    }
    changes
}

/// What using block `id` does.
pub fn use_kind(faces: &[BlockFaces], id: BlockId) -> UseKind {
    let Some(kind) = faces.get(usize::from(id)).filter(|kind| id != AIR && !kind.water) else { return UseKind::None };
    if kind.part & PART_USED == 0 {
        return UseKind::None;
    }
    UseKind::of(kind.shape, true, kind.motion > 0)
}

/// Block `id`'s state.
pub fn state_of(faces: &[BlockFaces], id: BlockId) -> Option<BlockState> {
    faces.get(usize::from(id)).filter(|_| id != AIR).map(|kind| BlockState::from_byte(kind.state))
}

/// The changes that put the block at `at` open (or on) as `moved` says,
/// and with a door, both its blocks and its pair's: none for a block
/// already so.
pub fn set_moved(map: &BlockMap, faces: &[BlockFaces], at: [i32; 3], moved: bool) -> Vec<StateChange> {
    let mut changes = Vec::new();
    for place in door_group(map, faces, at) {
        let id = map.get(place);
        let (Some(kind), Some(state)) = (faces.get(usize::from(id)), state_of(faces, id)) else { continue };
        let next = state.moved(kind.shape, moved);
        if next != state {
            changes.push(StateChange { at: place, family: kind.family, state: next.to_byte(), animate: true });
        }
    }
    changes
}

fn step(at: [i32; 3], way: BlockFacing) -> [i32; 3] {
    let s = way.step();
    [at[0] + s[0], at[1] + s[1], at[2] + s[2]]
}

/// The blocks used together with the one at `at`: a door's other block,
/// and the door hung beside it on the other side with its other block; the
/// block alone for anything else.
pub fn door_group(map: &BlockMap, faces: &[BlockFaces], at: [i32; 3]) -> Vec<[i32; 3]> {
    let id = map.get(at);
    let Some(kind) = faces.get(usize::from(id)).copied() else { return vec![at] };
    let Some(state) = state_of(faces, id) else { return vec![at] };
    if kind.shape != BlockShape::Door || kind.water {
        return vec![at];
    }
    let is_door = |place: [i32; 3], hinge: DoorHinge, half: BlockHalf| {
        let other = map.get(place);
        let theirs = state_of(faces, other);
        faces.get(usize::from(other)).is_some_and(|o| o.family == kind.family && o.shape == BlockShape::Door)
            && theirs.is_some_and(|s| s.facing == state.facing && s.hinge == hinge && s.half == half)
    };
    let other_half = |place: [i32; 3], hinge: DoorHinge, half: BlockHalf| {
        let (way, other) = match half {
            BlockHalf::Lower => (BlockFacing::Up, BlockHalf::Upper),
            BlockHalf::Upper => (BlockFacing::Down, BlockHalf::Lower),
        };
        let next = step(place, way);
        is_door(next, hinge, other).then_some(next)
    };
    let mut group = vec![at];
    group.extend(other_half(at, state.hinge, state.half));
    // The pair hangs on the far side from this door's hinge, on its own other side.
    let away = state.facing.turned(if state.hinge == DoorHinge::Left { 1 } else { 3 });
    let pair = step(at, away);
    let hinge = state.hinge.other();
    if is_door(pair, hinge, state.half) {
        group.push(pair);
        group.extend(other_half(pair, hinge, state.half));
    }
    group
}

/// The other block of the door at `at`, where it has one: the block over a
/// lower one, or under an upper one, of the same door.
pub fn other_half(map: &BlockMap, faces: &[BlockFaces], at: [i32; 3]) -> Option<[i32; 3]> {
    let id = map.get(at);
    let kind = faces.get(usize::from(id)).filter(|kind| kind.shape == BlockShape::Door && !kind.water)?;
    let state = state_of(faces, id)?;
    let (way, half) = match state.half {
        BlockHalf::Lower => (BlockFacing::Up, BlockHalf::Upper),
        BlockHalf::Upper => (BlockFacing::Down, BlockHalf::Lower),
    };
    let next = step(at, way);
    let other = map.get(next);
    let theirs = state_of(faces, other)?;
    let same = faces.get(usize::from(other)).is_some_and(|o| o.family == kind.family);
    (same && theirs.facing == state.facing && theirs.hinge == state.hinge && theirs.half == half).then_some(next)
}

/// The state of a door's upper block over a lower one in `state`.
pub fn upper_door(state: BlockState) -> BlockState {
    BlockState { half: BlockHalf::Upper, ..state }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{
        block::BlockDef,
        file::{BlockFile, BlockKind},
    };

    /// A world numbering, after stone, a door shut and open in each hinge
    /// and half, facing north, a lever off and on, and a button off and on.
    fn world() -> (BlockFile, Vec<BlockFaces>) {
        let mut file = BlockFile { blocks: vec![BlockKind::colored("Stone", [0.5; 3])], ..BlockFile::default() };
        let mut add = |reference: &str, shape: BlockShape, state: BlockState| {
            let mut kind = BlockKind::of_state(reference, state.normalized(shape).to_byte());
            BlockDef { shape, ..BlockDef::colored([0.5; 3]) }.fill(&mut kind);
            file.blocks.push(kind);
        };
        for hinge in DoorHinge::ALL {
            for half in BlockHalf::ALL {
                for open in [false, true] {
                    add("Blocks/Door.block", BlockShape::Door, BlockState { hinge, half, open, ..BlockState::default() });
                }
            }
        }
        let lever = BlockState::default_for(BlockShape::Lever);
        add("Blocks/Lever.block", BlockShape::Lever, lever);
        add("Blocks/Lever.block", BlockShape::Lever, BlockState { on: true, ..lever });
        let faces = file.faces().1;
        (file, faces)
    }

    fn number(faces: &[BlockFaces], family: BlockId, state: u8) -> BlockId {
        faces.iter().position(|kind| kind.family == family && kind.state == state).expect("numbered") as BlockId
    }

    fn door(faces: &[BlockFaces], hinge: DoorHinge, half: BlockHalf, open: bool) -> BlockId {
        let state = BlockState { hinge, half, open, ..BlockState::default() }.normalized(BlockShape::Door);
        number(faces, 2, state.to_byte())
    }

    #[test]
    fn a_double_door_opens_whole_and_a_lever_opens_the_doors_it_is_linked_to() {
        let (_, faces) = world();
        let mut map = BlockMap::default();
        // Two doors side by side facing north, hung on their outer sides: left at x 0, right at x 1.
        map.set([0, 1, 0], door(&faces, DoorHinge::Left, BlockHalf::Lower, false));
        map.set([0, 2, 0], door(&faces, DoorHinge::Left, BlockHalf::Upper, false));
        map.set([1, 1, 0], door(&faces, DoorHinge::Right, BlockHalf::Lower, false));
        map.set([1, 2, 0], door(&faces, DoorHinge::Right, BlockHalf::Upper, false));
        let used = use_at(&map, &faces, &Links::new(), [0, 2, 0]).expect("a door is used");
        assert_eq!((used.kind, used.now), (UseKind::Opens, true));
        let mut places: Vec<_> = used.changes.iter().map(|change| change.at).collect();
        places.sort();
        assert_eq!(places, vec![[0, 1, 0], [0, 2, 0], [1, 1, 0], [1, 2, 0]]);
        assert!(used.changes.iter().all(|change| BlockState::from_byte(change.state).open && change.animate));
        // A lever three blocks off, linked to the left door, opens both, and shuts them again.
        let lever = number(&faces, 10, BlockState::default_for(BlockShape::Lever).to_byte());
        map.set([4, 1, 0], lever);
        let links = Links::from([([4, 1, 0], vec![[0, 1, 0]])]);
        let on = use_at(&map, &faces, &links, [4, 1, 0]).expect("a lever is used");
        assert_eq!((on.kind, on.now, on.changes.len()), (UseKind::Toggles, true, 5));
        for change in &on.changes {
            let id = number(&faces, change.family, change.state);
            map.set(change.at, id);
        }
        let off = use_at(&map, &faces, &links, [4, 1, 0]).unwrap();
        assert!(!off.now && off.changes.iter().filter(|c| c.at != [4, 1, 0]).all(|c| !BlockState::from_byte(c.state).open));
        assert_eq!(off.changes.len(), 5);
        // Stone is used for nothing; a door alone opens alone.
        map.set([1, 1, 0], 1);
        map.set([1, 2, 0], AIR);
        assert!(use_at(&map, &faces, &links, [1, 1, 0]).is_none());
        assert_eq!(use_at(&map, &faces, &links, [0, 1, 0]).unwrap().changes.len(), 2);
    }
}
