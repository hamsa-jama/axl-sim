//! V07 to V09, V11 and V16 to V18: the hardware models (motors, battery, sensors, encoders)
//! against textbook answers.

use axl_compile::PhysicsParams;
use axl_compile::params::MotorParams;
use axl_hw::rig::{MotorSpec, Rig, WorldSpec, motor_rig, world_rig};
use axl_hw::{
    ColorSensor, FloorMat, Hardware, HwOptions, HwParams, MatTexture, MotorCommand, Noise, Slot,
};
use axl_mujoco::{BodyId, SiteId};
use axl_parts::types::DeviceModel;

use crate::report::{Row, TestResult};

fn failed(id: &'static str, title: &'static str, why: &str) -> TestResult {
    TestResult {
        id,
        title,
        rows: vec![Row::check("rig builds and loads", "loads", why, false)],
    }
}

fn quiet() -> HwOptions {
    HwOptions {
        noise: false,
        ..Default::default()
    }
}

fn spec(name: &'static str, model: DeviceModel, params: MotorParams, locked: bool) -> MotorSpec {
    MotorSpec {
        name,
        model,
        params,
        locked,
    }
}

fn tick(hw: &mut Hardware, r: &mut Rig) {
    hw.pre_step(&mut r.physics);
    r.physics.step();
    hw.post_step(&mut r.physics);
}

const TITLE_V07: &str = "Medium motor, no load, at V_nom";

/// V07: the steady speed is omega_0 (1 - tau_f / tau_s).
pub fn v07_no_load_speed() -> TestResult {
    let pp = PhysicsParams::builtin();
    let m = pp.motors.medium;
    let mut r = match motor_rig(
        &[spec("m", DeviceModel::MotorMedium, m, false)],
        pp.joints.motor_damping,
    ) {
        Ok(r) => r,
        Err(e) => return failed("V07", TITLE_V07, &e),
    };
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), quiet(), &r.physics);
    if let Some(mo) = hw.motor_mut("m") {
        mo.command(MotorCommand::Volts(m.v_nom));
    }
    for _ in 0..4000 {
        tick(&mut hw, &mut r);
    }
    let j = hw.motor("m").map(|x| x.joint());
    let Some(j) = j else {
        return failed("V07", TITLE_V07, "motor missing");
    };
    let w = r.physics.joint_vel(j);
    let expected = m.omega0 * (1.0 - m.friction_loss / m.stall_torque);
    TestResult {
        id: "V07",
        title: TITLE_V07,
        rows: vec![Row::rel("steady speed", expected, w, 0.005, "rad/s")],
    }
}

const TITLE_V08: &str = "Medium motor, shaft blocked";

/// V08: stall torque is tau_s V / V_nom and the current is V / R.
pub fn v08_blocked_shaft() -> TestResult {
    let pp = PhysicsParams::builtin();
    let m = pp.motors.medium;
    let mut r = match motor_rig(
        &[spec("m", DeviceModel::MotorMedium, m, true)],
        pp.joints.motor_damping,
    ) {
        Ok(r) => r,
        Err(e) => return failed("V08", TITLE_V08, &e),
    };
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), quiet(), &r.physics);
    let v = 6.0;
    if let Some(mo) = hw.motor_mut("m") {
        mo.command(MotorCommand::Volts(v));
    }
    for _ in 0..300 {
        tick(&mut hw, &mut r);
    }
    let Some(mo) = hw.motor("m") else {
        return failed("V08", TITLE_V08, "motor missing");
    };
    let torque = r.physics.actuator_force(mo.actuator());
    let rows = vec![
        Row::rel(
            "stall torque at 6 V",
            m.stall_torque * v / m.v_nom,
            torque,
            0.005,
            "N m",
        ),
        Row::rel("stall current at 6 V", v / m.resistance, mo.current(), 0.005, "A"),
        Row::abs(
            "shaft speed",
            0.0,
            r.physics.joint_vel(mo.joint()),
            1e-3,
            "rad/s",
        ),
    ];
    TestResult {
        id: "V08",
        title: TITLE_V08,
        rows,
    }
}

const TITLE_V09: &str = "Motor step response";

