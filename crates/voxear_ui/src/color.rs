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
        Self::oklab(lightness, chroma * cos, chroma * sin, alpha)
    }

    /// OKLab, OKLCH as a point rather than an angle: `a` runs from green to
    /// red and `b` from blue to yellow, so two colours mix by adding them.
    pub fn oklab(lightness: f32, a: f32, b: f32, alpha: f32) -> Self {
        let l = lightness;
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

    /// The colour in OKLCH, as [`oklch`](Self::oklch) takes it: lightness,
    /// chroma, and hue in degrees. Alpha is left out.
    pub fn to_oklch(self) -> [f32; 3] {
        let [r, g, b] = [self.r, self.g, self.b].map(srgb_to_linear);
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let b = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        [lightness, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0)]
    }

    /// As a sheet writes it: `#rrggbb`, or `#rrggbbaa` when see-through.
    pub fn css(self) -> String {
        let Self { r, g, b, a } = self;
        if a == 0xff { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
    }

    /// Linear and premultiplied, which is how the GPU blends correctly.
    pub fn linear_premultiplied(self) -> [f32; 4] {
        let a = self.a as f32 / 255.0;
        let [r, g, b] = [self.r, self.g, self.b].map(srgb_to_linear);
        [r * a, g * a, b * a, a]
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}
