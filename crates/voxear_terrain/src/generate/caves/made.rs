//! A cave room made by rules: where a prefab's room is the shapes someone
//! set, this one is worked out from a few numbers and a seed, so every room
//! of a network is its own.
//!
//! A made room is a handful of round shapes that overlap, joined smoothly
//! into one hollow; a level floor through its lowest third; pillars from
//! floor to roof, rock hanging from the roof and ledges against the walls;
//! and exits round it for tunnels to join. Its walls are roughened by the
//! network's noise, which may push them out as far as it likes but in no
//! further than a margin the room works out for itself: less than what
//! holds its shapes together and keeps its exits open, so roughness never
//! closes it.
//!
//! Only sums, products, divisions, square roots and comparisons, and the
//! same order whatever asks: a seed makes the same room on every machine.

use serde::{Deserialize, Serialize};

use super::{
    grid::V3,
    network::{mix, share},
    rooms::{NOTHING, RoomExit, ball, greater, lesser},
};
use crate::{
    generate::{
        compile::Key,
        nodes::{Value, ValueSpec},
    },
    noise::gradient3,
};

/// How high over a room's floor its exits lie, in metres.
const EXIT_HEIGHT: f32 = 1.2;

/// The rules a cave room is made by.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomRules {
    /// How wide a room is, from and to, in metres.
    pub size: [f32; 2],
    /// How many round shapes it is joined from.
    pub shapes: u32,
    /// How high it is, in metres.
    pub height: f32,
    /// How far its walls are pushed in and out, in metres.
    pub roughness: f32,
    /// How flat its floor is, 0 to 1: at 1 a level floor through its
    /// lowest third, at 0 the bowl its shapes leave.
    pub floor: f32,
    pub pillars: u32,
    /// How many rocks hang from its roof.
    pub hanging: u32,
    pub ledges: u32,
    /// How many exits it offers the tunnels, and how far apart round it
    /// they are kept, 0 to 1.
    pub exits: u32,
    pub exits_apart: f32,
}

impl Default for RoomRules {
    fn default() -> Self {
        Self {
            size: [18.0, 30.0],
            shapes: 4,
            height: 9.0,
            roughness: 1.0,
            floor: 0.7,
            pillars: 2,
            hanging: 6,
            ledges: 2,
            exits: 4,
            exits_apart: 0.7,
        }
    }
}

impl RoomRules {
    /// The rules held to what a room can be made from.
    pub(crate) fn kept(&self) -> Self {
        let from = self.size[0].clamp(6.0, 400.0);
        Self {
            size: [from, self.size[1].clamp(from, 400.0)],
            shapes: self.shapes.clamp(1, 8),
            height: self.height.clamp(4.0, 120.0),
            roughness: self.roughness.clamp(0.0, 8.0),
            floor: self.floor.clamp(0.0, 1.0),
            pillars: self.pillars.min(12),
            hanging: self.hanging.min(32),
            ledges: self.ledges.min(8),
            exits: self.exits.clamp(1, 8),
            exits_apart: self.exits_apart.clamp(0.0, 1.0),
        }
    }

    /// No place in a room made by these, and none of its exits, is further
    /// than this from its origin.
    pub(crate) fn reach(&self) -> f32 {
        let kept = self.kept();
        let (wide, high) = (kept.size[1] * 0.7, kept.height * 1.3);
        (wide * wide + high * high).sqrt() + kept.roughness + 1.0
    }

