//! Motors: command modes, encoders, servo, battery interaction.

use axl_compile::params::MotorParams;
use axl_hw::rig::{MotorSpec, motor_rig};
use axl_hw::*;
use axl_parts::types::DeviceModel;

fn params() -> axl_compile::PhysicsParams {
    axl_compile::PhysicsParams::builtin()
}

fn spec(name: &'static str, model: DeviceModel, p: MotorParams, locked: bool) -> MotorSpec {
    MotorSpec {
        name,
        model,
        params: p,
        locked,
    }
}

fn run(hw: &mut Hardware, r: &mut axl_hw::rig::Rig, ms: usize) {
    for _ in 0..ms {
        hw.pre_step(&mut r.physics);
        r.physics.step();
        hw.post_step(&mut r.physics);
    }
}

fn setup(model: DeviceModel, locked: bool) -> (Hardware, axl_hw::rig::Rig) {
    let p = params();
    let mp = match model {
        DeviceModel::MotorLarge => p.motors.large,
        DeviceModel::Servo => p.motors.servo,
        _ => p.motors.medium,
    };
    let r = motor_rig(&[spec("m", model, mp, locked)], p.joints.motor_damping).unwrap();
    let hw = Hardware::new(
        &r.map,
        &HwParams::builtin(),
        HwOptions {
            noise: false,
            ..Default::default()
        },
        &r.physics,
    );
    (hw, r)
}

#[test]
fn power_mode_is_a_fraction_of_the_battery_voltage() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    hw.motor_mut("m").unwrap().command(MotorCommand::Power(0.5));
    run(&mut hw, &mut r, 5);
    let m = hw.motor("m").unwrap();
    let vb = hw.battery_voltage();
    assert!(
        (m.applied_volts() - 0.5 * vb).abs() < 0.1,
        "{} vs {}",
        m.applied_volts(),
        vb
    );
    // Power is clamped to [-1, 1].
    hw.motor_mut("m").unwrap().command(MotorCommand::Power(7.0));
    run(&mut hw, &mut r, 2);
    assert!(hw.motor("m").unwrap().applied_volts() <= hw.battery_voltage() + 1e-9);
}

#[test]
fn speed_mode_reaches_the_reference_with_the_pi_loop() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    hw.motor_mut("m")
        .unwrap()
        .command(MotorCommand::Speed(10.0));
    run(&mut hw, &mut r, 2000);
    let m = hw.motor("m").unwrap();
    assert!((m.rate() - 10.0).abs() < 2.0, "encoder speed {}", m.rate());
    let w = r.physics.joint_vel(m.joint());
    assert!((w - 10.0).abs() < 0.5, "true speed {w}");
    // Negative reference runs the other way.
    hw.motor_mut("m")
        .unwrap()
        .command(MotorCommand::Speed(-10.0));
    run(&mut hw, &mut r, 2000);
    let w = r.physics.joint_vel(hw.motor("m").unwrap().joint());
    assert!((w + 10.0).abs() < 0.5, "{w}");
}

#[test]
fn speed_mode_saturates_without_windup() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    // Far above the no-load speed: the output saturates at the battery voltage.
    hw.motor_mut("m")
        .unwrap()
        .command(MotorCommand::Speed(100.0));
    run(&mut hw, &mut r, 1500);
    let vb = hw.battery_voltage();
    assert!((hw.motor("m").unwrap().applied_volts() - vb).abs() < 1e-6);
    // Back to a reachable reference: with anti-windup it settles quickly instead of overshooting
    // for seconds.
    hw.motor_mut("m")
        .unwrap()
        .command(MotorCommand::Speed(10.0));
    run(&mut hw, &mut r, 400);
    let w = r.physics.joint_vel(hw.motor("m").unwrap().joint());
    assert!((w - 10.0).abs() < 2.0, "{w}");
}

#[test]
fn position_mode_goes_to_the_angle_and_reports_done() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    let target = 3.0;
    hw.motor_mut("m").unwrap().command(MotorCommand::Position {
        target,
        max_speed: 8.0,
    });
    assert!(!hw.motor("m").unwrap().done());
    let mut done_at = None;
    for ms in 0..4000 {
        run(&mut hw, &mut r, 1);
        if hw.motor("m").unwrap().done() && done_at.is_none() {
            done_at = Some(ms);
        }
    }
    let m = hw.motor("m").unwrap();
    assert!(done_at.is_some(), "never done; angle {}", m.angle());
    assert!(m.done(), "still done at the end, angle {}", m.angle());
    assert!(
        (m.angle() - target).abs() < 1.0_f64.to_radians() + 1e-9,
        "{}",
        m.angle()
    );
    // It did not run faster than the limit (3 rad at 8 rad/s takes at least 0.37 s).
    assert!(done_at.unwrap() > 370, "{done_at:?}");
}

#[test]
fn brake_stops_the_motor_and_coast_draws_almost_no_current() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    hw.motor_mut("m").unwrap().command(MotorCommand::Volts(7.4));
    run(&mut hw, &mut r, 800);
    let w0 = r.physics.joint_vel(hw.motor("m").unwrap().joint());
    assert!(w0 > 20.0, "{w0}");
    // Coast: torque and current are about zero, so the speed barely falls over 50 ms.
    hw.motor_mut("m").unwrap().command(MotorCommand::Coast);
    run(&mut hw, &mut r, 50);
    let m = hw.motor("m").unwrap();
    assert!(m.current().abs() < 0.05, "current {}", m.current());
    let w1 = r.physics.joint_vel(m.joint());
    assert!(w1 > 0.9 * w0, "{w1} vs {w0}");
    // Brake: V = 0, back-EMF brakes it to a stop.
    hw.motor_mut("m").unwrap().command(MotorCommand::Brake);
    run(&mut hw, &mut r, 600);
    let m = hw.motor("m").unwrap();
    assert!(r.physics.joint_vel(m.joint()).abs() < 0.05);
    assert_eq!(m.applied_volts(), 0.0);
}

