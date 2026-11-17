//! Agents going round each other: each picks the velocity nearest the one
//! it wants that keeps it from walking into any agent near it for a while
//! ahead, each taking its share of the turning aside, so two meeting both
//! step aside a little rather than one stepping aside twice as far.
//!
//! For each neighbour, the velocities that would bring the two together
//! within [`Avoidance::ahead`] seconds make a cone; half of the smallest
//! change that leaves the cone is this agent's to make, which gives a
//! half-plane of velocities it may choose. The velocity nearest the wanted
//! one inside every half-plane, and no faster than the agent goes, is found
//! by a small linear program on the ground plane; where the half-planes
//! leave nothing, as in a crush, the velocity that least breaks them.
//!
//! Neighbours are found in a grid of cells as wide as an agent looks, and
//! the agents are shared out over the terrain's threads, each working only
//! from where every agent was at the start, so the order never matters.

use voxear_core::math::Vec2;

/// How agents avoid each other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Avoidance {
    /// How far round an agent looks for others, in metres.
    pub look: f32,
    /// How many of the nearest it heeds at most.
    pub neighbours: usize,
    /// How far ahead it keeps clear of them, in seconds: further turns
    /// aside sooner and more gently.
    pub ahead: f32,
}

impl Default for Avoidance {
    fn default() -> Self {
        Self { look: 4.0, neighbours: 10, ahead: 2.0 }
    }
}

/// An agent as avoidance sees it, on the ground plane.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mover {
    pub position: Vec2,
    pub velocity: Vec2,
    /// The velocity it wants: towards where it is going.
    pub wanted: Vec2,
    pub radius: f32,
    /// Its fastest.
    pub speed: f32,
    /// From 0 to 1: the higher, the less it turns aside for those under it.
    /// One that does not avoid at all has 1 and is gone round by the rest.
    pub priority: f32,
    /// Whether it turns aside for others; one that does not is still gone
    /// round.
    pub avoids: bool,
}

/// A line bounding the velocities an agent may choose: those on its left,
/// looking along `direction` from `point`.
#[derive(Clone, Copy, Debug)]
struct Line {
    point: Vec2,
    direction: Vec2,
}

fn det(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Where each mover's neighbours are, by a grid of cells.
pub struct Grid {
    cell: f32,
    cells: std::collections::HashMap<(i32, i32), Vec<u32>>,
}

impl Grid {
    pub fn of(movers: &[Mover], cell: f32) -> Self {
        let cell = cell.max(0.5);
        let mut cells: std::collections::HashMap<(i32, i32), Vec<u32>> = std::collections::HashMap::new();
        for (at, mover) in movers.iter().enumerate() {
            cells.entry(Self::key(mover.position, cell)).or_default().push(at as u32);
        }
        Self { cell, cells }
    }

    fn key(at: Vec2, cell: f32) -> (i32, i32) {
        ((at.x / cell).floor() as i32, (at.y / cell).floor() as i32)
    }

    /// The `most` movers nearest mover `at`, within `look`, nearest first.
    fn near(&self, movers: &[Mover], at: usize, look: f32, most: usize, found: &mut Vec<(f32, u32)>) {
        found.clear();
        let here = movers[at].position;
        let (cx, cz) = Self::key(here, self.cell);
        let reach = (look / self.cell).ceil() as i32;
        let look_sq = look * look;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let Some(list) = self.cells.get(&(cx + dx, cz + dz)) else { continue };
                for &other in list {
                    if other as usize == at {
                        continue;
                    }
                    let far = (movers[other as usize].position - here).length_squared();
                    if far >= look_sq {
                        continue;
                    }
                    if found.len() == most && found.last().is_some_and(|(kept, _)| *kept <= far) {
                        continue;
                    }
                    let place = found.partition_point(|(kept, _)| *kept < far);
                    found.insert(place, (far, other));
                    found.truncate(most);
                }
            }
        }
    }
}

/// The velocity each of `movers` goes at next, avoiding the others, for a
/// step of `delta` seconds.
pub fn avoid(movers: &[Mover], avoidance: &Avoidance, delta: f32) -> Vec<Vec2> {
    let grid = Grid::of(movers, avoidance.look);
    let indices: Vec<u32> = (0..movers.len() as u32).collect();
    let slices: Vec<&[u32]> = indices.chunks(64).collect();
    let work = |slice: &&[u32]| {
        let mut near = Vec::new();
        let mut lines = Vec::new();
        slice.iter().map(|&at| velocity(movers, at as usize, &grid, avoidance, delta, &mut near, &mut lines)).collect()
    };
    let made: Vec<Vec<Vec2>> = match slices.len() > 1 {
        true => voxear_terrain::workers::each(&slices, work),
        false => slices.iter().map(work).collect(),
    };
    made.into_iter().flatten().collect()
}

