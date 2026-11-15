//! The built stone of each kind of space, laid into its carved air: floors
//! of flagstones, walls dressed in stone to head height, columns and
//! pointed arches, galleries, stairs, bridges, water and lava, statues and
//! the rest of its pieces.

use super::build::{Grid, Kit};
use super::noise;
use super::pieces::{self, Frame};
use super::plan::{Space, SpaceKind, WayKind};
use crate::blocks::chunk::{AIR, BlockId};

/// Where a way meets a space: the block column of the way's end inside it,
/// the floor block's height there, and whether it is on the main path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Door {
    pub(crate) at: [i32; 2],
    pub(crate) level: i32,
    pub(crate) main: bool,
    pub(crate) grand: bool,
}

/// The doors of space `index` of the layout.
pub(crate) fn doors(kit: &Kit, index: usize) -> Vec<Door> {
    let mut doors = Vec::new();
    for way in &kit.layout.ways {
        let ends = [(way.from, way.line.first()), (Some(way.to), way.line.last())];
        for (space, point) in ends {
            if space != Some(index) {
                continue;
            }
            let Some(p) = point else { continue };
            doors.push(Door {
                at: [p[0].floor() as i32, p[1].floor() as i32],
                level: (p[2] as i32).div_euclid(2) - 1,
                main: way.main,
                grand: way.kind == WayKind::Grand,
            });
        }
    }
    doors
}

/// Whether column `(x, z)` lies within `room` blocks of any of `doors`.
fn by_door(doors: &[Door], x: i32, z: i32, room: i32) -> bool {
    doors.iter().any(|door| (door.at[0] - x).abs() <= room && (door.at[1] - z).abs() <= room)
}

/// Builds every space that reaches `grid`.
pub(crate) fn build_spaces(kit: &Kit, grid: &mut Grid) {
    let (lo, hi) = grid.columns();
    // Walls dressed first, from the carved rock alone, so each column's lining is the same from any chunk.
    for space in kit.layout.spaces.iter().filter(|space| space.height >= 0) {
        if !reaches(space, lo, hi, 8) {
            continue;
        }
        line_walls(kit, grid, space);
    }
    for (index, space) in kit.layout.spaces.iter().enumerate() {
        if space.height < 0 || !reaches(space, lo, hi, 16) {
            continue;
        }
        let doors = doors(kit, index);
        floor(kit, grid, space);
        match space.kind {
            SpaceKind::Hall => hall(kit, grid, space, &doors),
            SpaceKind::Room => room(kit, grid, space, &doors, true),
            SpaceKind::Landing => landing(kit, grid, space, &doors),
            SpaceKind::Gorge => gorge(kit, grid, space, &doors),
            SpaceKind::Chasm => chasm(kit, grid, space, &doors),
            SpaceKind::Flooded => flooded(kit, grid, space, &doors),
            SpaceKind::Arena => arena(kit, grid, space, &doors),
            SpaceKind::Descent => descent(kit, grid, space, &doors),
            SpaceKind::Treasury => treasury(kit, grid, space),
            SpaceKind::Shrine => shrine(kit, grid, space),
        }
        if let Some(at) = space.piece {
            let piece = &kit.dungeon.rooms[at].1.piece;
            for ([x, y, z], block) in crate::blocks::structure::model_pieces(piece.size, &piece.blocks) {
                grid.build([space.at[0] + x, space.at[1] + 1 + y, space.at[2] + z], block);
            }
        }
    }
}

fn reaches(space: &Space, lo: [i32; 2], hi: [i32; 2], pad: i32) -> bool {
    let (slo, shi) = (space.low(), space.high());
    !(shi[0] + pad < lo[0] || slo[0] - pad > hi[0] || shi[1] + pad < lo[1] || slo[1] - pad > hi[1])
}

/// The columns of a space and a few blocks round it, as far as its walls
/// may wander, within the grid.
fn columns(grid: &Grid, space: &Space, pad: i32) -> impl Iterator<Item = (i32, i32)> + use<> {
    let (lo, hi) = grid.columns();
    let (x0, x1) = ((space.at[0] - space.half[0] - pad).max(lo[0]), (space.at[0] + space.half[0] + pad).min(hi[0]));
    let (z0, z1) = ((space.at[2] - space.half[1] - pad).max(lo[1]), (space.at[2] + space.half[1] + pad).min(hi[1]));
    (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (x, z)))
}

