//! Bricks, plates, tiles and hole bricks: a rounded box shell with a hollow underside,
//! tubes, studs and optional through-holes in the long walls.
//!
//! The outer surface is built from a grid whose border cells follow a quarter-circle bevel
//! (so normals roll smoothly over every edge and corner); the flat middle of each face is
//! triangulated separately, which is how the underside opening and the wall holes are cut.

use crate::extrude::{Cap, extrude};
use crate::lathe::capped_cylinder;
use crate::profile::{Profile, circle};
use crate::trimesh::{TriMesh, V3, Xf, v3};
use std::f64::consts::PI;

/// Stud pitch, mm.
pub const PITCH: f64 = 8.0;
/// Stud diameter, mm.
pub const STUD_D: f64 = 4.8;
/// Stud height, mm.
pub const STUD_H: f64 = 1.7;
/// Wall thickness, mm.
pub const WALL: f64 = 1.2;
/// Thickness of the closed top (ceiling), mm.
pub const TOP_T: f64 = 1.0;
/// Tube outside diameter, mm.
pub const TUBE_OD: f64 = 6.5;
/// Centre post diameter in 1-wide bricks, mm.
pub const POST_D: f64 = 3.0;
/// Through-hole diameter, mm.
pub const HOLE_D: f64 = 4.8;
/// Rounding applied to the top edge of a stud, mm.
pub const STUD_ROUND: f64 = 0.25;
/// Bevel arc segments per 45 degrees on the shell grid.
const K: usize = 2;
const SEGS: usize = 24;

/// Everything that defines a block.
#[derive(Clone, Debug)]
pub struct BlockSpec {
    /// Studs along X.
    pub nx: u32,
    /// Studs along Y.
    pub ny: u32,
    /// Height without studs, mm.
    pub height: f64,
    /// Draw studs on top.
    pub studs: bool,
    /// Hollow underside.
    pub cavity: bool,
    /// Through-holes along Y in the long walls: x position (mm from the centre), z above the bottom.
    pub holes_y: Vec<(f64, f64)>,
    /// Outer edge bevel, mm.
    pub bevel: f64,
}

impl BlockSpec {
    /// Footprint length along X, mm: 8 n - 0.2.
    pub fn len_x(&self) -> f64 {
        PITCH * self.nx as f64 - 0.2
    }
    /// Footprint along Y, mm.
    pub fn len_y(&self) -> f64 {
        PITCH * self.ny as f64 - 0.2
    }
    /// Tube centres (x, y) relative to the block centre.
    pub fn tubes(&self) -> Vec<(f64, f64)> {
        let mut v = Vec::new();
        if !self.cavity || !self.holes_y.is_empty() {
            return v;
        }
        let (nx, ny) = (self.nx as i32, self.ny as i32);
        if nx >= 2 && ny >= 2 {
            for i in 1..nx {
                for j in 1..ny {
                    v.push(((i as f64 - nx as f64 / 2.0) * PITCH, (j as f64 - ny as f64 / 2.0) * PITCH));
                }
            }
        }
        v
    }
    /// Centre posts for 1-wide bricks (not plates).
    pub fn posts(&self) -> Vec<(f64, f64)> {
        let mut v = Vec::new();
        if !self.cavity || !self.holes_y.is_empty() || self.height < 8.0 {
            return v;
        }
        let (nx, ny) = (self.nx as i32, self.ny as i32);
        if ny == 1 && nx >= 2 {
            for i in 1..nx {
                v.push(((i as f64 - nx as f64 / 2.0) * PITCH, 0.0));
            }
        } else if nx == 1 && ny >= 2 {
            for j in 1..ny {
                v.push((0.0, (j as f64 - ny as f64 / 2.0) * PITCH));
            }
        }
        v
    }
    /// Stud centres relative to the block centre.
    pub fn stud_positions(&self) -> Vec<(f64, f64)> {
        let mut v = Vec::new();
        for i in 0..self.nx {
            for j in 0..self.ny {
                v.push((
                    (i as f64 - (self.nx as f64 - 1.0) / 2.0) * PITCH,
                    (j as f64 - (self.ny as f64 - 1.0) / 2.0) * PITCH,
                ));
            }
        }
        v
    }
    /// Analytic solid volume, mm3: outer box, minus the hollow, plus tubes, posts and studs,
    /// minus the wall holes, minus the volume the bevels remove. The stud top rounding
    /// (under 0.5 % of a stud) is ignored.
    pub fn volume(&self) -> f64 {
        let (l, w, h) = (self.len_x(), self.len_y(), self.height);
        let mut v = l * w * h;
        // Twelve rounded edges (corners overlap, ignored: they are below 0.01 %).
        let edge_len = 4.0 * (l + w + h) - 8.0 * 0.0;
        v -= crate::extrude::bevel_loss(edge_len, self.bevel);
        let depth = h - TOP_T;
        if self.cavity {
            v -= (l - 2.0 * WALL) * (w - 2.0 * WALL) * depth;
            let tube = PI * (TUBE_OD * TUBE_OD - STUD_D * STUD_D) / 4.0 * depth;
            v += self.tubes().len() as f64 * tube;
            v += self.posts().len() as f64 * PI * POST_D * POST_D / 4.0 * depth;
        }
        v -= self.holes_y.len() as f64 * PI * HOLE_D * HOLE_D / 4.0 * 2.0 * WALL;
        if self.studs {
            v += self.stud_positions().len() as f64 * PI * STUD_D * STUD_D / 4.0 * STUD_H;
        }
        v
    }
}

