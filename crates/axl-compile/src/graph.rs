//! Steps 1 to 8 and 10 of the compiler algorithm (SPEC.md, "Assembly-to-physics compiler"):
//! nodes, rigid merge, hinge links with the coaxial rule, pin clusters, spanning tree, loops,
//! gears, bodies, geoms and joints. The result is a [`RobotModel`]; `mjcf` turns it into text.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use axl_build::geom::derive_connections;
use axl_build::model::{Assembly, JointKind, PlacedPart};
use axl_parts::catalog::Catalog;
use axl_parts::types::{
    Axis, Category, CollisionPrim, ConnKind, DeviceModel, MaterialKind, PartDef, ShapeGen,
};

use crate::error::CompileError;
use crate::model::*;
use crate::params::{MotorParams, PhysicsParams};

const MM: f64 = 1e-3;
/// 0.1 mm in metres: the unit of stored positions.
const UNIT: f64 = 1e-4;
/// Directions are parallel when their cross product is below this (SPEC step 3).
const PARALLEL_TOL: f64 = 1e-6;
/// Axis lines coincide when closer than 0.05 mm (SPEC step 3).
const LINE_TOL: f64 = 0.05 * MM;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn int_to_m(p: [i32; 3]) -> V3 {
    p.map(|v| v as f64 * UNIT)
}

/// A line in space: a point and a unit direction.
#[derive(Clone, Copy, Debug)]
struct Line {
    p: V3,
    d: V3,
}

fn same_line(a: &Line, b: &Line) -> bool {
    norm(cross(a.d, b.d)) < PARALLEL_TOL && norm(cross(sub(b.p, a.p), a.d)) <= LINE_TOL
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkKind {
    Pin { friction: bool },
    Axle,
    Motor(u32),
    Ball,
}

#[derive(Clone, Copy, Debug)]
struct Link {
    a: usize,
    b: usize,
    line: Line,
    kind: LinkKind,
    /// Sort key: nodes, kind, connector refs.
    key: (usize, usize, u8, u32, u16),
}

#[derive(Clone, Copy, Debug)]
struct Node {
    uid: u32,
    sub: u8,
    /// The caster's ball: a second body made by the compiler, not drawn.
    virt: bool,
}

struct Uf(Vec<usize>);

impl Uf {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = x;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }
    /// Join two sets; the smallest node index is the root, so the root names the cluster.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi] = lo;
        }
    }
}

fn prim_volume(p: &CollisionPrim) -> f64 {
    match *p {
        CollisionPrim::Box { half, .. } => 8.0 * half[0] * half[1] * half[2],
        CollisionPrim::Cylinder {
            radius, half_len, ..
        } => std::f64::consts::PI * radius * radius * 2.0 * half_len,
        CollisionPrim::Sphere { radius, .. } => 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3),
    }
}

fn axis_vec(a: Axis) -> V3 {
    a.unit()
}

/// A part's placement, to move collision shapes and sites into the robot frame.
struct Placement<'a> {
    part: &'a PlacedPart,
}

impl Placement<'_> {
    fn pos(&self) -> V3 {
        int_to_m(self.part.pos)
    }
    fn rot(&self, v: V3) -> V3 {
        self.part.orient.rotate_f(v)
    }
    fn rot_axis(&self, a: Axis) -> V3 {
        self.rot(axis_vec(a))
    }
    /// A collision primitive in the robot frame.
    fn prim(&self, p: &CollisionPrim) -> GeomShape {
        let o = self.pos();
        match *p {
            CollisionPrim::Box { center, half } => {
                let h = self.rot(half);
                GeomShape::Box {
                    pos: add(o, self.rot(center)),
                    half: h.map(f64::abs),
                }
            }
            CollisionPrim::Cylinder {
                center,
                axis,
                radius,
                half_len,
            } => {
                let c = add(o, self.rot(center));
                let a = self.rot_axis(axis);
                GeomShape::Cylinder {
                    from: sub(c, scale(a, half_len)),
                    to: add(c, scale(a, half_len)),
                    radius,
                }
            }
            CollisionPrim::Sphere { center, radius } => GeomShape::Sphere {
                pos: add(o, self.rot(center)),
                radius,
            },
        }
    }
}

