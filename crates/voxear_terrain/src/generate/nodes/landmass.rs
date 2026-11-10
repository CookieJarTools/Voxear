//! The Landmass node: continents, ranges and gullies in one node, made as
//! `generate::landmass` makes them, to the bit. Its values are a landmass's
//! shape; its seed, scale and sea level are its generator's.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
    landmass::{Climate, Erosion, Field, Landmass, Preset, Warp},
};

/// A landmass's shape (see [`Landmass`], whose fields these are).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Land {
    pub height: f32,
    pub continents: Field,
    pub ruggedness: Field,
    pub ridges: Field,
    pub sharpness: f32,
    pub warp: Warp,
    pub coast: Curve,
    pub inland: Curve,
    pub relief: Curve,
    pub peaks: Curve,
    pub crags: f32,
    pub erosion: Erosion,
    pub bumps: f32,
}

impl Default for Land {
    fn default() -> Self {
        Self::from(&Landmass::default())
    }
}

impl From<&Landmass> for Land {
    fn from(land: &Landmass) -> Self {
        Self {
            height: land.height,
            continents: land.continents,
            ruggedness: land.ruggedness,
            ridges: land.ridges,
            sharpness: land.sharpness,
            warp: land.warp,
            coast: land.coast,
            inland: land.inland,
            relief: land.relief,
            peaks: land.peaks,
            crags: land.crags,
            erosion: land.erosion,
            bumps: land.bumps,
        }
    }
}

const PRESETS: &[&str] = &["Battlefield Plains", "Rolling Hills", "Alpine", "Archipelago", "Canyons", "Custom"];