/// Dresses the carved rock at a space's walls in stone up to head height,
/// capped with a band of trim: the blocks of rock beside its air, from its
/// floor up three blocks.
fn line_walls(kit: &Kit, grid: &mut Grid, space: &Space) {
    let b = &kit.blocks;
    let natural = matches!(space.kind, SpaceKind::Gorge | SpaceKind::Chasm);
    if natural {
        return;
    }
    let floor = space.at[1];
    let mut lining = Vec::new();
    for (x, z) in columns(grid, space, 5) {
        for up in 1..=4 {
            let y = floor + up;
            let at = [x, y, z];
            if grid.get(at) != super::build::LEAVE {
                continue;
            }
            let beside = [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|d| grid.open([x + d[0], y, z + d[1]]));
            if beside {
                lining.push((at, if up == 4 { b.trim } else { b.stone }));
            }
        }
    }
    for (at, block) in lining {
        grid.build(at, block);
    }
}

/// Flagstones under every column of a space carved open over its floor,
/// banded with trim every eight blocks.
fn floor(kit: &Kit, grid: &mut Grid, space: &Space) {
    let b = &kit.blocks;
    let y = space.at[1];
    for (x, z) in columns(grid, space, 5) {
        if !grid.open([x, y + 1, z]) || grid.open([x, y, z]) {
            continue;
        }
        let band = (x - space.at[0]).rem_euclid(8) == 0 || (z - space.at[2]).rem_euclid(8) == 0;
        grid.keep([x, y, z], if band { b.trim } else { b.floor });
    }
}

/// Space-local axes: the long way and across, with the space's length
/// along x or z.
fn axes(space: &Space) -> ([i32; 2], [i32; 2], i32, i32) {
    if space.along_x {
        ([1, 0], [0, 1], space.half[0], space.half[1])
    } else {
        ([0, 1], [1, 0], space.half[1], space.half[0])
    }
}

fn place(space: &Space, along: [i32; 2], across: [i32; 2], a: i32, c: i32, y: i32) -> [i32; 3] {
    [space.at[0] + along[0] * a + across[0] * c, y, space.at[2] + along[1] * a + across[1] * c]
}

/// The end of a space's length with fewer doors on the main path: where its
/// set piece faces the way in. +1 or -1 along its length.
fn quiet_end(space: &Space, doors: &[Door]) -> i32 {
    let (along, _, _, _) = axes(space);
    let score = |end: i32| {
        doors
            .iter()
            .filter(|door| door.main)
            .map(|door| {
                let a = (door.at[0] - space.at[0]) * along[0] + (door.at[1] - space.at[2]) * along[1];
                if a.signum() == end { 1 } else { 0 }
            })
            .sum::<i32>()
    };
    if score(1) <= score(-1) { 1 } else { -1 }
}

