//! The lightmapper: bakes the light falling on the level's still Objs into
//! lightmaps, and on its light probes into spherical harmonics, by tracing
//! paths on the GPU through the scene the GI's rays see (`shaders/light_bake.slang`),
//! by ray queries where the GPU has them and in a compute shader elsewhere.
//!
//! A bake goes in four steps:
//!
//! 1. **Lay out** ([`prepare`]): each part with a Lightmap that receives is
//!    given a square of texels, as many across as its lightmap UVs' square
//!    covers at the level's texels a metre times its Obj's scale, and the
//!    squares are packed in rows into lightmaps up to `lightmap_size`
//!    across, two texels apart. Each part's triangles are drawn into its
//!    square on the CPU: every texel whose middle a triangle covers, then
//!    every texel a triangle only touches, at the nearest point of it, so
//!    texels a lightmap's filtering reads at a chart's edge hold light from
//!    the surface rather than from beyond it. Each texel keeps where it is,
//!    its normals and how wide it is, in a list the GPU walks.
//! 2. **Trace**, a few dispatches a frame, as the frame's scene is built
//!    anyway: a fixed number of paths from each texel and twice as many from
//!    each probe each time, added to sums on the GPU. The work is cut the
//!    same way whatever the frame rate, and every path's random numbers come
//!    from its texel and number, so the same scene bakes the same.
//! 3. **Read back** the sums once all are traced.
//! 4. **Finish** ([`finish`], on a thread of its own): each texel's light is
//!    its sum over its paths; texels whose paths mostly met the backs of
//!    surfaces lie inside something and are dropped. The light is smoothed
//!    by an edge-stopping à-trous filter (Dammertz et al. 2010) across texels
//!    of like surfaces only, the dropped and empty texels round each chart
//!    are filled from their neighbours (dilation), so filtering at a chart's
//!    edge reads no black, and each lightmap is compressed to BC6H, and its
//!    directions to BC7. Probes inside something are dropped.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use ash::vk;
use voxear_core::math::{Mat3, Mat4, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_rhi::{Buffer, Gpu, MemoryLocation};

use crate::gpu::{GpuBakeJob, GpuBakeTexel};
use crate::lightmaps::{BakedAtlas, BakedProbes, LightmapPlace, ProbeLight};
use crate::texture_codec;

/// Texels between parts in a lightmap, and between a part and the edge.
const PART_GAP: u32 = 2;
/// The fewest texels across a part's square.
const LEAST_PART: u32 = 8;
/// Paths a dispatch traces at most, texels times paths each, so no dispatch
/// keeps the GPU long.
pub const PATHS_PER_DISPATCH: u32 = 1 << 19;
/// How many dispatches a frame may record.
pub const MOST_DISPATCHES: usize = 16;
/// Of a texel's paths, the share that first meets the backs of surfaces from
/// which it is taken to lie inside something.
const INSIDE: f32 = 0.4;
/// The same for a probe.
const PROBE_INSIDE: f32 = 0.25;
/// The filter's passes, each reading twice as far as the last.
const FILTER_PASSES: u32 = 4;
/// How far round each chart empty texels are filled.
const DILATE_TEXELS: u32 = 6;

/// A mesh's finest triangles, as a bake reads them: each vertex's point,
/// normal and lightmap UVs in the mesh's space, and three indices a triangle.
#[derive(Clone, Debug, Default)]
pub struct BakeMesh {
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl BakeMesh {
    /// Its surface and its lightmap UVs' areas, once placed by `model`.
    fn areas(&self, model: &Mat4) -> (f32, f32) {
        let mut surface = 0.0;
        let mut uv = 0.0;
        for t in self.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| t[i] as usize);
            let p = |i: usize| model.transform_point3(self.points[i]);
            surface += (p(b) - p(a)).cross(p(c) - p(a)).length() * 0.5;
            let u = |i: usize| Vec2::from(self.uvs[i]);
            uv += ((u(b) - u(a)).perp_dot(u(c) - u(a)) * 0.5).abs();
        }
        (surface, uv)
    }
}

/// What a bake asks for, from the level's `[post.global_illumination]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeSettings {
    pub texels_per_metre: f32,
    pub bounces: u32,
    pub samples: u32,
    pub directional: bool,
    pub denoise: bool,
    pub lightmap_size: u32,
}

impl BakeSettings {
    pub fn of(gi: &crate::GlobalIllumination) -> Self {
        Self {
            texels_per_metre: gi.texels_per_metre.clamp(0.01, 1000.0),
            bounces: gi.bounces.clamp(1, 16),
            samples: gi.samples.clamp(1, 1 << 16),
            directional: gi.directional,
            denoise: gi.denoise,
            lightmap_size: gi.lightmap_size.clamp(64, 8192).next_multiple_of(4),
        }
    }
}

/// A part a bake gives a lightmap: its Obj and which part, its mesh's
/// triangles, where it is, and its Obj's lightmap scale.
#[derive(Clone)]
pub struct BakePart {
    pub entity: Entity,
    pub part: u32,
    pub mesh: Arc<BakeMesh>,
    pub model: Mat4,
    pub scale: f32,
}

