//! A shader graph: its properties, its nodes and the wires between their
//! ports, as a `.shadergraph` file keeps them.
//!
//! A node and a property each have a number of their own, kept in the file
//! and never given to another, so wires and the nodes that read a property
//! stay with it whatever is added or taken away round it. A wire runs from
//! one node's output into another's input, one wire at most into each
//! input. An input with no wire takes the numbers the node keeps for it, or
//! else the port's own.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{nodes::NodeKind, types::Ty};

/// A node's number in its graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

/// A property's number in its graph, which the nodes that read it name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PropertyId(pub u32);

/// What a property holds, as a material sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    Number,
    Vector2,
    Vector3,
    Vector4,
    /// A colour picked as it looks, without alpha or with it.
    Color,
    ColorAlpha,
    /// One of the project's pictures.
    Picture,
}

impl PropertyKind {
    pub const ALL: [Self; 7] =
        [Self::Number, Self::Vector2, Self::Vector3, Self::Vector4, Self::Color, Self::ColorAlpha, Self::Picture];

    /// The value a Property node gives for it; none for a picture, which is sampled.
    pub fn ty(self) -> Option<Ty> {
        match self {
            Self::Number => Some(Ty::Float),
            Self::Vector2 => Some(Ty::Float2),
            Self::Vector3 | Self::Color => Some(Ty::Float3),
            Self::Vector4 | Self::ColorAlpha => Some(Ty::Float4),
            Self::Picture => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Vector2 => "Vector 2",
            Self::Vector3 => "Vector 3",
            Self::Vector4 => "Vector 4",
            Self::Color => "Color",
            Self::ColorAlpha => "Color with Alpha",
            Self::Picture => "Picture",
        }
    }
}

/// A value each material using the shader sets: its name, as the
/// material's file keeps it, what it is, what it starts as, and for a
/// number how far it may go, and what it does, for the Inspector.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub id: PropertyId,
    pub name: String,
    pub kind: PropertyKind,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub default: [f32; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tooltip: String,
}

fn is_zero(value: &[f32; 4]) -> bool {
    *value == [0.0; 4]
}

/// Numbers a node keeps for one of its inputs, used while nothing is wired in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Literal {
    pub input: usize,
    pub value: [f32; 4],
}

/// One node of a graph: what it is, its values, and where it is on the
/// canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(default)]
    pub at: [f32; 2],
    #[serde(flatten)]
    pub kind: NodeKind,
    #[serde(default, rename = "literal", skip_serializing_if = "Vec::is_empty")]
    pub literals: Vec<Literal>,
}

/// A wire: from output `output` of node `from` into input `input` of node
/// `to`, each counted among its node's ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Link {
    pub from: NodeId,
    pub output: usize,
    pub to: NodeId,
    pub input: usize,
}

/// A shader made of nodes: what its properties struct is named, how it is
/// drawn, its properties, nodes and wires.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Graph {
    /// The properties struct's name, as the shader's Slang names it.
    pub name: String,
    /// Drawn as its surface's colour and emission alone, without light.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unlit: bool,
    /// How far its Vertex Output moves a vertex at most, in metres, which
    /// culling allows for.
    #[serde(skip_serializing_if = "is_none")]
    pub max_offset: f32,
    #[serde(rename = "property", skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
    #[serde(rename = "node", skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<Node>,
    #[serde(rename = "link", skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
}

fn is_none(value: &f32) -> bool {
    *value == 0.0
}

