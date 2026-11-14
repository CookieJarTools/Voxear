//! Pictures used on their own, by a material, a light's cookie or a
//! shader's property, cooked for a build as a model's pictures are: shrunk
//! and turned as their import settings say, with their mips made, and
//! block-compressed for the GPU, BC7 or BC5 for a normal map, unless the
//! settings say not to. So a build ships no picture file, and loading one
//! is an upload.
//!
//! A cooked picture is `VXPICTUR`, a format number, its format, width,
//! height and how many mips it has, then each mip's size and bytes. One
//! left uncompressed holds its RGBA8 pixels alone, and the GPU makes its
//! mips as it would for a picture file.
//!
//! An HDR picture (`.exr` or `.hdr`) is read as floats, light as it is,
//! never sRGB, and cooks to BC6H with mips made in linear light, or to half
//! floats where it is left uncompressed: for skies, glow maps and, later,
//! light probes.

use anyhow::{Context, Result, ensure};
use voxear_assets::TextureImport;

use crate::{gltf_import::TextureFormat, texture_codec};

const MAGIC: &[u8; 8] = b"VXPICTUR";
/// Raised whenever cooked pictures change, so the content cache cooks again.
pub const COOKED_PICTURE_VERSION: u32 = 1;

/// How the shipped content uses a picture, which says how it is cooked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PictureUse {
    /// Colours, read as sRGB: a colour or glow map, a cookie, or a use the
    /// build cannot tell.
    Colour,
    /// Values, read as they are: a metallic or height map.
    Values,
    /// A normal map and nothing else, which ships as BC5's two channels.
    Normal,
}

impl PictureUse {
    /// What a picture used both ways is cooked as: colour wins, since it
    /// shows most, and a normal map that is also read for values keeps
    /// all its channels.
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Colour, _) | (_, Self::Colour) => Self::Colour,
            (Self::Normal, Self::Normal) => Self::Normal,
            _ => Self::Values,
        }
    }

    /// A byte for it, for a cooked picture's cache key.
    pub fn key(self) -> u8 {
        match self {
            Self::Colour => 0,
            Self::Values => 1,
            Self::Normal => 2,
        }
    }
}

/// A picture cooked for a build.
#[derive(Clone, Debug, PartialEq)]
pub struct CookedPicture {
    width: u32,
    height: u32,
    format: TextureFormat,
    /// Level 0 first; one RGBA8 level when it is not compressed.
    mips: Vec<Vec<u8>>,
}

impl CookedPicture {
    /// Cooks a picture file (PNG or JPEG) as `settings` say, for `usage`.
    /// A colour lookup table, as many squares wide as it is high, keeps its
    /// pixels exactly, whatever the settings say, as anything else would
    /// change its colours.
    pub fn cook(file: &[u8], settings: &TextureImport, usage: PictureUse) -> Result<Self> {
        if let Some(read) = read_hdr(file) {
            return Self::cook_hdr(read?, settings);
        }
        let mut picture = image::load_from_memory(file).context("reading the picture")?.to_rgba8();
        let (width, height) = picture.dimensions();
        if height >= 2 && width == height * height {
            return Ok(Self { width, height, format: TextureFormat::Rgba8, mips: vec![picture.into_raw()] });
        }
        settings.apply_flip(&mut picture);
        let (width, height) = settings.size(width, height);
        if (width, height) != picture.dimensions() {
            picture = image::imageops::resize(&picture, width, height, image::imageops::FilterType::Triangle);
        }
        let rgba = picture.into_raw();
        if !settings.compress || width % 4 != 0 || height % 4 != 0 {
            return Ok(Self { width, height, format: TextureFormat::Rgba8, mips: vec![rgba] });
        }
        let levels = texture_codec::mip_chain(&rgba, width, height, usage == PictureUse::Colour);
        let (format, encode): (_, fn(&[u8], u32, u32) -> Vec<u8>) = match usage {
            PictureUse::Normal => (TextureFormat::Bc5, texture_codec::encode_bc5),
            PictureUse::Colour | PictureUse::Values => (TextureFormat::Bc7, texture_codec::encode_bc7),
        };
        let mips = levels.iter().map(|(w, h, pixels)| encode(pixels, *w, *h)).collect();
        Ok(Self { width, height, format, mips })
    }

