//! The dappled light under a canopy of leaves, as a cookie the sun shines
//! through where a look or lighting settings give it a `canopy`: mostly shade, lit
//! green where light passes through the leaves, with clusters of gaps the sun
//! falls through as soft round flecks, as each gap makes an image of the sun
//! on the ground. Made on the CPU once, as a tiling picture, from whole-number
//! hashes, so it is the same everywhere.

/// The picture's side, in pixels.
pub const CANOPY_SIZE: u32 = 256;

/// The light let through where leaves are, as sRGB: a dim green.
const THROUGH_LEAVES: [f32; 3] = [0.16, 0.22, 0.1];

fn hash(mut x: u64) -> u64 {
    // SplitMix64's finaliser.
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A number from 0 to 1 for the `n`th draw of `seed`.
fn unit(seed: u64, n: u64) -> f32 {
    (hash(seed.wrapping_mul(0x1000_0001).wrapping_add(n)) >> 40) as f32 / (1u64 << 24) as f32
}

/// The canopy's picture: RGBA8, sRGB, `CANOPY_SIZE` a side, tiling.
pub fn canopy_cookie() -> Vec<u8> {
    let size = CANOPY_SIZE as f32;
    // Gaps, each a place, a radius and how much of the sun it lets through: a few wide ones where branches
    // part, and many small ones, gathered round them, between single leaves.
    let mut gaps: Vec<([f32; 2], f32, f32)> = Vec::new();
    for cluster in 0..24u64 {
        let centre = [unit(cluster, 0) * size, unit(cluster, 1) * size];
        gaps.push((centre, 10.0 + unit(cluster, 2) * 14.0, 1.0));
        for fleck in 0..14u64 {
            let n = 10 + fleck * 4;
            let angle = unit(cluster, n) * std::f32::consts::TAU;
            let reach = 8.0 + unit(cluster, n + 1) * 34.0;
            let at = [centre[0] + angle.cos() * reach, centre[1] + angle.sin() * reach];
            gaps.push((at, 2.0 + unit(cluster, n + 2) * 5.0, 0.6 + unit(cluster, n + 3) * 0.4));
        }
    }
    let mut pixels = Vec::with_capacity((CANOPY_SIZE * CANOPY_SIZE * 4) as usize);
    for y in 0..CANOPY_SIZE {
        for x in 0..CANOPY_SIZE {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let mut open = 0.0f32;
            for &(at, radius, light) in &gaps {
                // The nearest copy of the gap, so the picture tiles.
                let wrap = |d: f32| d - size * (d / size).round();
                let d = (wrap(p[0] - at[0]).powi(2) + wrap(p[1] - at[1]).powi(2)).sqrt();
                // A soft edge, a third of the gap wide: the sun's own width, as the gap's image of it.
                let t = ((radius - d) / (radius * 0.35)).clamp(0.0, 1.0);
                open = open.max(t * t * (3.0 - 2.0 * t) * light);
            }
            for leaf in THROUGH_LEAVES {
                pixels.push((((leaf + (1.0 - leaf) * open) * 255.0).round()) as u8);
            }
            pixels.push(255);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canopy_is_dappled_tiles_and_is_the_same_each_time() {
        let pixels = canopy_cookie();
        assert_eq!(pixels.len(), (CANOPY_SIZE * CANOPY_SIZE * 4) as usize);
        assert_eq!(pixels, canopy_cookie(), "the same every time");
        let red = |x: u32, y: u32| i32::from(pixels[((y * CANOPY_SIZE + x) * 4) as usize]);
        let lit = (0..CANOPY_SIZE * CANOPY_SIZE).filter(|i| red(i % CANOPY_SIZE, i / CANOPY_SIZE) > 200).count();
        let share = lit as f32 / (CANOPY_SIZE * CANOPY_SIZE) as f32;
        assert!((0.1..0.5).contains(&share), "flecks of sun in shade: {share}");
        // Across the picture's edge it carries on as within it.
        let last = CANOPY_SIZE - 1;
        let across: i32 = (0..CANOPY_SIZE).map(|y| (red(0, y) - red(last, y)).abs()).sum();
        let within: i32 = (0..CANOPY_SIZE).map(|y| (red(100, y) - red(101, y)).abs()).sum();
        assert!(across <= within * 3 + 64, "tiles: {across} across the edge, {within} within");
        // Leaves let a dim green through.
        let darkest = (0..CANOPY_SIZE * CANOPY_SIZE).map(|i| pixels[(i * 4 + 1) as usize]).min().unwrap();
        assert!(darkest > 30 && darkest < 80, "{darkest}");
    }
}