/// A pillared hall: two rows of columns down its length, pointed arches
/// between them carrying a gallery over the aisles behind, a balustrade at
/// its edge; statues in alcoves along the walls, braziers between; a dais
/// with a reliquary under a relief at its quiet end; ribs across its vault.
fn hall(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let (along, across, long, wide) = axes(space);
    let floor = space.at[1];
    let aisle = 6.min(wide / 3);
    let row = wide - aisle;
    let thick = if space.height >= 26 { 3 } else { 2 };
    let spring = floor + (space.height as f32 * 0.42) as i32;
    let bay = 8;
    let span = (bay - thick) as f32;
    let gallery = spring + pieces::arch_rise(span) + 2;
    let mut a = -long + 6;
    let mut stops = Vec::new();
    while a <= long - 6 {
        stops.push(a);
        a += bay;
    }
    for side in [-1, 1] {
        let c = side * row;
        // The columns, none in a doorway.
        let mut standing = Vec::new();
        for &a in &stops {
            let at = place(space, along, across, a, c, floor);
            if by_door(doors, at[0], at[2], 4) {
                standing.push(None);
                continue;
            }
            pieces::column(kit, grid, at, spring - floor, thick);
            standing.push(Some(at));
        }
        // Arches between each two, holding up the gallery.
        for pair in standing.windows(2) {
            let (Some(p), Some(q)) = (pair[0], pair[1]) else { continue };
            let half = thick as f32 * 0.5;
            let pa = [p[0] as f32 + 0.5 + along[0] as f32 * half, p[2] as f32 + 0.5 + along[1] as f32 * half];
            let pb = [q[0] as f32 + 0.5 - along[0] as f32 * half, q[2] as f32 + 0.5 - along[1] as f32 * half];
            pieces::arch(grid, b.trim, pa, pb, spring, 1, thick, Some(gallery - 1));
        }
        // The gallery's floor over the aisle, from the row out to the wall, a balustrade along its edge.
        for a in -long..=long {
            for c in row - thick / 2..=wide + 3 {
                let at = place(space, along, across, a, side * c, gallery);
                if !grid.open([at[0], gallery, at[2]]) && !grid.open([at[0], gallery + 1, at[2]]) {
                    continue;
                }
                grid.keep(at, b.floor);
                if c == row - thick / 2 && a.abs() < long - 2 {
                    pieces::balustrade(kit, grid, at);
                }
            }
        }
        // Statues in the bays against the walls, braziers between them.
        for (i, pair) in stops.windows(2).enumerate() {
            let a = (pair[0] + pair[1]) / 2;
            let foot = place(space, along, across, a, side * (wide - 2), floor);
            if by_door(doors, foot[0], foot[2], 5) {
                continue;
            }
            if i % 2 == 0 {
                let facing = [-across[0] * side, -across[1] * side];
                let height = (gallery - floor - 2).min(11);
                pieces::statue(kit, grid, Frame { foot, facing }, height, true);
            }
        }
    }
    // Ribs across the vault over each pair of columns.
    for &a in &stops {
        for c in -wide - 3..=wide + 3 {
            let at = place(space, along, across, a, c, 0);
            let mut y = floor + space.height + 4;
            while y > gallery && !grid.open([at[0], y, at[2]]) {
                y -= 1;
            }
            if y > gallery && grid.open([at[0], y, at[2]]) {
                grid.build([at[0], y, at[2]], b.trim);
            }
        }
    }
    // A dais at the quiet end: a reliquary on it, a relief on the wall over it.
    let end = quiet_end(space, doors);
    let dais = place(space, along, across, end * (long - 7), 0, floor);
    let facing = [-along[0] * end, -along[1] * end];
    let frame = Frame { foot: dais, facing };
    for (step, half) in [(1, 4), (2, 3)] {
        for ahead in -half..=half {
            for right in -half..=half {
                grid.build(frame.at(right, step, ahead), b.trim);
            }
        }
    }
    let glow = if b.cold_light != AIR { b.cold_light } else { b.light };
    grid.build(frame.at(0, 3, 0), b.statue);
    grid.keep(frame.at(0, 4, 0), glow);
    let wall = Frame { foot: place(space, along, across, end * (long - 1), 0, gallery + 1), facing };
    pieces::relief(kit, grid, wall, (wide - aisle) | 1, (space.height / 3).max(6));
}

/// A room where fights happen: pilasters round its walls, a raised edge
/// against the wall furthest from the way in with steps up at each end, two
/// thick columns for cover, none near a door.
fn room(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door], raised: bool) {
    let b = &kit.blocks;
    let (along, across, long, wide) = axes(space);
    let floor = space.at[1];
    // Pilasters every six blocks round the walls.
    for (x, z) in columns(grid, space, 4) {
        let (a, c) = (
            (x - space.at[0]) * along[0] + (z - space.at[2]) * along[1],
            (x - space.at[0]) * across[0] + (z - space.at[2]) * across[1],
        );
        if (a.rem_euclid(6) != 0 || c.abs() < wide - 1) && (c.rem_euclid(6) != 0 || a.abs() < long - 1) {
            continue;
        }
        if by_door(doors, x, z, 3) {
            continue;
        }
        for y in floor + 1..floor + space.height {
            if !grid.open([x, y, z]) {
                continue;
            }
            let beside = [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|d| !grid.open([x + d[0], y, z + d[1]]));
            if beside {
                grid.build([x, y, z], if y == floor + 1 { b.trim } else { b.stone });
            }
        }
    }
    // The raised edge.
    if raised {
        let end = quiet_end(space, doors);
        for a in long - 4..=long + 2 {
            for c in -wide - 2..=wide + 2 {
                let at = place(space, along, across, end * a, c, floor);
                if by_door(doors, at[0], at[2], 3) {
                    continue;
                }
                // Steps up at its two ends.
                let rise = if c.abs() > wide - 4 { 1 } else { 2 };
                for up in 1..=rise {
                    if grid.open([at[0], floor + up, at[2]]) {
                        let edge = a == long - 4 && up == rise;
                        grid.build([at[0], floor + up, at[2]], if edge { b.trim } else { b.stone });
                    }
                }
            }
        }
    }
    // Two thick columns for cover.
    for s in [-1, 1] {
        let at = place(space, along, across, s * (long / 3), s * (wide / 3), floor);
        if by_door(doors, at[0], at[2], 5) {
            continue;
        }
        pieces::column(kit, grid, at, space.height - 2, 2);
    }
}