    /// An HDR picture cooked: BC6H with its mips, or its half floats alone,
    /// whose mips the GPU makes, where the settings leave it uncompressed.
    fn cook_hdr(picture: image::Rgba32FImage, settings: &TextureImport) -> Result<Self> {
        let picture = sized(picture, settings);
        let (width, height) = picture.dimensions();
        let pixels: Vec<[f32; 4]> = picture.pixels().map(|pixel| pixel.0).collect();
        if !settings.compress || width % 4 != 0 || height % 4 != 0 {
            let mips = vec![texture_codec::to_f16_bytes(&pixels)];
            return Ok(Self { width, height, format: TextureFormat::Rgba16f, mips });
        }
        let levels = texture_codec::float_mip_chain(&pixels, width, height);
        let mips = levels.iter().map(|(w, h, pixels)| texture_codec::encode_bc6h(pixels, *w, *h)).collect();
        Ok(Self { width, height, format: TextureFormat::Bc6h, mips })
    }

    /// Its half floats, eight bytes a pixel, when it is an HDR picture left
    /// uncompressed.
    pub fn half_floats(&self) -> Option<&[u8]> {
        (self.format == TextureFormat::Rgba16f).then(|| self.mips[0].as_slice())
    }

    /// A small mip of it as linear floats, 64 pixels wide or less, for
    /// working out the light of a sky: sRGB colours made linear, HDR light as
    /// it is. None where it cannot be read so.
    pub fn small_pixels(&self) -> Option<(u32, u32, Vec<[f32; 4]>)> {
        let linear = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        match self.format {
            TextureFormat::Rgba8 => {
                let pixels = self.mips[0].chunks_exact(4).map(|p| [linear(p[0]), linear(p[1]), linear(p[2]), 1.0]);
                Some((self.width, self.height, pixels.collect()))
            }
            TextureFormat::Rgba16f => {
                let halves = self.mips[0].chunks_exact(2).map(|h| texture_codec::f16_value(u16::from_le_bytes([h[0], h[1]])));
                let values: Vec<f32> = halves.collect();
                Some((self.width, self.height, values.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect()))
            }
            TextureFormat::Bc6h => {
                // The first mip 64 wide or less, whose sides are still whole blocks.
                let level = (0..self.mips.len()).find(|&level| (self.width >> level) <= 64)?;
                let (w, h) = ((self.width >> level).max(1), (self.height >> level).max(1));
                if w % 4 != 0 || h % 4 != 0 {
                    return None;
                }
                let mut pixels = vec![[0.0; 4]; (w * h) as usize];
                for (index, block) in self.mips[level].chunks_exact(16).enumerate() {
                    let (bx, by) = (index as u32 % (w / 4), index as u32 / (w / 4));
                    let decoded = texture_codec::decode_bc6h(block.try_into().ok()?)?;
                    for (i, [r, g, b]) in decoded.into_iter().enumerate() {
                        let (x, y) = (bx * 4 + i as u32 % 4, by * 4 + i as u32 / 4);
                        pixels[(y * w + x) as usize] = [r, g, b, 1.0];
                    }
                }
                Some((w, h, pixels))
            }
            TextureFormat::Bc7 | TextureFormat::Bc5 => None,
        }
    }

    /// Whether it holds light rather than colours: an HDR picture.
    pub fn is_hdr(&self) -> bool {
        matches!(self.format, TextureFormat::Rgba16f | TextureFormat::Bc6h)
    }

