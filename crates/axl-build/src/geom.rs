//! Connectors and collision shapes in the robot frame, the connector hash grid, and the rules for
//! which connectors can join (SPEC.md, "Part catalog" and "Builder").

use std::collections::{BTreeSet, HashMap};

use axl_parts::catalog::Catalog;
use axl_parts::types::{Axis, CollisionPrim, ConnKind, PartDef};

use crate::model::{Assembly, ConnRef, Connection, JointKind};
use crate::orient::{Orient, axis_index, flip};

/// Cell size of the coarse grid used for radius searches: 6 mm in 0.1 mm steps.
pub const SNAP_RADIUS: i32 = 60;

/// A connector placed in the robot frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldConn {
    /// Which connector.
    pub r: ConnRef,
    /// Its kind.
    pub kind: ConnKind,
    /// Position, 0.1 mm steps.
    pub pos: [i32; 3],
    /// Axis in the robot frame.
    pub axis: Axis,
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// All connectors of a part placed at `pos` with `orient`.
pub fn part_conns(def: &PartDef, uid: u32, pos: [i32; 3], orient: Orient) -> Vec<WorldConn> {
    let mut out = Vec::new();
    for (bi, body) in def.bodies.iter().enumerate() {
        for (ci, c) in body.connectors.iter().enumerate() {
            out.push(WorldConn {
                r: ConnRef {
                    uid,
                    body: bi as u8,
                    conn: ci as u16,
                },
                kind: c.kind,
                pos: add(pos, orient.rotate(c.pos)),
                axis: orient.rotate_axis(c.axis),
            });
        }
    }
    out
}

/// Do two connectors join, and how? The rule is symmetric (SPEC.md connector table).
pub fn compat(a: &WorldConn, b: &WorldConn) -> Option<JointKind> {
    use ConnKind::*;
    let parallel = axis_index(a.axis) == axis_index(b.axis);
    let opposite = a.axis == flip(b.axis);
    let one_way = |m: &WorldConn, f: &WorldConn| -> Option<JointKind> {
        match (m.kind, f.kind) {
            (Stud, AntiStud) if opposite => Some(JointKind::Rigid),
            (PinShaft { friction }, RoundHole) if parallel => Some(JointKind::Hinge { friction }),
            (AxleShaft { .. }, RoundHole) if parallel => Some(JointKind::Hinge { friction: false }),
            (AxleShaft { .. }, AxleHole) if parallel => Some(JointKind::Rigid),
            _ => None,
        }
    };
    one_way(a, b).or_else(|| one_way(b, a))
}

/// A shaft that goes through the other part's hole. Such pairs overlap in their collision shapes
/// by design, so the overlap check skips them.
pub fn is_shaft_in_hole(a: &WorldConn, b: &WorldConn) -> bool {
    let shaft = |k: ConnKind| matches!(k, ConnKind::PinShaft { .. } | ConnKind::AxleShaft { .. });
    let hole = |k: ConnKind| matches!(k, ConnKind::RoundHole | ConnKind::AxleHole);
    (shaft(a.kind) && hole(b.kind)) || (shaft(b.kind) && hole(a.kind))
}

/// Connector lookup: exact position (constant time) and a coarse grid for radius searches.
#[derive(Clone, Debug, Default)]
pub struct ConnIndex {
    /// Every connector, in part order.
    pub conns: Vec<WorldConn>,
    fine: HashMap<[i32; 3], Vec<u32>>,
    coarse: HashMap<[i32; 3], Vec<u32>>,
}

fn cell(p: [i32; 3]) -> [i32; 3] {
    p.map(|v| v.div_euclid(SNAP_RADIUS))
}

impl ConnIndex {
    /// Index every connector of the given parts (parts whose id is not in the catalog are skipped).
    pub fn build(cat: &Catalog, asm: &Assembly) -> Self {
        let mut idx = ConnIndex::default();
        for p in &asm.parts {
            if let Some(def) = cat.get(&p.part.0) {
                for wc in part_conns(def, p.uid, p.pos, p.orient) {
                    idx.insert(wc);
                }
            }
        }
        idx
    }

    fn insert(&mut self, wc: WorldConn) {
        let i = self.conns.len() as u32;
        self.fine.entry(wc.pos).or_default().push(i);
        self.coarse.entry(cell(wc.pos)).or_default().push(i);
        self.conns.push(wc);
    }

