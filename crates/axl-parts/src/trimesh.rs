//! A plain triangle mesh (millimetres, f64) used while generating parts, and the
//! finished vertex data (metres, f32) handed to the renderer.
//!
//! Nothing here depends on Bevy. Generators build [`TriMesh`] values in millimetres with
//! outward-facing normals; [`TriMesh::finish`] converts one to [`MeshData`] with UVs and
//! tangents for the renderer's micro-surface normal map.

/// A 3-vector.
pub type V3 = [f64; 3];

/// Small vector helpers.
pub mod v3 {
    use super::V3;
    /// a + b
    pub fn add(a: V3, b: V3) -> V3 {
        [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
    }
    /// a - b
    pub fn sub(a: V3, b: V3) -> V3 {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }
    /// a * s
    pub fn mul(a: V3, s: f64) -> V3 {
        [a[0] * s, a[1] * s, a[2] * s]
    }
    /// Dot product.
    pub fn dot(a: V3, b: V3) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    /// Cross product.
    pub fn cross(a: V3, b: V3) -> V3 {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    /// Length.
    pub fn len(a: V3) -> f64 {
        dot(a, a).sqrt()
    }
    /// Unit vector (zero stays zero).
    pub fn norm(a: V3) -> V3 {
        let l = len(a);
        if l < 1e-300 { a } else { mul(a, 1.0 / l) }
    }
}

/// A rotation (rows) and a translation. Applies `R * p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf {
    /// Rotation matrix, row-major.
    pub r: [[f64; 3]; 3],
    /// Translation.
    pub t: V3,
}

impl Xf {
    /// The identity.
    pub const IDENTITY: Xf = Xf {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };
    /// A pure translation.
    pub fn translate(x: f64, y: f64, z: f64) -> Xf {
        Xf {
            t: [x, y, z],
            ..Xf::IDENTITY
        }
    }
    /// Rotation about X by `deg` degrees.
    pub fn rot_x(deg: f64) -> Xf {
        let (s, c) = deg.to_radians().sin_cos();
        Xf {
            r: [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
            t: [0.0; 3],
        }
    }
    /// Rotation about Y by `deg` degrees.
    pub fn rot_y(deg: f64) -> Xf {
        let (s, c) = deg.to_radians().sin_cos();
        Xf {
            r: [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
            t: [0.0; 3],
        }
    }
    /// Rotation about Z by `deg` degrees.
    pub fn rot_z(deg: f64) -> Xf {
        let (s, c) = deg.to_radians().sin_cos();
        Xf {
            r: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }
    /// `self` applied after `first` (that is, `self * first`).
    pub fn after(&self, first: &Xf) -> Xf {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.r[i][k] * first.r[k][j]).sum();
            }
        }
        Xf {
            r,
            t: v3::add(self.rotate(first.t), self.t),
        }
    }
    /// Rotate a vector (no translation).
    pub fn rotate(&self, v: V3) -> V3 {
        [
            self.r[0][0] * v[0] + self.r[0][1] * v[1] + self.r[0][2] * v[2],
            self.r[1][0] * v[0] + self.r[1][1] * v[1] + self.r[1][2] * v[2],
            self.r[2][0] * v[0] + self.r[2][1] * v[1] + self.r[2][2] * v[2],
        ]
    }
    /// Transform a point.
    pub fn apply(&self, p: V3) -> V3 {
        v3::add(self.rotate(p), self.t)
    }
}

/// A triangle mesh with per-vertex normals, in millimetres.
#[derive(Clone, Debug, Default)]
pub struct TriMesh {
    /// Vertex positions, mm.
    pub pos: Vec<V3>,
    /// Unit vertex normals, pointing out of the solid.
    pub nor: Vec<V3>,
    /// Triangle corner indices, counter-clockwise seen from outside.
    pub idx: Vec<u32>,
}

impl TriMesh {
    /// An empty mesh.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a vertex and return its index.
    pub fn vert(&mut self, p: V3, n: V3) -> u32 {
        self.pos.push(p);
        self.nor.push(v3::norm(n));
        (self.pos.len() - 1) as u32
    }

    /// Add a triangle with the given corner order.
    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.idx.extend_from_slice(&[a, b, c]);
    }

    /// Add a triangle, flipping it if its winding disagrees with `want` (a direction it should face).
    pub fn tri_facing(&mut self, a: u32, b: u32, c: u32, want: V3) {
        let (pa, pb, pc) = (self.pos[a as usize], self.pos[b as usize], self.pos[c as usize]);
        let g = v3::cross(v3::sub(pb, pa), v3::sub(pc, pa));
        if v3::dot(g, want) >= 0.0 {
            self.tri(a, b, c);
        } else {
            self.tri(a, c, b);
        }
    }

