//! V15: reproducibility.

use axl_mujoco::Physics;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

use crate::report::{Row, TestResult};
use crate::scenes;

const TITLE: &str = "Same scene and seed, run twice: identical joint-position hash";

/// Run the clutter scene from `seed` for 3 s and hash all joint positions
/// every 100 ms. Returns the hex digest, or an error message.
fn run_hash(seed: u64) -> Result<String, String> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut unit = move || (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let boxes = 6;
    let xml = scenes::clutter(boxes, &mut unit);
    let mut p = Physics::from_mjcf(&xml).map_err(|e| e.to_string())?;
    for i in 0..boxes {
        p.set_qvel(6 * i, (unit() - 0.5) * 2.0);
        p.set_qvel(6 * i + 1, (unit() - 0.5) * 2.0);
    }
    let mut hasher = Sha256::new();
    for step in 1..=3000 {
        p.step();
        if step % 100 == 0 {
            for q in p.qpos() {
                hasher.update(q.to_le_bytes());
            }
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// The Rover with every hardware model active (noisy sensors, battery, speed loops) reacting to
/// its own distance sensor, for 3 s. Hashes joint positions every 100 ms together with the
/// sensor readings and encoder counts the controller would see.
fn run_rover_hash(seed: u64) -> Result<String, String> {
    use axl_build::starter::build_starter;
    use axl_compile::{CompileOptions, PhysicsParams, compile};
    use axl_hw::{Hardware, HwOptions, HwParams, MotorCommand};
    use axl_parts::catalog::Catalog;

    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").map_err(|e| e.to_string())?;
    let opts = CompileOptions {
        extra_worldbody: r#"<geom name="wall" type="box" pos="0.9 0 0.1" size="0.01 1 0.1" contype="2" conaffinity="1" group="0" rgba="0.9 0.9 0.9 1"/>"#.into(),
        ..Default::default()
    };
    let c = compile(&cat, &asm, &PhysicsParams::builtin(), &opts).map_err(|e| e.to_string())?;
    let mut p = Physics::from_mjcf(&c.mjcf).map_err(|e| e.to_string())?;
    let mut hw = Hardware::new(
        &c.devices,
        &HwParams::builtin(),
        HwOptions {
            seed,
            ..Default::default()
        },
        &p,
    );
    let eye = hw
        .find("eye")
        .map(|h| hw.devices()[h].slot)
        .ok_or("no eye")?;
    let mut hasher = Sha256::new();
    for step in 1..=3000 {
        // A controller every 10 ms: drive on, back off when the wall is close.
        if step % 10 == 0 {
            let d = hw.distance(eye).unwrap_or(-1.0);
            let w = if d > 0.0 && d < 0.30 { -6.0 } else { 8.0 };
            for n in ["left", "right"] {
                if let Some(m) = hw.motor_mut(n) {
                    m.command(MotorCommand::Speed(w));
                }
            }
        }
        hw.pre_step(&mut p);
        p.step();
        hw.post_step(&mut p);
        if step % 100 == 0 {
            for q in p.qpos() {
                hasher.update(q.to_le_bytes());
            }
            hasher.update(hw.distance(eye).unwrap_or(-1.0).to_le_bytes());
            if let Some(imu) = hw.imu() {
                for v in imu
                    .gyro(hw.now_ms())
                    .into_iter()
                    .chain(imu.accel(hw.now_ms()))
                {
                    hasher.update(v.to_le_bytes());
                }
            }
            hasher.update(hw.battery().soc().to_le_bytes());
            for m in hw.motors() {
                hasher.update(m.counts().to_le_bytes());
            }
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// V15: two identical runs hash identically; a different seed does not.
pub fn v15_determinism() -> TestResult {
    let seed = 1;
    let (a, b, other) = match (run_hash(seed), run_hash(seed), run_hash(seed + 1)) {
        (Ok(a), Ok(b), Ok(c)) => (a, b, c),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
            return TestResult {
                id: "V15",
                title: TITLE,
                rows: vec![Row::check("scene runs", "runs", &e, false)],
            };
        }
    };
    let rover = match (
        run_rover_hash(seed),
        run_rover_hash(seed),
        run_rover_hash(seed + 1),
    ) {
        (Ok(a), Ok(b), Ok(c)) => (a, b, c),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
            return TestResult {
                id: "V15",
                title: TITLE,
                rows: vec![Row::check("rover runs", "runs", &e, false)],
            };
        }
    };
    TestResult {
        id: "V15",
        title: TITLE,
        rows: vec![
            Row::check(
                "rover with all hardware models: run 1 vs run 2",
                &rover.0,
                &rover.1,
                rover.0 == rover.1,
            ),
            Row::check(
                "rover with all hardware models: another seed differs",
                "different digest",
                if rover.0 == rover.2 {
                    "same digest"
                } else {
                    "different digest"
                },
                rover.0 != rover.2,
            ),
            Row::check("run 1 vs run 2 (seed 1)", &a, &b, a == b),
            Row::check(
                "seed 2 differs (the hash is sensitive)",
                "different digest",
                if a == other {
                    "same digest"
                } else {
                    "different digest"
                },
                a != other,
            ),
        ],
    }
}
