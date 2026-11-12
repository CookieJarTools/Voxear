//! The Climate node: how warm and how wet a generator's world is at each
//! place (see [`Climate`]), cooler the higher the ground. With nothing
//! wired into its Height it is cooled by the ground it is asked about: a
//! terrain's own, after every layer, or a block world's.

use super::{Kind, Port, Value, ValueSpec};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V, World},
    graph::NodeId,
    landmass::{Climate, Spot},
};

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("temperature", "Temperature", -100.0, f32::MAX, "°C"),
    ValueSpec::number("humidity", "Humidity", 0.0, 1.0, ""),
    ValueSpec::number("size", "Climate size", 1.0, f32::MAX, "m").advanced(),
    ValueSpec::number("temperature_range", "Temperature range", 0.0, f32::MAX, "°C").advanced(),
    ValueSpec::number("humidity_range", "Humidity range", 0.0, f32::MAX, "").advanced(),
    ValueSpec::number("cooling", "Cooling", 0.0, f32::MAX, "°C").advanced(),
];

impl Climate {
    /// Its Height input, among its inputs.
    pub const HEIGHT_IN: usize = 0;
    /// Its Temperature and Humidity outputs, among its outputs.
    pub const TEMPERATURE_OUT: usize = 0;
    pub const HUMIDITY_OUT: usize = 1;
    /// Its Temperature and Humidity values, among its values.
    pub const TEMPERATURE: usize = 0;
    pub const HUMIDITY: usize = 1;
}

const INPUTS: &[Port] = &[Port::field("height", "Height")];

const OUTPUTS: &[Port] = &[Port::field("temperature", "Temperature"), Port::field("humidity", "Humidity")];

impl Kind for Climate {
    const TITLE: &'static str = "Climate";
    const FOLDER: &'static str = "Climate";

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
        let number = match at {
            0 => self.temperature,
            1 => self.humidity,
            2 => self.size,
            3 => self.temperature_range,
            4 => self.humidity_range,
            5 => self.cooling,
            _ => return None,
        };
        Some(Value::Number(number))
    }

    fn set(&mut self, at: usize, value: Value) {
        let Value::Number(number) = value else { return };
        match at {
            0 => self.temperature = number,
            1 => self.humidity = number,
            2 => self.size = number,
            3 => self.temperature_range = number,
            4 => self.humidity_range = number,
            5 => self.cooling = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let (climate, world) = (*self, cx.world);
        if port == Self::HUMIDITY_OUT {
            return cx.run(Wetness { climate, world }, &[at.x, at.z]);
        }
        let ground = cx.input(node, Self::HEIGHT_IN, at).unwrap_or_else(|| cx.ground(node));
        cx.run(Warmth { climate, world }, &[at.x, at.z, ground])
    }
}

fn key_of(key: &mut Key, name: &str, climate: &Climate, world: World) {
    key.name(name).whole(world.seed).number(world.scale).number(world.sea_level);
    for number in [
        climate.size,
        climate.temperature,
        climate.temperature_range,
        climate.humidity,
        climate.humidity_range,
        climate.cooling,
    ] {
        key.number(number);
    }
}

/// How warm it is, in °C: from rows of x, z and the ground's height.
#[derive(Debug)]
struct Warmth {
    climate: Climate,
    world: World,
}

impl Kernel for Warmth {
    fn key(&self, key: &mut Key) {
        key_of(key, "warmth", &self.climate, self.world);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let World { seed, scale, sea_level } = self.world;
        for (at, out) in out.iter_mut().enumerate() {
            let spot = Spot(inputs[0][at], inputs[1][at]);
            *out = self.climate.warmth(spot, inputs[2][at] - sea_level, seed, scale);
        }
    }
}

/// How wet it is, 0 to 1: from rows of x and z.
#[derive(Debug)]
struct Wetness {
    climate: Climate,
    world: World,
}

impl Kernel for Wetness {
    fn key(&self, key: &mut Key) {
        key_of(key, "wetness", &self.climate, self.world);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        let World { seed, scale, .. } = self.world;
        for (at, out) in out.iter_mut().enumerate() {
            *out = self.climate.wetness(Spot(inputs[0][at], inputs[1][at]), seed, scale);
        }
    }
}
