//! The components heat is made of: the level's Climate, with its clock;
//! Heat Sources and Heat Zones, which warm or cool what is near or in them;
//! and Heat Bodies, which take the temperature of where they are and change
//! when they are held hot or cold enough for long enough.

use serde::{Deserialize, Serialize};
use voxear_ecs::Component;

/// The level's clock and its climate where no generator gives one. The
/// first in the level is used.
///
/// Time of day turns at a whole day every `day_length` minutes while the
/// game plays, and with `turns_sun` the level's first Directional Light
/// follows it. A world of blocks made by a generator takes its climate from
/// the generator; elsewhere it is `celsius` at nothing high, cooling with
/// height, and `humidity` wet.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Climate {
    /// The hour, from 0 to 24.
    pub time_of_day: f32,
    /// How many minutes a whole day takes; 0 stands the clock still.
    pub day_length: f32,
    /// Whether the clock turns the level's first Directional Light.
    pub turns_sun: bool,
    /// How warm it is at nothing high, in °C, where no generator says.
    pub celsius: f32,
    /// How much warmer the afternoon is, and colder the small hours, in °C.
    /// It swings half as much again where it is dry, and half as much where
    /// it is wet.
    pub day_swing: f32,
    /// How wet it is, from 0 to 1, where no generator says.
    pub humidity: f32,
}

impl Default for Climate {
    fn default() -> Self {
        Self { time_of_day: 12.0, day_length: 20.0, turns_sun: false, celsius: 15.0, day_swing: 8.0, humidity: 0.5 }
    }
}

impl Component for Climate {}

/// Something hot or cold, as a fire or a block of ice: at its middle it is
/// `celsius`, fading to where it is by `radius` metres away.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HeatSource {
    pub celsius: f32,
    pub radius: f32,
}

impl Default for HeatSource {
    fn default() -> Self {
        Self { celsius: 400.0, radius: 3.0 }
    }
}

impl Component for HeatSource {}

/// The shape of a Heat Zone, round its Obj and turned with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneShape {
    #[default]
    Box,
    Sphere,
}

impl ZoneShape {
    pub const ALL: [ZoneShape; 2] = [ZoneShape::Box, ZoneShape::Sphere];

    pub fn name(self) -> &'static str {
        match self {
            ZoneShape::Box => "Box",
            ZoneShape::Sphere => "Sphere",
        }
    }
}

/// What a Heat Zone does to the temperature in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneMode {
    /// It is the zone's temperature, whatever it would be.
    #[default]
    Set,
    /// The zone's temperature is added to what it would be.
    Add,
}

impl ZoneMode {
    pub const ALL: [ZoneMode; 2] = [ZoneMode::Set, ZoneMode::Add];

    pub fn name(self) -> &'static str {
        match self {
            ZoneMode::Set => "Set",
            ZoneMode::Add => "Add",
        }
    }
}

/// An area hotter or colder than round it, as a forge's room or a cold
/// store: a box `size` metres across, or a sphere `size[0]` across, that
/// sets or adds `celsius`, coming on over `edge` metres inside it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HeatZone {
    pub shape: ZoneShape,
    pub size: [f32; 3],
    pub mode: ZoneMode,
    pub celsius: f32,
    pub edge: f32,
}

impl Default for HeatZone {
    fn default() -> Self {
        Self { shape: ZoneShape::Box, size: [10.0, 4.0, 10.0], mode: ZoneMode::Set, celsius: 30.0, edge: 1.0 }
    }
}

impl Component for HeatZone {}

/// Whether a change waits for its Heat Body to be hot or cold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeWhen {
    /// At `celsius` or hotter.
    #[default]
    Above,
    /// At `celsius` or colder.
    Below,
}

impl ChangeWhen {
    pub const ALL: [ChangeWhen; 2] = [ChangeWhen::Above, ChangeWhen::Below];

    pub fn name(self) -> &'static str {
        match self {
            ChangeWhen::Above => "Above",
            ChangeWhen::Below => "Below",
        }
    }

    /// Whether a body at `temperature` meets it at `celsius`.
    pub fn holds(self, temperature: f32, celsius: f32) -> bool {
        match self {
            ChangeWhen::Above => temperature >= celsius,
            ChangeWhen::Below => temperature <= celsius,
        }
    }
}

/// What a Heat Body becomes once it has been hot or cold enough for long
/// enough: raw meat seared, or frozen. Its time counts while its Heat Body
/// is past `celsius` and holds still while it is not; once it has counted
/// `seconds`, scripts hear of it by `name` and the Obj becomes `becomes`,
/// a prefab, where it names one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HeatChange {
    pub name: String,
    pub when: ChangeWhen,
    pub celsius: f32,
    pub seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub becomes: Option<String>,
}

impl Default for HeatChange {
    fn default() -> Self {
        Self { name: "Cooked".into(), when: ChangeWhen::Above, celsius: 100.0, seconds: 10.0, becomes: None }
    }
}

/// How a character with a Heat Body feels the temperature it is at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feels {
    Freezing,
    Cold,
    #[default]
    Comfortable,
    Hot,
    Scorching,
}

impl Feels {
    pub const ALL: [Feels; 5] = [Feels::Freezing, Feels::Cold, Feels::Comfortable, Feels::Hot, Feels::Scorching];