/// V09: after a voltage step the speed reaches 63.2% of its steady value at t = J omega_0 / tau_s.
pub fn v09_step_response() -> TestResult {
    let pp = PhysicsParams::builtin();
    let m = pp.motors.medium;
    let mut r = match motor_rig(
        &[spec("m", DeviceModel::MotorMedium, m, false)],
        pp.joints.motor_damping,
    ) {
        Ok(r) => r,
        Err(e) => return failed("V09", TITLE_V09, &e),
    };
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), quiet(), &r.physics);
    let (Some(j), Ok(body)) = (hw.motor("m").map(|x| x.joint()), r.physics.body_id("rotor_m"))
    else {
        return failed("V09", TITLE_V09, "motor or rotor missing");
    };
    // J = the motor's reflected armature plus the rotor body's inertia about the hinge axis.
    let pos = r.physics.body_pose(BodyId(body.0)).pos;
    let j_total = m.armature + r.physics.moment_about_axis(body, pos, [0.0, 0.0, 1.0]);
    if let Some(mo) = hw.motor_mut("m") {
        mo.command(MotorCommand::Volts(m.v_nom));
    }
    let mut speeds = Vec::with_capacity(4000);
    for _ in 0..4000 {
        tick(&mut hw, &mut r);
        speeds.push(r.physics.joint_vel(j));
    }
    let steady = speeds[3500..].iter().sum::<f64>() / 500.0;
    let target = 0.632_120_558_8 * steady;
    let t63 = speeds
        .iter()
        .position(|&w| w >= target)
        .map(|i| {
            // Interpolate between step i-1 and i (steps end at (i+1) ms).
            let (w0, w1) = (if i == 0 { 0.0 } else { speeds[i - 1] }, speeds[i]);
            (i as f64 + (target - w0) / (w1 - w0)) * 1e-3
        })
        .unwrap_or(f64::NAN);
    TestResult {
        id: "V09",
        title: TITLE_V09,
        rows: vec![
            Row::rel(
                "time to 63.2% of steady speed",
                j_total * m.omega0 / m.stall_torque,
                t63,
                0.05,
                "s",
            ),
            Row::at_least("steady speed reached", 20.0, steady, "rad/s"),
        ],
    }
}

const TITLE_V11: &str = "Two large motors stalled";

/// V11: battery voltage = open-circuit voltage - R_int x total current.
pub fn v11_battery_sag() -> TestResult {
    let pp = PhysicsParams::builtin();
    let m = pp.motors.large;
    let mut r = match motor_rig(
        &[
            spec("a", DeviceModel::MotorLarge, m, true),
            spec("b", DeviceModel::MotorLarge, m, true),
        ],
        pp.joints.motor_damping,
    ) {
        Ok(r) => r,
        Err(e) => return failed("V11", TITLE_V11, &e),
    };
    let hp = HwParams::builtin();
    let mut hw = Hardware::new(&r.map, &hp, quiet(), &r.physics);
    for n in ["a", "b"] {
        if let Some(mo) = hw.motor_mut(n) {
            mo.command(MotorCommand::Power(1.0));
        }
    }
    for _ in 0..500 {
        tick(&mut hw, &mut r);
    }
    // Total current from the stalled-motor formula I = V / R with the voltage each motor got.
    let total: f64 = hp.battery.idle_a
        + ["a", "b"]
            .iter()
            .filter_map(|n| hw.motor(n))
            .map(|mo| (mo.applied_volts() / m.resistance).abs())
            .sum::<f64>();
    let expected = hw.battery().open_circuit() - hp.battery.internal_resistance * total;
    TestResult {
        id: "V11",
        title: TITLE_V11,
        rows: vec![
            Row::rel("battery voltage", expected, hw.battery_voltage(), 0.01, "V"),
            Row::at_least("total current", 2.0, total, "A"),
        ],
    }
}

const TITLE_V16: &str = "Distance sensors, noise off";

fn read_distances(w: &WorldSpec) -> Result<(f64, f64), String> {
    let mut r = world_rig(w)?;
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), quiet(), &r.physics);
    for _ in 0..200 {
        tick(&mut hw, &mut r);
    }
    let get = |name: &str| -> Result<f64, String> {
        let h = hw.find(name).ok_or("device missing")?;
        let slot = hw.devices()[h].slot;
        hw.distance(slot).ok_or_else(|| "not a distance sensor".to_owned())
    };
    Ok((get("eye")?, get("tof")?))
}

/// V16: the ultrasonic reads 0.500 m at a wall 0.5 m away and -1 when the wall is angled 50
/// degrees; the time-of-flight sensor still reads the angled wall.
pub fn v16_distance_sensors() -> TestResult {
    let flat = read_distances(&WorldSpec::default());
    let angled = read_distances(&WorldSpec {
        wall_angle: 50.0_f64.to_radians(),
        ..Default::default()
    });
    let (Ok((us0, tof0)), Ok((us50, tof50))) = (flat, angled) else {
        return failed("V16", TITLE_V16, "rig failed");
    };
    TestResult {
        id: "V16",
        title: TITLE_V16,
        rows: vec![
            Row::abs("ultrasonic, wall square at 0.5 m", 0.5, us0, 0.001, "m"),
            Row::abs("time of flight, wall square at 0.5 m", 0.5, tof0, 0.001, "m"),
            Row::abs("ultrasonic, wall angled 50 degrees", -1.0, us50, 1e-9, "m"),
            Row::abs(
                "time of flight, wall angled 50 degrees",
                0.5,
                tof50,
                0.001,
                "m",
            ),
        ],
    }
}

const TITLE_V17: &str = "Colour sensor across a black-white edge";

