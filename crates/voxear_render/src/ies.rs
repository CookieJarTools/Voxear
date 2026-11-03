//! IES light profiles: photometric files in the IESNA LM-63 format (the
//! 1986, 1991, 1995 and 2002 editions), which lamp makers publish to say how
//! bright a fitting is in each direction.
//!
//! A profile is baked into a small texture over the sphere of directions,
//! longitude across and latitude down, so a point or spot light can scale
//! its light by it (see [`IesProfile::bake`]). Only type C photometry is
//! read, the kind nearly every architectural fitting is measured in.

use anyhow::{Context, Result, bail, ensure};

/// Texels across a baked profile, for the horizontal angles all round.
pub const IES_WIDTH: u32 = 128;
/// Texels down, for the vertical angles from straight down to straight up.
pub const IES_HEIGHT: u32 = 64;

/// A fitting's brightness in candela at each pair of angles it was
/// measured at. Vertical angles run from 0, straight down the light's axis,
/// to 180, straight back; horizontal ones round the axis, from 0 to 360.
#[derive(Clone, Debug, PartialEq)]
pub struct IesProfile {
    /// Vertical angles in degrees, rising.
    pub vertical: Vec<f32>,
    /// Horizontal angles in degrees, rising. One angle means the fitting is
    /// the same all round; ending at 90 or 180, that it is mirrored.
    pub horizontal: Vec<f32>,
    /// Candela at each horizontal angle's vertical angles in turn, with the
    /// file's multiplier applied.
    pub candela: Vec<f32>,
    /// The lamps' total output as the file gives it, in lumens; -1 when the
    /// file says the output is absolute.
    pub lumens: f32,
}

impl IesProfile {
    /// Reads an LM-63 file's text.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        // Keyword lines ([TEST], [MANUFAC]…) and the format line come before TILT, and say nothing about light.
        let tilt = loop {
            let line = lines.next().context("no TILT= line: this is not an IES file")?;
            if let Some(tilt) = line.trim().strip_prefix("TILT=") {
                break tilt.trim().to_ascii_uppercase();
            }
        };
        let rest: Vec<&str> = lines.collect();
        let mut numbers = rest
            .iter()
            .flat_map(|line| line.split(|c: char| c.is_whitespace() || c == ','))
            .filter(|token| !token.is_empty())
            .map(|token| token.parse::<f32>().with_context(|| format!("`{token}` is not a number")));
        let mut next =
            |what: &str| -> Result<f32> { numbers.next().with_context(|| format!("the file ends before {what}"))? };
        // A tilt the file includes changes the output as the fitting leans, which a light in a game never measures;
        // its numbers are skipped. One in another file cannot be read, and is left out too.
        if tilt == "INCLUDE" {
            next("the tilt's geometry")?;
            let count = next("the tilt's angle count")?;
            ensure!(count >= 0.0 && count < 10_000.0, "the tilt has {count} angles");
            for _ in 0..(count as usize) * 2 {
                next("the tilt's angles")?;
            }
        }

        let lamps = next("the number of lamps")?;
        let lumens_per_lamp = next("the lumens per lamp")?;
        let multiplier = next("the candela multiplier")?;
        let vertical_count = next("the number of vertical angles")?;
        let horizontal_count = next("the number of horizontal angles")?;
        let photometric_type = next("the photometric type")?;
        for what in ["the units", "the width", "the length", "the height", "the ballast factor", "the future use"] {
            next(what)?;
        }
        next("the input watts")?;
        ensure!(photometric_type == 1.0, "only type C photometry is supported, not type {photometric_type}");
        let count = |n: f32, what: &str| -> Result<usize> {
            ensure!(n >= 1.0 && n <= 10_000.0 && n.fract() == 0.0, "{n} is not a count of {what}");
            Ok(n as usize)
        };
        let vertical_count = count(vertical_count, "vertical angles")?;
        let horizontal_count = count(horizontal_count, "horizontal angles")?;

