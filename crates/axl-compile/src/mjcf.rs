//! MJCF text (step 7 onwards of the compiler: bodies, geoms, joints, actuators, sensors, spawn),
//! the [`DeviceMap`] and the [`BodyMap`].

use std::collections::BTreeMap;
use std::fmt::Write;

use axl_build::Port;
use axl_parts::types::DeviceModel;

use crate::model::*;
use crate::params::{MotorParams, PhysicsParams};

/// Height of the lowest collision point above the floor at spawn: 0.5 mm (SPEC step 13).
pub const SPAWN_CLEARANCE_M: f64 = 0.0005;
/// Reach of the second loop-closing point along the axis: 8 mm (SPEC step 5).
pub const LOOP_SPAN_M: f64 = 0.008;

/// Where and how to spawn the robot, and any extra world geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompileOptions {
    /// Spawn position of the robot frame origin on the floor, metres.
    pub spawn_xy: [f64; 2],
    /// Spawn yaw, radians, counter-clockwise from world +X.
    pub spawn_yaw: f64,
    /// Extra MJCF placed inside `<worldbody>` after the floor (static world geoms and objects
    /// from a mission). Empty for the sandbox.
    pub extra_worldbody: String,
}

/// A MuJoCo element: its name in the MJCF and its id in the loaded model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedId {
    /// Name in the MJCF.
    pub name: String,
    /// Id in the loaded `MjModel`.
    pub id: usize,
}

/// What a sensor measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorRole {
    /// Motor joint angle.
    JointPos,
    /// Motor joint speed.
    JointVel,
    /// Hub gyroscope.
    Gyro,
    /// Hub accelerometer.
    Accel,
    /// Hub orientation quaternion.
    Quat,
    /// Touch plunger force.
    Touch,
}

/// A MuJoCo sensor of a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensorEntry {
    /// What it measures.
    pub role: SensorRole,
    /// Name and id.
    pub sensor: NamedId,
}

/// One device in the [`DeviceMap`].
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceEntry {
    /// Device name.
    pub name: String,
    /// Part uid.
    pub uid: u32,
    /// Hub port; `None` for the hub.
    pub port: Option<Port>,
    /// Device model.
    pub kind: DeviceModel,
    /// The motor's hinge joint.
    pub joint: Option<NamedId>,
    /// The motor's actuator.
    pub actuator: Option<NamedId>,
    /// Sensor frame site (the hub's is `imu`).
    pub site: Option<NamedId>,
    /// MuJoCo sensors.
    pub sensors: Vec<SensorEntry>,
    /// Motor parameters for `axl-hw`.
    pub motor: Option<MotorParams>,
}

/// Each device name mapped to its MuJoCo elements and hardware parameters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceMap {
    /// Devices in uid order.
    pub devices: Vec<DeviceEntry>,
}

impl DeviceMap {
    /// A device by name.
    pub fn get(&self, name: &str) -> Option<&DeviceEntry> {
        self.devices.iter().find(|d| d.name == name)
    }
}

/// A body and the parts it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyMapEntry {
    /// Name in the MJCF.
    pub name: String,
    /// Body id in the loaded model.
    pub id: usize,
    /// Parts, each with its transform in the body frame.
    pub parts: Vec<BodyPart>,
}

/// Each MuJoCo body mapped to the parts it holds. The renderer attaches part meshes to moving
/// bodies with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyMap {
    /// Bodies in MuJoCo id order (the world body 0 holds nothing and is left out).
    pub bodies: Vec<BodyMapEntry>,
}

impl BodyMap {
    /// The entry that holds sub-body `sub` of part `uid`.
    pub fn find(&self, uid: u32, sub: u8) -> Option<(&BodyMapEntry, &BodyPart)> {
        self.bodies.iter().find_map(|b| {
            b.parts
                .iter()
                .find(|p| p.uid == uid && p.sub == sub)
                .map(|p| (b, p))
        })
    }
}

/// The compiler's result.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    /// The full MJCF document.
    pub mjcf: String,
    /// Device name to MuJoCo ids and hardware parameters.
    pub devices: DeviceMap,
    /// MuJoCo body to part uids and transforms.
    pub bodies: BodyMap,
    /// The merged robot the MJCF was written from.
    pub model: RobotModel,
    /// Things the compiler adjusted (for example mass added to reach the minimum).
    pub warnings: Vec<String>,
}

