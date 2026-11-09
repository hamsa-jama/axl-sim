//! The step loop does not allocate (alone in its binary: the counter covers every thread).

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_hw::*;
use axl_mujoco::Physics;
use axl_mujoco::alloc_count::{CountingAlloc, count_allocs};
use axl_parts::catalog::Catalog;

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

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
fn the_step_loop_does_not_allocate_and_replays_identically() {
    let c = rover();
    let mut traces = Vec::new();
    for _ in 0..2 {
        let (mut hw, mut p) = start(&c, 7);
        for n in ["left", "right"] {
            hw.motor_mut(n).unwrap().command(MotorCommand::Speed(6.0));
        }
        let slot = hw.devices()[hw.find("eye").unwrap()].slot;
        let mut trace = Vec::new();
        let mut allocs = 0;
        for ms in 0..3000 {
            let ((), a) = count_allocs(|| hw.pre_step(&mut p));
            p.step();
            let ((), b) = count_allocs(|| hw.post_step(&mut p));
            allocs += a + b;
            if ms % 100 == 0 {
                trace.push((
                    hw.distance(slot).unwrap().to_bits(),
                    hw.imu().unwrap().gyro(hw.now_ms()).map(f64::to_bits),
                    hw.motor("left").unwrap().counts(),
                ));
            }
        }
        assert_eq!(allocs, 0, "hardware models allocated in the step loop");
        traces.push(trace);
    }
    assert_eq!(traces[0], traces[1], "same seed, same readings");
    // A different seed changes the noisy readings.
    let (mut hw, mut p) = start(&c, 8);
    for _ in 0..1000 {
        hw.pre_step(&mut p);
        p.step();
        hw.post_step(&mut p);
    }
    let g = hw.imu().unwrap().gyro(hw.now_ms()).map(f64::to_bits);
    assert_ne!(g, traces[0][9].1);
}