/// The landing at the foot of the grand stair: a room, a carved archway
/// round the stair's end with a beast's head over it and caged braziers
/// either side, two hooded statues facing in.
fn landing(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    room(kit, grid, space, doors, false);
    for door in doors.iter().filter(|door| door.grand) {
        archway(kit, grid, space, door, kit.dungeon.sizes_stair());
    }
}

/// A carved archway round the end of a way where it meets `space`'s wall:
/// pillars and a pointed arch of trim, a beast's head over its point,
/// braziers in cages either side.
pub(crate) fn archway(kit: &Kit, grid: &mut Grid, space: &Space, door: &Door, width: i32) {
    let b = &kit.blocks;
    // The way out of the space through this door, and the wall's line.
    let (dx, dz) = (door.at[0] - space.at[0], door.at[1] - space.at[2]);
    let out = if dx.abs() * space.half[1] >= dz.abs() * space.half[0] { [dx.signum(), 0] } else { [0, dz.signum()] };
    let wall = if out[0] != 0 { space.at[0] + out[0] * space.half[0] } else { space.at[2] + out[1] * space.half[1] };
    let foot =
        if out[0] != 0 { [wall - out[0], door.level, door.at[1]] } else { [door.at[0], door.level, wall - out[1]] };
    let frame = Frame { foot, facing: [-out[0], -out[1]] };
    let half = width / 2 + 1;
    let spring = 5;
    for side in [-1, 1] {
        for up in 1..=spring {
            grid.build(frame.at(side * half, up, 0), b.trim);
            grid.build(frame.at(side * (half + 1), up, 0), b.stone);
        }
        pieces::caged_brazier(kit, grid, frame.at(side * (half + 3), 1, 1));
    }
    let a = frame.at(-half, 0, 0);
    let c = frame.at(half, 0, 0);
    pieces::arch(
        grid,
        b.trim,
        [a[0] as f32 + 0.5, a[2] as f32 + 0.5],
        [c[0] as f32 + 0.5, c[2] as f32 + 0.5],
        foot[1] + spring + 1,
        2,
        1,
        None,
    );
    let rise = pieces::arch_rise((half * 2) as f32);
    pieces::beast_head(kit, grid, Frame { foot: frame.at(0, spring + rise + 2, 0), facing: frame.facing });
}