        let mut read = |n: usize, what: &str| (0..n).map(|_| next(what)).collect::<Result<Vec<f32>>>();
        let vertical = read(vertical_count, "the vertical angles")?;
        let horizontal = read(horizontal_count, "the horizontal angles")?;
        let candela = read(vertical_count * horizontal_count, "the candela values")?;
        let rising = |angles: &[f32]| angles.windows(2).all(|w| w[0] < w[1]);
        ensure!(rising(&vertical) && rising(&horizontal), "the angles do not rise");
        ensure!(vertical[0] >= 0.0 && vertical[vertical.len() - 1] <= 180.0, "vertical angles go outside 0 to 180");
        if horizontal[0] < 0.0 || horizontal[horizontal.len() - 1] > 360.0 {
            bail!("horizontal angles go outside 0 to 360");
        }
        let lumens = if lumens_per_lamp < 0.0 { -1.0 } else { lamps * lumens_per_lamp };
        Ok(Self {
            vertical,
            horizontal,
            candela: candela.into_iter().map(|c| (c * multiplier).max(0.0)).collect(),
            lumens,
        })
    }

    /// The brightest the fitting is in any direction, in candela.
    pub fn peak(&self) -> f32 {
        self.candela.iter().copied().fold(0.0, f32::max)
    }

    /// Candela at a vertical angle from straight down and a horizontal one
    /// round the axis, both in degrees, between the measured ones. Outside
    /// the vertical angles measured, the fitting gives no light.
    pub fn candela_at(&self, vertical: f32, horizontal: f32) -> f32 {
        let (first, last) = (self.vertical[0], self.vertical[self.vertical.len() - 1]);
        if vertical < first || vertical > last {
            return 0.0;
        }
        let horizontal = self.fold(horizontal);
        let (h0, h1, ht) = between(&self.horizontal, horizontal, self.wraps());
        let (v0, v1, vt) = between(&self.vertical, vertical, false);
        let at = |h: usize, v: usize| self.candela[h * self.vertical.len() + v];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        lerp(lerp(at(h0, v0), at(h0, v1), vt), lerp(at(h1, v0), at(h1, v1), vt), ht)
    }

    /// A horizontal angle brought into the range measured, by the
    /// symmetry the last angle says: none for 360, mirrored across the
    /// 0-180 plane for 180, and across both planes for 90.
    fn fold(&self, horizontal: f32) -> f32 {
        let mut angle = horizontal.rem_euclid(360.0);
        let last = self.horizontal[self.horizontal.len() - 1];
        if self.horizontal.len() == 1 {
            return self.horizontal[0];
        }
        if last <= 90.0 {
            angle = if angle > 180.0 { 360.0 - angle } else { angle };
            angle = if angle > 90.0 { 180.0 - angle } else { angle };
        } else if last <= 180.0 && angle > 180.0 {
            angle = 360.0 - angle;
        }
        angle
    }

    /// Whether the horizontal angles go all the way round, so past the
    /// last comes the first again.
    fn wraps(&self) -> bool {
        self.horizontal.len() > 1 && self.horizontal[self.horizontal.len() - 1] > 180.0
    }

    /// The profile over the sphere, `IES_WIDTH` horizontal angles across
    /// and `IES_HEIGHT` vertical ones down, at the texels' centres, as a
    /// share of its peak.
    pub fn bake(&self) -> Vec<f32> {
        let peak = self.peak().max(1e-6);
        let mut texels = Vec::with_capacity((IES_WIDTH * IES_HEIGHT) as usize);
        for y in 0..IES_HEIGHT {
            let vertical = (y as f32 + 0.5) / IES_HEIGHT as f32 * 180.0;
            for x in 0..IES_WIDTH {
                let horizontal = (x as f32 + 0.5) / IES_WIDTH as f32 * 360.0;
                texels.push(self.candela_at(vertical, horizontal) / peak);
            }
        }
        texels
    }
}