    pub fn name(self) -> &'static str {
        match self {
            Feels::Freezing => "Freezing",
            Feels::Cold => "Cold",
            Feels::Comfortable => "Comfortable",
            Feels::Hot => "Hot",
            Feels::Scorching => "Scorching",
        }
    }
}

/// How many °C past cold or hot freezing and scorching start.
pub const EXTREME: f32 = 15.0;

/// Something that warms and cools with where it is: food, a character.
///
/// Its temperature moves towards the place's, about two thirds of the way
/// in `warms_in` seconds. A character feels it by `comfort`, the temperatures
/// it is comfortable between, widened by its resistances: `cold_resistance`
/// °C colder and `heat_resistance` °C hotter. Past those it is cold or hot,
/// and [`EXTREME`] °C further it is freezing or scorching. Each of its
/// `changes` counts while it is past its temperature, and happens once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HeatBody {
    pub warms_in: f32,
    pub comfort: [f32; 2],
    pub cold_resistance: f32,
    pub heat_resistance: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<HeatChange>,
}

impl Default for HeatBody {
    fn default() -> Self {
        Self { warms_in: 10.0, comfort: [10.0, 28.0], cold_resistance: 0.0, heat_resistance: 0.0, changes: Vec::new() }
    }
}

impl Component for HeatBody {}

impl HeatBody {
    /// The temperatures it is comfortable between, its resistances counted.
    pub fn band(&self) -> [f32; 2] {
        let [from, to] = [self.comfort[0].min(self.comfort[1]), self.comfort[0].max(self.comfort[1])];
        [from - self.cold_resistance.max(0.0), to + self.heat_resistance.max(0.0)]
    }

    /// How it feels at `temperature`.
    pub fn feels(&self, temperature: f32) -> Feels {
        let [cold, hot] = self.band();
        match temperature {
            t if t < cold - EXTREME => Feels::Freezing,
            t if t < cold => Feels::Cold,
            t if t > hot + EXTREME => Feels::Scorching,
            t if t > hot => Feels::Hot,
            _ => Feels::Comfortable,
        }
    }

    /// How many °C past its band `temperature` is: above 0 hotter than it,
    /// below colder, 0 within it.
    pub fn beyond(&self, temperature: f32) -> f32 {
        let [cold, hot] = self.band();
        if temperature > hot {
            temperature - hot
        } else if temperature < cold {
            temperature - cold
        } else {
            0.0
        }
    }
}

/// Where a Heat Body is in its warming and its changes while the game
/// plays: never saved.
#[derive(Clone, Debug, PartialEq)]
pub struct HeatState {
    pub temperature: f32,
    /// The place's temperature, as last looked at.
    pub place: f32,
    /// How long each change has counted, by its place in the list.
    pub held: Vec<f32>,
    /// Which changes have happened.
    pub done: Vec<bool>,
    pub feels: Feels,
    /// Seconds since the place was last looked at.
    pub since: f32,
    /// Whether it is being made into what a change becomes.
    pub going: bool,
}

impl Component for HeatState {}

impl HeatState {
    /// A body starting at `temperature`, as a place it is new to is.
    pub fn new(body: &HeatBody, temperature: f32) -> Self {
        let count = body.changes.len();
        Self {
            temperature,
            place: temperature,
            held: vec![0.0; count],
            done: vec![false; count],
            feels: body.feels(temperature),
            since: 0.0,
            going: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feels_follows_comfort_widened_by_resistance() {
        let body = HeatBody::default();
        assert_eq!(body.feels(20.0), Feels::Comfortable);
        assert_eq!(body.feels(5.0), Feels::Cold);
        assert_eq!(body.feels(-10.0), Feels::Freezing);
        assert_eq!(body.feels(35.0), Feels::Hot);
        assert_eq!(body.feels(50.0), Feels::Scorching);
        assert_eq!((body.beyond(5.0), body.beyond(20.0), body.beyond(35.0)), (-5.0, 0.0, 7.0));

        // A fur coat: comfortable where it was cold, and cold where it was freezing.
        let coat = HeatBody { cold_resistance: 20.0, ..HeatBody::default() };
        assert_eq!(
            (coat.feels(5.0), coat.feels(-15.0), coat.feels(-30.0)),
            (Feels::Comfortable, Feels::Cold, Feels::Freezing)
        );
        let cool = HeatBody { heat_resistance: 25.0, ..HeatBody::default() };
        assert_eq!((cool.feels(50.0), cool.feels(60.0)), (Feels::Comfortable, Feels::Hot));
        // Comfort written the wrong way round reads the same.
        assert_eq!(HeatBody { comfort: [28.0, 10.0], ..HeatBody::default() }.band(), [10.0, 28.0]);
    }

    #[test]
    fn a_heat_body_reads_back_as_written_with_its_changes() {
        let body = HeatBody {
            changes: vec![
                HeatChange { becomes: Some("Prefabs/Seared.prefab".into()), ..HeatChange::default() },
                HeatChange {
                    name: "Frozen".into(),
                    when: ChangeWhen::Below,
                    celsius: -5.0,
                    seconds: 30.0,
                    becomes: None,
                },
            ],
            ..HeatBody::default()
        };
        let text = toml::to_string(&body).unwrap();
        assert!(text.contains("when = \"below\""), "{text}");
        assert_eq!(toml::from_str::<HeatBody>(&text).unwrap(), body);
        assert!(!toml::to_string(&HeatBody::default()).unwrap().contains("changes"));
    }
}
