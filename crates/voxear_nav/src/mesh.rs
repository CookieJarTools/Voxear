//! Nav meshes, for levels made of models rather than blocks: baked once
//! from the level's still colliders, kept beside the level, and searched
//! as a block world's places are.
//!
//! **The bake** ([`bake`]) lays every triangle into a grid of columns
//! [`BakeSettings::cell`] across: each triangle clipped to each column it
//! crosses gives a span of height there, so walls count as well as floors.
//! A span's top is somewhere to walk where its triangles are no steeper
//! than the steepest slope walked and the next span over it leaves room for
//! the walker's height. Walkable cells are joined to the cells beside them
//! whose floors are within a step, and those nearer than the walker's
//! radius to an edge are let go, so a walker's middle anywhere on the mesh
//! keeps its body off every wall. The cells left are gathered into
//! rectangles, each a convex polygon following the floor's height at its
//! corners, joined to the polygons beside it along the edges they share.
//!
//! **Searches** ([`NavMesh::find_path`]) are an A* over polygons, entering
//! each at the middle of the edge crossed into it, and then the funnel:
//! the way pulled tight through the edges it crosses, so its corners are
//! only where it must turn.
//!
//! **Cuts** ([`NavMesh::cut`]) take boxes out of a baked mesh while the
//! game runs, for obstacles that come and go.

use std::{cmp::Reverse, collections::BinaryHeap};

use voxear_core::math::Vec3;

mod cut;

/// How a nav mesh is baked: how fine its grid, and what walks on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeSettings {
    /// How wide a column of the grid is, in metres: finer finds narrower
    /// gaps, and costs more.
    pub cell: f32,
    /// How tall the walker is, how wide round its middle, how high a step
    /// it walks up, and the steepest slope it walks, in degrees.
    pub height: f32,
    pub radius: f32,
    pub climb: f32,
    pub slope: f32,
    /// The most cells a polygon is across, each way.
    pub largest: u32,
}

impl Default for BakeSettings {
    fn default() -> Self {
        Self { cell: 0.3, height: 1.8, radius: 0.35, climb: 0.45, slope: 45.0, largest: 32 }
    }
}

/// A polygon of a nav mesh: a rectangle on the ground, from `min` to `max`
/// along x and z, its corners' heights going round from `min`, and where
/// its edges to its neighbours begin among the mesh's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Poly {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub heights: [f32; 4],
    pub edges: u32,
    pub count: u32,
}

impl Poly {
    /// Its middle, on its floor.
    pub fn middle(&self) -> Vec3 {
        let (x, z) = ((self.min[0] + self.max[0]) * 0.5, (self.min[1] + self.max[1]) * 0.5);
        Vec3::new(x, self.heights.iter().sum::<f32>() * 0.25, z)
    }

    /// How high its floor is at `(x, z)`, from its corners.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let u = ((x - self.min[0]) / (self.max[0] - self.min[0]).max(1e-4)).clamp(0.0, 1.0);
        let v = ((z - self.min[1]) / (self.max[1] - self.min[1]).max(1e-4)).clamp(0.0, 1.0);
        let [a, b, c, d] = self.heights;
        let near = a + (b - a) * u;
        let far = d + (c - d) * u;
        near + (far - near) * v
    }

    pub fn holds(&self, x: f32, z: f32) -> bool {
        (self.min[0]..=self.max[0]).contains(&x) && (self.min[1]..=self.max[1]).contains(&z)
    }
}

/// An edge two polygons share, from polygon to `to`, from `a` to `b`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PolyEdge {
    pub to: u32,
    pub a: Vec3,
    pub b: Vec3,
}

/// A level's nav mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavMesh {
    pub settings: BakeSettings,
    /// Where its grid begins along x and z, and how many columns it has.
    pub origin: [f32; 2],
    pub columns: [u32; 2],
    pub polys: Vec<Poly>,
    pub edges: Vec<PolyEdge>,
    /// Which polygons lie over each square of a coarse grid, to find the
    /// one under a point: made from the polygons, never kept in a file.
    buckets: Buckets,
}

/// How wide a square of the grid that finds polygons is, in metres.
const BUCKET: f32 = 8.0;

/// The polygons over each square of a coarse grid, square after square:
/// where each square's begin among `polys`.
#[derive(Clone, Debug, Default, PartialEq)]
struct Buckets {
    origin: [f32; 2],
    count: [u32; 2],
    starts: Vec<u32>,
    polys: Vec<u32>,
}