    /// Its values as rows for an Inspector, in the order [`RoomRules::value`] counts them.
    pub const VALUES: &'static [ValueSpec] = &[
        ValueSpec::number("size", "Size", 6.0, 400.0, "m"),
        ValueSpec::number("size_to", "Largest", 6.0, 400.0, "m"),
        ValueSpec::whole("shapes", "Shapes", 1, 8),
        ValueSpec::number("height", "Height", 4.0, 120.0, "m"),
        ValueSpec::number("roughness", "Roughness", 0.0, 8.0, "m"),
        ValueSpec::number("floor", "Floor", 0.0, 1.0, ""),
        ValueSpec::whole("pillars", "Pillars", 0, 12),
        ValueSpec::whole("hanging", "Hanging Rock", 0, 32),
        ValueSpec::whole("ledges", "Ledges", 0, 8),
        ValueSpec::whole("exits", "Exits", 1, 8),
        ValueSpec::number("exits_apart", "Exits Apart", 0.0, 1.0, ""),
    ];

    /// Value `at` among its values, as its row shows it.
    pub fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Number(self.size[0]),
            1 => Value::Number(self.size[1]),
            2 => Value::Whole(self.shapes),
            3 => Value::Number(self.height),
            4 => Value::Number(self.roughness),
            5 => Value::Number(self.floor),
            6 => Value::Whole(self.pillars),
            7 => Value::Whole(self.hanging),
            8 => Value::Whole(self.ledges),
            9 => Value::Whole(self.exits),
            10 => Value::Number(self.exits_apart),
            _ => return None,
        })
    }

    /// Sets value `at`, held to its row's range, its largest size never under its least.
    pub fn set_value(&mut self, at: usize, value: Value) {
        let Some(kept) = Self::VALUES.get(at).and_then(|spec| spec.kept(value)) else { return };
        match (at, kept) {
            (0, Value::Number(number)) => self.size = [number, greater(self.size[1], number)],
            (1, Value::Number(number)) => self.size = [lesser(self.size[0], number), number],
            (2, Value::Whole(whole)) => self.shapes = whole,
            (3, Value::Number(number)) => self.height = number,
            (4, Value::Number(number)) => self.roughness = number,
            (5, Value::Number(number)) => self.floor = number,
            (6, Value::Whole(whole)) => self.pillars = whole,
            (7, Value::Whole(whole)) => self.hanging = whole,
            (8, Value::Whole(whole)) => self.ledges = whole,
            (9, Value::Whole(whole)) => self.exits = whole,
            (10, Value::Number(number)) => self.exits_apart = number,
            _ => {}
        }
    }

    /// One room these rules make, from `seed`, to look at by itself.
    pub fn sample(&self, seed: u32) -> RoomSample {
        RoomSample { made: Made::of(self, seed), seed }
    }

    pub(crate) fn key(&self, key: &mut Key) {
        key.name("rules").number(self.size[0]).number(self.size[1]).whole(self.shapes).number(self.height);
        key.number(self.roughness).number(self.floor).whole(self.pillars).whole(self.hanging);
        key.whole(self.ledges).whole(self.exits).number(self.exits_apart);
    }
}

/// A room made by rules, by itself: for a picture of what the rules make.
#[derive(Clone, Debug)]
pub struct RoomSample {
    made: Made,
    seed: u32,
}

impl RoomSample {
    /// How far inside its hollow `p` is, in metres, a place in the room's own space: its walls roughened as
    /// a network roughens them.
    pub fn inside(&self, p: [f32; 3]) -> f32 {
        let grain = 4.0 + 2.0 * self.made.rough;
        self.made.inside(p, Some(gradient3(p[0] / grain, p[1] / grain, p[2] / grain, self.seed.wrapping_add(11))))
    }

    /// No place in it is further than this from its origin.
    pub fn reach(&self) -> f32 {
        self.made.reach()
    }

    /// The height of its floor at its middle.
    pub fn floor(&self) -> f32 {
        self.made.stood()
    }

    /// Where its exits are, and the way out of each.
    pub fn exits(&self) -> impl Iterator<Item = ([f32; 3], [f32; 3])> + '_ {
        self.made.exits.iter().map(|exit| (exit.at, exit.way))
    }
}

/// One of a made room's round shapes: a ball of these half sizes round
/// `at`, or with `half` its upper half, flat side down at `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Blob {
    at: V3,
    half_sizes: V3,
    half: bool,
}

impl Blob {
    fn inside(&self, p: V3) -> f32 {
        let q = [p[0] - self.at[0], p[1] - self.at[1], p[2] - self.at[2]];
        let round = ball(q, self.half_sizes);
        if self.half { lesser(round, q[1]) } else { round }
    }

    fn bottom(&self) -> f32 {
        if self.half { self.at[1] } else { self.at[1] - self.half_sizes[1] }
    }

