//! The Math node: two numbers added, taken from one another, multiplied,
//! divided, or the lesser or greater of them, at each place. Either is
//! what is wired in, or the node's own value with no wire. A number
//! divided by nothing is nothing.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// What a Math node does with its two numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    #[default]
    Add,
    Subtract,
    Multiply,
    Min,
    Max,
    Divide,
}

impl Operation {
    pub const ALL: [Operation; 6] =
        [Operation::Add, Operation::Subtract, Operation::Multiply, Operation::Min, Operation::Max, Operation::Divide];

    pub fn name(self) -> &'static str {
        match self {
            Operation::Add => "Add",
            Operation::Subtract => "Subtract",
            Operation::Multiply => "Multiply",
            Operation::Min => "Min",
            Operation::Max => "Max",
            Operation::Divide => "Divide",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Math {
    pub operation: Operation,
    pub a: f32,
    pub b: f32,
}

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("operation", "Operation", &["Add", "Subtract", "Multiply", "Min", "Max", "Divide"]),
    ValueSpec::number("a", "A", f32::MIN, f32::MAX, "").for_input(0),
    ValueSpec::number("b", "B", f32::MIN, f32::MAX, "").for_input(1),
];

const INPUTS: &[Port] = &[Port::field("a", "A"), Port::field("b", "B")];

const OUTPUTS: &[Port] = &[Port::field("result", "Result")];

impl Kind for Math {
    const TITLE: &'static str = "Math";
    const FOLDER: &'static str = "Shape";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(Operation::ALL.iter().position(|&operation| operation == self.operation).unwrap_or(0)),
            1 => Value::Number(self.a),
            2 => Value::Number(self.b),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Choice(choice)) => self.operation = Operation::ALL[choice.min(Operation::ALL.len() - 1)],
            (1, Value::Number(number)) => self.a = number,
            (2, Value::Number(number)) => self.b = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, _: usize, at: Places) -> V {
        let a = cx.input(node, 0, at).unwrap_or_else(|| cx.constant(self.a));
        let b = cx.input(node, 1, at).unwrap_or_else(|| cx.constant(self.b));
        cx.run(Two(self.operation), &[a, b])
    }
}

/// One operation on two rows.
#[derive(Debug)]
struct Two(Operation);

impl Kernel for Two {
    fn key(&self, key: &mut Key) {
        key.name("math").text(self.0.name());
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let pairs = out.iter_mut().zip(inputs[0].iter().zip(inputs[1]));
        // The lesser and greater by comparing, which says the same on every machine (for a zero of either sign too).
        match self.0 {
            Operation::Add => pairs.for_each(|(out, (a, b))| *out = a + b),
            Operation::Subtract => pairs.for_each(|(out, (a, b))| *out = a - b),
            Operation::Multiply => pairs.for_each(|(out, (a, b))| *out = a * b),
            Operation::Min => pairs.for_each(|(out, (&a, &b))| *out = if a < b { a } else { b }),
            Operation::Max => pairs.for_each(|(out, (&a, &b))| *out = if a > b { a } else { b }),
            Operation::Divide => pairs.for_each(|(out, (&a, &b))| *out = if b == 0.0 { 0.0 } else { a / b }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        NodeKind,
        nodes::{Constant, Noise, testing},
    };

    #[test]
    fn math_divides_and_a_number_divided_by_nothing_is_nothing() {
        let hills = Noise { size: 300.0, low: -40.0, high: 90.0, ..Noise::default() };
        let divide = |by: f32| {
            let kind = NodeKind::Math(Math { operation: Operation::Divide, b: by, ..Math::default() });
            testing::world_of(kind, 0, &[Some(NodeKind::Noise(hills.clone()))])
        };
        let (quarter, none) = (divide(4.0), divide(0.0));
        let (low, high) = testing::checked(&quarter);
        assert!(low >= -10.0 && high <= 22.5 && high - low > 10.0, "{low} to {high}");
        assert_eq!(testing::checked(&none), (0.0, 0.0));
        let was = testing::flat(&testing::world_of(NodeKind::Noise(hills.clone()), 0, &[]), 4000);
        assert!(testing::flat(&quarter, 4000).iter().zip(&was).all(|(made, was)| *made == was / 4.0));
        // Each operation is one of the row's choices, by its place among them.
        let constant = |value: f32| Some(NodeKind::Constant(Constant { value }));
        for (at, meant) in [9.0, 3.0, 18.0, 3.0, 6.0, 2.0].into_iter().enumerate() {
            let mut kind = NodeKind::Math(Math::default());
            kind.set_value(0, Value::Choice(at));
            assert_eq!(kind.value(0), Some(Value::Choice(at)));
            let world = testing::world_of(kind, 0, &[constant(6.0), constant(3.0)]);
            assert_eq!(world.height_at(1.0, 2.0), meant);
        }
    }
}