const VALUES: &[ValueSpec] = &[
    ValueSpec::choice("preset", "Preset", PRESETS),
    ValueSpec::number("height", "Height", 0.0, f32::MAX, "m"),
    ValueSpec::number("continents_size", "Continents size", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::whole("continents_octaves", "Continents octaves", 1, 12).advanced(),
    ValueSpec::number("ruggedness_size", "Ruggedness size", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::whole("ruggedness_octaves", "Ruggedness octaves", 1, 12).advanced(),
    ValueSpec::number("ridges_size", "Ridges size", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::whole("ridges_octaves", "Ridges octaves", 1, 12).advanced(),
    ValueSpec::number("sharpness", "Sharpness", 0.0, 1.0, "").advanced(),
    ValueSpec::number("warp_size", "Warp size", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::number("warp_strength", "Warp strength", 0.0, f32::MAX, "m").advanced(),
    ValueSpec::curve("coast", "Coast", 0.0).advanced(),
    ValueSpec::curve("inland", "Inland", 1.0).advanced(),
    ValueSpec::curve("relief", "Relief", 1.0).advanced(),
    ValueSpec::curve("peaks", "Peaks", 0.0).advanced(),
    ValueSpec::number("crags", "Crags", 0.0, f32::MAX, "").advanced(),
    ValueSpec::number("erosion", "Erosion", 0.0, f32::MAX, "").advanced(),
    ValueSpec::number("gully_spacing", "Gully spacing", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::whole("gully_octaves", "Gully octaves", 0, 8).advanced(),
    ValueSpec::number("bumps", "Bumps", 0.0, f32::MAX, "m").advanced(),
];

impl Land {
    /// Its Height output, among its outputs.
    pub const HEIGHT_OUT: usize = 0;
    /// Its Preset and Height values, among its values.
    pub const PRESET: usize = 0;
    pub const HEIGHT: usize = 1;

    /// The landmass of this shape in a world with this seed, scale, sea
    /// level and climate.
    pub fn landmass(&self, seed: u32, scale: f32, sea_level: f32, climate: Climate) -> Landmass {
        Landmass {
            seed,
            scale,
            sea_level,
            height: self.height,
            continents: self.continents,
            ruggedness: self.ruggedness,
            ridges: self.ridges,
            sharpness: self.sharpness,
            warp: self.warp,
            coast: self.coast,
            inland: self.inland,
            relief: self.relief,
            peaks: self.peaks,
            crags: self.crags,
            erosion: self.erosion,
            climate,
            bumps: self.bumps,
        }
    }

    /// The preset it is shaped as, whatever its height; none once its shape
    /// is its own.
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|preset| Land { height: self.height, ..Land::from(&preset.landmass()) } == *self)
    }
}

const OUTPUTS: &[Port] = &[Port::field("height", "Height")];

impl Kind for Land {
    const TITLE: &'static str = "Landmass";
    const FOLDER: &'static str = "Sources";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Choice(
                self.preset()
                    .and_then(|preset| Preset::ALL.iter().position(|&p| p == preset))
                    .unwrap_or(Preset::ALL.len()),
            ),
            1 => Value::Number(self.height),
            2 => Value::Number(self.continents.size),
            3 => Value::Whole(self.continents.octaves),
            4 => Value::Number(self.ruggedness.size),
            5 => Value::Whole(self.ruggedness.octaves),
            6 => Value::Number(self.ridges.size),
            7 => Value::Whole(self.ridges.octaves),
            8 => Value::Number(self.sharpness),
            9 => Value::Number(self.warp.size),
            10 => Value::Number(self.warp.strength),
            11 => Value::Curve(self.coast),
            12 => Value::Curve(self.inland),
            13 => Value::Curve(self.relief),
            14 => Value::Curve(self.peaks),
            15 => Value::Number(self.crags),
            16 => Value::Number(self.erosion.strength),
            17 => Value::Number(self.erosion.spacing),
            18 => Value::Whole(self.erosion.octaves),
            19 => Value::Number(self.bumps),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            // A preset's shape and height; Custom leaves it as it is.
            (0, Value::Choice(choice)) => {
                if let Some(preset) = Preset::ALL.get(choice) {
                    *self = Land::from(&preset.landmass());
                }
            }
            (1, Value::Number(v)) => self.height = v,
            (2, Value::Number(v)) => self.continents.size = v,
            (3, Value::Whole(v)) => self.continents.octaves = v,
            (4, Value::Number(v)) => self.ruggedness.size = v,
            (5, Value::Whole(v)) => self.ruggedness.octaves = v,
            (6, Value::Number(v)) => self.ridges.size = v,
            (7, Value::Whole(v)) => self.ridges.octaves = v,
            (8, Value::Number(v)) => self.sharpness = v,
            (9, Value::Number(v)) => self.warp.size = v,
            (10, Value::Number(v)) => self.warp.strength = v,
            (11, Value::Curve(v)) => self.coast = v,
            (12, Value::Curve(v)) => self.inland = v,
            (13, Value::Curve(v)) => self.relief = v,
            (14, Value::Curve(v)) => self.peaks = v,
            (15, Value::Number(v)) => self.crags = v,
            (16, Value::Number(v)) => self.erosion.strength = v,
            (17, Value::Number(v)) => self.erosion.spacing = v,
            (18, Value::Whole(v)) => self.erosion.octaves = v,
            (19, Value::Number(v)) => self.bumps = v,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, _: NodeId, _: usize, at: Places) -> V {
        let world = cx.world;
        let land = Arc::new(self.landmass(world.seed, world.scale, world.sea_level, Climate::default()));
        match at {
            Places::Flat { x, z } => cx.run(Ground(land), &[x, z]),
            Places::Round { x, y, z } => cx.run(RoundGround(land), &[x, y, z]),
        }
    }
}

/// A landmass's height on flat ground.
#[derive(Debug)]
struct Ground(Arc<Landmass>);

impl Kernel for Ground {
    fn key(&self, key: &mut Key) {
        key.name("landmass").text(&self.0.to_text());
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for ((out, &x), &z) in out.iter_mut().zip(inputs[0]).zip(inputs[1]) {
            *out = self.0.height_at(x, z);
        }
    }
}

/// A landmass's height round a planet.
#[derive(Debug)]
struct RoundGround(Arc<Landmass>);

impl Kernel for RoundGround {
    fn key(&self, key: &mut Key) {
        key.name("landmass round").text(&self.0.to_text());
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            *out = self.0.height_round([inputs[0][at], inputs[1][at], inputs[2][at]]);
        }
    }
}
