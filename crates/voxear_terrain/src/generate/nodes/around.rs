//! What nodes that read the ground round each place share: places a little
//! way off along the ground, and masks eased in at their edges.
//!
//! A slope, a curvature or an average is read from what feeds a node at a
//! few places round each one. On flat ground those lie along x and z. Round
//! a planet they lie along two tangents of its surface there, at right
//! angles to each other and to the way out, found with sums, products and a
//! square root alone. Which way the two tangents point turns over a ring
//! near each pole, along y; what is read at the same distance each way (a
//! slope's steepness, a curvature, an average) barely changes with it.

use super::STEEPEST;
use crate::{
    generate::compile::{Compiler, Kernel, Key, Places, V},
    layers::smoothstep,
};

/// `at` moved `a` metres one way along the ground and `b` the other: along
/// x and z on flat ground, along its two tangents round a planet.
pub(super) fn moved(cx: &mut Compiler, at: Places, a: f32, b: f32) -> Places {
    match at {
        Places::Flat { x, z } => Places::Flat { x: shifted(cx, x, a), z: shifted(cx, z, b) },
        Places::Round { x, y, z } => {
            let mut axis = |axis: usize| cx.run(Tangent { axis, a, b }, &[x, y, z]);
            Places::Round { x: axis(0), y: axis(1), z: axis(2) }
        }
    }
}

/// The four places `step` metres from `at`: further along the first way,
/// back along it, further along the second, back along it.
pub(super) fn four(cx: &mut Compiler, at: Places, step: f32) -> [Places; 4] {
    [moved(cx, at, step, 0.0), moved(cx, at, -step, 0.0), moved(cx, at, 0.0, step), moved(cx, at, 0.0, -step)]
}

fn shifted(cx: &mut Compiler, row: V, by: f32) -> V {
    if by == 0.0 { row } else { cx.run(Shift(by), &[row]) }
}

/// A row with the same number added at every place.
#[derive(Debug)]
struct Shift(f32);

impl Kernel for Shift {
    fn key(&self, key: &mut Key) {
        key.name("shift").number(self.0);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, &value) in out.iter_mut().zip(inputs[0]) {
            *out = value + self.0;
        }
    }
}

/// One axis of places round a planet, each moved `a` metres along one
/// tangent of its surface there and `b` along the other.
#[derive(Debug)]
struct Tangent {
    axis: usize,
    a: f32,
    b: f32,
}

impl Kernel for Tangent {
    fn key(&self, key: &mut Key) {
        key.name("tangent").whole(self.axis as u32).number(self.a).number(self.b);
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            let place = [inputs[0][at], inputs[1][at], inputs[2][at]];
            let (first, second) = tangents(place);
            *out = place[self.axis] + self.a * first[self.axis] + self.b * second[self.axis];
        }
    }
}