/// A bake laid out: the texels to trace, in lightmap order, where each is,
/// the lightmaps' sizes, each part's place, and the probes.
pub struct Layout {
    pub texels: Vec<GpuBakeTexel>,
    /// Each texel's lightmap and place in it.
    pub at: Vec<(u32, u32, u32)>,
    pub sizes: Vec<(u32, u32)>,
    pub places: Vec<(Entity, u32, LightmapPlace)>,
    pub probes: Vec<Vec3>,
    pub settings: BakeSettings,
}

/// A part's square of texels in a lightmap.
struct Square {
    part: usize,
    side: u32,
    atlas: u32,
    x: u32,
    y: u32,
}

/// Lays a bake out: gives each part its square in a lightmap and draws its
/// triangles into it (step 1).
pub fn prepare(parts: &[BakePart], probes: Vec<Vec3>, settings: BakeSettings) -> Layout {
    let most = settings.lightmap_size - 2 * PART_GAP;
    let mut squares: Vec<Square> = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let (surface, uv) = part.mesh.areas(&part.model);
            let across = (surface / uv.max(1e-9)).sqrt() * settings.texels_per_metre * part.scale.max(0.0);
            let side = (across.ceil() as u32).clamp(LEAST_PART, most);
            Square { part: index, side, atlas: 0, x: 0, y: 0 }
        })
        .collect();
    let mut order: Vec<usize> = (0..squares.len()).collect();
    order.sort_by(|&a, &b| {
        let key = |s: &Square| (std::cmp::Reverse(s.side), parts[s.part].entity.to_bits(), parts[s.part].part);
        key(&squares[a]).cmp(&key(&squares[b]))
    });
    // Rows, tallest first, each lightmap filled before the next is begun.
    let size = settings.lightmap_size;
    let (mut atlas, mut x, mut y, mut row) = (0u32, PART_GAP, PART_GAP, 0u32);
    let mut used: Vec<(u32, u32)> = vec![(0, 0)];
    for &index in &order {
        let side = squares[index].side;
        if x + side + PART_GAP > size {
            (x, y, row) = (PART_GAP, y + row + PART_GAP, 0);
        }
        if y + side + PART_GAP > size {
            (atlas, x, y, row) = (atlas + 1, PART_GAP, PART_GAP, 0);
            used.push((0, 0));
        }
        let square = &mut squares[index];
        (square.atlas, square.x, square.y) = (atlas, x, y);
        let reach = &mut used[atlas as usize];
        *reach = (reach.0.max(x + side + PART_GAP), reach.1.max(y + side + PART_GAP));
        x += side + PART_GAP;
        row = row.max(side);
    }
    if parts.is_empty() {
        used.clear();
    }
    // Each lightmap only as large as its parts need, in whole blocks of four as BC6H keeps them.
    let sizes: Vec<(u32, u32)> = used
        .iter()
        .map(|&(w, h)| (w.next_multiple_of(4).min(size).max(4), h.next_multiple_of(4).min(size).max(4)))
        .collect();

    // Each lightmap's texels, by the index of the texel drawn there plus one.
    let mut grids: Vec<Vec<u32>> = sizes.iter().map(|&(w, h)| vec![0; (w * h) as usize]).collect();
    let mut drawn: Vec<GpuBakeTexel> = Vec::new();
    for conservative in [false, true] {
        for square in &squares {
            let part = &parts[square.part];
            let (width, _) = sizes[square.atlas as usize];
            draw(part, square, width, &mut grids[square.atlas as usize], &mut drawn, conservative);
        }
    }
    // In lightmap order, so neighbours trace together.
    let mut texels = Vec::with_capacity(drawn.len());
    let mut at = Vec::with_capacity(drawn.len());
    for (atlas, grid) in grids.iter().enumerate() {
        let width = sizes[atlas].0;
        for (i, &cell) in grid.iter().enumerate() {
            if cell > 0 {
                texels.push(drawn[cell as usize - 1]);
                at.push((atlas as u32, i as u32 % width, i as u32 / width));
            }
        }
    }
    let places = squares
        .iter()
        .map(|square| {
            let (w, h) = sizes[square.atlas as usize];
            let (w, h) = (w as f32, h as f32);
            let side = square.side as f32;
            let place = LightmapPlace {
                atlas: square.atlas,
                scale_offset: [side / w, side / h, square.x as f32 / w, square.y as f32 / h],
            };
            (parts[square.part].entity, parts[square.part].part, place)
        })
        .collect();
    Layout { texels, at, sizes, places, probes, settings }
}

