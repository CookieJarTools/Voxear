//! A terrain's layers: what its ground is made of, bottom to top.
//!
//! Each layer is a command rather than heights of its own: a picture
//! stretched over the terrain, a stamp, noise, a road laid along a line, a
//! sculpted layer the brushes paint into, ground cut into terraces or
//! smoothed. Each makes heights of its own, or a share of them, which its
//! blend puts on the ground below it, as strongly as its opacity and its
//! mask say. Moving, changing or switching off any layer makes the ground
//! again from the layers as they are now (see `composite`).

use serde::{Deserialize, Serialize};

use crate::noise::NoiseStyle;

/// A terrain's layers, on ground `base` metres high.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    pub base: f32,
    pub layers: Vec<Layer>,
}

/// One layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: LayerKind,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Mask::is_none")]
    pub mask: Mask,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn one() -> f32 {
    1.0
}

/// What a layer makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LayerKind {
    /// A grey picture stretched over the whole terrain, black at nothing and
    /// white `height` metres up: heights made in another tool.
    Import {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        picture: Option<String>,
        height: f32,
    },
    /// A grey picture, or a rounded hill without one, placed `position`
    /// metres from the terrain's corner, turned `rotation` degrees, `size`
    /// metres across and along and `height` metres high (below for less
    /// than nothing), its edges eased in over `falloff`, a share of its
    /// half-width.
    Stamp {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        picture: Option<String>,
        position: [f32; 2],
        #[serde(default)]
        rotation: f32,
        size: [f32; 2],
        height: f32,
        #[serde(default = "quarter")]
        falloff: f32,
    },
    /// Ground made by a world generator (see `generate`), the `.worldgen`
    /// file `world` names by its GUID, the terrain's corner `position`
    /// metres along x and z in the generator's world: none without one.
    Generate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        world: Option<String>,
        #[serde(default)]
        position: [f32; 2],
    },
    /// Fractal noise over the whole terrain, `scale` metres to its largest
    /// features and `height` metres from its lowest to its highest.
    Noise {
        #[serde(default)]
        style: NoiseStyle,
        scale: f32,
        height: f32,
        #[serde(default = "five")]
        octaves: u32,
        #[serde(default)]
        seed: u32,
    },
    /// Heights the sculpting brushes paint, kept in the terrain's `.layers`
    /// file as raster `raster`.
    Sculpt { raster: u32 },
    /// The ground laid along a line of points, metres from the terrain's
    /// corner with their heights, `width` metres across and eased into the
    /// ground beside it over `falloff` metres: a road, a riverbed, a trench.
    Spline {
        points: Vec<[f32; 3]>,
        width: f32,
        #[serde(default = "four")]
        falloff: f32,
        /// Dug along the line as a trench instead, its floor `width`
        /// metres wide, following the ground (see `dig`); it lays no heights.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dig: Option<crate::dig::Trench>,
    },
    /// The ground below cut into steps `step` metres high, their edges
    /// `sharpness` sharp, from 0 for gentle to 1 for cliffs.
    Terrace {
        step: f32,
        #[serde(default = "half")]
        sharpness: f32,
    },
    /// The ground below smoothed over `radius` metres.
    Smooth { radius: f32 },
    /// The ground below worn down by weather as `weather` says (see
    /// `erosion`): what that changed, kept as raster `raster` in the
    /// terrain's `.layers` file, worked out over the ground with hash
    /// `made_from` (0 before it is), which tells when that ground has changed
    /// since.
    Erosion {
        raster: u32,
        #[serde(default)]
        weather: crate::erosion::Weather,
        #[serde(default)]
        made_from: u64,
    },
    /// Rivers cut into the ground below as `settings` says, where water
    /// draining over it gathers (see `rivers`): what that changed, kept as
    /// raster `raster`, worked out over the ground with hash `made_from` (0
    /// before it is).
    Rivers {
        raster: u32,
        #[serde(default)]
        settings: crate::rivers::RiverSettings,
        #[serde(default)]
        made_from: u64,
    },
    /// Ground dug away or put back by the Sculpt tool's Dig and Fill
    /// brushes, a ball a stroke, in order (see `dig`). It lays no heights:
    /// it is dug into the ground every layer makes.
    Dig {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        strokes: Vec<crate::dig::DigStroke>,
    },
}

fn quarter() -> f32 {
    0.25
}

fn half() -> f32 {
    0.5
}