/// Break coordinates across one axis of a face: equal-angle steps over each bevel.
fn breaks(h: f64, b: f64) -> Vec<f64> {
    let inner = h - b;
    let mut v = Vec::new();
    for i in 0..=K {
        let th = std::f64::consts::FRAC_PI_4 * (K - i) as f64 / K as f64;
        v.push(-(inner + b * th.tan()));
    }
    for i in 0..=K {
        let th = std::f64::consts::FRAC_PI_4 * i as f64 / K as f64;
        v.push(inner + b * th.tan());
    }
    v
}

fn flat_with_holes(m: &mut TriMesh, rect: [[f64; 2]; 2], holes: &[(f64, f64, f64)], map: &dyn Fn(f64, f64) -> V3, normal: V3) {
    let [[x0, x1], [y0, y1]] = rect;
    let mut data = vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut starts = Vec::new();
    for &(cx, cy, r) in holes {
        starts.push(data.len());
        data.extend(circle(cx, cy, r, SEGS));
    }
    let tris = crate::earcut::triangulate(&data, &starts);
    let base: Vec<u32> = data.iter().map(|p| m.vert(map(p[0], p[1]), normal)).collect();
    for t in tris.chunks(3) {
        m.tri_facing(base[t[0]], base[t[1]], base[t[2]], normal);
    }
}

