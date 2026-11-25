//! Making a model into a [`VoxelPrint`]: its triangles into the voxels they
//! pass through, each the colour its picture has where the triangle is
//! nearest the voxel's middle; the inside filled, each voxel the colour of
//! the skin nearest it; and every colour the nearest of a few.
//!
//! A voxel is part of the skin wherever a triangle passes through it, its
//! box tested against the triangle on every axis that can part them, so a
//! closed skin has no gap the outside can come through, however thin. The
//! outside is then found from the box's border, a voxel at a time through
//! the faces of those not in the skin, and all the box it leaves is inside.

use std::collections::HashMap;

use anyhow::{Result, bail};
use voxear_core::math::{Mat4, Vec2, Vec3};

use super::print::{LARGEST_PRINT, MOST_COLOURS, VoxelPrint, VoxelPrintFill, VoxelPrintSettings};

/// The most voxels a print's box holds while it is made.
const MOST_CELLS: u64 = 400_000_000;

/// A picture a surface is coloured from: its size, its pixels as sRGB RGBA8,
/// and smaller copies of it, each half the last, for voxels much larger
/// than its pixels.
pub struct Picture {
    levels: Vec<(u32, u32, Vec<[f32; 4]>)>,
}

impl Picture {
    /// A picture of `width` by `height` sRGB RGBA8 pixels.
    pub fn new(width: u32, height: u32, rgba: &[u8]) -> Self {
        let first: Vec<[f32; 4]> =
            rgba.chunks_exact(4).map(|p| [linear(p[0]), linear(p[1]), linear(p[2]), f32::from(p[3]) / 255.0]).collect();
        let mut levels = vec![(width.max(1), height.max(1), first)];
        while let Some((w, h, last)) = levels.last().filter(|(w, h, _)| *w > 1 || *h > 1) {
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![[0.0; 4]; (nw * nh) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    let mut sum = [0.0; 4];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let p = last[(((y * 2 + dy).min(h - 1)) * w + (x * 2 + dx).min(w - 1)) as usize];
                        (0..4).for_each(|i| sum[i] += p[i] * 0.25);
                    }
                    next[(y * nw + x) as usize] = sum;
                }
            }
            levels.push((nw, nh, next));
        }
        Self { levels }
    }

    /// Its colour at `uv`, linear, from the level whose pixels are about
    /// `across` of the full picture's wide, wrapping.
    fn at(&self, uv: Vec2, across: f32) -> [f32; 4] {
        let level = (across.max(1.0).log2().floor() as usize).min(self.levels.len() - 1);
        let (w, h, pixels) = &self.levels[level];
        let x = ((uv.x - uv.x.floor()) * *w as f32) as u32;
        let y = ((uv.y - uv.y.floor()) * *h as f32) as u32;
        pixels[(y.min(h - 1) * w + x.min(w - 1)) as usize]
    }

    /// How many pixels its full picture is wide and tall.
    fn size(&self) -> Vec2 {
        Vec2::new(self.levels[0].0 as f32, self.levels[0].1 as f32)
    }
}

/// What colours a part of a surface: a colour, linear, and the picture it
/// multiplies, with the alpha under which a pixel is a hole.
pub struct Paint {
    pub colour: [f32; 4],
    pub picture: Option<Picture>,
    pub cutoff: Option<f32>,
}

impl Default for Paint {
    fn default() -> Self {
        Self { colour: [0.8, 0.8, 0.8, 1.0], picture: None, cutoff: None }
    }
}

/// A model's triangles as a print is made from them: their corners in
/// metres from its origin, their UVs, and which paint each triangle has.
#[derive(Default)]
pub struct Surface {
    pub points: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[u32; 3]>,
    pub paints: Vec<u16>,
}

impl Surface {
    /// Adds a mesh's triangles, its points moved by `place`, all in `paint`.
    pub fn add(&mut self, place: Mat4, points: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32], paint: u16) {
        let first = self.points.len() as u32;
        self.points.extend(points.iter().map(|&p| place.transform_point3(Vec3::from(p))));
        self.uvs.extend((0..points.len()).map(|i| uvs.get(i).map_or(Vec2::ZERO, |&uv| Vec2::from(uv))));
        for triangle in indices.chunks_exact(3) {
            self.triangles.push([first + triangle[0], first + triangle[1], first + triangle[2]]);
            self.paints.push(paint);
        }
    }
}

