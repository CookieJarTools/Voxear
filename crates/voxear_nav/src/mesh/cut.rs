//! Cutting a baked nav mesh round obstacles while the game runs: a crate
//! pushed into a doorway, a cart left in a street, a gate shut.
//!
//! Each obstacle is a box. Its footprint, widened by the walker's radius
//! and out to whole cells of the grid, is taken out of every polygon under
//! it whose floor the box stands in the way of: a rectangle less a
//! rectangle is up to four rectangles, so a polygon cut is a few smaller
//! ones, their floors following the polygon's. The edges they share with
//! each other and with the polygons beside them are found again, and every
//! other polygon and edge is kept as it was baked. Cutting the baked mesh
//! afresh for every set of obstacles keeps cuts from piling up: an
//! obstacle taken away leaves the mesh as it was.

use voxear_core::math::Vec3;

use super::{NavMesh, Poly, PolyEdge};

/// A rectangle on the ground, from `min` to `max` along x and z.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    min: [f32; 2],
    max: [f32; 2],
}

/// Rectangles narrower than this are no rectangles.
const SLIVER: f32 = 1e-3;

impl Rect {
    fn of(poly: &Poly) -> Self {
        Self { min: poly.min, max: poly.max }
    }

    fn overlaps(&self, other: &Rect) -> bool {
        (0..2).all(|axis| self.min[axis] < other.max[axis] - SLIVER && other.min[axis] < self.max[axis] - SLIVER)
    }

    fn holds(&self, x: f32, z: f32) -> bool {
        (self.min[0]..self.max[0]).contains(&x) && (self.min[1]..self.max[1]).contains(&z)
    }

    /// What is left of it with `cut` taken out: up to four rectangles, the
    /// sides first and then what is between them.
    fn less(&self, cut: &Rect, into: &mut Vec<Rect>) {
        if !self.overlaps(cut) {
            into.push(*self);
            return;
        }
        let mut push = |min: [f32; 2], max: [f32; 2]| {
            if max[0] - min[0] > SLIVER && max[1] - min[1] > SLIVER {
                into.push(Rect { min, max });
            }
        };
        push(self.min, [cut.min[0], self.max[1]]);
        push([cut.max[0], self.min[1]], self.max);
        let (x0, x1) = (self.min[0].max(cut.min[0]), self.max[0].min(cut.max[0]));
        push([x0, self.min[1]], [x1, cut.min[1]]);
        push([x0, cut.max[1]], [x1, self.max[1]]);
    }

    /// The side it shares with `other`, as the ends of it along x and z.
    fn side(&self, other: &Rect) -> Option<([f32; 2], [f32; 2])> {
        let near = |a: f32, b: f32| (a - b).abs() < SLIVER;
        for axis in 0..2 {
            let across = 1 - axis;
            let line = if near(self.max[axis], other.min[axis]) {
                self.max[axis]
            } else if near(self.min[axis], other.max[axis]) {
                self.min[axis]
            } else {
                continue;
            };
            let (low, high) = (self.min[across].max(other.min[across]), self.max[across].min(other.max[across]));
            if high - low <= SLIVER {
                return None;
            }
            let mut a = [0.0; 2];
            let mut b = [0.0; 2];
            (a[axis], b[axis], a[across], b[across]) = (line, line, low, high);
            return Some((a, b));
        }
        None
    }
}

/// A piece of a polygon, on its floor.
fn poly_of(rect: &Rect, floor: &Poly) -> Poly {
    let height = |x: f32, z: f32| floor.height_at(x, z);
    Poly {
        min: rect.min,
        max: rect.max,
        heights: [
            height(rect.min[0], rect.min[1]),
            height(rect.max[0], rect.min[1]),
            height(rect.max[0], rect.max[1]),
            height(rect.min[0], rect.max[1]),
        ],
        edges: 0,
        count: 0,
    }
}

