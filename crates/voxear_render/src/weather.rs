//! The level's weather: how much cloud, gloom, fog, wind, rain and snow it
//! adds to the level's lighting, eased from one state to the next.
//!
//! A level's lighting is how its maker left it: its sky, sun, fog and Wind
//! Zone. [`Weather`], a resource, is laid over that as the frame is drawn,
//! so nothing of the lighting is changed and clear weather is the lighting
//! as it is. Each of [`WeatherAmounts`] runs from 0, nothing added, to 1:
//!
//! - **cloud** fills the sky's clouds towards overcast.
//! - **darkness** thickens them, darkens their shadows and dims the sun, and
//!   with it the sky.
//! - **fog** thickens the fog and the air's haze.
//! - **wind** strengthens the level's Wind Zone, for all it blows.
//! - **rain** and **snow** fall round the camera, which the engine sees to.
//! - **lightning** strikes now and then, lighting the sky and the level
//!   under it for a moment.
//!
//! What has fallen stays a while ([`WeatherGround`]): rain soaks every
//! surface in half a minute and dries off over a few once it stops, and
//! snow settles on what faces up and melts away, sooner in the rain.
//!
//! A [`WeatherKind`] is a set of amounts by name. Setting the weather with
//! a time eases every amount from what it is now, by time passed, so every
//! machine given the same times is in the same weather.

use serde::{Deserialize, Serialize};

use crate::{WindZone, post::Fog, scene::Sky};

/// Weather by name: each a set of [`WeatherAmounts`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherKind {
    /// The level's lighting as it is.
    #[default]
    Clear,
    /// Half the sky in cloud.
    Cloudy,
    /// A grey sky from edge to edge and a dull day under it.
    Overcast,
    /// Steady rain from a dark sky, with some wind and mist.
    Rain,
    /// Heavy rain, a gale and a sky near black.
    Storm,
    /// Snow falling from a pale, close sky.
    Snow,
    /// Thick fog, the sky lost in it.
    Fog,
    /// Amounts of its own, set one by one.
    Custom,
}

impl WeatherKind {
    pub const ALL: [Self; 8] =
        [Self::Clear, Self::Cloudy, Self::Overcast, Self::Rain, Self::Storm, Self::Snow, Self::Fog, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Rain => "Rain",
            Self::Storm => "Storm",
            Self::Snow => "Snow",
            Self::Fog => "Fog",
            Self::Custom => "Custom",
        }
    }

    /// Its place in [`Self::ALL`], as scripts name it.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&kind| kind == self).unwrap_or(0)
    }

    /// What it adds; nothing for Custom, whose amounts are whatever they
    /// are set to.
    pub fn amounts(self) -> WeatherAmounts {
        let none = WeatherAmounts::default();
        match self {
            Self::Clear | Self::Custom => none,
            Self::Cloudy => WeatherAmounts { cloud: 0.5, ..none },
            Self::Overcast => WeatherAmounts { cloud: 1.0, darkness: 0.3, fog: 0.05, ..none },
            Self::Rain => WeatherAmounts { cloud: 0.95, darkness: 0.4, fog: 0.2, wind: 0.3, rain: 0.6, ..none },
            Self::Storm => {
                WeatherAmounts { cloud: 1.0, darkness: 0.75, fog: 0.3, wind: 1.0, rain: 1.0, lightning: 0.8, ..none }
            }
            Self::Snow => WeatherAmounts { cloud: 0.9, darkness: 0.2, fog: 0.3, wind: 0.15, snow: 0.8, ..none },
            // The sky is covered too: fog is cloud on the ground, and blue overhead would give it away.
            Self::Fog => WeatherAmounts { cloud: 1.0, darkness: 0.1, fog: 1.0, ..none },
        }
    }
}

/// How much the weather adds of each thing it is made of, each from 0 for
/// none to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeatherAmounts {
    /// How far the sky's clouds are filled in, towards overcast.
    pub cloud: f32,
    /// How heavy and dark the clouds are, and how much of the sun they take.
    pub darkness: f32,
    /// How much fog and haze is added to the air.
    pub fog: f32,
    /// How much harder the level's Wind Zone blows.
    pub wind: f32,
    /// How hard it rains.
    pub rain: f32,
    /// How hard it snows.
    pub snow: f32,
    /// How often lightning strikes: at 1, every eight seconds or so.
    pub lightning: f32,
}