/// A gorge: flagstones and rubble on its floor, a terrace at its upper way
/// in, and a stair cut down its walls from there, along one wall and back
/// along the other, built up solid from the rock under it; cold crystal in
/// the rock near its floor.
fn gorge(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let (along, across, long, wide) = axes(space);
    let floor = space.at[1];
    // Rubble on the floor.
    for (x, z) in columns(grid, space, 4) {
        if grid.open([x, floor + 1, z]) && noise::unit(x, floor, z, space.seed) < 0.12 {
            grid.build([x, floor + 1, z], b.rubble);
            if noise::unit(x, floor + 1, z, space.seed) < 0.3 && grid.open([x, floor + 2, z]) {
                grid.build([x, floor + 2, z], b.rubble);
            }
        }
    }
    // The upper way in: which end it is at.
    let upper = doors.iter().find(|door| door.level == space.upper);
    let near = upper.map_or(1, |door| {
        let a = (door.at[0] - space.at[0]) * along[0] + (door.at[1] - space.at[2]) * along[1];
        if a >= 0 { 1 } else { -1 }
    });
    // A terrace across the near end at the upper level.
    for a in long - 5..=long + 3 {
        for c in -wide - 4..=wide + 4 {
            let at = place(space, along, across, near * a, c, space.upper);
            fill_down(kit, grid, at, b.stone, 0);
            if a == long - 5 {
                pieces::balustrade(kit, grid, at);
            }
        }
    }
    if let Some(door) = upper {
        archway(kit, grid, space, door, kit.dungeon.sizes_passage());
    }
    // The stair: three wide, a half block down for every block along, against one wall then the other.
    let mut surface = (space.upper + 1) * 2;
    let bottom = (floor + 1) * 2;
    let mut side = if noise::hash(0, 0, 0, space.seed).is_multiple_of(2) { 1 } else { -1 };
    let mut heading = -near;
    let mut a = near * (long - 6);
    let inner = (wide as f32 * 0.62) as i32 - 2;
    let mut guard = 0;
    while surface > bottom && guard < 400 {
        guard += 1;
        for k in 0..3 {
            let c = side * (inner - k);
            let at = place(space, along, across, a, c, 0);
            tread(kit, grid, [at[0], 0, at[2]], surface);
            // Built out to the wall behind it.
            for out in 1..=6 {
                let wall_at = place(space, along, across, a, side * (inner + out), 0);
                tread(kit, grid, [wall_at[0], 0, wall_at[2]], surface);
            }
        }
        let next = a + heading;
        if next.abs() > long - 3 {
            // A landing, then back along the other wall.
            side = -side;
            heading = -heading;
            for c in -inner..=inner {
                let at = place(space, along, across, a, c, 0);
                tread(kit, grid, [at[0], 0, at[2]], surface);
            }
        } else {
            a = next;
            surface -= 1;
        }
    }
    // Cold crystal in the rock near the floor.
    for (x, z) in columns(grid, space, 4) {
        for y in floor + 1..floor + 5 {
            if !grid.open([x, y, z]) || noise::unit(x, y, z, space.seed ^ 0xC0) > 0.05 {
                continue;
            }
            let wall = [[1, 0], [-1, 0], [0, 1], [0, -1]]
                .iter()
                .any(|d| grid.get([x + d[0], y, z + d[1]]) == super::build::LEAVE);
            if wall && b.cold_light != AIR {
                grid.keep([x, y, z], b.cold_light);
            }
        }
    }
}

/// A tread of a stair at column `(at[0], at[2])` whose walking surface is
/// `surface` half blocks up: a slab on a half step, a whole block else,
/// built down solid under it to whatever is there, the air over it cleared.
fn tread(kit: &Kit, grid: &mut Grid, at: [i32; 3], surface: i32) {
    let b = &kit.blocks;
    let [x, _, z] = at;
    let top = surface.div_euclid(2);
    let (solid_to, slab) = if surface % 2 == 0 { (top - 1, None) } else { (top - 1, Some(top)) };
    if let Some(y) = slab {
        if !grid.open([x, y, z]) && grid.get([x, y, z]) == super::build::LEAVE {
            return;
        }
        grid.keep([x, y, z], if b.slab != AIR { b.slab } else { b.floor });
    } else if !grid.open([x, top, z]) {
        return;
    }
    grid.keep([x, solid_to, z], b.floor);
    let mut y = solid_to - 1;
    while y > grid.low[1] && grid.open([x, y, z]) {
        grid.build([x, y, z], b.stone);
        y -= 1;
    }
}

/// Fills the column at `at` with `block` down from it while it is open, and
/// sets `at` itself a floor; `depth` blocks at the most (0 for no limit).
fn fill_down(kit: &Kit, grid: &mut Grid, at: [i32; 3], block: BlockId, depth: i32) {
    let [x, y, z] = at;
    if !grid.open([x, y + 1, z]) && !grid.open([x, y, z]) {
        return;
    }
    grid.keep([x, y, z], kit.blocks.floor);
    let mut down = y - 1;
    while down > grid.low[1] && grid.open([x, down, z]) && (depth == 0 || y - down <= depth) {
        grid.build([x, down, z], block);
        down -= 1;
    }
}

