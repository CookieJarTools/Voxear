//! The kinds of node a generator's graph is made of, each in a file of its
//! own under `nodes/`: its values as a `.worldgen` file keeps them, its
//! ports, its values as rows for an Inspector, and the steps it compiles
//! to.
//!
//! A kind says what it has through [`Port`]s and [`ValueSpec`]s, so the
//! graph editor draws any node and its rows without knowing what it is.
//! Adding a kind is a file here, a line in [`NodeKind`] and one in
//! `each_kind`.

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use super::{
    compile::{Cells, Compiler, Places, V, World},
    graph::NodeId,
    landmass::{Climate, seed_of},
};

mod around;
mod blend;
mod cells;
mod clamp;
mod climate;
mod climate_mask;
mod constant;
mod curvature;
mod curve;
mod falloff;
mod gullies;
mod height_mask;
mod landmass;
mod math;
mod noise;
mod output;
mod slope_mask;
mod smooth;
mod terrace;
#[cfg(test)]
pub(crate) mod testing;
mod warp;

mod chambers;
mod combine;
mod noise3d;
mod overhangs;
mod solid;
mod tunnels;

pub use blend::Blend;
// The Cells node's values: `Cells` by itself is a row of places in space.
pub use cells::Cellular;
pub use clamp::Clamp;
pub use climate_mask::ClimateMask;
pub use constant::Constant;
pub use curvature::Curvature;
pub use curve::Remap;
pub use falloff::{Falloff, FalloffShape};
pub use gullies::Gullies;
pub use height_mask::HeightMask;
pub use landmass::Land;
pub use math::{Math, Operation};
pub use noise::Noise;
pub use output::Output;
pub use slope_mask::SlopeMask;
pub use smooth::Smooth;
pub use terrace::Terrace;
pub use warp::Bend;

pub use chambers::Chambers;
pub use combine::{Combine, Combining};
pub use noise3d::Noise3d;
pub use overhangs::Overhangs;
pub use solid::Solid;
pub use tunnels::Tunnels;

/// What a wire carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PortKind {
    /// A number at each place on the ground: a height in metres, or a mask
    /// from 0 to 1.
    Field,
    /// A number at each place in space: how many metres inside the ground
    /// it is, out of it where negative.
    Volume,
    /// One biome: where it is and what it brings.
    Biome,
    /// Biomes laid over one another: which cover each place.
    Biomes,
}

impl PortKind {
    /// Whether an output of this kind can be wired into an input of
    /// `into`: its own kind, or a Field into a Volume, the same all the way
    /// up.
    pub fn feeds(self, into: PortKind) -> bool {
        self == into || (self == PortKind::Field && into == PortKind::Volume)
    }
}

/// One of a node's ports: its name in a file, its label on the node, and
/// what it carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Port {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: PortKind,
}

impl Port {
    pub(crate) const fn field(name: &'static str, label: &'static str) -> Self {
        Self { name, label, kind: PortKind::Field }
    }

    pub(crate) const fn volume(name: &'static str, label: &'static str) -> Self {
        Self { name, label, kind: PortKind::Volume }
    }
}

/// What kind of row one of a node's values is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueKind {
    /// A number kept from `low` to `high`, in `unit` ("m", "°C" or
    /// nothing).
    Number { low: f32, high: f32, unit: &'static str },
    /// A whole number kept from `low` to `high`.
    Whole { low: u32, high: u32 },
    /// One of these, by its place among them.
    Choice(&'static [&'static str]),
    /// A curve from 0 to 1, worth `empty` with no keys.
    Curve { empty: f32 },
    /// An angle from level to straight up. Its number is the slope's rise
    /// over run (0 for level, 1 for 45°, [`STEEPEST`] for straight up), so
    /// that making ground needs no trigonometry; its row shows it in
    /// degrees (see [`degrees_of`] and [`rise_of`]).
    Angle,
}

/// The rise over run kept for an angle of 90°, which has none: steeper
/// than any ground.
pub const STEEPEST: f32 = 1.0e6;

/// The rise over run a [`ValueKind::Angle`] row keeps for an angle of
/// `degrees`. For rows only: nothing that makes ground calls it, since a
/// tangent can differ between machines.
pub fn rise_of(degrees: f32) -> f32 {
    if degrees >= 90.0 {
        return STEEPEST;
    }
    degrees.max(0.0).to_radians().tan().clamp(0.0, STEEPEST)
}

/// The angle, in degrees, a [`ValueKind::Angle`] row shows for the rise
/// over run it keeps. For rows only, as [`rise_of`] is.
pub fn degrees_of(rise: f32) -> f32 {
    if rise >= STEEPEST {
        return 90.0;
    }
    rise.max(0.0).atan().to_degrees()
}

/// One of a node's values: its name in a file, its label on its row, what
/// kind of row it is, the input it stands in for while that input has no
/// wire, and whether it is one of the rows shown first or kept under
/// Advanced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ValueKind,
    pub input: Option<usize>,
    pub advanced: bool,
}