/// Draws a part's triangles into its square: the texels whose middles they
/// cover, or with `conservative`, the texels left that they only touch, at
/// the nearest point of them.
fn draw(
    part: &BakePart,
    square: &Square,
    width: u32,
    grid: &mut [u32],
    texels: &mut Vec<GpuBakeTexel>,
    conservative: bool,
) {
    let mesh = &part.mesh;
    let normal_matrix = Mat3::from_mat4(part.model).inverse().transpose();
    let side = square.side as f32;
    let (surface, uv) = mesh.areas(&part.model);
    let texel_width = (surface / uv.max(1e-9)).sqrt() / side;
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| t[i] as usize);
        let uvs = [a, b, c].map(|i| Vec2::from(mesh.uvs[i]) * side);
        let points = [a, b, c].map(|i| part.model.transform_point3(mesh.points[i]));
        let normals = [a, b, c].map(|i| (normal_matrix * mesh.normals[i]).normalize_or_zero());
        let face = (points[1] - points[0]).cross(points[2] - points[0]);
        if face.length_squared() < 1e-20 {
            continue;
        }
        let mut face = face.normalize();
        let smooth = (normals[0] + normals[1] + normals[2]).normalize_or(face);
        // A turned-out model's winding runs the other way: its normals say which way it faces.
        if face.dot(smooth) < 0.0 {
            face = -face;
        }
        let area = (uvs[1] - uvs[0]).perp_dot(uvs[2] - uvs[0]);
        if area.abs() < 1e-12 {
            continue;
        }
        let reach = if conservative { 1.0 } else { 0.0 };
        let low = (uvs[0].min(uvs[1]).min(uvs[2]) - reach).floor().max(Vec2::ZERO);
        let high = (uvs[0].max(uvs[1]).max(uvs[2]) + reach).ceil().min(Vec2::splat(side));
        for y in low.y as u32..high.y as u32 {
            for x in low.x as u32..high.x as u32 {
                let cell = ((square.y + y) * width + square.x + x) as usize;
                if grid[cell] != 0 {
                    continue;
                }
                let middle = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let mut weights = barycentric(uvs, area, middle);
                if weights.min_element() < -1e-5 {
                    if !conservative {
                        continue;
                    }
                    let (nearest, distance) = nearest_on(uvs, middle);
                    if distance > 0.75 {
                        continue;
                    }
                    weights = barycentric(uvs, area, nearest).max(Vec3::ZERO);
                    weights /= weights.element_sum().max(1e-9);
                }
                let position = points[0] * weights.x + points[1] * weights.y + points[2] * weights.z;
                let normal =
                    (normals[0] * weights.x + normals[1] * weights.y + normals[2] * weights.z).normalize_or(face);
                texels.push(GpuBakeTexel {
                    position: [position.x, position.y, position.z, texel_width],
                    normal: [normal.x, normal.y, normal.z, 0.0],
                    face: [face.x, face.y, face.z, 0.0],
                });
                grid[cell] = texels.len() as u32;
            }
        }
    }
}

fn barycentric(corners: [Vec2; 3], area: f32, p: Vec2) -> Vec3 {
    let w0 = (corners[1] - p).perp_dot(corners[2] - p) / area;
    let w1 = (corners[2] - p).perp_dot(corners[0] - p) / area;
    Vec3::new(w0, w1, 1.0 - w0 - w1)
}

/// The nearest point of a triangle to `p`, and how far it is.
fn nearest_on(corners: [Vec2; 3], p: Vec2) -> (Vec2, f32) {
    let mut best = (corners[0], f32::MAX);
    for i in 0..3 {
        let (a, b) = (corners[i], corners[(i + 1) % 3]);
        let along = ((p - a).dot(b - a) / (b - a).length_squared().max(1e-12)).clamp(0.0, 1.0);
        let point = a + (b - a) * along;
        let distance = point.distance(p);
        if distance < best.1 {
            best = (point, distance);
        }
    }
    best
}

/// One dispatch's work: texels from `first_texel`, `count` of them, and
/// paths from `first_sample`, `samples` for each; the probes trace with the
/// first chunk of texels of each round of paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub first_texel: u32,
    pub count: u32,
    pub first_sample: u32,
    pub samples: u32,
    pub probes: bool,
}

/// Every dispatch a bake of `texels` texels at `samples` paths each makes,
/// in order: cut the same way whatever the frame rate, so the sums add up in
/// the same order every time.
pub fn schedule(texels: u32, probes: u32, samples: u32) -> Vec<Dispatch> {
    let chunk = texels.clamp(1, PATHS_PER_DISPATCH);
    let per = (PATHS_PER_DISPATCH / chunk).clamp(1, samples.max(1));
    let mut dispatches = Vec::new();
    let mut first_sample = 0;
    while first_sample < samples {
        let round = per.min(samples - first_sample);
        let mut first_texel = 0;
        loop {
            let count = chunk.min(texels - first_texel);
            dispatches.push(Dispatch {
                first_texel,
                count,
                first_sample,
                samples: round,
                probes: first_texel == 0 && probes > 0,
            });
            first_texel += count;
            if first_texel >= texels {
                break;
            }
        }
        first_sample += round;
    }
    dispatches
}

/// What a bake made: its lightmaps, each part's place in them, its probes,
/// and how long it took.
#[derive(Clone, Debug)]
pub struct LightBake {
    pub atlases: Vec<BakedAtlas>,
    pub places: Vec<(Entity, u32, LightmapPlace)>,
    pub probes: BakedProbes,
    pub texels: u64,
    /// From starting to finishing, and of that tracing on the GPU, in seconds.
    pub seconds: f32,
    pub tracing_seconds: f32,
}

/// Where a bake is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BakeStage {
    Tracing,
    Finishing,
}

/// A bake under way: from 0 to 1, and what it is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeProgress {
    pub fraction: f32,
    pub stage: BakeStage,
}