fn shape_lowest(s: &GeomShape) -> f64 {
    match *s {
        GeomShape::Box { pos, half } => pos[2] - half[2],
        GeomShape::Cylinder { from, to, radius } => {
            let vertical = (from[0] - to[0]).abs() < 1e-12 && (from[1] - to[1]).abs() < 1e-12;
            if vertical {
                from[2].min(to[2])
            } else {
                (from[2] + to[2]) / 2.0 - radius
            }
        }
        GeomShape::Sphere { pos, radius } => pos[2] - radius,
    }
}

fn shape_shift(s: &GeomShape, by: V3) -> GeomShape {
    match *s {
        GeomShape::Box { pos, half } => GeomShape::Box {
            pos: sub(pos, by),
            half,
        },
        GeomShape::Cylinder { from, to, radius } => GeomShape::Cylinder {
            from: sub(from, by),
            to: sub(to, by),
            radius,
        },
        GeomShape::Sphere { pos, radius } => GeomShape::Sphere {
            pos: sub(pos, by),
            radius,
        },
    }
}

fn surface_of(def: &PartDef, prim: &CollisionPrim) -> Surface {
    match def.category {
        Category::Wheel => Surface::Rubber,
        Category::Caster if matches!(prim, CollisionPrim::Sphere { .. }) => Surface::Steel,
        _ => match def.material {
            MaterialKind::Rubber => Surface::Rubber,
            MaterialKind::Steel => Surface::Steel,
            MaterialKind::Abs | MaterialKind::AbsTransparent => Surface::Abs,
        },
    }
}

fn motor_params(kind: DeviceModel, p: &PhysicsParams) -> Option<MotorParams> {
    match kind {
        DeviceModel::MotorMedium => Some(p.motors.medium),
        DeviceModel::MotorLarge => Some(p.motors.large),
        DeviceModel::Servo => Some(p.motors.servo),
        _ => None,
    }
}

/// Which way a motor turns for a positive voltage (decision 0010): the rotation about the shaft
/// axis whose dominant robot-frame component is along +X, +Y or +Z. Mirrored motors then turn
/// their wheels the same way for the same voltage, as the SPEC m01 templates assume.
fn positive_direction(d: V3) -> V3 {
    let k = (0..3)
        .max_by(|&i, &j| d[i].abs().total_cmp(&d[j].abs()).then(j.cmp(&i)))
        .unwrap_or(0);
    if d[k] < 0.0 { scale(d, -1.0) } else { d }
}

/// A joint between two clusters: all the hinge links between them, merged.
struct JointInfo {
    ca: usize,
    cb: usize,
    links: Vec<Link>,
}

fn cluster_pairs(links: &[Link], uf: &mut Uf) -> BTreeMap<(usize, usize), Vec<Link>> {
    let mut m: BTreeMap<(usize, usize), Vec<Link>> = BTreeMap::new();
    for l in links {
        let (a, b) = (uf.find(l.a), uf.find(l.b));
        if a == b {
            continue;
        }
        m.entry((a.min(b), a.max(b))).or_default().push(*l);
    }
    for v in m.values_mut() {
        v.sort_by_key(|l| l.key);
    }
    m
}

fn all_coaxial(links: &[Link]) -> bool {
    links.iter().all(|l| l.kind != LinkKind::Ball)
        && links.windows(2).all(|w| same_line(&w[0].line, &w[1].line))
}

