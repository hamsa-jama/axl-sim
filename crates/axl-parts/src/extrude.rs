//! Extrusion of a [`Profile`] along Z with rounded (bevelled) cap edges and smooth normals.
//!
//! The side wall and each bevel are built as rows of vertices that share normals, so light
//! rolls smoothly over the edge. Outer loops are pushed in by `cap.outer` at the cap, hole
//! loops are pushed out (into the material) by `cap.holes`.

use crate::profile::{Profile, P2};
use crate::trimesh::{TriMesh, V3};

/// Bevel widths for one cap. A zero width gives a sharp edge. `None` instead of a `Cap`
/// leaves the end open (the caller closes it).
#[derive(Clone, Copy, Debug)]
pub struct Cap {
    /// Bevel on the outer loop, mm.
    pub outer: f64,
    /// Bevel on hole loops, mm.
    pub holes: f64,
}

impl Cap {
    /// A flat cap with no bevel.
    pub const SHARP: Cap = Cap { outer: 0.0, holes: 0.0 };
    /// The same bevel on every loop.
    pub fn all(b: f64) -> Cap {
        Cap { outer: b, holes: b }
    }
}

/// Bevel arc segments (3 gives a visibly round edge at 0.15 mm).
pub const BEVEL_SEGS: usize = 3;

struct Row {
    pos: Vec<V3>,
    nor: Vec<V3>,
    idx: Vec<u32>,
}

fn loop_normals(l: &[P2]) -> (Vec<P2>, Vec<f64>) {
    let n = l.len();
    let edge_n: Vec<P2> = (0..n)
        .map(|i| {
            let (a, b) = (l[i], l[(i + 1) % n]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-12);
            [d[1] / len, -d[0] / len]
        })
        .collect();
    let mut vn = Vec::with_capacity(n);
    let mut miter = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (edge_n[(i + n - 1) % n], edge_n[i]);
        let s = [a[0] + b[0], a[1] + b[1]];
        let len = (s[0] * s[0] + s[1] * s[1]).sqrt();
        let v = if len < 1e-9 { b } else { [s[0] / len, s[1] / len] };
        vn.push(v);
        let c = (v[0] * b[0] + v[1] * b[1]).max(0.5);
        miter.push(1.0 / c);
    }
    (vn, miter)
}

/// Extrude `profile` from `z0` to `z1` (`z1 > z0`).
pub fn extrude(profile: &Profile, z0: f64, z1: f64, cap0: Option<Cap>, cap1: Option<Cap>) -> TriMesh {
    let prof = profile.normalised();
    let mut mesh = TriMesh::new();
    let mut cap_loops: [Vec<Vec<P2>>; 2] = [vec![], vec![]];
    let height = z1 - z0;
    let loops: Vec<(&Vec<P2>, bool)> = std::iter::once((&prof.outer, false))
        .chain(prof.holes.iter().map(|h| (h, true)))
        .collect();

    for (l, is_hole) in loops {
        let (vn, miter) = loop_normals(l);
        let n = l.len();
        let bevel = |cap: Option<Cap>| match cap {
            Some(c) => (if is_hole { c.holes } else { c.outer }).min(height * 0.5),
            None => 0.0,
        };
        let (b0, b1) = (bevel(cap0), bevel(cap1));
        let mut rows: Vec<Row> = Vec::new();
        let mut mk_row = |mesh: &mut TriMesh, f: &dyn Fn(usize) -> (V3, V3)| {
            let mut row = Row { pos: vec![], nor: vec![], idx: vec![] };
            for i in 0..n {
                let (p, nn) = f(i);
                row.idx.push(mesh.vert(p, nn));
                row.pos.push(p);
                row.nor.push(nn);
            }
            rows.push(row);
        };
        // Bottom bevel rows, from the cap edge (z0) up to where the wall starts.
        let arc = |i: usize, zc: f64, s: f64, b: f64, th: f64| -> (V3, V3) {
            let (st, ct) = th.sin_cos();
            let off = b * miter[i] * (1.0 - ct);
            (
                [l[i][0] - vn[i][0] * off, l[i][1] - vn[i][1] * off, zc - s * b * (1.0 - st)],
                [vn[i][0] * ct, vn[i][1] * ct, s * st],
            )
        };
        if cap0.is_some() && b0 > 1e-9 {
            for j in (0..=BEVEL_SEGS).rev() {
                let th = std::f64::consts::FRAC_PI_2 * j as f64 / BEVEL_SEGS as f64;
                mk_row(&mut mesh, &|i| arc(i, z0, -1.0, b0, th));
            }
        } else {
            mk_row(&mut mesh, &|i| ([l[i][0], l[i][1], z0], [vn[i][0], vn[i][1], 0.0]));
        }
        if cap1.is_some() && b1 > 1e-9 {
            for j in 0..=BEVEL_SEGS {
                let th = std::f64::consts::FRAC_PI_2 * j as f64 / BEVEL_SEGS as f64;
                mk_row(&mut mesh, &|i| arc(i, z1, 1.0, b1, th));
            }
        } else {
            mk_row(&mut mesh, &|i| ([l[i][0], l[i][1], z1], [vn[i][0], vn[i][1], 0.0]));
        }
        // Strips between consecutive rows.
        for w in rows.windows(2) {
            for i in 0..n {
                let j = (i + 1) % n;
                mesh.quad(w[0].idx[i], w[0].idx[j], w[1].idx[j], w[1].idx[i]);
            }
        }
        // Cap rings.
        if cap0.is_some() {
            cap_loops[0].push(rows[0].pos.iter().map(|p| [p[0], p[1]]).collect());
        }
        if cap1.is_some() {
            cap_loops[1].push(rows[rows.len() - 1].pos.iter().map(|p| [p[0], p[1]]).collect());
        }
    }
    for (side, z, s) in [(0usize, z0, -1.0), (1, z1, 1.0)] {
        let ls = &cap_loops[side];
        if ls.is_empty() {
            continue;
        }
        let mut data: Vec<P2> = Vec::new();
        let mut starts = Vec::new();
        for (k, l) in ls.iter().enumerate() {
            if k > 0 {
                starts.push(data.len());
            }
            data.extend(l);
        }
        let tris = crate::earcut::triangulate(&data, &starts);
        let base: Vec<u32> = data.iter().map(|p| mesh.vert([p[0], p[1], z], [0.0, 0.0, s])).collect();
        let mut seen = std::collections::BTreeSet::new();
        for t in tris.chunks(3) {
            let mut key = [t[0], t[1], t[2]];
            key.sort();
            if !seen.insert(key) {
                continue;
            }
            mesh.tri_facing(base[t[0]], base[t[1]], base[t[2]], [0.0, 0.0, s]);
        }
    }
    mesh
}