/// A chasm: a walkway round the edge of its pit, a balustrade at the pit's
/// edge, a ring of columns with arches against its dome; two bridges across
/// on piers, meeting at an island in its middle with an obelisk; lava in
/// the bottom of the pit.
fn chasm(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let floor = space.at[1];
    let r = space.half[0].min(space.half[1]) as f32;
    let pit = 0.66 * r;
    for (x, z) in columns(grid, space, 4) {
        let d = (((x - space.at[0]) as f32).powi(2) + ((z - space.at[2]) as f32).powi(2)).sqrt();
        // Lava in the bottom two blocks of the pit.
        if d < pit + 3.0 {
            for y in space.bottom() + 1..=space.bottom() + 2 {
                if grid.open([x, y, z]) && b.lava != AIR {
                    grid.keep([x, y, z], b.lava);
                }
            }
        }
        // The bridges: four wide along x and z through the middle, on the walkway's level.
        let (bx, bz) = ((x - space.at[0]).abs(), (z - space.at[2]).abs());
        let bridge = (bx <= 2 || bz <= 2) && d < pit + 2.0;
        let island = d < r * 0.16;
        if bridge || island {
            grid.keep([x, floor, z], b.floor);
            let edge = (bx == 2 || bz == 2) && !island && !(bx <= 1 || bz <= 1);
            if edge {
                pieces::balustrade(kit, grid, [x, floor, z]);
            }
            if island {
                let mut y = floor - 1;
                while y > space.bottom() + 2 && grid.open([x, y, z]) {
                    grid.build([x, y, z], b.stone);
                    y -= 1;
                }
            }
        }
        // The pit's edge.
        if (d - pit).abs() < 0.8 && !bridge && grid.open([x, floor + 1, z]) && !grid.open([x, floor, z]) {
            pieces::balustrade(kit, grid, [x, floor, z]);
        }
    }
    // Piers under the bridges every ten blocks, arches between them.
    for way in [[1, 0], [0, 1]] {
        let mut k = -(pit as i32) + 4;
        let mut last: Option<i32> = None;
        while k < pit as i32 {
            if k.abs() > (r * 0.16) as i32 + 2 {
                for w in -2..=2 {
                    let x = space.at[0] + way[0] * k + way[1] * w;
                    let z = space.at[2] + way[1] * k + way[0] * w;
                    for dk in -1..=1 {
                        let (px, pz) = (x + way[0] * dk, z + way[1] * dk);
                        let mut y = floor - 1;
                        while y > space.bottom() + 2 && grid.open([px, y, pz]) {
                            grid.build([px, y, pz], b.stone);
                            y -= 1;
                        }
                    }
                }
                if let Some(prev) = last {
                    let a = [
                        (space.at[0] + way[0] * (prev + 2)) as f32 + 0.5,
                        (space.at[2] + way[1] * (prev + 2)) as f32 + 0.5,
                    ];
                    let c =
                        [(space.at[0] + way[0] * (k - 1)) as f32 + 0.5, (space.at[2] + way[1] * (k - 1)) as f32 + 0.5];
                    let rise = pieces::arch_rise((k - prev - 3) as f32);
                    // Upside down: the arch's top meets the bridge, so it is drawn from under the deck.
                    pieces::arch(grid, b.trim, a, c, floor - 1 - rise, 1, 5, Some(floor - 1));
                }
                last = Some(k);
            }
            k += 10;
        }
    }
    pieces::obelisk(kit, grid, [space.at[0], floor, space.at[2]], (space.height as f32 * 0.6) as i32);
    // A ring of columns at the edge of the walkway, arches between.
    let ring = r * 0.9;
    let count = ((std::f32::consts::TAU * ring) / 9.0).floor().max(6.0) as i32;
    let mut feet = Vec::new();
    for i in 0..count {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        let at = [
            space.at[0] + (angle.cos() * ring).round() as i32,
            floor,
            space.at[2] + (angle.sin() * ring).round() as i32,
        ];
        if by_door(doors, at[0], at[2], 4) {
            feet.push(None);
            continue;
        }
        pieces::column(kit, grid, at, space.height / 2, 2);
        feet.push(Some(at));
    }
    for i in 0..feet.len() {
        let (Some(p), Some(q)) = (feet[i], feet[(i + 1) % feet.len()]) else { continue };
        pieces::arch(
            grid,
            b.trim,
            [p[0] as f32 + 0.5, p[2] as f32 + 0.5],
            [q[0] as f32 + 0.5, q[2] as f32 + 0.5],
            floor + space.height / 2 + 1,
            1,
            2,
            None,
        );
    }
}