impl Buckets {
    fn of(polys: &[Poly]) -> Self {
        let (mut low, mut high) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for poly in polys {
            for axis in 0..2 {
                (low[axis], high[axis]) = (low[axis].min(poly.min[axis]), high[axis].max(poly.max[axis]));
            }
        }
        if polys.is_empty() {
            return Self::default();
        }
        let count = [0, 1].map(|axis| ((high[axis] - low[axis]) / BUCKET).floor() as u32 + 1);
        let mut each: Vec<Vec<u32>> = vec![Vec::new(); (count[0] * count[1]) as usize];
        for (n, poly) in polys.iter().enumerate() {
            let first = [0, 1].map(|axis| ((poly.min[axis] - low[axis]) / BUCKET).floor() as u32);
            let last = [0, 1].map(|axis| (((poly.max[axis] - low[axis]) / BUCKET).floor() as u32).min(count[axis] - 1));
            for z in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    each[(z * count[0] + x) as usize].push(n as u32);
                }
            }
        }
        let mut starts = Vec::with_capacity(each.len() + 1);
        let mut flat = Vec::new();
        starts.push(0);
        for bucket in each {
            flat.extend(bucket);
            starts.push(flat.len() as u32);
        }
        Self { origin: low, count, starts, polys: flat }
    }

    /// The polygons over the squares from `low` to `high` along x and z, a
    /// polygon over several once for each.
    fn near(&self, low: [f32; 2], high: [f32; 2]) -> impl Iterator<Item = u32> + '_ {
        let square = |value: f32, axis: usize| {
            (((value - self.origin[axis]) / BUCKET).floor() as i64).clamp(0, i64::from(self.count[axis]) - 1) as u32
        };
        let inside = !self.starts.is_empty()
            && (0..2).all(|axis| {
                high[axis] >= self.origin[axis] && low[axis] <= self.origin[axis] + self.count[axis] as f32 * BUCKET
            });
        let (x0, x1, z0, z1) = match inside {
            true => (square(low[0], 0), square(high[0], 0), square(low[1], 1), square(high[1], 1)),
            false => (1, 0, 1, 0),
        };
        (z0..=z1)
            .flat_map(move |z| (x0..=x1).map(move |x| (z * self.count[0] + x) as usize))
            .flat_map(move |at| self.polys[self.starts[at] as usize..self.starts[at + 1] as usize].iter().copied())
    }
}

/// One span of solid in a column, as triangles laid there make it.
#[derive(Clone, Copy, Debug)]
struct Span {
    low: f32,
    high: f32,
    walkable: bool,
}

/// `polygon`, the corners of a shape in the world, clipped to `low..high`
/// along `axis` (0 for x, 2 for z).
fn clip(polygon: &[Vec3], axis: usize, low: f32, high: f32) -> Vec<Vec3> {
    let mut kept = polygon.to_vec();
    for (bound, keep_above) in [(low, true), (high, false)] {
        let inside = |p: &Vec3| if keep_above { p[axis] >= bound } else { p[axis] <= bound };
        let mut next = Vec::with_capacity(kept.len() + 2);
        for at in 0..kept.len() {
            let (a, b) = (kept[at], kept[(at + 1) % kept.len()]);
            if inside(&a) {
                next.push(a);
            }
            if inside(&a) != inside(&b) {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                next.push(a + (b - a) * t);
            }
        }
        kept = next;
        if kept.is_empty() {
            break;
        }
    }
    kept
}

/// Ground given as heights on a grid rather than triangles, as a terrain's:
/// `samples` across along x and z, `spacing` metres apart from `corner`,
/// each height over the corner's, row after row along z; and which samples
/// are holes, where there is no ground. Solid all the way down.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ground {
    pub corner: Vec3,
    pub spacing: f32,
    pub samples: [u32; 2],
    pub heights: Vec<f32>,
    pub holes: Vec<bool>,
}

impl Ground {
    /// Its height at `(x, z)` in metres from its corner, between its
    /// samples; `None` off its edges or over a hole.
    fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        let spacing = self.spacing.max(1e-3);
        let (fx, fz) = (x / spacing, z / spacing);
        let [w, d] = self.samples;
        if !(0.0..=(w.saturating_sub(1)) as f32).contains(&fx) || !(0.0..=(d.saturating_sub(1)) as f32).contains(&fz) {
            return None;
        }
        let (ix, iz) = ((fx as u32).min(w.saturating_sub(2)), (fz as u32).min(d.saturating_sub(2)));
        let at = |x: u32, z: u32| (z * w + x) as usize;
        let corners = [at(ix, iz), at(ix + 1, iz), at(ix, iz + 1), at(ix + 1, iz + 1)];
        if corners.iter().any(|n| self.holes.get(*n).copied().unwrap_or(false)) {
            return None;
        }
        let h = |n: usize| *self.heights.get(n).unwrap_or(&0.0);
        let (u, v) = (fx - ix as f32, fz - iz as f32);
        let near = h(corners[0]) + (h(corners[1]) - h(corners[0])) * u;
        let far = h(corners[2]) + (h(corners[3]) - h(corners[2])) * u;
        Some(self.corner.y + near + (far - near) * v)
    }

    /// Its lowest and highest points in the world.
    fn bounds(&self) -> (Vec3, Vec3) {
        let (low, high) =
            self.heights.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), y| (l.min(*y), h.max(*y)));
        let size = Vec3::new(
            self.samples[0].saturating_sub(1) as f32 * self.spacing,
            0.0,
            self.samples[1].saturating_sub(1) as f32 * self.spacing,
        );
        (self.corner + Vec3::Y * low, self.corner + size + Vec3::Y * high)
    }
}