    /// Whether `bytes` are a cooked picture rather than a picture file.
    pub fn is_cooked(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size: usize = self.mips.iter().map(|mip| mip.len() + 4).sum();
        let mut bytes = Vec::with_capacity(28 + size);
        bytes.extend_from_slice(MAGIC);
        for word in [COOKED_PICTURE_VERSION, self.format as u32, self.width, self.height, self.mips.len() as u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for mip in &self.mips {
            bytes.extend_from_slice(&(mip.len() as u32).to_le_bytes());
            bytes.extend_from_slice(mip);
        }
        bytes
    }

    /// A cooked picture from [`to_bytes`](Self::to_bytes)'s bytes; `what`
    /// names it for messages.
    pub fn from_bytes(bytes: &[u8], what: &str) -> Result<Self> {
        ensure!(Self::is_cooked(bytes), "{what} is not a cooked picture");
        let rest = &mut &bytes[MAGIC.len()..];
        let version = word(rest, what)?;
        ensure!(version == COOKED_PICTURE_VERSION, "{what} was cooked as version {version}; build the content again");
        let format = TextureFormat::from_u32(word(rest, what)?).with_context(|| format!("{what} has an unknown format"))?;
        let (width, height, count) = (word(rest, what)?, word(rest, what)?, word(rest, what)?);
        let mut mips = Vec::with_capacity(count.min(32) as usize);
        for _ in 0..count {
            let size = word(rest, what)? as usize;
            mips.push(take(rest, size, what)?.to_vec());
        }
        ensure!(!mips.is_empty() && width > 0 && height > 0, "{what} holds no picture");
        Ok(Self { width, height, format, mips })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Its RGBA8 pixels, four bytes each, when it was not compressed.
    pub fn rgba(&self) -> Option<&[u8]> {
        (self.format == TextureFormat::Rgba8).then(|| self.mips[0].as_slice())
    }

    pub(crate) fn format(&self) -> TextureFormat {
        self.format
    }

    pub(crate) fn mips(&self) -> &[Vec<u8>] {
        &self.mips
    }
}

/// An HDR picture file's pixels as floats, light as it is: `None` when the
/// file is not an OpenEXR or Radiance picture.
pub fn read_hdr(file: &[u8]) -> Option<Result<image::Rgba32FImage>> {
    let format = image::guess_format(file).ok()?;
    if !matches!(format, image::ImageFormat::OpenExr | image::ImageFormat::Hdr) {
        return None;
    }
    let read = image::load_from_memory_with_format(file, format).context("reading the HDR picture");
    Some(read.map(|picture| picture.to_rgba32f()))
}

/// An HDR picture at the size its import settings allow.
pub fn sized(picture: image::Rgba32FImage, settings: &TextureImport) -> image::Rgba32FImage {
    let (width, height) = settings.size(picture.width(), picture.height());
    match (width, height) == picture.dimensions() {
        true => picture,
        false => image::imageops::resize(&picture, width, height, image::imageops::FilterType::Triangle),
    }
}

/// The next `size` bytes of `rest`, which then starts after them.
fn take<'a>(rest: &mut &'a [u8], size: usize, what: &str) -> Result<&'a [u8]> {
    ensure!(rest.len() >= size, "{what} is cut short");
    let (found, after) = rest.split_at(size);
    *rest = after;
    Ok(found)
}