/// How many amounts weather is made of.
pub const AMOUNTS: usize = 7;

impl WeatherAmounts {
    /// Each amount as a list, in the order scripts and files give them.
    pub fn to_array(self) -> [f32; AMOUNTS] {
        [self.cloud, self.darkness, self.fog, self.wind, self.rain, self.snow, self.lightning]
    }

    /// The amounts a list gives, each kept between 0 and 1.
    pub fn from_array(amounts: [f32; AMOUNTS]) -> Self {
        let [cloud, darkness, fog, wind, rain, snow, lightning] = amounts.map(|amount| amount.clamp(0.0, 1.0));
        Self { cloud, darkness, fog, wind, rain, snow, lightning }
    }

    /// Part of the way from these to `other`.
    pub fn towards(self, other: Self, part: f32) -> Self {
        let (from, to) = (self.to_array(), other.to_array());
        Self::from_array(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * part))
    }

    /// Whether they add nothing.
    pub fn is_none(self) -> bool {
        self.to_array().iter().all(|&amount| amount <= 0.0)
    }

    /// A sky under this weather: its clouds filled in and made heavier, its
    /// air hazier, and its own colours greyed and dimmed where it is a
    /// gradient. The atmosphere dims with the sun (see [`Self::sun`]).
    pub fn sky(self, mut sky: Sky) -> Sky {
        if self.is_none() {
            return sky;
        }
        let clouds = &mut sky.clouds;
        clouds.cover += (1.0 - clouds.cover.clamp(0.0, 1.0)) * self.cloud;
        clouds.density *= 1.0 + self.darkness;
        clouds.thickness *= 1.0 + 0.5 * self.darkness;
        clouds.shadows += (1.0 - clouds.shadows.clamp(0.0, 1.0)) * self.darkness.max(self.cloud * 0.5);
        sky.haze *= 1.0 + 2.0 * self.fog;
        sky.distance_haze *= 1.0 + self.fog;
        let grey = self.darkness.max(self.cloud * 0.5) * 0.7;
        let dim = 1.0 - 0.6 * self.darkness;
        let weathered = |colour: voxear_core::math::Vec3| {
            let luminance = colour.dot(voxear_core::math::Vec3::new(0.2126, 0.7152, 0.0722));
            colour.lerp(voxear_core::math::Vec3::splat(luminance), grey) * dim
        };
        sky.zenith = weathered(sky.zenith);
        sky.horizon = weathered(sky.horizon);
        sky.stars *= 1.0 - self.cloud.max(self.fog);
        sky
    }

    /// How much of the sun's light is left: under the atmosphere the sky
    /// dims with it.
    pub fn sun(self) -> f32 {
        1.0 - 0.55 * self.darkness
    }

    /// The level's fog under this weather: on wherever the weather has any,
    /// and thicker by it.
    pub fn fog(self, fog: Fog) -> Fog {
        if self.fog <= 0.0 {
            return fog;
        }
        let own = if fog.enabled { fog.density } else { 0.0 };
        Fog { enabled: true, density: own + FOG_DENSITY * self.fog, ..fog }
    }

    /// The level's Wind Zone under this weather: blowing harder, in harder
    /// gusts.
    pub fn wind(self, zone: WindZone) -> WindZone {
        if self.wind <= 0.0 {
            return zone;
        }
        WindZone {
            strength: zone.strength * (1.0 + 2.0 * self.wind) + self.wind,
            gusts: zone.gusts * (1.0 + self.wind),
            ..zone
        }
    }
}

/// The density the thickest weather's fog adds: things fade out over a few
/// tens of metres.
const FOG_DENSITY: f32 = 0.03;

/// What the weather has left lying on the level: each from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherGround {
    /// How wet every surface is from the rain: darker, and shining.
    pub wet: f32,
    /// How much snow has settled on what faces up.
    pub snow: f32,
}

/// How many seconds rain takes to soak dry ground and wet ground to dry;
/// snow to settle as deep as it will and to melt, and to melt in heavy rain.
const SOAK: f32 = 25.0;
const DRY: f32 = 180.0;
const SETTLE: f32 = 90.0;
const MELT: f32 = 240.0;
const WASH: f32 = 40.0;