/// Build the [`RobotModel`] from an assembly.
pub fn build_model(
    cat: &Catalog,
    asm: &Assembly,
    params: &PhysicsParams,
) -> Result<RobotModel, CompileError> {
    // Parts in uid order: everything below is sorted by uid so the output never depends on the
    // order the builder happened to store them in.
    let mut parts: Vec<&PlacedPart> = asm.parts.iter().collect();
    parts.sort_by_key(|p| p.uid);
    for w in parts.windows(2) {
        if w[0].uid == w[1].uid {
            return Err(CompileError::DuplicateUid(w[0].uid));
        }
    }
    let mut defs: BTreeMap<u32, &PartDef> = BTreeMap::new();
    for p in &parts {
        let def = cat
            .get(&p.part.0)
            .ok_or_else(|| CompileError::UnknownPart(p.part.0.clone()))?;
        defs.insert(p.uid, def);
    }
    let hubs: Vec<u32> = parts
        .iter()
        .filter(|p| defs[&p.uid].category == Category::Hub)
        .map(|p| p.uid)
        .collect();
    let hub_uid = match hubs.as_slice() {
        [] => return Err(CompileError::NoHub),
        [h] => *h,
        _ => return Err(CompileError::MultipleHubs),
    };

    // Step 1: nodes. One per sub-body; the caster gets a second node for its ball.
    let mut nodes: Vec<Node> = Vec::new();
    for p in &parts {
        let def = defs[&p.uid];
        for sb in 0..def.bodies.len() {
            nodes.push(Node {
                uid: p.uid,
                sub: sb as u8,
                virt: false,
            });
        }
        if def.category == Category::Caster {
            nodes.push(Node {
                uid: p.uid,
                sub: 1,
                virt: true,
            });
        }
    }
    let node_of: BTreeMap<(u32, u8), usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| ((n.uid, n.sub), i))
        .collect();
    let placed: BTreeMap<u32, &PlacedPart> = parts.iter().map(|p| (p.uid, *p)).collect();

    // Step 2: rigid merge, and the hinge links for step 3.
    let mut uf = Uf((0..nodes.len()).collect());
    let mut links: Vec<Link> = Vec::new();
    let conns = derive_connections(cat, asm);
    let conn_kind = |r: axl_build::ConnRef| -> Option<ConnKind> {
        defs.get(&r.uid)
            .and_then(|d| d.bodies.get(r.body as usize))
            .and_then(|b| b.connectors.get(r.conn as usize))
            .map(|c| c.kind)
    };
    for c in &conns {
        let (Some(&na), Some(&nb)) = (
            node_of.get(&(c.a.uid, c.a.body)),
            node_of.get(&(c.b.uid, c.b.body)),
        ) else {
            continue;
        };
        match c.kind {
            JointKind::Rigid => uf.union(na, nb),
            JointKind::Hinge { friction } => {
                let (Some(ka), Some(kb)) = (conn_kind(c.a), conn_kind(c.b)) else {
                    continue;
                };
                let axle = matches!(ka, ConnKind::AxleShaft { .. })
                    || matches!(kb, ConnKind::AxleShaft { .. });
                // The connector that is a shaft gives the axis (both are parallel).
                let (pa, ca) = {
                    let pp = placed[&c.a.uid];
                    let def = defs[&c.a.uid];
                    let cn = &def.bodies[c.a.body as usize].connectors[c.a.conn as usize];
                    (
                        add(int_to_m(pp.pos), pp.orient.rotate_f(int_to_m(cn.pos))),
                        pp.orient.rotate_axis(cn.axis),
                    )
                };
                links.push(Link {
                    a: na,
                    b: nb,
                    line: Line {
                        p: pa,
                        d: axis_vec(ca),
                    },
                    kind: if axle {
                        LinkKind::Axle
                    } else {
                        LinkKind::Pin { friction }
                    },
                    key: (na.min(nb), na.max(nb), 1, c.a.uid, c.a.conn),
                });
            }
        }
    }
    // Internal hinges: motors (housing to rotor) and the caster's ball.
    for p in &parts {
        let def = defs[&p.uid];
        if let Some(ij) = &def.internal_joint
            && def.bodies.len() == 2
        {
            let pp = placed[&p.uid];
            let (na, nb) = (node_of[&(p.uid, 0)], node_of[&(p.uid, 1)]);
            links.push(Link {
                a: na,
                b: nb,
                line: Line {
                    p: add(int_to_m(pp.pos), pp.orient.rotate_f(int_to_m(ij.pos))),
                    d: pp.orient.rotate_f(axis_vec(ij.axis)),
                },
                kind: LinkKind::Motor(p.uid),
                key: (na, nb, 0, p.uid, 0),
            });
        }
        if def.category == Category::Caster {
            let pl = Placement {
                part: placed[&p.uid],
            };
            let centre = def.bodies[0]
                .collision
                .iter()
                .find_map(|c| match c {
                    CollisionPrim::Sphere { center, .. } => Some(*center),
                    _ => None,
                })
                .ok_or_else(|| CompileError::NoCollision(def.id.0.clone()))?;
            let (na, nb) = (node_of[&(p.uid, 0)], node_of[&(p.uid, 1)]);
            links.push(Link {
                a: na,
                b: nb,
                line: Line {
                    p: add(pl.pos(), pl.rot(centre)),
                    d: [0.0, 0.0, 1.0],
                },
                kind: LinkKind::Ball,
                key: (na, nb, 2, p.uid, 0),
            });
        }
    }

    // Steps 3 and 4, repeated until nothing changes.
    let is_pin = |n: usize| defs[&nodes[n].uid].category == Category::Pin;
    loop {
        let pairs = cluster_pairs(&links, &mut uf);
        // Step 3: links between one pair of clusters on one axis line make one hinge; anything
        // else merges the two clusters (two non-coaxial hinges lock rotation).
        if let Some((&(a, b), _)) = pairs.iter().find(|(_, v)| v.len() > 1 && !all_coaxial(v)) {
            uf.union(a, b);
            continue;
        }
        // Step 4: a cluster of only pins, hinged to exactly two clusters on the same axis, merges
        // into the neighbour with the lower smallest uid.
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for n in 0..nodes.len() {
            let r = uf.find(n);
            members.entry(r).or_default().push(n);
        }
        let mut merged = false;
        for (&c, ns) in &members {
            if !ns.iter().all(|&n| is_pin(n)) {
                continue;
            }
            let mut neighbours = BTreeSet::new();
            let mut lines: Vec<Line> = Vec::new();
            let mut ball = false;
            for (&(a, b), v) in &pairs {
                if a == c || b == c {
                    neighbours.insert(if a == c { b } else { a });
                    ball |= v.iter().any(|l| l.kind == LinkKind::Ball);
                    lines.extend(v.iter().map(|l| l.line));
                }
            }
            if neighbours.len() == 2
                && !ball
                && lines.windows(2).all(|w| same_line(&w[0], &w[1]))
                && let Some(&lower) = neighbours.iter().next()
            {
                // Cluster ids are smallest node indices, which follow uids.
                uf.union(c, lower);
                merged = true;
                break;
            }
        }
        if !merged {
            break;
        }
    }

    // Joints between clusters, in ascending id order (ordered by the clusters' smallest uids).
    let pairs = cluster_pairs(&links, &mut uf);
    let joints: Vec<JointInfo> = pairs
        .into_iter()
        .map(|((ca, cb), links)| JointInfo { ca, cb, links })
        .collect();

    // Step 5: spanning tree by breadth-first search from the hub's cluster.
    let root = uf.find(node_of[&(hub_uid, 0)]);
    let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (j, ji) in joints.iter().enumerate() {
        adj.entry(ji.ca).or_default().push(j);
        adj.entry(ji.cb).or_default().push(j);
    }
    let mut cluster_nodes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for n in 0..nodes.len() {
        let r = uf.find(n);
        cluster_nodes.entry(r).or_default().push(n);
    }
    let mut order: Vec<usize> = vec![root];
    let mut seen: BTreeSet<usize> = BTreeSet::from([root]);
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut tree: BTreeMap<usize, (usize, usize)> = BTreeMap::new(); // child cluster -> (parent cluster, joint)
    let mut closers: Vec<usize> = Vec::new();
    let mut queue = VecDeque::from([root]);
    while let Some(c) = queue.pop_front() {
        for &j in adj.get(&c).map(Vec::as_slice).unwrap_or(&[]) {
            if used.contains(&j) {
                continue;
            }
            used.insert(j);
            let other = if joints[j].ca == c {
                joints[j].cb
            } else {
                joints[j].ca
            };
            if seen.insert(other) {
                tree.insert(other, (c, j));
                order.push(other);
                queue.push_back(other);
            } else {
                closers.push(j);
            }
        }
    }
    if order.len() != cluster_nodes.len() {
        let floating: usize = cluster_nodes
            .iter()
            .filter(|(c, _)| !seen.contains(c))
            .flat_map(|(_, ns)| ns.iter().map(|&n| nodes[n].uid))
            .collect::<BTreeSet<_>>()
            .len();
        return Err(CompileError::Floating(floating));
    }

    // Devices by uid.
    let dev_name: BTreeMap<u32, String> = parts
        .iter()
        .filter_map(|p| p.device.as_ref().map(|d| (p.uid, d.name.clone())))
        .collect();

    // Step 7 and 10: bodies and joints.
    let hub_pos = int_to_m(placed[&hub_uid].pos);
    let body_index: BTreeMap<usize, usize> =
        order.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let min_uid_name = |c: usize| -> String {
        let n = nodes[c];
        if n.sub == 0 {
            format!("b{}", n.uid)
        } else {
            format!("b{}_{}", n.uid, n.sub)
        }
    };
    let mut bodies: Vec<ModelBody> = Vec::new();
    let mut joint_of_cluster: BTreeMap<usize, (usize, String, V3)> = BTreeMap::new();
    for &c in &order {
        let (parent, origin, joint) = match tree.get(&c) {
            None => (None, hub_pos, None),
            Some(&(pc, j)) => {
                let ji = &joints[j];
                let primary = ji
                    .links
                    .iter()
                    .find(|l| matches!(l.kind, LinkKind::Motor(_)))
                    .unwrap_or(&ji.links[0]);
                let mj = if ji.links.iter().any(|l| l.kind == LinkKind::Ball) {
                    let uid = nodes[c].uid;
                    ModelJoint::Ball {
                        name: format!("j_{uid}_ball"),
                        damping: params.joints.caster_ball_damping,
                    }
                } else if let LinkKind::Motor(uid) = primary.kind {
                    let def = defs[&uid];
                    let mp = def
                        .device
                        .as_ref()
                        .and_then(|d| motor_params(d.kind, params))
                        .ok_or_else(|| {
                            CompileError::Params(format!("{} has no motor parameters", def.id.0))
                        })?;
                    // The joint angle is the rotor angle about the axis. If the housing is the
                    // child the joint measures the housing relative to the rotor, so flip it.
                    let positive = positive_direction(primary.line.d);
                    let housing_is_child = uf.find(node_of[&(uid, 0)]) == c;
                    let axis = if housing_is_child {
                        scale(positive, -1.0)
                    } else {
                        positive
                    };
                    ModelJoint::Hinge {
                        name: format!(
                            "j_{}",
                            dev_name.get(&uid).cloned().unwrap_or(uid.to_string())
                        ),
                        axis,
                        damping: params.joints.motor_damping,
                        friction_loss: mp.friction_loss,
                        armature: mp.armature,
                        range: def.internal_joint.as_ref().and_then(|i| i.range_rad),
                        motor: Some(uid),
                    }
                } else {
                    let mut damping: f64 = 0.0;
                    let mut friction_loss: f64 = 0.0;
                    for l in &ji.links {
                        let jp = match l.kind {
                            LinkKind::Pin { friction: true } => params.joints.friction_pin,
                            LinkKind::Pin { friction: false } => params.joints.smooth_pin,
                            _ => params.joints.axle_in_hole,
                        };
                        damping = damping.max(jp.damping);
                        friction_loss = friction_loss.max(jp.friction_loss);
                    }
                    ModelJoint::Hinge {
                        name: format!("j_{}", nodes[c].uid),
                        axis: primary.line.d,
                        damping,
                        friction_loss,
                        armature: 0.0,
                        range: None,
                        motor: None,
                    }
                };
                let axis = match &mj {
                    ModelJoint::Hinge { axis, .. } => *axis,
                    ModelJoint::Ball { .. } => [0.0, 0.0, 1.0],
                };
                joint_of_cluster.insert(c, (j, mj.name().to_string(), axis));
                (Some(body_index[&pc]), primary.line.p, Some(mj))
            }
        };
        let name = if c == root {
            "root".to_string()
        } else {
            min_uid_name(c)
        };
        bodies.push(ModelBody {
            name,
            parent,
            origin,
            joint,
            parts: Vec::new(),
            geoms: Vec::new(),
            sites: Vec::new(),
        });
    }
    // Names of body 'b<uid>' must be unique: nodes in different clusters have different smallest
    // nodes, so they are.

    // Step 8: parts, geoms (mass split by volume) and sites.
    let mut min_z = f64::MAX;
    let mut warnings: Vec<String> = Vec::new();
    for &c in &order {
        let bi = body_index[&c];
        let origin = bodies[bi].origin;
        let mut geoms: Vec<ModelGeom> = Vec::new();
        let mut body_parts: Vec<BodyPart> = Vec::new();
        for &n in &cluster_nodes[&c] {
            let node = nodes[n];
            let def = defs[&node.uid];
            let pp = placed[&node.uid];
            let pl = Placement { part: pp };
            if !node.virt {
                body_parts.push(BodyPart {
                    uid: node.uid,
                    sub: node.sub,
                    pos: sub(pl.pos(), origin),
                    orient: pp.orient.index(),
                });
            }
            let sbody = &def.bodies[(node.sub as usize).min(def.bodies.len() - 1)];
            let total: f64 = sbody.collision.iter().map(prim_volume).sum();
            if sbody.collision.is_empty() || total <= 0.0 {
                return Err(CompileError::NoCollision(def.id.0.clone()));
            }
            for (k, prim) in sbody.collision.iter().enumerate() {
                // The caster's sphere belongs to the ball node, its box to the base node.
                let is_sphere = matches!(prim, CollisionPrim::Sphere { .. });
                if def.category == Category::Caster && is_sphere != node.virt {
                    continue;
                }
                let shape = pl.prim(prim);
                min_z = min_z.min(shape_lowest(&shape));
                geoms.push(ModelGeom {
                    name: format!(
                        "p{}_{}_{}",
                        node.uid,
                        node.sub.min(def.bodies.len() as u8 - 1),
                        k
                    ),
                    uid: node.uid,
                    shape: shape_shift(&shape, origin),
                    mass: sbody.mass_kg * prim_volume(prim) / total,
                    surface: surface_of(def, prim),
                });
            }
            // Sensor sites on the part's first node.
            if node.sub == 0
                && !node.virt
                && let (Some(dev), Some(inst)) = (&def.device, &pp.device)
                && let Some(site) = &dev.site
            {
                let is_hub = dev.kind == DeviceModel::Hub;
                bodies[bi].sites.push(ModelSite {
                    name: if is_hub {
                        "imu".into()
                    } else {
                        format!("site_{}", inst.name)
                    },
                    pos: sub(add(pl.pos(), pl.rot(int_to_m(site.pos))), origin),
                    zaxis: (!is_hub).then(|| pl.rot_axis(site.axis)),
                });
            }
        }
        // Minimum body mass (SPEC "Risks").
        let mass: f64 = geoms.iter().map(|g| g.mass).sum();
        if mass < params.solver.min_body_mass_kg && mass > 0.0 {
            let k = params.solver.min_body_mass_kg / mass;
            for g in &mut geoms {
                g.mass *= k;
            }
            warnings.push(format!(
                "body {} weighs {:.4} kg; raised to the minimum of {:.4} kg",
                bodies[bi].name, mass, params.solver.min_body_mass_kg
            ));
        }
        bodies[bi].parts = body_parts;
        bodies[bi].geoms = geoms;
    }
    if min_z == f64::MAX {
        min_z = 0.0;
    }

    // Loop closures (step 5): two connect constraints, at the anchor and 8 mm along the axis.
    let mut loops = Vec::new();
    for &j in &closers {
        let ji = &joints[j];
        if ji.links.iter().any(|l| l.kind == LinkKind::Ball) {
            return Err(CompileError::LoopThroughBall);
        }
        let l = &ji.links[0];
        loops.push(LoopClosure {
            name: format!("loop_{}", nodes[ji.ca].uid.min(nodes[ji.cb].uid)),
            body1: body_index[&ji.ca],
            body2: body_index[&ji.cb],
            anchor: l.line.p,
            axis: l.line.d,
        });
    }
    // Make loop names unique if two share a smallest uid.
    let mut names = BTreeMap::<String, usize>::new();
    for l in &mut loops {
        let n = names.entry(l.name.clone()).or_insert(0);
        if *n > 0 {
            l.name = format!("{}_{}", l.name, n);
        }
        *n += 1;
    }

    // Step 6: gears.
    let gears = gear_couplings(
        &parts,
        &defs,
        &mut uf,
        &node_of,
        &joint_of_cluster,
        &tree,
        &body_index,
        &mut bodies,
        params,
    )?;

    // Devices.
    let mut devices = Vec::new();
    for p in &parts {
        let (Some(inst), Some(dd)) = (&p.device, &defs[&p.uid].device) else {
            continue;
        };
        let motor = motor_params(dd.kind, params);
        let joint = if motor.is_some() {
            bodies.iter().find_map(|b| match &b.joint {
                Some(ModelJoint::Hinge {
                    name,
                    motor: Some(m),
                    ..
                }) if *m == p.uid => Some(name.clone()),
                _ => None,
            })
        } else {
            None
        };
        if motor.is_some() && joint.is_none() {
            warnings.push(format!("Motor {} turns nothing.", inst.name));
        }
        let site = bodies.iter().find_map(|b| {
            b.sites.iter().find_map(|s| {
                let want = if dd.kind == DeviceModel::Hub {
                    "imu".to_string()
                } else {
                    format!("site_{}", inst.name)
                };
                (s.name == want && dd.site.is_some()).then(|| s.name.clone())
            })
        });
        devices.push(ModelDevice {
            name: inst.name.clone(),
            uid: p.uid,
            port: inst.port,
            kind: dd.kind,
            joint,
            motor,
            site: if matches!(dd.kind, DeviceModel::Hub) || dd.site.is_some() {
                site
            } else {
                None
            },
        });
    }
    Ok(RobotModel {
        name: asm.name.clone(),
        bodies,
        loops,
        gears,
        devices,
        min_z,
        warnings,
    })
}