    fn top(&self) -> f32 {
        self.at[1] + self.half_sizes[1]
    }

    /// The lesser of its half sizes along the floor.
    fn across(&self) -> f32 {
        lesser(self.half_sizes[0], self.half_sizes[2])
    }
}

/// Rock put back into a made room.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rock {
    /// A pillar from floor to roof round `(x, z)`, `radius` wide at its
    /// waist, between these heights.
    Pillar { x: f32, z: f32, radius: f32, low: f32, high: f32 },
    /// Rock hanging from the roof over `(x, z)`: `radius` wide at `top`,
    /// coming to a point at `tip`.
    Hanging { x: f32, z: f32, radius: f32, top: f32, tip: f32 },
    /// A ledge against a wall: level at `top`, `radius` round `(x, z)`.
    Ledge { x: f32, z: f32, radius: f32, top: f32 },
}

impl Rock {
    /// How far inside the rock `p` is, in metres.
    fn inside(&self, p: V3) -> f32 {
        let away = |x: f32, z: f32| ((p[0] - x) * (p[0] - x) + (p[2] - z) * (p[2] - z)).sqrt();
        match *self {
            Rock::Pillar { x, z, radius, low, high } => {
                // Wider towards the floor and the roof than at its waist.
                let t = greater(lesser((p[1] - low) / greater(high - low, 0.1), 1.0), 0.0) * 2.0 - 1.0;
                radius * (1.0 + 0.9 * t * t) - away(x, z)
            }
            Rock::Hanging { x, z, radius, top, tip } => {
                let wide = radius * (p[1] - tip) / greater(top - tip, 0.1);
                lesser(lesser(wide, radius) - away(x, z), p[1] - tip)
            }
            Rock::Ledge { x, z, radius, top } => lesser(radius - away(x, z), top - p[1]),
        }
    }
}

/// A room made by rules from a seed, in its own space: x and z along its
/// floor, y up, its origin on its floor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Made {
    blobs: Vec<Blob>,
    /// The ways between shapes that overlap, along the floor: from one's middle to the other's.
    necks: Vec<[f32; 4]>,
    rocks: Vec<Rock>,
    /// How smoothly its shapes run together, in metres.
    blend: f32,
    /// The height of its level floor: none where its rules leave a bowl.
    floor: Option<f32>,
    /// How far its walls are pushed in and out, and the furthest in.
    rough: f32,
    margin: f32,
    pub exits: Vec<RoomExit>,
    /// Its lowest and highest places before roughness.
    pub low: f32,
    pub high: f32,
}

/// The greater of two numbers, run together smoothly over `blend`.
fn joined(a: f32, b: f32, blend: f32) -> f32 {
    let apart = if a > b { a - b } else { b - a };
    let near = greater(blend - apart, 0.0) / blend;
    greater(a, b) + near * near * blend * 0.25
}

/// A way along the floor, one long, from two numbers 0 to 1.
fn way(a: f32, b: f32) -> [f32; 2] {
    let (x, z) = (a * 2.0 - 1.0, b * 2.0 - 1.0);
    let length = (x * x + z * z).sqrt();
    if length > 1.0e-3 { [x / length, z / length] } else { [1.0, 0.0] }
}