/// Two directions along a planet's surface at `place`, in metres from its
/// middle: each of length 1, at right angles to each other and to the way
/// out.
pub(super) fn tangents([x, y, z]: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let length = (x * x + y * y + z * z).sqrt();
    if length <= 1e-6 {
        return ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    }
    let out = [x / length, y / length, z / length];
    // The first lies across the way out and y, or across x near a pole, where the way out is nearly y.
    let across = if out[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let first = cross(across, out);
    let size = (first[0] * first[0] + first[1] * first[1] + first[2] * first[2]).sqrt();
    let first = [first[0] / size, first[1] / size, first[2] / size];
    (first, cross(out, first))
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// How much a mask from `low` to `high` covers `value`: all of it between
/// them, easing to nothing over `feather` outside each, as a terrain
/// layer's mask does.
pub(super) fn within(low: f32, high: f32, feather: f32, value: f32) -> f32 {
    smoothstep(low - feather, low, value) * (1.0 - smoothstep(high, high + feather, value))
}

/// The slope `by` less steep than `slope`, each as rise over run: the
/// tangent of the difference of their angles, and level at the least.
pub(super) fn gentler(slope: f32, by: f32) -> f32 {
    let turned = (slope - by) / (1.0 + slope * by);
    if turned > 0.0 { turned } else { 0.0 }
}

/// The slope `by` steeper than `slope`, each as rise over run: the tangent
/// of the sum of their angles, and straight up at the most.
pub(super) fn steeper(slope: f32, by: f32) -> f32 {
    let under = 1.0 - slope * by;
    if under <= 0.0 {
        return STEEPEST;
    }
    let turned = (slope + by) / under;
    if turned < STEEPEST { turned } else { STEEPEST }
}

/// How much a mask from slope `low` to slope `high`, eased in over
/// `feather` outside each (all as rise over run), covers a slope of `rise`.
/// A `high` of straight up has no upper edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct Slopes {
    /// Where it eases in from, and where it is whole from.
    low: [f32; 2],
    /// Where it is whole to, and where it has eased out by.
    high: [f32; 2],
}

impl Slopes {
    pub fn new(low: f32, high: f32, feather: f32) -> Self {
        Self { low: [gentler(low, feather), low], high: [high, steeper(high, feather)] }
    }

    pub fn key(&self, key: &mut Key) {
        key.number(self.low[0]).number(self.low[1]).number(self.high[0]).number(self.high[1]);
    }

    pub fn cover(&self, rise: f32) -> f32 {
        let from = smoothstep(self.low[0], self.low[1], rise);
        if self.high[0] >= STEEPEST {
            return from;
        }
        from * (1.0 - smoothstep(self.high[0], self.high[1], rise))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::nodes::{degrees_of, rise_of};

    #[test]
    fn a_planets_tangents_are_at_right_angles_to_each_other_and_to_the_way_out() {
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for i in 0..4000 {
            // All round the planet, its poles and the ring where the tangents turn among them.
            let (turn, tilt) = (i as f32 * 0.37, (i % 200) as f32 / 199.0 * std::f32::consts::PI);
            let place = [tilt.sin() * turn.cos() * 3000.0, tilt.cos() * 3000.0, tilt.sin() * turn.sin() * 3000.0];
            let (first, second) = tangents(place);
            let out = place.map(|v| v / 3000.0);
            assert!((dot(first, first) - 1.0).abs() < 1e-4 && (dot(second, second) - 1.0).abs() < 1e-4, "{place:?}");
            assert!(dot(first, second).abs() < 1e-4 && dot(first, out).abs() < 1e-4, "{place:?}");
            assert!(dot(second, out).abs() < 1e-4, "{place:?}");
        }
        assert_eq!(tangents([0.0, 0.0, 0.0]), ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), "something at its middle");
    }

    #[test]
    fn slopes_turn_by_an_angle_with_no_trigonometry() {
        for (slope, by) in [(30.0, 5.0), (60.0, 10.0), (10.0, 10.0), (80.0, 5.0), (0.0, 3.0), (45.0, 45.0)] {
            let (less, more) = (gentler(rise_of(slope), rise_of(by)), steeper(rise_of(slope), rise_of(by)));
            let level = if slope > by { slope - by } else { 0.0 };
            assert!((degrees_of(less) - level).abs() < 1e-2, "{slope} less {by}: {}", degrees_of(less));
            assert!((degrees_of(more) - (slope + by)).abs() < 1e-2, "{slope} and {by}: {}", degrees_of(more));
        }
        // Never past level or straight up, and straight up is no slope at all.
        assert_eq!(gentler(rise_of(5.0), rise_of(20.0)), 0.0);
        assert_eq!(steeper(rise_of(80.0), rise_of(20.0)), STEEPEST);
        assert_eq!((rise_of(90.0), degrees_of(STEEPEST), rise_of(0.0)), (STEEPEST, 90.0, 0.0));
        assert!((degrees_of(gentler(STEEPEST, rise_of(5.0))) - 85.0).abs() < 1e-2);
        // A mask from 30° to 50° with 5° of feather: nothing by 25° and 55°, all between 30° and 50°.
        let slopes = Slopes::new(rise_of(30.0), rise_of(50.0), rise_of(5.0));
        let cover = |degrees: f32| slopes.cover(rise_of(degrees));
        assert_eq!([cover(10.0), cover(24.9), cover(30.1), cover(49.9), cover(55.1), cover(89.0)], [0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
        assert!((0.3..0.7).contains(&cover(27.5)) && (0.3..0.7).contains(&cover(52.5)));
        // To straight up there is no upper edge, and from level no lower one.
        let steep = Slopes::new(rise_of(40.0), STEEPEST, rise_of(5.0));
        assert_eq!([steep.cover(rise_of(30.0)), steep.cover(rise_of(41.0)), steep.cover(1.0e9)], [0.0, 1.0, 1.0]);
        assert_eq!(Slopes::new(0.0, rise_of(10.0), rise_of(5.0)).cover(0.0), 1.0);
        assert_eq!(within(10.0, 20.0, 5.0, 4.0), 0.0);
        assert_eq!((within(10.0, 20.0, 5.0, 15.0), within(10.0, 20.0, 5.0, 26.0)), (1.0, 0.0));
        assert!((within(10.0, 20.0, 5.0, 7.5) - 0.5).abs() < 1e-6 && (within(10.0, 20.0, 5.0, 22.5) - 0.5).abs() < 1e-6);
    }
}
