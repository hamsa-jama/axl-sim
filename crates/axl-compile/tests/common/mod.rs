#![allow(dead_code)]
//! Shared helpers for the compiler tests.

use axl_build::Assembly;
use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_mujoco::Physics;
use axl_parts::catalog::Catalog;

pub fn starter(id: &str) -> Assembly {
    build_starter(&Catalog::builtin(), id).expect("starter builds")
}

pub fn compile_asm(asm: &Assembly) -> Compiled {
    compile(
        &Catalog::builtin(),
        asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .expect("compiles")
}

pub fn physics(c: &Compiled) -> Physics {
    Physics::from_mjcf(&c.mjcf).expect("MuJoCo loads the MJCF")
}

/// Rotate `v` by the unit quaternion `[w, x, y, z]`.
pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [w, x, y, z] = q;
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}
