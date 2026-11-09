//! Heights from a picture or a raw file made elsewhere: grey from black,
//! the lowest, to white, the highest.

use anyhow::{Context, Result, ensure};

use crate::{file::TerrainFile, heights::Heightmap};

/// Greys, 0 to 1, `width` by `height`, row after row from the top.
pub struct Greys {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

impl Greys {
    /// A picture's greys, as finely as it has them: PNG at 8 or 16 bits, or
    /// JPEG.
    pub fn from_picture(bytes: &[u8]) -> Result<Self> {
        let picture = image::load_from_memory(bytes).context("reading the picture")?;
        let grey = picture.to_luma16();
        let values = grey.as_raw().iter().map(|&v| f32::from(v) / 65535.0).collect();
        Ok(Self { width: grey.width(), height: grey.height(), values })
    }

    /// A raw file of 16-bit greys, low byte first, square: as terrain tools
    /// export their heights.
    pub fn from_raw16(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len().is_multiple_of(2), "a raw 16-bit file has an even number of bytes");
        let count = bytes.len() / 2;
        let side = (count as f64).sqrt().round() as u32;
        ensure!(side >= 2 && (side * side) as usize == count, "a raw file of {count} samples is not square");
        let values =
            bytes.as_chunks::<2>().0.iter().map(|&pair| f32::from(u16::from_le_bytes(pair)) / 65535.0).collect();
        Ok(Self { width: side, height: side, values })
    }

    /// The grey at `(u, v)`, 0 to 1 across from the top left, blended from
    /// the four pixels round it.
    pub fn at(&self, u: f32, v: f32) -> f32 {
        let x = (u * (self.width - 1) as f32).clamp(0.0, (self.width - 1) as f32);
        let y = (v * (self.height - 1) as f32).clamp(0.0, (self.height - 1) as f32);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let get = |x: u32, y: u32| self.values[(y * self.width + x) as usize];
        let top = get(x0, y0) * (1.0 - tx) + get(x1, y0) * tx;
        let bottom = get(x0, y1) * (1.0 - tx) + get(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// Heights as large and as fine as `file` says, white `height` metres
    /// up. The picture's top is the terrain's far edge (largest z), as it
    /// shows looked down on from above with +z ahead.
    pub fn heights(&self, file: &TerrainFile, height: f32) -> Heightmap {
        let size = file.size;
        Heightmap::from_fn(file.tiles(), file.tiles(), file.spacing(), |x, z| {
            self.at(x / size, 1.0 - z / size) * height
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sixteen_bit_picture_keeps_its_fine_steps() {
        let mut picture = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(4, 4);
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = image::Luma([(x * 16384 + y) as u16]);
        }
        let mut png = Vec::new();
        picture.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).expect("a PNG");
        let greys = Greys::from_picture(&png).expect("greys");
        assert_eq!((greys.width, greys.height), (4, 4));
        // One step of 65,535 between rows: lost at 8 bits.
        assert!(greys.values[4] > greys.values[0]);
    }

    #[test]
    fn white_is_as_high_as_asked_and_the_top_of_the_picture_is_the_far_edge() {
        // White along the top row, black below.
        let mut raw = Vec::new();
        for y in 0..2u16 {
            for _ in 0..2 {
                raw.extend_from_slice(&(if y == 0 { u16::MAX } else { 0 }).to_le_bytes());
            }
        }
        let greys = Greys::from_raw16(&raw).expect("greys");
        let file = TerrainFile { size: 100.0, resolution: 257, ..TerrainFile::default() };
        let heights = greys.heights(&file, 40.0);
        assert!((heights.height_at(50.0, 100.0) - 40.0).abs() < 1e-3);
        assert!(heights.height_at(50.0, 0.0).abs() < 1e-3);
        assert!(Greys::from_raw16(&[0, 0, 0]).is_err());
    }
}
