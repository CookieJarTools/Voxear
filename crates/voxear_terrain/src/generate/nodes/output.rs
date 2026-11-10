//! The Output node: what a generator makes. The ground's height, in
//! metres, and its climate: how warm it is, in °C, and how wet, 0 to 1.
//! With no height wired in the ground lies at the sea level; with no
//! climate it is mild, and half wet.

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
}

const INPUTS: &[Port] =
    &[Port::field("height", "Height"), Port::field("temperature", "Temperature"), Port::field("humidity", "Humidity")];

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
