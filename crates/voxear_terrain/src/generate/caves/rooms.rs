//! A cave room as a generator keeps it: the shapes that hollow it and the
//! exits tunnels join it by, as plain numbers.
//!
//! A room is made in the editor as a prefab of Cave Shape and Cave Exit
//! Objs. The editor reads the prefab into these numbers and writes them
//! into the generator's file beside the prefab's reference, so making
//! ground needs no prefab, and every machine reads the same numbers.
//!
//! Everything is in the room's own space, in metres: x and z along its
//! floor, y up, its origin where the prefab's is.

use serde::{Deserialize, Serialize};

use crate::generate::compile::Key;

/// What nothing hollow is worth: further outside any hollow than anything
/// is.
pub(crate) const NOTHING: f32 = -1.0e9;

/// The form of one of a room's shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomForm {
    /// A ball as wide, high and long as its size.
    #[default]
    Ball,
    /// The upper half of a ball, its flat side down on the shape's origin
    /// and its top as high as its size.
    HalfBall,
    /// A box as wide, high and long as its size, round its origin.
    Box,
}

impl RoomForm {
    pub const ALL: [RoomForm; 3] = [RoomForm::Ball, RoomForm::HalfBall, RoomForm::Box];

    pub fn name(self) -> &'static str {
        match self {
            RoomForm::Ball => "ball",
            RoomForm::HalfBall => "half_ball",
            RoomForm::Box => "box",
        }
    }
}

/// A shape turned no way at all.
pub const UNTURNED: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn unturned(turn: &[f32; 9]) -> bool {
    *turn == UNTURNED
}

fn not(fill: &bool) -> bool {
    !*fill
}

/// One of a room's shapes: a form of some size, set somewhere in the room
/// and turned, that hollows rock out or, as a fill, puts it back into what
/// the shapes before it hollowed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomShape {
    pub shape: RoomForm,
    /// Puts rock back rather than taking it out.
    #[serde(skip_serializing_if = "not")]
    pub fill: bool,
    /// How likely a room placed in the ground has it, 0 to 1: with less
    /// than 1, one room gives many.
    pub chance: f32,
    /// How far in from its surface the walls' roughness may come, in
    /// metres.
    pub margin: f32,
    pub at: [f32; 3],
    pub size: [f32; 3],
    /// How it is turned: the room's x, y and z of its own x axis, then of
    /// its y, then of its z.
    #[serde(skip_serializing_if = "unturned")]
    pub turn: [f32; 9],
}

impl Default for RoomShape {
    fn default() -> Self {
        Self {
            shape: RoomForm::Ball,
            fill: false,
            chance: 1.0,
            margin: 0.5,
            at: [0.0; 3],
            size: [8.0, 5.0, 8.0],
            turn: UNTURNED,
        }
    }
}