impl WeatherGround {
    /// The ground as weather adding `amounts` leaves it in the end.
    pub fn under(amounts: WeatherAmounts) -> Self {
        // Any rain soaks the ground through, given time; snow lies as deep as it falls hard.
        let wet = if amounts.rain > 0.0 { (0.5 + amounts.rain).min(1.0) } else { 0.0 };
        let snow = if amounts.rain > 0.0 { 0.0 } else { (amounts.snow * 1.25).min(1.0) };
        Self { wet, snow }
    }

    /// The ground `delta` seconds on, under weather adding `amounts`.
    fn after(self, amounts: WeatherAmounts, delta: f32) -> Self {
        let wants = Self::under(amounts);
        let towards = |now: f32, wanted: f32, up: f32, down: f32| match wanted > now {
            true => (now + delta / up).min(wanted),
            false => (now - delta / down).max(wanted),
        };
        // Snow melts sooner the harder it rains.
        let melt = MELT + (WASH - MELT) * amounts.rain;
        Self { wet: towards(self.wet, wants.wet, SOAK, DRY), snow: towards(self.snow, wants.snow, SETTLE, melt) }
    }
}

/// How bright lightning at its brightest makes the sky's light, against a
/// sun of 1, and its colour.
pub(crate) const FLASH_LIGHT: f32 = 1.5;
pub(crate) const FLASH_COLOUR: voxear_core::math::Vec3 = voxear_core::math::Vec3::new(0.75, 0.83, 1.0);

