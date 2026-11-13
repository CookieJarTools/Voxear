//! The nodes a VoxShade shader is made of, and the ports each has.
//!
//! A port's type is fixed, or for most of the maths any width of numbers:
//! such a node works on the widest of what is wired into it, a single
//! number into a wider port standing for that number in every place, a
//! wider value into a narrower port giving up its last numbers.

use serde::{Deserialize, Serialize};

use voxear_assets::AssetGuid;

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
    /// A particle graph's, once a particle each step of the simulation: the
    /// force on it, or how fast it goes.
    Update,
    /// A particle graph's, once a particle where it is drawn: its colour,
    /// how opaque and how wide.
    Look,
}

impl Stage {
    /// A material shader's stages.
    pub const ALL: [Self; 3] = [Self::Surface, Self::Vertex, Self::Light];
    /// A particle graph's.
    pub const PARTICLES: [Self; 2] = [Self::Update, Self::Look];

    pub fn name(self) -> &'static str {
        match self {
            Self::Surface => "Surface",
            Self::Vertex => "Vertex",
            Self::Light => "Light",
            Self::Update => "Update",
            Self::Look => "Look",
        }
    }

    pub fn is_particles(self) -> bool {
        matches!(self, Self::Update | Self::Look)
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
    /// A particle's age in seconds, how much of its life it has lived, from
    /// 0 to 1, and a number of its own from 0 to 1, the same all its life.
    Age,
    Life,
    Seed,
    /// How fast and which way a particle goes, in metres a second, in the world.
    Velocity,
    /// How long the simulation's step is, in seconds.
    DeltaTime,
    /// A particle's colour and opacity, and how wide it is in metres, as its
    /// system's own settings make them.
    ParticleColor,
    ParticleSize,
}

impl Input {
    pub const ALL: [Self; 18] = [
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
        Self::Age,
        Self::Life,
        Self::Seed,
        Self::Velocity,
        Self::DeltaTime,
        Self::ParticleColor,
        Self::ParticleSize,
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
            Self::Age => "Age",
            Self::Life => "Life",
            Self::Seed => "Seed",
            Self::Velocity => "Velocity",
            Self::DeltaTime => "Delta Time",
            Self::ParticleColor => "Particle Color",
            Self::ParticleSize => "Particle Size",
        }
    }

    pub fn ty(self) -> Ty {
        match self {
            Self::Uv => Ty::Float2,
            Self::Time
            | Self::Thickness
            | Self::Age
            | Self::Life
            | Self::Seed
            | Self::DeltaTime
            | Self::ParticleSize => Ty::Float,
            Self::ParticleColor => Ty::Float4,
            _ => Ty::Float3,
        }
    }

    /// Whether `stage` is given it.
    pub fn in_stage(self, stage: Stage) -> bool {
        match self {
            Self::Position | Self::ObjectPosition | Self::Time => stage != Stage::Light,
            Self::Uv | Self::Normal | Self::LevelPosition => matches!(stage, Stage::Surface | Stage::Vertex),
            Self::View | Self::Thickness => stage == Stage::Surface,
            Self::LightDirection | Self::LightColor | Self::LightView => stage == Stage::Light,
            Self::Age | Self::Life | Self::Seed | Self::Velocity => stage.is_particles(),
            Self::DeltaTime => stage == Stage::Update,
            Self::ParticleColor | Self::ParticleSize => stage == Stage::Look,
        }
    }

    /// Whether it is the same all over a draw, rather than different from one place to the next.
    pub fn is_uniform(self) -> bool {
        matches!(self, Self::Time | Self::DeltaTime)
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
    /// A particle graph's step: a force on the particle, and how fast it
    /// goes. Left unwired, its system's own settings move it.
    UpdateOutput,
    /// How a particle graph's particle is drawn: its colour, opacity and
    /// width. Left unwired, its system's own settings say.
    LookOutput,
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
    /// Slang written by hand: a function of its inputs that sets its
    /// outputs, called where the graph needs them.
    Code {
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inputs: Vec<Pin>,
        outputs: Vec<Pin>,
        code: String,
    },
    /// A sub-graph, another VoxShade file, worked into this graph where it
    /// is used: its name, inputs and outputs as they were when it was last
    /// read, which the editor keeps up to date.
    SubGraph {
        graph: AssetGuid,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inputs: Vec<Pin>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        outputs: Vec<Pin>,
    },
    /// One of a sub-graph's inputs, by its number.
    SubGraphInput {
        input: u32,
    },
    /// What a sub-graph gives: an input for each of its outputs.
    SubGraphOutput,
}

