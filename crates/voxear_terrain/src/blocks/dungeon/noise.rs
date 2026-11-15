//! Numbers from places: a hash of whole numbers, and smooth noise read
//! between the hashes of a lattice's corners. Hashed from whole numbers
//! alone, so a place gives the same on every machine and every chunk.

/// A hash of a block's place and a seed.
pub(crate) fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    for v in [x as u32, y as u32, z as u32] {
        h ^= v.wrapping_mul(0x85EB_CA6B);
        h = h.rotate_left(13).wrapping_mul(0xC2B2_AE35);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// A hash of a block's place as a share from 0 to 1.
pub(crate) fn unit(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    (hash(x, y, z, seed) >> 8) as f32 / 16_777_216.0
}

/// Smooth noise at `p`, from -1 to 1, its lattice `cell` blocks apart, each
/// axis stretched by `stretch` (2 along y lays it in beds).
pub(crate) fn smooth(p: [f32; 3], cell: f32, stretch: [f32; 3], seed: u32) -> f32 {
    let q = [p[0] / (cell * stretch[0]), p[1] / (cell * stretch[1]), p[2] / (cell * stretch[2])];
    let base = q.map(|v| v.floor());
    let t = [0, 1, 2].map(|a| {
        let f = q[a] - base[a];
        f * f * (3.0 - 2.0 * f)
    });
    let b = base.map(|v| v as i32);
    let corner = |dx: i32, dy: i32, dz: i32| unit(b[0] + dx, b[1] + dy, b[2] + dz, seed) * 2.0 - 1.0;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), t[0]);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), t[0]);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), t[0]);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), t[0]);
    lerp(lerp(x00, x10, t[1]), lerp(x01, x11, t[1]), t[2])
}

/// Two octaves of [`smooth`], the second half as far apart and half as
/// strong.
pub(crate) fn rough(p: [f32; 3], cell: f32, stretch: [f32; 3], seed: u32) -> f32 {
    (smooth(p, cell, stretch, seed) + smooth(p, cell * 0.5, stretch, seed ^ 0x51ED_270B) * 0.5) / 1.5
}
