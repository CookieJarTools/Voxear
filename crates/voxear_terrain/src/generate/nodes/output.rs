//! The Output node: what a generator makes. The ground's height, in
//! metres, and its climate: how warm it is, in °C, and how wet, 0 to 1.
//! With no height wired in the ground lies at the sea level; with no
//! climate it is mild, and half wet.
//!
//! Its Volume is the ground in three dimensions, for worlds of blocks and
//! Voxel Terrains: how many metres into the ground each place in space is,
//! out of it where negative. With none wired in, a world of blocks makes
//! its ground from the height, and its own overhangs and caves. A terrain
//! of heights reads the height alone.
//!
//! Its Rock is a mask of where the ground is bare: where it is over a
//! half, a world of blocks leaves the column's top and under layers off
//! and shows its bottom block, so walls and crags are rock and not striped
//! with soil. Wire a Slope Mask into it, or a Plateaus node's Walls.

use serde::{Deserialize, Serialize};

use super::{Kind, Port};
use crate::generate::{
    compile::{Compiler, Places, V},
    graph::NodeId,
};

/// The Output node, which has no values of its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Output {}

impl Output {
    /// Its inputs, among its inputs.
    pub const HEIGHT: usize = 0;
    pub const TEMPERATURE: usize = 1;
    pub const HUMIDITY: usize = 2;
    pub const VOLUME: usize = 3;
    pub const ROCK: usize = 4;
}

const INPUTS: &[Port] = &[
    Port::field("height", "Height"),
    Port::field("temperature", "Temperature"),
    Port::field("humidity", "Humidity"),
    Port::volume("volume", "Volume"),
    Port::field("rock", "Rock"),
];

impl Kind for Output {
    const TITLE: &'static str = "Output";
    const FOLDER: &'static str = "";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        &[]
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(0.0)
    }
}