    /// Connectors at exactly this position.
    pub fn at(&self, pos: [i32; 3]) -> impl Iterator<Item = &WorldConn> {
        self.fine
            .get(&pos)
            .into_iter()
            .flatten()
            .map(|&i| &self.conns[i as usize])
    }

    /// Connectors whose position is within `radius` (0.1 mm steps) of `pos`.
    pub fn near(&self, pos: [i32; 3], radius: i32) -> Vec<&WorldConn> {
        let c = cell(pos);
        let mut out = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.coarse.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &i in v {
                            let w = &self.conns[i as usize];
                            let d: i64 = (0..3)
                                .map(|k| {
                                    let e = (w.pos[k] - pos[k]) as i64;
                                    e * e
                                })
                                .sum();
                            if d <= (radius as i64) * (radius as i64) {
                                out.push(w);
                            }
                        }
                    }
                }
            }
        }
        out.sort_by_key(|w| w.r);
        out
    }

    /// Every connection between connectors of different parts, sorted, each pair once.
    pub fn connections(&self) -> Vec<Connection> {
        let mut set = BTreeSet::new();
        for a in &self.conns {
            for b in self.at(a.pos) {
                if a.r.uid == b.r.uid || a.r >= b.r {
                    continue;
                }
                if let Some(kind) = compat(a, b) {
                    set.insert(Connection {
                        a: a.r,
                        b: b.r,
                        kind,
                    });
                }
            }
        }
        set.into_iter().collect()
    }
}

/// Derive all connections of an assembly (never stored).
pub fn derive_connections(cat: &Catalog, asm: &Assembly) -> Vec<Connection> {
    ConnIndex::build(cat, asm).connections()
}

/// A collision shape in the robot frame, millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WPrim {
    /// Axis-aligned box.
    Box {
        /// Minimum corner.
        lo: [f64; 3],
        /// Maximum corner.
        hi: [f64; 3],
    },
    /// Cylinder whose axis is a coordinate axis.
    Cyl {
        /// Centre.
        c: [f64; 3],
        /// Axis index 0..3.
        ax: usize,
        /// Radius.
        r: f64,
        /// Half length.
        h: f64,
    },
    /// Sphere.
    Sphere {
        /// Centre.
        c: [f64; 3],
        /// Radius.
        r: f64,
    },
}

impl WPrim {
    /// Axis-aligned bounds (lo, hi).
    pub fn aabb(&self) -> ([f64; 3], [f64; 3]) {
        match *self {
            WPrim::Box { lo, hi } => (lo, hi),
            WPrim::Cyl { c, ax, r, h } => {
                let mut lo = [c[0] - r, c[1] - r, c[2] - r];
                let mut hi = [c[0] + r, c[1] + r, c[2] + r];
                lo[ax] = c[ax] - h;
                hi[ax] = c[ax] + h;
                (lo, hi)
            }
            WPrim::Sphere { c, r } => (
                [c[0] - r, c[1] - r, c[2] - r],
                [c[0] + r, c[1] + r, c[2] + r],
            ),
        }
    }
}

/// The collision shapes of a part, per sub-body, in the robot frame (mm).
pub fn part_prims(def: &PartDef, pos: [i32; 3], orient: Orient) -> Vec<(u8, WPrim)> {
    let origin = pos.map(|v| v as f64 * 0.1);
    let place = |p: [f64; 3]| -> [f64; 3] {
        let r = orient.rotate_f([p[0] * 1000.0, p[1] * 1000.0, p[2] * 1000.0]);
        [origin[0] + r[0], origin[1] + r[1], origin[2] + r[2]]
    };
    let mut out = Vec::new();
    for (bi, body) in def.bodies.iter().enumerate() {
        for prim in &body.collision {
            let w = match *prim {
                CollisionPrim::Box { center, half } => {
                    let c = place(center);
                    let hv =
                        orient.rotate_f([half[0] * 1000.0, half[1] * 1000.0, half[2] * 1000.0]);
                    let h = hv.map(f64::abs);
                    WPrim::Box {
                        lo: [c[0] - h[0], c[1] - h[1], c[2] - h[2]],
                        hi: [c[0] + h[0], c[1] + h[1], c[2] + h[2]],
                    }
                }
                CollisionPrim::Cylinder {
                    center,
                    axis,
                    radius,
                    half_len,
                } => WPrim::Cyl {
                    c: place(center),
                    ax: axis_index(orient.rotate_axis(axis)),
                    r: radius * 1000.0,
                    h: half_len * 1000.0,
                },
                CollisionPrim::Sphere { center, radius } => WPrim::Sphere {
                    c: place(center),
                    r: radius * 1000.0,
                },
            };
            out.push((bi as u8, w));
        }
    }
    out
}

