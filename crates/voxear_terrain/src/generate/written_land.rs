//! Laying out the land a world of blocks writes into its generator (see
//! `blocks::generate::cells::write_land`): its nodes moved under the
//! graph's own, a part at a time, each part in a comment saying what it
//! does.
//!
//! The nodes are told apart by the order they are added in. A world lays
//! its land in steps (its biomes' land, rivers, its own overhangs and caves,
//! then its biomes' pillars and bridges), and its biomes' land in parts:
//! where the land rises, the Levels node, each biome's land from its
//! Climate Mask on, then the levels' floors and the climate counted from
//! them, which read the Levels node itself. Nothing here changes what a
//! node is or how it is wired, only where it sits.

use std::collections::HashMap;

use super::{
    BiomeDef, Graph, NodeId, NodeKind,
    nodes::{Arches, Chambers, Operation, Output, Overhangs, Spires, Tunnels},
};
use crate::noise::NoiseStyle;

/// The nodes each step of laying land in added, as the numbers after the
/// last before it to the last it added.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Steps {
    pub land: (u32, u32),
    pub rivers: (u32, u32),
    pub voxels: (u32, u32),
    pub volume: (u32, u32),
}

/// One part of the land, to frame: what it is called, what it does, and
/// its nodes, and the part framed within it, where it has one.
struct Part {
    title: String,
    text: String,
    nodes: Vec<NodeId>,
    within: Option<Box<Part>>,
}

impl Part {
    fn new(title: &str, text: &str, nodes: Vec<NodeId>) -> Self {
        Self { title: title.to_string(), text: text.to_string(), nodes, within: None }
    }
}

/// How far apart the columns of a part's nodes are, and its nodes down a
/// column, on the canvas.
const COLUMN: f32 = 300.0;
const BETWEEN: f32 = 40.0;
/// The room a frame leaves round its nodes, and between frames.
const MARGIN: f32 = 40.0;
const APART: f32 = 120.0;

/// Moves the nodes `steps` added to `graph` into parts under its own nodes,
/// each in a comment, and shows the amounts of the world's own overhangs
/// and caves as rows. `biomes` are the world's, `names` theirs, and
/// `levelled` whether its land lies in levels.
pub(crate) fn lay_out(graph: &mut Graph, steps: &Steps, biomes: &[BiomeDef], names: &[String], levelled: bool) {
    let within = |(after, last): (u32, u32)| -> Vec<NodeId> {
        let mut ids: Vec<NodeId> =
            graph.nodes.iter().map(|node| node.id).filter(|id| id.0 > after && id.0 <= last).collect();
        ids.sort();
        ids
    };
    let (land, rivers, voxels, volume) =
        (within(steps.land), within(steps.rivers), within(steps.voxels), within(steps.volume));
    let added: Vec<NodeId> = [&land, &rivers, &voxels, &volume].into_iter().flatten().copied().collect();

    let mut first_row = land_parts(graph, &land, biomes, names, levelled);
    first_row.retain(|part| !part.nodes.is_empty());
    let mut second_row = Vec::new();
    if !rivers.is_empty() {
        let text = "Rivers run down the land as it is shaped above, and lakes fill its hollows: in a world of \
                    levels a few great rivers first, carried on to the sea, and the rest running into them. Their \
                    water and falls go into the Output.";
        second_row.push(Part::new("Rivers", text, rivers));
    }
    if !voxels.is_empty() {
        let text = "The world's own overhangs and caves, which it used to lay in itself: their amounts are rows of \
                    the generator's own now.";
        second_row.push(Part::new("Overhangs and Caves", text, voxels.clone()));
        show_rows(graph, &voxels);
    }
    let (levels_volume, biome_volume): (Vec<NodeId>, Vec<NodeId>) =
        volume.iter().partition(|&&id| from_levels(graph, id));
    if !levels_volume.is_empty() {
        let text = "Fields of spires stand in the areas of spires, and natural bridges span the ravines of the areas \
                    of canyons: the Levels node says where.";
        second_row.push(Part::new("Spires and Arches", text, levels_volume));
    }
    if !biome_volume.is_empty() {
        let text = "Each biome's pillars of rock and natural bridges, and the overhangs pushed out of its steep \
                    faces, kept to the biome by its mask.";
        second_row.push(Part::new("Pillars, Bridges and Overhangs", text, biome_volume));
    }

    // Under everything the graph had before.
    let bottom = graph
        .nodes
        .iter()
        .filter(|node| !added.contains(&node.id))
        .map(|node| node.at[1] + size_of(&node.kind)[1])
        .fold(f32::MIN, f32::max);
    let left =
        graph.nodes.iter().filter(|node| !added.contains(&node.id)).map(|node| node.at[0]).fold(f32::MAX, f32::min);
    let (left, mut top) = if bottom == f32::MIN { (0.0, 0.0) } else { (left, bottom + APART * 2.0) };
    for row in [first_row, second_row] {
        let mut x = left;
        let mut tallest: f32 = 0.0;
        for part in &row {
            let [width, height] = place(graph, part, [x, top]);
            x += width + APART;
            tallest = tallest.max(height);
        }
        if !row.is_empty() {
            top += tallest + APART * 2.0;
        }
    }
}

