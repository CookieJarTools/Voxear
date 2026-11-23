//! The World Edge node: the ground wired in, brought down to a sea bed or up
//! to a wall of cliffs past the edges of a box, over Width metres inside
//! them. A world of blocks streamed to a set size has one put between its
//! ground and its Output (see `blocks::generate::cells`), so its coast or
//! its cliffs are the generator's own ground: rivers run off into the sea,
//! and structures, the far view and the map see it as any other ground. Its
//! Inside is 1 inside the box, easing to 0 where the ground has sunk or
//! risen all the way: a world keeps its biomes' pillars and arches to it.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// What lies past the edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeShape {
    /// The ground sinks to Level, a sea bed.
    #[default]
    Sea,
    /// The ground rises to Level, a wall of cliffs and the high land past it.
    Cliffs,
}

/// A box of the world, in metres along x and z, and the ground past it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldEdge {
    pub shape: EdgeShape,
    pub from: [f32; 2],
    pub to: [f32; 2],
    /// How far inside its edges the ground begins to sink or rise, in metres.
    pub width: f32,
    /// The height the ground sinks or rises to past its edges, in metres.
    pub level: f32,
}

impl Default for WorldEdge {
    fn default() -> Self {
        Self { shape: EdgeShape::Sea, from: [0.0; 2], to: [1024.0; 2], width: 256.0, level: -40.0 }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("shape", "Shape", &["Sea", "Cliffs"]),
    ValueSpec::number("from_x", "From X", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("from_z", "From Z", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("to_x", "To X", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("to_z", "To Z", f32::MIN, f32::MAX, "m"),
    ValueSpec::number("width", "Width", 0.0, f32::MAX, "m"),
    ValueSpec::number("level", "Level", f32::MIN, f32::MAX, "m"),
];

const INPUTS: &[Port] = &[Port::field("ground", "Ground")];

const OUTPUTS: &[Port] = &[Port::field("height", "Height"), Port::field("inside", "Inside")];

impl Kind for WorldEdge {
    const TITLE: &'static str = "World Edge";
    const FOLDER: &'static str = "Shape";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(usize::from(self.shape == EdgeShape::Cliffs)),
            1 => Value::Number(self.from[0]),
            2 => Value::Number(self.from[1]),
            3 => Value::Number(self.to[0]),
            4 => Value::Number(self.to[1]),
            5 => Value::Number(self.width),
            6 => Value::Number(self.level),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.shape = if choice == 1 { EdgeShape::Cliffs } else { EdgeShape::Sea },
            (1, Value::Number(number)) => self.from[0] = number,
            (2, Value::Number(number)) => self.from[1] = number,
            (3, Value::Number(number)) => self.to[0] = number,
            (4, Value::Number(number)) => self.to[1] = number,
            (5, Value::Number(number)) => self.width = number.max(0.0),
            (6, Value::Number(number)) => self.level = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, output: usize, at: Places) -> V {
        let edge = Edged { shape: self.shape, from: self.from, to: self.to, width: self.width, level: self.level };
        if output == 1 {
            return cx.run(Inside(edge), &[at.x, at.z]);
        }
        let ground = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(0.0));
        cx.run(edge, &[ground, at.x, at.z])
    }
}

/// Rows of ground brought to `level` past the box's edges.
#[derive(Debug)]
struct Edged {
    shape: EdgeShape,
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    level: f32,
}

impl Edged {
    /// How far it has sunk or risen at `x`, `z`, from 0 inside to 1.
    fn past(&self, x: f32, z: f32) -> f32 {
        // How far outside the box the place is: less than nothing inside it.
        let dx = (self.from[0] - x).max(x - self.to[0]);
        let dz = (self.from[1] - z).max(z - self.to[1]);
        let out = match dx > 0.0 || dz > 0.0 {
            true => (dx.max(0.0).powi(2) + dz.max(0.0).powi(2)).sqrt(),
            false => dx.max(dz),
        };
        let width = self.width.max(1e-3);
        // The edge wanders in and out by up to half its width, so a coast or a wall of cliffs is no ruled line.
        let out = out + wander(x, z, width) * width * 0.5;
        ((out + width) / width).clamp(0.0, 1.0)
    }

    fn at(&self, ground: f32, x: f32, z: f32) -> f32 {
        let t = self.past(x, z);
        match self.shape {
            // Eased in, so the coast keeps the land's own line where it meets the sea.
            EdgeShape::Sea => ground + (ground.min(self.level) - ground) * (t * t * (3.0 - 2.0 * t)),
            // Sheer faces with level ledges between them, so the rock's layers show on each face.
            EdgeShape::Cliffs => ground + (ground.max(self.level) - ground) * ledges(t),
        }
    }
}

impl Kernel for Edged {
    fn key(&self, key: &mut Key) {
        key.name("world edge")
            .whole(self.shape as u32)
            .number(self.from[0])
            .number(self.from[1])
            .number(self.to[0])
            .number(self.to[1])
            .number(self.width)
            .number(self.level);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (((out, &ground), &x), &z) in out.iter_mut().zip(inputs[0]).zip(inputs[1]).zip(inputs[2]) {
            *out = self.at(ground, x, z);
        }
    }
}

/// How far up a wall of cliffs is `t` of the way across it, from 0 to 1:
/// [`LEDGES`] sheer faces, each over the last fifth of its step, level
/// between them.
fn ledges(t: f32) -> f32 {
    if t >= 1.0 {
        return 1.0;
    }
    let step = t * LEDGES;
    let (whole, part) = (step.floor(), step - step.floor());
    let rise = ((part - 0.8) / 0.2).clamp(0.0, 1.0);
    ((whole + rise * rise * (3.0 - 2.0 * rise)) / LEDGES).min(1.0)
}

/// How many faces a wall of cliffs rises in.
const LEDGES: f32 = 3.0;

/// Smooth noise from -1 to 1 at `x`, `z`, its bumps about `width` metres
/// apart, and a finer half as strong: lattice values hashed from whole
/// numbers, so it is the same on every machine and has no seams.
fn wander(x: f32, z: f32, width: f32) -> f32 {
    let octave = |cell: f32, salt: u32| {
        let (fx, fz) = (x / cell, z / cell);
        let (ix, iz) = (fx.floor(), fz.floor());
        let (tx, tz) = (fx - ix, fz - iz);
        let (sx, sz) = (tx * tx * (3.0 - 2.0 * tx), tz * tz * (3.0 - 2.0 * tz));
        let value = |dx: i64, dz: i64| {
            let mut h = (ix as i64 + dx) as u64 ^ ((iz as i64 + dz) as u64).rotate_left(32) ^ u64::from(salt);
            h = (h ^ (h >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
            h = (h ^ (h >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
            ((h >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        let near = value(0, 0) + (value(1, 0) - value(0, 0)) * sx;
        let far = value(0, 1) + (value(1, 1) - value(0, 1)) * sx;
        near + (far - near) * sz
    };
    (octave(width.max(1.0), 0x51ed) + octave(width.max(1.0) * 0.4, 0x7a3c) * 0.5) / 1.5
}

/// How far inside the box's edge rows of places are: 1 inside, 0 past it.
#[derive(Debug)]
struct Inside(Edged);

impl Kernel for Inside {
    fn key(&self, key: &mut Key) {
        key.name("world edge inside");
        self.0.key(key);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for ((out, &x), &z) in out.iter_mut().zip(inputs[0]).zip(inputs[1]) {
            *out = 1.0 - self.0.past(x, z);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_sinks_to_a_sea_bed_or_rises_to_cliffs_past_the_edge() {
        let sea = Edged { shape: EdgeShape::Sea, from: [0.0, 0.0], to: [1000.0, 1000.0], width: 100.0, level: -40.0 };
        assert_eq!(sea.at(30.0, 500.0, 500.0), 30.0, "inside, as it was");
        assert_eq!(sea.at(30.0, 1200.0, 500.0), -40.0, "past the edge, the sea bed");
        assert_eq!(sea.at(-90.0, 1200.0, 500.0), -90.0, "deeper sea kept");
        let between = sea.at(30.0, 950.0, 500.0);
        assert!(between < 30.0 && between > -40.0, "sinking towards it: {between}");
        let cliffs = Edged { shape: EdgeShape::Cliffs, level: 160.0, width: 120.0, ..sea };
        assert_eq!(cliffs.at(30.0, -100.0, 500.0), 160.0);
        assert_eq!(cliffs.at(30.0, 500.0, 500.0), 30.0);
        // Across the wall most places stand on its foot, a ledge or its top: it rises in sheer faces.
        let across: Vec<f32> = (0..400).map(|at| cliffs.at(30.0, 820.0 + at as f32 * 0.6, 500.0)).collect();
        let levels = [30.0, 30.0 + 130.0 / 3.0, 30.0 + 260.0 / 3.0, 160.0];
        let on_ledges = across.iter().filter(|&&h| levels.iter().any(|level| (h - level).abs() < 0.5)).count();
        assert!(on_ledges * 10 > across.len() * 7, "{on_ledges} of {} on its ledges", across.len());
        assert!(across.windows(2).all(|pair| pair[1] >= pair[0] - 1e-3), "rising all the way out");
    }
}
