//! A world of small voxels, made on the CPU and traced on the GPU, for
//! measuring what voxels a few centimetres across cost: memory, frame time
//! and how fast they are made. A research prototype, not the engine's way of
//! drawing; `docs/research/small-voxels.md` has what it found.
//!
//! The world is a meadow of grass blades and flowers before a forest of
//! round-crowned trees and conifers, with hillsides behind and to the sides,
//! all of voxels `VOXEL` metres across. The shader (`small_voxels.slang`)
//! says how it is kept and walked.
//!
//! ```text
//! cargo run --release -p voxear_rhi --example small_voxels -- [metres across] [voxels a metre] [out folder]
//! ```

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

mod gpu_make;

use anyhow::{Context, Result};
use voxear_rhi::{
    Bindless, Buffer, Gpu, GraphicsPipelineDesc, ImageDesc, MemoryLocation, ShaderModule, create_compute_pipeline,
    create_graphics_pipeline, vk,
};

/// Sectors are 64 voxels a side, bricks 8.
const SECTOR: i32 = 64;
const BRICK: i32 = 8;
const SECTOR_WORDS: usize = 25;
const BRICK_WORDS: usize = 20;
const EMPTY: u32 = u32::MAX;

/// How tall the world is, in metres.
const HEIGHT_METRES: f32 = 96.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuWorld {
    sectors: u64,
    bricks: u64,
    materials: u64,
    palette: u64,
    sector_count: [i32; 3],
    voxel: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuView {
    eye: [f32; 3],
    pixel_angle: f32,
    forward: [f32; 3],
    tan_half: f32,
    right: [f32; 3],
    aspect: f32,
    up: [f32; 3],
    flags: u32,
    sun: [f32; 3],
    lod_scale: f32,
    size: [u32; 2],
    output: u64,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPush {
    world: u64,
    view: u64,
    draw: u64,
}

// Materials, linear colours.
const PALETTE: [[f32; 3]; 22] = [
    [0.0, 0.0, 0.0],
    [0.10, 0.42, 0.06], // 1 grass top
    [0.18, 0.11, 0.06], // 2 dirt
    [0.09, 0.45, 0.05], // 3-6 blades
    [0.14, 0.55, 0.07],
    [0.06, 0.32, 0.04],
    [0.20, 0.52, 0.08],
    [0.30, 0.62, 0.12], // 7 blade tips
    [0.55, 0.02, 0.02], // 8 red
    [0.18, 0.07, 0.60], // 9 purple
    [0.70, 0.72, 0.75], // 10 white
    [0.55, 0.22, 0.45], // 11 pink
    [0.05, 0.30, 0.04], // 12 stem
    [0.10, 0.08, 0.06], // 13 bark
    [0.40, 0.38, 0.35], // 14 pale bark
    [0.08, 0.28, 0.04], // 15 leaves
    [0.12, 0.36, 0.06], // 16
    [0.02, 0.12, 0.04], // 17 needles
    [0.03, 0.17, 0.05], // 18
    [0.06, 0.25, 0.04], // 19 hill grass
    [0.35, 0.30, 0.12], // 20 dry
    [0.30, 0.06, 0.03], // 21 red ground (hillside)
];

fn hash(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - x.floor(), z - z.floor());
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let at = |a, b| unit(hash(ix + a, 0, iz + b, seed));
    let low = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let high = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    low + (high - low) * sz
}

fn fbm(x: f32, z: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut scale = 1.0;
    let mut weight = 0.5;
    for octave in 0..4 {
        sum += value_noise(x * scale, z * scale, seed + octave) * weight;
        scale *= 2.0;
        weight *= 0.5;
    }
    sum / 0.9375
}

/// The scene: a meadow round `camera`, forest past it and hills beyond.
struct Scene {
    voxel: f32,
    across: f32,
    camera: [f32; 2],
}

impl Scene {
    fn ground(&self, x: f32, z: f32) -> f32 {
        let base = 6.0 + 1.4 * fbm(x / 30.0, z / 30.0, 1) + 0.4 * fbm(x / 6.0, z / 6.0, 2);
        let side = smoothstep(25.0, 95.0, (x - self.camera[0]).abs()) * (12.0 + 12.0 * fbm(x / 50.0, z / 50.0, 3));
        let back = smoothstep(self.camera[1] + 90.0, self.camera[1] + 220.0, z)
            * 24.0
            * (0.6 + 0.5 * fbm(x / 70.0, z / 70.0, 4));
        let behind = smoothstep(self.camera[1] - 10.0, self.camera[1] - 60.0, z) * 10.0;
        base + side + back + behind
    }

    /// How much forest grows at a place: little in the meadow before the camera.
    fn forest(&self, x: f32, z: f32) -> f32 {
        let ahead = z - self.camera[1];
        let meadow = smoothstep(55.0, 35.0, ahead)
            * smoothstep(-15.0, 5.0, ahead)
            * smoothstep(40.0, 25.0, (x - self.camera[0]).abs());
        (0.25 + 0.75 * fbm(x / 40.0, z / 40.0, 9)) * (1.0 - meadow)
    }
}

/// A plant that crosses columns: a flower or a tree, in voxels.
#[derive(Clone)]
enum Plant {
    Flower { x: i32, z: i32, base: i32, stem: i32, kind: u8 },
    Round { x: i32, z: i32, base: i32, trunk: i32, crowns: Vec<[i32; 4]> },
    Cone { x: i32, z: i32, base: i32, height: i32, radius: i32 },
}

impl Plant {
    fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        match self {
            Plant::Flower { x, z, base, stem, .. } => ([x - 3, *base, z - 3], [x + 3, base + stem + 7, z + 3]),
            Plant::Round { x, z, base, trunk, crowns } => {
                let mut low = [x - 2, *base, z - 2];
                let mut high = [x + 2, base + trunk, z + 2];
                for c in crowns {
                    for a in 0..3 {
                        low[a] = low[a].min(c[a] - c[3]);
                        high[a] = high[a].max(c[a] + c[3]);
                    }
                }
                (low, high)
            }
            Plant::Cone { x, z, base, height, radius } => {
                ([x - radius, *base, z - radius], [x + radius, base + height, z + radius])
            }
        }
    }

    fn material(&self, p: [i32; 3]) -> u8 {
        match *self {
            Plant::Flower { x, z, base, stem, kind } => {
                let (dx, dy, dz) = (p[0] - x, p[1] - base, p[2] - z);
                let top = stem;
                if dx == 0 && dz == 0 && dy > 0 && dy <= top {
                    return 12;
                }
                let h = hash(p[0], p[1], p[2], 77);
                match kind {
                    // A wide red head, two voxels thick, ragged at its rim.
                    0 => {
                        let r2 = dx * dx + dz * dz;
                        if (dy == top + 1 || dy == top + 2) && r2 <= 8 && (r2 < 5 || unit(h) < 0.6) {
                            return 8;
                        }
                    }
                    // A purple spike.
                    1 => {
                        if (0..=1).contains(&dx)
                            && (0..=1).contains(&dz)
                            && dy > top - 5
                            && dy <= top + 2
                            && unit(h) < 0.85
                        {
                            return 9;
                        }
                    }
                    // A white cube.
                    2 => {
                        if (-1..=1).contains(&dx) && (-1..=1).contains(&dz) && dy > top && dy <= top + 3 {
                            return 10;
                        }
                    }
                    _ => {
                        if dx.abs() + dz.abs() <= 1 && dy == top + 1 {
                            return 11;
                        }
                    }
                }
                0
            }
            Plant::Round { x, z, base, trunk, ref crowns } => {
                if (p[0] - x).abs() <= 1 && (p[2] - z).abs() <= 1 && p[1] >= base && p[1] <= base + trunk {
                    return 14;
                }
                for c in crowns {
                    let (dx, dy, dz) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
                    let d2 = dx * dx + dy * dy + dz * dz;
                    let r = c[3];
                    if d2 <= r * r {
                        // A shell of leaves, the inside left empty: rays rarely get past the shell.
                        let shell = r - 9;
                        if d2 >= shell * shell {
                            let h = hash(p[0], p[1], p[2], 5);
                            if unit(h) < 0.6 {
                                return if h & 1 == 0 { 15 } else { 16 };
                            }
                        }
                        return 0;
                    }
                }
                0
            }
            Plant::Cone { x, z, base, height, radius } => {
                let (dx, dy, dz) = (p[0] - x, p[1] - base, p[2] - z);
                if dx.abs() <= 1 && dz.abs() <= 1 && dy >= 0 && dy < height * 9 / 10 {
                    return 13;
                }
                let f = dy as f32 / height as f32;
                if !(0.18..=1.0).contains(&f) {
                    return 0;
                }
                let tiers = 7.0;
                let g = ((f - 0.18) / 0.82 * tiers).fract();
                let reach = radius as f32 * (1.0 - (f - 0.18) / 0.82) * (1.0 - 0.45 * g) + 2.0;
                let d = ((dx * dx + dz * dz) as f32).sqrt();
                if d <= reach && d >= reach - 9.0 {
                    let h = hash(p[0], p[1], p[2], 6);
                    if unit(h) < 0.62 {
                        return if h & 1 == 0 { 17 } else { 18 };
                    }
                }
                0
            }
        }
    }
}

