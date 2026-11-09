//! Ghost placement: snapping to free connectors, validity, and cursor rays (SPEC.md, "Placing a part").

use std::collections::{BTreeSet, HashSet};

use axl_parts::catalog::Catalog;
use axl_parts::types::{ColorId, PartId};

use crate::geom::{
    ConnIndex, SNAP_RADIUS, WPrim, compat, exempt_pairs, is_shaft_in_hole, part_conns, part_prims,
    penetration, OVERLAP_TOLERANCE_MM,
};
use crate::model::{Assembly, ConnRef, DeviceInstance};
use crate::orient::{Orient, TurnAxis};

/// A ray in the robot frame, millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    /// Start.
    pub origin: [f64; 3],
    /// Direction (need not be unit length).
    pub dir: [f64; 3],
}

impl Ray {
    /// Distance from a point to the ray (to its start if the point is behind it).
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        let d = self.dir;
        let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let v = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        let t = if len2 > 0.0 {
            ((v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len2).max(0.0)
        } else {
            0.0
        };
        let q = [self.origin[0] + d[0] * t, self.origin[1] + d[1] * t, self.origin[2] + d[2] * t];
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    }
}

/// A surface point under the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Point, mm.
    pub point: [f64; 3],
    /// Axis index of the surface normal.
    pub axis: usize,
    /// Sign of the normal along that axis.
    pub positive: bool,
    /// Uid of the part hit, or `None` for the floor.
    pub uid: Option<u32>,
    /// Distance along the ray.
    pub t: f64,
}

/// Ray against an axis-aligned box. Returns entry distance and the face hit.
pub fn ray_aabb(ray: &Ray, lo: [f64; 3], hi: [f64; 3]) -> Option<(f64, usize, bool)> {
    let mut t0 = 0.0_f64;
    let mut t1 = f64::MAX;
    let mut face = (0usize, true);
    for k in 0..3 {
        let o = ray.origin[k];
        let d = ray.dir[k];
        if d.abs() < 1e-12 {
            if o < lo[k] || o > hi[k] {
                return None;
            }
            continue;
        }
        let (mut a, mut b) = ((lo[k] - o) / d, (hi[k] - o) / d);
        let mut positive = false; // entering through the low face means a normal pointing to -axis
        if a > b {
            std::mem::swap(&mut a, &mut b);
            positive = true;
        }
        if a > t0 {
            t0 = a;
            face = (k, positive);
        }
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, face.0, face.1))
}

/// One part of the ghost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhostItem {
    /// Catalog part.
    pub part: PartId,
    /// Colour.
    pub color: ColorId,
    /// Orientation.
    pub orient: Orient,
    /// Offset of this part from the ghost anchor, 0.1 mm.
    pub rel: [i32; 3],
    /// Device name and port to keep (moves) or `None` to assign new ones.
    pub device: Option<DeviceInstance>,
    /// The part this item was picked up from (moves), if any.
    pub from_uid: Option<u32>,
}

/// What the ghost snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snap {
    /// The free connector on the robot.
    pub target: ConnRef,
    /// Ghost item index.
    pub item: usize,
    /// Connector index within the item's sub-body.
    pub conn: (u8, u16),
}

/// How a ghost would fit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Eval {
    /// Number of derived connections to the existing robot.
    pub joins: usize,
    /// Uids the ghost would overlap.
    pub overlaps: Vec<u32>,
}

/// Cached facts about the assembly a ghost is checked against.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// Connector index.
    pub index: ConnIndex,
    /// Connectors that already take part in a connection.
    pub occupied: HashSet<ConnRef>,
    /// Collision shapes per part uid.
    pub prims: Vec<(u32, Vec<(u8, WPrim)>)>,
    /// Lowest point of the robot, mm (0 when empty).
    pub floor_z: f64,
    /// Number of parts.
    pub count: usize,
}