/// A flooded hall: green water in its basin to a block under its walkways,
/// steps down into it at both ends, monoliths on pillars standing in it,
/// a relief on its quiet end's wall, balustrades along the walkways' edges.
fn flooded(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let (along, across, long, wide) = axes(space);
    let floor = space.at[1];
    let basin = (wide as f32 * 0.62) as i32;
    for (x, z) in columns(grid, space, 4) {
        let a = (x - space.at[0]) * along[0] + (z - space.at[2]) * along[1];
        let c = (x - space.at[0]) * across[0] + (z - space.at[2]) * across[1];
        // Steps down into the water at both ends.
        let from_end = long - a.abs();
        if c.abs() <= basin && from_end < 10 {
            let step = floor - (10 - from_end) / 2;
            let mut y = step;
            while y > space.bottom() && grid.open([x, y, z]) {
                grid.build([x, y, z], if y == step { b.floor } else { b.stone });
                y -= 1;
            }
        }
        for y in space.bottom() + 1..floor {
            if grid.open([x, y, z]) && b.water != AIR {
                grid.keep([x, y, z], b.water);
            }
        }
        if c.abs() == basin + 1 && from_end > 2 && !by_door(doors, x, z, 3) {
            pieces::balustrade(kit, grid, [x, floor, z]);
        }
    }
    // Monoliths in pairs down the basin.
    let mut a = -long + 12;
    while a <= long - 12 {
        for s in [-1, 1] {
            let foot = place(space, along, across, a, s * (basin / 2), space.bottom());
            pieces::monolith(kit, grid, foot, space.below + 2);
        }
        a += 14;
    }
    let end = quiet_end(space, doors);
    let facing = [-along[0] * end, -along[1] * end];
    let wall = Frame { foot: place(space, along, across, end * (long - 1), 0, floor + 3), facing };
    pieces::relief(kit, grid, wall, (wide | 1).min(17), (space.height / 2).max(8));
}

/// The boss's arena: rings of trim in its floor, a ring of columns round it
/// with arches between, a throne on a dais at its far side from the way in.
fn arena(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let floor = space.at[1];
    let r = space.half[0] as f32;
    for (x, z) in columns(grid, space, 2) {
        let d = (((x - space.at[0]) as f32).powi(2) + ((z - space.at[2]) as f32).powi(2)).sqrt();
        if grid.open([x, floor + 1, z]) && ((d - r * 0.35).abs() < 0.6 || (d - r * 0.6).abs() < 0.6) {
            grid.keep([x, floor, z], b.trim);
        }
    }
    let ring = r * 0.82;
    let count = 12;
    let mut feet = Vec::new();
    for i in 0..count {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        let at = [
            space.at[0] + (angle.cos() * ring).round() as i32,
            floor,
            space.at[2] + (angle.sin() * ring).round() as i32,
        ];
        if by_door(doors, at[0], at[2], 4) {
            feet.push(None);
            continue;
        }
        pieces::column(kit, grid, at, space.height * 2 / 5, 3);
        feet.push(Some(at));
    }
    for i in 0..feet.len() {
        let (Some(p), Some(q)) = (feet[i], feet[(i + 1) % feet.len()]) else { continue };
        pieces::arch(
            grid,
            b.trim,
            [p[0] as f32 + 0.5, p[2] as f32 + 0.5],
            [q[0] as f32 + 0.5, q[2] as f32 + 0.5],
            floor + space.height * 2 / 5 + 1,
            1,
            2,
            None,
        );
    }
    // The throne, away from the main way in.
    let way_in = doors.iter().find(|door| door.main).map_or([space.at[0] - 1, space.at[2]], |door| door.at);
    let (dx, dz) = (way_in[0] - space.at[0], way_in[1] - space.at[2]);
    let toward = if dx.abs() >= dz.abs() { [dx.signum(), 0] } else { [0, dz.signum()] };
    let foot = [space.at[0] - toward[0] * (ring as i32 - 7), floor, space.at[2] - toward[1] * (ring as i32 - 7)];
    pieces::throne(kit, grid, Frame { foot, facing: toward });
}