impl Made {
    /// The room `rules` make from `seed`.
    pub fn of(rules: &RoomRules, seed: u32) -> Made {
        let rules = rules.kept();
        let roll = |part: i32, at: i32| share(mix(seed, &[part, at]));
        let between = |low: f32, high: f32, roll: f32| low + (high - low) * roll;
        let width = between(rules.size[0], rules.size[1], roll(0, 0));
        let across = width * 0.5;
        let count = rules.shapes as usize;
        let mut blobs: Vec<Blob> = Vec::with_capacity(count);
        let mut necks = Vec::new();
        for at in 0..count {
            let part = at as i32;
            // The first holds the room's middle; each other overlaps the first, or one that does.
            let (middle, radius) = match at {
                0 => ([0.0, 0.0], across * if count == 1 { 1.0 } else { 0.62 }),
                _ => {
                    let ring = 1 + (count - 1).div_ceil(2);
                    let onto = if at < ring { 0 } else { 1 + (mix(seed, &[1, part]) as usize % (ring - 1)) };
                    let parent = blobs[onto];
                    let [x, z] = way(roll(2, part), roll(3, part));
                    let far = parent.across() * between(0.55, 0.85, roll(4, part));
                    let wide = match onto {
                        0 => between(0.36, 0.5, roll(5, part)),
                        _ => between(0.3, 0.4, roll(5, part)),
                    };
                    let middle = [parent.at[0] + x * far, parent.at[2] + z * far];
                    necks.push([parent.at[0], parent.at[2], middle[0], middle[1]]);
                    (middle, across * wide)
                }
            };
            let tall = rules.height * between(0.75, 1.0, roll(6, part));
            let form = roll(7, part);
            let sides = [radius * between(0.85, 1.15, roll(8, part)), radius * between(0.85, 1.15, roll(9, part))];
            // A third each: a half ball standing on the floor, a ball squashed, a ball.
            let blob = if form < 0.34 {
                Blob { at: [middle[0], 0.0, middle[1]], half_sizes: [sides[0], tall, sides[1]], half: true }
            } else {
                let high = if form < 0.67 { tall * 0.72 } else { tall };
                let lift = if at == 0 { 0.0 } else { rules.height * between(-0.08, 0.12, roll(10, part)) };
                let at = [middle[0], high * 0.5 + lift, middle[1]];
                Blob { at, half_sizes: [sides[0], high * 0.5, sides[1]], half: false }
            };
            blobs.push(blob);
        }
        let low = blobs.iter().map(Blob::bottom).fold(f32::MAX, lesser);
        let high = blobs.iter().map(Blob::top).fold(f32::MIN, greater);
        // The floor: through the lowest third at its flattest. Left a bowl, the room's floor is its lowest place.
        let floor = (rules.floor > 0.0).then(|| low + (high - low) * rules.floor / 3.0);
        let stood = floor.unwrap_or(low);
        let least = blobs.iter().map(|blob| lesser(blob.across(), blob.half_sizes[1])).fold(f32::MAX, lesser);
        let margin = lesser(rules.roughness, least * 0.2);
        let blend = greater(lesser(across * 0.25, 3.0), 0.5);
        let (rocks, exits, rough) = (Vec::new(), Vec::new(), rules.roughness);
        let mut made = Made { blobs, necks, rocks, blend, floor, rough, margin, exits, low, high };
        made.exits = made.exits_of(&rules, seed, stood);
        made.rocks = made.rocks_of(&rules, seed, stood);
        made
    }

    /// Exits round the room: each the way among a few tried that lies
    /// furthest from those before it, set where the hollow ends that way,
    /// far enough in that roughness cannot close it.
    fn exits_of(&self, rules: &RoomRules, seed: u32, stood: f32) -> Vec<RoomExit> {
        let roll = |part: i32, at: i32| share(mix(seed, &[part, at]));
        let tries = 1 + (rules.exits_apart * 12.0) as i32;
        let mut ways: Vec<[f32; 2]> = Vec::new();
        for exit in 0..rules.exits as i32 {
            let mut best: Option<(f32, [f32; 2])> = None;
            for attempt in 0..tries {
                let tried = way(roll(20 + attempt, exit), roll(40 + attempt, exit));
                let nearest = ways.iter().map(|other| tried[0] * other[0] + tried[1] * other[1]).fold(-1.0, greater);
                if best.is_none_or(|(least, _)| nearest < least) {
                    best = Some((nearest, tried));
                }
            }
            ways.extend(best.map(|(_, way)| way));
        }
        // Roughness leaves a floor where it is and moves the walls, so an exit is set by the walls alone: as
        // far in from them as they may come and a metre more. Over a bowl, as far over its bottom too.
        let open = self.margin + 1.0;
        let height = stood + if self.floor.is_some() { EXIT_HEIGHT } else { greater(EXIT_HEIGHT, open + 0.5) };
        let half_way = (height + self.high) * 0.5;
        let exits = ways.into_iter().map(|[x, z]| {
            // Out from the middle to where the hollow is no longer well open, at the exit's height or half
            // way up to the roof, whichever is wider there; then back in until the exit itself is.
            let at = |far: f32, height: f32| self.shell([x * far, height, z * far]);
            let wide = |far: f32| greater(at(far, height), at(far, half_way));
            let mut far = 0.0;
            while far < 600.0 && wide(far + 0.5) >= open {
                far += 0.5;
            }
            while far > 0.0 && self.shell([x * far, height, z * far]) < self.margin + 0.4 {
                far -= 0.5;
            }
            RoomExit { at: [x * far, height, z * far], way: [x, 0.0, z] }
        });
        exits.collect()
    }

