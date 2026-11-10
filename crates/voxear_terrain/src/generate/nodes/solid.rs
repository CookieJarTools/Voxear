//! The Solid node: a height made solid, the simplest Volume. At each place
//! in space, how many metres under the height's ground it is, out of the
//! ground where negative. Round a planet the ground lies as high over the
//! planet's sea as the height is over the generator's sea level, and a
//! place is as far under it as the surface is further from the middle.
//!
//! With nothing wired in it is the generator's own ground: what is wired
//! into the Output's height.

use serde::{Deserialize, Serialize};

use super::{Kind, Port};
use crate::generate::{
    compile::{Cells, Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// The Solid node, which has no values of its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Solid {}

impl Solid {
    /// Its height, among its inputs.
    pub const HEIGHT: usize = 0;

    /// The generator's own ground made solid, at the cells `at`: what a
    /// Volume node reads with nothing wired into its ground.
    pub(crate) fn of_ground(cx: &mut Compiler, at: Cells) -> V {
        Self::of(cx, at, |cx, places| cx.ground_at(places))
    }

    /// The height `height` makes for the cells' columns, made solid.
    fn of(cx: &mut Compiler, at: Cells, height: impl FnOnce(&mut Compiler, Places) -> V) -> V {
        // Whether the height is the generator's own ground, where the cells asked about are.
        let mut own = false;
        let made = |cx: &mut Compiler, places: Places| {
            let height = height(cx, places);
            own = height == cx.ground_at(places);
            height
        };
        let row = if cx.round() {
            let sea_level = cx.world.sea_level;
            // How far out the surface is, worked out before it is carried: what a planet reads between its points.
            let surface = cx.column(at, |cx, places| {
                let (height, radius) = (made(cx, places), cx.radius());
                cx.run(Out { sea_level }, &[radius, height])
            });
            cx.run(UnderRound, &[surface, at.x, at.y, at.z])
        } else {
            let height = cx.column(at, made);
            cx.run(Under, &[height, at.y])
        };
        if own && at == cx.cells() {
            cx.set_plain(row);
        }
        row
    }
}

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::volume("solid", "Solid")];

impl Kind for Solid {
    const TITLE: &'static str = "Solid";
    const FOLDER: &'static str = "Voxels";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }

    fn compile_volume(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Cells) -> V {
        Self::of(cx, at, |cx, places| cx.input(node, Self::HEIGHT, places).unwrap_or_else(|| cx.ground_at(places)))
    }
}

/// How far under a height a cell is, from rows of the height and the
/// cell's y.
#[derive(Debug)]
struct Under;

impl Kernel for Under {
    fn key(&self, key: &mut Key) {
        key.name("under");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for ((out, height), y) in out.iter_mut().zip(inputs[0]).zip(inputs[1]) {
            *out = height - y;
        }
    }
}

/// How far out from a planet's middle its surface is, from rows of the
/// planet's radius and the ground's height.
#[derive(Debug)]
struct Out {
    sea_level: f32,
}

impl Kernel for Out {
    fn key(&self, key: &mut Key) {
        key.name("out").number(self.sea_level);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for ((out, radius), height) in out.iter_mut().zip(inputs[0]).zip(inputs[1]) {
            *out = radius + height - self.sea_level;
        }
    }
}

/// How far under a planet's surface a cell is, from rows of how far out
/// the surface is and the cell's x, y and z.
#[derive(Debug)]
struct UnderRound;

impl Kernel for UnderRound {
    fn key(&self, key: &mut Key) {
        key.name("under round");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let (x, y, z) = (inputs[1][at], inputs[2][at], inputs[3][at]);
            *out = inputs[0][at] - (x * x + y * y + z * z).sqrt();
        }
    }
}