/// Turns what the GPU summed into a bake's lightmaps and probes (step 4).
/// `light` and `directions` have a row a texel, `probe_light` nine a probe.
pub fn finish(
    layout: &Layout,
    light: &[[f32; 4]],
    directions: &[[f32; 4]],
    probe_light: &[[f32; 4]],
) -> (Vec<BakedAtlas>, BakedProbes) {
    let settings = layout.settings;
    let samples = settings.samples as f32;
    let atlases: Vec<BakedAtlas> = std::thread::scope(|scope| {
        let jobs: Vec<_> = layout
            .sizes
            .iter()
            .enumerate()
            .map(|(atlas, &(width, height))| {
                scope.spawn(move || finish_atlas(layout, atlas as u32, (width, height), (light, directions), samples))
            })
            .collect();
        jobs.into_iter().map(|job| job.join().expect("a lightmap's finishing")).collect()
    });
    let mut probes = BakedProbes::default();
    let paths = samples * 2.0;
    for (index, &position) in layout.probes.iter().enumerate() {
        let Some(rows) = probe_light.get(index * 9..index * 9 + 9) else { break };
        if rows[0][3] > paths * PROBE_INSIDE {
            continue;
        }
        let scale = 4.0 * std::f32::consts::PI / paths;
        let coefficients = std::array::from_fn(|i| [rows[i][0] * scale, rows[i][1] * scale, rows[i][2] * scale]);
        probes.positions.push(position);
        probes.lights.push(ProbeLight::from_radiance(coefficients));
    }
    (atlases, probes)
}

fn finish_atlas(
    layout: &Layout,
    atlas: u32,
    (width, height): (u32, u32),
    (light, directions): (&[[f32; 4]], &[[f32; 4]]),
    samples: f32,
) -> BakedAtlas {
    let cells = (width * height) as usize;
    let mut color = vec![Vec3::ZERO; cells];
    let mut way = vec![[0.5f32, 0.5, 1.0, 1.0]; cells];
    // Which texel each cell holds, for the filter's edges, and whether it is lit.
    let mut texel_of: Vec<Option<usize>> = vec![None; cells];
    let mut lit = vec![false; cells];
    for (index, &(a, x, y)) in layout.at.iter().enumerate() {
        if a != atlas {
            continue;
        }
        let cell = (y * width + x) as usize;
        let [r, g, b, backs] = light[index];
        texel_of[cell] = Some(index);
        if backs > samples * INSIDE {
            continue;
        }
        let paths = (samples - backs).max(1.0);
        color[cell] = Vec3::new(r, g, b) / paths;
        lit[cell] = true;
        let [dx, dy, dz, weight] = directions[index];
        let normal = Vec3::from_slice(&layout.texels[index].normal[..3]);
        let along = if weight > 1e-6 { Vec3::new(dx, dy, dz) / weight } else { normal };
        let along = if along.length() > 1.0 { along.normalize() } else { along };
        let rebalance = along.dot(normal) * 0.5 + 0.5;
        way[cell] = [along.x * 0.5 + 0.5, along.y * 0.5 + 0.5, along.z * 0.5 + 0.5, rebalance.clamp(0.0, 1.0)];
    }
    if layout.settings.denoise {
        color = smooth(layout, (width, height), &color, &texel_of, &lit);
    }
    let (color, way) = dilate((width, height), color, way, lit);
    let pixels: Vec<[f32; 4]> = color.iter().map(|c| [c.x, c.y, c.z, 1.0]).collect();
    let light = texture_codec::encode_bc6h(&pixels, width, height);
    let directions = layout.settings.directional.then(|| {
        let rgba: Vec<u8> = way.iter().flat_map(|w| w.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
        texture_codec::encode_bc7(&rgba, width, height)
    });
    BakedAtlas { width, height, light, directions }
}

/// The edge-stopping à-trous filter: each pass blends each lit texel with
/// lit texels round it, twice as far apart as the last pass, weighed by a
/// B3 spline and by how alike their surfaces are: facing the same way, on
/// the same plane, near in the world, and not much brighter or darker.
fn smooth(
    layout: &Layout,
    (width, height): (u32, u32),
    color: &[Vec3],
    texel_of: &[Option<usize>],
    lit: &[bool],
) -> Vec<Vec3> {
    const KERNEL: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];
    let texel = |cell: usize| texel_of[cell].map(|i| &layout.texels[i]);
    let mut now = color.to_vec();
    for pass in 0..FILTER_PASSES {
        let step = 1i32 << pass;
        let mut next = now.clone();
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let cell = (y * width as i32 + x) as usize;
                let Some(me) = texel(cell).filter(|_| lit[cell]) else { continue };
                let position = Vec3::from_slice(&me.position[..3]);
                let normal = Vec3::from_slice(&me.normal[..3]);
                let reach = me.position[3] * step as f32 * 3.0;
                let brightness = now[cell].max_element();
                let (mut sum, mut total) = (Vec3::ZERO, 0.0);
                for (j, kj) in KERNEL.iter().enumerate() {
                    for (i, ki) in KERNEL.iter().enumerate() {
                        let (qx, qy) = (x + (i as i32 - 2) * step, y + (j as i32 - 2) * step);
                        if qx < 0 || qy < 0 || qx >= width as i32 || qy >= height as i32 {
                            continue;
                        }
                        let other = (qy * width as i32 + qx) as usize;
                        let Some(them) = texel(other).filter(|_| lit[other]) else { continue };
                        let apart = Vec3::from_slice(&them.position[..3]) - position;
                        let facing = normal.dot(Vec3::from_slice(&them.normal[..3])).max(0.0).powi(16);
                        let off_plane = (apart.dot(normal).abs() / me.position[3].max(1e-4)).min(8.0);
                        let near = (1.0 - apart.length() / reach.max(1e-4)).max(0.0);
                        let difference = (now[other].max_element() - brightness).abs()
                            / (brightness.max(now[other].max_element()) + 1e-3);
                        let weight = ki * kj * facing * (-off_plane).exp() * near * (-difference * 2.0).exp();
                        sum += now[other] * weight;
                        total += weight;
                    }
                }
                if total > 0.0 {
                    next[cell] = sum / total;
                }
            }
        }
        now = next;
    }
    now
}