/// The voxels of the skin: for each voxel a triangle passes through, the
/// sum of its colours where they are nearest its middle, and how many.
type Skin = HashMap<u32, [f32; 4]>;

/// Makes `surface`, coloured with `paints`, into a print as `settings` say.
pub fn voxelize(surface: &Surface, paints: &[Paint], settings: &VoxelPrintSettings) -> Result<VoxelPrint> {
    let per = settings.voxels_per_metre.clamp(1, super::MOST_VOXELS_A_METRE) as f32;
    let scale = if settings.scale > 0.0 { settings.scale } else { 1.0 };
    if surface.triangles.is_empty() {
        bail!("the model has no triangles");
    }
    let points: Vec<Vec3> = surface.points.iter().map(|&p| p * scale * per).collect();
    let mut low = Vec3::splat(f32::MAX);
    let mut high = Vec3::splat(f32::MIN);
    for triangle in &surface.triangles {
        for &i in triangle {
            low = low.min(points[i as usize]);
            high = high.max(points[i as usize]);
        }
    }
    // A voxel of room all round, which the outside is found from.
    let least = low.floor() - 1.0;
    let size = ((high - least).ceil() + 1.0).max(Vec3::splat(1.0));
    let size = [size.x as u32, size.y as u32, size.z as u32];
    if size.iter().any(|&a| a > LARGEST_PRINT)
        || u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]) > MOST_CELLS
    {
        bail!(
            "the model is {size:?} voxels at {} voxels a metre, too large to print: fewer voxels a metre make it smaller",
            settings.voxels_per_metre
        );
    }
    let points: Vec<Vec3> = points.iter().map(|&p| p - least).collect();
    let skin = skin_of(surface, &points, paints, size);
    if skin.is_empty() {
        bail!("no part of the model is solid: its pictures may be all holes");
    }
    let cells = (size[0] * size[1] * size[2]) as usize;
    // 0 outside, 1 skin, 2 inside; then each voxel's colour, as a place among the skin's.
    let mut kind = vec![2u8; cells];
    let mut colour_of = vec![u32::MAX; cells];
    let mut colours: Vec<[f32; 3]> = Vec::with_capacity(skin.len());
    for (&cell, sum) in &skin {
        kind[cell as usize] = 1;
        colour_of[cell as usize] = colours.len() as u32;
        colours.push([sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3]]);
    }
    outside(&mut kind, size);
    match settings.fill {
        VoxelPrintFill::Solid => {}
        VoxelPrintFill::Sealed => seal(&mut kind, size),
        VoxelPrintFill::Shell => kind.iter_mut().filter(|k| **k == 2).for_each(|k| *k = 0),
    }
    colour_inside(&kind, &mut colour_of, size);
    // The colours, each as many times as voxels have it, made the nearest of a few.
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    let srgb: Vec<[u8; 3]> = colours.iter().map(|c| c.map(encode)).collect();
    for (cell, &k) in kind.iter().enumerate() {
        if k != 0 {
            *counts.entry(srgb[colour_of[cell] as usize]).or_default() += 1;
        }
    }
    let palette = quantize(&counts, settings.colours.clamp(1, MOST_COLOURS) as usize);
    let nearest: HashMap<[u8; 3], u8> = counts.keys().map(|&c| (c, nearest_of(&palette, c))).collect();
    let palette: Vec<[u8; 4]> = palette.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
    let origin = -least;
    let print = VoxelPrint::from_voxels(size, per as u32, origin, palette, settings.clone(), |c| {
        let cell = (c[0] + size[0] * (c[1] + size[1] * c[2])) as usize;
        if kind[cell] == 0 { 0 } else { nearest[&srgb[colour_of[cell] as usize]] + 1 }
    });
    Ok(print)
}

