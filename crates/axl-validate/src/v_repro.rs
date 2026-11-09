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
    TestResult {
        id: "V15",
        title: TITLE,
        rows: vec![
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