/// Fills empty texels from their lit neighbours, ring by ring out to
/// `DILATE_TEXELS`, so filtering at a chart's edge reads the chart's light.
fn dilate(
    (width, height): (u32, u32),
    mut color: Vec<Vec3>,
    mut way: Vec<[f32; 4]>,
    mut lit: Vec<bool>,
) -> (Vec<Vec3>, Vec<[f32; 4]>) {
    for _ in 0..DILATE_TEXELS {
        let mut filled = Vec::new();
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let cell = (y * width as i32 + x) as usize;
                if lit[cell] {
                    continue;
                }
                let (mut sum, mut ways, mut count) = (Vec3::ZERO, [0.0f32; 4], 0.0);
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    let (qx, qy) = (x + dx, y + dy);
                    if qx < 0 || qy < 0 || qx >= width as i32 || qy >= height as i32 {
                        continue;
                    }
                    let other = (qy * width as i32 + qx) as usize;
                    if lit[other] {
                        sum += color[other];
                        for (w, o) in ways.iter_mut().zip(way[other]) {
                            *w += o;
                        }
                        count += 1.0;
                    }
                }
                if count > 0.0 {
                    filled.push((cell, sum / count, ways.map(|w| w / count)));
                }
            }
        }
        if filled.is_empty() {
            break;
        }
        for (cell, value, w) in filled {
            (color[cell], way[cell], lit[cell]) = (value, w, true);
        }
    }
    (color, way)
}

/// The GPU's side of a bake under way: the texels and probes, the sums, and
/// a few dispatches' work for each frame in flight.
struct Traced {
    texels: Buffer,
    light: Buffer,
    directions: Buffer,
    probes: Buffer,
    probe_light: Buffer,
    /// A buffer of `MOST_DISPATCHES` jobs for each frame in flight.
    jobs: Vec<Buffer>,
}

/// A bake the renderer runs, frame by frame.
pub(crate) struct Baking {
    layout: Arc<Layout>,
    traced: Option<Traced>,
    schedule: Vec<Dispatch>,
    /// The next dispatch to record, and the frame the last one was recorded in.
    next: usize,
    last_frame: u64,
    started: std::time::Instant,
    traced_for: f32,
    finishing: Option<std::thread::JoinHandle<(Vec<BakedAtlas>, BakedProbes)>>,
    pub done: Option<Result<LightBake>>,
}

/// This frame's dispatches, as the first view records them: its jobs'
/// buffer, and each dispatch's job number, texel count and whether it
/// traces the probes.
#[derive(Clone, Debug)]
pub(crate) struct BakeFrame {
    pub jobs: u64,
    pub dispatches: Vec<(u32, Dispatch)>,
    pub probes: u32,
}

impl Baking {
    /// Lays a bake out and uploads its texels and probes.
    pub(crate) fn start(gpu: &Gpu, layout: Layout, slots: usize) -> Result<Self> {
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let read = storage | vk::BufferUsageFlags::TRANSFER_SRC;
        let texel_count = layout.texels.len().max(1) as u64;
        let probe_count = layout.probes.len().max(1) as u64;
        let local = MemoryLocation::GpuOnly;
        let texels =
            gpu.create_buffer("bake texels", texel_count * size_of::<GpuBakeTexel>() as u64, storage, local)?;
        gpu.upload_buffer(&texels, 0, bytemuck::cast_slice(&layout.texels))?;
        let zeroes = |name: &str, rows: u64| -> Result<Buffer> {
            let buffer = gpu.create_buffer(name, rows * 16, read, local)?;
            gpu.upload_buffer(&buffer, 0, &vec![0u8; (rows * 16) as usize])?;
            Ok(buffer)
        };
        let light = zeroes("bake light", texel_count)?;
        let directions = zeroes("bake directions", texel_count)?;
        let positions: Vec<[f32; 4]> = layout.probes.iter().map(|p| [p.x, p.y, p.z, 0.0]).collect();
        let probes = gpu.create_buffer("bake probes", probe_count * 16, storage, local)?;
        gpu.upload_buffer(&probes, 0, bytemuck::cast_slice(&positions))?;
        let probe_light = zeroes("bake probe light", probe_count * 9)?;
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let jobs = (0..slots)
            .map(|slot| {
                let size = (MOST_DISPATCHES * size_of::<GpuBakeJob>()) as u64;
                gpu.create_buffer(&format!("bake jobs {slot}"), size, usage, MemoryLocation::CpuToGpu)
            })
            .collect::<Result<Vec<_>>>()?;
        let schedule = schedule(layout.texels.len() as u32, layout.probes.len() as u32, layout.settings.samples);
        Ok(Self {
            layout: Arc::new(layout),
            traced: Some(Traced { texels, light, directions, probes, probe_light, jobs }),
            schedule,
            next: 0,
            last_frame: 0,
            started: std::time::Instant::now(),
            traced_for: 0.0,
            finishing: None,
            done: None,
        })
    }