/// The voxels every triangle passes through, in voxels from the box's
/// least corner, coloured where each is nearest their middles; on as many
/// threads as there are cores.
fn skin_of(surface: &Surface, points: &[Vec3], paints: &[Paint], size: [u32; 3]) -> Skin {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    let chunk = surface.triangles.len().div_ceil(threads).max(1);
    let parts: Vec<Skin> = std::thread::scope(|scope| {
        let jobs: Vec<_> = surface
            .triangles
            .chunks(chunk)
            .zip(surface.paints.chunks(chunk))
            .map(|(triangles, kinds)| {
                scope.spawn(move || {
                    let mut skin = Skin::new();
                    for (triangle, &paint) in triangles.iter().zip(kinds) {
                        let paint = paints.get(usize::from(paint));
                        rasterize(surface, points, triangle, paint, size, &mut skin);
                    }
                    skin
                })
            })
            .collect();
        jobs.into_iter().map(|job| job.join().expect("a share of the triangles")).collect()
    });
    let mut parts = parts.into_iter();
    let mut skin = parts.next().unwrap_or_default();
    for part in parts {
        for (cell, sum) in part {
            let into = skin.entry(cell).or_insert([0.0; 4]);
            (0..4).for_each(|i| into[i] += sum[i]);
        }
    }
    skin
}

/// Adds the voxels one triangle passes through to `skin`.
fn rasterize(
    surface: &Surface,
    points: &[Vec3],
    triangle: &[u32; 3],
    paint: Option<&Paint>,
    size: [u32; 3],
    skin: &mut Skin,
) {
    let [a, b, c] = triangle.map(|i| points[i as usize]);
    let uv = triangle.map(|i| surface.uvs[i as usize]);
    let low = a.min(b).min(c).floor().max(Vec3::ZERO);
    let high = a.max(b).max(c).floor().min(Vec3::new(size[0] as f32 - 1.0, size[1] as f32 - 1.0, size[2] as f32 - 1.0));
    // How many of its picture's pixels a voxel spans: the picture's pixels on the triangle, over its voxels.
    let area = (b - a).cross(c - a).length() * 0.5;
    let uv_area = ((uv[1] - uv[0]).perp_dot(uv[2] - uv[0])).abs() * 0.5;
    let picture = paint.and_then(|paint| paint.picture.as_ref());
    let across = match picture {
        Some(picture) if area > 1e-12 => (uv_area * picture.size().x * picture.size().y / area).sqrt(),
        _ => 1.0,
    };
    for z in low.z as u32..=high.z as u32 {
        for y in low.y as u32..=high.y as u32 {
            for x in low.x as u32..=high.x as u32 {
                let middle = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if !overlaps([a - middle, b - middle, c - middle]) {
                    continue;
                }
                let colour = match paint {
                    Some(paint) => {
                        let (u, v, w) = nearest_on(middle, a, b, c);
                        let at = uv[0] * u + uv[1] * v + uv[2] * w;
                        let texel = picture.map_or([1.0; 4], |picture| picture.at(at, across));
                        let colour: [f32; 4] = std::array::from_fn(|i| paint.colour[i] * texel[i]);
                        if paint.cutoff.is_some_and(|cutoff| colour[3] < cutoff) {
                            continue;
                        }
                        colour
                    }
                    None => [0.8, 0.8, 0.8, 1.0],
                };
                let cell = x + size[0] * (y + size[1] * z);
                let sum = skin.entry(cell).or_insert([0.0; 4]);
                (0..3).for_each(|i| sum[i] += colour[i]);
                sum[3] += 1.0;
            }
        }
    }
}

/// Whether the triangle `v`, its corners from a voxel's middle, passes
/// through the voxel: no axis parts them, of the box's three, the
/// triangle's face, and the nine across their edges.
fn overlaps(v: [Vec3; 3]) -> bool {
    let half = 0.5;
    for axis in 0..3 {
        let (least, most) = (v[0][axis].min(v[1][axis]).min(v[2][axis]), v[0][axis].max(v[1][axis]).max(v[2][axis]));
        if least > half || most < -half {
            return false;
        }
    }
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let normal = edges[0].cross(edges[1]);
    let reach = half * (normal.x.abs() + normal.y.abs() + normal.z.abs());
    let d = normal.dot(v[0]);
    if d.abs() > reach {
        return false;
    }
    for edge in edges {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let across = axis.cross(edge);
            if across.length_squared() < 1e-20 {
                continue;
            }
            let p = [across.dot(v[0]), across.dot(v[1]), across.dot(v[2])];
            let r = half * (across.x.abs() + across.y.abs() + across.z.abs());
            if p[0].min(p[1]).min(p[2]) > r || p[0].max(p[1]).max(p[2]) < -r {
                return false;
            }
        }
    }
    true
}