struct Made {
    sector_count: [i32; 3],
    sectors: Vec<u32>,
    bricks: Vec<u32>,
    materials: Vec<u8>,
    solid: u64,
    plants: usize,
}

/// One sector column's bricks: for each sector, its mask and bricks in order.
struct ColumnOut {
    sectors: Vec<(usize, [u32; 16], Vec<([u32; BRICK_WORDS], Vec<u8>)>)>,
    solid: u64,
}

/// The trees and flowers, placed on jittered grids, how many of them are trees, and each sector column's list of
/// those that reach into it.
fn place(scene: &Scene) -> (Vec<Plant>, usize, Vec<Vec<usize>>) {
    let v = scene.voxel;
    let columns = (scene.across / v) as i32 / SECTOR;
    let to_voxels = |m: f32| (m / v).floor() as i32;
    let mut plants: Vec<Plant> = Vec::new();
    let cell = 6.0;
    let cells = (scene.across / cell) as i32;
    for cz in 0..cells {
        for cx in 0..cells {
            let h = hash(cx, 1, cz, 11);
            let x = (cx as f32 + 0.15 + 0.7 * unit(hash(cx, 2, cz, 11))) * cell;
            let z = (cz as f32 + 0.15 + 0.7 * unit(hash(cx, 3, cz, 11))) * cell;
            if unit(h) > scene.forest(x, z) * 0.9 {
                continue;
            }
            let ground = scene.ground(x, z);
            let (vx, vz, base) = (to_voxels(x), to_voxels(z), to_voxels(ground));
            let r = unit(hash(cx, 4, cz, 11));
            if ground > 14.0 || unit(hash(cx, 5, cz, 11)) < 0.45 {
                let height = to_voxels(12.0 + 14.0 * r);
                plants.push(Plant::Cone { x: vx, z: vz, base, height, radius: to_voxels(2.4 + 1.6 * r) });
            } else {
                let trunk = to_voxels(5.0 + 6.0 * r);
                let top = base + trunk;
                let radius = to_voxels(1.6 + 1.2 * r);
                let mut crowns = vec![[vx, top, vz, radius]];
                for k in 0..3 {
                    let a = unit(hash(cx, 10 + k, cz, 11)) * std::f32::consts::TAU;
                    let off = radius as f32 * 0.7;
                    crowns.push([
                        vx + (a.cos() * off) as i32,
                        top - radius / 2 + (unit(hash(cx, 20 + k, cz, 11)) * radius as f32) as i32,
                        vz + (a.sin() * off) as i32,
                        radius * 3 / 4,
                    ]);
                }
                plants.push(Plant::Round { x: vx, z: vz, base, trunk: trunk + radius / 2, crowns });
            }
        }
    }
    let tree_count = plants.len();
    let flower_cell = 0.5;
    let flower_cells = (scene.across / flower_cell) as i32;
    for cz in 0..flower_cells {
        for cx in 0..flower_cells {
            let x = (cx as f32 + unit(hash(cx, 1, cz, 13))) * flower_cell;
            let z = (cz as f32 + unit(hash(cx, 2, cz, 13))) * flower_cell;
            let patch = fbm(x / 8.0, z / 8.0, 14);
            let chance = 0.35 * smoothstep(0.45, 0.75, patch) * (1.0 - scene.forest(x, z)).max(0.05);
            if unit(hash(cx, 3, cz, 13)) > chance {
                continue;
            }
            let kind = (unit(hash(cx, 4, cz, 13)) * 4.0) as u8;
            let stem = ((0.37 + unit(hash(cx, 5, cz, 13)) * 0.55) / scene.voxel) as i32;
            let ground = to_voxels(scene.ground(x, z));
            plants.push(Plant::Flower { x: to_voxels(x), z: to_voxels(z), base: ground, stem, kind });
        }
    }
    // `SMALL_VOXELS_GROUND_ONLY` leaves the trees and flowers out, to measure what they hold.
    if std::env::var("SMALL_VOXELS_GROUND_ONLY").is_ok() {
        plants.clear();
    }
    let tree_count = tree_count.min(plants.len());
    let mut by_column: Vec<Vec<usize>> = vec![Vec::new(); (columns * columns) as usize];
    for (i, plant) in plants.iter().enumerate() {
        let (low, high) = plant.bounds();
        for sz in (low[2].max(0) / SECTOR)..=(high[2] / SECTOR).min(columns - 1) {
            for sx in (low[0].max(0) / SECTOR)..=(high[0] / SECTOR).min(columns - 1) {
                by_column[(sz * columns + sx) as usize].push(i);
            }
        }
    }

    (plants, tree_count, by_column)
}

