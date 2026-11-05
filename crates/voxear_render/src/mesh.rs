//! CPU-side mesh data and the built-in primitives.
//!
//! Winding follows Unity: seen from outside, a front face is clockwise in a
//! left-handed, Y-up world. In terms of plain vector maths that means
//! `cross(p1 - p0, p2 - p0)` points along the outward normal.

use voxear_core::math::Vec3;

/// The engine-wide vertex. Shaders pull it from a buffer by index, so the
/// layout matches `Vertex` in `shaders/common.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// xyz along +u; w is the bitangent sign.
    pub tangent: [f32; 4],
    /// How the wind moves it, 0 to 255 each (see `shaders/wind.slang`): how
    /// much its leaf flutters, its own phase, and how far its branch sways
    /// apart from the trunk; the fourth is unused. `NO_WIND_MASKS` leaves
    /// the plant to bend as one.
    pub wind: [u8; 4],
}

/// Wind masks that leave a vertex moving as a plant without any does: its
/// leaves fluttering, in phase with the plant, and no branch sway.
pub const NO_WIND_MASKS: [u8; 4] = [255, 0, 0, 0];

impl Default for Vertex {
    fn default() -> Self {
        Self { position: [0.0; 3], normal: [0.0; 3], uv: [0.0; 2], tangent: [0.0; 4], wind: NO_WIND_MASKS }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// A mesh's triangles, for physics and the editor's picking: its points,
/// and three indices a triangle, at its full detail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshTriangles {
    pub points: Vec<Vec3>,
    pub indices: Vec<u32>,
}

impl MeshTriangles {
    /// How far along a ray, in the mesh's own space, it first meets one of
    /// the triangles, from either side (Möller-Trumbore).
    pub fn first_hit(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let mut nearest: Option<f32> = None;
        for triangle in self.indices.chunks_exact(3) {
            let corner = |i: usize| self.points.get(triangle[i] as usize).copied();
            let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else { continue };
            let (ab, ac) = (b - a, c - a);
            let p = direction.cross(ac);
            let det = ab.dot(p);
            if det.abs() < 1e-12 {
                continue;
            }
            let inverse = 1.0 / det;
            let s = origin - a;
            let u = s.dot(p) * inverse;
            let q = s.cross(ab);
            let v = direction.dot(q) * inverse;
            let t = ac.dot(q) * inverse;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0 && nearest.is_none_or(|n| t < n) {
                nearest = Some(t);
            }
        }
        nearest
    }
}

impl MeshData {
    pub fn triangles(&self) -> MeshTriangles {
        let points = self.vertices.iter().map(|vertex| Vec3::from(vertex.position)).collect();
        MeshTriangles { points, indices: self.indices.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    Cube,
    Sphere,
    /// 1×1, facing +Y.
    Plane,
}

impl MeshData {
    pub fn primitive(primitive: Primitive) -> Self {
        match primitive {
            Primitive::Cube => Self::cube(),
            Primitive::Sphere => Self::sphere(64, 32),
            Primitive::Plane => Self::plane(),
        }
    }

    /// Bounding sphere: the box centre and the farthest vertex from it.
    pub fn bounds(&self) -> (Vec3, f32) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            min = min.min(Vec3::from(v.position));
            max = max.max(Vec3::from(v.position));
        }
        if self.vertices.is_empty() {
            return (Vec3::ZERO, 0.0);
        }
        let center = (min + max) * 0.5;
        let radius = self.vertices.iter().map(|v| Vec3::from(v.position).distance(center)).fold(0.0, f32::max);
        (center, radius)
    }

    /// A unit cube centred on the origin, with hard edges.
    pub fn cube() -> Self {
        let mut mesh = Self::default();
        for normal in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
            let up = if normal.y.abs() > 0.5 { Vec3::Z } else { Vec3::Y };
            mesh.add_quad(normal * 0.5, normal, up, 0.5);
        }
        mesh
    }

