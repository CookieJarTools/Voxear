//! A way pulled tight: of the places along a way, only the corners kept,
//! each where a straight line from the last cannot reach past it.
//!
//! A straight line is walkable when, block by block along it, each step is
//! one the walker can take ([`straight`]): up a step, down a drop, or on
//! the level cornerwise where the line passes a corner, and never into an
//! area dearer than the way it shortens went through.

use super::{BlockNav, PlaceRef, step, walking::Walker};

/// What a block of a place costs to cross.
pub type Cost<'a> = dyn Fn(PlaceRef) -> f32 + 'a;

/// The longest straight line looked for, in blocks: past this a way keeps
/// a corner even on open ground, so pulling a long way tight stays cheap.
const LONGEST: i32 = 96;

/// Whether `walker` can walk straight from place `from` to place `to`,
/// through no area costing more than `dearest`.
pub fn straight(nav: &BlockNav, from: PlaceRef, to: PlaceRef, walker: Walker, costs: &Cost<'_>, dearest: f32) -> bool {
    walk_line(nav, from, to, walker, costs, dearest).is_some_and(|end| end == to)
}

/// Walks straight from place `from` towards place `to`'s column, block by
/// block: the place it ends in there, or `None` where a step on the way
/// cannot be taken.
pub fn walk_line(
    nav: &BlockNav,
    from: PlaceRef,
    to: PlaceRef,
    walker: Walker,
    costs: &Cost<'_>,
    dearest: f32,
) -> Option<PlaceRef> {
    let (a, b) = (nav.cell(from)?, nav.cell(to)?);
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    if dx == 0 && dz == 0 {
        return Some(from);
    }
    let (sx, sz) = (dx.signum(), dz.signum());
    // How far along the line, from 0 to 1, the next side of a block is crossed each way.
    let (across_x, across_z) = (1.0 / dx.abs().max(1) as f64, 1.0 / dz.abs().max(1) as f64);
    let (mut next_x, mut next_z) =
        (if dx == 0 { f64::INFINITY } else { 0.5 * across_x }, if dz == 0 { f64::INFINITY } else { 0.5 * across_z });
    let mut at = from;
    let mut cell = [a[0], a[2]];
    while cell != [b[0], b[2]] {
        let (mx, mz) = match next_x - next_z {
            gap if gap.abs() < 1e-9 => (sx, sz),
            gap if gap < 0.0 => (sx, 0),
            _ => (0, sz),
        };
        at = match (mx != 0, mz != 0) {
            // Through a corner: both ways round it walkable, ending in one place, so no wall's corner is cut.
            (true, true) => {
                // Each way round, the block it passes may be crossed too.
                let passable = |on: PlaceRef| costs(on) <= dearest;
                let by_x = step(nav, at, mx, 0, walker)
                    .filter(|(on, _)| passable(*on))
                    .and_then(|(on, _)| step(nav, on, 0, mz, walker));
                let by_z = step(nav, at, 0, mz, walker)
                    .filter(|(on, _)| passable(*on))
                    .and_then(|(on, _)| step(nav, on, mx, 0, walker));
                match (by_x, by_z) {
                    (Some((a, _)), Some((b, _))) if a == b => a,
                    _ => return None,
                }
            }
            _ => step(nav, at, mx, mz, walker)?.0,
        };
        if mx != 0 {
            next_x += across_x;
            cell[0] += mx;
        }
        if mz != 0 {
            next_z += across_z;
            cell[1] += mz;
        }
        if costs(at) > dearest {
            return None;
        }
    }
    Some(at)
}

/// Which of a way's `places` to keep as corners: the first, the last, and
/// between them each place a straight line from the corner before cannot
/// reach past.
pub fn pull(nav: &BlockNav, places: &[PlaceRef], walker: Walker, costs: &Cost<'_>) -> Vec<usize> {
    if places.len() <= 2 {
        return (0..places.len()).collect();
    }
    let cells: Vec<[i32; 3]> = places.iter().map(|&at| nav.cell(at).unwrap_or_default()).collect();
    let cost = |at: usize| costs(places[at]);
    // Where the way turns: a line to the end of each straight run is all that needs trying.
    let way = |at: usize| [cells[at + 1][0] - cells[at][0], cells[at + 1][2] - cells[at][2]];
    let mut turns: Vec<usize> = (1..places.len() - 1).filter(|&at| way(at) != way(at - 1)).collect();
    turns.push(places.len() - 1);
    let mut kept = vec![0];
    let mut anchor = 0;
    let mut reached = 0;
    let mut dearest = cost(0);
    let reaches = |from: usize, to: usize, dearest: f32| {
        let (a, b) = (cells[from], cells[to]);
        (a[0] - b[0]).abs().max((a[2] - b[2]).abs()) <= LONGEST
            && straight(nav, places[from], places[to], walker, costs, dearest)
    };
    let mut at = 0;
    while at < turns.len() {
        let turn = turns[at];
        let segment = (reached + 1..=turn).map(cost).fold(dearest, f32::max);
        if reached == anchor || reaches(anchor, turn, segment) {
            (reached, dearest) = (turn, segment);
            at += 1;
            continue;
        }
        // The line from the last corner fails here: a corner where it last reached.
        kept.push(reached);
        anchor = reached;
        dearest = cost(anchor);
        // A straight run of the way is always walkable from its start, but where it is not, keep every place.
        if !reaches(anchor, turn, (anchor..=turn).map(cost).fold(0.0, f32::max)) {
            kept.extend(anchor + 1..turn);
            anchor = turn - 1;
        }
        reached = anchor;
    }
    if kept.last() != Some(&(places.len() - 1)) {
        kept.push(places.len() - 1);
    }
    kept.dedup();
    kept
}