/// A plant as the GPU's generator reads it (make.slang, `plantMaterial`).
fn plant_ints(plant: &Plant) -> [i32; 32] {
    let mut ints = [0i32; 32];
    let (low, high) = plant.bounds();
    ints[8..11].copy_from_slice(&low);
    ints[11..14].copy_from_slice(&high);
    match plant {
        Plant::Flower { x, z, base, stem, kind } => {
            ints[..6].copy_from_slice(&[0, *x, *z, *base, *stem, i32::from(*kind)]);
        }
        Plant::Round { x, z, base, trunk, crowns } => {
            ints[..5].copy_from_slice(&[1, *x, *z, *base, *trunk]);
            for (k, c) in crowns.iter().enumerate().take(4) {
                ints[16 + k * 4..20 + k * 4].copy_from_slice(c);
            }
        }
        Plant::Cone { x, z, base, height, radius } => {
            ints[..6].copy_from_slice(&[2, *x, *z, *base, *height, *radius]);
        }
    }
    ints
}

fn make(scene: &Scene) -> Made {
    let v = scene.voxel;
    let columns = (scene.across / v) as i32 / SECTOR;
    let tall = (HEIGHT_METRES / v) as i32 / SECTOR;
    let sector_count = [columns, tall, columns];
    let (plants, tree_count, by_column) = place(scene);

    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
    let outs: Vec<Vec<(usize, ColumnOut)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= by_column.len() {
                            break;
                        }
                        let (sx, sz) = (i as i32 % columns, i as i32 / columns);
                        done.push((i, make_column(scene, sx, sz, tall, columns, &plants, &by_column[i])));
                    }
                    done
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });

    let count = (columns * columns * tall) as usize;
    let mut sectors = vec![0u32; count * SECTOR_WORDS];
    for s in 0..count {
        sectors[s * SECTOR_WORDS + 24] = EMPTY;
    }
    let mut bricks = Vec::new();
    let mut materials = Vec::new();
    let mut solid = 0;
    let mut all: Vec<(usize, ColumnOut)> = outs.into_iter().flatten().collect();
    all.sort_by_key(|(i, _)| *i);
    for (_, out) in all {
        solid += out.solid;
        for (index, mask, list) in out.sectors {
            let at = index * SECTOR_WORDS;
            sectors[at..at + 16].copy_from_slice(&mask);
            let mut before = 0u32;
            for pair in 0..8 {
                let low = before;
                before += mask[pair * 2].count_ones();
                let high = before;
                before += mask[pair * 2 + 1].count_ones();
                sectors[at + 16 + pair] = low | (high << 16);
            }
            sectors[at + 24] = (bricks.len() / BRICK_WORDS) as u32;
            for (mut words, mats) in list {
                words[18] = materials.len() as u32;
                materials.extend_from_slice(&mats);
                bricks.extend_from_slice(&words);
            }
        }
    }
    while materials.len() % 4 != 0 {
        materials.push(0);
    }
    println!("plants: {tree_count} trees, {} flowers", plants.len() - tree_count);
    Made { sector_count, sectors, bricks, materials, solid, plants: plants.len() }
}