/// Bakes a nav mesh from `triangles`, in metres, as `settings` say. `None`
/// where there are none.
pub fn bake(triangles: &[[Vec3; 3]], settings: BakeSettings) -> Option<NavMesh> {
    bake_ground(triangles, &[], settings)
}

/// Bakes a nav mesh from `triangles` and `grounds`, in metres, as
/// `settings` say. `None` where there is neither.
pub fn bake_ground(triangles: &[[Vec3; 3]], grounds: &[Ground], settings: BakeSettings) -> Option<NavMesh> {
    voxear_core::profile_scope!("Nav Mesh Bake");
    let cell = settings.cell.max(0.05);
    let (mut low, mut high) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for corner in triangles.iter().flatten() {
        (low, high) = (low.min(*corner), high.max(*corner));
    }
    for ground in grounds {
        let (a, b) = ground.bounds();
        (low, high) = (low.min(a), high.max(b));
    }
    if (triangles.is_empty() && grounds.is_empty()) || !low.is_finite() || !high.is_finite() {
        return None;
    }
    let origin = [low.x, low.z];
    let columns = [((high.x - low.x) / cell).ceil() as u32 + 1, ((high.z - low.z) / cell).ceil() as u32 + 1];
    let column = |x: u32, z: u32| (z * columns[0] + x) as usize;
    let steep = settings.slope.to_radians().cos();

    // Every triangle into the columns it crosses.
    let mut spans: Vec<(u32, Span)> = Vec::new();
    for triangle in triangles {
        let normal = (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]);
        let walkable = normal.length() > 1e-9 && normal.normalize().y.abs() >= steep;
        let (a, b) = (triangle[0].min(triangle[1]).min(triangle[2]), triangle[0].max(triangle[1]).max(triangle[2]));
        let x0 = ((a.x - origin[0]) / cell).floor().max(0.0) as u32;
        let x1 = (((b.x - origin[0]) / cell).floor() as u32).min(columns[0] - 1);
        let z0 = ((a.z - origin[1]) / cell).floor().max(0.0) as u32;
        let z1 = (((b.z - origin[1]) / cell).floor() as u32).min(columns[1] - 1);
        for z in z0..=z1 {
            let along_z = clip(triangle, 2, origin[1] + z as f32 * cell, origin[1] + (z + 1) as f32 * cell);
            if along_z.is_empty() {
                continue;
            }
            for x in x0..=x1 {
                let piece = clip(&along_z, 0, origin[0] + x as f32 * cell, origin[0] + (x + 1) as f32 * cell);
                if piece.is_empty() {
                    continue;
                }
                let (low, high) =
                    piece.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), p| (l.min(p.y), h.max(p.y)));
                spans.push((column(x, z) as u32, Span { low, high, walkable }));
            }
        }
    }
    // Each ground under each column: its height at the column's middle, solid below; walkable where no steeper
    // than the steepest slope across the column.
    for ground in grounds {
        let (a, b) = ground.bounds();
        let floor = a.y - 1.0;
        let x0 = ((a.x - origin[0]) / cell).floor().max(0.0) as u32;
        let x1 = (((b.x - origin[0]) / cell).floor() as u32).min(columns[0] - 1);
        let z0 = ((a.z - origin[1]) / cell).floor().max(0.0) as u32;
        let z1 = (((b.z - origin[1]) / cell).floor() as u32).min(columns[1] - 1);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let (cx, cz) = (origin[0] + (x as f32 + 0.5) * cell, origin[1] + (z as f32 + 0.5) * cell);
                let at = |dx: f32, dz: f32| ground.height_at(cx + dx - ground.corner.x, cz + dz - ground.corner.z);
                let Some(top) = at(0.0, 0.0) else { continue };
                let half = cell * 0.5;
                let slope = |a: Option<f32>, b: Option<f32>| a.zip(b).map_or(0.0, |(a, b)| (b - a) / cell);
                let (dx, dz) = (slope(at(-half, 0.0), at(half, 0.0)), slope(at(0.0, -half), at(0.0, half)));
                let walkable = 1.0 / (1.0 + dx * dx + dz * dz).sqrt() >= steep;
                spans.push((column(x, z) as u32, Span { low: floor, high: top, walkable }));
            }
        }
    }
    spans.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.low.total_cmp(&b.1.low)));

    // Each column's spans joined where they touch, the walkable top of each with room over it a cell.
    let count = (columns[0] * columns[1]) as usize;
    let mut tops: Vec<Vec<(f32, f32)>> = vec![Vec::new(); count];
    let mut at = 0;
    while at < spans.len() {
        let here = spans[at].0;
        let mut joined: Vec<Span> = Vec::new();
        while at < spans.len() && spans[at].0 == here {
            let span = spans[at].1;
            match joined.last_mut() {
                Some(last) if span.low <= last.high + 0.05 => {
                    if span.high > last.high + 0.01 {
                        (last.high, last.walkable) = (span.high, span.walkable);
                    } else if (span.high - last.high).abs() <= 0.01 {
                        last.walkable |= span.walkable;
                    }
                }
                _ => joined.push(span),
            }
            at += 1;
        }
        for (n, span) in joined.iter().enumerate() {
            let ceiling = joined.get(n + 1).map_or(f32::INFINITY, |over| over.low);
            if span.walkable && ceiling - span.high >= settings.height {
                tops[here as usize].push((span.high, ceiling));
            }
        }
    }

    // Steps between cells beside each other: floors within a step, room for the walker over both.
    let step = |from: (f32, f32), to: (f32, f32)| {
        (from.0 - to.0).abs() <= settings.climb && from.1.min(to.1) - from.0.max(to.0) >= settings.height
    };
    let ways: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];
    let beside = |x: u32, z: u32, way: [i32; 2]| {
        let (nx, nz) = (x as i32 + way[0], z as i32 + way[1]);
        (nx >= 0 && nz >= 0 && (nx as u32) < columns[0] && (nz as u32) < columns[1]).then(|| (nx as u32, nz as u32))
    };
    let neighbour =
        |tops: &Vec<Vec<(f32, f32)>>, x: u32, z: u32, n: usize, way: [i32; 2]| -> Option<(u32, u32, usize)> {
            let (nx, nz) = beside(x, z, way)?;
            let from = tops[column(x, z)][n];
            let found = tops[column(nx, nz)].iter().position(|to| step(from, *to))?;
            Some((nx, nz, found))
        };

    // Kept only as far from every edge as the walker is wide.
    let keep = (settings.radius / cell).ceil() as u32;
    let mut far: Vec<Vec<u32>> = tops.iter().map(|cells| vec![u32::MAX; cells.len()]).collect();
    let mut queue = std::collections::VecDeque::new();
    for z in 0..columns[1] {
        for x in 0..columns[0] {
            for n in 0..tops[column(x, z)].len() {
                if ways.iter().any(|way| neighbour(&tops, x, z, n, *way).is_none()) {
                    far[column(x, z)][n] = 0;
                    queue.push_back((x, z, n));
                }
            }
        }
    }
    while let Some((x, z, n)) = queue.pop_front() {
        let next = far[column(x, z)][n] + 1;
        for way in ways {
            if let Some((nx, nz, found)) = neighbour(&tops, x, z, n, way)
                && far[column(nx, nz)][found] > next
            {
                far[column(nx, nz)][found] = next;
                queue.push_back((nx, nz, found));
            }
        }
    }
    let kept = |x: u32, z: u32, n: usize, far: &Vec<Vec<u32>>| far[column(x, z)][n] >= keep.max(1);

    // Rectangles of kept cells, each grown along x and then z while every cell joins the one before.
    let mut poly_of: Vec<Vec<u32>> = tops.iter().map(|cells| vec![u32::MAX; cells.len()]).collect();
    let mut polys: Vec<Poly> = Vec::new();
    let largest = settings.largest.max(1);
    for z in 0..columns[1] {
        for x in 0..columns[0] {
            for n in 0..tops[column(x, z)].len() {
                if poly_of[column(x, z)][n] != u32::MAX || !kept(x, z, n, &far) {
                    continue;
                }
                let id = polys.len() as u32;
                let free = |x: u32, z: u32, n: usize, poly_of: &Vec<Vec<u32>>| {
                    kept(x, z, n, &far) && poly_of[column(x, z)][n] == u32::MAX
                };
                // Along x: the row's cells, each joined to the one before.
                let mut row = vec![n];
                while row.len() < largest as usize {
                    let (last, lx) = (*row.last().unwrap(), x + row.len() as u32 - 1);
                    match neighbour(&tops, lx, z, last, [1, 0]) {
                        Some((nx, nz, found)) if free(nx, nz, found, &poly_of) => row.push(found),
                        _ => break,
                    }
                }
                // Along z: whole rows, each cell joined to the one under it and to the one before it.
                let mut rows = vec![row];
                'grow: while rows.len() < largest as usize {
                    let (under, uz) = (rows.last().unwrap(), z + rows.len() as u32 - 1);
                    let mut next = Vec::with_capacity(under.len());
                    for (i, &below) in under.iter().enumerate() {
                        let Some((_, _, found)) = neighbour(&tops, x + i as u32, uz, below, [0, 1]) else {
                            break 'grow;
                        };
                        if !free(x + i as u32, uz + 1, found, &poly_of) {
                            break 'grow;
                        }
                        if i > 0
                            && neighbour(&tops, x + i as u32 - 1, uz + 1, next[i - 1], [1, 0]).map(|(.., f)| f)
                                != Some(found)
                        {
                            break 'grow;
                        }
                        next.push(found);
                    }
                    rows.push(next);
                }
                for (dz, row) in rows.iter().enumerate() {
                    for (dx, &found) in row.iter().enumerate() {
                        poly_of[column(x + dx as u32, z + dz as u32)][found] = id;
                    }
                }
                let (w, d) = (rows[0].len() as u32, rows.len() as u32);
                let y = |dx: u32, dz: u32| tops[column(x + dx, z + dz)][rows[dz as usize][dx as usize]].0;
                polys.push(Poly {
                    min: [origin[0] + x as f32 * cell, origin[1] + z as f32 * cell],
                    max: [origin[0] + (x + w) as f32 * cell, origin[1] + (z + d) as f32 * cell],
                    heights: [y(0, 0), y(w - 1, 0), y(w - 1, d - 1), y(0, d - 1)],
                    edges: 0,
                    count: 0,
                });
            }
        }
    }

    // The edges polygons share: each pair's unit edges joined end to end.
    let mut shared: std::collections::HashMap<(u32, u32), (Vec3, Vec3)> = std::collections::HashMap::new();
    for z in 0..columns[1] {
        for x in 0..columns[0] {
            for n in 0..tops[column(x, z)].len() {
                let from = poly_of[column(x, z)][n];
                if from == u32::MAX {
                    continue;
                }
                for way in ways {
                    let Some((nx, nz, found)) = neighbour(&tops, x, z, n, way) else { continue };
                    let to = poly_of[column(nx, nz)][found];
                    if to == u32::MAX || to == from {
                        continue;
                    }
                    let y = (tops[column(x, z)][n].0 + tops[column(nx, nz)][found].0) * 0.5;
                    let corner = |dx: f32, dz: f32| {
                        Vec3::new(origin[0] + (x as f32 + dx) * cell, y, origin[1] + (z as f32 + dz) * cell)
                    };
                    let (a, b) = match way {
                        [1, 0] => (corner(1.0, 0.0), corner(1.0, 1.0)),
                        [-1, 0] => (corner(0.0, 0.0), corner(0.0, 1.0)),
                        [0, 1] => (corner(0.0, 1.0), corner(1.0, 1.0)),
                        _ => (corner(0.0, 0.0), corner(1.0, 0.0)),
                    };
                    let edge = shared.entry((from, to)).or_insert((a, b));
                    edge.0 = edge.0.min(a);
                    edge.1 = edge.1.max(b);
                }
            }
        }
    }
    let mut pairs: Vec<((u32, u32), (Vec3, Vec3))> = shared.into_iter().collect();
    pairs.sort_unstable_by_key(|(key, _)| *key);
    let mut edges = Vec::with_capacity(pairs.len());
    for ((from, to), (a, b)) in pairs {
        let poly = &mut polys[from as usize];
        if poly.count == 0 {
            poly.edges = edges.len() as u32;
        }
        poly.count += 1;
        edges.push(PolyEdge { to, a, b });
    }

    Some(NavMesh::new(settings, origin, columns, polys, edges))
}