fn four() -> f32 {
    4.0
}

fn five() -> u32 {
    5
}

/// How a layer's heights meet the ground below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    /// Raised by the layer's heights.
    #[default]
    Add,
    /// Lowered by them.
    Subtract,
    /// The higher of the two.
    Max,
    /// The lower of the two.
    Min,
    /// The layer's heights in place of the ground's.
    Set,
    /// The ground's heights scaled by the layer's.
    Multiply,
}

impl Blend {
    pub const ALL: [Blend; 6] = [Blend::Add, Blend::Subtract, Blend::Max, Blend::Min, Blend::Set, Blend::Multiply];

    pub fn name(self) -> &'static str {
        match self {
            Blend::Add => "Add",
            Blend::Subtract => "Subtract",
            Blend::Max => "Max",
            Blend::Min => "Min",
            Blend::Set => "Set",
            Blend::Multiply => "Multiply",
        }
    }

    /// The ground `below` with `value` blended onto it.
    pub fn apply(self, below: f32, value: f32) -> f32 {
        match self {
            Blend::Add => below + value,
            Blend::Subtract => below - value,
            Blend::Max => below.max(value),
            Blend::Min => below.min(value),
            Blend::Set => value,
            Blend::Multiply => below * value,
        }
    }
}

/// Where on the ground below a layer counts: between heights, in metres,
/// and between slopes, in degrees, each eased in over `feather` (metres or
/// degrees). A range that is not there leaves the layer everywhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope: Option<[f32; 2]>,
    #[serde(default)]
    pub feather: f32,
}

impl Mask {
    pub fn is_none(&self) -> bool {
        self.height.is_none() && self.slope.is_none()
    }

    /// How much of the layer counts on ground `height` metres high whose
    /// slope's steepness is `rise` (metres up a metre along).
    pub fn weight(&self, height: f32, rise: f32) -> f32 {
        // In degrees, as the Inspector shows them; the rise is turned into one only where a slope is masked.
        self.weight_by(height, || rise.atan().to_degrees())
    }

    /// How much counts on ground `height` metres high whose slope is
    /// `degrees()` degrees, asked only where a slope is masked.
    pub fn weight_by(&self, height: f32, degrees: impl FnOnce() -> f32) -> f32 {
        let mut weight = 1.0;
        let feather = self.feather.max(1e-3);
        if let Some([low, high]) = self.height {
            weight *= smoothstep(low - feather, low, height) * (1.0 - smoothstep(high, high + feather, height));
        }
        if let Some([low, high]) = self.slope {
            let degrees = degrees();
            weight *= smoothstep(low - feather, low, degrees) * (1.0 - smoothstep(high, high + feather, degrees));
        }
        weight
    }
}

/// 0 below `edge0`, 1 above `edge1`, easing between; a step where they meet.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge0 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A rectangle of the terrain, in metres from its corner: `[min_x, min_z,
/// max_x, max_z]`.
pub type Area = [f32; 4];

impl LayerKind {
    /// What the Inspector calls it.
    pub fn title(&self) -> &'static str {
        match self {
            LayerKind::Import { .. } => "Import",
            LayerKind::Stamp { .. } => "Stamp",
            LayerKind::Generate { .. } => "Generate",
            LayerKind::Noise { .. } => "Noise",
            LayerKind::Sculpt { .. } => "Sculpt",
            LayerKind::Spline { .. } => "Spline",
            LayerKind::Terrace { .. } => "Terrace",
            LayerKind::Smooth { .. } => "Smooth",
            LayerKind::Erosion { .. } => "Erosion",
            LayerKind::Rivers { .. } => "Rivers",
            LayerKind::Dig { .. } => "Dig",
        }
    }

    /// Whether it digs into the ground rather than laying heights: a Dig
    /// layer, or a Spline dug as a trench.
    pub fn digs(&self) -> bool {
        matches!(self, LayerKind::Dig { .. } | LayerKind::Spline { dig: Some(_), .. })
    }

    /// Whether it makes ground on a terrain without end: what is worked out
    /// from where each sample is, not painted, read from a picture or
    /// worked out over the whole terrain at once, as a terrain of a set size
    /// is.
    pub fn endless(&self) -> bool {
        matches!(
            self,
            LayerKind::Generate { .. }
                | LayerKind::Noise { .. }
                | LayerKind::Stamp { .. }
                | LayerKind::Spline { dig: None, .. }
                | LayerKind::Terrace { .. }
                | LayerKind::Smooth { .. }
        )
    }

    /// The blend a new layer of its kind starts with.
    pub fn usual_blend(&self) -> Blend {
        match self {
            LayerKind::Generate { .. }
            | LayerKind::Spline { .. }
            | LayerKind::Terrace { .. }
            | LayerKind::Smooth { .. } => Blend::Set,
            _ => Blend::Add,
        }
    }

    /// The part of the terrain it changes, or `None` for all of it.
    pub fn area(&self) -> Option<Area> {
        match self {
            LayerKind::Stamp { position: [x, z], size: [w, l], .. } => {
                // A turned stamp stays within a circle round its corners.
                let reach = (w * w + l * l).sqrt() * 0.5;
                Some([x - reach, z - reach, x + reach, z + reach])
            }
            LayerKind::Spline { points, width, falloff, .. } => {
                let reach = width * 0.5 + falloff;
                let first = points.first()?;
                let mut area = [first[0], first[2], first[0], first[2]];
                for p in points {
                    area = [area[0].min(p[0]), area[1].min(p[2]), area[2].max(p[0]), area[3].max(p[2])];
                }
                Some([area[0] - reach, area[1] - reach, area[2] + reach, area[3] + reach])
            }
            _ => None,
        }
    }
}

