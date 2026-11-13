//! The storms and whirlpools a world has by itself, worked out from its
//! seed and the storms' clock alone: the same on every machine, with
//! nothing kept or sent.
//!
//! The level is cut into squares [`CELL`] metres a side and time into
//! stretches of [`GENERATION`] seconds. Each square may bear one storm in
//! each stretch, by a number hashed from the seed, the square and the
//! stretch: when in the stretch it begins, where in the square, how long it
//! lives, how far it reaches, how hard it blows and which way it drifts,
//! all from that number. Where a storm is at any moment is where it began
//! and how long it has drifted, so looking for the storms near a place is
//! looking through the squares one could have come from.
//!
//! Everything here is whole-number hashing, sums and products: the same
//! bits on every machine.

use voxear_core::math::DVec2;

use super::{MadeWhirlpool, NATURAL, Storm, StormKind, StormRules};

/// How wide a square that bears storms is, in metres, and how long a
/// stretch of time, in seconds.
pub const CELL: f64 = 4096.0;
pub const GENERATION: f64 = 900.0;
/// The shortest and longest a natural storm lives, in seconds: never as
/// long as a stretch, so only this stretch's and the last's are alive.
const SHORTEST: f32 = 300.0;
const LONGEST: f32 = 720.0;
/// The slowest and fastest one drifts, in metres a second.
const SLOWEST: f32 = 3.0;
const FASTEST: f32 = 9.0;
/// How far the smallest reaches, in metres.
const SMALLEST: f32 = 250.0;
/// How wide the squares great standing storms are looked for in, how many
/// of them hold one, and how near the level's middle none stands.
const STANDING_CELL: f64 = 12288.0;
const STANDING_CHANCE: f32 = 0.22;
const STANDING_CLEAR: f64 = 2500.0;
/// How wide the squares great whirlpools are looked for in, how many hold
/// one where the sea is deep enough, and how deep that is, in metres.
const POOL_CELL: f64 = 3072.0;
const POOL_CHANCE: f32 = 0.4;
const POOL_DEEP: f32 = 14.0;

/// A number for a seed and four whole numbers, the same every time.
fn hash(seed: u32, a: i64, b: i64, c: i64, salt: u64) -> u64 {
    let mut x = u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15;
    for part in [a as u64, b as u64, c as u64, salt] {
        x = (x ^ part).wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
    }
    x
}