    pub(crate) fn tracing(&self) -> bool {
        self.traced.is_some() && self.next < self.schedule.len()
    }

    pub(crate) fn progress(&self) -> BakeProgress {
        let total = self.schedule.len().max(1) as f32;
        match self.finishing.is_some() || self.done.is_some() {
            true => BakeProgress { fraction: 0.95, stage: BakeStage::Finishing },
            false => BakeProgress { fraction: self.next as f32 / total * 0.9, stage: BakeStage::Tracing },
        }
    }

    /// The dispatches this frame records, at most `pace`, their jobs written
    /// into this frame's buffer.
    pub(crate) fn frame(&mut self, slot: usize, pace: usize) -> Option<BakeFrame> {
        let traced = self.traced.as_ref()?;
        if self.next >= self.schedule.len() {
            return None;
        }
        let end = (self.next + pace.clamp(1, MOST_DISPATCHES)).min(self.schedule.len());
        let dispatches: Vec<(u32, Dispatch)> =
            self.schedule[self.next..end].iter().enumerate().map(|(i, d)| (i as u32, *d)).collect();
        let jobs: Vec<GpuBakeJob> = dispatches
            .iter()
            .map(|(_, d)| GpuBakeJob {
                texels: traced.texels.address,
                light: traced.light.address,
                directions: traced.directions.address,
                probes: traced.probes.address,
                probe_light: traced.probe_light.address,
                texel_count: self.layout.texels.len() as u32,
                probe_count: self.layout.probes.len() as u32,
                first_texel: d.first_texel,
                first_sample: d.first_sample,
                samples: d.samples,
                bounces: self.layout.settings.bounces,
                pad: [0; 2],
            })
            .collect();
        let buffer = &traced.jobs[slot % traced.jobs.len()];
        buffer.write(0, &jobs);
        Some(BakeFrame { jobs: buffer.address, dispatches, probes: self.layout.probes.len() as u32 })
    }

    /// Counts this frame's dispatches recorded.
    pub(crate) fn recorded(&mut self, count: usize, frame: u64) {
        self.next += count;
        self.last_frame = frame;
    }

    /// Once every dispatch has been traced, reads the sums back and finishes
    /// on a thread of its own; once that is done, keeps what it made.
    pub(crate) fn update(&mut self, gpu: &Gpu, frame: u64) {
        if self.next >= self.schedule.len() && frame > self.last_frame + 1 && self.traced.is_some() {
            gpu.wait_idle();
            self.traced_for = self.started.elapsed().as_secs_f32();
            let traced = self.traced.take().expect("looked at above");
            let read = || -> Result<(Vec<[f32; 4]>, Vec<[f32; 4]>, Vec<[f32; 4]>)> {
                let rows = |buffer: &Buffer, count: usize| -> Result<Vec<[f32; 4]>> {
                    let bytes = gpu.read_buffer(buffer, 0, (count.max(1) * 16) as u64)?;
                    Ok(bytemuck::pod_collect_to_vec::<u8, [f32; 4]>(&bytes)[..count].to_vec())
                };
                let texels = self.layout.texels.len();
                Ok((
                    rows(&traced.light, texels)?,
                    rows(&traced.directions, texels)?,
                    rows(&traced.probe_light, self.layout.probes.len() * 9)?,
                ))
            };
            let sums = read().context("reading a lightmap bake back");
            destroy(gpu, traced);
            match sums {
                Ok((light, directions, probe_light)) => {
                    let layout = self.layout.clone();
                    self.finishing =
                        Some(std::thread::spawn(move || finish(&layout, &light, &directions, &probe_light)));
                }
                Err(error) => self.done = Some(Err(error)),
            }
        }
        if self.finishing.as_ref().is_some_and(|thread| thread.is_finished()) {
            let made = self.finishing.take().expect("looked at above").join();
            self.done = Some(match made {
                Ok((atlases, probes)) => Ok(LightBake {
                    atlases,
                    places: self.layout.places.clone(),
                    probes,
                    texels: self.layout.texels.len() as u64,
                    seconds: self.started.elapsed().as_secs_f32(),
                    tracing_seconds: self.traced_for,
                }),
                Err(_) => Err(anyhow::anyhow!("finishing a lightmap bake failed")),
            });
        }
    }

    /// Frees what it holds on the GPU, once frames in flight are done.
    pub(crate) fn cancel(mut self, gpu: &Gpu) {
        if let Some(traced) = self.traced.take() {
            gpu.wait_idle();
            destroy(gpu, traced);
        }
    }
}

