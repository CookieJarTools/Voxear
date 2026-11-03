//! The tables rectangle and disc lights shade with: linearly transformed
//! cosines, one for each roughness and view angle, fitted to GGX ahead of
//! time (see [`crate::ltc_fit`]) and shipped in `data/ltc.bin`.
//!
//! The renderer uploads them as two 64×64 half-float textures. The first
//! holds each cell's inverse matrix, the four terms that are not 0 or 1;
//! the second its magnitude and the share Fresnel weighs. Rows are the
//! roughness, from 0 to 1, and columns sqrt(1 - cos θ) of the view angle.

use std::sync::OnceLock;

/// Cells each way.
pub const LTC_SIZE: usize = 64;

/// What the file starts with, then the size and the cells.
const MAGIC: &[u8; 4] = b"LTC1";

/// One roughness and view angle's fit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LtcCell {
    /// The inverse matrix's terms at (0, 0), (0, 2), (2, 0) and (2, 2),
    /// column by row, scaled so (1, 1) is one.
    pub inverse: [f32; 4],
    /// GGX's directional albedo, which the lobe integrates to.
    pub magnitude: f32,
    /// The share of it Schlick's Fresnel weighs by (1 - v·h)^5.
    pub fresnel: f32,
}

/// The shipped cells, rows of roughness within columns of view angle.
pub fn cells() -> &'static [LtcCell] {
    static CELLS: OnceLock<Vec<LtcCell>> = OnceLock::new();
    CELLS.get_or_init(|| from_bytes(include_bytes!("../data/ltc.bin")).expect("data/ltc.bin is a table of LTC fits"))
}

/// The table as the file stores it: the magic, the size, then six
/// little-endian floats a cell. Only the fit writes it.
#[cfg(test)]
pub fn to_bytes(cells: &[LtcCell]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend((LTC_SIZE as u32).to_le_bytes());
    for cell in cells {
        for value in cell.inverse.into_iter().chain([cell.magnitude, cell.fresnel]) {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes
}

fn from_bytes(bytes: &[u8]) -> Option<Vec<LtcCell>> {
    let rest = bytes.strip_prefix(MAGIC)?;
    let (size, rest) = rest.split_at_checked(4)?;
    let size = u32::from_le_bytes(size.try_into().ok()?) as usize;
    if size != LTC_SIZE || rest.len() != size * size * 24 {
        return None;
    }
    let floats: Vec<f32> = rest.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
    let cell = |c: &[f32]| LtcCell { inverse: [c[0], c[1], c[2], c[3]], magnitude: c[4], fresnel: c[5] };
    Some(floats.chunks_exact(6).map(cell).collect())
}

/// The two textures' texels as half floats, four to a texel: the inverse
/// matrices, then the magnitudes and Fresnel shares.
pub fn encode() -> (Vec<u16>, Vec<u16>) {
    let cells = cells();
    let matrix = cells.iter().flat_map(|c| c.inverse.map(half)).collect();
    let amplitude = cells.iter().flat_map(|c| [c.magnitude, c.fresnel, 0.0, 0.0].map(half)).collect();
    (matrix, amplitude)
}

/// A float as IEEE half precision, rounded to nearest, for textures. Values
/// past the largest half become infinity, and tiny ones zero.
pub fn half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x7F_FFFF;
    if exponent == 0xFF {
        return sign | 0x7C00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    let exponent = exponent - 127 + 15;
    if exponent >= 0x1F {
        return sign | 0x7C00;
    }
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        // Subnormal: the hidden bit joins the mantissa, shifted down.
        let mantissa = mantissa | 0x80_0000;
        let shift = (14 - exponent) as u32;
        let rounded = (mantissa + (1 << (shift - 1))) >> shift;
        return sign | rounded as u16;
    }
    let rounded = (exponent as u32) << 10 | (mantissa >> 13);
    // Round half up on the dropped bits; a carry rolls into the exponent, as it should.
    sign | (rounded + ((mantissa >> 12) & 1)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_trip_common_values() {
        assert_eq!(half(0.0), 0);
        assert_eq!(half(1.0), 0x3C00);
        assert_eq!(half(-2.0), 0xC000);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(65504.0), 0x7BFF);
        assert_eq!(half(1e6), 0x7C00);
        assert_eq!(half(6.0e-5), 0x03EF, "subnormal");
    }

    #[test]
    fn the_table_reads_back_as_written() {
        let cells = cells();
        assert_eq!(cells.len(), LTC_SIZE * LTC_SIZE);
        assert_eq!(from_bytes(&to_bytes(cells)).as_deref(), Some(cells));
    }
}