/// Step 6: find meshed gear pairs and turn each into a joint coupling.
#[allow(clippy::too_many_arguments)]
fn gear_couplings(
    parts: &[&PlacedPart],
    defs: &BTreeMap<u32, &PartDef>,
    uf: &mut Uf,
    node_of: &BTreeMap<(u32, u8), usize>,
    joint_of_cluster: &BTreeMap<usize, (usize, String, V3)>,
    tree: &BTreeMap<usize, (usize, usize)>,
    body_index: &BTreeMap<usize, usize>,
    bodies: &mut [ModelBody],
    params: &PhysicsParams,
) -> Result<Vec<GearCouple>, CompileError> {
    struct G {
        uid: u32,
        teeth: u32,
        half_t: f64,
        centre: V3,
        axis: V3,
    }
    let mut gs: Vec<G> = Vec::new();
    for p in parts {
        let def = defs[&p.uid];
        if let ShapeGen::Gear { teeth, thickness } = def.shape {
            let pl = Placement { part: p };
            gs.push(G {
                uid: p.uid,
                teeth,
                half_t: thickness / 2.0,
                centre: pl.pos(),
                axis: pl.rot_axis(Axis::PosZ),
            });
        }
    }
    let mut out = Vec::new();
    for i in 0..gs.len() {
        for j in (i + 1)..gs.len() {
            let (a, b) = (&gs[i], &gs[j]);
            if norm(cross(a.axis, b.axis)) > PARALLEL_TOL {
                continue;
            }
            let d = sub(b.centre, a.centre);
            let axial = dot(d, a.axis);
            let lateral = norm(sub(d, scale(a.axis, axial)));
            let want = (a.teeth + b.teeth) as f64 / 2.0 * MM;
            if (lateral - want).abs() > LINE_TOL {
                continue;
            }
            // Faces overlap: the thickness intervals along the axis intersect.
            if axial.abs() >= a.half_t + b.half_t - 1e-9 {
                continue;
            }
            let ca = uf.find(node_of[&(a.uid, 0)]);
            let cb = uf.find(node_of[&(b.uid, 0)]);
            if ca == cb {
                // Both gears are locked into one rigid piece: nothing can turn.
                continue;
            }
            let (Some((ja, name_a, axis_a)), Some((jb, name_b, axis_b))) =
                (joint_of_cluster.get(&ca), joint_of_cluster.get(&cb))
            else {
                let gear = if joint_of_cluster.contains_key(&ca) {
                    b.uid
                } else {
                    a.uid
                };
                return Err(CompileError::GearNotOnAxle { gear });
            };
            // The gear's axle must be the joint's axis.
            for (g, ax) in [(a, axis_a), (b, axis_b)] {
                if norm(cross(g.axis, *ax)) > PARALLEL_TOL {
                    return Err(CompileError::GearNotOnAxle { gear: g.uid });
                }
            }
            let (pa, pb) = (tree[&ca].0, tree[&cb].0);
            if pa != pb {
                return Err(CompileError::GearParents { a: a.uid, b: b.uid });
            }
            let _ = (ja, jb);
            // q1 = -(t2/t1) * s1 * s2 * q2, where s is +1 when the joint axis points along the
            // gear's axis (SPEC: r = -teeth2/teeth1 for equally oriented axes).
            let s1 = dot(*axis_a, a.axis).signum();
            let s2 = dot(*axis_b, b.axis).signum();
            let ratio = -(b.teeth as f64) / (a.teeth as f64) * s1 * s2;
            out.push(GearCouple {
                name: format!("gear_{}_{}", a.uid, b.uid),
                joint1: name_a.clone(),
                joint2: name_b.clone(),
                ratio,
            });
            // Friction of the mesh goes on the driven joint: the one without a motor.
            let motor_of = |bi: usize| {
                matches!(
                    &bodies[bi].joint,
                    Some(ModelJoint::Hinge { motor: Some(_), .. })
                )
            };
            let (bia, bib) = (body_index[&ca], body_index[&cb]);
            let driven = if motor_of(bib) && !motor_of(bia) {
                bia
            } else {
                bib
            };
            if let Some(ModelJoint::Hinge { friction_loss, .. }) = &mut bodies[driven].joint {
                *friction_loss += params.joints.gear_mesh_friction_loss;
            }
        }
    }
    Ok(out)
}