/// How a way over a nav mesh ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshPathStatus {
    #[default]
    None,
    Complete,
    Partial,
}

/// A way over a nav mesh: its corners in the world, for each the link it is
/// reached by where it is one's end (by its place among those given), how
/// it ends, and how many polygons were looked at.
#[derive(Clone, Debug, Default)]
pub struct MeshPath {
    pub status: MeshPathStatus,
    pub corners: Vec<Vec3>,
    pub links: Vec<Option<usize>>,
    pub looked: usize,
}

/// A way over a nav mesh that walking does not take: from one point to
/// another in one step, costing as far as it is to its start and `cost`
/// more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshLink {
    pub from: Vec3,
    pub to: Vec3,
    pub cost: f32,
}

/// How a search came into a polygon: where it began, over an edge, or by a
/// link.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Via {
    Start,
    Edge(u32),
    Link(usize),
}

/// Twice the area of the triangle `a`, `b`, `c` on the ground: which side of
/// `a` to `b` the point `c` is on.
fn area2(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (c.x - a.x) * (b.z - a.z) - (b.x - a.x) * (c.z - a.z)
}

impl NavMesh {
    /// A mesh of `polys` joined by `edges`, from a grid of `columns` at
    /// `origin` baked as `settings` say.
    pub(crate) fn new(
        settings: BakeSettings,
        origin: [f32; 2],
        columns: [u32; 2],
        polys: Vec<Poly>,
        edges: Vec<PolyEdge>,
    ) -> Self {
        let buckets = Buckets::of(&polys);
        Self { settings, origin, columns, polys, edges, buckets }
    }

