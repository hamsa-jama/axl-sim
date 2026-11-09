//! axl-compile: assembly to `RobotModel` to deterministic MJCF, plus the device map and the body
//! map (SPEC.md, "Assembly-to-physics compiler").
//!
//! [`compile`] is the entry point. The same assembly always gives byte-identical MJCF: everything
//! is ordered by part uid, numbers are printed with a fixed rule, and nothing depends on hash order.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
mod graph;
pub mod mjcf;
pub mod model;
pub mod params;
pub mod rigs;

use axl_build::model::Assembly;
use axl_parts::catalog::Catalog;

pub use error::CompileError;
pub use graph::build_model;
pub use mjcf::{
    BodyMap, BodyMapEntry, CompileOptions, Compiled, DeviceEntry, DeviceMap, NamedId, SensorEntry,
    SensorRole, emit,
};
pub use model::RobotModel;
pub use params::PhysicsParams;

/// Compile an assembly into MJCF, a device map and a body map.
pub fn compile(
    cat: &Catalog,
    asm: &Assembly,
    params: &PhysicsParams,
    opts: &CompileOptions,
) -> Result<Compiled, CompileError> {
    let model = build_model(cat, asm, params)?;
    Ok(emit(&model, params, opts))
}

/// Write the MJCF of a compiled robot to a file ("Export to MuJoCo").
pub fn export_mjcf(compiled: &Compiled, path: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(path, compiled.mjcf.as_bytes())
}
