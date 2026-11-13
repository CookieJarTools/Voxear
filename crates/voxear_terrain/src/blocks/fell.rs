//! Trees that come down: what a log cut through held up, and blocks that
//! have come loose from their world.
//!
//! When a log is cut through, the logs of its block joined to it at its
//! height and above, and the leaves round them, may hang on nothing:
//! [`loose_tree`] finds them. They come loose only where none of those logs
//! goes lower than the cut or stands on ground of its own, and where they
//! hold leaves, so a tree comes down and a wall of logs does not. Leaves
//! nearer the logs of another tree stay with it, and vines hanging from
//! what comes loose come with it.
//!
//! Blocks that came loose are drawn as a world of their own, turned as its
//! Obj is turned ([`LooseBlocks`]): a tree toppling looks as it stood, its
//! bark, its thin logs, its bushy leaves. They are no ground: nothing
//! stands on them, rays and collisions pass them by, and they are no part
//! of what the world saves.

use std::collections::{HashSet, VecDeque};

use voxear_ecs::Component;

use super::{BlockMap, BlockWorld, chunk::BlockId};

/// The most logs a tree that comes down has: more is a wood of joined
/// trees, or a building, and stays.
pub const MOST_LOGS: usize = 2048;

/// How far its leaves reach from its logs, in blocks counted face to face
/// or corner to corner.
pub const LEAF_REACH: u32 = 5;

/// The fewest leaves a tree that comes down holds.
pub const FEWEST_LEAVES: usize = 3;

/// What a block of a world is, as a tree comes down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreePart {
    /// A log of the block that was cut.
    Log,
    /// A log of any other block: another tree's.
    OtherLog,
    Leaf,
    Vine,
    /// Ground that holds what stands on it.
    Ground,
    /// Air, water and whatever else holds nothing.
    Nothing,
}

/// The 26 ways to the blocks round one, face, edge and corner.
fn round() -> impl Iterator<Item = [i32; 3]> {
    (0..27).filter(|&d| d != 13).map(|d| [d % 3 - 1, (d / 3) % 3 - 1, d / 9 - 1])
}

fn plus(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The blocks a log cut through at `cut` held up, if they come loose: its
/// logs, then its leaves and vines, each where it is. `part` says what each
/// block of the world is. None where they stay: held by a log lower than
/// the cut or standing on ground, too many to be a tree, or without leaves.
pub fn loose_tree(part: &dyn Fn([i32; 3]) -> TreePart, cut: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    // Its logs: from those round the cut at its height and over it, joined corner to corner.
    let mut logs: Vec<[i32; 3]> = Vec::new();
    let mut seen: HashSet<[i32; 3]> = HashSet::from([cut]);
    let mut next: VecDeque<[i32; 3]> = VecDeque::new();
    for way in round().filter(|way| way[1] >= 0) {
        let at = plus(cut, way);
        if part(at) == TreePart::Log && seen.insert(at) {
            next.push_back(at);
        }
    }
    while let Some(at) = next.pop_front() {
        // Lower than the cut, or on ground of its own: held.
        let under = part(plus(at, [0, -1, 0]));
        if at[1] < cut[1] || under == TreePart::Ground || under == TreePart::OtherLog {
            return None;
        }
        logs.push(at);
        if logs.len() > MOST_LOGS {
            return None;
        }
        for way in round() {
            let beside = plus(at, way);
            if part(beside) == TreePart::Log && seen.insert(beside) {
                next.push_back(beside);
            }
        }
    }
    if logs.is_empty() {
        return None;
    }
    // Its leaves: out from its logs, as far as they reach, but none beside another tree's log.
    let ours: HashSet<[i32; 3]> = logs.iter().copied().collect();
    let mut leaves: Vec<[i32; 3]> = Vec::new();
    let mut reach: VecDeque<([i32; 3], u32)> = VecDeque::new();
    for &log in &logs {
        for way in round() {
            let at = plus(log, way);
            if part(at) == TreePart::Leaf && seen.insert(at) {
                reach.push_back((at, 1));
            }
        }
    }
    while let Some((at, far)) = reach.pop_front() {
        // Another tree's leaf where one of its logs is nearer it than ours are.
        let near = far as i32 - 1;
        let theirs = (-near..=near).any(|dy| {
            (-near..=near).any(|dz| (-near..=near).any(|dx| part(plus(at, [dx, dy, dz])) == TreePart::OtherLog))
        });
        if theirs {
            continue;
        }
        leaves.push(at);
        if far >= LEAF_REACH {
            continue;
        }
        for way in round() {
            let beside = plus(at, way);
            if part(beside) == TreePart::Leaf && !ours.contains(&beside) && seen.insert(beside) {
                reach.push_back((beside, far + 1));
            }
        }
    }
    if leaves.len() < FEWEST_LEAVES {
        return None;
    }
    // Vines on it: beside any of it face to face, and on down the vines hanging from those.
    let mut vines: Vec<[i32; 3]> = Vec::new();
    let mut hanging: Vec<[i32; 3]> = Vec::new();
    for &at in logs.iter().chain(&leaves) {
        for way in [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]] {
            let beside = plus(at, way);
            if part(beside) == TreePart::Vine && seen.insert(beside) {
                hanging.push(beside);
            }
        }
    }
    while let Some(at) = hanging.pop() {
        vines.push(at);
        let below = plus(at, [0, -1, 0]);
        if part(below) == TreePart::Vine && seen.insert(below) {
            hanging.push(below);
        }
    }
    Some(logs.into_iter().chain(leaves).chain(vines).collect())
}

