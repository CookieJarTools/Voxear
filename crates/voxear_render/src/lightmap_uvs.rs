//! Lightmap UVs made for a mesh that has none of its own: a second set of
//! UVs in which no two triangles overlap, every part of the surface takes
//! room for its size, and the pieces are kept apart, so a lightmap can hold
//! the light falling on each point of the surface.
//!
//! The surface is cut into charts: from each triangle not yet taken, the
//! triangles joined to it by an edge grow a chart while they face within
//! `CHART_ANGLE` of its average facing. Each chart is laid flat along that
//! facing, at its true size in metres, and turned so its longest way lies
//! along U. The charts are then packed in rows, tallest first, into a
//! square, each with a margin round it, and the square is scaled to 0 to 1.
//! A vertex on the edge of two charts becomes two, one for each.
//!
//! The margin is in lightmap texels, at the texels a metre a mesh of its
//! size is expected to be baked at; a bake at fewer texels a metre has
//! thinner margins, which the bake's dilation covers.

use std::collections::HashMap;

use voxear_core::math::{Vec2, Vec3};

use crate::mesh::MeshData;

/// How far apart, in degrees, a chart's triangles may face from its
/// average: more makes fewer charts and more stretching on the slopes.
const CHART_ANGLE: f32 = 60.0;
/// Texels of margin round each chart, at the expected texels a metre.
pub const MARGIN_TEXELS: f32 = 2.0;
/// Texels a metre a mesh is expected to be baked at, which the margins are
/// worked out for; as the levels' default.
pub const EXPECTED_TEXELS_PER_METRE: f32 = 10.0;
/// The fewest and most texels across a mesh's lightmap the margins are
/// worked out for: a small mesh is often given more texels than its size
/// asks for, by its Obj's scale or the level's, and margins worked out for
/// a few texels across would take most of its square.
const EXPECTED_TEXELS: (f32, f32) = (64.0, 1024.0);

/// A chart: its triangles, the facing it is laid flat along and its two
/// axes on that plane.
struct Chart {
    triangles: Vec<usize>,
    u: Vec3,
    v: Vec3,
    /// Its flattened box's low corner and size in its own axes, and where
    /// that corner goes once packed.
    low: Vec2,
    size: Vec2,
    at: Vec2,
}

/// Makes lightmap UVs for a mesh, splitting its vertices where charts meet;
/// its skin, if it has one, is split with them. A mesh with no triangles
/// is left as it is.
pub fn generate(data: &mut MeshData) {
    let triangles = data.indices.len() / 3;
    if triangles == 0 {
        return;
    }
    let positions: Vec<Vec3> = data.vertices.iter().map(|v| Vec3::from(v.position)).collect();
    let corner = |t: usize, c: usize| data.indices[t * 3 + c] as usize;
    let faces: Vec<Vec3> = (0..triangles)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|i| positions[corner(t, i)]);
            (b - a).cross(c - a)
        })
        .collect();

    let neighbours = neighbours(data, &positions);
    let mut charts = grow_charts(&faces, &neighbours);
    let total: f32 = faces.iter().map(|f| f.length() * 0.5).sum();
    for chart in &mut charts {
        flatten(chart, data, &positions);
    }
    let expected = (total.sqrt() * EXPECTED_TEXELS_PER_METRE).clamp(EXPECTED_TEXELS.0, EXPECTED_TEXELS.1);
    let side = pack(&mut charts, MARGIN_TEXELS / expected);

    // Each vertex as many times as charts it is in, at its place in each.
    let mut chart_of = vec![0usize; triangles];
    for (index, chart) in charts.iter().enumerate() {
        for &t in &chart.triangles {
            chart_of[t] = index;
        }
    }
    let mut made: HashMap<(u32, usize), u32> = HashMap::new();
    let mut vertices = Vec::with_capacity(data.vertices.len());
    let mut skin = Vec::with_capacity(data.skin.len());
    let mut uvs = Vec::with_capacity(data.vertices.len());
    let mut indices = Vec::with_capacity(data.indices.len());
    for t in 0..triangles {
        let chart = &charts[chart_of[t]];
        for c in 0..3 {
            let old = data.indices[t * 3 + c];
            let index = *made.entry((old, chart_of[t])).or_insert_with(|| {
                vertices.push(data.vertices[old as usize]);
                skin.extend(data.skin.get(old as usize).copied());
                uvs.push(place(chart, positions[old as usize], side));
                vertices.len() as u32 - 1
            });
            indices.push(index);
        }
    }
    data.vertices = vertices;
    data.skin = skin;
    data.indices = indices;
    data.lightmap_uvs = uvs;
}