/// The velocity mover `at` goes at next.
fn velocity(
    movers: &[Mover],
    at: usize,
    grid: &Grid,
    avoidance: &Avoidance,
    delta: f32,
    near: &mut Vec<(f32, u32)>,
    lines: &mut Vec<Line>,
) -> Vec2 {
    let mover = movers[at];
    let wanted = mover.wanted.clamp_length_max(mover.speed);
    if !mover.avoids {
        return wanted;
    }
    grid.near(movers, at, avoidance.look, avoidance.neighbours, near);
    lines.clear();
    let ahead = avoidance.ahead.max(0.1);
    let inverse = 1.0 / ahead;
    for &(_, other) in near.iter() {
        let other = movers[other as usize];
        let apart = other.position - mover.position;
        let closing = mover.velocity - other.velocity;
        let far_sq = apart.length_squared();
        let reach = mover.radius + other.radius;
        let reach_sq = reach * reach;
        // The smallest change that clears the cone, and the line it lies on.
        let (direction, change) = if far_sq > reach_sq {
            let w = closing - apart * inverse;
            let w_sq = w.length_squared();
            let along = w.dot(apart);
            if along < 0.0 && along * along > reach_sq * w_sq {
                // Nearest the cone's rounded end.
                let length = w_sq.sqrt();
                let unit = w / length.max(1e-6);
                (Vec2::new(unit.y, -unit.x), unit * (reach * inverse - length))
            } else {
                // Nearest one of its sides.
                let leg = (far_sq - reach_sq).sqrt();
                let direction = if det(apart, w) > 0.0 {
                    Vec2::new(apart.x * leg - apart.y * reach, apart.x * reach + apart.y * leg) / far_sq
                } else {
                    -Vec2::new(apart.x * leg + apart.y * reach, -apart.x * reach + apart.y * leg) / far_sq
                };
                (direction, direction * closing.dot(direction) - closing)
            }
        } else {
            // Already touching: apart within this step.
            let step = 1.0 / delta.max(1e-3);
            let w = closing - apart * step;
            let length = w.length();
            let unit = w / length.max(1e-6);
            (Vec2::new(unit.y, -unit.x), unit * (reach * step - length))
        };
        // Its share of the change: half each, less for the higher priority, all of it for one the other ignores.
        let share = match other.avoids {
            false => 1.0,
            true => (0.5 + (other.priority - mover.priority) * 0.5).clamp(0.1, 0.9),
        };
        lines.push(Line { point: mover.velocity + change * share, direction });
    }
    let (failed, mut chosen) = linear_program(lines, mover.speed, wanted, false);
    if failed < lines.len() {
        chosen = least_broken(lines, failed, mover.speed, chosen);
    }
    chosen
}

/// The velocity nearest `wanted` (or, with `along`, furthest along it, a
/// unit way) on the left of `lines[at]` and of every line before it, no
/// faster than `speed`; `None` where there is none.
fn on_line(lines: &[Line], at: usize, speed: f32, wanted: Vec2, along: bool) -> Option<Vec2> {
    let line = lines[at];
    let dot = line.point.dot(line.direction);
    let disc = dot * dot + speed * speed - line.point.length_squared();
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (mut left, mut right) = (-dot - root, -dot + root);
    for other in &lines[..at] {
        let denominator = det(line.direction, other.direction);
        let numerator = det(other.direction, line.point - other.point);
        if denominator.abs() <= 1e-6 {
            if numerator < 0.0 {
                return None;
            }
            continue;
        }
        let t = numerator / denominator;
        if denominator >= 0.0 {
            right = right.min(t);
        } else {
            left = left.max(t);
        }
        if left > right {
            return None;
        }
    }
    let t = match along {
        true if wanted.dot(line.direction) > 0.0 => right,
        true => left,
        false => line.direction.dot(wanted - line.point).clamp(left, right),
    };
    Some(line.point + line.direction * t)
}

/// The velocity nearest `wanted` within every line and `speed`: the index
/// of the first line that could not be kept (`lines.len()` where all were),
/// and the best found up to it.
fn linear_program(lines: &[Line], speed: f32, wanted: Vec2, along: bool) -> (usize, Vec2) {
    let mut chosen = match along {
        true => wanted * speed,
        false if wanted.length_squared() > speed * speed => wanted.normalize_or_zero() * speed,
        false => wanted,
    };
    for at in 0..lines.len() {
        if det(lines[at].direction, lines[at].point - chosen) > 0.0 {
            match on_line(lines, at, speed, wanted, along) {
                Some(found) => chosen = found,
                None => return (at, chosen),
            }
        }
    }
    (lines.len(), chosen)
}