fn make_column(
    scene: &Scene,
    sx: i32,
    sz: i32,
    tall: i32,
    columns: i32,
    plants: &[Plant],
    near: &[usize],
) -> ColumnOut {
    let v = scene.voxel;
    let x0 = sx * SECTOR;
    let z0 = sz * SECTOR;
    let side = (SECTOR + 2) as usize;
    // Heights with a border of one, so the ground can be filled down to its lowest neighbour and leave no gaps.
    let mut heights = vec![0i32; side * side];
    for z in 0..side {
        for x in 0..side {
            let (wx, wz) = ((x0 + x as i32 - 1) as f32 + 0.5, (z0 + z as i32 - 1) as f32 + 0.5);
            heights[z * side + x] = (scene.ground(wx * v, wz * v) / v).floor() as i32;
        }
    }
    let n = SECTOR as usize;
    let mut low = vec![0i32; n * n];
    let mut blade = vec![0i32; n * n];
    let mut blade_kind = vec![0u8; n * n];
    let mut top_kind = vec![1u8; n * n];
    for z in 0..n {
        for x in 0..n {
            let h = heights[(z + 1) * side + x + 1];
            let around = [
                heights[z * side + x + 1],
                heights[(z + 2) * side + x + 1],
                heights[(z + 1) * side + x],
                heights[(z + 1) * side + x + 2],
            ];
            low[z * n + x] = around.iter().copied().min().unwrap().min(h) - 1;
            let (wx, wz) = (x0 + x as i32, z0 + z as i32);
            let (mx, mz) = (wx as f32 * v, wz as f32 * v);
            let forest = scene.forest(mx, mz);
            let hill = h as f32 * v > 13.0;
            let r = hash(wx, 0, wz, 21);
            let chance = if hill { 0.35 } else { 0.62 - 0.3 * forest };
            if unit(r) < chance {
                // In metres, so a coarser grid keeps the meadow as tall.
                let tallest = (if hill { 0.25 } else { 0.7 - 0.37 * forest }) / v;
                blade[z * n + x] = 1 + (unit(hash(wx, 1, wz, 21)) * tallest) as i32;
                blade_kind[z * n + x] = 3 + (r & 3) as u8;
            }
            if hill {
                top_kind[z * n + x] = if fbm(mx / 5.0, mz / 5.0, 31) > 0.72 { 21 } else { 19 };
            }
        }
    }
    let mut out = ColumnOut { sectors: Vec::new(), solid: 0 };
    let near: Vec<(&Plant, [i32; 3], [i32; 3])> = near
        .iter()
        .map(|&i| {
            let (l, h) = plants[i].bounds();
            (&plants[i], l, h)
        })
        .collect();
    for sy in 0..tall {
        let mut mask = [0u32; 16];
        let mut list = Vec::new();
        for by in 0..8 {
            for bz in 0..8 {
                for bx in 0..8 {
                    let low_corner = [x0 + bx * BRICK, sy * SECTOR + by * BRICK, z0 + bz * BRICK];
                    let high_corner = [low_corner[0] + BRICK - 1, low_corner[1] + BRICK - 1, low_corner[2] + BRICK - 1];
                    // Which columns and plants could put voxels here.
                    let mut reach = (i32::MAX, i32::MIN);
                    for z in 0..BRICK as usize {
                        for x in 0..BRICK as usize {
                            let c = (bz as usize * 8 + z) * n + bx as usize * 8 + x;
                            let h = heights[(bz as usize * 8 + z + 1) * side + bx as usize * 8 + x + 1];
                            reach.0 = reach.0.min(low[c]);
                            reach.1 = reach.1.max(h + blade[c]);
                        }
                    }
                    let ground_here = reach.0 <= high_corner[1] && reach.1 >= low_corner[1];
                    let plants_here: Vec<&Plant> = near
                        .iter()
                        .filter(|(_, l, h)| (0..3).all(|a| l[a] <= high_corner[a] && h[a] >= low_corner[a]))
                        .map(|(p, _, _)| *p)
                        .collect();
                    if !ground_here && plants_here.is_empty() {
                        continue;
                    }
                    let mut words = [0u32; BRICK_WORDS];
                    let mut mats = Vec::new();
                    let mut sum = [0.0f32; 3];
                    let mut weighed = 0.0f32;
                    for lz in 0..BRICK {
                        for ly in 0..BRICK {
                            for lx in 0..BRICK {
                                let p = [low_corner[0] + lx, low_corner[1] + ly, low_corner[2] + lz];
                                let c = ((bz * 8 + lz) as usize) * n + (bx * 8 + lx) as usize;
                                let h = heights[((bz * 8 + lz) as usize + 1) * side + (bx * 8 + lx) as usize + 1];
                                let y = p[1];
                                let mut m = 0u8;
                                if y <= h && y >= low[c] {
                                    m = if y == h { top_kind[c] } else { 2 };
                                } else if y > h && y <= h + blade[c] {
                                    m = if y == h + blade[c] && blade[c] as f32 * v > 0.2 { 7 } else { blade_kind[c] };
                                } else {
                                    for plant in &plants_here {
                                        m = plant.material(p);
                                        if m != 0 {
                                            break;
                                        }
                                    }
                                }
                                if m != 0 {
                                    let _ = columns;
                                    let index = (lx + 8 * (ly + 8 * lz)) as usize;
                                    words[index >> 5] |= 1 << (index & 31);
                                    let g = ((lx >> 1) + 4 * ((ly >> 1) + 4 * (lz >> 1))) as usize;
                                    words[16 + (g >> 5)] |= 1 << (g & 31);
                                    // The average a far brick is drawn in leaves out the dirt under the surface.
                                    let weight = if m == 2 { 0.001 } else { 1.0 };
                                    let col = PALETTE[m as usize];
                                    for a in 0..3 {
                                        sum[a] += col[a] * weight;
                                    }
                                    weighed += weight;
                                }
                            }
                        }
                    }
                    // Materials in the order of the bits.
                    for index in 0..512usize {
                        if words[index >> 5] >> (index & 31) & 1 == 1 {
                            let (lx, ly, lz) = ((index % 8) as i32, ((index / 8) % 8) as i32, (index / 64) as i32);
                            let p = [low_corner[0] + lx, low_corner[1] + ly, low_corner[2] + lz];
                            let c = ((bz * 8 + lz) as usize) * n + (bx * 8 + lx) as usize;
                            let h = heights[((bz * 8 + lz) as usize + 1) * side + (bx * 8 + lx) as usize + 1];
                            let y = p[1];
                            let m = if y <= h && y >= low[c] {
                                if y == h { top_kind[c] } else { 2 }
                            } else if y > h && y <= h + blade[c] {
                                if y == h + blade[c] && blade[c] as f32 * v > 0.2 { 7 } else { blade_kind[c] }
                            } else {
                                plants_here.iter().map(|plant| plant.material(p)).find(|&m| m != 0).unwrap_or(0)
                            };
                            mats.push(m);
                        }
                    }
                    if mats.is_empty() {
                        continue;
                    }
                    let k = weighed;
                    let avg = (0..3).map(|a| ((sum[a] / k).clamp(0.0, 1.0) * 255.0) as u32).collect::<Vec<_>>();
                    words[19] = avg[0] | (avg[1] << 8) | (avg[2] << 16);
                    out.solid += mats.len() as u64;
                    let index = (bx + 8 * (by + 8 * bz)) as usize;
                    mask[index >> 5] |= 1 << (index & 31);
                    list.push((index, words, mats));
                }
            }
        }
        if !list.is_empty() {
            list.sort_by_key(|(i, _, _)| *i);
            let sector = ((sy * columns + sz) * columns + sx) as usize;
            out.sectors.push((sector, mask, list.into_iter().map(|(_, w, m)| (w, m)).collect()));
        }
    }
    out
}

/// Each brick's place in bricks, found from the sectors, in the bricks' order.
fn brick_places(made: &Made) -> Vec<[i32; 3]> {
    let [cx, cy, cz] = made.sector_count;
    let mut places = vec![[0i32; 3]; made.bricks.len() / BRICK_WORDS];
    for s in 0..(cx * cy * cz) as usize {
        let at = s * SECTOR_WORDS;
        if made.sectors[at + 24] == EMPTY {
            continue;
        }
        let (sx, rest) = (s as i32 % cx, s as i32 / cx);
        let (sz, sy) = (rest % cz, rest / cz);
        let mut k = made.sectors[at + 24] as usize;
        for bi in 0..512usize {
            if made.sectors[at + (bi >> 5)] >> (bi & 31) & 1 == 1 {
                places[k] = [sx * 8 + (bi % 8) as i32, sy * 8 + ((bi / 8) % 8) as i32, sz * 8 + (bi / 64) as i32];
                k += 1;
            }
        }
    }
    places
}