    /// Pillars, hanging rock and ledges, each tried a few places and left
    /// out where it would stand in the way to an exit.
    fn rocks_of(&self, rules: &RoomRules, seed: u32, stood: f32) -> Vec<Rock> {
        let roll = |part: i32, at: i32| share(mix(seed, &[part, at]));
        let between = |low: f32, high: f32, roll: f32| low + (high - low) * roll;
        let count = self.blobs.len();
        let blob = |part: i32, at: i32| self.blobs[mix(seed, &[part, at]) as usize % count];
        // How near a place along the floor lies to the ways that must stay open: from the middle to each
        // exit, and between shapes that overlap.
        let from_ways = |x: f32, z: f32| {
            let exits = self.exits.iter().map(|exit| [0.0, 0.0, exit.at[0], exit.at[2]]);
            let near = exits.chain(self.necks.iter().copied()).map(|[ax, az, bx, bz]| {
                let (wx, wz) = (bx - ax, bz - az);
                let length = wx * wx + wz * wz;
                let along = ((x - ax) * wx + (z - az) * wz) / greater(length, 1.0e-6);
                let t = greater(lesser(along, 1.0), 0.0);
                let (dx, dz) = (x - ax - wx * t, z - az - wz * t);
                (dx * dx + dz * dz).sqrt()
            });
            near.fold(f32::MAX, lesser)
        };
        let mut rocks = Vec::new();
        for pillar in 0..rules.pillars as i32 {
            for attempt in 0..10 {
                let (part, on) = (100 + attempt * 8, blob(100 + attempt * 8, pillar));
                let [wx, wz] = way(roll(part + 1, pillar), roll(part + 2, pillar));
                let far = on.across() * between(0.2, 0.6, roll(part + 3, pillar));
                let (x, z) = (on.at[0] + wx * far, on.at[2] + wz * far);
                let radius = on.across() * between(0.06, 0.11, roll(part + 4, pillar));
                // Its foot is about twice its waist. None in a shape too small to walk round one in.
                if radius < 0.35 || from_ways(x, z) < radius * 2.0 + 1.2 {
                    continue;
                }
                rocks.push(Rock::Pillar { x, z, radius, low: stood, high: on.top() });
                break;
            }
        }
        for hanging in 0..rules.hanging as i32 {
            let on = blob(200, hanging);
            let [wx, wz] = way(roll(201, hanging), roll(202, hanging));
            let far = on.across() * between(0.0, 0.6, roll(203, hanging));
            let (top, room) = (on.top() + 1.0, on.top() - stood);
            // Never further down than leaves standing room under it.
            let long = lesser(room * between(0.15, 0.45, roll(204, hanging)), room - 2.2);
            if long < 0.5 {
                continue;
            }
            let radius = greater(long * between(0.25, 0.45, roll(205, hanging)), 0.4);
            let (x, z) = (on.at[0] + wx * far, on.at[2] + wz * far);
            rocks.push(Rock::Hanging { x, z, radius, top, tip: top - 1.0 - long });
        }
        for ledge in 0..rules.ledges as i32 {
            for attempt in 0..6 {
                let (part, on) = (300 + attempt * 8, blob(300 + attempt * 8, ledge));
                let [wx, wz] = way(roll(part + 1, ledge), roll(part + 2, ledge));
                let radius = on.across() * between(0.3, 0.5, roll(part + 3, ledge));
                let (x, z) = (on.at[0] + wx * on.across() * 0.9, on.at[2] + wz * on.across() * 0.9);
                if from_ways(x, z) < radius + 2.5 || (x * x + z * z).sqrt() < radius + 2.0 {
                    continue;
                }
                let top = stood + (on.top() - stood) * between(0.18, 0.36, roll(part + 4, ledge));
                rocks.push(Rock::Ledge { x, z, radius, top });
                break;
            }
        }
        rocks
    }