    /// The polygon whose floor is nearest `at`, within `reach` metres up or
    /// down, and across, and the point of its floor nearest: of those
    /// nearest across, a cell at a time, the one nearest up or down.
    pub fn locate(&self, at: Vec3, reach: f32) -> Option<(u32, Vec3)> {
        let cell = self.settings.cell.max(0.05);
        let (low, high) = ([at.x - reach, at.z - reach], [at.x + reach, at.z + reach]);
        let mut best: Option<((u32, f32), u32, Vec3)> = None;
        for n in self.buckets.near(low, high) {
            let poly = &self.polys[n as usize];
            let inset = (cell * 0.05).min((poly.max[0] - poly.min[0]) * 0.5).min((poly.max[1] - poly.min[1]) * 0.5);
            let x = at.x.clamp(poly.min[0] + inset, poly.max[0] - inset);
            let z = at.z.clamp(poly.min[1] + inset, poly.max[1] - inset);
            let across = (at.x - x).hypot(at.z - z);
            let floor = poly.height_at(x, z);
            let off = (floor - at.y).abs();
            if across > reach + inset || off > reach {
                continue;
            }
            let key = ((across / cell).ceil() as u32, off);
            if best.is_none_or(|(kept, ..)| key.0 < kept.0 || key.0 == kept.0 && key.1 < kept.1) {
                best = Some((key, n, Vec3::new(x, floor, z)));
            }
        }
        best.map(|(_, poly, point)| (poly, point))
    }