impl NavMesh {
    /// It cut round `boxes`, each from its low corner to its high one, in
    /// the mesh's own space: no way over it goes where one stands. The
    /// mesh itself is left as it is.
    pub fn cut(&self, boxes: &[[Vec3; 2]]) -> NavMesh {
        voxear_core::profile_scope!("Nav Mesh Cut");
        let settings = self.settings;
        let cell = settings.cell.max(0.05);
        // Each box's footprint widened by the walker's radius out to whole cells, and the floors it is in the way of:
        // those under it by less than the walker is tall, up to its top.
        let snap = |value: f32, origin: f32, up: bool| {
            let cells = (value - origin) / cell;
            origin + if up { cells.ceil() } else { cells.floor() } * cell
        };
        let cuts: Vec<(Rect, f32, f32)> = boxes
            .iter()
            .filter(|[low, high]| low.is_finite() && high.is_finite())
            .map(|[low, high]| {
                let (low, high) = (low.min(*high), low.max(*high));
                let r = settings.radius;
                let rect = Rect {
                    min: [snap(low.x - r, self.origin[0], false), snap(low.z - r, self.origin[1], false)],
                    max: [snap(high.x + r, self.origin[0], true), snap(high.z + r, self.origin[1], true)],
                };
                (rect, low.y - settings.height, high.y)
            })
            .collect();

        // Each polygon whole, or the pieces of it left.
        let mut polys: Vec<Poly> = Vec::with_capacity(self.polys.len());
        let mut pieces_of: Vec<(u32, u32, bool)> = Vec::with_capacity(self.polys.len());
        let mut pieces = Vec::new();
        let mut next = Vec::new();
        for poly in &self.polys {
            let whole = Rect::of(poly);
            let (floor_low, floor_high) =
                poly.heights.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), y| (l.min(*y), h.max(*y)));
            pieces.clear();
            pieces.push(whole);
            let mut cut = false;
            for (rect, low, high) in &cuts {
                if !whole.overlaps(rect) || floor_high <= *low || floor_low >= *high {
                    continue;
                }
                cut = true;
                next.clear();
                for piece in &pieces {
                    piece.less(rect, &mut next);
                }
                std::mem::swap(&mut pieces, &mut next);
            }
            let first = polys.len() as u32;
            if cut {
                polys.extend(pieces.iter().map(|piece| poly_of(piece, poly)));
            } else {
                polys.push(Poly { edges: 0, count: 0, ..*poly });
            }
            pieces_of.push((first, polys.len() as u32 - first, cut));
        }
        if pieces_of.iter().all(|(.., cut)| !cut) {
            return self.clone();
        }
        let range = |old: usize| {
            let (first, count, _) = pieces_of[old];
            first..first + count
        };

        // The edges: those between polygons left whole as they were, and those of pieces found again.
        let mut out: Vec<(u32, PolyEdge)> = Vec::with_capacity(self.edges.len() + 64);
        for (old, poly) in self.polys.iter().enumerate() {
            let (_, _, cut) = pieces_of[old];
            for edge in self.edges_of(old as u32) {
                let to = edge.to as usize;
                if !cut && !pieces_of[to].2 {
                    out.push((pieces_of[old].0, PolyEdge { to: pieces_of[to].0, ..*edge }));
                    continue;
                }
                // Clipped to the baked edge: the two polygons meet only along it.
                let (low, high) = (edge.a.min(edge.b), edge.a.max(edge.b));
                let beside = &self.polys[to];
                for from in range(old) {
                    for into in range(to) {
                        let (p, q) = (Rect::of(&polys[from as usize]), Rect::of(&polys[into as usize]));
                        let Some((a, b)) = p.side(&q) else { continue };
                        let clip = |at: [f32; 2]| [at[0].clamp(low.x, high.x), at[1].clamp(low.z, high.z)];
                        let (a, b) = (clip(a), clip(b));
                        if (a[0] - b[0]).abs() + (a[1] - b[1]).abs() <= SLIVER {
                            continue;
                        }
                        let y = |at: [f32; 2]| (poly.height_at(at[0], at[1]) + beside.height_at(at[0], at[1])) * 0.5;
                        let (a, b) = (Vec3::new(a[0], y(a), a[1]), Vec3::new(b[0], y(b), b[1]));
                        out.push((from, PolyEdge { to: into, a, b }));
                    }
                }
            }
            // Pieces of one polygon, where they meet.
            if cut {
                for from in range(old) {
                    for into in range(old) {
                        if from == into {
                            continue;
                        }
                        let (p, q) = (Rect::of(&polys[from as usize]), Rect::of(&polys[into as usize]));
                        if let Some((a, b)) = p.side(&q) {
                            let at = |at: [f32; 2]| Vec3::new(at[0], poly.height_at(at[0], at[1]), at[1]);
                            out.push((from, PolyEdge { to: into, a: at(a), b: at(b) }));
                        }
                    }
                }
            }
        }
        out.sort_by_key(|(from, edge)| (*from, edge.to));
        let mut edges = Vec::with_capacity(out.len());
        for (from, edge) in out {
            let poly = &mut polys[from as usize];
            if poly.count == 0 {
                poly.edges = edges.len() as u32;
            }
            poly.count += 1;
            edges.push(edge);
        }

        // Each column's cells, on the pieces they are in now; those under an obstacle let go.
        let mut starts = Vec::with_capacity(self.starts.len());
        let mut cells = Vec::with_capacity(self.cells.len());
        starts.push(0);
        let columns = self.columns[0] as usize;
        for column in 0..self.starts.len() - 1 {
            let (x, z) = ((column % columns) as f32 + 0.5, (column / columns) as f32 + 0.5);
            let (x, z) = (self.origin[0] + x * cell, self.origin[1] + z * cell);
            for &(floor, old) in &self.cells[self.starts[column] as usize..self.starts[column + 1] as usize] {
                let (first, count, cut) = pieces_of[old as usize];
                if !cut {
                    cells.push((floor, first));
                } else if let Some(piece) = (first..first + count).find(|at| Rect::of(&polys[*at as usize]).holds(x, z))
                {
                    cells.push((floor, piece));
                }
            }
            starts.push(cells.len() as u32);
        }
        NavMesh { settings, origin: self.origin, columns: self.columns, starts, cells, polys, edges }
    }
}