/// Records this frame's dispatches of a bake, with `pipelines` the texels'
/// and the probes', after the scene its rays trace is built. Each adds to
/// the sums the last left, so each waits for the one before, this frame's or
/// the last's.
pub(crate) fn record(
    fg: &mut crate::graph::FrameGraph<'_>,
    pipelines: [vk::Pipeline; 2],
    push: crate::gpu::Push,
    bake: &BakeFrame,
) {
    let dispatches = bake.dispatches.clone();
    let probes = bake.probes;
    fg.add_pass("Lightmap bake", &[], &[], move |ctx| {
        for (job, dispatch) in &dispatches {
            barrier(ctx);
            let traced = crate::gpu::Push { level: *job, size: [dispatch.count, 0], ..push };
            crate::passes::bind(ctx, pipelines[0], vk::PipelineBindPoint::COMPUTE, &traced);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, dispatch.count.div_ceil(64), 1, 1) };
            if dispatch.probes && probes > 0 {
                crate::passes::bind(ctx, pipelines[1], vk::PipelineBindPoint::COMPUTE, &traced);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, probes, 1, 1) };
            }
        }
        barrier(ctx);
    });
}

fn barrier(ctx: &crate::graph::PassContext) {
    let barrier = [vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ | vk::AccessFlags2::SHADER_STORAGE_WRITE)];
    let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
    unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
}

fn destroy(gpu: &Gpu, traced: Traced) {
    let Traced { texels, light, directions, probes, probe_light, jobs } = traced;
    for buffer in [texels, light, directions, probes, probe_light].into_iter().chain(jobs) {
        gpu.destroy_buffer(buffer);
    }
}

/// Each mesh's triangles read once for a bake, as several parts share them.
#[derive(Default)]
pub(crate) struct MeshCache(HashMap<crate::MeshHandle, Option<Arc<BakeMesh>>>);

impl MeshCache {
    pub(crate) fn get(
        &mut self,
        gpu: &Gpu,
        assets: &crate::assets::Assets,
        mesh: crate::MeshHandle,
    ) -> Result<Option<Arc<BakeMesh>>> {
        if let Some(kept) = self.0.get(&mesh) {
            return Ok(kept.clone());
        }
        let read = assets.lightmap_triangles(gpu, mesh)?.map(Arc::new);
        self.0.insert(mesh, read.clone());
        Ok(read)
    }
}