/// The triangles that share an edge with each triangle, by where their
/// corners are rather than which vertices they use, as a surface's UV seams
/// split its vertices without cutting it.
fn neighbours(data: &MeshData, positions: &[Vec3]) -> Vec<Vec<usize>> {
    let (low, high) =
        positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let step = ((high - low).max_element() * 1e-6).max(1e-9);
    let mut welded: HashMap<[i64; 3], u32> = HashMap::new();
    let ids: Vec<u32> = positions
        .iter()
        .map(|p| {
            let key = ((*p - low) / step).round().to_array().map(|c| c as i64);
            let next = welded.len() as u32;
            *welded.entry(key).or_insert(next)
        })
        .collect();
    let triangles = data.indices.len() / 3;
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for t in 0..triangles {
        for c in 0..3 {
            let a = ids[data.indices[t * 3 + c] as usize];
            let b = ids[data.indices[t * 3 + (c + 1) % 3] as usize];
            if a != b {
                edges.entry((a.min(b), a.max(b))).or_default().push(t);
            }
        }
    }
    let mut around = vec![Vec::new(); triangles];
    let mut keys: Vec<&(u32, u32)> = edges.keys().collect();
    keys.sort_unstable();
    for key in keys {
        let sharing = &edges[key];
        for &a in sharing {
            around[a].extend(sharing.iter().copied().filter(|&b| b != a));
        }
    }
    for list in &mut around {
        list.sort_unstable();
        list.dedup();
    }
    around
}

/// Grows charts from the triangles in order, each taking the triangles
/// joined to it that face within `CHART_ANGLE` of its average facing.
fn grow_charts(faces: &[Vec3], neighbours: &[Vec<usize>]) -> Vec<Chart> {
    let within = CHART_ANGLE.to_radians().cos();
    let mut taken = vec![false; faces.len()];
    let mut charts = Vec::new();
    for seed in 0..faces.len() {
        if taken[seed] {
            continue;
        }
        taken[seed] = true;
        let mut sum = faces[seed];
        let mut facing = sum.normalize_or(Vec3::Y);
        let mut triangles = vec![seed];
        let mut next = 0;
        while next < triangles.len() {
            let t = triangles[next];
            next += 1;
            for &n in &neighbours[t] {
                let unit = faces[n].normalize_or_zero();
                if taken[n] || unit.dot(facing) < within {
                    continue;
                }
                taken[n] = true;
                triangles.push(n);
                sum += faces[n];
                facing = sum.normalize_or(facing);
            }
        }
        let u = facing.any_orthonormal_vector();
        let v = facing.cross(u);
        charts.push(Chart { triangles, u, v, low: Vec2::ZERO, size: Vec2::ZERO, at: Vec2::ZERO });
    }
    charts
}

/// Lays a chart flat on its plane, turned so the box round it is as small
/// as it can be: the smallest box round a shape lies along one of the edges
/// of its convex hull (Freeman and Shapira 1975), so each is tried.
fn flatten(chart: &mut Chart, data: &MeshData, positions: &[Vec3]) {
    let flat = |p: Vec3, u: Vec3, v: Vec3| Vec2::new(p.dot(u), p.dot(v));
    let mut points: Vec<Vec2> = chart
        .triangles
        .iter()
        .flat_map(|&t| (0..3).map(move |c| t * 3 + c))
        .map(|i| flat(positions[data.indices[i] as usize], chart.u, chart.v))
        .collect();
    let hull = convex_hull(&mut points);
    let bounds = |angle: f32| {
        let (s, c) = angle.sin_cos();
        hull.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| {
            let turned = Vec2::new(p.x * c + p.y * s, p.y * c - p.x * s);
            (lo.min(turned), hi.max(turned))
        })
    };
    let area = |(lo, hi): (Vec2, Vec2)| (hi - lo).x * (hi - lo).y;
    let mut best = (0.0, area(bounds(0.0)));
    for (i, &a) in hull.iter().enumerate() {
        let edge = hull[(i + 1) % hull.len()] - a;
        if edge.length_squared() < 1e-12 {
            continue;
        }
        let angle = edge.y.atan2(edge.x);
        let tried = area(bounds(angle));
        if tried < best.1 * 0.999 {
            best = (angle, tried);
        }
    }
    let (s, c) = best.0.sin_cos();
    let (u, v) = (chart.u * c + chart.v * s, chart.v * c - chart.u * s);
    (chart.u, chart.v) = (u, v);
    let (mut low, mut high) = bounds(best.0);
    // Wider than tall, so the rows pack tighter.
    if (high - low).y > (high - low).x {
        (chart.u, chart.v) = (-chart.v, chart.u);
        (low, high) = (Vec2::new(-high.y, low.x), Vec2::new(-low.y, high.x));
    }
    chart.low = low;
    chart.size = (high - low).max(Vec2::splat(1e-6));
}