/// A rounded box with the given half sizes, centred at the origin. The bottom (-Z) face has a
/// rectangular opening of half sizes `cavity` if given, and the +/-Y faces get circular
/// holes (x, z, r) which are bored through to the cavity.
fn shell(h: V3, b: f64, cavity: Option<(f64, f64, f64)>, holes_y: &[(f64, f64, f64)]) -> TriMesh {
    let mut m = TriMesh::new();
    let inner = [h[0] - b, h[1] - b, h[2] - b];
    let br = [breaks(h[0], b), breaks(h[1], b), breaks(h[2], b)];
    for a in 0..3 {
        for s in [-1.0f64, 1.0] {
            let (b1, b2) = ((a + 1) % 3, (a + 2) % 3);
            let ring = a == 2 && s < 0.0 && cavity.is_some();
            let ext = |br: &Vec<f64>, c: f64| -> Vec<f64> {
                if !ring {
                    return br.clone();
                }
                let mut v = br[..=K].to_vec();
                v.push(-c);
                v.push(c);
                v.extend_from_slice(&br[K + 1..]);
                v
            };
            let (cxy, _) = cavity.map(|c| ((c.0, c.1), 0)).unwrap_or(((0.0, 0.0), 0));
            let cb = [cxy.0, cxy.1, 0.0];
            let (u, v) = (ext(&br[b1], cb[b1]), ext(&br[b2], cb[b2]));
            let (nu, nv) = (u.len(), v.len());
            let skip = if ring { K + 1 } else { K };
            let mut grid = vec![0u32; nu * nv];
            for i in 0..nu {
                for j in 0..nv {
                    let mut q = [0.0; 3];
                    q[a] = s * h[a];
                    q[b1] = u[i];
                    q[b2] = v[j];
                    let c = [
                        q[0].clamp(-inner[0], inner[0]),
                        q[1].clamp(-inner[1], inner[1]),
                        q[2].clamp(-inner[2], inner[2]),
                    ];
                    let d = v3::sub(q, c);
                    let n = v3::norm(d);
                    grid[i * nv + j] = m.vert(v3::add(c, v3::mul(n, b)), n);
                }
            }
            let flat_hi = if ring { K + 3 } else { K + 1 };
            let fj: Vec<usize> = (K..=flat_hi).collect();
            let bevels: Vec<usize> = (0..K).chain(flat_hi..flat_hi + K).collect();
            let strip = |m: &mut TriMesh, l: &[u32], r: &[u32]| {
                let want = l.iter().chain(r).fold([0.0; 3], |s, &i| v3::add(s, m.nor[i as usize]));
                let (mut i, mut j) = (0, 0);
                while i + 1 < l.len() || j + 1 < r.len() {
                    let adv_left = j + 1 >= r.len() || (i + 1 < l.len() && (i + 1) * (r.len() - 1) <= (j + 1) * (l.len() - 1));
                    if adv_left {
                        m.tri_facing(l[i], r[j], l[i + 1], want);
                        i += 1;
                    } else {
                        m.tri_facing(l[i], r[j], r[j + 1], want);
                        j += 1;
                    }
                }
            };
            // Bevel columns, and bevel rows, along the flat range: split only inside, so the
            // outermost seam to the neighbouring face keeps its single edge.
            for &i in &bevels {
                let edge = |b: usize| -> Vec<u32> {
                    if b == 0 || b == nu - 1 {
                        vec![grid[b * nv + K], grid[b * nv + flat_hi]]
                    } else {
                        fj.iter().map(|&j| grid[b * nv + j]).collect()
                    }
                };
                strip(&mut m, &edge(i), &edge(i + 1));
            }
            for &j in &bevels {
                let edge = |b: usize| -> Vec<u32> {
                    if b == 0 || b == nv - 1 {
                        vec![grid[K * nv + b], grid[flat_hi * nv + b]]
                    } else {
                        fj.iter().map(|&i| grid[i * nv + b]).collect()
                    }
                };
                strip(&mut m, &edge(j), &edge(j + 1));
            }
            let cell = |m: &mut TriMesh, (i0, i1): (usize, usize), (j0, j1): (usize, usize)| {
                m.quad(grid[i0 * nv + j0], grid[i1 * nv + j0], grid[i1 * nv + j1], grid[i0 * nv + j1]);
            };
            for &i in &bevels {
                for &j in &bevels {
                    cell(&mut m, (i, i + 1), (j, j + 1));
                }
            }
            if ring {
                for i in K..K + 3 {
                    for j in K..K + 3 {
                        if !(i == skip && j == skip) {
                            cell(&mut m, (i, i + 1), (j, j + 1));
                        }
                    }
                }
            }
            let mut nrm = [0.0; 3];
            nrm[a] = s;
            let mapf = |p: f64, q: f64| -> V3 {
                let mut o = [0.0; 3];
                o[a] = s * h[a] - 0.0;
                o[b1] = p;
                o[b2] = q;
                // The flat region sits `b` inside the outermost plane's bevel start: it IS on the plane.
                o
            };
            let rect = [[-inner[b1], inner[b1]], [-inner[b2], inner[b2]]];
            if ring {
                // The opening is the skipped cell of the grid above.
            } else if a == 1 && !holes_y.is_empty() {
                // Face normal +-Y: tangent axes are (z, x); holes are given as (x, z, r).
                let hs: Vec<(f64, f64, f64)> = holes_y.iter().map(|&(x, z, r)| (z, x, r)).collect();
                flat_with_holes(&mut m, rect, &hs, &mapf, nrm);
            } else {
                let q: Vec<u32> = [(0, 0), (1, 0), (1, 1), (0, 1)]
                    .iter()
                    .map(|&(i, j)| m.vert(mapf(rect[0][i], rect[1][j]), nrm))
                    .collect();
                m.quad(q[0], q[1], q[2], q[3]);
            }
        }
    }
    if let Some((cx, cy, ztop)) = cavity {
        let zb = -h[2];
        // Ceiling.
        let c = [[-cx, -cy], [cx, -cy], [cx, cy], [-cx, cy]].map(|p| m.vert([p[0], p[1], ztop], [0.0, 0.0, -1.0]));
        m.quad(c[0], c[1], c[2], c[3]);
        // Inner walls, normals into the cavity. X walls are plain; Y walls may have holes.
        for s in [-1.0f64, 1.0] {
            let nx = [-s, 0.0, 0.0];
            let q = [(-cy, zb), (cy, zb), (cy, ztop), (-cy, ztop)].map(|(y, z)| m.vert([s * cx, y, z], nx));
            m.quad(q[0], q[1], q[2], q[3]);
            let ny = [0.0, -s, 0.0];
            let mapf = |z: f64, x: f64| -> V3 { [x, s * cy, z] };
            let hs: Vec<(f64, f64, f64)> = holes_y.iter().map(|&(x, z, r)| (z, x, r)).collect();
            flat_with_holes(&mut m, [[zb, ztop], [-cx, cx]], &hs, &mapf, ny);
            // Bores from the outer face to the inner wall.
            for &(x, z, r) in holes_y {
                let mut rows = [vec![], vec![]];
                for k in 0..=SEGS {
                    let a = std::f64::consts::TAU * k as f64 / SEGS as f64;
                    let (sa, ca) = a.sin_cos();
                    let n = [-ca, 0.0, -sa];
                    rows[0].push(m.vert([x + r * ca, s * h[1], z + r * sa], n));
                    rows[1].push(m.vert([x + r * ca, s * cy, z + r * sa], n));
                }
                for k in 0..SEGS {
                    m.quad(rows[0][k], rows[0][k + 1], rows[1][k + 1], rows[1][k]);
                }
            }
        }
    }
    m
}