impl Context {
    /// Build for an assembly.
    pub fn build(cat: &Catalog, asm: &Assembly) -> Self {
        let index = ConnIndex::build(cat, asm);
        let conns = index.connections();
        let mut occupied = HashSet::new();
        for c in &conns {
            occupied.insert(c.a);
            occupied.insert(c.b);
        }
        let prims: Vec<(u32, Vec<(u8, WPrim)>)> = asm
            .parts
            .iter()
            .filter_map(|p| cat.get(&p.part.0).map(|d| (p.uid, part_prims(d, p.pos, p.orient))))
            .collect();
        let mut floor_z = f64::MAX;
        for (_, ps) in &prims {
            for (_, p) in ps {
                floor_z = floor_z.min(p.aabb().0[2]);
            }
        }
        if floor_z == f64::MAX {
            floor_z = 0.0;
        }
        Context {
            index,
            occupied,
            prims,
            floor_z,
            count: asm.parts.len(),
        }
    }

    /// Nearest surface under a ray: a part's collision bounds or the floor.
    pub fn hit(&self, ray: &Ray) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for (uid, ps) in &self.prims {
            for (_, p) in ps {
                let (lo, hi) = p.aabb();
                if let Some((t, axis, positive)) = ray_aabb(ray, lo, hi)
                    && best.as_ref().is_none_or(|b| t < b.t)
                {
                    let point = [0, 1, 2].map(|k| ray.origin[k] + ray.dir[k] * t);
                    best = Some(Hit { point, axis, positive, uid: Some(*uid), t });
                }
            }
        }
        if ray.dir[2] < -1e-12 {
            let t = (self.floor_z - ray.origin[2]) / ray.dir[2];
            if t >= 0.0 && best.as_ref().is_none_or(|b| t < b.t) {
                let point = [0, 1, 2].map(|k| ray.origin[k] + ray.dir[k] * t);
                best = Some(Hit { point, axis: 2, positive: true, uid: None, t });
            }
        }
        best
    }
}

/// The ghost: one part, or a group being moved or duplicated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ghost {
    /// Parts in the ghost.
    pub items: Vec<GhostItem>,
    /// Position of the reference point, 0.1 mm.
    pub anchor: [i32; 3],
    /// Set when the anchor is a real location under the cursor.
    pub placed: bool,
    /// The snap in use, if any.
    pub snap: Option<Snap>,
    /// Can it be placed now?
    pub valid: bool,
    /// Why not, in words for the learner.
    pub problem: Option<String>,
}

impl Ghost {
    /// A ghost of one part.
    pub fn single(part: &PartId, color: &ColorId) -> Self {
        Ghost {
            items: vec![GhostItem {
                part: part.clone(),
                color: color.clone(),
                orient: Orient::IDENTITY,
                rel: [0; 3],
                device: None,
                from_uid: None,
            }],
            anchor: [0; 3],
            placed: false,
            snap: None,
            valid: false,
            problem: None,
        }
    }

    /// Turn the whole ghost 90 degrees about a world axis through the anchor.
    pub fn turn(&mut self, about: TurnAxis) {
        for it in &mut self.items {
            it.rel = Orient::turn_vec(about, it.rel);
            it.orient = it.orient.turned(about);
        }
    }

    /// Bounds of the ghost relative to its anchor, mm.
    pub fn bounds_rel(&self, cat: &Catalog) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::MAX; 3];
        let mut hi = [f64::MIN; 3];
        for it in &self.items {
            let def = cat.get(&it.part.0)?;
            for (_, p) in part_prims(def, it.rel, it.orient) {
                let (l, h) = p.aabb();
                for k in 0..3 {
                    lo[k] = lo[k].min(l[k]);
                    hi[k] = hi[k].max(h[k]);
                }
            }
        }
        (lo[0] != f64::MAX).then_some((lo, hi))
    }
}

