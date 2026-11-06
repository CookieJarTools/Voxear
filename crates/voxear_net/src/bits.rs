//! Packing values into as few bits as they need: whole numbers in a known
//! number of bits or as varints, floats quantised to a range and precision,
//! and rotations as their three smallest components. Replication writes
//! everything through these, so a moving player costs about a hundred bits.

use std::f32::consts::FRAC_1_SQRT_2;

/// Writes bits into bytes, lowest first.
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    scratch: u64,
    held: u32,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes the low `bits` bits of `value`, up to 32.
    pub fn write_bits(&mut self, value: u32, bits: u32) {
        debug_assert!(bits <= 32);
        if bits == 0 {
            return;
        }
        let mask = if bits == 32 { u32::MAX } else { (1 << bits) - 1 };
        self.scratch |= u64::from(value & mask) << self.held;
        self.held += bits;
        while self.held >= 8 {
            self.bytes.push(self.scratch as u8);
            self.scratch >>= 8;
            self.held -= 8;
        }
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_bits(u32::from(value), 1);
    }

    /// A whole number from `min` to `max`, in as few bits as that range needs.
    pub fn write_ranged(&mut self, value: u32, min: u32, max: u32) {
        debug_assert!(min <= value && value <= max);
        self.write_bits(value.clamp(min, max) - min, bits_for(max - min));
    }

    /// A whole number in groups of seven bits, small ones short.
    pub fn write_varint(&mut self, mut value: u64) {
        loop {
            let group = (value & 0x7F) as u32;
            value >>= 7;
            self.write_bits(group | if value != 0 { 0x80 } else { 0 }, 8);
            if value == 0 {
                break;
            }
        }
    }

    /// A float within `min` to `max`, to `precision`, clamped into range.
    pub fn write_quantized(&mut self, value: f32, min: f32, max: f32, precision: f32) {
        let steps = quantize_steps(min, max, precision);
        let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
        self.write_bits((t * steps as f32).round() as u32, bits_for(steps));
    }

    pub fn write_f32(&mut self, value: f32) {
        self.write_bits(value.to_bits(), 32);
    }

    /// A unit quaternion `[x, y, z, w]` as its three smallest components,
    /// each in `bits` bits, and which was largest.
    pub fn write_rotation(&mut self, q: [f32; 4], bits: u32) {
        let largest = (0..4).max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs())).unwrap_or(3);
        // q and -q are the same turn: the largest is made positive and left out.
        let sign = if q[largest] < 0.0 { -1.0 } else { 1.0 };
        self.write_bits(largest as u32, 2);
        let steps = (1u32 << bits) - 1;
        for (i, &c) in q.iter().enumerate() {
            if i != largest {
                let t = ((c * sign + FRAC_1_SQRT_2) / (2.0 * FRAC_1_SQRT_2)).clamp(0.0, 1.0);
                self.write_bits((t * steps as f32).round() as u32, bits);
            }
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_bits(u32::from(b), 8);
        }
    }

    /// How many bits are written so far.
    pub fn bits(&self) -> usize {
        self.bytes.len() * 8 + self.held as usize
    }

    /// The bytes written, the last padded with zeros.
    pub fn finish(mut self) -> Vec<u8> {
        if self.held > 0 {
            self.bytes.push(self.scratch as u8);
        }
        self.bytes
    }
}

