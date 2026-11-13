//! The types a graph's values have: a number, two to four of them, or a
//! truth.

use serde::{Deserialize, Serialize};

/// What a value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ty {
    Float,
    Float2,
    Float3,
    Float4,
    Bool,
}

impl Ty {
    /// How many numbers it holds; a truth counts as one.
    pub fn width(self) -> usize {
        match self {
            Self::Float | Self::Bool => 1,
            Self::Float2 => 2,
            Self::Float3 => 3,
            Self::Float4 => 4,
        }
    }

    /// The type of numbers so many wide.
    pub fn of_width(width: usize) -> Self {
        match width {
            0 | 1 => Self::Float,
            2 => Self::Float2,
            3 => Self::Float3,
            _ => Self::Float4,
        }
    }

    pub fn is_number(self) -> bool {
        self != Self::Bool
    }

    /// As Slang names it.
    pub fn slang(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Float2 => "float2",
            Self::Float3 => "float3",
            Self::Float4 => "float4",
            Self::Bool => "bool",
        }
    }

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Float => "Number",
            Self::Float2 => "Vector 2",
            Self::Float3 => "Vector 3",
            Self::Float4 => "Vector 4",
            Self::Bool => "True or False",
        }
    }
}