fn interval_overlap(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.1.min(b.1) - a.0.max(b.0)
}

fn others(ax: usize) -> (usize, usize) {
    match ax {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// How deep two shapes sit inside each other in mm. Zero or negative means they do not overlap.
/// Where the exact value is awkward (two crossed cylinders) the box bounds are used, which can only
/// over-report: the builder never accepts an overlap, it may refuse a touching pair.
pub fn penetration(a: &WPrim, b: &WPrim) -> f64 {
    use WPrim::*;
    match (a, b) {
        (Box { lo: al, hi: ah }, Box { lo: bl, hi: bh }) => {
            let mut m = f64::MAX;
            for k in 0..3 {
                m = m.min(interval_overlap((al[k], ah[k]), (bl[k], bh[k])));
            }
            m
        }
        (Sphere { c, r }, Box { lo, hi }) | (Box { lo, hi }, Sphere { c, r }) => {
            sphere_box(*c, *r, *lo, *hi)
        }
        (Cyl { c, ax, r, h }, Box { lo, hi }) | (Box { lo, hi }, Cyl { c, ax, r, h }) => {
            let axial = interval_overlap((c[*ax] - h, c[*ax] + h), (lo[*ax], hi[*ax]));
            if axial <= 0.0 {
                return axial;
            }
            let (p, q) = others(*ax);
            let d = circle_rect([c[p], c[q]], *r, [lo[p], lo[q]], [hi[p], hi[q]]);
            axial.min(d)
        }
        (Sphere { c: c1, r: r1 }, Sphere { c: c2, r: r2 }) => r1 + r2 - dist(*c1, *c2),
        (
            Sphere { c, r },
            Cyl {
                c: cc,
                ax,
                r: cr,
                h,
            },
        )
        | (
            Cyl {
                c: cc,
                ax,
                r: cr,
                h,
            },
            Sphere { c, r },
        ) => {
            let (p, q) = others(*ax);
            let mut near = *c;
            near[*ax] = c[*ax].clamp(cc[*ax] - h, cc[*ax] + h);
            let dx = c[p] - cc[p];
            let dy = c[q] - cc[q];
            let rad = (dx * dx + dy * dy).sqrt();
            if rad > *cr && rad > 0.0 {
                near[p] = cc[p] + dx / rad * cr;
                near[q] = cc[q] + dy / rad * cr;
            }
            let inside = near == *c;
            if inside {
                return *r + (cr - rad).min(h - (c[*ax] - cc[*ax]).abs());
            }
            r - dist(*c, near)
        }
        (
            Cyl {
                c: c1,
                ax: a1,
                r: r1,
                h: h1,
            },
            Cyl {
                c: c2,
                ax: a2,
                r: r2,
                h: h2,
            },
        ) => {
            if a1 == a2 {
                let axial =
                    interval_overlap((c1[*a1] - h1, c1[*a1] + h1), (c2[*a1] - h2, c2[*a1] + h2));
                if axial <= 0.0 {
                    return axial;
                }
                let (p, q) = others(*a1);
                let d = ((c1[p] - c2[p]).powi(2) + (c1[q] - c2[q]).powi(2)).sqrt();
                axial.min(r1 + r2 - d)
            } else {
                let (al, ah) = a.aabb();
                let (bl, bh) = b.aabb();
                penetration(&Box { lo: al, hi: ah }, &Box { lo: bl, hi: bh })
            }
        }
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn sphere_box(c: [f64; 3], r: f64, lo: [f64; 3], hi: [f64; 3]) -> f64 {
    let mut q = c;
    for k in 0..3 {
        q[k] = c[k].clamp(lo[k], hi[k]);
    }
    if q == c {
        // Centre inside the box: depth is the radius plus the distance to the nearest face.
        let mut m = f64::MAX;
        for k in 0..3 {
            m = m.min(c[k] - lo[k]).min(hi[k] - c[k]);
        }
        return r + m;
    }
    r - dist(c, q)
}

fn circle_rect(c: [f64; 2], r: f64, lo: [f64; 2], hi: [f64; 2]) -> f64 {
    let qx = c[0].clamp(lo[0], hi[0]);
    let qy = c[1].clamp(lo[1], hi[1]);
    if qx == c[0] && qy == c[1] {
        let m = (c[0] - lo[0])
            .min(hi[0] - c[0])
            .min(c[1] - lo[1])
            .min(hi[1] - c[1]);
        return r + m;
    }
    r - ((c[0] - qx).powi(2) + (c[1] - qy).powi(2)).sqrt()
}

/// Overlaps deeper than this (mm) are rejected; touching and 0.1 mm of give are allowed.
pub const OVERLAP_TOLERANCE_MM: f64 = 0.1;

/// Cheap bounds test before the exact one.
pub fn aabb_may_overlap(a: &WPrim, b: &WPrim) -> bool {
    let (al, ah) = a.aabb();
    let (bl, bh) = b.aabb();
    (0..3).all(|k| al[k] < bh[k] - OVERLAP_TOLERANCE_MM && bl[k] < ah[k] - OVERLAP_TOLERANCE_MM)
}

/// Do two sets of shapes overlap by more than the tolerance?
pub fn prims_overlap(a: &[(u8, WPrim)], b: &[(u8, WPrim)]) -> bool {
    a.iter().any(|(_, p)| {
        b.iter()
            .any(|(_, q)| aabb_may_overlap(p, q) && penetration(p, q) > OVERLAP_TOLERANCE_MM)
    })
}

/// Pairs of part uids that overlap, with the shaft-through-hole pairs exempt. `conns` is the
/// derived connection list of the same assembly.
pub fn overlapping_pairs(
    cat: &Catalog,
    asm: &Assembly,
    idx: &ConnIndex,
    conns: &[Connection],
) -> Vec<(u32, u32)> {
    let exempt = exempt_pairs(idx, conns);
    let prims: Vec<(u32, Vec<(u8, WPrim)>)> = asm
        .parts
        .iter()
        .filter_map(|p| {
            cat.get(&p.part.0)
                .map(|d| (p.uid, part_prims(d, p.pos, p.orient)))
        })
        .collect();
    let mut out = Vec::new();
    for i in 0..prims.len() {
        for j in i + 1..prims.len() {
            let key = (prims[i].0.min(prims[j].0), prims[i].0.max(prims[j].0));
            if exempt.contains(&key) {
                continue;
            }
            if prims_overlap(&prims[i].1, &prims[j].1) {
                out.push(key);
            }
        }
    }
    out
}

/// Part pairs joined by a shaft in a hole.
pub fn exempt_pairs(idx: &ConnIndex, conns: &[Connection]) -> BTreeSet<(u32, u32)> {
    let find = |r: ConnRef| idx.conns.iter().find(|w| w.r == r);
    let mut set = BTreeSet::new();
    for c in conns {
        if let (Some(a), Some(b)) = (find(c.a), find(c.b))
            && is_shaft_in_hole(a, b)
        {
            set.insert((a.r.uid.min(b.r.uid), a.r.uid.max(b.r.uid)));
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_boxes_do_not_overlap_but_deep_ones_do() {
        let a = WPrim::Box {
            lo: [0.0; 3],
            hi: [10.0; 3],
        };
        let touching = WPrim::Box {
            lo: [10.0, 0.0, 0.0],
            hi: [20.0, 10.0, 10.0],
        };
        let deep = WPrim::Box {
            lo: [9.0, 0.0, 0.0],
            hi: [20.0, 10.0, 10.0],
        };
        assert!(penetration(&a, &touching) <= 0.0);
        assert!(penetration(&a, &deep) > 0.9);
    }

    #[test]
    fn sphere_and_cylinder_depths() {
        let s = WPrim::Sphere {
            c: [0.0, 0.0, 9.0],
            r: 8.0,
        };
        let floor = WPrim::Box {
            lo: [-50.0, -50.0, 0.0],
            hi: [50.0, 50.0, 2.0],
        };
        assert!((penetration(&s, &floor) - 1.0).abs() < 1e-9);
        let cyl = WPrim::Cyl {
            c: [0.0; 3],
            ax: 0,
            r: 2.0,
            h: 10.0,
        };
        let b = WPrim::Box {
            lo: [8.0, -5.0, -5.0],
            hi: [20.0, 5.0, 5.0],
        };
        assert!((penetration(&cyl, &b) - 2.0).abs() < 1e-9);
    }
}