/// The point of the triangle `a`, `b`, `c` nearest `p`, as how much of
/// each corner it is.
fn nearest_on(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> (f32, f32, f32) {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (1.0, 0.0, 0.0);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (0.0, 1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (1.0 - v, v, 0.0);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (0.0, 0.0, 1.0);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (1.0 - w, 0.0, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (0.0, 1.0 - w, w);
    }
    let denom = 1.0 / (va + vb + vc).max(1e-30);
    let (v, w) = (vb * denom, vc * denom);
    (1.0 - v - w, v, w)
}

/// Marks every voxel the outside reaches, through the faces of voxels not
/// in the skin, from the box's border, 0.
fn outside(kind: &mut [u8], size: [u32; 3]) {
    let [sx, sy, sz] = size.map(|a| a as i64);
    let index = |x: i64, y: i64, z: i64| (x + sx * (y + sy * z)) as usize;
    let mut queue: Vec<(i64, i64, i64)> = Vec::new();
    let visit = |kind: &mut [u8], queue: &mut Vec<(i64, i64, i64)>, x, y, z| {
        if x < 0 || y < 0 || z < 0 || x >= sx || y >= sy || z >= sz {
            return;
        }
        let i = index(x, y, z);
        if kind[i] == 2 {
            kind[i] = 0;
            queue.push((x, y, z));
        }
    };
    for z in 0..sz {
        for y in 0..sy {
            for x in 0..sx {
                if x == 0 || y == 0 || z == 0 || x == sx - 1 || y == sy - 1 || z == sz - 1 {
                    visit(kind, &mut queue, x, y, z);
                }
            }
        }
    }
    while let Some((x, y, z)) = queue.pop() {
        for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            visit(kind, &mut queue, x + dx, y + dy, z + dz);
        }
    }
}

/// Takes as inside every voxel the outside reached that has the skin on
/// five or six of its six sides, along the lines through it.
fn seal(kind: &mut [u8], size: [u32; 3]) {
    let [sx, sy, sz] = size.map(|a| a as usize);
    let index = |x: usize, y: usize, z: usize| x + sx * (y + sy * z);
    let mut sides = vec![0u8; kind.len()];
    // Along each axis both ways: whether skin was met before each voxel.
    let lines: [(usize, [usize; 2], fn(usize, usize, usize) -> [usize; 3]); 3] =
        [(sx, [sy, sz], |t, a, b| [t, a, b]), (sy, [sx, sz], |t, a, b| [a, t, b]), (sz, [sx, sy], |t, a, b| [a, b, t])];
    for (length, [n, m], at) in lines {
        for a in 0..n {
            for b in 0..m {
                let mut met = false;
                for t in 0..length {
                    let [x, y, z] = at(t, a, b);
                    let i = index(x, y, z);
                    met |= kind[i] == 1;
                    sides[i] += u8::from(met && kind[i] != 1);
                }
                met = false;
                for t in (0..length).rev() {
                    let [x, y, z] = at(t, a, b);
                    let i = index(x, y, z);
                    met |= kind[i] == 1;
                    sides[i] += u8::from(met && kind[i] != 1);
                }
            }
        }
    }
    for (k, &s) in kind.iter_mut().zip(&sides) {
        if *k == 0 && s >= 5 {
            *k = 2;
        }
    }
}

/// Gives each voxel inside the colour of the skin nearest it, a face at a
/// time inwards from the skin.
fn colour_inside(kind: &[u8], colour_of: &mut [u32], size: [u32; 3]) {
    let [sx, sy, sz] = size.map(|a| a as i64);
    let mut front: Vec<usize> = (0..kind.len()).filter(|&i| kind[i] == 1).collect();
    while !front.is_empty() {
        let mut next = Vec::new();
        for &i in &front {
            let (x, y, z) = ((i as i64) % sx, (i as i64 / sx) % sy, i as i64 / (sx * sy));
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                if nx < 0 || ny < 0 || nz < 0 || nx >= sx || ny >= sy || nz >= sz {
                    continue;
                }
                let n = (nx + sx * (ny + sy * nz)) as usize;
                if kind[n] == 2 && colour_of[n] == u32::MAX {
                    colour_of[n] = colour_of[i];
                    next.push(n);
                }
            }
        }
        front = next;
    }
}