/// A number from 0 to 1 for a whole number, the same every time.
fn chance(of: u64) -> f32 {
    let mut x = of.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((x ^ (x >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

/// How long each stretch of time that may hold one strike of lightning is,
/// and how long a strike's flickers last, in seconds.
const STRIKE_EVERY: f64 = 3.0;
const STRIKE_LASTS: f32 = 0.5;

/// How bright lightning is, from 0 to 1, `time` seconds into weather with
/// this much `lightning`: nothing between strikes, and each strike two or
/// three flickers, the first the brightest. The same at the same time.
pub fn flash_at(time: f64, lightning: f32) -> f32 {
    if lightning <= 0.0 || time < 0.0 {
        return 0.0;
    }
    let stretch = (time / STRIKE_EVERY).floor();
    let key = stretch as u64;
    // More of the stretches hold a strike the more lightning there is: at 1, three in eight.
    if chance(key * 3) > 0.08 + 0.3 * lightning {
        return 0.0;
    }
    let starts = chance(key * 3 + 1) * (STRIKE_EVERY as f32 - STRIKE_LASTS);
    let since = (time - stretch * STRIKE_EVERY) as f32 - starts;
    if !(0.0..STRIKE_LASTS).contains(&since) {
        return 0.0;
    }
    let third = if chance(key * 3 + 2) > 0.5 { 0.4 } else { 0.0 };
    let flicker = |at: f32, bright: f32| if since >= at { bright * (-(since - at) * 30.0).exp() } else { 0.0 };
    (flicker(0.0, 1.0) + flicker(0.12, 0.6) + flicker(0.27, third)).min(1.0)
}

/// The level's weather, a resource: what it is now and what it is turning
/// to. Without one the weather is clear.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    kind: WeatherKind,
    from: WeatherAmounts,
    target: WeatherAmounts,
    now: WeatherAmounts,
    /// How long the change takes and how much of that has passed, in seconds.
    seconds: f32,
    passed: f32,
    ground: WeatherGround,
    /// How long the weather has run, in seconds, which times its lightning.
    clock: f64,
}

impl Weather {
    /// Weather of a kind, as it is once it has settled in: the ground as
    /// wet or as snowed over as that weather leaves it.
    pub fn of(kind: WeatherKind) -> Self {
        Self::settled_in(kind, kind.amounts())
    }

    /// Weather of amounts of its own, settled in.
    pub fn custom(amounts: WeatherAmounts) -> Self {
        Self::settled_in(WeatherKind::Custom, WeatherAmounts::from_array(amounts.to_array()))
    }

    fn settled_in(kind: WeatherKind, amounts: WeatherAmounts) -> Self {
        let ground = WeatherGround::under(amounts);
        Self { kind, from: amounts, target: amounts, now: amounts, seconds: 0.0, passed: 0.0, ground, clock: 0.0 }
    }

    /// Turns the weather to `kind` over `seconds`, from what it is now; at
    /// once with no time. Custom keeps the amounts it is turning to.
    pub fn set(&mut self, kind: WeatherKind, seconds: f32) {
        let target = if kind == WeatherKind::Custom { self.target } else { kind.amounts() };
        self.turn(kind, target, seconds);
    }

    /// Turns the weather to amounts of its own over `seconds`.
    pub fn set_amounts(&mut self, amounts: WeatherAmounts, seconds: f32) {
        self.turn(WeatherKind::Custom, WeatherAmounts::from_array(amounts.to_array()), seconds);
    }

    fn turn(&mut self, kind: WeatherKind, target: WeatherAmounts, seconds: f32) {
        *self = Self { kind, from: self.now, target, seconds: seconds.max(0.0), passed: 0.0, ..*self };
        self.advance(0.0);
    }

    /// Lets `delta` seconds pass: the weather turns, and the ground takes
    /// what falls on it or dries. Says whether anything changed.
    pub fn advance(&mut self, delta: f32) -> bool {
        let before = (self.now, self.ground);
        let delta = delta.max(0.0);
        self.passed = (self.passed + delta).min(self.seconds);
        let part = if self.seconds > 0.0 { self.passed / self.seconds } else { 1.0 };
        // Slow to start and to settle, as weather comes on.
        self.now = self.from.towards(self.target, part * part * (3.0 - 2.0 * part));
        self.ground = self.ground.after(self.now, delta);
        self.clock += f64::from(delta);
        (self.now, self.ground) != before
    }

    /// What it has left lying on the level.
    pub fn ground(&self) -> WeatherGround {
        self.ground
    }

    /// How bright its lightning is this moment, from 0 to 1.
    pub fn flash(&self) -> f32 {
        flash_at(self.clock, self.now.lightning)
    }

    /// The kind it is, or is turning to.
    pub fn kind(&self) -> WeatherKind {
        self.kind
    }

    /// What it adds this moment.
    pub fn now(&self) -> WeatherAmounts {
        self.now
    }

    /// What it adds once settled.
    pub fn target(&self) -> WeatherAmounts {
        self.target
    }

    /// Whether it has finished turning.
    pub fn settled(&self) -> bool {
        self.passed >= self.seconds
    }

    /// What the world's weather adds this moment: nothing without any.
    pub fn in_world(world: &voxear_ecs::World) -> WeatherAmounts {
        world.resource::<Self>().map(Self::now).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Clouds;

    #[test]
    fn weather_eases_from_what_it_is_to_what_it_is_set_to() {
        let mut weather = Weather::default();
        assert!(weather.now().is_none() && weather.settled(), "clear to begin with");
        weather.set(WeatherKind::Storm, 10.0);
        assert!(weather.now().is_none(), "nothing yet");
        assert!(weather.advance(5.0));
        let half = weather.now();
        assert!((half.rain - 0.5).abs() < 1e-4 && (half.cloud - 0.5).abs() < 1e-4, "half way: {half:?}");
        // Turned again half way, it eases from where it had got to.
        weather.set(WeatherKind::Clear, 4.0);
        assert_eq!(weather.now(), half);
        weather.advance(1.0);
        assert!(weather.now().rain < half.rain && weather.now().rain > 0.0);
        weather.advance(100.0);
        assert!(weather.now().is_none() && weather.settled());
        assert!(!weather.advance(1.0), "settled weather does not change");
        // With no time it is there at once, and amounts keep within 0 and 1.
        weather.set_amounts(WeatherAmounts { snow: 3.0, fog: -1.0, ..WeatherAmounts::default() }, 0.0);
        assert_eq!((weather.now().snow, weather.now().fog, weather.kind()), (1.0, 0.0, WeatherKind::Custom));
    }

    #[test]
    fn rain_soaks_the_ground_which_dries_after_and_snow_settles_and_melts() {
        let mut weather = Weather::default();
        weather.set(WeatherKind::Rain, 0.0);
        assert_eq!(weather.ground(), WeatherGround::default(), "dry as the rain starts");
        weather.advance(SOAK * 0.5);
        assert!((weather.ground().wet - 0.5).abs() < 0.01, "half soaked: {:?}", weather.ground());
        weather.advance(SOAK);
        assert_eq!(weather.ground().wet, 1.0);
        // The rain stops: still wet, and dry only minutes later.
        weather.set(WeatherKind::Clear, 0.0);
        weather.advance(SOAK);
        assert!(weather.ground().wet > 0.8);
        assert!(weather.advance(DRY) && weather.ground().wet == 0.0);
        assert!(!weather.advance(1.0), "nothing more to change");
        // Snow settles slowly, lies while it is cold, and the rain washes it off far sooner than it melts.
        weather.set(WeatherKind::Snow, 0.0);
        weather.advance(SETTLE * 0.5);
        let half = weather.ground().snow;
        assert!(half > 0.4 && half < 0.6 && weather.ground().wet == 0.0, "{:?}", weather.ground());
        weather.advance(SETTLE);
        assert_eq!(weather.ground().snow, 1.0);
        weather.set(WeatherKind::Storm, 0.0);
        weather.advance(WASH + 1.0);
        assert_eq!(weather.ground().snow, 0.0);
        // Weather the level starts in has been there a while.
        assert_eq!(Weather::of(WeatherKind::Rain).ground().wet, 1.0);
        assert_eq!(Weather::of(WeatherKind::Snow).ground().snow, 1.0);
    }

    #[test]
    fn lightning_strikes_now_and_then_in_a_storm_and_never_without() {
        let brightest = |lightning: f32| {
            let frames = (0..3600).map(|frame| flash_at(f64::from(frame) / 60.0, lightning));
            let lit: Vec<f32> = frames.collect();
            // A strike is a frame bright after a dark one.
            let strikes = lit.windows(2).filter(|pair| pair[0] == 0.0 && pair[1] > 0.3).count();
            (strikes, lit.iter().copied().fold(0.0, f32::max))
        };
        assert_eq!(brightest(0.0), (0, 0.0));
        let (strikes, peak) = brightest(1.0);
        assert!((3..=14).contains(&strikes), "a few a minute: {strikes}");
        assert!(peak > 0.5 && peak <= 1.0, "{peak}");
        assert!(brightest(0.3).0 < strikes, "fewer with less");
        // The same weather, the same lightning; and a storm has it where rain has none.
        let mut storm = Weather::of(WeatherKind::Storm);
        let mut rain = Weather::of(WeatherKind::Rain);
        let mut seen = 0.0f32;
        for _ in 0..3600 {
            storm.advance(1.0 / 60.0);
            rain.advance(1.0 / 60.0);
            seen = seen.max(storm.flash());
            assert_eq!(rain.flash(), 0.0);
        }
        assert!(seen > 0.5, "a storm's lightning: {seen}");
    }

    #[test]
    fn clear_weather_leaves_the_lighting_alone_and_a_storm_fills_and_darkens_it() {
        let sky = Sky { clouds: Clouds::covering(0.3), ..Sky::default() };
        let fog = Fog::default();
        let zone = WindZone::default();
        let clear = WeatherKind::Clear.amounts();
        let untouched = clear.sky(sky);
        assert_eq!((untouched.clouds, untouched.haze, untouched.horizon), (sky.clouds, sky.haze, sky.horizon));
        assert_eq!((clear.fog(fog), clear.wind(zone), clear.sun()), (fog, zone, 1.0));
        let storm = WeatherKind::Storm.amounts();
        let stormy = storm.sky(sky);
        assert_eq!(stormy.clouds.cover, 1.0);
        assert!(stormy.clouds.density > sky.clouds.density && stormy.clouds.shadows > sky.clouds.shadows);
        assert!(stormy.horizon.length() < sky.horizon.length(), "a gradient sky is dimmed");
        assert!(storm.sun() < 0.7);
        assert!(storm.fog(fog).enabled && storm.fog(fog).density > 0.0);
        assert!(storm.wind(zone).strength > zone.strength * 2.0);
        // Half cloud fills half of what was open.
        assert!((WeatherKind::Cloudy.amounts().sky(sky).clouds.cover - 0.65).abs() < 1e-5);
        for kind in WeatherKind::ALL {
            assert_eq!(WeatherKind::ALL[kind.index()], kind);
            assert!(kind.amounts().to_array().iter().all(|amount| (0.0..=1.0).contains(amount)), "{kind:?}");
        }
    }
}
