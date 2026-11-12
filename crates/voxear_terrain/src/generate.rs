//! Making ground from a seed: a world generator, as a `.worldgen` file
//! keeps it, and the ground it makes at any point.
//!
//! A generator is a graph of nodes ([`Graph`]). Each node makes a number at
//! every place on the ground, from its own values and from what other nodes
//! make, carried to it along wires: a height in metres, or a mask from
//! nothing to one. One Output node takes the ground's height and its
//! climate. A generator has three values of its own, which every node reads
//! and so does whatever it makes ground for: its seed, its scale (how large
//! everything along the ground is) and its sea level.
//!
//! The Output also takes a Volume, the ground in three dimensions, for
//! worlds of blocks: how many metres into the ground each place in space
//! is. Its nodes (`nodes/solid.rs` and those beside it) push cliffs in and
//! out and hollow caves, and it is asked once a column and then once a
//! cell ([`Volume`]). A world of blocks whose generator has none wires its
//! own overhangs and caves in as those nodes ([`Graph::add_voxels`]).
//!
//! The simplest graph is a [`Landmass`] wired to the Output with a Climate
//! beside it: continents, ranges and gullies in one node, and all a
//! generator was before it was a graph. A file written then reads as that
//! graph and makes the same ground to the bit (see [`WorldGen::parse`]).
//!
//! A graph is compiled once, when its generator is made, into steps
//! (`compile`): each a kernel that fills one row of numbers from others, in
//! an order the graph alone sets, with work two nodes share done once. The
//! steps run over many places at a time (`eval`), and the numbers a place
//! gets are the same however many are asked for together.
//!
//! It is all sums, products, divisions, square roots and `floor`, with no
//! trigonometry or powers, so a seed makes the same ground on every
//! machine, and a world made from one need only send its seed.

pub mod biomes;
pub mod caves;
mod compile;
mod eval;
mod graph;
mod landmass;
pub mod nodes;
mod starters;
pub mod structures;
mod voxels;
mod world;

pub use biomes::{BiomeBlocks, BiomeDef, BiomeGround, Biomes, WorldBiomes};
pub use caves::{
    CaveExit, CaveLayout, CaveMode, CaveShape, Caves, PlacedExit, PlacedRoom, PlacedSpot, Room, RoomExit, RoomForm, RoomRules,
    RoomSample, RoomShape,
};
pub use compile::Fault;
pub use graph::{Exposed, Graph, Link, Node, NodeId};
pub use landmass::{Climate, Erosion, Field, Landmass, Preset, Sample, Warp, smooth};
pub use nodes::{NodeKind, Port, PortKind, Value, ValueKind, ValueSpec};
pub use starters::{Starter, starters};
pub use voxels::OwnVoxels;
pub use world::{Carried, Cells, PreviewArea, Volume, WorldGen};