/// How far apart two colours look, as sRGB: green counts most, then blue.
fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    let d = |i: usize| (i32::from(a[i]) - i32::from(b[i])).pow(2) as u32;
    2 * d(0) + 4 * d(1) + 3 * d(2)
}

fn nearest_of(palette: &[[u8; 3]], colour: [u8; 3]) -> u8 {
    palette.iter().enumerate().min_by_key(|(_, p)| distance(**p, colour)).map_or(0, |(i, _)| i as u8)
}

/// At most `most` colours standing for `counts`: the colours split again
/// and again across their widest spread, the box with the most voxels
/// first, each its voxels' average; then each moved to the average of the
/// colours nearest it a few times.
pub(crate) fn quantize(counts: &HashMap<[u8; 3], u32>, most: usize) -> Vec<[u8; 3]> {
    let mut all: Vec<([u8; 3], u32)> = counts.iter().map(|(&c, &n)| (c, n)).collect();
    all.sort_unstable();
    if all.len() <= most {
        return all.into_iter().map(|(c, _)| c).collect();
    }
    let mut boxes: Vec<Vec<([u8; 3], u32)>> = vec![all];
    while boxes.len() < most {
        // The box with the most voxels that can still be split.
        let Some(at) = (0..boxes.len())
            .filter(|&i| boxes[i].len() > 1)
            .max_by_key(|&i| boxes[i].iter().map(|(_, n)| u64::from(*n)).sum::<u64>())
        else {
            break;
        };
        let mut split = boxes.swap_remove(at);
        let spread = |axis: usize| {
            let (lo, hi) = split.iter().fold((255u8, 0u8), |(lo, hi), (c, _)| (lo.min(c[axis]), hi.max(c[axis])));
            hi - lo
        };
        let axis = (0..3).max_by_key(|&a| spread(a)).unwrap_or(0);
        split.sort_unstable_by_key(|(c, _)| c[axis]);
        let total: u64 = split.iter().map(|(_, n)| u64::from(*n)).sum();
        let mut seen = 0u64;
        let mut cut = 1;
        for (i, (_, n)) in split.iter().enumerate() {
            seen += u64::from(*n);
            if seen * 2 >= total {
                cut = (i + 1).clamp(1, split.len() - 1);
                break;
            }
        }
        let rest = split.split_off(cut);
        boxes.push(split);
        boxes.push(rest);
    }
    let average = |colours: &[([u8; 3], u32)]| {
        let mut sum = [0u64; 3];
        let mut n = 0u64;
        for (c, count) in colours {
            (0..3).for_each(|i| sum[i] += u64::from(c[i]) * u64::from(*count));
            n += u64::from(*count);
        }
        sum.map(|s| (s / n.max(1)) as u8)
    };
    let mut palette: Vec<[u8; 3]> = boxes.iter().map(|b| average(b)).collect();
    let colours: Vec<([u8; 3], u32)> = boxes.into_iter().flatten().collect();
    for _ in 0..4 {
        let mut groups: Vec<Vec<([u8; 3], u32)>> = vec![Vec::new(); palette.len()];
        for &(c, n) in &colours {
            groups[usize::from(nearest_of(&palette, c))].push((c, n));
        }
        for (entry, group) in palette.iter_mut().zip(&groups) {
            if !group.is_empty() {
                *entry = average(group);
            }
        }
    }
    palette
}

fn linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// Makes the model at `path` (a `.gltf`, `.glb`, `.fbx` or `.boxmodel`,
/// read through `read` as an import reads it, its `.meta`'s settings and
/// all, but no LODs) into a print as `settings` say. A part named in
/// `pictures` (`material0`) is coloured from that picture in place of its
/// own; a part with neither, from its material's colour.
pub fn print_model(
    path: &str,
    read: &mut crate::gltf_import::ReadFile,
    settings: &VoxelPrintSettings,
    pictures: HashMap<String, Picture>,
) -> Result<VoxelPrint> {
    let meta = voxear_assets::meta_path(std::path::Path::new(path)).to_string_lossy().into_owned();
    let import = read(&meta).map(|bytes| voxear_assets::ModelImport::from_meta(&String::from_utf8_lossy(&bytes)));
    let import =
        voxear_assets::ModelImport { lods: false, own_lods: false, materials: true, ..import.unwrap_or_default() };
    let model = crate::gltf_import::prepare_with(path, read, &import)?;
    let (surface, paints) = surface_of(&model, pictures);
    voxelize(&surface, &paints, settings)
}