/// The parts of a world's biomes' land, from the nodes `land` the first
/// step added, in the order they feed one another.
fn land_parts(graph: &Graph, land: &[NodeId], biomes: &[BiomeDef], names: &[String], levelled: bool) -> Vec<Part> {
    let kind = |id: NodeId| graph.node(id).map(|node| &node.kind);
    let levels = land.iter().copied().find(|&id| matches!(kind(id), Some(NodeKind::Levels(_))));
    let masks: Vec<NodeId> =
        land.iter().copied().filter(|&id| matches!(kind(id), Some(NodeKind::ClimateMask(_)))).collect();
    // The levels' floors begin with the first node after it that reads the Levels node.
    let finish = levels.and_then(|levels| {
        land.iter()
            .copied()
            .find(|&id| id > levels && graph.links.iter().any(|link| link.to == id && link.from == levels))
    });
    let end = NodeId(u32::MAX);
    let starts = [levels, masks.first().copied(), finish].into_iter().flatten().min().unwrap_or(end);
    let between = |from: NodeId, to: NodeId| -> Vec<NodeId> {
        land.iter().copied().filter(|&id| id >= from && id < to).collect()
    };
    let mut parts = Vec::new();

    let rising = land.iter().copied().filter(|&id| id < starts).collect();
    let text = if levelled {
        "How far inland each place is, from 0 at the coast to 1 a few tens of metres above the sea: the land \
         rises out of the generator's own ground there, and keeps clear of the world's start."
    } else {
        "How far inland each place is, from 0 at the coast to 1 a few tens of metres above the sea: each biome's \
         land rises out of the generator's own ground there, so the coasts stay where they were."
    };
    parts.push(Part::new("Where the Land Rises", text, rising));

    let after_masks = finish.unwrap_or(end);
    if let Some(levels) = levels {
        let text = "The Levels node lays the ground at a few set heights, an area at a time, with sheer walls \
                    between. Its areas are the world's biome regions, so each area is one biome. The Constant is \
                    the level ground each biome's land is made from.";
        parts.push(Part::new("Levels", text, between(levels, masks.first().copied().unwrap_or(after_masks))));
    }
    for (place, &mask) in masks.iter().enumerate() {
        let next = masks.get(place + 1).copied().unwrap_or(after_masks);
        let nodes = between(mask, next);
        let biome = biome_of(graph, mask, biomes);
        let name = biome.and_then(|at| names.get(at).cloned()).unwrap_or_else(|| format!("Biome {}", place + 1));
        let form = biome.map(|at| biomes[at].land.form.name()).unwrap_or("Its");
        let kept = match kind(mask) {
            Some(NodeKind::ClimateMask(mask)) if mask.world_biome > 0 => "where its regions lie",
            _ => "where its climate is",
        };
        let over = if levelled { "rolled over its area's floor" } else { "risen out of the generator's ground" };
        let text =
            format!("{form} land, {over}, kept by its Climate Mask to {kept} and blended over the land before it.");
        let mut part = Part::new(&format!("{name} Land"), &text, nodes.clone());
        if let Some(massifs) = massifs(graph, &nodes) {
            let text = "A broad noise says where lone massifs stand: the peaks are cut down to level ground between \
                        them, so mountains are not ranges over all the land.";
            part.within = Some(Box::new(Part::new("Mountains Cut to Massifs", text, massifs)));
        }
        parts.push(part);
    }
    if let Some(finish) = finish {
        let nodes = between(finish, end);
        let climate =
            graph.output().and_then(|output| graph.link_into(output.id, Output::TEMPERATURE)).map(|link| link.from);
        let height = graph.output().and_then(|output| graph.link_into(output.id, Output::HEIGHT)).map(|link| link.from);
        let cools = |id: NodeId| {
            climate.is_some_and(|climate| graph.reads(climate, id))
                && height.is_none_or(|height| !graph.reads(height, id))
        };
        let (cooling, floors): (Vec<NodeId>, Vec<NodeId>) = nodes.into_iter().partition(|&id| cools(id));
        let text = "Each biome's land rolls over its area's floor as much as the area's character says; the \
                    generator's own ground stays within the world's start, and the walls and their rubble are \
                    bare rock.";
        parts.push(Part::new("Floors and Walls", text, floors));
        let text = "The climate cools with the height over each area's floor, not with the floor's own, so a high \
                    level is as green as a low one and only what rises over it is cold.";
        parts.push(Part::new("Climate Counted from the Floor", text, cooling));
    }
    parts
}