impl ValueSpec {
    pub(crate) const fn number(
        name: &'static str,
        label: &'static str,
        low: f32,
        high: f32,
        unit: &'static str,
    ) -> Self {
        Self { name, label, kind: ValueKind::Number { low, high, unit }, input: None, advanced: false }
    }

    pub(crate) const fn whole(name: &'static str, label: &'static str, low: u32, high: u32) -> Self {
        Self { name, label, kind: ValueKind::Whole { low, high }, input: None, advanced: false }
    }

    pub(crate) const fn choice(name: &'static str, label: &'static str, choices: &'static [&'static str]) -> Self {
        Self { name, label, kind: ValueKind::Choice(choices), input: None, advanced: false }
    }

    pub(crate) const fn curve(name: &'static str, label: &'static str, empty: f32) -> Self {
        Self { name, label, kind: ValueKind::Curve { empty }, input: None, advanced: false }
    }

    pub(crate) const fn angle(name: &'static str, label: &'static str) -> Self {
        Self { name, label, kind: ValueKind::Angle, input: None, advanced: false }
    }

    /// Kept under Advanced.
    pub(crate) const fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    /// Standing in for input `input` while it has no wire.
    pub(crate) const fn for_input(mut self, input: usize) -> Self {
        self.input = Some(input);
        self
    }

    /// `value` as this row keeps it: a number or whole number held within
    /// its range, a choice among those it has, an angle's rise over run from
    /// level to straight up; none for a value of another kind.
    pub fn kept(&self, value: Value) -> Option<Value> {
        match (self.kind, value) {
            (ValueKind::Number { low, high, .. }, Value::Number(number)) if !number.is_nan() => {
                Some(Value::Number(number.clamp(low, high)))
            }
            (ValueKind::Whole { low, high }, Value::Whole(whole)) => Some(Value::Whole(whole.clamp(low, high))),
            (ValueKind::Whole { low, high }, Value::Number(number)) if !number.is_nan() => {
                Some(Value::Whole((number.max(0.0) as u32).clamp(low, high)))
            }
            (ValueKind::Choice(choices), Value::Choice(choice)) if choice < choices.len() => Some(value),
            (ValueKind::Curve { .. }, Value::Curve(_)) => Some(value),
            (ValueKind::Angle, Value::Number(rise)) if !rise.is_nan() => Some(Value::Number(rise.clamp(0.0, STEEPEST))),
            _ => None,
        }
    }
}

/// One of a node's values, as its row shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Whole(u32),
    Choice(usize),
    Curve(Curve),
}

impl Value {
    pub fn number(self) -> Option<f32> {
        match self {
            Value::Number(number) => Some(number),
            Value::Whole(whole) => Some(whole as f32),
            _ => None,
        }
    }
}

/// The part of a generator's seed nodes' own noise starts from: past every
/// part a landmass uses.
const NODE_SEEDS: u32 = 0x1000;

/// The seed of `node`'s own noise in `world`: from the generator's seed and
/// the node's number, so each node's is its own and stays with it.
pub(crate) fn node_seed(world: World, node: NodeId) -> u32 {
    seed_of(world.seed, NODE_SEEDS.wrapping_add(node.0))
}

/// What every kind of node has.
pub(crate) trait Kind {
    /// Its name in the list of nodes to add.
    const TITLE: &'static str;
    /// The folder of that list it is in.
    const FOLDER: &'static str;

    fn inputs(&self) -> &'static [Port] {
        &[]
    }

    fn outputs(&self) -> &'static [Port];

    fn values(&self) -> &'static [ValueSpec] {
        &[]
    }

    /// Value `at` among its values, as its row shows it.
    fn value(&self, _at: usize) -> Option<Value> {
        None
    }

    /// Sets value `at` to `value`, already held to its row's range.
    fn set(&mut self, _at: usize, _value: Value) {}

    /// The steps that make its output `port` at `at`, added to what `cx`
    /// compiles: the row their last fills.
    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V;

    /// The steps that make its Volume output `port` at the cells `at`: for
    /// a kind with one. Its Volume inputs it asks of [`Compiler::volume`],
    /// and what it reads of the ground under the cells of
    /// [`Compiler::field`] or [`Compiler::column`], which make it once a
    /// column.
    fn compile_volume(&self, cx: &mut Compiler, _node: NodeId, _port: usize, _at: Cells) -> V {
        cx.constant(0.0)
    }
}

/// A node's kind, with its values. A graph has few nodes, so a landmass's many values lie in the node itself.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum NodeKind {
    Landmass(Land),
    Climate(Climate),
    Output(Output),
    Constant(Constant),
    Noise(Noise),
    Math(Math),
    Curve(Remap),
    Cells(Cellular),
    Falloff(Falloff),
    Terrace(Terrace),
    Clamp(Clamp),
    Blend(Blend),
    Warp(Bend),
    Smooth(Smooth),
    #[serde(rename = "height_mask")]
    HeightMask(HeightMask),
    #[serde(rename = "slope_mask")]
    SlopeMask(SlopeMask),
    Curvature(Curvature),
    #[serde(rename = "climate_mask")]
    ClimateMask(ClimateMask),
    Gullies(Gullies),
    Solid(Solid),
    Noise3d(Noise3d),
    Overhangs(Overhangs),
    Tunnels(Tunnels),
    Chambers(Chambers),
    Combine(Combine),
}