/// Whether `name` will do as a name in Slang: letters, digits and
/// underscores, not starting with a digit.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Graph {
    /// A graph with nothing in it but a Surface Output, for a struct named `name`.
    pub fn new(name: &str) -> Self {
        let mut graph = Self { name: name.into(), ..Self::default() };
        graph.add(NodeKind::SurfaceOutput, [400.0, 0.0]);
        graph
    }

    /// Reads a graph from its file's text.
    pub fn from_toml(text: &str) -> Result<Self> {
        let graph: Self = toml::from_str(text).context("reading a shader graph")?;
        graph.check()?;
        Ok(graph)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a graph is always written")
    }

    /// What is wrong with how it is put together, if anything: a name Slang
    /// will not take, two of anything with one number, a wire to a port or
    /// node that is not there, or two wires into one input.
    pub fn check(&self) -> Result<()> {
        if !is_identifier(&self.name) {
            bail!("\"{}\" will not do as a shader's name: use letters, digits and _", self.name);
        }
        let mut seen = std::collections::HashSet::new();
        for property in &self.properties {
            if !is_identifier(&property.name) || !seen.insert(property.name.clone()) {
                bail!("the property \"{}\" needs a name of its own, of letters, digits and _", property.name);
            }
        }
        let mut ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if !ids.insert(node.id) {
                bail!("two nodes are numbered {}", node.id.0);
            }
        }
        let mut fed = std::collections::HashSet::new();
        for link in &self.links {
            let (Some(from), Some(to)) = (self.node(link.from), self.node(link.to)) else {
                bail!("a wire runs to a node that is not there");
            };
            let property = |kind: &NodeKind| match kind {
                NodeKind::Property { property } => self.property(*property).and_then(|p| p.kind.ty()),
                _ => None,
            };
            if link.output >= from.kind.outputs(property(&from.kind)).len() || link.input >= to.kind.inputs().len() {
                bail!("a wire runs to a port {} or {} does not have", from.kind.title(), to.kind.title());
            }
            if !fed.insert((link.to, link.input)) {
                bail!("two wires go into one input of {}", to.kind.title());
            }
        }
        Ok(())
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn property(&self, id: PropertyId) -> Option<&Property> {
        self.properties.iter().find(|property| property.id == id)
    }

    /// What a Property node reading `id` gives.
    pub fn property_ty(&self, id: PropertyId) -> Option<Ty> {
        self.property(id).and_then(|property| property.kind.ty())
    }

    /// Adds a node at `at`; its number.
    pub fn add(&mut self, kind: NodeKind, at: [f32; 2]) -> NodeId {
        let id = NodeId(self.nodes.iter().map(|node| node.id.0).max().unwrap_or(0) + 1);
        self.nodes.push(Node { id, at, kind, literals: Vec::new() });
        id
    }

    /// Adds a property; its number.
    pub fn add_property(&mut self, name: &str, kind: PropertyKind, default: [f32; 4]) -> PropertyId {
        let id = PropertyId(self.properties.iter().map(|property| property.id.0).max().unwrap_or(0) + 1);
        self.properties.push(Property { id, name: name.into(), kind, default, range: None, tooltip: String::new() });
        id
    }

    /// Wires output `output` of `from` into input `input` of `to`, in place
    /// of any wire into that input.
    pub fn link(&mut self, from: NodeId, output: usize, to: NodeId, input: usize) {
        self.links.retain(|link| !(link.to == to && link.input == input));
        self.links.push(Link { from, output, to, input });
    }

    /// Sets the numbers `node` keeps for input `input`.
    pub fn set_literal(&mut self, node: NodeId, input: usize, value: [f32; 4]) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node) {
            node.literals.retain(|literal| literal.input != input);
            node.literals.push(Literal { input, value });
        }
    }

    /// Takes a node away, and the wires to and from it.
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.retain(|node| node.id != id);
        self.links.retain(|link| link.from != id && link.to != id);
    }

    /// The wire into input `input` of `node`, if any.
    pub fn link_into(&self, node: NodeId, input: usize) -> Option<&Link> {
        self.links.iter().find(|link| link.to == node && link.input == input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{Input, MathOp};

    #[test]
    fn a_graph_is_written_and_read_back_with_only_what_is_set() {
        let mut graph = Graph::new("Glow");
        let tint = graph.add_property("tint", PropertyKind::Color, [1.0, 0.5, 0.0, 0.0]);
        let uv = graph.add(NodeKind::Input { input: Input::Uv }, [0.0, 0.0]);
        let times = graph.add(NodeKind::Math { op: MathOp::Multiply }, [200.0, 0.0]);
        graph.set_literal(times, 1, [2.0; 4]);
        let colour = graph.add(NodeKind::Property { property: tint }, [0.0, 100.0]);
        graph.link(uv, 0, times, 0);
        graph.link(colour, 0, NodeId(1), 0);
        let text = graph.to_toml();
        assert!(text.contains("kind = \"math\"") && text.contains("op = \"multiply\""), "{text}");
        assert!(!text.contains("unlit") && !text.contains("tooltip"), "nothing left as new: {text}");
        assert_eq!(Graph::from_toml(&text).unwrap(), graph);
    }

    #[test]
    fn a_graph_put_together_wrongly_is_refused() {
        let mut graph = Graph::new("Bad");
        let uv = graph.add(NodeKind::Input { input: Input::Uv }, [0.0, 0.0]);
        graph.links.push(Link { from: uv, output: 3, to: NodeId(1), input: 0 });
        assert!(graph.check().is_err(), "an output it does not have");
        graph.links.clear();
        graph.links.push(Link { from: uv, output: 0, to: NodeId(1), input: 0 });
        graph.links.push(Link { from: uv, output: 0, to: NodeId(1), input: 0 });
        assert!(graph.check().is_err(), "two wires into one input");
        assert!(Graph::from_toml("name = \"2bad\"").is_err(), "not a name Slang takes");
        let mut twice = Graph::new("Twice");
        twice.add_property("tint", PropertyKind::Color, [0.0; 4]);
        twice.add_property("tint", PropertyKind::Number, [0.0; 4]);
        assert!(twice.check().is_err());
    }
}