/// Which of `biomes` the Climate Mask `mask` keeps to: the one whose
/// regions it keeps to, or else the one whose climate it is.
fn biome_of(graph: &Graph, mask: NodeId, biomes: &[BiomeDef]) -> Option<usize> {
    let Some(NodeKind::ClimateMask(mask)) = graph.node(mask).map(|node| &node.kind) else { return None };
    match mask.world_biome {
        0 => biomes.iter().position(|biome| biome.temperature == mask.temperature && biome.humidity == mask.humidity),
        number => Some(number as usize - 1).filter(|&at| at < biomes.len()),
    }
}

/// The nodes of a biome's land that cut its mountains to lone massifs,
/// where it has them: a broad noise, a curve of it, and the maths that cut
/// the peaks down by it, one after another.
fn massifs(graph: &Graph, nodes: &[NodeId]) -> Option<Vec<NodeId>> {
    let kind = |id: NodeId| graph.node(id).map(|node| &node.kind);
    let math = |id: NodeId, operation: Operation| matches!(kind(id), Some(NodeKind::Math(math)) if math.operation == operation);
    let cuts = |run: &[NodeId]| {
        matches!(kind(run[0]), Some(NodeKind::Noise(noise)) if noise.style == NoiseStyle::Fractal && noise.octaves == 2)
            && matches!(kind(run[1]), Some(NodeKind::Curve(_)))
            && math(run[2], Operation::Subtract)
            && math(run[3], Operation::Multiply)
            && math(run[4], Operation::Subtract)
            && math(run[5], Operation::Max)
    };
    nodes.windows(6).find(|run| cuts(run)).map(<[NodeId]>::to_vec)
}

/// Whether node `id` is a Spires or an Arches node that stands where a
/// Levels node says.
fn from_levels(graph: &Graph, id: NodeId) -> bool {
    let mask = match graph.node(id).map(|node| &node.kind) {
        Some(NodeKind::Spires(_)) => Spires::MASK,
        Some(NodeKind::Arches(_)) => Arches::MASK,
        _ => return false,
    };
    let from = graph.link_into(id, mask).and_then(|link| graph.node(link.from));
    from.is_some_and(|node| matches!(node.kind, NodeKind::Levels(_)))
}

/// Shows the amounts of the overhangs and caves among `nodes` as rows of
/// the generator's own, as moving a world's own into its generator does.
fn show_rows(graph: &mut Graph, nodes: &[NodeId]) {
    for &id in nodes {
        let row = match graph.node(id).map(|node| &node.kind) {
            Some(NodeKind::Overhangs(_)) => Some((Overhangs::AMOUNT, "Overhangs")),
            Some(NodeKind::Tunnels(_)) => Some((Tunnels::AMOUNT, "Tunnels")),
            Some(NodeKind::Chambers(_)) => Some((Chambers::AMOUNT, "Chambers")),
            _ => None,
        };
        if let Some((value, label)) = row.filter(|(_, label)| !graph.exposed.iter().any(|row| row.label == *label)) {
            graph.expose(id, value, label);
        }
    }
}

