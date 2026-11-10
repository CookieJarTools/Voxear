//! A generator's graph: its nodes, the wires between their ports, and the
//! values of its nodes it shows as rows of its own.
//!
//! A node has a number of its own, kept in the file and never given to
//! another, so wires and noise seeds stay with it whatever is added or
//! removed round it; and a place on the graph editor's canvas. A wire runs
//! from one node's output to another's input, one at most into each input,
//! and never round in a loop.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use super::{
    landmass::{Climate, Landmass},
    nodes::{Land, NodeKind, Output, Value},
};

/// A node's number in its graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// One node of a graph: what it is and its values, and where it is on the
/// canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub at: [f32; 2],
    pub kind: NodeKind,
}

/// A wire: from output `output` of node `from` into input `input` of node
/// `to`, each counted among its node's ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: NodeId,
    pub output: usize,
    pub to: NodeId,
    pub input: usize,
}

/// A node's value shown as a row of the generator's own, under `label`:
/// value `value` of node `node`, counted among its values.
#[derive(Clone, Debug, PartialEq)]
pub struct Exposed {
    pub node: NodeId,
    pub value: usize,
    pub label: String,
}

/// A generator's nodes, wires and exposed values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub exposed: Vec<Exposed>,
}

/// The numbers the nodes of a graph made from a landmass have.
const LANDMASS: NodeId = NodeId(1);
const CLIMATE: NodeId = NodeId(2);
const OUTPUT: NodeId = NodeId(3);

impl Graph {
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    /// Adds a node at `at` on the canvas, with a number none has had.
    pub fn add(&mut self, kind: NodeKind, at: [f32; 2]) -> NodeId {
        let id = NodeId(self.nodes.iter().map(|node| node.id.0).max().unwrap_or(0) + 1);
        self.nodes.push(Node { id, at, kind });
        id
    }

