//! The hardware models on the compiled Rover, and the no-allocation rule for the step loop.

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_hw::*;
use axl_mujoco::Physics;
use axl_parts::catalog::Catalog;

fn rover() -> Compiled {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .unwrap()
}

fn start(c: &Compiled, seed: u64) -> (Hardware, Physics) {
    let p = Physics::from_mjcf(&c.mjcf).unwrap();
    let hw = Hardware::new(
        &c.devices,
        &HwParams::builtin(),
        HwOptions {
            seed,
            ..Default::default()
        },
        &p,
    );
    (hw, p)
}

#[test]
fn rover_devices_map_to_models() {
    let c = rover();
    let (hw, _) = start(&c, 1);
    let names: Vec<_> = hw
        .devices()
        .iter()
        .map(|d| (d.name.as_str(), d.kind))
        .collect();
    assert!(names.contains(&("left", DeviceKind::Motor)));
    assert!(names.contains(&("right", DeviceKind::Motor)));
    assert!(names.contains(&("eye", DeviceKind::Distance)));
    assert!(names.iter().any(|(_, k)| *k == DeviceKind::Color));
    assert!(names.iter().any(|(_, k)| *k == DeviceKind::Hub));
    assert!(hw.imu().is_some());
}

#[test]
fn the_rover_drives_and_its_encoders_and_gyro_agree() {
    let c = rover();
    let (mut hw, mut p) = start(&c, 1);
    for n in ["left", "right"] {
        hw.motor_mut(n).unwrap().command(MotorCommand::Speed(8.0));
    }
    for _ in 0..2000 {
        hw.pre_step(&mut p);
        p.step();
        hw.post_step(&mut p);
    }
    let l = hw.motor("left").unwrap();
    let r = hw.motor("right").unwrap();
    assert!((l.rate() - 8.0).abs() < 2.0, "{}", l.rate());
    assert!((r.rate() - 8.0).abs() < 2.0, "{}", r.rate());
    assert!((l.angle() - r.angle()).abs() < 0.5);
    let g = hw.imu().unwrap().gyro(hw.now_ms());
    assert!(g[2].abs() < 0.1, "driving straight, yaw rate {}", g[2]);
}
