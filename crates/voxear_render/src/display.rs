//! HDR display output: a window's swapchain in HDR10 or scRGB where the
//! display and its driver offer either, and SDR where they offer neither.
//!
//! With HDR on, the picture is not drawn into the window's images: the
//! views and the UI draw into a display image of half floats, in linear
//! BT.709 colours where 1 is paper white, and an encode pass fits that to
//! the window, as HDR10's PQ curve and BT.2020 colours, scRGB's 80 nits, or
//! SDR's sRGB for a window beside it that has no HDR. The tonemapper's
//! picture keeps its look up to paper white; above it, highlights roll off
//! towards the display's peak rather than to white (`highlight_gain`).
//!
//! The functions here are the shaders' maths (`post.slang`) on the CPU, so
//! they can be tested.

use voxear_rhi::vk;
pub use voxear_rhi::{Encoding, HdrChoice, Offered};

/// What the picture and the UI are drawn into while any output is HDR:
/// half floats, linear BT.709, 1 at paper white.
pub const DISPLAY_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// What a game asks of the display, as its options set it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplaySettings {
    pub hdr: HdrChoice,
    /// How bright white paper is, in nits: the brightness the SDR picture's
    /// white, and the UI, are shown at on an HDR display.
    pub paper_white: f32,
    /// The brightest the display shows, in nits.
    pub peak: f32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self { hdr: HdrChoice::Off, paper_white: 200.0, peak: 1000.0 }
    }
}

impl DisplaySettings {
    /// Paper white and the peak held to what displays show: paper white
    /// from 40 to 1000 nits, the peak from it to 10,000.
    pub fn held(self) -> Self {
        let paper_white = self.paper_white.clamp(40.0, 1000.0);
        Self { paper_white, peak: self.peak.clamp(paper_white, 10_000.0), ..self }
    }

    /// How many times brighter than paper white the display shows at most,
    /// for an output shown as `encoding`: 1 for SDR.
    pub fn peak_ratio(self, encoding: Encoding) -> f32 {
        let held = self.held();
        match encoding {
            Encoding::Sdr => 1.0,
            Encoding::Hdr10 | Encoding::ScRgb => held.peak / held.paper_white,
        }
    }
}

/// What an output shows now, and what its display offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayState {
    pub encoding: Encoding,
    pub offered: Offered,
}

/// How much brighter than the tonemapper's picture a pixel is shown on a
/// display `ratio` times as bright as paper white, from how bright it was
/// before tonemapping (exposed, 1 at paper white): not at all up to paper
/// white, so the picture keeps its look, then more and more, towards
/// `ratio` for the brightest, so highlights reach the display's peak
/// rather than stopping at white.
pub fn highlight_gain(luminance: f32, ratio: f32) -> f32 {
    let over = (luminance - 1.0).max(0.0);
    1.0 + (ratio - 1.0).max(0.0) * over / (over + ratio.max(1.0))
}

/// Linear BT.709 colours as BT.2020's, which HDR10 is in.
pub fn bt709_to_bt2020(c: [f32; 3]) -> [f32; 3] {
    [
        0.627_403_9 * c[0] + 0.329_283_04 * c[1] + 0.043_313_06 * c[2],
        0.069_097_29 * c[0] + 0.919_540_4 * c[1] + 0.011_362_32 * c[2],
        0.016_391_44 * c[0] + 0.088_013_31 * c[1] + 0.895_595_2 * c[2],
    ]
}

/// ST 2084's PQ curve: light from 0 to 1 for 0 to 10,000 nits, as a signal
/// from 0 to 1.
pub fn pq(light: f32) -> f32 {
    const M1: f32 = 2610.0 / 16384.0;
    const M2: f32 = 2523.0 / 4096.0 * 128.0;
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 4096.0 * 32.0;
    const C3: f32 = 2392.0 / 4096.0 * 32.0;
    let y = light.clamp(0.0, 1.0).powf(M1);
    ((C1 + C2 * y) / (1.0 + C3 * y)).powf(M2)
}

/// PQ's signal back to light, 1 for 10,000 nits.
pub fn pq_light(signal: f32) -> f32 {
    const M1: f32 = 2610.0 / 16384.0;
    const M2: f32 = 2523.0 / 4096.0 * 128.0;
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 4096.0 * 32.0;
    const C3: f32 = 2392.0 / 4096.0 * 32.0;
    let e = signal.clamp(0.0, 1.0).powf(1.0 / M2);
    ((e - C1).max(0.0) / (C2 - C3 * e)).powf(1.0 / M1)
}

