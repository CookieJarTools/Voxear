//! The nodes a VoxShade shader is made of, and the ports each has.
//!
//! A port's type is fixed, or for most of the maths any width of numbers:
//! such a node works on the widest of what is wired into it, a single
//! number into a wider port standing for that number in every place, a
//! wider value into a narrower port giving up its last numbers.

use serde::{Deserialize, Serialize};

use crate::{graph::PropertyId, types::Ty};

/// Which of a shader's functions a graph's nodes run in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    /// Once a pixel: the surface's colour, normal, how rough and what it gives off.
    Surface,
    /// Once a vertex: how far the vertex moves.
    Vertex,
    /// Once a pixel for each light: how much of the light the surface sends to the camera.
    Light,
}

impl Stage {
    pub const ALL: [Self; 3] = [Self::Surface, Self::Vertex, Self::Light];

    pub fn name(self) -> &'static str {
        match self {
            Self::Surface => "Surface",
            Self::Vertex => "Vertex",
            Self::Light => "Light",
        }
    }
}

/// What a shader is given where it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// The mesh's texture coordinates.
    Uv,
    /// Where it is in the world.
    Position,
    /// The way the surface faces, in the world.
    Normal,
    /// The way to the camera.
    View,
    /// Where the Obj is in the world.
    ObjectPosition,
    /// Where it is, measured from the level's middle: patterns read from it
    /// stay put as the world's middle moves with the camera.
    LevelPosition,
    /// The time in seconds.
    Time,
    /// How far the view goes through a Glass material, in metres.
    Thickness,
    /// The way to a light, its colour as it arrives, and the way to the camera, as `light` sees them.
    LightDirection,
    LightColor,
    LightView,
}

impl Input {
    pub const ALL: [Self; 11] = [
        Self::Uv,
        Self::Position,
        Self::Normal,
        Self::View,
        Self::ObjectPosition,
        Self::LevelPosition,
        Self::Time,
        Self::Thickness,
        Self::LightDirection,
        Self::LightColor,
        Self::LightView,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Uv => "UV",
            Self::Position => "Position",
            Self::Normal => "Normal",
            Self::View => "View Direction",
            Self::ObjectPosition => "Object Position",
            Self::LevelPosition => "Level Position",
            Self::Time => "Time",
            Self::Thickness => "Thickness",
            Self::LightDirection => "Light Direction",
            Self::LightColor => "Light Color",
            Self::LightView => "Light View",
        }
    }

    pub fn ty(self) -> Ty {
        match self {
            Self::Uv => Ty::Float2,
            Self::Time | Self::Thickness => Ty::Float,
            _ => Ty::Float3,
        }
    }

    /// Whether `stage` is given it.
    pub fn in_stage(self, stage: Stage) -> bool {
        match self {
            Self::Uv | Self::Position | Self::Normal | Self::ObjectPosition | Self::LevelPosition | Self::Time => {
                stage != Stage::Light
            }
            Self::View | Self::Thickness => stage == Stage::Surface,
            Self::LightDirection | Self::LightColor | Self::LightView => stage == Stage::Light,
        }
    }

    /// Whether it is the same all over a draw, rather than different from one place to the next.
    pub fn is_uniform(self) -> bool {
        self == Self::Time
    }
}

/// One of the sums a Math node does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Minimum,
    Maximum,
    Power,
    Negate,
    Absolute,
    Floor,
    Ceiling,
    Fraction,
    Sine,
    Cosine,
    SquareRoot,
    Exponential,
    Saturate,
    OneMinus,
    Sign,
    Normalize,
    Length,
    Distance,
    Dot,
    Cross,
    Reflect,
    Step,
    Lerp,
    Clamp,
    Smoothstep,
}

