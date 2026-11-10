//! Which chunks of a world of blocks the camera can see into: caves and
//! rooms closed off from it are passed over before the GPU sees them.
//!
//! Each chunk notes which of its six faces see each other through its air
//! ([`sights`]): air flooded from one face that reaches another links the
//! two. A walk from the camera's chunk goes from chunk to chunk through
//! those links, never back towards the camera along an axis it has moved
//! away along, and only the chunks it reaches are drawn ([`seen`]). It is
//! cheap and never hides what can be seen; a ravine's walls hide less than
//! they could, which the depth test takes care of.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    BlockMap,
    chunk::{AIR, CHUNK, Chunk},
};

/// Every pair of faces linked: what a chunk of air sees.
pub const ALL_SIGHTS: u16 = (1 << 15) - 1;

/// The bit for faces `a` and `b` (+x, -x, +y, -y, +z, -z) seeing each other.
pub fn sight(a: usize, b: usize) -> u16 {
    let (a, b) = (a.min(b), a.max(b));
    if a == b {
        return 0;
    }
    // Pairs in order: (0, 1) to (0, 5), (1, 2) to (1, 5), and so on.
    let before: usize = (0..a).map(|i| 5 - i).sum();
    1 << (before + b - a - 1)
}

/// Which of a chunk's faces see each other through its air.
pub fn sights(chunk: &Chunk) -> u16 {
    if !chunk.has_blocks() {
        return ALL_SIGHTS;
    }
    let side = CHUNK as usize;
    let index = |x: usize, y: usize, z: usize| (y * side + z) * side + x;
    let mut seen = vec![false; side * side * side];
    let mut found = 0u16;
    let mut stack: Vec<[usize; 3]> = Vec::new();
    for start in 0..side * side * side {
        let (x, rest) = (start % side, start / side);
        let (z, y) = (rest % side, rest / side);
        if seen[start] || chunk.get(x as u32, y as u32, z as u32) != AIR {
            continue;
        }
        // Only air on a face can link faces.
        let on_face = [x, y, z].iter().any(|&v| v == 0 || v == side - 1);
        if !on_face {
            continue;
        }
        let mut faces = 0u8;
        seen[start] = true;
        stack.push([x, y, z]);
        while let Some(at) = stack.pop() {
            for axis in 0..3 {
                if at[axis] == side - 1 {
                    faces |= 1 << (axis * 2);
                }
                if at[axis] == 0 {
                    faces |= 1 << (axis * 2 + 1);
                }
            }
            for axis in 0..3 {
                for step in [-1i32, 1] {
                    let next = at[axis] as i32 + step;
                    if !(0..side as i32).contains(&next) {
                        continue;
                    }
                    let mut to = at;
                    to[axis] = next as usize;
                    let i = index(to[0], to[1], to[2]);
                    if !seen[i] && chunk.get(to[0] as u32, to[1] as u32, to[2] as u32) == AIR {
                        seen[i] = true;
                        stack.push(to);
                    }
                }
            }
        }
        for a in 0..6 {
            for b in a + 1..6 {
                if faces & (1 << a) != 0 && faces & (1 << b) != 0 {
                    found |= sight(a, b);
                }
            }
        }
        if found == ALL_SIGHTS {
            break;
        }
    }
    found
}