    /// Removes a node, with its wires and the rows it showed.
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.retain(|node| node.id != id);
        self.links.retain(|link| link.from != id && link.to != id);
        self.exposed.retain(|exposed| exposed.node != id);
    }

    /// The wire into input `input` of node `to`, if it has one.
    pub fn link_into(&self, to: NodeId, input: usize) -> Option<&Link> {
        self.links.iter().find(|link| link.to == to && link.input == input)
    }

    /// Wires output `output` of `from` into input `input` of `to`, in place
    /// of any wire into it: false, and nothing done, where either port is
    /// missing, the output's kind cannot feed the input's, or the wire
    /// would close a loop.
    pub fn connect(&mut self, from: NodeId, output: usize, to: NodeId, input: usize) -> bool {
        let (Some(source), Some(target)) = (self.node(from), self.node(to)) else { return false };
        let (Some(out), Some(into)) = (source.kind.outputs().get(output), target.kind.inputs().get(input)) else {
            return false;
        };
        if !out.kind.feeds(into.kind) || self.reads(from, to) {
            return false;
        }
        self.links.retain(|link| !(link.to == to && link.input == input));
        self.links.push(Link { from, output, to, input });
        true
    }

    /// Takes away the wire into input `input` of `to`: whether it had one.
    pub fn disconnect(&mut self, to: NodeId, input: usize) -> bool {
        let before = self.links.len();
        self.links.retain(|link| !(link.to == to && link.input == input));
        self.links.len() != before
    }

    /// Whether `node` reads `from`, itself or through others: so a wire
    /// from `node` into `from` would close a loop.
    pub fn reads(&self, node: NodeId, from: NodeId) -> bool {
        let (mut seen, mut next) = (vec![node], vec![node]);
        while let Some(at) = next.pop() {
            if at == from {
                return true;
            }
            for link in self.links.iter().filter(|link| link.to == at) {
                if !seen.contains(&link.from) {
                    seen.push(link.from);
                    next.push(link.from);
                }
            }
        }
        false
    }

    /// Its Output node: the one with the lowest number, where it has several.
    pub fn output(&self) -> Option<&Node> {
        self.nodes.iter().filter(|node| matches!(node.kind, NodeKind::Output(_))).min_by_key(|node| node.id)
    }

    /// Shows value `value` of `node` as a row of the generator's own under
    /// `label`: false where the node has no such value.
    pub fn expose(&mut self, node: NodeId, value: usize, label: &str) -> bool {
        if self.node(node).is_none_or(|found| found.kind.values().get(value).is_none()) {
            return false;
        }
        self.exposed.retain(|exposed| !(exposed.node == node && exposed.value == value));
        self.exposed.push(Exposed { node, value, label: label.to_string() });
        true
    }

    /// Exposed row `row`'s value now.
    pub fn exposed_value(&self, row: usize) -> Option<Value> {
        let exposed = self.exposed.get(row)?;
        self.node(exposed.node)?.kind.value(exposed.value)
    }

    /// Sets exposed row `row`'s value: whether it changed.
    pub fn set_exposed(&mut self, row: usize, value: Value) -> bool {
        let Some((node, at)) = self.exposed.get(row).map(|exposed| (exposed.node, exposed.value)) else { return false };
        self.node_mut(node).is_some_and(|node| node.kind.set_value(at, value))
    }

    /// The graph a landmass makes alone: a Landmass node wired to the
    /// Output's height, a Climate node to its temperature and humidity, and
    /// its height, temperature and humidity shown as rows.
    pub fn of_landmass(land: &Landmass) -> Self {
        let mut graph = Graph {
            nodes: vec![
                Node { id: LANDMASS, at: [0.0, 0.0], kind: NodeKind::Landmass(Land::from(land)) },
                Node { id: CLIMATE, at: [0.0, 260.0], kind: NodeKind::Climate(land.climate) },
                Node { id: OUTPUT, at: [320.0, 120.0], kind: NodeKind::Output(Output::default()) },
            ],
            ..Graph::default()
        };
        let wired = graph.connect(LANDMASS, Land::HEIGHT_OUT, OUTPUT, Output::HEIGHT)
            && graph.connect(CLIMATE, Climate::TEMPERATURE_OUT, OUTPUT, Output::TEMPERATURE)
            && graph.connect(CLIMATE, Climate::HUMIDITY_OUT, OUTPUT, Output::HUMIDITY);
        let shown = graph.expose(LANDMASS, Land::HEIGHT, "Height")
            && graph.expose(CLIMATE, Climate::TEMPERATURE, "Temperature")
            && graph.expose(CLIMATE, Climate::HUMIDITY, "Humidity");
        debug_assert!(wired && shown, "a landmass's graph is whole");
        graph
    }

    /// The landmass it is, in a world with this seed, scale and sea level,
    /// where the ground and climate it makes are one landmass's and nothing
    /// more: a Landmass node alone into its Output's height, and a Climate
    /// node alone into its temperature and humidity, cooled by the ground
    /// asked about. None for any other graph.
    pub fn as_landmass(&self, seed: u32, scale: f32, sea_level: f32) -> Option<Landmass> {
        let output = self.output()?.id;
        let source = |input: usize| self.link_into(output, input).and_then(|link| Some((self.node(link.from)?, link)));
        let (ground, height) = source(Output::HEIGHT)?;
        let (weather, warmth) = source(Output::TEMPERATURE)?;
        let (wet_from, wetness) = source(Output::HUMIDITY)?;
        let (NodeKind::Landmass(land), NodeKind::Climate(climate)) = (&ground.kind, &weather.kind) else { return None };
        let whole = height.output == Land::HEIGHT_OUT
            && warmth.output == Climate::TEMPERATURE_OUT
            && wetness.output == Climate::HUMIDITY_OUT
            && wet_from.id == weather.id
            && self.link_into(weather.id, Climate::HEIGHT_IN).is_none();
        whole.then(|| land.landmass(seed, scale, sea_level, *climate))
    }

    /// As a `.worldgen` file keeps it, ports and values by their names.
    pub(crate) fn written(&self) -> Written {
        let name = |id: NodeId, pick: &dyn Fn(&NodeKind) -> Option<&'static str>| {
            self.node(id).and_then(|node| pick(&node.kind)).unwrap_or("").to_string()
        };
        Written {
            nodes: self
                .nodes
                .iter()
                .map(|node| WrittenNode { id: node.id.0, at: node.at, kind: node.kind.clone() })
                .collect(),
            links: self
                .links
                .iter()
                .map(|link| WrittenLink {
                    from: link.from.0,
                    out: name(link.from, &|kind| kind.outputs().get(link.output).map(|port| port.name)),
                    to: link.to.0,
                    into: name(link.to, &|kind| kind.inputs().get(link.input).map(|port| port.name)),
                })
                .collect(),
            exposed: self
                .exposed
                .iter()
                .map(|exposed| WrittenExposed {
                    node: exposed.node.0,
                    value: name(exposed.node, &|kind| kind.values().get(exposed.value).map(|value| value.name)),
                    label: exposed.label.clone(),
                })
                .collect(),
        }
    }

    /// As read from a file: an error where two nodes share a number, or a
    /// wire or row names a node, port or value there is none of, or a wire
    /// cannot be (see [`Graph::connect`]).
    pub(crate) fn read(written: Written) -> anyhow::Result<Self> {
        let mut graph = Graph::default();
        for node in written.nodes {
            if graph.node(NodeId(node.id)).is_some() {
                bail!("two nodes are numbered {}", node.id);
            }
            graph.nodes.push(Node { id: NodeId(node.id), at: node.at, kind: node.kind });
        }
        for link in written.links {
            let (from, to) = (NodeId(link.from), NodeId(link.to));
            let source =
                graph.node(from).with_context(|| format!("a wire from node {}, which is missing", link.from))?;
            let target = graph.node(to).with_context(|| format!("a wire into node {}, which is missing", link.to))?;
            let output = source.kind.outputs().iter().position(|port| port.name == link.out);
            let input = target.kind.inputs().iter().position(|port| port.name == link.into);
            let (Some(output), Some(input)) = (output, input) else {
                bail!(
                    "a wire from {} of node {} into {} of node {}: no such port",
                    link.out,
                    link.from,
                    link.into,
                    link.to
                );
            };
            if !graph.connect(from, output, to, input) {
                bail!(
                    "the wire from {} of node {} cannot go into {} of node {}",
                    link.out,
                    link.from,
                    link.into,
                    link.to
                );
            }
        }
        for exposed in written.exposed {
            let node = graph.node(NodeId(exposed.node));
            let value = node.and_then(|node| node.kind.values().iter().position(|value| value.name == exposed.value));
            let Some(value) = value else {
                bail!("the row {} shows {} of node {}: no such value", exposed.label, exposed.value, exposed.node);
            };
            graph.expose(NodeId(exposed.node), value, &exposed.label);
        }
        Ok(graph)
    }
}

/// A graph as a file keeps it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Written {
    pub nodes: Vec<WrittenNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<WrittenLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposed: Vec<WrittenExposed>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WrittenNode {
    id: u32,
    #[serde(default)]
    at: [f32; 2],
    #[serde(flatten)]
    kind: NodeKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WrittenLink {
    from: u32,
    out: String,
    to: u32,
    #[serde(rename = "in")]
    into: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WrittenExposed {
    node: u32,
    value: String,
    label: String,
}