/// The parts of the world a bake gives lightmaps, the probes it bakes, and
/// its settings: every Obj with a Lightmap that receives and a Mesh Renderer
/// or Model Renderer whose meshes have lightmap UVs, switched on.
pub(crate) fn gather(
    world: &World,
    (gpu, assets): (&Gpu, &crate::assets::Assets),
) -> Result<(Vec<BakePart>, Vec<Vec3>, BakeSettings)> {
    use voxear_scene::{Destroying, Switch, Switched, Transform, WorldTransform};
    let switched = Switched::of(world);
    let post = world.resource::<crate::PostProcess>().copied().unwrap_or_default();
    let settings = BakeSettings::of(&post.global_illumination);
    let matrix = |transform: &Transform, placed: Option<&WorldTransform>| {
        placed.map_or_else(|| transform.matrix(), |placed| placed.0)
    };
    let mut cache = MeshCache::default();
    let mut parts = Vec::new();
    let meshes = world
        .query_ref::<(Entity, &crate::Lightmap, &crate::MeshRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>();
    for (entity, lightmap, renderer, transform, placed) in meshes {
        if !lightmap.receive || !switched.is_on(world, entity, Switch::Renderer) {
            continue;
        }
        if let Some(mesh) = cache.get(gpu, assets, renderer.mesh)? {
            parts.push(BakePart { entity, part: 0, mesh, model: matrix(transform, placed), scale: lightmap.scale });
        }
    }
    let models = world
        .query_ref::<(Entity, &crate::Lightmap, &crate::ModelRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>();
    for (entity, lightmap, drawn, transform, placed) in models {
        if !lightmap.receive || !switched.is_on(world, entity, Switch::Model) {
            continue;
        }
        let obj = matrix(transform, placed);
        for (index, part) in drawn.parts.iter().enumerate() {
            if let Some(mesh) = cache.get(gpu, assets, part.mesh)? {
                let model = obj * part.transform;
                parts.push(BakePart { entity, part: index as u32, mesh, model, scale: lightmap.scale });
            }
        }
    }
    parts.sort_by_key(|part| (part.entity.to_bits(), part.part));
    let mut probes = Vec::new();
    let groups = world
        .query_ref::<(Entity, &crate::LightProbeGroup, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>();
    let mut groups: Vec<_> =
        groups.into_iter().filter(|(entity, ..)| switched.is_on(world, *entity, Switch::LightProbeGroup)).collect();
    groups.sort_by_key(|(entity, ..)| entity.to_bits());
    for (_, group, transform, placed) in groups {
        probes.extend(group.probes(&matrix(transform, placed)));
    }
    Ok((parts, probes, settings))
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    fn quad() -> Arc<BakeMesh> {
        let mut data = crate::MeshData::plane();
        crate::lightmap_uvs::generate(&mut data);
        Arc::new(BakeMesh {
            points: data.vertices.iter().map(|v| Vec3::from(v.position)).collect(),
            normals: data.vertices.iter().map(|v| Vec3::from(v.normal)).collect(),
            uvs: data.lightmap_uvs.clone(),
            indices: data.indices.clone(),
        })
    }

    fn settings() -> BakeSettings {
        BakeSettings {
            texels_per_metre: 4.0,
            bounces: 2,
            samples: 64,
            directional: true,
            denoise: true,
            lightmap_size: 64,
        }
    }

    fn floor(entity: u32, size: f32) -> BakePart {
        let model = Mat4::from_scale_rotation_translation(Vec3::new(size, 1.0, size), Quat::IDENTITY, Vec3::ZERO);
        BakePart { entity: Entity::from_bits(u64::from(entity) | (1 << 32)), part: 0, mesh: quad(), model, scale: 1.0 }
    }

    #[test]
    fn parts_are_packed_apart_and_their_texels_drawn_once_each() {
        let parts = [floor(1, 4.0), floor(2, 2.0), floor(3, 12.0)];
        let layout = prepare(&parts, vec![Vec3::Y], settings());
        assert_eq!(layout.places.len(), 3);
        assert!(!layout.texels.is_empty());
        let mut seen = std::collections::HashSet::new();
        assert!(layout.at.iter().all(|&at| seen.insert(at)), "each texel drawn once");
        for &(_, x, y) in &layout.at {
            let (w, h) = layout.sizes[0];
            assert!(x < w && y < h);
        }
        // The biggest floor, 12 m at 4 texels a metre, takes about 48 texels across its square's share.
        let big = layout.places.iter().find(|(e, ..)| *e == parts[2].entity).unwrap().2;
        assert!(big.scale_offset[0] * layout.sizes[0].0 as f32 >= 40.0, "{:?}", big);
        for texel in &layout.texels {
            assert!((texel.face[1] - 1.0).abs() < 1e-4, "a floor faces up");
            assert!(
                texel.position[3] > 0.15 && texel.position[3] < 0.4,
                "a texel a quarter metre wide: {}",
                texel.position[3]
            );
        }
    }

    #[test]
    fn parts_too_many_for_one_lightmap_go_on_to_another() {
        let parts: Vec<BakePart> = (0..12).map(|i| floor(i + 1, 6.0)).collect();
        let layout = prepare(&parts, Vec::new(), settings());
        assert!(layout.sizes.len() > 1, "{:?}", layout.sizes);
        assert!(layout.sizes.iter().all(|&(w, h)| w % 4 == 0 && h % 4 == 0 && w <= 64 && h <= 64));
    }

    #[test]
    fn the_work_is_cut_the_same_way_and_covers_every_path_once() {
        let dispatches = schedule(3_000_000, 10, 100);
        assert_eq!(dispatches, schedule(3_000_000, 10, 100));
        let mut paths = 0u64;
        for d in &dispatches {
            paths += u64::from(d.count) * u64::from(d.samples);
            assert!(u64::from(d.count) * u64::from(d.samples) <= u64::from(PATHS_PER_DISPATCH));
        }
        assert_eq!(paths, 3_000_000 * 100);
        assert_eq!(
            dispatches.iter().filter(|d| d.probes).map(|d| d.samples).sum::<u32>(),
            100,
            "the probes every round"
        );
        let small = schedule(1000, 0, 256);
        assert_eq!(small.len(), 1, "a small bake in one go: {small:?}");
    }

    #[test]
    fn a_bake_finishes_into_lightmaps_with_its_charts_dilated_and_its_probes_kept() {
        let parts = [floor(1, 4.0)];
        let layout = prepare(&parts, vec![Vec3::Y, Vec3::NEG_Y], settings());
        let samples = layout.settings.samples as f32;
        let light: Vec<[f32; 4]> = layout.texels.iter().map(|_| [samples * 0.5, samples * 0.25, 0.0, 0.0]).collect();
        let directions: Vec<[f32; 4]> = layout.texels.iter().map(|_| [0.0, samples, 0.0, samples]).collect();
        let mut probes = vec![[0.0; 4]; 18];
        probes[0] = [1.0, 1.0, 1.0, 0.0];
        probes[9] = [0.0, 0.0, 0.0, samples * 2.0];
        let (atlases, baked) = finish(&layout, &light, &directions, &probes);
        assert_eq!(atlases.len(), 1);
        let atlas = &atlases[0];
        assert_eq!(atlas.light.len() as u32, atlas.width * atlas.height, "BC6H: a byte a texel");
        assert!(atlas.directions.is_some());
        assert_eq!(baked.positions, vec![Vec3::Y], "the probe inside something dropped");
        // Decoded, the floor's chart is the light it was given, and the texels round it too.
        let block = texture_codec::decode_bc6h(atlas.light[..16].try_into().unwrap()).unwrap();
        let corner = block[(PART_GAP as usize) * 4 + PART_GAP as usize];
        assert!((corner[0] - 0.5).abs() < 0.03 && (corner[1] - 0.25).abs() < 0.03, "{corner:?}");
        let outside = block[0];
        assert!(outside[0] > 0.4, "dilated over the gap: {outside:?}");
        let again = finish(&layout, &light, &directions, &probes);
        assert_eq!(again.0, atlases, "the same sums finish the same");
    }
}