/// The `which`th number from 0 to 1 a hash gives.
fn part(of: u64, which: u64) -> f32 {
    let mut x = of ^ which.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x = (x ^ (x >> 32)).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    x ^= x >> 32;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// The way and back of a turn from 0 to 1, from sums and products alone:
/// a point on a circle, near enough, for a storm's heading.
fn heading(turn: f32) -> [f32; 2] {
    // A square's edge drawn in to the circle.
    let t = turn.rem_euclid(1.0) * 4.0;
    let (x, y) = match t {
        t if t < 1.0 => (1.0 - t, t),
        t if t < 2.0 => (1.0 - t, 2.0 - t),
        t if t < 3.0 => (t - 3.0, 2.0 - t),
        t => (t - 3.0, t - 4.0),
    };
    let length = (x * x + y * y).sqrt();
    [x / length, y / length]
}

/// Every natural storm alive at `clock` that reaches within `reach` metres
/// of `around`, in metres from the level's middle. `climate` gives how
/// warm, in °C, and how wet, from 0 to 1, a place is at the sea's height:
/// fewer storms begin where it is dry, and fog banks only where it is cool
/// and wet.
pub fn natural_storms(
    seed: u32,
    rules: &StormRules,
    clock: f64,
    around: DVec2,
    reach: f64,
    climate: &dyn Fn(DVec2) -> (f32, f32),
) -> Vec<Storm> {
    let mut found = Vec::new();
    let largest = rules.largest.max(SMALLEST);
    if rules.frequency > 0.0 {
        // As far as a storm can have come from, and still reach.
        let travel = f64::from(FASTEST * LONGEST) + f64::from(largest) + reach;
        let cells = |v: f64| ((v - travel) / CELL).floor() as i64..=((v + travel) / CELL).floor() as i64;
        let stretch = (clock / GENERATION).floor() as i64;
        // The wind the storms mostly go with, the level's own.
        let prevailing = part(hash(seed, 0, 0, 0, 7), 0);
        for generation in [stretch - 1, stretch] {
            for cz in cells(around.y) {
                for cx in cells(around.x) {
                    let h = hash(seed, cx, cz, generation, 1);
                    let born = (generation as f64 + f64::from(part(h, 1)) * 0.999) * GENERATION;
                    let lasts = SHORTEST + (LONGEST - SHORTEST) * part(h, 2);
                    let age = clock - born;
                    if age < 0.0 || age >= f64::from(lasts) {
                        continue;
                    }
                    let began = DVec2::new(
                        (cx as f64 + f64::from(part(h, 3))) * CELL,
                        (cz as f64 + f64::from(part(h, 4))) * CELL,
                    );
                    let (celsius, humidity) = climate(began);
                    if part(h, 0) >= rules.frequency.clamp(0.0, 1.0) * (0.35 + 1.1 * humidity.clamp(0.0, 1.0)) {
                        continue;
                    }
                    // Fog banks where it is cool and wet: slow, and never the hardest weather.
                    let fog = (2.0..14.0).contains(&celsius) && humidity > 0.6 && part(h, 9) < 0.45;
                    let speed = (SLOWEST + (FASTEST - SLOWEST) * part(h, 5)) * if fog { 0.25 } else { 1.0 };
                    let [wx, wz] = heading(prevailing + (part(h, 6) - 0.5) * 0.22);
                    let velocity = [wx * speed, wz * speed];
                    let size = part(h, 7);
                    let radius = SMALLEST + (largest - SMALLEST) * size * size;
                    let middle = began + DVec2::new(f64::from(velocity[0]), f64::from(velocity[1])) * age;
                    if middle.distance(around) >= reach + f64::from(radius) {
                        continue;
                    }
                    found.push(Storm {
                        id: NATURAL | (h as u32 & 0x3FFF_FFFF),
                        kind: if fog { StormKind::FogBank } else { StormKind::ByTemperature },
                        middle: middle.to_array(),
                        radius,
                        strength: if fog { 0.7 + 0.3 * part(h, 8) } else { 0.45 + 0.55 * part(h, 8) },
                        velocity,
                        eye: 0.0,
                        born,
                        grows: lasts * 0.25,
                        fades: lasts * 0.3,
                        lasts,
                        path: Vec::new(),
                    });
                }
            }
        }
    }
    if rules.standing {
        let far = reach + 1400.0;
        let cells = |v: f64| ((v - far) / STANDING_CELL).floor() as i64..=((v + far) / STANDING_CELL).floor() as i64;
        for cz in cells(around.y) {
            for cx in cells(around.x) {
                let h = hash(seed, cx, cz, 0, 2);
                if part(h, 0) >= STANDING_CHANCE {
                    continue;
                }
                let middle = DVec2::new(
                    (cx as f64 + 0.15 + 0.7 * f64::from(part(h, 1))) * STANDING_CELL,
                    (cz as f64 + 0.15 + 0.7 * f64::from(part(h, 2))) * STANDING_CELL,
                );
                let radius = 700.0 + 500.0 * part(h, 3);
                if middle.length() < STANDING_CLEAR || middle.distance(around) >= reach + f64::from(radius) {
                    continue;
                }
                found.push(Storm {
                    id: NATURAL | 0x4000_0000 | (h as u32 & 0x3FFF_FFFF),
                    middle: middle.to_array(),
                    radius,
                    strength: 1.0,
                    eye: 0.25 + 0.1 * part(h, 4),
                    born: -1.0e6,
                    grows: 0.0,
                    ..Storm::default()
                });
            }
        }
    }
    found
}

/// Every great whirlpool that stands within `reach` metres of `around`, in
/// metres from the level's middle: at seeded places where `depth`, the
/// sea's depth at a place in metres, is deep all round.
pub fn natural_whirlpools(seed: u32, around: DVec2, reach: f64, depth: &dyn Fn(DVec2) -> f32) -> Vec<MadeWhirlpool> {
    let mut found = Vec::new();
    let far = reach + 64.0;
    let cells = |v: f64| ((v - far) / POOL_CELL).floor() as i64..=((v + far) / POOL_CELL).floor() as i64;
    for cz in cells(around.y) {
        for cx in cells(around.x) {
            let h = hash(seed, cx, cz, 0, 3);
            if part(h, 0) >= POOL_CHANCE {
                continue;
            }
            let middle = DVec2::new(
                (cx as f64 + f64::from(part(h, 1))) * POOL_CELL,
                (cz as f64 + f64::from(part(h, 2))) * POOL_CELL,
            );
            let radius = 22.0 + 16.0 * part(h, 3);
            if middle.distance(around) >= reach + f64::from(radius) {
                continue;
            }
            let ring = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]].map(|[x, z]: [f64; 2]| {
                depth(middle + DVec2::new(x, z) * f64::from(radius))
            });
            let shallowest = ring.into_iter().fold(depth(middle), f32::min);
            if shallowest < POOL_DEEP {
                continue;
            }
            let drawn = (7.0 + 5.0 * part(h, 4)).min(shallowest - 2.0);
            found.push(MadeWhirlpool { id: NATURAL | (h as u32 & 0x3FFF_FFFF), middle: middle.to_array(), radius, depth: drawn });
        }
    }
    found
}