/// Reads bits a [`BitWriter`] wrote; every read is `None` past the end, so
/// a short or garbled packet is turned away rather than read wrongly.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub fn read_bits(&mut self, bits: u32) -> Option<u32> {
        debug_assert!(bits <= 32);
        if self.at + bits as usize > self.bytes.len() * 8 {
            return None;
        }
        let mut value = 0u64;
        let mut got = 0;
        while got < bits {
            let byte = self.bytes[self.at / 8];
            let offset = (self.at % 8) as u32;
            let take = (8 - offset).min(bits - got);
            let part = (u64::from(byte) >> offset) & ((1 << take) - 1);
            value |= part << got;
            got += take;
            self.at += take as usize;
        }
        Some(value as u32)
    }

    pub fn read_bool(&mut self) -> Option<bool> {
        self.read_bits(1).map(|b| b != 0)
    }

    pub fn read_ranged(&mut self, min: u32, max: u32) -> Option<u32> {
        let value = self.read_bits(bits_for(max - min))? + min;
        (value <= max).then_some(value)
    }

    pub fn read_varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for group in 0..10 {
            let byte = self.read_bits(8)?;
            value |= u64::from(byte & 0x7F) << (group * 7);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    pub fn read_quantized(&mut self, min: f32, max: f32, precision: f32) -> Option<f32> {
        let steps = quantize_steps(min, max, precision);
        let step = self.read_bits(bits_for(steps))?;
        (step <= steps).then(|| min + (max - min) * step as f32 / steps as f32)
    }

    pub fn read_f32(&mut self) -> Option<f32> {
        self.read_bits(32).map(f32::from_bits)
    }

    pub fn read_rotation(&mut self, bits: u32) -> Option<[f32; 4]> {
        let largest = self.read_bits(2)? as usize;
        let steps = (1u32 << bits) - 1;
        let mut q = [0.0f32; 4];
        let mut sum = 0.0;
        for (i, c) in q.iter_mut().enumerate() {
            if i != largest {
                let t = self.read_bits(bits)? as f32 / steps as f32;
                *c = t * 2.0 * FRAC_1_SQRT_2 - FRAC_1_SQRT_2;
                sum += *c * *c;
            }
        }
        q[largest] = (1.0 - sum).max(0.0).sqrt();
        let length = q.iter().map(|c| c * c).sum::<f32>().sqrt().max(1e-6);
        Some(q.map(|c| c / length))
    }

    pub fn read_bytes(&mut self, out: &mut [u8]) -> Option<()> {
        for b in out {
            *b = self.read_bits(8)? as u8;
        }
        Some(())
    }

    /// How many bits are left to read.
    pub fn remaining(&self) -> usize {
        self.bytes.len() * 8 - self.at
    }
}

/// How many bits hold any number up to `max`.
pub fn bits_for(max: u32) -> u32 {
    32 - max.leading_zeros()
}

/// How many steps a range is cut into at a precision, at most 2³¹.
fn quantize_steps(min: f32, max: f32, precision: f32) -> u32 {
    (((max - min) / precision.max(f32::EPSILON)).ceil() as f64).clamp(1.0, f64::from(1u32 << 31)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_come_back_as_written() {
        let mut w = BitWriter::new();
        w.write_bits(5, 3);
        w.write_bool(true);
        w.write_ranged(70, 10, 100);
        w.write_varint(300);
        w.write_varint(u64::MAX);
        w.write_quantized(12.345, -100.0, 100.0, 0.01);
        w.write_f32(-1.5);
        w.write_bytes(b"hi");
        let bits = w.bits();
        let bytes = w.finish();
        assert_eq!(bytes.len(), bits.div_ceil(8));
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(3), Some(5));
        assert_eq!(r.read_bool(), Some(true));
        assert_eq!(r.read_ranged(10, 100), Some(70));
        assert_eq!(r.read_varint(), Some(300));
        assert_eq!(r.read_varint(), Some(u64::MAX));
        let q = r.read_quantized(-100.0, 100.0, 0.01).unwrap();
        assert!((q - 12.345).abs() <= 0.005 + 1e-4, "{q}");
        assert_eq!(r.read_f32(), Some(-1.5));
        let mut hi = [0; 2];
        assert_eq!(r.read_bytes(&mut hi), Some(()));
        assert_eq!(&hi, b"hi");
    }

    #[test]
    fn reading_past_the_end_is_none() {
        let bytes = [0xFF];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(6), Some(0x3F));
        assert_eq!(r.read_bits(3), None);
        assert_eq!(BitReader::new(&[0xFF; 3]).read_varint(), None, "an unended varint");
    }

    #[test]
    fn rotations_survive_in_29_bits() {
        let turns = [
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, -1.0],
            [0.5, -0.5, 0.5, 0.5],
            [0.1, 0.7, -0.2, 0.676],
            [-0.9, 0.1, 0.3, 0.3],
        ];
        for q in turns {
            let length = q.iter().map(|c: &f32| c * c).sum::<f32>().sqrt();
            let q = q.map(|c| c / length);
            let mut w = BitWriter::new();
            w.write_rotation(q, 9);
            assert_eq!(w.bits(), 29);
            let back = BitReader::new(&w.finish()).read_rotation(9).unwrap();
            // The same turn whichever sign.
            let dot: f32 = q.iter().zip(&back).map(|(a, b)| a * b).sum();
            assert!(dot.abs() > 0.999, "{q:?} came back {back:?}");
        }
    }
}