/// A number with at most nine decimals, never `-0`, so equal values print equally everywhere.
fn n(v: f64) -> String {
    let r = (v * 1e9).round() / 1e9;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

fn v3(v: [f64; 3]) -> String {
    format!("{} {} {}", n(v[0]), n(v[1]), n(v[2]))
}

/// Write the MJCF and the maps for a model.
pub fn emit(model: &RobotModel, params: &PhysicsParams, opts: &CompileOptions) -> Compiled {
    let mut out = String::new();
    let s = &params.solver;
    let _ = write!(
        out,
        r#"<mujoco model="axl">
  <compiler angle="radian" inertiafromgeom="true" autolimits="true"/>
  <option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10" gravity="0 0 -9.80665"/>
  <default>
    <geom condim="3" solref="{} {}" solimp="{} {} {}"/>
  </default>
  <worldbody>
    <geom name="floor" type="plane" size="0 0 0.05" contype="2" conaffinity="1" group="0" friction="{} 0.005 0.0001"/>
"#,
        n(s.solref.0),
        n(s.solref.1),
        n(s.solimp.0),
        n(s.solimp.1),
        n(s.solimp.2),
        n(params.friction.floor)
    );
    if !opts.extra_worldbody.is_empty() {
        for line in opts.extra_worldbody.lines() {
            let _ = writeln!(out, "    {line}");
        }
    }

    // Children of each body, in body order.
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, b) in model.bodies.iter().enumerate() {
        if let Some(p) = b.parent {
            children.entry(p).or_default().push(i);
        }
    }

    // Spawn: lowest collision point 0.5 mm above the floor, robot frame origin at spawn_xy.
    let (sy, cy) = opts.spawn_yaw.sin_cos();
    let hub = model.bodies[0].origin;
    let root_world = [
        opts.spawn_xy[0] + cy * hub[0] - sy * hub[1],
        opts.spawn_xy[1] + sy * hub[0] + cy * hub[1],
        hub[2] + SPAWN_CLEARANCE_M - model.min_z,
    ];

    // Depth-first emission; ids follow MuJoCo's order (pre-order for bodies, joints and sites).
    struct Ids {
        body: usize,
        joint: usize,
        site: usize,
        body_ids: BTreeMap<usize, usize>,
        joint_ids: BTreeMap<String, usize>,
        site_ids: BTreeMap<String, usize>,
    }
    let mut ids = Ids {
        body: 0,
        joint: 0,
        site: 0,
        body_ids: BTreeMap::new(),
        joint_ids: BTreeMap::new(),
        site_ids: BTreeMap::new(),
    };

    #[allow(clippy::too_many_arguments)]
    fn emit_body(
        out: &mut String,
        model: &RobotModel,
        params: &PhysicsParams,
        children: &BTreeMap<usize, Vec<usize>>,
        ids: &mut Ids,
        bi: usize,
        depth: usize,
        root_world: [f64; 3],
        yaw: f64,
    ) {
        let b = &model.bodies[bi];
        let pad = "  ".repeat(depth);
        ids.body += 1;
        ids.body_ids.insert(bi, ids.body);
        let (pos, euler) = match b.parent {
            None => (
                root_world,
                if yaw == 0.0 {
                    String::new()
                } else {
                    format!(" euler=\"0 0 {}\"", n(yaw))
                },
            ),
            Some(p) => (
                [
                    b.origin[0] - model.bodies[p].origin[0],
                    b.origin[1] - model.bodies[p].origin[1],
                    b.origin[2] - model.bodies[p].origin[2],
                ],
                String::new(),
            ),
        };
        let _ = writeln!(
            out,
            "{pad}<body name=\"{}\" pos=\"{}\"{euler}>",
            b.name,
            v3(pos)
        );
        match &b.joint {
            None => {
                ids.joint_ids.insert("root".into(), ids.joint);
                ids.joint += 1;
                let _ = writeln!(out, "{pad}  <freejoint name=\"root\"/>");
            }
            Some(ModelJoint::Hinge {
                name,
                axis,
                damping,
                friction_loss,
                armature,
                range,
                ..
            }) => {
                ids.joint_ids.insert(name.clone(), ids.joint);
                ids.joint += 1;
                let r = range
                    .map(|r| format!(" range=\"{} {}\"", n(r[0]), n(r[1])))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "{pad}  <joint name=\"{name}\" type=\"hinge\" pos=\"0 0 0\" axis=\"{}\" damping=\"{}\" frictionloss=\"{}\" armature=\"{}\"{r}/>",
                    v3(*axis),
                    n(*damping),
                    n(*friction_loss),
                    n(*armature)
                );
            }
            Some(ModelJoint::Ball { name, damping }) => {
                ids.joint_ids.insert(name.clone(), ids.joint);
                ids.joint += 1;
                let _ = writeln!(
                    out,
                    "{pad}  <joint name=\"{name}\" type=\"ball\" pos=\"0 0 0\" damping=\"{}\"/>",
                    n(*damping)
                );
            }
        }
        for g in &b.geoms {
            let (kind, geometry) = match g.shape {
                GeomShape::Box { pos, half } => {
                    ("box", format!("pos=\"{}\" size=\"{}\"", v3(pos), v3(half)))
                }
                GeomShape::Cylinder { from, to, radius } => (
                    "cylinder",
                    format!("fromto=\"{} {}\" size=\"{}\"", v3(from), v3(to), n(radius)),
                ),
                GeomShape::Sphere { pos, radius } => (
                    "sphere",
                    format!("pos=\"{}\" size=\"{}\"", v3(pos), n(radius)),
                ),
            };
            let (friction, condim) = match g.surface {
                Surface::Abs => (format!("{} 0.005 0.0001", n(params.friction.abs)), ""),
                Surface::Rubber => (
                    format!(
                        "{} {} 0.0001",
                        n(params.friction.rubber_sliding),
                        n(params.friction.rubber_torsional)
                    ),
                    " condim=\"4\"",
                ),
                Surface::Steel => (format!("{} 0.005 0.0001", n(params.friction.steel)), ""),
            };
            let _ = writeln!(
                out,
                "{pad}  <geom name=\"{}\" type=\"{kind}\" {geometry} mass=\"{}\" contype=\"1\" conaffinity=\"2\" group=\"1\" friction=\"{friction}\"{condim}/>",
                g.name,
                n(g.mass)
            );
        }
        for st in &b.sites {
            ids.site_ids.insert(st.name.clone(), ids.site);
            ids.site += 1;
            let z = st
                .zaxis
                .map(|z| format!(" zaxis=\"{}\"", v3(z)))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{pad}  <site name=\"{}\" pos=\"{}\" size=\"0.002\" group=\"4\"{z}/>",
                st.name,
                v3(st.pos)
            );
        }
        for &c in children.get(&bi).map(Vec::as_slice).unwrap_or(&[]) {
            emit_body(
                out,
                model,
                params,
                children,
                ids,
                c,
                depth + 1,
                root_world,
                yaw,
            );
        }
        let _ = writeln!(out, "{pad}</body>");
    }
    emit_body(
        &mut out,
        model,
        params,
        &children,
        &mut ids,
        0,
        2,
        root_world,
        opts.spawn_yaw,
    );
    out.push_str("  </worldbody>\n");

    // Equality constraints: loop closures (two connects each) and gear couplings.
    if !model.loops.is_empty() || !model.gears.is_empty() {
        out.push_str("  <equality>\n");
        for l in &model.loops {
            let o1 = model.bodies[l.body1].origin;
            let p0 = l.anchor;
            let p1 = [
                p0[0] + l.axis[0] * LOOP_SPAN_M,
                p0[1] + l.axis[1] * LOOP_SPAN_M,
                p0[2] + l.axis[2] * LOOP_SPAN_M,
            ];
            for (tag, p) in [("a", p0), ("b", p1)] {
                let rel = [p[0] - o1[0], p[1] - o1[1], p[2] - o1[2]];
                let _ = writeln!(
                    out,
                    "    <connect name=\"{}_{tag}\" body1=\"{}\" body2=\"{}\" anchor=\"{}\"/>",
                    l.name,
                    model.bodies[l.body1].name,
                    model.bodies[l.body2].name,
                    v3(rel)
                );
            }
        }
        for g in &model.gears {
            let _ = writeln!(
                out,
                "    <joint name=\"{}\" joint1=\"{}\" joint2=\"{}\" polycoef=\"0 {} 0 0 0\"/>",
                g.name,
                g.joint1,
                g.joint2,
                n(g.ratio)
            );
        }
        out.push_str("  </equality>\n");
    }

    // Actuators: one DC-motor `general` actuator per motor hinge.
    let mut devices = DeviceMap::default();
    let mut actuator_id = 0usize;
    let mut act_lines = String::new();
    let mut sensor_id = 0usize;
    let mut sens_lines = String::new();
    for d in &model.devices {
        let mut entry = DeviceEntry {
            name: d.name.clone(),
            uid: d.uid,
            port: d.port,
            kind: d.kind,
            joint: None,
            actuator: None,
            site: None,
            sensors: Vec::new(),
            motor: d.motor,
        };
        if let (Some(jn), Some(m)) = (&d.joint, &d.motor) {
            entry.joint = ids.joint_ids.get(jn).map(|&id| NamedId {
                name: jn.clone(),
                id,
            });
            let an = format!("a_{}", d.name);
            let force = m.stall_torque * m.v_max / m.v_nom;
            let _ = writeln!(
                act_lines,
                "    <general name=\"{an}\" joint=\"{jn}\" gaintype=\"fixed\" biastype=\"affine\" gainprm=\"{}\" biasprm=\"0 0 {}\" ctrlrange=\"{} {}\" forcerange=\"{} {}\"/>",
                n(m.stall_torque / m.v_nom),
                n(-m.stall_torque / m.omega0),
                n(-m.v_max),
                n(m.v_max),
                n(-force),
                n(force)
            );
            entry.actuator = Some(NamedId {
                name: an,
                id: actuator_id,
            });
            actuator_id += 1;
            for (role, suffix, tag) in [
                (SensorRole::JointPos, "pos", "jointpos"),
                (SensorRole::JointVel, "vel", "jointvel"),
            ] {
                let sn = format!("{}_{suffix}", d.name);
                let _ = writeln!(sens_lines, "    <{tag} name=\"{sn}\" joint=\"{jn}\"/>");
                entry.sensors.push(SensorEntry {
                    role,
                    sensor: NamedId {
                        name: sn,
                        id: sensor_id,
                    },
                });
                sensor_id += 1;
            }
        }
        if let Some(sn) = &d.site {
            entry.site = ids.site_ids.get(sn).map(|&id| NamedId {
                name: sn.clone(),
                id,
            });
            if d.kind == DeviceModel::Hub {
                for (role, name, line) in [
                    (
                        SensorRole::Gyro,
                        "gyro",
                        "<gyro name=\"gyro\" site=\"imu\"/>",
                    ),
                    (
                        SensorRole::Accel,
                        "accel",
                        "<accelerometer name=\"accel\" site=\"imu\"/>",
                    ),
                    (
                        SensorRole::Quat,
                        "quat",
                        "<framequat name=\"quat\" objtype=\"site\" objname=\"imu\"/>",
                    ),
                ] {
                    let _ = writeln!(sens_lines, "    {line}");
                    entry.sensors.push(SensorEntry {
                        role,
                        sensor: NamedId {
                            name: name.into(),
                            id: sensor_id,
                        },
                    });
                    sensor_id += 1;
                }
            }
            if d.kind == DeviceModel::Touch {
                let tn = format!("{}_touch", d.name);
                let _ = writeln!(sens_lines, "    <touch name=\"{tn}\" site=\"{sn}\"/>");
                entry.sensors.push(SensorEntry {
                    role: SensorRole::Touch,
                    sensor: NamedId {
                        name: tn,
                        id: sensor_id,
                    },
                });
                sensor_id += 1;
            }
        }
        devices.devices.push(entry);
    }
    if !act_lines.is_empty() {
        out.push_str("  <actuator>\n");
        out.push_str(&act_lines);
        out.push_str("  </actuator>\n");
    }
    if !sens_lines.is_empty() {
        out.push_str("  <sensor>\n");
        out.push_str(&sens_lines);
        out.push_str("  </sensor>\n");
    }
    out.push_str("</mujoco>\n");

    // Body map in MuJoCo id order.
    let mut entries: Vec<BodyMapEntry> = model
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| BodyMapEntry {
            name: b.name.clone(),
            id: ids.body_ids[&i],
            parts: b.parts.clone(),
        })
        .collect();
    entries.sort_by_key(|e| e.id);

    Compiled {
        mjcf: out,
        devices,
        bodies: BodyMap { bodies: entries },
        model: model.clone(),
        warnings: model.warnings.clone(),
    }
}