    pub fn plane() -> Self {
        let mut mesh = Self::default();
        mesh.add_quad(Vec3::ZERO, Vec3::Y, Vec3::Z, 0.5);
        mesh
    }

    /// A UV sphere of radius 0.5.
    pub fn sphere(segments: u32, rings: u32) -> Self {
        let mut mesh = Self::default();
        for ring in 0..=rings {
            let v = ring as f32 / rings as f32;
            let theta = v * std::f32::consts::PI;
            for segment in 0..=segments {
                let u = segment as f32 / segments as f32;
                let phi = u * std::f32::consts::TAU;
                let normal = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
                let tangent = Vec3::new(-phi.sin(), 0.0, phi.cos());
                mesh.vertices.push(Vertex {
                    position: (normal * 0.5).into(),
                    normal: normal.into(),
                    uv: [u, v],
                    tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                    wind: NO_WIND_MASKS,
                });
            }
        }
        let stride = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let a = ring * stride + segment;
                let b = a + stride;
                mesh.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        mesh.orient_outward();
        mesh
    }

    /// A square of half-size `half` centred on `center`, facing `normal`, with
    /// `up` as the texture's up direction.
    fn add_quad(&mut self, center: Vec3, normal: Vec3, up: Vec3, half: f32) {
        let right = normal.cross(up);
        let base = self.vertices.len() as u32;
        for (s, t) in [(-1.0, 1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let position = center + right * (s * half) + up * (t * half);
            self.vertices.push(Vertex {
                position: position.into(),
                normal: normal.into(),
                uv: [(s + 1.0) * 0.5, (1.0 - t) * 0.5],
                tangent: [right.x, right.y, right.z, 1.0],
                wind: NO_WIND_MASKS,
            });
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Flips any triangle whose winding disagrees with its vertex normals.
    fn orient_outward(&mut self) {
        for tri in self.indices.as_chunks_mut::<3>().0 {
            let [p0, p1, p2] = tri.map(|i| Vec3::from(self.vertices[i as usize].position));
            let normal: Vec3 = tri.map(|i| Vec3::from(self.vertices[i as usize].normal)).iter().sum();
            if (p1 - p0).cross(p2 - p0).dot(normal) < 0.0 {
                tri.swap(1, 2);
            }
        }
    }

    /// Right-handed (glTF) to left-handed (Unity): mirror X and flip winding.
    pub fn mirror_x(&mut self) {
        for v in &mut self.vertices {
            v.position[0] = -v.position[0];
            v.normal[0] = -v.normal[0];
            v.tangent[0] = -v.tangent[0];
            // Mirroring flips the bitangent's handedness.
            v.tangent[3] = -v.tangent[3];
        }
        for tri in self.indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_outward(mesh: &MeshData) {
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [p0, p1, p2] = tri.map(|i| Vec3::from(mesh.vertices[i as usize].position));
            let face = (p1 - p0).cross(p2 - p0);
            if face.length() < 1e-6 {
                continue;
            }
            let normal = Vec3::from(mesh.vertices[tri[0] as usize].normal);
            assert!(face.dot(normal) > 0.0, "triangle {tri:?} winds inward");
        }
    }

    #[test]
    fn primitives_wind_outward() {
        assert_outward(&MeshData::cube());
        assert_outward(&MeshData::plane());
        assert_outward(&MeshData::sphere(16, 8));
    }

    #[test]
    fn plane_faces_up_and_cube_is_unit() {
        let plane = MeshData::plane();
        assert!(plane.vertices.iter().all(|v| v.normal == [0.0, 1.0, 0.0]));
        let (center, radius) = MeshData::cube().bounds();
        assert!(center.length() < 1e-6);
        assert!((radius - 3f32.sqrt() * 0.5).abs() < 1e-5);
    }

    #[test]
    fn mirroring_keeps_triangles_outward() {
        let mut cube = MeshData::cube();
        cube.mirror_x();
        assert_outward(&cube);
    }
}