    /// The edges out of polygon `poly`.
    pub fn edges_of(&self, poly: u32) -> &[PolyEdge] {
        let poly = &self.polys[poly as usize];
        &self.edges[poly.edges as usize..(poly.edges + poly.count) as usize]
    }

    /// The way from `from` to `to`, each within `reach` metres of the mesh.
    pub fn find_path(&self, from: Vec3, to: Vec3, reach: f32) -> MeshPath {
        self.find_path_linked(from, to, reach, &[])
    }

    /// The way from `from` to `to`, each within `reach` metres of the mesh,
    /// taking `links` where they make it shorter: a ladder, a gap, a
    /// Portal, each one step from the polygon under its start to the one
    /// under its end.
    pub fn find_path_linked(&self, from: Vec3, to: Vec3, reach: f32, links: &[MeshLink]) -> MeshPath {
        let Some((start, from)) = self.locate(from, reach.max(self.settings.cell * 2.0)) else {
            return MeshPath::default();
        };
        let goal = self.locate(to, reach);
        let target = goal.map_or(to, |(_, at)| at);
        // Each link's ends on the mesh, by the polygon it leaves from.
        let ends: Vec<Option<(u32, u32, Vec3, Vec3)>> = links
            .iter()
            .map(|link| {
                let near = self.settings.cell * 4.0 + 0.5;
                let (a, at_a) = self.locate(link.from, near)?;
                let (b, at_b) = self.locate(link.to, near)?;
                Some((a, b, at_a, at_b))
            })
            .collect();
        let mut leaving: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
        for (n, end) in ends.iter().enumerate() {
            if let Some((a, ..)) = end {
                leaving.entry(*a).or_default().push(n);
            }
        }
        // An A* over polygons, each entered at the middle of the edge crossed into it, or at a link's end.
        let count = self.polys.len();
        let mut g = vec![f32::INFINITY; count];
        let mut parent = vec![u32::MAX; count];
        let mut via = vec![Via::Start; count];
        let mut entry = vec![Vec3::ZERO; count];
        let mut closed = vec![false; count];
        let mut open = BinaryHeap::new();
        g[start as usize] = 0.0;
        entry[start as usize] = from;
        open.push(Reverse((from.distance(target).to_bits(), start)));
        let mut best = (from.distance(target), start);
        let mut reached = false;
        let mut looked = 0;
        while let Some(Reverse((_, at))) = open.pop() {
            if closed[at as usize] {
                continue;
            }
            closed[at as usize] = true;
            looked += 1;
            if goal.is_some_and(|(poly, _)| poly == at) {
                reached = true;
                best = (0.0, at);
                break;
            }
            let (here, base) = (entry[at as usize], g[at as usize]);
            let mut step = |to: u32, cost: f32, by: Via, into: Vec3| {
                let to_index = to as usize;
                if closed[to_index] || cost >= g[to_index] {
                    return;
                }
                (g[to_index], parent[to_index], via[to_index], entry[to_index]) = (cost, at, by, into);
                let ahead = into.distance(target);
                if ahead < best.0 {
                    best = (ahead, to);
                }
                open.push(Reverse(((cost + ahead).to_bits(), to)));
            };
            for (n, edge) in self.edges_of(at).iter().enumerate() {
                let middle = (edge.a + edge.b) * 0.5;
                let edge_at = self.polys[at as usize].edges + n as u32;
                step(edge.to, base + here.distance(middle), Via::Edge(edge_at), middle);
            }
            for &n in leaving.get(&at).into_iter().flatten() {
                let Some((_, b, a_at, b_at)) = ends[n] else { continue };
                let cost = base + here.distance(a_at) + links[n].cost.max(0.0);
                step(b, cost, Via::Link(n), b_at);
            }
        }
        // The polygons gone through, in turn.
        let mut chain = vec![best.1];
        while let Some(&last) = chain.last() {
            match parent[last as usize] {
                u32::MAX => break,
                before => chain.push(before),
            }
        }
        chain.reverse();
        let end = if reached { target } else { self.nearest_in(best.1, target) };
        // Pulled tight a stretch at a time: each walked stretch through the edges it crosses, a link between two.
        let mut corners: Vec<Vec3> = Vec::new();
        let mut taken: Vec<Option<usize>> = Vec::new();
        let mut portals: Vec<(Vec3, Vec3)> = vec![(from, from)];
        let put =
            |portals: &mut Vec<(Vec3, Vec3)>, last: Vec3, corners: &mut Vec<Vec3>, taken: &mut Vec<Option<usize>>| {
                portals.push((last, last));
                for corner in funnel(portals) {
                    if corners.last() != Some(&corner) {
                        corners.push(corner);
                        taken.push(None);
                    }
                }
            };
        for pair in chain.windows(2) {
            match via[pair[1] as usize] {
                Via::Link(n) => {
                    let (_, _, a_at, b_at) = ends[n].expect("taken, so found");
                    put(&mut portals, a_at, &mut corners, &mut taken);
                    corners.push(b_at);
                    taken.push(Some(n));
                    portals = vec![(b_at, b_at)];
                }
                Via::Edge(edge) => {
                    let edge = self.edges[edge as usize];
                    let (centre, next) = (self.polys[pair[0] as usize].middle(), self.polys[pair[1] as usize].middle());
                    // Facing from one polygon to the next, the edge's end on the left first.
                    let (left, right) =
                        if area2(centre, next, edge.a) < 0.0 { (edge.a, edge.b) } else { (edge.b, edge.a) };
                    portals.push((left, right));
                }
                Via::Start => {}
            }
        }
        put(&mut portals, end, &mut corners, &mut taken);
        let status = if reached { MeshPathStatus::Complete } else { MeshPathStatus::Partial };
        MeshPath { status, corners, links: taken, looked }
    }