/// Check a ghost at `anchor` against the robot.
pub fn evaluate(cat: &Catalog, ctx: &Context, items: &[GhostItem], anchor: [i32; 3]) -> Eval {
    let mut ev = Eval::default();
    let mut exempt: BTreeSet<(usize, u32)> = BTreeSet::new();
    for (i, it) in items.iter().enumerate() {
        let Some(def) = cat.get(&it.part.0) else { continue };
        let pos = [0, 1, 2].map(|k| anchor[k] + it.rel[k]);
        for g in part_conns(def, u32::MAX - i as u32, pos, it.orient) {
            for t in ctx.index.at(g.pos) {
                if compat(&g, t).is_some() {
                    ev.joins += 1;
                    if is_shaft_in_hole(&g, t) {
                        exempt.insert((i, t.r.uid));
                    }
                }
            }
        }
    }
    let mut over = BTreeSet::new();
    for (i, it) in items.iter().enumerate() {
        let Some(def) = cat.get(&it.part.0) else { continue };
        let pos = [0, 1, 2].map(|k| anchor[k] + it.rel[k]);
        let mine = part_prims(def, pos, it.orient);
        for (uid, theirs) in &ctx.prims {
            if exempt.contains(&(i, *uid)) {
                continue;
            }
            let hit = mine.iter().any(|(_, p)| {
                theirs.iter().any(|(_, q)| {
                    let (pl, ph) = p.aabb();
                    let (ql, qh) = q.aabb();
                    (0..3).all(|k| pl[k] < qh[k] - OVERLAP_TOLERANCE_MM && ql[k] < ph[k] - OVERLAP_TOLERANCE_MM)
                        && penetration(p, q) > OVERLAP_TOLERANCE_MM
                })
            });
            if hit {
                over.insert(*uid);
            }
        }
    }
    ev.overlaps = over.into_iter().collect();
    ev
}

/// Is this evaluation a legal placement?
pub fn is_valid(ctx: &Context, ev: &Eval) -> bool {
    (ctx.count == 0 || ev.joins >= 1) && ev.overlaps.is_empty()
}

/// Why a placement is refused, in words.
pub fn problem_text(ctx: &Context, ev: &Eval) -> Option<String> {
    if !ev.overlaps.is_empty() {
        Some("This part would overlap another part.".into())
    } else if ctx.count > 0 && ev.joins == 0 {
        Some("This part must connect to the robot. Move it onto a free stud, hole or axle.".into())
    } else {
        None
    }
}

/// Find the best snap for a ghost whose reference point may be at any of `frees` (the unsnapped
/// position, and the same position pushed into the surface, because a ghost resting on a surface
/// is often further than 6 mm from a hole that lies inside the part).
/// Returns the snapped anchor and what it snapped to. Ties go to the connector nearest the cursor
/// ray, then to the placement with fewer overlaps.
pub fn find_snap(cat: &Catalog, ctx: &Context, items: &[GhostItem], frees: &[[i32; 3]], ray: &Ray) -> Option<([i32; 3], Snap)> {
    struct Cand {
        anchor: [i32; 3],
        snap: Snap,
        d: f64,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for free in frees {
        for (i, it) in items.iter().enumerate() {
            let Some(def) = cat.get(&it.part.0) else { continue };
            let pos = [0, 1, 2].map(|k| free[k] + it.rel[k]);
            for g in part_conns(def, u32::MAX - i as u32, pos, it.orient) {
                for t in ctx.index.near(g.pos, SNAP_RADIUS) {
                    if ctx.occupied.contains(&t.r) || compat(&g, t).is_none() {
                        continue;
                    }
                    let anchor = [0, 1, 2].map(|k| free[k] + t.pos[k] - g.pos[k]);
                    let tp = t.pos.map(|v| v as f64 * 0.1);
                    cands.push(Cand {
                        anchor,
                        snap: Snap { target: t.r, item: i, conn: (g.r.body, g.r.conn) },
                        d: ray.distance_to(tp),
                    });
                }
            }
        }
    }
    cands.sort_by(|a, b| a.d.total_cmp(&b.d).then(a.snap.target.cmp(&b.snap.target)));
    let mut seen = HashSet::new();
    let mut best: Option<(f64, usize, [i32; 3], Snap)> = None;
    for c in cands.into_iter().filter(|c| seen.insert(c.anchor)).take(48) {
        let ev = evaluate(cat, ctx, items, c.anchor);
        let key = (c.d * 10.0).round();
        let score = (!ev.overlaps.is_empty()) as usize * 1000 + ev.overlaps.len();
        let better = match &best {
            None => true,
            Some((bd, bs, ..)) => key < *bd || (key == *bd && score < *bs),
        };
        if better {
            best = Some((key, score, c.anchor, c.snap));
        }
    }
    best.map(|(_, _, a, s)| (a, s))
}

/// Exempt pairs for a finished assembly (used by validation).
pub fn assembly_exempt(ctx: &Context) -> BTreeSet<(u32, u32)> {
    exempt_pairs(&ctx.index, &ctx.index.connections())
}