/// The way each face faces, in chunks.
const STEPS: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// The chunks of `map` a camera in chunk `from` can see into, walking from
/// it through each chunk's sights (`sights_of`, a chunk not kept being air)
/// and only into chunks `in_view` lets through; `None` where the camera is
/// outside the world's chunks, when every chunk may be seen.
pub fn seen(
    map: &BlockMap,
    sights_of: &HashMap<[i32; 3], u16>,
    from: [i32; 3],
    in_view: impl Fn([i32; 3]) -> bool,
) -> Option<HashSet<[i32; 3]>> {
    let mut low = [i32::MAX; 3];
    let mut high = [i32::MIN; 3];
    for (key, _) in map.chunks() {
        for i in 0..3 {
            low[i] = low[i].min(key[i] - 1);
            high[i] = high[i].max(key[i] + 1);
        }
    }
    if (0..3).any(|i| from[i] < low[i] || from[i] > high[i]) {
        return None;
    }
    let sights = |key: [i32; 3]| match map.chunk(key) {
        None => ALL_SIGHTS,
        Some(_) => sights_of.get(&key).copied().unwrap_or(0),
    };
    let mut reached = HashSet::new();
    let mut queue = VecDeque::new();
    // From the camera's chunk every way, as though it came in through every face.
    reached.insert(from);
    queue.push_back((from, None::<usize>, 0u8));
    while let Some((key, came, gone)) = queue.pop_front() {
        let through = sights(key);
        for (face, step) in STEPS.iter().enumerate() {
            // Never back along an axis it has moved away along.
            let back = face ^ 1;
            if gone & (1 << back) != 0 {
                continue;
            }
            if let Some(came) = came
                && through & sight(came, face) == 0
            {
                continue;
            }
            let next = [0, 1, 2].map(|i| key[i] + step[i]);
            if (0..3).any(|i| next[i] < low[i] || next[i] > high[i]) || reached.contains(&next) || !in_view(next) {
                continue;
            }
            reached.insert(next);
            queue.push_back((next, Some(back), gone | (1 << face)));
        }
    }
    Some(reached)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunks_faces_see_each_other_through_its_air_alone() {
        assert_eq!(sights(&Chunk::filled(AIR)), ALL_SIGHTS);
        assert_eq!(sights(&Chunk::filled(1)), 0);
        // A wall across the middle along y: its two sides' faces see along it, but not through it.
        let mut wall = Chunk::filled(AIR);
        for z in 0..CHUNK {
            for x in 0..CHUNK {
                wall.set(x, 16, z, 1);
            }
        }
        let found = sights(&wall);
        assert_eq!(found & sight(2, 3), 0, "+y and -y are apart");
        assert_ne!(found & sight(0, 1), 0, "+x and -x see each other above and below it");
        assert_ne!(found & sight(2, 0), 0, "+y and +x see each other above it");
        // Every pair has its own bit.
        let all: u16 = (0..6).flat_map(|a| (a + 1..6).map(move |b| sight(a, b))).fold(0, |all, bit| {
            assert_eq!(all & bit, 0);
            all | bit
        });
        assert_eq!(all, ALL_SIGHTS);
    }

    #[test]
    fn a_cave_closed_off_from_the_camera_is_not_seen_and_an_open_one_is() {
        // Ground 3 chunks a side under air, a cave chunk in its middle: closed, then opened by a shaft.
        let mut map = BlockMap::default();
        for y in 0..3 {
            for z in -1..2 {
                for x in -1..2 {
                    map.put_chunk([x, y, z], Chunk::filled(1));
                }
            }
        }
        let mut cave = Chunk::filled(1);
        for y in 10..20 {
            for z in 10..20 {
                for x in 10..20 {
                    cave.set(x, y, z, AIR);
                }
            }
        }
        map.put_chunk([0, 1, 0], cave.clone());
        let all = |map: &BlockMap| -> HashMap<[i32; 3], u16> {
            map.chunks().map(|(key, chunk)| (*key, sights(chunk))).collect()
        };
        let camera = [0, 3, 0];
        let seen_now = seen(&map, &all(&map), camera, |_| true).expect("the camera among its chunks");
        assert!(seen_now.contains(&[0, 2, 0]), "the ground's top chunk, under the camera");
        assert!(!seen_now.contains(&[0, 1, 0]), "the closed cave");
        // A shaft from the cave up through the top chunk.
        for y in 20..32 {
            cave.set(15, y, 15, AIR);
        }
        let mut top = Chunk::filled(1);
        for y in 0..32 {
            top.set(15, y, 15, AIR);
        }
        map.put_chunk([0, 1, 0], cave);
        map.put_chunk([0, 2, 0], top);
        let seen_now = seen(&map, &all(&map), camera, |_| true).unwrap();
        assert!(seen_now.contains(&[0, 1, 0]), "the cave, down the shaft");
        assert!(!seen_now.contains(&[0, 0, 0]), "solid ground under it");
        assert!(seen(&map, &all(&map), [40, 0, 0], |_| true).is_none(), "from far off, everything");
    }
}