/// A shaft down to the next floor: a stair winding down round its wall from
/// its upper way in, a half block for every block along, built out from the
/// wall; a well in its middle going on down to lava; a parapet round the
/// well at the foot.
fn descent(kit: &Kit, grid: &mut Grid, space: &Space, doors: &[Door]) {
    let b = &kit.blocks;
    let floor = space.at[1];
    let r = space.half[0] as f32;
    let well = r * 0.28;
    let middle = r - 3.0;
    // Where the stair starts: at the upper way in, or wherever.
    let start = doors
        .iter()
        .find(|door| door.level == space.upper)
        .map_or(0.0, |door| ((door.at[1] - space.at[2]) as f32).atan2((door.at[0] - space.at[0]) as f32));
    let top = (space.upper + 1) * 2;
    let bottom = (floor + 1) * 2;
    for (x, z) in columns(grid, space, 2) {
        let (dx, dz) = ((x - space.at[0]) as f32 + 0.5, (z - space.at[2]) as f32 + 0.5);
        let d = (dx * dx + dz * dz).sqrt();
        if d < well + 0.5 {
            // Lava at the bottom of the well.
            for y in space.bottom() + 1..=space.bottom() + 2 {
                if grid.open([x, y, z]) && b.lava != AIR {
                    grid.keep([x, y, z], b.lava);
                }
            }
            continue;
        }
        if (d - well - 1.0).abs() < 0.7 && grid.open([x, floor + 1, z]) {
            pieces::balustrade(kit, grid, [x, floor, z]);
        }
        if d < r - 4.6 || d > r + 1.5 {
            continue;
        }
        // Every turn of the stair over this column: the angle round from its start, a turn at a time.
        let mut angle = dz.atan2(dx) - start;
        angle = angle.rem_euclid(std::f32::consts::TAU);
        let mut turn = 0;
        loop {
            let along = (angle + turn as f32 * std::f32::consts::TAU) * middle;
            let surface = top - along.floor() as i32;
            if surface < bottom {
                break;
            }
            tread_ledge(kit, grid, [x, 0, z], surface);
            turn += 1;
        }
    }
    // A landing at the upper way in.
    for door in doors.iter().filter(|door| door.level == space.upper) {
        for dz in -3..=3 {
            for dx in -3..=3 {
                let at = [door.at[0] + dx, space.upper, door.at[1] + dz];
                let d = (((at[0] - space.at[0]) as f32).powi(2) + ((at[2] - space.at[2]) as f32).powi(2)).sqrt();
                if d > well + 2.0 {
                    tread_ledge(kit, grid, at, (space.upper + 1) * 2);
                }
            }
        }
    }
}

/// A tread jutting out of a wall, two blocks deep: the tread and a block of
/// stone under it.
fn tread_ledge(kit: &Kit, grid: &mut Grid, at: [i32; 3], surface: i32) {
    let b = &kit.blocks;
    let [x, _, z] = at;
    let top = surface.div_euclid(2);
    if surface % 2 == 1 {
        grid.keep([x, top, z], if b.slab != AIR { b.slab } else { b.floor });
    }
    grid.keep([x, top - 1, z], b.floor);
    if grid.open([x, top - 2, z]) {
        grid.build([x, top - 2, z], b.stone);
    }
    for y in top + if surface % 2 == 1 { 1 } else { 0 }..top + 3 {
        if grid.get([x, y, z]) == super::build::LEAVE {
            grid.set([x, y, z], AIR, super::build::Tag::Air);
        }
    }
}

/// A treasury: a reliquary on a pedestal in its middle, glowing.
fn treasury(kit: &Kit, grid: &mut Grid, space: &Space) {
    let b = &kit.blocks;
    let [x, y, z] = space.at;
    for dz in -1..=1 {
        for dx in -1..=1 {
            grid.build([x + dx, y + 1, z + dz], b.trim);
        }
    }
    grid.build([x, y + 2, z], b.statue);
    let glow = if b.cold_light != AIR { b.cold_light } else { b.light };
    grid.keep([x, y + 3, z], glow);
}

/// A shrine: an altar of trim, braziers either side.
fn shrine(kit: &Kit, grid: &mut Grid, space: &Space) {
    let b = &kit.blocks;
    let [x, y, z] = space.at;
    for d in -1..=1 {
        grid.build([x + d, y + 1, z], b.trim);
        grid.build([x + d, y + 2, z], b.trim);
    }
    pieces::caged_brazier(kit, grid, [x - 3, y + 1, z]);
    pieces::caged_brazier(kit, grid, [x + 3, y + 1, z]);
}