/// The convex hull of points, anticlockwise (Andrew's monotone chain).
fn convex_hull(points: &mut [Vec2]) -> Vec<Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    if points.len() < 3 {
        return points.to_vec();
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<Vec2> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Vec2>> =
            if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for &p in ordered {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Packs the charts in rows, tallest first, into a square, each with
/// `margin` of the square's side round it, and returns the square's side
/// in metres. Each chart's `at` is where its box's low corner goes.
fn pack(charts: &mut [Chart], margin: f32) -> f32 {
    let mut order: Vec<usize> = (0..charts.len()).collect();
    order.sort_by(|&a, &b| charts[b].size.y.total_cmp(&charts[a].size.y).then(a.cmp(&b)));
    let area: f32 = charts.iter().map(|c| c.size.x * c.size.y).sum();
    let mut side = area.sqrt().max(1e-6);
    for _ in 0..200 {
        let gap = side * margin;
        if shelves(charts, &order, side, gap) <= side {
            return side;
        }
        side *= 1.03;
    }
    // Never left unpacked: whatever the last try made, scaled into the square.
    let gap = side * margin;
    let height = shelves(charts, &order, side, gap);
    side.max(height)
}

/// Lays the charts in `order` along rows `width` wide, each `gap` from its
/// neighbours and the edges; returns how high the rows reach.
fn shelves(charts: &mut [Chart], order: &[usize], width: f32, gap: f32) -> f32 {
    let (mut x, mut y, mut row) = (gap, gap, 0.0f32);
    for &index in order {
        let size = charts[index].size;
        if x + size.x + gap > width && x > gap {
            y += row + gap;
            (x, row) = (gap, 0.0);
        }
        charts[index].at = Vec2::new(x, y);
        x += size.x + gap;
        row = row.max(size.y);
    }
    y + row + gap
}

/// Where a point of a chart lies in the packed square, from 0 to 1.
fn place(chart: &Chart, point: Vec3, side: f32) -> [f32; 2] {
    let flat = Vec2::new(point.dot(chart.u), point.dot(chart.v));
    let packed = (flat - chart.low + chart.at) / side;
    packed.clamp(Vec2::ZERO, Vec2::ONE).to_array()
}

/// How much of the 0-to-1 square a mesh's lightmap UVs cover, and the
/// area of its surface in its own units: what a bake sizes its lightmap by.
pub fn areas(data: &MeshData) -> (f32, f32) {
    let mut uv_area = 0.0;
    let mut surface = 0.0;
    for t in data.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| t[i] as usize);
        let p = |i: usize| Vec3::from(data.vertices[i].position);
        surface += (p(b) - p(a)).cross(p(c) - p(a)).length() * 0.5;
        if let (Some(&ua), Some(&ub), Some(&uc)) =
            (data.lightmap_uvs.get(a), data.lightmap_uvs.get(b), data.lightmap_uvs.get(c))
        {
            let (ua, ub, uc) = (Vec2::from(ua), Vec2::from(ub), Vec2::from(uc));
            uv_area += ((ub - ua).perp_dot(uc - ua) * 0.5).abs();
        }
    }
    (uv_area, surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether any two triangles of the lightmap UVs overlap, by marking a
    /// fine grid with each triangle's inside and finding a cell marked twice.
    fn overlaps(data: &MeshData, cells: usize) -> bool {
        let mut grid = vec![u32::MAX; cells * cells];
        for (t, tri) in data.indices.chunks_exact(3).enumerate() {
            let uv = |i: usize| Vec2::from(data.lightmap_uvs[tri[i] as usize]) * cells as f32;
            let (a, b, c) = (uv(0), uv(1), uv(2));
            let low = a.min(b).min(c).floor().max(Vec2::ZERO);
            let high = a.max(b).max(c).ceil().min(Vec2::splat(cells as f32));
            for y in low.y as usize..high.y as usize {
                for x in low.x as usize..high.x as usize {
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let side = |e0: Vec2, e1: Vec2| (e1 - e0).perp_dot(p - e0);
                    let (s0, s1, s2) = (side(a, b), side(b, c), side(c, a));
                    let inside = (s0 > 0.0 && s1 > 0.0 && s2 > 0.0) || (s0 < 0.0 && s1 < 0.0 && s2 < 0.0);
                    if !inside {
                        continue;
                    }
                    let cell = &mut grid[y * cells + x];
                    if *cell != u32::MAX && *cell != t as u32 {
                        return true;
                    }
                    *cell = t as u32;
                }
            }
        }
        false
    }

    #[test]
    fn a_cubes_six_faces_lie_apart_each_its_own_size() {
        let mut cube = MeshData::cube();
        generate(&mut cube);
        assert_eq!(cube.lightmap_uvs.len(), cube.vertices.len());
        assert!(!overlaps(&cube, 256), "no two triangles share texels");
        let (uv, surface) = areas(&cube);
        assert!((surface - 6.0).abs() < 1e-4);
        assert!(uv > 0.3 && uv < 1.0, "the faces fill a fair share of the square: {uv}");
        assert!(cube.lightmap_uvs.iter().all(|uv| uv.iter().all(|c| (0.0..=1.0).contains(c))));
    }

    #[test]
    fn a_sphere_is_cut_into_charts_that_do_not_overlap() {
        let mut sphere = MeshData::sphere(32, 16);
        let before = sphere.vertices.len();
        generate(&mut sphere);
        assert!(sphere.vertices.len() >= before, "vertices split where charts meet");
        assert!(!overlaps(&sphere, 512));
        let (uv, _) = areas(&sphere);
        assert!(uv > 0.25, "{uv}");
    }

    #[test]
    fn the_same_mesh_gets_the_same_uvs() {
        let mut a = MeshData::sphere(24, 12);
        let mut b = a.clone();
        generate(&mut a);
        generate(&mut b);
        assert_eq!(a.lightmap_uvs, b.lightmap_uvs);
        assert_eq!(a.indices, b.indices);
    }

    #[test]
    fn charts_keep_their_margins_apart() {
        let mut cube = MeshData::cube();
        generate(&mut cube);
        // At the texels the margins are made for, a unit cube is 64 texels across at the least: every
        // pair of charts is at least two margins apart, so their texels and those dilated round them never meet.
        let charts: Vec<Vec<Vec2>> = cube
            .indices
            .chunks_exact(6)
            .map(|quad| quad.iter().map(|&i| Vec2::from(cube.lightmap_uvs[i as usize])).collect())
            .collect();
        let expected = (6.0f32.sqrt() * EXPECTED_TEXELS_PER_METRE).clamp(EXPECTED_TEXELS.0, EXPECTED_TEXELS.1);
        let texel = 1.0 / expected;
        for (i, a) in charts.iter().enumerate() {
            for b in &charts[i + 1..] {
                let (alow, ahigh) = a.iter().fold((Vec2::MAX, Vec2::MIN), |(l, h), &p| (l.min(p), h.max(p)));
                let (blow, bhigh) = b.iter().fold((Vec2::MAX, Vec2::MIN), |(l, h), &p| (l.min(p), h.max(p)));
                let apart = (blow - ahigh).max(alow - bhigh).max_element();
                assert!(apart >= MARGIN_TEXELS * texel * 0.99, "{apart} apart, margin {}", MARGIN_TEXELS * texel);
            }
        }
    }
}