impl Layer {
    /// A new layer of `kind`, named for it, as its kind usually blends.
    pub fn new(kind: LayerKind) -> Self {
        Self {
            name: kind.title().to_string(),
            enabled: true,
            blend: kind.usual_blend(),
            opacity: 1.0,
            mask: Mask::default(),
            kind,
        }
    }
}

/// The FNV-1a hash of some bytes: the same on every machine and in every
/// build, as what it keeps is kept in files.
pub fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// A hash of everything about these layers that shapes the ground.
pub fn hash_layers(base: f32, layers: &[Layer]) -> u64 {
    let mut text = format!("{base}");
    // Digging lays no heights, so it is left out.
    for layer in layers.iter().filter(|layer| layer.enabled && !layer.kind.digs()) {
        text.push('\n');
        text.push_str(&toml::to_string(layer).unwrap_or_default());
    }
    stable_hash(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_read_and_write_as_tables_named_by_their_kind() {
        let text = r#"
base = 2.0

[[layers]]
name = "Hills"
kind = "noise"
style = "ridged"
scale = 300.0
height = 40.0

[[layers]]
name = "Crater"
kind = "stamp"
position = [100.0, 50.0]
size = [30.0, 30.0]
height = -6.0
blend = "add"
opacity = 0.5
mask = { height = [0.0, 20.0], feather = 2.0 }
"#;
        let stack: Stack = toml::from_str(text).expect("a stack");
        assert_eq!(stack.layers.len(), 2);
        assert!(matches!(stack.layers[0].kind, LayerKind::Noise { style: NoiseStyle::Ridged, octaves: 5, .. }));
        assert_eq!(stack.layers[1].opacity, 0.5);
        assert_eq!(
            stack.layers[1].kind.area(),
            Some([100.0 - 21.213203, 50.0 - 21.213203, 100.0 + 21.213203, 50.0 + 21.213203])
        );
        let again: Stack = toml::from_str(&toml::to_string(&stack).unwrap()).unwrap();
        assert_eq!(again, stack);
        assert_eq!(hash_layers(again.base, &again.layers), hash_layers(stack.base, &stack.layers));
    }

    #[test]
    fn a_mask_eases_a_layer_in_over_its_feather() {
        let mask = Mask { height: Some([10.0, 20.0]), slope: None, feather: 2.0 };
        assert_eq!(mask.weight(15.0, 0.0), 1.0);
        assert_eq!(mask.weight(5.0, 0.0), 0.0);
        assert!((mask.weight(9.0, 0.0) - 0.5).abs() < 1e-6);
        let steep = Mask { slope: Some([30.0, 90.0]), height: None, feather: 0.0 };
        assert_eq!(steep.weight(0.0, 1.0), 1.0, "45 degrees");
        assert_eq!(steep.weight(0.0, 0.1), 0.0);
        // A switched-off layer leaves the hash as without it.
        let mut layers = vec![Layer::new(LayerKind::Smooth { radius: 2.0 })];
        let before = hash_layers(0.0, &[]);
        layers[0].enabled = false;
        assert_eq!(hash_layers(0.0, &layers), before);
    }
}