/// Generate the block mesh (part frame: origin at the centre of the bottom face).
pub fn block_mesh(spec: &BlockSpec) -> TriMesh {
    let (l, w, h) = (spec.len_x(), spec.len_y(), spec.height);
    let hz = h / 2.0;
    let cav = spec.cavity.then(|| (l / 2.0 - WALL, w / 2.0 - WALL, hz - TOP_T));
    let holes: Vec<(f64, f64, f64)> = spec.holes_y.iter().map(|&(x, z)| (x, z - hz, HOLE_D / 2.0)).collect();
    let mut m = shell([l / 2.0, w / 2.0, hz], spec.bevel, cav, &holes);
    m.transform(&Xf::translate(0.0, 0.0, hz));
    if spec.cavity {
        let depth = h - TOP_T;
        for (x, y) in spec.tubes() {
            let p = Profile::solid(circle(0.0, 0.0, TUBE_OD / 2.0, SEGS)).with_hole(circle(0.0, 0.0, STUD_D / 2.0, SEGS));
            let mut t = extrude(&p, 0.0, depth, Some(Cap::SHARP), Some(Cap::SHARP));
            t.transform(&Xf::translate(x, y, 0.0));
            m.append(&t);
        }
        for (x, y) in spec.posts() {
            let p = Profile::solid(circle(0.0, 0.0, POST_D / 2.0, 16));
            let mut t = extrude(&p, 0.0, depth, Some(Cap::SHARP), Some(Cap::SHARP));
            t.transform(&Xf::translate(x, y, 0.0));
            m.append(&t);
        }
    }
    if spec.studs {
        let stud = capped_cylinder(STUD_D / 2.0, STUD_H, STUD_ROUND, SEGS);
        for (x, y) in spec.stud_positions() {
            m.append(&stud.transformed(&Xf::translate(x, y, h)));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brick(nx: u32, ny: u32) -> BlockSpec {
        BlockSpec { nx, ny, height: 9.6, studs: true, cavity: true, holes_y: vec![], bevel: 0.15 }
    }

    #[test]
    fn brick_2x4_is_closed_and_mesh_volume_matches_analytic() {
        let s = brick(4, 2);
        let m = block_mesh(&s);
        assert_eq!(m.open_edges(), 0);
        let (v, a) = (m.volume(), s.volume());
        assert!((v - a).abs() / a < 0.01, "mesh {v} analytic {a}");
    }

    #[test]
    fn footprint_matches_the_spec() {
        let m = block_mesh(&brick(4, 2));
        let (lo, hi) = m.bounds();
        assert!((hi[0] - lo[0] - 31.8).abs() < 1e-9);
        assert!((hi[1] - lo[1] - 15.8).abs() < 1e-9);
        assert!((hi[2] - 9.6 - 1.7).abs() < 1e-9 && lo[2].abs() < 1e-9);
    }

    #[test]
    fn plate_tile_and_1x1_work() {
        for s in [
            BlockSpec { height: 3.2, ..brick(2, 4) },
            BlockSpec { height: 3.2, studs: false, ..brick(2, 2) },
            brick(1, 1),
            brick(1, 6),
        ] {
            let m = block_mesh(&s);
            assert_eq!(m.open_edges(), 0);
            assert!((m.volume() - s.volume()).abs() / s.volume() < 0.012, "{}x{}", s.nx, s.ny);
        }
    }

    #[test]
    fn hole_brick_has_bores_and_is_closed() {
        let s = BlockSpec { holes_y: vec![(-8.0, 4.8), (0.0, 4.8), (8.0, 4.8)], ..brick(4, 1) };
        let m = block_mesh(&s);
        assert_eq!(m.open_edges(), 0);
        assert!((m.volume() - s.volume()).abs() / s.volume() < 0.012);
    }
}