/// A pixel of the display image (linear BT.709, 1 at paper white) as the
/// window's pixels hold it.
pub fn encode(color: [f32; 3], encoding: Encoding, settings: DisplaySettings) -> [f32; 3] {
    let held = settings.held();
    let peak = held.peak / held.paper_white;
    match encoding {
        Encoding::Sdr => color.map(|c| c.clamp(0.0, 1.0)),
        Encoding::ScRgb => color.map(|c| c.min(peak) * held.paper_white / 80.0),
        Encoding::Hdr10 => bt709_to_bt2020(color).map(|c| pq(c.clamp(0.0, peak) * held.paper_white / 10_000.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_roll_off_to_the_peak() {
        assert_eq!(highlight_gain(0.5, 5.0), 1.0, "below paper white the picture is as it was");
        assert_eq!(highlight_gain(1.0, 5.0), 1.0);
        assert_eq!(highlight_gain(100.0, 1.0), 1.0, "SDR is never brightened");
        let mut last = 1.0;
        for step in 1..200 {
            let gain = highlight_gain(1.0 + step as f32 * 0.5, 5.0);
            assert!(gain > last && gain < 5.0, "brighter, never past the peak: {gain}");
            last = gain;
        }
        assert!(highlight_gain(1e6, 5.0) > 4.99, "the brightest reach the peak");
    }

    #[test]
    fn pq_matches_its_table() {
        // ST 2084's own points: 100 nits is about 0.508, 1000 about 0.752, 10,000 is 1.
        assert!((pq(100.0 / 10_000.0) - 0.508).abs() < 1e-3, "{}", pq(0.01));
        assert!((pq(0.1) - 0.752).abs() < 1e-3, "{}", pq(0.1));
        assert!((pq(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(pq(0.0), (3424.0f32 / 4096.0).powf(2523.0 / 4096.0 * 128.0));
        for nits in [0.5, 10.0, 200.0, 1000.0, 4000.0] {
            let light = nits / 10_000.0;
            assert!((pq_light(pq(light)) - light).abs() < light * 1e-3, "back again at {nits} nits");
        }
    }

    #[test]
    fn colours_keep_their_white_and_brightness() {
        let white = bt709_to_bt2020([1.0; 3]);
        assert!(white.iter().all(|c| (c - 1.0).abs() < 1e-4), "white stays white: {white:?}");
        let red = bt709_to_bt2020([1.0, 0.0, 0.0]);
        assert!(red[0] < 1.0 && red[1] > 0.0, "sRGB's red lies inside BT.2020's");
    }

    #[test]
    fn each_output_shows_paper_white_at_its_brightness() {
        let settings = DisplaySettings { hdr: HdrChoice::On, paper_white: 200.0, peak: 1000.0 };
        assert_eq!(encode([1.0; 3], Encoding::ScRgb, settings), [2.5; 3], "200 nits is 2.5 in scRGB");
        assert_eq!(encode([9.0; 3], Encoding::ScRgb, settings), [12.5; 3], "held to the peak's 1000 nits");
        let hdr10 = encode([1.0; 3], Encoding::Hdr10, settings);
        assert!(hdr10.iter().all(|c| (c - pq(0.02)).abs() < 1e-4), "{hdr10:?}");
        let brightest = encode([50.0; 3], Encoding::Hdr10, settings);
        assert!(brightest.iter().all(|c| (c - pq(0.1)).abs() < 1e-4), "the peak's 1000 nits: {brightest:?}");
        assert_eq!(encode([1.5, 0.5, -0.1], Encoding::Sdr, settings), [1.0, 0.5, 0.0]);
        assert_eq!(settings.peak_ratio(Encoding::Hdr10), 5.0);
        assert_eq!(settings.peak_ratio(Encoding::Sdr), 1.0);
        let odd = DisplaySettings { paper_white: 5000.0, peak: 10.0, ..settings }.held();
        assert_eq!((odd.paper_white, odd.peak), (1000.0, 1000.0), "a peak below paper white is paper white");
    }
}
