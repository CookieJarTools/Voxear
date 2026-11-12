//! What the tests of each kind of node share: a generator whose ground is
//! one node's output, and the checks every kind must pass.

use crate::generate::{Graph, NodeKind, WorldGen, nodes::Output};

/// A generator whose ground is output `port` of a node of `kind`, with
/// output 0 of a node of each of `sources` wired into its inputs in order,
/// and none into an input whose source is none. Seed 5, scale 1 and the
/// sea at nothing.
pub fn world_of(kind: NodeKind, port: usize, sources: &[Option<NodeKind>]) -> WorldGen {
    let mut graph = Graph::default();
    let sources: Vec<_> = sources.iter().map(|source| source.clone().map(|kind| graph.add(kind, [0.0, 0.0]))).collect();
    let node = graph.add(kind, [200.0, 0.0]);
    for (input, source) in sources.iter().enumerate() {
        if let Some(source) = source {
            assert!(graph.connect(*source, 0, node, input), "input {input} takes a field");
        }
    }
    let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
    assert!(graph.connect(node, port, output, Output::HEIGHT), "output {port} is a field");
    WorldGen::new(5, 1.0, 0.0, graph)
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

/// `count` places over flat ground, a few kilometres across.
pub fn flat_places(count: usize) -> (Vec<f32>, Vec<f32>) {
    let xs = (0..count).map(|i| (i % 61) as f32 * 37.3 - 900.0 + (i / 61) as f32 * 0.7).collect();
    let zs = (0..count).map(|i| (i / 61) as f32 * 41.9 - 700.0 + (i % 7) as f32 * 3.1).collect();
    (xs, zs)
}

/// Checks what every generator must hold to: its graph is whole; however
/// many places are asked for at once, each gets the bits it gets alone; and
/// it reads back from its text as it was written. Gives the least and most of its ground over flat places.
pub fn checked(world: &WorldGen) -> (f32, f32) {
    assert!(world.faults().is_empty(), "{:?}", world.faults());
    for count in [1usize, 7, 289, 513, 1300] {
        let (xs, zs) = flat_places(count);
        let mut heights = vec![0.0; count];
        world.heights(&xs, &zs, &mut heights);
        let alone: Vec<f32> = (0..count).map(|i| world.height_at(xs[i], zs[i])).collect();
        assert_eq!(bits(&heights), bits(&alone), "{count} places at once");
    }
    let text = world.to_text();
    let read = WorldGen::parse(&text).unwrap_or_else(|error| panic!("{error}: {text}"));
    assert_eq!(&read, world, "{text}");
    assert_eq!(read.to_text(), text);
    assert_eq!(read.hash(), world.hash());
    assert_eq!(read.height_at(321.0, -45.0).to_bits(), world.height_at(321.0, -45.0).to_bits());
    range(&flat(world, 4000))
}

/// Its ground at 4000 or so flat places.
pub fn flat(world: &WorldGen, count: usize) -> Vec<f32> {
    let (xs, zs) = flat_places(count);
    let mut heights = vec![0.0; count];
    world.heights(&xs, &zs, &mut heights);
    heights
}

/// The least and most of `values`, none of which may be anything but a
/// number.
pub fn range(values: &[f32]) -> (f32, f32) {
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for &value in values {
        assert!(value.is_finite(), "{value}");
        (low, high) = (low.min(value), high.max(value));
    }
    (low, high)
}