    /// Add a quad `a b c d` (in loop order) as two triangles facing along the sum of its vertex normals.
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        let want = [a, b, c, d]
            .iter()
            .fold([0.0; 3], |s, &i| v3::add(s, self.nor[i as usize]));
        // Split along the shorter diagonal for a nicer fan.
        let dac = v3::len(v3::sub(self.pos[a as usize], self.pos[c as usize]));
        let dbd = v3::len(v3::sub(self.pos[b as usize], self.pos[d as usize]));
        if dac <= dbd {
            self.tri_facing(a, b, c, want);
            self.tri_facing(a, c, d, want);
        } else {
            self.tri_facing(a, b, d, want);
            self.tri_facing(b, c, d, want);
        }
    }

    /// Add a flat polygon (convex or not, no holes) with one normal, using ear clipping.
    pub fn flat_polygon(&mut self, pts: &[V3], normal: V3) {
        // Project to the plane of the normal and triangulate there.
        let n = v3::norm(normal);
        let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = v3::norm(v3::cross(n, helper));
        let v = v3::cross(n, u);
        let pts2: Vec<[f64; 2]> = pts
            .iter()
            .map(|&p| [v3::dot(p, u), v3::dot(p, v)])
            .collect();
        let tris = crate::earcut::triangulate(&pts2, &[]);
        let base: Vec<u32> = pts.iter().map(|&p| self.vert(p, n)).collect();
        for t in tris.chunks(3) {
            self.tri_facing(base[t[0]], base[t[1]], base[t[2]], n);
        }
    }

    /// Append another mesh.
    pub fn append(&mut self, other: &TriMesh) {
        let base = self.pos.len() as u32;
        self.pos.extend_from_slice(&other.pos);
        self.nor.extend_from_slice(&other.nor);
        self.idx.extend(other.idx.iter().map(|i| i + base));
    }

    /// Move every vertex by a rigid transform (normals are rotated).
    pub fn transform(&mut self, xf: &Xf) {
        for p in &mut self.pos {
            *p = xf.apply(*p);
        }
        for n in &mut self.nor {
            *n = xf.rotate(*n);
        }
    }

    /// A transformed copy.
    pub fn transformed(&self, xf: &Xf) -> TriMesh {
        let mut m = self.clone();
        m.transform(xf);
        m
    }

    /// Number of triangles.
    pub fn triangles(&self) -> usize {
        self.idx.len() / 3
    }

    /// Enclosed volume in mm3 (signed tetrahedra sum; positive when triangles face outwards).
    pub fn volume(&self) -> f64 {
        let mut v = 0.0;
        for t in self.idx.chunks(3) {
            let (a, b, c) = (
                self.pos[t[0] as usize],
                self.pos[t[1] as usize],
                self.pos[t[2] as usize],
            );
            v += v3::dot(a, v3::cross(b, c));
        }
        v / 6.0
    }

    /// Surface area in mm2.
    pub fn area(&self) -> f64 {
        self.idx
            .chunks(3)
            .map(|t| {
                let (a, b, c) = (
                    self.pos[t[0] as usize],
                    self.pos[t[1] as usize],
                    self.pos[t[2] as usize],
                );
                0.5 * v3::len(v3::cross(v3::sub(b, a), v3::sub(c, a)))
            })
            .sum()
    }

    /// Axis-aligned bounds (min, max), mm.
    pub fn bounds(&self) -> (V3, V3) {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }

    /// True when the surface has no net boundary: the enclosed volume does not change when the
    /// mesh is moved relative to the origin. This tolerates zero-area slivers that stitch
    /// collinear points, which [`open_edges`](Self::open_edges) would flag.
    pub fn is_closed(&self) -> bool {
        let v0 = self.volume();
        let mut moved = self.clone();
        moved.transform(&Xf::translate(137.0, -251.0, 89.0));
        (moved.volume() - v0).abs() <= 1e-9 * v0.abs().max(1.0)
    }

    /// Count directed edges (by position, to 1e-6 mm) that have no opposite partner.
    /// Zero means the surface is closed and consistently oriented.
    pub fn open_edges(&self) -> usize {
        self.open_edge_list().len()
    }

    /// The unmatched directed edges (start, end), mm.
    pub fn open_edge_list(&self) -> Vec<(V3, V3)> {
        use std::collections::BTreeMap;
        let q = |p: V3| {
            [
                (p[0] * 1e5).round() as i64,
                (p[1] * 1e5).round() as i64,
                (p[2] * 1e5).round() as i64,
            ]
        };
        let mut count: BTreeMap<([i64; 3], [i64; 3]), i32> = BTreeMap::new();
        for t in self.idx.chunks(3) {
            for k in 0..3 {
                let a = q(self.pos[t[k] as usize]);
                let b = q(self.pos[t[(k + 1) % 3] as usize]);
                if a == b {
                    continue;
                }
                *count.entry((a, b)).or_insert(0) += 1;
            }
        }
        let mut open = Vec::new();
        let f = |p: [i64; 3]| [p[0] as f64 * 1e-5, p[1] as f64 * 1e-5, p[2] as f64 * 1e-5];
        for (&(a, b), &n) in &count {
            let back = count.get(&(b, a)).copied().unwrap_or(0);
            for _ in 0..(n - back).max(0) {
                open.push((f(a), f(b)));
            }
        }
        open
    }

    /// Convert to renderer data: metres, f32, with UVs and tangents.
    ///
    /// UVs are a box projection (the axis the normal points along most), in tiles of
    /// [`MICRO_TILE_M`] metres, which is what the micro-surface normal map expects.
    pub fn finish(&self) -> MeshData {
        let mut out = MeshData::default();
        for (p, n) in self.pos.iter().zip(&self.nor) {
            let pm = [p[0] * 1e-3, p[1] * 1e-3, p[2] * 1e-3];
            let a = [n[0].abs(), n[1].abs(), n[2].abs()];
            // (u axis, v axis) for the dominant normal axis.
            let (ua, va) = if a[0] >= a[1] && a[0] >= a[2] {
                (1, 2)
            } else if a[1] >= a[2] {
                (0, 2)
            } else {
                (0, 1)
            };
            let mut tdir = [0.0; 3];
            tdir[ua] = 1.0;
            let mut bdir = [0.0; 3];
            bdir[va] = 1.0;
            // Gram-Schmidt the tangent against the normal.
            let t = v3::norm(v3::sub(tdir, v3::mul(*n, v3::dot(tdir, *n))));
            let w = if v3::dot(v3::cross(*n, t), bdir) >= 0.0 { 1.0 } else { -1.0 };
            out.positions.push([pm[0] as f32, pm[1] as f32, pm[2] as f32]);
            out.normals.push([n[0] as f32, n[1] as f32, n[2] as f32]);
            out.uvs.push([
                (pm[ua] / MICRO_TILE_M) as f32,
                (pm[va] / MICRO_TILE_M) as f32,
            ]);
            out.tangents.push([t[0] as f32, t[1] as f32, t[2] as f32, w]);
        }
        out.indices = self.idx.clone();
        out
    }
}