/// Faces of solid voxels that show (beside empty ones), and the quads they
/// make when runs of one material along one side are joined, for comparing
/// with meshing the same voxels.
fn faces(made: &Made) -> (u64, u64) {
    let [cx, cy, cz] = made.sector_count;
    let solid = |p: [i32; 3]| -> Option<u8> {
        if p.iter().any(|&a| a < 0) || p[0] >= cx * 64 || p[1] >= cy * 64 || p[2] >= cz * 64 {
            return None;
        }
        let s = [p[0] >> 6, p[1] >> 6, p[2] >> 6];
        let at = (((s[1] * cz + s[2]) * cx + s[0]) as usize) * SECTOR_WORDS;
        if made.sectors[at + 24] == EMPTY {
            return None;
        }
        let b = [(p[0] >> 3) & 7, (p[1] >> 3) & 7, (p[2] >> 3) & 7];
        let bi = (b[0] + 8 * (b[1] + 8 * b[2])) as usize;
        if made.sectors[at + (bi >> 5)] >> (bi & 31) & 1 == 0 {
            return None;
        }
        let pair = made.sectors[at + 16 + (bi >> 6)];
        let mut before = if (bi >> 5) & 1 == 1 { pair >> 16 } else { pair & 0xFFFF };
        before += (made.sectors[at + (bi >> 5)] & ((1u32 << (bi & 31)) - 1)).count_ones();
        let brick = (made.sectors[at + 24] + before) as usize * BRICK_WORDS;
        let l = [p[0] & 7, p[1] & 7, p[2] & 7];
        let vi = (l[0] + 8 * (l[1] + 8 * l[2])) as usize;
        let words = &made.bricks[brick..brick + 16];
        if words[vi >> 5] >> (vi & 31) & 1 == 0 {
            return None;
        }
        let mut k = (words[vi >> 5] & ((1u32 << (vi & 31)) - 1)).count_ones();
        for w in &words[..vi >> 5] {
            k += w.count_ones();
        }
        Some(made.materials[(made.bricks[brick + 18] + k) as usize])
    };
    let count = made.bricks.len() / BRICK_WORDS;
    let next = AtomicUsize::new(0);
    let places: Vec<[i32; 3]> = brick_places(made).iter().map(|p| p.map(|a| a * 8)).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
    let sums: Vec<(u64, u64)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let (mut shown, mut quads) = (0u64, 0u64);
                    loop {
                        let b = next.fetch_add(256, Ordering::Relaxed);
                        if b >= count {
                            break;
                        }
                        for brick in b..(b + 256).min(count) {
                            let o = places[brick];
                            for vi in 0..512 {
                                let p = [o[0] + vi % 8, o[1] + (vi / 8) % 8, o[2] + vi / 64];
                                let Some(m) = solid(p) else { continue };
                                for axis in 0..3 {
                                    for sign in [-1, 1] {
                                        let mut q = p;
                                        q[axis] += sign;
                                        if solid(q).is_some() {
                                            continue;
                                        }
                                        shown += 1;
                                        // Joined to the face before it along the next axis when that one shows and matches.
                                        let along = (axis + 1) % 3;
                                        let mut r = p;
                                        r[along] -= 1;
                                        let mut rq = r;
                                        rq[axis] += sign;
                                        if !(solid(r) == Some(m) && solid(rq).is_none()) {
                                            quads += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (shown, quads)
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    sums.iter().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
}

fn compile(source: &Path, out: &Path) -> Result<Vec<u8>> {
    let output = Command::new(std::env::var("SLANGC").unwrap_or_else(|_| "slangc".into()))
        .arg(source)
        .args([
            "-target",
            "spirv",
            "-profile",
            "spirv_1_6",
            "-O2",
            "-fvk-use-scalar-layout",
            "-fvk-use-entrypoint-name",
        ])
        .arg("-I")
        .arg(source.parent().unwrap())
        .arg("-o")
        .arg(out)
        .output()
        .context("running slangc")?;
    anyhow::ensure!(output.status.success(), "slangc: {}", String::from_utf8_lossy(&output.stderr));
    Ok(std::fs::read(out)?)
}

fn buffer(gpu: &Gpu, name: &str, size: u64, extra: vk::BufferUsageFlags) -> Result<Buffer> {
    gpu.create_buffer(
        name,
        size.max(4),
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | extra,
        MemoryLocation::GpuOnly,
    )
}

fn upload(gpu: &Gpu, name: &str, bytes: &[u8]) -> Result<Buffer> {
    let made = buffer(gpu, name, bytes.len() as u64, vk::BufferUsageFlags::empty())?;
    // Large buffers go up a piece at a time.
    for (i, piece) in bytes.chunks(256 << 20).enumerate() {
        gpu.upload_buffer(&made, (i as u64) * (256 << 20), piece)?;
    }
    Ok(made)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuDraw {
    places: u64,
    drawn: u64,
    bands: u64,
    draws: u64,
    brick_count: u32,
    near: f32,
    coarse: u64,
    coarse_step: u32,
    coarse_size: [u32; 2],
    pad: u32,
    seen: u64,
    depths: u64,
}

const FROM_COARSE: u32 = 8;
const BANDS: u64 = 32;
const COARSE_STEP: u32 = 4;
const NEAR: f32 = 0.02;

/// The three ways of drawing measured: every pixel's ray walked through the
/// whole grid; the same after a coarse pass finds where each patch of rays
/// first meets a brick; and each brick's box drawn with triangles and
/// walked inside alone.
#[derive(Clone, Copy, PartialEq)]
enum Method {
    Trace,
    Coarse,
    Bricks,
    /// The plain walk through the world the GPU made, to check it against the CPU's.
    GpuMade,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Method::Trace => "trace",
            Method::Coarse => "coarse-first",
            Method::Bricks => "bricks",
            Method::GpuMade => "trace-gpu-made",
        }
    }
}

struct Shot {
    name: &'static str,
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    flags: u32,
    lod_scale: f32,
}

fn compute_barrier(gpu: &Gpu, cmd: vk::CommandBuffer, dst: vk::PipelineStageFlags2, dst_access: vk::AccessFlags2) {
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::ALL_TRANSFER)
        .src_access_mask(vk::AccessFlags2::SHADER_WRITE | vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(dst)
        .dst_access_mask(dst_access);
    unsafe {
        gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[barrier]));
    }
}

fn image_barrier(
    gpu: &Gpu,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    layout: vk::ImageLayout,
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .image(image)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(layout)
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(stage)
        .dst_access_mask(access)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    unsafe {
        gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&[barrier]));
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let across: f32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(256.0);
    let per_metre: f32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(16.0);
    let folder = PathBuf::from(args.get(3).cloned().unwrap_or_else(|| "small-voxels-out".into()));
    let (width, height) = (1920u32, 1080u32);
    std::fs::create_dir_all(&folder)?;
    let scene = Scene { voxel: 1.0 / per_metre, across, camera: [across / 2.0, across / 2.0 - 60.0] };

    let started = Instant::now();
    let made = make(&scene);
    let made_ms = started.elapsed().as_secs_f64() * 1000.0;
    let brick_count = made.bricks.len() / BRICK_WORDS;
    let bytes = (made.sectors.len() * 4 + made.bricks.len() * 4 + made.materials.len()) as f64;
    println!(
        "world {across} m across at {per_metre} voxels a metre: {} x {} x {} sectors, {brick_count} bricks, {} solid voxels, {} plants",
        made.sector_count[0], made.sector_count[1], made.sector_count[2], made.solid, made.plants
    );
    println!(
        "memory: sectors {:.1} MB, bricks {:.1} MB, materials {:.1} MB, all {:.1} MB ({:.2} bytes a solid voxel)",
        made.sectors.len() as f64 * 4.0 / 1e6,
        made.bricks.len() as f64 * 4.0 / 1e6,
        made.materials.len() as f64 / 1e6,
        bytes / 1e6,
        bytes / made.solid as f64
    );
    println!(
        "made in {made_ms:.0} ms on {} threads ({:.1} m² a ms)",
        std::thread::available_parallelism()?.get(),
        across * across / made_ms as f32
    );
    if std::env::var("SMALL_VOXELS_FACES").is_ok() {
        let started = Instant::now();
        let (shown, quads) = faces(&made);
        println!(
            "faces that show: {shown}; joined along one side: {quads} quads ({:.0} MB at 8 bytes a quad), counted in {:.0} ms",
            quads as f64 * 8.0 / 1e6,
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    let gpu = Gpu::new_headless()?;
    println!("GPU: {}", gpu.info.name);
    let bindless = Bindless::new(&gpu)?;
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/small_voxels");
    let trace_module =
        ShaderModule::new(&gpu, "small voxels trace", &compile(&here.join("trace.slang"), &folder.join("trace.spv"))?)?;
    let raster_module = ShaderModule::new(
        &gpu,
        "small voxels bricks",
        &compile(&here.join("raster.slang"), &folder.join("raster.spv"))?,
    )?;
    let layout = bindless.pipeline_layout;
    let trace_view = create_compute_pipeline(&gpu, layout, "trace", &trace_module, c"traceView")?;
    let coarse_view = create_compute_pipeline(&gpu, layout, "coarse", &trace_module, c"coarseView")?;
    let count_bands = create_compute_pipeline(&gpu, layout, "count bands", &raster_module, c"countBands")?;
    let start_bands = create_compute_pipeline(&gpu, layout, "start bands", &raster_module, c"startBands")?;
    let sort_bands = create_compute_pipeline(&gpu, layout, "sort bands", &raster_module, c"sortBands")?;
    let shade_bricks = create_compute_pipeline(&gpu, layout, "shade bricks", &raster_module, c"shadeBricks")?;
    let colour_format = [vk::Format::R32G32_UINT];
    let draw_bricks = create_graphics_pipeline(
        &gpu,
        layout,
        &GraphicsPipelineDesc {
            color_formats: &colour_format,
            depth_format: Some(vk::Format::D32_SFLOAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS,
            cull: vk::CullModeFlags::NONE,
            ..GraphicsPipelineDesc::new("bricks", &raster_module, c"brickVertex", c"brickFragment")
        },
    )?;

    let up_started = Instant::now();
    let sectors = upload(&gpu, "sectors", bytemuck::cast_slice(&made.sectors))?;
    let bricks = upload(&gpu, "bricks", bytemuck::cast_slice(&made.bricks))?;
    let materials = upload(&gpu, "materials", &made.materials)?;
    let palette: Vec<u32> = PALETTE
        .iter()
        .map(|c| (c[0] * 255.0) as u32 | ((c[1] * 255.0) as u32) << 8 | ((c[2] * 255.0) as u32) << 16)
        .collect();
    let palette = upload(&gpu, "palette", bytemuck::cast_slice(&palette))?;
    println!("uploaded in {:.0} ms", up_started.elapsed().as_secs_f64() * 1000.0);
    let places: Vec<u32> =
        brick_places(&made).iter().map(|p| p[0] as u32 | (p[1] as u32) << 11 | (p[2] as u32) << 21).collect();
    let places = upload(&gpu, "places", bytemuck::cast_slice(&places))?;
    let drawn = buffer(&gpu, "drawn", brick_count as u64 * 4, vk::BufferUsageFlags::empty())?;
    let bands = buffer(&gpu, "bands", BANDS * 3 * 4, vk::BufferUsageFlags::empty())?;
    let draws = buffer(&gpu, "draws", 16, vk::BufferUsageFlags::INDIRECT_BUFFER)?;
    let coarse_size = [width / COARSE_STEP + 2, height / COARSE_STEP + 2];
    let coarse = buffer(&gpu, "coarse", u64::from(coarse_size[0] * coarse_size[1]) * 4, vk::BufferUsageFlags::empty())?;
    let seen = buffer(&gpu, "seen", u64::from(width * height) * 8, vk::BufferUsageFlags::empty())?;
    let depths = buffer(&gpu, "depths", u64::from(width * height) * 4, vk::BufferUsageFlags::empty())?;
    let world = GpuWorld {
        sectors: sectors.address,
        bricks: bricks.address,
        materials: materials.address,
        palette: palette.address,
        sector_count: made.sector_count,
        voxel: scene.voxel,
    };
    let world_buffer = upload(&gpu, "world", bytemuck::bytes_of(&world))?;
    let make_module =
        ShaderModule::new(&gpu, "small voxels make", &compile(&here.join("make.slang"), &folder.join("make.spv"))?)?;
    let gpu_made = gpu_make::make_on_gpu(&gpu, &bindless, &make_module, &scene, &made)?;
    let gpu_world_buffer = upload(
        &gpu,
        "gpu-made world",
        bytemuck::bytes_of(&GpuWorld {
            sectors: gpu_made.sectors.address,
            bricks: gpu_made.bricks.address,
            materials: gpu_made.materials.address,
            ..world
        }),
    )?;
    let draw = GpuDraw {
        places: places.address,
        drawn: drawn.address,
        bands: bands.address,
        draws: draws.address,
        brick_count: brick_count as u32,
        near: NEAR / scene.voxel,
        coarse: coarse.address,
        coarse_step: COARSE_STEP,
        coarse_size,
        pad: 0,
        seen: seen.address,
        depths: depths.address,
    };
    let draw_buffer = upload(&gpu, "draw", bytemuck::bytes_of(&draw))?;
    let output = buffer(&gpu, "output", u64::from(width * height) * 4, vk::BufferUsageFlags::empty())?;
    let colour_desc = ImageDesc::new(
        width,
        height,
        vk::Format::R32G32_UINT,
        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
    );
    let colour = gpu.create_image("colour", colour_desc)?;
    let depth = gpu.create_image(
        "depth",
        ImageDesc::new(
            width,
            height,
            vk::Format::D32_SFLOAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        ),
    )?;
    let queries = unsafe {
        gpu.device.create_query_pool(
            &vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(2),
            None,
        )?
    };

    let cam = scene.camera;
    let ground = scene.ground(cam[0], cam[1]);
    let low = [cam[0], ground + 1.1, cam[1]];
    let high = [cam[0], ground + 30.0, cam[1] - 30.0];
    let shots = [
        Shot { name: "meadow", eye: low, yaw: 0.0, pitch: 7.0, flags: 3, lod_scale: 2.0 },
        Shot { name: "meadow-primary-only", eye: low, yaw: 0.0, pitch: 7.0, flags: 0, lod_scale: 2.0 },
        Shot { name: "meadow-steps", eye: low, yaw: 0.0, pitch: 7.0, flags: 4, lod_scale: 2.0 },
        Shot {
            name: "close",
            eye: [cam[0] + 0.3, ground + 1.0, cam[1] + 2.0],
            yaw: 20.0,
            pitch: -25.0,
            flags: 3,
            lod_scale: 2.0,
        },
        Shot { name: "over", eye: high, yaw: 0.0, pitch: -12.0, flags: 3, lod_scale: 2.0 },
        Shot { name: "over-primary-only", eye: high, yaw: 0.0, pitch: -12.0, flags: 0, lod_scale: 2.0 },
    ];
    let methods = [Method::Trace, Method::Coarse, Method::Bricks, Method::GpuMade];
    let moon = {
        let s = [-0.35f32, 0.8, -0.45];
        let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
        [s[0] / l, s[1] / l, s[2] / l]
    };
    let fov = 62.0f32.to_radians();
    for shot in shots {
        let mut traced: Option<Vec<u8>> = None;
        // Each method's view, then the methods taken in turn each round, so that other work on the GPU weighs on
        // them alike.
        let mut views = Vec::new();
        for method in methods {
            let (yaw, pitch) = (shot.yaw.to_radians(), shot.pitch.to_radians());
            let forward = [yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()];
            let right = [yaw.cos(), 0.0, -yaw.sin()];
            let up = [
                forward[1] * right[2] - forward[2] * right[1],
                forward[2] * right[0] - forward[0] * right[2],
                forward[0] * right[1] - forward[1] * right[0],
            ];
            // `up` above is forward x right, which points down; the shader wants it up.
            let up = [-up[0], -up[1], -up[2]];
            let view = GpuView {
                eye: shot.eye.map(|a| a / scene.voxel),
                pixel_angle: 2.0 * (fov / 2.0).tan() / height as f32,
                forward,
                tan_half: (fov / 2.0).tan(),
                right,
                aspect: width as f32 / height as f32,
                up,
                flags: shot.flags | if method == Method::Coarse { FROM_COARSE } else { 0 },
                sun: moon,
                lod_scale: shot.lod_scale,
                size: [width, height],
                output: output.address,
            };
            views.push((method, upload(&gpu, "view", bytemuck::bytes_of(&view))?));
        }
        let run = |method: Method, push: GpuPush| -> Result<f64> {
            gpu.immediate(|cmd| unsafe {
                let device = &gpu.device;
                device.cmd_reset_query_pool(cmd, queries, 0, 2);
                device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::TOP_OF_PIPE, queries, 0);
                let compute = |pipeline, groups: [u32; 2]| {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
                    bindless.push(&gpu, cmd, &push);
                    device.cmd_dispatch(cmd, groups[0], groups[1], 1);
                };
                let rw = vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE;
                match method {
                    Method::Trace | Method::GpuMade => compute(trace_view, [width.div_ceil(8), height.div_ceil(8)]),
                    Method::Coarse => {
                        compute(coarse_view, [coarse_size[0].div_ceil(8), coarse_size[1].div_ceil(8)]);
                        compute_barrier(&gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
                        compute(trace_view, [width.div_ceil(8), height.div_ceil(8)]);
                    }
                    Method::Bricks => {
                        device.cmd_fill_buffer(cmd, bands.handle, 0, vk::WHOLE_SIZE, 0);
                        compute_barrier(&gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
                        let groups = (brick_count as u32).div_ceil(256);
                        compute(count_bands, [groups, 1]);
                        compute_barrier(&gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
                        compute(start_bands, [1, 1]);
                        compute_barrier(&gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
                        compute(sort_bands, [groups, 1]);
                        compute_barrier(
                            &gpu,
                            cmd,
                            vk::PipelineStageFlags2::DRAW_INDIRECT
                                | vk::PipelineStageFlags2::VERTEX_SHADER
                                | vk::PipelineStageFlags2::FRAGMENT_SHADER,
                            vk::AccessFlags2::INDIRECT_COMMAND_READ | vk::AccessFlags2::SHADER_READ,
                        );
                        image_barrier(
                            &gpu,
                            cmd,
                            colour.handle,
                            vk::ImageAspectFlags::COLOR,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                        image_barrier(
                            &gpu,
                            cmd,
                            depth.handle,
                            vk::ImageAspectFlags::DEPTH,
                            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                        );
                        let colours = [vk::RenderingAttachmentInfo::default()
                            .image_view(colour.view)
                            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                            .load_op(vk::AttachmentLoadOp::CLEAR)
                            .store_op(vk::AttachmentStoreOp::STORE)
                            .clear_value(vk::ClearValue { color: vk::ClearColorValue { uint32: [EMPTY; 4] } })];
                        let depth_attachment = vk::RenderingAttachmentInfo::default()
                            .image_view(depth.view)
                            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                            .load_op(vk::AttachmentLoadOp::CLEAR)
                            .store_op(vk::AttachmentStoreOp::STORE)
                            .clear_value(vk::ClearValue {
                                depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                            });
                        let area =
                            vk::Rect2D { offset: vk::Offset2D::default(), extent: vk::Extent2D { width, height } };
                        let rendering = vk::RenderingInfo::default()
                            .render_area(area)
                            .layer_count(1)
                            .color_attachments(&colours)
                            .depth_attachment(&depth_attachment);
                        device.cmd_begin_rendering(cmd, &rendering);
                        device.cmd_set_viewport(
                            cmd,
                            0,
                            &[vk::Viewport {
                                x: 0.0,
                                y: 0.0,
                                width: width as f32,
                                height: height as f32,
                                min_depth: 0.0,
                                max_depth: 1.0,
                            }],
                        );
                        device.cmd_set_scissor(cmd, 0, &[area]);
                        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, draw_bricks);
                        bindless.push(&gpu, cmd, &push);
                        device.cmd_draw_indirect(cmd, draws.handle, 0, 1, 16);
                        device.cmd_end_rendering(cmd);
                        // What each pixel saw, and its depth, copied out for the shading pass.
                        let to_copy = |image: vk::Image, aspect, layout, stage, access| {
                            let barrier = vk::ImageMemoryBarrier2::default()
                                .image(image)
                                .old_layout(layout)
                                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                                .src_stage_mask(stage)
                                .src_access_mask(access)
                                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: aspect,
                                    base_mip_level: 0,
                                    level_count: 1,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                });
                            device.cmd_pipeline_barrier2(
                                cmd,
                                &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
                            );
                            let region = vk::BufferImageCopy::default()
                                .image_subresource(vk::ImageSubresourceLayers {
                                    aspect_mask: aspect,
                                    mip_level: 0,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                })
                                .image_extent(vk::Extent3D { width, height, depth: 1 });
                            let target =
                                if aspect == vk::ImageAspectFlags::COLOR { seen.handle } else { depths.handle };
                            device.cmd_copy_image_to_buffer(
                                cmd,
                                image,
                                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                                target,
                                &[region],
                            );
                        };
                        to_copy(
                            colour.handle,
                            vk::ImageAspectFlags::COLOR,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                        to_copy(
                            depth.handle,
                            vk::ImageAspectFlags::DEPTH,
                            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                        );
                        compute_barrier(&gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
                        compute(shade_bricks, [width.div_ceil(8), height.div_ceil(8)]);
                    }
                }
                device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::BOTTOM_OF_PIPE, queries, 1);
            })?;
            let mut stamps = [0u64; 2];
            unsafe {
                gpu.device.get_query_pool_results(
                    queries,
                    0,
                    &mut stamps,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )?
            };
            Ok((stamps[1] - stamps[0]) as f64 * f64::from(gpu.info.timestamp_period) / 1e6)
        };
        let push_of = |method: Method, view: &Buffer| GpuPush {
            world: if method == Method::GpuMade { gpu_world_buffer.address } else { world_buffer.address },
            view: view.address,
            draw: draw_buffer.address,
        };
        let mut times = vec![Vec::new(); views.len()];
        for _ in 0..80 {
            for (k, (method, view)) in views.iter().enumerate() {
                times[k].push(run(*method, push_of(*method, view))?);
            }
        }
        for (k, (method, view)) in views.into_iter().enumerate() {
            run(method, push_of(method, &view))?;
            let mut last: Vec<f64> = times[k][20..].to_vec();
            last.sort_by(f64::total_cmp);
            let pixels = gpu.read_buffer(&output, 0, u64::from(width * height) * 4)?;
            // How far this method's picture is from the plain walk's, over the pixels both drew ground in.
            let mut differs = String::new();
            if let Some(base) = &traced {
                let (mut sum, mut count, mut far) = (0u64, 0u64, 0u64);
                for (a, b) in pixels.chunks(4).zip(base.chunks(4)) {
                    let d = (0..3).map(|i| (i32::from(a[i]) - i32::from(b[i])).unsigned_abs()).max().unwrap();
                    sum += u64::from(d);
                    count += 1;
                    far += u64::from(d > 24);
                }
                differs = format!(
                    ", against the walk: mean {:.2} levels, {:.2}% of pixels by more than 24",
                    sum as f64 / count.max(1) as f64,
                    far as f64 * 100.0 / count.max(1) as f64
                );
            }
            println!(
                "{} {}: least {:.2} ms, median {:.2} ms over {} frames{differs}",
                shot.name,
                method.name(),
                last[0],
                last[last.len() / 2],
                last.len()
            );
            let file = std::fs::File::create(folder.join(format!("{}-{}.png", shot.name, method.name())))?;
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&pixels)?;
            if method == Method::Trace {
                traced = Some(pixels);
            }
            gpu.destroy_buffer(view);
        }
    }
    gpu.wait_idle();
    unsafe {
        gpu.device.destroy_query_pool(queries, None);
        for pipeline in [trace_view, coarse_view, count_bands, start_bands, sort_bands, shade_bricks, draw_bricks] {
            gpu.device.destroy_pipeline(pipeline, None);
        }
    }
    for made in [
        sectors,
        bricks,
        materials,
        palette,
        world_buffer,
        output,
        places,
        drawn,
        bands,
        draws,
        coarse,
        draw_buffer,
        seen,
        depths,
    ] {
        gpu.destroy_buffer(made);
    }
    for made in [gpu_made.sectors, gpu_made.bricks, gpu_made.materials, gpu_world_buffer] {
        gpu.destroy_buffer(made);
    }
    make_module.destroy(&gpu);
    gpu.destroy_image(colour);
    gpu.destroy_image(depth);
    trace_module.destroy(&gpu);
    raster_module.destroy(&gpu);
    Ok(())
}