/// A model's triangles, each part placed where the model puts it, and a
/// paint for each of its materials, then one for parts with none.
fn surface_of(
    model: &crate::gltf_import::PreparedModel,
    mut pictures: HashMap<String, Picture>,
) -> (Surface, Vec<Paint>) {
    use crate::gltf_import::TextureFormat;
    let mut paints: Vec<Paint> = Vec::new();
    let mut paint_of: HashMap<Option<String>, u16> = HashMap::new();
    let mut surface = Surface::default();
    for node in &model.nodes {
        let Some((mesh, _)) = model.meshes.get(node.mesh) else { continue };
        let paint = *paint_of.entry(node.material_part.clone()).or_insert_with(|| {
            let material = node.material.and_then(|i| model.materials.get(i));
            let own = material.and_then(|m| m.base_color).and_then(|i| model.textures.get(i));
            let own =
                own.filter(|t| t.format == TextureFormat::Rgba8).map(|t| Picture::new(t.width, t.height, &t.mips[0]));
            let picture = node.material_part.as_ref().and_then(|part| pictures.remove(part)).or(own);
            paints.push(Paint {
                colour: material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.desc.base_color),
                picture,
                cutoff: material.and_then(|m| m.desc.alpha_cutoff),
            });
            (paints.len() - 1) as u16
        });
        // A skinned part's points are already the model's own.
        let place = match node.skin {
            Some(_) => Mat4::IDENTITY,
            None => Mat4::from_scale_rotation_translation(node.scale, node.rotation, node.position),
        };
        let points: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
        let uvs: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.uv).collect();
        surface.add(place, &points, &uvs, &mesh.indices, paint);
    }
    (surface, paints)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed box, `half` metres each way from its middle at `middle`, its
    /// top half one colour and its bottom another, from a picture of two.
    fn closed_box(middle: Vec3, half: Vec3) -> Surface {
        let mut surface = Surface::default();
        let corners: Vec<[f32; 3]> = (0..8)
            .map(|i| {
                let s = Vec3::new(
                    if i & 1 == 0 { -1.0 } else { 1.0 },
                    if i & 2 == 0 { -1.0 } else { 1.0 },
                    if i & 4 == 0 { -1.0 } else { 1.0 },
                );
                (middle + s * half).to_array()
            })
            .collect();
        // The picture's first row, at V 0, colours the top.
        let uvs: Vec<[f32; 2]> = (0..8).map(|i| [0.5, if i & 2 == 0 { 0.9 } else { 0.1 }]).collect();
        let faces = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut indices = Vec::new();
        for [a, b, c, d] in faces {
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
        surface.add(Mat4::IDENTITY, &corners, &uvs, &indices, 0);
        surface
    }

    fn two_colours() -> Paint {
        // A picture two pixels tall: red over blue.
        let rgba = [200, 30, 30, 255, 30, 30, 200, 255];
        Paint { colour: [1.0; 4], picture: Some(Picture::new(1, 2, &rgba)), cutoff: None }
    }

    #[test]
    fn a_closed_box_is_solid_all_through_and_coloured_from_its_picture() {
        let surface = closed_box(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.5, 0.5, 0.25));
        let settings = VoxelPrintSettings { voxels_per_metre: 16, ..VoxelPrintSettings::default() };
        let print = voxelize(&surface, &[two_colours()], &settings).unwrap();
        // A metre by a metre by half a metre, a voxel of room round it.
        assert_eq!(print.size, [18, 18, 10]);
        assert_eq!(print.origin, Vec3::new(9.0, 1.0, 5.0));
        let at = |p: Vec3| print.get((print.origin + p * 16.0).floor().as_ivec3().to_array());
        // Deep inside, and at the skin, solid; out of it, nothing.
        assert_ne!(at(Vec3::new(0.0, 0.5, 0.0)), 0, "its middle");
        assert_ne!(at(Vec3::new(0.45, 0.95, 0.2)), 0, "near a corner");
        assert_eq!(at(Vec3::new(0.0, 1.1, 0.0)), 0, "over it");
        // Every voxel of a box 16 by 16 by 8 is solid, with those the skin's faces pass through.
        assert!(print.solid >= 16 * 16 * 8, "{}", print.solid);
        assert!(print.solid <= 18 * 18 * 10);
        // Its top is red, its bottom blue, and its middle the colour of the skin nearest.
        let colour = |v: u8| print.palette[usize::from(v - 1)];
        let top = colour(at(Vec3::new(0.0, 0.97, 0.0)));
        let bottom = colour(at(Vec3::new(0.0, 0.03, 0.0)));
        assert!(top[0] > 150 && top[2] < 80, "{top:?}");
        assert!(bottom[2] > 150 && bottom[0] < 80, "{bottom:?}");
        assert!(print.palette.len() <= 32);
    }

    #[test]
    fn a_shell_keeps_only_the_skin_and_sealed_fills_a_box_open_underneath() {
        let surface = closed_box(Vec3::new(0.0, 0.5, 0.0), Vec3::splat(0.5));
        let shell = VoxelPrintSettings { fill: VoxelPrintFill::Shell, ..VoxelPrintSettings::default() };
        let print = voxelize(&surface, &[Paint::default()], &shell).unwrap();
        let at = |print: &VoxelPrint, p: Vec3| print.get((print.origin + p * 16.0).floor().as_ivec3().to_array());
        assert_eq!(at(&print, Vec3::new(0.0, 0.5, 0.0)), 0, "hollow");
        assert_ne!(at(&print, Vec3::new(0.0, 1.01, 0.0)), 0, "its skin");
        // Without its bottom, the outside comes in: solid leaves it hollow, sealed fills it.
        let mut open = closed_box(Vec3::new(0.0, 0.5, 0.0), Vec3::splat(0.5));
        open.triangles.drain(0..2);
        open.paints.drain(0..2);
        let solid = voxelize(&open, &[Paint::default()], &VoxelPrintSettings::default()).unwrap();
        assert_eq!(at(&solid, Vec3::new(0.0, 0.5, 0.0)), 0, "open underneath");
        let sealed = VoxelPrintSettings { fill: VoxelPrintFill::Sealed, ..VoxelPrintSettings::default() };
        let sealed = voxelize(&open, &[Paint::default()], &sealed).unwrap();
        assert_ne!(at(&sealed, Vec3::new(0.0, 0.5, 0.0)), 0, "sealed");
    }

    #[test]
    fn a_thin_slanted_triangle_leaves_no_gap_the_outside_comes_through() {
        // A tetrahedron with long thin faces: solid inside all the same.
        let mut surface = Surface::default();
        let corners = [[0.0, 0.0, 0.0], [1.7, 0.1, 0.05], [0.1, 1.3, 0.2], [0.2, 0.15, 1.1]];
        let indices = [0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];
        surface.add(Mat4::IDENTITY, &corners, &[], &indices, 0);
        let print = voxelize(&surface, &[Paint::default()], &VoxelPrintSettings::default()).unwrap();
        let at = |p: Vec3| print.get((print.origin + p * 16.0).floor().as_ivec3().to_array());
        assert_ne!(at(Vec3::new(0.3, 0.3, 0.2)), 0);
    }

    #[test]
    fn a_holes_pixels_leave_no_voxels_and_a_model_too_large_says_so() {
        let mut paint = Paint::default();
        paint.picture = Some(Picture::new(1, 1, &[255, 255, 255, 0]));
        paint.cutoff = Some(0.5);
        let surface = closed_box(Vec3::ZERO, Vec3::splat(0.5));
        assert!(voxelize(&surface, &[paint], &VoxelPrintSettings::default()).is_err());
        let huge = closed_box(Vec3::ZERO, Vec3::splat(100.0));
        let error = voxelize(&huge, &[Paint::default()], &VoxelPrintSettings::default()).unwrap_err();
        assert!(format!("{error}").contains("too large"), "{error}");
    }

    #[test]
    fn many_colours_are_made_the_nearest_of_a_few() {
        let mut counts = HashMap::new();
        for r in 0..32u8 {
            counts.insert([r * 8, 100, 50], 10);
            counts.insert([30, r * 8, 200], 5);
        }
        let palette = quantize(&counts, 6);
        assert_eq!(palette.len(), 6);
        // Both runs are kept: some colours blue and some not.
        assert!(palette.iter().any(|c| c[2] > 150) && palette.iter().any(|c| c[2] < 80), "{palette:?}");
    }
}