/// The two angles in `angles` around `angle`, and how far it is from the
/// first to the second. Past the ends it holds the end, or with `wrap`,
/// goes from the last back round to the first.
fn between(angles: &[f32], angle: f32, wrap: bool) -> (usize, usize, f32) {
    let last = angles.len() - 1;
    if angles.len() == 1 {
        return (0, 0, 0.0);
    }
    match angles.iter().position(|&a| a > angle) {
        Some(0) if wrap => {
            let span = angles[0] + 360.0 - angles[last];
            (last, 0, (angle + 360.0 - angles[last]) / span)
        }
        Some(0) => (0, 0, 0.0),
        Some(i) => (i - 1, i, (angle - angles[i - 1]) / (angles[i] - angles[i - 1])),
        None if wrap && angles[last] < 360.0 => {
            let span = angles[0] + 360.0 - angles[last];
            (last, 0, (angle - angles[last]) / span)
        }
        None => (last, last, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A downlight the same all round, in the 2002 format, brightest
    /// straight down and dark from 90 degrees.
    const DOWNLIGHT: &str = "IESNA:LM-63-2002
[TEST] Hand-written for Voxear's tests
[MANUFAC] Nobody
[LUMCAT] DL-1
[LUMINAIRE] Round downlight
TILT=NONE
1 1000 1.0 7 1 1 2 0.1 0.1 0.0
1.0 1.0 12
0 15 30 45 60 75 90
0
1000 950 800 500 200 50 0
";

    /// Mirrored across both planes, as a linear fitting is: brighter
    /// across its length (90) than along it (0). An older file with a tilt
    /// it includes, commas between numbers and values over several lines.
    const LINEAR: &str = "IESNA91
[TEST] Hand-written
TILT=INCLUDE
1
3
0, 45, 90
1.0, 0.9, 0.8
2, 500, 2.0, 3, 3, 1, 1, 1.2, 0.2, 0.1
1.0, 1.0, 40
0, 45, 90
0, 45, 90
100, 60, 0
150, 90,
0
200 120 0
";

    /// The 1986 format has no first line naming it.
    const OLD: &str = "Old fitting from 1986
Some description
TILT=NONE
1 -1 1 3 1 1 2 0 0 0
1 1 10
0 90 180
0
10 20 30
";

    #[test]
    fn a_downlight_reads_with_its_angles_and_values() {
        let profile = IesProfile::parse(DOWNLIGHT).unwrap();
        assert_eq!(profile.vertical, [0.0, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0]);
        assert_eq!(profile.horizontal, [0.0]);
        assert_eq!(profile.peak(), 1000.0);
        assert_eq!(profile.lumens, 1000.0);
        assert_eq!(profile.candela_at(0.0, 0.0), 1000.0);
        assert_eq!(profile.candela_at(37.5, 123.0), 650.0, "halfway between 30 and 45, at any horizontal angle");
        assert_eq!(profile.candela_at(120.0, 0.0), 0.0, "nothing above the angles measured");
    }

    #[test]
    fn a_linear_fitting_mirrors_round_its_quadrant_and_skips_its_tilt() {
        let profile = IesProfile::parse(LINEAR).unwrap();
        assert_eq!(profile.horizontal, [0.0, 45.0, 90.0]);
        assert_eq!(profile.peak(), 400.0, "the multiplier of 2 applies");
        assert_eq!(profile.lumens, 1000.0, "two lamps of 500");
        assert_eq!(profile.candela_at(0.0, 0.0), 200.0);
        assert_eq!(profile.candela_at(0.0, 90.0), 400.0);
        for mirrored in [270.0, 180.0 - 30.0, 180.0 + 30.0, 360.0 - 30.0] {
            let expected = profile.candela_at(0.0, if mirrored == 270.0 { 90.0 } else { 30.0 });
            assert_eq!(profile.candela_at(0.0, mirrored), expected, "{mirrored} mirrors into the quadrant");
        }
        assert_eq!(profile.candela_at(22.5, 0.0), 160.0);
    }

    #[test]
    fn a_1986_file_without_its_format_line_reads() {
        let profile = IesProfile::parse(OLD).unwrap();
        assert_eq!(profile.vertical, [0.0, 90.0, 180.0]);
        assert_eq!(profile.lumens, -1.0, "absolute photometry");
        assert_eq!(profile.candela_at(135.0, 10.0), 25.0, "it shines up as well as down");
    }

    #[test]
    fn baking_spreads_the_profile_over_the_sphere_as_a_share_of_its_peak() {
        let texels = IesProfile::parse(DOWNLIGHT).unwrap().bake();
        assert_eq!(texels.len(), (IES_WIDTH * IES_HEIGHT) as usize);
        let at = |x: u32, y: u32| texels[(y * IES_WIDTH + x) as usize];
        assert!(at(0, 0) > 0.99, "straight down is the peak");
        assert!(at(0, IES_HEIGHT / 4) > 0.4 && at(0, IES_HEIGHT / 4) < 0.6, "45 degrees is half");
        assert_eq!(at(0, IES_HEIGHT - 1), 0.0, "straight up is dark");
        assert!((0..IES_WIDTH).all(|x| at(x, 10) == at(0, 10)), "the same all round");
    }

    #[test]
    fn files_that_are_not_profiles_say_why() {
        let error = IesProfile::parse("hello\nworld").unwrap_err().to_string();
        assert!(error.contains("TILT"), "{error}");
        let short = IesProfile::parse("TILT=NONE\n1 1000 1 7 1 1 2 0 0 0\n1 1 12\n0 15 30").unwrap_err();
        assert!(short.to_string().contains("ends before"), "{short}");
        let type_b = DOWNLIGHT.replace("1 1000 1.0 7 1 1 2", "1 1000 1.0 7 1 2 2");
        assert!(IesProfile::parse(&type_b).unwrap_err().to_string().contains("type C"));
        let falling = DOWNLIGHT.replace("0 15 30 45", "0 30 15 45");
        assert!(IesProfile::parse(&falling).unwrap_err().to_string().contains("rise"));
    }
}
