//! V01 to V06: rigid-body dynamics against textbook answers.

use axl_mujoco::Physics;

use crate::report::{Row, TestResult};
use crate::scenes;

/// Load a scene. A failure here is a bug in the scene text, reported as a
/// failed row instead of a panic.
fn load(xml: &str) -> Result<Physics, Row> {
    Physics::from_mjcf(xml).map_err(|e| Row::check("scene loads", "loads", &e.to_string(), false))
}

fn failed(id: &'static str, title: &'static str, row: Row) -> TestResult {
    TestResult {
        id,
        title,
        rows: vec![row],
    }
}

/// Step until `time >= seconds`.
fn run_until(p: &mut Physics, seconds: f64) {
    let dt = p.timestep();
    while p.time() + 0.5 * dt < seconds {
        p.step();
    }
}

const TITLE_V01: &str = "Free fall from 1 m";

/// V01: a ball dropped from 1 m reaches the floor at t = sqrt(2h/g).
pub fn v01_free_fall() -> TestResult {
    let (h, r) = (1.0, 0.01);
    let mut p = match load(&scenes::free_fall(h, r)) {
        Ok(p) => p,
        Err(row) => return failed("V01", TITLE_V01, row),
    };
    let g = -p.gravity()[2];
    let t_expected = (2.0 * h / g).sqrt();

    // Centre height is qpos[2]. Find when it first reaches the radius,
    // interpolating inside the step (the ball is still in free fall there).
    let mut z_at_0_3 = f64::NAN;
    let mut t_at_0_3 = 0.0;
    let mut t_impact = f64::NAN;
    let (mut t_prev, mut z_prev) = (p.time(), p.qpos()[2]);
    for _ in 0..1000 {
        p.step();
        let (t, z) = (p.time(), p.qpos()[2]);
        if z_at_0_3.is_nan() && t >= 0.3 - 1e-9 {
            z_at_0_3 = z;
            t_at_0_3 = t;
        }
        if z <= r {
            let frac = (z_prev - r) / (z_prev - z);
            t_impact = t_prev + frac * (t - t_prev);
            break;
        }
        (t_prev, z_prev) = (t, z);
    }
    // The ball must then be in contact with the floor within a step or two.
    p.step();
    p.forward();
    let touching = !p.contacts().is_empty();

    let z_expected = h + r - 0.5 * g * t_at_0_3 * t_at_0_3;
    TestResult {
        id: "V01",
        title: TITLE_V01,
        rows: vec![
            Row::abs("impact time", t_expected, t_impact, 0.001, "s"),
            Row::abs("height error at 0.3 s", z_expected, z_at_0_3, 0.002, "m"),
            Row::check(
                "floor contact after impact",
                "contact exists",
                if touching { "contact exists" } else { "none" },
                touching,
            ),
        ],
    }
}

const TITLE_V02: &str = "Pendulum, 2 degree swing: period";

/// A hanging pendulum's expected small-swing period from the model itself.
fn pendulum_period_expected(p: &Physics) -> Option<f64> {
    let body = p.body_id("pend").ok()?;
    let pivot = p.body_pose(body).pos;
    let inertia = p.moment_about_axis(body, pivot, [0.0, 1.0, 0.0]);
    let com = p.body_com(body);
    let d = (com[2] - pivot[2]).abs();
    let m = p.body_mass(body);
    let g = -p.gravity()[2];
    Some(2.0 * std::f64::consts::PI * (inertia / (m * g * d)).sqrt())
}

/// V02: small-swing period of a physical pendulum.
pub fn v02_pendulum_period() -> TestResult {
    let mut p = match load(&scenes::pendulum()) {
        Ok(p) => p,
        Err(row) => return failed("V02", TITLE_V02, row),
    };
    let Some(t_expected) = pendulum_period_expected(&p) else {
        return failed(
            "V02",
            TITLE_V02,
            Row::check("model has body", "pend", "missing", false),
        );
    };
    let Ok(j) = p.joint_id("hinge") else {
        return failed(
            "V02",
            TITLE_V02,
            Row::check("model has joint", "hinge", "missing", false),
        );
    };
    p.set_joint_pos(j, 2.0_f64.to_radians());
    p.forward();

    // Times of upward zero crossings, linearly interpolated.
    let mut crossings = Vec::new();
    let (mut t_prev, mut a_prev) = (p.time(), p.joint_pos(j));
    while p.time() < 22.0 * t_expected {
        p.step();
        let (t, a) = (p.time(), p.joint_pos(j));
        if a_prev < 0.0 && a >= 0.0 {
            crossings.push(t_prev + (t - t_prev) * (-a_prev) / (a - a_prev));
        }
        (t_prev, a_prev) = (t, a);
    }
    let t_measured = match (crossings.first(), crossings.last()) {
        (Some(first), Some(last)) if crossings.len() > 10 => {
            (last - first) / (crossings.len() - 1) as f64
        }
        _ => f64::NAN,
    };
    TestResult {
        id: "V02",
        title: TITLE_V02,
        rows: vec![Row::rel(
            "period over 20 swings",
            t_expected,
            t_measured,
            0.002,
            "s",
        )],
    }
}

const TITLE_V03: &str = "Frictionless pendulum, 30 degrees, 10 s: energy";