/// The greater of two numbers by comparing, which says the same on every
/// machine.
pub(crate) fn greater(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

pub(crate) fn lesser(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

impl RoomShape {
    /// How far inside it `p` is, in metres, a place in the room's space:
    /// outside where negative. Exact for a box; for a ball that is not
    /// round, near enough at its surface.
    pub(crate) fn inside(&self, p: [f32; 3]) -> f32 {
        let d = [p[0] - self.at[0], p[1] - self.at[1], p[2] - self.at[2]];
        let t = &self.turn;
        // Into its own space: along each of its axes.
        let q = [
            d[0] * t[0] + d[1] * t[1] + d[2] * t[2],
            d[0] * t[3] + d[1] * t[4] + d[2] * t[5],
            d[0] * t[6] + d[1] * t[7] + d[2] * t[8],
        ];
        let size = self.size.map(|side| greater(side, 0.01));
        match self.shape {
            RoomForm::Box => {
                let over = [q[0].abs() - size[0] * 0.5, q[1].abs() - size[1] * 0.5, q[2].abs() - size[2] * 0.5];
                let most = greater(over[0], greater(over[1], over[2]));
                if most <= 0.0 {
                    return -most;
                }
                let out = over.map(|side| greater(side, 0.0));
                -(out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt()
            }
            RoomForm::Ball => ball(q, [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5]),
            RoomForm::HalfBall => lesser(ball(q, [size[0] * 0.5, size[1], size[2] * 0.5]), q[1]),
        }
    }

    /// No place in it is further than this from its origin in the room.
    pub(crate) fn reach(&self) -> f32 {
        let [x, y, z] = self.at;
        let [w, h, l] = self.size;
        let half = match self.shape {
            RoomForm::HalfBall => (w * w * 0.25 + h * h + l * l * 0.25).sqrt(),
            _ => (w * w + h * h + l * l).sqrt() * 0.5,
        };
        (x * x + y * y + z * z).sqrt() + half
    }

    fn key(&self, key: &mut Key) {
        key.text(self.shape.name()).whole(u32::from(self.fill)).number(self.chance).number(self.margin);
        for number in self.at.iter().chain(&self.size).chain(&self.turn) {
            key.number(*number);
        }
    }
}

/// How far inside a ball of half sizes `r` round nothing `q` is.
fn ball(q: [f32; 3], r: [f32; 3]) -> f32 {
    let least = lesser(r[0], lesser(r[1], r[2]));
    let a = [q[0] / r[0], q[1] / r[1], q[2] / r[2]];
    let b = [a[0] / r[0], a[1] / r[1], a[2] / r[2]];
    let k0 = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let k1 = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    // At its very middle, as far in as its least half size.
    if k1 <= 0.0 { least } else { lesser(k0 * (1.0 - k0) / k1, least) }
}

/// Where a tunnel may join a room, and the way it leaves by.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoomExit {
    pub at: [f32; 3],
    /// The way out, one long.
    pub way: [f32; 3],
}

impl Default for RoomExit {
    fn default() -> Self {
        Self { at: [0.0; 3], way: [1.0, 0.0, 0.0] }
    }
}

/// A room a Cave Network places: the prefab it was read from, how often it
/// is chosen beside the others, and its shapes and exits as last read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Room {
    /// The room's prefab, as a reference to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefab: Option<String>,
    pub weight: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<RoomShape>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exits: Vec<RoomExit>,
}

impl Default for Room {
    fn default() -> Self {
        Self { prefab: None, weight: 1.0, shapes: Vec::new(), exits: Vec::new() }
    }
}

impl Room {
    /// The room a network places while it lists none: a domed chamber 14 m
    /// across with a way out on each side.
    pub fn chamber() -> Self {
        let dome =
            RoomShape { shape: RoomForm::HalfBall, size: [14.0, 6.0, 14.0], margin: 0.6, ..RoomShape::default() };
        let side = RoomShape {
            shape: RoomForm::HalfBall,
            chance: 0.5,
            at: [5.0, 0.0, 3.5],
            size: [8.0, 4.0, 8.0],
            ..RoomShape::default()
        };
        let exit = |x: f32, z: f32| RoomExit { at: [x * 6.5, 1.2, z * 6.5], way: [x, 0.0, z] };
        Self {
            prefab: None,
            weight: 1.0,
            shapes: vec![dome, side],
            exits: vec![exit(1.0, 0.0), exit(-1.0, 0.0), exit(0.0, 1.0), exit(0.0, -1.0)],
        }
    }

    /// No place in it, and none of its exits, is further than this from its
    /// origin.
    pub(crate) fn reach(&self) -> f32 {
        let exits = self.exits.iter().map(|exit| {
            let [x, y, z] = exit.at;
            (x * x + y * y + z * z).sqrt()
        });
        self.shapes.iter().map(RoomShape::reach).chain(exits).fold(1.0, greater)
    }

    /// How far inside its hollow `p` is, a place in its own space, with the
    /// shapes `present` says it has (a bit each, the first the lowest), its
    /// walls moved out by `wobble` metres where it is given: inward no
    /// further than each shape's margin.
    pub(crate) fn inside(&self, p: [f32; 3], present: u64, wobble: Option<f32>) -> f32 {
        let mut hollow = NOTHING;
        for (at, shape) in self.shapes.iter().enumerate() {
            if at < 64 && present & (1 << at) == 0 {
                continue;
            }
            let moved = wobble.map_or(0.0, |wobble| greater(wobble, -greater(shape.margin, 0.0)));
            let inside = shape.inside(p);
            hollow = match shape.fill {
                false => greater(hollow, inside + moved),
                true => lesser(hollow, -(inside + moved)),
            };
        }
        hollow
    }

    pub(crate) fn key(&self, key: &mut Key) {
        key.text(self.prefab.as_deref().unwrap_or("")).number(self.weight);
        key.whole(self.shapes.len() as u32).whole(self.exits.len() as u32);
        for shape in &self.shapes {
            shape.key(key);
        }
        for number in self.exits.iter().flat_map(|exit| exit.at.iter().chain(&exit.way)) {
            key.number(*number);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_says_how_far_inside_it_a_place_is_turned_or_not() {
        let cube = RoomShape { shape: RoomForm::Box, size: [4.0, 2.0, 6.0], ..RoomShape::default() };
        assert_eq!(cube.inside([0.0, 0.0, 0.0]), 1.0);
        assert_eq!(cube.inside([1.5, 0.0, 0.0]), 0.5);
        assert_eq!(cube.inside([5.0, 0.0, 0.0]), -3.0);
        assert_eq!(cube.inside([5.0, 5.0, 0.0]), -5.0);
        // A quarter turn about y: its long side lies along the room's x.
        let turned = RoomShape { turn: [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0], ..cube.clone() };
        assert_eq!(turned.inside([2.5, 0.0, 0.0]), 0.5);
        assert_eq!(turned.inside([0.0, 0.0, 2.5]), -0.5);
        let ball = RoomShape { size: [8.0, 8.0, 8.0], at: [10.0, 0.0, 0.0], ..RoomShape::default() };
        assert!((ball.inside([10.0, 0.0, 0.0]) - 4.0).abs() < 1e-5);
        assert!((ball.inside([13.0, 0.0, 0.0]) - 1.0).abs() < 1e-5);
        assert!((ball.inside([10.0, 6.0, 0.0]) + 2.0).abs() < 1e-5);
        let dome = RoomShape { shape: RoomForm::HalfBall, size: [8.0, 3.0, 8.0], ..RoomShape::default() };
        assert!(dome.inside([0.0, 1.0, 0.0]) > 0.9 && dome.inside([0.0, -0.5, 0.0]) == -0.5);
        assert!(dome.inside([0.0, 3.5, 0.0]) < 0.0 && dome.inside([3.5, 0.2, 0.0]) > 0.0);
    }

    #[test]
    fn a_fill_puts_rock_back_and_a_shape_left_out_hollows_nothing() {
        let mut room = Room::chamber();
        room.shapes.push(RoomShape {
            shape: RoomForm::Box,
            fill: true,
            size: [2.0, 20.0, 2.0],
            ..RoomShape::default()
        });
        let all = 0b111;
        assert!(room.inside([0.0, 2.0, 0.0], 0b011, None) > 1.0, "open without the pillar");
        assert!(room.inside([0.0, 2.0, 0.0], all, None) < -0.9, "rock inside the pillar");
        assert!(room.inside([3.0, 2.0, 0.0], all, None) > 0.5, "open beside it");
        assert_eq!(room.inside([0.0, 2.0, 0.0], 0, None), NOTHING);
        // Roughness moves a wall in no further than the shape's margin.
        let wall = [6.9, 0.3, 0.0];
        let (plain, rough) = (room.inside(wall, 0b001, None), room.inside(wall, 0b001, Some(-5.0)));
        assert!((plain - rough - 0.6).abs() < 1e-5, "{plain} {rough}");
        assert!(room.reach() >= 9.0 && room.reach() < 14.0);
    }

    #[test]
    fn a_room_is_written_with_only_what_is_not_as_made() {
        let room = Room { prefab: Some("0123".into()), ..Room::chamber() };
        let text = toml::to_string(&room).unwrap();
        assert!(!text.contains("turn") && !text.contains("fill"), "{text}");
        assert_eq!(toml::from_str::<Room>(&text).unwrap(), room);
    }
}