/// `blocks`, each where it was in its world, as a map of their own: what
/// came loose, to be drawn apart from its world. Kept where they were, so
/// each is drawn as it was, its tufts and pictures as they were there.
pub fn loose_map(blocks: &[([i32; 3], BlockId)]) -> BlockMap {
    let mut map = BlockMap::default();
    for &(at, block) in blocks {
        map.set(at, block);
    }
    map
}

/// Blocks come loose from a world of blocks, drawn as a world of their own
/// turned as their Obj is: its position the point they turn about, and
/// `offset` where their world's corner is from there, before it is turned,
/// in metres. Their blocks are where they were in the world they came from
/// (see [`loose_map`]), so `offset` reaches back to that world's corner. No ground: only the renderer looks at them. They are lit by
/// the light bounced in the world they came `from`, where it is still there.
#[derive(Clone, Debug)]
pub struct LooseBlocks {
    pub world: BlockWorld,
    pub offset: [f32; 3],
    pub from: Option<voxear_ecs::Entity>,
}

impl Component for LooseBlocks {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A world of parts: a tree of logs and leaves, on ground.
    struct Wood(HashMap<[i32; 3], TreePart>);

    impl Wood {
        fn part(&self, at: [i32; 3]) -> TreePart {
            match self.0.get(&at) {
                Some(part) => *part,
                None if at[1] < 0 => TreePart::Ground,
                None => TreePart::Nothing,
            }
        }

        /// A trunk at x, z up to `top`, a ball of leaves round its top.
        fn tree(&mut self, x: i32, z: i32, top: i32, log: TreePart) {
            for y in 0..=top {
                self.0.insert([x, y, z], log);
            }
            for dy in -2i32..=2 {
                for dz in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let at = [x + dx, top + dy, z + dz];
                        if dx * dx + dy * dy + dz * dz <= 5 && !self.0.contains_key(&at) {
                            self.0.insert(at, TreePart::Leaf);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_tree_cut_at_its_foot_comes_loose_with_its_leaves_and_vines_and_none_of_its_neighbours() {
        let mut wood = Wood(HashMap::new());
        wood.tree(0, 0, 6, TreePart::Log);
        // Another tree beside it, its crown touching this one's.
        wood.tree(4, 0, 6, TreePart::OtherLog);
        wood.0.insert([-3, 6, 0], TreePart::Vine);
        wood.0.insert([-3, 5, 0], TreePart::Vine);
        // Cut at its foot: the block there is gone.
        wood.0.remove(&[0, 0, 0]);
        let part = |at: [i32; 3]| wood.part(at);
        let loose = loose_tree(&part, [0, 0, 0]).expect("it comes down");
        let count = |kind: TreePart| loose.iter().filter(|at| wood.part(**at) == kind).count();
        assert_eq!(count(TreePart::Log), 6, "the trunk over the cut");
        assert!(count(TreePart::Leaf) > 15);
        assert_eq!(count(TreePart::Vine), 2, "the vine hangs from its leaves");
        assert_eq!(count(TreePart::OtherLog), 0);
        // None of the leaves beside the other tree's trunk.
        assert!(loose.iter().all(|at| at[0] < 3), "{loose:?}");
        assert_eq!(loose.iter().collect::<HashSet<_>>().len(), loose.len(), "each once");
    }

    #[test]
    fn logs_held_below_the_cut_or_on_ground_or_without_leaves_stay() {
        let mut wood = Wood(HashMap::new());
        wood.tree(0, 0, 6, TreePart::Log);
        // A branch from the trunk down to the ground beside it: still held.
        for (x, y) in [(1, 4), (2, 3), (2, 2), (2, 1), (2, 0)] {
            wood.0.insert([x, y, 0], TreePart::Log);
        }
        wood.0.remove(&[0, 0, 0]);
        let part = |at: [i32; 3]| wood.part(at);
        assert_eq!(loose_tree(&part, [0, 0, 0]), None);
        // A post of logs with no leaves: a wall, not a tree.
        let mut post = Wood(HashMap::new());
        for y in 1..5 {
            post.0.insert([0, y, 0], TreePart::Log);
        }
        let part = |at: [i32; 3]| post.part(at);
        assert_eq!(loose_tree(&part, [0, 0, 0]), None);
        // Cut high in the trunk: only what is over it comes down.
        let mut tall = Wood(HashMap::new());
        tall.tree(0, 0, 8, TreePart::Log);
        tall.0.remove(&[0, 4, 0]);
        let part = |at: [i32; 3]| tall.part(at);
        let loose = loose_tree(&part, [0, 4, 0]).unwrap();
        assert!(loose.iter().all(|at| at[1] > 4), "the stump stays");
        assert_eq!(loose.iter().filter(|at| tall.part(**at) == TreePart::Log).count(), 4);
    }

    #[test]
    fn what_came_loose_is_a_map_of_its_own_each_block_where_it_was() {
        let map = loose_map(&[([5, 3, 5], 7), ([5, 4, 6], 8)]);
        assert_eq!((map.get([5, 3, 5]), map.get([5, 4, 6]), map.get([0, 0, 0])), (7, 8, 0));
    }
}
