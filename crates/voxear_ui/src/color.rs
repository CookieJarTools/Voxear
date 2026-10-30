/// A colour as designers pick it: sRGB with straight alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `0xRRGGBB`, as written in design tools.
    pub const fn hex(rgb: u32) -> Self {
        Self::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// OKLCH, the space the editor's themes are made in: equal steps of
    /// lightness look equal, whatever the hue. `lightness` and `alpha` run
    /// from 0 to 1, `chroma` from 0 to about 0.37, and `hue` is in degrees.
    /// Colours outside sRGB are clipped.
    pub fn oklch(lightness: f32, chroma: f32, hue: f32, alpha: f32) -> Self {
        let (sin, cos) = hue.to_radians().sin_cos();
        let (l, a, b) = (lightness, chroma * cos, chroma * sin);
        let cube = |v: f32| v * v * v;
        let l_ = cube(l + 0.396_337_78 * a + 0.215_803_76 * b);
        let m_ = cube(l - 0.105_561_346 * a - 0.063_854_17 * b);
        let s_ = cube(l - 0.089_484_18 * a - 1.291_485_5 * b);
        let linear = [
            4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
            -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
            -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
        ];
        let [r, g, b] = linear.map(|c| (linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8);
        Self { r, g, b, a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8 }
    }

    /// Linear and premultiplied, which is how the GPU blends correctly.
    pub fn linear_premultiplied(self) -> [f32; 4] {
        let a = self.a as f32 / 255.0;
        let linear = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        [linear(self.r) * a, linear(self.g) * a, linear(self.b) * a, a]
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