    /// The point of polygon `poly` nearest `to`, on its floor.
    pub fn nearest_in(&self, poly: u32, to: Vec3) -> Vec3 {
        let poly = &self.polys[poly as usize];
        let (x, z) = (to.x.clamp(poly.min[0], poly.max[0]), to.z.clamp(poly.min[1], poly.max[1]));
        Vec3::new(x, poly.height_at(x, z), z)
    }

    /// How high the mesh's floor is under `(x, z)` nearest the height `near`.
    pub fn height_at(&self, x: f32, z: f32, near: f32) -> Option<f32> {
        let (poly, _) = self.locate(Vec3::new(x, near, z), 2.0)?;
        Some(self.polys[poly as usize].height_at(x, z))
    }

    /// How many polygons and edges it has, and how many bytes it keeps.
    pub fn counts(&self) -> (usize, usize, usize) {
        let buckets = self.buckets.starts.len() * 4 + self.buckets.polys.len() * 4;
        let bytes = self.polys.len() * 48 + self.edges.len() * 28 + buckets;
        (self.polys.len(), self.edges.len(), bytes)
    }
}

/// The corners of a way through `portals`, each an edge crossed as its left
/// and right ends, the first and last the way's ends: pulled tight, a
/// corner only where the way must turn round an edge's end.
fn funnel(portals: &[(Vec3, Vec3)]) -> Vec<Vec3> {
    let mut corners = vec![portals[0].0];
    let (mut apex, mut left, mut right) = (portals[0].0, portals[0].0, portals[0].1);
    let (mut left_at, mut right_at) = (0, 0);
    let mut apex_at;
    let mut at = 1;
    while at < portals.len() {
        let (l, r) = portals[at];
        // The right side narrows, unless it crosses the left: then the left's end is a corner.
        if area2(apex, right, r) <= 0.0 {
            if apex == right || area2(apex, left, r) > 0.0 {
                (right, right_at) = (r, at);
            } else {
                corners.push(left);
                (apex, apex_at) = (left, left_at);
                (right, right_at) = (apex, apex_at);
                at = apex_at + 1;
                continue;
            }
        }
        if area2(apex, left, l) >= 0.0 {
            if apex == left || area2(apex, right, l) < 0.0 {
                (left, left_at) = (l, at);
            } else {
                corners.push(right);
                (apex, apex_at) = (right, right_at);
                (left, left_at) = (apex, apex_at);
                at = apex_at + 1;
                continue;
            }
        }
        at += 1;
    }
    let end = portals[portals.len() - 1].0;
    if corners.last() != Some(&end) {
        corners.push(end);
    }
    corners
}