#[test]
fn encoder_counts_whole_steps_and_speed_is_over_ten_milliseconds() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    hw.motor_mut("m").unwrap().command(MotorCommand::Volts(3.0));
    let step = std::f64::consts::TAU / 360.0;
    for _ in 0..1500 {
        run(&mut hw, &mut r, 1);
        let m = hw.motor("m").unwrap();
        let k = m.angle() / step;
        assert!(
            (k - k.round()).abs() < 1e-9,
            "angle is not whole counts: {k}"
        );
        // The rate is a whole number of counts per 10 ms.
        let c = m.rate() * 0.01 / step;
        assert!((c - c.round()).abs() < 1e-6, "rate {c}");
    }
    let m = hw.motor("m").unwrap();
    let true_angle = r.physics.joint_pos(m.joint());
    assert_eq!(m.counts(), (true_angle / step).floor() as i64);
    // Reset makes the present position zero.
    hw.motor_mut("m").unwrap().reset_angle();
    assert_eq!(hw.motor("m").unwrap().counts(), 0);
}

#[test]
fn servo_holds_zero_obeys_position_and_stops_at_ninety_degrees() {
    let (mut hw, mut r) = setup(DeviceModel::Servo, false);
    assert_eq!(hw.motor("m").unwrap().kind(), MotorKind::Servo);
    run(&mut hw, &mut r, 200);
    assert!(hw.motor("m").unwrap().angle().abs() < 0.02);
    hw.motor_mut("m").unwrap().command(MotorCommand::Position {
        target: 1.0,
        max_speed: 6.0,
    });
    run(&mut hw, &mut r, 1500);
    let m = hw.motor("m").unwrap();
    assert!((m.angle() - 1.0).abs() < 0.02, "{}", m.angle());
    assert!(m.done());
    // A target beyond +-90 degrees is limited to 90 degrees.
    hw.motor_mut("m").unwrap().command(MotorCommand::Position {
        target: 3.0,
        max_speed: 6.0,
    });
    run(&mut hw, &mut r, 2500);
    let m = hw.motor("m").unwrap();
    assert!(
        (m.angle() - std::f64::consts::FRAC_PI_2).abs() < 0.03,
        "{}",
        m.angle()
    );
    // The joint limit holds even under full power.
    hw.motor_mut("m").unwrap().command(MotorCommand::Power(1.0));
    run(&mut hw, &mut r, 1000);
    let q = r.physics.joint_pos(hw.motor("m").unwrap().joint());
    assert!(q < std::f64::consts::FRAC_PI_2 + 0.05, "{q}");
    // Encoder: 1024 counts over 300 degrees.
    let per_rad = 1024.0 / 300.0_f64.to_radians();
    assert!((hw.motor("m").unwrap().counts() as f64 / per_rad - q).abs() < 1.0 / per_rad + 1e-9);
}

#[test]
fn battery_sags_under_load_and_charge_falls() {
    let p = params();
    let r = motor_rig(
        &[
            spec("a", DeviceModel::MotorLarge, p.motors.large, true),
            spec("b", DeviceModel::MotorLarge, p.motors.large, true),
        ],
        p.joints.motor_damping,
    )
    .unwrap();
    let mut r = r;
    let mut hw = Hardware::new(
        &r.map,
        &HwParams::builtin(),
        HwOptions {
            noise: false,
            ..Default::default()
        },
        &r.physics,
    );
    run(&mut hw, &mut r, 20);
    let idle_v = hw.battery_voltage();
    assert!((idle_v - (8.2 - 0.15 * 0.08)).abs() < 1e-3, "{idle_v}");
    for n in ["a", "b"] {
        hw.motor_mut(n).unwrap().command(MotorCommand::Power(1.0));
    }
    run(&mut hw, &mut r, 100);
    // Two stalled 4 ohm motors: V = 8.2 - 0.15 (0.08 + 2 V / 4)  =>  V = 7.6179.
    assert!(
        (hw.battery_voltage() - 7.6179).abs() < 0.01,
        "{}",
        hw.battery_voltage()
    );
    let soc = hw.battery().soc();
    assert!(soc < 0.9);
    run(&mut hw, &mut r, 1000);
    assert!(hw.battery().soc() < soc);
}

#[test]
fn reset_restores_the_start_and_replays_identically() {
    let (mut hw, mut r) = setup(DeviceModel::MotorMedium, false);
    hw.motor_mut("m").unwrap().command(MotorCommand::Speed(8.0));
    run(&mut hw, &mut r, 700);
    let a = (hw.motor("m").unwrap().counts(), hw.battery().soc());
    r.physics.reset();
    hw.reset(&r.physics);
    assert_eq!(hw.now_ms(), 0);
    assert_eq!(hw.motor("m").unwrap().counts(), 0);
    hw.motor_mut("m").unwrap().command(MotorCommand::Speed(8.0));
    run(&mut hw, &mut r, 700);
    assert_eq!(a, (hw.motor("m").unwrap().counts(), hw.battery().soc()));
}