/// Size in metres that one tile of the micro-surface texture covers.
pub const MICRO_TILE_M: f64 = 0.016;

/// Finished vertex data for one surface of a part: metres, f32, Z-up part frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    /// Positions, metres.
    pub positions: Vec<[f32; 3]>,
    /// Unit normals.
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates for the micro-surface normal map.
    pub uvs: Vec<[f32; 2]>,
    /// Tangents (xyz, handedness in w).
    pub tangents: Vec<[f32; 4]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Number of triangles.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> TriMesh {
        let mut m = TriMesh::new();
        let faces: [(V3, V3, V3); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ];
        for (n, u, v) in faces {
            let c = |s: f64, t: f64| {
                v3::add(n, v3::add(v3::mul(u, s), v3::mul(v, t)))
            };
            let q = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(s, t)| m.vert(c(s, t), n));
            m.quad(q[0], q[1], q[2], q[3]);
        }
        m
    }

    #[test]
    fn cube_has_volume_eight_and_is_closed() {
        let m = cube();
        assert!((m.volume() - 8.0).abs() < 1e-9);
        assert!(m.is_closed());
        assert!((m.area() - 24.0).abs() < 1e-9);
    }

    #[test]
    fn transform_moves_and_keeps_volume() {
        let m = cube().transformed(&Xf::translate(5.0, 0.0, 0.0).after(&Xf::rot_z(33.0)));
        assert!((m.volume() - 8.0).abs() < 1e-9);
        let (lo, _) = m.bounds();
        assert!(lo[0] > 2.0);
    }

    #[test]
    fn finish_converts_to_metres_and_gives_tangents() {
        let d = cube().finish();
        assert!((d.positions[0][0].abs() - 0.001).abs() < 1e-9);
        for (t, n) in d.tangents.iter().zip(&d.normals) {
            let dot = t[0] * n[0] + t[1] * n[1] + t[2] * n[2];
            assert!(dot.abs() < 1e-5);
            assert!(t[3].abs() == 1.0);
        }
    }

    #[test]
    fn xf_composition_order() {
        let a = Xf::translate(1.0, 0.0, 0.0).after(&Xf::rot_z(90.0));
        let p = a.apply([1.0, 0.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-12 && (p[1] - 1.0).abs() < 1e-12);
    }
}