/// V03: total energy of a frictionless pendulum stays constant.
pub fn v03_pendulum_energy() -> TestResult {
    let mut p = match load(&scenes::pendulum()) {
        Ok(p) => p,
        Err(row) => return failed("V03", TITLE_V03, row),
    };
    let (Ok(j), Ok(body)) = (p.joint_id("hinge"), p.body_id("pend")) else {
        return failed(
            "V03",
            TITLE_V03,
            Row::check("model has pendulum", "present", "missing", false),
        );
    };
    let theta0 = 30.0_f64.to_radians();
    p.set_joint_pos(j, theta0);
    p.forward();
    let e0 = p.total_energy();

    // Energy the swing exchanges between potential and kinetic: m g d (1 - cos).
    let pivot_z = p.body_pose(body).pos[2];
    let swing_scale = {
        let d = (pivot_z - p.body_com(body)[2]).abs();
        // At theta0 the centre has risen by d (1 - cos theta0) relative to hanging.
        p.body_mass(body) * -p.gravity()[2] * d * (1.0 - theta0.cos())
    };
    let mut worst = 0.0_f64;
    for step in 1..=10_000 {
        p.step();
        if step % 10 == 0 {
            worst = worst.max((p.total_energy() - e0).abs());
        }
    }
    TestResult {
        id: "V03",
        title: TITLE_V03,
        rows: vec![Row::at_most(
            "worst energy drift (relative to the swing's energy)",
            0.01,
            worst / swing_scale,
            "fraction",
        )],
    }
}

const TITLE_V04: &str = "Box on a slope, mu = 0.5: holds below the friction angle, slides above";

/// Distance the free body "obj" has moved down a slope of `angle`.
fn slope_travel(p: &Physics, start: [f64; 3], angle: f64) -> f64 {
    let q = p.qpos();
    let down = [angle.cos(), 0.0, -angle.sin()];
    (0..3).map(|i| (q[i] - start[i]) * down[i]).sum()
}

fn slope_run(angle_deg: f64, mu: f64, seconds: f64) -> Result<f64, Row> {
    let angle = angle_deg.to_radians();
    let mut p = load(&scenes::box_on_slope(angle, mu))?;
    let start = [p.qpos()[0], p.qpos()[1], p.qpos()[2]];
    run_until(&mut p, seconds);
    Ok(slope_travel(&p, start, angle))
}

/// V04: static friction threshold on a slope.
pub fn v04_slope_friction() -> TestResult {
    let mu: f64 = 0.5;
    let threshold = mu.atan().to_degrees();
    let (hold, slide) = match (slope_run(25.5, mu, 2.0), slope_run(27.5, mu, 2.0)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(row), _) | (_, Err(row)) => return failed("V04", TITLE_V04, row),
    };
    TestResult {
        id: "V04",
        title: TITLE_V04,
        rows: vec![
            Row::at_most(
                &format!("creep in 2 s at 25.5 deg (threshold {threshold:.2} deg)"),
                0.002,
                hold.abs(),
                "m",
            ),
            Row::at_least(
                &format!("slide in 2 s at 27.5 deg (threshold {threshold:.2} deg)"),
                0.05,
                slide,
                "m",
            ),
        ],
    }
}

const TITLE_V05: &str = "Box sliding from 1 m/s, mu = 0.4: stopping distance";

/// V05: stopping distance v^2 / (2 mu g).
pub fn v05_sliding_stop() -> TestResult {
    let (mu, v0) = (0.4, 1.0);
    let mut p = match load(&scenes::box_on_floor(mu)) {
        Ok(p) => p,
        Err(row) => return failed("V05", TITLE_V05, row),
    };
    let g = -p.gravity()[2];
    let expected = v0 * v0 / (2.0 * mu * g);
    let x0 = p.qpos()[0];
    p.set_qvel(0, v0);
    // Run until the box has stopped (speed below 0.1 mm/s) or 2 s pass.
    while p.time() < 2.0 {
        p.step();
        if p.qvel()[0].abs() < 1e-4 {
            break;
        }
    }
    TestResult {
        id: "V05",
        title: TITLE_V05,
        rows: vec![Row::rel(
            "stopping distance",
            expected,
            p.qpos()[0] - x0,
            0.03,
            "m",
        )],
    }
}

const TITLE_V06: &str = "Solid cylinder and sphere rolling down 10 degrees";

/// Roll `scene` down the slope for 1 s and compare with the model's own
/// inertia: a = g sin(theta) / (1 + I / (m r^2)).
fn rolling_row(label: &str, scene: &str, angle: f64, radius: f64) -> Row {
    let mut p = match load(scene) {
        Ok(p) => p,
        Err(row) => return row,
    };
    let Ok(body) = p.body_id("obj") else {
        return Row::check("model has body", "obj", "missing", false);
    };
    let start = [p.qpos()[0], p.qpos()[1], p.qpos()[2]];
    let inertia = p.moment_about_axis(body, p.body_com(body), [0.0, 1.0, 0.0]);
    let m = p.body_mass(body);
    let g = -p.gravity()[2];
    let a = g * angle.sin() / (1.0 + inertia / (m * radius * radius));
    run_until(&mut p, 1.0);
    let t = p.time();
    Row::rel(
        label,
        0.5 * a * t * t,
        slope_travel(&p, start, angle),
        0.01,
        "m",
    )
}

/// V06: rolling acceleration of a cylinder and a sphere.
pub fn v06_rolling() -> TestResult {
    let (angle, radius) = (10.0_f64.to_radians(), 0.03);
    TestResult {
        id: "V06",
        title: TITLE_V06,
        rows: vec![
            rolling_row(
                "cylinder, distance in 1 s (a = 2/3 g sin)",
                &scenes::cylinder_on_slope(angle, radius),
                angle,
                radius,
            ),
            rolling_row(
                "sphere, distance in 1 s (a = 5/7 g sin)",
                &scenes::sphere_on_slope(angle, radius),
                angle,
                radius,
            ),
        ],
    }
}