    /// How far inside its shapes run together `p` is: its walls and roof,
    /// with no floor, no rock put back and no roughness.
    fn shell(&self, p: V3) -> f32 {
        self.blobs.iter().fold(NOTHING, |hollow, blob| joined(hollow, blob.inside(p), self.blend))
    }

    /// How far inside its hollow `p` is, in metres, a place in its own
    /// space: its walls moved by `noise`, about -1 to 1, where it is given,
    /// times its roughness and inward no further than its margin.
    pub fn inside(&self, p: V3, noise: Option<f32>) -> f32 {
        let moved = noise.map_or(0.0, |noise| greater(noise * self.rough, -self.margin));
        let mut hollow = self.shell(p) + moved;
        if let Some(floor) = self.floor {
            hollow = lesser(hollow, p[1] - floor);
        }
        for rock in &self.rocks {
            // Rock put back is roughened a third as much, and never grows by it.
            hollow = lesser(hollow, -(rock.inside(p) + lesser(moved, 0.0) * 0.3));
        }
        hollow
    }

    /// No place in it, and none of its exits, is further than this from
    /// its origin.
    pub fn reach(&self) -> f32 {
        let far = |v: V3| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let blobs = self.blobs.iter().map(|blob| {
            let [x, y, z] = blob.half_sizes;
            far(blob.at) + greater(x, greater(y, z))
        });
        blobs.chain(self.exits.iter().map(|exit| far(exit.at))).fold(1.0, greater) + self.rough + self.blend
    }

