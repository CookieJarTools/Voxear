//! The Constant node: one number, the same at every place.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Places, V},
    graph::NodeId,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Constant {
    pub value: f32,
}

const OUTPUTS: &[Port] = &[Port::field("value", "Value")];

const VALUES: &[ValueSpec] = &[ValueSpec::number("value", "Value", f32::MIN, f32::MAX, "")];

impl Kind for Constant {
    const TITLE: &'static str = "Constant";
    const FOLDER: &'static str = "Sources";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        (at == 0).then_some(Value::Number(self.value))
    }

    fn set(&mut self, at: usize, value: Value) {
        if let (0, Value::Number(number)) = (at, value) {
            self.value = number;
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, _: Places) -> V {
        cx.constant(self.value)
    }
}