/// A port of a Slang node, or an input or output of a sub-graph: its name,
/// what it takes or gives, and for an input the numbers it has with
/// nothing wired in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    /// Its number among a sub-graph's inputs or outputs, which an Input
    /// node reads it by; 0 on a Slang node's.
    #[serde(default, skip_serializing_if = "is_zero_id")]
    pub id: u32,
    pub name: String,
    pub ty: Ty,
    #[serde(default, skip_serializing_if = "is_zero_value")]
    pub default: [f32; 4],
}

fn is_zero_id(id: &u32) -> bool {
    *id == 0
}

fn is_zero_value(value: &[f32; 4]) -> bool {
    *value == [0.0; 4]
}

impl Pin {
    pub fn new(name: &str, ty: Ty) -> Self {
        Self { id: 0, name: name.into(), ty, default: [0.0; 4] }
    }

    /// As a node's port.
    pub fn port(&self) -> Port {
        Port { label: self.name.clone(), ty: PortTy::Fixed(self.ty), default: self.default }
    }
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
    pub label: String,
    pub ty: PortTy,
    pub default: [f32; 4],
}

fn port(label: &'static str, ty: PortTy, default: f32) -> Port {
    Port { label: label.into(), ty, default: [default; 4] }
}

/// What an Update Output's and a Look Output's inputs set, in order.
pub const UPDATE_OUTPUTS: [(&str, Ty); 2] = [("Force", Ty::Float3), ("Velocity", Ty::Float3)];
pub const LOOK_OUTPUTS: [(&str, Ty); 3] = [("Color", Ty::Float3), ("Alpha", Ty::Float), ("Size", Ty::Float)];

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
            Self::UpdateOutput => "Update Output".into(),
            Self::LookOutput => "Look Output".into(),
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
            Self::Code { name, .. } if name.is_empty() => "Slang".into(),
            Self::Code { name, .. } | Self::SubGraph { name, .. } => name.clone(),
            Self::SubGraphInput { .. } => "Input".into(),
            Self::SubGraphOutput => "Sub-graph Output".into(),
        }
    }

    /// The stage its output node stands for, for the three output nodes.
    pub fn output_stage(&self) -> Option<Stage> {
        match self {
            Self::SurfaceOutput => Some(Stage::Surface),
            Self::VertexOutput => Some(Stage::Vertex),
            Self::LightOutput => Some(Stage::Light),
            Self::UpdateOutput => Some(Stage::Update),
            Self::LookOutput => Some(Stage::Look),
            _ => None,
        }
    }

    /// Whether it can run in `stage`: an input only where it is given, the
    /// surface as it stands only where there is one.
    pub fn in_stage(&self, stage: Stage) -> bool {
        match self {
            Self::Input { input } => input.in_stage(stage),
            Self::Surface => matches!(stage, Stage::Surface | Stage::Light),
            Self::SamplePicture { .. } => matches!(stage, Stage::Surface | Stage::Vertex),
            _ => true,
        }
    }

    pub fn inputs(&self) -> Vec<Port> {
        use PortTy::{Any, Fixed};
        match self {
            Self::SurfaceOutput => SURFACE_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
            Self::VertexOutput => vec![port("Offset", Fixed(Ty::Float3), 0.0)],
            Self::LightOutput => vec![port("Light", Fixed(Ty::Float3), 0.0)],
            Self::UpdateOutput => UPDATE_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
            Self::LookOutput => LOOK_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
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
            Self::Code { inputs, .. } | Self::SubGraph { inputs, .. } => inputs.iter().map(Pin::port).collect(),
            _ => Vec::new(),
        }
    }

    /// Its outputs; `property` says what a Property node's is, or a
    /// sub-graph's Input node's. A Sub-graph Output's inputs and an Input's
    /// output are the graph's to say: see `Graph::inputs_of`.
    pub fn outputs(&self, property: Option<Ty>) -> Vec<Port> {
        use PortTy::{Any, Fixed};
        let out = |label, ty| vec![port(label, ty, 0.0)];
        match self {
            Self::SurfaceOutput
            | Self::VertexOutput
            | Self::LightOutput
            | Self::UpdateOutput
            | Self::LookOutput
            | Self::SubGraphOutput => Vec::new(),
            Self::Input { input } => out("Out", Fixed(input.ty())),
            Self::Surface => SURFACE_OUTPUTS.iter().map(|&(label, ty)| port(label, Fixed(ty), 0.0)).collect(),
            Self::Constant { ty, .. } => out("Out", Fixed(*ty)),
            Self::Color { .. } => vec![port("RGB", Fixed(Ty::Float3), 0.0), port("Alpha", Fixed(Ty::Float), 0.0)],
            Self::Property { .. } | Self::SubGraphInput { .. } => out("Out", Fixed(property.unwrap_or(Ty::Float))),
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
            Self::Code { outputs, .. } | Self::SubGraph { outputs, .. } => outputs.iter().map(Pin::port).collect(),
        }
    }
}