/// Volume removed by a bevel of width `b` along `perimeter` (one rounded edge), mm3.
pub fn bevel_loss(perimeter: f64, b: f64) -> f64 {
    (1.0 - std::f64::consts::FRAC_PI_4) * b * b * perimeter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{circle, perimeter, rect};
    use std::f64::consts::PI;

    #[test]
    fn plain_prism_volume_and_closed() {
        let p = Profile::solid(rect(0.0, 0.0, 4.0, 3.0));
        let m = extrude(&p, 0.0, 2.0, Some(Cap::SHARP), Some(Cap::SHARP));
        assert!((m.volume() - 24.0).abs() < 1e-9);
        assert!(m.is_closed());
    }

    #[test]
    fn bevelled_cylinder_volume_matches_the_formula() {
        let r = 5.0;
        let c = circle(0.0, 0.0, r, 96);
        let p = Profile::solid(c.clone());
        let b = 0.5;
        let m = extrude(&p, 0.0, 4.0, Some(Cap::all(b)), Some(Cap::all(b)));
        assert!(m.is_closed());
        let poly_area = crate::profile::signed_area(&c);
        let want = poly_area * 4.0 - 2.0 * bevel_loss(perimeter(&c), b);
        assert!((m.volume() - want).abs() / want < 0.003, "{} vs {want}", m.volume());
        let ideal = PI * r * r * 4.0;
        assert!((m.volume() - ideal).abs() / ideal < 0.015);
    }

    #[test]
    fn holes_are_subtracted_and_surface_is_closed() {
        let p = Profile::solid(rect(0.0, 0.0, 20.0, 10.0)).with_hole(circle(0.0, 0.0, 2.4, 48));
        let m = extrude(&p, 0.0, 5.0, Some(Cap::all(0.3)), Some(Cap::all(0.3)));
        assert!(m.is_closed());
        let ideal = (200.0 - PI * 2.4 * 2.4) * 5.0;
        assert!((m.volume() - ideal).abs() / ideal < 0.01, "{} vs {ideal}", m.volume());
    }

    #[test]
    fn normals_agree_with_winding() {
        let p = Profile::solid(circle(0.0, 0.0, 3.0, 24)).with_hole(circle(0.0, 0.0, 1.0, 24));
        let m = extrude(&p, 0.0, 2.0, Some(Cap::all(0.2)), Some(Cap::all(0.2)));
        for t in m.idx.chunks(3) {
            let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
            let g = crate::trimesh::v3::cross(crate::trimesh::v3::sub(b, a), crate::trimesh::v3::sub(c, a));
            let n = m.nor[t[0] as usize];
            if crate::trimesh::v3::len(g) > 1e-12 {
                assert!(crate::trimesh::v3::dot(g, n) > -1e-9);
            }
        }
    }
}