/// The next four bytes of `rest`, as a number.
fn word(rest: &mut &[u8], what: &str) -> Result<u32> {
    Ok(u32::from_le_bytes(take(rest, 4, what)?.try_into().expect("four bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG file of `width` × `height` pixels, each from `pixel`.
    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let picture = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
        let mut file = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut file, image::ImageFormat::Png).unwrap();
        file.into_inner()
    }

    fn gradient(x: u32, y: u32) -> [u8; 4] {
        [(x * 4) as u8, (y * 4) as u8, 128, 255]
    }

    #[test]
    fn a_picture_cooks_compressed_with_its_mips_and_reads_back() {
        let file = png(64, 32, gradient);
        let colour = CookedPicture::cook(&file, &TextureImport::default(), PictureUse::Colour).unwrap();
        assert_eq!(colour.format(), TextureFormat::Bc7);
        assert_eq!(colour.size(), (64, 32));
        assert_eq!(colour.mips().len(), 7, "64 down to 1");
        assert_eq!(colour.mips()[0].len(), 64 * 32, "BC7 is a byte a pixel");
        assert!(colour.rgba().is_none());
        assert_eq!(CookedPicture::from_bytes(&colour.to_bytes(), "it").unwrap(), colour);
        assert!(!colour.to_bytes().windows(4).any(|w| w == b"\x89PNG"), "no picture file inside");

        let normal = CookedPicture::cook(&file, &TextureImport::default(), PictureUse::Normal).unwrap();
        assert_eq!(normal.format(), TextureFormat::Bc5);
        let values = CookedPicture::cook(&file, &TextureImport::default(), PictureUse::Values).unwrap();
        assert_eq!(values.format(), TextureFormat::Bc7);
    }

    #[test]
    fn its_import_settings_shrink_turn_or_leave_it_uncompressed() {
        let file = png(64, 32, gradient);
        let sharp = TextureImport { compress: false, ..TextureImport::default() };
        let kept = CookedPicture::cook(&file, &sharp, PictureUse::Colour).unwrap();
        assert_eq!(kept.rgba().unwrap()[..4], gradient(0, 0), "uncompressed, its pixels as they were");
        assert_eq!(kept.mips().len(), 1, "the GPU makes its mips");

        let small = TextureImport { max_size: Some(16), ..sharp };
        assert_eq!(CookedPicture::cook(&file, &small, PictureUse::Colour).unwrap().size(), (16, 8));

        let flipped = TextureImport { flip_green: true, ..sharp };
        let turned = CookedPicture::cook(&file, &flipped, PictureUse::Normal).unwrap();
        assert_eq!(turned.rgba().unwrap()[4 * 64 * 2 + 1], 255 - 8, "green turned over");

        let odd = CookedPicture::cook(&png(30, 30, gradient), &TextureImport::default(), PictureUse::Colour).unwrap();
        assert!(odd.rgba().is_some(), "a side that is not a multiple of 4 stays RGBA8");
    }

    #[test]
    fn a_lookup_table_keeps_its_pixels_exactly() {
        let file = png(256, 16, |x, y| [(x % 16 * 17) as u8, (y * 17) as u8, (x / 16 * 17) as u8, 255]);
        let shrink = TextureImport { max_size: Some(64), flip_green: true, ..TextureImport::default() };
        let table = CookedPicture::cook(&file, &shrink, PictureUse::Colour).unwrap();
        assert_eq!(table.size(), (256, 16));
        let source = image::load_from_memory(&file).unwrap().to_rgba8().into_raw();
        assert_eq!(table.rgba().unwrap(), source.as_slice());
    }

    /// A Radiance HDR file of `width` × `height` pixels, each from `pixel`.
    fn radiance(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [f32; 3]) -> Vec<u8> {
        let picture = image::Rgb32FImage::from_fn(width, height, |x, y| image::Rgb(pixel(x, y)));
        let mut file = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb32F(picture).write_to(&mut file, image::ImageFormat::Hdr).unwrap();
        file.into_inner()
    }

    /// An OpenEXR file of `width` × `height` pixels, each from `pixel`.
    fn exr(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [f32; 4]) -> Vec<u8> {
        let picture = image::Rgba32FImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
        let mut file = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba32F(picture).write_to(&mut file, image::ImageFormat::OpenExr).unwrap();
        file.into_inner()
    }

    #[test]
    fn hdr_pictures_cook_to_bc6h_keeping_light_past_white() {
        let sky = |x: u32, y: u32| [0.5 + x as f32, 30.0 + y as f32, 2.0];
        for file in [radiance(32, 16, sky), exr(32, 16, |x, y| { let [r, g, b] = sky(x, y); [r, g, b, 1.0] })] {
            assert!(read_hdr(&file).is_some(), "read as HDR");
            let cooked = CookedPicture::cook(&file, &TextureImport::default(), PictureUse::Colour).unwrap();
            assert_eq!(cooked.format(), TextureFormat::Bc6h);
            assert!(cooked.is_hdr() && cooked.rgba().is_none() && cooked.half_floats().is_none());
            assert_eq!(cooked.mips().len(), 6, "32 down to 1");
            assert_eq!(cooked.mips()[0].len(), 32 * 16, "BC6H is a byte a pixel");
            assert_eq!(CookedPicture::from_bytes(&cooked.to_bytes(), "it").unwrap(), cooked);

            let kept = TextureImport { compress: false, ..TextureImport::default() };
            let halves = CookedPicture::cook(&file, &kept, PictureUse::Colour).unwrap();
            let bytes = halves.half_floats().unwrap();
            assert_eq!(bytes.len(), 32 * 16 * 8);
            let green = texture_codec::f16_value(u16::from_le_bytes([bytes[2], bytes[3]]));
            assert!((green - 30.0).abs() < 0.3, "light past white kept: {green}");
        }
        assert!(read_hdr(&png(4, 4, gradient)).is_none(), "a PNG is not HDR");
    }

    #[test]
    fn uses_combine_toward_what_keeps_most() {
        assert_eq!(PictureUse::Normal.and(PictureUse::Normal), PictureUse::Normal);
        assert_eq!(PictureUse::Normal.and(PictureUse::Values), PictureUse::Values);
        assert_eq!(PictureUse::Values.and(PictureUse::Colour), PictureUse::Colour);
    }
}