    /// The height of its floor at its middle.
    pub fn stood(&self) -> f32 {
        self.floor.unwrap_or(self.low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many open cells of `made`, half a metre a side and with its walls roughened by noise from
    /// `seed`, are not reached from its middle, and how many open cells there are.
    fn whole(made: &Made, seed: u32) -> (usize, usize) {
        let reach = made.reach().ceil() as i32 * 2 + 2;
        let side = (reach * 2 + 1) as usize;
        let place = |cell: [i32; 3]| cell.map(|v| v as f32 * 0.5);
        // The noise as coarse as a network makes it for walls this rough.
        let grain = 4.0 + 2.0 * made.rough;
        let open = |cell: [i32; 3]| {
            let p = place(cell);
            made.inside(p, Some(gradient3(p[0] / grain, p[1] / grain, p[2] / grain, seed))) > 0.0
        };
        let index = |cell: [i32; 3]| {
            ((cell[1] + reach) as usize * side + (cell[2] + reach) as usize) * side + (cell[0] + reach) as usize
        };
        let mut opens = vec![false; side * side * side];
        let mut count = 0;
        for y in -reach..=reach {
            for z in -reach..=reach {
                for x in -reach..=reach {
                    if open([x, y, z]) {
                        opens[index([x, y, z])] = true;
                        count += 1;
                    }
                }
            }
        }
        let start = [0, ((made.stood() + 1.0) * 2.0) as i32, 0];
        assert!(opens[index(start)], "its middle is open");
        let mut seen = vec![false; opens.len()];
        seen[index(start)] = true;
        let (mut next, mut reached) = (vec![start], 1);
        while let Some(cell) = next.pop() {
            for step in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
                let to = [cell[0] + step[0], cell[1] + step[1], cell[2] + step[2]];
                if to.iter().any(|v| v.abs() > reach) {
                    continue;
                }
                let at = index(to);
                if opens[at] && !seen[at] {
                    seen[at] = true;
                    reached += 1;
                    next.push(to);
                }
            }
        }
        (count - reached, count)
    }

    #[test]
    fn a_made_room_is_one_hollow_whatever_its_seed_and_roughness_never_closes_it_or_its_exits() {
        let kinds = [
            RoomRules::default(),
            RoomRules { size: [8.0, 12.0], shapes: 8, height: 4.0, roughness: 8.0, ..RoomRules::default() },
            RoomRules {
                size: [40.0, 60.0],
                shapes: 6,
                height: 22.0,
                pillars: 12,
                hanging: 32,
                ledges: 8,
                ..RoomRules::default()
            },
            RoomRules { shapes: 1, floor: 0.0, exits: 8, roughness: 3.0, ..RoomRules::default() },
        ];
        for (kind, rules) in kinds.iter().enumerate() {
            for seed in 0..12 {
                let made = Made::of(rules, seed * 7919 + kind as u32);
                assert!(made.reach() <= rules.reach(), "{kind} {seed}: {} past {}", made.reach(), rules.reach());
                if rules.size[1] <= 30.0 {
                    // One hollow, but for a speck or two where a ledge's rim meets a wall.
                    let (lost, cells) = whole(&made, seed + 100);
                    let whole = lost * 400 <= cells && cells > 200;
                    assert!(whole, "room {kind} of seed {seed}: {lost} of {cells} cut off");
                }
                assert_eq!(made.exits.len(), rules.exits as usize);
                for exit in &made.exits {
                    // Open at the exit and a metre back from it, at the roughest the noise gets either way.
                    for back in [0.0, 0.5] {
                        let p = [exit.at[0] - exit.way[0] * back, exit.at[1], exit.at[2] - exit.way[2] * back];
                        for noise in [-1.5, 0.0, 1.5] {
                            let open = made.inside(p, Some(noise)) > 0.0;
                            assert!(open, "{kind} {seed}: exit {exit:?} closed at {noise}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_made_room_follows_its_rules_and_the_same_seed_makes_the_same_room() {
        let rules = RoomRules { size: [20.0, 20.0], pillars: 3, hanging: 5, ledges: 2, ..RoomRules::default() };
        let (made, again, other) = (Made::of(&rules, 5), Made::of(&rules, 5), Made::of(&rules, 6));
        assert!(made == again && made != other);
        assert_eq!(made.blobs.len(), 4);
        // A level floor: open just over it across the middle, rock just under it.
        let floor = made.stood();
        assert!(made.inside([0.0, floor + 0.5, 0.0], None) > 0.4 && made.inside([0.0, floor - 0.5, 0.0], None) < -0.4);
        let bare = Made { rocks: Vec::new(), ..made.clone() };
        assert!((bare.inside([2.0, floor + 0.25, 1.0], None) - 0.25).abs() < 1e-4, "level");
        let bowl = Made::of(&RoomRules { floor: 0.0, ..rules }, 5);
        assert!(bowl.stood() < floor && bowl.inside([0.0, floor - 0.5, 0.0], None) > 0.0);
        // Rock is put back: a pillar is rock from floor to roof, and hanging rock leaves standing room.
        let pillars: Vec<&Rock> = made.rocks.iter().filter(|rock| matches!(rock, Rock::Pillar { .. })).collect();
        assert!(!pillars.is_empty());
        for rock in &made.rocks {
            match *rock {
                Rock::Pillar { x, z, low, high, .. } => {
                    for t in [0.1, 0.5, 0.9] {
                        assert!(made.inside([x, low + (high - low) * t, z], None) < 0.0);
                    }
                }
                Rock::Hanging { tip, .. } => assert!(tip - floor >= 2.2 - 1e-3, "{tip} over {floor}"),
                Rock::Ledge { top, .. } => assert!(top > floor && top < floor + rules.height * 0.5),
            }
        }
        // Exits kept apart lie further from each other than exits left where they fall.
        let nearest = |made: &Made| {
            let mut most = -1.0f32;
            for (at, a) in made.exits.iter().enumerate() {
                for b in &made.exits[at + 1..] {
                    most = most.max(a.way[0] * b.way[0] + a.way[2] * b.way[2]);
                }
            }
            most
        };
        let (mut apart, mut loose) = (0.0, 0.0);
        for seed in 0..20 {
            apart += nearest(&Made::of(&RoomRules { exits_apart: 1.0, ..rules }, seed));
            loose += nearest(&Made::of(&RoomRules { exits_apart: 0.0, ..rules }, seed));
        }
        assert!(apart < loose - 4.0, "{apart} {loose}");
    }
}
