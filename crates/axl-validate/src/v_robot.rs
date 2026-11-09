//! V10 and V12 to V14: the compiled robots (gears, driving, resting) against textbook answers.

use axl_build::Assembly;
use axl_build::starter::build_starter;
use axl_compile::model::{GeomShape, Surface};
use axl_compile::rigs;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_mujoco::{ActuatorId, BodyId, Physics};
use axl_parts::catalog::Catalog;

use crate::report::{Row, TestResult};

fn rover() -> Result<Assembly, String> {
    build_starter(&Catalog::builtin(), "rover").map_err(|e| e.to_string())
}

fn compiled(asm: &Assembly) -> Result<(Compiled, Physics), String> {
    let c = compile(
        &Catalog::builtin(),
        asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let p = Physics::from_mjcf(&c.mjcf).map_err(|e| e.to_string())?;
    Ok((c, p))
}

fn failed(id: &'static str, title: &'static str, why: &str) -> TestResult {
    TestResult {
        id,
        title,
        rows: vec![Row::check("rig builds and loads", "loads", why, false)],
    }
}

fn act(c: &Compiled, name: &str) -> Option<ActuatorId> {
    c.devices
        .get(name)?
        .actuator
        .as_ref()
        .map(|a| ActuatorId(a.id))
}

fn yaw(p: &Physics, b: BodyId) -> f64 {
    let [w, x, y, z] = p.body_pose(b).quat;
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

const TITLE_V10: &str = "8-tooth gear driving 40-tooth, 10 turns";

/// V10: the output angle of a meshed pair is -(teeth_in / teeth_out) times the input angle.
pub fn v10_gear_ratio() -> TestResult {
    let (t_in, t_out) = (8u32, 40u32);
    let asm = match rigs::gear_pair(&Catalog::builtin(), t_in, t_out) {
        Ok(a) => a,
        Err(e) => return failed("V10", TITLE_V10, &e.to_string()),
    };
    let (c, mut p) = match compiled(&asm) {
        Ok(x) => x,
        Err(e) => return failed("V10", TITLE_V10, &e),
    };
    let (Some(drive), Some(idle)) = (act(&c, "drive"), c.devices.get("idle")) else {
        return failed("V10", TITLE_V10, "devices drive and idle are missing");
    };
    let (Some(jd), Some(ji)) = (
        c.devices.get("drive").and_then(|d| d.joint.as_ref()),
        idle.joint.as_ref(),
    ) else {
        return failed("V10", TITLE_V10, "motor joints are missing");
    };
    let (jd, ji) = (axl_mujoco::JointId(jd.id), axl_mujoco::JointId(ji.id));
    // Drive at the nominal voltage until the input has turned 10 times (at most 30 s).
    let target = 10.0 * std::f64::consts::TAU;
    for _ in 0..30_000 {
        p.set_ctrl(drive, 7.4);
        p.step();
        if p.joint_pos(jd) >= target {
            break;
        }
    }
    let (q_in, q_out) = (p.joint_pos(jd), p.joint_pos(ji));
    let expected = -(t_in as f64) / (t_out as f64) * q_in;
    TestResult {
        id: "V10",
        title: TITLE_V10,
        rows: vec![
            Row::at_least("input turns", 9.99, q_in / std::f64::consts::TAU, "turns"),
            Row::rel("output angle", expected, q_out, 0.001, "rad"),
        ],
    }
}

/// Wheel centres (left, right) in the robot frame, from the rubber cylinders of the model.
fn wheel_spacing(c: &Compiled) -> Option<f64> {
    let mut ys = Vec::new();
    for b in &c.model.bodies {
        for g in &b.geoms {
            if g.surface == Surface::Rubber
                && let GeomShape::Cylinder { from, to, .. } = g.shape
            {
                ys.push(b.origin[1] + (from[1] + to[1]) / 2.0);
            }
        }
    }
    ys.sort_by(f64::total_cmp);
    (ys.len() == 2).then(|| ys[1] - ys[0])
}

/// The two wheel bodies, found through the motors' hinge joints.
fn wheel_bodies(c: &Compiled, p: &Physics) -> Option<(BodyId, BodyId)> {
    let name_of = |dev: &str| -> Option<String> {
        let j = &c.devices.get(dev)?.joint.as_ref()?.name;
        c.model
            .bodies
            .iter()
            .find(|b| b.joint.as_ref().is_some_and(|x| x.name() == j))
            .map(|b| b.name.clone())
    };
    Some((
        p.body_id(&name_of("left")?).ok()?,
        p.body_id(&name_of("right")?).ok()?,
    ))
}

const TITLE_V12: &str = "Rover, equal wheel speeds, 1 m";

/// V12: with equal wheel voltages the Rover drives straight; lateral drift after 1 m is under 1 mm.
pub fn v12_straight() -> TestResult {
    let asm = match rover() {
        Ok(a) => a,
        Err(e) => return failed("V12", TITLE_V12, &e),
    };
    let (c, mut p) = match compiled(&asm) {
        Ok(x) => x,
        Err(e) => return failed("V12", TITLE_V12, &e),
    };
    let (Some(l), Some(r)) = (act(&c, "left"), act(&c, "right")) else {
        return failed("V12", TITLE_V12, "motors are missing");
    };
    let Ok(root) = p.body_id("root") else {
        return failed("V12", TITLE_V12, "no root body");
    };
    p.forward();
    let start = p.body_pose(root).pos;
    let (mut y_at_1m, mut yaw_at_1m) = (f64::NAN, f64::NAN);
    let (mut prev_x, mut prev_y) = (0.0, 0.0);
    for _ in 0..20_000 {
        p.set_ctrl(l, 4.0);
        p.set_ctrl(r, 4.0);
        p.step();
        let pos = p.body_pose(root).pos;
        let (x, y) = (pos[0] - start[0], pos[1] - start[1]);
        if x >= 1.0 {
            let f = (1.0 - prev_x) / (x - prev_x);
            y_at_1m = prev_y + f * (y - prev_y);
            yaw_at_1m = yaw(&p, root);
            break;
        }
        (prev_x, prev_y) = (x, y);
    }
    TestResult {
        id: "V12",
        title: TITLE_V12,
        rows: vec![
            Row::at_most("lateral drift after 1 m", 0.001, y_at_1m.abs(), "m"),
            Row::at_most("heading change after 1 m", 0.01, yaw_at_1m.abs(), "rad"),
        ],
    }
}

const TITLE_V13: &str = "Rover, unequal wheel speeds, turn radius";

/// V13: R = (b/2)(wR + wL)/(wR - wL), with b the wheel spacing, within 3%.
pub fn v13_turn_radius() -> TestResult {
    let asm = match rover() {
        Ok(a) => a,
        Err(e) => return failed("V13", TITLE_V13, &e),
    };
    let (c, mut p) = match compiled(&asm) {
        Ok(x) => x,
        Err(e) => return failed("V13", TITLE_V13, &e),
    };
    let (Some(l), Some(r), Some(b)) = (act(&c, "left"), act(&c, "right"), wheel_spacing(&c)) else {
        return failed("V13", TITLE_V13, "motors or wheels are missing");
    };
    let (Some((wl, wr)), Ok(root)) = (wheel_bodies(&c, &p), p.body_id("root")) else {
        return failed("V13", TITLE_V13, "wheel bodies are missing");
    };
    let (Some(jl), Some(jr)) = (
        c.devices.get("left").and_then(|d| d.joint.as_ref()),
        c.devices.get("right").and_then(|d| d.joint.as_ref()),
    ) else {
        return failed("V13", TITLE_V13, "motor joints are missing");
    };
    let (jl, jr) = (axl_mujoco::JointId(jl.id), axl_mujoco::JointId(jr.id));
    p.forward();
    // Settle for 1.5 s, then measure over the next 3 s: path length of the point midway between
    // the wheels, heading change, and the mean wheel speeds.
    let mid = |p: &Physics| {
        let (a, b) = (p.body_pose(wl).pos, p.body_pose(wr).pos);
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    let (mut s, mut psi, mut sum_l, mut sum_r, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut last = [0.0; 2];
    let mut last_yaw = 0.0;
    for step in 0..4500 {
        p.set_ctrl(l, 3.0);
        p.set_ctrl(r, 6.0);
        p.step();
        if step == 1500 {
            last = mid(&p);
            last_yaw = yaw(&p, root);
        }
        if step > 1500 {
            let m = mid(&p);
            s += ((m[0] - last[0]).powi(2) + (m[1] - last[1]).powi(2)).sqrt();
            last = m;
            let y = yaw(&p, root);
            let mut d = y - last_yaw;
            while d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            while d < -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            }
            psi += d;
            last_yaw = y;
            sum_l += p.joint_vel(jl);
            sum_r += p.joint_vel(jr);
            n += 1.0;
        }
    }
    let (wl_m, wr_m) = (sum_l / n, sum_r / n);
    let expected = b / 2.0 * (wr_m + wl_m) / (wr_m - wl_m);
    let measured = s / psi.abs();
    TestResult {
        id: "V13",
        title: TITLE_V13,
        rows: vec![
            Row::abs("wheel spacing b", b, b, 0.0, "m"),
            Row::rel("turn radius", expected, measured, 0.03, "m"),
        ],
    }
}

const TITLE_V14: &str = "Rover at rest sinks less than 0.2 mm";

/// V14: after settling, no contact is deeper than 0.2 mm.
pub fn v14_rest_penetration() -> TestResult {
    let asm = match rover() {
        Ok(a) => a,
        Err(e) => return failed("V14", TITLE_V14, &e),
    };
    let (_, mut p) = match compiled(&asm) {
        Ok(x) => x,
        Err(e) => return failed("V14", TITLE_V14, &e),
    };
    for _ in 0..3000 {
        p.step();
    }
    p.forward();
    let contacts = p.contacts();
    let depth = contacts.iter().map(|c| -c.dist).fold(0.0_f64, f64::max);
    TestResult {
        id: "V14",
        title: TITLE_V14,
        rows: vec![
            Row::at_least("floor contacts", 3.0, contacts.len() as f64, "contacts"),
            Row::at_most("deepest penetration", 0.0002, depth, "m"),
        ],
    }
}