impl MathOp {
    pub const ALL: [Self; 29] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Minimum,
        Self::Maximum,
        Self::Power,
        Self::Negate,
        Self::Absolute,
        Self::Floor,
        Self::Ceiling,
        Self::Fraction,
        Self::Sine,
        Self::Cosine,
        Self::SquareRoot,
        Self::Exponential,
        Self::Saturate,
        Self::OneMinus,
        Self::Sign,
        Self::Normalize,
        Self::Length,
        Self::Distance,
        Self::Dot,
        Self::Cross,
        Self::Reflect,
        Self::Step,
        Self::Lerp,
        Self::Clamp,
        Self::Smoothstep,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::Minimum => "Minimum",
            Self::Maximum => "Maximum",
            Self::Power => "Power",
            Self::Negate => "Negate",
            Self::Absolute => "Absolute",
            Self::Floor => "Floor",
            Self::Ceiling => "Ceiling",
            Self::Fraction => "Fraction",
            Self::Sine => "Sine",
            Self::Cosine => "Cosine",
            Self::SquareRoot => "Square Root",
            Self::Exponential => "Exponential",
            Self::Saturate => "Saturate",
            Self::OneMinus => "One Minus",
            Self::Sign => "Sign",
            Self::Normalize => "Normalize",
            Self::Length => "Length",
            Self::Distance => "Distance",
            Self::Dot => "Dot",
            Self::Cross => "Cross",
            Self::Reflect => "Reflect",
            Self::Step => "Step",
            Self::Lerp => "Lerp",
            Self::Clamp => "Clamp",
            Self::Smoothstep => "Smoothstep",
        }
    }

    /// Its inputs: each one's label, and the number it takes when nothing is wired in.
    fn inputs(self) -> &'static [(&'static str, f32)] {
        match self {
            Self::Add | Self::Subtract => &[("A", 0.0), ("B", 0.0)],
            Self::Multiply | Self::Divide => &[("A", 1.0), ("B", 1.0)],
            Self::Minimum | Self::Maximum | Self::Dot | Self::Distance | Self::Cross | Self::Reflect => {
                &[("A", 0.0), ("B", 0.0)]
            }
            Self::Power => &[("Base", 1.0), ("Exponent", 2.0)],
            Self::Step => &[("Edge", 0.5), ("In", 0.0)],
            Self::Lerp => &[("A", 0.0), ("B", 1.0), ("T", 0.5)],
            Self::Clamp => &[("In", 0.0), ("Min", 0.0), ("Max", 1.0)],
            Self::Smoothstep => &[("Edge 1", 0.0), ("Edge 2", 1.0), ("In", 0.0)],
            _ => &[("In", 0.0)],
        }
    }

    /// Whether it gives a single number whatever it is given.
    pub fn gives_number(self) -> bool {
        matches!(self, Self::Length | Self::Distance | Self::Dot)
    }
}

/// How a Compare node compares A with B.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Equal,
    NotEqual,
}

impl CompareOp {
    pub const ALL: [Self; 6] =
        [Self::Greater, Self::GreaterOrEqual, Self::Less, Self::LessOrEqual, Self::Equal, Self::NotEqual];

    pub fn slang(self) -> &'static str {
        match self {
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
        }
    }

    pub fn holds(self, a: f32, b: f32) -> bool {
        match self {
            Self::Greater => a > b,
            Self::GreaterOrEqual => a >= b,
            Self::Less => a < b,
            Self::LessOrEqual => a <= b,
            Self::Equal => a == b,
            Self::NotEqual => a != b,
        }
    }
}

/// One of the numbers of a vector, for a Swizzle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    X,
    Y,
    Z,
    W,
}

impl Component {
    pub const ALL: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::W];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn letter(self) -> char {
        ['x', 'y', 'z', 'w'][self as usize]
    }
}

/// What a node is, and the values of its own that it keeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    /// What the surface is: its colour, how opaque, which way it faces, how
    /// metal and how rough, and what light it gives off. An input left
    /// unwired leaves the material's own.
    SurfaceOutput,
    /// How far each vertex moves, in metres, in the world.
    VertexOutput,
    /// How much of a light the surface sends to the camera, in place of the
    /// standard lighting.
    LightOutput,
    /// Something the shader is given where it runs.
    Input {
        input: Input,
    },
    /// The surface as it stands: the material's own, in Surface, or as
    /// Surface left it, in Light.
    Surface,
    /// A number, or numbers, set here.
    Constant {
        ty: Ty,
        value: [f32; 4],
    },
    /// A colour set here, as it looks on screen.
    Color {
        value: [f32; 4],
    },
    /// One of the shader's properties, which each material sets.
    Property {
        property: PropertyId,
    },
    /// A picture property read at a place.
    SamplePicture {
        property: PropertyId,
    },
    Math {
        op: MathOp,
    },
    /// Whether A is greater than B, or less, or the same.
    Compare {
        op: CompareOp,
    },
    /// One value or another, as a condition holds. Worked out as a real
    /// branch, so only one side runs, where that side is worth skipping.
    Branch,
    /// A vector's numbers one by one.
    Split,
    /// A vector made of numbers.
    Combine {
        ty: Ty,
    },
    /// A vector's numbers picked and put in an order.
    Swizzle {
        components: Vec<Component>,
    },
    /// A number from 0 to 1 that looks random, the same for the same seed.
    Random,
}

/// What a port takes or gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortTy {
    /// Exactly this.
    Fixed(Ty),
    /// Numbers of any width: the node works on the widest wired into it.
    Any,
}

impl PortTy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed(ty) => ty.name(),
            Self::Any => "Numbers",
        }
    }
}

/// A port as the node has it: its label, what it takes or gives, and for an
/// input, the numbers it has with nothing wired in.
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub label: &'static str,
    pub ty: PortTy,
    pub default: [f32; 4],
}

fn port(label: &'static str, ty: PortTy, default: f32) -> Port {
    Port { label, ty, default: [default; 4] }
}

/// What a surface output's inputs set, in order.
pub const SURFACE_OUTPUTS: [(&str, Ty); 6] = [
    ("Color", Ty::Float3),
    ("Alpha", Ty::Float),
    ("Normal", Ty::Float3),
    ("Metallic", Ty::Float),
    ("Roughness", Ty::Float),
    ("Emission", Ty::Float3),
];