/// Where the lines from `from` on leave no velocity: the one that breaks
/// the worst of them least.
fn least_broken(lines: &[Line], from: usize, speed: f32, mut chosen: Vec2) -> Vec2 {
    let mut distance = 0.0;
    for at in from..lines.len() {
        if det(lines[at].direction, lines[at].point - chosen) <= distance {
            continue;
        }
        // Each line before it, moved to where it meets this one.
        let mut moved: Vec<Line> = Vec::with_capacity(at);
        for before in &lines[..at] {
            let determinant = det(lines[at].direction, before.direction);
            let point = if determinant.abs() <= 1e-6 {
                if lines[at].direction.dot(before.direction) > 0.0 {
                    continue;
                }
                (lines[at].point + before.point) * 0.5
            } else {
                lines[at].point
                    + lines[at].direction * (det(before.direction, lines[at].point - before.point) / determinant)
            };
            moved.push(Line { point, direction: (before.direction - lines[at].direction).normalize_or_zero() });
        }
        let away = Vec2::new(-lines[at].direction.y, lines[at].direction.x);
        let (failed, found) = linear_program(&moved, speed, away, true);
        if failed == moved.len() {
            chosen = found;
        }
        distance = det(lines[at].direction, lines[at].point - chosen);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mover(x: f32, z: f32, wanted: Vec2) -> Mover {
        Mover {
            position: Vec2::new(x, z),
            velocity: wanted,
            wanted,
            radius: 0.4,
            speed: 1.5,
            priority: 0.5,
            avoids: true,
        }
    }

    /// Steps `movers` on for `seconds`, each frame avoiding: the closest any
    /// two came, edge to edge.
    fn walk(movers: &mut [Mover], goals: &[Vec2], seconds: f32) -> f32 {
        let delta = 1.0 / 30.0;
        let mut closest = f32::INFINITY;
        for _ in 0..(seconds / delta) as usize {
            for (mover, goal) in movers.iter_mut().zip(goals) {
                let way = *goal - mover.position;
                mover.wanted = if way.length() < 0.05 {
                    Vec2::ZERO
                } else {
                    way.normalize() * mover.speed.min(way.length() / delta)
                };
            }
            let chosen = avoid(movers, &Avoidance::default(), delta);
            for (mover, velocity) in movers.iter_mut().zip(chosen) {
                mover.velocity = velocity;
                mover.position += velocity * delta;
            }
            for a in 0..movers.len() {
                for b in a + 1..movers.len() {
                    let gap = (movers[a].position - movers[b].position).length() - movers[a].radius - movers[b].radius;
                    closest = closest.min(gap);
                }
            }
        }
        closest
    }

    #[test]
    fn two_walking_at_each_other_both_step_aside_and_get_past() {
        let mut movers = [mover(-4.0, 0.0, Vec2::X), mover(4.0, 0.0, -Vec2::X)];
        let goals = [Vec2::new(4.0, 0.0), Vec2::new(-4.0, 0.0)];
        let closest = walk(&mut movers, &goals, 12.0);
        assert!(closest > -0.02, "they never walked into each other: {closest}");
        for (mover, goal) in movers.iter().zip(goals) {
            assert!((mover.position - goal).length() < 0.2, "got past: {:?}", mover.position);
        }
    }

    #[test]
    fn one_that_does_not_avoid_is_gone_round_by_the_other() {
        let mut movers = [mover(-4.0, 0.05, Vec2::X), mover(0.0, 0.0, Vec2::ZERO)];
        movers[1].avoids = false;
        let goals = [Vec2::new(4.0, 0.0), Vec2::new(0.0, 0.0)];
        let closest = walk(&mut movers, &goals, 10.0);
        assert!(closest > -0.05, "{closest}");
        assert!((movers[1].position).length() < 0.01, "it stood its ground");
        assert!((movers[0].position - goals[0]).length() < 0.2, "{:?}", movers[0].position);
    }

    #[test]
    fn a_crowd_crossing_a_square_both_ways_gets_through_without_walking_into_each_other() {
        let mut movers = Vec::new();
        let mut goals = Vec::new();
        for row in 0..5 {
            for column in 0..4 {
                let (x, z) = (-8.0 - column as f32 * 1.2, row as f32 * 1.2 - 2.4);
                movers.push(mover(x, z, Vec2::X));
                goals.push(Vec2::new(-x, z));
                movers.push(mover(-x, z + 0.3, -Vec2::X));
                goals.push(Vec2::new(x, z + 0.3));
            }
        }
        let closest = walk(&mut movers, &goals, 40.0);
        assert!(closest > -0.1, "overlapping at most a hand's breadth: {closest}");
        let home = movers.iter().zip(&goals).filter(|(mover, goal)| (mover.position - **goal).length() < 0.5).count();
        assert!(home >= movers.len() * 9 / 10, "{home} of {} got across", movers.len());
    }
}