/// What a `.nav` file begins with, and its version.
const MAGIC: &[u8; 4] = b"VXNV";
/// Version 1 kept each column's cells as well, which are read past.
const VERSION: u32 = 2;

impl NavMesh {
    /// It as the bytes of a `.nav` file: LZ4 after a short header.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw: Vec<u8> = Vec::new();
        let f32s = |raw: &mut Vec<u8>, numbers: &[f32]| numbers.iter().for_each(|n| raw.extend(n.to_le_bytes()));
        let s = self.settings;
        f32s(&mut raw, &[s.cell, s.height, s.radius, s.climb, s.slope, s.largest as f32]);
        f32s(&mut raw, &self.origin);
        for value in [self.columns[0], self.columns[1], self.polys.len() as u32, self.edges.len() as u32] {
            raw.extend(value.to_le_bytes());
        }
        for poly in &self.polys {
            f32s(&mut raw, &[poly.min[0], poly.min[1], poly.max[0], poly.max[1]]);
            f32s(&mut raw, &poly.heights);
            raw.extend(poly.edges.to_le_bytes());
            raw.extend(poly.count.to_le_bytes());
        }
        for edge in &self.edges {
            raw.extend(edge.to.to_le_bytes());
            f32s(&mut raw, &edge.a.to_array());
            f32s(&mut raw, &edge.b.to_array());
        }
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        out.extend(lz4_flex::compress_prepend_size(&raw));
        out
    }

    /// A nav mesh from a `.nav` file's bytes; `None` where they are not one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let version = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
        if bytes.get(..4)? != MAGIC || !(1..=VERSION).contains(&version) {
            return None;
        }
        let raw = lz4_flex::decompress_size_prepended(bytes.get(8..)?).ok()?;
        let mut at = 0usize;
        let mut word = || -> Option<[u8; 4]> {
            let read = raw.get(at..at + 4)?.try_into().ok()?;
            at += 4;
            Some(read)
        };
        let mut f = || word().map(f32::from_le_bytes);
        let settings =
            BakeSettings { cell: f()?, height: f()?, radius: f()?, climb: f()?, slope: f()?, largest: f()? as u32 };
        let origin = [f()?, f()?];
        let mut u = || word().map(u32::from_le_bytes);
        let columns = [u()?, u()?];
        let (polys_len, edges_len) = match version {
            1 => {
                let (starts_len, cells_len, polys_len, edges_len) =
                    (u()? as usize, u()? as usize, u()? as usize, u()? as usize);
                for _ in 0..starts_len + cells_len * 2 {
                    u()?;
                }
                (polys_len, edges_len)
            }
            _ => (u()? as usize, u()? as usize),
        };
        let polys = (0..polys_len)
            .map(|_| {
                let mut n = || u().map(f32::from_bits);
                let (min, max) = ([n()?, n()?], [n()?, n()?]);
                let heights = [n()?, n()?, n()?, n()?];
                Some(Poly { min, max, heights, edges: u()?, count: u()? })
            })
            .collect::<Option<Vec<_>>>()?;
        let edges = (0..edges_len)
            .map(|_| {
                let to = u()?;
                let mut n = || u().map(f32::from_bits);
                Some(PolyEdge { to, a: Vec3::new(n()?, n()?, n()?), b: Vec3::new(n()?, n()?, n()?) })
            })
            .collect::<Option<Vec<_>>>()?;
        let fits = polys.iter().all(|poly| (poly.edges + poly.count) as usize <= edges.len())
            && edges.iter().all(|edge| (edge.to as usize) < polys.len());
        fits.then(|| Self::new(settings, origin, columns, polys, edges))
    }
}

#[cfg(test)]
mod tests;