impl NodeKind {
    /// As the editor's search and the node's title show it.
    pub fn title(&self) -> String {
        match self {
            Self::SurfaceOutput => "Surface Output".into(),
            Self::VertexOutput => "Vertex Output".into(),
            Self::LightOutput => "Light Output".into(),
            Self::Input { input } => input.name().into(),
            Self::Surface => "Surface".into(),
            Self::Constant { ty, .. } => ty.name().into(),
            Self::Color { .. } => "Color".into(),
            Self::Property { .. } => "Property".into(),
            Self::SamplePicture { .. } => "Sample Picture".into(),
            Self::Math { op } => op.name().into(),
            Self::Compare { .. } => "Compare".into(),
            Self::Branch => "Branch".into(),
            Self::Split => "Split".into(),
            Self::Combine { .. } => "Combine".into(),
            Self::Swizzle { .. } => "Swizzle".into(),
            Self::Random => "Random".into(),
        }
    }

    /// The stage its output node stands for, for the three output nodes.
    pub fn output_stage(&self) -> Option<Stage> {
        match self {
            Self::SurfaceOutput => Some(Stage::Surface),
            Self::VertexOutput => Some(Stage::Vertex),
            Self::LightOutput => Some(Stage::Light),
            _ => None,
        }
    }

    /// Whether it can run in `stage`: an input only where it is given, the
    /// surface as it stands only where there is one.
    pub fn in_stage(&self, stage: Stage) -> bool {
        match self {
            Self::Input { input } => input.in_stage(stage),
            Self::Surface => stage != Stage::Vertex,
            Self::SamplePicture { .. } => stage != Stage::Light,
            _ => true,
        }
    }

    pub fn inputs(&self) -> Vec<Port> {
        use PortTy::{Any, Fixed};
        match self {
            Self::SurfaceOutput => SURFACE_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
            Self::VertexOutput => vec![port("Offset", Fixed(Ty::Float3), 0.0)],
            Self::LightOutput => vec![port("Light", Fixed(Ty::Float3), 0.0)],
            Self::SamplePicture { .. } => vec![port("UV", Fixed(Ty::Float2), 0.0)],
            Self::Math { op } => {
                let ty = if *op == MathOp::Cross { Fixed(Ty::Float3) } else { Any };
                op.inputs().iter().map(|&(label, default)| port(label, ty, default)).collect()
            }
            Self::Compare { .. } => vec![port("A", Fixed(Ty::Float), 0.0), port("B", Fixed(Ty::Float), 0.0)],
            Self::Branch => {
                vec![port("Condition", Fixed(Ty::Bool), 0.0), port("True", Any, 1.0), port("False", Any, 0.0)]
            }
            Self::Split => vec![port("In", Fixed(Ty::Float4), 0.0)],
            Self::Combine { ty } => {
                ["X", "Y", "Z", "W"][..ty.width()].iter().map(|&l| port(l, Fixed(Ty::Float), 0.0)).collect()
            }
            Self::Swizzle { .. } => vec![port("In", Fixed(Ty::Float4), 0.0)],
            Self::Random => vec![port("Seed", Any, 0.0)],
            _ => Vec::new(),
        }
    }

    /// Its outputs; `properties` says what a Property node's is.
    pub fn outputs(&self, property: Option<Ty>) -> Vec<Port> {
        use PortTy::{Any, Fixed};
        let out = |label, ty| vec![port(label, ty, 0.0)];
        match self {
            Self::SurfaceOutput | Self::VertexOutput | Self::LightOutput => Vec::new(),
            Self::Input { input } => out("Out", Fixed(input.ty())),
            Self::Surface => SURFACE_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
            Self::Constant { ty, .. } => out("Out", Fixed(*ty)),
            Self::Color { .. } => vec![port("RGB", Fixed(Ty::Float3), 0.0), port("Alpha", Fixed(Ty::Float), 0.0)],
            Self::Property { .. } => out("Out", Fixed(property.unwrap_or(Ty::Float))),
            Self::SamplePicture { .. } => vec![
                port("RGBA", Fixed(Ty::Float4), 0.0),
                port("RGB", Fixed(Ty::Float3), 0.0),
                port("R", Fixed(Ty::Float), 0.0),
                port("G", Fixed(Ty::Float), 0.0),
                port("B", Fixed(Ty::Float), 0.0),
                port("A", Fixed(Ty::Float), 0.0),
            ],
            Self::Math { op } if op.gives_number() => out("Out", Fixed(Ty::Float)),
            Self::Math { op: MathOp::Cross } => out("Out", Fixed(Ty::Float3)),
            Self::Math { .. } | Self::Branch => out("Out", Any),
            Self::Compare { .. } => out("Out", Fixed(Ty::Bool)),
            Self::Split => ["X", "Y", "Z", "W"].iter().map(|&label| port(label, Fixed(Ty::Float), 0.0)).collect(),
            Self::Combine { ty } => out("Out", Fixed(*ty)),
            Self::Swizzle { components } => out("Out", Fixed(Ty::of_width(components.len()))),
            Self::Random => out("Out", Fixed(Ty::Float)),
        }
    }
}
