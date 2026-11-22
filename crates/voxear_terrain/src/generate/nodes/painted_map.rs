//! The Painted Map node: a world's painted Land, Height or Biome map as a
//! field, its value where painted and Default where not (see
//! `generate::painted`). The world's maps are given it as its generator is
//! made for the world; in a generator on its own, as its preview shows it,
//! it is Default everywhere.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
    painted::{PaintKind, Painted},
};

/// A painted map read as a field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaintedMapNode {
    pub map: PaintedMapKind,
    /// Its value where nothing is painted.
    pub default: f32,
    /// The world's maps, given it as the world's generator is made: never in
    /// its file.
    #[serde(skip)]
    pub painted: Option<Painted>,
}

/// Which of a world's maps a Painted Map node reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaintedMapKind {
    #[default]
    Land,
    Height,
    Biome,
}

impl PaintedMapKind {
    pub const ALL: [PaintedMapKind; 3] = [PaintedMapKind::Land, PaintedMapKind::Height, PaintedMapKind::Biome];

    fn kind(self) -> PaintKind {
        match self {
            PaintedMapKind::Land => PaintKind::Land,
            PaintedMapKind::Height => PaintKind::Height,
            PaintedMapKind::Biome => PaintKind::Biome,
        }
    }
}

impl Default for PaintedMapNode {
    fn default() -> Self {
        Self { map: PaintedMapKind::Land, default: 0.0, painted: None }
    }
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("map", "Map", &["Land", "Height", "Biome"]),
    ValueSpec::number("default", "Default", f32::MIN, f32::MAX, ""),
];

const OUTPUTS: &[Port] = &[Port::field("value", "Value")];

impl Kind for PaintedMapNode {
    const TITLE: &'static str = "Painted Map";
    const FOLDER: &'static str = "Sources";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(PaintedMapKind::ALL.iter().position(|&map| map == self.map).unwrap_or(0)),
            1 => Value::Number(self.default),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.map = PaintedMapKind::ALL[choice.min(PaintedMapKind::ALL.len() - 1)],
            (1, Value::Number(number)) => self.default = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, at: Places) -> V {
        match &self.painted {
            Some(painted) => {
                cx.run(Read { painted: painted.clone(), map: self.map, default: self.default }, &[at.x, at.z])
            }
            None => cx.constant(self.default),
        }
    }
}

/// A world's painted map read at rows of x and z.
#[derive(Debug)]
struct Read {
    painted: Painted,
    map: PaintedMapKind,
    default: f32,
}

impl Kernel for Read {
    fn key(&self, key: &mut Key) {
        // Known by the maps' handle, not what they hold, which changes as they are painted.
        key.name("painted map").whole(self.map as u32).number(self.default).text(&self.painted.id.to_string());
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let maps = self.painted.maps();
        let map = maps.map(self.map.kind());
        for ((out, &x), &z) in out.iter_mut().zip(inputs[0]).zip(inputs[1]) {
            *out = match self.map {
                PaintedMapKind::Biome => map.nearest(x, z),
                _ => map.sample(x, z, self.default),
            }
            .unwrap_or(self.default);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing, painted::PaintedMaps};

    #[test]
    fn a_painted_map_node_reads_its_worlds_map_and_its_default_elsewhere() {
        let painted = Painted::new(4, PaintedMaps::default());
        painted.change(|maps| maps.height.paint([100.0, 100.0], 30.0, 0.0, &|_, _| Some(0.8)));
        let node = PaintedMapNode { map: PaintedMapKind::Height, default: -1.0, painted: Some(painted) };
        let world = testing::world_of(NodeKind::PaintedMap(node), 0, &[]);
        assert!((world.height_at(100.0, 100.0) - 0.8).abs() < 1e-4);
        assert_eq!(world.height_at(900.0, 900.0), -1.0);
        let alone = testing::world_of(
            NodeKind::PaintedMap(PaintedMapNode { default: 2.0, ..PaintedMapNode::default() }),
            0,
            &[],
        );
        assert_eq!(alone.height_at(100.0, 100.0), 2.0, "Default with no world's maps");
    }
}