/// V17: sliding over a black/white edge, the reflectance crosses 50% at the edge.
pub fn v17_colour_edge() -> TestResult {
    let mut r = match world_rig(&WorldSpec::default()) {
        Ok(r) => r,
        Err(e) => return failed("V17", TITLE_V17, &e),
    };
    let hp = HwParams::builtin();
    let (Ok(floor), Ok(slide), Ok(look)) = (
        r.physics.geom_id("floor"),
        r.physics.joint_id("slide"),
        r.physics.site_id("look"),
    ) else {
        return failed("V17", TITLE_V17, "floor, slide or look missing");
    };
    // Black for x < 0, white for x >= 0 (the pixel boundary is exactly the edge).
    let mat = FloorMat {
        geom: floor,
        texture: MatTexture::from_fn([-1.0, -1.0], [1.0, 1.0], 2, 1, |u, _| {
            if u < 0.5 { [0.0; 3] } else { [1.0; 3] }
        }),
    };
    let mut s = ColorSensor::new(
        "color",
        SiteId(look.0),
        hp.sensors.color,
        &hp.colors,
        Noise::new(1, b'C', 0, false),
    );
    let mut curve = Vec::new();
    for k in -100..=100 {
        let x = k as f64 * 0.0001;
        r.physics.set_joint_pos(slide, x);
        r.physics.forward();
        let c = s.measure(&mut r.physics, Some(&mat));
        curve.push((x, c.reflect));
    }
    let cross = curve
        .windows(2)
        .find(|w| w[0].1 < 50.0 && w[1].1 >= 50.0)
        .map(|w| w[0].0 + (50.0 - w[0].1) / (w[1].1 - w[0].1) * (w[1].0 - w[0].0))
        .unwrap_or(f64::NAN);
    let monotonic = curve.windows(2).all(|w| w[1].1 >= w[0].1 - 1e-9);
    TestResult {
        id: "V17",
        title: TITLE_V17,
        rows: vec![
            Row::abs("position where reflectance crosses 50%", 0.0, cross, 0.001, "m"),
            Row::abs("reflectance on black", 0.0, curve[0].1, 1e-9, "%"),
            Row::abs("reflectance on white", 100.0, curve[curve.len() - 1].1, 1e-9, "%"),
            Row::check(
                "reflectance never falls while moving onto white",
                "monotonic",
                if monotonic { "monotonic" } else { "not monotonic" },
                monotonic,
            ),
        ],
    }
}

const TITLE_V18: &str = "Gyro and encoders";

/// V18: a gyro with noise off integrates 1 rad/s over 5 s to 5 rad; encoder readings are whole
/// multiples of 2 pi / 360.
pub fn v18_gyro_and_encoder() -> TestResult {
    let mut r = match world_rig(&WorldSpec {
        gravity: false,
        ..Default::default()
    }) {
        Ok(r) => r,
        Err(e) => return failed("V18", TITLE_V18, &e),
    };
    let mut hw = Hardware::new(&r.map, &HwParams::builtin(), quiet(), &r.physics);
    r.physics.set_qvel(0, 1.0);
    r.physics.forward();
    let spin = r.physics.joint_id("spin").map(|j| r.physics.joint_vel(j));
    let mut angle = 0.0;
    for ms in 1..=5100u64 {
        tick(&mut hw, &mut r);
        // A 5 s window in steady state, after the first readings have arrived.
        if ms > 100
            && let Some(imu) = hw.imu()
        {
            angle += imu.gyro(hw.now_ms())[2] * 1e-3;
        }
    }
    let gyro_row = Row::rel("integrated angle, 1 rad/s for 5 s", 5.0, angle, 0.001, "rad");

    // Encoder: spin a medium motor and check every reading.
    let pp = PhysicsParams::builtin();
    let mut enc_rows = Vec::new();
    match motor_rig(
        &[spec("m", DeviceModel::MotorMedium, pp.motors.medium, false)],
        pp.joints.motor_damping,
    ) {
        Ok(mut mr) => {
            let mut h = Hardware::new(&mr.map, &HwParams::builtin(), quiet(), &mr.physics);
            if let Some(mo) = h.motor_mut("m") {
                mo.command(MotorCommand::Volts(5.0));
            }
            let step = std::f64::consts::TAU / 360.0;
            let (mut worst, mut n) = (0.0f64, 0);
            for _ in 0..3000 {
                tick(&mut h, &mut mr);
                if let Some(mo) = h.motor("m") {
                    let k = mo.angle() / step;
                    worst = worst.max((k - k.round()).abs());
                    n += 1;
                }
            }
            let turned = h.motor("m").map_or(0.0, |m| m.angle());
            enc_rows.push(Row::abs(
                "encoder angle minus nearest whole count (worst of 3000 readings)",
                0.0,
                worst * step,
                1e-9,
                "rad",
            ));
            enc_rows.push(Row::at_least("encoder travelled", 10.0, turned, "rad"));
            let _ = n;
        }
        Err(e) => enc_rows.push(Row::check("encoder rig", "loads", &e, false)),
    }
    let mut rows = vec![
        Row::abs("hub spin rate set", 1.0, spin.unwrap_or(f64::NAN), 1e-9, "rad/s"),
        gyro_row,
    ];
    rows.extend(enc_rows);
    let _ = Slot::Hub;
    TestResult {
        id: "V18",
        title: TITLE_V18,
        rows,
    }
}