/// `$do` for the values of whichever kind `$node` is, as `$kind`.
macro_rules! each_kind {
    ($node:expr, $kind:ident => $do:expr) => {
        match $node {
            NodeKind::Landmass($kind) => $do,
            NodeKind::Climate($kind) => $do,
            NodeKind::Output($kind) => $do,
            NodeKind::Constant($kind) => $do,
            NodeKind::Noise($kind) => $do,
            NodeKind::Math($kind) => $do,
            NodeKind::Curve($kind) => $do,
            NodeKind::Cells($kind) => $do,
            NodeKind::Falloff($kind) => $do,
            NodeKind::Terrace($kind) => $do,
            NodeKind::Clamp($kind) => $do,
            NodeKind::Blend($kind) => $do,
            NodeKind::Warp($kind) => $do,
            NodeKind::Smooth($kind) => $do,
            NodeKind::HeightMask($kind) => $do,
            NodeKind::SlopeMask($kind) => $do,
            NodeKind::Curvature($kind) => $do,
            NodeKind::ClimateMask($kind) => $do,
            NodeKind::Gullies($kind) => $do,
            NodeKind::Solid($kind) => $do,
            NodeKind::Noise3d($kind) => $do,
            NodeKind::Overhangs($kind) => $do,
            NodeKind::Tunnels($kind) => $do,
            NodeKind::Chambers($kind) => $do,
            NodeKind::Combine($kind) => $do,
        }
    };
}

fn title_of<K: Kind>(_: &K) -> (&'static str, &'static str) {
    (K::TITLE, K::FOLDER)
}

impl NodeKind {
    /// One of every kind, as it is when added to a graph, in the order the
    /// list of nodes to add shows them.
    pub fn all() -> Vec<NodeKind> {
        vec![
            NodeKind::Landmass(Land::default()),
            NodeKind::Noise(Noise::default()),
            NodeKind::Cells(Cellular::default()),
            NodeKind::Falloff(Falloff::default()),
            NodeKind::Constant(Constant::default()),
            NodeKind::Curve(Remap::default()),
            NodeKind::Terrace(Terrace::default()),
            NodeKind::Clamp(Clamp::default()),
            NodeKind::Math(Math::default()),
            NodeKind::Blend(Blend::default()),
            NodeKind::Warp(Bend::default()),
            NodeKind::Smooth(Smooth::default()),
            NodeKind::HeightMask(HeightMask::default()),
            NodeKind::SlopeMask(SlopeMask::default()),
            NodeKind::Curvature(Curvature::default()),
            NodeKind::ClimateMask(ClimateMask::default()),
            NodeKind::Gullies(Gullies::default()),
            NodeKind::Climate(Climate::default()),
            NodeKind::Solid(Solid::default()),
            NodeKind::Noise3d(Noise3d::default()),
            NodeKind::Overhangs(Overhangs::default()),
            NodeKind::Tunnels(Tunnels::default()),
            NodeKind::Chambers(Chambers::default()),
            NodeKind::Combine(Combine::default()),
            NodeKind::Output(Output::default()),
        ]
    }

    /// Its name in the list of nodes to add, and on a node.
    pub fn title(&self) -> &'static str {
        each_kind!(self, kind => title_of(kind).0)
    }

    /// The folder of the list of nodes to add it is in.
    pub fn folder(&self) -> &'static str {
        each_kind!(self, kind => title_of(kind).1)
    }

    pub fn inputs(&self) -> &'static [Port] {
        each_kind!(self, kind => kind.inputs())
    }

    pub fn outputs(&self) -> &'static [Port] {
        each_kind!(self, kind => kind.outputs())
    }

    pub fn values(&self) -> &'static [ValueSpec] {
        each_kind!(self, kind => kind.values())
    }

    /// Value `at` among its values, as its row shows it.
    pub fn value(&self, at: usize) -> Option<Value> {
        each_kind!(self, kind => kind.value(at))
    }

    /// Sets value `at`, held to its row's range: whether it changed.
    pub fn set_value(&mut self, at: usize, value: Value) -> bool {
        let Some(kept) = self.values().get(at).and_then(|spec| spec.kept(value)) else { return false };
        let before = self.clone();
        each_kind!(self, kind => kind.set(at, kept));
        *self != before
    }

    pub(crate) fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        each_kind!(self, kind => kind.compile(cx, node, port, at))
    }

    pub(crate) fn compile_volume(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Cells) -> V {
        each_kind!(self, kind => kind.compile_volume(cx, node, port, at))
    }
}