/// The whirlpool a strong storm raises under it, where the sea there is
/// deep: it drifts with its storm, and goes as the storm weakens.
pub(super) fn storm_whirlpool(storm: &Storm, clock: f64, depth: &dyn Fn(DVec2) -> f32) -> Option<MadeWhirlpool> {
    if matches!(storm.kind, StormKind::FogBank | StormKind::Sandstorm) {
        return None;
    }
    let strength = storm.strength_at(clock);
    let risen = super::smooth(0.7, 0.9, strength);
    if risen <= 0.0 {
        return None;
    }
    let h = hash(storm.id, 0, 0, 0, 4);
    let [x, z] = heading(part(h, 0));
    let out = f64::from(storm.radius * (0.15 + 0.35 * part(h, 1)));
    let middle = DVec2::from(storm.middle) + DVec2::new(f64::from(x), f64::from(z)) * out;
    let sea = depth(middle);
    if sea < 10.0 {
        return None;
    }
    let drawn = ((5.0 + 3.0 * part(h, 2)) * risen).min(sea - 2.0);
    Some(MadeWhirlpool { id: storm.id, middle: middle.to_array(), radius: 12.0 + 8.0 * part(h, 3), depth: drawn })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mild(_: DVec2) -> (f32, f32) {
        (14.0, 0.6)
    }

    fn rules() -> StormRules {
        StormRules { standing: false, ..StormRules::default() }
    }

    #[test]
    fn a_heading_is_a_way_of_length_one_all_the_way_round() {
        let mut ways = Vec::new();
        for step in 0..64 {
            let [x, z] = heading(step as f32 / 64.0);
            assert!(((x * x + z * z).sqrt() - 1.0).abs() < 1e-5);
            ways.push((x, z));
        }
        assert_eq!(heading(0.0), [1.0, 0.0]);
        assert!(ways.iter().any(|&(x, _)| x < -0.9) && ways.iter().any(|&(_, z)| z < -0.9));
    }

    #[test]
    fn the_same_seed_and_clock_give_the_same_storms_and_another_seed_others() {
        let around = DVec2::new(1500.0, -700.0);
        // Over two hours of looking, there is weather about a good part of the time, and not all of it.
        let mut with = 0;
        let mut ids = std::collections::HashSet::new();
        for minute in 0..120 {
            let clock = f64::from(minute) * 60.0;
            let storms = natural_storms(42, &rules(), clock, around, 3000.0, &mild);
            assert_eq!(storms, natural_storms(42, &rules(), clock, around, 3000.0, &mild), "the same again");
            with += usize::from(!storms.is_empty());
            for storm in &storms {
                assert!(storm.is_natural() && storm.strength_at(clock) >= 0.0);
                assert!(DVec2::from(storm.middle).distance(around) < 3000.0 + f64::from(storm.radius));
                assert!((250.0..=900.0).contains(&storm.radius) && (0.45..=1.0).contains(&storm.strength));
                ids.insert(storm.id);
            }
        }
        assert!((15..110).contains(&with), "storms about in {with} minutes of 120");
        assert!(ids.len() >= 3, "several storms came by: {}", ids.len());
        let other: Vec<u32> = (0..120)
            .flat_map(|minute| natural_storms(43, &rules(), f64::from(minute) * 60.0, around, 3000.0, &mild))
            .map(|storm| storm.id)
            .collect();
        assert!(other.iter().any(|id| !ids.contains(id)), "another seed's storms are others");
        // Never, where the rules say never.
        let never = StormRules { frequency: 0.0, ..rules() };
        assert!((0..120).all(|minute| natural_storms(42, &never, f64::from(minute) * 60.0, around, 3000.0, &mild).is_empty()));
    }

    #[test]
    fn a_natural_storm_drifts_on_its_way_and_is_the_same_storm_from_wherever_it_is_looked_for() {
        let around = DVec2::ZERO;
        let (clock, storm) = (0..400)
            .find_map(|step| {
                let clock = f64::from(step) * 30.0;
                let storms = natural_storms(7, &rules(), clock, around, 2000.0, &mild);
                storms.into_iter().find(|storm| storm.kind == StormKind::ByTemperature).map(|storm| (clock, storm))
            })
            .expect("a storm comes by");
        let later = clock + 60.0;
        if later - storm.born < f64::from(storm.lasts) {
            // Looked for from where it has got to, it is there, a minute's drift on.
            let moved = DVec2::from(storm.middle) + DVec2::new(f64::from(storm.velocity[0]), f64::from(storm.velocity[1])) * 60.0;
            let again = natural_storms(7, &rules(), later, moved, 100.0, &mild);
            let same = again.iter().find(|other| other.id == storm.id).expect("the same storm");
            assert!(DVec2::from(same.middle).distance(moved) < 0.01);
            assert_eq!((same.born, same.radius, same.strength), (storm.born, storm.radius, storm.strength));
        }
        let speed = (storm.velocity[0].powi(2) + storm.velocity[1].powi(2)).sqrt();
        assert!((SLOWEST..=FASTEST).contains(&speed), "{speed} m/s");
        // It comes on and dies away within its life, and is gone after.
        assert_eq!(storm.strength_at(storm.born + f64::from(storm.lasts) + 1.0), 0.0);
        assert!(storm.strength_at(storm.born + f64::from(storm.lasts) * 0.5) == storm.strength);
    }

    #[test]
    fn dry_places_bear_fewer_storms_and_cool_wet_ones_fog_banks() {
        let count = |climate: &dyn Fn(DVec2) -> (f32, f32)| {
            let storms = (0..200).flat_map(|step| natural_storms(5, &rules(), f64::from(step) * 120.0, DVec2::ZERO, 6000.0, climate));
            let mut ids = std::collections::HashMap::new();
            for storm in storms {
                ids.insert(storm.id, storm.kind);
            }
            let fogs = ids.values().filter(|&&kind| kind == StormKind::FogBank).count();
            (ids.len(), fogs)
        };
        let (wet, wet_fogs) = count(&|_| (14.0, 0.9));
        let (dry, _) = count(&|_| (30.0, 0.05));
        assert!(wet > dry * 2 && dry > 0, "{wet} over wet country, {dry} over dry");
        assert_eq!(wet_fogs, 0, "too warm for fog");
        let (cool, fogs) = count(&|_| (8.0, 0.8));
        assert!(fogs * 5 > cool && fogs < cool, "{fogs} of {cool} are fog banks");
    }

    #[test]
    fn great_storms_stand_for_ever_round_an_eye_away_from_the_levels_middle() {
        let standing = StormRules { frequency: 0.0, ..StormRules::default() };
        let found = natural_storms(3, &standing, 0.0, DVec2::ZERO, 60_000.0, &mild);
        assert!(found.len() > 5, "{} within 60 km", found.len());
        for storm in &found {
            assert!(storm.eye > 0.2 && storm.lasts.is_infinite() && storm.velocity == [0.0; 2]);
            assert!(DVec2::from(storm.middle).length() >= STANDING_CLEAR);
            assert_eq!((storm.strength_at(0.0), storm.strength_at(1.0e7)), (1.0, 1.0));
        }
        assert_eq!(found, natural_storms(3, &standing, 9999.0, DVec2::ZERO, 60_000.0, &mild), "whenever it is");
        assert!(natural_storms(3, &rules(), 0.0, DVec2::ZERO, 60_000.0, &|_| (14.0, 0.0)).iter().all(|storm| storm.eye == 0.0));
    }

    #[test]
    fn great_whirlpools_stand_only_where_the_sea_is_deep_and_a_strong_storm_raises_one() {
        let deep = |_: DVec2| 40.0f32;
        let found = natural_whirlpools(11, DVec2::ZERO, 20_000.0, &deep);
        assert!(found.len() > 10, "{}", found.len());
        assert_eq!(found, natural_whirlpools(11, DVec2::ZERO, 20_000.0, &deep));
        assert!(found.iter().all(|pool| pool.depth >= 7.0 && pool.depth <= 12.0 && pool.radius >= 22.0));
        assert!(natural_whirlpools(11, DVec2::ZERO, 20_000.0, &|_| 6.0).is_empty(), "none in the shallows");
        // West of x = 0 is land: none stands there, or with a side over it.
        let coast = |at: DVec2| if at.x < 0.0 { -5.0 } else { 40.0 };
        let east = natural_whirlpools(11, DVec2::ZERO, 20_000.0, &coast);
        assert!(!east.is_empty() && east.iter().all(|pool| pool.middle[0] - f64::from(pool.radius) >= 0.0));

        let storm = Storm { id: NATURAL | 9, grows: 0.0, middle: [500.0, 500.0], ..Storm::default() };
        let pool = storm_whirlpool(&storm, 10.0, &deep).expect("under a storm at its height");
        let from = DVec2::from(pool.middle).distance(DVec2::new(500.0, 500.0));
        assert!(from < f64::from(storm.radius) * 0.5 && pool.depth >= 5.0, "{from} m from its middle");
        assert!(storm_whirlpool(&Storm { strength: 0.6, ..storm.clone() }, 10.0, &deep).is_none(), "not under a weak one");
        assert!(storm_whirlpool(&storm, 10.0, &|_| 3.0).is_none(), "nor in shallow water");
        assert!(storm_whirlpool(&Storm { kind: StormKind::FogBank, ..storm }, 10.0, &deep).is_none());
    }
}