/// Lays out `part`'s nodes with its top left corner at `at`, each node a
/// column to the right of the furthest of those in the part it reads, and
/// frames them in a comment, with the part within it framed in its own:
/// how large the frame is.
fn place(graph: &mut Graph, part: &Part, at: [f32; 2]) -> [f32; 2] {
    let text_height = |width: f32, text: &str| {
        let lines = (text.chars().count() as f32 * 6.4 / (width - 24.0).max(60.0)).ceil();
        40.0 + lines * 17.0
    };
    // How many columns along each node is: one past the furthest of the part's nodes it reads.
    let mut columns: HashMap<NodeId, usize> = HashMap::new();
    for &id in &part.nodes {
        column(graph, &part.nodes, id, &mut columns);
    }
    let count = columns.values().copied().max().map_or(0, |most| most + 1);
    let widest =
        part.nodes.iter().filter_map(|&id| graph.node(id)).map(|node| size_of(&node.kind)[0]).fold(0.0, f32::max);
    let width = ((count.max(1) - 1) as f32 * COLUMN + widest + MARGIN * 2.0).max(360.0);
    let header = text_height(width, &part.text);
    let mut downs = vec![at[1] + header; count];
    for &id in &part.nodes {
        let across = columns[&id];
        let Some(node) = graph.node_mut(id) else { continue };
        node.at = [snap(at[0] + MARGIN + across as f32 * COLUMN), snap(downs[across])];
        downs[across] += size_of(&node.kind)[1] + BETWEEN;
    }
    let height = downs.iter().fold(at[1] + header, |most, &down| most.max(down)) - at[1] + MARGIN - BETWEEN;
    graph.add_comment(at, [snap(width), snap(height)], &part.title, &part.text);
    if let Some(inner) = &part.within {
        // Round its own nodes, where they now are.
        let placed: Vec<([f32; 2], [f32; 2])> =
            inner.nodes.iter().filter_map(|&id| graph.node(id)).map(|node| (node.at, size_of(&node.kind))).collect();
        if !placed.is_empty() {
            let low = placed.iter().fold([f32::MAX; 2], |low, (at, _)| [low[0].min(at[0]), low[1].min(at[1])]);
            let high = placed
                .iter()
                .fold([f32::MIN; 2], |high, (at, size)| [high[0].max(at[0] + size[0]), high[1].max(at[1] + size[1])]);
            let inner_width = high[0] - low[0] + 40.0;
            let inner_header = text_height(inner_width, &inner.text);
            let corner = [low[0] - 20.0, low[1] - inner_header];
            graph.add_comment(corner, [snap(inner_width), snap(high[1] - corner[1] + 20.0)], &inner.title, &inner.text);
        }
    }
    [snap(width), snap(height)]
}

/// How many columns along node `id` goes, among `part`: noted in `columns`.
fn column(graph: &Graph, part: &[NodeId], id: NodeId, columns: &mut HashMap<NodeId, usize>) -> usize {
    if let Some(&known) = columns.get(&id) {
        return known;
    }
    // Wires run from lower numbers to higher within a part, so this ends; a wire the other way is left out.
    let reads: Vec<NodeId> = graph
        .links
        .iter()
        .filter(|link| link.to == id && part.contains(&link.from) && link.from < id)
        .map(|link| link.from)
        .collect();
    let along = reads.into_iter().map(|from| column(graph, part, from, columns) + 1).max().unwrap_or(0);
    columns.insert(id, along);
    along
}

/// About how large the graph editor draws a node of this kind: as wide as
/// its title and its ports' labels need, and as tall as its ports, its
/// picture and its line of small text.
fn size_of(kind: &NodeKind) -> [f32; 2] {
    let (inputs, outputs) = (kind.inputs(), kind.outputs());
    let rows = inputs.len().max(outputs.len());
    let label = |port: Option<&super::Port>| port.map_or(0.0, |port| port.label.chars().count() as f32 * 6.3);
    let labels = (0..rows).map(|row| label(inputs.get(row)) + label(outputs.get(row)) + 40.0);
    let title = kind.title().chars().count() as f32 * 6.9 + 16.0;
    let width = labels.fold(144.0f32.max(title), f32::max);
    let picture = if outputs.is_empty() { 0.0 } else { 136.0 + 16.0 };
    [(width / 10.0).ceil() * 10.0, 30.0 + rows as f32 * 20.0 + picture]
}

fn snap(value: f32) -> f32 {
    (value / 10.0).round() * 10.0
}
